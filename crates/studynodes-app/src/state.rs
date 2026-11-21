use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::SystemTime;

use rune::{ImageCache, Pixels, TextEdit, Waker, WindowHandle};
use serde::{Deserialize, Serialize};

use studynodes_graph::coords::{ScreenPos, WorldPos};
use studynodes_graph::model::{
    Edge, EdgeEndpoint, EdgeId, EdgeRelation, EdgeStatus, NodeId, NodeKind, PortDirection, PortId,
};
use studynodes_graph::{CommandHistory, ProjectBundle, StudyGraph};
use studynodes_llm::LlmProvider;
use studynodes_vision::{
    LocalVisionService, NeuralModelStatus, OcrEnginePreference, OcrMode, OcrOutput,
    TranslationOutput, VisionConfig, VisionService,
};

use crate::doc_reader::{
    self, DocFormat, ReaderMode, ReaderPreferences, extract_document_markdown, render_pdf_page,
};
use crate::formula_math;
use crate::reader_window::{
    ReaderBridge, ReaderCommand, ReaderSnapshot, SectionChildSnapshot, SectionEdgeSnapshot,
    can_pop_out, open_reader_window,
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

#[derive(Clone)]
pub struct CardPdfPreview {
    pub page_count: usize,
    pub pixels: Pixels,
}

pub struct WorkspaceTab {
    pub path: PathBuf,
    pub title: String,
    pub bundle: ProjectBundle,
    pub graph: StudyGraph,
    pub selected_nodes: HashSet<NodeId>,
    pub selected_edge: Option<EdgeId>,
    pub editing_node_id: Option<NodeId>,
    pub doc_view_tab: DocViewTab,
    pub inspector_open: bool,
    pub ai_panel_open: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub folder_name: String,
    pub title: String,
    pub path: PathBuf,
    pub node_count: usize,
    #[serde(default)]
    pub doc_count: usize,
    pub edge_count: usize,
    pub last_modified: String,
    #[serde(default)]
    pub modified_secs: u64,
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
    DraggingSection {
        node_id: NodeId,
        start_pos: WorldPos,
        children: Vec<(NodeId, WorldPos)>,
    },
    ResizingSection {
        node_id: NodeId,
        start_size: (f32, f32),
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
        NodeKind::Document => (272.0, 214.0),
        NodeKind::Image => (264.0, 200.0),
        NodeKind::LlmSynthesis => (260.0, 130.0),
        NodeKind::Concept => (240.0, 110.0),
        NodeKind::Section => (640.0, 400.0),
        NodeKind::Excerpt => (265.0, 165.0),
        NodeKind::Flashcard => (270.0, 185.0),
        NodeKind::Formula => (272.0, 168.0),
        NodeKind::Checklist => (260.0, 195.0),
        NodeKind::Sequence => (248.0, 145.0),
    }
}

/// Effective world dimensions `(w, h)` for a specific node instance (honors resizable `Section` bounds).
pub fn node_world_size(node: &studynodes_graph::model::Node) -> (f32, f32) {
    if node.kind == NodeKind::Section {
        let w = if node.size.0 >= 180.0 {
            node.size.0
        } else {
            640.0
        };
        let h = if node.meta.section_framed {
            if node.size.1 >= 120.0 {
                node.size.1
            } else {
                400.0
            }
        } else {
            44.0
        };
        (w, h)
    } else {
        node_base_size(node.kind)
    }
}

pub enum VisionJobResult {
    OcrFinished {
        node_id: NodeId,
        mode: OcrMode,
        result: Result<OcrOutput, String>,
    },
    TranslationFinished {
        node_id: NodeId,
        target_lang: String,
        result: Result<TranslationOutput, String>,
    },
    ModelsDownloaded {
        result: Result<NeuralModelStatus, String>,
    },
}

pub struct AppState {
    pub screen: AppScreen,
    pub projects_dir: PathBuf,
    pub recent_projects: Vec<ProjectMetadata>,
    pub new_project_input: TextEdit,
    pub project_search_input: TextEdit,
    pub confirm_delete_project: Option<PathBuf>,

    // Multi-project open workspace tabs
    pub tabs: Vec<WorkspaceTab>,
    pub active_tab: Option<usize>,

    // Active project state
    pub bundle: Option<ProjectBundle>,
    pub graph: StudyGraph,
    pub history: CommandHistory,
    pub canvas_mode: CanvasMode,

    // Selection
    pub selected_nodes: HashSet<NodeId>,
    pub selected_edge: Option<EdgeId>,

    // Side panels & pop-out window
    pub inspector_open: bool,
    pub inspector_width: f32,
    pub inspector_resize_start_w: f32,
    pub inspector_popped_out: bool,
    pub reader_bridge: Arc<Mutex<ReaderBridge>>,
    pub popout_window: Option<WindowHandle>,
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
    pub canvas_size: [f32; 2],

    // Integrated & Custom Document Reader + Card Preview state
    pub reader_prefs: ReaderPreferences,
    pub reader_read_only: bool,
    pub custom_reader_edit: TextEdit,
    pub reader_settings_open: bool,
    pub doc_view_tab: DocViewTab,
    pub active_pdf: Option<ActivePdfState>,
    pub image_cache: ImageCache,
    pub active_image: Option<(NodeId, PathBuf, rune::Image)>,
    pub card_pdf_previews: HashMap<(NodeId, usize), CardPdfPreview>,
    pub card_image_previews: HashMap<NodeId, rune::Image>,

    // Wire draft
    #[allow(dead_code)]
    pub wire_draft: Option<ActiveWireDraft>,
    pub show_block_menu: bool,
    pub flipped_flashcards: HashSet<NodeId>,
    pub inspector_answer_edit: TextEdit,
    pub hide_all_edge_labels: bool,
    pub wire_anim_phase: f32,
    pub popped_out_node_id: Option<NodeId>,

    // Background OCR & Translation pipeline (non-blocking)
    pub vision_service_override: Option<Arc<dyn VisionService>>,
    pub vision_tx: mpsc::Sender<VisionJobResult>,
    pub vision_rx: mpsc::Receiver<VisionJobResult>,
    pub vision_waker: Option<Waker>,
    pub vision_pending_nodes: HashSet<NodeId>,
    pub vision_downloading_models: bool,
    pub neural_models_ready: bool,
    pub vision_last_error: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::with_projects_dir(dirs_or_local_projects_dir())
    }
}

fn dirs_or_local_projects_dir() -> PathBuf {
    let p = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("study_projects");
    let _ = std::fs::create_dir_all(&p);
    p
}

fn format_modified_time(path: &Path) -> (String, u64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return ("Unknown".into(), 0);
    };
    let Ok(modified) = meta.modified() else {
        return ("Just now".into(), 0);
    };
    let secs = modified
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let label = match SystemTime::now().duration_since(modified) {
        Ok(elapsed) => {
            let s = elapsed.as_secs();
            if s < 60 {
                "Just now".into()
            } else if s < 3600 {
                format!("{}m ago", s / 60)
            } else if s < 86400 {
                format!("{}h ago", s / 3600)
            } else {
                format!("{}d ago", s / 86400)
            }
        }
        Err(_) => "Just now".into(),
    };
    (label, secs)
}

pub fn same_project_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

impl AppState {
    pub fn with_projects_dir(projects_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&projects_dir);
        let reader_prefs = doc_reader::load_preferences(&projects_dir);
        let custom_reader_edit = TextEdit::new(&reader_prefs.custom_reader_path);
        let inspector_width = reader_prefs.inspector_width.clamp(300.0, 860.0);
        let reader_read_only = reader_prefs.read_only;
        let neural_models_ready =
            studynodes_vision::neural_ocr::model_status(&projects_dir.join("models")).is_ready();
        let (vision_tx, vision_rx) = mpsc::channel();

        let mut state = Self {
            screen: AppScreen::Dashboard,
            projects_dir,
            recent_projects: Vec::new(),
            new_project_input: TextEdit::new(""),
            project_search_input: TextEdit::new(""),
            confirm_delete_project: None,
            tabs: Vec::new(),
            active_tab: None,
            bundle: None,
            graph: StudyGraph::new("Untitled Project"),
            history: CommandHistory::new(),
            canvas_mode: CanvasMode::Idle,
            selected_nodes: HashSet::new(),
            selected_edge: None,
            inspector_open: false,
            inspector_width,
            inspector_resize_start_w: inspector_width,
            inspector_popped_out: false,
            reader_bridge: Arc::new(Mutex::new(ReaderBridge::default())),
            popout_window: None,
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
            canvas_size: [1280.0, 734.0],
            reader_prefs,
            reader_read_only,
            custom_reader_edit,
            reader_settings_open: false,
            doc_view_tab: DocViewTab::Preview,
            active_pdf: None,
            image_cache: ImageCache::new(Arc::new(|| {})),
            active_image: None,
            card_pdf_previews: HashMap::new(),
            card_image_previews: HashMap::new(),
            wire_draft: None,
            show_block_menu: false,
            flipped_flashcards: HashSet::new(),
            inspector_answer_edit: TextEdit::new(""),
            hide_all_edge_labels: false,
            wire_anim_phase: 0.35,
            popped_out_node_id: None,
            vision_service_override: None,
            vision_tx,
            vision_rx,
            vision_waker: None,
            vision_pending_nodes: HashSet::new(),
            vision_downloading_models: false,
            neural_models_ready,
            vision_last_error: None,
        };

