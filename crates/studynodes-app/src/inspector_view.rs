use rune::tok::{self, radius, space, type_};
use rune::{Color, Cx, El, FileDialogOptions, TextStyle, Tx, col, el, row, text, text_input};
use studynodes_graph::model::{EdgeRelation, NodeKind};
use studynodes_llm::LlmProvider;

use crate::state::AppState;

const TITLE_STYLE: TextStyle = TextStyle::new(15.0, 600.0);
const SECTION_TITLE: TextStyle = TextStyle::new(10.5, 700.0);
const BODY_STYLE: TextStyle = TextStyle::new(13.0, 400.0).lh(19.0);
const READER_STYLE: TextStyle = TextStyle::new(13.5, 400.0).lh(22.0);
const BADGE_STYLE: TextStyle = TextStyle::new(11.0, 600.0);
const MUTED_STYLE: TextStyle = TextStyle::new(11.0, 400.0);

fn kind_color(kind: NodeKind) -> Color {
    match kind {
        NodeKind::Note => Color { r: 0.96, g: 0.68, b: 0.20, a: 1.0 },       // Amber
        NodeKind::Document => Color { r: 0.30, g: 0.60, b: 0.98, a: 1.0 },   // Neon Blue
        NodeKind::Image => Color { r: 0.20, g: 0.78, b: 0.55, a: 1.0 },      // Emerald
        NodeKind::LlmSynthesis => Color { r: 0.66, g: 0.45, b: 0.95, a: 1.0 }, // Violet
        NodeKind::Concept => Color { r: 0.95, g: 0.40, b: 0.65, a: 1.0 },    // Pink
    }
}

fn badge_pill(label: &str, color: Color) -> El {
    el()
        .px(8.0)
        .py(3.0)
        .radius(12.0)
        .fill(color.alpha(0.14))
        .border(1.0, color.alpha(0.35))
        .child(text(label).font(BADGE_STYLE).color(color))
}

fn subtle_badge(label: &str, text_color: Color, bg_color: Color) -> El {
    el()
        .px(7.0)
        .py(2.5)
        .radius(6.0)
        .fill(bg_color)
        .child(text(label).font(MUTED_STYLE).color(text_color))
}

fn quick_format_chip(label: &'static str, action: impl Fn(&mut AppState) + 'static, bg: Color, border: Color, fg: Color) -> El {
    el()
        .px(8.0)
        .py(4.0)
        .radius(radius::SM)
        .fill(bg)
        .border(1.0, border)
        .on_press(action)
        .child(text(label).font(MUTED_STYLE).color(fg))
}

