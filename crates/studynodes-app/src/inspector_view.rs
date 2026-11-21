use rune::{
    Color, Cursor, Cx, El, FileDialogOptions, Phase, ScrollSpec, SurfaceFit, TextStyle, Tx, col,
    el, row, text, text_input,
};
use rune_kit::Markdown;
use studynodes_graph::model::{EdgeRelation, Node, NodeKind};
use studynodes_vision::{OcrEnginePreference, OcrMode, SUPPORTED_LANGUAGES};

use crate::doc_reader::{self, DocFormat, ReaderMode};
use crate::formula_math;
use crate::state::{AppState, DocViewTab};
use crate::theme;

const SHEET_TITLE: TextStyle = TextStyle::new(19.0, 600.0).lh(24.0);

#[inline]
fn hairline() -> El {
    el().w_full().h(1.0).fill(theme::border_subtle())
}

fn subtle_tag(label: &str, color: Color) -> El {
    text(label).font(theme::SECTION_LABEL).color(color)
}

fn ghost_btn(
    key: &'static str,
    label: &str,
    fg: Color,
    action: impl Fn(&mut AppState) + 'static,
) -> El {
    el()
        .key(key)
        .px(7.0)
        .py(4.0)
        .radius(6.0)
        .fill(Color::TRANSPARENT)
        .hover(theme::hover_subtle())
        .press(theme::press_subtle())
        .center()
        .cursor(Cursor::Pointer)
        .on_press(action)
        .child(text(label).font(theme::STATUS_TEXT).color(fg))
}

fn ghost_icon_btn(
    key: &'static str,
    glyph: &'static str,
    fg: Color,
    active: bool,
    action: impl Fn(&mut AppState) + 'static,
) -> El {
    el()
        .key(key)
        .size(26.0, 26.0)
        .radius(6.0)
        .fill(if active {
            theme::hover_subtle()
        } else {
            Color::TRANSPARENT
        })
        .hover(theme::hover_subtle())
        .press(theme::press_subtle())
        .center()
        .cursor(Cursor::Pointer)
        .on_press(action)
        .child(
            text(glyph)
                .font(TextStyle::new(13.0, 500.0).mono())
                .color(fg),
        )
}

fn underline_tab(key: &'static str, label: &str, active: bool) -> El {
    el()
        .key(key)
        .px(4.0)
        .py(5.0)
        .border_sides(
            [0.0, 0.0, 1.5, 0.0],
            if active {
                theme::accent_claude()
            } else {
                Color::TRANSPARENT
            },
        )
        .hover(theme::hover_subtle())
        .center()
        .cursor(Cursor::Pointer)
        .child(
            text(label)
                .font(theme::STATUS_TEXT)
                .color(if active {
                    theme::text_primary()
                } else {
                    theme::text_muted()
                }),
        )
}

fn popout_panel_btn() -> El {
    ghost_icon_btn("btn_popout_reader", "⧉", theme::text_secondary(), false, |s| {
        s.pop_out_reader();
    })
}

fn close_panel_btn() -> El {
    ghost_icon_btn("btn_close_inspector", "×", theme::text_muted(), false, |s| {
        s.sync_node_edits();
        s.sync_edge_edits();
        s.inspector_open = false;
        s.ai_panel_open = false;
        s.inspector_more_menu_open = false;
    })
}

fn more_menu_item(
    key: &'static str,
    label: &str,
    fg: Color,
    action: impl Fn(&mut AppState) + 'static,
) -> El {
    el()
        .key(key)
        .w_full()
        .px(10.0)
        .py(6.0)
        .radius(6.0)
        .hover(theme::hover_subtle())
        .cursor(Cursor::Pointer)
        .on_press(action)
        .child(text(label).font(theme::STATUS_TEXT).color(fg))
}

pub fn render_inspector(s: &AppState, _: &mut Cx<AppState>) -> El {
    let panel_width = s.inspector_width.clamp(300.0, 860.0);

    // Flush 1px hairline with invisible 6px resize hit edge
    let resize_handle = el()
        .key("inspector_resize_handle")
        .w(6.0)
        .h_full()
        .fill(Color::TRANSPARENT)
        .hover(theme::accent_claude().alpha(0.32))
        .border_sides([0.0, 0.0, 0.0, 1.0], theme::border_subtle())
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
        });

    // Distraction-Free Read-Only Reader layout (Document / Image) when active
    if !s.ai_panel_open && s.reader_read_only {
        if let Some(selected_node_id) = s.selected_nodes.iter().next() {
            if let Some(node) = s.graph.get_node(selected_node_id) {
                if matches!(node.kind, NodeKind::Document | NodeKind::Image) {
                    let ro_panel = render_readonly_doc_panel(s, node, panel_width);
                    return row()
                        .key("inspector_container")
                        .h_full()
                        .child(resize_handle)
                        .child(ro_panel);
                }
            }
        }
    }

    let mut content = col().w_full().gap(14.0);

    if s.ai_panel_open {
        content = content
            .child(
                row()
                    .w_full()
                    .center_y()
                    .gap(8.0)
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
                text("Synthesize connections, surface contradictions, or generate study cards across selected nodes.")
                    .font(theme::BODY_TEXT)
                    .color(theme::text_secondary())
                    .w_full()
                    .wrap(),
            )
            .child(hairline())
            .child(
                el()
                    .key("btn_run_ai_synth")
                    .w_full()
                    .px(12.0)
                    .py(10.0)
                    .radius(8.0)
                    .fill(theme::accent_claude().alpha(0.14))
                    .hover(theme::accent_claude().alpha(0.24))
                    .border(1.0, theme::accent_claude().alpha(0.45))
                    .center()
                    .cursor(Cursor::Pointer)
                    .on_press(|s: &mut AppState| {
                        s.run_ai_synthesize_connections();
                    })
                    .child(
                        text("✦ Synthesize Connections")
                            .font(theme::CONTROL_TEXT)
                            .color(theme::text_primary()),
                    ),
            )
            .child(
                el()
                    .key("btn_ai_generate_cards")
                    .w_full()
                    .px(12.0)
                    .py(9.0)
                    .radius(8.0)
                    .hover(theme::hover_subtle())
                    .border(1.0, theme::border_subtle())
                    .center()
                    .cursor(Cursor::Pointer)
                    .on_press(|s: &mut AppState| {
                        s.generate_study_cards_ai();
                    })
                    .child(
                        text("◇ Generate Flashcards & Checklist")
                            .font(theme::CONTROL_TEXT)
                            .color(theme::text_primary()),
                    ),
            )
            .child(
                ghost_btn("btn_back_from_ai", "← Back to Selection", theme::text_muted(), |s| {
                    s.ai_panel_open = false;
                }),
            );
    } else if let Some(selected_node_id) = s.selected_nodes.iter().next() {
        if let Some(node) = s.graph.get_node(selected_node_id) {
            let node_kind = node.kind;
            if matches!(node_kind, NodeKind::Document | NodeKind::Image) {
                content = render_edit_doc_inspector(s, node);
            } else {
                content = render_generic_node_inspector(s, node);
            }
        }
    } else if let Some(edge_id) = &s.selected_edge {
        if let Some(edge) = s.graph.get_edge(edge_id) {
            content = render_edge_inspector(s, edge);
        }
    } else {
        content = content
            .child(
                row()
                    .w_full()
                    .center_y()
                    .child(
                        text("INSPECTOR")
                            .font(theme::SECTION_LABEL)
                            .color(theme::text_muted()),
                    )
                    .child(el().grow(1.0))
                    .child(close_panel_btn()),
            )
            .child(
                text("Select any node to read or edit its sheet, or click a wire to annotate a connection.")
                    .font(theme::BODY_TEXT)
                    .color(theme::text_secondary())
                    .w_full()
                    .wrap(),
            )
            .child(hairline())
            .child(
                col()
                    .w_full()
                    .gap(6.0)
                    .child(
                        text("GESTURES & TIPS")
                            .font(theme::SECTION_LABEL)
                            .color(theme::text_muted()),
                    )
                    .child(
                        text("· Two-finger trackpad scroll pans X & Y\n· Pinch zooms out, expand zooms in\n· Drag left edge to resize sheet\n· Click ⧉ to open in a full window")
                            .font(theme::STATUS_TEXT)
                            .color(theme::text_muted())
                            .w_full()
                            .wrap(),
                    ),
            );
    }

    let panel = col()
        .key("inspector_panel")
        .w(panel_width)
        .h_full()
        .fill(theme::bg_chrome())
        .px(22.0)
        .py(16.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(content);

    row()
        .key("inspector_container")
        .h_full()
        .child(resize_handle)
        .child(panel)
}

