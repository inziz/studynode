use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::coords::{Camera, WorldPos};
use crate::model::{Edge, EdgeId, EdgeStatus, Node, NodeId, PortId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user" or "assistant"
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_summary: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatThread {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudyGraph {
    pub schema_version: u32,
    pub project_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    pub viewport: Camera,
    pub nodes: HashMap<NodeId, Node>,
    pub edges: HashMap<EdgeId, Edge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chat_threads: Vec<ChatThread>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_chat_id: Option<String>,
}

impl Default for StudyGraph {
    fn default() -> Self {
        Self::new("Untitled Project")
    }
}

impl StudyGraph {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    pub fn new(title: impl Into<String>) -> Self {
        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            project_id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            collection: None,
            viewport: Camera::default(),
            nodes: HashMap::new(),
            edges: HashMap::new(),
            chat_threads: Vec::new(),
            active_chat_id: None,
        }
    }

    pub fn add_node(&mut self, node: Node) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn get_node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn get_node_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    /// Removes a node and all incident edges connected to it.
    pub fn remove_node(&mut self, id: &NodeId) -> Option<(Node, Vec<Edge>)> {
        let removed_node = self.nodes.remove(id)?;
        let mut removed_edges = Vec::new();

        self.edges.retain(|_, edge| {
            if &edge.from.node == id || &edge.to.node == id {
                removed_edges.push(edge.clone());
                false
            } else {
                true
            }
        });

        Some((removed_node, removed_edges))
    }

    pub fn add_edge(&mut self, edge: Edge) {
        self.edges.insert(edge.id.clone(), edge);
    }

    pub fn get_edge(&self, id: &EdgeId) -> Option<&Edge> {
        self.edges.get(id)
    }

    pub fn get_edge_mut(&mut self, id: &EdgeId) -> Option<&mut Edge> {
        self.edges.get_mut(id)
    }

    pub fn remove_edge(&mut self, id: &EdgeId) -> Option<Edge> {
        self.edges.remove(id)
    }

    /// Find incident edges (both incoming and outgoing) for a node.
    pub fn incident_edges(&self, node_id: &NodeId) -> Vec<&Edge> {
        self.edges
            .values()
            .filter(|e| &e.from.node == node_id || &e.to.node == node_id)
            .collect()
    }

    /// Find 1-hop neighbor node IDs for a given node.
    pub fn neighbors(&self, node_id: &NodeId) -> Vec<NodeId> {
        let mut set = std::collections::HashSet::new();
        for edge in self.edges.values() {
            if &edge.from.node == node_id {
                set.insert(edge.to.node.clone());
            } else if &edge.to.node == node_id {
                set.insert(edge.from.node.clone());
            }
        }
        set.into_iter().collect()
    }

    /// Extract a bounded 1-hop subgraph centered around a target node (used for LLM context budgeting).
    pub fn subgraph_1hop(&self, target: &NodeId) -> Option<StudyGraph> {
        let center = self.nodes.get(target)?;
        let mut sub = StudyGraph::new(format!("1-hop context for {}", center.title));
        sub.add_node(center.clone());

        for edge in self.incident_edges(target) {
            sub.add_edge(edge.clone());
            let neighbor_id = if &edge.from.node == target {
                &edge.to.node
            } else {
                &edge.from.node
            };
            if let Some(neighbor) = self.nodes.get(neighbor_id) {
                sub.add_node(neighbor.clone());
            }
        }

        Some(sub)
    }

    /// Find which node (if any) contains the given world coordinates.
    pub fn hit_test_node(&self, world_pt: WorldPos) -> Option<NodeId> {
        // Iterate in reverse or check all (could be enhanced with z-order)
        for (id, node) in &self.nodes {
            let (w, h) = node.size;
            if world_pt.x >= node.pos.x
                && world_pt.x <= node.pos.x + w
                && world_pt.y >= node.pos.y
                && world_pt.y <= node.pos.y + h
            {
                return Some(id.clone());
            }
        }
        None
    }

    /// Find which port on which node is closest within `radius` of the given world coordinates.
    pub fn hit_test_port(&self, world_pt: WorldPos, radius: f32) -> Option<(NodeId, PortId)> {
        for (node_id, node) in &self.nodes {
            for port in &node.ports {
                if let Some(port_pos) = node.port_world_pos(&port.id) {
                    if port_pos.distance_to(&world_pt) <= radius {
                        return Some((node_id.clone(), port.id.clone()));
                    }
                }
            }
        }
        None
    }

    /// Accept a proposed "Ghost Edge" created by LLM.
    pub fn accept_edge(&mut self, edge_id: &EdgeId) -> bool {
        if let Some(edge) = self.edges.get_mut(edge_id) {
            edge.status = EdgeStatus::Accepted;
            true
        } else {
            false
        }
    }

    /// Reject/dismiss a proposed "Ghost Edge" created by LLM.
    pub fn reject_edge(&mut self, edge_id: &EdgeId) -> bool {
        self.edges.remove(edge_id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeEndpoint, NodeKind};

    #[test]
    fn test_node_removal_cascades_edges() {
        let mut g = StudyGraph::new("Test");
        let n1 = Node::new(NodeKind::Document, "Doc A", WorldPos::new(0.0, 0.0));
        let n2 = Node::new(NodeKind::Note, "Note B", WorldPos::new(200.0, 0.0));
        let id1 = n1.id.clone();
        let id2 = n2.id.clone();

        g.add_node(n1);
        g.add_node(n2);

        let edge = Edge::new(
            EdgeEndpoint {
                node: id1.clone(),
                port: PortId::new(PortId::DEFAULT_OUT),
            },
            EdgeEndpoint {
                node: id2.clone(),
                port: PortId::new(PortId::DEFAULT_IN),
            },
        );
        let edge_id = edge.id.clone();
        g.add_edge(edge);

        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);

        // Remove node 1, edge must be cascade deleted
        let (removed, cascade_edges) = g.remove_node(&id1).unwrap();
        assert_eq!(removed.id, id1);
        assert_eq!(cascade_edges.len(), 1);
        assert_eq!(cascade_edges[0].id, edge_id);
        assert_eq!(g.edges.len(), 0);
        assert_eq!(g.nodes.len(), 1);
    }

    #[test]
    fn test_chat_threads_serde_and_backward_compat() {
        let mut g = StudyGraph::new("Per Project Chat");
        g.chat_threads.push(ChatThread {
            id: "chat-1".into(),
            title: "Cell Cycle".into(),
            messages: vec![
                ChatMessage {
                    role: "user".into(),
                    text: "Summarize mitosis".into(),
                    action_summary: None,
                },
                ChatMessage {
                    role: "assistant".into(),
                    text: "PMAT phases".into(),
                    action_summary: Some("Added 1 note".into()),
                },
            ],
        });
        g.active_chat_id = Some("chat-1".into());
        let json = serde_json::to_string(&g).unwrap();
        let restored: StudyGraph = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.active_chat_id.as_deref(), Some("chat-1"));
        assert_eq!(restored.chat_threads.len(), 1);
        assert_eq!(restored.chat_threads[0].messages.len(), 2);

        // Legacy graph.json without chat_threads / active_chat_id must still load
        let legacy_json = serde_json::to_string(&StudyGraph::new("Legacy")).unwrap();
        assert!(!legacy_json.contains("chat_threads"));
        let legacy: StudyGraph = serde_json::from_str(&legacy_json).unwrap();
        assert!(legacy.chat_threads.is_empty());
        assert_eq!(legacy.active_chat_id, None);
    }
}
