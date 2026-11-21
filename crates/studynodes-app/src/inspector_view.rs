use rune::tok::{radius, space};
use rune::{
    Color, Cursor, Cx, El, FileDialogOptions, ScrollSpec, TextStyle, Tx, col, el, row, text,
    text_input,
};
use studynodes_graph::model::{EdgeRelation, NodeKind};
use studynodes_llm::LlmProvider;

use crate::state::AppState;
use crate::theme;

fn badge_pill(label: &str, color: Color) -> El {
    el()
        .px(8.0)
        .py(3.0)
        .radius(10.0)
        .fill(color.alpha(0.14))
        .border(1.0, color.alpha(0.32))
        .child(text(label).font(theme::BADGE_TEXT).color(color))
}

fn subtle_badge(label: &str, text_color: Color) -> El {
    el()
        .px(7.0)
        .py(2.5)
        .radius(6.0)
        .fill(theme::bg_control())
        .border(1.0, theme::border_subtle())
        .child(text(label).font(theme::BADGE_TEXT).color(text_color))
}

fn close_panel_btn() -> El {
    el()
        .key("btn_close_inspector")
        .size(24.0, 24.0)
        .radius(6.0)
        .fill(theme::bg_control())
        .hover(theme::hover_subtle())
        .press(theme::press_subtle())
        .border(1.0, theme::border_subtle())
        .center()
        .cursor(Cursor::Pointer)
        .on_press(|s: &mut AppState| {
            s.sync_node_edits();
            s.sync_edge_edits();
            s.inspector_open = false;
            s.ai_panel_open = false;
        })
        .child(
            text("×")
                .font(TextStyle::new(14.0, 500.0))
                .color(theme::text_secondary()),
        )
}

fn quick_chip(
    key: &'static str,
    label: &'static str,
    fg: Color,
    action: impl Fn(&mut AppState) + 'static,
) -> El {
    el()
        .key(key)
        .px(8.0)
        .py(4.0)
        .radius(radius::SM)
        .fill(theme::bg_control())
        .hover(theme::hover_subtle())
        .press(theme::press_subtle())
        .border(1.0, theme::border_subtle())
        .cursor(Cursor::Pointer)
        .on_press(action)
        .child(text(label).font(theme::STATUS_TEXT).color(fg))
}

