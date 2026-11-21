use rune::tok::space;
use rune::{
    Color, Cursor, Cx, El, FileDialogOptions, Phase, ScrollSpec, SurfaceFit, TextStyle, Tx, col,
    el, row, text, text_input,
};
use rune_kit::Markdown;
use studynodes_graph::model::{EdgeRelation, NodeKind};
use studynodes_llm::LlmProvider;

use crate::doc_reader::{self, DocFormat, ReaderMode};
use crate::state::{AppState, DocViewTab};
use crate::theme;

fn badge_pill(label: &str, color: Color) -> El {
    el()
        .px(7.0)
        .py(2.5)
        .radius(4.0)
        .fill(theme::bg_control())
        .border(1.0, theme::border_default())
        .child(text(label).font(theme::BADGE_TEXT).color(color))
}

fn subtle_badge(label: &str, text_color: Color) -> El {
    el()
        .px(6.0)
        .py(2.5)
        .radius(4.0)
        .fill(theme::bg_control())
        .border(1.0, theme::border_subtle())
        .child(text(label).font(theme::BADGE_TEXT).color(text_color))
}

fn popout_panel_btn() -> El {
    el()
        .key("btn_popout_reader")
        .size(24.0, 24.0)
        .radius(4.0)
        .fill(theme::bg_control())
        .hover(theme::hover_subtle())
        .press(theme::press_subtle())
        .border(1.0, theme::border_subtle())
        .center()
        .cursor(Cursor::Pointer)
        .on_press(|s: &mut AppState| {
            s.pop_out_reader();
        })
        .child(
            text("⧉")
                .font(TextStyle::new(12.5, 500.0).mono())
                .color(theme::text_primary()),
        )
}

fn close_panel_btn() -> El {
    el()
        .key("btn_close_inspector")
        .size(24.0, 24.0)
        .radius(4.0)
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
                .font(TextStyle::new(14.0, 500.0).mono())
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
        .radius(4.0)
        .fill(theme::bg_control())
        .hover(theme::hover_subtle())
        .press(theme::press_subtle())
        .border(1.0, theme::border_subtle())
        .cursor(Cursor::Pointer)
        .on_press(action)
        .child(text(label).font(theme::STATUS_TEXT).color(fg))
}

fn seg_tab(key: &'static str, label: &str, active: bool) -> El {
    el()
        .key(key)
        .px(8.0)
        .py(3.5)
        .radius(3.0)
        .fill(if active {
            theme::bg_card_selected()
        } else {
            Color::TRANSPARENT
        })
        .hover(theme::hover_subtle())
        .border(
            1.0,
            if active {
                theme::border_strong()
            } else {
                Color::TRANSPARENT
            },
        )
        .center()
        .cursor(Cursor::Pointer)
        .child(
            text(label)
                .font(theme::BADGE_TEXT)
                .color(if active {
                    theme::text_primary()
                } else {
                    theme::text_muted()
                }),
        )
}