fn render_ingest_button(node_id: studynodes_graph::model::NodeId, is_img: bool) -> El {
    ghost_btn(
        "btn_doc_ingest",
        if is_img { "Choose Image" } else { "Ingest File" },
        theme::text_secondary(),
        |_| {},
    )
    .on_press_tx(move |_s: &mut AppState, tx: &mut Tx| {
        let opts = if is_img {
            FileDialogOptions {
                title: "Choose Image File".into(),
                ..Default::default()
            }
            .filter(
                "Images",
                &["png", "jpg", "jpeg", "webp", "gif", "bmp", "svg"],
            )
        } else {
            FileDialogOptions {
                title: "Ingest Document File".into(),
                ..Default::default()
            }
            .filter(
                "All Documents",
                &[
                    "pdf", "md", "markdown", "docx", "odt", "pptx", "xlsx", "epub", "html", "htm",
                    "rtf", "csv", "tsv", "txt", "json", "rs", "py", "js", "ts", "png", "jpg",
                    "jpeg", "webp", "svg",
                ],
            )
            .filter("PDF & eBooks", &["pdf", "epub"])
            .filter("Office", &["docx", "odt", "pptx", "xlsx", "rtf"])
            .filter(
                "Markdown & Code",
                &["md", "markdown", "txt", "json", "rs", "py", "csv", "html"],
            )
        };

        let target_id = node_id.clone();
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
}

fn render_custom_reader_box(
    s: &AppState,
    node_id_for_ext_btn: studynodes_graph::model::NodeId,
) -> El {
    let is_integrated = s.reader_prefs.mode == ReaderMode::Integrated;
    col()
        .key("custom_reader_box")
        .w_full()
        .gap(8.0)
        .padding(12.0)
        .radius(8.0)
        .fill(theme::bg_card())
        .border(1.0, theme::border_subtle())
        .child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(subtle_tag("EXTERNAL READER", theme::text_muted()))
                .child(el().grow(1.0))
                .child(underline_tab(
                    "reader_mode_integrated",
                    "Built-in",
                    is_integrated,
                )
                .on_press(|s: &mut AppState| {
                    s.set_reader_mode(ReaderMode::Integrated);
                    s.reader_settings_open = false;
                }))
                .child(underline_tab(
                    "reader_mode_custom",
                    "Custom",
                    !is_integrated,
                )
                .on_press(|s: &mut AppState| {
                    s.set_reader_mode(ReaderMode::CustomExternal);
                    s.reader_settings_open = true;
                }))
                .child(ghost_btn(
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
                .center_y()
                .child(
                    el()
                        .grow(1.0)
                        .min_w(0.0)
                        .px(8.0)
                        .py(5.0)
                        .radius(6.0)
                        .fill(theme::bg_input())
                        .border(1.0, theme::border_subtle())
                        .child(
                            text_input(&s.custom_reader_edit, theme::STATUS_TEXT)
                                .key("custom_reader_path_input")
                                .label("Custom reader executable")
                                .single_line()
                                .w_full()
                                .color(theme::text_primary())
                                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
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
                        .radius(6.0)
                        .hover(theme::hover_subtle())
                        .center()
                        .cursor(Cursor::Pointer)
                        .on_press_tx(|_s: &mut AppState, tx: &mut Tx| {
                            let opts = FileDialogOptions {
                                title: "Select Custom Reader Application".into(),
                                ..Default::default()
                            };
                            tx.open_file(opts, |s: &mut AppState, res, _| {
                                if let Ok(Some(path)) = res {
                                    s.save_custom_reader_path(path.to_string_lossy().into_owned());
                                }
                            });
                        })
                        .child(
                            text("Browse")
                                .font(theme::STATUS_TEXT)
                                .color(theme::text_secondary()),
                        ),
                )
                .child(ghost_btn(
                    "btn_launch_custom_now",
                    "Launch ↗",
                    theme::text_primary(),
                    move |s| {
                        s.open_node_in_external_reader(&node_id_for_ext_btn);
                    },
                )),
        )
}

fn render_edit_doc_inspector(s: &AppState, node: &Node) -> El {
    let nid_ext = node.id.clone();
    let nid_ext_custom = node.id.clone();
    let nid_sum = node.id.clone();
    let nid_sum_menu = node.id.clone();
    let nid_sample = node.id.clone();
    let nid_del = node.id.clone();

    let node_kind = node.kind;
    let is_img_kind = node_kind == NodeKind::Image;
    let doc_format = s.node_doc_format(&node.id);
    let fmt_badge = doc_reader::format_badge(doc_format);
    let has_pdf = s.active_pdf.as_ref().is_some_and(|p| p.node_id == node.id);
    let is_integrated = s.reader_prefs.mode == ReaderMode::Integrated;

    // 4-Control Minimal Header: Kind tag | Edit/Done | Pop Out | overflow ⋯ | Close ×
    let header = row()
        .w_full()
        .center_y()
        .gap(6.0)
        .child(
            text(theme::kind_glyph(node_kind))
                .font(theme::CONTROL_TEXT)
                .color(theme::kind_accent(node_kind)),
        )
        .child(subtle_tag(fmt_badge, theme::text_muted()))
        .child(el().grow(1.0))
        .child(ghost_btn(
            "btn_toggle_readonly",
            "Done",
            theme::text_primary(),
            |s| {
                s.set_reader_read_only(true);
            },
        ))
        .child(popout_panel_btn())
        .child(ghost_icon_btn(
            "btn_inspector_more",
            "⋯",
            theme::text_secondary(),
            s.inspector_more_menu_open,
            |s| {
                s.inspector_more_menu_open = !s.inspector_more_menu_open;
            },
        ))
        .child(close_panel_btn());

    let mut content = col().w_full().gap(14.0).child(header);

    if s.inspector_more_menu_open {
        let mut menu = col()
            .key("inspector_more_menu")
            .w_full()
            .padding(6.0)
            .gap(2.0)
            .radius(8.0)
            .fill(theme::bg_card())
            .border(1.0, theme::border_subtle())
            .child(more_menu_item(
                "btn_open_external_reader",
                "Open in External App ↗",
                theme::text_primary(),
                move |s| {
                    s.inspector_more_menu_open = false;
                    s.open_node_in_external_reader(&nid_ext);
                },
            ))
            .child(more_menu_item(
                "btn_menu_custom_reader",
                "External Reader Settings...",
                theme::text_secondary(),
                |s| {
                    s.reader_settings_open = !s.reader_settings_open;
                    s.inspector_more_menu_open = false;
                },
            ))
            .child(more_menu_item(
                "btn_menu_ai_summarize",
                "✦ Summarize with AI",
                theme::accent_claude(),
                move |s| {
                    s.inspector_more_menu_open = false;
                    s.explain_node_ai(&nid_sum_menu);
                },
            ))
            .child(hairline())
            .child(more_menu_item(
                "btn_delete_doc",
                "Delete Node",
                theme::accent_danger(),
                move |s| {
                    s.inspector_more_menu_open = false;
                    s.delete_node(&nid_del);
                },
            ));

        if is_img_kind {
            menu = menu.child(more_menu_item(
                "btn_doc_excerpt",
                "❝ Create Excerpt",
                theme::text_secondary(),
                |s| {
                    s.inspector_more_menu_open = false;
                    s.create_excerpt_from_selected_doc(None);
                },
            ));
        }

        content = content.child(menu);
    }

    if !is_integrated || s.reader_settings_open {
        content = content.child(render_custom_reader_box(s, nid_ext_custom));
    }

    // Borderless in-place editorial title + concise ghost inline actions
    let mut actions_strip = row()
        .w_full()
        .center_y()
        .gap(4.0)
        .child(render_ingest_button(node.id.clone(), is_img_kind));

    if !is_img_kind {
        actions_strip = actions_strip.child(ghost_btn(
            "btn_doc_sample",
            "Sample",
            theme::text_muted(),
            move |s| {
                s.load_sample_document(&nid_sample);
            },
        ));
    }

    actions_strip = actions_strip
        .child(ghost_btn(
            "btn_doc_summarize",
            "✦ Summarize",
            theme::accent_claude(),
            move |s| {
                s.explain_node_ai(&nid_sum);
            },
        ))
        .child(ghost_btn(
            "btn_doc_excerpt",
            "❝ Excerpt",
            theme::text_secondary(),
            |s| {
                s.create_excerpt_from_selected_doc(None);
            },
        ));

    content = content
        .child(
            col()
                .w_full()
                .gap(8.0)
                .child(
                    text_input(&s.inspector_title_edit, SHEET_TITLE)
                        .key("inspector_title")
                        .label("Document title")
                        .single_line()
                        .w_full()
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder("Untitled document...", theme::text_muted())
                        .on_edit(|s: &mut AppState| &mut s.inspector_title_edit)
                        .on_input_change(|s: &mut AppState, _| {
                            s.sync_from_inspector_edits();
                        })
                        .on_submit(|s: &mut AppState, _| {
                            s.sync_from_inspector_edits();
                        }),
                )
                .child(actions_strip)
                .child(hairline()),
        );

    if let Some(summary) = &node.summary {
        content = content.child(
            col()
                .w_full()
                .gap(4.0)
                .pl(12.0)
                .py(2.0)
                .border_sides([0.0, 0.0, 0.0, 2.0], theme::accent_claude().alpha(0.65))
                .child(subtle_tag("✦ KEY INSIGHTS", theme::accent_claude()))
                .child(
                    text(summary)
                        .font(theme::BODY_TEXT)
                        .color(theme::text_secondary())
                        .w_full()
                        .wrap(),
                ),
        );
    }

    // Quiet underline view switcher (`Pages · Preview · Source`)
    let mut view_switcher = row()
        .key("doc_view_tabs")
        .w_full()
        .gap(14.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle());

    if has_pdf {
        view_switcher = view_switcher.child(
            underline_tab("tab_doc_pages", "Pages", s.doc_view_tab == DocViewTab::Pages)
                .on_press(|s: &mut AppState| {
                    s.doc_view_tab = DocViewTab::Pages;
                }),
        );
    }
    view_switcher = view_switcher
        .child(
            underline_tab(
                "tab_doc_preview",
                if is_img_kind { "Image" } else { "Preview" },
                s.doc_view_tab == DocViewTab::Preview
                    || (!has_pdf && s.doc_view_tab == DocViewTab::Pages),
            )
            .on_press(|s: &mut AppState| {
                s.doc_view_tab = DocViewTab::Preview;
            }),
        )
        .child(
            underline_tab("tab_doc_source", "Source", s.doc_view_tab == DocViewTab::Source)
                .on_press(|s: &mut AppState| {
                    s.doc_view_tab = DocViewTab::Source;
                }),
        );

    content = content.child(view_switcher);

    // Flush Reader / Source body (no inner box border)
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

        let mut pdf_col = col().key("integrated_pdf_viewer").w_full().gap(10.0).child(
            row()
                .w_full()
                .center_y()
                .gap(4.0)
                .child(ghost_btn("btn_pdf_prev", "‹ Prev", theme::text_secondary(), move |s| {
                    s.set_pdf_page(cur_page.saturating_sub(1));
                }))
                .child(
                    text(format!("{} / {}", cur_page + 1, total_pages))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                )
                .child(ghost_btn("btn_pdf_next", "Next ›", theme::text_secondary(), move |s| {
                    s.set_pdf_page(cur_page + 1);
                }))
                .child(el().grow(1.0))
                .child(ghost_btn("btn_pdf_zout", "−", theme::text_muted(), move |s| {
                    s.set_pdf_zoom(cur_zoom - 0.25);
                }))
                .child(
                    text(format!("{}%", (cur_zoom * 100.0).round() as u32))
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_muted()),
                )
                .child(ghost_btn("btn_pdf_zin", "+", theme::text_muted(), move |s| {
                    s.set_pdf_zoom(cur_zoom + 0.25);
                })),
        );

        if let Some(err) = &pdf.error {
            pdf_col = pdf_col.child(
                text(format!("PDF render note: {} — switch to Preview or External ↗.", err))
                    .font(theme::BODY_TEXT)
                    .color(theme::text_secondary())
                    .w_full()
                    .wrap(),
            );
        } else {
            let mut page_el = el()
                .key(format!("pdf_page_canvas_{}", cur_page))
                .w_full()
                .radius(4.0)
                .border(1.0, theme::border_subtle())
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
            .min_h(240.0)
            .max_h(460.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .child(
                text_input(&s.inspector_content_edit, theme::MONO_BODY)
                    .key("inspector_content")
                    .label("Document source")
                    .w_full()
                    .min_h(230.0)
                    .color(theme::text_primary())
                    .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                    .placeholder(
                        if is_img_kind {
                            "Optional image caption or notes..."
                        } else {
                            "Markdown or raw document text..."
                        },
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
        let mut preview_col = col()
            .key("integrated_md_reader")
            .w_full()
            .gap(12.0)
            .min_h(180.0)
            .max_h(470.0)
            .scroll(ScrollSpec { stick_bottom: false });

        let panel_width = s.inspector_width.clamp(300.0, 860.0);
        let md_w = (panel_width - 52.0).max(220.0);
        let preview_img = s
            .active_image
            .as_ref()
            .filter(|(img_nid, _, _)| img_nid == &node.id)
            .map(|(_, _, img)| img)
            .or_else(|| s.card_image_previews.get(&node.id));
        if doc_format == DocFormat::Image {
            if let Some(img) = preview_img {
                let img_h = preview_image_height(img, md_w, 160.0, 320.0, 220.0);
                preview_col = preview_col.child(render_image_preview_box(
                    "integrated_image_preview",
                    img,
                    img_h,
                ));
            }
        }

        if node.kind == NodeKind::Image {
            if !node.content.trim().is_empty() {
                let md_src = doc_reader::wrap_for_preview(&node.content, doc_format);
                let md_style = theme::markdown_style().text_width(md_w);
                let md_el = Markdown::new(&md_src)
                    .selectable(true)
                    .on_task(|s: &mut AppState, _block_key, offset, checked| {
                        s.toggle_selected_doc_task(offset, checked);
                    })
                    .render(&md_style);
                preview_col =
                    preview_col.child(el().key("image_caption_markdown").w_full().child(md_el));
            }
            preview_col
        } else {
            let is_empty = node.content.trim().is_empty();
            let md_src = if is_empty {
                "*Empty document. Click **Ingest File** above or switch to **Source** to write Markdown.*".to_string()
            } else {
                doc_reader::wrap_for_preview(&node.content, doc_format)
            };

            let md_style = theme::markdown_style().text_width(md_w);
            let md_el = Markdown::new(&md_src)
                .selectable(true)
                .on_task(|s: &mut AppState, _block_key, offset, checked| {
                    s.toggle_selected_doc_task(offset, checked);
                })
                .render(&md_style);

            if is_empty {
                preview_col.child(el().key("empty_doc_placeholder").w_full().child(md_el))
            } else {
                preview_col.child(md_el)
            }
        }
    };

    content = content.child(reader_body);

    if is_img_kind {
        content = content.child(hairline()).child(render_image_vision_section(s, node));
    } else {
        content = content.child(hairline()).child(render_translation_section(s, node));
    }

    // Minimal inline append strip
    content = content.child(hairline()).child(
        row()
            .w_full()
            .center_y()
            .gap(8.0)
            .child(
                el().grow(1.0).min_w(0.0).py(4.0).child(
                    text_input(&s.append_note_input, theme::STATUS_TEXT)
                        .key("append_note")
                        .label("Append markdown note or excerpt")
                        .single_line()
                        .w_full()
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder("Quick append note...", theme::text_muted())
                        .on_edit(|s: &mut AppState| &mut s.append_note_input)
                        .on_submit(|s: &mut AppState, _| {
                            s.commit_append_input();
                        }),
                ),
            )
            .child(ghost_btn(
                "btn_append_doc",
                "+ Add",
                theme::text_secondary(),
                |s| s.commit_append_input(),
            )),
    );

    if !s.inspector_more_menu_open {
        let nid_save_footer_del = node.id.clone();
        content = content.child(
            row()
                .w_full()
                .center_y()
                .child(ghost_btn(
                    "btn_save_doc",
                    "Save",
                    theme::text_muted(),
                    |s| {
                        s.sync_node_edits();
                        if let Some(bundle) = &s.bundle {
                            let _ = bundle.save_graph_atomic(&s.graph);
                        }
                        s.notify_toast("Saved");
                    },
                ))
                .child(el().grow(1.0))
                .child(ghost_btn(
                    "btn_delete_doc",
                    "Delete",
                    theme::accent_danger().alpha(0.85),
                    move |s| s.delete_node(&nid_save_footer_del),
                )),
        );
    }

    content
}

fn render_generic_node_inspector(s: &AppState, node: &Node) -> El {
    let nid_explain = node.id.clone();
    let nid_del = node.id.clone();
    let nid_del_menu = node.id.clone();
    let node_kind = node.kind;

    let mut hdr = row()
        .w_full()
        .center_y()
        .gap(6.0)
        .child(
            text(theme::kind_glyph(node_kind))
                .font(theme::CONTROL_TEXT)
                .color(theme::kind_accent(node_kind)),
        )
        .child(subtle_tag(theme::kind_label(node_kind), theme::text_muted()))
        .child(el().grow(1.0));
    if crate::reader_window::can_pop_out(node_kind) {
        hdr = hdr.child(popout_panel_btn());
    }
    hdr = hdr
        .child(ghost_icon_btn(
            "btn_inspector_more",
            "⋯",
            theme::text_secondary(),
            s.inspector_more_menu_open,
            |s| {
                s.inspector_more_menu_open = !s.inspector_more_menu_open;
            },
        ))
        .child(close_panel_btn());

    let mut content = col().w_full().gap(14.0).child(hdr);

    if s.inspector_more_menu_open {
        let nid_exp_m = nid_explain.clone();
        content = content.child(
            col()
                .key("inspector_more_menu")
                .w_full()
                .padding(6.0)
                .gap(2.0)
                .radius(8.0)
                .fill(theme::bg_card())
                .border(1.0, theme::border_subtle())
                .child(more_menu_item(
                    "btn_menu_ai_explain",
                    "✦ Explain with AI",
                    theme::accent_claude(),
                    move |s| {
                        s.inspector_more_menu_open = false;
                        s.explain_node_ai(&nid_exp_m);
                    },
                ))
                .child(hairline())
                .child(more_menu_item(
                    "btn_menu_del_node",
                    "Delete Node",
                    theme::accent_danger(),
                    move |s| {
                        s.inspector_more_menu_open = false;
                        s.delete_node(&nid_del_menu);
                    },
                )),
        );
    }

    content = content
        .child(
            col()
                .w_full()
                .gap(6.0)
                .child(
                    text_input(&s.inspector_title_edit, SHEET_TITLE)
                        .key("inspector_title")
                        .label("Node title")
                        .single_line()
                        .w_full()
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder("Untitled...", theme::text_muted())
                        .on_edit(|s: &mut AppState| &mut s.inspector_title_edit)
                        .on_input_change(|s: &mut AppState, _| {
                            s.sync_from_inspector_edits();
                        })
                        .on_submit(|s: &mut AppState, _| {
                            s.sync_from_inspector_edits();
                        }),
                )
                .child(hairline()),
        )
        .child(
            col()
                .w_full()
                .min_h(160.0)
                .max_h(320.0)
                .scroll(ScrollSpec { stick_bottom: false })
                .child(
                    text_input(&s.inspector_content_edit, theme::BODY_TEXT)
                        .key("inspector_content")
                        .label("Note body")
                        .w_full()
                        .min_h(140.0)
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder(
                            "Write notes, markdown, or observations...",
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
        )
        .child(
            row()
                .w_full()
                .gap(4.0)
                .child(ghost_btn("fmt_bullet", "• List", theme::text_muted(), |s| {
                    s.append_to_selected_node("- ")
                }))
                .child(ghost_btn("fmt_heading", "Heading", theme::text_muted(), |s| {
                    s.append_to_selected_node("### ")
                }))
                .child(ghost_btn("fmt_todo", "Todo", theme::text_muted(), |s| {
                    s.append_to_selected_node("- [ ] ")
                }))
                .child(ghost_btn("fmt_quote", "Quote", theme::text_muted(), |s| {
                    s.append_to_selected_node("> ")
                })),
        )
        .child(render_specialized_node_inspector(s, node));

    if matches!(
        node_kind,
        NodeKind::Note | NodeKind::Excerpt | NodeKind::Concept | NodeKind::LlmSynthesis
    ) {
        content = content
            .child(hairline())
            .child(render_translation_section(s, node));
    }

    if let Some(summary) = &node.summary {
        content = content.child(
            col()
                .w_full()
                .gap(4.0)
                .pl(12.0)
                .py(2.0)
                .border_sides([0.0, 0.0, 0.0, 2.0], theme::accent_claude().alpha(0.65))
                .child(subtle_tag("✦ AI INSIGHTS", theme::accent_claude()))
                .child(
                    text(summary)
                        .font(theme::BODY_TEXT)
                        .color(theme::text_secondary())
                        .w_full()
                        .wrap(),
                ),
        );
    }

    content
        .child(hairline())
        .child(
            row()
                .w_full()
                .center_y()
                .gap(4.0)
                .child(ghost_btn(
                    "btn_node_ai_explain",
                    "✦ Explain",
                    theme::accent_claude(),
                    move |s| {
                        s.explain_node_ai(&nid_explain);
                    },
                ))
                .child(ghost_btn("btn_save_node", "Save", theme::text_muted(), |s| {
                    s.sync_node_edits();
                    if let Some(bundle) = &s.bundle {
                        let _ = bundle.save_graph_atomic(&s.graph);
                    }
                    s.notify_toast("Saved");
                }))
                .child(el().grow(1.0))
                .child(ghost_btn(
                    "btn_delete_node",
                    "Delete",
                    theme::accent_danger().alpha(0.85),
                    move |s| {
                        s.delete_node(&nid_del);
                    },
                )),
        )
}

fn render_edge_inspector(s: &AppState, edge: &studynodes_graph::model::Edge) -> El {
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

    let eid_show = edge_id_for_rel.clone();
    let eid_hide = edge_id_for_rel.clone();
    let eid_sup = edge_id_for_rel.clone();
    let eid_con = edge_id_for_rel.clone();
    let eid_exp = edge_id_for_rel.clone();

    col()
        .w_full()
        .gap(14.0)
        .child(
            row()
                .w_full()
                .center_y()
                .gap(6.0)
                .child(subtle_tag("CONNECTION", theme::text_muted()))
                .child(el().grow(1.0))
                .child(close_panel_btn()),
        )
        .child(
            col()
                .w_full()
                .gap(4.0)
                .pl(12.0)
                .border_sides([0.0, 0.0, 0.0, 2.0], theme::border_strong())
                .child(
                    text(format!("{}  →  {}", from_title, to_title))
                        .font(theme::PANEL_TITLE)
                        .color(theme::text_primary())
                        .w_full()
                        .wrap(),
                ),
        )
        .child(
            row()
                .w_full()
                .gap(8.0)
                .child(underline_tab("rel_supports", "Supports", edge.relation == EdgeRelation::Supports).on_press(
                    move |s: &mut AppState| {
                        if let Some(e) = s.graph.get_edge_mut(&eid_sup) {
                            e.relation = EdgeRelation::Supports;
                        }
                    },
                ))
                .child(underline_tab("rel_contradicts", "Contradicts", edge.relation == EdgeRelation::Contradicts).on_press(
                    move |s: &mut AppState| {
                        if let Some(e) = s.graph.get_edge_mut(&eid_con) {
                            e.relation = EdgeRelation::Contradicts;
                        }
                    },
                ))
                .child(underline_tab("rel_explains", "Explains", edge.relation == EdgeRelation::Explains).on_press(
                    move |s: &mut AppState| {
                        if let Some(e) = s.graph.get_edge_mut(&eid_exp) {
                            e.relation = EdgeRelation::Explains;
                        }
                    },
                )),
        )
        .child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(
                    underline_tab("btn_edge_show_label", "Show Label", !edge.hide_label).on_press(
                        move |s: &mut AppState| {
                            s.toggle_edge_hide_label(&eid_show, false);
                        },
                    ),
                )
                .child(
                    underline_tab("btn_edge_hide_label", "Hide Label", edge.hide_label).on_press(
                        move |s: &mut AppState| {
                            s.toggle_edge_hide_label(&eid_hide, true);
                        },
                    ),
                )
                .child(el().grow(1.0))
                .child(ghost_btn(
                    "btn_toggle_all_edge_labels",
                    if s.hide_all_edge_labels {
                        "Show All"
                    } else {
                        "Hide All"
                    },
                    theme::text_muted(),
                    |s| s.toggle_all_edge_labels(),
                )),
        )
        .child(hairline())
        .child(
            col()
                .w_full()
                .gap(6.0)
                .child(subtle_tag("LABEL", theme::text_muted()))
                .child(
                    text_input(&s.edit_edge_label, theme::CONTROL_TEXT)
                        .key("edge_label")
                        .label("Connection label")
                        .single_line()
                        .w_full()
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder("Short label...", theme::text_muted())
                        .on_edit(|s: &mut AppState| &mut s.edit_edge_label)
                        .on_input_change(|s: &mut AppState, _| {
                            s.sync_edge_edits();
                        }),
                ),
        )
        .child(hairline())
        .child(
            col()
                .w_full()
                .gap(6.0)
                .child(subtle_tag("NOTE", theme::text_muted()))
                .child(
                    text_input(&s.edit_edge_note, theme::BODY_TEXT)
                        .key("edge_note")
                        .label("Connection note")
                        .w_full()
                        .min_h(80.0)
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
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
        )
        .child(hairline())
        .child(
            ghost_btn(
                "btn_delete_edge",
                "Delete Connection",
                theme::accent_danger().alpha(0.85),
                move |s| {
                    s.graph.remove_edge(&edge_id_for_delete);
                    s.selected_edge = None;
                    s.notify_toast("Deleted connection");
                },
            ),
        )
}

fn render_readonly_doc_panel(s: &AppState, node: &Node, panel_width: f32) -> El {
    let doc_format = s.node_doc_format(&node.id);
    let fmt_badge = doc_reader::format_badge(doc_format);
    let has_pdf = s.active_pdf.as_ref().is_some_and(|p| p.node_id == node.id);
    let nid_ext = node.id.clone();
    let nid_sum = node.id.clone();

    // Minimal 4-control editorial header (key("reader_readonly_toolbar") preserved)
    let small_toolbar = row()
        .key("reader_readonly_toolbar")
        .w_full()
        .center_y()
        .gap(6.0)
        .child(
            text(theme::kind_glyph(node.kind))
                .font(theme::CONTROL_TEXT)
                .color(theme::kind_accent(node.kind)),
        )
        .child(subtle_tag(fmt_badge, theme::text_muted()))
        .child(el().grow(1.0))
        .child(ghost_btn(
            "btn_toggle_readonly",
            "Edit",
            theme::accent_claude(),
            |s| {
                s.set_reader_read_only(false);
            },
        ))
        .child(popout_panel_btn())
        .child(ghost_icon_btn(
            "btn_inspector_more",
            "⋯",
            theme::text_secondary(),
            s.inspector_more_menu_open,
            |s| {
                s.inspector_more_menu_open = !s.inspector_more_menu_open;
            },
        ))
        .child(close_panel_btn());

    let effective_tab = if s.doc_view_tab == DocViewTab::Source {
        if has_pdf {
            DocViewTab::Pages
        } else {
            DocViewTab::Preview
        }
    } else {
        s.doc_view_tab
    };

    let reader_body = if has_pdf && effective_tab == DocViewTab::Pages {
        let pdf = s.active_pdf.as_ref().unwrap();
        let cur_page = pdf.page_index;
        let total_pages = pdf.page_count.max(1);
        let cur_zoom = pdf.zoom;
        let aspect = if pdf.page_size.0 > 0 && pdf.page_size.1 > 0 {
            Some(pdf.page_size.0 as f32 / pdf.page_size.1 as f32)
        } else {
            None
        };

        let mut pdf_col = col().w_full().grow(1.0).min_h(0.0).gap(10.0).child(
            row()
                .key("readonly_pdf_controls")
                .w_full()
                .center_y()
                .gap(4.0)
                .child(ghost_btn("btn_pdf_prev", "‹", theme::text_secondary(), move |s| {
                    s.set_pdf_page(cur_page.saturating_sub(1));
                }))
                .child(
                    text(format!("{} / {}", cur_page + 1, total_pages))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                )
                .child(ghost_btn("btn_pdf_next", "›", theme::text_secondary(), move |s| {
                    s.set_pdf_page(cur_page + 1);
                }))
                .child(el().grow(1.0))
                .child(ghost_btn("btn_pdf_zout", "−", theme::text_muted(), move |s| {
                    s.set_pdf_zoom(cur_zoom - 0.25);
                }))
                .child(
                    text(format!("{}%", (cur_zoom * 100.0).round() as u32))
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_muted()),
                )
                .child(ghost_btn("btn_pdf_zin", "+", theme::text_muted(), move |s| {
                    s.set_pdf_zoom(cur_zoom + 0.25);
                })),
        );

        if let Some(err) = &pdf.error {
            pdf_col = pdf_col.child(
                text(format!("PDF render note: {}", err))
                    .font(theme::BODY_TEXT)
                    .color(theme::text_secondary())
                    .w_full()
                    .wrap(),
            );
        } else {
            let mut page_el = el()
                .key(format!("pdf_page_canvas_{}", cur_page))
                .w_full()
                .radius(4.0)
                .border(1.0, theme::border_subtle())
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
                    .grow(1.0)
                    .min_h(0.0)
                    .scroll(ScrollSpec { stick_bottom: false })
                    .child(page_el),
            );
        }
        pdf_col
    } else {
        let mut preview_col = col()
            .key("integrated_md_reader")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .gap(12.0)
            .scroll(ScrollSpec { stick_bottom: false });

        let md_w = (panel_width - 52.0).max(220.0);
        let preview_img = s
            .active_image
            .as_ref()
            .filter(|(img_nid, _, _)| img_nid == &node.id)
            .map(|(_, _, img)| img)
            .or_else(|| s.card_image_previews.get(&node.id));
        if doc_format == DocFormat::Image {
            if let Some(img) = preview_img {
                let img_h = preview_image_height(img, md_w, 150.0, 260.0, 190.0);
                preview_col = preview_col.child(render_image_preview_box(
                    "integrated_image_preview",
                    img,
                    img_h,
                ));
            }
        }

        if node.kind == NodeKind::Image {
            preview_col = preview_col
                .child(hairline())
                .child(render_image_vision_section(s, node));
            if !node.content.trim().is_empty() {
                let md_src = doc_reader::wrap_for_preview(&node.content, doc_format);
                let md_style = theme::markdown_style().text_width(md_w);
                let md_el = Markdown::new(&md_src).selectable(true).render(&md_style);
                preview_col =
                    preview_col.child(el().key("image_caption_markdown").w_full().child(md_el));
            }
            preview_col
        } else {
            let is_empty = node.content.trim().is_empty();
            let md_src = if is_empty {
                "*Read-only mode. Click **Edit** above to ingest a file or write Markdown.*".to_string()
            } else {
                doc_reader::wrap_for_preview(&node.content, doc_format)
            };

            let md_style = theme::markdown_style().text_width(md_w);
            let md_el = Markdown::new(&md_src).selectable(true).render(&md_style);

            if is_empty {
                preview_col.child(el().key("empty_doc_placeholder").w_full().child(md_el))
            } else {
                preview_col.child(md_el)
            }
        }
    };

    let mut sheet = col()
        .key("inspector_panel")
        .w(panel_width)
        .h_full()
        .fill(theme::bg_chrome())
        .px(22.0)
        .py(16.0)
        .gap(12.0)
        .child(small_toolbar);

    if s.inspector_more_menu_open {
        sheet = sheet.child(
            col()
                .key("inspector_more_menu")
                .w_full()
                .padding(6.0)
                .gap(2.0)
                .radius(8.0)
                .fill(theme::bg_card())
                .border(1.0, theme::border_subtle())
                .child(more_menu_item(
                    "btn_readonly_excerpt",
                    "❝ Create Excerpt",
                    theme::text_primary(),
                    |s| {
                        s.inspector_more_menu_open = false;
                        s.create_excerpt_from_selected_doc(None);
                    },
                ))
                .child(more_menu_item(
                    "btn_readonly_summarize",
                    "✦ Summarize with AI",
                    theme::accent_claude(),
                    move |s| {
                        s.inspector_more_menu_open = false;
                        s.explain_node_ai(&nid_sum);
                    },
                ))
                .child(more_menu_item(
                    "btn_open_external_reader",
                    "Open in External App ↗",
                    theme::text_secondary(),
                    move |s| {
                        s.inspector_more_menu_open = false;
                        s.open_node_in_external_reader(&nid_ext);
                    },
                )),
        );
    }

    sheet = sheet.child(
        text(&node.title)
            .font(SHEET_TITLE)
            .color(theme::text_primary())
            .w_full()
            .wrap(),
    );

    if has_pdf {
        sheet = sheet.child(
            row()
                .key("doc_view_tabs")
                .w_full()
                .gap(14.0)
                .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
                .child(
                    underline_tab(
                        "tab_doc_pages",
                        "Pages",
                        s.doc_view_tab == DocViewTab::Pages || s.doc_view_tab == DocViewTab::Source,
                    )
                    .on_press(|s: &mut AppState| {
                        s.doc_view_tab = DocViewTab::Pages;
                    }),
                )
                .child(
                    underline_tab("tab_doc_preview", "Text", s.doc_view_tab == DocViewTab::Preview)
                        .on_press(|s: &mut AppState| {
                            s.doc_view_tab = DocViewTab::Preview;
                        }),
                ),
        );
    } else {
        sheet = sheet.child(hairline());
    }

    sheet.child(reader_body)
}

fn render_specialized_node_inspector(s: &AppState, node: &Node) -> El {
    let nid = node.id.clone();
    match node.kind {
        NodeKind::Flashcard => {
            let cur = node.meta.mastery.as_deref().unwrap_or("new");
            let mut mastery_row = row().w_full().gap(10.0);
            for (key, label, val) in [
                ("mast_new", "New", "new"),
                ("mast_again", "Again", "again"),
                ("mast_good", "Good", "good"),
                ("mast_easy", "Easy", "easy"),
            ] {
                let id_c = nid.clone();
                mastery_row = mastery_row.child(underline_tab(key, label, cur == val).on_press(
                    move |s: &mut AppState| {
                        s.set_flashcard_mastery(&id_c, val);
                    },
                ));
            }

            col()
                .w_full()
                .gap(10.0)
                .child(hairline())
                .child(subtle_tag("ANSWER / BACK", theme::text_muted()))
                .child(
                    text_input(&s.inspector_answer_edit, theme::BODY_TEXT)
                        .key("inspector_answer")
                        .label("Flashcard answer")
                        .w_full()
                        .min_h(72.0)
                        .wrap()
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder(
                            "Recall explanation shown when card is flipped...",
                            theme::text_muted(),
                        )
                        .on_edit(|s: &mut AppState| &mut s.inspector_answer_edit)
                        .on_input_change(|s: &mut AppState, _| {
                            s.sync_from_inspector_edits();
                        }),
                )
                .child(subtle_tag("MASTERY", theme::text_muted()))
                .child(mastery_row)
        }
        NodeKind::Formula => {
            let cur = node.meta.language.as_deref().unwrap_or("MATH");
            let mut langs = row().w_full().gap(12.0);
            for (key, lang) in [
                ("lang_math", "MATH"),
                ("lang_rust", "RUST"),
                ("lang_py", "PY"),
                ("lang_chem", "CHEM"),
            ] {
                let id_c = nid.clone();
                langs = langs.child(
                    underline_tab(key, lang, cur.eq_ignore_ascii_case(lang)).on_press(
                        move |s: &mut AppState| {
                            s.set_formula_language(&id_c, lang);
                        },
                    ),
                );
            }

            let fmt_nid = nid.clone();
            let formula_col = col()
                .w_full()
                .gap(10.0)
                .child(hairline())
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .child(langs)
                        .child(el().grow(1.0))
                        .child(ghost_btn(
                            "btn_inspector_formula_format",
                            "∑ Format",
                            theme::accent_claude(),
                            move |s| {
                                s.format_formula_node(&fmt_nid);
                            },
                        )),
                );

            let mut sym_panel = col()
                .w_full()
                .gap(5.0)
                .child(subtle_tag("SYMBOLS", theme::text_muted()));
            for (group_name, syms) in [
                ("Greek", formula_math::GREEK_SYMBOLS),
                ("Operators", formula_math::OP_SYMBOLS),
                ("Scripts", formula_math::SCRIPT_SYMBOLS),
            ] {
                let mut g_col = col().w_full().gap(3.0).child(
                    text(group_name)
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_muted()),
                );
                for (r_idx, chunk) in syms.chunks(8).enumerate() {
                    let mut r = row().w_full().gap(4.0);
                    for (c_idx, sym) in chunk.iter().enumerate() {
                        let id_s = nid.clone();
                        let sym_str = *sym;
                        r = r.child(
                            el()
                                .key(format!("insp_sym_{}_{}_{}", group_name, r_idx, c_idx))
                                .min_w(26.0)
                                .h(24.0)
                                .px(5.0)
                                .radius(5.0)
                                .hover(theme::hover_subtle())
                                .center()
                                .cursor(Cursor::Pointer)
                                .on_press(move |s: &mut AppState| {
                                    s.insert_formula_symbol(&id_s, sym_str);
                                })
                                .child(
                                    text(sym_str)
                                        .font(TextStyle::new(12.0, 500.0).mono())
                                        .color(theme::text_primary()),
                                ),
                        );
                    }
                    g_col = g_col.child(r);
                }
                sym_panel = sym_panel.child(g_col);
            }

            let mut tpl_panel = col()
                .w_full()
                .gap(4.0)
                .child(subtle_tag("TEMPLATES", theme::text_muted()));
            for chunk in formula_math::FORMULA_TEMPLATES.chunks(3) {
                let mut r = row().w_full().gap(4.0);
                for &(name, tpl) in chunk {
                    let id_t = nid.clone();
                    r = r.child(
                        el()
                            .key(format!("insp_tpl_{}", name))
                            .grow(1.0)
                            .py(5.0)
                            .px(6.0)
                            .radius(6.0)
                            .hover(theme::hover_subtle())
                            .center()
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                let selected = s.selected_nodes.contains(&id_t);
                                if let Some(n) = s.graph.get_node_mut(&id_t) {
                                    n.content = tpl.to_string();
                                    if selected {
                                        s.canvas_content_edit = rune::TextEdit::new(&n.content);
                                        s.inspector_content_edit = rune::TextEdit::new(&n.content);
                                    }
                                }
                                if let Some(bundle) = &s.bundle {
                                    let _ = bundle.save_graph_atomic(&s.graph);
                                }
                                s.push_reader_snapshot();
                                s.notify_toast(format!("Applied {} template", name));
                            })
                            .child(
                                text(name)
                                    .font(theme::BADGE_TEXT)
                                    .color(theme::text_secondary()),
                            ),
                    );
                }
                tpl_panel = tpl_panel.child(r);
            }

            formula_col.child(sym_panel).child(tpl_panel)
        }
        NodeKind::Section => {
            let framed = node.meta.section_framed;
            let id_a = nid.clone();
            let id_b = nid.clone();
            let sz = crate::state::node_world_size(node);
            col()
                .w_full()
                .gap(8.0)
                .child(hairline())
                .child(
                    row()
                        .w_full()
                        .gap(14.0)
                        .child(
                            underline_tab("insp_sec_framed", "▭ Framed Group", framed).on_press(
                                move |s: &mut AppState| {
                                    if let Some(sec) = s.graph.get_node(&id_a) {
                                        if !sec.meta.section_framed {
                                            s.toggle_section_framed(&id_a);
                                        }
                                    }
                                },
                            ),
                        )
                        .child(
                            underline_tab("insp_sec_banner", "— Banner Only", !framed).on_press(
                                move |s: &mut AppState| {
                                    if let Some(sec) = s.graph.get_node(&id_b) {
                                        if sec.meta.section_framed {
                                            s.toggle_section_framed(&id_b);
                                        }
                                    }
                                },
                            ),
                        ),
                )
                .child(
                    text(format!("Canvas bounds: {} × {} px", sz.0 as i32, sz.1 as i32))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                )
        }
        NodeKind::Excerpt => {
            let jump_label = match (&node.meta.source_node, node.meta.source_page) {
                (Some(src_id), page_opt) => {
                    let doc_title = s
                        .graph
                        .get_node(src_id)
                        .map(|d| d.title.as_str())
                        .unwrap_or("Source Document");
                    format!("▤ Open '{}' · p.{} ↗", doc_title, page_opt.unwrap_or(0) + 1)
                }
                _ => "No source document linked".into(),
            };
            ghost_btn("btn_inspector_excerpt_jump", &jump_label, theme::accent_claude(), move |s| {
                s.jump_to_excerpt_source(&nid);
            })
        }
        NodeKind::Sequence => ghost_btn(
            "btn_inspector_seq_next",
            "+ Spawn Next Step →",
            theme::accent_claude(),
            move |s| {
                s.add_next_sequence_step(&nid);
            },
        ),
        NodeKind::Checklist => {
            let tasks = doc_reader::parse_markdown_tasks(&node.content);
            let done = tasks.iter().filter(|(c, _)| *c).count();
            text(format!("Progress: {}/{} tasks completed", done, tasks.len()))
                .font(theme::STATUS_TEXT)
                .color(theme::text_muted())
        }
        _ => el(),
    }
}

fn preview_image_height(
    img: &rune::Image,
    avail_w: f32,
    min_h: f32,
    max_h: f32,
    fallback_h: f32,
) -> f32 {
    if let Some([iw, ih]) = img.size() {
        if iw > 0 && ih > 0 {
            return (avail_w * (ih as f32 / iw as f32)).clamp(min_h, max_h);
        }
    }
    fallback_h
}

fn render_image_preview_box(key: &'static str, img: &rune::Image, height: f32) -> El {
    if let Some(err) = img.error() {
        col()
            .key(key)
            .w_full()
            .h(120.0)
            .padding(10.0)
            .radius(6.0)
            .fill(theme::bg_input())
            .border(1.0, theme::accent_danger().alpha(0.45))
            .center()
            .child(
                text(format!("Image error: {}", err))
                    .font(theme::STATUS_TEXT)
                    .color(theme::accent_danger())
                    .wrap(),
            )
    } else if img.is_ready() {
        el()
            .key(key)
            .w_full()
            .h(height)
            .radius(6.0)
            .fill(theme::bg_input())
            .pixels_fit(img.pixels(), SurfaceFit::Fit)
    } else {
        el()
            .key(key)
            .w_full()
            .h(height)
            .radius(6.0)
            .fill(theme::bg_input())
            .center()
            .child(
                text("Loading image...")
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_muted()),
            )
    }
}

fn vision_input_field(
    key: &'static str,
    state: &rune::TextEdit,
    placeholder_text: &'static str,
    on_edit_fn: impl Fn(&mut AppState) -> &mut rune::TextEdit + 'static,
) -> El {
    el()
        .w_full()
        .px(8.0)
        .py(5.0)
        .radius(6.0)
        .fill(theme::bg_input())
        .border(1.0, theme::border_subtle())
        .child(
            text_input(state, theme::STATUS_TEXT)
                .key(key)
                .single_line()
                .w_full()
                .color(theme::text_primary())
                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                .placeholder(placeholder_text, theme::text_muted())
                .on_edit(on_edit_fn)
                .on_input_change(|s: &mut AppState, _| {
                    s.save_vision_api_settings();
                })
                .on_submit(|s: &mut AppState, _| {
                    s.save_vision_api_settings();
                }),
        )
}

fn render_image_vision_section(s: &AppState, node: &Node) -> El {
    let nid_text = node.id.clone();
    let nid_formula = node.id.clone();
    let nid_spawn_note = node.id.clone();
    let nid_spawn_formula = node.id.clone();
    let nid_clear_ocr = node.id.clone();

    let engine = s.reader_prefs.ocr_engine;
    let is_busy = s.vision_pending_nodes.contains(&node.id);
    let local_selected = engine != OcrEnginePreference::CloudVision;

    let mut action_row = row()
        .key("image_vision_section")
        .w_full()
        .center_y()
        .gap(4.0)
        .child(
            el()
                .key("btn_img_ocr_text")
                .h(26.0)
                .px(10.0)
                .radius(6.0)
                .fill(theme::accent_primary())
                .hover(theme::accent_primary().alpha(0.88))
                .center()
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.run_image_ocr(&nid_text, OcrMode::PlainText);
                })
                .child(
                    text(if is_busy { "Extracting..." } else { "Extract Text" })
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_inverted()),
                ),
        )
        .child(ghost_btn(
            "btn_img_ocr_formula",
            "Formula",
            theme::text_secondary(),
            move |s| {
                s.run_image_ocr(&nid_formula, OcrMode::EquationFormula);
            },
        ))
        .child(el().grow(1.0))
        .child(ghost_btn(
            "btn_cycle_ocr_engine",
            engine.short_label(),
            theme::text_muted(),
            |s| {
                s.cycle_ocr_engine();
            },
        ));

    if local_selected && !s.neural_models_ready {
        action_row = action_row.child(ghost_btn(
            "btn_download_neural_models",
            if s.vision_downloading_models {
                "..."
            } else {
                "↓ Models"
            },
            theme::accent_claude(),
            |s| {
                s.download_neural_ocr_models();
            },
        ));
    }

    action_row = action_row.child(ghost_icon_btn(
        "btn_toggle_vision_settings",
        "⚙",
        if s.vision_settings_open || s.cloud_vision_configured() {
            theme::text_primary()
        } else {
            theme::text_muted()
        },
        s.vision_settings_open,
        |s| {
            s.vision_settings_open = !s.vision_settings_open;
        },
    ));

    let mut sec = col().w_full().gap(10.0).child(action_row);

    if s.vision_settings_open {
        let cloud_status = if s.cloud_vision_configured() {
            "Ready"
        } else {
            "Not set"
        };
        let local_status = if s.neural_models_ready {
            "Ready"
        } else {
            "~21 MB"
        };
        sec = sec.child(
            col()
                .key("vision_api_settings_panel")
                .w_full()
                .gap(6.0)
                .padding(10.0)
                .radius(8.0)
                .fill(theme::bg_card())
                .border(1.0, theme::border_subtle())
                .child(vision_input_field(
                    "input_vision_api_key",
                    &s.vision_api_key_edit,
                    "API key (Gemini / DeepInfra / SiliconFlow / OpenAI)",
                    |s| &mut s.vision_api_key_edit,
                ))
                .child(vision_input_field(
                    "input_vision_base_url",
                    &s.vision_base_url_edit,
                    "Base URL (blank = Gemini, or https://.../v1)",
                    |s| &mut s.vision_base_url_edit,
                ))
                .child(vision_input_field(
                    "input_vision_model",
                    &s.vision_model_edit,
                    "Model (gemini-2.5-flash or deepseek-ai/DeepSeek-OCR)",
                    |s| &mut s.vision_model_edit,
                ))
                .child(
                    text(format!(
                        "Cloud AI: {} · PP-OCRv5: {}",
                        cloud_status, local_status
                    ))
                    .font(theme::BADGE_TEXT)
                    .color(theme::text_muted()),
                ),
        );
    }

    if let Some(err) = &s.vision_last_error {
        sec = sec.child(
            text(err)
                .key("ocr_error_banner")
                .font(theme::STATUS_TEXT)
                .color(theme::accent_danger())
                .w_full()
                .wrap(),
        );
    }

    if let Some(extracted) = node
        .meta
        .extracted_text
        .as_deref()
        .filter(|t| !t.trim().is_empty())
    {
        let engine_badge = node.meta.ocr_engine.as_deref().unwrap_or("OCR");
        let extracted_owned = extracted.to_string();
        let ocr_copied = s.is_recently_copied("ocr");
        sec = sec.child(
            col()
                .key("ocr_extracted_preview")
                .w_full()
                .gap(8.0)
                .pl(12.0)
                .py(2.0)
                .border_sides([0.0, 0.0, 0.0, 2.0], theme::border_strong())
                .child(
                    text(extracted)
                        .font(theme::BODY_TEXT)
                        .color(theme::text_primary())
                        .w_full()
                        .wrap(),
                )
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .gap(4.0)
                        .child(
                            text(engine_badge)
                                .font(theme::BADGE_TEXT)
                                .color(theme::text_muted())
                                .truncate(),
                        )
                        .child(el().grow(1.0))
                        .child(
                            el()
                                .key("btn_ocr_copy")
                                .px(6.0)
                                .py(3.0)
                                .radius(5.0)
                                .hover(theme::hover_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                                    tx.copy(extracted_owned.clone());
                                    s.mark_copied("ocr", "Copied extracted text");
                                })
                                .child(
                                    text(if ocr_copied { "Copied ✓" } else { "Copy" })
                                        .font(theme::BADGE_TEXT)
                                        .color(if ocr_copied {
                                            theme::accent_claude()
                                        } else {
                                            theme::text_secondary()
                                        }),
                                ),
                        )
                        .child(ghost_btn(
                            "btn_ocr_spawn_note",
                            "+ Note",
                            theme::text_secondary(),
                            move |s| {
                                s.spawn_note_from_extracted_text(&nid_spawn_note);
                            },
                        ))
                        .child(ghost_btn(
                            "btn_ocr_spawn_formula",
                            "+ Formula",
                            theme::text_secondary(),
                            move |s| {
                                s.spawn_formula_from_extracted_text(&nid_spawn_formula);
                            },
                        ))
                        .child(ghost_btn("btn_ocr_clear", "×", theme::text_muted(), move |s| {
                            s.clear_extracted_text(&nid_clear_ocr);
                        })),
                ),
        );
    }

    sec.child(hairline())
        .child(render_translation_section(s, node))
}

