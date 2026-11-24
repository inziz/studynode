use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime};

use rune::{ImageCache, Live, MediaSource, Pixels, Player, TextEdit, Tx, Waker, WindowHandle};
use serde::{Deserialize, Serialize};

use studynodes_graph::coords::{Camera, ScreenPos, WorldPos};
use studynodes_graph::model::{
    CalendarEvent, CalendarViewMode, DrawingStroke, Edge, EdgeEndpoint, EdgeId, EdgeRelation,
    EdgeStatus, MediaTimestamp, Node, NodeId, NodeKind, PortDirection, PortId, QuizDifficulty,
    QuizDisplayMode, QuizQuestion, SubtitleCue, resolve_trace_color_hex, resolve_trace_color_label,
};
use studynodes_graph::{
    ChatMessage, ChatThread, CommandHistory, GraphCommand, ProjectBundle, StudyGraph,
};
use studynodes_llm::{LlmProvider, resolve_universal_llm_provider};
use studynodes_vision::{
    LocalVisionService, NeuralModelStatus, OcrEnginePreference, OcrMode, OcrOutput,
    TranslationOutput, VisionConfig, VisionService,
};

use crate::doc_reader::{
    self, DocFormat, DockPosition, PdfLayoutMode, ReaderMode, ReaderPreferences,
    extract_document_markdown,
};
use crate::formula_math;
use crate::graph_formula;
use crate::i18n::{self, UiLocale};
use crate::media_helper;
use crate::project_io;
use crate::reader_window::{
    ReaderBridge, ReaderCommand, ReaderSnapshot, SectionChildSnapshot, SectionEdgeSnapshot,
    can_pop_out, open_reader_window,
};
use crate::touchpad_win32::TouchpadEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppScreen {
    Dashboard,
    Workspace,
    Settings,
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
    pub page_dimensions: Vec<(f32, f32)>,
    pub pages: HashMap<usize, (u32, u32, Pixels)>,
    pub loading_pages: HashSet<usize>,
    pub error: Option<String>,
}

impl ActivePdfState {
    #[allow(dead_code)]
    pub fn current_pixels(&self) -> Option<&Pixels> {
        self.pages.get(&self.page_index).map(|(_, _, px)| px)
    }

    #[allow(dead_code)]
    pub fn current_page_size(&self) -> (u32, u32) {
        self.pages
            .get(&self.page_index)
            .map(|(w, h, _)| (*w, *h))
            .unwrap_or((0, 0))
    }
}

pub struct PdfWorkerRequest {
    pub node_id: NodeId,
    pub asset_path: PathBuf,
    pub page_index: usize,
    pub render_scale: f32,
    pub zoom_bucket: u32,
    pub for_card_thumbnail: bool,
}

