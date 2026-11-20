use studynodes_graph::coords::WorldPos;
use studynodes_graph::model::{EdgeRelation, EdgeStatus, NodeKind};
use studynodes_graph::ProjectBundle;
use studynodes_llm::{LlmContext, LlmProvider, MockLlmProvider};

// We test against studynodes-app and graph logic
#[test]
fn test_full_studynodes_lifecycle() {
    let temp_dir = tempfile::tempdir().unwrap();
    let projects_dir = temp_dir.path().to_path_buf();

    // 1. Create a Project Bundle
    let (bundle, mut graph) = ProjectBundle::create(
        &projects_dir,
        "neuro_study.studynodes",
        "Neuroscience of Learning",
    )
    .unwrap();

    assert_eq!(graph.title, "Neuroscience of Learning");
    assert!(bundle.root_dir.join(ProjectBundle::PROJECT_FILE).exists());

    // 2. Ingest Multi-modal Content (Documents & Notes)
    let note1_path = temp_dir.path().join("hebbian_theory.md");
    std::fs::write(
        &note1_path,
        "# Hebbian Plasticity\nNeurons that fire together wire together.",
    )
    .unwrap();

    let (hash1, _) = bundle.ingest_asset(&note1_path).unwrap();
    let mut n1 = studynodes_graph::model::Node::new(
        NodeKind::Document,
        "Hebbian Theory (1949)",
        WorldPos::new(100.0, 150.0),
    );
    n1.content = "Neurons that fire together wire together.".into();
    n1.asset_hash = Some(hash1);
    let id1 = n1.id.clone();
    graph.add_node(n1);

    let mut n2 = studynodes_graph::model::Node::new(
        NodeKind::Note,
        "LTP Clinical Observation",
        WorldPos::new(500.0, 150.0),
    );
    n2.content = "Long-Term Potentiation observed in hippocampal slices.".into();
    let id2 = n2.id.clone();
    graph.add_node(n2);

    assert_eq!(graph.nodes.len(), 2);

    // 3. Connect Nodes with Annotated Bézier Wire
    let mut edge = studynodes_graph::model::Edge::new(
        studynodes_graph::model::EdgeEndpoint {
            node: id1.clone(),
            port: studynodes_graph::model::PortId::new("out_main"),
        },
        studynodes_graph::model::EdgeEndpoint {
            node: id2.clone(),
            port: studynodes_graph::model::PortId::new("in_main"),
        },
    );
    edge.relation = EdgeRelation::Supports;
    edge.label = "Biochemical Proof".into();
    edge.note = "Bliss & Lomo (1973) validated Hebb's hypothesis via rabbit hippocampus stimulation.".into();
    let edge_id = edge.id.clone();
    graph.add_edge(edge);

    assert_eq!(graph.edges.len(), 1);

    // 4. Run LLM Synthesis & Relationship Discovery
    let context = LlmContext::build(&graph, &[id1.clone(), id2.clone()], "Synthesize and connect");
    assert_eq!(context.focus_nodes.len(), 2);
    assert_eq!(context.existing_edges.len(), 1);

    let provider = MockLlmProvider;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let patch = rt.block_on(provider.generate_patch(&context)).unwrap();

    assert!(!patch.ops.is_empty());

    // 5. Apply Patch and Verify Ghost Edge Creation
    let staged = graph.validate_and_stage_patch(&patch);
    assert!(!staged.new_nodes.is_empty(), "AI created synthesis node");
    assert!(!staged.proposed_edges.is_empty(), "AI proposed Ghost Edges");

    for edge in &staged.proposed_edges {
        assert_eq!(edge.status, EdgeStatus::Proposed);
    }

    // 6. Accept the Proposed Ghost Edge
    let proposed_id = staged.proposed_edges[0].id.clone();
    assert!(graph.accept_edge(&proposed_id));
    assert_eq!(graph.get_edge(&proposed_id).unwrap().status, EdgeStatus::Accepted);

    // 7. Atomic Persistence Roundtrip
    bundle.save_graph_atomic(&graph).unwrap();

    let (_, loaded_graph) = ProjectBundle::open(&bundle.root_dir).unwrap();
    assert_eq!(loaded_graph.title, "Neuroscience of Learning");
    assert_eq!(loaded_graph.nodes.len(), graph.nodes.len());
    assert_eq!(loaded_graph.edges.len(), graph.edges.len());
    assert_eq!(
        loaded_graph.get_edge(&edge_id).unwrap().label,
        "Biochemical Proof"
    );
}
