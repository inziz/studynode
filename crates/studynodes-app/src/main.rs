#![forbid(unsafe_code)]

mod canvas_view;
mod dashboard_view;
mod inspector_view;
mod state;

use rune::tok::{self, space, type_};
use rune::{AppSpec, Color, Cx, El, col, el, row, text};

use crate::canvas_view::render_canvas_view;
use crate::dashboard_view::render_dashboard;
use crate::inspector_view::render_inspector;
use crate::state::{AppScreen, AppState};

fn view(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let t = tok::theme(true);
    let border = Color { r: 0.20, g: 0.22, b: 0.27, a: 1.0 };
    let status_bar_bg = Color { r: 0.09, g: 0.10, b: 0.12, a: 1.0 };

    let main_content = match s.screen {
        AppScreen::Dashboard => render_dashboard(s, cx),
        AppScreen::Workspace => {
            let canvas = render_canvas_view(s, cx);
            let mut workspace_layout = row().w_full().grow(1.0).child(canvas);

            if s.inspector_open {
                let inspector = render_inspector(s, cx);
                workspace_layout = workspace_layout.child(inspector);
            }

            workspace_layout
        }
    };

    // Global bottom status bar
    let status_bar = row()
        .w_full()
        .padding(space::S2)
        .gap(space::S3)
        .fill(status_bar_bg)
        .border(1.0, border)
        .child(
            text("StudyNodes v0.1.0")
                .font(type_::LABEL)
                .color(t.text_secondary),
        )
        .child(text("│").font(type_::LABEL).color(border))
        .child(
            text(&s.status_message)
                .font(type_::LABEL)
                .color(Color { r: 0.38, g: 0.60, b: 1.0, a: 1.0 }),
        )
        .child(el().grow(1.0))
        .child(
            text(match s.screen {
                AppScreen::Dashboard => "Dashboard View".into(),
                AppScreen::Workspace => format!("Workspace: {}", s.graph.title),
            })
            .font(type_::LABEL)
            .color(t.text_secondary.alpha(0.6)),
        );

    col().w_full().h_full().child(main_content).child(status_bar)
}

fn main() {
    rune::run(AppSpec::<AppState> {
        scenes: &[
            ("dashboard", |_s| {}),
            ("workspace", |s| {
                s.create_project("Cognitive Neuroscience");
                let mut n1 = studynodes_graph::model::Node::new(
                    studynodes_graph::model::NodeKind::Document,
                    "Hebbian Learning (1949)",
                    studynodes_graph::coords::WorldPos::new(80.0, 120.0),
                );
                n1.content = "Neurons that fire together wire together. Synaptic weight increases when pre- and post-synaptic neurons fire synchronously.".into();
                let id1 = n1.id.clone();
                s.graph.add_node(n1);

                let mut n2 = studynodes_graph::model::Node::new(
                    studynodes_graph::model::NodeKind::Note,
                    "LTP Mechanism",
                    studynodes_graph::coords::WorldPos::new(480.0, 120.0),
                );
                n2.content = "Long-Term Potentiation validates Hebb's law biochemically through NMDA receptor channel activation.".into();
                let id2 = n2.id.clone();
                s.graph.add_node(n2);

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
                edge.relation = studynodes_graph::model::EdgeRelation::Supports;
                edge.label = "Validates hypothesis".into();
                edge.note = "Biochemical validation through NMDA activation".into();
                s.graph.add_edge(edge);

                // Add an AI Ghost Edge and Synthesis node
                let mut synth = studynodes_graph::model::Node::new(
                    studynodes_graph::model::NodeKind::LlmSynthesis,
                    "Synthesis: Memory Consolidation",
                    studynodes_graph::coords::WorldPos::new(280.0, 360.0),
                );
                synth.content = "Cross-document synthesis: Hebbian plasticity serves as the cellular basis for systemic memory consolidation in the neocortex.".into();
                let synth_id = synth.id.clone();
                s.graph.add_node(synth);

                let ghost_edge = studynodes_graph::model::Edge::proposed_by_llm(
                    studynodes_graph::model::EdgeEndpoint {
                        node: id1,
                        port: studynodes_graph::model::PortId::new("out_main"),
                    },
                    studynodes_graph::model::EdgeEndpoint {
                        node: synth_id,
                        port: studynodes_graph::model::PortId::new("in_main"),
                    },
                    studynodes_graph::model::EdgeRelation::DerivedFrom,
                    "Cellular foundation",
                    "Provides cellular substrate",
                );
                s.graph.add_edge(ghost_edge);
                s.status_message = "Loaded project 'Cognitive Neuroscience' with 3 nodes & AI Ghost connection".into();
            }),
        ],
        ..AppSpec::new(
            "StudyNodes - Visual Knowledge Canvas",
            (1280.0, 800.0),
            AppState::default,
            view,
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};
    use studynodes_graph::coords::WorldPos;
    use studynodes_graph::model::NodeKind;

    fn make_test_runtime() -> Runtime<AppState> {
        let env = Env {
            system_dark: true,
            scale: 2.0,
        };
        let mut rt = Runtime::new(AppState::default(), view, (1280.0, 800.0), env);
        rt.frame(0.0);
        rt
    }

    #[test]
    fn test_dashboard_to_workspace_transition() {
        let mut rt = make_test_runtime();
        assert_eq!(rt.state.screen, AppScreen::Dashboard);

        // Create a new study project
        rt.state.create_project("Neuroscience 101");
        rt.frame(1.0 / 60.0);

        assert_eq!(rt.state.screen, AppScreen::Workspace);
        assert_eq!(rt.state.graph.title, "Neuroscience 101");
    }

    #[test]
    fn test_node_creation_and_edge_linking() {
        let mut rt = make_test_runtime();
        rt.state.create_project("Cognitive Science");

        let n1 = studynodes_graph::model::Node::new(
            NodeKind::Document,
            "Hebb 1949",
            WorldPos::new(100.0, 100.0),
        );
        let n2 = studynodes_graph::model::Node::new(
            NodeKind::Note,
            "Synaptic Plasticity",
            WorldPos::new(450.0, 100.0),
        );
        let id1 = n1.id.clone();
        let id2 = n2.id.clone();

        rt.state.graph.add_node(n1);
        rt.state.graph.add_node(n2);
        rt.frame(1.0 / 60.0);

        assert_eq!(rt.state.graph.nodes.len(), 2);

        // Connect with edge
        let edge = studynodes_graph::model::Edge::new(
            studynodes_graph::model::EdgeEndpoint {
                node: id1.clone(),
                port: studynodes_graph::model::PortId::new("out_main"),
            },
            studynodes_graph::model::EdgeEndpoint {
                node: id2.clone(),
                port: studynodes_graph::model::PortId::new("in_main"),
            },
        );
        let edge_id = edge.id.clone();
        rt.state.graph.add_edge(edge);
        rt.frame(1.0 / 60.0);

        assert_eq!(rt.state.graph.edges.len(), 1);

        // Select edge and verify inspector state
        rt.state.select_edge(edge_id.clone());
        assert_eq!(rt.state.selected_edge, Some(edge_id));
    }
}