pub fn render_inspector(s: &AppState, _: &mut Cx<AppState>) -> El {
    let mut content = col().w_full().gap(space::S3);

    if s.ai_panel_open {
        // AI Synthesis Panel (Claude Code monochrome + restrained terracotta accent)
        content = content
            .child(
                row()
                    .w_full()
                    .center_y()
                    .gap(space::S2)
                    .child(
                        text("✦")
                            .font(theme::CONTROL_TEXT)
                            .color(theme::accent_claude()),
                    )
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
                    .radius(4.0)
                    .fill(theme::accent_claude().alpha(0.16))
                    .hover(theme::accent_claude().alpha(0.24))
                    .press(theme::accent_claude().alpha(0.32))
                    .border(1.0, theme::accent_claude().alpha(0.50))
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
                                    "✦ AI proposed {} link(s) & {} insight node(s)",
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
                                    .color(theme::text_primary()),
                            ),
                    ),
            )
            .child(
                el()
                    .key("btn_back_from_ai")
                    .w_full()
                    .py(8.0)
                    .radius(4.0)
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
            let node_id_for_ext = node.id.clone();
            let node_id_for_ext_btn = node.id.clone();
            let node_kind = node.kind;
            let doc_format = s.node_doc_format(&node.id);
            let fmt_badge = doc_reader::format_badge(doc_format);

            let word_count = node.content.split_whitespace().count();
            let char_count = node.content.len();
            let line_count = node.content.lines().count().max(1);
            let read_min = (word_count as f32 / 200.0).ceil().max(1.0) as usize;

            if matches!(node_kind, NodeKind::Document | NodeKind::Image) {
                let is_integrated = s.reader_prefs.mode == ReaderMode::Integrated;
                let has_pdf = s
                    .active_pdf
                    .as_ref()
                    .is_some_and(|p| p.node_id == node.id);

                // INTEGRATED & CUSTOM DOCUMENT READER HEADER
                content = content
                    .child(
                        row()
                            .w_full()
                            .center_y()
                            .gap(5.0)
                            .child(badge_pill(fmt_badge, theme::text_primary()))
                            .child(subtle_badge(&format!("{}w", word_count), theme::text_secondary()))
                            .child(subtle_badge(&format!("~{}m", read_min), theme::text_muted()))
                            .child(el().grow(1.0))
                            // Integrated | Custom switcher
                            .child(
                                row()
                                    .key("reader_mode_switch")
                                    .padding(2.0)
                                    .gap(2.0)
                                    .radius(4.0)
                                    .fill(theme::bg_input())
                                    .border(1.0, theme::border_subtle())
                                    .child(
                                        seg_tab("reader_mode_integrated", "Built-in", is_integrated)
                                            .on_press(|s: &mut AppState| {
                                                s.set_reader_mode(ReaderMode::Integrated);
                                                s.reader_settings_open = false;
                                            }),
                                    )
                                    .child(
                                        seg_tab("reader_mode_custom", "Custom", !is_integrated)
                                            .on_press(|s: &mut AppState| {
                                                s.set_reader_mode(ReaderMode::CustomExternal);
                                                s.reader_settings_open = true;
                                            }),
                                    ),
                            )
                            .child(popout_panel_btn())
                            .child(
                                el()
                                    .key("btn_open_external_reader")
                                    .size(24.0, 24.0)
                                    .radius(4.0)
                                    .fill(theme::bg_control())
                                    .hover(theme::hover_subtle())
                                    .border(1.0, theme::border_subtle())
                                    .center()
                                    .cursor(Cursor::Pointer)
                                    .on_press(move |s: &mut AppState| {
                                        s.open_node_in_external_reader(&node_id_for_ext);
                                    })
                                    .child(
                                        text("↗")
                                            .font(theme::CONTROL_TEXT)
                                            .color(theme::text_primary()),
                                    ),
                            )
                            .child(close_panel_btn()),
                    );

                // Optional Custom Reader Configuration Strip (visible when Custom mode active)
                if !is_integrated || s.reader_settings_open {
                    content = content.child(
                        col()
                            .key("custom_reader_box")
                            .w_full()
                            .gap(6.0)
                            .padding(10.0)
                            .radius(4.0)
                            .fill(theme::bg_card())
                            .border(1.0, theme::border_default())
                            .child(
                                row()
                                    .w_full()
                                    .center_y()
                                    .child(
                                        text("CUSTOM READER EXECUTABLE")
                                            .font(theme::SECTION_LABEL)
                                            .color(theme::text_secondary()),
                                    )
                                    .child(el().grow(1.0))
                                    .child(quick_chip(
                                        "btn_reset_custom_reader",
                                        "OS Default",
                                        theme::text_muted(),
                                        |s| s.save_custom_reader_path(String::new()),
                                    )),
                            )
                            .child(
                                row()
                                    .w_full()
                                    .gap(6.0)
                                    .child(
                                        el()
                                            .grow(1.0)
                                            .min_w(0.0)
                                            .px(8.0)
                                            .py(5.5)
                                            .radius(4.0)
                                            .fill(theme::bg_input())
                                            .border(1.0, theme::border_default())
                                            .child(
                                                text_input(&s.custom_reader_edit, theme::STATUS_TEXT)
                                                    .key("custom_reader_path_input")
                                                    .single_line()
                                                    .w_full()
                                                    .color(theme::text_primary())
                                                    .caret(
                                                        theme::accent_claude(),
                                                        theme::accent_claude().alpha(0.25),
                                                    )
                                                    .placeholder(
                                                        "OS default (or path e.g. SumatraPDF.exe)...",
                                                        theme::text_muted(),
                                                    )
                                                    .on_edit(|s: &mut AppState| &mut s.custom_reader_edit)
                                                    .on_input_change(|s: &mut AppState, _| {
                                                        let path = s.custom_reader_edit.text.clone();
                                                        s.save_custom_reader_path(path);
                                                    })
                                                    .on_submit(|s: &mut AppState, _| {
                                                        let path = s.custom_reader_edit.text.clone();
                                                        s.save_custom_reader_path(path);
                                                    }),
                                            ),
                                    )
                                    .child(
                                        el()
                                            .key("btn_browse_custom_reader")
                                            .px(8.0)
                                            .h(26.0)
                                            .radius(4.0)
                                            .fill(theme::bg_control())
                                            .hover(theme::hover_subtle())
                                            .border(1.0, theme::border_subtle())
                                            .center()
                                            .cursor(Cursor::Pointer)
                                            .on_press_tx(|_s: &mut AppState, tx: &mut Tx| {
                                                let opts = FileDialogOptions {
                                                    title: "Select Custom Reader Application".into(),
                                                    ..Default::default()
                                                };
                                                tx.open_file(opts, |s: &mut AppState, res, _| {
                                                    if let Ok(Some(path)) = res {
                                                        s.save_custom_reader_path(
                                                            path.to_string_lossy().into_owned(),
                                                        );
                                                    }
                                                });
                                            })
                                            .child(
                                                text("Browse")
                                                    .font(theme::STATUS_TEXT)
                                                    .color(theme::text_primary()),
                                            ),
                                    ),
                            )
                            .child(
                                el()
                                    .key("btn_launch_custom_now")
                                    .w_full()
                                    .h(28.0)
                                    .radius(4.0)
                                    .fill(theme::accent_primary())
                                    .hover(theme::accent_primary().alpha(0.88))
                                    .center()
                                    .cursor(Cursor::Pointer)
                                    .on_press(move |s: &mut AppState| {
                                        s.open_node_in_external_reader(&node_id_for_ext_btn);
                                    })
                                    .child(
                                        text("Open in Custom Reader ↗")
                                            .font(theme::CONTROL_TEXT)
                                            .color(theme::text_inverted()),
                                    ),
                            ),
                    );
                }

                // Document Title
                content = content
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
                                    .py(7.0)
                                    .radius(4.0)
                                    .fill(theme::bg_input())
                                    .border(1.0, theme::border_default())
                                    .child(
                                        text_input(&s.inspector_title_edit, theme::PANEL_TITLE)
                                            .key("inspector_title")
                                            .single_line()
                                            .w_full()
                                            .color(theme::text_primary())
                                            .caret(
                                                theme::accent_claude(),
                                                theme::accent_claude().alpha(0.25),
                                            )
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
                                    .radius(4.0)
                                    .fill(theme::accent_primary())
                                    .hover(theme::accent_primary().alpha(0.88))
                                    .center()
                                    .cursor(Cursor::Pointer)
                                    .on_press_tx(move |_s: &mut AppState, tx: &mut Tx| {
                                        let opts = FileDialogOptions {
                                            title: "Ingest Document File".into(),
                                            ..Default::default()
                                        }
                                        .filter(
                                            "All Documents",
                                            &[
                                                "pdf", "md", "markdown", "docx", "odt", "pptx",
                                                "xlsx", "epub", "html", "htm", "rtf", "csv", "tsv",
                                                "txt", "json", "rs", "py", "js", "ts", "png", "jpg",
                                                "jpeg", "webp", "svg",
                                            ],
                                        )
                                        .filter("PDF & eBooks", &["pdf", "epub"])
                                        .filter("Office", &["docx", "odt", "pptx", "xlsx", "rtf"])
                                        .filter(
                                            "Markdown & Code",
                                            &["md", "markdown", "txt", "json", "rs", "py", "csv", "html"],
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
                                            .color(theme::text_inverted()),
                                    ),
                            )
                            .child(
                                el()
                                    .key("btn_doc_sample")
                                    .grow(1.0)
                                    .h(28.0)
                                    .px(8.0)
                                    .radius(4.0)
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
                                    .radius(4.0)
                                    .fill(theme::bg_control())
                                    .hover(theme::hover_subtle())
                                    .border(1.0, theme::border_subtle())
                                    .center()
                                    .cursor(Cursor::Pointer)
                                    .on_press(move |s: &mut AppState| {
                                        s.explain_node_ai(&node_id_for_explain);
                                    })
                                    .child(
                                        row()
                                            .center_y()
                                            .gap(4.0)
                                            .child(
                                                text("✦")
                                                    .font(theme::CONTROL_TEXT)
                                                    .color(theme::accent_claude()),
                                            )
                                            .child(
                                                text("Summarize")
                                                    .font(theme::CONTROL_TEXT)
                                                    .color(theme::text_primary()),
                                            ),
                                    ),
                            ),
                    );

                // AI Key Insights card if present
                if let Some(summary) = &node.summary {
                    content = content.child(
                        col()
                            .w_full()
                            .gap(4.0)
                            .child(
                                text("✦ AI KEY INSIGHTS")
                                    .font(theme::SECTION_LABEL)
                                    .color(theme::accent_claude()),
                            )
                            .child(
                                el()
                                    .w_full()
                                    .padding(10.0)
                                    .radius(4.0)
                                    .fill(theme::bg_card())
                                    .border(1.0, theme::accent_claude().alpha(0.35))
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

                // Integrated Reader Mode Tabs + Stats Header
                let mut tabs_row = row()
                    .w_full()
                    .center_y()
                    .gap(6.0)
                    .child(
                        text("READER")
                            .font(theme::SECTION_LABEL)
                            .color(theme::text_muted()),
                    )
                    .child(subtle_badge(&format!("{} lines", line_count), theme::text_muted()))
                    .child(subtle_badge(&format!("{} chars", char_count), theme::text_muted()))
                    .child(el().grow(1.0));

                let mut view_switcher = row()
                    .key("doc_view_tabs")
                    .padding(2.0)
                    .gap(2.0)
                    .radius(4.0)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_subtle());

                if has_pdf {
                    view_switcher = view_switcher.child(
                        seg_tab("tab_doc_pages", "Pages", s.doc_view_tab == DocViewTab::Pages)
                            .on_press(|s: &mut AppState| {
                                s.doc_view_tab = DocViewTab::Pages;
                            }),
                    );
                }
                view_switcher = view_switcher
                    .child(
                        seg_tab(
                            "tab_doc_preview",
                            "Preview",
                            s.doc_view_tab == DocViewTab::Preview
                                || (!has_pdf && s.doc_view_tab == DocViewTab::Pages),
                        )
                        .on_press(|s: &mut AppState| {
                            s.doc_view_tab = DocViewTab::Preview;
                        }),
                    )
                    .child(
                        seg_tab(
                            "tab_doc_source",
                            "Source",
                            s.doc_view_tab == DocViewTab::Source,
                        )
                        .on_press(|s: &mut AppState| {
                            s.doc_view_tab = DocViewTab::Source;
                        }),
                    );

                tabs_row = tabs_row.child(view_switcher);

                // Reader Content Body
                let reader_body = if has_pdf && s.doc_view_tab == DocViewTab::Pages {
                    let pdf = s.active_pdf.as_ref().unwrap();
                    let cur_page = pdf.page_index;
                    let total_pages = pdf.page_count.max(1);
                    let cur_zoom = pdf.zoom;
                    let aspect = if pdf.page_size.0 > 0 && pdf.page_size.1 > 0 {
                        Some(pdf.page_size.0 as f32 / pdf.page_size.1 as f32)
                    } else {
                        None
                    };

                    let mut pdf_col = col()
                        .key("integrated_pdf_viewer")
                        .w_full()
                        .gap(8.0)
                        .child(
                            row()
                                .w_full()
                                .center_y()
                                .gap(6.0)
                                .padding(6.0)
                                .radius(4.0)
                                .fill(theme::bg_card())
                                .border(1.0, theme::border_subtle())
                                .child(quick_chip("btn_pdf_prev", "‹ Prev", theme::text_primary(), move |s| {
                                    s.set_pdf_page(cur_page.saturating_sub(1));
                                }))
                                .child(
                                    text(format!("{} / {}", cur_page + 1, total_pages))
                                        .font(theme::STATUS_TEXT)
                                        .color(theme::text_secondary()),
                                )
                                .child(quick_chip("btn_pdf_next", "Next ›", theme::text_primary(), move |s| {
                                    s.set_pdf_page(cur_page + 1);
                                }))
                                .child(el().grow(1.0))
                                .child(quick_chip("btn_pdf_zout", "−", theme::text_secondary(), move |s| {
                                    s.set_pdf_zoom(cur_zoom - 0.25);
                                }))
                                .child(
                                    text(format!("{}%", (cur_zoom * 100.0).round() as u32))
                                        .font(theme::BADGE_TEXT)
                                        .color(theme::text_muted()),
                                )
                                .child(quick_chip("btn_pdf_zin", "+", theme::text_secondary(), move |s| {
                                    s.set_pdf_zoom(cur_zoom + 0.25);
                                })),
                        );

                    if let Some(err) = &pdf.error {
                        pdf_col = pdf_col.child(
                            el()
                                .w_full()
                                .padding(10.0)
                                .radius(4.0)
                                .fill(theme::bg_input())
                                .border(1.0, theme::accent_danger().alpha(0.45))
                                .child(
                                    text(format!("PDF render note: {} — switch to Text Preview or open via ↗.", err))
                                        .font(theme::BODY_TEXT)
                                        .color(theme::text_secondary())
                                        .w_full()
                                        .wrap(),
                                ),
                        );
                    } else {
                        let mut page_el = el()
                            .key(format!("pdf_page_canvas_{}", cur_page))
                            .w_full()
                            .radius(4.0)
                            .border(1.0, theme::border_default())
                            .clip_soft(0.0)
                            .pixels_fit(&pdf.pixels, SurfaceFit::Fit);
                        if let Some(ar) = aspect {
                            page_el = page_el.aspect_ratio(ar);
                        } else {
                            page_el = page_el.min_h(320.0);
                        }
                        pdf_col = pdf_col.child(
                            col()
                                .w_full()
                                .max_h(470.0)
                                .scroll(ScrollSpec { stick_bottom: false })
                                .child(page_el),
                        );
                    }
                    pdf_col
                } else if s.doc_view_tab == DocViewTab::Source {
                    col()
                        .key("integrated_source_editor")
                        .w_full()
                        .min_h(260.0)
                        .max_h(470.0)
                        .scroll(ScrollSpec { stick_bottom: false })
                        .child(
                            text_input(&s.inspector_content_edit, theme::MONO_BODY)
                                .key("inspector_content")
                                .w_full()
                                .min_h(250.0)
                                .color(theme::text_primary())
                                .caret(
                                    theme::accent_claude(),
                                    theme::accent_claude().alpha(0.25),
                                )
                                .placeholder(
                                    "Markdown or raw document text...",
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
                        )
                } else {
                    // Integrated Rich Interpreted Markdown (+ Image preview if Image asset)
                    let mut preview_col = col()
                        .key("integrated_md_reader")
                        .w_full()
                        .gap(10.0)
                        .min_h(240.0)
                        .max_h(470.0)
                        .scroll(ScrollSpec { stick_bottom: false });

                    if let Some((img_nid, _, img)) = &s.active_image {
                        if img_nid == &node.id && doc_format == DocFormat::Image {
                            preview_col = preview_col.child(
                                el()
                                    .key("integrated_image_preview")
                                    .w_full()
                                    .min_h(220.0)
                                    .max_h(340.0)
                                    .radius(4.0)
                                    .border(1.0, theme::border_default())
                                    .pixels_fit(img.pixels(), SurfaceFit::Fit),
                            );
                        }
                    }

                    let md_src = if node.content.trim().is_empty() {
                        "*Empty document. Click **Ingest** to load `.pdf`, `.docx`, `.epub`, `.md`, `.odt`, `.pptx`, `.xlsx`, `.html`, `.rtf`, `.csv`, or switch to **Source** to write Markdown.*".to_string()
                    } else {
                        doc_reader::wrap_for_preview(&node.content, doc_format)
                    };

                    let panel_width = s.inspector_width.clamp(300.0, 860.0);
                    let md_w = (panel_width - 76.0).max(220.0);
                    let md_style = theme::markdown_style().text_width(md_w);
                    let md_el = Markdown::new(&md_src)
                        .selectable(true)
                        .on_task(|s: &mut AppState, _block_key, offset, checked| {
                            s.toggle_selected_doc_task(offset, checked);
                        })
                        .render(&md_style);

                    preview_col.child(md_el)
                };

                content = content.child(
                    col()
                        .w_full()
                        .gap(6.0)
                        .child(tabs_row)
                        .child(
                            col()
                                .w_full()
                                .radius(4.0)
                                .fill(theme::bg_input())
                                .border(1.0, theme::border_default())
                                .padding(12.0)
                                .child(reader_body),
                        ),
                );

                // Quick append + Save / Delete footer
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
                                    .radius(4.0)
                                    .fill(theme::bg_input())
                                    .border(1.0, theme::border_default())
                                    .child(
                                        text_input(&s.append_note_input, theme::CONTROL_TEXT)
                                            .key("append_note")
                                            .single_line()
                                            .w_full()
                                            .color(theme::text_primary())
                                            .caret(
                                                theme::accent_claude(),
                                                theme::accent_claude().alpha(0.25),
                                            )
                                            .placeholder(
                                                "Append markdown note or excerpt...",
                                                theme::text_muted(),
                                            )
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
                                    .radius(4.0)
                                    .fill(theme::bg_control())
                                    .hover(theme::hover_subtle())
                                    .border(1.0, theme::border_default())
                                    .cursor(Cursor::Pointer)
                                    .on_press(|s: &mut AppState| {
                                        s.commit_append_input();
                                    })
                                    .child(
                                        text("+ Add")
                                            .font(theme::CONTROL_TEXT)
                                            .color(theme::text_primary()),
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
                                    .radius(4.0)
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
                                    .radius(4.0)
                                    .fill(theme::accent_danger().alpha(0.12))
                                    .hover(theme::accent_danger().alpha(0.22))
                                    .border(1.0, theme::accent_danger().alpha(0.35))
                                    .center()
                                    .cursor(Cursor::Pointer)
                                    .on_press(move |s: &mut AppState| {
                                        s.delete_node(&node_id_for_delete);
                                    })
                                    .child(
                                        text("Delete")
                                            .font(theme::CONTROL_TEXT)
                                            .color(theme::accent_danger()),
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
                            .child(badge_pill(
                                theme::kind_label(node_kind),
                                theme::kind_accent(node_kind),
                            ))
                            .child(subtle_badge(
                                &format!("#{}", &node.id.as_str()[..node.id.as_str().len().min(6)]),
                                theme::text_muted(),
                            ))
                            .child(el().grow(1.0))
                            .child(popout_panel_btn())
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
                                    .radius(4.0)
                                    .fill(theme::bg_input())
                                    .border(1.0, theme::border_default())
                                    .child(
                                        text_input(&s.inspector_title_edit, theme::PANEL_TITLE)
                                            .key("inspector_title")
                                            .single_line()
                                            .w_full()
                                            .color(theme::text_primary())
                                            .caret(
                                                theme::accent_claude(),
                                                theme::accent_claude().alpha(0.25),
                                            )
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
                                    .radius(4.0)
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
                                                    .caret(
                                                        theme::accent_claude(),
                                                        theme::accent_claude().alpha(0.25),
                                                    )
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
                                        |s| s.append_to_selected_node("- "),
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
                                text("✦ AI SYNTHESIS")
                                    .font(theme::SECTION_LABEL)
                                    .color(theme::accent_claude()),
                            )
                            .child(
                                el()
                                    .w_full()
                                    .padding(10.0)
                                    .radius(4.0)
                                    .fill(theme::bg_card())
                                    .border(1.0, theme::accent_claude().alpha(0.35))
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
                            .radius(4.0)
                            .fill(theme::accent_claude().alpha(0.14))
                            .hover(theme::accent_claude().alpha(0.22))
                            .border(1.0, theme::accent_claude().alpha(0.45))
                            .center()
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.explain_node_ai(&node_id_for_explain);
                            })
                            .child(
                                text("✦ Explain with AI")
                                    .font(theme::CONTROL_TEXT)
                                    .color(theme::text_primary()),
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
                                    .radius(4.0)
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
                                    .radius(4.0)
                                    .fill(theme::accent_danger().alpha(0.12))
                                    .hover(theme::accent_danger().alpha(0.22))
                                    .border(1.0, theme::accent_danger().alpha(0.35))
                                    .center()
                                    .cursor(Cursor::Pointer)
                                    .on_press(move |s: &mut AppState| {
                                        s.delete_node(&node_id_for_delete);
                                    })
                                    .child(
                                        text("Delete")
                                            .font(theme::CONTROL_TEXT)
                                            .color(theme::accent_danger()),
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
                        .child(badge_pill("CONNECTION", theme::text_primary()))
                        .child(el().grow(1.0))
                        .child(close_panel_btn()),
                )
                .child(
                    el()
                        .w_full()
                        .padding(10.0)
                        .radius(4.0)
                        .fill(theme::bg_card())
                        .border(1.0, theme::border_subtle())
                        .child(
                            text(format!("{}  →  {}", from_title, to_title))
                                .font(theme::MONO_BODY)
                                .color(theme::text_primary())
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
                                        theme::text_primary(),
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
                                        theme::accent_claude(),
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
                                        theme::text_secondary(),
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
                                .radius(4.0)
                                .fill(theme::bg_input())
                                .border(1.0, theme::border_default())
                                .child(
                                    text_input(&s.edit_edge_label, theme::CONTROL_TEXT)
                                        .key("edge_label")
                                        .single_line()
                                        .w_full()
                                        .color(theme::text_primary())
                                        .caret(
                                            theme::accent_claude(),
                                            theme::accent_claude().alpha(0.25),
                                        )
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
                                .radius(4.0)
                                .fill(theme::bg_input())
                                .border(1.0, theme::border_default())
                                .child(
                                    text_input(&s.edit_edge_note, theme::BODY_TEXT)
                                        .key("edge_note")
                                        .w_full()
                                        .min_h(80.0)
                                        .color(theme::text_primary())
                                        .caret(
                                            theme::accent_claude(),
                                            theme::accent_claude().alpha(0.25),
                                        )
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
                        .radius(4.0)
                        .fill(theme::accent_danger().alpha(0.12))
                        .hover(theme::accent_danger().alpha(0.22))
                        .border(1.0, theme::accent_danger().alpha(0.35))
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
                                .color(theme::accent_danger()),
                        ),
                );
        }
    } else {
        // Clean Monochrome Empty State
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
                text("Click any note to edit directly, click a document to open the integrated reader, or select a link wire.")
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
                    .radius(4.0)
                    .fill(theme::bg_card())
                    .border(1.0, theme::border_subtle())
                    .child(
                        text("SHORTCUTS")
                            .font(theme::SECTION_LABEL)
                            .color(theme::text_muted()),
                    )
                    .child(
                        text("• Two-finger trackpad scroll pans X & Y\n• Pinch zooms out, expand zooms in\n• Drag left edge of panel to resize\n• Click ⧉ to pop reader into full window\n• Drag note header to move, port to link")
                            .font(theme::STATUS_TEXT)
                            .color(theme::text_secondary())
                            .w_full()
                            .wrap(),
                    ),
            );
    }

    let panel_width = s.inspector_width.clamp(300.0, 860.0);

    let resize_handle = el()
        .key("inspector_resize_handle")
        .w(6.0)
        .h_full()
        .fill(theme::bg_sidebar())
        .hover(theme::accent_claude().alpha(0.32))
        .border_sides([0.0, 0.0, 0.0, 1.0], theme::border_subtle())
        .center()
        .cursor(Cursor::ResizeColumn)
        .on_drag(|s: &mut AppState, d, _| match d.phase {
            Phase::Down => {
                s.inspector_resize_start_w = s.inspector_width;
            }
            Phase::Move => {
                s.inspector_width = (s.inspector_resize_start_w - d.dx).clamp(300.0, 860.0);
            }
            Phase::Up => {
                s.reader_prefs.inspector_width = s.inspector_width;
                let _ = doc_reader::save_preferences(&s.projects_dir, &s.reader_prefs);
            }
        })
        .child(
            el().w(1.5)
                .h(36.0)
                .radius(1.0)
                .fill(theme::border_strong()),
        );

    let panel = col()
        .key("inspector_panel")
        .w(panel_width)
        .h_full()
        .fill(theme::bg_sidebar())
        .padding(16.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(content);

    row()
        .key("inspector_container")
        .h_full()
        .child(resize_handle)
        .child(panel)
}
