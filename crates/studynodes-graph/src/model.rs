use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::coords::WorldPos;

/// Unique identifier for a node in the graph.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn from_str(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a port/socket on a node.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortId(pub String);

impl PortId {
    pub const DEFAULT_IN: &'static str = "in_main";
    pub const DEFAULT_OUT: &'static str = "out_main";

    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortDirection {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortType {
    Document,
    Note,
    Image,
    Any,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Port {
    pub id: PortId,
    pub name: String,
    pub direction: PortDirection,
    pub port_type: PortType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    /// Note created directly by the user (Markdown/plaintext)
    Note,
    /// Ingested document (PDF, EPUB, TXT, MD)
    Document,
    /// Visual asset (PNG, JPEG, SVG)
    Image,
    /// Node synthesizing multiple inputs via LLM
    LlmSynthesis,
    /// Conceptual cluster or topic anchor
    Concept,
    /// Spatial section title or collapsible/draggable group frame behind cards
    Section,
    /// Highlighted quotation deep-linked to a source document page
    Excerpt,
    /// Interactive spaced-repetition prompt/answer flip card
    Flashcard,
    /// Monospace mathematical equation, code snippet, or reaction schema
    Formula,
    /// Interactive mathematical formula editor and 2D function/equation plotter
    GraphFormula,
    /// Interactive task/study-checklist card backed by Markdown checkboxes
    Checklist,
    /// Numbered procedural or causal step card with chained spawning
    Sequence,
    /// Video player / web embed card with timestamp bookmarks
    Video,
    /// Interactive audio/music player card with timestamp bookmarks
    Audio,
    /// Interactive study calendar & schedule card accessible to both user and AI
    Calendar,
    /// Freehand canvas drawing / sketch card for handwritten notes, schemes & formulas
    Drawing,
    /// Frameless floating canvas text annotation with customizable font and size
    Text,
    /// Interactive multi-question quiz node with choices, answers, and hints
    Quiz,
}

impl NodeKind {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Document => "Document",
            Self::Image => "Image",
            Self::LlmSynthesis => "AI Synthesis",
            Self::Concept => "Concept",
            Self::Section => "Section",
            Self::Excerpt => "Excerpt",
            Self::Flashcard => "Flashcard",
            Self::Formula => "Formula",
            Self::GraphFormula => "Graphic Formula",
            Self::Checklist => "Checklist",
            Self::Sequence => "Sequence",
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Calendar => "Calendar",
            Self::Drawing => "Drawing",
            Self::Text => "Text",
            Self::Quiz => "Quiz",
        }
    }

    pub fn default_color_hex(&self) -> &'static str {
        match self {
            Self::Note => "#eab308",         // Yellow/Amber
            Self::Document => "#3b82f6",     // Blue
            Self::Image => "#10b981",        // Emerald
            Self::LlmSynthesis => "#8b5cf6", // Purple/Violet
            Self::Concept => "#ec4899",      // Pink
            Self::Section => "#73726e",      // Warm stone frame
            Self::Excerpt => "#d97757",      // Terracotta citation
            Self::Flashcard => "#d4d3ce",    // Chalk prompt/flip
            Self::Formula => "#b8b7b0",      // Technical mono slate
            Self::GraphFormula => "#4db6ac", // Interactive graphing teal
            Self::Checklist => "#9a9993",    // Muted task checklist
            Self::Sequence => "#ecebe6",     // Bright ordinal step
            Self::Video => "#c97b74",        // Muted rosewood video embed
            Self::Audio => "#7fa99b",        // Muted sage audio/music
            Self::Calendar => "#c8a46d",     // Muted warm ochre schedule
            Self::Drawing => "#8fa8c8",      // Cool steel-blue freehand sketch
            Self::Text => "#e5e5e0",         // Frameless floating typography
            Self::Quiz => "#d9a74a",         // Interactive quiz amber
        }
    }
}

/// Timestamp bookmark attached to a `Video` or `Audio` node.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaTimestamp {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub seconds: f64,
    #[serde(default)]
    pub label: String,
}