fn render_translation_section(s: &AppState, node: &Node) -> El {
    let nid_trans = node.id.clone();
    let nid_spawn_tr = node.id.clone();
    let nid_clear_tr = node.id.clone();
    let active_lang = s.reader_prefs.target_language.to_ascii_lowercase();
    let is_busy = s.vision_pending_nodes.contains(&node.id);

    let mut lang_strip = row().key("translation_lang_chips").center_y().gap(6.0);
    for &(code, label) in SUPPORTED_LANGUAGES {
        let selected = active_lang == code;
        let key: &'static str = match code {
            "en" => "lang_en",
            "es" => "lang_es",
            "fr" => "lang_fr",
            "de" => "lang_de",
            "pt" => "lang_pt",
            "ja" => "lang_ja",
            _ => "lang_en",
        };
        lang_strip = lang_strip.child(
            underline_tab(key, label, selected).on_press(move |s: &mut AppState| {
                s.set_target_language(code);
            }),
        );
    }

    let action_row = row()
        .key("translation_section")
        .w_full()
        .center_y()
        .gap(8.0)
        .child(lang_strip)
        .child(el().grow(1.0))
        .child(ghost_btn(
            "btn_translate_node",
            if is_busy { "Translating..." } else { "Translate" },
            theme::text_primary(),
            move |s| {
                s.run_node_translation(&nid_trans, None);
            },
        ));

    let mut tr_col = col().w_full().gap(10.0).child(action_row);

    if let Some(translated) = node
        .meta
        .translated_text
        .as_deref()
        .filter(|t| !t.trim().is_empty())
    {
        let lang_badge = node
            .meta
            .target_language
            .as_deref()
            .unwrap_or(&active_lang)
            .to_uppercase();
        let translated_owned = translated.to_string();
        let tr_copied = s.is_recently_copied("translation");
        tr_col = tr_col.child(
            col()
                .key("translated_text_preview")
                .w_full()
                .gap(8.0)
                .pl(12.0)
                .py(2.0)
                .border_sides([0.0, 0.0, 0.0, 2.0], theme::accent_claude().alpha(0.65))
                .child(
                    text(translated)
                        .font(theme::BODY_TEXT)
                        .color(theme::text_primary())
                        .w_full()
                        .wrap(),
                )
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .gap(4.0)
                        .child(subtle_tag(&lang_badge, theme::accent_claude()))
                        .child(el().grow(1.0))
                        .child(
                            el()
                                .key("btn_translate_copy")
                                .px(6.0)
                                .py(3.0)
                                .radius(5.0)
                                .hover(theme::hover_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                                    tx.copy(translated_owned.clone());
                                    s.mark_copied("translation", "Copied translation");
                                })
                                .child(
                                    text(if tr_copied { "Copied ✓" } else { "Copy" })
                                        .font(theme::BADGE_TEXT)
                                        .color(if tr_copied {
                                            theme::accent_claude()
                                        } else {
                                            theme::text_secondary()
                                        }),
                                ),
                        )
                        .child(ghost_btn(
                            "btn_translate_spawn_note",
                            "+ Note",
                            theme::text_secondary(),
                            move |s| {
                                s.spawn_note_from_translation(&nid_spawn_tr);
                            },
                        ))
                        .child(ghost_btn(
                            "btn_translate_clear",
                            "×",
                            theme::text_muted(),
                            move |s| {
                                s.clear_translated_text(&nid_clear_tr);
                            },
                        )),
                ),
        );
    }

    tr_col
}