pub fn render_inspector(s: &AppState, _: &mut Cx<AppState>) -> El {
    let mut content = col().w_full().gap(space::S3);

    if s.ai_panel_open {
        // AI Synthesis Panel
        content = content
            .child(
                row()
                    .w_full()
                    .center_y()
                    .gap(space::S2)
                    .child(el().size(8.0, 8.0).radius(4.0).fill(theme::accent_violet()))
                    .child(
                        text("AI Synthesis")
                            .font(theme::PANEL_TITLE)
                            .color(theme::text_primary()),
                    )
                    .child(el().grow(1.0))
                    .child(close_panel_btn()),
            )
            .child(
                text("Synthesize latent connections, surface contradictions, or generate conceptual bridges across selected nodes.")
                    .font(theme::BODY_TEXT)
                    .color(theme::text_secondary())
                    .w_full()
                    .wrap(),
            )
            .child(
                el()
                    .key("btn_run_ai_synth")
                    .w_full()
                    .px(space::S3)
                    .py(10.0)
                    .radius(radius::MD)
                    .fill(theme::accent_violet().alpha(0.18))
                    .hover(theme::accent_violet().alpha(0.28))
                    .press(theme::accent_violet().alpha(0.35))
                    .border(1.0, theme::accent_violet().alpha(0.45))
                    .cursor(Cursor::Pointer)
                    .on_press(|s: &mut AppState| {
                        let focus_ids: Vec<_> = s.selected_nodes.iter().cloned().collect();
                        let target_ids = if focus_ids.is_empty() {
                            s.graph.nodes.keys().take(3).cloned().collect()
                        } else {
                            focus_ids
                        };

                        let context = studynodes_llm::LlmContext::build(
                            &s.graph,
                            &target_ids,
                            "Discover Connections",
                        );
                        let provider = studynodes_llm::MockLlmProvider;

                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build();
                        if let Ok(rt) = rt {
                            if let Ok(patch) = rt.block_on(provider.generate_patch(&context)) {
                                let res = s.graph.validate_and_stage_patch(&patch);
                                if let Some(bundle) = &s.bundle {
                                    let _ = bundle.save_graph_atomic(&s.graph);
                                }
                                s.status_message = format!(
                                    "AI proposed {} link(s) & {} insight node(s)",
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
                            .child(
                                text("✦ Synthesize Connections")
                                    .font(theme::CONTROL_TEXT)
                                    .color(theme::accent_violet()),
                            ),
                    ),
            )
            .child(
                el()
                    .key("btn_back_from_ai")
                    .w_full()
                    .py(8.0)
                    .radius(radius::SM)
                    .fill(theme::bg_control())
                    .hover(theme::hover_subtle())
                    .border(1.0, theme::border_subtle())
                    .center()
                    .cursor(Cursor::Pointer)
                    .on_press(|s: &mut AppState| {
                        s.ai_panel_open = false;
                    })
                    .child(
                        text("Back to Selection")
                            .font(theme::CONTROL_TEXT)
                            .color(theme::text_secondary()),
                    ),
            );
    } else if let Some(selected_node_id) = s.selected_nodes.iter().next() {
        if let Some(node) = s.graph.get_node(selected_node_id) {
            let node_id_for_delete = node.id.clone();
            let node_id_for_ingest = node.id.clone();
            let node_id_for_sample = node.id.clone();
            let node_id_for_explain = node.id.clone();
            let node_kind = node.kind;
            let kc = theme::kind_accent(node_kind);

            let word_count = node.content.split_whitespace().count();
            let char_count = node.content.len();
            let line_count = node.content.lines().count().max(1);
            let read_min = (word_count as f32 / 200.0).ceil().max(1.0) as usize;

            if node_kind == NodeKind::Document {
                // DEDICATED RIGHT-SIDE DOCUMENT READER & EDITOR
                content = content
                    .child(
                        row()
                            .w_full()
                            .center_y()
                            .gap(6.0)
                            .child(badge_pill("DOC READER", kc))
                            .child(subtle_badge(&format!("{}w", word_count), theme::text_secondary()))
                            .child(subtle_badge(&format!("~{}m", read_min), theme::text_muted()))
                            .child(el().grow(1.0))
                            .child(close_panel_btn()),
                    )
                    // Document Title
                    .child(
                        col()
                            .w_full()
                            .gap(4.0)
                            .child(
                                text("DOCUMENT TITLE")
                                    .font(theme::SECTION_LABEL)
                                    .color(theme::text_muted()),
                            )
                            .child(
                                el()
                                    .w_full()
                                    .px(10.0)
                                    .py(8.0)
                                    .radius(radius::SM)
                                    .fill(theme::bg_input())
                                    .border(1.0, theme::border_default())
                                    .child(
                                        text_input(&s.inspector_title_edit, theme::PANEL_TITLE)
                                            .key("inspector_title")
                                            .single_line()
                                            .w_full()
                                            .color(theme::text_primary())
                                            .caret(kc, kc.alpha(0.25))
                                            .placeholder("Document title...", theme::text_muted())
                                            .on_edit(|s: &mut AppState| &mut s.inspector_title_edit)
                                            .on_input_change(|s: &mut AppState, _| {
                                                s.sync_from_inspector_edits();
                                            })
                                            .on_submit(|s: &mut AppState, _| {
                                                s.sync_from_inspector_edits();
                                            }),
                                    ),
                            ),
                    )
                    // Concise Document Action Strip
                    .child(
                        row()
                            .w_full()
                            .gap(6.0)
                            .child(
                                el()
                                    .key("btn_doc_ingest")
                                    .grow(1.0)
                                    .h(28.0)
                                    .px(8.0)
                                    .radius(radius::SM)
                                    .fill(theme::accent_blue().alpha(0.14))
                                    .hover(theme::accent_blue().alpha(0.24))
                                    .border(1.0, theme::accent_blue().alpha(0.35))
                                    .center()
                                    .cursor(Cursor::Pointer)
                                    .on_press_tx(move |_s: &mut AppState, tx: &mut Tx| {
                                        let opts = FileDialogOptions {
                                            title: "Ingest Document File".into(),
                                            ..Default::default()
                                        }
                                        .filter(
                                            "Supported Files",
                                            &["pdf", "epub", "txt", "md", "json", "rs", "toml", "csv"],
                                        )
                                        .filter("Documents", &["pdf", "epub"])
                                        .filter(
                                            "Text & Markdown",
                                            &["txt", "md", "json", "rs", "toml", "csv"],
                                        );

                                        let target_id = node_id_for_ingest.clone();
                                        tx.open_file(opts, move |s: &mut AppState, res, _| match res {
                                            Ok(Some(path)) => {
                                                s.ingest_file_into_node(&target_id, &path);
                                            }
                                            Ok(None) => {}
                                            Err(e) => {
                                                s.status_message = format!("File picker error: {}", e);
                                            }
                                        });
                                    })
                                    .child(
                                        text("Ingest")
                                            .font(theme::CONTROL_TEXT)
                                            .color(theme::accent_blue()),
                                    ),
                            )
                            .child(
                                el()
                                    .key("btn_doc_sample")
                                    .grow(1.0)
                                    .h(28.0)
                                    .px(8.0)
                                    .radius(radius::SM)
                                    .fill(theme::bg_control())
                                    .hover(theme::hover_subtle())
                                    .border(1.0, theme::border_subtle())
                                    .center()
                                    .cursor(Cursor::Pointer)
                                    .on_press(move |s: &mut AppState| {
                                        s.load_sample_document(&node_id_for_sample);
                                    })
                                    .child(
                                        text("Sample")
                                            .font(theme::CONTROL_TEXT)
                                            .color(theme::text_primary()),
                                    ),
                            )
                            .child(
                                el()
                                    .key("btn_doc_summarize")
                                    .grow(1.0)
                                    .h(28.0)
                                    .px(8.0)
                                    .radius(radius::SM)
                                    .fill(theme::accent_violet().alpha(0.14))
                                    .hover(theme::accent_violet().alpha(0.24))
                                    .border(1.0, theme::accent_violet().alpha(0.35))
                                    .center()
                                    .cursor(Cursor::Pointer)
                                    .on_press(move |s: &mut AppState| {
                                        s.explain_node_ai(&node_id_for_explain);
                                    })
                                    .child(
                                        text("✦ Summarize")
                                            .font(theme::CONTROL_TEXT)
                                            .color(theme::accent_violet()),
                                    ),
                            ),
                    );

                // Asset / stats bar
                content = content.child(
                    row()
                        .gap(6.0)
                        .child(subtle_badge(&format!("{} lines", line_count), theme::text_muted()))
                        .child(subtle_badge(&format!("{} chars", char_count), theme::text_muted()))
                        .child(if let Some(hash) = &node.asset_hash {
                            subtle_badge(&format!("sha:{}", &hash[..hash.len().min(7)]), kc)
                        } else {
                            el()
                        }),
                );

                // AI Key Insights card if present
                if let Some(summary) = &node.summary {
                    content = content.child(
                        col()
                            .w_full()
                            .gap(4.0)
                            .child(
                                text("AI KEY INSIGHTS")
                                    .font(theme::SECTION_LABEL)
                                    .color(theme::accent_violet()),
                            )
                            .child(
                                el()
                                    .w_full()
                                    .padding(10.0)
                                    .radius(radius::SM)
                                    .fill(theme::accent_violet().alpha(0.10))
                                    .border(1.0, theme::accent_violet().alpha(0.30))
                                    .child(
                                        text(summary)
                                            .font(theme::BODY_TEXT)
                                            .color(theme::text_primary())
                                            .w_full()
                                            .wrap(),
                                    ),
                            ),
                    );
                }

                // Comfortable Reader Viewport
                content = content.child(
                    col()
                        .w_full()
                        .gap(4.0)
                        .child(
                            row()
                                .w_full()
                                .child(
                                    text("DOCUMENT READER")
                                        .font(theme::SECTION_LABEL)
                                        .color(theme::text_muted()),
                                )
                                .child(el().grow(1.0))
                                .child(
                                    text("Live sync")
                                        .font(theme::BADGE_TEXT)
                                        .color(theme::text_muted()),
                                ),
                        )
                        .child(
                            col()
                                .w_full()
                                .radius(radius::MD)
                                .fill(theme::bg_input())
                                .border(1.0, theme::border_default())
                                .padding(12.0)
                                .child(
                                    col()
                                        .w_full()
                                        .min_h(280.0)
                                        .max_h(460.0)
                                        .scroll(ScrollSpec { stick_bottom: false })
                                        .child(
                                            text_input(&s.inspector_content_edit, theme::READER_BODY)
                                                .key("inspector_content")
                                                .w_full()
                                                .min_h(260.0)
                                                .color(theme::text_primary())
                                                .caret(kc, kc.alpha(0.25))
                                                .placeholder(
                                                    "Document is empty. Click Ingest to load a file or type/paste text here...",
                                                    theme::text_muted(),
                                                )
                                                .on_edit(|s: &mut AppState| &mut s.inspector_content_edit)
                                                .on_input_change(|s: &mut AppState, _| {
                                                    s.sync_from_inspector_edits();
                                                })
                                                .on_submit(|s: &mut AppState, _| {
                                                    s.inspector_content_edit.insert("\n");
                                                    s.sync_from_inspector_edits();
                                                }),
                                        ),
                                ),
                        ),
                );

                // Quick append + Delete footer
                content = content
                    .child(
                        row()
                            .w_full()
                            .gap(6.0)
                            .child(
                                el()
                                    .grow(1.0)
                                    .px(8.0)
                                    .py(6.0)
                                    .radius(radius::SM)
                                    .fill(theme::bg_input())
                                    .border(1.0, theme::border_default())
                                    .child(
                                        text_input(&s.append_note_input, theme::CONTROL_TEXT)
                                            .key("append_note")
                                            .single_line()
                                            .w_full()
                                            .color(theme::text_primary())
                                            .caret(kc, kc.alpha(0.25))
                                            .placeholder("Append excerpt or note...", theme::text_muted())
                                            .on_edit(|s: &mut AppState| &mut s.append_note_input)
                                            .on_submit(|s: &mut AppState, _| {
                                                s.commit_append_input();
                                            }),
                                    ),
                            )
                            .child(
                                el()
                                    .key("btn_append_doc")
                                    .px(10.0)
                                    .py(6.0)
                                    .radius(radius::SM)
                                    .fill(theme::accent_blue().alpha(0.16))
                                    .hover(theme::accent_blue().alpha(0.25))
                                    .border(1.0, theme::accent_blue().alpha(0.35))
                                    .cursor(Cursor::Pointer)
                                    .on_press(|s: &mut AppState| {
                                        s.commit_append_input();
                                    })
                                    .child(
                                        text("+ Add")
                                            .font(theme::CONTROL_TEXT)
                                            .color(theme::accent_blue()),
                                    ),
                            ),
                    )
                    .child(
                        row()
                            .w_full()
                            .gap(6.0)
                            .child(
                                el()
                                    .key("btn_save_doc")
                                    .grow(1.0)
                                    .h(28.0)
                                    .radius(radius::SM)
                                    .fill(theme::bg_control())
                                    .hover(theme::hover_subtle())
                                    .border(1.0, theme::border_subtle())
                                    .center()
                                    .cursor(Cursor::Pointer)
                                    .on_press(|s: &mut AppState| {
                                        s.sync_node_edits();
                                        if let Some(bundle) = &s.bundle {
                                            let _ = bundle.save_graph_atomic(&s.graph);
                                        }
                                        s.status_message = "Document saved".into();
                                    })
                                    .child(
                                        text("Save")
                                            .font(theme::CONTROL_TEXT)
                                            .color(theme::text_primary()),
                                    ),
                            )
                            .child(
                                el()
                                    .key("btn_delete_doc")
                                    .px(12.0)
                                    .h(28.0)
                                    .radius(radius::SM)
                                    .fill(theme::accent_rose().alpha(0.12))
                                    .hover(theme::accent_rose().alpha(0.22))
                                    .border(1.0, theme::accent_rose().alpha(0.35))
                                    .center()
                                    .cursor(Cursor::Pointer)
                                    .on_press(move |s: &mut AppState| {
                                        s.delete_node(&node_id_for_delete);
                                    })
                                    .child(
                                        text("Delete")
                                            .font(theme::CONTROL_TEXT)
                                            .color(theme::accent_rose()),
                                    ),
                            ),
                    );
            } else {
                // NOTE / SYNTHESIS / CONCEPT INSPECTOR
                content = content
                    .child(
                        row()
                            .w_full()
                            .center_y()
                            .gap(6.0)
                            .child(badge_pill(theme::kind_label(node_kind), kc))
                            .child(subtle_badge(
                                &format!("#{}", &node.id.as_str()[..node.id.as_str().len().min(6)]),
                                theme::text_muted(),
                            ))
                            .child(el().grow(1.0))
                            .child(close_panel_btn()),
                    )
                    .child(
                        col()
                            .w_full()
                            .gap(4.0)
                            .child(
                                text("TITLE")
                                    .font(theme::SECTION_LABEL)
                                    .color(theme::text_muted()),
                            )
                            .child(
                                el()
                                    .w_full()
                                    .px(10.0)
                                    .py(8.0)
                                    .radius(radius::SM)
                                    .fill(theme::bg_input())
                                    .border(1.0, theme::border_default())
                                    .child(
                                        text_input(&s.inspector_title_edit, theme::PANEL_TITLE)
                                            .key("inspector_title")
                                            .single_line()
                                            .w_full()
                                            .color(theme::text_primary())
                                            .caret(kc, kc.alpha(0.25))
                                            .placeholder("Node title...", theme::text_muted())
                                            .on_edit(|s: &mut AppState| &mut s.inspector_title_edit)
                                            .on_input_change(|s: &mut AppState, _| {
                                                s.sync_from_inspector_edits();
                                            })
                                            .on_submit(|s: &mut AppState, _| {
                                                s.sync_from_inspector_edits();
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        row()
                            .gap(6.0)
                            .child(subtle_badge(&format!("{}w", word_count), theme::text_secondary()))
                            .child(subtle_badge(&format!("{}c", char_count), theme::text_secondary()))
                            .child(subtle_badge(&format!("~{}m", read_min), theme::text_muted())),
                    )
                    .child(
                        col()
                            .w_full()
                            .gap(4.0)
                            .child(
                                row()
                                    .w_full()
                                    .child(
                                        text("NOTES")
                                            .font(theme::SECTION_LABEL)
                                            .color(theme::text_muted()),
                                    )
                                    .child(el().grow(1.0))
                                    .child(
                                        text(format!("{} lines", line_count))
                                            .font(theme::BADGE_TEXT)
                                            .color(theme::text_muted()),
                                    ),
                            )
                            .child(
                                col()
                                    .w_full()
                                    .radius(radius::SM)
                                    .fill(theme::bg_input())
                                    .border(1.0, theme::border_default())
                                    .padding(10.0)
                                    .child(
                                        col()
                                            .w_full()
                                            .min_h(180.0)
                                            .max_h(320.0)
                                            .scroll(ScrollSpec { stick_bottom: false })
                                            .child(
                                                text_input(&s.inspector_content_edit, theme::BODY_TEXT)
                                                    .key("inspector_content")
                                                    .w_full()
                                                    .min_h(160.0)
                                                    .color(theme::text_primary())
                                                    .caret(kc, kc.alpha(0.25))
                                                    .placeholder(
                                                        "Type your notes, markdown, or study observations...",
                                                        theme::text_muted(),
                                                    )
                                                    .on_edit(|s: &mut AppState| &mut s.inspector_content_edit)
                                                    .on_input_change(|s: &mut AppState, _| {
                                                        s.sync_from_inspector_edits();
                                                    })
                                                    .on_submit(|s: &mut AppState, _| {
                                                        s.inspector_content_edit.insert("\n");
                                                        s.sync_from_inspector_edits();
                                                    }),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        col()
                            .w_full()
                            .gap(6.0)
                            .child(
                                text("FORMAT")
                                    .font(theme::SECTION_LABEL)
                                    .color(theme::text_muted()),
                            )
                            .child(
                                row()
                                    .gap(6.0)
                                    .child(quick_chip(
                                        "fmt_bullet",
                                        "• Bullet",
                                        theme::text_secondary(),
                                        |s| s.append_to_selected_node("• "),
                                    ))
                                    .child(quick_chip(
                                        "fmt_heading",
                                        "Heading",
                                        theme::text_secondary(),
                                        |s| s.append_to_selected_node("### "),
                                    ))
                                    .child(quick_chip(
                                        "fmt_todo",
                                        "Todo",
                                        theme::text_secondary(),
                                        |s| s.append_to_selected_node("- [ ] "),
                                    ))
                                    .child(quick_chip(
                                        "fmt_quote",
                                        "Quote",
                                        theme::text_secondary(),
                                        |s| s.append_to_selected_node("> "),
                                    )),
                            ),
                    );

                if let Some(summary) = &node.summary {
                    content = content.child(
                        col()
                            .w_full()
                            .gap(4.0)
                            .child(
                                text("AI SYNTHESIS")
                                    .font(theme::SECTION_LABEL)
                                    .color(theme::accent_violet()),
                            )
                            .child(
                                el()
                                    .w_full()
                                    .padding(10.0)
                                    .radius(radius::SM)
                                    .fill(theme::accent_violet().alpha(0.10))
                                    .border(1.0, theme::accent_violet().alpha(0.30))
                                    .child(
                                        text(summary)
                                            .font(theme::BODY_TEXT)
                                            .color(theme::text_primary())
                                            .w_full()
                                            .wrap(),
                                    ),
                            ),
                    );
                }

                content = content
                    .child(
                        el()
                            .key("btn_node_ai_explain")
                            .w_full()
                            .h(30.0)
                            .radius(radius::SM)
                            .fill(theme::accent_violet().alpha(0.14))
                            .hover(theme::accent_violet().alpha(0.24))
                            .border(1.0, theme::accent_violet().alpha(0.35))
                            .center()
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.explain_node_ai(&node_id_for_explain);
                            })
                            .child(
                                text("✦ Explain with AI")
                                    .font(theme::CONTROL_TEXT)
                                    .color(theme::accent_violet()),
                            ),
                    )
                    .child(
                        row()
                            .w_full()
                            .gap(6.0)
                            .child(
                                el()
                                    .key("btn_save_node")
                                    .grow(1.0)
                                    .h(28.0)
                                    .radius(radius::SM)
                                    .fill(theme::bg_control())
                                    .hover(theme::hover_subtle())
                                    .border(1.0, theme::border_subtle())
                                    .center()
                                    .cursor(Cursor::Pointer)
                                    .on_press(|s: &mut AppState| {
                                        s.sync_node_edits();
                                        if let Some(bundle) = &s.bundle {
                                            let _ = bundle.save_graph_atomic(&s.graph);
                                        }
                                        s.status_message = "Node saved".into();
                                    })
                                    .child(
                                        text("Save")
                                            .font(theme::CONTROL_TEXT)
                                            .color(theme::text_primary()),
                                    ),
                            )
                            .child(
                                el()
                                    .key("btn_delete_node")
                                    .px(12.0)
                                    .h(28.0)
                                    .radius(radius::SM)
                                    .fill(theme::accent_rose().alpha(0.12))
                                    .hover(theme::accent_rose().alpha(0.22))
                                    .border(1.0, theme::accent_rose().alpha(0.35))
                                    .center()
                                    .cursor(Cursor::Pointer)
                                    .on_press(move |s: &mut AppState| {
                                        s.delete_node(&node_id_for_delete);
                                    })
                                    .child(
                                        text("Delete")
                                            .font(theme::CONTROL_TEXT)
                                            .color(theme::accent_rose()),
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
                        .child(badge_pill("CONNECTION", theme::accent_blue()))
                        .child(el().grow(1.0))
                        .child(close_panel_btn()),
                )
                .child(
                    el()
                        .w_full()
                        .padding(10.0)
                        .radius(radius::SM)
                        .fill(theme::bg_card())
                        .border(1.0, theme::border_subtle())
                        .child(
                            text(format!("{}  →  {}", from_title, to_title))
                                .font(theme::BODY_TEXT)
                                .color(theme::accent_blue())
                                .w_full()
                                .wrap(),
                        ),
                )
                .child(
                    col()
                        .w_full()
                        .gap(6.0)
                        .child(
                            text("RELATION")
                                .font(theme::SECTION_LABEL)
                                .color(theme::text_muted()),
                        )
                        .child(
                            row()
                                .gap(6.0)
                                .child({
                                    let eid = edge_id_for_rel.clone();
                                    quick_chip(
                                        "rel_supports",
                                        "Supports",
                                        theme::accent_emerald(),
                                        move |s| {
                                            if let Some(e) = s.graph.get_edge_mut(&eid) {
                                                e.relation = EdgeRelation::Supports;
                                            }
                                        },
                                    )
                                })
                                .child({
                                    let eid = edge_id_for_rel.clone();
                                    quick_chip(
                                        "rel_contradicts",
                                        "Contradicts",
                                        theme::accent_rose(),
                                        move |s| {
                                            if let Some(e) = s.graph.get_edge_mut(&eid) {
                                                e.relation = EdgeRelation::Contradicts;
                                            }
                                        },
                                    )
                                })
                                .child({
                                    let eid = edge_id_for_rel.clone();
                                    quick_chip(
                                        "rel_explains",
                                        "Explains",
                                        theme::accent_blue(),
                                        move |s| {
                                            if let Some(e) = s.graph.get_edge_mut(&eid) {
                                                e.relation = EdgeRelation::Explains;
                                            }
                                        },
                                    )
                                }),
                        ),
                )
                .child(
                    col()
                        .w_full()
                        .gap(4.0)
                        .child(
                            text("LABEL")
                                .font(theme::SECTION_LABEL)
                                .color(theme::text_muted()),
                        )
                        .child(
                            el()
                                .w_full()
                                .px(10.0)
                                .py(8.0)
                                .radius(radius::SM)
                                .fill(theme::bg_input())
                                .border(1.0, theme::border_default())
                                .child(
                                    text_input(&s.edit_edge_label, theme::CONTROL_TEXT)
                                        .key("edge_label")
                                        .single_line()
                                        .w_full()
                                        .color(theme::text_primary())
                                        .caret(theme::accent_blue(), theme::accent_blue().alpha(0.25))
                                        .placeholder("Label...", theme::text_muted())
                                        .on_edit(|s: &mut AppState| &mut s.edit_edge_label)
                                        .on_input_change(|s: &mut AppState, _| {
                                            s.sync_edge_edits();
                                        }),
                                ),
                        ),
                )
                .child(
                    col()
                        .w_full()
                        .gap(4.0)
                        .child(
                            text("ANNOTATION")
                                .font(theme::SECTION_LABEL)
                                .color(theme::text_muted()),
                        )
                        .child(
                            col()
                                .w_full()
                                .min_h(100.0)
                                .padding(10.0)
                                .radius(radius::SM)
                                .fill(theme::bg_input())
                                .border(1.0, theme::border_default())
                                .child(
                                    text_input(&s.edit_edge_note, theme::BODY_TEXT)
                                        .key("edge_note")
                                        .w_full()
                                        .min_h(80.0)
                                        .color(theme::text_primary())
                                        .caret(theme::accent_blue(), theme::accent_blue().alpha(0.25))
                                        .placeholder("Why these nodes connect...", theme::text_muted())
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
                        .key("btn_delete_edge")
                        .w_full()
                        .h(28.0)
                        .radius(radius::SM)
                        .fill(theme::accent_rose().alpha(0.12))
                        .hover(theme::accent_rose().alpha(0.22))
                        .border(1.0, theme::accent_rose().alpha(0.35))
                        .center()
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.graph.remove_edge(&edge_id_for_delete);
                            s.selected_edge = None;
                            s.status_message = "Deleted connection".into();
                        })
                        .child(
                            text("Delete Connection")
                                .font(theme::CONTROL_TEXT)
                                .color(theme::accent_rose()),
                        ),
                );
        }
    } else {
        // Clean Empty State
        content = content
            .child(
                row()
                    .w_full()
                    .center_y()
                    .child(
                        text("Inspector")
                            .font(theme::PANEL_TITLE)
                            .color(theme::text_primary()),
                    )
                    .child(el().grow(1.0))
                    .child(close_panel_btn()),
            )
            .child(
                text("Click any note to edit directly, click a document to open the reader, or select a link wire.")
                    .font(theme::BODY_TEXT)
                    .color(theme::text_secondary())
                    .w_full()
                    .wrap(),
            )
            .child(
                col()
                    .w_full()
                    .gap(6.0)
                    .padding(10.0)
                    .radius(radius::MD)
                    .fill(theme::bg_card())
                    .border(1.0, theme::border_subtle())
                    .child(
                        text("SHORTCUTS")
                            .font(theme::SECTION_LABEL)
                            .color(theme::text_muted()),
                    )
                    .child(
                        text("• Two-finger trackpad scroll moves X & Y freely\n• Pinch or Ctrl+scroll zooms at pointer\n• Drag note header to move card\n• Drag from right port to link nodes\n• Space + drag pans anywhere")
                            .font(theme::STATUS_TEXT)
                            .color(theme::text_secondary())
                            .w_full()
                            .wrap(),
                    ),
            );
    }

    let is_doc_selected = s
        .selected_nodes
        .iter()
        .next()
        .and_then(|id| s.graph.get_node(id))
        .is_some_and(|n| n.kind == NodeKind::Document);
    let panel_width = if is_doc_selected { 420.0 } else { 340.0 };

    col()
        .key("inspector_panel")
        .w(panel_width)
        .h_full()
        .fill(theme::bg_sidebar())
        .border_sides([0.0, 0.0, 0.0, 1.0], theme::border_subtle())
        .padding(16.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(content)
}