/// Scheduled study event stored on a `Calendar` node (`YYYY-MM-DD`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEvent {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default)]
    pub completed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_node: Option<NodeId>,
}

/// View mode for a `Calendar` node (`Month` grid or `Week` 7-column planner).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarViewMode {
    #[default]
    Month,
    Week,
}

/// Timed subtitle or transcription cue attached to an `Audio` (or `Video`) node.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SubtitleCue {
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: f64,
    #[serde(default)]
    pub text: String,
}

/// Freehand vector stroke stored on a `Drawing` node.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DrawingStroke {
    /// Normalized coordinates in `0.0..=1.0` relative to the drawing pad bounds.
    #[serde(default)]
    pub points: Vec<[f32; 2]>,
    /// Hex stroke color (e.g. `"#F4F3EE"`).
    #[serde(default)]
    pub color_hex: String,
    /// Stroke width in logical points.
    #[serde(default)]
    pub width: f32,
}

/// Interactive question item stored on a `Quiz` node.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuizQuestion {
    #[serde(default)]
    pub id: String,
    #[serde(default, alias = "prompt", alias = "q")]
    pub question: String,
    #[serde(default, alias = "choices", alias = "answers")]
    pub options: Vec<String>,
    #[serde(default, alias = "correct", alias = "answer_index")]
    pub correct_index: Option<usize>,
    #[serde(default, alias = "explanation", alias = "solution")]
    pub answer: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    /// Optional SHA-256 asset hash (in `assets/`) for an image shown in the question.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_image_hash: Option<String>,
    /// Optional SHA-256 asset hash (in `assets/`) for an image shown in the answer/explanation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_image_hash: Option<String>,
}

/// Display mode for a `Quiz` node on the canvas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuizDisplayMode {
    /// Interactive quiz card directly inside the canvas node.
    #[default]
    Regular,
    /// Compact launcher card on the canvas that opens a full-screen quiz overlay.
    Popout,
}

/// Difficulty / locking mode for a `Quiz` node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuizDifficulty {
    /// Learner can pause and return to the canvas scheme at any time and resume later.
    #[default]
    Easy,
    /// Learner is locked into the quiz until finishing or giving up (which marks it failed until retried).
    Hard,
}

