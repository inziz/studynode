use serde::{Deserialize, Serialize};
use studynodes_graph::model::NodeId;
use studynodes_graph::StudyGraph;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusedNodeContext {
    pub node_id: String,
    pub title: String,
    pub kind: String,
    pub content: String,
    pub summary: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborNodeContext {
    pub node_id: String,
    pub title: String,
    pub kind: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingEdgeContext {
    pub edge_id: String,
    pub from: String,
    pub to: String,
    pub relation: String,
    pub label: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmContext {
    pub task: String,
    pub focus_nodes: Vec<FocusedNodeContext>,
    pub neighbor_nodes: Vec<NeighborNodeContext>,
    pub existing_edges: Vec<ExistingEdgeContext>,
}

impl LlmContext {
    /// Build bounded neighborhood context around a set of selected focus nodes.
    pub fn build(graph: &StudyGraph, focus_ids: &[NodeId], task: impl Into<String>) -> Self {
        let mut focus_nodes = Vec::new();
        let mut neighbor_nodes = Vec::new();
        let mut existing_edges = Vec::new();
        let mut neighbor_id_set = std::collections::HashSet::new();
        let mut edge_id_set = std::collections::HashSet::new();

        // 1. Pack focus nodes
        for id in focus_ids {
            if let Some(n) = graph.get_node(id) {
                focus_nodes.push(FocusedNodeContext {
                    node_id: n.id.as_str().to_string(),
                    title: n.title.clone(),
                    kind: n.kind.display_name().to_string(),
                    content: n.content.clone(),
                    summary: n.summary.clone(),
                    tags: n.tags.clone(),
                });

                // 2. Discover 1-hop neighbors
                for neighbor_id in graph.neighbors(id) {
                    if !focus_ids.contains(&neighbor_id) && neighbor_id_set.insert(neighbor_id.clone()) {
                        if let Some(nbr) = graph.get_node(&neighbor_id) {
                            neighbor_nodes.push(NeighborNodeContext {
                                node_id: nbr.id.as_str().to_string(),
                                title: nbr.title.clone(),
                                kind: nbr.kind.display_name().to_string(),
                                summary: nbr.summary.clone(),
                            });
                        }
                    }
                }

                // 3. Collect incident edges
                for edge in graph.incident_edges(id) {
                    if edge_id_set.insert(edge.id.clone()) {
                        existing_edges.push(ExistingEdgeContext {
                            edge_id: edge.id.as_str().to_string(),
                            from: edge.from.node.as_str().to_string(),
                            to: edge.to.node.as_str().to_string(),
                            relation: edge.relation.display_label().to_string(),
                            label: edge.label.clone(),
                            note: edge.note.clone(),
                        });
                    }
                }
            }
        }

        Self {
            task: task.into(),
            focus_nodes,
            neighbor_nodes,
            existing_edges,
        }
    }
}
