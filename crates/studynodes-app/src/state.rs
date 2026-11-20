use std::collections::HashSet;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use studynodes_graph::coords::{ScreenPos, WorldPos};
use studynodes_graph::model::{EdgeId, NodeId, PortId};
use studynodes_graph::{CommandHistory, ProjectBundle, StudyGraph};

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
        grab_offset: WorldPos,
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
    pub edit_node_title: String,
    pub edit_node_content: String,
    pub edit_edge_label: String,
    pub edit_edge_note: String,

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
            edit_node_title: String::new(),
            edit_node_content: String::new(),
            edit_edge_label: String::new(),
            edit_edge_note: String::new(),
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
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
        self.screen = AppScreen::Dashboard;
        self.refresh_recent_projects();
    }

    /// Select a node and populate inspector buffer.
    pub fn select_node(&mut self, id: NodeId, additive: bool) {
        if !additive {
            self.selected_nodes.clear();
        }
        self.selected_edge = None;
        self.selected_nodes.insert(id.clone());

        if let Some(n) = self.graph.get_node(&id) {
            self.edit_node_title = n.title.clone();
            self.edit_node_content = n.content.clone();
        }
    }

    /// Select an edge and populate inspector buffer.
    pub fn select_edge(&mut self, id: EdgeId) {
        self.selected_nodes.clear();
        self.selected_edge = Some(id.clone());

        if let Some(e) = self.graph.get_edge(&id) {
            self.edit_edge_label = e.label.clone();
            self.edit_edge_note = e.note.clone();
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