/// Optional metadata extending specialized study nodes (`Flashcard`, `Excerpt`, `Formula`, `Sequence`, `Section`, `Video`, `Audio`, `Calendar`, `Drawing`, `Image`, `Text`, `Quiz`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeMeta {
    /// Answer / back face for `Flashcard` cards.
    #[serde(default)]
    pub answer: Option<String>,
    /// Spaced repetition / recall mastery rating (`"new"`, `"again"`, `"good"`, `"easy"`).
    #[serde(default)]
    pub mastery: Option<String>,
    /// Source document node referenced by an `Excerpt`.
    #[serde(default)]
    pub source_node: Option<NodeId>,
    /// Zero-based page or section index referenced by an `Excerpt`.
    #[serde(default)]
    pub source_page: Option<usize>,
    /// Language or notation tag for `Formula` cards (`"MATH"`, `"RUST"`, `"CHEM"`, etc.).
    #[serde(default)]
    pub language: Option<String>,
    /// 1-based ordinal step index for `Sequence` cards.
    #[serde(default)]
    pub step_index: Option<u32>,
    /// Whether a `Section` renders as a framed group backdrop (`true`) or title banner only (`false`).
    #[serde(default)]
    pub section_framed: bool,
    /// Cached OCR-extracted text for `Image` or `Drawing` nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extracted_text: Option<String>,
    /// Label describing which OCR engine produced `extracted_text`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocr_engine: Option<String>,
    /// Cached translated text for `Image`, `Note`, `Excerpt`, or `Document` nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translated_text: Option<String>,
    /// Target language code used for `translated_text` (e.g. `"es"`, `"en"`, `"ja"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_language: Option<String>,
    /// External URL (YouTube, Spotify, Vimeo, or direct media URL) for `Video` / `Audio` nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_url: Option<String>,
    /// Time-indexed bookmarks for `Video` / `Audio` nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timestamps: Vec<MediaTimestamp>,
    /// Events tracked on a `Calendar` node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calendar_events: Vec<CalendarEvent>,
    /// Visible calendar year on a `Calendar` node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_year: Option<i32>,
    /// Visible 1-based calendar month (`1..=12`) on a `Calendar` node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_month: Option<u32>,
    /// Display mode (`Month` or `Week`) for a `Calendar` node.
    #[serde(default)]
    pub calendar_view_mode: CalendarViewMode,
    /// Per-date notes keyed by `"YYYY-MM-DD"` for `Calendar` day cells and Week planner columns.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub calendar_day_notes: std::collections::BTreeMap<String, String>,
    /// Monday `"YYYY-MM-DD"` anchor when viewing `CalendarViewMode::Week`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_week_start: Option<String>,
    /// Timed subtitle segments for `Audio` (or `Video`) nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtitles: Vec<SubtitleCue>,
    /// Plain-text transcript / subtitles for `Audio` (or `Video`) nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_text: Option<String>,
    /// Whether subtitles are displayed directly inside the `Audio` canvas card.
    #[serde(default)]
    pub show_subtitles_on_card: bool,
    /// Freehand vector strokes for `Drawing` nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawing_strokes: Vec<DrawingStroke>,
    /// When true, `Image` cards render without card frame/header/controls (image-only).
    #[serde(default)]
    pub image_frameless: bool,
    /// Font preset for `Text` nodes (`"sans"`, `"serif"`, `"mono"`, `"display"`, `"hand"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_font: Option<String>,
    /// World-space base font size in points for `Text` nodes (defaults to 18.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_size: Option<f32>,
    /// When true, `Text` nodes automatically resize their frame to fit their content.
    #[serde(default)]
    pub text_auto_fit: bool,
    /// Question deck for `Quiz` nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quiz_questions: Vec<QuizQuestion>,
    /// Display mode for `Quiz` nodes (`Regular` inline card or `Popout` full-screen quiz).
    #[serde(default)]
    pub quiz_mode: QuizDisplayMode,
    /// Difficulty / exit-lock mode for `Quiz` nodes (`Easy` allows returning to scheme; `Hard` locks until finished or given up).
    #[serde(default)]
    pub quiz_difficulty: QuizDifficulty,
    /// Plot viewport X center in math coordinates for `GraphFormula` nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_center_x: Option<f64>,
    /// Plot viewport Y center in math coordinates for `GraphFormula` nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_center_y: Option<f64>,
    /// Plot viewport horizontal span in math units for `GraphFormula` nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_span_x: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub title: String,
    pub content: String,
    pub pos: WorldPos,
    pub size: (f32, f32),
    /// Content hash for external assets (stored in assets/ directory)
    pub asset_hash: Option<String>,
    /// Selected preview page/section index rendered on the canvas Document card.
    #[serde(default)]
    pub preview_page: usize,
    /// Summary generated by user or LLM
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub ports: Vec<Port>,
    /// Specialized metadata for study card kinds (`Section`, `Flashcard`, `Excerpt`, `Formula`, `Checklist`, `Sequence`).
    #[serde(default)]
    pub meta: NodeMeta,
}

impl Node {
    pub const DEFAULT_WIDTH: f32 = 280.0;
    pub const DEFAULT_HEIGHT: f32 = 180.0;

    pub fn new(kind: NodeKind, title: impl Into<String>, pos: WorldPos) -> Self {
        let ports = vec![
            Port {
                id: PortId::new(PortId::DEFAULT_IN),
                name: "In".into(),
                direction: PortDirection::Input,
                port_type: PortType::Any,
            },
            Port {
                id: PortId::new(PortId::DEFAULT_OUT),
                name: "Out".into(),
                direction: PortDirection::Output,
                port_type: PortType::Any,
            },
        ];

        Self {
            id: NodeId::new(),
            kind,
            title: title.into(),
            content: String::new(),
            pos,
            size: (Self::DEFAULT_WIDTH, Self::DEFAULT_HEIGHT),
            asset_hash: None,
            preview_page: 0,
            summary: None,
            tags: Vec::new(),
            ports,
            meta: NodeMeta::default(),
        }
    }