        state.refresh_recent_projects();
        state
    }

    pub fn set_wakers(&mut self, waker: Waker) {
        self.image_cache = ImageCache::new(waker.clone());
        self.vision_waker = Some(waker.clone());
        if let Ok(mut b) = self.reader_bridge.lock() {
            b.main_waker = Some(waker);
        }
    }

    /// Scan `projects_dir` for existing `.studynodes` folders.
    pub fn refresh_recent_projects(&mut self) {
        self.recent_projects.clear();
        if let Ok(entries) = std::fs::read_dir(&self.projects_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let project_file = path.join(ProjectBundle::PROJECT_FILE);
                if path.is_dir() && project_file.exists() {
                    let folder_name = entry.file_name().to_string_lossy().into_owned();
                    if let Ok((_, g)) = ProjectBundle::open(&path) {
                        let doc_count = g
                            .nodes
                            .values()
                            .filter(|n| n.kind == NodeKind::Document)
                            .count();
                        let (last_modified, modified_secs) = format_modified_time(&project_file);
                        self.recent_projects.push(ProjectMetadata {
                            folder_name,
                            title: g.title,
                            path,
                            node_count: g.nodes.len(),
                            doc_count,
                            edge_count: g.edges.len(),
                            last_modified,
                            modified_secs,
                        });
                    }
                }
            }
        }
        self.recent_projects.sort_by(|a, b| {
            b.modified_secs
                .cmp(&a.modified_secs)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
    }

    /// Snapshot live edits & graph back into the active tab slot and save atomically.
    pub fn save_active_into_tab(&mut self) {
        self.sync_node_edits();
        self.sync_edge_edits();
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
        if let Some(idx) = self.active_tab {
            if let Some(tab) = self.tabs.get_mut(idx) {
                tab.title = self.graph.title.clone();
                tab.graph = self.graph.clone();
                if let Some(bundle) = &self.bundle {
                    tab.bundle = bundle.clone();
                }
                tab.selected_nodes = self.selected_nodes.clone();
                tab.selected_edge = self.selected_edge.clone();
                tab.editing_node_id = self.editing_node_id.clone();
                tab.doc_view_tab = self.doc_view_tab;
                tab.inspector_open = self.inspector_open;
                tab.ai_panel_open = self.ai_panel_open;
            }
        }
    }

    fn load_tab_into_active(&mut self, idx: usize) {
        let Some(tab) = self.tabs.get(idx) else {
            return;
        };
        self.active_tab = Some(idx);
        self.bundle = Some(tab.bundle.clone());
        self.graph = tab.graph.clone();
        self.history = CommandHistory::new();
        self.canvas_mode = CanvasMode::Idle;
        self.selected_nodes = tab.selected_nodes.clone();
        self.selected_edge = tab.selected_edge.clone();
        self.editing_node_id = tab.editing_node_id.clone();
        self.doc_view_tab = tab.doc_view_tab;
        self.inspector_open = if self.inspector_popped_out {
            false
        } else {
            tab.inspector_open
        };
        self.ai_panel_open = tab.ai_panel_open;
        self.show_block_menu = false;
        self.flipped_flashcards.clear();
        self.popped_out_node_id = None;
        self.active_pdf = None;
        self.active_image = None;
        self.card_pdf_previews.clear();
        self.card_image_previews.clear();

        self.canvas_title_edit = TextEdit::new("");
        self.canvas_content_edit = TextEdit::new("");
        self.inspector_title_edit = TextEdit::new("");
        self.inspector_content_edit = TextEdit::new("");
        self.inspector_answer_edit = TextEdit::new("");
        self.edit_edge_label = TextEdit::new("");
        self.edit_edge_note = TextEdit::new("");

        if let Some(nid) = self.selected_nodes.iter().next().cloned() {
            if let Some(n) = self.graph.get_node(&nid) {
                self.canvas_title_edit = TextEdit::new(&n.title);
                self.canvas_content_edit = TextEdit::new(&n.content);
                self.inspector_title_edit = TextEdit::new(&n.title);
                self.inspector_content_edit = TextEdit::new(&n.content);
                self.inspector_answer_edit = TextEdit::new(n.meta.answer.as_deref().unwrap_or(""));
                if matches!(n.kind, NodeKind::Document | NodeKind::Image) {
                    self.ensure_reader_media_for_node(&nid, false);
                }
            }
        } else if let Some(eid) = self.selected_edge.clone() {
            if let Some(e) = self.graph.get_edge(&eid) {
                self.edit_edge_label = TextEdit::new(&e.label);
                self.edit_edge_note = TextEdit::new(&e.note);
            }
        }

        self.refresh_card_previews();
        self.push_reader_snapshot();
        self.status_message = format!("Active workspace: '{}'", self.graph.title);
    }

    /// Create and open a new study project as a workspace tab.
    pub fn create_project(&mut self, title: &str) {
        self.save_active_into_tab();

        let trimmed = title.trim();
        let title = if trimmed.is_empty() {
            "New Study Project"
        } else {
            trimmed
        };

        let mut clean_name: String = title
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect();
        if clean_name.is_empty() {
            clean_name = "Study_Project".into();
        }

        let mut folder = format!("{}.studynodes", clean_name);
        let mut suffix = 2usize;
        while self.projects_dir.join(&folder).exists() {
            folder = format!("{}_{}.studynodes", clean_name, suffix);
            suffix += 1;
        }

        match ProjectBundle::create(&self.projects_dir, &folder, title) {
            Ok((bundle, graph)) => {
                let tab = WorkspaceTab {
                    path: bundle.root_dir.clone(),
                    title: title.to_string(),
                    bundle,
                    graph,
                    selected_nodes: HashSet::new(),
                    selected_edge: None,
                    editing_node_id: None,
                    doc_view_tab: DocViewTab::Preview,
                    inspector_open: false,
                    ai_panel_open: false,
                };
                self.tabs.push(tab);
                let new_idx = self.tabs.len() - 1;
                self.load_tab_into_active(new_idx);
                self.screen = AppScreen::Workspace;
                self.confirm_delete_project = None;
                self.refresh_recent_projects();
                self.status_message = format!("Created '{}'", title);
            }
            Err(e) => {
                self.status_message = format!("Failed to create project: {}", e);
            }
        }
    }

    /// Create a project pre-seeded with a starter structure for Dashboard template chips.
    pub fn create_project_from_template(&mut self, title: &str, template: &str) {
        self.create_project(title);
        if self.bundle.is_none() {
            return;
        }

        match template {
            "literature" => {
                let mut doc = studynodes_graph::model::Node::new(
                    NodeKind::Document,
                    "Primary Paper",
                    WorldPos::new(60.0, 90.0),
                );
                doc.size = node_base_size(doc.kind);
                doc.content = "# Literature Overview\n\n## Research Question\nState the core hypothesis and scope of the paper.\n\n## Methodology & Evidence\n- [ ] Ingest PDF / DOCX source file\n- [ ] Extract key empirical results\n\n## Open Limitations\nDocument assumptions and follow-up questions.\n".into();
                let doc_id = doc.id.clone();
                self.graph.add_node(doc);

                let mut note = studynodes_graph::model::Node::new(
                    NodeKind::Note,
                    "Critique & Takeaways",
                    WorldPos::new(430.0, 110.0),
                );
                note.size = node_base_size(note.kind);
                note.content = "Key claims, threats to validity, and links to prior papers.".into();
                let note_id = note.id.clone();
                self.graph.add_node(note);

                self.connect_nodes(&doc_id, &note_id);
                self.select_node(doc_id, false);
            }
            "exam" => {
                let mut doc = studynodes_graph::model::Node::new(
                    NodeKind::Document,
                    "Exam Syllabus & Guide",
                    WorldPos::new(60.0, 90.0),
                );
                doc.size = node_base_size(doc.kind);
                doc.content = "# Exam Study Plan\n\n## Core Topics\n- [ ] Definitions and first-principles derivations\n- [ ] Worked practice problems\n\n## High-Yield Formulas\nAdd derivations, corner cases, and common pitfalls.\n".into();
                let doc_id = doc.id.clone();
                self.graph.add_node(doc);

                let mut note = studynodes_graph::model::Node::new(
                    NodeKind::Note,
                    "Active Recall Prompts",
                    WorldPos::new(430.0, 110.0),
                );
                note.size = node_base_size(note.kind);
                note.content = "Q1: What assumptions break down first?\nQ2: Summarize without looking.".into();
                let note_id = note.id.clone();
                self.graph.add_node(note);

                self.connect_nodes(&doc_id, &note_id);
                self.select_node(doc_id, false);
            }
            "concept" => {
                let mut c1 = studynodes_graph::model::Node::new(
                    NodeKind::Concept,
                    "Core Thesis",
                    WorldPos::new(80.0, 140.0),
                );
                c1.size = node_base_size(c1.kind);
                c1.content = "Anchor idea connecting evidence and counter-arguments.".into();
                let c1_id = c1.id.clone();
                self.graph.add_node(c1);

                let mut n1 = studynodes_graph::model::Node::new(
                    NodeKind::Note,
                    "Supporting Evidence",
                    WorldPos::new(420.0, 110.0),
                );
                n1.size = node_base_size(n1.kind);
                n1.content = "Add examples, intuitions, or empirical observations.".into();
                let n1_id = n1.id.clone();
                self.graph.add_node(n1);

                self.connect_nodes(&c1_id, &n1_id);
                self.select_node(n1_id, false);
            }
            _ => {}
        }

        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
        self.save_active_into_tab();
        self.refresh_card_previews();
        self.refresh_recent_projects();
        self.push_reader_snapshot();
    }

    /// Open an existing project bundle or focus its already-open tab.
    pub fn open_project(&mut self, path: PathBuf) {
        self.save_active_into_tab();

        if let Some(existing_idx) = self
            .tabs
            .iter()
            .position(|t| same_project_path(&t.path, &path))
        {
            self.load_tab_into_active(existing_idx);
            self.screen = AppScreen::Workspace;
            self.confirm_delete_project = None;
            return;
        }

        match ProjectBundle::open(&path) {
            Ok((bundle, graph)) => {
                let tab = WorkspaceTab {
                    path: bundle.root_dir.clone(),
                    title: graph.title.clone(),
                    bundle,
                    graph,
                    selected_nodes: HashSet::new(),
                    selected_edge: None,
                    editing_node_id: None,
                    doc_view_tab: DocViewTab::Preview,
                    inspector_open: false,
                    ai_panel_open: false,
                };
                self.tabs.push(tab);
                let idx = self.tabs.len() - 1;
                self.load_tab_into_active(idx);
                self.screen = AppScreen::Workspace;
                self.confirm_delete_project = None;
            }
            Err(e) => {
                self.status_message = format!("Failed to open project: {}", e);
            }
        }
    }

    /// Switch directly between open project tabs.
    pub fn switch_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        if self.active_tab == Some(idx) {
            self.screen = AppScreen::Workspace;
            return;
        }
        self.save_active_into_tab();
        self.load_tab_into_active(idx);
        self.screen = AppScreen::Workspace;
    }

    /// Save and close a project tab, focusing an adjacent tab or Dashboard if none remain.
    pub fn close_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        let is_active = self.active_tab == Some(idx);
        if is_active {
            self.sync_node_edits();
            self.sync_edge_edits();
            if let Some(bundle) = &self.bundle {
                let _ = bundle.save_graph_atomic(&self.graph);
            }
        }
        let closed_title = self.tabs[idx].title.clone();
        self.tabs.remove(idx);

        if self.tabs.is_empty() {
            self.active_tab = None;
            self.bundle = None;
            self.graph = StudyGraph::new("Untitled Project");
            self.selected_nodes.clear();
            self.selected_edge = None;
            self.editing_node_id = None;
            self.inspector_open = false;
            self.ai_panel_open = false;
            self.active_pdf = None;
            self.active_image = None;
            self.card_pdf_previews.clear();
            self.card_image_previews.clear();
            self.screen = AppScreen::Dashboard;
            crate::touchpad_win32::set_canvas_bounds(None);
            self.refresh_recent_projects();
            self.push_reader_snapshot();
        } else if is_active {
            let next_idx = idx.min(self.tabs.len() - 1);
            self.load_tab_into_active(next_idx);
        } else if let Some(cur) = self.active_tab {
            if idx < cur {
                self.active_tab = Some(cur - 1);
            }
        }
        self.status_message = format!("Closed tab '{}'", closed_title);
    }

    /// Delete a project folder from disk (closing any open tab for it first without re-saving).
    pub fn delete_project(&mut self, path: &Path) {
        let in_projects_dir = path.starts_with(&self.projects_dir)
            || match (
                std::fs::canonicalize(path),
                std::fs::canonicalize(&self.projects_dir),
            ) {
                (Ok(p), Ok(root)) => p.starts_with(&root) && p != root,
                _ => false,
            };
        if !in_projects_dir || path == self.projects_dir {
            self.status_message = "Refused to delete path outside workspace directory".into();
            return;
        }

        let deleted_title = self
            .recent_projects
            .iter()
            .find(|p| same_project_path(&p.path, path))
            .map(|p| p.title.clone())
            .unwrap_or_else(|| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("workspace")
                    .to_string()
            });

        // Remove any matching open tab without writing to the folder we are about to remove.
        if let Some(tab_idx) = self
            .tabs
            .iter()
            .position(|t| same_project_path(&t.path, path))
        {
            let was_active = self.active_tab == Some(tab_idx);
            self.tabs.remove(tab_idx);
            if self.tabs.is_empty() {
                self.active_tab = None;
                self.bundle = None;
                self.graph = StudyGraph::new("Untitled Project");
                self.selected_nodes.clear();
                self.selected_edge = None;
                self.editing_node_id = None;
                self.inspector_open = false;
                self.ai_panel_open = false;
                self.active_pdf = None;
                self.active_image = None;
                self.card_pdf_previews.clear();
                self.card_image_previews.clear();
                self.push_reader_snapshot();
            } else if was_active {
                let next_idx = tab_idx.min(self.tabs.len() - 1);
                self.load_tab_into_active(next_idx);
            } else if let Some(cur) = self.active_tab {
                if tab_idx < cur {
                    self.active_tab = Some(cur - 1);
                }
            }
        }

        self.confirm_delete_project = None;
        match std::fs::remove_dir_all(path) {
            Ok(_) => {
                self.refresh_recent_projects();
                self.status_message = format!("Deleted '{}'", deleted_title);
            }
            Err(e) => {
                self.status_message = format!("Failed to delete '{}': {}", deleted_title, e);
            }
        }
    }

    /// Return to dashboard while keeping open workspace tabs preserved.
    pub fn close_to_dashboard(&mut self) {
        self.save_active_into_tab();
        self.confirm_delete_project = None;
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
                    if let Some(idx) = self.active_tab {
                        if let Some(tab) = self.tabs.get_mut(idx) {
                            tab.title = self.graph.title.clone();
                        }
                    }
                    if let Some(bundle) = &self.bundle {
                        let _ = bundle.save_graph_atomic(&self.graph);
                    }
                    self.push_reader_snapshot();
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
                let answer = self.inspector_answer_edit.text.clone();
                let answer_changed = node.kind == NodeKind::Flashcard
                    && node.meta.answer.as_deref().unwrap_or("") != answer;
                let changed = node.title != title || node.content != content || answer_changed;
                node.title = title;
                node.content = content;
                if node.kind == NodeKind::Flashcard && answer_changed {
                    node.meta.answer = if answer.trim().is_empty() {
                        None
                    } else {
                        Some(answer)
                    };
                }
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
                    self.push_reader_snapshot();
                }
            }
        }
    }

    /// Sync whichever node edit buffer currently differs from the model.
    pub fn sync_node_edits(&mut self) {
        let Some(id) = self.selected_nodes.iter().next().cloned() else {
            return;
        };
        let Some((cur_title, cur_content, cur_answer, is_flashcard)) =
            self.graph.get_node(&id).map(|n| {
                (
                    n.title.clone(),
                    n.content.clone(),
                    n.meta.answer.clone().unwrap_or_default(),
                    n.kind == NodeKind::Flashcard,
                )
            })
        else {
            return;
        };

        let canvas_dirty =
            self.canvas_title_edit.text != cur_title || self.canvas_content_edit.text != cur_content;
        let inspector_dirty = self.inspector_title_edit.text != cur_title
            || self.inspector_content_edit.text != cur_content
            || (is_flashcard && self.inspector_answer_edit.text != cur_answer);

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
                let desired_page = self
                    .graph
                    .get_node(node_id)
                    .map(|n| n.preview_page)
                    .unwrap_or(0);
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
                    page_index: desired_page,
                    page_count: 1,
                    zoom: 1.5,
                    pixels,
                    page_size: (0, 0),
                    error: None,
                };
                match render_pdf_page(&asset_path, desired_page, state.zoom) {
                    Ok(rendered) => {
                        state.page_count = rendered.page_count;
                        state.page_index = desired_page.min(rendered.page_count.saturating_sub(1));
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
        let mut updated_node_id = None;
        let mut updated_page = page_index;
        {
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
                    updated_node_id = Some(pdf.node_id.clone());
                    updated_page = clamped;
                    self.status_message =
                        format!("PDF page {} / {}", clamped + 1, rendered.page_count);
                }
                Err(e) => {
                    pdf.error = Some(e);
                }
            }
        }

        if let Some(nid) = updated_node_id {
            if let Some(node) = self.graph.get_node_mut(&nid) {
                node.preview_page = updated_page;
            }
            if let Some(bundle) = &self.bundle {
                let _ = bundle.save_graph_atomic(&self.graph);
            }
            self.ensure_card_preview(&nid);
            self.push_reader_snapshot();
        }
    }

    pub fn set_pdf_zoom(&mut self, next_zoom: f32) {
        let mut ok = false;
        {
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
                    ok = true;
                }
                Err(e) => {
                    pdf.error = Some(e);
                }
            }
        }
        if ok {
            self.push_reader_snapshot();
        }
    }

    /// Ensure miniature card preview media (PDF thumbnail or Image thumbnail) is cached for `node_id`.
    pub fn ensure_card_preview(&mut self, node_id: &NodeId) {
        let Some(asset_path) = self.resolved_node_asset_path(node_id) else {
            return;
        };
        let fmt = doc_reader::detect_format(&asset_path);
        match fmt {
            DocFormat::Pdf => {
                let desired = self
                    .graph
                    .get_node(node_id)
                    .map(|n| n.preview_page)
                    .unwrap_or(0);
                if self
                    .card_pdf_previews
                    .contains_key(&(node_id.clone(), desired))
                {
                    return;
                }
                if let Ok(rendered) = render_pdf_page(&asset_path, desired, 0.75) {
                    let clamped = desired.min(rendered.page_count.saturating_sub(1));
                    if let Some(n) = self.graph.get_node_mut(node_id) {
                        n.preview_page = clamped;
                    }
                    let pixels = Pixels::new();
                    pixels.set(rendered.width, rendered.height, rendered.rgba);
                    self.card_pdf_previews.insert(
                        (node_id.clone(), clamped),
                        CardPdfPreview {
                            page_count: rendered.page_count,
                            pixels,
                        },
                    );
                }
            }
            DocFormat::Image => {
                if !self.card_image_previews.contains_key(node_id) {
                    let img = self.image_cache.file(&asset_path, 280.0, 2.0);
                    self.card_image_previews.insert(node_id.clone(), img);
                }
            }
            _ => {}
        }
    }

    pub fn refresh_card_previews(&mut self) {
        let ids: Vec<NodeId> = self
            .graph
            .nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::Document | NodeKind::Image))
            .map(|n| n.id.clone())
            .collect();
        for id in ids {
            self.ensure_card_preview(&id);
        }
    }

    /// Step the preview page/section displayed directly on a canvas Document card.
    pub fn step_node_preview_page(&mut self, node_id: &NodeId, delta: isize) {
        let is_pdf = self.node_doc_format(node_id) == DocFormat::Pdf
            && self.resolved_node_asset_path(node_id).is_some();
        if is_pdf {
            self.ensure_card_preview(node_id);
        }

        let Some(node) = self.graph.get_node(node_id) else {
            return;
        };
        let cur = node.preview_page;
        let total = if is_pdf {
            self.card_pdf_previews
                .get(&(node_id.clone(), cur))
                .map(|p| p.page_count)
                .unwrap_or(1)
                .max(1)
        } else {
            doc_reader::split_document_preview_pages(&node.content, 7)
                .len()
                .max(1)
        };

        let next = (cur as isize + delta).clamp(0, total.saturating_sub(1) as isize) as usize;
        if let Some(n) = self.graph.get_node_mut(node_id) {
            n.preview_page = next;
        }
        if is_pdf {
            self.ensure_card_preview(node_id);
        }
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
        if self
            .active_pdf
            .as_ref()
            .is_some_and(|p| &p.node_id == node_id)
        {
            self.set_pdf_page(next);
        }
        self.status_message = format!("Card preview page {} / {}", next + 1, total);
    }

    /// Push the active document/node snapshot to the pop-out reader bridge and wake it.
    pub fn push_reader_snapshot(&mut self) {
        let selected_id = self.selected_nodes.iter().next().cloned();
        let target_id = selected_id
            .as_ref()
            .filter(|id| self.graph.get_node(id).is_some_and(|n| can_pop_out(n.kind)))
            .cloned()
            .or_else(|| {
                self.popped_out_node_id
                    .as_ref()
                    .filter(|id| self.graph.get_node(id).is_some_and(|n| can_pop_out(n.kind)))
                    .cloned()
            });

        let snapshot_data = target_id.as_ref().and_then(|id| {
            self.graph.get_node(id).map(|n| {
                let fmt = self.node_doc_format(id);
                let asset = self.resolved_node_asset_path(id);
                let flipped = self.flipped_flashcards.contains(id);
                let sec_size = node_world_size(n);

                let (sec_children, sec_edges) = if n.kind == NodeKind::Section {
                    let captured = self.capture_section_children(&n.id);
                    let mut child_map: HashMap<NodeId, ((f32, f32), (f32, f32))> = HashMap::new();
                    let mut children_snaps = Vec::new();
                    for (cid, _) in &captured {
                        if let Some(cn) = self.graph.get_node(cid) {
                            let rel = (cn.pos.x - n.pos.x, cn.pos.y - n.pos.y);
                            let sz = node_world_size(cn);
                            child_map.insert(cid.clone(), (rel, sz));
                            children_snaps.push(SectionChildSnapshot {
                                id: cn.id.clone(),
                                kind: cn.kind,
                                title: cn.title.clone(),
                                content: cn.content.clone(),
                                rel_pos: rel,
                                size: sz,
                                meta: cn.meta.clone(),
                                flipped: self.flipped_flashcards.contains(&cn.id),
                                asset_path: self.resolved_node_asset_path(&cn.id),
                            });
                        }
                    }
                    children_snaps.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));

                    let mut edges_snaps = Vec::new();
                    let mut sorted_edges: Vec<_> = self.graph.edges.values().collect();
                    sorted_edges.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
                    for edge in sorted_edges {
                        if let (Some((f_rel, f_sz)), Some((t_rel, t_sz))) = (
                            child_map.get(&edge.from.node),
                            child_map.get(&edge.to.node),
                        ) {
                            let from_is_in = self
                                .graph
                                .get_node(&edge.from.node)
                                .and_then(|fn_node| fn_node.ports.iter().find(|p| p.id == edge.from.port))
                                .is_some_and(|p| p.direction == PortDirection::Input);
                            let to_is_out = self
                                .graph
                                .get_node(&edge.to.node)
                                .and_then(|tn_node| tn_node.ports.iter().find(|p| p.id == edge.to.port))
                                .is_some_and(|p| p.direction == PortDirection::Output);

                            let from_xy = (
                                if from_is_in { f_rel.0 } else { f_rel.0 + f_sz.0 },
                                f_rel.1 + f_sz.1 * 0.5,
                            );
                            let to_xy = (
                                if to_is_out { t_rel.0 + t_sz.0 } else { t_rel.0 },
                                t_rel.1 + t_sz.1 * 0.5,
                            );
                            let is_ghost = edge.status == EdgeStatus::Proposed;
                            let label = if edge.label.is_empty() {
                                format!("[{}]", edge.relation.display_label())
                            } else {
                                format!("[{}] {}", edge.relation.display_label(), edge.label)
                            };
                            edges_snaps.push(SectionEdgeSnapshot {
                                from_rel: from_xy,
                                to_rel: to_xy,
                                label,
                                is_ghost,
                                hide_label: (edge.hide_label || self.hide_all_edge_labels) && !is_ghost,
                            });
                        }
                    }

                    (children_snaps, edges_snaps)
                } else {
                    (Vec::new(), Vec::new())
                };

                (
                    n.id.clone(),
                    n.kind,
                    n.title.clone(),
                    n.content.clone(),
                    n.summary.clone(),
                    n.meta.clone(),
                    flipped,
                    sec_size,
                    sec_children,
                    sec_edges,
                    fmt,
                    asset,
                )
            })
        });

        if let Ok(mut b) = self.reader_bridge.lock() {
            let rev = b.snapshot.revision.saturating_add(1);
            let (pdf_page_index, pdf_page_count, pdf_zoom) = match &self.active_pdf {
                Some(p) if target_id.as_ref() == Some(&p.node_id) => {
                    (p.page_index, p.page_count.max(1), p.zoom)
                }
                _ => (0, 1, 1.5),
            };
            b.snapshot = match snapshot_data {
                Some((
                    id,
                    node_kind,
                    title,
                    content,
                    summary,
                    meta,
                    flashcard_flipped,
                    section_size,
                    section_children,
                    section_edges,
                    doc_format,
                    asset_path,
                )) => ReaderSnapshot {
                    revision: rev,
                    project_title: self.graph.title.clone(),
                    vision_pending: self.vision_pending_nodes.contains(&id),
                    node_id: Some(id),
                    node_kind,
                    title,
                    content,
                    summary,
                    meta,
                    flashcard_flipped,
                    section_size,
                    section_children,
                    section_edges,
                    doc_format,
                    asset_path,
                    doc_view_tab: self.doc_view_tab,
                    pdf_page_index,
                    pdf_page_count,
                    pdf_zoom,
                    read_only: self.reader_read_only,
                    ocr_engine: self.reader_prefs.ocr_engine,
                    target_language: self.reader_prefs.target_language.clone(),
                    vision_downloading_models: self.vision_downloading_models,
                    neural_models_ready: self.neural_models_ready,
                    vision_error: self.vision_last_error.clone(),
                },
                None => ReaderSnapshot {
                    revision: rev,
                    project_title: self.graph.title.clone(),
                    doc_view_tab: self.doc_view_tab,
                    read_only: self.reader_read_only,
                    ocr_engine: self.reader_prefs.ocr_engine,
                    target_language: self.reader_prefs.target_language.clone(),
                    vision_downloading_models: self.vision_downloading_models,
                    neural_models_ready: self.neural_models_ready,
                    vision_error: self.vision_last_error.clone(),
                    ..ReaderSnapshot::default()
                },
            };
            if b.is_open {
                b.wake_popout();
            }
        }
    }

    /// Pop the right reader/inspector panel out into a dedicated full-view native window
    /// directly in Read-Only Focus Mode.
    pub fn pop_out_reader(&mut self) {
        self.sync_node_edits();
        let selected_id = self.selected_nodes.iter().next().cloned();
        if let Some(ref sid) = selected_id {
            if let Some(n) = self.graph.get_node(sid) {
                if !can_pop_out(n.kind) {
                    self.status_message =
                        "Checklists and Excerpts stay inline on canvas".into();
                    return;
                }
            }
        }
        self.popped_out_node_id = selected_id;
        self.reader_read_only = true;
        self.reader_prefs.read_only = true;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        if self.doc_view_tab == DocViewTab::Source {
            self.doc_view_tab = if self.active_pdf.is_some() {
                DocViewTab::Pages
            } else {
                DocViewTab::Preview
            };
        }
        self.push_reader_snapshot();
        self.inspector_popped_out = true;
        self.inspector_open = false;

        let already_open = self.reader_bridge.lock().map(|b| b.is_open).unwrap_or(false);
        if already_open {
            if let Some(h) = &self.popout_window {
                h.focus();
            }
        } else {
            let handle = open_reader_window(self.reader_bridge.clone());
            self.popout_window = Some(handle);
        }
        self.status_message = "Popped reader out to dedicated window".into();
    }

    /// Dock the pop-out reader back into the workspace right sidebar.
    pub fn dock_reader_back(&mut self) {
        self.inspector_popped_out = false;
        self.inspector_open = true;
        if let Some(h) = self.popout_window.take() {
            h.close();
        }
        if let Ok(mut b) = self.reader_bridge.lock() {
            b.is_open = false;
        }
        self.status_message = "Docked reader back into workspace".into();
    }

    /// Process inbound commands sent by the standalone pop-out Reader window.
    pub fn poll_reader_bridge(&mut self) -> bool {
        let cmds = match self.reader_bridge.lock() {
            Ok(mut b) => std::mem::take(&mut b.commands),
            Err(_) => return false,
        };
        if cmds.is_empty() {
            return false;
        }

        let mut push_snap = false;
        for cmd in cmds {
            match cmd {
                ReaderCommand::DockBack => {
                    self.inspector_popped_out = false;
                    self.inspector_open = true;
                    self.popout_window = None;
                    self.popped_out_node_id = None;
                    self.status_message = "Docked reader back into workspace".into();
                }
                ReaderCommand::WindowClosed => {
                    if self.inspector_popped_out {
                        self.inspector_popped_out = false;
                        self.inspector_open = true;
                    }
                    self.popout_window = None;
                    self.popped_out_node_id = None;
                }
                ReaderCommand::SetReadOnly(ro) => {
                    self.set_reader_read_only(ro);
                    push_snap = true;
                }
                ReaderCommand::UpdateTitleContent {
                    node_id,
                    title,
                    content,
                } => {
                    if let Some(node) = self.graph.get_node_mut(&node_id) {
                        node.title = title.clone();
                        node.content = content.clone();
                        if self.selected_nodes.contains(&node_id) {
                            self.canvas_title_edit = TextEdit::new(&title);
                            self.canvas_content_edit = TextEdit::new(&content);
                            self.inspector_title_edit = TextEdit::new(&title);
                            self.inspector_content_edit = TextEdit::new(&content);
                        }
                        if let Some(bundle) = &self.bundle {
                            let _ = bundle.save_graph_atomic(&self.graph);
                        }
                        push_snap = true;
                    }
                }
                ReaderCommand::ToggleTask {
                    node_id,
                    marker_offset,
                    checked,
                } => {
                    if !self.selected_nodes.contains(&node_id) {
                        self.select_node(node_id, false);
                    }
                    self.toggle_selected_doc_task(marker_offset, checked);
                    push_snap = true;
                }
                ReaderCommand::AppendText { node_id, text } => {
                    if !self.selected_nodes.contains(&node_id) {
                        self.select_node(node_id, false);
                    }
                    self.append_to_selected_node(&text);
                    push_snap = true;
                }
                ReaderCommand::SetDocViewTab(tab) => {
                    self.doc_view_tab = tab;
                    push_snap = true;
                }
                ReaderCommand::SetPdfPage(page) => {
                    self.set_pdf_page(page);
                    push_snap = true;
                }
                ReaderCommand::SetPdfZoom(zoom) => {
                    self.set_pdf_zoom(zoom);
                    push_snap = true;
                }
                ReaderCommand::OpenExternal(node_id) => {
                    self.open_node_in_external_reader(&node_id);
                }
                ReaderCommand::ExplainAi(node_id) => {
                    self.explain_node_ai(&node_id);
                    push_snap = true;
                }
                ReaderCommand::FlipFlashcard(node_id) => {
                    self.toggle_flashcard_flip(&node_id);
                    push_snap = true;
                }
                ReaderCommand::SetFlashcardMastery { node_id, mastery } => {
                    self.set_flashcard_mastery(&node_id, &mastery);
                    push_snap = true;
                }
                ReaderCommand::UpdateFlashcardAnswer { node_id, answer } => {
                    if let Some(n) = self.graph.get_node_mut(&node_id) {
                        if n.kind == NodeKind::Flashcard {
                            n.meta.answer = if answer.trim().is_empty() {
                                None
                            } else {
                                Some(answer.clone())
                            };
                        }
                    }
                    if self.selected_nodes.contains(&node_id) {
                        self.inspector_answer_edit = TextEdit::new(&answer);
                    }
                    if let Some(bundle) = &self.bundle {
                        let _ = bundle.save_graph_atomic(&self.graph);
                    }
                    push_snap = true;
                }
                ReaderCommand::SetFormulaLanguage { node_id, language } => {
                    self.set_formula_language(&node_id, &language);
                    push_snap = true;
                }
                ReaderCommand::FormatFormula(node_id) => {
                    self.format_formula_node(&node_id);
                    push_snap = true;
                }
                ReaderCommand::InsertFormulaSymbol { node_id, symbol } => {
                    self.insert_formula_symbol(&node_id, &symbol);
                    push_snap = true;
                }
                ReaderCommand::AddNextSequenceStep(node_id) => {
                    if let Some(next_id) = self.add_next_sequence_step(&node_id) {
                        if self.inspector_popped_out {
                            self.popped_out_node_id = Some(next_id);
                        }
                    }
                    push_snap = true;
                }
                ReaderCommand::ToggleSectionFramed(node_id) => {
                    self.toggle_section_framed(&node_id);
                    push_snap = true;
                }
                ReaderCommand::ToggleChecklistItem {
                    node_id,
                    task_index,
                } => {
                    self.toggle_checklist_item_on_node(&node_id, task_index);
                    push_snap = true;
                }
                ReaderCommand::IngestImagePath { node_id, path } => {
                    self.ingest_file_into_node(&node_id, &path);
                    push_snap = true;
                }
                ReaderCommand::SetOcrEngine(engine) => {
                    self.set_ocr_engine(engine);
                    push_snap = true;
                }
                ReaderCommand::DownloadNeuralModels => {
                    self.download_neural_ocr_models();
                    push_snap = true;
                }
                ReaderCommand::RunImageOcr { node_id, mode } => {
                    self.run_image_ocr(&node_id, mode);
                    push_snap = true;
                }
                ReaderCommand::SpawnNoteFromOcr(node_id) => {
                    self.spawn_note_from_extracted_text(&node_id);
                    push_snap = true;
                }
                ReaderCommand::SpawnFormulaFromOcr(node_id) => {
                    self.spawn_formula_from_extracted_text(&node_id);
                    push_snap = true;
                }
                ReaderCommand::SetTargetLanguage(lang) => {
                    self.set_target_language(&lang);
                    push_snap = true;
                }
                ReaderCommand::RunNodeTranslation(node_id) => {
                    self.run_node_translation(&node_id, None);
                    push_snap = true;
                }
                ReaderCommand::SpawnNoteFromTranslation(node_id) => {
                    self.spawn_note_from_translation(&node_id);
                    push_snap = true;
                }
            }
        }

        if push_snap {
            self.push_reader_snapshot();
        }
        true
    }

    pub fn set_reader_read_only(&mut self, read_only: bool) {
        if read_only {
            self.sync_node_edits();
            if self.doc_view_tab == DocViewTab::Source {
                self.doc_view_tab = if self.active_pdf.is_some() {
                    DocViewTab::Pages
                } else {
                    DocViewTab::Preview
                };
            }
        }
        self.reader_read_only = read_only;
        self.reader_prefs.read_only = read_only;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.push_reader_snapshot();
        self.status_message = if read_only {
            "Reader: Read-Only focus mode".into()
        } else {
            "Reader: Edit mode enabled".into()
        };
    }

    /// Recenter the canvas camera on the nearest cluster of nodes to the current viewport center
    /// so the user never gets lost panning on the infinite canvas.
    pub fn locate_nearest_nodes(&mut self) -> Option<NodeId> {
        if self.graph.nodes.is_empty() {
            self.graph.viewport = studynodes_graph::coords::Camera::default();
            self.status_message = "Canvas empty — reset view to origin".into();
            return None;
        }

        let canvas_w = self.canvas_size[0].max(320.0);
        let canvas_h = self.canvas_size[1].max(240.0);
        let view_center = self
            .graph
            .viewport
            .screen_to_world(ScreenPos::new(canvas_w * 0.5, canvas_h * 0.5));

        let mut ranked: Vec<(f32, NodeId, WorldPos)> = self
            .graph
            .nodes
            .values()
            .map(|n| {
                let sz = node_world_size(n);
                let center = WorldPos::new(n.pos.x + sz.0 * 0.5, n.pos.y + sz.1 * 0.5);
                let dx = center.x - view_center.x;
                let dy = center.y - view_center.y;
                (dx * dx + dy * dy, n.id.clone(), center)
            })
            .collect();
        ranked.sort_by(|a, b| a.0.total_cmp(&b.0));

        const CLUSTER_RADIUS: f32 = 680.0;
        let (nearest_dist_sq, mut anchor_id, mut anchor_center) = ranked[0].clone();

        // If already centered directly on the nearest node, jump to the closest distinct
        // cluster further away (when one exists) so repeated presses help explore.
        if nearest_dist_sq < 42.0 * 42.0 {
            if let Some((_, far_id, far_center)) = ranked
                .iter()
                .find(|(_, _, c)| c.distance_to(&anchor_center) > CLUSTER_RADIUS)
            {
                anchor_id = far_id.clone();
                anchor_center = *far_center;
            }
        }

        let neighbor_ids: HashSet<NodeId> = self.graph.neighbors(&anchor_id).into_iter().collect();
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut count = 0usize;

        for n in self.graph.nodes.values() {
            let (w, h) = node_base_size(n.kind);
            let c = WorldPos::new(n.pos.x + w * 0.5, n.pos.y + h * 0.5);
            if n.id == anchor_id
                || neighbor_ids.contains(&n.id)
                || c.distance_to(&anchor_center) <= CLUSTER_RADIUS
            {
                min_x = min_x.min(n.pos.x);
                min_y = min_y.min(n.pos.y);
                max_x = max_x.max(n.pos.x + w);
                max_y = max_y.max(n.pos.y + h);
                count += 1;
            }
        }

        if count == 0 {
            return None;
        }

        let bbox_w = (max_x - min_x).max(240.0) + 120.0;
        let bbox_h = (max_y - min_y).max(180.0) + 120.0;
        let bbox_center = WorldPos::new((min_x + max_x) * 0.5, (min_y + max_y) * 0.5);

        let fit_zoom = (canvas_w / bbox_w).min(canvas_h / bbox_h).clamp(0.45, 1.0);
        if self.graph.viewport.zoom > 1.35 || self.graph.viewport.zoom < 0.45 || count > 1 {
            self.graph.viewport.zoom = fit_zoom;
        }
        self.graph
            .viewport
            .center_on(bbox_center, canvas_w, canvas_h);

        let title = self
            .graph
            .get_node(&anchor_id)
            .map(|n| n.title.as_str())
            .unwrap_or("nodes");
        self.status_message = format!("Located {} nearest node(s) near '{}'", count, title);
        Some(anchor_id)
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
        self.push_reader_snapshot();
    }

    /// Select a node and populate edit buffers (preserving active edits/caret when already singly selected).
    pub fn select_node(&mut self, id: NodeId, additive: bool) {
        if !additive && self.selected_nodes.len() == 1 && self.selected_nodes.contains(&id) {
            let mut open_doc = false;
            let mut popout_sync = false;
            if let Some(n) = self.graph.get_node(&id) {
                if self.inspector_popped_out && can_pop_out(n.kind) {
                    self.popped_out_node_id = Some(id.clone());
                    popout_sync = true;
                }
                if matches!(n.kind, NodeKind::Document | NodeKind::Image) {
                    if !self.inspector_popped_out {
                        self.inspector_open = true;
                    }
                    self.ai_panel_open = false;
                    open_doc = true;
                }
            }
            if open_doc {
                self.ensure_reader_media_for_node(&id, false);
            }
            if open_doc || popout_sync {
                self.push_reader_snapshot();
            }
            return;
        }

        self.sync_node_edits();
        self.sync_edge_edits();
        self.show_block_menu = false;
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
            self.inspector_answer_edit = TextEdit::new(n.meta.answer.as_deref().unwrap_or(""));
            node_kind = Some(n.kind);
            if matches!(n.kind, NodeKind::Document | NodeKind::Image) {
                if !self.inspector_popped_out {
                    self.inspector_open = true;
                }
                self.ai_panel_open = false;
                self.editing_node_id = None;
            } else if matches!(n.kind, NodeKind::Note | NodeKind::Formula) {
                self.editing_node_id = Some(id.clone());
            } else {
                self.editing_node_id = None;
            }
            if self.inspector_popped_out && can_pop_out(n.kind) {
                self.popped_out_node_id = Some(id.clone());
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
        self.push_reader_snapshot();
    }

    /// Clear all active node/edge selections (e.g. clicking empty canvas).
    pub fn deselect_all(&mut self) {
        self.sync_node_edits();
        self.sync_edge_edits();
        self.show_block_menu = false;
        self.selected_nodes.clear();
        self.selected_edge = None;
        self.editing_node_id = None;
        self.active_pdf = None;
        self.active_image = None;
        self.canvas_title_edit = TextEdit::new("");
        self.canvas_content_edit = TextEdit::new("");
        self.inspector_title_edit = TextEdit::new("");
        self.inspector_content_edit = TextEdit::new("");
        self.inspector_answer_edit = TextEdit::new("");
        self.push_reader_snapshot();
    }

    fn persist_graph_and_snapshot(&mut self) {
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
        self.push_reader_snapshot();
    }

    pub fn viewport_spawn_center(&self, dx: f32, dy: f32) -> WorldPos {
        let w = if self.canvas_size[0] > 120.0 {
            self.canvas_size[0]
        } else {
            920.0
        };
        let h = if self.canvas_size[1] > 120.0 {
            self.canvas_size[1]
        } else {
            640.0
        };
        let stagger = ((self.graph.nodes.len() % 5) as f32 - 2.0) * 28.0;
        let center = self
            .graph
            .viewport
            .screen_to_world(ScreenPos::new(w * 0.5, h * 0.45));
        WorldPos::new(center.x + dx + stagger, center.y + dy + stagger)
    }

    /// Capture non-section nodes whose centers fall inside a framed Section.
    pub fn capture_section_children(&self, section_id: &NodeId) -> Vec<(NodeId, WorldPos)> {
        let Some(sec) = self.graph.get_node(section_id) else {
            return Vec::new();
        };
        if sec.kind != NodeKind::Section || !sec.meta.section_framed {
            return Vec::new();
        }
        let (sw, sh) = node_world_size(sec);
        let x0 = sec.pos.x;
        let y0 = sec.pos.y;
        let x1 = x0 + sw;
        let y1 = y0 + sh;

        self.graph
            .nodes
            .values()
            .filter(|n| n.id != *section_id && n.kind != NodeKind::Section)
            .filter_map(|n| {
                let (nw, nh) = node_world_size(n);
                let cx = n.pos.x + nw * 0.5;
                let cy = n.pos.y + nh * 0.5;
                if cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1 {
                    Some((n.id.clone(), n.pos))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn toggle_section_framed(&mut self, id: &NodeId) {
        let mut label = "Section frame";
        if let Some(sec) = self.graph.get_node_mut(id) {
            if sec.kind == NodeKind::Section {
                sec.meta.section_framed = !sec.meta.section_framed;
                label = if sec.meta.section_framed {
                    "Section: Framed Group"
                } else {
                    "Section: Title Banner"
                };
            }
        }
        self.persist_graph_and_snapshot();
        self.status_message = label.into();
    }

    pub fn toggle_flashcard_flip(&mut self, id: &NodeId) {
        if self.flipped_flashcards.contains(id) {
            self.flipped_flashcards.remove(id);
        } else {
            self.flipped_flashcards.insert(id.clone());
        }
    }

    pub fn set_flashcard_mastery(&mut self, id: &NodeId, mastery: &str) {
        if let Some(n) = self.graph.get_node_mut(id) {
            if n.kind == NodeKind::Flashcard {
                n.meta.mastery = Some(mastery.to_string());
            }
        }
        self.persist_graph_and_snapshot();
        self.status_message = format!("Flashcard marked: {}", mastery);
    }

    pub fn set_formula_language(&mut self, id: &NodeId, lang: &str) {
        if let Some(n) = self.graph.get_node_mut(id) {
            if n.kind == NodeKind::Formula {
                n.meta.language = Some(lang.to_string());
            }
        }
        self.persist_graph_and_snapshot();
        self.status_message = format!("Formula notation: {}", lang);
    }

    pub fn toggle_checklist_item_on_node(&mut self, id: &NodeId, task_index: usize) {
        let is_selected = self.selected_nodes.contains(id);
        let Some(node) = self.graph.get_node_mut(id) else {
            return;
        };
        node.content = doc_reader::toggle_markdown_task(&node.content, task_index);
        if is_selected {
            self.canvas_content_edit = TextEdit::new(&node.content);
            self.inspector_content_edit = TextEdit::new(&node.content);
        }
        self.persist_graph_and_snapshot();
        self.status_message = "Updated checklist task".into();
    }

    pub fn toggle_edge_hide_label(&mut self, edge_id: &EdgeId, hide: bool) {
        if let Some(edge) = self.graph.get_edge_mut(edge_id) {
            edge.hide_label = hide;
        }
        self.persist_graph_and_snapshot();
        self.status_message = if hide {
            "Edge label hidden".into()
        } else {
            "Edge label visible".into()
        };
    }

    pub fn toggle_all_edge_labels(&mut self) {
        self.hide_all_edge_labels = !self.hide_all_edge_labels;
        self.push_reader_snapshot();
        self.status_message = if self.hide_all_edge_labels {
            "All connector labels hidden".into()
        } else {
            "Connector labels visible".into()
        };
    }

    pub fn insert_formula_symbol(&mut self, node_id: &NodeId, sym: &str) {
        let is_selected = self.selected_nodes.contains(node_id);
        if is_selected && self.canvas_content_edit.focused {
            self.canvas_content_edit.insert(sym);
            self.sync_from_canvas_edits();
            self.status_message = format!("Inserted {}", sym);
            return;
        }
        if is_selected && self.inspector_content_edit.focused {
            self.inspector_content_edit.insert(sym);
            self.sync_from_inspector_edits();
            self.status_message = format!("Inserted {}", sym);
            return;
        }

        if let Some(node) = self.graph.get_node_mut(node_id) {
            node.content.push_str(sym);
            if is_selected {
                self.canvas_content_edit = TextEdit::new(&node.content);
                self.inspector_content_edit = TextEdit::new(&node.content);
            }
        }
        self.persist_graph_and_snapshot();
        self.status_message = format!("Inserted {}", sym);
    }

    pub fn format_formula_node(&mut self, node_id: &NodeId) {
        self.sync_node_edits();
        let is_selected = self.selected_nodes.contains(node_id);
        let Some(node) = self.graph.get_node_mut(node_id) else {
            return;
        };
        let expanded = formula_math::expand_math_shorthand(&node.content);
        if expanded != node.content {
            node.content = expanded;
            if is_selected {
                self.canvas_content_edit = TextEdit::new(&node.content);
                self.inspector_content_edit = TextEdit::new(&node.content);
            }
            self.persist_graph_and_snapshot();
            self.status_message = "Formatted math notation".into();
        } else {
            self.status_message = "Formula already formatted".into();
        }
    }

    pub fn maybe_auto_expand_formula_on_canvas_edit(&mut self) {
        let Some(id) = self.selected_nodes.iter().next().cloned() else {
            return;
        };
        let Some((is_formula, is_code)) = self.graph.get_node(&id).map(|n| {
            let lang = n.meta.language.as_deref().unwrap_or("MATH");
            (
                n.kind == NodeKind::Formula,
                lang.eq_ignore_ascii_case("RUST") || lang.eq_ignore_ascii_case("PY"),
            )
        }) else {
            return;
        };
        if !is_formula || is_code {
            return;
        }

        if let Some(expanded) = formula_math::expand_trailing_command_on_space(&self.canvas_content_edit.text) {
            let was_focused = self.canvas_content_edit.focused;
            let mut next_edit = TextEdit::new(&expanded);
            next_edit.focused = was_focused;
            self.canvas_content_edit = next_edit;
            self.sync_from_canvas_edits();
        }
    }

    /// Spawn a specialized Phase 2 study node with clean monochrome defaults.
    pub fn add_study_node(&mut self, kind: NodeKind, pos_opt: Option<WorldPos>) -> NodeId {
        self.show_block_menu = false;
        if kind == NodeKind::Excerpt && pos_opt.is_none() {
            if let Some(created) = self.create_excerpt_from_selected_doc(None) {
                return created;
            }
        }

        let default_pos = match kind {
            NodeKind::Section => self.viewport_spawn_center(-320.0, -200.0),
            _ => self.viewport_spawn_center(-130.0, -85.0),
        };
        let pos = pos_opt.unwrap_or(default_pos);

        let step_num = (self
            .graph
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Sequence)
            .count() as u32)
            + 1;

        let title = match kind {
            NodeKind::Section => "New Section".to_string(),
            NodeKind::Image => "New Image".to_string(),
            NodeKind::Excerpt => "Source Excerpt".to_string(),
            NodeKind::Flashcard => "Recall Question".to_string(),
            NodeKind::Formula => "Key Equation".to_string(),
            NodeKind::Checklist => "Study Checklist".to_string(),
            NodeKind::Sequence => format!("Step {:02}", step_num),
            _ => kind.display_name().to_string(),
        };

        let mut node = studynodes_graph::model::Node::new(kind, title, pos);
        node.size = node_base_size(kind);

        match kind {
            NodeKind::Section => {
                node.meta.section_framed = true;
            }
            NodeKind::Excerpt => {
                node.content =
                    "\"Key passage or empirical claim highlighted from source.\"".into();
            }
            NodeKind::Flashcard => {
                node.content = "What mechanism or concept does this card test?".into();
                node.meta.answer = Some("Concise recall explanation shown on flip.".into());
                node.meta.mastery = Some("new".into());
            }
            NodeKind::Formula => {
                node.content = "Δw_ij = η · x_i · y_j".into();
                node.meta.language = Some("MATH".into());
            }
            NodeKind::Checklist => {
                node.content =
                    "- [ ] Review foundational definition\n- [ ] Trace worked example\n- [ ] Self-test without notes"
                        .into();
            }
            NodeKind::Sequence => {
                node.content = "Describe this procedural or causal stage.".into();
                node.meta.step_index = Some(step_num);
            }
            _ => {}
        }

        let id = node.id.clone();
        self.graph.add_node(node);
        if kind != NodeKind::Section {
            self.select_node(id.clone(), false);
        } else {
            self.selected_nodes.clear();
            self.selected_nodes.insert(id.clone());
            self.selected_edge = None;
            if let Some(n) = self.graph.get_node(&id) {
                self.canvas_title_edit = TextEdit::new(&n.title);
                self.canvas_content_edit = TextEdit::new(&n.content);
                self.inspector_title_edit = TextEdit::new(&n.title);
                self.inspector_content_edit = TextEdit::new(&n.content);
                self.inspector_answer_edit = TextEdit::new("");
            }
        }
        self.persist_graph_and_snapshot();
        self.status_message = format!("Added {} block", kind.display_name());
        id
    }

    /// Create a Source Excerpt linked to the currently selected Document/Image & active page.
    pub fn create_excerpt_from_selected_doc(
        &mut self,
        snippet_opt: Option<&str>,
    ) -> Option<NodeId> {
        let doc_id = self
            .selected_nodes
            .iter()
            .find(|id| {
                self.graph
                    .get_node(id)
                    .is_some_and(|n| matches!(n.kind, NodeKind::Document | NodeKind::Image))
            })
            .cloned()?;

        let page_idx = self
            .active_pdf
            .as_ref()
            .filter(|p| p.node_id == doc_id)
            .map(|p| p.page_index)
            .unwrap_or_else(|| {
                self.graph
                    .get_node(&doc_id)
                    .map(|n| n.preview_page)
                    .unwrap_or(0)
            });

        let (doc_title, doc_pos, doc_content) = self
            .graph
            .get_node(&doc_id)
            .map(|n| (n.title.clone(), n.pos, n.content.clone()))?;

        let sibling_count = self
            .graph
            .nodes
            .values()
            .filter(|n| {
                n.kind == NodeKind::Excerpt && n.meta.source_node.as_ref() == Some(&doc_id)
            })
            .count() as f32;

        let snippet = snippet_opt
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                let pages = doc_reader::split_document_preview_pages(&doc_content, 7);
                let page_text = pages.get(page_idx).map(|p| p.body.as_str()).unwrap_or("");
                let clean: String = page_text
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("```"))
                    .collect::<Vec<_>>()
                    .join(" ");
                let trimmed = clean.trim();
                if trimmed.is_empty() {
                    format!("Cited passage from '{}' (page {}).", doc_title, page_idx + 1)
                } else {
                    let short: String = trimmed.chars().take(150).collect();
                    if trimmed.chars().count() > 150 {
                        format!("\"{}...\"", short.trim_end())
                    } else {
                        format!("\"{}\"", short)
                    }
                }
            });

        let spawn_pos =
            WorldPos::new(doc_pos.x + 310.0, doc_pos.y + 30.0 + sibling_count * 185.0);
        let mut excerpt = studynodes_graph::model::Node::new(
            NodeKind::Excerpt,
            format!("{} · p.{}", doc_title, page_idx + 1),
            spawn_pos,
        );
        excerpt.size = node_base_size(NodeKind::Excerpt);
        excerpt.content = snippet;
        excerpt.meta.source_node = Some(doc_id.clone());
        excerpt.meta.source_page = Some(page_idx);

        let ex_id = excerpt.id.clone();
        self.graph.add_node(excerpt);

        let mut edge = studynodes_graph::model::Edge::new(
            studynodes_graph::model::EdgeEndpoint {
                node: doc_id,
                port: PortId::new(PortId::DEFAULT_OUT),
            },
            studynodes_graph::model::EdgeEndpoint {
                node: ex_id.clone(),
                port: PortId::new(PortId::DEFAULT_IN),
            },
        );
        edge.relation = studynodes_graph::model::EdgeRelation::DerivedFrom;
        edge.label = format!("p.{}", page_idx + 1);
        edge.note = format!("Excerpt anchored to page {} of '{}'.", page_idx + 1, doc_title);
        self.graph.add_edge(edge);

        self.persist_graph_and_snapshot();
        self.status_message = format!("Pinned Excerpt from '{}' · p.{}", doc_title, page_idx + 1);
        Some(ex_id)
    }

    /// Jump the Integrated Reader to the Document node and page referenced by an `Excerpt`.
    pub fn jump_to_excerpt_source(&mut self, excerpt_id: &NodeId) {
        let Some((src_id, src_page)) = self.graph.get_node(excerpt_id).and_then(|n| {
            n.meta
                .source_node
                .clone()
                .map(|id| (id, n.meta.source_page.unwrap_or(0)))
        }) else {
            self.status_message = "Excerpt has no linked source document".into();
            return;
        };

        let Some(doc_title) = self.graph.get_node(&src_id).map(|d| d.title.clone()) else {
            self.status_message = "Linked source document no longer exists".into();
            return;
        };

        if let Some(doc) = self.graph.get_node_mut(&src_id) {
            doc.preview_page = src_page;
        }
        self.select_node(src_id.clone(), false);
        if self
            .active_pdf
            .as_ref()
            .is_some_and(|p| p.node_id == src_id)
        {
            self.set_pdf_page(src_page);
        } else {
            self.ensure_card_preview(&src_id);
            self.push_reader_snapshot();
        }
        self.status_message = format!("Opened '{}' · p.{}", doc_title, src_page + 1);
    }

    /// Spawn the next chained Sequence step right of `prev_id` and connect them.
    pub fn add_next_sequence_step(&mut self, prev_id: &NodeId) -> Option<NodeId> {
        let (prev_pos, prev_step) = self
            .graph
            .get_node(prev_id)
            .map(|n| (n.pos, n.meta.step_index.unwrap_or(1)))?;
        let next_step = prev_step + 1;
        let next_pos = WorldPos::new(prev_pos.x + 296.0, prev_pos.y);

        let mut next = studynodes_graph::model::Node::new(
            NodeKind::Sequence,
            format!("Step {:02}", next_step),
            next_pos,
        );
        next.size = node_base_size(NodeKind::Sequence);
        next.content = "Continue sequence step...".into();
        next.meta.step_index = Some(next_step);
        let next_id = next.id.clone();
        self.graph.add_node(next);

        let mut edge = studynodes_graph::model::Edge::new(
            studynodes_graph::model::EdgeEndpoint {
                node: prev_id.clone(),
                port: PortId::new(PortId::DEFAULT_OUT),
            },
            studynodes_graph::model::EdgeEndpoint {
                node: next_id.clone(),
                port: PortId::new(PortId::DEFAULT_IN),
            },
        );
        edge.relation = studynodes_graph::model::EdgeRelation::Custom("then".into());
        edge.label = "Next".into();
        self.graph.add_edge(edge);

        self.select_node(next_id.clone(), false);
        self.persist_graph_and_snapshot();
        self.status_message = format!("Added Step {:02}", next_step);
        Some(next_id)
    }

    /// Generate active-recall Flashcard & Study Checklist nodes for selected focus nodes.
    pub fn generate_study_cards_ai(&mut self) {
        self.sync_node_edits();
        let mut focus_ids: Vec<NodeId> = self.selected_nodes.iter().cloned().collect();
        if focus_ids.is_empty() {
            focus_ids = self
                .graph
                .nodes
                .values()
                .filter(|n| n.kind != NodeKind::Section)
                .take(2)
                .map(|n| n.id.clone())
                .collect();
        }
        if focus_ids.is_empty() {
            self.status_message = "Add at least one node first to generate study cards".into();
            return;
        }

        let context =
            studynodes_llm::LlmContext::build(&self.graph, &focus_ids, "Generate Study Cards");
        let provider = studynodes_llm::MockLlmProvider;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        if let Ok(rt) = rt {
            if let Ok(patch) = rt.block_on(provider.generate_patch(&context)) {
                let res = self.graph.validate_and_stage_patch(&patch);
                self.persist_graph_and_snapshot();
                self.status_message = format!(
                    "✦ AI generated {} study cards & {} connections",
                    res.new_nodes.len(),
                    res.proposed_edges.len()
                );
            }
        }
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
        self.flipped_flashcards.remove(id);
        if let Some((removed, edges)) = self.graph.remove_node(id) {
            self.selected_nodes.remove(id);
            if self.popped_out_node_id.as_ref() == Some(id) {
                self.popped_out_node_id = None;
            }
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
            self.card_pdf_previews.retain(|(nid, _), _| nid != id);
            self.card_image_previews.remove(id);
            if let Some(bundle) = &self.bundle {
                let _ = bundle.save_graph_atomic(&self.graph);
            }
            self.push_reader_snapshot();
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
                self.push_reader_snapshot();
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
        if !self.inspector_popped_out {
            self.inspector_open = true;
        }
        self.ai_panel_open = false;

        if let Some(e) = self.graph.get_edge(&id) {
            self.edit_edge_label = TextEdit::new(&e.label);
            self.edit_edge_note = TextEdit::new(&e.note);
        }
        self.push_reader_snapshot();
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
        self.push_reader_snapshot();
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
            node.preview_page = 0;
            self.active_pdf = None;
            self.active_image = None;
            self.card_pdf_previews.retain(|(id, _), _| id != node_id);
            self.card_image_previews.remove(node_id);
            self.doc_view_tab = DocViewTab::Preview;
            self.status_message = "Loaded styled Markdown paper".into();
        }
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
        self.push_reader_snapshot();
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
                || node.title == "New Image"
            {
                node.title = filename.clone();
            }
            node.preview_page = 0;
            node.content = extracted.markdown;
            self.canvas_title_edit = TextEdit::new(&node.title);
            self.canvas_content_edit = TextEdit::new(&node.content);
            self.inspector_title_edit = TextEdit::new(&node.title);
            self.inspector_content_edit = TextEdit::new(&node.content);
        }

        self.card_pdf_previews.retain(|(id, _), _| id != node_id);
        self.card_image_previews.remove(node_id);

        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }

        self.ensure_reader_media_for_node(node_id, true);
        self.ensure_card_preview(node_id);
        self.push_reader_snapshot();
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
        self.refresh_card_previews();
        if let Some(id) = last_id {
            self.select_node(id, false);
        }
        self.status_message = format!("Ingested {} file(s)", files.len());
    }

    #[allow(dead_code)]
    pub fn set_vision_service_override(&mut self, svc: Arc<dyn VisionService>) {
        self.neural_models_ready = svc.model_status().is_ready();
        self.vision_service_override = Some(svc);
        self.push_reader_snapshot();
    }

    fn active_vision_service(&self) -> Arc<dyn VisionService> {
        if let Some(over) = &self.vision_service_override {
            return Arc::clone(over);
        }
        Arc::new(LocalVisionService::new(VisionConfig {
            models_dir: self.projects_dir.join("models"),
            engine: self.reader_prefs.ocr_engine,
            ollama_url: self.reader_prefs.ollama_url.clone(),
            ollama_model: self.reader_prefs.ollama_model.clone(),
            gemini_api_key: std::env::var("GEMINI_API_KEY")
                .ok()
                .filter(|k| !k.trim().is_empty()),
        }))
    }

    pub fn set_ocr_engine(&mut self, engine: OcrEnginePreference) {
        self.reader_prefs.ocr_engine = engine;
        self.neural_models_ready = self.active_vision_service().model_status().is_ready();
        self.vision_last_error = None;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.status_message = format!("OCR Engine: {}", engine.display_name());
        self.push_reader_snapshot();
    }

    pub fn set_target_language(&mut self, lang: &str) {
        let code = lang.trim().to_ascii_lowercase();
        if code.is_empty() {
            return;
        }
        self.reader_prefs.target_language = code.clone();
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.status_message = format!(
            "Target language: {}",
            studynodes_vision::language_display_name(&code)
        );
        self.push_reader_snapshot();
    }

    /// Download Tier 2 cross-platform pure-Rust neural OCR weights in a non-blocking thread.
    pub fn download_neural_ocr_models(&mut self) {
        if self.vision_downloading_models {
            return;
        }
        let svc = self.active_vision_service();
        if svc.model_status().is_ready() {
            self.neural_models_ready = true;
            self.status_message = "Tier 2 neural OCR models already cached".into();
            self.push_reader_snapshot();
            return;
        }

        self.vision_downloading_models = true;
        self.vision_last_error = None;
        self.status_message = "Downloading Tier 2 neural OCR weights (~12 MB)...".into();
        self.push_reader_snapshot();

        let tx = self.vision_tx.clone();
        let waker = self.vision_waker.clone();
        std::thread::spawn(move || {
            let result = svc.download_neural_models().map_err(|e| e.to_string());
            let _ = tx.send(VisionJobResult::ModelsDownloaded { result });
            if let Some(w) = waker {
                (w)();
            }
        });
    }

    /// Trigger offline Tier 1 / Tier 2 OCR on an `Image` node in a background worker thread.
    pub fn run_image_ocr(&mut self, node_id: &NodeId, mode: OcrMode) {
        self.sync_node_edits();
        if self.vision_pending_nodes.contains(node_id) {
            return;
        }

        let Some(img_path) = self.resolved_node_asset_path(node_id) else {
            let msg = "Attach an image file first before running OCR.".to_string();
            self.vision_last_error = Some(msg.clone());
            self.status_message = msg;
            self.push_reader_snapshot();
            return;
        };

        let svc = self.active_vision_service();
        self.vision_pending_nodes.insert(node_id.clone());
        self.vision_last_error = None;
        self.status_message = match mode {
            OcrMode::EquationFormula => format!(
                "Extracting equation via {}...",
                self.reader_prefs.ocr_engine.short_label()
            ),
            _ => format!(
                "Extracting text via {}...",
                self.reader_prefs.ocr_engine.short_label()
            ),
        };
        self.push_reader_snapshot();

        let nid = node_id.clone();
        let tx = self.vision_tx.clone();
        let waker = self.vision_waker.clone();
        std::thread::spawn(move || {
            let result = svc
                .extract_image(&img_path, mode, None)
                .map_err(|e| e.to_string());
            let _ = tx.send(VisionJobResult::OcrFinished {
                node_id: nid,
                mode,
                result,
            });
            if let Some(w) = waker {
                (w)();
            }
        });
    }

    /// Translate extracted text (or Node body text) into the target language off the UI thread.
    pub fn run_node_translation(&mut self, node_id: &NodeId, target_lang_opt: Option<&str>) {
        self.sync_node_edits();
        if self.vision_pending_nodes.contains(node_id) {
            return;
        }

        let target_lang = target_lang_opt
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.reader_prefs.target_language.clone());
        if self.reader_prefs.target_language != target_lang {
            self.reader_prefs.target_language = target_lang.clone();
            let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        }

        let source_text = self.graph.get_node(node_id).and_then(|n| {
            if n.kind == NodeKind::Image {
                n.meta
                    .extracted_text
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .or_else(|| {
                        let body = n.content.trim();
                        if body.is_empty() || body.starts_with("Attached image:") {
                            None
                        } else {
                            Some(body.to_string())
                        }
                    })
            } else {
                let body = n.content.trim();
                if !body.is_empty() {
                    Some(body.to_string())
                } else {
                    n.summary
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToOwned::to_owned)
                }
            }
        });

        let Some(text_to_translate) = source_text else {
            let msg = "No text to translate yet (run Extract Text or type content first).".into();
            self.vision_last_error = Some(msg);
            self.status_message = "No text available to translate".into();
            self.push_reader_snapshot();
            return;
        };

        let svc = self.active_vision_service();
        self.vision_pending_nodes.insert(node_id.clone());
        self.vision_last_error = None;
        self.status_message = format!("Translating to {}...", target_lang.to_uppercase());
        self.push_reader_snapshot();

        let nid = node_id.clone();
        let lang_clone = target_lang.clone();
        let tx = self.vision_tx.clone();
        let waker = self.vision_waker.clone();
        std::thread::spawn(move || {
            let result = svc
                .translate(&text_to_translate, &lang_clone)
                .map_err(|e| e.to_string());
            let _ = tx.send(VisionJobResult::TranslationFinished {
                node_id: nid,
                target_lang: lang_clone,
                result,
            });
            if let Some(w) = waker {
                (w)();
            }
        });
    }

    /// Poll completed OCR / model download / translation background jobs on the UI loop.
    pub fn poll_vision_jobs(&mut self) -> bool {
        let mut changed = false;
        while let Ok(job) = self.vision_rx.try_recv() {
            changed = true;
            match job {
                VisionJobResult::ModelsDownloaded { result } => {
                    self.vision_downloading_models = false;
                    match result {
                        Ok(st) => {
                            self.neural_models_ready = st.is_ready();
                            self.vision_last_error = None;
                            self.status_message =
                                "Tier 2 neural OCR models downloaded and cached".into();
                        }
                        Err(err) => {
                            self.vision_last_error = Some(err.clone());
                            self.status_message = format!("Model download error: {}", err);
                        }
                    }
                }
                VisionJobResult::OcrFinished {
                    node_id,
                    mode,
                    result,
                } => {
                    self.vision_pending_nodes.remove(&node_id);
                    self.neural_models_ready =
                        self.active_vision_service().model_status().is_ready();
                    match result {
                        Ok(out) => {
                            let formatted = if mode == OcrMode::EquationFormula {
                                formula_math::expand_math_shorthand(&out.text)
                            } else {
                                out.text.clone()
                            };
                            self.vision_last_error = None;
                            if let Some(node) = self.graph.get_node_mut(&node_id) {
                                node.meta.extracted_text = if formatted.trim().is_empty() {
                                    None
                                } else {
                                    Some(formatted.clone())
                                };
                                node.meta.ocr_engine = Some(out.engine_label.clone());
                            }
                            if formatted.trim().is_empty() {
                                self.status_message = format!(
                                    "{} completed — no readable text detected",
                                    out.engine_label
                                );
                            } else {
                                self.status_message = format!(
                                    "{} extracted {} chars",
                                    out.engine_label,
                                    formatted.chars().count()
                                );
                            }
                            if let Some(bundle) = &self.bundle {
                                let _ = bundle.save_graph_atomic(&self.graph);
                            }
                        }
                        Err(err) => {
                            self.vision_last_error = Some(err.clone());
                            self.status_message = format!("OCR error: {}", err);
                        }
                    }
                }
                VisionJobResult::TranslationFinished {
                    node_id,
                    target_lang,
                    result,
                } => {
                    self.vision_pending_nodes.remove(&node_id);
                    match result {
                        Ok(out) => {
                            self.vision_last_error = None;
                            if let Some(node) = self.graph.get_node_mut(&node_id) {
                                node.meta.translated_text = Some(out.translated_text.clone());
                                node.meta.target_language = Some(out.target_language.clone());
                            }
                            self.status_message = format!(
                                "Translated to {} via {}",
                                target_lang.to_uppercase(),
                                out.backend_label
                            );
                            if let Some(bundle) = &self.bundle {
                                let _ = bundle.save_graph_atomic(&self.graph);
                            }
                        }
                        Err(err) => {
                            self.vision_last_error = Some(err.clone());
                            self.status_message = format!("Translation error: {}", err);
                        }
                    }
                }
            }
        }
        if changed {
            self.push_reader_snapshot();
        }
        changed
    }

    fn spawn_derived_card(
        &mut self,
        source_id: &NodeId,
        kind: NodeKind,
        title: String,
        content: String,
        edge_label: &str,
    ) -> Option<NodeId> {
        let (src_pos, src_title) = self
            .graph
            .get_node(source_id)
            .map(|n| (n.pos, n.title.clone()))?;
        let src_sz = self
            .graph
            .get_node(source_id)
            .map(node_world_size)
            .unwrap_or_else(|| node_base_size(kind));

        let sibling_count = self
            .graph
            .edges
            .values()
            .filter(|e| {
                e.from.node == *source_id && e.relation == EdgeRelation::DerivedFrom
            })
            .count() as f32;

        let spawn_pos = WorldPos::new(
            src_pos.x + src_sz.0 + 56.0,
            src_pos.y + sibling_count * 190.0,
        );
        let mut node = studynodes_graph::model::Node::new(kind, title, spawn_pos);
        node.size = node_base_size(kind);
        node.content = content;
        node.meta.source_node = Some(source_id.clone());
        if kind == NodeKind::Formula {
            node.meta.language = Some("MATH".into());
        }
        let new_id = node.id.clone();
        self.graph.add_node(node);

        let mut edge = Edge::new(
            EdgeEndpoint {
                node: source_id.clone(),
                port: PortId::new(PortId::DEFAULT_OUT),
            },
            EdgeEndpoint {
                node: new_id.clone(),
                port: PortId::new(PortId::DEFAULT_IN),
            },
        );
        edge.relation = EdgeRelation::DerivedFrom;
        edge.label = edge_label.to_string();
        edge.note = format!("Derived from '{}'", src_title);
        self.graph.add_edge(edge);

        self.persist_graph_and_snapshot();
        Some(new_id)
    }

    /// Spawn a linked `Note` card from an Image node's `meta.extracted_text` (`[derived from] OCR`).
    pub fn spawn_note_from_extracted_text(&mut self, image_id: &NodeId) -> Option<NodeId> {
        let (title, body) = self.graph.get_node(image_id).and_then(|n| {
            let txt = n
                .meta
                .extracted_text
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())?;
            Some((format!("{} · OCR", n.title), txt.to_string()))
        })?;
        let new_id = self.spawn_derived_card(image_id, NodeKind::Note, title, body, "OCR");
        if new_id.is_some() {
            self.status_message = "Spawned linked OCR Note".into();
        }
        new_id
    }

    /// Spawn a linked `Formula` card from an Image node's extracted equation (`[derived from] Equation OCR`).
    pub fn spawn_formula_from_extracted_text(&mut self, image_id: &NodeId) -> Option<NodeId> {
        let (title, eq_body) = self.graph.get_node(image_id).and_then(|n| {
            let txt = n
                .meta
                .extracted_text
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())?;
            let formatted = formula_math::expand_math_shorthand(txt);
            Some((format!("{} · Formula", n.title), formatted))
        })?;
        let new_id = self.spawn_derived_card(
            image_id,
            NodeKind::Formula,
            title,
            eq_body,
            "Equation OCR",
        );
        if new_id.is_some() {
            self.status_message = "Spawned linked Formula card from OCR".into();
        }
        new_id
    }

    /// Spawn a linked translated `Note` card from `meta.translated_text` (`[derived from] Translation`).
    pub fn spawn_note_from_translation(&mut self, source_id: &NodeId) -> Option<NodeId> {
        let (title, body) = self.graph.get_node(source_id).and_then(|n| {
            let tr = n
                .meta
                .translated_text
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())?;
            let lang = n
                .meta
                .target_language
                .as_deref()
                .unwrap_or(&self.reader_prefs.target_language)
                .to_uppercase();
            Some((format!("{} ({})", n.title, lang), tr.to_string()))
        })?;
        let new_id = self.spawn_derived_card(
            source_id,
            NodeKind::Note,
            title,
            body,
            "Translation",
        );
        if new_id.is_some() {
            self.status_message = "Spawned linked Translation Note".into();
        }
        new_id
    }

    /// Apply unlocked simultaneous 2D touchpad Pan and Zoom events (reversed pan direction; pinch=out, expand=in).
    pub fn apply_touchpad_events(&mut self, events: Vec<TouchpadEvent>) -> bool {
        if self.screen != AppScreen::Workspace || events.is_empty() {
            return false;
        }
        let mut changed = false;
        for ev in events {
            match ev {
                TouchpadEvent::Pan { dx, dy } => {
                    self.graph.viewport.pan_by_screen_delta(-dx, -dy);
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
