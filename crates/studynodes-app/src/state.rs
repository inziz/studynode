use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rune::{ImageCache, Pixels, TextEdit, Waker};
use serde::{Deserialize, Serialize};

use studynodes_graph::coords::{ScreenPos, WorldPos};
use studynodes_graph::model::{EdgeId, NodeId, NodeKind, PortId};
use studynodes_graph::{CommandHistory, ProjectBundle, StudyGraph};
use studynodes_llm::LlmProvider;

use crate::doc_reader::{
    self, DocFormat, ReaderMode, ReaderPreferences, extract_document_markdown, render_pdf_page,
};
use crate::touchpad_win32::TouchpadEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppScreen {
    Dashboard,
    Workspace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DocViewTab {
    #[default]
    Preview,
    Pages,
    Source,
}

#[derive(Clone)]
pub struct ActivePdfState {
    pub node_id: NodeId,
    pub asset_path: PathBuf,
    pub page_index: usize,
    pub page_count: usize,
    pub zoom: f32,
    pub pixels: Pixels,
    pub page_size: (u32, u32),
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub folder_name: String,
    pub title: String,
    pub path: PathBuf,
    pub node_count: usize,
    pub edge_count: usize,
    pub last_modified: String,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ActiveWireDraft {
    pub from_node: NodeId,
    pub from_port: PortId,
    pub start_world: WorldPos,
    pub cursor_world: WorldPos,
    pub snap_target: Option<(NodeId, PortId)>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasMode {
    Idle,
    Panning {
        start_screen: ScreenPos,
        start_offset: WorldPos,
    },
    DraggingNode {
        node_id: NodeId,
        start_pos: WorldPos,
    },
    DraggingWire {
        from_node: NodeId,
        from_port: PortId,
        current_cursor: WorldPos,
    },
}

/// Canonical canvas dimensions `(base_w, base_h)` per node kind so rendered cards and wire sockets match.
pub fn node_base_size(kind: NodeKind) -> (f32, f32) {
    match kind {
        NodeKind::Note => (280.0, 170.0),
        NodeKind::Document => (256.0, 112.0),
        NodeKind::Image => (240.0, 110.0),
        NodeKind::LlmSynthesis => (260.0, 130.0),
        NodeKind::Concept => (240.0, 110.0),
    }
}

pub struct AppState {
    pub screen: AppScreen,
    pub projects_dir: PathBuf,
    pub recent_projects: Vec<ProjectMetadata>,
    pub new_project_input: TextEdit,

    // Active project state
    pub bundle: Option<ProjectBundle>,
    pub graph: StudyGraph,
    pub history: CommandHistory,
    pub canvas_mode: CanvasMode,

    // Selection
    pub selected_nodes: HashSet<NodeId>,
    pub selected_edge: Option<EdgeId>,

    // Side panels
    pub inspector_open: bool,
    pub ai_panel_open: bool,
    pub status_message: String,
    #[allow(dead_code)]
    pub is_ai_loading: bool,

    // Separate TextEdit instances per view region so simultaneous mounts never fight over focus.
    pub canvas_title_edit: TextEdit,
    pub canvas_content_edit: TextEdit,
    pub inspector_title_edit: TextEdit,
    pub inspector_content_edit: TextEdit,

    pub edit_edge_label: TextEdit,
    pub edit_edge_note: TextEdit,
    pub append_note_input: TextEdit,
    pub editing_node_id: Option<NodeId>,
    pub space_held: bool,
    pub canvas_origin: [f32; 2],

    // Integrated & Custom Document Reader state
    pub reader_prefs: ReaderPreferences,
    pub custom_reader_edit: TextEdit,
    pub reader_settings_open: bool,
    pub doc_view_tab: DocViewTab,
    pub active_pdf: Option<ActivePdfState>,
    pub image_cache: ImageCache,
    pub active_image: Option<(NodeId, PathBuf, rune::Image)>,

    // Wire draft
    #[allow(dead_code)]
    pub wire_draft: Option<ActiveWireDraft>,
}

impl Default for AppState {
    fn default() -> Self {
        let projects_dir = dirs_or_local_projects_dir();
        let reader_prefs = doc_reader::load_preferences(&projects_dir);
        let custom_reader_edit = TextEdit::new(&reader_prefs.custom_reader_path);

        let mut state = Self {
            screen: AppScreen::Dashboard,
            projects_dir,
            recent_projects: Vec::new(),
            new_project_input: TextEdit::new(""),
            bundle: None,
            graph: StudyGraph::new("Untitled Project"),
            history: CommandHistory::new(),
            canvas_mode: CanvasMode::Idle,
            selected_nodes: HashSet::new(),
            selected_edge: None,
            inspector_open: false,
            ai_panel_open: false,
            status_message: "Ready".into(),
            is_ai_loading: false,
            canvas_title_edit: TextEdit::new(""),
            canvas_content_edit: TextEdit::new(""),
            inspector_title_edit: TextEdit::new(""),
            inspector_content_edit: TextEdit::new(""),
            edit_edge_label: TextEdit::new(""),
            edit_edge_note: TextEdit::new(""),
            append_note_input: TextEdit::new(""),
            editing_node_id: None,
            space_held: false,
            canvas_origin: [0.0, 40.0],
            reader_prefs,
            custom_reader_edit,
            reader_settings_open: false,
            doc_view_tab: DocViewTab::Preview,
            active_pdf: None,
            image_cache: ImageCache::new(Arc::new(|| {})),
            active_image: None,
            wire_draft: None,
        };

        state.refresh_recent_projects();
        state
    }
}

fn dirs_or_local_projects_dir() -> PathBuf {
    let p = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("study_projects");
    let _ = std::fs::create_dir_all(&p);
    p
}

impl AppState {
    pub fn set_image_waker(&mut self, waker: Waker) {
        self.image_cache = ImageCache::new(waker);
    }

    /// Scan `projects_dir` for existing `.studynodes` folders.
    pub fn refresh_recent_projects(&mut self) {
        self.recent_projects.clear();
        if let Ok(entries) = std::fs::read_dir(&self.projects_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join(ProjectBundle::PROJECT_FILE).exists() {
                    let folder_name = entry.file_name().to_string_lossy().into_owned();
                    if let Ok((_, g)) = ProjectBundle::open(&path) {
                        self.recent_projects.push(ProjectMetadata {
                            folder_name,
                            title: g.title,
                            path,
                            node_count: g.nodes.len(),
                            edge_count: g.edges.len(),
                            last_modified: "Just now".into(),
                        });
                    }
                }
            }
        }
    }

    /// Create and open a new study project.
    pub fn create_project(&mut self, title: &str) {
        let clean_name = title
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>();
        let folder = format!("{}.studynodes", clean_name);

        match ProjectBundle::create(&self.projects_dir, &folder, title) {
            Ok((bundle, graph)) => {
                self.bundle = Some(bundle);
                self.graph = graph;
                self.history = CommandHistory::new();
                self.selected_nodes.clear();
                self.selected_edge = None;
                self.editing_node_id = None;
                self.inspector_open = false;
                self.ai_panel_open = false;
                self.active_pdf = None;
                self.active_image = None;
                self.doc_view_tab = DocViewTab::Preview;
                self.screen = AppScreen::Workspace;
                self.status_message = format!("Created '{}'", title);
            }
            Err(e) => {
                self.status_message = format!("Failed to create project: {}", e);
            }
        }
    }

    /// Open an existing project bundle.
    pub fn open_project(&mut self, path: PathBuf) {
        match ProjectBundle::open(&path) {
            Ok((bundle, graph)) => {
                self.bundle = Some(bundle);
                self.graph = graph;
                self.history = CommandHistory::new();
                self.selected_nodes.clear();
                self.selected_edge = None;
                self.editing_node_id = None;
                self.inspector_open = false;
                self.ai_panel_open = false;
                self.active_pdf = None;
                self.active_image = None;
                self.doc_view_tab = DocViewTab::Preview;
                self.screen = AppScreen::Workspace;
                self.status_message = format!("Opened '{}'", self.graph.title);
            }
            Err(e) => {
                self.status_message = format!("Failed to open project: {}", e);
            }
        }
    }

    /// Return to dashboard.
    pub fn close_to_dashboard(&mut self) {
        self.sync_node_edits();
        self.sync_edge_edits();
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
        self.screen = AppScreen::Dashboard;
        crate::touchpad_win32::set_canvas_bounds(None);
        self.refresh_recent_projects();
    }

    /// Persist canvas TextEdit buffers into the selected node and mirror to inspector without
    /// altering canvas caret or focus.
    pub fn sync_from_canvas_edits(&mut self) {
        if let Some(id) = self.selected_nodes.iter().next().cloned() {
            if let Some(node) = self.graph.get_node_mut(&id) {
                let title = self.canvas_title_edit.text.clone();
                let content = self.canvas_content_edit.text.clone();
                let changed = node.title != title || node.content != content;
                node.title = title;
                node.content = content;
                if self.inspector_title_edit.text != node.title {
                    self.inspector_title_edit = TextEdit::new(&node.title);
                }
                if self.inspector_content_edit.text != node.content {
                    self.inspector_content_edit = TextEdit::new(&node.content);
                }
                if changed {
                    if let Some(bundle) = &self.bundle {
                        let _ = bundle.save_graph_atomic(&self.graph);
                    }
                }
            }
        }
    }

    /// Persist inspector TextEdit buffers into the selected node and mirror to canvas without
    /// altering inspector caret or focus.
    pub fn sync_from_inspector_edits(&mut self) {
        if let Some(id) = self.selected_nodes.iter().next().cloned() {
            if let Some(node) = self.graph.get_node_mut(&id) {
                let title = self.inspector_title_edit.text.clone();
                let content = self.inspector_content_edit.text.clone();
                let changed = node.title != title || node.content != content;
                node.title = title;
                node.content = content;
                if self.canvas_title_edit.text != node.title {
                    self.canvas_title_edit = TextEdit::new(&node.title);
                }
                if self.canvas_content_edit.text != node.content {
                    self.canvas_content_edit = TextEdit::new(&node.content);
                }
                if changed {
                    if let Some(bundle) = &self.bundle {
                        let _ = bundle.save_graph_atomic(&self.graph);
                    }
                }
            }
        }
    }

    /// Sync whichever node edit buffer currently differs from the model.
    pub fn sync_node_edits(&mut self) {
        let Some(id) = self.selected_nodes.iter().next().cloned() else {
            return;
        };
        let Some((cur_title, cur_content)) = self
            .graph
            .get_node(&id)
            .map(|n| (n.title.clone(), n.content.clone()))
        else {
            return;
        };

        let canvas_dirty =
            self.canvas_title_edit.text != cur_title || self.canvas_content_edit.text != cur_content;
        let inspector_dirty = self.inspector_title_edit.text != cur_title
            || self.inspector_content_edit.text != cur_content;

        if canvas_dirty {
            self.sync_from_canvas_edits();
        } else if inspector_dirty {
            self.sync_from_inspector_edits();
        }
    }

    /// Sync active editing buffer back into the selected edge.
    pub fn sync_edge_edits(&mut self) {
        if let Some(id) = self.selected_edge.clone() {
            if let Some(edge) = self.graph.get_edge_mut(&id) {
                let label = self.edit_edge_label.text.clone();
                let note = self.edit_edge_note.text.clone();
                let changed = edge.label != label || edge.note != note;
                edge.label = label;
                edge.note = note;
                if changed {
                    if let Some(bundle) = &self.bundle {
                        let _ = bundle.save_graph_atomic(&self.graph);
                    }
                }
            }
        }
    }

    /// Resolve a node's stored asset file on disk, if any.
    pub fn resolved_node_asset_path(&self, node_id: &NodeId) -> Option<PathBuf> {
        let hash = self.graph.get_node(node_id)?.asset_hash.as_deref()?;
        self.bundle.as_ref()?.find_asset_path(hash)
    }

    /// Detect the active document format for a node.
    pub fn node_doc_format(&self, node_id: &NodeId) -> DocFormat {
        if let Some(path) = self.resolved_node_asset_path(node_id) {
            return doc_reader::detect_format(&path);
        }
        if let Some(node) = self.graph.get_node(node_id) {
            if node.kind == NodeKind::Image {
                return DocFormat::Image;
            }
        }
        DocFormat::Markdown
    }

    /// Ensure integrated reader media (Hayro PDF page or Rune image) is loaded for `node_id`.
    pub fn ensure_reader_media_for_node(&mut self, node_id: &NodeId, switch_tab: bool) {
        let Some(asset_path) = self.resolved_node_asset_path(node_id) else {
            self.active_pdf = None;
            self.active_image = None;
            if switch_tab {
                self.doc_view_tab = DocViewTab::Preview;
            }
            return;
        };

        let fmt = doc_reader::detect_format(&asset_path);
        match fmt {
            DocFormat::Pdf => {
                self.active_image = None;
                let already_loaded = self.active_pdf.as_ref().is_some_and(|p| {
                    &p.node_id == node_id && p.asset_path == asset_path && !switch_tab
                });
                if already_loaded {
                    return;
                }

                let pixels = Pixels::new();
                let mut state = ActivePdfState {
                    node_id: node_id.clone(),
                    asset_path: asset_path.clone(),
                    page_index: 0,
                    page_count: 1,
                    zoom: 1.5,
                    pixels,
                    page_size: (0, 0),
                    error: None,
                };
                match render_pdf_page(&asset_path, 0, state.zoom) {
                    Ok(rendered) => {
                        state.page_count = rendered.page_count;
                        state.page_size = (rendered.width, rendered.height);
                        state
                            .pixels
                            .set(rendered.width, rendered.height, rendered.rgba);
                    }
                    Err(e) => {
                        state.error = Some(e);
                    }
                }
                self.active_pdf = Some(state);
                if switch_tab {
                    self.doc_view_tab = DocViewTab::Pages;
                }
            }
            DocFormat::Image => {
                self.active_pdf = None;
                let already_loaded = self
                    .active_image
                    .as_ref()
                    .is_some_and(|(id, p, _)| id == node_id && p == &asset_path);
                if !already_loaded {
                    let img = self.image_cache.file(&asset_path, 760.0, 2.0);
                    self.active_image = Some((node_id.clone(), asset_path, img));
                }
                if switch_tab {
                    self.doc_view_tab = DocViewTab::Preview;
                }
            }
            _ => {
                self.active_pdf = None;
                self.active_image = None;
                if switch_tab {
                    self.doc_view_tab = DocViewTab::Preview;
                }
            }
        }
    }

    pub fn set_pdf_page(&mut self, page_index: usize) {
        let Some(pdf) = self.active_pdf.as_mut() else {
            return;
        };
        let clamped = page_index.min(pdf.page_count.saturating_sub(1));
        match render_pdf_page(&pdf.asset_path, clamped, pdf.zoom) {
            Ok(rendered) => {
                pdf.page_index = clamped;
                pdf.page_count = rendered.page_count;
                pdf.page_size = (rendered.width, rendered.height);
                pdf.error = None;
                pdf.pixels
                    .set(rendered.width, rendered.height, rendered.rgba);
                self.status_message = format!("PDF page {} / {}", clamped + 1, rendered.page_count);
            }
            Err(e) => {
                pdf.error = Some(e);
            }
        }
    }

    pub fn set_pdf_zoom(&mut self, next_zoom: f32) {
        let Some(pdf) = self.active_pdf.as_mut() else {
            return;
        };
        let zoom = next_zoom.clamp(0.75, 2.75);
        match render_pdf_page(&pdf.asset_path, pdf.page_index, zoom) {
            Ok(rendered) => {
                pdf.zoom = zoom;
                pdf.page_count = rendered.page_count;
                pdf.page_size = (rendered.width, rendered.height);
                pdf.error = None;
                pdf.pixels
                    .set(rendered.width, rendered.height, rendered.rgba);
            }
            Err(e) => {
                pdf.error = Some(e);
            }
        }
    }

    pub fn set_reader_mode(&mut self, mode: ReaderMode) {
        self.reader_prefs.mode = mode;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.status_message = match mode {
            ReaderMode::Integrated => "Switched to Integrated Reader".into(),
            ReaderMode::CustomExternal => "Switched to Custom External Reader".into(),
        };
    }

    pub fn save_custom_reader_path(&mut self, path: String) {
        self.reader_prefs.custom_reader_path = path.clone();
        self.custom_reader_edit = TextEdit::new(&path);
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.status_message = if path.trim().is_empty() {
            "Custom reader reset to system default".into()
        } else {
            "Saved custom reader executable".into()
        };
    }

    pub fn open_node_in_external_reader(&mut self, node_id: &NodeId) {
        self.sync_node_edits();
        let target_path = if let Some(asset) = self.resolved_node_asset_path(node_id) {
            asset
        } else if let Some(node) = self.graph.get_node(node_id) {
            let export_path = if let Some(bundle) = &self.bundle {
                bundle
                    .root_dir
                    .join(ProjectBundle::EXTRACTED_DIR)
                    .join(format!("{}.md", node.id.as_str()))
            } else {
                std::env::temp_dir().join(format!("studynodes_{}.md", node.id.as_str()))
            };
            let _ = std::fs::write(&export_path, &node.content);
            export_path
        } else {
            return;
        };

        match doc_reader::launch_external_reader(&target_path, &self.reader_prefs.custom_reader_path)
        {
            Ok(msg) => self.status_message = msg,
            Err(err) => self.status_message = err,
        }
    }

    pub fn toggle_selected_doc_task(&mut self, marker_offset: usize, checked: bool) {
        self.sync_node_edits();
        let Some(id) = self.selected_nodes.iter().next().cloned() else {
            return;
        };
        if let Some(node) = self.graph.get_node_mut(&id) {
            let bytes = node.content.as_bytes();
            if bytes.get(marker_offset..marker_offset + 3).is_some_and(|w| {
                w[0] == b'[' && matches!(w[1], b' ' | b'x' | b'X') && w[2] == b']'
            }) {
                let mut updated = bytes.to_vec();
                updated[marker_offset + 1] = if checked { b'x' } else { b' ' };
                if let Ok(s) = String::from_utf8(updated) {
                    node.content = s;
                }
            } else {
                node.content = doc_reader::toggle_markdown_task(&node.content, marker_offset);
            }
            self.canvas_content_edit = TextEdit::new(&node.content);
            self.inspector_content_edit = TextEdit::new(&node.content);
            self.status_message = "Updated Markdown checklist".into();
        }
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
    }

    /// Select a node and populate edit buffers (preserving active edits/caret when already singly selected).
    pub fn select_node(&mut self, id: NodeId, additive: bool) {
        if !additive && self.selected_nodes.len() == 1 && self.selected_nodes.contains(&id) {
            let mut open_doc = false;
            if let Some(n) = self.graph.get_node(&id) {
                if matches!(n.kind, NodeKind::Document | NodeKind::Image) {
                    self.inspector_open = true;
                    self.ai_panel_open = false;
                    open_doc = true;
                }
            }
            if open_doc {
                self.ensure_reader_media_for_node(&id, false);
            }
            return;
        }

        self.sync_node_edits();
        self.sync_edge_edits();
        if !additive {
            self.selected_nodes.clear();
        }
        self.selected_edge = None;
        self.selected_nodes.insert(id.clone());

        let mut node_kind = None;
        if let Some(n) = self.graph.get_node(&id) {
            self.canvas_title_edit = TextEdit::new(&n.title);
            self.canvas_content_edit = TextEdit::new(&n.content);
            self.inspector_title_edit = TextEdit::new(&n.title);
            self.inspector_content_edit = TextEdit::new(&n.content);
            node_kind = Some(n.kind);
            if matches!(n.kind, NodeKind::Document | NodeKind::Image) {
                self.inspector_open = true;
                self.ai_panel_open = false;
                self.editing_node_id = None;
            } else if n.kind == NodeKind::Note {
                self.editing_node_id = Some(id.clone());
            } else {
                self.editing_node_id = None;
            }
        }

        if matches!(node_kind, Some(NodeKind::Document | NodeKind::Image)) {
            self.ensure_reader_media_for_node(&id, true);
            if self.reader_prefs.mode == ReaderMode::CustomExternal
                && !cfg!(test)
                && !rune::is_headless()
            {
                self.open_node_in_external_reader(&id);
            }
        } else {
            self.active_pdf = None;
            self.active_image = None;
        }
    }

    /// Clear all active node/edge selections (e.g. clicking empty canvas).
    pub fn deselect_all(&mut self) {
        self.sync_node_edits();
        self.sync_edge_edits();
        self.selected_nodes.clear();
        self.selected_edge = None;
        self.editing_node_id = None;
        self.active_pdf = None;
        self.active_image = None;
        self.canvas_title_edit = TextEdit::new("");
        self.canvas_content_edit = TextEdit::new("");
        self.inspector_title_edit = TextEdit::new("");
        self.inspector_content_edit = TextEdit::new("");
    }

    /// Enter in-card editing mode directly on the canvas.
    #[allow(dead_code)]
    pub fn start_editing_node(&mut self, id: NodeId) {
        self.select_node(id.clone(), false);
        self.editing_node_id = Some(id);
        self.status_message = "Editing note on canvas".into();
    }

    /// Exit in-card editing mode and sync back to model.
    #[allow(dead_code)]
    pub fn stop_editing_node(&mut self) {
        self.sync_node_edits();
        self.editing_node_id = None;
        self.status_message = "Saved note".into();
    }

    /// Delete a node and all connected edges from the graph.
    pub fn delete_node(&mut self, id: &NodeId) {
        if let Some((removed, edges)) = self.graph.remove_node(id) {
            self.selected_nodes.remove(id);
            if self.editing_node_id.as_ref() == Some(id) {
                self.editing_node_id = None;
            }
            if self
                .active_pdf
                .as_ref()
                .is_some_and(|p| &p.node_id == id)
            {
                self.active_pdf = None;
            }
            if self
                .active_image
                .as_ref()
                .is_some_and(|(nid, _, _)| nid == id)
            {
                self.active_image = None;
            }
            if let Some(bundle) = &self.bundle {
                let _ = bundle.save_graph_atomic(&self.graph);
            }
            self.status_message = format!(
                "Deleted '{}' ({} link(s) removed)",
                removed.title,
                edges.len()
            );
        }
    }

    /// Run AI explanation for a single node.
    pub fn explain_node_ai(&mut self, id: &NodeId) {
        self.sync_node_edits();
        let context = studynodes_llm::LlmContext::build(&self.graph, &[id.clone()], "Explain Node");
        let provider = studynodes_llm::MockLlmProvider;
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
        if let Ok(rt) = rt {
            if let Ok(patch) = rt.block_on(provider.generate_patch(&context)) {
                let res = self.graph.validate_and_stage_patch(&patch);
                if let Some(node) = self.graph.get_node_mut(id) {
                    if node.summary.is_none() {
                        node.summary = Some(format!("AI Explanation: Key insights for {}.", node.title));
                    }
                }
                if let Some(bundle) = &self.bundle {
                    let _ = bundle.save_graph_atomic(&self.graph);
                }
                self.status_message = format!(
                    "✦ AI explained '{}' ({} proposed link(s))",
                    self.graph.get_node(id).map(|n| n.title.as_str()).unwrap_or("node"),
                    res.proposed_edges.len()
                );
            }
        }
    }

    /// Connect two nodes via an Edge with auto-persistence.
    pub fn connect_nodes(&mut self, from_id: &NodeId, to_id: &NodeId) {
        if from_id == to_id {
            return;
        }
        let from_title = self.graph.get_node(from_id).map(|n| n.title.clone()).unwrap_or_default();
        let to_title = self.graph.get_node(to_id).map(|n| n.title.clone()).unwrap_or_default();

        let edge = studynodes_graph::model::Edge::new(
            studynodes_graph::model::EdgeEndpoint {
                node: from_id.clone(),
                port: PortId::new(PortId::DEFAULT_OUT),
            },
            studynodes_graph::model::EdgeEndpoint {
                node: to_id.clone(),
                port: PortId::new(PortId::DEFAULT_IN),
            },
        );
        self.graph.add_edge(edge);
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
        self.status_message = format!("Linked '{}' → '{}'", from_title, to_title);
    }

    /// Select an edge and populate inspector buffer.
    pub fn select_edge(&mut self, id: EdgeId) {
        self.sync_node_edits();
        self.sync_edge_edits();
        self.selected_nodes.clear();
        self.editing_node_id = None;
        self.active_pdf = None;
        self.active_image = None;
        self.selected_edge = Some(id.clone());
        self.inspector_open = true;
        self.ai_panel_open = false;

        if let Some(e) = self.graph.get_edge(&id) {
            self.edit_edge_label = TextEdit::new(&e.label);
            self.edit_edge_note = TextEdit::new(&e.note);
        }
    }

    /// Append text to the currently selected node's content.
    pub fn append_to_selected_node(&mut self, text_to_append: &str) {
        self.sync_node_edits();
        if let Some(id) = self.selected_nodes.iter().next().cloned() {
            if let Some(node) = self.graph.get_node_mut(&id) {
                if !node.content.is_empty() && !node.content.ends_with('\n') {
                    node.content.push('\n');
                }
                node.content.push_str(text_to_append);
                self.canvas_content_edit = TextEdit::new(&node.content);
                self.inspector_content_edit = TextEdit::new(&node.content);
                self.status_message = "Appended to note".into();
            }
        }
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
    }

    /// Commit the text in `append_note_input` to the selected node.
    pub fn commit_append_input(&mut self) {
        let text = self.append_note_input.text.trim().to_string();
        if !text.is_empty() {
            self.append_to_selected_node(&text);
            self.append_note_input = TextEdit::new("");
        }
    }

    /// Create and select a new Note node with clean empty content ready for immediate typing.
    pub fn add_note_node(&mut self, pos_opt: Option<WorldPos>) -> NodeId {
        let pos = pos_opt.unwrap_or_else(|| {
            let center = self.graph.viewport.screen_to_world(ScreenPos::new(360.0, 240.0));
            WorldPos::new(center.x, center.y)
        });
        let mut node = studynodes_graph::model::Node::new(NodeKind::Note, "New Note", pos);
        node.size = node_base_size(NodeKind::Note);
        node.content = String::new();
        let id = node.id.clone();
        self.graph.add_node(node);
        self.select_node(id.clone(), false);
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
        self.status_message = "Created Note".into();
        id
    }

    /// Create and select a new Document node and open its right-side reader.
    pub fn add_document_node(&mut self, pos_opt: Option<WorldPos>) -> NodeId {
        let pos = pos_opt.unwrap_or_else(|| {
            let center = self.graph.viewport.screen_to_world(ScreenPos::new(420.0, 280.0));
            WorldPos::new(center.x, center.y)
        });
        let mut node = studynodes_graph::model::Node::new(NodeKind::Document, "New Document", pos);
        node.size = node_base_size(NodeKind::Document);
        node.content = "# New Document\n\nIngest a `.pdf`, `.docx`, `.epub`, `.odt`, `.pptx`, `.xlsx`, `.md`, `.html`, or `.rtf` file to read it here.\n\n- [ ] Ingest source file\n- [ ] Summarize key takeaways\n".into();
        let id = node.id.clone();
        self.graph.add_node(node);
        self.select_node(id.clone(), false);
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
        self.status_message = "Created Document".into();
        id
    }

    /// Load a rich sample document into a node.
    pub fn load_sample_document(&mut self, node_id: &NodeId) {
        if let Some(node) = self.graph.get_node_mut(node_id) {
            node.title = "Attention Is All You Need (Excerpt)".into();
            node.content = r#"# Attention Is All You Need
*Vaswani et al., NeurIPS 2017*

> We propose a new simple network architecture, the **Transformer**, based solely on attention mechanisms, dispensing with recurrence and convolutions entirely.

## Scaled Dot-Product Attention

```text
Attention(Q, K, V) = softmax((Q * K^T) / sqrt(d_k)) * V
```

## Architecture Comparison

| Mechanism | Complexity / Layer | Max Path Length |
| --- | --- | --- |
| Self-Attention | `O(n^2 * d)` | `O(1)` |
| Recurrent | `O(n * d^2)` | `O(n)` |
| Convolutional | `O(k * n * d^2)` | `O(log_k(n))` |

## Study Checklist
- [x] Understand scaled `1 / sqrt(d_k)` variance stabilization
- [ ] Trace multi-head projections across `h = 8` heads"#
                .into();
            self.canvas_title_edit = TextEdit::new(&node.title);
            self.canvas_content_edit = TextEdit::new(&node.content);
            self.inspector_title_edit = TextEdit::new(&node.title);
            self.inspector_content_edit = TextEdit::new(&node.content);
            self.active_pdf = None;
            self.active_image = None;
            self.doc_view_tab = DocViewTab::Preview;
            self.status_message = "Loaded styled Markdown paper".into();
        }
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
    }

    /// Ingest a file (PDF, DOCX, ODT, PPTX, XLSX, EPUB, MD, HTML, RTF, CSV, Image, Code) into a node.
    pub fn ingest_file_into_node(&mut self, node_id: &NodeId, path: &Path) {
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Imported Document".into());

        let extracted = extract_document_markdown(path).unwrap_or_else(|err| doc_reader::ExtractedDoc {
            format: doc_reader::detect_format(path),
            markdown: format!("# {}\n\n> Failed to parse file cleanly: {}", filename, err),
            page_count: None,
        });

        let hash_opt = if let Some(bundle) = &self.bundle {
            bundle.ingest_asset(path).ok().map(|(h, _)| {
                let _ = bundle.save_extracted_text(&h, &extracted.markdown);
                h
            })
        } else {
            None
        };

        if let Some(node) = self.graph.get_node_mut(node_id) {
            if let Some(hash) = hash_opt {
                node.asset_hash = Some(hash);
            }
            if node.title.is_empty()
                || node.title == "New Document"
                || node.title == "New Note"
            {
                node.title = filename.clone();
            }
            node.content = extracted.markdown;
            self.canvas_title_edit = TextEdit::new(&node.title);
            self.canvas_content_edit = TextEdit::new(&node.content);
            self.inspector_title_edit = TextEdit::new(&node.title);
            self.inspector_content_edit = TextEdit::new(&node.content);
        }

        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }

        self.ensure_reader_media_for_node(node_id, true);
        self.status_message = match extracted.page_count {
            Some(pages) => format!("Ingested '{}' ({} · {} pages)", filename, doc_reader::format_badge(extracted.format), pages),
            None => format!("Ingested '{}' ({})", filename, doc_reader::format_badge(extracted.format)),
        };
    }

    /// Ingest dropped files into active project.
    pub fn handle_dropped_files(&mut self, files: &[PathBuf], drop_screen_pos: ScreenPos) {
        let bundle = match &self.bundle {
            Some(b) => b.clone(),
            None => return,
        };

        let drop_world = self.graph.viewport.screen_to_world(drop_screen_pos);
        let mut offset_x = 0.0;
        let mut last_id = None;

        for path in files {
            if let Ok((hash, _)) = bundle.ingest_asset(path) {
                let filename = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Imported File".into());

                let extracted = extract_document_markdown(path).unwrap_or_else(|_| {
                    doc_reader::ExtractedDoc {
                        format: doc_reader::detect_format(path),
                        markdown: format!("# {}", filename),
                        page_count: None,
                    }
                });
                let _ = bundle.save_extracted_text(&hash, &extracted.markdown);

                let kind = match extracted.format {
                    DocFormat::Image => NodeKind::Image,
                    DocFormat::PlainText => NodeKind::Note,
                    _ => NodeKind::Document,
                };

                let mut node = studynodes_graph::model::Node::new(
                    kind,
                    filename,
                    WorldPos::new(drop_world.x + offset_x, drop_world.y),
                );
                node.size = node_base_size(kind);
                node.content = extracted.markdown;
                node.asset_hash = Some(hash);
                let id = node.id.clone();
                self.graph.add_node(node);
                last_id = Some(id);
                offset_x += 300.0;
            }
        }

        let _ = bundle.save_graph_atomic(&self.graph);
        if let Some(id) = last_id {
            self.select_node(id, false);
        }
        self.status_message = format!("Ingested {} file(s)", files.len());
    }

    /// Apply unlocked simultaneous 2D touchpad Pan and Zoom events.
    pub fn apply_touchpad_events(&mut self, events: Vec<TouchpadEvent>) -> bool {
        if self.screen != AppScreen::Workspace || events.is_empty() {
            return false;
        }
        let mut changed = false;
        for ev in events {
            match ev {
                TouchpadEvent::Pan { dx, dy } => {
                    self.graph.viewport.pan_by_screen_delta(dx, dy);
                    changed = true;
                }
                TouchpadEvent::Zoom { factor, x, y } => {
                    let local = ScreenPos::new(x - self.canvas_origin[0], y - self.canvas_origin[1]);
                    self.graph.viewport.zoom_at(local, factor);
                    changed = true;
                }
            }
        }
        changed
    }
}