pub enum BackgroundIoResult {
    RecentProjectsLoaded(Vec<ProjectMetadata>),
    PdfRendered {
        node_id: NodeId,
        asset_path: PathBuf,
        page_index: usize,
        zoom_bucket: u32,
        for_card_thumbnail: bool,
        result: Result<doc_reader::RenderedPdfPage, String>,
    },
    ScreenshotFinished {
        result: Result<PathBuf, String>,
    },
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
    #[serde(default)]
    pub collection: Option<String>,
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
    DraggingDock {
        start_pos: [f32; 2],
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
    ResizingNode {
        node_id: NodeId,
        start_size: (f32, f32),
        min_size: (f32, f32),
        max_size: (f32, f32),
    },
    DraggingWire {
        from_node: NodeId,
        from_port: PortId,
        current_cursor: WorldPos,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuizAttemptStatus {
    #[default]
    NotStarted,
    InProgress,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuizSessionState {
    pub active_index: usize,
    pub selected_options: HashMap<usize, usize>,
    pub revealed_hints: HashMap<usize, usize>,
    pub revealed_answers: HashSet<usize>,
    pub status: QuizAttemptStatus,
    pub gave_up: bool,
    pub difficulty_used: Option<QuizDifficulty>,
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
        NodeKind::GraphFormula => (360.0, 320.0),
        NodeKind::Checklist => (260.0, 195.0),
        NodeKind::Sequence => (248.0, 145.0),
        NodeKind::Video => (360.0, 248.0),
        NodeKind::Audio => (288.0, 124.0),
        NodeKind::Calendar => (312.0, 286.0),
        NodeKind::Drawing => (320.0, 240.0),
        NodeKind::Text => (240.0, 64.0),
        NodeKind::Quiz => (320.0, 260.0),
    }
}

/// Minimum and maximum `(width, height)` in world coordinates for canvas-resizable nodes
/// (`Section` uses its dedicated banner/frame resize logic).
pub(crate) fn resizable_node_bounds(kind: NodeKind) -> Option<((f32, f32), (f32, f32))> {
    match kind {
        NodeKind::Note => Some(((200.0, 120.0), (960.0, 760.0))),
        NodeKind::Document => Some(((220.0, 160.0), (960.0, 820.0))),
        NodeKind::Image => Some(((120.0, 90.0), (1400.0, 1100.0))),
        NodeKind::Flashcard => Some(((210.0, 140.0), (860.0, 640.0))),
        NodeKind::Video => Some(((240.0, 176.0), (1280.0, 860.0))),
        NodeKind::Audio => Some(((230.0, 112.0), (900.0, 520.0))),
        NodeKind::Drawing => Some(((240.0, 180.0), (960.0, 720.0))),
        NodeKind::GraphFormula => Some(((260.0, 220.0), (1100.0, 880.0))),
        NodeKind::Text => Some(((64.0, 28.0), (1200.0, 900.0))),
        NodeKind::Quiz => Some(((240.0, 128.0), (960.0, 780.0))),
        _ => None,
    }
}

/// Effective world dimensions `(w, h)` for a specific node instance (honors resizable cards and Calendar `Week` bounds).
pub fn node_world_size(node: &studynodes_graph::model::Node) -> (f32, f32) {
    let is_unmodified_default = (node.size.0 - studynodes_graph::model::Node::DEFAULT_WIDTH).abs()
        < 0.5
        && (node.size.1 - studynodes_graph::model::Node::DEFAULT_HEIGHT).abs() < 0.5;
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
    } else if node.kind == NodeKind::Calendar
        && node.meta.calendar_view_mode == CalendarViewMode::Week
    {
        (680.0, 200.0)
    } else if node.kind == NodeKind::Audio {
        let has_subs = node.meta.show_subtitles_on_card
            && (!node.meta.subtitles.is_empty()
                || node
                    .meta
                    .transcript_text
                    .as_deref()
                    .is_some_and(|t| !t.trim().is_empty()));
        let is_iframe = node
            .meta
            .media_url
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .is_some_and(AppState::media_uses_iframe);
        let is_auto_audio = is_unmodified_default
            || ((node.size.0 - 288.0).abs() < 0.5 && (node.size.1 - 124.0).abs() < 0.5)
            || ((node.size.0 - 304.0).abs() < 0.5 && (node.size.1 - 210.0).abs() < 0.5)
            || ((node.size.0 - 320.0).abs() < 0.5 && (node.size.1 - 176.0).abs() < 0.5);
        let (min_sz, max_sz) = resizable_node_bounds(NodeKind::Audio).unwrap();
        if !is_auto_audio && node.size.0 >= 200.0 && node.size.1 >= 100.0 {
            let mut h = node.size.1;
            if has_subs && h < 190.0 {
                h = 190.0;
            }
            (node.size.0.clamp(min_sz.0, max_sz.0), h.clamp(min_sz.1, max_sz.1))
        } else if has_subs {
            (304.0, 210.0)
        } else if is_iframe {
            (320.0, 176.0)
        } else {
            node_base_size(NodeKind::Audio)
        }
    } else if let Some((min_sz, max_sz)) = resizable_node_bounds(node.kind) {
        if !is_unmodified_default
            && node.size.0 >= min_sz.0 - 0.5
            && node.size.1 >= min_sz.1 - 0.5
        {
            (
                node.size.0.clamp(min_sz.0, max_sz.0),
                node.size.1.clamp(min_sz.1, max_sz.1),
            )
        } else if node.kind == NodeKind::Text && node.meta.text_auto_fit {
            compute_text_node_fit_size(node)
        } else {
            node_base_size(node.kind)
        }
    } else {
        node_base_size(node.kind)
    }
}

/// Compute tight world `(width, height)` fitting a frameless `Text` node's content, font, and size.
pub fn compute_text_node_fit_size(node: &studynodes_graph::model::Node) -> (f32, f32) {
    let pt = node.meta.text_size.unwrap_or(18.0).clamp(10.0, 96.0);
    let advance = match node.meta.text_font.as_deref().unwrap_or("sans") {
        "mono" => 0.62_f32,
        "serif" => 0.54,
        "display" | "hand" => 0.58,
        _ => 0.55,
    };
    let text_src = if node.content.trim().is_empty() {
        "Text"
    } else {
        node.content.as_str()
    };
    let ((min_w, min_h), (max_w, max_h)) =
        resizable_node_bounds(NodeKind::Text).unwrap_or(((64.0, 28.0), (1200.0, 900.0)));
    let max_line_chars = text_src
        .lines()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(4)
        .max(2) as f32;
    let ideal_w = (max_line_chars * pt * advance + 18.0).clamp(min_w, max_w);
    let usable_w = (ideal_w - 10.0).max(16.0);
    let chars_per_line = (usable_w / (pt * advance).max(1.0)).floor().max(1.0);
    let mut wrapped_lines = 0_usize;
    for raw_line in text_src.split('\n') {
        let len = raw_line.chars().count();
        wrapped_lines += ((len as f32) / chars_per_line).ceil().max(1.0) as usize;
    }
    let wrapped_lines = wrapped_lines.max(1) as f32;
    let ideal_h = (wrapped_lines * pt * 1.28 + 10.0).clamp(min_h, max_h);
    (ideal_w, ideal_h)
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

#[derive(Clone, Debug)]
pub struct AiChatMessage {
    pub role: &'static str, // "user" or "assistant"
    pub text: String,
    pub action_summary: Option<String>,
    pub can_undo: bool,
}

pub enum LlmJobResult {
    SummaryFinished {
        node_id: NodeId,
        provider_label: String,
        result: Result<String, String>,
    },
    PatchFinished {
        task_label: String,
        provider_label: String,
        result: Result<studynodes_graph::patch::GraphPatch, String>,
    },
    ChatFinished {
        project_id: String,
        chat_id: String,
        provider_label: String,
        result: Result<String, String>,
    },
    ChatWithActionsFinished {
        project_id: String,
        chat_id: String,
        provider_label: String,
        result: Result<studynodes_llm::ChatActionResponse, String>,
    },
}

pub enum MediaJobResult {
    OembedTitleFetched {
        node_id: NodeId,
        title: Option<String>,
    },
    ImageSearched {
        node_id: NodeId,
        query: String,
        result: Result<media_helper::DownloadedImage, String>,
    },
    SubtitlesGenerated {
        node_id: NodeId,
        result: Result<studynodes_llm::AudioTranscriptionResult, String>,
    },
}

pub struct AppState {
    pub screen: AppScreen,
    pub settings_return_screen: AppScreen,
    pub is_fullscreen: bool,
    pub projects_dir: PathBuf,
    pub recent_projects: Vec<ProjectMetadata>,
    pub new_project_input: TextEdit,
    pub project_search_input: TextEdit,
    pub confirm_delete_project: Option<PathBuf>,
    pub active_collection_filter: Option<String>,
    pub new_collection_input: TextEdit,
    pub show_new_collection_input: bool,
    pub assigning_collection_for: Option<PathBuf>,

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
    pub committed_viewport: Camera,
    pub cam_live_pan: Live<[f32; 2]>,
    pub cam_live_zoom: Live<f32>,
    pub cam_live_dirty: bool,
    pub cam_live_last_input_at: Option<Instant>,
    pub smooth_zoom_enabled: bool,
    pub smooth_zoom_target: Option<f32>,
    pub smooth_zoom_anchor: ScreenPos,
    pub smooth_zoom_rate: f32,
    pub last_zoom_step_clock: Option<f64>,

    // Integrated & Custom Document Reader + Card Preview state
    pub reader_prefs: ReaderPreferences,
    pub reader_read_only: bool,
    pub custom_reader_edit: TextEdit,
    pub reader_settings_open: bool,
    pub doc_view_tab: DocViewTab,
    pub active_pdf: Option<ActivePdfState>,
    pub image_cache: ImageCache,
    pub decoded_images: HashMap<PathBuf, rune::Image>,
    pub active_image: Option<(NodeId, PathBuf, rune::Image)>,
    pub card_pdf_previews: HashMap<(NodeId, usize), CardPdfPreview>,
    pub card_image_previews: HashMap<NodeId, rune::Image>,

    // Wire draft
    #[allow(dead_code)]
    pub wire_draft: Option<ActiveWireDraft>,
    pub show_block_menu: bool,
    pub block_menu_anchor: Option<ScreenPos>,
    pub block_menu_spawn_world: Option<WorldPos>,
    pub show_ai_menu: bool,
    pub inspector_more_menu_open: bool,
    pub copied_feedback: Option<(&'static str, Instant)>,
    pub toast_text: Option<String>,
    pub toast_expires_at: Option<Instant>,
    pub last_toast_status: String,
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
    pub vision_settings_open: bool,
    pub vision_api_key_edit: TextEdit,
    pub vision_base_url_edit: TextEdit,
    pub vision_model_edit: TextEdit,

    // Settings page & background AI LLM pipeline (non-blocking)
    pub ai_api_key_edit: TextEdit,
    pub ai_base_url_edit: TextEdit,
    pub ai_model_edit: TextEdit,
    pub chat_api_key_edit: TextEdit,
    pub chat_base_url_edit: TextEdit,
    pub chat_model_edit: TextEdit,
    pub ai_chat_key_drawer_open: bool,
    pub ollama_url_edit: TextEdit,
    pub ollama_model_edit: TextEdit,
    pub llm_provider_override: Option<Arc<dyn LlmProvider>>,
    pub llm_tx: mpsc::Sender<LlmJobResult>,
    pub llm_rx: mpsc::Receiver<LlmJobResult>,
    pub llm_waker: Option<Waker>,
    pub llm_pending_nodes: HashSet<NodeId>,
    pub llm_pending_patches: usize,
    pub llm_last_error: Option<String>,
    pub ai_chat_messages: Vec<AiChatMessage>,
    pub ai_chat_input: TextEdit,
    pub ai_chat_pending: bool,
    pub ai_chat_pending_target: Option<(String, String)>,
    pub ai_chat_canvas_actions: bool,

    // Video & Music playback + HTML iframe embed + subtitles + image search buffers
    pub media_players: HashMap<NodeId, (String, Player)>,
    pub media_url_edit: TextEdit,
    pub media_ts_time_edit: TextEdit,
    pub media_ts_label_edit: TextEdit,
    pub media_resolve_tx: mpsc::Sender<MediaJobResult>,
    pub media_resolve_rx: mpsc::Receiver<MediaJobResult>,
    pub embed_rects: HashMap<NodeId, [f32; 4]>,
    pub embed_collapsed: HashSet<NodeId>,
    pub embed_start_secs: HashMap<NodeId, (u64, u64)>,
    pub oembed_fetched: HashSet<String>,
    pub image_search_pending: HashSet<NodeId>,
    pub audio_subtitle_pending: HashSet<NodeId>,
    pub audio_subtitle_errors: HashMap<NodeId, String>,

    // Calendar selection & event creation buffers
    pub selected_calendar_date: HashMap<NodeId, String>,
    pub cal_event_date_edit: TextEdit,
    pub cal_event_time_edit: TextEdit,
    pub cal_event_title_edit: TextEdit,
    pub cal_event_notes_edit: TextEdit,
    pub cal_day_note_edit: TextEdit,

    // Drawing canvas tool settings
    pub drawing_color_hex: String,
    pub drawing_stroke_width: f32,
    pub drawing_eraser: bool,
    pub dock_measured_size: Option<[f32; 2]>,

    // Interactive Quiz card sessions, full-screen overlay state, & question composer buffers
    pub quiz_sessions: HashMap<NodeId, QuizSessionState>,
    pub active_quiz_overlay: Option<NodeId>,
    pub quiz_q_edit: TextEdit,
    pub quiz_opts_edit: TextEdit,
    pub quiz_ans_edit: TextEdit,
    pub quiz_hint_edit: TextEdit,
    pub quiz_correct_idx: usize,

    // Graphic Formula sidebar keypad mode & plot pan state
    pub graph_keypad_mode: graph_formula::KeypadMode,
    pub graph_drag_origin: Option<(NodeId, f64, f64)>,

    // Performance caches & non-blocking background IO workers
    pub asset_path_cache: HashMap<String, PathBuf>,
    pub doc_preview_cache: RefCell<HashMap<NodeId, (u64, Vec<doc_reader::TextPreviewPage>)>>,
    pub pdf_dimensions_cache: HashMap<PathBuf, Vec<(f32, f32)>>,
    pub pdf_page_cache: HashMap<(PathBuf, usize, u32), (u32, u32, Pixels)>,
    pub pdf_lru_order: VecDeque<(PathBuf, usize, u32)>,
    pub pdf_inflight: HashSet<(PathBuf, usize, u32)>,
    pub pdf_tx: mpsc::Sender<PdfWorkerRequest>,
    pub io_tx: mpsc::Sender<BackgroundIoResult>,
    pub io_rx: mpsc::Receiver<BackgroundIoResult>,
    pub io_waker_slot: Arc<Mutex<Option<Waker>>>,
    pub save_tx: mpsc::Sender<(ProjectBundle, StudyGraph)>,
    pub pending_graph_save_at: Option<Instant>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::with_projects_dir_opts(dirs_or_local_projects_dir(), cfg!(test))
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

fn spawn_pdf_worker(
    pdf_rx: mpsc::Receiver<PdfWorkerRequest>,
    io_tx: mpsc::Sender<BackgroundIoResult>,
    waker_slot: Arc<Mutex<Option<Waker>>>,
) {
    std::thread::spawn(move || {
        let mut sessions: Vec<(PathBuf, doc_reader::PdfRenderSession)> = Vec::new();
        while let Ok(req) = pdf_rx.recv() {
            let result = (|| -> Result<doc_reader::RenderedPdfPage, String> {
                if let Some(pos) = sessions.iter().position(|(p, _)| p == &req.asset_path) {
                    if pos != 0 {
                        let item = sessions.remove(pos);
                        sessions.insert(0, item);
                    }
                    sessions[0].1.render_page(req.page_index, req.render_scale)
                } else {
                    let sess = doc_reader::PdfRenderSession::open(&req.asset_path)?;
                    let page = sess.render_page(req.page_index, req.render_scale)?;
                    sessions.insert(0, (req.asset_path.clone(), sess));
                    if sessions.len() > 4 {
                        sessions.pop();
                    }
                    Ok(page)
                }
            })();
            let _ = io_tx.send(BackgroundIoResult::PdfRendered {
                node_id: req.node_id,
                asset_path: req.asset_path,
                page_index: req.page_index,
                zoom_bucket: req.zoom_bucket,
                for_card_thumbnail: req.for_card_thumbnail,
                result,
            });
            if let Ok(guard) = waker_slot.lock() {
                if let Some(w) = guard.as_ref() {
                    (w)();
                }
            }
        }
    });
}

fn spawn_graph_saver_worker(save_rx: mpsc::Receiver<(ProjectBundle, StudyGraph)>) {
    std::thread::spawn(move || {
        while let Ok((bundle, graph)) = save_rx.recv() {
            let mut batch: HashMap<PathBuf, (ProjectBundle, StudyGraph)> = HashMap::new();
            batch.insert(bundle.root_dir.clone(), (bundle, graph));
            while let Ok((b, g)) = save_rx.try_recv() {
                batch.insert(b.root_dir.clone(), (b, g));
            }
            for (_, (b, g)) in batch {
                let _ = b.save_graph_atomic(&g);
            }
        }
    });
}

impl AppState {
    pub fn with_projects_dir(projects_dir: PathBuf) -> Self {
        Self::with_projects_dir_opts(projects_dir, true)
    }

    fn with_projects_dir_opts(projects_dir: PathBuf, sync_scan: bool) -> Self {
        let _ = std::fs::create_dir_all(&projects_dir);
        let reader_prefs = doc_reader::load_preferences(&projects_dir);
        crate::theme::set_active_theme(reader_prefs.theme);
        crate::i18n::set_active_locale(reader_prefs.ui_locale);
        crate::touchpad_win32::set_enabled(reader_prefs.use_direct_manipulation);
        let custom_reader_edit = TextEdit::new(&reader_prefs.custom_reader_path);
        let inspector_width = reader_prefs.inspector_width.clamp(300.0, 860.0);
        let reader_read_only = reader_prefs.read_only;
        let vision_api_key_edit = TextEdit::new(&reader_prefs.vision_api_key);
        let vision_base_url_edit = TextEdit::new(&reader_prefs.vision_base_url);
        let vision_model_edit = TextEdit::new(&reader_prefs.vision_model);
        let ai_api_key_edit = TextEdit::new(&reader_prefs.ai_api_key);
        let ai_base_url_edit = TextEdit::new(&reader_prefs.ai_base_url);
        let ai_model_edit = TextEdit::new(&reader_prefs.ai_model);
        let chat_api_key_edit = TextEdit::new(&reader_prefs.chat_api_key);
        let chat_base_url_edit = TextEdit::new(&reader_prefs.chat_base_url);
        let chat_model_edit = TextEdit::new(&reader_prefs.chat_model);
        let ollama_url_edit = TextEdit::new(&reader_prefs.ollama_url);
        let ollama_model_edit = TextEdit::new(&reader_prefs.ollama_model);
        let neural_models_ready =
            studynodes_vision::ppocr::model_status(&projects_dir.join("models")).is_ready();
        let (vision_tx, vision_rx) = mpsc::channel();
        let (llm_tx, llm_rx) = mpsc::channel();
        let (io_tx, io_rx) = mpsc::channel();
        let (pdf_tx, pdf_rx) = mpsc::channel();
        let (save_tx, save_rx) = mpsc::channel();
        let (media_resolve_tx, media_resolve_rx) = mpsc::channel();
        let io_waker_slot: Arc<Mutex<Option<Waker>>> = Arc::new(Mutex::new(None));
        spawn_pdf_worker(pdf_rx, io_tx.clone(), io_waker_slot.clone());
        spawn_graph_saver_worker(save_rx);

        let (today_y, today_m, today_d) = media_helper::today_ymd();
        let today_str = media_helper::format_ymd(today_y, today_m, today_d);

        let mut state = Self {
            screen: AppScreen::Dashboard,
            settings_return_screen: AppScreen::Dashboard,
            is_fullscreen: false,
            projects_dir,
            recent_projects: Vec::new(),
            new_project_input: TextEdit::new(""),
            project_search_input: TextEdit::new(""),
            confirm_delete_project: None,
            active_collection_filter: None,
            new_collection_input: TextEdit::new(""),
            show_new_collection_input: false,
            assigning_collection_for: None,
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
            canvas_origin: [0.0, 36.0],
            canvas_size: [1280.0, 764.0],
            committed_viewport: Camera::default(),
            cam_live_pan: Live::new([0.0, 0.0]),
            cam_live_zoom: Live::new(1.0),
            cam_live_dirty: false,
            cam_live_last_input_at: None,
            smooth_zoom_enabled: false,
            smooth_zoom_target: None,
            smooth_zoom_anchor: ScreenPos::new(640.0, 382.0),
            smooth_zoom_rate: 22.0,
            last_zoom_step_clock: None,
            reader_prefs,
            reader_read_only,
            custom_reader_edit,
            reader_settings_open: false,
            doc_view_tab: DocViewTab::Preview,
            active_pdf: None,
            image_cache: ImageCache::new(Arc::new(|| {})),
            decoded_images: HashMap::new(),
            active_image: None,
            card_pdf_previews: HashMap::new(),
            card_image_previews: HashMap::new(),
            wire_draft: None,
            show_block_menu: false,
            block_menu_anchor: None,
            block_menu_spawn_world: None,
            show_ai_menu: false,
            inspector_more_menu_open: false,
            copied_feedback: None,
            toast_text: None,
            toast_expires_at: None,
            last_toast_status: "Ready".into(),
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
            vision_settings_open: false,
            vision_api_key_edit,
            vision_base_url_edit,
            vision_model_edit,
            ai_api_key_edit,
            ai_base_url_edit,
            ai_model_edit,
            chat_api_key_edit,
            chat_base_url_edit,
            chat_model_edit,
            ai_chat_key_drawer_open: false,
            ollama_url_edit,
            ollama_model_edit,
            llm_provider_override: None,
            llm_tx,
            llm_rx,
            llm_waker: None,
            llm_pending_nodes: HashSet::new(),
            llm_pending_patches: 0,
            llm_last_error: None,
            ai_chat_messages: Vec::new(),
            ai_chat_input: TextEdit::new(""),
            ai_chat_pending: false,
            ai_chat_pending_target: None,
            ai_chat_canvas_actions: true,
            media_players: HashMap::new(),
            media_url_edit: TextEdit::new(""),
            media_ts_time_edit: TextEdit::new(""),
            media_ts_label_edit: TextEdit::new(""),
            media_resolve_tx,
            media_resolve_rx,
            embed_rects: HashMap::new(),
            embed_collapsed: HashSet::new(),
            embed_start_secs: HashMap::new(),
            oembed_fetched: HashSet::new(),
            image_search_pending: HashSet::new(),
            audio_subtitle_pending: HashSet::new(),
            audio_subtitle_errors: HashMap::new(),
            selected_calendar_date: HashMap::new(),
            cal_event_date_edit: TextEdit::new(&today_str),
            cal_event_time_edit: TextEdit::new("18:00"),
            cal_event_title_edit: TextEdit::new(""),
            cal_event_notes_edit: TextEdit::new(""),
            cal_day_note_edit: TextEdit::new(""),
            drawing_color_hex: "#F4F3EE".into(),
            drawing_stroke_width: 2.6,
            drawing_eraser: false,
            dock_measured_size: None,
            quiz_sessions: HashMap::new(),
            active_quiz_overlay: None,
            quiz_q_edit: TextEdit::new(""),
            quiz_opts_edit: TextEdit::new(""),
            quiz_ans_edit: TextEdit::new(""),
            quiz_hint_edit: TextEdit::new(""),
            quiz_correct_idx: 0,
            graph_keypad_mode: graph_formula::KeypadMode::default(),
            graph_drag_origin: None,
            asset_path_cache: HashMap::new(),
            doc_preview_cache: RefCell::new(HashMap::new()),
            pdf_dimensions_cache: HashMap::new(),
            pdf_page_cache: HashMap::new(),
            pdf_lru_order: VecDeque::new(),
            pdf_inflight: HashSet::new(),
            pdf_tx,
            io_tx,
            io_rx,
            io_waker_slot,
            save_tx,
            pending_graph_save_at: None,
        };

        if sync_scan {
            state.refresh_recent_projects();
        } else {
            state.refresh_recent_projects_async();
        }
        state
    }

    pub fn set_wakers(&mut self, waker: Waker) {
        self.image_cache = ImageCache::new(waker.clone());
        self.vision_waker = Some(waker.clone());
        self.llm_waker = Some(waker.clone());
        if let Ok(mut guard) = self.io_waker_slot.lock() {
            *guard = Some(waker.clone());
        }
        if let Ok(mut b) = self.reader_bridge.lock() {
            b.main_waker = Some(waker.clone());
        }
        for (_, player) in self.media_players.values() {
            player.set_waker(waker.clone());
        }
        // Rebind any pre-setup handles so background image decodes wake the real window.
        if !self.decoded_images.is_empty()
            || !self.card_image_previews.is_empty()
            || self.active_image.is_some()
        {
            self.decoded_images.clear();
            self.card_image_previews.clear();
            self.active_image = None;
            self.refresh_card_previews();
            if let Some(nid) = self.selected_nodes.iter().next().cloned() {
                self.ensure_reader_media_for_node(&nid, false);
                self.push_reader_snapshot();
            }
        }
    }

    /// Return a cached `rune::Image` handle for `path`, decoding only once per asset path.
    pub fn get_or_load_image(&mut self, path: &Path) -> rune::Image {
        if let Some(img) = self.decoded_images.get(path) {
            return img.clone();
        }
        let img = self.image_cache.file(path, 1024.0, 2.0);
        self.decoded_images.insert(path.to_path_buf(), img.clone());
        img
    }

    fn scan_projects_dir(projects_dir: &Path) -> Vec<ProjectMetadata> {
        let mut projects = Vec::new();
        if let Ok(entries) = std::fs::read_dir(projects_dir) {
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
                        projects.push(ProjectMetadata {
                            folder_name,
                            title: g.title,
                            path,
                            collection: g.collection,
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
        projects.sort_by(|a, b| {
            b.modified_secs
                .cmp(&a.modified_secs)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        projects
    }

    /// Scan `projects_dir` synchronously for existing `.studynodes` folders.
    pub fn refresh_recent_projects(&mut self) {
        self.recent_projects = Self::scan_projects_dir(&self.projects_dir);
    }

    /// Scan `projects_dir` in a background thread so launch never blocks the UI thread.
    pub fn refresh_recent_projects_async(&self) {
        let dir = self.projects_dir.clone();
        let tx = self.io_tx.clone();
        let waker_slot = self.io_waker_slot.clone();
        std::thread::spawn(move || {
            let projects = Self::scan_projects_dir(&dir);
            let _ = tx.send(BackgroundIoResult::RecentProjectsLoaded(projects));
            if let Ok(guard) = waker_slot.lock() {
                if let Some(w) = guard.as_ref() {
                    (w)();
                }
            }
        });
    }

    pub fn rebuild_asset_path_cache(&mut self) {
        self.asset_path_cache.clear();
        let Some(bundle) = &self.bundle else {
            return;
        };
        let assets_dir = bundle.root_dir.join(ProjectBundle::ASSETS_DIR);
        if let Ok(entries) = std::fs::read_dir(assets_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        self.asset_path_cache.insert(stem.to_string(), path);
                    }
                }
            }
        }
    }

    pub fn doc_preview_pages_for(
        &self,
        node: &studynodes_graph::model::Node,
        max_lines: usize,
    ) -> Vec<doc_reader::TextPreviewPage> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        max_lines.hash(&mut hasher);
        node.content.hash(&mut hasher);
        let key_hash = hasher.finish();

        if let Some((cached_hash, pages)) = self.doc_preview_cache.borrow().get(&node.id) {
            if *cached_hash == key_hash {
                return pages.clone();
            }
        }
        let pages = doc_reader::split_document_preview_pages(&node.content, max_lines);
        self.doc_preview_cache
            .borrow_mut()
            .insert(node.id.clone(), (key_hash, pages.clone()));
        pages
    }

    fn schedule_graph_save(&mut self) {
        if self.bundle.is_some() {
            self.pending_graph_save_at = Some(Instant::now() + Duration::from_millis(350));
        }
    }

    fn flush_graph_save_async(&mut self) {
        self.pending_graph_save_at = None;
        if let Some(bundle) = &self.bundle {
            let _ = self.save_tx.send((bundle.clone(), self.graph.clone()));
        }
    }

    pub fn flush_graph_save_now(&mut self) {
        self.pending_graph_save_at = None;
        if let Some(bundle) = &self.bundle {
            let _ = bundle.save_graph_atomic(&self.graph);
        }
    }

    /// Snapshot live edits & graph back into the active tab slot and save atomically.
    pub fn save_active_into_tab(&mut self) {
        self.sync_node_edits();
        self.sync_edge_edits();
        self.sync_ai_chat_to_graph();
        self.flush_graph_save_now();
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

    pub fn stop_all_media_players(&mut self) {
        for (_, (_, player)) in self.media_players.drain() {
            player.pause();
        }
        self.embed_rects.clear();
        self.audio_subtitle_pending.clear();
        self.audio_subtitle_errors.clear();
        self.image_search_pending.clear();
    }

    fn load_tab_into_active(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        self.stop_all_media_players();
        self.selected_calendar_date.clear();
        self.cancel_smooth_zoom();
        let tab = &self.tabs[idx];
        self.active_tab = Some(idx);
        self.bundle = Some(tab.bundle.clone());
        self.graph = tab.graph.clone();
        self.pending_graph_save_at = None;
        self.doc_preview_cache.borrow_mut().clear();
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
        self.flush_live_camera();
        self.rebuild_asset_path_cache();
        self.show_block_menu = false;
        self.flipped_flashcards.clear();
        self.popped_out_node_id = None;
        self.active_pdf = None;
        self.active_image = None;
        self.card_pdf_previews.clear();
        self.card_image_previews.clear();
        self.load_ai_chat_from_graph();

        self.canvas_title_edit = TextEdit::new("");
        self.canvas_content_edit = TextEdit::new("");
        self.inspector_title_edit = TextEdit::new("");
        self.inspector_content_edit = TextEdit::new("");
        self.inspector_answer_edit = TextEdit::new("");
        self.edit_edge_label = TextEdit::new("");
        self.edit_edge_note = TextEdit::new("");

        if let Some(nid) = self.selected_nodes.iter().next().cloned() {
            let mut init_media = false;
            let mut init_cal = None;
            let mut day_note_init = String::new();
            if let Some(n) = self.graph.get_node(&nid) {
                self.canvas_title_edit = TextEdit::new(&n.title);
                self.canvas_content_edit = TextEdit::new(&n.content);
                self.inspector_title_edit = TextEdit::new(&n.title);
                self.inspector_content_edit = TextEdit::new(&n.content);
                self.inspector_answer_edit = TextEdit::new(n.meta.answer.as_deref().unwrap_or(""));
                self.media_url_edit = TextEdit::new(n.meta.media_url.as_deref().unwrap_or(""));
                if matches!(n.kind, NodeKind::Document | NodeKind::Image) {
                    self.ensure_reader_media_for_node(&nid, false);
                } else if matches!(n.kind, NodeKind::Video | NodeKind::Audio) {
                    init_media = true;
                } else if n.kind == NodeKind::Calendar {
                    let (y, m, d) = media_helper::today_ymd();
                    let cy = n.meta.calendar_year.unwrap_or(y);
                    let cm = n.meta.calendar_month.unwrap_or(m);
                    let day = if cy == y && cm == m { d } else { 1 };
                    let fallback = media_helper::format_ymd(cy, cm, day);
                    let chosen = self
                        .selected_calendar_date
                        .get(&nid)
                        .cloned()
                        .unwrap_or(fallback);
                    day_note_init = n
                        .meta
                        .calendar_day_notes
                        .get(&chosen)
                        .cloned()
                        .unwrap_or_default();
                    init_cal = Some(chosen);
                }
            }
            if init_media {
                let _ = self.ensure_media_player(&nid);
            }
            if let Some(date) = init_cal {
                self.selected_calendar_date.insert(nid, date.clone());
                self.cal_event_date_edit = TextEdit::new(&date);
                self.cal_day_note_edit = TextEdit::new(&day_note_init);
            } else {
                self.cal_day_note_edit = TextEdit::new("");
            }
        } else if let Some(eid) = self.selected_edge.clone() {
            if let Some(e) = self.graph.get_edge(&eid) {
                self.edit_edge_label = TextEdit::new(&e.label);
                self.edit_edge_note = TextEdit::new(&e.note);
            }
            self.cal_day_note_edit = TextEdit::new("");
        } else {
            self.cal_day_note_edit = TextEdit::new("");
        }

        self.ensure_all_media_players();
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
            Ok((bundle, mut graph)) => {
                if let Some(col) = self
                    .active_collection_filter
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    graph.collection = Some(col.to_string());
                    let _ = bundle.save_graph_atomic(&graph);
                }
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
                self.active_quiz_overlay = None;
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
            self.active_quiz_overlay = None;
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
                self.active_quiz_overlay = None;
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
        if self.is_quiz_hard_locked() || idx >= self.tabs.len() {
            return;
        }
        if self.active_tab == Some(idx) {
            self.screen = AppScreen::Workspace;
            return;
        }
        self.save_active_into_tab();
        self.load_tab_into_active(idx);
        self.active_quiz_overlay = None;
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
            self.flush_graph_save_now();
        }
        let closed_title = self.tabs[idx].title.clone();
        self.tabs.remove(idx);

        if self.tabs.is_empty() {
            self.stop_all_media_players();
            self.active_tab = None;
            self.bundle = None;
            self.cancel_smooth_zoom();
            self.graph = StudyGraph::new("Untitled Project");
            self.flush_live_camera();
            self.asset_path_cache.clear();
            self.doc_preview_cache.borrow_mut().clear();
            self.selected_nodes.clear();
            self.selected_edge = None;
            self.editing_node_id = None;
            self.inspector_open = false;
            self.ai_panel_open = false;
            self.active_pdf = None;
            self.active_image = None;
            self.card_pdf_previews.clear();
            self.card_image_previews.clear();
            self.ai_chat_messages.clear();
            self.ai_chat_input = TextEdit::new("");
            self.ai_chat_pending = false;
            self.active_quiz_overlay = None;
            self.screen = AppScreen::Dashboard;
            crate::touchpad_win32::set_canvas_bounds(None);
            self.refresh_recent_projects();
            self.push_reader_snapshot();
        } else if is_active {
            self.active_quiz_overlay = None;
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
                self.stop_all_media_players();
                self.active_tab = None;
                self.bundle = None;
                self.cancel_smooth_zoom();
                self.graph = StudyGraph::new("Untitled Project");
                self.flush_live_camera();
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
        if self.is_quiz_hard_locked() {
            return;
        }
        self.save_active_into_tab();
        self.cancel_smooth_zoom();
        self.flush_live_camera();
        self.stop_all_media_players();
        self.confirm_delete_project = None;
        self.show_block_menu = false;
        self.show_ai_menu = false;
        self.inspector_more_menu_open = false;
        self.active_quiz_overlay = None;
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
                if changed && node.kind == NodeKind::Text && node.meta.text_auto_fit {
                    node.size = compute_text_node_fit_size(node);
                }
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
                    self.schedule_graph_save();
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
                if changed && node.kind == NodeKind::Text && node.meta.text_auto_fit {
                    node.size = compute_text_node_fit_size(node);
                }
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
                    self.schedule_graph_save();
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

    /// Resolve a node's stored asset file on disk using the in-memory asset cache.
    pub fn resolved_node_asset_path(&self, node_id: &NodeId) -> Option<PathBuf> {
        let hash = self.graph.get_node(node_id)?.asset_hash.as_deref()?;
        self.asset_path_cache.get(hash).cloned()
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

    fn pdf_render_scale(zoom: f32) -> (f32, u32) {
        let clamped_zoom = zoom.clamp(0.6, 2.5);
        let render_scale = (clamped_zoom * 1.6).clamp(0.85, 3.0);
        let zoom_bucket = (clamped_zoom * 100.0).round() as u32;
        (render_scale, zoom_bucket)
    }

    fn touch_pdf_lru(&mut self, key: &(PathBuf, usize, u32)) {
        if let Some(pos) = self.pdf_lru_order.iter().position(|k| k == key) {
            self.pdf_lru_order.remove(pos);
        }
        self.pdf_lru_order.push_front(key.clone());
    }

    fn cache_pdf_page(
        &mut self,
        key: (PathBuf, usize, u32),
        w: u32,
        h: u32,
        pixels: Pixels,
    ) {
        self.pdf_page_cache.insert(key.clone(), (w, h, pixels));
        self.touch_pdf_lru(&key);
        while self.pdf_lru_order.len() > 24 {
            if let Some(evicted) = self.pdf_lru_order.pop_back() {
                self.pdf_page_cache.remove(&evicted);
            }
        }
    }

    fn enqueue_pdf_render(
        &mut self,
        node_id: &NodeId,
        asset_path: &PathBuf,
        page_index: usize,
        render_scale: f32,
        zoom_bucket: u32,
        for_card_thumbnail: bool,
    ) {
        let key = (asset_path.clone(), page_index, zoom_bucket);
        if self.pdf_inflight.contains(&key) {
            return;
        }
        self.pdf_inflight.insert(key);
        let _ = self.pdf_tx.send(PdfWorkerRequest {
            node_id: node_id.clone(),
            asset_path: asset_path.clone(),
            page_index,
            render_scale,
            zoom_bucket,
            for_card_thumbnail,
        });
    }

    fn request_pdf_pages_for_active(&mut self) {
        let Some((node_id, asset_path, page_index, page_count, zoom, has_dims)) =
            self.active_pdf.as_ref().map(|p| {
                (
                    p.node_id.clone(),
                    p.asset_path.clone(),
                    p.page_index,
                    p.page_count.max(1),
                    p.zoom,
                    !p.page_dimensions.is_empty(),
                )
            })
        else {
            return;
        };

        let (render_scale, zoom_bucket) = Self::pdf_render_scale(zoom);
        let last = page_count.saturating_sub(1);
        let p = page_index.min(last);
        let mut targets: Vec<usize> = Vec::new();
        let mut push_unique = |idx: usize| {
            if idx <= last && !targets.contains(&idx) {
                targets.push(idx);
            }
        };

        match self.reader_prefs.pdf_layout_mode {
            PdfLayoutMode::Continuous => {
                push_unique(p);
                push_unique(p + 1);
                push_unique(p.saturating_sub(1));
                if has_dims {
                    for offset in 2..=6 {
                        push_unique(p + offset);
                    }
                    push_unique(p.saturating_sub(2));
                    if page_count <= 12 {
                        for idx in 0..page_count {
                            push_unique(idx);
                        }
                    }
                }
            }
            PdfLayoutMode::TwoPage => {
                let pair = (p / 2) * 2;
                push_unique(pair);
                push_unique(pair + 1);
                if has_dims {
                    push_unique(pair + 2);
                    push_unique(pair + 3);
                    push_unique(pair.saturating_sub(2));
                    push_unique(pair.saturating_sub(1));
                }
            }
            PdfLayoutMode::SinglePage => {
                push_unique(p);
                if has_dims {
                    push_unique(p + 1);
                    push_unique(p.saturating_sub(1));
                }
            }
        }

        for pg in targets {
            let key = (asset_path.clone(), pg, zoom_bucket);
            if let Some((w, h, px)) = self.pdf_page_cache.get(&key).cloned() {
                self.touch_pdf_lru(&key);
                if let Some(pdf) = self.active_pdf.as_mut() {
                    pdf.pages.insert(pg, (w, h, px));
                    pdf.loading_pages.remove(&pg);
                }
            } else {
                if let Some(pdf) = self.active_pdf.as_mut() {
                    pdf.loading_pages.insert(pg);
                }
                self.enqueue_pdf_render(&node_id, &asset_path, pg, render_scale, zoom_bucket, false);
            }
        }
    }

    /// Ensure integrated reader media (async multi-page Hayro PDF or Rune image) is loaded for `node_id`.
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

                let page_dims = self
                    .pdf_dimensions_cache
                    .get(&asset_path)
                    .cloned()
                    .unwrap_or_default();
                let page_count = page_dims.len().max(1);
                let clamped_page = if page_dims.is_empty() {
                    desired_page
                } else {
                    desired_page.min(page_count.saturating_sub(1))
                };

                self.active_pdf = Some(ActivePdfState {
                    node_id: node_id.clone(),
                    asset_path,
                    page_index: clamped_page,
                    page_count,
                    zoom: 1.0,
                    page_dimensions: page_dims,
                    pages: HashMap::new(),
                    loading_pages: HashSet::new(),
                    error: None,
                });
                self.request_pdf_pages_for_active();
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
                    let img = self.get_or_load_image(&asset_path);
                    self.active_image = Some((node_id.clone(), asset_path.clone(), img.clone()));
                    self.card_image_previews.insert(node_id.clone(), img);
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
        let (nid, clamped, total) = {
            let Some(pdf) = self.active_pdf.as_mut() else {
                return;
            };
            let clamped = page_index.min(pdf.page_count.saturating_sub(1));
            pdf.page_index = clamped;
            (pdf.node_id.clone(), clamped, pdf.page_count.max(1))
        };

        self.request_pdf_pages_for_active();
        if let Some(node) = self.graph.get_node_mut(&nid) {
            node.preview_page = clamped;
        }
        self.schedule_graph_save();
        self.ensure_card_preview(&nid);
        self.status_message = i18n::fmt_page_counter(clamped, total);
        self.push_reader_snapshot();
    }

    pub fn set_pdf_zoom(&mut self, next_zoom: f32) {
        {
            let Some(pdf) = self.active_pdf.as_mut() else {
                return;
            };
            pdf.zoom = next_zoom.clamp(0.6, 2.5);
        }
        self.request_pdf_pages_for_active();
        self.push_reader_snapshot();
    }

    pub fn set_pdf_layout_mode(&mut self, mode: PdfLayoutMode) {
        self.reader_prefs.pdf_layout_mode = mode;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.request_pdf_pages_for_active();
        self.push_reader_snapshot();
    }

    /// Ensure miniature card preview media (async PDF thumbnail or Image thumbnail) is cached for `node_id`.
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
                let desired = if let Some(dims) = self.pdf_dimensions_cache.get(&asset_path) {
                    desired.min(dims.len().saturating_sub(1))
                } else {
                    desired
                };
                if self
                    .card_pdf_previews
                    .contains_key(&(node_id.clone(), desired))
                {
                    return;
                }
                let thumb_bucket = 75u32;
                let key = (asset_path.clone(), desired, thumb_bucket);
                if let Some((_, _, px)) = self.pdf_page_cache.get(&key).cloned() {
                    let page_count = self
                        .pdf_dimensions_cache
                        .get(&asset_path)
                        .map(|d| d.len())
                        .unwrap_or(1)
                        .max(1);
                    self.touch_pdf_lru(&key);
                    self.card_pdf_previews.insert(
                        (node_id.clone(), desired),
                        CardPdfPreview {
                            page_count,
                            pixels: px,
                        },
                    );
                    return;
                }
                self.enqueue_pdf_render(node_id, &asset_path, desired, 0.75, thumb_bucket, true);
            }
            DocFormat::Image => {
                if !self.card_image_previews.contains_key(node_id) {
                    let img = self.get_or_load_image(&asset_path);
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
        let asset_opt = self.resolved_node_asset_path(node_id);
        let is_pdf = self.node_doc_format(node_id) == DocFormat::Pdf && asset_opt.is_some();
        if is_pdf {
            self.ensure_card_preview(node_id);
        }

        let Some(node) = self.graph.get_node(node_id).cloned() else {
            return;
        };
        let cur = node.preview_page;
        let total = if is_pdf {
            asset_opt
                .as_ref()
                .and_then(|p| self.pdf_dimensions_cache.get(p))
                .map(|d| d.len())
                .or_else(|| {
                    self.card_pdf_previews
                        .get(&(node_id.clone(), cur))
                        .map(|p| p.page_count)
                })
                .unwrap_or(1)
                .max(1)
        } else {
            self.doc_preview_pages_for(&node, 7).len().max(1)
        };

        let next = (cur as isize + delta).clamp(0, total.saturating_sub(1) as isize) as usize;
        if let Some(n) = self.graph.get_node_mut(node_id) {
            n.preview_page = next;
        }
        if is_pdf {
            self.ensure_card_preview(node_id);
        }
        self.schedule_graph_save();
        if self
            .active_pdf
            .as_ref()
            .is_some_and(|p| &p.node_id == node_id)
        {
            self.set_pdf_page(next);
        }
        self.status_message = i18n::fmt_page_counter(next, total);
    }

    fn build_section_snapshot_data(
        &self,
        n: &studynodes_graph::model::Node,
    ) -> (Vec<SectionChildSnapshot>, Vec<SectionEdgeSnapshot>) {
        let captured = self.capture_section_children(&n.id);
        let mut child_map: HashMap<NodeId, ((f32, f32), (f32, f32))> = HashMap::new();
        let mut children_snaps = Vec::new();
        for (cid, _) in &captured {
            if let Some(cn) = self.graph.get_node(cid) {
                let rel = (cn.pos.x - n.pos.x, cn.pos.y - n.pos.y);
                let sz = node_world_size(cn);
                child_map.insert(cid.clone(), (rel, sz));
                let c_asset = self.resolved_node_asset_path(&cn.id);
                let c_img = c_asset.as_ref().and_then(|p| {
                    self.card_image_previews
                        .get(&cn.id)
                        .cloned()
                        .or_else(|| self.decoded_images.get(p).cloned())
                });
                children_snaps.push(SectionChildSnapshot {
                    id: cn.id.clone(),
                    kind: cn.kind,
                    title: cn.title.clone(),
                    content: cn.content.clone(),
                    rel_pos: rel,
                    size: sz,
                    meta: cn.meta.clone(),
                    flipped: self.flipped_flashcards.contains(&cn.id),
                    selected_date: self.selected_calendar_date.get(&cn.id).cloned(),
                    asset_path: c_asset,
                    image_handle: c_img,
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
                let rel_lbl = i18n::edge_relation_label(&edge.relation);
                let label = if edge.label.is_empty() {
                    format!("[{}]", rel_lbl)
                } else {
                    format!("[{}] {}", rel_lbl, edge.label)
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
    }

    pub fn build_canvas_scheme_snapshot(&self) -> ReaderSnapshot {
        let mut nodes: Vec<_> = self
            .graph
            .nodes
            .values()
            .filter(|n| n.kind != NodeKind::Section)
            .collect();
        if nodes.is_empty() {
            nodes = self.graph.nodes.values().collect();
        }
        nodes.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for n in &nodes {
            let sz = node_world_size(n);
            min_x = min_x.min(n.pos.x);
            min_y = min_y.min(n.pos.y);
            max_x = max_x.max(n.pos.x + sz.0);
            max_y = max_y.max(n.pos.y + sz.1);
        }
        if !min_x.is_finite() {
            min_x = 0.0;
            min_y = 0.0;
            max_x = 640.0;
            max_y = 400.0;
        }
        let origin_x = min_x - 28.0;
        let origin_y = min_y - 28.0;
        let sec_w = (max_x - origin_x + 28.0).clamp(360.0, 1800.0);
        let sec_h = (max_y - origin_y + 28.0).clamp(240.0, 1200.0);

        let mut child_map: HashMap<NodeId, ((f32, f32), (f32, f32))> = HashMap::new();
        let mut children_snaps = Vec::with_capacity(nodes.len());
        for cn in &nodes {
            let rel = (cn.pos.x - origin_x, cn.pos.y - origin_y);
            let sz = node_world_size(cn);
            child_map.insert(cn.id.clone(), (rel, sz));
            let c_asset = self.resolved_node_asset_path(&cn.id);
            let c_img = c_asset.as_ref().and_then(|p| {
                self.card_image_previews
                    .get(&cn.id)
                    .cloned()
                    .or_else(|| self.decoded_images.get(p).cloned())
            });
            children_snaps.push(SectionChildSnapshot {
                id: cn.id.clone(),
                kind: cn.kind,
                title: cn.title.clone(),
                content: cn.content.clone(),
                rel_pos: rel,
                size: sz,
                meta: cn.meta.clone(),
                flipped: self.flipped_flashcards.contains(&cn.id),
                selected_date: self.selected_calendar_date.get(&cn.id).cloned(),
                asset_path: c_asset,
                image_handle: c_img,
            });
        }
        let mut edges_snaps = Vec::new();
        let mut sorted_edges: Vec<_> = self.graph.edges.values().collect();
        sorted_edges.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        for edge in sorted_edges {
            if let (Some((f_rel, f_sz)), Some((t_rel, t_sz))) = (
                child_map.get(&edge.from.node),
                child_map.get(&edge.to.node),
            ) {
                let from_xy = (f_rel.0 + f_sz.0, f_rel.1 + f_sz.1 * 0.5);
                let to_xy = (t_rel.0, t_rel.1 + t_sz.1 * 0.5);
                let is_ghost = edge.status == EdgeStatus::Proposed;
                let rel_lbl = i18n::edge_relation_label(&edge.relation);
                let label = if edge.label.is_empty() {
                    format!("[{}]", rel_lbl)
                } else {
                    format!("[{}] {}", rel_lbl, edge.label)
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
        ReaderSnapshot {
            node_kind: NodeKind::Section,
            title: self.graph.title.clone(),
            project_title: self.graph.title.clone(),
            section_size: (sec_w, sec_h),
            section_children: children_snaps,
            section_edges: edges_snaps,
            ..ReaderSnapshot::default()
        }
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
                let selected_cal_date = self.selected_calendar_date.get(id).cloned();
                let sec_size = node_world_size(n);

                let (sec_children, sec_edges) = if n.kind == NodeKind::Section {
                    self.build_section_snapshot_data(n)
                } else {
                    (Vec::new(), Vec::new())
                };

                let active_image_handle = asset.as_ref().and_then(|p| {
                    self.active_image
                        .as_ref()
                        .filter(|(aid, _, _)| aid == &n.id)
                        .map(|(_, _, img)| img.clone())
                        .or_else(|| self.card_image_previews.get(&n.id).cloned())
                        .or_else(|| self.decoded_images.get(p).cloned())
                });

                (
                    n.id.clone(),
                    n.kind,
                    n.title.clone(),
                    n.content.clone(),
                    n.summary.clone(),
                    n.meta.clone(),
                    flipped,
                    selected_cal_date,
                    sec_size,
                    sec_children,
                    sec_edges,
                    fmt,
                    asset,
                    active_image_handle,
                )
            })
        });

        if let Ok(mut b) = self.reader_bridge.lock() {
            let rev = b.snapshot.revision.saturating_add(1);
            let (
                pdf_page_index,
                pdf_page_count,
                pdf_zoom,
                pdf_page_dimensions,
                pdf_rendered_pages,
                pdf_error,
            ) = match &self.active_pdf {
                Some(p) if target_id.as_ref() == Some(&p.node_id) => (
                    p.page_index,
                    p.page_count.max(1),
                    p.zoom,
                    p.page_dimensions.clone(),
                    p.pages.clone(),
                    p.error.clone(),
                ),
                _ => (0, 1, 1.0, Vec::new(), HashMap::new(), None),
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
                    selected_calendar_date,
                    section_size,
                    section_children,
                    section_edges,
                    doc_format,
                    asset_path,
                    active_image_handle,
                )) => ReaderSnapshot {
                    revision: rev,
                    project_title: self.graph.title.clone(),
                    vision_pending: self.vision_pending_nodes.contains(&id),
                    ai_summarizing: self.llm_pending_nodes.contains(&id),
                    llm_last_error: self.llm_last_error.clone(),
                    node_id: Some(id),
                    node_kind,
                    title,
                    content,
                    summary,
                    meta,
                    flashcard_flipped,
                    selected_calendar_date,
                    section_size,
                    section_children,
                    section_edges,
                    doc_format,
                    asset_path,
                    active_image_handle,
                    doc_view_tab: self.doc_view_tab,
                    pdf_page_index,
                    pdf_page_count,
                    pdf_zoom,
                    pdf_layout_mode: self.reader_prefs.pdf_layout_mode,
                    pdf_page_dimensions,
                    pdf_rendered_pages,
                    pdf_error,
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
                    pdf_layout_mode: self.reader_prefs.pdf_layout_mode,
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
                ReaderCommand::SetPdfLayoutMode(mode) => {
                    self.set_pdf_layout_mode(mode);
                    push_snap = true;
                }
                ReaderCommand::ClearNodeSummary(node_id) => {
                    self.clear_node_summary(&node_id);
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
                ReaderCommand::ZoomGraphFormula {
                    node_id,
                    factor,
                    anchor_norm,
                    plot_size,
                } => {
                    self.zoom_graph_formula(&node_id, factor, anchor_norm, plot_size);
                    push_snap = true;
                }
                ReaderCommand::PanGraphFormula {
                    node_id,
                    start_cx,
                    start_cy,
                    dx,
                    dy,
                    plot_size,
                    commit,
                } => {
                    self.pan_graph_formula(&node_id, start_cx, start_cy, dx, dy, plot_size);
                    if commit {
                        self.finish_pan_graph_formula(&node_id);
                    }
                    push_snap = true;
                }
                ReaderCommand::ResetGraphFormulaView(node_id) => {
                    self.reset_graph_formula_view(&node_id);
                    push_snap = true;
                }
                ReaderCommand::ApplyGraphFormulaKeypad { node_id, action } => {
                    self.apply_graph_formula_keypad(&node_id, action);
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
                ReaderCommand::ClearExtractedText(node_id) => {
                    self.clear_extracted_text(&node_id);
                    push_snap = true;
                }
                ReaderCommand::ClearTranslatedText(node_id) => {
                    self.clear_translated_text(&node_id);
                    push_snap = true;
                }
                ReaderCommand::TriggerSectionAiReview(node_id) => {
                    self.explain_node_ai(&node_id);
                    push_snap = true;
                }
                ReaderCommand::CaptureSectionScreenshot { node_id, path } => {
                    let _ = self.export_scheme_screenshot(node_id.as_ref(), path);
                }
                ReaderCommand::InterpretDrawing { node_id, mode } => {
                    self.interpret_drawing_node(&node_id, mode);
                    push_snap = true;
                }
                ReaderCommand::BeginDrawingStroke {
                    node_id,
                    nx,
                    ny,
                    color_hex,
                    width,
                    eraser,
                } => {
                    self.begin_drawing_stroke_styled(&node_id, nx, ny, &color_hex, width, eraser);
                }
                ReaderCommand::ExtendDrawingStroke {
                    node_id,
                    nx,
                    ny,
                    eraser,
                } => {
                    let fallback_color = self.drawing_color_hex.clone();
                    let fallback_width = self.drawing_stroke_width;
                    self.extend_drawing_stroke_with_mode(
                        &node_id,
                        nx,
                        ny,
                        &fallback_color,
                        fallback_width,
                        eraser,
                    );
                }
                ReaderCommand::FinishDrawingStroke(node_id) => {
                    self.finish_drawing_stroke(&node_id);
                    push_snap = true;
                }
                ReaderCommand::UndoDrawingStroke(node_id) => {
                    self.undo_drawing_stroke(&node_id);
                    push_snap = true;
                }
                ReaderCommand::ClearDrawingStrokes(node_id) => {
                    self.clear_drawing_strokes(&node_id);
                    push_snap = true;
                }
                ReaderCommand::CalendarSelectDate { node_id, date } => {
                    self.calendar_select_date(&node_id, date);
                    push_snap = true;
                }
                ReaderCommand::CalendarStepMonth { node_id, delta } => {
                    self.calendar_step_month(&node_id, delta);
                    push_snap = true;
                }
                ReaderCommand::CalendarStepWeek { node_id, delta } => {
                    self.calendar_step_week(&node_id, delta);
                    push_snap = true;
                }
                ReaderCommand::CalendarSetMode { node_id, mode } => {
                    self.set_calendar_view_mode(&node_id, mode);
                    push_snap = true;
                }
                ReaderCommand::ToggleCalendarEvent { node_id, event_id } => {
                    self.toggle_calendar_event(&node_id, &event_id);
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

        self.cancel_smooth_zoom();
        let fit_zoom = (canvas_w / bbox_w).min(canvas_h / bbox_h).clamp(0.45, 1.0);
        if self.graph.viewport.zoom > 1.35 || self.graph.viewport.zoom < 0.45 || count > 1 {
            self.graph.viewport.zoom = fit_zoom;
        }
        self.graph
            .viewport
            .center_on(bbox_center, canvas_w, canvas_h);
        self.flush_live_camera();

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
        self.flush_live_camera();
        let same_single =
            !additive && self.selected_nodes.len() == 1 && self.selected_nodes.contains(&id);
        let was_selected = self.selected_nodes.contains(&id);
        if same_single {
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
        self.block_menu_anchor = None;
        self.block_menu_spawn_world = None;
        self.show_ai_menu = false;
        self.inspector_more_menu_open = false;
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
            self.media_url_edit = TextEdit::new(n.meta.media_url.as_deref().unwrap_or(""));
            self.media_ts_time_edit = TextEdit::new("");
            self.media_ts_label_edit = TextEdit::new("");
            if !was_selected {
                self.quiz_q_edit = TextEdit::new("");
                self.quiz_opts_edit = TextEdit::new("");
                self.quiz_ans_edit = TextEdit::new("");
                self.quiz_hint_edit = TextEdit::new("");
                self.quiz_correct_idx = 0;
            }
            node_kind = Some(n.kind);
            if matches!(n.kind, NodeKind::Document | NodeKind::Image) {
                if !self.inspector_popped_out {
                    self.inspector_open = true;
                }
                self.ai_panel_open = false;
                self.editing_node_id = None;
            } else if n.kind == NodeKind::GraphFormula {
                if !self.inspector_popped_out {
                    self.inspector_open = true;
                }
                self.ai_panel_open = false;
                self.editing_node_id = Some(id.clone());
            } else if matches!(n.kind, NodeKind::Note | NodeKind::Formula | NodeKind::Text) {
                self.editing_node_id = Some(id.clone());
            } else {
                self.editing_node_id = None;
            }
            if n.kind == NodeKind::Calendar {
                let (y, m, d) = media_helper::today_ymd();
                let cy = n.meta.calendar_year.unwrap_or(y);
                let cm = n.meta.calendar_month.unwrap_or(m);
                let def_day = if cy == y && cm == m { d } else { 1 };
                let fallback_date = media_helper::format_ymd(cy, cm, def_day);
                let selected_date = self
                    .selected_calendar_date
                    .entry(id.clone())
                    .or_insert_with(|| fallback_date)
                    .clone();
                self.cal_event_date_edit = TextEdit::new(&selected_date);
                let day_note = n
                    .meta
                    .calendar_day_notes
                    .get(&selected_date)
                    .cloned()
                    .unwrap_or_default();
                self.cal_day_note_edit = TextEdit::new(&day_note);
            } else {
                self.cal_day_note_edit = TextEdit::new("");
            }
            if self.inspector_popped_out && can_pop_out(n.kind) {
                self.popped_out_node_id = Some(id.clone());
            }
        }

        if matches!(node_kind, Some(NodeKind::Video | NodeKind::Audio)) {
            let _ = self.ensure_media_player(&id);
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
        self.flush_live_camera();
        self.sync_node_edits();
        self.sync_edge_edits();
        self.show_block_menu = false;
        self.block_menu_anchor = None;
        self.block_menu_spawn_world = None;
        self.show_ai_menu = false;
        self.inspector_more_menu_open = false;
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
        self.flush_live_camera();
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
                matches!(n.kind, NodeKind::Formula | NodeKind::GraphFormula),
                n.kind == NodeKind::Formula
                    && (lang.eq_ignore_ascii_case("RUST") || lang.eq_ignore_ascii_case("PY")),
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

    pub fn zoom_graph_formula(
        &mut self,
        id: &NodeId,
        factor: f64,
        anchor_norm: Option<(f32, f32)>,
        plot_size: (f32, f32),
    ) {
        let Some(node) = self.graph.get_node_mut(id) else {
            return;
        };
        if node.kind != NodeKind::GraphFormula {
            return;
        }
        let mut vp = graph_formula::GraphViewport::from_meta(&node.meta);
        let (nx, ny) = anchor_norm.unwrap_or((0.5, 0.5));
        vp.zoom_at(nx, ny, factor, plot_size.0.max(64.0), plot_size.1.max(48.0));
        vp.write_to_meta(&mut node.meta);
        self.persist_graph_and_snapshot();
    }

    pub fn pan_graph_formula(
        &mut self,
        id: &NodeId,
        start_cx: f64,
        start_cy: f64,
        dx: f32,
        dy: f32,
        plot_size: (f32, f32),
    ) {
        let Some(node) = self.graph.get_node_mut(id) else {
            return;
        };
        if node.kind != NodeKind::GraphFormula {
            return;
        }
        let mut vp = graph_formula::GraphViewport::from_meta(&node.meta);
        vp.pan_by_px(
            start_cx,
            start_cy,
            dx,
            dy,
            plot_size.0.max(64.0),
            plot_size.1.max(48.0),
        );
        vp.write_to_meta(&mut node.meta);
    }

    pub fn finish_pan_graph_formula(&mut self, id: &NodeId) {
        if self
            .graph
            .get_node(id)
            .is_some_and(|n| n.kind == NodeKind::GraphFormula)
        {
            self.persist_graph_and_snapshot();
        }
    }

    pub fn reset_graph_formula_view(&mut self, id: &NodeId) {
        let Some(node) = self.graph.get_node_mut(id) else {
            return;
        };
        if node.kind != NodeKind::GraphFormula {
            return;
        }
        graph_formula::GraphViewport::default().write_to_meta(&mut node.meta);
        self.persist_graph_and_snapshot();
        self.status_message = "Reset graph view".into();
    }

    pub fn apply_graph_formula_keypad(
        &mut self,
        id: &NodeId,
        action: graph_formula::KeypadAction,
    ) {
        if action == graph_formula::KeypadAction::ToggleMode {
            self.graph_keypad_mode = match self.graph_keypad_mode {
                graph_formula::KeypadMode::Sci => graph_formula::KeypadMode::Basic,
                graph_formula::KeypadMode::Basic => graph_formula::KeypadMode::Sci,
            };
            return;
        }

        let is_selected = self.selected_nodes.contains(id);
        if !is_selected {
            self.select_node(id.clone(), false);
        }

        if self.inspector_content_edit.focused {
            graph_formula::apply_keypad_to_edit(&mut self.inspector_content_edit, action);
            self.sync_from_inspector_edits();
        } else {
            graph_formula::apply_keypad_to_edit(&mut self.canvas_content_edit, action);
            self.sync_from_canvas_edits();
            // Keep inspector selection aligned with canvas edit without dropping focus
            let (a, b) = self.canvas_content_edit.range();
            self.inspector_content_edit.set_selection(a, b);
        }
        self.persist_graph_and_snapshot();
    }

    pub fn set_graph_formula_preset(&mut self, id: &NodeId, expr: &str) {
        let is_selected = self.selected_nodes.contains(id);
        if let Some(n) = self.graph.get_node_mut(id) {
            if n.kind == NodeKind::GraphFormula {
                n.content = expr.to_string();
                if is_selected {
                    self.canvas_content_edit = TextEdit::new(&n.content);
                    self.inspector_content_edit = TextEdit::new(&n.content);
                }
            }
        }
        self.persist_graph_and_snapshot();
        self.status_message = format!("Plotted {expr}");
    }

    /// Spawn a specialized Phase 2 study node with clean monochrome defaults.
    pub fn add_study_node(&mut self, kind: NodeKind, pos_opt: Option<WorldPos>) -> NodeId {
        self.show_block_menu = false;
        self.block_menu_anchor = None;
        self.block_menu_spawn_world = None;
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

        let title = i18n::default_node_title(kind, step_num);

        let mut node = studynodes_graph::model::Node::new(kind, title, pos);
        node.size = node_base_size(kind);

        match kind {
            NodeKind::Section => {
                node.meta.section_framed = true;
            }
            NodeKind::Excerpt => {
                node.content = i18n::default_node_hint(kind).into();
            }
            NodeKind::Flashcard => {
                node.content = i18n::default_node_hint(kind).into();
                node.meta.answer = Some("Concise recall explanation shown on flip.".into());
                node.meta.mastery = Some("new".into());
            }
            NodeKind::Formula => {
                node.content = "Δw_ij = η · x_i · y_j".into();
                node.meta.language = Some("MATH".into());
            }
            NodeKind::GraphFormula => {
                node.content = "y = sin(x)".into();
                graph_formula::GraphViewport::default().write_to_meta(&mut node.meta);
            }
            NodeKind::Checklist => {
                node.content = i18n::default_node_hint(kind).into();
            }
            NodeKind::Sequence => {
                node.content = i18n::default_node_hint(kind).into();
                node.meta.step_index = Some(step_num);
            }
            NodeKind::Video | NodeKind::Drawing => {
                node.content.clear();
            }
            NodeKind::Audio => {
                node.content = i18n::default_node_hint(kind).into();
            }
            NodeKind::Calendar => {
                let (ty, tm, _td) = media_helper::today_ymd();
                node.meta.calendar_year = Some(ty);
                node.meta.calendar_month = Some(tm);
            }
            NodeKind::Text => {
                node.content.clear();
                node.meta.text_font = Some("sans".into());
                node.meta.text_size = Some(18.0);
            }
            NodeKind::Quiz => {
                node.content.clear();
                node.meta.quiz_questions = vec![QuizQuestion {
                    id: uuid::Uuid::new_v4().to_string(),
                    question: "Which mechanism explains this concept best?".into(),
                    options: vec!["Option A".into(), "Option B".into(), "Option C".into()],
                    correct_index: Some(0),
                    answer: "Option A captures the primary mechanism.".into(),
                    hints: vec!["Review the key definition in your notes.".into()],
                }];
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

    /// Show a transient floating canvas toast for 3 seconds.
    pub fn notify_toast(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        self.status_message = m.clone();
        self.last_toast_status = m.clone();
        self.toast_text = Some(m);
        self.toast_expires_at = Some(Instant::now() + Duration::from_secs(3));
    }

    /// Synchronize transient toast notifications and copied-chip timers on UI poll.
    pub fn sync_toast_and_feedback(&mut self) -> bool {
        let mut changed = false;
        let now = Instant::now();

        if self.status_message != self.last_toast_status {
            self.last_toast_status = self.status_message.clone();
            if self.status_message != "Ready" && !self.status_message.starts_with("Loaded '") {
                self.toast_text = Some(self.status_message.clone());
                self.toast_expires_at = Some(now + Duration::from_secs(3));
                changed = true;
            }
        }

        if let Some(exp) = self.toast_expires_at {
            if now >= exp {
                self.toast_text = None;
                self.toast_expires_at = None;
                changed = true;
            }
        }

        if let Some((_, ts)) = self.copied_feedback {
            if now.duration_since(ts) >= Duration::from_secs(2) {
                self.copied_feedback = None;
                changed = true;
            }
        }

        changed
    }

    /// Record short-lived `"Copied ✓"` feedback and show a transient toast.
    pub fn mark_copied(&mut self, slot: &'static str, label: &str) {
        self.copied_feedback = Some((slot, Instant::now()));
        self.notify_toast(label);
    }

    pub fn is_recently_copied(&self, slot: &'static str) -> bool {
        self.copied_feedback
            .is_some_and(|(s, ts)| s == slot && ts.elapsed() < Duration::from_secs(2))
    }

    /// Discover latent conceptual connections across selected (or top) nodes using AI synthesis.
    pub fn run_ai_synthesize_connections(&mut self) {
        self.sync_node_edits();
        let focus_ids: Vec<_> = self.selected_nodes.iter().cloned().collect();
        let target_ids = if focus_ids.is_empty() {
            self.graph.nodes.keys().take(3).cloned().collect()
        } else {
            focus_ids
        };
        if target_ids.is_empty() {
            self.notify_toast("Add nodes first to synthesize connections");
            return;
        }

        let preferred_lang = self
            .reader_prefs
            .ai_summary_follow_ui_language
            .then(|| self.reader_prefs.ui_locale.english_name().to_string());
        let context =
            studynodes_llm::LlmContext::build(&self.graph, &target_ids, "Discover Connections")
                .with_preferred_language(preferred_lang);
        self.execute_ai_patch_task(context, "Synthesize Connections");
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
            self.notify_toast("Add at least one node first to generate study cards");
            return;
        }

        let preferred_lang = self
            .reader_prefs
            .ai_summary_follow_ui_language
            .then(|| self.reader_prefs.ui_locale.english_name().to_string());
        let context =
            studynodes_llm::LlmContext::build(&self.graph, &focus_ids, "Generate Study Cards")
                .with_preferred_language(preferred_lang);
        self.execute_ai_patch_task(context, "Generate Study Cards");
    }

    fn execute_ai_patch_task(&mut self, context: studynodes_llm::LlmContext, task_label: &str) {
        if let Some((provider, label)) = self.resolve_ai_provider() {
            self.llm_pending_patches += 1;
            self.is_ai_loading = true;
            self.llm_last_error = None;
            self.notify_toast(format!("{} via {}...", task_label, label));
            let tx = self.llm_tx.clone();
            let waker = self.llm_waker.clone();
            let task_owned = task_label.to_string();
            std::thread::spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())
                    .and_then(|rt| {
                        rt.block_on(provider.generate_patch(&context))
                            .map_err(|e| e.to_string())
                    });
                let _ = tx.send(LlmJobResult::PatchFinished {
                    task_label: task_owned,
                    provider_label: label,
                    result,
                });
                if let Some(w) = waker {
                    (w)();
                }
            });
            return;
        }

        let provider = studynodes_llm::MockLlmProvider;
        if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            if let Ok(patch) = rt.block_on(provider.generate_patch(&context)) {
                let res = self.apply_ai_patch_with_undo(&patch);
                let line = res.summary_line();
                if line.is_empty() {
                    self.notify_toast(format!(
                        "AI proposed {} link(s) & {} node(s)",
                        res.proposed_edges.len(),
                        res.new_nodes.len()
                    ));
                } else {
                    self.notify_toast(format!("{}: {}", task_label, line));
                }
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
        self.flush_live_camera();
        self.flipped_flashcards.remove(id);
        self.llm_pending_nodes.remove(id);
        self.vision_pending_nodes.remove(id);
        if let Some((_, player)) = self.media_players.remove(id) {
            player.pause();
        }
        self.embed_rects.remove(id);
        self.embed_collapsed.remove(id);
        self.embed_start_secs.remove(id);
        self.image_search_pending.remove(id);
        self.audio_subtitle_pending.remove(id);
        self.audio_subtitle_errors.remove(id);
        self.selected_calendar_date.remove(id);
        self.quiz_sessions.remove(id);
        if self.active_quiz_overlay.as_ref() == Some(id) {
            self.active_quiz_overlay = None;
        }
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
            self.doc_preview_cache.borrow_mut().remove(id);
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

    /// Generate or refresh a concise AI summary for a single node (disabled for embed media nodes).
    pub fn explain_node_ai(&mut self, id: &NodeId) {
        self.sync_node_edits();
        let Some(node) = self.graph.get_node(id) else {
            return;
        };
        if matches!(node.kind, NodeKind::Video | NodeKind::Audio) {
            self.notify_toast("AI explain is disabled for video/audio embed nodes");
            return;
        }
        let preferred_lang = self
            .reader_prefs
            .ai_summary_follow_ui_language
            .then(|| self.reader_prefs.ui_locale.english_name().to_string());
        let (focus_ids, task_label) = if node.kind == NodeKind::Section {
            let mut ids = vec![id.clone()];
            let mut children = self.capture_section_children(id);
            children.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
            for (cid, _) in children {
                ids.push(cid);
            }
            (
                ids,
                "Review and summarize this section and its grouped study nodes",
            )
        } else {
            (vec![id.clone()], "Summarize Node")
        };
        let context = studynodes_llm::LlmContext::build(&self.graph, &focus_ids, task_label)
            .with_preferred_language(preferred_lang);

        if let Some((provider, label)) = self.resolve_ai_provider() {
            if self.llm_pending_nodes.contains(id) {
                return;
            }
            self.llm_pending_nodes.insert(id.clone());
            self.is_ai_loading = true;
            self.llm_last_error = None;
            self.notify_toast(format!("Summarizing via {}...", label));
            self.push_reader_snapshot();

            let tx = self.llm_tx.clone();
            let waker = self.llm_waker.clone();
            let node_id = id.clone();
            std::thread::spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())
                    .and_then(|rt| {
                        rt.block_on(provider.summarize(&context))
                            .map_err(|e| e.to_string())
                    });
                let _ = tx.send(LlmJobResult::SummaryFinished {
                    node_id,
                    provider_label: label,
                    result,
                });
                if let Some(w) = waker {
                    (w)();
                }
            });
            return;
        }

        let provider = studynodes_llm::MockLlmProvider;
        if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            if let Ok(summary) = rt.block_on(provider.summarize(&context)) {
                if let Some(node) = self.graph.get_node_mut(id) {
                    node.summary = Some(summary);
                }
                self.persist_graph_and_snapshot();
                self.notify_toast(
                    "Offline summary ready (add an API key in Settings for live AI)",
                );
            }
        }
    }

    pub fn summarize_selected_nodes_ai(&mut self) {
        let target = self
            .selected_nodes
            .iter()
            .find(|id| {
                self.graph
                    .get_node(id)
                    .is_some_and(|n| !matches!(n.kind, NodeKind::Video | NodeKind::Audio))
            })
            .cloned();

        if let Some(id) = target {
            self.explain_node_ai(&id);
        } else if self
            .selected_nodes
            .iter()
            .any(|id| {
                self.graph
                    .get_node(id)
                    .is_some_and(|n| matches!(n.kind, NodeKind::Video | NodeKind::Audio))
            })
        {
            self.notify_toast("AI explain is disabled for video/audio embed nodes");
        } else {
            self.notify_toast("Select a node to summarize");
        }
    }

    pub fn clear_node_summary(&mut self, id: &NodeId) {
        if let Some(node) = self.graph.get_node_mut(id) {
            node.summary = None;
        }
        self.llm_last_error = None;
        self.persist_graph_and_snapshot();
        self.notify_toast("Cleared AI summary");
    }

    /// Open the right-hand Inspector panel directly on the Project AI Chat view.
    pub fn open_ai_chat_panel(&mut self) {
        self.sync_node_edits();
        self.sync_edge_edits();
        self.ensure_active_chat_thread();
        self.chat_api_key_edit = TextEdit::new(&self.reader_prefs.chat_api_key);
        self.chat_base_url_edit = TextEdit::new(&self.reader_prefs.chat_base_url);
        self.chat_model_edit = TextEdit::new(&self.reader_prefs.chat_model);
        self.show_ai_menu = false;
        self.show_block_menu = false;
        self.inspector_more_menu_open = false;
        self.ai_panel_open = true;
        if self.inspector_popped_out {
            self.dock_reader_back();
        } else {
            self.inspector_open = true;
        }
    }

    pub fn save_chat_api_settings(&mut self) {
        self.reader_prefs.chat_api_key = self.chat_api_key_edit.text.trim().to_string();
        self.reader_prefs.chat_base_url = self.chat_base_url_edit.text.trim().to_string();
        self.reader_prefs.chat_model = self.chat_model_edit.text.trim().to_string();
        self.llm_last_error = None;
        self.ai_chat_key_drawer_open = false;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.push_reader_snapshot();
        self.notify_toast(crate::i18n::t(crate::i18n::TextKey::SettingsSavedToast));
    }

    fn refresh_chat_pending_flag(&mut self) {
        self.ai_chat_pending = self
            .ai_chat_pending_target
            .as_ref()
            .is_some_and(|(pid, cid)| {
                pid == &self.graph.project_id
                    && self.graph.active_chat_id.as_deref() == Some(cid.as_str())
            });
    }

    pub fn ensure_active_chat_thread(&mut self) -> String {
        if self.graph.chat_threads.is_empty() {
            let id = uuid::Uuid::new_v4().to_string();
            self.graph.chat_threads.push(ChatThread {
                id: id.clone(),
                title: "Chat 1".into(),
                messages: Vec::new(),
            });
            self.graph.active_chat_id = Some(id.clone());
            return id;
        }
        if let Some(active) = self.graph.active_chat_id.as_ref() {
            if self.graph.chat_threads.iter().any(|t| &t.id == active) {
                return active.clone();
            }
        }
        let fallback = self.graph.chat_threads[0].id.clone();
        self.graph.active_chat_id = Some(fallback.clone());
        fallback
    }

    fn load_ai_chat_from_graph(&mut self) {
        let active_id = self.ensure_active_chat_thread();
        let msgs = self
            .graph
            .chat_threads
            .iter()
            .find(|t| t.id == active_id)
            .map(|t| {
                t.messages
                    .iter()
                    .map(|m| AiChatMessage {
                        role: if m.role == "user" {
                            "user"
                        } else {
                            "assistant"
                        },
                        text: m.text.clone(),
                        action_summary: m.action_summary.clone(),
                        can_undo: false,
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.ai_chat_messages = msgs;
        self.ai_chat_input = TextEdit::new("");
        self.llm_last_error = None;
        self.refresh_chat_pending_flag();
    }

    fn sync_ai_chat_to_graph(&mut self) {
        let active_id = self.ensure_active_chat_thread();
        let records: Vec<ChatMessage> = self
            .ai_chat_messages
            .iter()
            .map(|m| ChatMessage {
                role: m.role.to_string(),
                text: m.text.clone(),
                action_summary: m.action_summary.clone(),
            })
            .collect();
        if let Some(thread) = self
            .graph
            .chat_threads
            .iter_mut()
            .find(|t| t.id == active_id)
        {
            thread.messages = records;
            if thread.title.starts_with("Chat ") {
                if let Some(first_user) = thread.messages.iter().find(|m| m.role == "user") {
                    let clean: String = first_user
                        .text
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !clean.is_empty() {
                        let short: String = clean.chars().take(22).collect();
                        thread.title = if clean.chars().count() > 22 {
                            format!("{}…", short.trim_end())
                        } else {
                            short
                        };
                    }
                }
            }
        }
    }

    fn persist_chat_change(&mut self) {
        self.sync_ai_chat_to_graph();
        self.persist_graph_and_snapshot();
        if let Some(tab) = self.active_tab.and_then(|i| self.tabs.get_mut(i)) {
            tab.graph = self.graph.clone();
        }
    }

    pub fn new_ai_chat_tab(&mut self) -> String {
        self.sync_ai_chat_to_graph();
        let mut next_num = self.graph.chat_threads.len() + 1;
        while self
            .graph
            .chat_threads
            .iter()
            .any(|t| t.title == format!("Chat {}", next_num))
        {
            next_num += 1;
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.graph.chat_threads.push(ChatThread {
            id: id.clone(),
            title: format!("Chat {}", next_num),
            messages: Vec::new(),
        });
        self.graph.active_chat_id = Some(id.clone());
        self.load_ai_chat_from_graph();
        self.persist_graph_and_snapshot();
        if let Some(tab) = self.active_tab.and_then(|i| self.tabs.get_mut(i)) {
            tab.graph = self.graph.clone();
        }
        id
    }

    pub fn switch_ai_chat_tab(&mut self, chat_id: &str) {
        if self.graph.active_chat_id.as_deref() == Some(chat_id) {
            return;
        }
        if !self.graph.chat_threads.iter().any(|t| t.id == chat_id) {
            return;
        }
        self.sync_ai_chat_to_graph();
        self.graph.active_chat_id = Some(chat_id.to_string());
        self.load_ai_chat_from_graph();
        self.persist_graph_and_snapshot();
        if let Some(tab) = self.active_tab.and_then(|i| self.tabs.get_mut(i)) {
            tab.graph = self.graph.clone();
        }
    }

    pub fn close_ai_chat_tab(&mut self, chat_id: &str) {
        if self.graph.chat_threads.len() <= 1 {
            if let Some(first) = self.graph.chat_threads.first_mut() {
                first.title = "Chat 1".into();
                first.messages.clear();
            }
            self.ai_chat_messages.clear();
            self.llm_last_error = None;
            self.persist_chat_change();
            return;
        }
        let Some(idx) = self.graph.chat_threads.iter().position(|t| t.id == chat_id) else {
            return;
        };
        let was_active = self.graph.active_chat_id.as_deref() == Some(chat_id);
        self.graph.chat_threads.remove(idx);
        if was_active {
            let next_idx = idx.min(self.graph.chat_threads.len() - 1);
            self.graph.active_chat_id = Some(self.graph.chat_threads[next_idx].id.clone());
            self.load_ai_chat_from_graph();
        }
        self.persist_graph_and_snapshot();
        if let Some(tab) = self.active_tab.and_then(|i| self.tabs.get_mut(i)) {
            tab.graph = self.graph.clone();
        }
    }

    fn append_chat_reply_for_target(
        &mut self,
        project_id: &str,
        chat_id: &str,
        reply: String,
        action_summary: Option<String>,
        can_undo: bool,
    ) {
        if project_id == self.graph.project_id {
            if self.graph.active_chat_id.as_deref() == Some(chat_id) {
                self.ai_chat_messages.push(AiChatMessage {
                    role: "assistant",
                    text: reply,
                    action_summary,
                    can_undo,
                });
                self.persist_chat_change();
            } else if let Some(thread) = self.graph.chat_threads.iter_mut().find(|t| t.id == chat_id) {
                thread.messages.push(ChatMessage {
                    role: "assistant".into(),
                    text: reply,
                    action_summary,
                });
                self.persist_graph_and_snapshot();
                if let Some(tab) = self.active_tab.and_then(|i| self.tabs.get_mut(i)) {
                    tab.graph = self.graph.clone();
                }
            }
            return;
        }
        if let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|t| t.graph.project_id == project_id)
        {
            if let Some(thread) = tab.graph.chat_threads.iter_mut().find(|t| t.id == chat_id) {
                thread.messages.push(ChatMessage {
                    role: "assistant".into(),
                    text: reply,
                    action_summary,
                });
                let _ = tab.bundle.save_graph_atomic(&tab.graph);
            }
        }
    }

    pub fn clear_ai_chat(&mut self) {
        self.ai_chat_messages.clear();
        self.llm_last_error = None;
        if let Some(active_id) = self.graph.active_chat_id.clone() {
            if let Some((idx, thread)) = self
                .graph
                .chat_threads
                .iter_mut()
                .enumerate()
                .find(|(_, t)| t.id == active_id)
            {
                thread.messages.clear();
                thread.title = format!("Chat {}", idx + 1);
            }
        }
        self.persist_chat_change();
    }

    pub(crate) fn sync_edit_buffers_from_selected(&mut self) {
        let Some(nid) = self.selected_nodes.iter().next().cloned() else {
            return;
        };
        if let Some(n) = self.graph.get_node(&nid) {
            self.canvas_title_edit = TextEdit::new(&n.title);
            self.canvas_content_edit = TextEdit::new(&n.content);
            self.inspector_title_edit = TextEdit::new(&n.title);
            self.inspector_content_edit = TextEdit::new(&n.content);
            self.inspector_answer_edit = TextEdit::new(n.meta.answer.as_deref().unwrap_or(""));
            self.media_url_edit = TextEdit::new(n.meta.media_url.as_deref().unwrap_or(""));
            if n.kind == NodeKind::Calendar {
                let (y, m, d) = media_helper::today_ymd();
                let def_date = media_helper::format_ymd(y, m, d);
                let sel_date = self
                    .selected_calendar_date
                    .get(&nid)
                    .cloned()
                    .unwrap_or(def_date);
                let note = n
                    .meta
                    .calendar_day_notes
                    .get(&sel_date)
                    .cloned()
                    .unwrap_or_default();
                self.cal_day_note_edit = TextEdit::new(&note);
            }
        }
    }

    pub fn focus_node_if_offscreen(&mut self, id: &NodeId) {
        let Some(n) = self.graph.get_node(id) else {
            return;
        };
        let (w, h) = node_world_size(n);
        let cw = if self.canvas_size[0] > 120.0 {
            self.canvas_size[0]
        } else {
            920.0
        };
        let ch = if self.canvas_size[1] > 120.0 {
            self.canvas_size[1]
        } else {
            640.0
        };
        let z = self.graph.viewport.zoom.max(0.01);
        let screen_tl = self.graph.viewport.world_to_screen(n.pos);
        let screen_br_x = screen_tl.x + w * z;
        let screen_br_y = screen_tl.y + h * z;
        let visible = screen_br_x > 40.0
            && screen_tl.x < cw - 40.0
            && screen_br_y > 40.0
            && screen_tl.y < ch - 40.0;
        if !visible {
            let cx = n.pos.x + w * 0.5;
            let cy = n.pos.y + h * 0.5;
            self.graph.viewport.offset.x = cx - (cw * 0.5) / z;
            self.graph.viewport.offset.y = cy - (ch * 0.5) / z;
        }
    }

    /// Stage an AI `GraphPatch` atomically onto `self.graph` while recording a Snapshot command on `self.history`.
    pub fn apply_ai_patch_with_undo(
        &mut self,
        patch: &studynodes_graph::patch::GraphPatch,
    ) -> studynodes_graph::patch::StagedPatchResult {
        self.sync_node_edits();
        self.sync_edge_edits();
        let before_nodes = self.graph.nodes.clone();
        let before_edges = self.graph.edges.clone();
        let spawn_origin = self.viewport_spawn_center(-140.0, -90.0);
        let (ty, tm, td) = media_helper::today_ymd();
        let today_str = media_helper::format_ymd(ty, tm, td);
        let res = self.graph.validate_and_stage_patch_full(
            patch,
            spawn_origin,
            Some(today_str.as_str()),
        );
        for del_id in &res.deleted_nodes {
            if let Some((_, player)) = self.media_players.remove(del_id) {
                player.pause();
            }
            self.embed_rects.remove(del_id);
            self.embed_collapsed.remove(del_id);
            self.embed_start_secs.remove(del_id);
            self.image_search_pending.remove(del_id);
            self.audio_subtitle_pending.remove(del_id);
            self.audio_subtitle_errors.remove(del_id);
            self.selected_nodes.remove(del_id);
            self.selected_calendar_date.remove(del_id);
            self.quiz_sessions.remove(del_id);
            if self.active_quiz_overlay.as_ref() == Some(del_id) {
                self.active_quiz_overlay = None;
            }
            self.doc_preview_cache.borrow_mut().remove(del_id);
            if self.editing_node_id.as_ref() == Some(del_id) {
                self.editing_node_id = None;
            }
        }
        let mut focus_target: Option<NodeId> = res.new_nodes.first().map(|n| n.id.clone());
        let mut touched_ids: Vec<NodeId> = res.new_nodes.iter().map(|n| n.id.clone()).collect();
        for (nid, _) in &res.updated_nodes {
            if focus_target.is_none() {
                focus_target = Some(nid.clone());
            }
            touched_ids.push(nid.clone());
        }
        for nid in &res.moved_nodes {
            if focus_target.is_none() {
                focus_target = Some(nid.clone());
            }
            if !touched_ids.contains(nid) {
                touched_ids.push(nid.clone());
            }
        }
        if let Some((cal_id, cal_date)) = res.last_calendar_target.clone() {
            self.calendar_select_date(&cal_id, cal_date);
            if focus_target.is_none() {
                focus_target = Some(cal_id.clone());
            }
            touched_ids.push(cal_id);
        } else if res.calendar_notes_changed > 0 || res.calendar_events_changed > 0 {
            if let Some(cal_id) = self
                .graph
                .nodes
                .values()
                .find(|n| n.kind == NodeKind::Calendar)
                .map(|n| n.id.clone())
            {
                if focus_target.is_none() {
                    focus_target = Some(cal_id.clone());
                }
                touched_ids.push(cal_id);
            }
        }
        for nid in &touched_ids {
            self.doc_preview_cache.borrow_mut().remove(nid);
            let _ = self.ensure_media_player(nid);
        }
        if !res.pending_image_searches.is_empty() && !rune::is_headless() {
            self.spawn_pending_image_searches(&res.pending_image_searches);
        }
        if res.total_changes() > 0 {
            let after_nodes = self.graph.nodes.clone();
            let after_edges = self.graph.edges.clone();
            self.history.push_applied(GraphCommand::Snapshot {
                before_nodes,
                before_edges,
                after_nodes,
                after_edges,
            });
            self.sync_edit_buffers_from_selected();
            if let Some(target_id) = focus_target {
                self.focus_node_if_offscreen(&target_id);
            }
            self.persist_graph_and_snapshot();
            self.save_active_into_tab();
            self.refresh_card_previews();
        }
        res
    }

    pub fn undo_graph(&mut self) -> bool {
        if self.history.undo(&mut self.graph) {
            self.selected_nodes
                .retain(|nid| self.graph.nodes.contains_key(nid));
            if let Some(ref eid) = self.selected_edge {
                if !self.graph.edges.contains_key(eid) {
                    self.selected_edge = None;
                }
            }
            if let Some(ref edit_id) = self.editing_node_id {
                if !self.graph.nodes.contains_key(edit_id) {
                    self.editing_node_id = None;
                }
            }
            self.media_players.retain(|nid, (_, player)| {
                let keep = self.graph.nodes.contains_key(nid);
                if !keep {
                    player.pause();
                }
                keep
            });
            if let Some(ref oid) = self.active_quiz_overlay {
                if !self.graph.get_node(oid).is_some_and(|n| {
                    n.kind == NodeKind::Quiz && n.meta.quiz_mode == QuizDisplayMode::Popout
                }) {
                    self.active_quiz_overlay = None;
                }
            }
            self.doc_preview_cache.borrow_mut().clear();
            self.sync_edit_buffers_from_selected();
            self.ensure_all_media_players();
            self.persist_graph_and_snapshot();
            self.save_active_into_tab();
            self.notify_toast("↶ Undid graph change");
            true
        } else {
            false
        }
    }

    pub fn redo_graph(&mut self) -> bool {
        if self.history.redo(&mut self.graph) {
            self.selected_nodes
                .retain(|nid| self.graph.nodes.contains_key(nid));
            if let Some(ref eid) = self.selected_edge {
                if !self.graph.edges.contains_key(eid) {
                    self.selected_edge = None;
                }
            }
            if let Some(ref oid) = self.active_quiz_overlay {
                if !self.graph.get_node(oid).is_some_and(|n| {
                    n.kind == NodeKind::Quiz && n.meta.quiz_mode == QuizDisplayMode::Popout
                }) {
                    self.active_quiz_overlay = None;
                }
            }
            self.doc_preview_cache.borrow_mut().clear();
            self.sync_edit_buffers_from_selected();
            self.ensure_all_media_players();
            self.persist_graph_and_snapshot();
            self.save_active_into_tab();
            self.notify_toast("↷ Redid graph change");
            true
        } else {
            false
        }
    }

    pub fn undo_ai_chat_action(&mut self, msg_index: usize) {
        if self.undo_graph() {
            if let Some(msg) = self.ai_chat_messages.get_mut(msg_index) {
                msg.can_undo = false;
            }
        }
    }

    /// Ask a free-form question grounded in the active project and selected nodes (and apply direct canvas actions when toggled on).
    pub fn submit_ai_chat_question(&mut self) {
        self.sync_node_edits();
        let q = self.ai_chat_input.text.trim().to_string();
        if q.is_empty() || self.ai_chat_pending {
            return;
        }
        self.ai_chat_input = TextEdit::new("");
        let target_project_id = self.graph.project_id.clone();
        let target_chat_id = self.ensure_active_chat_thread();

        let history: Vec<(String, String)> = self
            .ai_chat_messages
            .iter()
            .map(|m| (m.role.to_string(), m.text.clone()))
            .collect();
        self.ai_chat_messages.push(AiChatMessage {
            role: "user",
            text: q.clone(),
            action_summary: None,
            can_undo: false,
        });
        self.persist_chat_change();

        let selected_ids: Vec<NodeId> = self.selected_nodes.iter().cloned().collect();
        let preferred_lang = self
            .reader_prefs
            .ai_summary_follow_ui_language
            .then(|| self.reader_prefs.ui_locale.english_name().to_string());
        let (ty, tm, td) = media_helper::today_ymd();
        let today_str = media_helper::format_ymd(ty, tm, td);
        let context = studynodes_llm::LlmContext::build_project_context(
            &self.graph,
            &selected_ids,
            "Project Chat",
        )
        .with_preferred_language(preferred_lang)
        .with_current_date(Some(today_str));

        let with_actions = self.ai_chat_canvas_actions;

        if let Some((provider, label)) = self.resolve_chat_ai_provider() {
            self.ai_chat_pending_target = Some((target_project_id.clone(), target_chat_id.clone()));
            self.refresh_chat_pending_flag();
            self.is_ai_loading = true;
            self.llm_last_error = None;
            let tx = self.llm_tx.clone();
            let waker = self.llm_waker.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                if with_actions {
                    let result = rt.map_err(|e| e.to_string()).and_then(|rt| {
                        rt.block_on(provider.ask_project_chat_with_actions(&context, &history, &q))
                            .map_err(|e| e.to_string())
                    });
                    let _ = tx.send(LlmJobResult::ChatWithActionsFinished {
                        project_id: target_project_id,
                        chat_id: target_chat_id,
                        provider_label: label,
                        result,
                    });
                } else {
                    let result = rt.map_err(|e| e.to_string()).and_then(|rt| {
                        rt.block_on(provider.ask_project_chat(&context, &history, &q))
                            .map_err(|e| e.to_string())
                    });
                    let _ = tx.send(LlmJobResult::ChatFinished {
                        project_id: target_project_id,
                        chat_id: target_chat_id,
                        provider_label: label,
                        result,
                    });
                }
                if let Some(w) = waker {
                    (w)();
                }
            });
            return;
        }

        let provider = studynodes_llm::MockLlmProvider;
        if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            if with_actions {
                if let Ok(resp) = rt.block_on(provider.ask_project_chat_with_actions(&context, &history, &q)) {
                    let (action_summary, can_undo) = if let Some(patch) = &resp.patch {
                        let res = self.apply_ai_patch_with_undo(patch);
                        if res.total_changes() > 0 {
                            let line = res.summary_line();
                            self.notify_toast(format!("Canvas updated: {}", line));
                            (Some(line), true)
                        } else if res.dropped_ops_count > 0 {
                            (
                                Some(format!(
                                    "Could not apply {} requested action(s) (target node not found)",
                                    res.dropped_ops_count
                                )),
                                false,
                            )
                        } else {
                            (resp.parse_warning.clone(), false)
                        }
                    } else {
                        (resp.parse_warning.clone(), false)
                    };
                    self.ai_chat_messages.push(AiChatMessage {
                        role: "assistant",
                        text: resp.reply,
                        action_summary,
                        can_undo,
                    });
                    self.persist_chat_change();
                }
            } else if let Ok(reply) = rt.block_on(provider.ask_project_chat(&context, &history, &q)) {
                self.ai_chat_messages.push(AiChatMessage {
                    role: "assistant",
                    text: reply,
                    action_summary: None,
                    can_undo: false,
                });
                self.persist_chat_change();
            }
        }
    }

    /// Spawn a new canvas `Note` card from an assistant message in Project AI Chat.
    pub fn spawn_note_from_ai_chat(&mut self, msg_index: usize) -> Option<NodeId> {
        let reply = self
            .ai_chat_messages
            .get(msg_index)
            .filter(|m| m.role == "assistant")
            .map(|m| m.text.trim().to_string())
            .filter(|s| !s.is_empty())?;

        // Find the preceding user question to form a clean note title
        let question_snippet = (0..msg_index)
            .rev()
            .find_map(|i| {
                self.ai_chat_messages
                    .get(i)
                    .filter(|m| m.role == "user")
                    .map(|m| m.text.trim())
            })
            .unwrap_or("Project AI Note");
        let title_chars: String = question_snippet.chars().take(42).collect();
        let title = if question_snippet.chars().count() > 42 {
            format!("{}…", title_chars.trim_end())
        } else {
            title_chars
        };

        let first_selected = self
            .selected_nodes
            .iter()
            .find(|id| {
                self.graph
                    .get_node(id)
                    .is_some_and(|n| n.kind != NodeKind::Section)
            })
            .cloned();

        let new_id = if let Some(src_id) = first_selected {
            self.spawn_derived_card(&src_id, NodeKind::Note, title, reply, "AI Chat")
        } else {
            let center_screen = ScreenPos::new(
                self.canvas_size[0] * 0.45,
                self.canvas_size[1] * 0.45,
            );
            let center_world = self.graph.viewport.screen_to_world(center_screen);
            let offset = (self.graph.nodes.len() % 5) as f32 * 28.0;
            let pos = WorldPos::new(center_world.x - 130.0 + offset, center_world.y - 80.0 + offset);
            let mut node = studynodes_graph::model::Node::new(NodeKind::Note, title, pos);
            node.size = node_base_size(NodeKind::Note);
            node.content = reply;
            let id = node.id.clone();
            self.graph.add_node(node);
            self.persist_graph_and_snapshot();
            Some(id)
        };

        if new_id.is_some() {
            self.notify_toast("Added AI answer as a Note on canvas");
        }
        new_id
    }

    /// Poll completed background AI summary / patch / chat jobs on the UI loop.
    pub fn poll_llm_jobs(&mut self) -> bool {
        let mut changed = false;
        while let Ok(job) = self.llm_rx.try_recv() {
            changed = true;
            match job {
                LlmJobResult::SummaryFinished {
                    node_id,
                    provider_label,
                    result,
                } => {
                    self.llm_pending_nodes.remove(&node_id);
                    match result {
                        Ok(summary) => {
                            self.llm_last_error = None;
                            if let Some(node) = self.graph.get_node_mut(&node_id) {
                                node.summary = Some(summary);
                            }
                            self.persist_graph_and_snapshot();
                            let title = self
                                .graph
                                .get_node(&node_id)
                                .map(|n| n.title.clone())
                                .unwrap_or_else(|| "node".into());
                            self.notify_toast(format!(
                                "Summary ready for '{}' via {}",
                                title, provider_label
                            ));
                        }
                        Err(err) => {
                            self.llm_last_error = Some(err.clone());
                            self.notify_toast(format!("AI summary error: {}", err));
                            self.push_reader_snapshot();
                        }
                    }
                }
                LlmJobResult::PatchFinished {
                    task_label,
                    provider_label,
                    result,
                } => {
                    self.llm_pending_patches = self.llm_pending_patches.saturating_sub(1);
                    match result {
                        Ok(patch) => {
                            self.llm_last_error = None;
                            let res = self.apply_ai_patch_with_undo(&patch);
                            let line = res.summary_line();
                            if line.is_empty() {
                                self.notify_toast(format!(
                                    "{} ({}): {} node(s), {} link(s)",
                                    task_label,
                                    provider_label,
                                    res.new_nodes.len(),
                                    res.proposed_edges.len()
                                ));
                            } else {
                                self.notify_toast(format!(
                                    "{} ({}): {}",
                                    task_label, provider_label, line
                                ));
                            }
                        }
                        Err(err) => {
                            self.llm_last_error = Some(err.clone());
                            self.notify_toast(format!("AI {} error: {}", task_label, err));
                        }
                    }
                }
                LlmJobResult::ChatFinished {
                    project_id,
                    chat_id,
                    provider_label,
                    result,
                } => {
                    if self.ai_chat_pending_target.as_ref().is_some_and(|(p, c)| p == &project_id && c == &chat_id) {
                        self.ai_chat_pending_target = None;
                    }
                    self.refresh_chat_pending_flag();
                    match result {
                        Ok(reply) => {
                            self.llm_last_error = None;
                            self.append_chat_reply_for_target(&project_id, &chat_id, reply, None, false);
                            self.status_message = format!("AI answered via {}", provider_label);
                        }
                        Err(err) => {
                            self.llm_last_error = Some(err.clone());
                            self.notify_toast(format!("AI chat error: {}", err));
                        }
                    }
                }
                LlmJobResult::ChatWithActionsFinished {
                    project_id,
                    chat_id,
                    provider_label,
                    result,
                } => {
                    if self.ai_chat_pending_target.as_ref().is_some_and(|(p, c)| p == &project_id && c == &chat_id) {
                        self.ai_chat_pending_target = None;
                    }
                    self.refresh_chat_pending_flag();
                    match result {
                        Ok(resp) => {
                            self.llm_last_error = None;
                            let (action_summary, can_undo) = if project_id == self.graph.project_id {
                                if let Some(patch) = &resp.patch {
                                    let res = self.apply_ai_patch_with_undo(patch);
                                    if res.total_changes() > 0 {
                                        let mut line = res.summary_line();
                                        if res.dropped_ops_count > 0 {
                                            line.push_str(&format!(
                                                " ({} skipped)",
                                                res.dropped_ops_count
                                            ));
                                        }
                                        self.notify_toast(format!(
                                            "{} ({}): {}",
                                            provider_label, "Canvas", line
                                        ));
                                        (Some(line), true)
                                    } else if res.dropped_ops_count > 0 {
                                        let msg = format!(
                                            "Skipped {} action(s): target node ID/title did not match canvas",
                                            res.dropped_ops_count
                                        );
                                        self.notify_toast(msg.clone());
                                        (Some(msg), false)
                                    } else {
                                        (resp.parse_warning.clone(), false)
                                    }
                                } else {
                                    if let Some(ref w) = resp.parse_warning {
                                        self.notify_toast(format!("AI action parse issue: {}", w));
                                    }
                                    (resp.parse_warning.clone(), false)
                                }
                            } else {
                                (None, false)
                            };
                            self.append_chat_reply_for_target(
                                &project_id,
                                &chat_id,
                                resp.reply,
                                action_summary,
                                can_undo,
                            );
                            self.status_message = format!("AI answered via {}", provider_label);
                        }
                        Err(err) => {
                            self.llm_last_error = Some(err.clone());
                            self.notify_toast(format!("AI chat error: {}", err));
                        }
                    }
                }
            }
        }
        if changed {
            self.is_ai_loading = !self.llm_pending_nodes.is_empty()
                || self.llm_pending_patches > 0
                || self.ai_chat_pending;
        }
        changed
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
        self.flush_live_camera();
        self.sync_node_edits();
        self.sync_edge_edits();
        self.show_block_menu = false;
        self.show_ai_menu = false;
        self.inspector_more_menu_open = false;
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
        let mut node = studynodes_graph::model::Node::new(
            NodeKind::Note,
            i18n::default_node_title(NodeKind::Note, 1),
            pos,
        );
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
        let mut node = studynodes_graph::model::Node::new(
            NodeKind::Document,
            i18n::default_node_title(NodeKind::Document, 1),
            pos,
        );
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
            bundle.ingest_asset(path).ok().map(|(h, dest_path)| {
                let _ = bundle.save_extracted_text(&h, &extracted.markdown);
                self.asset_path_cache.insert(h.clone(), dest_path);
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
        self.doc_preview_cache.borrow_mut().remove(node_id);

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
            if let Ok((hash, dest_path)) = bundle.ingest_asset(path) {
                self.asset_path_cache.insert(hash.clone(), dest_path);
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

                let kind = if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if media_helper::is_video_ext(ext) {
                        NodeKind::Video
                    } else if media_helper::is_supported_media_ext(ext) {
                        NodeKind::Audio
                    } else {
                        match extracted.format {
                            DocFormat::Image => NodeKind::Image,
                            DocFormat::PlainText => NodeKind::Note,
                            _ => NodeKind::Document,
                        }
                    }
                } else {
                    match extracted.format {
                        DocFormat::Image => NodeKind::Image,
                        DocFormat::PlainText => NodeKind::Note,
                        _ => NodeKind::Document,
                    }
                };

                let mut node = studynodes_graph::model::Node::new(
                    kind,
                    filename,
                    WorldPos::new(drop_world.x + offset_x, drop_world.y),
                );
                node.size = node_base_size(kind);
                node.content = extracted.markdown;
                node.asset_hash = Some(hash);
                let mut had_companion_subs = false;
                if kind == NodeKind::Audio {
                    if let Some(sub_path) = media_helper::find_companion_subtitle_file(path) {
                        if let Ok(raw_sub) = std::fs::read_to_string(&sub_path) {
                            let cues = media_helper::parse_srt_or_vtt(&raw_sub);
                            let txt = if cues.is_empty() {
                                raw_sub.trim().to_string()
                            } else {
                                media_helper::cues_to_plain_text(&cues)
                            };
                            if !txt.is_empty() || !cues.is_empty() {
                                node.meta.subtitles = cues;
                                node.meta.transcript_text = Some(txt);
                                node.meta.show_subtitles_on_card = true;
                                node.size = node_world_size(&node);
                                had_companion_subs = true;
                            }
                        }
                    }
                }
                let id = node.id.clone();
                self.graph.add_node(node);
                if kind == NodeKind::Audio && !had_companion_subs {
                    self.maybe_auto_generate_audio_subtitles(&id);
                }
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

    fn first_non_empty_env(names: &[&str]) -> Option<String> {
        names.iter().find_map(|name| {
            std::env::var(name)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        })
    }

const UNIVERSAL_API_ENV_VARS: &[&str] = &[
    "UNIVERSAL_API_KEY",
    "AI_API_KEY",
    "AIRFORCE_API_KEY",
    "OPENROUTER_API_KEY",
    "GROQ_API_KEY",
    "ANTHROPIC_API_KEY",
    "DEEPSEEK_API_KEY",
    "GEMINI_API_KEY",
    "OPENAI_API_KEY",
    "LLM_API_KEY",
];

    fn build_vision_config(&self) -> VisionConfig {
        let vision_pref_key = {
            let trimmed = self.reader_prefs.vision_api_key.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        };
        let vision_pref_base = {
            let trimmed = self.reader_prefs.vision_base_url.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        };
        let vision_pref_model = {
            let trimmed = self.reader_prefs.vision_model.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        };

        let ai_key = {
            let trimmed = self.reader_prefs.ai_api_key.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        };
        let ai_base = {
            let trimmed = self.reader_prefs.ai_base_url.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        };
        let ai_model = {
            let trimmed = self.reader_prefs.ai_model.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        };

        let vision_explicitly_set = vision_pref_key.is_some() || vision_pref_base.is_some();
        let env_cloud_key = Self::first_non_empty_env(&["VISION_API_KEY"])
            .or_else(|| Self::first_non_empty_env(Self::UNIVERSAL_API_ENV_VARS));

        let cloud_vision_api_key = vision_pref_key
            .clone()
            .or_else(|| if !vision_explicitly_set { ai_key.clone() } else { None })
            .or(env_cloud_key);

        let gemini_api_key = Self::first_non_empty_env(&["GEMINI_API_KEY"])
            .or_else(|| vision_pref_key.filter(|k| k.starts_with("AIza")))
            .or_else(|| {
                if !vision_explicitly_set {
                    ai_key.clone().filter(|k| k.starts_with("AIza"))
                } else {
                    None
                }
            });

        let cloud_vision_base_url = vision_pref_base
            .or_else(|| if !vision_explicitly_set { ai_base } else { None })
            .or_else(|| Self::first_non_empty_env(&["VISION_API_URL", "UNIVERSAL_API_URL", "AI_BASE_URL"]))
            .unwrap_or_default();

        let cloud_vision_model = vision_pref_model
            .or_else(|| if !vision_explicitly_set { ai_model } else { None })
            .or_else(|| Self::first_non_empty_env(&["VISION_MODEL"]))
            .unwrap_or_default();

        VisionConfig {
            models_dir: self.projects_dir.join("models"),
            engine: self.reader_prefs.ocr_engine,
            ollama_url: self.reader_prefs.ollama_url.clone(),
            ollama_model: self.reader_prefs.ollama_model.clone(),
            gemini_api_key,
            cloud_vision_api_key,
            cloud_vision_base_url,
            cloud_vision_model,
        }
    }

    pub fn cloud_vision_configured(&self) -> bool {
        self.build_vision_config().has_cloud_vision()
    }

    fn active_vision_service(&self) -> Arc<dyn VisionService> {
        if let Some(over) = &self.vision_service_override {
            return Arc::clone(over);
        }
        Arc::new(LocalVisionService::new(self.build_vision_config()))
    }

    pub fn save_vision_api_settings(&mut self) {
        self.reader_prefs.vision_api_key = self.vision_api_key_edit.text.trim().to_string();
        self.reader_prefs.vision_base_url = self.vision_base_url_edit.text.trim().to_string();
        self.reader_prefs.vision_model = self.vision_model_edit.text.trim().to_string();
        self.vision_last_error = None;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.push_reader_snapshot();
    }

    pub fn open_settings(&mut self) {
        if self.is_quiz_hard_locked() {
            return;
        }
        if self.screen != AppScreen::Settings {
            self.settings_return_screen = self.screen;
        }
        self.ai_api_key_edit = TextEdit::new(&self.reader_prefs.ai_api_key);
        self.ai_base_url_edit = TextEdit::new(&self.reader_prefs.ai_base_url);
        self.ai_model_edit = TextEdit::new(&self.reader_prefs.ai_model);
        self.chat_api_key_edit = TextEdit::new(&self.reader_prefs.chat_api_key);
        self.chat_base_url_edit = TextEdit::new(&self.reader_prefs.chat_base_url);
        self.chat_model_edit = TextEdit::new(&self.reader_prefs.chat_model);
        self.vision_api_key_edit = TextEdit::new(&self.reader_prefs.vision_api_key);
        self.vision_base_url_edit = TextEdit::new(&self.reader_prefs.vision_base_url);
        self.vision_model_edit = TextEdit::new(&self.reader_prefs.vision_model);
        self.ollama_url_edit = TextEdit::new(&self.reader_prefs.ollama_url);
        self.ollama_model_edit = TextEdit::new(&self.reader_prefs.ollama_model);
        self.show_block_menu = false;
        self.show_ai_menu = false;
        self.inspector_more_menu_open = false;
        self.screen = AppScreen::Settings;
    }

    pub fn close_settings(&mut self) {
        self.save_all_api_settings();
        self.screen = self.settings_return_screen;
    }

    pub fn save_all_api_settings(&mut self) {
        self.reader_prefs.ai_api_key = self.ai_api_key_edit.text.trim().to_string();
        self.reader_prefs.ai_base_url = self.ai_base_url_edit.text.trim().to_string();
        self.reader_prefs.ai_model = self.ai_model_edit.text.trim().to_string();
        self.reader_prefs.chat_api_key = self.chat_api_key_edit.text.trim().to_string();
        self.reader_prefs.chat_base_url = self.chat_base_url_edit.text.trim().to_string();
        self.reader_prefs.chat_model = self.chat_model_edit.text.trim().to_string();
        self.reader_prefs.vision_api_key = self.vision_api_key_edit.text.trim().to_string();
        self.reader_prefs.vision_base_url = self.vision_base_url_edit.text.trim().to_string();
        self.reader_prefs.vision_model = self.vision_model_edit.text.trim().to_string();

        let ollama_url = self.ollama_url_edit.text.trim();
        self.reader_prefs.ollama_url = if ollama_url.is_empty() {
            doc_reader::default_ollama_url()
        } else {
            ollama_url.to_string()
        };
        let ollama_model = self.ollama_model_edit.text.trim();
        self.reader_prefs.ollama_model = if ollama_model.is_empty() {
            doc_reader::default_ollama_model()
        } else {
            ollama_model.to_string()
        };

        self.llm_last_error = None;
        self.vision_last_error = None;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.push_reader_snapshot();
    }

    pub fn set_theme(&mut self, id: crate::theme::ThemeId) {
        self.reader_prefs.theme = id;
        crate::theme::set_active_theme(id);
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.push_reader_snapshot();
        self.notify_toast(crate::i18n::fmt_theme_toast(id.display_name()));
    }

    pub fn set_ui_locale(&mut self, locale: UiLocale) {
        self.reader_prefs.ui_locale = locale;
        self.dock_measured_size = None;
        crate::i18n::set_active_locale(locale);
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.push_reader_snapshot();
        self.notify_toast(crate::i18n::fmt_locale_toast(locale));
    }

    pub fn set_dock_position(&mut self, pos: DockPosition) {
        self.reader_prefs.dock_position = pos;
        self.reader_prefs.dock_custom_pos = None;
        self.dock_measured_size = None;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.notify_toast(format!(
            "{}: {}",
            i18n::t(i18n::TextKey::SettingsDockPosition),
            i18n::dock_position_label(pos)
        ));
    }

    pub fn cycle_dock_position(&mut self) {
        let next = self.reader_prefs.dock_position.cycle_next();
        self.set_dock_position(next);
    }

    pub fn toggle_dock_auto_orient(&mut self) {
        self.reader_prefs.dock_auto_orient = !self.reader_prefs.dock_auto_orient;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.notify_toast(if self.reader_prefs.dock_auto_orient {
            crate::i18n::t(crate::i18n::TextKey::SettingsDockAutoOrientOn)
        } else {
            crate::i18n::t(crate::i18n::TextKey::SettingsDockAutoOrientOff)
        });
    }

    pub fn toggle_animate_wires(&mut self) {
        self.reader_prefs.animate_wires = !self.reader_prefs.animate_wires;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.notify_toast(if self.reader_prefs.animate_wires {
            crate::i18n::t(crate::i18n::TextKey::SettingsWireAnimOn)
        } else {
            crate::i18n::t(crate::i18n::TextKey::SettingsWireAnimOff)
        });
    }

    pub fn toggle_trace_colors(&mut self) {
        self.reader_prefs.enable_trace_colors = !self.reader_prefs.enable_trace_colors;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.notify_toast(if self.reader_prefs.enable_trace_colors {
            crate::i18n::t(crate::i18n::TextKey::SettingsTraceColorsOn)
        } else {
            crate::i18n::t(crate::i18n::TextKey::SettingsTraceColorsOff)
        });
    }

    pub fn set_edge_color(
        &mut self,
        edge_id: &EdgeId,
        color: Option<&str>,
        category: Option<&str>,
    ) {
        let Some(edge) = self.graph.get_edge_mut(edge_id) else {
            return;
        };
        let mut toast = "Trace color updated".to_string();
        if let Some(raw) = color.map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(hex) = resolve_trace_color_hex(raw) {
                edge.color_hex = Some(hex);
                edge.color_category = category
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .or_else(|| resolve_trace_color_label(raw).map(ToOwned::to_owned));
                if let Some(cat) = &edge.color_category {
                    toast = format!("Trace color: {}", cat);
                }
            }
        } else {
            edge.color_hex = None;
            edge.color_category = None;
            toast = "Trace color: Auto".to_string();
        }
        self.persist_graph_and_snapshot();
        self.notify_toast(toast);
    }

    pub fn toggle_image_frameless(&mut self, id: &NodeId) {
        let mut msg = "Image frame updated";
        if let Some(n) = self.graph.get_node_mut(id) {
            if n.kind == NodeKind::Image {
                n.meta.image_frameless = !n.meta.image_frameless;
                msg = if n.meta.image_frameless {
                    "Image: Frameless"
                } else {
                    "Image: Framed"
                };
            }
        }
        self.persist_graph_and_snapshot();
        self.notify_toast(msg);
    }

    pub fn set_text_node_font(&mut self, id: &NodeId, font: &str) {
        if let Some(n) = self.graph.get_node_mut(id) {
            if n.kind == NodeKind::Text {
                n.meta.text_font = Some(font.trim().to_string());
                if n.meta.text_auto_fit {
                    n.size = compute_text_node_fit_size(n);
                }
            }
        }
        self.persist_graph_and_snapshot();
        self.status_message = format!("Text font: {}", font);
    }

    pub fn set_text_node_size(&mut self, id: &NodeId, size: f32) {
        let clamped = size.clamp(10.0, 96.0);
        if let Some(n) = self.graph.get_node_mut(id) {
            if n.kind == NodeKind::Text {
                n.meta.text_size = Some(clamped);
                if n.meta.text_auto_fit {
                    n.size = compute_text_node_fit_size(n);
                }
            }
        }
        self.persist_graph_and_snapshot();
        self.status_message = format!("Text size: {}pt", clamped.round() as i32);
    }

    pub fn fit_text_node_to_content(&mut self, id: &NodeId) {
        self.sync_node_edits();
        if let Some(n) = self.graph.get_node_mut(id) {
            if n.kind == NodeKind::Text {
                n.size = compute_text_node_fit_size(n);
            }
        }
        self.persist_graph_and_snapshot();
        self.notify_toast(crate::i18n::t(crate::i18n::TextKey::TextFitToContent));
    }

    pub fn toggle_text_auto_fit(&mut self, id: &NodeId) {
        self.sync_node_edits();
        let mut now_on = false;
        if let Some(n) = self.graph.get_node_mut(id) {
            if n.kind == NodeKind::Text {
                n.meta.text_auto_fit = !n.meta.text_auto_fit;
                now_on = n.meta.text_auto_fit;
                if now_on {
                    n.size = compute_text_node_fit_size(n);
                }
            }
        }
        self.persist_graph_and_snapshot();
        self.notify_toast(if now_on {
            crate::i18n::t(crate::i18n::TextKey::TextAutoFitOn)
        } else {
            crate::i18n::t(crate::i18n::TextKey::TextAutoFitOff)
        });
    }

    pub fn quiz_step_question(&mut self, id: &NodeId, delta: isize) {
        let total = self
            .graph
            .get_node(id)
            .map(|n| n.meta.quiz_questions.len())
            .unwrap_or(0);
        if total == 0 {
            return;
        }
        let sess = self.quiz_sessions.entry(id.clone()).or_default();
        if sess.status == QuizAttemptStatus::Failed {
            return;
        }
        let next = (sess.active_index as isize + delta).clamp(0, (total - 1) as isize) as usize;
        sess.active_index = next;
    }

    pub fn quiz_jump_question(&mut self, id: &NodeId, q_idx: usize) {
        let total = self
            .graph
            .get_node(id)
            .map(|n| n.meta.quiz_questions.len())
            .unwrap_or(0);
        if total == 0 {
            return;
        }
        let sess = self.quiz_sessions.entry(id.clone()).or_default();
        if sess.status == QuizAttemptStatus::Failed {
            return;
        }
        if sess.status == QuizAttemptStatus::NotStarted {
            sess.status = QuizAttemptStatus::InProgress;
        }
        sess.active_index = q_idx.min(total - 1);
    }

    pub fn quiz_select_option(&mut self, id: &NodeId, q_idx: usize, opt_idx: usize) {
        let sess = self.quiz_sessions.entry(id.clone()).or_default();
        if sess.status == QuizAttemptStatus::Failed {
            return;
        }
        if sess.status == QuizAttemptStatus::NotStarted {
            sess.status = QuizAttemptStatus::InProgress;
        }
        sess.selected_options.insert(q_idx, opt_idx);
    }

    pub fn quiz_reveal_next_hint(&mut self, id: &NodeId, q_idx: usize) {
        let total_hints = self
            .graph
            .get_node(id)
            .and_then(|n| n.meta.quiz_questions.get(q_idx))
            .map(|q| q.hints.len())
            .unwrap_or(0);
        if total_hints == 0 {
            return;
        }
        let sess = self.quiz_sessions.entry(id.clone()).or_default();
        if sess.status == QuizAttemptStatus::Failed {
            return;
        }
        if sess.status == QuizAttemptStatus::NotStarted {
            sess.status = QuizAttemptStatus::InProgress;
        }
        let cur = sess.revealed_hints.get(&q_idx).copied().unwrap_or(0);
        if cur < total_hints {
            sess.revealed_hints.insert(q_idx, cur + 1);
        }
    }

    pub fn quiz_reveal_answer(&mut self, id: &NodeId, q_idx: usize) {
        let sess = self.quiz_sessions.entry(id.clone()).or_default();
        if sess.status == QuizAttemptStatus::Failed {
            return;
        }
        if sess.status == QuizAttemptStatus::NotStarted {
            sess.status = QuizAttemptStatus::InProgress;
        }
        if sess.revealed_answers.contains(&q_idx) {
            sess.revealed_answers.remove(&q_idx);
        } else {
            sess.revealed_answers.insert(q_idx);
        }
    }

    pub fn quiz_reset_session(&mut self, id: &NodeId) {
        self.quiz_sessions.remove(id);
    }

    /// True when a `Hard` popout quiz attempt is currently `InProgress`, locking exit until finished or given up.
    pub fn is_quiz_hard_locked(&self) -> bool {
        let Some(id) = self.active_quiz_overlay.as_ref() else {
            return false;
        };
        let Some(node) = self.graph.get_node(id) else {
            return false;
        };
        if node.kind != NodeKind::Quiz || node.meta.quiz_difficulty != QuizDifficulty::Hard {
            return false;
        }
        self.quiz_sessions
            .get(id)
            .is_some_and(|sess| sess.status == QuizAttemptStatus::InProgress)
    }

    pub fn set_quiz_mode(&mut self, id: &NodeId, mode: QuizDisplayMode) {
        if let Some(n) = self.graph.get_node_mut(id) {
            if n.kind != NodeKind::Quiz {
                return;
            }
            if n.meta.quiz_mode != mode {
                n.meta.quiz_mode = mode;
                let is_default_regular =
                    (n.size.0 - 320.0).abs() < 1.0 && (n.size.1 - 260.0).abs() < 1.0;
                let is_default_popout =
                    (n.size.0 - 280.0).abs() < 1.0 && (n.size.1 - 148.0).abs() < 1.0;
                if mode == QuizDisplayMode::Popout
                    && (is_default_regular || (n.size.1 > 240.0 && n.size.0 <= 330.0))
                {
                    n.size = (280.0, 148.0);
                } else if mode == QuizDisplayMode::Regular
                    && (is_default_popout || n.size.1 < 200.0)
                {
                    n.size = (320.0, 260.0);
                }
                if mode == QuizDisplayMode::Regular && self.active_quiz_overlay.as_ref() == Some(id)
                {
                    self.active_quiz_overlay = None;
                }
                self.persist_graph_and_snapshot();
            }
        }
    }

    pub fn set_quiz_difficulty(&mut self, id: &NodeId, diff: QuizDifficulty) {
        if let Some(n) = self.graph.get_node_mut(id) {
            if n.kind != NodeKind::Quiz {
                return;
            }
            if n.meta.quiz_difficulty != diff {
                n.meta.quiz_difficulty = diff;
                self.persist_graph_and_snapshot();
            }
        }
    }

    /// Launch or resume the full-screen popout quiz overlay for `id`.
    pub fn start_or_resume_quiz_popout(&mut self, id: &NodeId) -> bool {
        if self.is_quiz_hard_locked() && self.active_quiz_overlay.as_ref() != Some(id) {
            self.notify_toast("Hard mode: finish the active quiz or Give Up first");
            return false;
        }
        let Some(node) = self.graph.get_node(id) else {
            return false;
        };
        if node.kind != NodeKind::Quiz {
            return false;
        }
        let total = node.meta.quiz_questions.len();
        if total == 0 {
            self.notify_toast("Add at least one question to start this quiz");
            return false;
        }
        let diff = node.meta.quiz_difficulty;
        let sess = self.quiz_sessions.entry(id.clone()).or_default();
        sess.active_index = sess.active_index.min(total - 1);
        if sess.status == QuizAttemptStatus::NotStarted {
            sess.status = QuizAttemptStatus::InProgress;
            sess.gave_up = false;
            sess.difficulty_used = Some(diff);
        }
        self.active_quiz_overlay = Some(id.clone());
        true
    }

    /// Reset a quiz attempt (e.g. after Hard-mode failure or completion) and start fresh in popout overlay.
    pub fn quiz_retry_popout(&mut self, id: &NodeId) -> bool {
        let Some(node) = self.graph.get_node(id) else {
            return false;
        };
        if node.kind != NodeKind::Quiz {
            return false;
        }
        if node.meta.quiz_questions.is_empty() {
            self.notify_toast("Add at least one question to start this quiz");
            return false;
        }
        let diff = node.meta.quiz_difficulty;
        let mut sess = QuizSessionState::default();
        sess.active_index = 0;
        sess.status = QuizAttemptStatus::InProgress;
        sess.gave_up = false;
        sess.difficulty_used = Some(diff);
        self.quiz_sessions.insert(id.clone(), sess);
        self.active_quiz_overlay = Some(id.clone());
        true
    }

    /// Return from the full-screen quiz overlay back to the workspace scheme (blocked while Hard-locked).
    pub fn quiz_return_to_scheme(&mut self) -> bool {
        if self.is_quiz_hard_locked() {
            self.notify_toast("Hard mode: finish the quiz or press Give Up to exit");
            return false;
        }
        self.active_quiz_overlay = None;
        true
    }

    /// Give up an active quiz attempt, marking it `Failed` and lifting the Hard-mode lock until retried.
    pub fn quiz_give_up_popout(&mut self) -> bool {
        let Some(id) = self.active_quiz_overlay.clone() else {
            return false;
        };
        let diff = self.graph.get_node(&id).map(|n| n.meta.quiz_difficulty);
        let sess = self.quiz_sessions.entry(id).or_default();
        sess.status = QuizAttemptStatus::Failed;
        sess.gave_up = true;
        if let Some(d) = diff {
            sess.difficulty_used = Some(d);
        }
        true
    }

    /// Mark the active popout quiz attempt as completed for summary review.
    pub fn quiz_finish_popout(&mut self) -> bool {
        let Some(id) = self.active_quiz_overlay.clone() else {
            return false;
        };
        let Some(node) = self.graph.get_node(&id) else {
            self.active_quiz_overlay = None;
            return false;
        };
        let diff = node.meta.quiz_difficulty;
        let sess = self.quiz_sessions.entry(id).or_default();
        if sess.status == QuizAttemptStatus::Failed {
            return false;
        }
        sess.status = QuizAttemptStatus::Completed;
        sess.gave_up = false;
        sess.difficulty_used = Some(diff);
        true
    }

    pub fn quiz_add_question_from_inspector(&mut self, id: &NodeId) -> bool {
        let q_text = self.quiz_q_edit.text.trim().to_string();
        if q_text.is_empty() {
            self.notify_toast("Enter a question prompt first");
            return false;
        }
        let options: Vec<String> = self
            .quiz_opts_edit
            .text
            .lines()
            .flat_map(|line| line.split('|'))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let correct_index = if options.is_empty() {
            None
        } else {
            Some(self.quiz_correct_idx.min(options.len() - 1))
        };
        let answer = self.quiz_ans_edit.text.trim().to_string();
        let hints: Vec<String> = self
            .quiz_hint_edit
            .text
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut added_idx = None;
        if let Some(n) = self.graph.get_node_mut(id) {
            if n.kind == NodeKind::Quiz {
                n.meta.quiz_questions.push(QuizQuestion {
                    id: uuid::Uuid::new_v4().to_string(),
                    question: q_text,
                    options,
                    correct_index,
                    answer,
                    hints,
                });
                added_idx = Some(n.meta.quiz_questions.len() - 1);
            }
        }
        if let Some(idx) = added_idx {
            self.quiz_q_edit = TextEdit::new("");
            self.quiz_opts_edit = TextEdit::new("");
            self.quiz_ans_edit = TextEdit::new("");
            self.quiz_hint_edit = TextEdit::new("");
            self.quiz_correct_idx = 0;
            self.quiz_sessions.entry(id.clone()).or_default().active_index = idx;
            self.persist_graph_and_snapshot();
            self.notify_toast(format!("Added quiz Q{}", idx + 1));
            return true;
        }
        false
    }

    pub fn quiz_delete_question(&mut self, id: &NodeId, q_idx: usize) {
        let mut removed = false;
        let mut new_len = 0;
        if let Some(n) = self.graph.get_node_mut(id) {
            if n.kind == NodeKind::Quiz && q_idx < n.meta.quiz_questions.len() {
                n.meta.quiz_questions.remove(q_idx);
                new_len = n.meta.quiz_questions.len();
                removed = true;
            }
        }
        if removed {
            if let Some(sess) = self.quiz_sessions.get_mut(id) {
                sess.active_index = if new_len == 0 {
                    0
                } else {
                    sess.active_index.min(new_len - 1)
                };
                sess.selected_options.clear();
                sess.revealed_hints.clear();
                sess.revealed_answers.clear();
                if new_len == 0 {
                    sess.status = QuizAttemptStatus::NotStarted;
                    sess.gave_up = false;
                }
            }
            if new_len == 0 && self.active_quiz_overlay.as_ref() == Some(id) {
                self.active_quiz_overlay = None;
            }
            self.persist_graph_and_snapshot();
            self.status_message = format!("Deleted quiz Q{}", q_idx + 1);
        }
    }

    pub fn quiz_append_hint(&mut self, id: &NodeId, q_idx: usize, hint: &str) {
        let clean = hint.trim();
        if clean.is_empty() {
            return;
        }
        if let Some(n) = self.graph.get_node_mut(id) {
            if let Some(q) = n.meta.quiz_questions.get_mut(q_idx) {
                q.hints.push(clean.to_string());
                self.persist_graph_and_snapshot();
            }
        }
    }

    /// True if a node holds no authored content, media assets, strokes, events, or quiz items.
    pub fn is_node_empty(node: &Node) -> bool {
        match node.kind {
            NodeKind::Document | NodeKind::Image => {
                node.asset_hash.is_none()
                    && node.content.trim().is_empty()
                    && node
                        .meta
                        .extracted_text
                        .as_deref()
                        .is_none_or(|s| s.trim().is_empty())
            }
            NodeKind::Drawing => {
                node.meta.drawing_strokes.is_empty()
                    && node.content.trim().is_empty()
                    && node.asset_hash.is_none()
            }
            NodeKind::Video => {
                node.asset_hash.is_none()
                    && node
                        .meta
                        .media_url
                        .as_deref()
                        .is_none_or(|s| s.trim().is_empty())
                    && node.content.trim().is_empty()
                    && node.meta.timestamps.is_empty()
            }
            NodeKind::Audio => {
                let c = node.content.trim();
                node.asset_hash.is_none()
                    && node
                        .meta
                        .media_url
                        .as_deref()
                        .is_none_or(|s| s.trim().is_empty())
                    && node.meta.timestamps.is_empty()
                    && node.meta.subtitles.is_empty()
                    && (c.is_empty() || c == i18n::default_node_hint(NodeKind::Audio))
            }
            NodeKind::Calendar => {
                node.meta.calendar_events.is_empty()
                    && node.meta.calendar_day_notes.is_empty()
                    && node.content.trim().is_empty()
            }
            NodeKind::Quiz => {
                node.meta.quiz_questions.is_empty() && node.content.trim().is_empty()
            }
            NodeKind::Flashcard => {
                node.content.trim().is_empty()
                    && node
                        .meta
                        .answer
                        .as_deref()
                        .is_none_or(|a| a.trim().is_empty())
            }
            NodeKind::Section => {
                let t = node.title.trim();
                node.content.trim().is_empty()
                    && (t.is_empty() || t == i18n::default_node_title(NodeKind::Section, 1))
            }
            NodeKind::Note
            | NodeKind::Text
            | NodeKind::LlmSynthesis
            | NodeKind::Concept
            | NodeKind::Excerpt
            | NodeKind::Formula
            | NodeKind::GraphFormula
            | NodeKind::Checklist
            | NodeKind::Sequence => node.content.trim().is_empty(),
        }
    }

    /// Delete selected nodes (or selected edge), or when nothing is selected, remove all empty nodes.
    pub fn delete_selected_or_empty_nodes(&mut self) -> usize {
        if !self.selected_nodes.is_empty() {
            let ids: Vec<NodeId> = self.selected_nodes.iter().cloned().collect();
            let count = ids.len();
            for id in &ids {
                self.delete_node(id);
            }
            self.selected_nodes.clear();
            self.editing_node_id = None;
            self.notify_toast(if count == 1 {
                "Deleted selected node".into()
            } else {
                format!("Deleted {} selected nodes", count)
            });
            return count;
        }
        if let Some(eid) = self.selected_edge.take() {
            self.graph.remove_edge(&eid);
            self.persist_graph_and_snapshot();
            self.notify_toast("Deleted selected link");
            return 1;
        }
        let empty_ids: Vec<NodeId> = self
            .graph
            .nodes
            .values()
            .filter(|n| {
                Self::is_node_empty(n)
                    && (n.kind != NodeKind::Section
                        || self.capture_section_children(&n.id).is_empty())
            })
            .map(|n| n.id.clone())
            .collect();
        let count = empty_ids.len();
        for id in &empty_ids {
            self.delete_node(id);
        }
        if count > 0 {
            self.notify_toast(format!("Deleted {} empty node(s)", count));
        } else {
            self.notify_toast("No empty nodes");
        }
        count
    }

    /// Returns true if a world-space point hits any non-Section study node bounding rectangle.
    pub fn is_world_point_over_node(&self, wp: WorldPos) -> bool {
        self.graph
            .nodes
            .values()
            .filter(|n| n.kind != NodeKind::Section)
            .any(|n| {
                let (w, h) = node_world_size(n);
                wp.x >= n.pos.x
                    && wp.x <= n.pos.x + w
                    && wp.y >= n.pos.y
                    && wp.y <= n.pos.y + h
            })
    }

    pub fn toggle_ai_summary_language_mode(&mut self) {
        self.reader_prefs.ai_summary_follow_ui_language =
            !self.reader_prefs.ai_summary_follow_ui_language;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        let msg = if self.reader_prefs.ai_summary_follow_ui_language {
            crate::i18n::fmt_summary_lang_ui(self.reader_prefs.ui_locale)
        } else {
            crate::i18n::t(crate::i18n::TextKey::SettingsAiSummaryLangSource).to_string()
        };
        self.notify_toast(msg);
    }

    /// Poll background project scans, async PDF renders, and debounced saves.
    pub fn poll_background_io(&mut self) -> bool {
        let mut changed = false;
        if self
            .pending_graph_save_at
            .is_some_and(|due| Instant::now() >= due)
        {
            self.flush_graph_save_async();
        }

        while let Ok(msg) = self.io_rx.try_recv() {
            changed = true;
            match msg {
                BackgroundIoResult::RecentProjectsLoaded(projects) => {
                    self.recent_projects = projects;
                }
                BackgroundIoResult::PdfRendered {
                    node_id,
                    asset_path,
                    page_index,
                    zoom_bucket,
                    for_card_thumbnail,
                    result,
                } => {
                    self.pdf_inflight
                        .remove(&(asset_path.clone(), page_index, zoom_bucket));
                    match result {
                        Ok(rendered) => {
                            if !rendered.page_dimensions.is_empty() {
                                self.pdf_dimensions_cache
                                    .insert(asset_path.clone(), rendered.page_dimensions.clone());
                            }
                            let pixels = Pixels::new();
                            pixels.set(rendered.width, rendered.height, rendered.rgba);
                            self.cache_pdf_page(
                                (asset_path.clone(), page_index, zoom_bucket),
                                rendered.width,
                                rendered.height,
                                pixels.clone(),
                            );
                            if for_card_thumbnail {
                                self.card_pdf_previews.insert(
                                    (node_id.clone(), page_index),
                                    CardPdfPreview {
                                        page_count: rendered.page_count.max(1),
                                        pixels: pixels.clone(),
                                    },
                                );
                            }
                            let mut need_more_pages = false;
                            if let Some(pdf) = self.active_pdf.as_mut() {
                                if pdf.node_id == node_id && pdf.asset_path == asset_path {
                                    let had_dims = !pdf.page_dimensions.is_empty();
                                    let prev_count = pdf.page_count;
                                    pdf.page_count = rendered.page_count.max(1);
                                    if !rendered.page_dimensions.is_empty() {
                                        pdf.page_dimensions = rendered.page_dimensions;
                                    }
                                    let (_, cur_bucket) = Self::pdf_render_scale(pdf.zoom);
                                    if zoom_bucket == cur_bucket
                                        || !pdf.pages.contains_key(&page_index)
                                    {
                                        pdf.pages.insert(
                                            page_index,
                                            (rendered.width, rendered.height, pixels),
                                        );
                                    }
                                    pdf.loading_pages.remove(&page_index);
                                    pdf.error = None;
                                    if !had_dims || prev_count != pdf.page_count {
                                        need_more_pages = true;
                                    }
                                }
                            }
                            if need_more_pages {
                                self.request_pdf_pages_for_active();
                            }
                            self.push_reader_snapshot();
                        }
                        Err(err) => {
                            if let Some(pdf) = self.active_pdf.as_mut() {
                                if pdf.node_id == node_id && pdf.asset_path == asset_path {
                                    pdf.loading_pages.remove(&page_index);
                                    pdf.error = Some(err);
                                }
                            }
                            self.push_reader_snapshot();
                        }
                    }
                }
                BackgroundIoResult::ScreenshotFinished { result } => match result {
                    Ok(path) => {
                        self.notify_toast(format!(
                            "{}: {}",
                            i18n::t(i18n::TextKey::ScreenshotSavedToast),
                            path.file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("scheme.png")
                        ));
                    }
                    Err(err) => {
                        self.notify_toast(format!("Screenshot error: {}", err));
                    }
                },
            }
        }
        changed
    }

    pub fn toggle_direct_manipulation(&mut self) {
        self.reader_prefs.use_direct_manipulation = !self.reader_prefs.use_direct_manipulation;
        crate::touchpad_win32::set_enabled(self.reader_prefs.use_direct_manipulation);
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.notify_toast(if self.reader_prefs.use_direct_manipulation {
            "Trackpad: DirectManipulation 2D"
        } else {
            "Trackpad: OS Wheel Fallback"
        });
    }

    #[allow(dead_code)]
    pub fn set_llm_provider_override(&mut self, provider: Arc<dyn LlmProvider>) {
        self.llm_provider_override = Some(provider);
    }

    pub fn apply_ai_provider_preset(&mut self, preset_id: &str) {
        if let Some(preset) = studynodes_llm::find_api_provider_preset(preset_id) {
            self.ai_base_url_edit = TextEdit::new(preset.base_url);
            if !preset.default_model.is_empty() {
                self.ai_model_edit = TextEdit::new(preset.default_model);
            } else if preset.id == "auto" {
                self.ai_model_edit = TextEdit::new("");
            }
            self.save_all_api_settings();
            self.notify_toast(format!("AI preset: {}", preset.name));
        }
    }

    pub fn resolve_ai_provider(&self) -> Option<(Arc<dyn LlmProvider>, String)> {
        if let Some(p) = &self.llm_provider_override {
            return Some((Arc::clone(p), "Custom AI".into()));
        }

        let key = {
            let trimmed = self.reader_prefs.ai_api_key.trim();
            if !trimmed.is_empty() {
                trimmed.to_string()
            } else {
                Self::first_non_empty_env(Self::UNIVERSAL_API_ENV_VARS).unwrap_or_default()
            }
        };

        let base_url = {
            let trimmed = self.reader_prefs.ai_base_url.trim();
            if !trimmed.is_empty() {
                trimmed.to_string()
            } else {
                Self::first_non_empty_env(&["UNIVERSAL_API_URL", "AI_BASE_URL", "LLM_API_URL"])
                    .unwrap_or_default()
            }
        };

        let model = {
            let trimmed = self.reader_prefs.ai_model.trim();
            if !trimmed.is_empty() {
                Some(trimmed.to_string())
            } else {
                Self::first_non_empty_env(&["UNIVERSAL_MODEL", "AI_MODEL", "LLM_MODEL"])
            }
        };

        resolve_universal_llm_provider(&key, &base_url, model)
    }

    pub fn ai_api_configured(&self) -> bool {
        self.resolve_ai_provider().is_some()
    }

    pub fn ai_provider_status_label(&self) -> String {
        if let Some((_, label)) = self.resolve_ai_provider() {
            crate::i18n::fmt_ai_status_ready(&label)
        } else {
            crate::i18n::ai_status_offline().to_string()
        }
    }

    pub fn chat_has_custom_api_key(&self) -> bool {
        !self.reader_prefs.chat_api_key.trim().is_empty()
            || !self.reader_prefs.chat_base_url.trim().is_empty()
            || !self.reader_prefs.chat_model.trim().is_empty()
    }

    pub fn resolve_chat_ai_provider(&self) -> Option<(Arc<dyn LlmProvider>, String)> {
        if let Some(p) = &self.llm_provider_override {
            return Some((Arc::clone(p), "Custom AI".into()));
        }

        let chat_key = self.reader_prefs.chat_api_key.trim();
        let chat_base = self.reader_prefs.chat_base_url.trim();
        let chat_model = self.reader_prefs.chat_model.trim();

        let key = if !chat_key.is_empty() {
            chat_key.to_string()
        } else if let Some(env_chat) = Self::first_non_empty_env(&["CHAT_API_KEY"]) {
            env_chat
        } else if !self.reader_prefs.ai_api_key.trim().is_empty() {
            self.reader_prefs.ai_api_key.trim().to_string()
        } else {
            Self::first_non_empty_env(Self::UNIVERSAL_API_ENV_VARS).unwrap_or_default()
        };

        let base_url = if !chat_base.is_empty() {
            chat_base.to_string()
        } else if chat_key.is_empty() && !self.reader_prefs.ai_base_url.trim().is_empty() {
            self.reader_prefs.ai_base_url.trim().to_string()
        } else {
            Self::first_non_empty_env(&[
                "CHAT_API_URL",
                "UNIVERSAL_API_URL",
                "AI_BASE_URL",
                "LLM_API_URL",
            ])
            .unwrap_or_default()
        };

        let model = if !chat_model.is_empty() {
            Some(chat_model.to_string())
        } else if chat_key.is_empty() && !self.reader_prefs.ai_model.trim().is_empty() {
            Some(self.reader_prefs.ai_model.trim().to_string())
        } else {
            Self::first_non_empty_env(&["CHAT_MODEL", "UNIVERSAL_MODEL", "AI_MODEL", "LLM_MODEL"])
        };

        resolve_universal_llm_provider(&key, &base_url, model)
    }

    pub fn chat_api_configured(&self) -> bool {
        self.resolve_chat_ai_provider().is_some()
    }

    pub fn chat_provider_status_label(&self) -> String {
        if let Some((_, label)) = self.resolve_chat_ai_provider() {
            crate::i18n::fmt_ai_status_ready(&label)
        } else {
            crate::i18n::ai_status_offline().to_string()
        }
    }

    pub fn set_ocr_engine(&mut self, engine: OcrEnginePreference) {
        self.reader_prefs.ocr_engine = engine;
        self.neural_models_ready = self.active_vision_service().model_status().is_ready();
        self.vision_last_error = None;
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        self.status_message = format!("OCR Engine: {}", engine.display_name());
        self.push_reader_snapshot();
    }

    pub fn cycle_ocr_engine(&mut self) {
        let next = match self.reader_prefs.ocr_engine {
            OcrEnginePreference::Auto => OcrEnginePreference::LocalPpOcrV5,
            OcrEnginePreference::LocalPpOcrV5 => OcrEnginePreference::CloudVision,
            OcrEnginePreference::CloudVision => OcrEnginePreference::Auto,
        };
        self.set_ocr_engine(next);
    }

    pub fn clear_extracted_text(&mut self, node_id: &NodeId) {
        if let Some(node) = self.graph.get_node_mut(node_id) {
            node.meta.extracted_text = None;
            node.meta.ocr_engine = None;
        }
        self.persist_graph_and_snapshot();
        self.status_message = "Cleared extracted OCR text".into();
    }

    pub fn clear_translated_text(&mut self, node_id: &NodeId) {
        if let Some(node) = self.graph.get_node_mut(node_id) {
            node.meta.translated_text = None;
            node.meta.target_language = None;
        }
        self.persist_graph_and_snapshot();
        self.status_message = "Cleared translated text".into();
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

    /// Download local PP-OCRv5 ONNX weights (~21 MB) in a non-blocking thread.
    pub fn download_neural_ocr_models(&mut self) {
        if self.vision_downloading_models {
            return;
        }
        let svc = self.active_vision_service();
        if svc.model_status().is_ready() {
            self.neural_models_ready = true;
            self.status_message = "PP-OCRv5 models already cached".into();
            self.push_reader_snapshot();
            return;
        }

        self.vision_downloading_models = true;
        self.vision_last_error = None;
        self.status_message = "Downloading PP-OCRv5 models (~21 MB)...".into();
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
            if matches!(n.kind, NodeKind::Image | NodeKind::Drawing) {
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
                                "PP-OCRv5 models downloaded and cached".into();
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

    pub fn cancel_smooth_zoom(&mut self) {
        self.smooth_zoom_target = None;
        self.last_zoom_step_clock = None;
    }

    pub fn is_zoom_animating(&self) -> bool {
        self.smooth_zoom_target.is_some()
    }

    pub fn sync_live_camera(&mut self) {
        let z0 = self.committed_viewport.zoom.max(Camera::MIN_ZOOM);
        let o0 = self.committed_viewport.offset;
        let z = self.graph.viewport.zoom.max(Camera::MIN_ZOOM);
        let o = self.graph.viewport.offset;
        let k = (z / z0).clamp(0.1, 10.0);
        let cw = self.canvas_size[0].max(200.0);
        let ch = self.canvas_size[1].max(200.0);
        let px = (o0.x - o.x) * z - (cw * 0.5) * (1.0 - k);
        let py = (o0.y - o.y) * z - (ch * 0.5) * (1.0 - k);
        self.cam_live_zoom.set(k);
        self.cam_live_pan.set([px, py]);
        self.cam_live_dirty = (k - 1.0).abs() > 1e-4 || px.abs() > 0.25 || py.abs() > 0.25;
        self.cam_live_last_input_at = Some(Instant::now());
    }

    pub fn flush_live_camera(&mut self) -> bool {
        let was_dirty = self.cam_live_dirty
            || (self.committed_viewport.zoom - self.graph.viewport.zoom).abs() > 1e-5
            || (self.committed_viewport.offset.x - self.graph.viewport.offset.x).abs() > 0.05
            || (self.committed_viewport.offset.y - self.graph.viewport.offset.y).abs() > 0.05;
        self.committed_viewport = self.graph.viewport;
        self.cam_live_zoom.set(1.0);
        self.cam_live_pan.set([0.0, 0.0]);
        self.cam_live_dirty = false;
        self.cam_live_last_input_at = None;
        was_dirty
    }

    pub fn should_commit_live_camera(&self) -> bool {
        if !self.smooth_zoom_enabled || !self.cam_live_dirty {
            return false;
        }
        let z0 = self.committed_viewport.zoom.max(Camera::MIN_ZOOM);
        let z = self.graph.viewport.zoom.max(Camera::MIN_ZOOM);
        let k = z / z0;
        let crossed_lod = (z0 < 0.48) != (z < 0.48);
        let pan = self.cam_live_pan.get();
        if crossed_lod
            || k < 0.40
            || k > 2.50
            || pan[0].abs() > 480.0
            || pan[1].abs() > 480.0
        {
            return true;
        }
        if self.is_zoom_animating()
            || crate::touchpad_win32::is_active()
            || matches!(self.canvas_mode, CanvasMode::Panning { .. })
        {
            return false;
        }
        self.cam_live_last_input_at
            .is_some_and(|at| at.elapsed() >= Duration::from_millis(260))
    }

    pub fn pan_live_by(&mut self, dx: f32, dy: f32, tx: Option<&mut Tx>) {
        self.graph.viewport.pan_by_screen_delta(dx, dy);
        if self.smooth_zoom_enabled {
            self.sync_live_camera();
            if let Some(t) = tx {
                t.live_only();
                t.wake_after(Duration::from_millis(16));
            }
        } else {
            self.flush_live_camera();
            if let Some(t) = tx {
                t.instant();
            }
        }
    }

    /// Zoom around `anchor` by `factor`. When `smooth_zoom_enabled` is true, accumulates into
    /// `smooth_zoom_target` in log-space and eases frame-by-frame via GPU `Live` camera values
    /// without re-running view layout or text shaping.
    pub fn queue_zoom_at(
        &mut self,
        anchor: ScreenPos,
        factor: f32,
        rate: f32,
        tx: Option<&mut Tx>,
    ) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        if !self.smooth_zoom_enabled {
            self.cancel_smooth_zoom();
            self.graph.viewport.zoom_at(anchor, factor);
            self.flush_live_camera();
            if let Some(t) = tx {
                t.instant();
            }
            return;
        }
        let base = self.smooth_zoom_target.unwrap_or(self.graph.viewport.zoom);
        let next_target = (base * factor).clamp(Camera::MIN_ZOOM, Camera::MAX_ZOOM);
        self.smooth_zoom_target = Some(next_target);
        self.smooth_zoom_anchor = anchor;
        self.smooth_zoom_rate = rate.max(8.0);
        if let Some(t) = tx {
            let now = t.clock();
            let _ = self.step_smooth_zoom(now);
            if self.should_commit_live_camera() {
                self.flush_live_camera();
                t.instant();
            } else {
                t.live_only();
            }
            t.wake_after(Duration::ZERO);
        }
    }

    /// Advance smooth camera zoom by one display frame toward `smooth_zoom_target`.
    pub fn step_smooth_zoom(&mut self, now_secs: f64) -> bool {
        let Some(target) = self.smooth_zoom_target else {
            return false;
        };
        let dt = match self.last_zoom_step_clock {
            Some(prev) if now_secs > prev => ((now_secs - prev) as f32).clamp(1.0 / 240.0, 0.033),
            _ => 1.0 / 120.0,
        };
        let cur = self.graph.viewport.zoom.max(Camera::MIN_ZOOM);
        let log_diff = target.ln() - cur.ln();
        let moved = if log_diff.abs() <= 0.001 {
            self.graph.viewport.zoom_at(self.smooth_zoom_anchor, target / cur);
            self.cancel_smooth_zoom();
            (target - cur).abs() > 1e-6
        } else {
            let blend = 1.0 - (-dt * self.smooth_zoom_rate).exp();
            let next_zoom = (cur.ln() + log_diff * blend)
                .exp()
                .clamp(Camera::MIN_ZOOM, Camera::MAX_ZOOM);
            let step_factor = next_zoom / cur;
            self.graph
                .viewport
                .zoom_at(self.smooth_zoom_anchor, step_factor);
            self.last_zoom_step_clock = Some(now_secs);
            true
        };
        if moved && self.smooth_zoom_enabled {
            self.sync_live_camera();
        }
        moved
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
                    if self.smooth_zoom_enabled {
                        self.cancel_smooth_zoom();
                    }
                    self.graph.viewport.zoom_at(local, factor);
                    changed = true;
                }
            }
        }
        if changed && self.smooth_zoom_enabled {
            self.sync_live_camera();
        }
        changed
    }

    // ── Collections & Project Archive IO ──────────────────────────────────────────

    pub fn all_collections(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in &self.reader_prefs.collections {
            let t = c.trim();
            if !t.is_empty() && !out.iter().any(|existing| existing.eq_ignore_ascii_case(t)) {
                out.push(t.to_string());
            }
        }
        let mut extras: Vec<String> = self
            .recent_projects
            .iter()
            .filter_map(|p| p.collection.as_deref())
            .chain(self.graph.collection.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect();
        extras.sort();
        for c in extras {
            if !out.iter().any(|existing| existing.eq_ignore_ascii_case(&c)) {
                out.push(c);
            }
        }
        out
    }

    pub fn create_collection(&mut self, name: &str) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }
        if !self
            .reader_prefs
            .collections
            .iter()
            .any(|c| c.eq_ignore_ascii_case(trimmed))
        {
            self.reader_prefs.collections.push(trimmed.to_string());
            let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        }
        self.new_collection_input = TextEdit::new("");
        self.show_new_collection_input = false;
        self.status_message = format!("Collection '{}' ready", trimmed);
    }

    pub fn delete_collection(&mut self, name: &str) {
        let target = name.trim();
        if target.is_empty() {
            return;
        }
        self.reader_prefs
            .collections
            .retain(|c| !c.eq_ignore_ascii_case(target));
        let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
        if self
            .active_collection_filter
            .as_deref()
            .is_some_and(|c| c.eq_ignore_ascii_case(target))
        {
            self.active_collection_filter = None;
        }
        if self
            .graph
            .collection
            .as_deref()
            .is_some_and(|c| c.eq_ignore_ascii_case(target))
        {
            self.graph.collection = None;
            self.persist_graph_and_snapshot();
        }
        for tab in &mut self.tabs {
            if tab
                .graph
                .collection
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case(target))
            {
                tab.graph.collection = None;
                let _ = tab.bundle.save_graph_atomic(&tab.graph);
            }
        }
        let matching_paths: Vec<PathBuf> = self
            .recent_projects
            .iter()
            .filter(|p| {
                p.collection
                    .as_deref()
                    .is_some_and(|c| c.eq_ignore_ascii_case(target))
            })
            .map(|p| p.path.clone())
            .collect();
        for p in matching_paths {
            if let Ok((b, mut g)) = ProjectBundle::open(&p) {
                g.collection = None;
                let _ = b.save_graph_atomic(&g);
            }
        }
        self.refresh_recent_projects();
        self.status_message = format!("Removed collection '{}'", target);
    }

    pub fn set_project_collection(&mut self, project_path: &Path, collection: Option<String>) {
        let clean_col = collection
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if let Some(ref col) = clean_col {
            if !self
                .reader_prefs
                .collections
                .iter()
                .any(|c| c.eq_ignore_ascii_case(col))
            {
                self.reader_prefs.collections.push(col.clone());
                let _ = doc_reader::save_preferences(&self.projects_dir, &self.reader_prefs);
            }
        }

        let mut updated_active = false;
        if let Some(bundle) = &self.bundle {
            if same_project_path(&bundle.root_dir, project_path) {
                self.graph.collection = clean_col.clone();
                let _ = bundle.save_graph_atomic(&self.graph);
                updated_active = true;
            }
        }
        if updated_active {
            self.save_active_into_tab();
            self.push_reader_snapshot();
        }

        let mut updated_tab = false;
        for tab in &mut self.tabs {
            if same_project_path(&tab.path, project_path) {
                tab.graph.collection = clean_col.clone();
                let _ = tab.bundle.save_graph_atomic(&tab.graph);
                updated_tab = true;
            }
        }

        if !updated_active && !updated_tab {
            if let Ok((b, mut g)) = ProjectBundle::open(project_path) {
                g.collection = clean_col.clone();
                let _ = b.save_graph_atomic(&g);
            }
        }

        for proj in &mut self.recent_projects {
            if same_project_path(&proj.path, project_path) {
                proj.collection = clean_col.clone();
            }
        }
        self.assigning_collection_for = None;
        self.status_message = match &clean_col {
            Some(c) => format!("Assigned project to collection '{}'", c),
            None => "Cleared project collection".into(),
        };
    }

    pub fn export_project_to_archive(&mut self, project_path: &Path, dest: &Path) {
        let is_active = self
            .bundle
            .as_ref()
            .is_some_and(|b| same_project_path(&b.root_dir, project_path));
        if is_active {
            self.sync_node_edits();
            self.sync_edge_edits();
            self.flush_graph_save_now();
            if let Some(bundle) = &self.bundle {
                match project_io::export_project_archive(bundle, &self.graph, dest) {
                    Ok(()) => {
                        self.status_message = format!("Exported project to {}", dest.display());
                    }
                    Err(e) => {
                        self.status_message = format!("Export failed: {}", e);
                    }
                }
                return;
            }
        }

        match ProjectBundle::open(project_path) {
            Ok((bundle, graph)) => {
                match project_io::export_project_archive(&bundle, &graph, dest) {
                    Ok(()) => {
                        self.status_message = format!("Exported project to {}", dest.display());
                    }
                    Err(e) => {
                        self.status_message = format!("Export failed: {}", e);
                    }
                }
            }
            Err(e) => {
                self.status_message = format!("Export failed: {}", e);
            }
        }
    }

    pub fn import_project_from_archive(&mut self, archive_path: &Path) {
        let collection_override = match self.active_collection_filter.as_deref() {
            Some("") => None,
            Some(c) => Some(c.to_string()),
            None => None,
        };
        match project_io::import_project_archive(
            archive_path,
            &self.projects_dir,
            collection_override,
        ) {
            Ok(dest_root) => {
                if let Ok((_, imported_graph)) = ProjectBundle::open(&dest_root) {
                    if let Some(ref col) = imported_graph.collection {
                        let trimmed = col.trim();
                        if !trimmed.is_empty()
                            && !self
                                .reader_prefs
                                .collections
                                .iter()
                                .any(|c| c.eq_ignore_ascii_case(trimmed))
                        {
                            self.reader_prefs.collections.push(trimmed.to_string());
                            let _ = doc_reader::save_preferences(
                                &self.projects_dir,
                                &self.reader_prefs,
                            );
                        }
                    }
                }
                self.refresh_recent_projects();
                self.open_project(dest_root);
                self.status_message = format!(
                    "Imported '{}'",
                    archive_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("project")
                );
            }
            Err(e) => {
                self.status_message = format!("Import failed: {}", e);
            }
        }
    }

    // ── Video & Audio Playback + HTML Iframe Embeds + Subtitles + Image Search ──

    pub fn normalize_media_url(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.contains("://")
            || trimmed.starts_with("mailto:")
            || trimmed.starts_with("spotify:")
        {
            trimmed.to_string()
        } else {
            format!("https://{}", trimmed)
        }
    }

    pub fn media_uses_iframe(url: &str) -> bool {
        let norm = Self::normalize_media_url(url);
        media_helper::embed_iframe_url(&norm, None).is_some()
    }

    pub fn ensure_media_player(&mut self, id: &NodeId) -> Option<Player> {
        let (kind, title, media_url) = {
            let n = self.graph.get_node(id)?;
            if !matches!(n.kind, NodeKind::Video | NodeKind::Audio) {
                return None;
            }
            (n.kind, n.title.clone(), n.meta.media_url.clone())
        };

        enum ResolvedSource {
            File(PathBuf),
            Url(String),
        }

        let resolved = if let Some(asset_path) = self.resolved_node_asset_path(id) {
            if asset_path.is_file() {
                Some(ResolvedSource::File(asset_path))
            } else {
                None
            }
        } else {
            None
        }
        .or_else(|| {
            let raw_url = media_url.as_deref().map(str::trim).unwrap_or("");
            if raw_url.is_empty() {
                return None;
            }
            let normalized = Self::normalize_media_url(raw_url);
            match media_helper::classify_media_url(&normalized) {
                media_helper::MediaPlatform::DirectUrl => Some(ResolvedSource::Url(normalized)),
                media_helper::MediaPlatform::YouTube { .. }
                | media_helper::MediaPlatform::Vimeo { .. }
                | media_helper::MediaPlatform::Spotify => {
                    // HTML <iframe> embeds play via WebView2; optionally auto-fill untitled cards via public oEmbed.
                    let needs_title = title.trim().is_empty()
                        || title == "New Video"
                        || title == "New Audio"
                        || title == "New Music";
                    if needs_title
                        && !rune::is_headless()
                        && self.oembed_fetched.insert(normalized.clone())
                    {
                        let tx = self.media_resolve_tx.clone();
                        let waker = self.vision_waker.clone();
                        let nid = id.clone();
                        let url_task = normalized;
                        std::thread::spawn(move || {
                            let fetched = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .ok()
                                .and_then(|rt| rt.block_on(media_helper::fetch_media_oembed_title(&url_task)));
                            let _ = tx.send(MediaJobResult::OembedTitleFetched {
                                node_id: nid,
                                title: fetched,
                            });
                            if let Some(w) = waker {
                                (w)();
                            }
                        });
                    }
                    None
                }
                _ => None,
            }
        });

        let Some(src) = resolved else {
            if let Some((_, old)) = self.media_players.remove(id) {
                old.pause();
            }
            return None;
        };

        let key = match &src {
            ResolvedSource::File(p) => format!("{:?}:file:{}", kind, p.display()),
            ResolvedSource::Url(u) => format!("{:?}:url:{}", kind, u),
        };

        if let Some((cached_key, player)) = self.media_players.get(id) {
            if cached_key == &key {
                return Some(player.clone());
            }
        }

        if let Some((_, old)) = self.media_players.remove(id) {
            old.pause();
        }

        let player = match (kind, src) {
            (NodeKind::Audio, ResolvedSource::File(p)) => Player::audio(MediaSource::File(p)),
            (NodeKind::Audio, ResolvedSource::Url(u)) => Player::audio(MediaSource::Url(u)),
            (_, ResolvedSource::File(p)) => {
                let p_inst = Player::file(p);
                p_inst.set_muted(false);
                p_inst
            }
            (_, ResolvedSource::Url(u)) => {
                let p_inst = Player::url(u);
                p_inst.set_muted(false);
                p_inst
            }
        };

        if let Some(w) = &self.vision_waker {
            player.set_waker(w.clone());
        }
        player.preload();
        self.media_players.insert(id.clone(), (key, player.clone()));
        Some(player)
    }

    pub fn ensure_all_media_players(&mut self) {
        let ids: Vec<NodeId> = self
            .graph
            .nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::Video | NodeKind::Audio))
            .map(|n| n.id.clone())
            .collect();
        for id in ids {
            let _ = self.ensure_media_player(&id);
        }
    }

    pub fn toggle_embed_collapsed(&mut self, id: &NodeId) {
        if !self.embed_collapsed.remove(id) {
            self.embed_collapsed.insert(id.clone());
        }
    }

    pub fn reload_embed_iframe(&mut self, id: &NodeId) {
        self.embed_collapsed.remove(id);
        let entry = self.embed_start_secs.entry(id.clone()).or_insert((0, 0));
        entry.1 = entry.1.wrapping_add(1).max(1);
    }

    fn clip_rect_to_canvas(&self, r: [f32; 4]) -> ([f32; 4], bool) {
        let bx0 = self.canvas_origin[0] + 1.0;
        let by0 = self.canvas_origin[1] + 1.0;
        let bx1 = (self.canvas_origin[0] + self.canvas_size[0] - 1.0).max(bx0);
        let by1 = (self.canvas_origin[1] + self.canvas_size[1] - 1.0).max(by0);
        let ix0 = r[0].max(bx0);
        let iy0 = r[1].max(by0);
        let ix1 = (r[0] + r[2]).min(bx1);
        let iy1 = (r[1] + r[3]).min(by1);
        let iw = (ix1 - ix0).max(0.0);
        let ih = (iy1 - iy0).max(0.0);
        let full_area = (r[2] * r[3]).max(1.0);
        let vis = iw >= 80.0 && ih >= 55.0 && (iw * ih) / full_area >= 0.70;
        ([ix0, iy0, iw, ih], vis)
    }

    pub fn desired_embeds(&self) -> Vec<crate::embed_webview::DesiredEmbed> {
        let cam_live = self.cam_live_dirty
            || self.is_zoom_animating()
            || crate::touchpad_win32::is_active()
            || matches!(self.canvas_mode, CanvasMode::Panning { .. });
        let global_ok = self.screen == AppScreen::Workspace
            && self.active_quiz_overlay.is_none()
            && !self.show_block_menu
            && !self.show_ai_menu
            && !cam_live
            && self.graph.viewport.zoom >= 0.45;
        let mut out = Vec::new();
        for node in self.graph.nodes.values() {
            if !matches!(node.kind, NodeKind::Video | NodeKind::Audio) {
                continue;
            }
            let Some(raw_url) = node
                .meta
                .media_url
                .as_deref()
                .map(str::trim)
                .filter(|u| !u.is_empty())
            else {
                continue;
            };
            let norm = Self::normalize_media_url(raw_url);
            let (start_secs, seek_nonce) = self
                .embed_start_secs
                .get(&node.id)
                .copied()
                .unwrap_or((0, 0));
            let start_opt = (start_secs > 0).then_some(start_secs);
            let Some(iframe_src) = media_helper::embed_iframe_url(&norm, start_opt) else {
                continue;
            };
            let collapsed = self.embed_collapsed.contains(&node.id);
            let is_resizing = matches!(
                &self.canvas_mode,
                CanvasMode::ResizingNode { node_id, .. } if node_id == &node.id
            );
            let drawn = self.embed_rects.get(&node.id).copied();
            let (rect, visible) = if global_ok && !collapsed && !is_resizing {
                if let Some(raw_r) = drawn {
                    self.clip_rect_to_canvas(raw_r)
                } else {
                    ([0.0, 0.0, 0.0, 0.0], false)
                }
            } else {
                ([0.0, 0.0, 0.0, 0.0], false)
            };
            out.push(crate::embed_webview::DesiredEmbed {
                node_id: node.id.clone(),
                iframe_src,
                title: node.title.clone(),
                rect,
                visible,
                seek_nonce,
            });
        }
        out
    }

    pub fn spawn_pending_image_searches(
        &mut self,
        searches: &[studynodes_graph::patch::PendingImageSearch],
    ) {
        if searches.is_empty() {
            return;
        }
        // Group consecutive placeholders that share the exact same query so a count=2..3 search issues
        // one network search and assigns each downloaded image to its corresponding card.
        let mut by_query: Vec<(String, Vec<NodeId>)> = Vec::new();
        for entry in searches {
            self.image_search_pending.insert(entry.node_id.clone());
            if let Some((_, ids)) = by_query.iter_mut().find(|(q, _)| q == &entry.query) {
                ids.push(entry.node_id.clone());
            } else {
                by_query.push((entry.query.clone(), vec![entry.node_id.clone()]));
            }
        }
        for (query, node_ids) in by_query {
            let tx = self.media_resolve_tx.clone();
            let waker = self.vision_waker.clone();
            let limit = node_ids.len().clamp(1, 3);
            std::thread::spawn(move || {
                let res = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())
                    .and_then(|rt| {
                        rt.block_on(media_helper::search_and_download_images(&query, limit))
                    });
                match res {
                    Ok(imgs) => {
                        for (idx, nid) in node_ids.into_iter().enumerate() {
                            let item_res = imgs.get(idx).cloned().ok_or_else(|| {
                                format!("No additional image result for \"{}\"", query)
                            });
                            let _ = tx.send(MediaJobResult::ImageSearched {
                                node_id: nid,
                                query: query.clone(),
                                result: item_res,
                            });
                        }
                    }
                    Err(err) => {
                        for nid in node_ids {
                            let _ = tx.send(MediaJobResult::ImageSearched {
                                node_id: nid,
                                query: query.clone(),
                                result: Err(err.clone()),
                            });
                        }
                    }
                }
                if let Some(w) = waker {
                    (w)();
                }
            });
        }
    }

    pub fn apply_searched_image_to_node(
        &mut self,
        id: &NodeId,
        query: &str,
        downloaded: media_helper::DownloadedImage,
    ) {
        let Some(bundle) = &self.bundle else {
            return;
        };
        let Ok((hash, dest_path)) = bundle.ingest_bytes(&downloaded.bytes, &downloaded.ext) else {
            return;
        };
        self.asset_path_cache.insert(hash.clone(), dest_path.clone());
        let img = self.get_or_load_image(&dest_path);
        self.card_image_previews.insert(id.clone(), img.clone());
        if self.selected_nodes.contains(id) && self.selected_nodes.len() == 1 {
            self.active_image = Some((id.clone(), dest_path, img));
        }
        if let Some(node) = self.graph.get_node_mut(id) {
            node.asset_hash = Some(hash);
            if node.title.trim().is_empty() || node.title == "Image" {
                node.title = downloaded.caption.clone();
            }
            node.content = format!(
                "{}\n\nSearch: \"{}\"\nSource: {}",
                downloaded.caption.trim(),
                query.trim(),
                downloaded.source_url
            );
        }
        self.doc_preview_cache.borrow_mut().remove(id);
        self.sync_edit_buffers_from_selected();
        self.persist_graph_and_snapshot();
        self.save_active_into_tab();
    }

    pub fn apply_audio_subtitles(
        &mut self,
        id: &NodeId,
        text: String,
        mut cues: Vec<SubtitleCue>,
    ) {
        cues.retain(|c| !c.text.trim().is_empty());
        cues.sort_by(|a, b| a.start.total_cmp(&b.start));
        let clean_text = if !text.trim().is_empty() {
            text.trim().to_string()
        } else {
            media_helper::cues_to_plain_text(&cues)
        };
        if let Some(node) = self.graph.get_node_mut(id) {
            node.meta.subtitles = cues;
            node.meta.transcript_text = (!clean_text.is_empty()).then_some(clean_text);
            node.meta.show_subtitles_on_card = true;
            node.size = node_world_size(node);
        }
        self.audio_subtitle_errors.remove(id);
        self.persist_graph_and_snapshot();
        self.save_active_into_tab();
    }

    pub fn toggle_audio_subtitles_on_card(&mut self, id: &NodeId) {
        let mut changed = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            if node.kind == NodeKind::Audio {
                node.meta.show_subtitles_on_card = !node.meta.show_subtitles_on_card;
                node.size = node_world_size(node);
                changed = true;
            }
        }
        if changed {
            self.persist_graph_and_snapshot();
            self.save_active_into_tab();
        }
    }

    pub fn ingest_subtitle_file_into_node(&mut self, id: &NodeId, path: &Path) {
        let Ok(raw) = std::fs::read_to_string(path) else {
            self.audio_subtitle_errors
                .insert(id.clone(), "Could not read subtitle file".into());
            return;
        };
        let cues = media_helper::parse_srt_or_vtt(&raw);
        let plain = if cues.is_empty() {
            raw.trim().to_string()
        } else {
            media_helper::cues_to_plain_text(&cues)
        };
        if plain.is_empty() && cues.is_empty() {
            self.audio_subtitle_errors
                .insert(id.clone(), "Subtitle file was empty".into());
            return;
        }
        let cnt = cues.len();
        self.apply_audio_subtitles(id, plain, cues);
        self.notify_toast(if cnt > 0 {
            format!("CC Loaded {} subtitle cue(s)", cnt)
        } else {
            "CC Loaded transcript text".into()
        });
    }

    pub fn maybe_auto_generate_audio_subtitles(&mut self, id: &NodeId) {
        if !self.reader_prefs.auto_transcribe_audio || rune::is_headless() {
            return;
        }
        if self.resolve_chat_ai_provider().or_else(|| self.resolve_ai_provider()).is_some() {
            self.generate_audio_subtitles(id);
        }
    }

    pub fn generate_audio_subtitles(&mut self, id: &NodeId) {
        if self.audio_subtitle_pending.contains(id) {
            return;
        }
        let Some((kind, filename, media_url)) = self
            .graph
            .get_node(id)
            .map(|n| (n.kind, n.title.clone(), n.meta.media_url.clone()))
        else {
            return;
        };
        if kind != NodeKind::Audio {
            return;
        }
        self.audio_subtitle_errors.remove(id);

        // 1. Check companion .srt / .vtt next to local file path if any asset path exists
        let asset_path = self.resolved_node_asset_path(id);
        if let Some(ref p) = asset_path {
            if let Some(sub_file) = media_helper::find_companion_subtitle_file(p) {
                if let Ok(raw) = std::fs::read_to_string(&sub_file) {
                    let cues = media_helper::parse_srt_or_vtt(&raw);
                    if !cues.is_empty() || !raw.trim().is_empty() {
                        let txt = if cues.is_empty() {
                            raw.trim().to_string()
                        } else {
                            media_helper::cues_to_plain_text(&cues)
                        };
                        self.apply_audio_subtitles(id, txt, cues);
                        self.notify_toast("CC Loaded companion subtitles");
                        return;
                    }
                }
            }
        }

        // 2. Locate transcription provider
        let provider_opt = self
            .resolve_chat_ai_provider()
            .or_else(|| self.resolve_ai_provider());
        let Some((provider, label)) = provider_opt else {
            self.audio_subtitle_errors.insert(
                id.clone(),
                "Configure a Gemini or OpenAI API key in Settings for auto subtitles, or load .srt/.vtt".into(),
            );
            return;
        };

        let lang_hint = self
            .reader_prefs
            .ai_summary_follow_ui_language
            .then(|| self.reader_prefs.ui_locale.english_name().to_string());

        // 3. Determine audio source: local asset bytes vs direct HTTP media URL
        let direct_url = media_url
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .map(Self::normalize_media_url)
            .filter(|u| {
                matches!(
                    media_helper::classify_media_url(u),
                    media_helper::MediaPlatform::DirectUrl
                )
            });

        if asset_path.as_ref().is_none_or(|p| !p.is_file()) && direct_url.is_none() {
            self.audio_subtitle_errors.insert(
                id.clone(),
                "Auto subtitles support local/direct audio files (or load .srt/.vtt)".into(),
            );
            return;
        }

        self.audio_subtitle_pending.insert(id.clone());
        let tx = self.media_resolve_tx.clone();
        let waker = self.vision_waker.clone();
        let nid = id.clone();
        self.notify_toast(format!("CC Generating subtitles via {}...", label));

        std::thread::spawn(move || {
            let run = || -> Result<studynodes_llm::AudioTranscriptionResult, String> {
                let (bytes, fname, mime) = if let Some(ref path) = asset_path {
                    let data = std::fs::read(path)
                        .map_err(|e| format!("Could not read audio file: {}", e))?;
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("mp3");
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&filename)
                        .to_string();
                    (data, name, media_helper::infer_audio_mime(ext).to_string())
                } else if let Some(ref url) = direct_url {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| e.to_string())?;
                    let data = rt.block_on(async {
                        let c = reqwest::Client::builder()
                            .timeout(Duration::from_secs(20))
                            .build()
                            .map_err(|e| e.to_string())?;
                        let r = c.get(url).send().await.map_err(|e| e.to_string())?;
                        if !r.status().is_success() {
                            return Err(format!("Audio download HTTP {}", r.status()));
                        }
                        let b = r.bytes().await.map_err(|e| e.to_string())?;
                        if b.len() > 25 * 1024 * 1024 {
                            return Err("Audio file exceeds 25 MB limit".into());
                        }
                        Ok(b.to_vec())
                    })?;
                    let tail = url
                        .split(['?', '#'])
                        .next()
                        .unwrap_or(url)
                        .rsplit('/')
                        .next()
                        .unwrap_or("audio.mp3")
                        .to_string();
                    let mime = media_helper::infer_audio_mime(&tail).to_string();
                    (data, tail, mime)
                } else {
                    return Err("No readable audio source".into());
                };

                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())?;
                rt.block_on(provider.transcribe_audio(&studynodes_llm::AudioTranscriptionRequest {
                    bytes,
                    filename: fname,
                    mime_type: mime,
                    language_hint: lang_hint,
                }))
                .map_err(|e| e.to_string())
            };

            let _ = tx.send(MediaJobResult::SubtitlesGenerated {
                node_id: nid,
                result: run(),
            });
            if let Some(w) = waker {
                (w)();
            }
        });
    }

    pub fn poll_media_jobs(&mut self) -> bool {
        let mut changed = false;
        while let Ok(msg) = self.media_resolve_rx.try_recv() {
            changed = true;
            match msg {
                MediaJobResult::OembedTitleFetched { node_id, title } => {
                    if let Some(title) = title {
                        if let Some(node) = self.graph.get_node_mut(&node_id) {
                            if node.title == "New Video"
                                || node.title == "New Audio"
                                || node.title == "New Music"
                                || node.title.trim().is_empty()
                            {
                                node.title = title;
                                if self.selected_nodes.contains(&node_id)
                                    && self.selected_nodes.len() == 1
                                {
                                    self.canvas_title_edit = TextEdit::new(&node.title);
                                    self.inspector_title_edit = TextEdit::new(&node.title);
                                }
                                self.persist_graph_and_snapshot();
                            }
                        }
                    }
                }
                MediaJobResult::ImageSearched {
                    node_id,
                    query,
                    result,
                } => {
                    self.image_search_pending.remove(&node_id);
                    match result {
                        Ok(img) => {
                            self.apply_searched_image_to_node(&node_id, &query, img);
                            self.notify_toast(format!("Found image for '{}'", query));
                        }
                        Err(err) => {
                            if let Some(node) = self.graph.get_node_mut(&node_id) {
                                node.content = format!("Image search failed ({}): {}", query, err);
                            }
                            self.sync_edit_buffers_from_selected();
                            self.notify_toast(format!("Image search: {}", err));
                        }
                    }
                }
                MediaJobResult::SubtitlesGenerated { node_id, result } => {
                    self.audio_subtitle_pending.remove(&node_id);
                    match result {
                        Ok(res) => {
                            let cues: Vec<SubtitleCue> = res
                                .segments
                                .into_iter()
                                .map(|s| SubtitleCue {
                                    start: s.start,
                                    end: s.end,
                                    text: s.text,
                                })
                                .collect();
                            self.apply_audio_subtitles(&node_id, res.text, cues);
                            let title = self
                                .graph
                                .get_node(&node_id)
                                .map(|n| n.title.clone())
                                .unwrap_or_else(|| "audio".into());
                            self.notify_toast(format!("CC Subtitles ready for '{}'", title));
                        }
                        Err(err) => {
                            self.audio_subtitle_errors.insert(node_id, err.clone());
                            self.notify_toast(format!("Subtitles error: {}", err));
                        }
                    }
                }
            }
        }
        changed
    }

    pub fn media_player_for(&self, id: &NodeId) -> Option<&Player> {
        self.media_players.get(id).map(|(_, p)| p)
    }

    pub fn toggle_media_play(&mut self, id: &NodeId) {
        self.flush_live_camera();
        let Some(player) = self.ensure_media_player(id) else {
            return;
        };
        if player.is_playing() {
            player.pause();
        } else {
            player.play();
        }
    }

    pub fn seek_media(&mut self, id: &NodeId, secs: f64) {
        let target = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
        let Some(player) = self.ensure_media_player(id) else {
            return;
        };
        let clamped = match player.duration() {
            Some(dur) if dur > 0.0 => target.min(dur),
            _ => target,
        };
        player.seek(clamped);
    }

    pub fn toggle_media_mute(&mut self, id: &NodeId) {
        let Some(player) = self.ensure_media_player(id) else {
            return;
        };
        player.set_muted(!player.muted());
    }

    pub fn cycle_media_rate(&mut self, id: &NodeId) {
        let Some(player) = self.ensure_media_player(id) else {
            return;
        };
        let r = player.rate();
        let next = if r < 1.15 {
            1.25
        } else if r < 1.4 {
            1.5
        } else if r < 1.85 {
            2.0
        } else {
            1.0
        };
        player.set_rate(next);
    }

    pub fn has_playing_media(&self) -> bool {
        self.media_players.values().any(|(_, p)| p.is_playing())
    }

    pub fn ingest_media_into_node(&mut self, id: &NodeId, path: &Path) {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Media")
            .to_string();
        let hash_opt = if let Some(bundle) = &self.bundle {
            match bundle.ingest_asset(path) {
                Ok((hash, dest_path)) => {
                    self.asset_path_cache.insert(hash.clone(), dest_path);
                    Some(hash)
                }
                Err(e) => {
                    self.status_message = format!("Failed to attach media '{}': {}", filename, e);
                    return;
                }
            }
        } else {
            None
        };

        let mut is_audio = false;
        let mut had_companion_subs = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            is_audio = node.kind == NodeKind::Audio;
            if let Some(hash) = hash_opt {
                node.asset_hash = Some(hash);
            }
            if node.title.trim().is_empty()
                || node.title == "New Video"
                || node.title == "New Audio"
                || node.title == "New Music"
            {
                node.title = filename.clone();
                self.canvas_title_edit = TextEdit::new(&node.title);
                self.inspector_title_edit = TextEdit::new(&node.title);
            }
            if is_audio {
                if let Some(sub_path) = media_helper::find_companion_subtitle_file(path) {
                    if let Ok(raw_sub) = std::fs::read_to_string(&sub_path) {
                        let cues = media_helper::parse_srt_or_vtt(&raw_sub);
                        let txt = if cues.is_empty() {
                            raw_sub.trim().to_string()
                        } else {
                            media_helper::cues_to_plain_text(&cues)
                        };
                        if !txt.is_empty() || !cues.is_empty() {
                            node.meta.subtitles = cues;
                            node.meta.transcript_text = Some(txt);
                            node.meta.show_subtitles_on_card = true;
                            node.size = node_world_size(node);
                            had_companion_subs = true;
                        }
                    }
                }
            }
        }
        self.persist_graph_and_snapshot();
        let _ = self.ensure_media_player(id);
        if is_audio && !had_companion_subs {
            self.maybe_auto_generate_audio_subtitles(id);
        }
        self.status_message = format!("Attached '{}'", filename);
    }

    pub fn save_media_url(&mut self, id: &NodeId) {
        let url_trimmed = self.media_url_edit.text.trim().to_string();
        let mut changed = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            let next_val = if url_trimmed.is_empty() {
                None
            } else {
                Some(url_trimmed.clone())
            };
            if node.meta.media_url != next_val {
                node.meta.media_url = next_val;
                changed = true;
            }
        }
        if changed {
            self.embed_collapsed.remove(id);
            self.embed_start_secs.remove(id);
            self.persist_graph_and_snapshot();
            let _ = self.ensure_media_player(id);
            self.status_message = if url_trimmed.is_empty() {
                "Cleared media URL".into()
            } else {
                format!(
                    "Saved {} media URL",
                    media_helper::classify_media_url(&url_trimmed).label()
                )
            };
        }
    }

    pub fn add_media_timestamp(&mut self, id: &NodeId, secs: f64, label: String) {
        let secs = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
        let clean_label = if label.trim().is_empty() {
            format!("Bookmark @ {}", media_helper::format_timestamp(secs))
        } else {
            label.trim().to_string()
        };
        let ts_id = uuid::Uuid::new_v4().to_string();
        let mut added = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            if matches!(node.kind, NodeKind::Video | NodeKind::Audio) {
                node.meta.timestamps.push(MediaTimestamp {
                    id: ts_id,
                    seconds: secs,
                    label: clean_label.clone(),
                });
                node.meta
                    .timestamps
                    .sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
                added = true;
            }
        }
        if added {
            self.persist_graph_and_snapshot();
            self.status_message = format!(
                "Added timestamp {} · {}",
                media_helper::format_timestamp(secs),
                clean_label
            );
        }
    }

    pub fn mark_current_media_timestamp(&mut self, id: &NodeId) {
        let secs = self.media_players.get(id).map(|(_, p)| p.time()).unwrap_or(0.0);
        let label = format!("Bookmark @ {}", media_helper::format_timestamp(secs));
        self.add_media_timestamp(id, secs, label);
    }

    pub fn remove_media_timestamp(&mut self, id: &NodeId, ts_id: &str) {
        let mut removed = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            let before = node.meta.timestamps.len();
            node.meta.timestamps.retain(|ts| ts.id != ts_id);
            removed = node.meta.timestamps.len() != before;
        }
        if removed {
            self.persist_graph_and_snapshot();
        }
    }

    pub fn open_media_external_url(&mut self, id: &NodeId, secs: f64, tx: &mut rune::Tx) {
        let secs = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
        if let Some(url) = self
            .graph
            .get_node(id)
            .and_then(|n| n.meta.media_url.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let normalized = Self::normalize_media_url(url);
            let target_url = media_helper::timestamp_jump_url(&normalized, secs);
            let _ = tx.open_url(&target_url);
            self.status_message = if secs > 0.0 {
                format!(
                    "Opened timestamp {} in browser",
                    media_helper::format_timestamp(secs)
                )
            } else {
                "Opened media link in browser".into()
            };
        }
    }

    pub fn jump_to_media_timestamp(&mut self, id: &NodeId, secs: f64, tx: &mut rune::Tx) {
        let secs = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
        if self.ensure_media_player(id).is_some() {
            self.seek_media(id, secs);
            if let Some(p) = self.media_player_for(id) {
                if !p.is_playing() {
                    p.play();
                }
            }
            self.status_message = format!("Jumped to {}", media_helper::format_timestamp(secs));
            return;
        }
        if let Some(url) = self
            .graph
            .get_node(id)
            .and_then(|n| n.meta.media_url.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
        {
            let normalized = Self::normalize_media_url(&url);
            if matches!(
                media_helper::classify_media_url(&normalized),
                media_helper::MediaPlatform::YouTube { .. }
                    | media_helper::MediaPlatform::Vimeo { .. }
            ) {
                self.embed_collapsed.remove(id);
                let prev_nonce = self.embed_start_secs.get(id).map(|(_, n)| *n).unwrap_or(0);
                self.embed_start_secs
                    .insert(id.clone(), (secs.round().max(0.0) as u64, prev_nonce.wrapping_add(1)));
                self.status_message = format!(
                    "Jumped embedded player to {}",
                    media_helper::format_timestamp(secs)
                );
                return;
            }
            let target_url = media_helper::timestamp_jump_url(&normalized, secs);
            let _ = tx.open_url(&target_url);
            self.status_message = format!(
                "Opened timestamp {} in browser",
                media_helper::format_timestamp(secs)
            );
        }
    }

    // ── Calendar Node Helpers ───────────────────────────────────────────────────

    pub fn calendar_step_month(&mut self, id: &NodeId, delta: i32) {
        let (today_y, today_m, _) = media_helper::today_ymd();
        let mut changed = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            if node.kind == NodeKind::Calendar {
                let y = node.meta.calendar_year.unwrap_or(today_y);
                let m = node.meta.calendar_month.unwrap_or(today_m);
                let (ny, nm) = media_helper::step_month(y, m, delta);
                node.meta.calendar_year = Some(ny);
                node.meta.calendar_month = Some(nm);
                changed = true;
            }
        }
        if changed {
            self.persist_graph_and_snapshot();
        }
    }

    pub fn calendar_select_date(&mut self, id: &NodeId, date: String) {
        self.flush_live_camera();
        let trimmed = date.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        self.selected_calendar_date.insert(id.clone(), trimmed.clone());
        self.cal_event_date_edit = TextEdit::new(&trimmed);
        let note = self
            .graph
            .get_node(id)
            .and_then(|n| n.meta.calendar_day_notes.get(&trimmed))
            .cloned()
            .unwrap_or_default();
        self.cal_day_note_edit = TextEdit::new(&note);
    }

    pub fn set_calendar_view_mode(&mut self, id: &NodeId, mode: CalendarViewMode) {
        let (today_y, today_m, today_d) = media_helper::today_ymd();
        let today_str = media_helper::format_ymd(today_y, today_m, today_d);
        let anchor = self
            .selected_calendar_date
            .get(id)
            .cloned()
            .unwrap_or(today_str);
        let mut changed = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            if node.kind == NodeKind::Calendar {
                if node.meta.calendar_view_mode != mode {
                    node.meta.calendar_view_mode = mode;
                    node.size = if mode == CalendarViewMode::Week {
                        (680.0, 200.0)
                    } else {
                        (312.0, 286.0)
                    };
                    changed = true;
                }
                if mode == CalendarViewMode::Week && node.meta.calendar_week_start.is_none() {
                    node.meta.calendar_week_start = Some(media_helper::monday_of_week(&anchor));
                    changed = true;
                }
            }
        }
        if changed {
            self.persist_graph_and_snapshot();
        }
    }

    pub fn calendar_step_week(&mut self, id: &NodeId, delta_weeks: i32) {
        let (today_y, today_m, today_d) = media_helper::today_ymd();
        let today_str = media_helper::format_ymd(today_y, today_m, today_d);
        let anchor = self
            .selected_calendar_date
            .get(id)
            .cloned()
            .unwrap_or(today_str);
        let mut new_selected = None;
        if let Some(node) = self.graph.get_node_mut(id) {
            if node.kind == NodeKind::Calendar {
                let cur_mon = node
                    .meta
                    .calendar_week_start
                    .clone()
                    .unwrap_or_else(|| media_helper::monday_of_week(&anchor));
                let next_mon = media_helper::add_days_ymd(&cur_mon, (delta_weeks as i64) * 7);
                node.meta.calendar_week_start = Some(next_mon.clone());
                new_selected = Some(next_mon);
            }
        }
        if let Some(sel) = new_selected {
            self.calendar_select_date(id, sel);
            self.persist_graph_and_snapshot();
        }
    }

    pub fn set_calendar_day_note(&mut self, id: &NodeId, date: String, note: String) {
        let clean_date = date.trim().to_string();
        if clean_date.is_empty() {
            return;
        }
        let mut changed = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            if node.kind == NodeKind::Calendar {
                if note.trim().is_empty() {
                    changed = node.meta.calendar_day_notes.remove(&clean_date).is_some();
                } else {
                    let prev = node.meta.calendar_day_notes.get(&clean_date).cloned();
                    if prev.as_deref() != Some(note.as_str()) {
                        node.meta
                            .calendar_day_notes
                            .insert(clean_date.clone(), note.clone());
                        changed = true;
                    }
                }
            }
        }
        let (today_y, today_m, today_d) = media_helper::today_ymd();
        let fallback = media_helper::format_ymd(today_y, today_m, today_d);
        let active_date = self
            .selected_calendar_date
            .get(id)
            .cloned()
            .unwrap_or(fallback);
        if active_date == clean_date && self.cal_day_note_edit.text != note {
            self.cal_day_note_edit = TextEdit::new(&note);
        }
        if changed {
            self.persist_graph_and_snapshot();
        }
    }

    pub fn sync_calendar_day_note_edit(&mut self, id: &NodeId) {
        let (today_y, today_m, today_d) = media_helper::today_ymd();
        let fallback = media_helper::format_ymd(today_y, today_m, today_d);
        let date = self
            .selected_calendar_date
            .get(id)
            .cloned()
            .unwrap_or(fallback);
        let note_text = self.cal_day_note_edit.text.clone();
        let mut changed = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            if node.kind == NodeKind::Calendar {
                if note_text.trim().is_empty() {
                    changed = node.meta.calendar_day_notes.remove(&date).is_some();
                } else if node.meta.calendar_day_notes.get(&date).map(String::as_str)
                    != Some(note_text.as_str())
                {
                    node.meta.calendar_day_notes.insert(date, note_text);
                    changed = true;
                }
            }
        }
        if changed {
            self.schedule_graph_save();
            self.push_reader_snapshot();
        }
    }

    pub fn add_calendar_event(
        &mut self,
        id: &NodeId,
        date: String,
        time: Option<String>,
        title: String,
        notes: String,
    ) {
        let clean_title = title.trim().to_string();
        if clean_title.is_empty() {
            return;
        }
        let clean_date = {
            let t = date.trim().to_string();
            if t.is_empty() {
                let (y, m, d) = media_helper::today_ymd();
                media_helper::format_ymd(y, m, d)
            } else {
                t
            }
        };
        let clean_time = time.and_then(|s| {
            let t = s.trim().to_string();
            if t.is_empty() { None } else { Some(t) }
        });
        let event_id = uuid::Uuid::new_v4().to_string();
        let mut added = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            if node.kind == NodeKind::Calendar {
                node.meta.calendar_events.push(CalendarEvent {
                    id: event_id,
                    date: clean_date.clone(),
                    time: clean_time,
                    title: clean_title.clone(),
                    notes: notes.trim().to_string(),
                    completed: false,
                    linked_node: None,
                });
                node.meta.calendar_events.sort_by(|a, b| {
                    a.date
                        .cmp(&b.date)
                        .then_with(|| a.time.as_deref().unwrap_or("").cmp(b.time.as_deref().unwrap_or("")))
                        .then_with(|| a.title.cmp(&b.title))
                });
                added = true;
            }
        }
        if added {
            self.selected_calendar_date.insert(id.clone(), clean_date.clone());
            self.persist_graph_and_snapshot();
            self.status_message = format!("Scheduled '{}' on {}", clean_title, clean_date);
        }
    }

