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
    /// Interactive task/study-checklist card backed by Markdown checkboxes
    Checklist,
    /// Numbered procedural or causal step card with chained spawning
    Sequence,
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
            Self::Checklist => "Checklist",
            Self::Sequence => "Sequence",
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
            Self::Checklist => "#9a9993",    // Muted task checklist
            Self::Sequence => "#ecebe6",     // Bright ordinal step
        }
    }
}

/// Optional metadata extending specialized Phase 2 study nodes (`Flashcard`, `Excerpt`, `Formula`, `Sequence`, `Section`).
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
    }
}
