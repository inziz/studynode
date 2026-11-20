use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::coords::WorldPos;
use crate::graph::StudyGraph;
use crate::model::{
    Edge, EdgeEndpoint, EdgeRelation, Node, NodeId, NodeKind, PortId,
};
#[cfg(test)]
use crate::model::{EdgeProvenance, EdgeStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticRelation {
    Supports,
    Contradicts,
    DerivedFrom,
    Elaborates,
    PrerequisiteFor,
    ExampleOf,
    Custom,
}

impl From<SemanticRelation> for EdgeRelation {
    fn from(r: SemanticRelation) -> Self {
        match r {
            SemanticRelation::Supports => EdgeRelation::Supports,
            SemanticRelation::Contradicts => EdgeRelation::Contradicts,
            SemanticRelation::DerivedFrom => EdgeRelation::DerivedFrom,
            SemanticRelation::Elaborates => EdgeRelation::RelatesTo,
            SemanticRelation::PrerequisiteFor => EdgeRelation::Custom("prerequisite for".into()),
            SemanticRelation::ExampleOf => EdgeRelation::ExampleOf,
            SemanticRelation::Custom => EdgeRelation::RelatesTo,
        }
    }
}

/// A validated, atomic batch of AI-proposed graph mutations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphPatch {
    /// Short high-level explanation of what this patch accomplishes (shown in UI toast/panel).
    pub summary: String,
    /// Ordered list of operations. `CreateSynthesisNode` must precede edges referencing its `temp_id`.
    pub ops: Vec<PatchOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PatchOp {
    /// Creates a "Ghost Edge" (status = Proposed, provenance = Llm) between two nodes.
    ProposeConnection {
        from_node: String,
        to_node: String,
        relation: SemanticRelation,
        label: String,
        note: String,
        #[serde(default)]
        evidence: Vec<String>,
        confidence: f32,
    },

    /// Enriches or refines an existing edge's annotation without overwriting user-locked notes.
    AnnotateEdge {
        edge_id: String,
        #[serde(default)]
        relation: Option<SemanticRelation>,
        #[serde(default)]
        label: Option<String>,
        suggested_note: String,
    },

    /// Synthesizes multiple nodes into a new concept/summary node.
    CreateSynthesisNode {
        temp_id: String,
        title: String,
        markdown_body: String,
        source_nodes: Vec<String>,
    },

    /// Populates or updates the concise summary field on an existing node.
    UpdateNodeSummary {
        node_id: String,
        summary: String,
    },
}

#[derive(Debug, Default)]
pub struct StagedPatchResult {
    pub new_nodes: Vec<Node>,
    pub proposed_edges: Vec<Edge>,
    pub updated_nodes: Vec<(NodeId, String)>,
    pub dropped_ops_count: usize,
}

impl StudyGraph {
    /// Validates a `GraphPatch` against the current graph topology, resolving temp IDs and
    /// filtering out any hallucinated IDs or duplicate edges.
    pub fn validate_and_stage_patch(&mut self, patch: &GraphPatch) -> StagedPatchResult {
        let mut result = StagedPatchResult::default();
        let mut temp_to_real: HashMap<String, NodeId> = HashMap::new();

        for op in &patch.ops {
            match op {
                PatchOp::CreateSynthesisNode {
                    temp_id,
                    title,
                    markdown_body,
                    source_nodes,
                } => {
                    // Compute centroid of valid source nodes
                    let mut sum_x = 0.0;
                    let mut sum_y = 0.0;
                    let mut valid_count = 0;

                    for src in source_nodes {
                        let id = temp_to_real
                            .get(src)
                            .cloned()
                            .unwrap_or_else(|| NodeId::from_str(src));
                        if let Some(n) = self.nodes.get(&id) {
                            sum_x += n.pos.x;
                            sum_y += n.pos.y;
                            valid_count += 1;
                        }
                    }

                    let spawn_pos = if valid_count > 0 {
                        WorldPos::new(
                            sum_x / valid_count as f32,
                            sum_y / valid_count as f32 + 220.0,
                        )
                    } else {
                        WorldPos::new(100.0, 100.0)
                    };

                    let mut node = Node::new(NodeKind::LlmSynthesis, title, spawn_pos);
                    node.content = markdown_body.clone();
                    temp_to_real.insert(temp_id.clone(), node.id.clone());

                    self.add_node(node.clone());
                    result.new_nodes.push(node);
                }

                PatchOp::ProposeConnection {
                    from_node,
                    to_node,
                    relation,
                    label,
                    note,
                    confidence,
                    ..
                } => {
                    // Discard low confidence proposals
                    if *confidence < 0.5 {
                        result.dropped_ops_count += 1;
                        continue;
                    }

                    let resolved_from = temp_to_real
                        .get(from_node)
                        .cloned()
                        .unwrap_or_else(|| NodeId::from_str(from_node));
                    let resolved_to = temp_to_real
                        .get(to_node)
                        .cloned()
                        .unwrap_or_else(|| NodeId::from_str(to_node));

                    // Verify both endpoints exist
                    if !self.nodes.contains_key(&resolved_from)
                        || !self.nodes.contains_key(&resolved_to)
                        || resolved_from == resolved_to
                    {
                        result.dropped_ops_count += 1;
                        continue;
                    }

                    let edge_rel: EdgeRelation = relation.clone().into();

                    // Check deduplication
                    let exists = self.edges.values().any(|e| {
                        (e.from.node == resolved_from && e.to.node == resolved_to)
                            || (e.from.node == resolved_to && e.to.node == resolved_from)
                    });

                    if exists {
                        result.dropped_ops_count += 1;
                        continue;
                    }

                    let edge = Edge::proposed_by_llm(
                        EdgeEndpoint {
                            node: resolved_from,
                            port: PortId::new(PortId::DEFAULT_OUT),
                        },
                        EdgeEndpoint {
                            node: resolved_to,
                            port: PortId::new(PortId::DEFAULT_IN),
                        },
                        edge_rel,
                        label.clone(),
                        note.clone(),
                    );

                    self.add_edge(edge.clone());
                    result.proposed_edges.push(edge);
                }

                PatchOp::AnnotateEdge {
                    edge_id,
                    relation,
                    label,
                    suggested_note,
                } => {
                    let eid = crate::model::EdgeId::from_str(edge_id);
                    if let Some(edge) = self.get_edge_mut(&eid) {
                        if let Some(r) = relation {
                            edge.relation = r.clone().into();
                        }
                        if let Some(l) = label {
                            edge.label = l.clone();
                        }
                        if edge.note.is_empty() {
                            edge.note = suggested_note.clone();
                        } else {
                            edge.note.push_str(&format!("\n[AI Note]: {}", suggested_note));
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::UpdateNodeSummary { node_id, summary } => {
                    let nid = NodeId::from_str(node_id);
                    if let Some(node) = self.get_node_mut(&nid) {
                        node.summary = Some(summary.clone());
                        result.updated_nodes.push((nid, summary.clone()));
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_staging_with_temp_ids() {
        let mut g = StudyGraph::new("Patch Test");
        let n1 = Node::new(NodeKind::Document, "Paper 1", WorldPos::new(0.0, 0.0));
        let n2 = Node::new(NodeKind::Document, "Paper 2", WorldPos::new(200.0, 0.0));
        let id1 = n1.id.as_str().to_string();
        let id2 = n2.id.as_str().to_string();
        g.add_node(n1);
        g.add_node(n2);

        let patch = GraphPatch {
            summary: "Synthesize and connect papers".into(),
            ops: vec![
                PatchOp::CreateSynthesisNode {
                    temp_id: "tmp_1".into(),
                    title: "Comparative Analysis".into(),
                    markdown_body: "Both papers explore synaptic plasticity...".into(),
                    source_nodes: vec![id1.clone(), id2.clone()],
                },
                PatchOp::ProposeConnection {
                    from_node: id1.clone(),
                    to_node: "tmp_1".into(),
                    relation: SemanticRelation::DerivedFrom,
                    label: "Basis for review".into(),
                    note: "Provides foundation".into(),
                    evidence: vec![],
                    confidence: 0.9,
                },
                PatchOp::ProposeConnection {
                    from_node: "hallucinated_node".into(),
                    to_node: id2.clone(),
                    relation: SemanticRelation::Contradicts,
                    label: "Invalid link".into(),
                    note: "".into(),
                    evidence: vec![],
                    confidence: 0.9,
                },
            ],
        };

        let result = g.validate_and_stage_patch(&patch);

        assert_eq!(result.new_nodes.len(), 1);
        assert_eq!(result.proposed_edges.len(), 1);
        assert_eq!(result.dropped_ops_count, 1); // The hallucinated node link was dropped cleanly
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.edges.len(), 1);

        // Verify proposed edge status
        let edge = result.proposed_edges.first().unwrap();
        assert_eq!(edge.status, EdgeStatus::Proposed);
        assert_eq!(edge.provenance, EdgeProvenance::Llm);
    }
}