pub fn render_inspector(s: &AppState, _: &mut Cx<AppState>) -> El {
    let t = tok::theme(true);
    let panel_bg = Color { r: 0.09, g: 0.10, b: 0.13, a: 0.98 };
    let card_bg = Color { r: 0.12, g: 0.14, b: 0.18, a: 1.0 };
    let input_bg = Color { r: 0.07, g: 0.08, b: 0.11, a: 1.0 };
    let border = Color { r: 0.19, g: 0.22, b: 0.28, a: 1.0 };
    let border_subtle = Color { r: 0.15, g: 0.17, b: 0.22, a: 1.0 };
    let accent = Color { r: 0.38, g: 0.60, b: 1.0, a: 1.0 };
    let text_muted = Color { r: 0.50, g: 0.55, b: 0.64, a: 1.0 };

    let mut content = col().w_full().gap(space::S3);

    if s.ai_panel_open {
        // AI Synthesis Panel
        content = content
            .child(
                row()
                    .w_full()
                    .center_y()
                    .child(text("✦ AI SYNTHESIS & DISCOVERY").font(TITLE_STYLE).color(accent))
                    .child(el().grow(1.0))
                    .child(
                        el()
                            .px(6.0)
                            .py(2.0)
                            .radius(radius::SM)
                            .fill(card_bg)
                            .border(1.0, border)
                            .on_press(|s: &mut AppState| {
                                s.ai_panel_open = false;
                            })
                            .child(text("✕").font(MUTED_STYLE).color(text_muted)),
                    ),
            )
            .child(
                text("Synthesize latent connections, surface contradictions, or generate overarching conceptual bridges across nodes.")
                    .font(type_::LABEL)
                    .color(t.text_secondary),
            )
            .child(
                el()
                    .padding(space::S3)
                    .radius(radius::MD)
                    .fill(accent)
                    .on_press(|s: &mut AppState| {
                        let focus_ids: Vec<_> = s.selected_nodes.iter().cloned().collect();
                        let target_ids = if focus_ids.is_empty() {
                            s.graph.nodes.keys().take(3).cloned().collect()
                        } else {
                            focus_ids
                        };

                        let context = studynodes_llm::LlmContext::build(&s.graph, &target_ids, "Discover Connections");
                        let provider = studynodes_llm::MockLlmProvider;
                        
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
                    .child(
                        row()
                            .center()
                            .gap(space::S2)
                            .child(text("✦ Run AI Synthesis").font(type_::BODY).color(Color::WHITE)),
                    ),
            )
            .child(
                el()
                    .padding(space::S2)
                    .radius(radius::SM)
                    .fill(card_bg)
                    .border(1.0, border)
                    .on_press(|s: &mut AppState| {
                        s.ai_panel_open = false;
                    })
                    .child(text("Close AI Panel").font(type_::BODY).color(t.text_secondary)),
            );
    } else if let Some(selected_node_id) = s.selected_nodes.iter().next() {
        if let Some(node) = s.graph.get_node(selected_node_id) {
            let node_id_for_delete = node.id.clone();
            let node_id_for_ingest = node.id.clone();
            let node_id_for_sample = node.id.clone();
            let node_id_for_explain = node.id.clone();
            let node_kind = node.kind;
            let kc = kind_color(node_kind);

            // Compute document / note stats
            let word_count = node.content.split_whitespace().count();
            let char_count = node.content.len();
            let line_count = node.content.lines().count().max(1);
            let read_min = (word_count as f32 / 200.0).ceil() as usize;

            if node_kind == NodeKind::Document {
                // ==========================================
                // DEDICATED FULL DOCUMENT READER & EDITOR
                // ==========================================
                content = content
                    // 1. Reader Header Bar
                    .child(
                        row()
                            .w_full()
                            .center_y()
                            .gap(space::S2)
                            .child(badge_pill("📄 DOCUMENT READER", kc))
                            .child(subtle_badge(&format!("{} words", word_count), t.text_secondary, card_bg))
                            .child(subtle_badge(&format!("~{}m read", read_min), text_muted, card_bg))
                            .child(el().grow(1.0))
                            .child(
                                el()
                                    .px(8.0)
                                    .py(3.0)
                                    .radius(radius::SM)
                                    .fill(card_bg)
                                    .border(1.0, border_subtle)
                                    .on_press(|s: &mut AppState| {
                                        s.sync_node_edits();
                                        s.inspector_open = false;
                                    })
                                    .child(text("✕ Close").font(MUTED_STYLE).color(text_muted)),
                            ),
                    )
                    // 2. Editable Document Title
                    .child(
                        col()
                            .gap(4.0)
                            .child(text("DOCUMENT TITLE").font(SECTION_TITLE).color(kc))
                            .child(
                                el()
                                    .w_full()
                                    .padding(8.0)
                                    .radius(radius::SM)
                                    .fill(input_bg)
                                    .border(1.0, border)
                                    .child(
                                        text_input(&s.edit_node_title, TITLE_STYLE)
                                            .single_line()
                                            .w_full()
                                            .color(t.text)
                                            .caret(accent, accent.alpha(0.25))
                                            .placeholder("Document Title...", Color::WHITE.alpha(0.35))
                                            .on_edit(|s: &mut AppState| &mut s.edit_node_title)
                                            .on_input_change(|s: &mut AppState, _| {
                                                s.sync_node_edits();
                                            })
                                            .on_submit(|s: &mut AppState, _| {
                                                s.sync_node_edits();
                                            }),
                                    ),
                            ),
                    )
                    // 3. Document Action Bar: Ingest, Sample, AI Explain, Save
                    .child(
                        row()
                            .gap(space::S2)
                            .child(
                                el()
                                    .grow(1.0)
                                    .px(space::S3)
                                    .py(space::S2)
                                    .radius(radius::SM)
                                    .fill(Color { r: 0.16, g: 0.28, b: 0.52, a: 0.45 })
                                    .border(1.0, Color { r: 0.28, g: 0.50, b: 0.90, a: 0.75 })
                                    .on_press_tx(move |_s: &mut AppState, tx: &mut Tx| {
                                        let opts = FileDialogOptions {
                                            title: "Ingest Document File".into(),
                                            ..Default::default()
                                        }
                                        .filter("All Supported Files", &["pdf", "epub", "txt", "md", "json", "rs", "toml", "csv"])
                                        .filter("Documents", &["pdf", "epub"])
                                        .filter("Text & Markdown", &["txt", "md", "json", "rs", "toml", "csv"]);

                                        let target_id = node_id_for_ingest.clone();
                                        tx.open_file(opts, move |s: &mut AppState, res, _| {
                                            match res {
                                                Ok(Some(path)) => {
                                                    s.ingest_file_into_node(&target_id, &path);
                                                }
                                                Ok(None) => {}
                                                Err(e) => {
                                                    s.status_message = format!("File picker error: {}", e);
                                                }
                                            }
                                        });
                                    })
                                    .child(
                                        row()
                                            .center()
                                            .gap(4.0)
                                            .child(text("📂 Ingest File").font(type_::LABEL).color(Color::WHITE)),
                                    ),
                            )
                            .child(
                                el()
                                    .px(space::S3)
                                    .py(space::S2)
                                    .radius(radius::SM)
                                    .fill(card_bg)
                                    .border(1.0, border)
                                    .on_press(move |s: &mut AppState| {
                                        s.load_sample_document(&node_id_for_sample);
                                    })
                                    .child(text("✦ Sample Paper").font(type_::LABEL).color(accent)),
                            )
                            .child(
                                el()
                                    .px(space::S3)
                                    .py(space::S2)
                                    .radius(radius::SM)
                                    .fill(Color { r: 0.22, g: 0.16, b: 0.35, a: 0.40 })
                                    .border(1.0, Color { r: 0.55, g: 0.35, b: 0.85, a: 0.65 })
                                    .on_press(move |s: &mut AppState| {
                                        s.explain_node_ai(&node_id_for_explain);
                                    })
                                    .child(text("✦ Summarize").font(type_::LABEL).color(Color { r: 0.78, g: 0.60, b: 1.0, a: 1.0 })),
                            ),
                    );

                // Stats / Asset info row
                content = content.child(
                    row()
                        .gap(space::S1)
                        .child(subtle_badge(&format!("{} lines", line_count), text_muted, card_bg))
                        .child(subtle_badge(&format!("{} characters", char_count), text_muted, card_bg))
                        .child(
                            if let Some(hash) = &node.asset_hash {
                                subtle_badge(&format!("📎 Asset: {}", &hash[..hash.len().min(8)]), kc, card_bg)
                            } else {
                                el()
                            },
                        ),
                );

                // AI Summary / Key Insights if present
                if let Some(summary) = &node.summary {
                    content = content.child(
                        col()
                            .gap(4.0)
                            .child(text("✦ AI KEY INSIGHTS & SYNTHESIS").font(SECTION_TITLE).color(Color { r: 0.78, g: 0.60, b: 1.0, a: 1.0 }))
                            .child(
                                el()
                                    .w_full()
                                    .padding(space::S3)
                                    .radius(radius::SM)
                                    .fill(Color { r: 0.22, g: 0.16, b: 0.35, a: 0.25 })
                                    .border(1.0, Color { r: 0.55, g: 0.35, b: 0.85, a: 0.40 })
                                    .child(text(summary).font(BODY_STYLE).color(t.text)),
                            ),
                    );
                }

                // 4. Large Comfortable Reader Viewport
                content = content.child(
                    col()
                        .gap(4.0)
                        .child(
                            row()
                                .w_full()
                                .child(text("DOCUMENT TEXT & EXCERPTS").font(SECTION_TITLE).color(text_muted))
                                .child(el().grow(1.0))
                                .child(text("Direct live editor").font(MUTED_STYLE).color(text_muted)),
                        )
                        .child(
                            col()
                                .w_full()
                                .radius(radius::MD)
                                .fill(input_bg)
                                .border(1.0, border)
                                .padding(12.0)
                                .child(
                                    col()
                                        .w_full()
                                        .min_h(320.0)
                                        .max_h(560.0)
                                        .scroll(rune::ScrollSpec { stick_bottom: false })
                                        .child(
                                            text_input(&s.edit_node_content, READER_STYLE)
                                                .w_full()
                                                .color(Color { r: 0.90, g: 0.92, b: 0.95, a: 1.0 })
                                                .caret(accent, accent.alpha(0.25))
                                                .placeholder(
                                                    "Document content is empty.\n\nIngest a file (PDF, TXT, MD) using the button above or type/paste document excerpts here...",
                                                    Color::WHITE.alpha(0.35),
                                                )
                                                .on_edit(|s: &mut AppState| &mut s.edit_node_content)
                                                .on_input_change(|s: &mut AppState, _| {
                                                    s.sync_node_edits();
                                                })
                                                .on_submit(|s: &mut AppState, _| {
                                                    s.edit_node_content.insert("\n");
                                                    s.sync_node_edits();
                                                }),
                                        ),
                                ),
                        ),
                );

                // 5. Quick Append & Document Actions
                content = content.child(
                    row()
                        .w_full()
                        .gap(space::S2)
                        .child(
                            el()
                                .grow(1.0)
                                .px(6.0)
                                .py(6.0)
                                .radius(radius::SM)
                                .fill(input_bg)
                                .border(1.0, border)
                                .child(
                                    text_input(&s.append_note_input, TextStyle::new(12.0, 400.0))
                                        .single_line()
                                        .w_full()
                                        .color(t.text)
                                        .caret(accent, accent.alpha(0.25))
                                        .placeholder("Append note to document...", Color::WHITE.alpha(0.35))
                                        .on_edit(|s: &mut AppState| &mut s.append_note_input)
                                        .on_submit(|s: &mut AppState, _| {
                                            s.commit_append_input();
                                        }),
                                ),
                        )
                        .child(
                            el()
                                .px(10.0)
                                .py(6.0)
                                .radius(radius::SM)
                                .fill(accent)
                                .on_press(|s: &mut AppState| {
                                    s.commit_append_input();
                                })
                                .child(text("+ Append").font(type_::LABEL).color(Color::WHITE)),
                        ),
                );

                // Save & Delete
                content = content.child(
                    row()
                        .w_full()
                        .gap(space::S2)
                        .child(
                            el()
                                .grow(1.0)
                                .padding(space::S2)
                                .radius(radius::SM)
                                .fill(card_bg)
                                .border(1.0, border)
                                .on_press(|s: &mut AppState| {
                                    s.sync_node_edits();
                                    if let Some(bundle) = &s.bundle {
                                        let _ = bundle.save_graph_atomic(&s.graph);
                                    }
                                    s.status_message = "Document saved".into();
                                })
                                .child(
                                    row()
                                        .center()
                                        .gap(4.0)
                                        .child(text("💾 Save Document").font(type_::LABEL).color(t.text)),
                                ),
                        )
                        .child(
                            el()
                                .padding(space::S2)
                                .radius(radius::SM)
                                .fill(Color { r: 0.55, g: 0.15, b: 0.15, a: 0.25 })
                                .border(1.0, Color { r: 0.8, g: 0.25, b: 0.25, a: 0.7 })
                                .on_press(move |s: &mut AppState| {
                                    s.delete_node(&node_id_for_delete);
                                })
                                .child(
                                    text("Delete")
                                        .font(type_::LABEL)
                                        .color(Color { r: 1.0, g: 0.45, b: 0.45, a: 1.0 }),
                                ),
                        ),
                );
            } else {
                // ==========================================
                // NOTE & CONCEPT INSPECTOR
                // ==========================================
                // 1. Header with Badge Pill and Deselect
                content = content
                    .child(
                        row()
                            .w_full()
                            .center_y()
                            .gap(space::S2)
                            .child(badge_pill(node_kind.display_name(), kc))
                            .child(
                                subtle_badge(
                                    &format!("#{}", &node.id.as_str()[..node.id.as_str().len().min(6)]),
                                    text_muted,
                                    card_bg,
                                ),
                            )
                            .child(el().grow(1.0))
                            .child(
                                el()
                                    .px(6.0)
                                    .py(2.0)
                                    .radius(radius::SM)
                                    .fill(card_bg)
                                    .border(1.0, border_subtle)
                                    .on_press(|s: &mut AppState| {
                                        s.sync_node_edits();
                                        s.selected_nodes.clear();
                                    })
                                    .child(text("✕").font(MUTED_STYLE).color(text_muted)),
                            ),
                    );

                // 2. Editable Title
                content = content
                    .child(
                        col()
                            .gap(4.0)
                            .child(text("TITLE").font(SECTION_TITLE).color(text_muted))
                            .child(
                                el()
                                    .w_full()
                                    .padding(8.0)
                                    .radius(radius::SM)
                                    .fill(input_bg)
                                    .border(1.0, border)
                                    .child(
                                        text_input(&s.edit_node_title, TITLE_STYLE)
                                            .single_line()
                                            .w_full()
                                            .color(t.text)
                                            .caret(accent, accent.alpha(0.25))
                                            .placeholder("Node Title...", Color::WHITE.alpha(0.35))
                                            .on_edit(|s: &mut AppState| &mut s.edit_node_title)
                                            .on_input_change(|s: &mut AppState, _| {
                                                s.sync_node_edits();
                                            })
                                            .on_submit(|s: &mut AppState, _| {
                                                s.sync_node_edits();
                                            }),
                                    ),
                            ),
                    );

                // Stats Pill Row
                content = content.child(
                    row()
                        .gap(space::S1)
                        .child(subtle_badge(&format!("{} words", word_count), t.text_secondary, card_bg))
                        .child(subtle_badge(&format!("{} chars", char_count), t.text_secondary, card_bg))
                        .child(subtle_badge(&format!("~{}m read", read_min), text_muted, card_bg)),
                );

                // 3. Editable Content Area
                content = content
                    .child(
                        col()
                            .gap(4.0)
                            .child(
                                row()
                                    .w_full()
                                    .child(text("CONTENT & NOTES").font(SECTION_TITLE).color(text_muted))
                                    .child(el().grow(1.0))
                                    .child(text(format!("{} lines", line_count)).font(MUTED_STYLE).color(text_muted)),
                            )
                            .child(
                                col()
                                    .w_full()
                                    .radius(radius::SM)
                                    .fill(input_bg)
                                    .border(1.0, border)
                                    .padding(8.0)
                                    .child(
                                        col()
                                            .w_full()
                                            .min_h(180.0)
                                            .max_h(320.0)
                                            .scroll(rune::ScrollSpec { stick_bottom: false })
                                            .child(
                                                text_input(&s.edit_node_content, BODY_STYLE)
                                                    .w_full()
                                                    .color(t.text)
                                                    .caret(accent, accent.alpha(0.25))
                                                    .placeholder(
                                                        "Type your notes, markdown, or study observations here...",
                                                        Color::WHITE.alpha(0.35),
                                                    )
                                                    .on_edit(|s: &mut AppState| &mut s.edit_node_content)
                                                    .on_input_change(|s: &mut AppState, _| {
                                                        s.sync_node_edits();
                                                    })
                                                    .on_submit(|s: &mut AppState, _| {
                                                        s.edit_node_content.insert("\n");
                                                        s.sync_node_edits();
                                                    }),
                                            ),
                                    ),
                            ),
                    );

                // 4. Quick Append & Note Actions
                content = content
                    .child(
                        col()
                            .gap(space::S2)
                            .child(text("QUICK APPEND").font(SECTION_TITLE).color(text_muted))
                            .child(
                                row()
                                    .w_full()
                                    .gap(space::S2)
                                    .child(
                                        el()
                                            .grow(1.0)
                                            .padding(6.0)
                                            .radius(radius::SM)
                                            .fill(input_bg)
                                            .border(1.0, border)
                                            .child(
                                                text_input(&s.append_note_input, TextStyle::new(12.0, 400.0))
                                                    .single_line()
                                                    .w_full()
                                                    .color(t.text)
                                                    .caret(accent, accent.alpha(0.25))
                                                    .placeholder("Quick note or bullet to append...", Color::WHITE.alpha(0.35))
                                                    .on_edit(|s: &mut AppState| &mut s.append_note_input)
                                                    .on_submit(|s: &mut AppState, _| {
                                                        s.commit_append_input();
                                                    }),
                                            ),
                                    )
                                    .child(
                                        el()
                                            .px(10.0)
                                            .py(6.0)
                                            .radius(radius::SM)
                                            .fill(accent)
                                            .on_press(|s: &mut AppState| {
                                                s.commit_append_input();
                                            })
                                            .child(text("+ Append").font(type_::LABEL).color(Color::WHITE)),
                                    ),
                            )
                            .child(
                                row()
                                    .gap(space::S1)
                                    .child(quick_format_chip("+ Bullet", |s| s.append_to_selected_node("• "), card_bg, border, t.text))
                                    .child(quick_format_chip("+ Heading", |s| s.append_to_selected_node("\n### "), card_bg, border, t.text))
                                    .child(quick_format_chip("+ Checkbox", |s| s.append_to_selected_node("\n- [ ] "), card_bg, border, t.text))
                                    .child(quick_format_chip("+ Quote", |s| s.append_to_selected_node("\n> "), card_bg, border, t.text)),
                            ),
                    );

                // 5. AI Summary Card (if present)
                if let Some(summary) = &node.summary {
                    content = content.child(
                        col()
                            .gap(4.0)
                            .child(text("AI SYNTHESIS SUMMARY").font(SECTION_TITLE).color(accent))
                            .child(
                                el()
                                    .w_full()
                                    .padding(space::S3)
                                    .radius(radius::SM)
                                    .fill(accent.alpha(0.08))
                                    .border(1.0, accent.alpha(0.25))
                                    .child(text(summary).font(BODY_STYLE).color(t.text)),
                            ),
                    );
                }

                // AI Explain Action Button
                content = content.child(
                    el()
                        .w_full()
                        .padding(space::S2)
                        .radius(radius::SM)
                        .fill(Color { r: 0.22, g: 0.16, b: 0.35, a: 0.40 })
                        .border(1.0, Color { r: 0.55, g: 0.35, b: 0.85, a: 0.65 })
                        .on_press(move |s: &mut AppState| {
                            s.explain_node_ai(&node_id_for_explain);
                        })
                        .child(
                            row()
                                .center()
                                .gap(space::S1)
                                .child(text("✦ Explain Concept with AI").font(type_::LABEL).color(Color { r: 0.78, g: 0.60, b: 1.0, a: 1.0 })),
                        ),
                );

                // 6. Save & Delete Node Actions
                content = content.child(
                    row()
                        .w_full()
                        .gap(space::S2)
                        .child(
                            el()
                                .grow(1.0)
                                .padding(space::S2)
                                .radius(radius::SM)
                                .fill(card_bg)
                                .border(1.0, border)
                                .on_press(|s: &mut AppState| {
                                    s.sync_node_edits();
                                    if let Some(bundle) = &s.bundle {
                                        let _ = bundle.save_graph_atomic(&s.graph);
                                    }
                                    s.status_message = "Node content saved".into();
                                })
                                .child(
                                    row()
                                        .center()
                                        .gap(4.0)
                                        .child(text("💾 Save Changes").font(type_::LABEL).color(t.text)),
                                ),
                        )
                        .child(
                            el()
                                .padding(space::S2)
                                .radius(radius::SM)
                                .fill(Color { r: 0.55, g: 0.15, b: 0.15, a: 0.25 })
                                .border(1.0, Color { r: 0.8, g: 0.25, b: 0.25, a: 0.7 })
                                .on_press(move |s: &mut AppState| {
                                    s.delete_node(&node_id_for_delete);
                                })
                                .child(
                                    text("Delete")
                                        .font(type_::LABEL)
                                        .color(Color { r: 1.0, g: 0.45, b: 0.45, a: 1.0 }),
                                ),
                        ),
                );
            }
        }
    } else if let Some(edge_id) = &s.selected_edge {
        if let Some(edge) = s.graph.get_edge(edge_id) {
            let edge_id_for_delete = edge.id.clone();
            let edge_id_for_rel = edge.id.clone();
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
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .gap(space::S2)
                        .child(badge_pill("CONNECTION", accent))
                        .child(el().grow(1.0))
                        .child(
                            el()
                                .px(6.0)
                                .py(2.0)
                                .radius(radius::SM)
                                .fill(card_bg)
                                .border(1.0, border_subtle)
                                .on_press(|s: &mut AppState| {
                                    s.sync_edge_edits();
                                    s.selected_edge = None;
                                })
                                .child(text("✕").font(MUTED_STYLE).color(text_muted)),
                        ),
                )
                .child(
                    el()
                        .w_full()
                        .padding(space::S2)
                        .radius(radius::SM)
                        .fill(card_bg)
                        .border(1.0, border_subtle)
                        .child(
                            text(format!("{}  ──→  {}", from_title, to_title))
                                .font(BODY_STYLE)
                                .color(accent),
                        ),
                )
                .child(
                    col()
                        .gap(4.0)
                        .child(text("RELATION TYPE").font(SECTION_TITLE).color(text_muted))
                        .child(
                            row()
                                .gap(space::S1)
                                .child(
                                    badge_pill(edge.relation.display_label(), accent),
                                ),
                        )
                        .child(
                            row()
                                .gap(space::S1)
                                .child({
                                    let eid = edge_id_for_rel.clone();
                                    quick_format_chip("Supports", move |s| {
                                        if let Some(e) = s.graph.get_edge_mut(&eid) { e.relation = EdgeRelation::Supports; }
                                    }, card_bg, border, t.text)
                                })
                                .child({
                                    let eid = edge_id_for_rel.clone();
                                    quick_format_chip("Contradicts", move |s| {
                                        if let Some(e) = s.graph.get_edge_mut(&eid) { e.relation = EdgeRelation::Contradicts; }
                                    }, card_bg, border, Color { r: 0.95, g: 0.4, b: 0.4, a: 1.0 })
                                })
                                .child({
                                    let eid = edge_id_for_rel.clone();
                                    quick_format_chip("Explains", move |s| {
                                        if let Some(e) = s.graph.get_edge_mut(&eid) { e.relation = EdgeRelation::Explains; }
                                    }, card_bg, border, t.text)
                                }),
                        ),
                )
                .child(
                    col()
                        .gap(4.0)
                        .child(text("LABEL").font(SECTION_TITLE).color(text_muted))
                        .child(
                            el()
                                .w_full()
                                .padding(8.0)
                                .radius(radius::SM)
                                .fill(input_bg)
                                .border(1.0, border)
                                .child(
                                    text_input(&s.edit_edge_label, TITLE_STYLE)
                                        .single_line()
                                        .w_full()
                                        .color(t.text)
                                        .caret(accent, accent.alpha(0.25))
                                        .placeholder("Connection label (e.g. causes, corroborates)...", Color::WHITE.alpha(0.35))
                                        .on_edit(|s: &mut AppState| &mut s.edit_edge_label)
                                        .on_input_change(|s: &mut AppState, _| {
                                            s.sync_edge_edits();
                                        })
                                        .on_submit(|s: &mut AppState, _| {
                                            s.sync_edge_edits();
                                        }),
                                ),
                        ),
                )
                .child(
                    col()
                        .gap(4.0)
                        .child(text("CONNECTION NOTES").font(SECTION_TITLE).color(text_muted))
                        .child(
                            col()
                                .w_full()
                                .min_h(100.0)
                                .padding(8.0)
                                .radius(radius::SM)
                                .fill(input_bg)
                                .border(1.0, border)
                                .child(
                                    text_input(&s.edit_edge_note, BODY_STYLE)
                                        .w_full()
                                        .color(t.text)
                                        .caret(accent, accent.alpha(0.25))
                                        .placeholder("Notes on why this connection exists...", Color::WHITE.alpha(0.35))
                                        .on_edit(|s: &mut AppState| &mut s.edit_edge_note)
                                        .on_input_change(|s: &mut AppState, _| {
                                            s.sync_edge_edits();
                                        })
                                        .on_submit(|s: &mut AppState, _| {
                                            s.edit_edge_note.insert("\n");
                                            s.sync_edge_edits();
                                        }),
                                ),
                        ),
                )
                .child(
                    el()
                        .padding(space::S2)
                        .radius(radius::SM)
                        .fill(Color { r: 0.55, g: 0.15, b: 0.15, a: 0.25 })
                        .border(1.0, Color { r: 0.8, g: 0.25, b: 0.25, a: 0.7 })
                        .on_press(move |s: &mut AppState| {
                            s.graph.remove_edge(&edge_id_for_delete);
                            s.selected_edge = None;
                            s.status_message = "Deleted connection".into();
                        })
                        .child(
                            row()
                                .center()
                                .child(
                                    text("Delete Connection")
                                        .font(type_::LABEL)
                                        .color(Color { r: 1.0, g: 0.45, b: 0.45, a: 1.0 }),
                                ),
                        ),
                );
        }
    } else {
        // Nothing selected: Clean Antigravity empty state with Quick Add buttons
        content = content
            .child(
                row()
                    .w_full()
                    .center_y()
                    .child(text("INSPECTOR").font(TITLE_STYLE).color(t.text))
                    .child(el().grow(1.0))
                    .child(badge_pill("Idle", text_muted)),
            )
            .child(
                text("Select any node or connection wire in the canvas to view details, edit contents, or read documents.")
                    .font(BODY_STYLE)
                    .color(t.text_secondary),
            )
            .child(
                col()
                    .gap(space::S2)
                    .padding(space::S3)
                    .radius(radius::MD)
                    .fill(card_bg)
                    .border(1.0, border)
                    .child(text("QUICK CREATE").font(SECTION_TITLE).color(accent))
                    .child(
                        row()
                            .gap(space::S2)
                            .child(
                                el()
                                    .grow(1.0)
                                    .px(space::S3)
                                    .py(space::S2)
                                    .radius(radius::SM)
                                    .fill(Color { r: 0.96, g: 0.68, b: 0.20, a: 0.18 })
                                    .border(1.0, Color { r: 0.96, g: 0.68, b: 0.20, a: 0.5 })
                                    .on_press(|s: &mut AppState| {
                                        s.add_note_node(None);
                                    })
                                    .child(
                                        row()
                                            .center()
                                            .gap(4.0)
                                            .child(
                                                text("+ New Note")
                                                    .font(type_::LABEL)
                                                    .color(Color { r: 0.96, g: 0.68, b: 0.20, a: 1.0 }),
                                            ),
                                    ),
                            )
                            .child(
                                el()
                                    .grow(1.0)
                                    .px(space::S3)
                                    .py(space::S2)
                                    .radius(radius::SM)
                                    .fill(Color { r: 0.30, g: 0.60, b: 0.98, a: 0.18 })
                                    .border(1.0, Color { r: 0.30, g: 0.60, b: 0.98, a: 0.5 })
                                    .on_press(|s: &mut AppState| {
                                        s.add_document_node(None);
                                    })
                                    .child(
                                        row()
                                            .center()
                                            .gap(4.0)
                                            .child(
                                                text("+ New Document")
                                                    .font(type_::LABEL)
                                                    .color(Color { r: 0.30, g: 0.60, b: 0.98, a: 1.0 }),
                                            ),
                                    ),
                            ),
                    ),
            )
            .child(
                col()
                    .gap(space::S2)
                    .padding(space::S3)
                    .radius(radius::MD)
                    .fill(card_bg)
                    .border(1.0, border_subtle)
                    .child(text("✦ WORKFLOW TIPS").font(SECTION_TITLE).color(text_muted))
                    .child(
                        text("• Click any Document node to open the full Document Reader\n• Drag canvas background, Space+drag, or right-click to pan in 2D\n• Type directly into notes on canvas or in this sidebar\n• Ingest PDFs, EPUBs, or TXT directly into document nodes\n• Connect nodes via socket wire dragging")
                            .font(MUTED_STYLE)
                            .color(t.text_secondary),
                    ),
            );
    }

    let is_doc_selected = s.selected_nodes.iter().next().and_then(|id| s.graph.get_node(id)).map_or(false, |n| n.kind == NodeKind::Document);
    let panel_width = if is_doc_selected { 400.0 } else { 340.0 };

    col()
        .w(panel_width)
        .h_full()
        .fill(panel_bg)
        .border(1.0, border)
        .padding(16.0)
        .scroll(rune::ScrollSpec { stick_bottom: false })
        .child(content)
}