    /// Effective world-space bounding box dimensions accounting for kind-specific defaults.
    pub fn effective_size(&self) -> (f32, f32) {
        if self.kind == NodeKind::Section {
            let w = if self.size.0 >= 180.0 {
                self.size.0
            } else {
                640.0
            };
            let h = if self.meta.section_framed {
                if self.size.1 >= 120.0 {
                    self.size.1
                } else {
                    400.0
                }
            } else {
                44.0
            };
            (w, h)
        } else if self.kind == NodeKind::Calendar {
            match self.meta.calendar_view_mode {
                CalendarViewMode::Week => (680.0, 200.0),
                CalendarViewMode::Month => {
                    if self.size.0 > 1.0
                        && self.size.1 > 1.0
                        && ((self.size.0 - Self::DEFAULT_WIDTH).abs() > 0.5
                            || (self.size.1 - Self::DEFAULT_HEIGHT).abs() > 0.5)
                    {
                        self.size
                    } else {
                        (312.0, 286.0)
                    }
                }
            }
        } else if self.size.0 > 1.0 && self.size.1 > 1.0 {
            self.size
        } else {
            (Self::DEFAULT_WIDTH, Self::DEFAULT_HEIGHT)
        }
    }

    /// Calculate world-space socket position for a given port
    pub fn port_world_pos(&self, port_id: &PortId) -> Option<WorldPos> {
        let port = self.ports.iter().find(|p| &p.id == port_id)?;
        let (w, h) = self.size;
        let y_center = self.pos.y + h * 0.5;

        match port.direction {
            PortDirection::Input => Some(WorldPos::new(self.pos.x, y_center)),
            PortDirection::Output => Some(WorldPos::new(self.pos.x + w, y_center)),
        }
    }
}

/// Unique identifier for an edge wire.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeId(pub String);