    pub fn toggle_calendar_event(&mut self, id: &NodeId, event_id: &str) {
        let mut changed = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            if let Some(ev) = node
                .meta
                .calendar_events
                .iter_mut()
                .find(|e| e.id == event_id)
            {
                ev.completed = !ev.completed;
                changed = true;
            }
        }
        if changed {
            self.persist_graph_and_snapshot();
        }
    }

    pub fn remove_calendar_event(&mut self, id: &NodeId, event_id: &str) {
        let mut removed = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            let before = node.meta.calendar_events.len();
            node.meta.calendar_events.retain(|e| e.id != event_id);
            removed = node.meta.calendar_events.len() != before;
        }
        if removed {
            self.persist_graph_and_snapshot();
        }
    }

    // -------------------------------------------------------------------------
    // Freehand Drawing Node & Handwriting / Formula Interpreter + Screenshots
    // -------------------------------------------------------------------------

    pub fn begin_drawing_stroke(&mut self, id: &NodeId, nx: f32, ny: f32) {
        let color_hex = self.drawing_color_hex.clone();
        let width = self.drawing_stroke_width;
        let eraser = self.drawing_eraser;
        self.begin_drawing_stroke_styled(id, nx, ny, &color_hex, width, eraser);
    }

    pub fn begin_drawing_stroke_styled(
        &mut self,
        id: &NodeId,
        nx: f32,
        ny: f32,
        color_hex: &str,
        width: f32,
        eraser: bool,
    ) {
        if let Some(node) = self.graph.get_node_mut(id) {
            if node.kind == NodeKind::Drawing {
                drawing_begin_stroke(
                    &mut node.meta.drawing_strokes,
                    nx,
                    ny,
                    color_hex,
                    width,
                    eraser,
                );
            }
        }
    }

    pub fn extend_drawing_stroke(&mut self, id: &NodeId, nx: f32, ny: f32) {
        let color_hex = self.drawing_color_hex.clone();
        let width = self.drawing_stroke_width;
        let eraser = self.drawing_eraser;
        self.extend_drawing_stroke_with_mode(id, nx, ny, &color_hex, width, eraser);
    }

    pub fn extend_drawing_stroke_with_mode(
        &mut self,
        id: &NodeId,
        nx: f32,
        ny: f32,
        fallback_color_hex: &str,
        fallback_width: f32,
        eraser: bool,
    ) {
        let Some(node) = self.graph.get_node_mut(id) else {
            return;
        };
        if node.kind != NodeKind::Drawing {
            return;
        }
        drawing_extend_stroke(
            &mut node.meta.drawing_strokes,
            nx,
            ny,
            fallback_color_hex,
            fallback_width,
            eraser,
        );
    }

    pub fn finish_drawing_stroke(&mut self, _id: &NodeId) {
        self.persist_graph_and_snapshot();
    }

    pub fn undo_drawing_stroke(&mut self, id: &NodeId) {
        let mut changed = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            if node.kind == NodeKind::Drawing {
                changed = node.meta.drawing_strokes.pop().is_some();
            }
        }
        if changed {
            self.persist_graph_and_snapshot();
        }
    }

    pub fn clear_drawing_strokes(&mut self, id: &NodeId) {
        let mut changed = false;
        if let Some(node) = self.graph.get_node_mut(id) {
            if node.kind == NodeKind::Drawing && !node.meta.drawing_strokes.is_empty() {
                node.meta.drawing_strokes.clear();
                changed = true;
            }
        }
        if changed {
            self.persist_graph_and_snapshot();
        }
    }

    pub fn rasterize_drawing_to_png(&self, id: &NodeId) -> Result<PathBuf, String> {
        let node = self
            .graph
            .get_node(id)
            .ok_or_else(|| "Drawing node not found".to_string())?;
        if node.meta.drawing_strokes.is_empty() {
            return Err("Draw some strokes first before interpreting.".into());
        }

        const W: u32 = 960;
        const H: u32 = 640;
        const PAD: f32 = 48.0;
        let inner_w = (W as f32) - PAD * 2.0;
        let inner_h = (H as f32) - PAD * 2.0;
        let mut rgba = vec![255u8; (W as usize) * (H as usize) * 4];

        fn paint_disc(rgba: &mut [u8], w: u32, h: u32, cx: f32, cy: f32, r: f32) {
            let x0 = (cx - r - 1.5).floor().max(0.0) as u32;
            let x1 = (cx + r + 1.5).ceil().min((w - 1) as f32) as u32;
            let y0 = (cy - r - 1.5).floor().max(0.0) as u32;
            let y1 = (cy + r + 1.5).ceil().min((h - 1) as f32) as u32;
            for py in y0..=y1 {
                for px in x0..=x1 {
                    let dist = ((px as f32 + 0.5 - cx).powi(2) + (py as f32 + 0.5 - cy).powi(2)).sqrt();
                    let a = (r + 0.75 - dist).clamp(0.0, 1.0);
                    if a <= 0.0 {
                        continue;
                    }
                    let idx = ((py as usize) * (w as usize) + (px as usize)) * 4;
                    let inv = 1.0 - a;
                    rgba[idx] = (rgba[idx] as f32 * inv + 16.0 * a).round() as u8;
                    rgba[idx + 1] = (rgba[idx + 1] as f32 * inv + 16.0 * a).round() as u8;
                    rgba[idx + 2] = (rgba[idx + 2] as f32 * inv + 20.0 * a).round() as u8;
                }
            }
        }

        fn paint_seg(rgba: &mut [u8], w: u32, h: u32, p0: [f32; 2], p1: [f32; 2], r: f32) {
            let min_x = (p0[0].min(p1[0]) - r - 2.0).floor().max(0.0) as u32;
            let max_x = (p0[0].max(p1[0]) + r + 2.0).ceil().min((w - 1) as f32) as u32;
            let min_y = (p0[1].min(p1[1]) - r - 2.0).floor().max(0.0) as u32;
            let max_y = (p0[1].max(p1[1]) + r + 2.0).ceil().min((h - 1) as f32) as u32;
            let vx = p1[0] - p0[0];
            let vy = p1[1] - p0[1];
            let len2 = vx * vx + vy * vy;
            for py in min_y..=max_y {
                for px in min_x..=max_x {
                    let qx = px as f32 + 0.5;
                    let qy = py as f32 + 0.5;
                    let t = if len2 > 1e-6 {
                        (((qx - p0[0]) * vx + (qy - p0[1]) * vy) / len2).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let dx = qx - (p0[0] + vx * t);
                    let dy = qy - (p0[1] + vy * t);
                    let dist = (dx * dx + dy * dy).sqrt();
                    let a = (r + 0.75 - dist).clamp(0.0, 1.0);
                    if a <= 0.0 {
                        continue;
                    }
                    let idx = ((py as usize) * (w as usize) + (px as usize)) * 4;
                    let inv = 1.0 - a;
                    rgba[idx] = (rgba[idx] as f32 * inv + 16.0 * a).round() as u8;
                    rgba[idx + 1] = (rgba[idx + 1] as f32 * inv + 16.0 * a).round() as u8;
                    rgba[idx + 2] = (rgba[idx + 2] as f32 * inv + 20.0 * a).round() as u8;
                }
            }
        }

        for stroke in &node.meta.drawing_strokes {
            let r = (stroke.width.max(1.5) * 1.8).clamp(3.2, 10.0);
            if stroke.points.len() == 1 {
                let p = stroke.points[0];
                paint_disc(&mut rgba, W, H, PAD + p[0] * inner_w, PAD + p[1] * inner_h, r);
            } else {
                for win in stroke.points.windows(2) {
                    let p0 = [PAD + win[0][0] * inner_w, PAD + win[0][1] * inner_h];
                    let p1 = [PAD + win[1][0] * inner_w, PAD + win[1][1] * inner_h];
                    paint_seg(&mut rgba, W, H, p0, p1, r);
                }
            }
        }

        let dir = self.projects_dir.join("drawings_tmp");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let out_path = dir.join(format!("{}.png", id.as_str()));
        rune::render::write_png(&out_path, W, H, &rgba).map_err(|e| e.to_string())?;
        Ok(out_path)
    }

    pub fn interpret_drawing_node(&mut self, node_id: &NodeId, mode: OcrMode) {
        self.sync_node_edits();
        if self.vision_pending_nodes.contains(node_id) {
            return;
        }
        let img_path = match self.rasterize_drawing_to_png(node_id) {
            Ok(p) => p,
            Err(msg) => {
                self.vision_last_error = Some(msg.clone());
                self.notify_toast(msg);
                self.push_reader_snapshot();
                return;
            }
        };

        let svc = self.active_vision_service();
        self.vision_pending_nodes.insert(node_id.clone());
        self.vision_last_error = None;
        self.status_message = i18n::t(i18n::TextKey::DrawingInterpreting).into();
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

    pub fn export_scheme_screenshot(
        &mut self,
        section_id: Option<&NodeId>,
        target_path: Option<PathBuf>,
    ) -> Result<PathBuf, String> {
        let snap = if let Some(sec_id) = section_id {
            let n = self
                .graph
                .get_node(sec_id)
                .ok_or_else(|| "Section node not found".to_string())?;
            let sec_size = node_world_size(n);
            let (sec_children, sec_edges) = self.build_section_snapshot_data(n);
            ReaderSnapshot {
                node_id: Some(n.id.clone()),
                node_kind: NodeKind::Section,
                title: n.title.clone(),
                content: n.content.clone(),
                summary: n.summary.clone(),
                meta: n.meta.clone(),
                section_size: sec_size,
                section_children: sec_children,
                section_edges: sec_edges,
                project_title: self.graph.title.clone(),
                ..ReaderSnapshot::default()
            }
        } else {
            self.build_canvas_scheme_snapshot()
        };

        let out_path = if let Some(p) = target_path {
            p
        } else {
            let dir = self.projects_dir.join("screenshots");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let slug: String = snap
                .title
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c.to_ascii_lowercase()
                    } else {
                        '_'
                    }
                })
                .take(40)
                .collect();
            let ts = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let name = if slug.trim_matches('_').is_empty() {
                format!("scheme_{}.png", ts)
            } else {
                format!("{}_{}.png", slug.trim_matches('_'), ts)
            };
            dir.join(name)
        };

        if rune::is_headless() {
            crate::reader_window::render_scheme_to_png(&snap, &out_path)?;
            self.notify_toast(format!(
                "{}: {}",
                i18n::t(i18n::TextKey::ScreenshotSavedToast),
                out_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("scheme.png")
            ));
        } else {
            let path_clone = out_path.clone();
            let io_tx = self.io_tx.clone();
            let waker_slot = self.io_waker_slot.clone();
            self.notify_toast("Capturing high-res screenshot...");
            std::thread::spawn(move || {
                let res = crate::reader_window::render_scheme_to_png(&snap, &path_clone)
                    .map(|_| path_clone);
                let _ = io_tx.send(BackgroundIoResult::ScreenshotFinished { result: res });
                if let Ok(g) = waker_slot.lock() {
                    if let Some(w) = g.as_ref() {
                        (w)();
                    }
                }
            });
        }
        Ok(out_path)
    }
}

