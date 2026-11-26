pub mod bezier;
pub mod bundle;
pub mod command;
pub mod coords;
pub mod graph;
pub mod model;
pub mod patch;

pub use bezier::{point_to_segment_distance, CubicBezier};
pub use bundle::{BundleError, ProjectBundle};
pub use command::{CommandHistory, GraphCommand};
pub use coords::{Camera, ScreenPos, WorldPos};
pub use graph::{ChatMessage, ChatThread, StudyGraph};
pub use model::{
    default_relation_color_hex, resolve_trace_color_hex, resolve_trace_color_label, DrawingStroke,
    Edge, EdgeEndpoint, EdgeId, EdgeProvenance, EdgeRelation, EdgeStatus, Node, NodeId, NodeKind,
    Port, PortDirection, PortId, PortType, QuizDifficulty, QuizDisplayMode, QuizQuestion,
    TRACE_COLOR_PRESETS,
};
pub use patch::{GraphPatch, PatchOp, SemanticRelation, StagedPatchResult};
