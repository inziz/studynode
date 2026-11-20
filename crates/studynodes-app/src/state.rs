use std::collections::HashSet;
use std::path::PathBuf;
use rune::TextEdit;
use serde::{Deserialize, Serialize};

use studynodes_graph::coords::{ScreenPos, WorldPos};
use studynodes_graph::model::{EdgeId, NodeId, PortId};
use studynodes_graph::{CommandHistory, ProjectBundle, StudyGraph};
use studynodes_llm::LlmProvider;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppScreen {
    Dashboard,
    Workspace,
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

pub struct AppState {
    pub screen: AppScreen,
    pub projects_dir: PathBuf,
    pub recent_projects: Vec<ProjectMetadata>,
    pub new_project_input: String,

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

    // Editing buffers
    pub edit_node_title: TextEdit,
    pub edit_node_content: TextEdit,
    pub edit_edge_label: TextEdit,
    pub edit_edge_note: TextEdit,
    pub append_note_input: TextEdit,
    pub editing_node_id: Option<NodeId>,
    pub space_held: bool,

    // Wire draft
    #[allow(dead_code)]
    pub wire_draft: Option<ActiveWireDraft>,
}

impl Default for AppState {
    fn default() -> Self {
        let projects_dir = dirs_or_local_projects_dir();
        let mut state = Self {
            screen: AppScreen::Dashboard,
            projects_dir,
            recent_projects: Vec::new(),
            new_project_input: "My Study Project".into(),
            bundle: None,
            graph: StudyGraph::new("Untitled Project"),
            history: CommandHistory::new(),
            canvas_mode: CanvasMode::Idle,
            selected_nodes: HashSet::new(),
            selected_edge: None,
            inspector_open: true,
            ai_panel_open: false,
            status_message: "Ready".into(),
            is_ai_loading: false,
            edit_node_title: TextEdit::new(""),
            edit_node_content: TextEdit::new(""),
            edit_edge_label: TextEdit::new(""),
            edit_edge_note: TextEdit::new(""),
            append_note_input: TextEdit::new(""),
            editing_node_id: None,
            space_held: false,
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
                self.screen = AppScreen::Workspace;
                self.status_message = format!("Created project '{}'", title);
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
                self.screen = AppScreen::Workspace;
                self.status_message = format!("Opened project '{}'", self.graph.title);
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
        self.refresh_recent_projects();
    }

    /// Sync active editing buffer back into the selected node.
    pub fn sync_node_edits(&mut self) {
        if let Some(id) = self.selected_nodes.iter().next().cloned() {
            if let Some(node) = self.graph.get_node_mut(&id) {
                let title = self.edit_node_title.text.clone();
                let content = self.edit_node_content.text.clone();
                let changed = node.title != title || node.content != content;
                node.title = title;
                node.content = content;
                if changed {
                    if let Some(bundle) = &self.bundle {
                        let _ = bundle.save_graph_atomic(&self.graph);
                    }
                }
            }
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

    /// Select a node and populate inspector buffer.
    pub fn select_node(&mut self, id: NodeId, additive: bool) {
        self.sync_node_edits();
        self.sync_edge_edits();
        if !additive {
            self.selected_nodes.clear();
        }
        self.selected_edge = None;
        self.selected_nodes.insert(id.clone());

        if let Some(n) = self.graph.get_node(&id) {
            self.edit_node_title = TextEdit::new(&n.title);
            self.edit_node_content = TextEdit::new(&n.content);
            self.inspector_open = true;
        }
    }

    /// Enter in-card editing mode directly on the canvas.
    #[allow(dead_code)]
    pub fn start_editing_node(&mut self, id: NodeId) {
        self.select_node(id.clone(), false);
        self.editing_node_id = Some(id);
        self.status_message = "Editing note directly on card".into();
    }

    /// Exit in-card editing mode and sync back to model.
    #[allow(dead_code)]
    pub fn stop_editing_node(&mut self) {
        self.sync_node_edits();
        self.editing_node_id = None;
        self.status_message = "Saved note changes".into();
    }

    /// Delete a node and all connected edges from the graph.
    pub fn delete_node(&mut self, id: &NodeId) {
        if let Some((removed, edges)) = self.graph.remove_node(id) {
            self.selected_nodes.remove(id);
            if self.editing_node_id.as_ref() == Some(id) {
                self.editing_node_id = None;
            }
            if let Some(bundle) = &self.bundle {
                let _ = bundle.save_graph_atomic(&self.graph);
            }
            self.status_message = format!("Deleted '{}' (removed {} links)", removed.title, edges.len());
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
                    "✦ AI explained '{}' ({} proposed connection(s))",
                    self.graph.get_node(id).map(|n| n.title.as_str()).unwrap_or("node"),
                    res.proposed_edges.len()
                );
            }
        }
    }

    /// Connect two nodes via an Edge with auto-persistence.
    #[allow(dead_code)]
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
        self.status_message = format!("Connected wire: '{}' → '{}'", from_title, to_title);
    }

    /// Select an edge and populate inspector buffer.
    pub fn select_edge(&mut self, id: EdgeId) {
        self.sync_node_edits();
        self.sync_edge_edits();
        self.selected_nodes.clear();
        self.selected_edge = Some(id.clone());

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
                self.edit_node_content = TextEdit::new(&node.content);
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

    /// Create and select a new Note node.
    pub fn add_note_node(&mut self, pos_opt: Option<WorldPos>) -> NodeId {
        let pos = pos_opt.unwrap_or_else(|| {
            let center = self.graph.viewport.screen_to_world(ScreenPos::new(360.0, 240.0));
            WorldPos::new(center.x, center.y)
        });
        let mut node = studynodes_graph::model::Node::new(
            studynodes_graph::model::NodeKind::Note,
            "New Note",
            pos,
        );
        node.content = "Write study observations or synthesis notes here...".into();
        let id = node.id.clone();
        self.graph.add_node(node);
        self.select_node(id.clone(), false);
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
        self.status_message = "Created new Note".into();
        id
    }

    /// Create and select a new Document node.
    pub fn add_document_node(&mut self, pos_opt: Option<WorldPos>) -> NodeId {
        let pos = pos_opt.unwrap_or_else(|| {
            let center = self.graph.viewport.screen_to_world(ScreenPos::new(420.0, 280.0));
            WorldPos::new(center.x, center.y)
        });
        let mut node = studynodes_graph::model::Node::new(
            studynodes_graph::model::NodeKind::Document,
            "New Document",
            pos,
        );
        node.content = "# Research Document\n\nIngest a PDF, EPUB, or markdown file, or write source document text here.".into();
        let id = node.id.clone();
        self.graph.add_node(node);
        self.select_node(id.clone(), false);
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
        self.status_message = "Created new Document node".into();
        id
    }

    /// Load a rich sample document into a node.
    pub fn load_sample_document(&mut self, node_id: &NodeId) {
        if let Some(node) = self.graph.get_node_mut(node_id) {
            node.title = "Attention Is All You Need (Excerpt)".into();
            node.content = r#"# Attention Is All You Need
Vaswani et al., 2017

## Abstract
The dominant sequence transduction models are based on complex recurrent or convolutional neural networks that include an encoder and a decoder. The best performing models also connect the encoder and decoder through an attention mechanism. We propose a new simple network architecture, the Transformer, based solely on attention mechanisms, dispensing with recurrence and convolutions entirely.

## Model Architecture
The Transformer follows an encoder-decoder structure using stacked self-attention and point-wise, fully connected layers for both the encoder and decoder.

### Multi-Head Attention
Multi-head attention allows the model to jointly attend to information from different representation subspaces at different positions:
MultiHead(Q, K, V) = Concat(head_1, ..., head_h) W^O
where head_i = Attention(Q W_i^Q, K W_i^K, V W_i^V)

### Scaled Dot-Product Attention
Attention(Q, K, V) = softmax(Q K^T / sqrt(d_k)) V"#.into();
            self.edit_node_title = TextEdit::new(&node.title);
            self.edit_node_content = TextEdit::new(&node.content);
            self.status_message = "Loaded sample academic paper into document".into();
        }
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
    }

    /// Ingest a file into a specific node.
    pub fn ingest_file_into_node(&mut self, node_id: &NodeId, path: &PathBuf) {
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Imported Document".into());

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let text_content = if matches!(ext.as_str(), "txt" | "md" | "json" | "rs" | "toml" | "csv" | "tex") {
            std::fs::read_to_string(path).ok()
        } else {
            None
        };

        if let Some(bundle) = &self.bundle {
            if let Ok((hash, _)) = bundle.ingest_asset(path) {
                if let Some(node) = self.graph.get_node_mut(node_id) {
                    node.asset_hash = Some(hash.clone());
                    if node.title.is_empty() || node.title == "New Document" || node.title == "New Note" {
                        node.title = filename.clone();
                    }
                    if let Some(content) = text_content {
                        node.content = content;
                    } else if node.content.is_empty() || node.content.starts_with("# Research Document") {
                        node.content = format!(
                            "# Document: {}\n\n• File: {}\n• SHA-256: {}\n• Ingested into project assets library.\n",
                            filename,
                            path.display(),
                            hash
                        );
                    }
                    self.edit_node_title = TextEdit::new(&node.title);
                    self.edit_node_content = TextEdit::new(&node.content);
                }
                let _ = bundle.save_graph_atomic(&self.graph);
                self.status_message = format!("Ingested '{}' into document node", filename);
                return;
            }
        }

        // Fallback without bundle:
        if let Some(content) = text_content {
            if let Some(node) = self.graph.get_node_mut(node_id) {
                if node.title.is_empty() || node.title == "New Document" || node.title == "New Note" {
                    node.title = filename.clone();
                }
                node.content = content;
                self.edit_node_title = TextEdit::new(&node.title);
                self.edit_node_content = TextEdit::new(&node.content);
            }
            self.status_message = format!("Loaded text from '{}'", filename);
        } else {
            self.status_message = format!("Ingested file reference: '{}'", filename);
        }
    }

    /// Ingest dropped files into active project.
    pub fn handle_dropped_files(&mut self, files: &[PathBuf], drop_screen_pos: ScreenPos) {
        let bundle = match &self.bundle {
            Some(b) => b.clone(),
            None => return,
        };

        let drop_world = self.graph.viewport.screen_to_world(drop_screen_pos);
        let mut offset_x = 0.0;

        for path in files {
            if let Ok((hash, _)) = bundle.ingest_asset(path) {
                let filename = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Imported File".into());

                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                let kind = match ext.as_str() {
                    "png" | "jpg" | "jpeg" | "gif" | "svg" => studynodes_graph::model::NodeKind::Image,
                    "pdf" | "epub" => studynodes_graph::model::NodeKind::Document,
                    "md" | "txt" => {
                        // Extract content if small text file
                        let content = std::fs::read_to_string(path).unwrap_or_default();
                        let mut node = studynodes_graph::model::Node::new(
                            studynodes_graph::model::NodeKind::Note,
                            filename.clone(),
                            WorldPos::new(drop_world.x + offset_x, drop_world.y),
                        );
                        node.content = content;
                        node.asset_hash = Some(hash.clone());
                        self.graph.add_node(node);
                        offset_x += 300.0;
                        continue;
                    }
                    _ => studynodes_graph::model::NodeKind::Document,
                };

                let mut node = studynodes_graph::model::Node::new(
                    kind,
                    filename,
                    WorldPos::new(drop_world.x + offset_x, drop_world.y),
                );
                node.asset_hash = Some(hash);
                self.graph.add_node(node);
                offset_x += 300.0;
            }
        }

        let _ = bundle.save_graph_atomic(&self.graph);
        self.status_message = format!("Ingested {} file(s) into project canvas", files.len());
    }
}