pub(crate) fn drawing_begin_stroke(
    strokes: &mut Vec<DrawingStroke>,
    nx: f32,
    ny: f32,
    color_hex: &str,
    width: f32,
    eraser: bool,
) {
    let nx = nx.clamp(0.0, 1.0);
    let ny = ny.clamp(0.0, 1.0);
    if eraser {
        drawing_extend_stroke(strokes, nx, ny, color_hex, width, true);
        return;
    }
    strokes.push(DrawingStroke {
        points: vec![[nx, ny]],
        color_hex: color_hex.to_string(),
        width: width.clamp(1.0, 12.0),
    });
}

pub(crate) fn drawing_extend_stroke(
    strokes: &mut Vec<DrawingStroke>,
    nx: f32,
    ny: f32,
    fallback_color_hex: &str,
    fallback_width: f32,
    eraser: bool,
) {
    let nx = nx.clamp(0.0, 1.0);
    let ny = ny.clamp(0.0, 1.0);
    if eraser {
        let r2 = 0.035 * 0.035;
        strokes.retain(|stroke| {
            !stroke
                .points
                .iter()
                .any(|p| (p[0] - nx).powi(2) + (p[1] - ny).powi(2) <= r2)
        });
        return;
    }
    if let Some(last) = strokes.last_mut() {
        if let Some(prev) = last.points.last() {
            if (prev[0] - nx).hypot(prev[1] - ny) < 0.004 {
                return;
            }
        }
        last.points.push([nx, ny]);
    } else {
        strokes.push(DrawingStroke {
            points: vec![[nx, ny]],
            color_hex: fallback_color_hex.to_string(),
            width: fallback_width.clamp(1.0, 12.0),
        });
    }
}