impl EdgeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn from_str(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for EdgeId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeEndpoint {
    pub node: NodeId,
    pub port: PortId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeRelation {
    Supports,
    Contradicts,
    DerivedFrom,
    RelatesTo,
    Explains,
    ExampleOf,
    Custom(String),
}

impl EdgeRelation {
    pub fn display_label(&self) -> &str {
        match self {
            Self::Supports => "supports",
            Self::Contradicts => "contradicts",
            Self::DerivedFrom => "derived from",
            Self::RelatesTo => "relates to",
            Self::Explains => "explains",
            Self::ExampleOf => "example of",
            Self::Custom(s) => s.as_str(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeStatus {
    /// Proposed by LLM (Ghost Edge) waiting for user review
    Proposed,
    /// Confirmed and accepted into graph
    Accepted,
    /// Rejected/dismissed
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeProvenance {
    User,
    Llm,
}

/// Semantic trace color presets grouped by relevance & study topic: `(id, label, hex)`.
pub const TRACE_COLOR_PRESETS: &[(&str, &str, &str)] = &[
    // Relevance
    ("critical", "Critical", "#E06C75"),
    ("core", "Core", "#D97757"),
    ("supporting", "Supporting", "#6AB07A"),
    ("reference", "Reference", "#7A8A9E"),
    // Topics
    ("theory", "Theory", "#5B9BD5"),
    ("synthesis", "Synthesis", "#A37CE0"),
    ("example", "Example", "#D9A74A"),
    ("method", "Method", "#4DB6AC"),
];

/// Resolve a semantic preset name, alias, or `#RRGGBB` string into a normalized uppercase `#RRGGBB` hex color.
pub fn resolve_trace_color_hex(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(hex_body) = trimmed.strip_prefix('#') {
        if (hex_body.len() == 6 || hex_body.len() == 8)
            && hex_body.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Some(format!("#{}", hex_body.to_ascii_uppercase()));
        }
        if hex_body.len() == 3 && hex_body.chars().all(|c| c.is_ascii_hexdigit()) {
            let mut full = String::with_capacity(7);
            full.push('#');
            for c in hex_body.chars() {
                let u = c.to_ascii_uppercase();
                full.push(u);
                full.push(u);
            }
            return Some(full);
        }
        return None;
    }

    let norm = trimmed.to_ascii_lowercase().replace(['-', ' '], "_");
    for &(id, _, hex) in TRACE_COLOR_PRESETS {
        if norm == id {
            return Some(hex.to_string());
        }
    }

    let mapped = match norm.as_str() {
        "high" | "important" | "urgent" | "warning" | "contradict" | "contradicts" | "red"
        | "rose" => "#E06C75",
        "key" | "main" | "primary" | "orange" | "terracotta" => "#D97757",
        "medium" | "support" | "validates" | "proof" | "green" | "emerald" => "#6AB07A",
        "low" | "minor" | "context" | "background" | "slate" | "gray" | "grey" => "#7A8A9E",
        "concept" | "definition" | "explains" | "explanation" | "science" | "physics" | "blue" => {
            "#5B9BD5"
        }
        "derived" | "summary" | "connection" | "abstract" | "purple" | "violet" => "#A37CE0",
        "case" | "practice" | "quiz" | "application" | "history" | "amber" | "yellow" | "gold" => {
            "#D9A74A"
        }
        "math" | "formula" | "code" | "algorithm" | "biology" | "teal" | "cyan" => "#4DB6AC",
        _ => return None,
    };
    Some(mapped.to_string())
}

/// Optional semantic preset label matching `input` (used to populate `color_category`).
pub fn resolve_trace_color_label(input: &str) -> Option<&'static str> {
    let norm = input.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    for &(id, label, _) in TRACE_COLOR_PRESETS {
        if norm == id {
            return Some(label);
        }
    }
    None
}

/// Default semantic trace color derived from `EdgeRelation` when trace colors are enabled and no override was set.
pub fn default_relation_color_hex(rel: &EdgeRelation) -> Option<&'static str> {
    match rel {
        EdgeRelation::Supports => Some("#6AB07A"),
        EdgeRelation::Contradicts => Some("#E06C75"),
        EdgeRelation::Explains => Some("#5B9BD5"),
        EdgeRelation::DerivedFrom => Some("#A37CE0"),
        EdgeRelation::ExampleOf => Some("#D9A74A"),
        EdgeRelation::RelatesTo | EdgeRelation::Custom(_) => None,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub from: EdgeEndpoint,
    pub to: EdgeEndpoint,
    pub relation: EdgeRelation,
    pub label: String,
    pub note: String,
    pub provenance: EdgeProvenance,
    pub status: EdgeStatus,
    /// When true, the edge text label pill is hidden on canvas to keep diagrams uncluttered.
    #[serde(default)]
    pub hide_label: bool,
    /// Optional explicit `#RRGGBB` trace color assigned by user or AI (relevance / topic).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_hex: Option<String>,
    /// Optional semantic tag explaining why the trace color was chosen (e.g. `"Critical"`, `"Theory"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_category: Option<String>,
}

impl Edge {
    pub fn new(from: EdgeEndpoint, to: EdgeEndpoint) -> Self {
        Self {
            id: EdgeId::new(),
            from,
            to,
            relation: EdgeRelation::RelatesTo,
            label: String::new(),
            note: String::new(),
            provenance: EdgeProvenance::User,
            status: EdgeStatus::Accepted,
            hide_label: false,
            color_hex: None,
            color_category: None,
        }
    }

    pub fn proposed_by_llm(
        from: EdgeEndpoint,
        to: EdgeEndpoint,
        relation: EdgeRelation,
        label: impl Into<String>,
        note: impl Into<String>,
    ) -> Self {
        Self {
            id: EdgeId::new(),
            from,
            to,
            relation,
            label: label.into(),
            note: note.into(),
            provenance: EdgeProvenance::Llm,
            status: EdgeStatus::Proposed,
            hide_label: false,
            color_hex: None,
            color_category: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_hide_label_backward_compat() {
        let legacy_edge_json = r#"{
            "id": "e1",
            "from": { "node": "n1", "port": "out_main" },
            "to": { "node": "n2", "port": "in_main" },
            "relation": "Supports",
            "label": "Validates",
            "note": "",
            "provenance": "User",
            "status": "Accepted"
        }"#;
        let edge: Edge = serde_json::from_str(legacy_edge_json).expect("legacy edge json");
        assert!(!edge.hide_label);
        assert!(edge.color_hex.is_none());
        assert!(edge.color_category.is_none());

        assert_eq!(resolve_trace_color_hex("critical").as_deref(), Some("#E06C75"));
        assert_eq!(resolve_trace_color_hex("Theory").as_deref(), Some("#5B9BD5"));
        assert_eq!(resolve_trace_color_hex("#a1b2c3").as_deref(), Some("#A1B2C3"));
        assert_eq!(default_relation_color_hex(&EdgeRelation::Supports), Some("#6AB07A"));
    }

    #[test]
    fn test_node_meta_ocr_translation_backward_compat() {
        let legacy_meta_json = r#"{
            "answer": "42",
            "mastery": "good",
            "section_framed": false
        }"#;
        let meta: NodeMeta = serde_json::from_str(legacy_meta_json).expect("legacy meta json");
        assert_eq!(meta.answer.as_deref(), Some("42"));
        assert!(meta.extracted_text.is_none());
        assert!(meta.ocr_engine.is_none());
        assert!(meta.translated_text.is_none());
        assert!(meta.target_language.is_none());
        assert_eq!(meta.quiz_mode, QuizDisplayMode::Regular);
        assert_eq!(meta.quiz_difficulty, QuizDifficulty::Easy);

        let populated = NodeMeta {
            extracted_text: Some("E = mc²".into()),
            ocr_engine: Some("Windows Native OCR".into()),
            translated_text: Some("Energía = masa · c²".into()),
            target_language: Some("es".into()),
            quiz_mode: QuizDisplayMode::Popout,
            quiz_difficulty: QuizDifficulty::Hard,
            ..Default::default()
        };
        let encoded = serde_json::to_string(&populated).expect("serialize");
        let decoded: NodeMeta = serde_json::from_str(&encoded).expect("round-trip");
        assert_eq!(decoded, populated);

        let legacy_q_json = r#"{"id":"q1","question":"Prompt?","options":["A","B"],"correct_index":0,"answer":"Exp"}"#;
        let q_dec: QuizQuestion = serde_json::from_str(legacy_q_json).expect("legacy quiz q");
        assert!(q_dec.question_image_hash.is_none());
        assert!(q_dec.answer_image_hash.is_none());
    }

    #[test]
    fn test_graph_formula_kind_and_meta_round_trip() {
        let mut node = Node::new(
            NodeKind::GraphFormula,
            "Graphic Formula",
            WorldPos::new(10.0, 20.0),
        );
        node.content = "y = sin(x)".into();
        node.meta.graph_center_x = Some(1.5);
        node.meta.graph_center_y = Some(-0.5);
        node.meta.graph_span_x = Some(8.0);

        let encoded = serde_json::to_string(&node).expect("serialize GraphFormula");
        let decoded: Node = serde_json::from_str(&encoded).expect("deserialize GraphFormula");
        assert_eq!(decoded.kind, NodeKind::GraphFormula);
        assert_eq!(decoded.meta.graph_center_x, Some(1.5));
        assert_eq!(decoded.meta.graph_center_y, Some(-0.5));
        assert_eq!(decoded.meta.graph_span_x, Some(8.0));

        let legacy_meta: NodeMeta = serde_json::from_str("{}").expect("legacy empty meta");
        assert!(legacy_meta.graph_center_x.is_none());
        assert!(legacy_meta.graph_center_y.is_none());
        assert!(legacy_meta.graph_span_x.is_none());
    }
}
