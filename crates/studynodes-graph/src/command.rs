use std::collections::HashMap;
use crate::coords::WorldPos;
use crate::graph::StudyGraph;
use crate::model::{Edge, EdgeId, Node, NodeId};

#[derive(Clone, Debug)]
pub enum GraphCommand {
    AddNode(Node),
    DeleteNode {
        node: Node,
        cascade_edges: Vec<Edge>,
    },
    MoveNodes {
        /// Map from NodeId -> (old_position, new_position)
        moves: HashMap<NodeId, (WorldPos, WorldPos)>,
    },
    AddEdge(Edge),
    DeleteEdge(Edge),
    UpdateAnnotation {
        edge_id: EdgeId,
        old_label: String,
        new_label: String,
        old_note: String,
        new_note: String,
    },
    Batch(Vec<GraphCommand>),
}

impl GraphCommand {
    /// Execute or apply this command forward to the graph.
    pub fn apply(&self, graph: &mut StudyGraph) {
        match self {
            Self::AddNode(node) => {
                graph.add_node(node.clone());
            }
            Self::DeleteNode { node, .. } => {
                graph.remove_node(&node.id);
            }
            Self::MoveNodes { moves } => {
                for (id, (_, new_pos)) in moves {
                    if let Some(node) = graph.get_node_mut(id) {
                        node.pos = *new_pos;
                    }
                }
            }
            Self::AddEdge(edge) => {
                graph.add_edge(edge.clone());
            }
            Self::DeleteEdge(edge) => {
                graph.remove_edge(&edge.id);
            }
            Self::UpdateAnnotation {
                edge_id,
                new_label,
                new_note,
                ..
            } => {
                if let Some(edge) = graph.get_edge_mut(edge_id) {
                    edge.label = new_label.clone();
                    edge.note = new_note.clone();
                }
            }
            Self::Batch(cmds) => {
                for cmd in cmds {
                    cmd.apply(graph);
                }
            }
        }
    }

    /// Revert or undo this command on the graph.
    pub fn undo(&self, graph: &mut StudyGraph) {
        match self {
            Self::AddNode(node) => {
                graph.remove_node(&node.id);
            }
            Self::DeleteNode { node, cascade_edges } => {
                graph.add_node(node.clone());
                for edge in cascade_edges {
                    graph.add_edge(edge.clone());
                }
            }
            Self::MoveNodes { moves } => {
                for (id, (old_pos, _)) in moves {
                    if let Some(node) = graph.get_node_mut(id) {
                        node.pos = *old_pos;
                    }
                }
            }
            Self::AddEdge(edge) => {
                graph.remove_edge(&edge.id);
            }
            Self::DeleteEdge(edge) => {
                graph.add_edge(edge.clone());
            }
            Self::UpdateAnnotation {
                edge_id,
                old_label,
                old_note,
                ..
            } => {
                if let Some(edge) = graph.get_edge_mut(edge_id) {
                    edge.label = old_label.clone();
                    edge.note = old_note.clone();
                }
            }
            Self::Batch(cmds) => {
                for cmd in cmds.iter().rev() {
                    cmd.undo(graph);
                }
            }
        }
    }
}

/// Manages undo and redo stacks for a graph session.
#[derive(Default)]
pub struct CommandHistory {
    undo_stack: Vec<GraphCommand>,
    redo_stack: Vec<GraphCommand>,
}

impl CommandHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn execute(&mut self, cmd: GraphCommand, graph: &mut StudyGraph) {
        cmd.apply(graph);
        self.undo_stack.push(cmd);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, graph: &mut StudyGraph) -> bool {
        if let Some(cmd) = self.undo_stack.pop() {
            cmd.undo(graph);
            self.redo_stack.push(cmd);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, graph: &mut StudyGraph) -> bool {
        if let Some(cmd) = self.redo_stack.pop() {
            cmd.apply(graph);
            self.undo_stack.push(cmd);
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NodeKind;

    #[test]
    fn test_undo_redo_add_node() {
        let mut g = StudyGraph::new("Test");
        let mut history = CommandHistory::new();

        let n = Node::new(NodeKind::Note, "Undoable Note", WorldPos::new(10.0, 20.0));
        let id = n.id.clone();

        history.execute(GraphCommand::AddNode(n), &mut g);
        assert_eq!(g.nodes.len(), 1);

        assert!(history.undo(&mut g));
        assert_eq!(g.nodes.len(), 0);

        assert!(history.redo(&mut g));
        assert_eq!(g.nodes.len(), 1);
        assert!(g.get_node(&id).is_some());
    }
}
