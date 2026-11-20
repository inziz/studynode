use rune::tok::{self, radius, space, type_};
use rune::{Color, Cx, El, TextStyle, col, el, row, text};
use studynodes_llm::LlmProvider;

use crate::state::AppState;

const HEADLINE: TextStyle = TextStyle::new(14.0, 600.0);

pub fn render_inspector(s: &AppState, _: &mut Cx<AppState>) -> El {
    let t = tok::theme(true);
    let panel_bg = Color { r: 0.10, g: 0.11, b: 0.14, a: 0.98 };
    let border = Color { r: 0.20, g: 0.22, b: 0.27, a: 1.0 };
    let accent = Color { r: 0.38, g: 0.60, b: 1.0, a: 1.0 };

    let mut content = col().w_full().gap(space::S3);

    if s.ai_panel_open {
        // AI Synthesis Panel
        content = content
            .child(text("✦ AI Synthesis & Discovery").font(HEADLINE).color(accent))
            .child(
                text("Analyze the current graph neighborhood to surface latent conceptual connections, contradictions, or synthesize overarching summaries.")
                    .font(type_::LABEL)
                    .color(t.text_secondary),
            )
            .child(
                el()
                    .padding(space::S3)
                    .radius(radius::MD)
                    .fill(accent)
                    .on_press(|s: &mut AppState| {
                        // Run synthesis
                        let focus_ids: Vec<_> = s.selected_nodes.iter().cloned().collect();
                        let target_ids = if focus_ids.is_empty() {
                            s.graph.nodes.keys().take(3).cloned().collect()
                        } else {
                            focus_ids
                        };

                        let context = studynodes_llm::LlmContext::build(&s.graph, &target_ids, "Discover Connections");
                        let provider = studynodes_llm::MockLlmProvider;
                        
                        // In synchronous Rune turn, execute mock synthesis
                        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
                        if let Ok(rt) = rt {
                            if let Ok(patch) = rt.block_on(provider.generate_patch(&context)) {
                                let res = s.graph.validate_and_stage_patch(&patch);
                                s.status_message = format!(
                                    "AI proposed {} edge(s) and {} synthesis node(s)",
                                    res.proposed_edges.len(),
                                    res.new_nodes.len()
                                );
                            }
                        }
                    })
                    .child(text("Run AI Synthesis").font(type_::BODY).color(Color::WHITE)),
            )
            .child(
                el()
                    .padding(space::S2)
                    .radius(radius::SM)
                    .fill(t.card)
                    .border(1.0, border)
                    .on_press(|s: &mut AppState| {
                        s.ai_panel_open = false;
                    })
                    .child(text("Close AI Panel").font(type_::BODY).color(t.text_secondary)),
            );
    } else if let Some(selected_node_id) = s.selected_nodes.iter().next() {
        if let Some(node) = s.graph.get_node(selected_node_id) {
            let node_id_for_delete = node.id.clone();
            content = content
                .child(text("Node Inspector").font(HEADLINE).color(t.text))
                .child(
                    row()
                        .gap(space::S2)
                        .child(
                            text(format!("[{}]", node.kind.display_name()))
                                .font(type_::LABEL)
                                .color(accent),
                        )
                        .child(text(&node.title).font(HEADLINE).color(t.text)),
                )
                .child(
                    col()
                        .gap(space::S1)
                        .child(text("Content / Excerpt").font(type_::LABEL).color(t.text_secondary))
                        .child(
                            el()
                                .w_full()
                                .padding(space::S3)
                                .radius(radius::SM)
                                .fill(t.card)
                                .border(1.0, border)
                                .child(
                                    text(if node.content.is_empty() {
                                        "No content text recorded."
                                    } else {
                                        &node.content
                                    })
                                    .font(type_::BODY)
                                    .color(t.text),
                                ),
                        ),
                );

            if let Some(summary) = &node.summary {
                content = content.child(
                    col()
                        .gap(space::S1)
                        .child(text("AI Summary").font(type_::LABEL).color(accent))
                        .child(
                            el()
                                .w_full()
                                .padding(space::S2)
                                .radius(radius::SM)
                                .fill(accent.alpha(0.1))
                                .child(text(summary).font(type_::LABEL).color(t.text)),
                        ),
                );
            }

            // Delete button
            content = content.child(
                el()
                    .padding(space::S2)
                    .radius(radius::SM)
                    .fill(Color { r: 0.6, g: 0.15, b: 0.15, a: 0.3 })
                    .border(1.0, Color { r: 0.8, g: 0.2, b: 0.2, a: 1.0 })
                    .on_press(move |s: &mut AppState| {
                        s.graph.remove_node(&node_id_for_delete);
                        s.selected_nodes.clear();
                        s.status_message = "Deleted node".into();
                    })
                    .child(
                        text("Delete Node")
                            .font(type_::BODY)
                            .color(Color { r: 1.0, g: 0.4, b: 0.4, a: 1.0 }),
                    ),
            );
        }
    } else if let Some(edge_id) = &s.selected_edge {
        if let Some(edge) = s.graph.get_edge(edge_id) {
            let edge_id_for_delete = edge.id.clone();
            let from_title = s
                .graph
                .get_node(&edge.from.node)
                .map(|n| n.title.as_str())
                .unwrap_or("?");
            let to_title = s
                .graph
                .get_node(&edge.to.node)
                .map(|n| n.title.as_str())
                .unwrap_or("?");

            content = content
                .child(text("Connection Inspector").font(HEADLINE).color(t.text))
                .child(
                    text(format!("{}  ──→  {}", from_title, to_title))
                        .font(type_::BODY)
                        .color(accent),
                )
                .child(
                    col()
                        .gap(space::S1)
                        .child(text("Relation").font(type_::LABEL).color(t.text_secondary))
                        .child(
                            text(format!("Type: {}", edge.relation.display_label()))
                                .font(type_::BODY)
                                .color(t.text),
                        ),
                )
                .child(
                    col()
                        .gap(space::S1)
                        .child(text("Label").font(type_::LABEL).color(t.text_secondary))
                        .child(
                            text(if edge.label.is_empty() { "(no label)" } else { &edge.label })
                                .font(type_::BODY)
                                .color(t.text),
                        ),
                )
                .child(
                    col()
                        .gap(space::S1)
                        .child(text("Notes on Connection").font(type_::LABEL).color(t.text_secondary))
                        .child(
                            el()
                                .w_full()
                                .padding(space::S3)
                                .radius(radius::SM)
                                .fill(t.card)
                                .border(1.0, border)
                                .child(
                                    text(if edge.note.is_empty() { "(no notes)" } else { &edge.note })
                                        .font(type_::BODY)
                                        .color(t.text),
                                ),
                        ),
                )
                .child(
                    el()
                        .padding(space::S2)
                        .radius(radius::SM)
                        .fill(Color { r: 0.6, g: 0.15, b: 0.15, a: 0.3 })
                        .border(1.0, Color { r: 0.8, g: 0.2, b: 0.2, a: 1.0 })
                        .on_press(move |s: &mut AppState| {
                            s.graph.remove_edge(&edge_id_for_delete);
                            s.selected_edge = None;
                            s.status_message = "Deleted connection".into();
                        })
                        .child(
                            text("Delete Connection")
                                .font(type_::BODY)
                                .color(Color { r: 1.0, g: 0.4, b: 0.4, a: 1.0 }),
                        ),
                );
        }
    } else {
        content = content
            .child(text("Inspector").font(HEADLINE).color(t.text))
            .child(
                text("Select any node or connection wire in the canvas to inspect its details, notes, and relationships.")
                    .font(type_::BODY)
                    .color(t.text_secondary),
            );
    }

    col()
        .w(320.0)
        .h_full()
        .padding(16.0)
        .fill(panel_bg)
        .border(1.0, border)
        .child(content)
}
