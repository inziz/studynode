pub mod client;
pub mod context;
pub mod prompt;

pub use client::{GeminiLlmProvider, LlmError, LlmProvider, MockLlmProvider};
pub use context::{ExistingEdgeContext, FocusedNodeContext, LlmContext, NeighborNodeContext};
pub use prompt::SYSTEM_PROMPT;

#[cfg(test)]
mod tests {
    use super::*;
    use studynodes_graph::coords::WorldPos;
    use studynodes_graph::model::{Node, NodeKind};
    use studynodes_graph::StudyGraph;

    #[tokio::test]
    async fn test_mock_llm_generates_valid_patch() {
        let mut graph = StudyGraph::new("Test");
        let n1 = Node::new(NodeKind::Document, "Dopamine & Learning", WorldPos::new(0.0, 0.0));
        let n2 = Node::new(NodeKind::Document, "Reward Prediction Error", WorldPos::new(300.0, 0.0));
        let id1 = n1.id.clone();
        let id2 = n2.id.clone();

        graph.add_node(n1);
        graph.add_node(n2);

        let context = LlmContext::build(&graph, &[id1.clone(), id2.clone()], "Find Connections");
        let provider = MockLlmProvider;
        let patch = provider.generate_patch(&context).await.unwrap();

        assert!(!patch.ops.is_empty());
        let result = graph.validate_and_stage_patch(&patch);
        assert!(!result.proposed_edges.is_empty());
        assert!(!result.new_nodes.is_empty());
    }

    #[tokio::test]
    async fn test_mock_llm_generates_study_cards() {
        let mut graph = StudyGraph::new("Study Cards Test");
        let mut doc = Node::new(NodeKind::Document, "Action Potential", WorldPos::new(0.0, 0.0));
        doc.content = "Voltage-gated Na+ channels drive rapid depolarization above -55 mV.".into();
        let doc_id = doc.id.clone();
        graph.add_node(doc);

        let context = LlmContext::build(&graph, &[doc_id], "Generate Study Cards");
        let provider = MockLlmProvider;
        let patch = provider.generate_patch(&context).await.unwrap();
        let result = graph.validate_and_stage_patch(&patch);

        assert_eq!(result.new_nodes.len(), 2);
        assert_eq!(result.new_nodes[0].kind, NodeKind::Flashcard);
        assert!(result.new_nodes[0].meta.answer.is_some());
        assert_eq!(result.new_nodes[1].kind, NodeKind::Checklist);
        assert_eq!(result.proposed_edges.len(), 1);
    }
}
