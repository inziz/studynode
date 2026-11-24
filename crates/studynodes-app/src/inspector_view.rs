use rune::{
    Color, Cursor, Cx, El, FileDialogOptions, Icon, Phase, ScrollSpec, SurfaceFit, TextStyle, Tx,
    auto_grow, canvas, col, el, memo, row, text, text_input,
};
use rune_kit::Markdown;
use studynodes_graph::model::{
    CalendarViewMode, EdgeRelation, Node, NodeId, NodeKind, QuizDifficulty, QuizDisplayMode,
};
use studynodes_vision::{OcrEnginePreference, OcrMode, SUPPORTED_LANGUAGES};

use crate::doc_reader::{self, DocFormat, PdfLayoutMode, ReaderMode};
use crate::formula_math;
use crate::graph_formula;
use crate::i18n::{self, TextKey};
use crate::icons;
use crate::media_helper;
use crate::state::{ActivePdfState, AppState, DocViewTab, QuizAttemptStatus};
use crate::theme;

const SHEET_TITLE: TextStyle = TextStyle::new(19.0, 600.0).lh(24.0);

#[inline]
fn hairline() -> El {
    el().w_full().h(1.0).fill(theme::border_subtle())
}

fn subtle_tag(label: &str, color: Color) -> El {
    text(label).font(theme::SECTION_LABEL).color(color)
}

/// Extract byte ranges of `**...**` spans (outside fenced code blocks) so `Markdown::highlights`
/// paints a subtle accent highlight behind bolded terms in AI Chat messages.
fn extract_bold_highlight_ranges(src: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut ranges = Vec::new();
    let mut i = 0;
    let mut in_fence = false;

    while i < len {
        if i + 3 <= len && &bytes[i..i + 3] == b"```" {
            in_fence = !in_fence;
            i += 3;
            continue;
        }
        if !in_fence && i + 4 <= len && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            let content_start = i + 2;
            if content_start < len && !bytes[content_start].is_ascii_whitespace() {
                let mut end = content_start;
                let mut found = None;
                while end + 1 < len {
                    if bytes[end] == b'\n' {
                        break;
                    }
                    if bytes[end] == b'*' && bytes[end + 1] == b'*' {
                        if end > content_start && !bytes[end - 1].is_ascii_whitespace() {
                            found = Some(end);
                        }
                        break;
                    }
                    end += 1;
                }
                if let Some(close_pos) = found {
                    ranges.push(content_start..close_pos);
                    i = close_pos + 2;
                    continue;
                }
            }
        }
        i += 1;
    }
    ranges
}

fn render_chat_markdown(key_id: &str, md_text: &str) -> El {
    let md_hash = theme::hash_str(md_text);
    let theme_id = theme::active_theme();
    let md_owned = md_text.to_string();
    let key_owned = key_id.to_string();
    memo(key_id, (md_hash, theme_id), move || {
        let bold_ranges = extract_bold_highlight_ranges(&md_owned);
        Markdown::new(&md_owned)
            .selectable(true)
            .highlights(bold_ranges)
            .render(&theme::chat_markdown_style())
            .key(key_owned)
    })
}

fn render_inline_chat_key_drawer(s: &AppState) -> El {
    let has_custom = s.chat_has_custom_api_key();
    col()
        .key("ai_chat_key_drawer")
        .w_full()
        .px(16.0)
        .py(12.0)
        .gap(8.0)
        .fill(theme::bg_card())
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
        .child(
            row()
                .w_full()
                .center_y()
                .gap(6.0)
                .child(subtle_tag(
                    i18n::t(TextKey::SettingsChatAiSectionTitle),
                    theme::accent_claude(),
                ))
                .child(el().grow(1.0))
                .child(
                    text(s.chat_provider_status_label())
                        .font(theme::BADGE_TEXT)
                        .color(if s.chat_api_configured() {
                            theme::accent_claude()
                        } else {
                            theme::text_muted()
                        }),
                ),
        )
        .child(
            el()
                .w_full()
                .px(8.0)
                .py(6.0)
                .radius(6.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_default())
                .child(
                    text_input(&s.chat_api_key_edit, theme::STATUS_TEXT)
                        .key("input_inline_chat_api_key")
                        .label("Custom Chat API Key")
                        .single_line()
                        .w_full()
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder(
                            "sk-air-... / sk-or-... / gsk_... / AIza... / sk-ant-... / sk-...",
                            theme::text_muted(),
                        )
                        .on_edit(|s: &mut AppState| &mut s.chat_api_key_edit)
                        .on_submit(|s: &mut AppState, _| {
                            s.save_chat_api_settings();
                        }),
                ),
        )
        .child(
            el()
                .w_full()
                .px(8.0)
                .py(5.0)
                .radius(6.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_subtle())
                .child(
                    text_input(&s.chat_base_url_edit, theme::STATUS_TEXT)
                        .key("input_inline_chat_base_url")
                        .label("Custom Chat Base URL")
                        .single_line()
                        .w_full()
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder(
                            "Base URL (optional, e.g. https://api.airforce/v1)",
                            theme::text_muted(),
                        )
                        .on_edit(|s: &mut AppState| &mut s.chat_base_url_edit)
                        .on_submit(|s: &mut AppState, _| {
                            s.save_chat_api_settings();
                        }),
                ),
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
                            text_input(&s.chat_model_edit, theme::STATUS_TEXT)
                                .key("input_inline_chat_model")
                                .label("Custom Chat Model")
                                .single_line()
                                .w_full()
                                .color(theme::text_primary())
                                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                                .placeholder(
                                    "Model (optional, e.g. gemini-2.5-flash or gpt-4o-mini)",
                                    theme::text_muted(),
                                )
                                .on_edit(|s: &mut AppState| &mut s.chat_model_edit)
                                .on_submit(|s: &mut AppState, _| {
                                    s.save_chat_api_settings();
                                }),
                        ),
                )
                .child(if has_custom || !s.chat_api_key_edit.text.trim().is_empty() {
                    ghost_btn(
                        "btn_inline_chat_key_clear",
                        i18n::t(TextKey::ReaderClear),
                        theme::text_muted(),
                        |s| {
                            s.chat_api_key_edit = rune::TextEdit::new("");
                            s.chat_base_url_edit = rune::TextEdit::new("");
                            s.chat_model_edit = rune::TextEdit::new("");
                            s.save_chat_api_settings();
                        },
                    )
                } else {
                    el()
                })
                .child(ghost_btn(
                    "btn_ai_chat_open_settings",
                    i18n::t(TextKey::DashSettingsBtn),
                    theme::text_secondary(),
                    |s| {
                        s.open_settings();
                    },
                ))
                .child(
                    el()
                        .key("btn_inline_chat_key_save")
                        .px(10.0)
                        .py(5.0)
                        .radius(6.0)
                        .fill(theme::accent_claude())
                        .hover(theme::accent_claude().alpha(0.86))
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.save_chat_api_settings();
                        })
                        .child(
                            text(i18n::t(TextKey::ReaderSave))
                                .font(theme::BADGE_TEXT)
                                .color(Color::WHITE),
                        ),
                ),
        )
}

fn render_dedicated_ai_chat_panel(s: &AppState, panel_width: f32) -> El {
    let total_nodes = s
        .graph
        .nodes
        .values()
        .filter(|n| n.kind != NodeKind::Section)
        .count();
    let sel_count = s.selected_nodes.len();
    let ctx_badge = if sel_count > 0 {
        format!("{} selected · {}", sel_count, total_nodes)
    } else {
        format!("{} nodes", total_nodes)
    };
    let has_selection = !s.selected_nodes.is_empty() || s.selected_edge.is_some();
    let has_custom_key = s.chat_has_custom_api_key();

    // 1. Clean Top Header Bar
    let mut topbar = row()
        .key("ai_chat_topbar")
        .w_full()
        .h(44.0)
        .px(16.0)
        .gap(8.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle());

    if has_selection {
        topbar = topbar.child(
            el()
                .key("btn_back_from_ai")
                .size(24.0, 24.0)
                .radius(5.0)
                .hover(theme::hover_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.ai_panel_open = false;
                })
                .child(icons::ph(
                    &icons::ARROW_LEFT,
                    14.0,
                    theme::text_secondary(),
                )),
        );
    }

    topbar = topbar
        .child(icons::ph(&icons::SPARKLE, 14.0, theme::accent_claude()))
        .child(
            text(i18n::t(TextKey::AiChatPanelTitle))
                .font(theme::PANEL_TITLE)
                .color(theme::text_primary()),
        )
        .child(
            el()
                .px(6.0)
                .py(2.0)
                .radius(4.0)
                .fill(theme::bg_card())
                .border(1.0, theme::border_subtle())
                .child(
                    text(ctx_badge)
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_muted()),
                ),
        )
        .child(el().grow(1.0))
        .child(
            el()
                .key("btn_ai_chat_toggle_key_drawer")
                .px(7.0)
                .py(3.5)
                .radius(5.0)
                .fill(if s.ai_chat_key_drawer_open || has_custom_key {
                    theme::accent_claude().alpha(0.14)
                } else {
                    Color::TRANSPARENT
                })
                .hover(theme::hover_subtle())
                .border(
                    1.0,
                    if s.ai_chat_key_drawer_open || has_custom_key {
                        theme::accent_claude().alpha(0.42)
                    } else {
                        theme::border_subtle()
                    },
                )
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.ai_chat_key_drawer_open = !s.ai_chat_key_drawer_open;
                    if s.ai_chat_key_drawer_open {
                        s.chat_api_key_edit = rune::TextEdit::new(&s.reader_prefs.chat_api_key);
                        s.chat_base_url_edit = rune::TextEdit::new(&s.reader_prefs.chat_base_url);
                        s.chat_model_edit = rune::TextEdit::new(&s.reader_prefs.chat_model);
                    }
                })
                .child(
                    text(i18n::t(TextKey::AiChatCustomKeyBtn))
                        .font(theme::BADGE_TEXT)
                        .color(if s.ai_chat_key_drawer_open || has_custom_key {
                            theme::accent_claude()
                        } else {
                            theme::text_secondary()
                        }),
                ),
        );

    if !s.ai_chat_messages.is_empty() {
        topbar = topbar.child(ghost_btn(
            "btn_ai_chat_clear",
            i18n::t(TextKey::ReaderClear),
            theme::text_muted(),
            |s| {
                s.clear_ai_chat();
            },
        ));
    }

    topbar = topbar.child(
        el()
            .key("btn_close_inspector")
            .size(24.0, 24.0)
            .radius(5.0)
            .hover(theme::hover_subtle())
            .press(theme::press_subtle())
            .center()
            .cursor(Cursor::Pointer)
            .on_press(|s: &mut AppState| {
                s.inspector_open = false;
                s.ai_panel_open = false;
            })
            .child(icons::ph(&icons::X, 13.0, theme::text_muted())),
    );

    // 2. Scrollable Messages Feed
    let mut msg_list = col()
        .key("ai_chat_messages_list")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .px(18.0)
        .py(16.0)
        .gap(16.0)
        .scroll(ScrollSpec { stick_bottom: true });

    if s.ai_chat_messages.is_empty() && !s.ai_chat_pending {
        msg_list = msg_list.child(
            col()
                .key("ai_chat_empty_hint")
                .w_full()
                .grow(1.0)
                .center()
                .py(28.0)
                .child(
                    text(i18n::t(TextKey::AiChatEmptyHint))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                ),
        );
    }

    for (idx, msg) in s.ai_chat_messages.iter().enumerate() {
        let md_key = format!("ai_chat_md_{}", idx);
        if msg.role == "user" {
            msg_list = msg_list.child(
                row()
                    .key(format!("ai_chat_msg_{}", idx))
                    .w_full()
                    .child(el().grow(1.0))
                    .child(
                        col()
                            .max_w((panel_width - 76.0).max(180.0))
                            .px(13.0)
                            .py(9.0)
                            .radius(14.0)
                            .continuous_corners()
                            .fill(theme::bg_control())
                            .child(render_chat_markdown(&md_key, &msg.text)),
                    ),
            );
        } else {
            let reply_copy = msg.text.clone();
            let copied = s.is_recently_copied("ai_chat_reply");
            let mut action_footer = row()
                .w_full()
                .center_y()
                .gap(6.0)
                .child(
                    el()
                        .key(format!("btn_ai_chat_copy_{}", idx))
                        .px(6.0)
                        .py(2.5)
                        .radius(4.0)
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                            tx.copy(reply_copy.clone());
                            s.mark_copied("ai_chat_reply", "Copied AI response");
                        })
                        .child(
                            text(if copied {
                                i18n::t(TextKey::ReaderCopied)
                            } else {
                                i18n::t(TextKey::ReaderCopy)
                            })
                            .font(theme::BADGE_TEXT)
                            .color(if copied {
                                theme::accent_claude()
                            } else {
                                theme::text_muted()
                            }),
                        ),
                )
                .child(
                    el()
                        .key(format!("btn_ai_chat_spawn_note_{}", idx))
                        .px(7.0)
                        .py(2.5)
                        .radius(4.0)
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.spawn_note_from_ai_chat(idx);
                        })
                        .child(
                            text(format!("+ {}", i18n::t(TextKey::DockNote)))
                                .font(theme::BADGE_TEXT)
                                .color(theme::text_muted()),
                        ),
                );
            if msg.can_undo {
                action_footer = action_footer.child(
                    el()
                        .key(format!("btn_ai_chat_undo_{}", idx))
                        .px(7.0)
                        .py(2.5)
                        .radius(4.0)
                        .fill(theme::accent_claude().alpha(0.16))
                        .border(1.0, theme::accent_claude().alpha(0.42))
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.undo_ai_chat_action(idx);
                        })
                        .child(
                            text(format!("↶ {}", i18n::t(TextKey::AiChatUndoAction)))
                                .font(theme::BADGE_TEXT)
                                .color(theme::accent_claude()),
                        ),
                );
            }
            action_footer = action_footer.child(el().grow(1.0));

            let mut msg_col = col()
                .key(format!("ai_chat_msg_{}", idx))
                .w_full()
                .gap(6.0);
            if let Some(summary) = msg.action_summary.as_deref() {
                msg_col = msg_col.child(
                    row()
                        .key(format!("ai_chat_action_summary_{}", idx))
                        .w_full()
                        .px(8.0)
                        .py(4.0)
                        .radius(6.0)
                        .fill(theme::accent_claude().alpha(0.12))
                        .border(1.0, theme::accent_claude().alpha(0.35))
                        .child(
                            text(summary)
                                .font(theme::BADGE_TEXT)
                                .color(theme::accent_claude())
                                .wrap(),
                        ),
                );
            }
            msg_list = msg_list.child(
                msg_col
                    .child(render_chat_markdown(&md_key, &msg.text))
                    .child(action_footer),
            );
        }
    }

    if s.ai_chat_pending {
        msg_list = msg_list.child(
            row()
                .key("ai_chat_thinking")
                .w_full()
                .py(4.0)
                .child(
                    text(i18n::t(TextKey::AiChatThinking))
                        .font(theme::STATUS_TEXT)
                        .color(theme::accent_claude()),
                ),
        );
    }

    // 3. Pinned Bottom Rounded-Square Composer Box + Circular Send Button
    let prompt_bar = col()
        .key("ai_chat_prompt_bar")
        .w_full()
        .px(14.0)
        .pb(14.0)
        .pt(6.0)
        .child(
            col()
                .w_full()
                .min_h(76.0)
                .padding(12.0)
                .gap(8.0)
                .radius(14.0)
                .continuous_corners()
                .fill(theme::bg_card())
                .border(1.0, theme::border_default())
                .child(
                    auto_grow(
                        text_input(&s.ai_chat_input, theme::BODY_TEXT)
                            .key("ai_chat_input")
                            .label("Project AI question")
                            .w_full()
                            .color(theme::text_primary())
                            .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                            .placeholder(i18n::t(TextKey::AiChatPlaceholder), theme::text_muted())
                            .on_edit(|s: &mut AppState| &mut s.ai_chat_input)
                            .on_submit(|s: &mut AppState, _| {
                                s.submit_ai_chat_question();
                            }),
                        2,
                        7,
                    )
                    .w_full()
                    .max_h(156.0),
                )
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .gap(6.0)
                        .child(
                            el()
                                .key("btn_toggle_chat_canvas_actions")
                                .px(8.0)
                                .py(4.0)
                                .radius(6.0)
                                .fill(if s.ai_chat_canvas_actions {
                                    theme::accent_claude().alpha(0.18)
                                } else {
                                    theme::bg_input()
                                })
                                .border(
                                    1.0,
                                    if s.ai_chat_canvas_actions {
                                        theme::accent_claude().alpha(0.65)
                                    } else {
                                        theme::border_subtle()
                                    },
                                )
                                .hover(theme::hover_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press(|s: &mut AppState| {
                                    s.ai_chat_canvas_actions = !s.ai_chat_canvas_actions;
                                })
                                .child(
                                    text(i18n::t(TextKey::AiChatCanvasActionsToggle))
                                        .font(theme::BADGE_TEXT)
                                        .color(if s.ai_chat_canvas_actions {
                                            theme::accent_claude()
                                        } else {
                                            theme::text_muted()
                                        }),
                                ),
                        )
                        .child(el().grow(1.0))
                        .child(
                            el()
                                .key("btn_ai_chat_send")
                                .size(32.0, 32.0)
                                .radius(16.0)
                                .fill(theme::accent_claude())
                                .hover(theme::accent_claude().alpha(0.86))
                                .press(Color::BLACK.alpha(0.18))
                                .center()
                                .cursor(Cursor::Pointer)
                                .on_press(|s: &mut AppState| {
                                    s.submit_ai_chat_question();
                                })
                                .child(icons::ph(&icons::ARROW_UP, 14.0, Color::WHITE)),
                        ),
                ),
        );

    let mut chat_tabs_strip = row()
        .key("ai_chat_tabs_strip")
        .gap(2.0)
        .center_y()
        .grow(1.0)
        .min_w(0.0);
    let active_chat_id = s.graph.active_chat_id.as_deref();
    let multi_chats = s.graph.chat_threads.len() > 1;
    for (idx, thread) in s.graph.chat_threads.iter().enumerate() {
        let is_active = active_chat_id == Some(thread.id.as_str());
        let tid_switch = thread.id.clone();
        let tid_close = thread.id.clone();
        let mut tab_shell = row()
            .key(format!("ai_chat_tab_shell_{}", idx))
            .h(35.0)
            .shrink()
            .min_w(52.0)
            .max_w(156.0)
            .fill(if is_active {
                theme::bg_chrome()
            } else {
                Color::TRANSPARENT
            })
            .border_sides(
                [0.0, 0.0, 1.5, 0.0],
                if is_active {
                    theme::accent_claude()
                } else {
                    Color::TRANSPARENT
                },
            )
            .hover(theme::hover_subtle())
            .center_y()
            .child(
                row()
                    .key(format!("ai_chat_tab_{}", idx))
                    .grow(1.0)
                    .min_w(0.0)
                    .h_full()
                    .pl(9.0)
                    .pr(if multi_chats { 3.0 } else { 9.0 })
                    .gap(5.0)
                    .center_y()
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.switch_ai_chat_tab(&tid_switch);
                    })
                    .child(icons::ph(
                        &icons::CHAT_CIRCLE,
                        11.5,
                        if is_active {
                            theme::accent_claude()
                        } else {
                            theme::text_muted()
                        },
                    ))
                    .child(
                        text(&thread.title)
                            .font(theme::STATUS_TEXT)
                            .color(if is_active {
                                theme::text_primary()
                            } else {
                                theme::text_muted()
                            })
                            .truncate(),
                    ),
            );
        if multi_chats {
            tab_shell = tab_shell.child(
                el()
                    .key(format!("btn_close_ai_chat_tab_{}", idx))
                    .size(20.0, 22.0)
                    .radius(4.0)
                    .hover(theme::hover_subtle())
                    .center()
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.close_ai_chat_tab(&tid_close);
                    })
                    .child(icons::ph(
                        &icons::X,
                        10.5,
                        if is_active {
                            theme::text_secondary()
                        } else {
                            theme::text_muted().alpha(0.6)
                        },
                    )),
            );
        }
        chat_tabs_strip = chat_tabs_strip.child(tab_shell);
    }

    let chat_tabs_bar = row()
        .key("ai_chat_tabs_bar")
        .w_full()
        .h(35.0)
        .px(10.0)
        .gap(4.0)
        .center_y()
        .fill(theme::bg_canvas().alpha(0.55))
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
        .child(chat_tabs_strip)
        .child(
            el()
                .key("btn_new_ai_chat_tab")
                .size(26.0, 26.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .on_press_tx(|s: &mut AppState, tx| {
                    s.new_ai_chat_tab();
                    tx.focus_when_shown("ai_chat_input", false);
                })
                .child(icons::ph(&icons::PLUS, 13.0, theme::text_muted())),
        );

    let mut panel = col()
        .key("inspector_panel")
        .w(panel_width)
        .h_full()
        .fill(theme::bg_chrome())
        .child(topbar)
        .child(chat_tabs_bar);

    if s.ai_chat_key_drawer_open {
        panel = panel.child(render_inline_chat_key_drawer(s));
    }

    if let Some(err) = s.llm_last_error.as_deref() {
        panel = panel.child(
            row()
                .key("ai_panel_error_banner")
                .w_full()
                .pl(14.0)
                .pr(10.0)
                .py(8.0)
                .gap(6.0)
                .center_y()
                .fill(theme::accent_danger().alpha(0.08))
                .border_sides([0.0, 0.0, 1.0, 2.0], theme::accent_danger().alpha(0.75))
                .child(
                    text(err)
                        .font(theme::STATUS_TEXT)
                        .color(theme::accent_danger())
                        .grow(1.0)
                        .min_w(0.0)
                        .wrap(),
                )
                .child(ghost_btn(
                    "btn_ai_panel_error_settings",
                    i18n::t(TextKey::AiChatCustomKeyBtn),
                    theme::text_secondary(),
                    |s| {
                        s.ai_chat_key_drawer_open = true;
                        s.chat_api_key_edit = rune::TextEdit::new(&s.reader_prefs.chat_api_key);
                        s.chat_base_url_edit = rune::TextEdit::new(&s.reader_prefs.chat_base_url);
                        s.chat_model_edit = rune::TextEdit::new(&s.reader_prefs.chat_model);
                    },
                ))
                .child(
                    el()
                        .key("btn_dismiss_ai_error")
                        .size(22.0, 22.0)
                        .radius(5.0)
                        .hover(theme::hover_subtle())
                        .center()
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.llm_last_error = None;
                        })
                        .child(icons::ph(&icons::X, 12.0, theme::text_muted())),
                ),
        );
    }

    panel.child(msg_list).child(prompt_bar)
}

fn render_ai_summary_callout(s: &AppState, node: &Node) -> Option<El> {
    let is_pending = s.llm_pending_nodes.contains(&node.id);
    let err_opt = s.llm_last_error.as_deref();
    let summary_opt = node.summary.as_deref().filter(|t| !t.trim().is_empty());

    if !is_pending && err_opt.is_none() && summary_opt.is_none() {
        return None;
    }

    let mut box_col = col().w_full().gap(6.0);

    if is_pending {
        box_col = box_col.child(
            row()
                .key("ai_summary_pending")
                .w_full()
                .pl(12.0)
                .py(4.0)
                .border_sides([0.0, 0.0, 0.0, 2.0], theme::accent_claude().alpha(0.65))
                .child(
                    text(i18n::t(TextKey::ReaderSummarizing))
                        .font(theme::STATUS_TEXT)
                        .color(theme::accent_claude()),
                ),
        );
    }

    if let Some(err) = err_opt {
        box_col = box_col.child(
            row()
                .key("ai_summary_error")
                .w_full()
                .gap(8.0)
                .center_y()
                .pl(12.0)
                .py(4.0)
                .border_sides([0.0, 0.0, 0.0, 2.0], theme::accent_danger().alpha(0.7))
                .child(
                    text(err)
                        .font(theme::STATUS_TEXT)
                        .color(theme::accent_danger())
                        .grow(1.0)
                        .min_w(0.0)
                        .wrap(),
                )
                .child(ghost_btn(
                    "btn_ai_error_settings",
                    i18n::t(TextKey::DashSettingsBtn),
                    theme::text_secondary(),
                    |s| {
                        s.open_settings();
                    },
                ))
                .child(
                    el()
                        .key("btn_dismiss_summary_ai_error")
                        .size(22.0, 22.0)
                        .radius(5.0)
                        .hover(theme::hover_subtle())
                        .center()
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.llm_last_error = None;
                        })
                        .child(icons::ph(&icons::X, 12.0, theme::text_muted())),
                ),
        );
    }

    if let Some(summary) = summary_opt {
        let summary_owned = summary.to_string();
        let summary_copied = s.is_recently_copied("summary");
        let nid_clear = node.id.clone();

        box_col = box_col.child(
            col()
                .key("ai_summary_callout")
                .w_full()
                .gap(4.0)
                .pl(12.0)
                .py(2.0)
                .border_sides([0.0, 0.0, 0.0, 2.0], theme::accent_claude().alpha(0.65))
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .gap(4.0)
                        .child(subtle_tag(
                            i18n::t(TextKey::ReaderAiSummaryTag),
                            theme::accent_claude(),
                        ))
                        .child(el().grow(1.0))
                        .child(
                            el()
                                .key("btn_copy_ai_summary")
                                .px(6.0)
                                .py(2.5)
                                .radius(5.0)
                                .hover(theme::hover_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                                    tx.copy(summary_owned.clone());
                                    s.mark_copied("summary", "Copied AI summary");
                                })
                                .child(
                                    text(if summary_copied {
                                        i18n::t(TextKey::ReaderCopied)
                                    } else {
                                        i18n::t(TextKey::ReaderCopy)
                                    })
                                    .font(theme::BADGE_TEXT)
                                    .color(if summary_copied {
                                        theme::accent_claude()
                                    } else {
                                        theme::text_secondary()
                                    }),
                                ),
                        )
                        .child(
                            el()
                                .key("btn_clear_ai_summary")
                                .px(5.0)
                                .py(2.5)
                                .radius(5.0)
                                .hover(theme::hover_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press(move |s: &mut AppState| {
                                    s.clear_node_summary(&nid_clear);
                                })
                                .child(icons::ph(&icons::X, 11.0, theme::text_muted())),
                        ),
                )
                .child(
                    text(summary)
                        .font(theme::BODY_TEXT)
                        .color(theme::text_secondary())
                        .w_full()
                        .wrap(),
                ),
        );
    }

    Some(box_col)
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
    key: impl Into<String>,
    ico: &'static Icon,
    fg: Color,
    active: bool,
    action: impl Fn(&mut AppState) + 'static,
) -> El {
    el()
        .key(key.into())
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
        .child(icons::ph(ico, 14.0, fg))
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
    ghost_icon_btn(
        "btn_popout_reader",
        &icons::ARROW_SQUARE_OUT,
        theme::text_secondary(),
        false,
        |s| {
            s.pop_out_reader();
        },
    )
}

fn close_panel_btn() -> El {
    ghost_icon_btn("btn_close_inspector", &icons::X, theme::text_muted(), false, |s| {
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

    // Dedicated Full-Height Project AI Chat panel when active
    if s.ai_panel_open {
        let chat_panel = render_dedicated_ai_chat_panel(s, panel_width);
        return row()
            .key("inspector_container")
            .h_full()
            .child(resize_handle)
            .child(chat_panel);
    }

    // Distraction-Free Read-Only Reader layout (Document / Image) when active
    if s.reader_read_only {
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

    if let Some(selected_node_id) = s.selected_nodes.iter().next() {
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
            .child(
                row()
                    .key("btn_inspector_open_ai_chat")
                    .w_full()
                    .px(12.0)
                    .py(9.0)
                    .gap(8.0)
                    .center_y()
                    .radius(8.0)
                    .fill(theme::accent_claude().alpha(0.14))
                    .hover(theme::accent_claude().alpha(0.24))
                    .border(1.0, theme::accent_claude().alpha(0.45))
                    .cursor(Cursor::Pointer)
                    .on_press_tx(|s: &mut AppState, tx| {
                        s.open_ai_chat_panel();
                        tx.focus_when_shown("ai_chat_input", false);
                    })
                    .child(
                        text(i18n::t(TextKey::AiAskProjectChat))
                            .font(theme::STATUS_TEXT)
                            .color(theme::text_primary()),
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

fn render_memoized_markdown(
    node_id: &NodeId,
    md_src: String,
    doc_fmt: DocFormat,
    md_w: f32,
    is_readonly: bool,
) -> El {
    let md_hash = theme::hash_str(&md_src);
    let version = (
        node_id.clone(),
        md_hash,
        doc_fmt,
        md_w.round() as i32,
        theme::active_theme(),
        i18n::active_locale(),
        is_readonly,
    );
    memo("inspector_md_preview", version, move || {
        let md_style = theme::markdown_style().text_width(md_w);
        let mut md = Markdown::new(&md_src).selectable(true);
        if !is_readonly {
            md = md.on_task(|s: &mut AppState, _block_key, offset, checked| {
                s.toggle_selected_doc_task(offset, checked);
            });
        }
        md.render(&md_style)
    })
}

fn inspector_pdf_aspect(pdf: &ActivePdfState, page_idx: usize) -> f32 {
    if let Some(&(w, h)) = pdf.page_dimensions.get(page_idx) {
        if w > 0.0 && h > 0.0 {
            return (w / h).clamp(0.35, 2.5);
        }
    }
    if let Some(&(w, h, _)) = pdf.pages.get(&page_idx) {
        if w > 0 && h > 0 {
            return (w as f32 / h as f32).clamp(0.35, 2.5);
        }
    }
    8.5 / 11.0
}

fn render_inspector_pdf_page(
    pdf: &ActivePdfState,
    page_idx: usize,
    target_w: f32,
    show_caption: bool,
) -> El {
    let ar = inspector_pdf_aspect(pdf, page_idx);
    let page_box = if let Some((_, _, px)) = pdf.pages.get(&page_idx) {
        el()
            .key(format!("pdf_page_canvas_{}", page_idx))
            .w(target_w)
            .aspect_ratio(ar)
            .radius(4.0)
            .border(1.0, theme::border_subtle())
            .clip_soft(0.0)
            .pixels_fit(px, SurfaceFit::Fit)
    } else {
        el()
            .key(format!("pdf_page_canvas_{}", page_idx))
            .w(target_w)
            .aspect_ratio(ar)
            .radius(4.0)
            .fill(theme::bg_card())
            .border(1.0, theme::border_subtle())
            .center()
            .child(
                text(format!(
                    "{} · p. {}",
                    i18n::t(TextKey::PdfRenderingPage),
                    page_idx + 1
                ))
                .font(theme::BADGE_TEXT)
                .color(theme::text_muted()),
            )
    };

    if show_caption {
        col()
            .gap(3.0)
            .items_center()
            .child(page_box)
            .child(
                text(format!("p. {}", page_idx + 1))
                    .font(theme::BADGE_TEXT)
                    .color(theme::text_muted()),
            )
    } else {
        page_box
    }
}

fn render_pdf_reader_panel(
    s: &AppState,
    pdf: &ActivePdfState,
    panel_width: f32,
    max_h: Option<f32>,
) -> El {
    let total_pages = pdf.page_count.max(1);
    let cur_page = pdf.page_index.min(total_pages - 1);
    let cur_zoom = pdf.zoom;
    let mode = s.reader_prefs.pdf_layout_mode;
    let step = if mode == PdfLayoutMode::TwoPage { 2 } else { 1 };

    let controls_row = row()
        .key("readonly_pdf_controls")
        .w_full()
        .center_y()
        .gap(4.0)
        .child(ghost_btn(
            "btn_pdf_prev",
            i18n::t(TextKey::PdfPrevPage),
            theme::text_secondary(),
            move |s| {
                s.set_pdf_page(cur_page.saturating_sub(step));
            },
        ))
        .child(
            text(i18n::fmt_page_counter(cur_page, total_pages))
                .font(theme::STATUS_TEXT)
                .color(theme::text_muted()),
        )
        .child(ghost_btn(
            "btn_pdf_next",
            i18n::t(TextKey::PdfNextPage),
            theme::text_secondary(),
            move |s| {
                let next = (cur_page + step).min(total_pages.saturating_sub(1));
                s.set_pdf_page(next);
            },
        ))
        .child(el().grow(1.0))
        .child(ghost_btn("btn_pdf_zout", "−", theme::text_muted(), move |s| {
            s.set_pdf_zoom(cur_zoom - 0.25);
        }))
        .child(ghost_btn(
            "btn_pdf_zoom_reset",
            &format!("{}%", (cur_zoom * 100.0).round() as u32),
            theme::text_muted(),
            |s| {
                s.set_pdf_zoom(1.0);
            },
        ))
        .child(ghost_btn("btn_pdf_zin", "+", theme::text_muted(), move |s| {
            s.set_pdf_zoom(cur_zoom + 0.25);
        }));

    let mut mode_row = row()
        .key("pdf_mode_controls")
        .w_full()
        .center_y()
        .gap(10.0)
        .child(
            underline_tab(
                "btn_pdf_mode_continuous",
                i18n::t(TextKey::PdfModeContinuous),
                mode == PdfLayoutMode::Continuous,
            )
            .on_press(|s: &mut AppState| {
                s.set_pdf_layout_mode(PdfLayoutMode::Continuous);
            }),
        )
        .child(
            underline_tab(
                "btn_pdf_mode_twopage",
                i18n::t(TextKey::PdfModeTwoPage),
                mode == PdfLayoutMode::TwoPage,
            )
            .on_press(|s: &mut AppState| {
                s.set_pdf_layout_mode(PdfLayoutMode::TwoPage);
            }),
        )
        .child(
            underline_tab(
                "btn_pdf_mode_single",
                i18n::t(TextKey::PdfModeSingle),
                mode == PdfLayoutMode::SinglePage,
            )
            .on_press(|s: &mut AppState| {
                s.set_pdf_layout_mode(PdfLayoutMode::SinglePage);
            }),
        );

    if total_pages > 1 {
        let last = total_pages - 1;
        let mut jump_pages: Vec<usize> = vec![0];
        if cur_page > 1 {
            jump_pages.push(cur_page - 1);
        }
        jump_pages.push(cur_page);
        if cur_page + 1 < last {
            jump_pages.push(cur_page + 1);
        }
        jump_pages.push(last);
        jump_pages.sort_unstable();
        jump_pages.dedup();

        mode_row = mode_row.child(el().grow(1.0));
        let mut prev_pg: Option<usize> = None;
        for pg in jump_pages {
            if prev_pg.is_some_and(|p| pg > p + 1) {
                mode_row = mode_row.child(icons::ph(&icons::DOTS_THREE, 12.0, theme::text_muted()));
            }
            let is_cur = pg == cur_page;
            mode_row = mode_row.child(
                el()
                    .key(format!("btn_pdf_jump_{}", pg))
                    .px(5.0)
                    .py(2.0)
                    .radius(4.0)
                    .fill(if is_cur {
                        theme::accent_claude().alpha(0.20)
                    } else {
                        Color::TRANSPARENT
                    })
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.set_pdf_page(pg);
                    })
                    .child(
                        text(format!("{}", pg + 1))
                            .font(theme::BADGE_TEXT)
                            .color(if is_cur {
                                theme::accent_claude()
                            } else {
                                theme::text_muted()
                            }),
                    ),
            );
            prev_pg = Some(pg);
        }
    }

    let mut pdf_col = col()
        .key("integrated_pdf_viewer")
        .w_full()
        .gap(8.0)
        .child(controls_row)
        .child(mode_row);
    if max_h.is_none() {
        pdf_col = pdf_col.grow(1.0).min_h(0.0);
    }

    if let Some(err) = &pdf.error {
        return pdf_col.child(
            text(format!("PDF render note: {}", err))
                .font(theme::BODY_TEXT)
                .color(theme::text_secondary())
                .w_full()
                .wrap(),
        );
    }

    let avail_w = (panel_width - 52.0).max(200.0);
    let page_w = (avail_w * cur_zoom).max(180.0);
    let mut inner = col().w_full().gap(12.0).items_center().py(2.0);

    match mode {
        PdfLayoutMode::Continuous => {
            let (start, end) = if total_pages <= 12 {
                (0, total_pages - 1)
            } else {
                (cur_page.saturating_sub(2), (cur_page + 6).min(total_pages - 1))
            };
            if start > 0 {
                inner = inner.child(ghost_btn(
                    "btn_pdf_prev_window",
                    i18n::t(TextKey::PdfPrevWindow),
                    theme::text_secondary(),
                    move |s| {
                        s.set_pdf_page(start.saturating_sub(1));
                    },
                ));
            }
            for pg in start..=end {
                inner = inner.child(render_inspector_pdf_page(pdf, pg, page_w, total_pages > 1));
            }
            if end + 1 < total_pages {
                inner = inner.child(ghost_btn(
                    "btn_pdf_next_window",
                    i18n::t(TextKey::PdfNextWindow),
                    theme::text_secondary(),
                    move |s| {
                        s.set_pdf_page(end + 1);
                    },
                ));
            }
        }
        PdfLayoutMode::TwoPage => {
            let left = (cur_page / 2) * 2;
            let right = left + 1;
            if right < total_pages && avail_w >= 420.0 {
                let spread_w = ((avail_w - 10.0) * 0.48 * cur_zoom).max(140.0);
                inner = inner.child(
                    row()
                        .gap(10.0)
                        .center()
                        .child(render_inspector_pdf_page(pdf, left, spread_w, true))
                        .child(render_inspector_pdf_page(pdf, right, spread_w, true)),
                );
            } else {
                inner = inner.child(render_inspector_pdf_page(pdf, left, page_w, total_pages > 1));
                if right < total_pages {
                    inner = inner.child(render_inspector_pdf_page(pdf, right, page_w, true));
                }
            }
        }
        PdfLayoutMode::SinglePage => {
            inner = inner.child(render_inspector_pdf_page(pdf, cur_page, page_w, false));
        }
    }

    let mut scroll_box = col().w_full().scroll(ScrollSpec { stick_bottom: false });
    if let Some(h) = max_h {
        scroll_box = scroll_box.max_h(h);
    } else {
        scroll_box = scroll_box.grow(1.0).min_h(0.0);
    }
    pdf_col.child(scroll_box.child(inner))
}

fn render_ingest_button(node_id: studynodes_graph::model::NodeId, is_img: bool) -> El {
    ghost_btn(
        "btn_doc_ingest",
        if is_img {
            i18n::t(TextKey::ReaderIngestImage)
        } else {
            i18n::t(TextKey::ReaderIngestFile)
        },
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
    let is_summarizing = s.llm_pending_nodes.contains(&node.id);

    // 4-Control Minimal Header: Kind tag | Edit/Done | Pop Out | overflow ⋯ | Close ×
    let header = row()
        .w_full()
        .center_y()
        .gap(6.0)
        .child(icons::ph(
            icons::kind_icon(node_kind),
            15.0,
            theme::kind_accent(node_kind),
        ))
        .child(subtle_tag(fmt_badge, theme::text_muted()))
        .child(el().grow(1.0))
        .child(ghost_btn(
            "btn_toggle_readonly",
            i18n::t(TextKey::ReaderDone),
            theme::text_primary(),
            |s| {
                s.set_reader_read_only(true);
            },
        ))
        .child(popout_panel_btn())
        .child(ghost_icon_btn(
            "btn_inspector_more",
            &icons::DOTS_THREE,
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
                i18n::t(TextKey::ReaderOpenExternal),
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
                if is_summarizing {
                    i18n::t(TextKey::ReaderSummarizing)
                } else {
                    i18n::t(TextKey::ReaderSummarizeWithAi)
                },
                theme::accent_claude(),
                move |s| {
                    s.inspector_more_menu_open = false;
                    s.explain_node_ai(&nid_sum_menu);
                },
            ))
            .child(hairline())
            .child(more_menu_item(
                "btn_delete_doc",
                i18n::t(TextKey::ReaderDeleteNode),
                theme::accent_danger(),
                move |s| {
                    s.inspector_more_menu_open = false;
                    s.delete_node(&nid_del);
                },
            ));

        if is_img_kind {
            menu = menu.child(more_menu_item(
                "btn_doc_excerpt",
                i18n::t(TextKey::ReaderCreateExcerpt),
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
            i18n::t(TextKey::ReaderSample),
            theme::text_muted(),
            move |s| {
                s.load_sample_document(&nid_sample);
            },
        ));
    }

    actions_strip = actions_strip
        .child(ghost_btn(
            "btn_doc_summarize",
            if is_summarizing {
                i18n::t(TextKey::ReaderSummarizing)
            } else {
                i18n::t(TextKey::ReaderSummarize)
            },
            theme::accent_claude(),
            move |s| {
                s.explain_node_ai(&nid_sum);
            },
        ))
        .child(ghost_btn(
            "btn_doc_excerpt",
            i18n::t(TextKey::BlockExcerptTitle),
            theme::text_secondary(),
            |s| {
                s.create_excerpt_from_selected_doc(None);
            },
        ));
    if is_img_kind {
        let nid_frame = node.id.clone();
        let frameless = node.meta.image_frameless;
        actions_strip = actions_strip.child(ghost_btn(
            "btn_toggle_image_frame",
            if frameless { "Framed" } else { "Frameless" },
            if frameless {
                theme::accent_claude()
            } else {
                theme::text_secondary()
            },
            move |s| {
                s.toggle_image_frameless(&nid_frame);
            },
        ));
    }

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

    if let Some(callout) = render_ai_summary_callout(s, node) {
        content = content.child(callout);
    }

    // Quiet underline view switcher (`Pages · Preview · Source`)
    let mut view_switcher = row()
        .key("doc_view_tabs")
        .w_full()
        .gap(14.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle());

    if has_pdf {
        view_switcher = view_switcher.child(
            underline_tab(
                "tab_doc_pages",
                i18n::t(TextKey::ReaderTabPages),
                s.doc_view_tab == DocViewTab::Pages,
            )
            .on_press(|s: &mut AppState| {
                s.doc_view_tab = DocViewTab::Pages;
            }),
        );
    }
    view_switcher = view_switcher
        .child(
            underline_tab(
                "tab_doc_preview",
                if is_img_kind {
                    i18n::t(TextKey::DockImage)
                } else {
                    i18n::t(TextKey::ReaderTabPreview)
                },
                s.doc_view_tab == DocViewTab::Preview
                    || (!has_pdf && s.doc_view_tab == DocViewTab::Pages),
            )
            .on_press(|s: &mut AppState| {
                s.doc_view_tab = DocViewTab::Preview;
            }),
        )
        .child(
            underline_tab(
                "tab_doc_source",
                i18n::t(TextKey::ReaderTabSource),
                s.doc_view_tab == DocViewTab::Source,
            )
            .on_press(|s: &mut AppState| {
                s.doc_view_tab = DocViewTab::Source;
            }),
        );

    content = content.child(view_switcher);

    // Flush Reader / Source body (no inner box border)
    let panel_width = s.inspector_width.clamp(300.0, 860.0);
    let reader_body = if has_pdf && s.doc_view_tab == DocViewTab::Pages {
        let pdf = s.active_pdf.as_ref().unwrap();
        render_pdf_reader_panel(s, pdf, panel_width, Some(470.0))
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
                    .label(i18n::t(TextKey::ReaderTabSource))
                    .w_full()
                    .min_h(230.0)
                    .color(theme::text_primary())
                    .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                    .placeholder(
                        if is_img_kind {
                            i18n::t(TextKey::ReaderImageCaptionPlaceholder)
                        } else {
                            i18n::t(TextKey::ReaderSourcePlaceholder)
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

        let md_w = (panel_width - 52.0).max(220.0);
        let preview_img = s
            .active_image
            .as_ref()
            .filter(|(img_nid, _, _)| img_nid == &node.id)
            .map(|(_, _, img)| img)
            .or_else(|| s.card_image_previews.get(&node.id));
        if doc_format == DocFormat::Image {
            if let Some(img) = preview_img {
                let img_h = theme::scaled_image_height(img, md_w, 160.0, 320.0, 220.0);
                preview_col = preview_col.child(theme::render_fitted_image_box(
                    "integrated_image_preview",
                    img,
                    img_h,
                    6.0,
                ));
            }
        }

        if node.kind == NodeKind::Image {
            if !node.content.trim().is_empty() {
                let md_src = doc_reader::wrap_for_preview(&node.content, doc_format);
                let md_el = render_memoized_markdown(&node.id, md_src, doc_format, md_w, false);
                preview_col =
                    preview_col.child(el().key("image_caption_markdown").w_full().child(md_el));
            }
            preview_col
        } else {
            let is_empty = node.content.trim().is_empty();
            let md_src = if is_empty {
                i18n::t(TextKey::ReaderEmptyEditMd).to_string()
            } else {
                doc_reader::wrap_for_preview(&node.content, doc_format)
            };

            let md_el = render_memoized_markdown(&node.id, md_src, doc_format, md_w, false);

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
                        .label(i18n::t(TextKey::ReaderAppendPlaceholder))
                        .single_line()
                        .w_full()
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder(i18n::t(TextKey::ReaderAppendPlaceholder), theme::text_muted())
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
                    i18n::t(TextKey::ReaderSave),
                    theme::text_muted(),
                    |s| {
                        s.sync_node_edits();
                        if let Some(bundle) = &s.bundle {
                            let _ = bundle.save_graph_atomic(&s.graph);
                        }
                        s.notify_toast(i18n::t(TextKey::SettingsSavedToast));
                    },
                ))
                .child(el().grow(1.0))
                .child(ghost_btn(
                    "btn_delete_doc",
                    i18n::t(TextKey::DashConfirmDelete),
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
        .child(icons::ph(
            icons::kind_icon(node_kind),
            15.0,
            theme::kind_accent(node_kind),
        ))
        .child(subtle_tag(theme::kind_label(node_kind), theme::text_muted()))
        .child(el().grow(1.0));
    if crate::reader_window::can_pop_out(node_kind) {
        hdr = hdr.child(popout_panel_btn());
    }
    hdr = hdr
        .child(ghost_icon_btn(
            "btn_inspector_more",
            &icons::DOTS_THREE,
            theme::text_secondary(),
            s.inspector_more_menu_open,
            |s| {
                s.inspector_more_menu_open = !s.inspector_more_menu_open;
            },
        ))
        .child(close_panel_btn());

    let is_summarizing = s.llm_pending_nodes.contains(&node.id);
    let is_embed_media = matches!(node_kind, NodeKind::Video | NodeKind::Audio);
    let mut content = col().w_full().gap(14.0).child(hdr);

    if s.inspector_more_menu_open {
        let nid_exp_m = nid_explain.clone();
        let mut more_menu = col()
            .key("inspector_more_menu")
            .w_full()
            .padding(6.0)
            .gap(2.0)
            .radius(8.0)
            .fill(theme::bg_card())
            .border(1.0, theme::border_subtle());
        if !is_embed_media {
            more_menu = more_menu
                .child(more_menu_item(
                    "btn_menu_ai_explain",
                    if is_summarizing {
                        i18n::t(TextKey::ReaderSummarizing)
                    } else {
                        i18n::t(TextKey::ReaderExplainWithAi)
                    },
                    theme::accent_claude(),
                    move |s| {
                        s.inspector_more_menu_open = false;
                        s.explain_node_ai(&nid_exp_m);
                    },
                ))
                .child(hairline());
        }
        more_menu = more_menu.child(more_menu_item(
            "btn_menu_del_node",
            i18n::t(TextKey::ReaderDeleteNode),
            theme::accent_danger(),
            move |s| {
                s.inspector_more_menu_open = false;
                s.delete_node(&nid_del_menu);
            },
        ));
        content = content.child(more_menu);
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
        );

    if matches!(node_kind, NodeKind::Video | NodeKind::Audio) {
        content = content
            .child(render_media_inspector(s, node))
            .child(hairline());
    } else if node_kind == NodeKind::Calendar {
        content = content
            .child(render_calendar_inspector(s, node))
            .child(hairline());
    } else if node_kind == NodeKind::Drawing {
        content = content
            .child(render_drawing_inspector(s, node))
            .child(hairline());
    } else if node_kind == NodeKind::GraphFormula {
        content = content
            .child(render_graph_formula_inspector(s, node))
            .child(hairline());
    } else if node_kind == NodeKind::Formula {
        content = content
            .child(render_formula_inspector(s, node))
            .child(hairline());
    }

    if !matches!(
        node_kind,
        NodeKind::Video | NodeKind::Drawing | NodeKind::Quiz | NodeKind::GraphFormula | NodeKind::Formula
    ) {
        content = content
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
                                i18n::t(TextKey::ReaderSourcePlaceholder),
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
                    .child(ghost_btn(
                        "fmt_bullet",
                        i18n::t(TextKey::FmtList),
                        theme::text_muted(),
                        |s| s.append_to_selected_node("- "),
                    ))
                    .child(ghost_btn(
                        "fmt_heading",
                        i18n::t(TextKey::FmtHeading),
                        theme::text_muted(),
                        |s| s.append_to_selected_node("### "),
                    ))
                    .child(ghost_btn(
                        "fmt_todo",
                        i18n::t(TextKey::FmtTodo),
                        theme::text_muted(),
                        |s| s.append_to_selected_node("- [ ] "),
                    ))
                    .child(ghost_btn(
                        "fmt_quote",
                        i18n::t(TextKey::FmtQuote),
                        theme::text_muted(),
                        |s| s.append_to_selected_node("> "),
                    )),
            );

        if node_kind == NodeKind::Note && !node.content.trim().is_empty() {
            let md_w = (s.inspector_width.clamp(300.0, 860.0) - 68.0).max(200.0);
            content = content.child(
                col()
                    .key("note_inspector_md_preview")
                    .w_full()
                    .padding(10.0)
                    .gap(6.0)
                    .radius(8.0)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_subtle())
                    .child(render_memoized_markdown(
                        &node.id,
                        node.content.clone(),
                        DocFormat::Markdown,
                        md_w,
                        false,
                    )),
            );
        }
    }

    if !matches!(
        node_kind,
        NodeKind::Video | NodeKind::Drawing | NodeKind::GraphFormula | NodeKind::Formula
    ) {
        content = content.child(render_specialized_node_inspector(s, node));
    }

    if matches!(
        node_kind,
        NodeKind::Note
            | NodeKind::Excerpt
            | NodeKind::Concept
            | NodeKind::LlmSynthesis
            | NodeKind::Drawing
    ) {
        content = content
            .child(hairline())
            .child(render_translation_section(s, node));
    }

    if !is_embed_media {
        if let Some(callout) = render_ai_summary_callout(s, node) {
            content = content.child(callout);
        }
    }

    let mut footer_row = row().w_full().center_y().gap(4.0);
    if !is_embed_media {
        footer_row = footer_row.child(ghost_btn(
            "btn_node_ai_explain",
            if is_summarizing {
                i18n::t(TextKey::ReaderSummarizing)
            } else {
                i18n::t(TextKey::ReaderExplain)
            },
            theme::accent_claude(),
            move |s| {
                s.explain_node_ai(&nid_explain);
            },
        ));
    }
    footer_row = footer_row
        .child(ghost_btn(
            "btn_save_node",
            i18n::t(TextKey::ReaderSave),
            theme::text_muted(),
            |s| {
                s.sync_node_edits();
                if let Some(bundle) = &s.bundle {
                    let _ = bundle.save_graph_atomic(&s.graph);
                }
                s.notify_toast(i18n::t(TextKey::SettingsSavedToast));
            },
        ))
        .child(el().grow(1.0));

    content
        .child(hairline())
        .child(footer_row
                .child(ghost_btn(
                    "btn_delete_node",
                    i18n::t(TextKey::DashConfirmDelete),
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
                .child(subtle_tag(
                    i18n::t(TextKey::EdgeInspectorHeading),
                    theme::text_muted(),
                ))
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
                .child(
                    underline_tab(
                        "rel_supports",
                        &i18n::edge_relation_label(&EdgeRelation::Supports),
                        edge.relation == EdgeRelation::Supports,
                    )
                    .on_press(move |s: &mut AppState| {
                        if let Some(e) = s.graph.get_edge_mut(&eid_sup) {
                            e.relation = EdgeRelation::Supports;
                        }
                    }),
                )
                .child(
                    underline_tab(
                        "rel_contradicts",
                        &i18n::edge_relation_label(&EdgeRelation::Contradicts),
                        edge.relation == EdgeRelation::Contradicts,
                    )
                    .on_press(move |s: &mut AppState| {
                        if let Some(e) = s.graph.get_edge_mut(&eid_con) {
                            e.relation = EdgeRelation::Contradicts;
                        }
                    }),
                )
                .child(
                    underline_tab(
                        "rel_explains",
                        &i18n::edge_relation_label(&EdgeRelation::Explains),
                        edge.relation == EdgeRelation::Explains,
                    )
                    .on_press(move |s: &mut AppState| {
                        if let Some(e) = s.graph.get_edge_mut(&eid_exp) {
                            e.relation = EdgeRelation::Explains;
                        }
                    }),
                ),
        )
        .child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(
                    underline_tab(
                        "btn_edge_show_label",
                        i18n::t(TextKey::EdgeShowLabel),
                        !edge.hide_label,
                    )
                    .on_press(move |s: &mut AppState| {
                        s.toggle_edge_hide_label(&eid_show, false);
                    }),
                )
                .child(
                    underline_tab(
                        "btn_edge_hide_label",
                        i18n::t(TextKey::EdgeHideLabel),
                        edge.hide_label,
                    )
                    .on_press(move |s: &mut AppState| {
                        s.toggle_edge_hide_label(&eid_hide, true);
                    }),
                )
                .child(el().grow(1.0))
                .child(ghost_btn(
                    "btn_toggle_all_edge_labels",
                    if s.hide_all_edge_labels {
                        i18n::t(TextKey::EdgeShowAll)
                    } else {
                        i18n::t(TextKey::EdgeHideAll)
                    },
                    theme::text_muted(),
                    |s| s.toggle_all_edge_labels(),
                )),
        )
        .child(hairline())
        .child({
            let eid_auto = edge.id.clone();
            let active_hex = edge.color_hex.as_deref().unwrap_or("");
            let trace_on = s.reader_prefs.enable_trace_colors;

            let mut swatch_rows = col().w_full().gap(6.0);
            let mut hdr_row = row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(subtle_tag("TRACE COLOR", theme::text_muted()))
                .child(el().grow(1.0));
            if let Some(cat) = edge.color_category.as_deref().filter(|c| !c.is_empty()) {
                hdr_row = hdr_row.child(subtle_tag(cat, theme::text_secondary()));
            }
            hdr_row = hdr_row.child(ghost_btn(
                "btn_inspector_toggle_trace_colors",
                if trace_on { "On" } else { "Off" },
                if trace_on {
                    theme::accent_claude()
                } else {
                    theme::text_muted()
                },
                |s| s.toggle_trace_colors(),
            ));
            swatch_rows = swatch_rows.child(hdr_row);

            let mut r1 = row().w_full().gap(5.0).center_y();
            let auto_selected = edge.color_hex.is_none();
            r1 = r1.child(
                el()
                    .key("btn_edge_color_auto")
                    .px(7.0)
                    .py(4.0)
                    .radius(5.0)
                    .fill(if auto_selected {
                        theme::bg_control()
                    } else {
                        theme::bg_input()
                    })
                    .hover(theme::hover_subtle())
                    .border(
                        if auto_selected { 1.5 } else { 1.0 },
                        if auto_selected {
                            theme::border_strong()
                        } else {
                            theme::border_subtle()
                        },
                    )
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.set_edge_color(&eid_auto, None, None);
                    })
                    .child(
                        text("Auto")
                            .font(theme::BADGE_TEXT)
                            .color(theme::text_secondary()),
                    ),
            );
            for (preset_id, preset_label, preset_hex) in studynodes_graph::model::TRACE_COLOR_PRESETS.iter().take(4) {
                let eid_p = edge.id.clone();
                let col_v = theme::hex(preset_hex);
                let is_sel = active_hex.eq_ignore_ascii_case(preset_hex);
                r1 = r1.child(
                    row()
                        .key(format!("btn_edge_color_{}", preset_id))
                        .grow(1.0)
                        .min_w(0.0)
                        .px(6.0)
                        .py(4.0)
                        .gap(4.0)
                        .center_y()
                        .radius(5.0)
                        .fill(if is_sel {
                            col_v.alpha(0.18)
                        } else {
                            theme::bg_input()
                        })
                        .hover(theme::hover_subtle())
                        .border(
                            if is_sel { 1.5 } else { 1.0 },
                            if is_sel { col_v } else { theme::border_subtle() },
                        )
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.set_edge_color(&eid_p, Some(preset_hex), Some(preset_label));
                        })
                        .child(el().size(7.0, 7.0).radius(3.5).fill(col_v))
                        .child(
                            text(*preset_label)
                                .font(theme::BADGE_TEXT)
                                .color(theme::text_primary())
                                .truncate(),
                        ),
                );
            }
            swatch_rows = swatch_rows.child(r1);

            let mut r2 = row().w_full().gap(5.0).center_y();
            for (preset_id, preset_label, preset_hex) in studynodes_graph::model::TRACE_COLOR_PRESETS.iter().skip(4) {
                let eid_p = edge.id.clone();
                let col_v = theme::hex(preset_hex);
                let is_sel = active_hex.eq_ignore_ascii_case(preset_hex);
                r2 = r2.child(
                    row()
                        .key(format!("btn_edge_color_{}", preset_id))
                        .grow(1.0)
                        .min_w(0.0)
                        .px(6.0)
                        .py(4.0)
                        .gap(4.0)
                        .center_y()
                        .radius(5.0)
                        .fill(if is_sel {
                            col_v.alpha(0.18)
                        } else {
                            theme::bg_input()
                        })
                        .hover(theme::hover_subtle())
                        .border(
                            if is_sel { 1.5 } else { 1.0 },
                            if is_sel { col_v } else { theme::border_subtle() },
                        )
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.set_edge_color(&eid_p, Some(preset_hex), Some(preset_label));
                        })
                        .child(el().size(7.0, 7.0).radius(3.5).fill(col_v))
                        .child(
                            text(*preset_label)
                                .font(theme::BADGE_TEXT)
                                .color(theme::text_primary())
                                .truncate(),
                        ),
                );
            }
            swatch_rows.child(r2)
        })
        .child(hairline())
        .child(
            col()
                .w_full()
                .gap(6.0)
                .child(subtle_tag(
                    i18n::t(TextKey::EdgeLabelField),
                    theme::text_muted(),
                ))
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
                .child(subtle_tag(
                    i18n::t(TextKey::EdgeNoteField),
                    theme::text_muted(),
                ))
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
        .child(ghost_btn(
            "btn_delete_edge",
            i18n::t(TextKey::EdgeDeleteBtn),
            theme::accent_danger().alpha(0.85),
            move |s| {
                s.graph.remove_edge(&edge_id_for_delete);
                s.selected_edge = None;
                s.notify_toast(i18n::t(TextKey::EdgeDeleteBtn));
            },
        ))
}

fn render_readonly_doc_panel(s: &AppState, node: &Node, panel_width: f32) -> El {
    let doc_format = s.node_doc_format(&node.id);
    let fmt_badge = doc_reader::format_badge(doc_format);
    let has_pdf = s.active_pdf.as_ref().is_some_and(|p| p.node_id == node.id);
    let is_summarizing = s.llm_pending_nodes.contains(&node.id);
    let nid_ext = node.id.clone();
    let nid_sum = node.id.clone();
    let is_img_kind = node.kind == NodeKind::Image;
    let frameless_img = node.meta.image_frameless;

    // Minimal 4-control editorial header (key("reader_readonly_toolbar") preserved)
    let mut small_toolbar = row()
        .key("reader_readonly_toolbar")
        .w_full()
        .center_y()
        .gap(6.0)
        .child(icons::ph(
            icons::kind_icon(node.kind),
            15.0,
            theme::kind_accent(node.kind),
        ))
        .child(subtle_tag(fmt_badge, theme::text_muted()))
        .child(el().grow(1.0));
    if is_img_kind {
        let nid_frame = node.id.clone();
        small_toolbar = small_toolbar.child(ghost_btn(
            "btn_toggle_image_frame",
            if frameless_img { "Framed" } else { "Frameless" },
            if frameless_img {
                theme::accent_claude()
            } else {
                theme::text_secondary()
            },
            move |s| {
                s.toggle_image_frameless(&nid_frame);
            },
        ));
    }
    let small_toolbar = small_toolbar
        .child(ghost_btn(
            "btn_toggle_readonly",
            i18n::t(TextKey::ReaderEdit),
            theme::accent_claude(),
            |s| {
                s.set_reader_read_only(false);
            },
        ))
        .child(popout_panel_btn())
        .child(ghost_icon_btn(
            "btn_inspector_more",
            &icons::DOTS_THREE,
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
        render_pdf_reader_panel(s, pdf, panel_width, None)
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
                let img_h = theme::scaled_image_height(img, md_w, 150.0, 260.0, 190.0);
                preview_col = preview_col.child(theme::render_fitted_image_box(
                    "integrated_image_preview",
                    img,
                    img_h,
                    6.0,
                ));
            }
        }

        if node.kind == NodeKind::Image {
            preview_col = preview_col
                .child(hairline())
                .child(render_image_vision_section(s, node));
            if !node.content.trim().is_empty() {
                let md_src = doc_reader::wrap_for_preview(&node.content, doc_format);
                let md_el = render_memoized_markdown(&node.id, md_src, doc_format, md_w, true);
                preview_col =
                    preview_col.child(el().key("image_caption_markdown").w_full().child(md_el));
            }
            preview_col
        } else {
            let is_empty = node.content.trim().is_empty();
            let md_src = if is_empty {
                i18n::t(TextKey::ReaderEmptyReadonlyMd).to_string()
            } else {
                doc_reader::wrap_for_preview(&node.content, doc_format)
            };

            let md_el = render_memoized_markdown(&node.id, md_src, doc_format, md_w, true);

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
                    i18n::t(TextKey::ReaderCreateExcerpt),
                    theme::text_primary(),
                    |s| {
                        s.inspector_more_menu_open = false;
                        s.create_excerpt_from_selected_doc(None);
                    },
                ))
                .child(more_menu_item(
                    "btn_readonly_summarize",
                    if is_summarizing {
                        i18n::t(TextKey::ReaderSummarizing)
                    } else {
                        i18n::t(TextKey::ReaderSummarizeWithAi)
                    },
                    theme::accent_claude(),
                    move |s| {
                        s.inspector_more_menu_open = false;
                        s.explain_node_ai(&nid_sum);
                    },
                ))
                .child(more_menu_item(
                    "btn_open_external_reader",
                    i18n::t(TextKey::ReaderOpenExternal),
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

    if let Some(callout) = render_ai_summary_callout(s, node) {
        sheet = sheet.child(callout);
    }

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
                        i18n::t(TextKey::ReaderTabPages),
                        s.doc_view_tab == DocViewTab::Pages || s.doc_view_tab == DocViewTab::Source,
                    )
                    .on_press(|s: &mut AppState| {
                        s.doc_view_tab = DocViewTab::Pages;
                    }),
                )
                .child(
                    underline_tab(
                        "tab_doc_preview",
                        i18n::t(TextKey::ReaderTabText),
                        s.doc_view_tab == DocViewTab::Preview,
                    )
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
                ("mast_new", i18n::t(TextKey::ReaderMasteryNew), "new"),
                ("mast_again", i18n::t(TextKey::ReaderMasteryAgain), "again"),
                ("mast_good", i18n::t(TextKey::ReaderMasteryGood), "good"),
                ("mast_easy", i18n::t(TextKey::ReaderMasteryEasy), "easy"),
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
                .child(subtle_tag(
                    i18n::t(TextKey::FlashcardAnswerLabel),
                    theme::text_muted(),
                ))
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
                            i18n::t(TextKey::FlashcardAnswerPlaceholder),
                            theme::text_muted(),
                        )
                        .on_edit(|s: &mut AppState| &mut s.inspector_answer_edit)
                        .on_input_change(|s: &mut AppState, _| {
                            s.sync_from_inspector_edits();
                        }),
                )
                .child(subtle_tag(
                    i18n::t(TextKey::FlashcardMasteryLabel),
                    theme::text_muted(),
                ))
                .child(mastery_row)
        }
        NodeKind::Formula => el(),
        NodeKind::Section => {
            let framed = node.meta.section_framed;
            let id_a = nid.clone();
            let id_b = nid.clone();
            let id_shot = nid.clone();
            let sec_slug: String = node
                .title
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c.to_ascii_lowercase()
                    } else {
                        '_'
                    }
                })
                .take(40)
                .collect();
            let shot_name = if sec_slug.trim_matches('_').is_empty() {
                "section_scheme.png".to_string()
            } else {
                format!("{}_scheme.png", sec_slug.trim_matches('_'))
            };
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
                            underline_tab(
                                "insp_sec_framed",
                                i18n::t(TextKey::SectionFramedTab),
                                framed,
                            )
                            .on_press(move |s: &mut AppState| {
                                if let Some(sec) = s.graph.get_node(&id_a) {
                                    if !sec.meta.section_framed {
                                        s.toggle_section_framed(&id_a);
                                    }
                                }
                            }),
                        )
                        .child(
                            underline_tab(
                                "insp_sec_banner",
                                i18n::t(TextKey::SectionBannerTab),
                                !framed,
                            )
                            .on_press(move |s: &mut AppState| {
                                if let Some(sec) = s.graph.get_node(&id_b) {
                                    if sec.meta.section_framed {
                                        s.toggle_section_framed(&id_b);
                                    }
                                }
                            }),
                        ),
                )
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .gap(8.0)
                        .child(
                            text(i18n::fmt_canvas_bounds(sz.0 as i32, sz.1 as i32))
                                .font(theme::STATUS_TEXT)
                                .color(theme::text_muted()),
                        )
                        .child(el().grow(1.0))
                        .child(
                            el()
                                .key("btn_inspector_section_screenshot")
                                .size(26.0, 26.0)
                                .radius(6.0)
                                .fill(theme::bg_control())
                                .border(1.0, theme::border_subtle())
                                .hover(theme::hover_subtle())
                                .press(theme::press_subtle())
                                .center()
                                .cursor(Cursor::Pointer)
                                .on_press_tx(move |_s: &mut AppState, tx: &mut Tx| {
                                    let nid = id_shot.clone();
                                    let opts = FileDialogOptions {
                                        title: i18n::t(TextKey::SectionScreenshotBtn).to_string(),
                                        file_name: Some(shot_name.clone()),
                                        ..Default::default()
                                    }
                                    .filter("PNG Image", &["png"]);
                                    tx.save_file(opts, move |s: &mut AppState, res, _| {
                                        if let Ok(Some(dest)) = res {
                                            let _ =
                                                s.export_scheme_screenshot(Some(&nid), Some(dest));
                                        }
                                    });
                                })
                                .child(icons::ph(&icons::CAMERA, 13.5, theme::text_secondary())),
                        ),
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
                    format!("{} · p.{}", doc_title, page_opt.unwrap_or(0) + 1)
                }
                _ => i18n::t(TextKey::ExcerptNoSource).into(),
            };
            el()
                .key("btn_inspector_excerpt_jump")
                .px(7.0)
                .py(4.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.jump_to_excerpt_source(&nid);
                })
                .child(
                    row()
                        .center_y()
                        .gap(5.0)
                        .child(icons::ph(&icons::FILE_TEXT, 12.5, theme::accent_claude()))
                        .child(
                            text(&jump_label)
                                .font(theme::STATUS_TEXT)
                                .color(theme::accent_claude()),
                        )
                        .child(icons::ph(
                            &icons::ARROW_SQUARE_OUT,
                            11.5,
                            theme::accent_claude(),
                        )),
                )
        }
        NodeKind::Sequence => ghost_btn(
            "btn_inspector_seq_next",
            i18n::t(TextKey::SequenceSpawnNext),
            theme::accent_claude(),
            move |s| {
                s.add_next_sequence_step(&nid);
            },
        ),
        NodeKind::Checklist => {
            let tasks = doc_reader::parse_markdown_tasks(&node.content);
            let done = tasks.iter().filter(|(c, _)| *c).count();
            text(i18n::fmt_tasks_progress(done, tasks.len()))
                .font(theme::STATUS_TEXT)
                .color(theme::text_muted())
        }
        NodeKind::Text => {
            let cur_font = node.meta.text_font.as_deref().unwrap_or("sans");
            let cur_size = node.meta.text_size.unwrap_or(18.0).clamp(10.0, 96.0);
            let mut font_row = row().w_full().gap(8.0);
            for (key, label, val) in [
                ("btn_text_font_sans", "Sans", "sans"),
                ("btn_text_font_serif", "Serif", "serif"),
                ("btn_text_font_mono", "Mono", "mono"),
                ("btn_text_font_display", "Display", "display"),
                ("btn_text_font_hand", "Hand", "hand"),
            ] {
                let id_c = nid.clone();
                font_row = font_row.child(
                    underline_tab(key, label, cur_font.eq_ignore_ascii_case(val)).on_press(
                        move |s: &mut AppState| {
                            s.set_text_node_font(&id_c, val);
                        },
                    ),
                );
            }

            let id_dec = nid.clone();
            let id_inc = nid.clone();
            let mut size_row = row()
                .w_full()
                .center_y()
                .gap(6.0)
                .child(ghost_icon_btn(
                    "btn_text_size_dec",
                    &icons::MINUS,
                    theme::text_secondary(),
                    false,
                    move |s| {
                        s.set_text_node_size(&id_dec, cur_size - 2.0);
                    },
                ))
                .child(
                    text(format!("{} pt", cur_size.round() as i32))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_primary()),
                )
                .child(ghost_icon_btn(
                    "btn_text_size_inc",
                    &icons::PLUS,
                    theme::text_secondary(),
                    false,
                    move |s| {
                        s.set_text_node_size(&id_inc, cur_size + 2.0);
                    },
                ))
                .child(el().grow(1.0));
            for (k, sz) in [
                ("btn_text_size_14", 14.0),
                ("btn_text_size_18", 18.0),
                ("btn_text_size_24", 24.0),
                ("btn_text_size_36", 36.0),
                ("btn_text_size_48", 48.0),
            ] {
                let id_p = nid.clone();
                let active = (cur_size - sz).abs() < 0.5;
                size_row = size_row.child(underline_tab(k, &format!("{}", sz as i32), active).on_press(
                    move |s: &mut AppState| {
                        s.set_text_node_size(&id_p, sz);
                    },
                ));
            }

            let id_fit = nid.clone();
            let id_auto = nid.clone();
            let auto_on = node.meta.text_auto_fit;
            let fit_row = row()
                .w_full()
                .center_y()
                .gap(6.0)
                .child(
                    el()
                        .key("btn_text_fit_now")
                        .px(9.0)
                        .py(5.0)
                        .radius(6.0)
                        .fill(theme::bg_control())
                        .border(1.0, theme::border_subtle())
                        .hover(theme::hover_subtle())
                        .press(theme::press_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.fit_text_node_to_content(&id_fit);
                        })
                        .child(
                            row()
                                .center_y()
                                .gap(5.0)
                                .child(icons::ph(
                                    &icons::FRAME_CORNERS,
                                    12.5,
                                    theme::accent_claude(),
                                ))
                                .child(
                                    text(i18n::t(TextKey::TextFitToContent))
                                        .font(theme::STATUS_TEXT)
                                        .color(theme::text_primary()),
                                ),
                        ),
                )
                .child(
                    el()
                        .key("btn_text_auto_fit")
                        .px(9.0)
                        .py(5.0)
                        .radius(6.0)
                        .fill(if auto_on {
                            theme::accent_claude().alpha(0.16)
                        } else {
                            theme::bg_control()
                        })
                        .border(
                            1.0,
                            if auto_on {
                                theme::accent_claude().alpha(0.6)
                            } else {
                                theme::border_subtle()
                            },
                        )
                        .hover(theme::hover_subtle())
                        .press(theme::press_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.toggle_text_auto_fit(&id_auto);
                        })
                        .child(
                            text(if auto_on {
                                i18n::t(TextKey::TextAutoFitOn)
                            } else {
                                i18n::t(TextKey::TextAutoFitOff)
                            })
                            .font(theme::STATUS_TEXT)
                            .color(if auto_on {
                                theme::accent_claude()
                            } else {
                                theme::text_secondary()
                            }),
                        ),
                );

            col()
                .w_full()
                .gap(10.0)
                .child(hairline())
                .child(subtle_tag("FONT", theme::text_muted()))
                .child(font_row)
                .child(subtle_tag("SIZE", theme::text_muted()))
                .child(size_row)
                .child(fit_row)
        }
        NodeKind::Quiz => {
            let mode_id_reg = node.id.clone();
            let mode_id_pop = node.id.clone();
            let diff_id_easy = node.id.clone();
            let diff_id_hard = node.id.clone();
            let insp_start_id = node.id.clone();
            let is_popout = node.meta.quiz_mode == QuizDisplayMode::Popout;
            let is_hard = node.meta.quiz_difficulty == QuizDifficulty::Hard;
            let total_qs = node.meta.quiz_questions.len();
            let attempt_status = s
                .quiz_sessions
                .get(&node.id)
                .map(|st| st.status)
                .unwrap_or(QuizAttemptStatus::NotStarted);

            let mut deck_col = col()
                .w_full()
                .gap(10.0)
                .child(
                    row()
                        .w_full()
                        .gap(16.0)
                        .child(
                            col()
                                .gap(4.0)
                                .child(subtle_tag("MODE", theme::text_muted()))
                                .child(
                                    row()
                                        .gap(8.0)
                                        .child(
                                            underline_tab("btn_quiz_mode_regular", "Regular", !is_popout)
                                                .on_press(move |s: &mut AppState| {
                                                    s.set_quiz_mode(&mode_id_reg, QuizDisplayMode::Regular);
                                                }),
                                        )
                                        .child(
                                            underline_tab("btn_quiz_mode_popout", "Popout", is_popout)
                                                .on_press(move |s: &mut AppState| {
                                                    s.set_quiz_mode(&mode_id_pop, QuizDisplayMode::Popout);
                                                }),
                                        ),
                                ),
                        )
                        .child(
                            col()
                                .gap(4.0)
                                .child(subtle_tag("DIFFICULTY", theme::text_muted()))
                                .child(
                                    row()
                                        .gap(8.0)
                                        .child(
                                            underline_tab("btn_quiz_diff_easy", "Easy", !is_hard).on_press(
                                                move |s: &mut AppState| {
                                                    s.set_quiz_difficulty(&diff_id_easy, QuizDifficulty::Easy);
                                                },
                                            ),
                                        )
                                        .child(
                                            underline_tab("btn_quiz_diff_hard", "Hard", is_hard).on_press(
                                                move |s: &mut AppState| {
                                                    s.set_quiz_difficulty(&diff_id_hard, QuizDifficulty::Hard);
                                                },
                                            ),
                                        ),
                                ),
                        ),
                )
                .child(
                    text(if is_hard {
                        "Hard locks exit until completed or given up."
                    } else {
                        "Easy lets you pause to the scheme and resume."
                    })
                    .font(theme::BADGE_TEXT)
                    .color(theme::text_muted()),
                );

            if is_popout {
                let danger = theme::accent_danger();
                let accent = theme::kind_accent(NodeKind::Quiz);
                let ok_col = theme::hex("#6AB07A");
                let (lbl, ico, fg, retry) = match attempt_status {
                    QuizAttemptStatus::Failed => ("Try Again", &icons::ARROW_CLOCKWISE, danger, true),
                    QuizAttemptStatus::InProgress => ("Resume Quiz", &icons::PLAY, accent, false),
                    QuizAttemptStatus::Completed => ("Review Quiz", &icons::CHECK, ok_col, false),
                    QuizAttemptStatus::NotStarted => ("Start Quiz", &icons::PLAY, accent, false),
                };
                let mut action_row = row().w_full().items_center().gap(6.0).child(
                    el()
                        .key("btn_inspector_start_quiz")
                        .grow(1.0)
                        .px(10.0)
                        .py(7.0)
                        .radius(6.0)
                        .fill(if total_qs == 0 {
                            theme::bg_control()
                        } else {
                            fg.alpha(0.16)
                        })
                        .hover(if total_qs == 0 {
                            theme::bg_control()
                        } else {
                            fg.alpha(0.24)
                        })
                        .border(
                            1.0,
                            if total_qs == 0 {
                                theme::border_subtle()
                            } else {
                                fg.alpha(0.50)
                            },
                        )
                        .cursor(if total_qs == 0 {
                            Cursor::Default
                        } else {
                            Cursor::Pointer
                        })
                        .center()
                        .on_press(move |s: &mut AppState| {
                            if retry {
                                s.quiz_retry_popout(&insp_start_id);
                            } else {
                                s.start_or_resume_quiz_popout(&insp_start_id);
                            }
                        })
                        .child(
                            row()
                                .items_center()
                                .gap(6.0)
                                .child(icons::ph(
                                    ico,
                                    13.0,
                                    if total_qs == 0 {
                                        theme::text_muted()
                                    } else {
                                        fg
                                    },
                                ))
                                .child(text(lbl).font(theme::CONTROL_TEXT).color(if total_qs == 0 {
                                    theme::text_muted()
                                } else {
                                    theme::text_primary()
                                })),
                        ),
                );
                if !is_hard && attempt_status != QuizAttemptStatus::NotStarted {
                    let reset_id = node.id.clone();
                    action_row = action_row.child(ghost_icon_btn(
                        "btn_inspector_reset_quiz",
                        &icons::ARROW_CLOCKWISE,
                        theme::text_secondary(),
                        false,
                        move |s: &mut AppState| {
                            s.quiz_reset_session(&reset_id);
                        },
                    ));
                }
                deck_col = deck_col.child(action_row);
            }

            deck_col = deck_col
                .child(hairline())
                .child(subtle_tag(
                    &format!("QUESTIONS ({})", total_qs),
                    theme::text_muted(),
                ));

            let thumb_w = (s.inspector_width.clamp(300.0, 860.0) - 76.0).max(140.0);
            for (qi, q) in node.meta.quiz_questions.iter().enumerate() {
                let del_nid = nid.clone();
                let hint_nid = nid.clone();
                let q_pick_nid = nid.clone();
                let q_clear_nid = nid.clone();
                let ans_pick_nid = nid.clone();
                let ans_clear_nid = nid.clone();
                let q_label = if q.question.trim().is_empty() && q.question_image_hash.is_some() {
                    "[Image Question]"
                } else {
                    q.question.as_str()
                };
                let mut q_card = col()
                    .w_full()
                    .padding(8.0)
                    .gap(5.0)
                    .radius(6.0)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_subtle())
                    .child(
                        row()
                            .w_full()
                            .center_y()
                            .gap(4.0)
                            .child(subtle_tag(&format!("Q{}", qi + 1), theme::kind_accent(NodeKind::Quiz)))
                            .child(
                                el().grow(1.0).min_w(0.0).child(
                                    text(q_label)
                                        .font(theme::STATUS_TEXT)
                                        .color(theme::text_primary())
                                        .w_full()
                                        .wrap(),
                                ),
                            )
                            .child(quiz_image_pick_btn(
                                format!("quiz_q_img_pick_{}", qi),
                                "Q",
                                "Choose Question Image",
                                q.question_image_hash.is_some(),
                                move |s, path| {
                                    s.quiz_set_question_image_from_path(&q_pick_nid, qi, false, path);
                                },
                            ))
                            .child(quiz_image_pick_btn(
                                format!("quiz_ans_img_pick_{}", qi),
                                "A",
                                "Choose Answer Image",
                                q.answer_image_hash.is_some(),
                                move |s, path| {
                                    s.quiz_set_question_image_from_path(&ans_pick_nid, qi, true, path);
                                },
                            ))
                            .child(ghost_icon_btn(
                                format!("quiz_add_hint_{}", qi),
                                &icons::LIGHTBULB,
                                theme::accent_claude(),
                                false,
                                move |s| {
                                    let h = s.quiz_hint_edit.text.trim().to_string();
                                    if !h.is_empty() {
                                        s.quiz_append_hint(&hint_nid, qi, &h);
                                        s.quiz_hint_edit = Default::default();
                                    }
                                },
                            ))
                            .child(ghost_icon_btn(
                                format!("quiz_del_q_{}", qi),
                                &icons::X,
                                theme::accent_danger(),
                                false,
                                move |s| {
                                    s.quiz_delete_question(&del_nid, qi);
                                },
                            )),
                    );
                if !q.hints.is_empty() {
                    q_card = q_card.child(
                        text(format!("Hints: {}", q.hints.join(" · ")))
                            .font(theme::BADGE_TEXT)
                            .color(theme::text_muted())
                            .w_full()
                            .wrap(),
                    );
                }
                if q.question_image_hash.is_some() {
                    let clr_nid = q_clear_nid.clone();
                    let preview_el = if let Some(q_img) =
                        s.quiz_image_for_hash(q.question_image_hash.as_deref())
                    {
                        let h = theme::scaled_image_height(q_img, thumb_w, 44.0, 96.0, 64.0);
                        theme::render_fitted_image_box(
                            format!("quiz_insp_q_img_{}", qi),
                            q_img,
                            h,
                            5.0,
                        )
                    } else {
                        text("Question image attached")
                            .font(theme::BADGE_TEXT)
                            .color(theme::text_muted())
                    };
                    q_card = q_card.child(
                        row()
                            .w_full()
                            .center_y()
                            .gap(6.0)
                            .child(el().grow(1.0).min_w(0.0).child(preview_el))
                            .child(ghost_icon_btn(
                                format!("quiz_q_img_clear_{}", qi),
                                &icons::X,
                                theme::accent_danger(),
                                false,
                                move |s| {
                                    s.quiz_clear_question_image(&clr_nid, qi, false);
                                },
                            )),
                    );
                }
                if q.answer_image_hash.is_some() {
                    let clr_nid = ans_clear_nid.clone();
                    let preview_el = if let Some(a_img) =
                        s.quiz_image_for_hash(q.answer_image_hash.as_deref())
                    {
                        let h = theme::scaled_image_height(a_img, thumb_w, 44.0, 96.0, 64.0);
                        theme::render_fitted_image_box(
                            format!("quiz_insp_ans_img_{}", qi),
                            a_img,
                            h,
                            5.0,
                        )
                    } else {
                        text("Answer image attached")
                            .font(theme::BADGE_TEXT)
                            .color(theme::text_muted())
                    };
                    q_card = q_card.child(
                        row()
                            .w_full()
                            .center_y()
                            .gap(6.0)
                            .child(el().grow(1.0).min_w(0.0).child(preview_el))
                            .child(ghost_icon_btn(
                                format!("quiz_ans_img_clear_{}", qi),
                                &icons::X,
                                theme::accent_danger(),
                                false,
                                move |s| {
                                    s.quiz_clear_question_image(&clr_nid, qi, true);
                                },
                            )),
                    );
                }
                deck_col = deck_col.child(q_card);
            }

            let add_nid = nid.clone();
            let sub_q_nid = nid.clone();
            let sub_ans_nid = nid.clone();
            let mut correct_picker = row().w_full().center_y().gap(6.0).child(
                text("Correct:")
                    .font(theme::BADGE_TEXT)
                    .color(theme::text_muted()),
            );
            for (oi, letter) in ["A", "B", "C", "D"].iter().enumerate() {
                let active = s.quiz_correct_idx == oi;
                correct_picker = correct_picker.child(
                    el()
                        .key(format!("quiz_new_correct_{}", oi))
                        .px(7.0)
                        .py(3.0)
                        .radius(4.0)
                        .fill(if active {
                            theme::accent_claude().alpha(0.20)
                        } else {
                            theme::bg_control()
                        })
                        .border(
                            1.0,
                            if active {
                                theme::accent_claude()
                            } else {
                                theme::border_subtle()
                            },
                        )
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.quiz_correct_idx = oi;
                        })
                        .child(
                            text(*letter)
                                .font(theme::BADGE_TEXT)
                                .color(if active {
                                    theme::accent_claude()
                                } else {
                                    theme::text_secondary()
                                }),
                        ),
                );
            }

            deck_col = deck_col
                .child(hairline())
                .child(subtle_tag("ADD QUESTION", theme::text_muted()))
                .child(quiz_draft_field_header(
                    "Question",
                    "btn_quiz_draft_q_img",
                    "btn_quiz_draft_q_img_clear",
                    false,
                    s.quiz_q_img_hash.is_some(),
                    "Choose Question Image",
                ))
                .child(
                    el()
                        .w_full()
                        .min_h(34.0)
                        .px(8.0)
                        .py(6.0)
                        .radius(6.0)
                        .fill(theme::bg_input())
                        .border(1.0, theme::border_subtle())
                        .role("textbox")
                        .child(
                            text_input(&s.quiz_q_edit, theme::BODY_TEXT)
                                .key("quiz_q_edit")
                                .label("Quiz question prompt")
                                .w_full()
                                .color(theme::text_primary())
                                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                                .placeholder("Question prompt...", theme::text_muted())
                                .on_edit(|s: &mut AppState| &mut s.quiz_q_edit)
                                .on_submit(move |s: &mut AppState, tx| {
                                    if !s.quiz_add_question_from_inspector(&sub_q_nid) {
                                        tx.focus("quiz_q_edit", false);
                                    }
                                }),
                        ),
                );
            if let Some(q_img) = s.quiz_image_for_hash(s.quiz_q_img_hash.as_deref()) {
                let h = theme::scaled_image_height(q_img, thumb_w, 40.0, 64.0, 48.0);
                deck_col = deck_col.child(theme::render_fitted_image_box(
                    "quiz_draft_q_img_preview",
                    q_img,
                    h,
                    5.0,
                ));
            }
            deck_col = deck_col
                .child(
                    text("Options")
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_muted()),
                )
                .child(
                    el()
                        .w_full()
                        .min_h(48.0)
                        .px(8.0)
                        .py(6.0)
                        .radius(6.0)
                        .fill(theme::bg_input())
                        .border(1.0, theme::border_subtle())
                        .role("textbox")
                        .child(
                            text_input(&s.quiz_opts_edit, theme::STATUS_TEXT)
                                .key("quiz_opts_edit")
                                .label("Quiz answer choices")
                                .w_full()
                                .color(theme::text_primary())
                                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                                .placeholder("Choices separated by | or newline (Option A | Option B | ...)", theme::text_muted())
                                .on_edit(|s: &mut AppState| &mut s.quiz_opts_edit)
                                .on_submit(|s: &mut AppState, _| {
                                    s.quiz_opts_edit.insert("\n");
                                }),
                        ),
                )
                .child(correct_picker)
                .child(quiz_draft_field_header(
                    "Explanation",
                    "btn_quiz_draft_ans_img",
                    "btn_quiz_draft_ans_img_clear",
                    true,
                    s.quiz_ans_img_hash.is_some(),
                    "Choose Answer Image",
                ))
                .child(
                    el()
                        .w_full()
                        .min_h(34.0)
                        .px(8.0)
                        .py(6.0)
                        .radius(6.0)
                        .fill(theme::bg_input())
                        .border(1.0, theme::border_subtle())
                        .role("textbox")
                        .child(
                            text_input(&s.quiz_ans_edit, theme::STATUS_TEXT)
                                .key("quiz_ans_edit")
                                .label("Quiz explanation")
                                .w_full()
                                .color(theme::text_primary())
                                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                                .placeholder("Explanation / answer...", theme::text_muted())
                                .on_edit(|s: &mut AppState| &mut s.quiz_ans_edit)
                                .on_submit(move |s: &mut AppState, tx| {
                                    if !s.quiz_add_question_from_inspector(&sub_ans_nid) {
                                        tx.focus("quiz_q_edit", false);
                                    }
                                }),
                        ),
                );
            if let Some(a_img) = s.quiz_image_for_hash(s.quiz_ans_img_hash.as_deref()) {
                let h = theme::scaled_image_height(a_img, thumb_w, 40.0, 64.0, 48.0);
                deck_col = deck_col.child(theme::render_fitted_image_box(
                    "quiz_draft_ans_img_preview",
                    a_img,
                    h,
                    5.0,
                ));
            }
            deck_col
                .child(
                    text("Hints")
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_muted()),
                )
                .child(
                    el()
                        .w_full()
                        .min_h(34.0)
                        .px(8.0)
                        .py(6.0)
                        .radius(6.0)
                        .fill(theme::bg_input())
                        .border(1.0, theme::border_subtle())
                        .role("textbox")
                        .child(
                            text_input(&s.quiz_hint_edit, theme::STATUS_TEXT)
                                .key("quiz_hint_edit")
                                .label("Quiz optional hints")
                                .w_full()
                                .color(theme::text_primary())
                                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                                .placeholder("Optional hints (one per line)...", theme::text_muted())
                                .on_edit(|s: &mut AppState| &mut s.quiz_hint_edit)
                                .on_submit(|s: &mut AppState, _| {
                                    s.quiz_hint_edit.insert("\n");
                                }),
                        ),
                )
                .child(
                    el()
                        .key("btn_quiz_add_q")
                        .px(10.0)
                        .py(6.0)
                        .radius(6.0)
                        .fill(theme::accent_claude().alpha(0.18))
                        .border(1.0, theme::accent_claude().alpha(0.60))
                        .hover(theme::accent_claude().alpha(0.28))
                        .press(theme::press_subtle())
                        .center()
                        .cursor(Cursor::Pointer)
                        .on_press_tx(move |s: &mut AppState, tx| {
                            if !s.quiz_add_question_from_inspector(&add_nid) {
                                tx.focus("quiz_q_edit", false);
                            }
                        })
                        .child(
                            row()
                                .center_y()
                                .gap(6.0)
                                .child(icons::ph(&icons::PLUS, 13.0, theme::accent_claude()))
                                .child(
                                    text("Add Question")
                                        .font(theme::STATUS_TEXT)
                                        .color(theme::accent_claude()),
                                ),
                        ),
                )
        }
        _ => el(),
    }
}

fn quiz_image_pick_btn(
    key: String,
    label: &'static str,
    dialog_title: &'static str,
    active: bool,
    on_pick: impl Fn(&mut AppState, &std::path::PathBuf) + Clone + 'static,
) -> El {
    let accent = theme::kind_accent(NodeKind::Quiz);
    el()
        .key(key)
        .px(5.0)
        .py(2.0)
        .radius(4.0)
        .fill(if active {
            accent.alpha(0.14)
        } else {
            theme::bg_control()
        })
        .hover(theme::hover_subtle())
        .border(
            1.0,
            if active {
                accent.alpha(0.45)
            } else {
                theme::border_subtle()
            },
        )
        .cursor(Cursor::Pointer)
        .on_press_tx(move |_: &mut AppState, tx: &mut Tx| {
            let cb = on_pick.clone();
            let opts = FileDialogOptions {
                title: dialog_title.into(),
                ..Default::default()
            }
            .filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp", "svg"]);
            tx.open_file(opts, move |s: &mut AppState, res, _| {
                if let Ok(Some(path)) = res {
                    cb(s, &path);
                }
            });
        })
        .child(
            row()
                .center_y()
                .gap(4.0)
                .child(icons::ph(
                    &icons::IMAGE,
                    11.0,
                    if active { accent } else { theme::text_secondary() },
                ))
                .child(
                    text(label)
                        .font(theme::BADGE_TEXT)
                        .color(if active {
                            theme::text_primary()
                        } else {
                            theme::text_secondary()
                        }),
                ),
        )
}

fn quiz_draft_field_header(
    title: &'static str,
    pick_key: &'static str,
    clear_key: &'static str,
    is_answer: bool,
    has_img: bool,
    dialog_title: &'static str,
) -> El {
    let mut header = row()
        .w_full()
        .center_y()
        .justify_between()
        .gap(6.0)
        .child(
            text(title)
                .font(theme::BADGE_TEXT)
                .color(theme::text_muted()),
        );
    let mut btns = row().center_y().gap(4.0).child(quiz_image_pick_btn(
        pick_key.to_string(),
        if has_img { "Replace" } else { "Image" },
        dialog_title,
        has_img,
        move |s, path| {
            s.quiz_set_draft_image_from_path(is_answer, path);
        },
    ));
    if has_img {
        btns = btns.child(ghost_icon_btn(
            clear_key,
            &icons::X,
            theme::accent_danger(),
            false,
            move |s| {
                s.quiz_clear_draft_image(is_answer);
            },
        ));
    }
    header = header.child(btns);
    header
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
                    text(if is_busy {
                        i18n::t(TextKey::ReaderOcrExtracting)
                    } else {
                        i18n::t(TextKey::ReaderOcrExtractText)
                    })
                    .font(theme::BADGE_TEXT)
                    .color(theme::text_inverted()),
                ),
        )
        .child(ghost_btn(
            "btn_img_ocr_formula",
            i18n::t(TextKey::ReaderOcrFormula),
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
        action_row = action_row.child(
            el()
                .key("btn_download_neural_models")
                .px(7.0)
                .py(4.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.download_neural_ocr_models();
                })
                .child(if s.vision_downloading_models {
                    icons::ph(&icons::DOTS_THREE, 13.0, theme::accent_claude())
                } else {
                    row()
                        .center_y()
                        .gap(4.0)
                        .child(icons::ph(&icons::ARROW_DOWN, 12.0, theme::accent_claude()))
                        .child(
                            text("Models")
                                .font(theme::STATUS_TEXT)
                                .color(theme::accent_claude()),
                        )
                }),
        );
    }

    action_row = action_row.child(ghost_icon_btn(
        "btn_toggle_vision_settings",
        &icons::GEAR,
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
                                    text(if ocr_copied {
                                        i18n::t(TextKey::ReaderCopied)
                                    } else {
                                        i18n::t(TextKey::ReaderCopy)
                                    })
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
                        .child(ghost_icon_btn(
                            "btn_ocr_clear",
                            &icons::X,
                            theme::text_muted(),
                            false,
                            move |s| {
                                s.clear_extracted_text(&nid_clear_ocr);
                            },
                        )),
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
            if is_busy {
                i18n::t(TextKey::ReaderTranslating)
            } else {
                i18n::t(TextKey::ReaderTranslate)
            },
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
                                    text(if tr_copied {
                                        i18n::t(TextKey::ReaderCopied)
                                    } else {
                                        i18n::t(TextKey::ReaderCopy)
                                    })
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
                        .child(ghost_icon_btn(
                            "btn_translate_clear",
                            &icons::X,
                            theme::text_muted(),
                            false,
                            move |s| {
                                s.clear_translated_text(&nid_clear_tr);
                            },
                        )),
                ),
        );
    }

    tr_col
}

pub(crate) const DRAWING_COLORS: [(&str, &str); 5] = [
    ("draw_col_ink", "#F4F3EE"),
    ("draw_col_coral", "#E07A5F"),
    ("draw_col_blue", "#8FA8C8"),
    ("draw_col_sage", "#81B29A"),
    ("draw_col_gold", "#F2CC8F"),
];

pub(crate) fn drawing_swatch_color(hex: &str) -> Color {
    if !theme::active_theme().is_dark() && hex == "#F4F3EE" {
        theme::text_primary()
    } else {
        theme::hex(hex)
    }
}

fn render_formula_inspector(s: &AppState, node: &Node) -> El {
    let nid = node.id.clone();
    let cur_lang = node.meta.language.as_deref().unwrap_or("MATH");
    let is_code = formula_math::is_code_language(Some(cur_lang));

    let mut langs = row().gap(10.0);
    for (key, lang) in [
        ("lang_math", "MATH"),
        ("lang_rust", "RUST"),
        ("lang_py", "PY"),
        ("lang_chem", "CHEM"),
    ] {
        let id_c = nid.clone();
        langs = langs.child(
            underline_tab(key, lang, cur_lang.eq_ignore_ascii_case(lang)).on_press(
                move |s: &mut AppState| {
                    s.set_formula_language(&id_c, lang);
                },
            ),
        );
    }

    if is_code {
        return col()
            .w_full()
            .gap(10.0)
            .child(
                col()
                    .w_full()
                    .min_h(140.0)
                    .max_h(260.0)
                    .px(10.0)
                    .py(8.0)
                    .radius(8.0)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_default())
                    .scroll(ScrollSpec { stick_bottom: false })
                    .child(
                        text_input(&s.inspector_content_edit, theme::MONO_BODY)
                            .key("inspector_formula_input")
                            .label("Formula code editor")
                            .w_full()
                            .min_h(120.0)
                            .color(theme::text_primary())
                            .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                            .placeholder("Write code snippet...", theme::text_muted())
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
            .child(row().w_full().center_y().child(langs));
    }

    let is_editing = s.selected_nodes.contains(&node.id)
        && s.editing_node_id.as_ref() == Some(&node.id);
    let (active_content, active_sel) = if is_editing {
        if s.canvas_content_edit.focused {
            (
                s.canvas_content_edit.text.as_str(),
                s.canvas_content_edit.range(),
            )
        } else {
            (
                s.inspector_content_edit.text.as_str(),
                s.inspector_content_edit.range(),
            )
        }
    } else {
        (node.content.as_str(), (node.content.len(), node.content.len()))
    };

    let mut out = col().w_full().gap(8.0);

    // 1. Photomath-style 2D Live Expression Area
    let mut display_row = row().w_full().center_y().gap(6.0).child(
        el().grow(1.0)
            .min_w(0.0)
            .clip_soft(0.0)
            .child(graph_formula::render_pretty_math_formula(
                active_content,
                19.0,
                if is_editing { Some(active_sel) } else { None },
                "Type your equation here...",
            )),
    );
    if !active_content.trim().is_empty() {
        let clr_id = nid.clone();
        display_row = display_row.child(
            el().key("insp_formula_clear")
                .size(22.0, 22.0)
                .radius(11.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.apply_formula_keypad(&clr_id, formula_math::FormulaKeyAction::Clear);
                })
                .child(icons::ph(&icons::X, 12.0, theme::text_muted())),
        );
    }

    out = out.child(
        col()
            .key("insp_formula_pretty")
            .w_full()
            .min_h(72.0)
            .max_h(140.0)
            .px(12.0)
            .py(10.0)
            .radius(8.0)
            .fill(theme::bg_input())
            .border(1.0, theme::border_default())
            .scroll(ScrollSpec { stick_bottom: false })
            .child(display_row),
    );

    // 2. Live Photomath Simplification / Solution Readout Banner
    if let Some(ro) = graph_formula::evaluate_formula_readout(active_content) {
        out = out.child(
            row()
                .key("insp_formula_readout")
                .w_full()
                .center_y()
                .px(12.0)
                .py(7.0)
                .radius(6.0)
                .fill(theme::accent_claude())
                .child(
                    text(ro)
                        .font(TextStyle::new(14.0, 650.0).mono())
                        .color(theme::text_inverted())
                        .truncate(),
                )
                .child(el().grow(1.0))
                .child(icons::ph(
                    &icons::CARET_RIGHT,
                    12.5,
                    theme::text_inverted().alpha(0.85),
                )),
        );
    }

    // 3. Optional Collapsible Raw Text/LaTeX Input (synced with keypad & canvas)
    if s.formula_show_raw_input {
        out = out.child(
            col()
                .w_full()
                .px(10.0)
                .py(6.0)
                .radius(6.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_subtle())
                .child(
                    text_input(&s.inspector_content_edit, theme::MONO_BODY)
                        .key("inspector_formula_input")
                        .label("Formula raw math input")
                        .w_full()
                        .min_h(32.0)
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder(
                            "Type math or \\alpha, \\frac{a}{b}, \\sqrt{x}...",
                            theme::text_muted(),
                        )
                        .on_edit(|s: &mut AppState| &mut s.inspector_content_edit)
                        .on_input_change(|s: &mut AppState, _| {
                            s.sync_from_inspector_edits();
                            let (a, b) = s.inspector_content_edit.range();
                            let cb = b.min(s.canvas_content_edit.text.len());
                            s.canvas_content_edit.set_selection(a.min(cb), cb);
                        }),
                ),
        );
    }

    // 4. Photomath Multi-Page 6x4 Math Keyboard (Dark Toolbar + Shaded Zones + 2D Keycaps)
    let page = s.formula_keypad_page;
    let page_tag = page.tag();
    let kp_rows = formula_math::keypad_rows(page);
    let is_dark = theme::active_theme().is_dark();
    let toolbar_bg = if is_dark {
        theme::bg_canvas()
    } else {
        theme::hex("#2E3033")
    };
    let toolbar_fg = if is_dark {
        theme::text_secondary()
    } else {
        theme::hex("#ECEBE6")
    };
    let toolbar_hover = Color::WHITE.alpha(0.10);

    let mut keypad = col()
        .key("insp_formula_keypad")
        .w_full()
        .radius(8.0)
        .clip_rounded(8.0)
        .fill(theme::bg_chrome())
        .border(1.0, theme::border_default());

    let mut top_bar = row()
        .w_full()
        .h(32.0)
        .px(4.0)
        .gap(2.0)
        .center_y()
        .fill(toolbar_bg);
    for (bkey, ico, act, active) in [
        (
            "raw",
            &icons::KEYBOARD,
            formula_math::FormulaKeyAction::ToggleRawInput,
            s.formula_show_raw_input,
        ),
        (
            "nl",
            &icons::ARROW_ELBOW_DOWN_LEFT,
            formula_math::FormulaKeyAction::Newline,
            false,
        ),
        (
            "left",
            &icons::CARET_LEFT,
            formula_math::FormulaKeyAction::MoveLeft,
            false,
        ),
        (
            "right",
            &icons::CARET_RIGHT,
            formula_math::FormulaKeyAction::MoveRight,
            false,
        ),
        (
            "bs",
            &icons::BACKSPACE,
            formula_math::FormulaKeyAction::Backspace,
            false,
        ),
    ] {
        let nid_b = nid.clone();
        top_bar = top_bar.child(
            el()
                .key(format!("insp_fkp_{bkey}"))
                .grow(1.0)
                .h(25.0)
                .radius(4.0)
                .fill(if active {
                    theme::accent_claude().alpha(0.22)
                } else {
                    Color::TRANSPARENT
                })
                .hover(toolbar_hover)
                .press(theme::press_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.apply_formula_keypad(&nid_b, act);
                })
                .child(icons::ph(
                    ico,
                    13.5,
                    if active {
                        theme::accent_claude()
                    } else {
                        toolbar_fg
                    },
                )),
        );
    }
    keypad = keypad.child(top_bar);

    let key_h = if page == formula_math::FormulaKeypadPage::Alpha {
        28.0
    } else {
        32.0
    };
    let mut grid = col().w_full().padding(5.0).gap(3.0);
    for row_keys in kp_rows {
        let mut r = row().w_full().gap(3.0);
        for k in *row_keys {
            let nid_k = nid.clone();
            let act = k.action;
            let fill_c = match k.zone {
                formula_math::FormulaKeyZone::Center => theme::bg_card(),
                formula_math::FormulaKeyZone::Side => theme::bg_control(),
                formula_math::FormulaKeyZone::Op => theme::bg_control(),
                formula_math::FormulaKeyZone::PageSwitch => theme::bg_card_selected(),
            };
            let keycap = if let Some(expr) = k.math_keycap {
                graph_formula::render_keycap_math(expr, 11.0)
            } else {
                let wt = match k.zone {
                    formula_math::FormulaKeyZone::Center => 550.0,
                    formula_math::FormulaKeyZone::PageSwitch => 600.0,
                    _ => 500.0,
                };
                text(k.label)
                    .font(TextStyle::new(12.5, wt))
                    .color(match k.zone {
                        formula_math::FormulaKeyZone::PageSwitch => theme::text_secondary(),
                        _ => theme::text_primary(),
                    })
            };
            r = r.child(
                el()
                    .key(format!("insp_fkp_{}_{}", page_tag, k.id))
                    .grow(1.0)
                    .min_w(0.0)
                    .h(key_h)
                    .radius(5.0)
                    .fill(fill_c)
                    .hover(theme::hover_subtle())
                    .press(theme::press_subtle())
                    .border(1.0, theme::border_subtle())
                    .center()
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.apply_formula_keypad(&nid_k, act);
                    })
                    .child(keycap),
            );
        }
        grid = grid.child(r);
    }
    keypad = keypad.child(grid);
    out = out.child(keypad);

    // 5. Notation Mode Footer + Shorthand Format Trigger
    let fmt_nid = nid.clone();
    out = out.child(
        row()
            .w_full()
            .center_y()
            .child(langs)
            .child(el().grow(1.0))
            .child(ghost_btn(
                "btn_inspector_formula_format",
                "\\→∑",
                theme::accent_claude(),
                move |s| {
                    s.apply_formula_keypad(&fmt_nid, formula_math::FormulaKeyAction::FormatShorthand);
                },
            )),
    );

    // 6. Compact Quick Formula Presets
    let mut tpl_panel = col().w_full().gap(4.0);
    for chunk in formula_math::FORMULA_TEMPLATES.chunks(3) {
        let mut r = row().w_full().gap(4.0);
        for &(name, tpl) in chunk {
            let id_t = nid.clone();
            r = r.child(
                el()
                    .key(format!("insp_tpl_{}", name))
                    .grow(1.0)
                    .py(4.0)
                    .px(6.0)
                    .radius(5.0)
                    .fill(theme::bg_control().alpha(0.6))
                    .hover(theme::hover_subtle())
                    .border(1.0, theme::border_subtle())
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

    out.child(tpl_panel)
}

fn render_graph_formula_inspector(s: &AppState, node: &Node) -> El {
    let accent = theme::kind_accent(NodeKind::GraphFormula);
    let plot_w = (s.inspector_width - 42.0).max(220.0);
    let plot_h = 220.0;
    let vp = graph_formula::GraphViewport::from_meta(&node.meta);
    let active_content = if s.selected_nodes.contains(&node.id) {
        s.inspector_content_edit.text.as_str()
    } else {
        node.content.as_str()
    };
    let plot = graph_formula::build_graph_plot(active_content, &vp, plot_w, plot_h, 1.0);
    let (xmin, xmax) = vp.x_bounds();
    let (ymin, ymax) = vp.y_bounds(plot_w, plot_h);

    let nid_zoom_in = node.id.clone();
    let nid_zoom_out = node.id.clone();
    let nid_zoom_reset = node.id.clone();
    let nid_wheel = node.id.clone();
    let nid_drag = node.id.clone();

    let mut out = col().w_full().gap(10.0);

    // 1. Readable 2D Formatted Equation Preview
    out = out.child(
        col()
            .key("insp_graph_formula_pretty")
            .w_full()
            .padding(12.0)
            .gap(8.0)
            .radius(8.0)
            .fill(theme::bg_input())
            .border(1.0, theme::border_default())
            .child(graph_formula::render_pretty_math(active_content, 17.0)),
    );

    // 2. Highlighted Simplification / Evaluation Readout Banner (Photomath-style)
    if let Some(err_msg) = &plot.error {
        out = out.child(
            el()
                .key("insp_graph_formula_error")
                .w_full()
                .px(12.0)
                .py(7.0)
                .radius(6.0)
                .fill(theme::accent_danger().alpha(0.16))
                .border(1.0, theme::accent_danger().alpha(0.45))
                .child(
                    text(err_msg)
                        .font(theme::STATUS_TEXT)
                        .color(theme::accent_danger())
                        .wrap(),
                ),
        );
    } else if let Some(ro) = &plot.readout {
        out = out.child(
            row()
                .key("insp_graph_formula_readout")
                .w_full()
                .center_y()
                .px(12.0)
                .py(8.0)
                .radius(6.0)
                .fill(theme::accent_claude())
                .child(
                    text(ro)
                        .font(TextStyle::new(14.0, 600.0).mono())
                        .color(theme::text_inverted())
                        .truncate(),
                )
                .child(el().grow(1.0))
                .child(
                    text(graph_formula::format_zoom_badge(&vp))
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_inverted().alpha(0.85)),
                ),
        );
    }

    // 3. Direct Text Input (synced seamlessly with keypad buttons)
    out = out.child(
        col()
            .w_full()
            .px(10.0)
            .py(7.0)
            .radius(6.0)
            .fill(theme::bg_input())
            .border(1.0, theme::border_subtle())
            .child(
                text_input(&s.inspector_content_edit, theme::MONO_BODY)
                    .key("inspector_graph_formula_input")
                    .label("Graphic formula equation")
                    .w_full()
                    .min_h(34.0)
                    .color(theme::text_primary())
                    .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                    .placeholder(i18n::t(TextKey::GraphFormulaPlaceholder), theme::text_muted())
                    .on_edit(|s: &mut AppState| &mut s.inspector_content_edit)
                    .on_input_change(|s: &mut AppState, _| {
                        s.sync_from_inspector_edits();
                    }),
            ),
    );

    // 4. Photomath-Style Interactive Formatting Keypad
    let mode = s.graph_keypad_mode;
    let kp_rows = match mode {
        graph_formula::KeypadMode::Sci => graph_formula::SCI_KEYPAD_ROWS,
        graph_formula::KeypadMode::Basic => graph_formula::BASIC_KEYPAD_ROWS,
    };
    let mut keypad = col()
        .key("insp_graph_formula_keypad")
        .w_full()
        .gap(4.0)
        .padding(6.0)
        .radius(8.0)
        .fill(theme::bg_chrome())
        .border(1.0, theme::border_subtle());

    let mut ctrl_bar = row().w_full().gap(4.0);
    for (bkey, blabel, act) in [
        (
            "mode",
            if mode == graph_formula::KeypadMode::Sci {
                "123"
            } else {
                "f(x)"
            },
            graph_formula::KeypadAction::ToggleMode,
        ),
        ("nl", "↵", graph_formula::KeypadAction::Newline),
        ("left", "<", graph_formula::KeypadAction::MoveLeft),
        ("right", ">", graph_formula::KeypadAction::MoveRight),
        ("bs", "⌫", graph_formula::KeypadAction::Backspace),
        ("clr", "✕", graph_formula::KeypadAction::Clear),
    ] {
        let nid_b = node.id.clone();
        ctrl_bar = ctrl_bar.child(
            el()
                .key(format!("insp_kp_{bkey}"))
                .grow(1.0)
                .h(26.0)
                .radius(4.0)
                .fill(theme::bg_control())
                .hover(theme::hover_subtle())
                .border(1.0, theme::border_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.apply_graph_formula_keypad(&nid_b, act);
                })
                .child(
                    text(blabel)
                        .font(TextStyle::new(11.5, 600.0).mono())
                        .color(theme::text_primary()),
                ),
        );
    }
    keypad = keypad.child(ctrl_bar);

    for (r_idx, row_btns) in kp_rows.iter().enumerate() {
        let mut r = row().w_full().gap(4.0);
        for btn in *row_btns {
            let nid_k = node.id.clone();
            let act = btn.action;
            r = r.child(
                el()
                    .key(format!("insp_kp_{}_{}_{}", mode as u8, r_idx, btn.id))
                    .grow(1.0)
                    .min_w(0.0)
                    .h(30.0)
                    .radius(5.0)
                    .fill(theme::bg_input())
                    .hover(theme::hover_subtle())
                    .border(1.0, theme::border_subtle())
                    .center()
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.apply_graph_formula_keypad(&nid_k, act);
                    })
                    .child(
                        text(btn.label)
                            .font(TextStyle::new(12.0, 500.0))
                            .color(theme::text_primary()),
                    ),
            );
        }
        keypad = keypad.child(r);
    }
    out = out.child(keypad);

    // 5. Large Interactive 2D Graph Plot + Zoom/Pan Controls
    let mut plot_box = el()
        .key("insp_graph_formula_plot")
        .w(plot_w)
        .h(plot_h)
        .radius(8.0)
        .clip_rounded(8.0)
        .fill(theme::bg_input())
        .border(1.0, theme::border_default())
        .cursor(Cursor::Grab)
        .on_wheel(move |s: &mut AppState, e, tx| {
            let factor = (-e.dy * 0.0022).clamp(-0.35, 0.35).exp() as f64;
            let anchor_norm = if e.x >= 0.0 && e.x <= plot_w && e.y >= 0.0 && e.y <= plot_h {
                Some((e.x / plot_w.max(1.0), e.y / plot_h.max(1.0)))
            } else {
                Some((0.5, 0.5))
            };
            s.zoom_graph_formula(&nid_wheel, factor, anchor_norm, (plot_w, plot_h));
            tx.instant();
            true
        })
        .on_drag(move |s: &mut AppState, d, tx| match d.phase {
            Phase::Down => {
                if let Some(n) = s.graph.get_node(&nid_drag) {
                    let cur_vp = graph_formula::GraphViewport::from_meta(&n.meta);
                    s.graph_drag_origin = Some((nid_drag.clone(), cur_vp.center_x, cur_vp.center_y));
                }
            }
            Phase::Move => {
                if let Some((orig_id, cx0, cy0)) = s.graph_drag_origin.clone() {
                    if orig_id == nid_drag {
                        s.pan_graph_formula(&nid_drag, cx0, cy0, d.dx, d.dy, (plot_w, plot_h));
                        tx.instant();
                    }
                }
            }
            Phase::Up => {
                s.graph_drag_origin = None;
                s.finish_pan_graph_formula(&nid_drag);
            }
        })
        .child(canvas(plot.shape).absolute(0.0, 0.0).w_full().h_full());

    for tl in plot.tick_labels {
        let tx_pos = if tl.align_right {
            (tl.x - (tl.text.len() as f32 * 5.8)).clamp(2.0, (plot_w - 22.0).max(2.0))
        } else {
            tl.x.clamp(2.0, (plot_w - 22.0).max(2.0))
        };
        let ty_pos = tl.y.clamp(2.0, (plot_h - 12.0).max(2.0));
        plot_box = plot_box.child(
            el().absolute(tx_pos, ty_pos).child(
                text(tl.text)
                    .font(TextStyle::new(9.5, 450.0).mono())
                    .color(theme::text_muted()),
            ),
        );
    }

    let plot_toolbar = row()
        .w_full()
        .center_y()
        .gap(6.0)
        .child(
            text(format!(
                "x ∈ [{:.2}, {:.2}] · y ∈ [{:.2}, {:.2}]",
                xmin, xmax, ymin, ymax
            ))
            .font(theme::BADGE_TEXT)
            .color(theme::text_muted())
            .truncate(),
        )
        .child(el().grow(1.0))
        .child(ghost_btn(
            "btn_insp_graph_zoom_in",
            "+",
            theme::text_primary(),
            move |s| {
                s.zoom_graph_formula(&nid_zoom_in, 0.72, None, (plot_w, plot_h));
            },
        ))
        .child(ghost_btn(
            "btn_insp_graph_zoom_out",
            "−",
            theme::text_primary(),
            move |s| {
                s.zoom_graph_formula(&nid_zoom_out, 1.39, None, (plot_w, plot_h));
            },
        ))
        .child(ghost_btn(
            "btn_insp_graph_zoom_reset",
            i18n::t(TextKey::GraphFormulaResetView),
            accent,
            move |s| {
                s.reset_graph_formula_view(&nid_zoom_reset);
            },
        ));

    out = out.child(plot_box);
    if plot.curve_legend.len() > 1 {
        let mut legend = row().w_full().wrap().gap(8.0);
        for (expr_lbl, col_dot) in &plot.curve_legend {
            legend = legend.child(
                row()
                    .center_y()
                    .gap(4.0)
                    .child(el().size(7.0, 7.0).radius(3.5).fill(*col_dot))
                    .child(
                        text(expr_lbl)
                            .font(theme::BADGE_TEXT)
                            .color(theme::text_secondary())
                            .truncate(),
                    ),
            );
        }
        out = out.child(legend);
    }
    out = out.child(plot_toolbar);

    // 6. Quick Presets
    let mut presets_col = col()
        .w_full()
        .gap(4.0)
        .child(subtle_tag(
            i18n::t(TextKey::GraphFormulaPresetsTitle),
            theme::text_muted(),
        ));
    for chunk in graph_formula::GRAPH_PRESETS.chunks(4) {
        let mut r = row().w_full().gap(4.0);
        for &(name, expr) in chunk {
            let nid_p = node.id.clone();
            r = r.child(
                el()
                    .key(format!("insp_graph_preset_{name}"))
                    .grow(1.0)
                    .min_w(0.0)
                    .py(5.0)
                    .px(4.0)
                    .radius(5.0)
                    .fill(theme::bg_input())
                    .hover(theme::hover_subtle())
                    .border(1.0, theme::border_subtle())
                    .center()
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.set_graph_formula_preset(&nid_p, expr);
                    })
                    .child(
                        text(name)
                            .font(theme::BADGE_TEXT)
                            .color(theme::text_secondary())
                            .truncate(),
                    ),
            );
        }
        presets_col = presets_col.child(r);
    }

    out.child(presets_col)
}

fn render_drawing_inspector(s: &AppState, node: &Node) -> El {
    let accent = theme::kind_accent(NodeKind::Drawing);
    let pad_w = (s.inspector_width - 42.0).max(200.0);
    let pad_h = 220.0;
    let shape = crate::canvas_view::build_drawing_shape(&node.meta.drawing_strokes, pad_w, pad_h, 1.0);
    let drag_id = node.id.clone();
    let undo_id = node.id.clone();
    let clear_id = node.id.clone();
    let text_id = node.id.clone();
    let formula_id = node.id.clone();
    let spawn_note_id = node.id.clone();
    let spawn_formula_id = node.id.clone();
    let clear_ocr_id = node.id.clone();
    let is_busy = s.vision_pending_nodes.contains(&node.id);
    let eraser_active = s.drawing_eraser;

    let mut pad = el()
        .key("inspector_drawing_pad")
        .w(pad_w)
        .h(pad_h)
        .radius(8.0)
        .fill(theme::bg_input())
        .border(1.0, theme::border_default())
        .cursor(Cursor::Crosshair)
        .on_drag(move |s: &mut AppState, d, tx| match d.phase {
            Phase::Down => {
                let nx = (d.x / d.w.max(1.0)).clamp(0.0, 1.0);
                let ny = (d.y / d.h.max(1.0)).clamp(0.0, 1.0);
                s.begin_drawing_stroke(&drag_id, nx, ny);
                tx.instant();
            }
            Phase::Move => {
                let nx = (d.x / d.w.max(1.0)).clamp(0.0, 1.0);
                let ny = (d.y / d.h.max(1.0)).clamp(0.0, 1.0);
                s.extend_drawing_stroke(&drag_id, nx, ny);
                tx.instant();
            }
            Phase::Up => {
                s.finish_drawing_stroke(&drag_id);
            }
        })
        .child(canvas(shape).absolute(0.0, 0.0).w_full().h_full());

    if node.meta.drawing_strokes.is_empty() {
        pad = pad.child(
            el().absolute(0.0, 0.0).w_full().h_full().center().child(
                text(i18n::t(TextKey::DrawingEmptyHint))
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_muted()),
            ),
        );
    }

    let mut color_swatches = row().center_y().gap(5.0);
    for (ckey, hex) in DRAWING_COLORS {
        let active = !eraser_active && s.drawing_color_hex.eq_ignore_ascii_case(hex);
        let swatch_color = drawing_swatch_color(hex);
        color_swatches = color_swatches.child(
            el()
                .key(ckey)
                .size(16.0, 16.0)
                .radius(8.0)
                .fill(swatch_color)
                .border(
                    if active { 2.0 } else { 1.0 },
                    if active {
                        theme::text_primary()
                    } else {
                        theme::border_default()
                    },
                )
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.drawing_eraser = false;
                    s.drawing_color_hex = hex.to_string();
                }),
        );
    }

    let widths = [
        ("draw_w_thin", i18n::t(TextKey::DrawingThin), 2.0f32),
        ("draw_w_med", i18n::t(TextKey::DrawingMedium), 3.5f32),
        ("draw_w_thick", i18n::t(TextKey::DrawingThick), 6.0f32),
    ];
    let mut width_row = row().center_y().gap(4.0);
    for (wkey, wlabel, wval) in widths {
        let active = (s.drawing_stroke_width - wval).abs() < 0.4;
        width_row = width_row.child(
            el()
                .key(wkey)
                .px(7.0)
                .py(3.0)
                .radius(5.0)
                .fill(if active {
                    theme::bg_control()
                } else {
                    Color::TRANSPARENT
                })
                .border(
                    1.0,
                    if active {
                        theme::border_strong()
                    } else {
                        theme::border_subtle()
                    },
                )
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.drawing_stroke_width = wval;
                })
                .child(
                    text(wlabel)
                        .font(theme::BADGE_TEXT)
                        .color(if active {
                            theme::text_primary()
                        } else {
                            theme::text_muted()
                        }),
                ),
        );
    }

    let tools_row = row()
        .w_full()
        .center_y()
        .gap(5.0)
        .child(
            el()
                .key("btn_draw_pen")
                .px(8.0)
                .py(4.0)
                .radius(5.0)
                .fill(if !eraser_active {
                    accent.alpha(0.18)
                } else {
                    theme::bg_control()
                })
                .border(
                    1.0,
                    if !eraser_active {
                        accent.alpha(0.55)
                    } else {
                        theme::border_subtle()
                    },
                )
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.drawing_eraser = false;
                })
                .child(
                    text(i18n::t(TextKey::DrawingPen))
                        .font(theme::BADGE_TEXT)
                        .color(if !eraser_active {
                            accent
                        } else {
                            theme::text_secondary()
                        }),
                ),
        )
        .child(
            el()
                .key("btn_draw_eraser")
                .px(8.0)
                .py(4.0)
                .radius(5.0)
                .fill(if eraser_active {
                    accent.alpha(0.18)
                } else {
                    theme::bg_control()
                })
                .border(
                    1.0,
                    if eraser_active {
                        accent.alpha(0.55)
                    } else {
                        theme::border_subtle()
                    },
                )
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.drawing_eraser = true;
                })
                .child(
                    text(i18n::t(TextKey::DrawingEraser))
                        .font(theme::BADGE_TEXT)
                        .color(if eraser_active {
                            accent
                        } else {
                            theme::text_secondary()
                        }),
                ),
        )
        .child(el().grow(1.0))
        .child(ghost_icon_btn(
            "btn_draw_undo",
            &icons::ARROW_CLOCKWISE,
            theme::text_secondary(),
            false,
            move |s| s.undo_drawing_stroke(&undo_id),
        ))
        .child(ghost_icon_btn(
            "btn_draw_clear",
            &icons::X,
            theme::text_muted(),
            false,
            move |s| s.clear_drawing_strokes(&clear_id),
        ));

    let style_row = row()
        .w_full()
        .center_y()
        .gap(8.0)
        .child(color_swatches)
        .child(el().grow(1.0))
        .child(width_row);

    let interpret_row = row()
        .w_full()
        .center_y()
        .gap(6.0)
        .child(
            el()
                .key("btn_draw_interpret_text")
                .h(26.0)
                .px(10.0)
                .radius(6.0)
                .fill(theme::accent_primary())
                .hover(theme::accent_primary().alpha(0.88))
                .center()
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.interpret_drawing_node(&text_id, OcrMode::PlainText);
                })
                .child(
                    text(if is_busy {
                        i18n::t(TextKey::DrawingInterpreting)
                    } else {
                        i18n::t(TextKey::DrawingInterpretText)
                    })
                    .font(theme::BADGE_TEXT)
                    .color(theme::text_inverted()),
                ),
        )
        .child(ghost_btn(
            "btn_draw_interpret_formula",
            i18n::t(TextKey::DrawingInterpretFormula),
            theme::accent_claude(),
            move |s| s.interpret_drawing_node(&formula_id, OcrMode::EquationFormula),
        ));

    let mut sec = col()
        .key("drawing_inspector_section")
        .w_full()
        .gap(10.0)
        .child(tools_row)
        .child(style_row)
        .child(pad)
        .child(interpret_row);

    if let Some(err) = &s.vision_last_error {
        sec = sec.child(
            text(err)
                .font(theme::BADGE_TEXT)
                .color(theme::accent_danger())
                .w_full()
                .wrap(),
        );
    }

    if let Some(extracted) = node
        .meta
        .extracted_text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        let extracted_owned = extracted.to_string();
        let draw_copied = s.is_recently_copied("drawing_ocr");
        sec = sec.child(
            col()
                .key("drawing_ocr_result")
                .w_full()
                .gap(8.0)
                .padding(10.0)
                .radius(8.0)
                .fill(theme::bg_card())
                .border(1.0, accent.alpha(0.35))
                .child(subtle_tag(
                    i18n::t(TextKey::DrawingRecognizedTitle),
                    accent,
                ))
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
                            el()
                                .key("btn_draw_copy_ocr")
                                .px(7.0)
                                .py(3.0)
                                .radius(5.0)
                                .fill(theme::bg_control())
                                .hover(theme::hover_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                                    tx.copy(extracted_owned.clone());
                                    s.mark_copied(
                                        "drawing_ocr",
                                        i18n::t(TextKey::ReaderCopied),
                                    );
                                })
                                .child(
                                    text(if draw_copied {
                                        i18n::t(TextKey::ReaderCopied)
                                    } else {
                                        i18n::t(TextKey::DrawingCopyText)
                                    })
                                    .font(theme::BADGE_TEXT)
                                    .color(if draw_copied {
                                        accent
                                    } else {
                                        theme::text_primary()
                                    }),
                                ),
                        )
                        .child(el().grow(1.0))
                        .child(ghost_btn(
                            "btn_draw_spawn_note",
                            "+ Note",
                            theme::text_secondary(),
                            move |s| {
                                s.spawn_note_from_extracted_text(&spawn_note_id);
                            },
                        ))
                        .child(ghost_btn(
                            "btn_draw_spawn_formula",
                            "+ Formula",
                            theme::text_secondary(),
                            move |s| {
                                s.spawn_formula_from_extracted_text(&spawn_formula_id);
                            },
                        ))
                        .child(ghost_icon_btn(
                            "btn_draw_clear_ocr",
                            &icons::X,
                            theme::text_muted(),
                            false,
                            move |s| s.clear_extracted_text(&clear_ocr_id),
                        )),
                ),
        );
    }

    sec
}

fn render_media_inspector(s: &AppState, node: &Node) -> El {
    let accent = theme::kind_accent(node.kind);
    let nid_pick = node.id.clone();
    let nid_url_submit = node.id.clone();
    let nid_url_btn = node.id.clone();
    let nid_ts_submit = node.id.clone();
    let nid_ts_btn = node.id.clone();

    let mut sec = col()
        .key("media_inspector_section")
        .w_full()
        .gap(10.0)
        .child(
            row()
                .w_full()
                .center_y()
                .gap(6.0)
                .child(
                    el()
                        .key("btn_ingest_media")
                        .px(9.0)
                        .py(5.0)
                        .radius(6.0)
                        .fill(theme::bg_control())
                        .border(1.0, theme::border_default())
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press_tx(move |_s: &mut AppState, tx: &mut Tx| {
                            let opts = FileDialogOptions {
                                title: i18n::t(TextKey::MediaIngestFile).to_string(),
                                ..Default::default()
                            }
                            .filter(
                                "Media",
                                &[
                                    "mp4", "mov", "m4v", "webm", "mkv", "mp3", "wav", "m4a",
                                    "flac", "ogg", "aac", "opus",
                                ],
                            );
                            let nid = nid_pick.clone();
                            tx.open_file(opts, move |s: &mut AppState, res, _| {
                                if let Ok(Some(path)) = res {
                                    s.ingest_media_into_node(&nid, &path);
                                }
                            });
                        })
                        .child(
                            text(format!("+ {}", i18n::t(TextKey::MediaIngestFile)))
                                .font(theme::STATUS_TEXT)
                                .color(theme::text_primary()),
                        ),
                )
                .child(el().grow(1.0)),
        )
        .child(
            row()
                .w_full()
                .h(34.0)
                .pl(10.0)
                .pr(4.0)
                .gap(6.0)
                .center_y()
                .radius(6.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_subtle())
                .child(
                    text_input(&s.media_url_edit, theme::STATUS_TEXT)
                        .key("input_media_url")
                        .label("Media URL")
                        .single_line()
                        .grow(1.0)
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder(i18n::t(TextKey::MediaUrlPlaceholder), theme::text_muted())
                        .on_edit(|s: &mut AppState| &mut s.media_url_edit)
                        .on_submit(move |s: &mut AppState, _| {
                            s.save_media_url(&nid_url_submit);
                        }),
                )
                .child(
                    el()
                        .key("btn_save_media_url")
                        .px(8.0)
                        .py(4.0)
                        .radius(5.0)
                        .fill(accent.alpha(0.18))
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.save_media_url(&nid_url_btn);
                        })
                        .child(
                            text(i18n::t(TextKey::MediaSaveBtn))
                                .font(theme::BADGE_TEXT)
                                .color(accent),
                        ),
                ),
        );

    if let Some(player) = s.media_player_for(&node.id) {
        if node.kind == NodeKind::Video {
            sec = sec.child(
                el()
                    .w_full()
                    .h(168.0)
                    .radius(8.0)
                    .fill(Color::BLACK)
                    .video(player, SurfaceFit::Fit),
            );
        }
        let nid_play = node.id.clone();
        let nid_back = node.id.clone();
        let nid_fwd = node.id.clone();
        let nid_mute = node.id.clone();
        let nid_rate = node.id.clone();
        let nid_mark = node.id.clone();
        let nid_scrub = node.id.clone();
        let nid_retry_fail = node.id.clone();
        let cur = player.time();
        let dur = player.duration().unwrap_or(0.0);
        let frac = if dur > 0.0 {
            (cur / dur).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let bar_w = (s.inspector_width - 42.0).max(160.0);
        let playing = player.is_playing();
        let muted = player.muted();
        let rate = player.rate();
        let player_err = match player.state() {
            rune::PlayerState::Failed(msg) => Some(msg),
            _ => None,
        };

        sec = sec
            .child(
                el()
                    .key("insp_media_scrub")
                    .w(bar_w)
                    .h(8.0)
                    .radius(4.0)
                    .fill(theme::border_subtle())
                    .cursor(Cursor::Pointer)
                    .on_drag(move |s: &mut AppState, d, tx| match d.phase {
                        Phase::Down | Phase::Move => {
                            let ratio = (d.x / d.w.max(1.0)).clamp(0.0, 1.0) as f64;
                            if let Some(dur_val) = s.media_player_for(&nid_scrub).and_then(|p| p.duration()) {
                                if dur_val > 0.0 {
                                    s.seek_media(&nid_scrub, ratio * dur_val);
                                    tx.instant();
                                }
                            }
                        }
                        Phase::Up => {}
                    })
                    .child(
                        el()
                            .w((bar_w * frac).clamp(0.0, bar_w))
                            .h_full()
                            .radius(4.0)
                            .fill(accent),
                    ),
            )
            .child(
                row()
                    .w_full()
                    .center_y()
                    .gap(5.0)
                    .child(
                        el()
                            .key("btn_insp_media_play")
                            .px(8.0)
                            .py(4.0)
                            .radius(5.0)
                            .fill(theme::bg_control())
                            .border(1.0, theme::border_default())
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.toggle_media_play(&nid_play);
                            })
                            .child(
                                row()
                                    .center_y()
                                    .gap(5.0)
                                    .child(icons::ph(
                                        if playing { &icons::PAUSE } else { &icons::PLAY },
                                        12.0,
                                        accent,
                                    ))
                                    .child(
                                        text(if playing { "Pause" } else { "Play" })
                                            .font(theme::BADGE_TEXT)
                                            .color(accent),
                                    ),
                            ),
                    )
                    .child(
                        el()
                            .key("btn_insp_media_back")
                            .px(6.0)
                            .py(4.0)
                            .radius(5.0)
                            .fill(theme::bg_control())
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                let t = s
                                    .media_player_for(&nid_back)
                                    .map(|p| p.time() - 10.0)
                                    .unwrap_or(0.0);
                                s.seek_media(&nid_back, t.max(0.0));
                            })
                            .child(
                                text("-10s")
                                    .font(theme::BADGE_TEXT)
                                    .color(theme::text_secondary()),
                            ),
                    )
                    .child(
                        el()
                            .key("btn_insp_media_fwd")
                            .px(6.0)
                            .py(4.0)
                            .radius(5.0)
                            .fill(theme::bg_control())
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                let t = s
                                    .media_player_for(&nid_fwd)
                                    .map(|p| p.time() + 10.0)
                                    .unwrap_or(10.0);
                                s.seek_media(&nid_fwd, t);
                            })
                            .child(
                                text("+10s")
                                    .font(theme::BADGE_TEXT)
                                    .color(theme::text_secondary()),
                            ),
                    )
                    .child(
                        el()
                            .key("btn_insp_media_mute")
                            .px(6.0)
                            .py(4.0)
                            .radius(5.0)
                            .fill(theme::bg_control())
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.toggle_media_mute(&nid_mute);
                            })
                            .child(
                                text(if muted { "Unmute" } else { "Mute" })
                                    .font(theme::BADGE_TEXT)
                                    .color(theme::text_secondary()),
                            ),
                    )
                    .child(
                        el()
                            .key("btn_insp_media_rate")
                            .px(6.0)
                            .py(4.0)
                            .radius(5.0)
                            .fill(theme::bg_control())
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.cycle_media_rate(&nid_rate);
                            })
                            .child(
                                text(format!("{:.2}x", rate))
                                    .font(theme::BADGE_TEXT)
                                    .color(theme::text_secondary()),
                            ),
                    )
                    .child(el().grow(1.0))
                    .child(
                        el()
                            .key("btn_insp_media_mark")
                            .px(7.0)
                            .py(4.0)
                            .radius(5.0)
                            .fill(theme::bg_control())
                            .border(1.0, theme::border_subtle())
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.mark_current_media_timestamp(&nid_mark);
                            })
                            .child(
                                text("+ Mark")
                                    .font(theme::BADGE_TEXT)
                                    .color(theme::text_primary()),
                            ),
                    ),
            )
            .child(
                text(if dur > 0.0 {
                    format!(
                        "{} / {}",
                        media_helper::format_timestamp(cur),
                        media_helper::format_timestamp(dur)
                    )
                } else {
                    media_helper::format_timestamp(cur)
                })
                .font(theme::BADGE_TEXT)
                .color(theme::text_muted()),
            );
        if let Some(err) = player_err {
            sec = sec.child(
                row()
                    .w_full()
                    .center_y()
                    .gap(6.0)
                    .child(
                        text(err)
                            .font(theme::BADGE_TEXT)
                            .color(theme::accent_danger())
                            .grow(1.0)
                            .wrap(),
                    )
                    .child(
                        el()
                            .key("btn_insp_media_retry")
                            .px(7.0)
                            .py(3.5)
                            .radius(5.0)
                            .fill(theme::bg_control())
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.media_players.remove(&nid_retry_fail);
                                let _ = s.ensure_media_player(&nid_retry_fail);
                            })
                            .child(icons::ph(
                                &icons::ARROW_CLOCKWISE,
                                12.0,
                                theme::text_primary(),
                            )),
                    ),
            );
        }
    } else if let Some(url) = node
        .meta
        .media_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
    {
        let nid_open = node.id.clone();
        let nid_reload = node.id.clone();
        let nid_toggle = node.id.clone();
        let plat = media_helper::classify_media_url(url);
        let is_iframe = AppState::media_uses_iframe(url);
        let collapsed = s.embed_collapsed.contains(&node.id);
        let mut actions_row = row().w_full().center_y().gap(6.0);
        if is_iframe {
            actions_row = actions_row
                .child(
                    el()
                        .key("btn_insp_reload_embed")
                        .px(8.0)
                        .py(4.0)
                        .radius(5.0)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.reload_embed_iframe(&nid_reload);
                        })
                        .child(icons::ph(
                            &icons::ARROW_CLOCKWISE,
                            12.0,
                            theme::text_primary(),
                        )),
                )
                .child(
                    el()
                        .key("btn_insp_toggle_embed")
                        .px(8.0)
                        .py(4.0)
                        .radius(5.0)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.toggle_embed_collapsed(&nid_toggle);
                        })
                        .child(
                            text(if collapsed {
                                i18n::t(TextKey::MediaShowEmbed)
                            } else {
                                i18n::t(TextKey::MediaHideEmbed)
                            })
                            .font(theme::BADGE_TEXT)
                            .color(theme::text_secondary()),
                        ),
                );
        }
        actions_row = actions_row.child(el().grow(1.0)).child(
            el()
                .key("btn_insp_media_open_url")
                .px(8.0)
                .py(4.0)
                .radius(5.0)
                .fill(theme::bg_control())
                .border(1.0, theme::border_subtle())
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                    s.open_media_external_url(&nid_open, 0.0, tx);
                })
                .child(
                    text(i18n::t(TextKey::MediaOpenLink))
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_primary()),
                ),
        );
        sec = sec.child(
            col()
                .w_full()
                .padding(8.0)
                .gap(6.0)
                .radius(6.0)
                .fill(theme::bg_card())
                .border(1.0, theme::border_subtle())
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .gap(8.0)
                        .child(subtle_tag(plat.label(), accent))
                        .child(
                            text(if is_iframe {
                                i18n::t(TextKey::MediaIframeActive)
                            } else {
                                ""
                            })
                            .font(theme::BADGE_TEXT)
                            .color(theme::text_secondary())
                            .grow(1.0),
                        ),
                )
                .child(actions_row),
        );
    }

    if node.kind == NodeKind::Audio {
        sec = sec.child(render_audio_subtitles_inspector(s, node, accent));
    }

    // Add timestamp row
    sec = sec.child(
        row()
            .w_full()
            .h(32.0)
            .gap(6.0)
            .center_y()
            .child(
                el()
                    .w(68.0)
                    .h_full()
                    .px(8.0)
                    .center_y()
                    .radius(5.0)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_subtle())
                    .child(
                        text_input(&s.media_ts_time_edit, theme::STATUS_TEXT)
                            .key("input_ts_time")
                            .label("Timestamp time")
                            .single_line()
                            .w_full()
                            .color(theme::text_primary())
                            .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                            .placeholder("01:30", theme::text_muted())
                            .on_edit(|s: &mut AppState| &mut s.media_ts_time_edit),
                    ),
            )
            .child(
                el()
                    .grow(1.0)
                    .h_full()
                    .px(8.0)
                    .center_y()
                    .radius(5.0)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_subtle())
                    .child(
                        text_input(&s.media_ts_label_edit, theme::STATUS_TEXT)
                            .key("input_ts_label")
                            .label("Timestamp label")
                            .single_line()
                            .w_full()
                            .color(theme::text_primary())
                            .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                            .placeholder("Key moment...", theme::text_muted())
                            .on_edit(|s: &mut AppState| &mut s.media_ts_label_edit)
                            .on_submit(move |s: &mut AppState, _| {
                                let secs = media_helper::parse_timestamp(&s.media_ts_time_edit.text)
                                    .unwrap_or_else(|| {
                                        s.media_player_for(&nid_ts_submit)
                                            .map(|p| p.time())
                                            .unwrap_or(0.0)
                                    });
                                let lbl = s.media_ts_label_edit.text.clone();
                                s.add_media_timestamp(&nid_ts_submit, secs, lbl);
                                s.media_ts_time_edit = rune::TextEdit::new("");
                                s.media_ts_label_edit = rune::TextEdit::new("");
                            }),
                    ),
            )
            .child(
                el()
                    .key("btn_add_media_ts")
                    .h_full()
                    .px(9.0)
                    .center()
                    .radius(5.0)
                    .fill(accent.alpha(0.2))
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        let secs = media_helper::parse_timestamp(&s.media_ts_time_edit.text)
                            .unwrap_or_else(|| {
                                s.media_player_for(&nid_ts_btn)
                                    .map(|p| p.time())
                                    .unwrap_or(0.0)
                            });
                        let lbl = s.media_ts_label_edit.text.clone();
                        s.add_media_timestamp(&nid_ts_btn, secs, lbl);
                        s.media_ts_time_edit = rune::TextEdit::new("");
                        s.media_ts_label_edit = rune::TextEdit::new("");
                    })
                    .child(
                        text(i18n::t(TextKey::MediaAddTimestamp))
                            .font(theme::BADGE_TEXT)
                            .color(accent),
                    ),
            ),
    );

    // Existing timestamps
    for (idx, ts) in node.meta.timestamps.iter().enumerate() {
        let nid_j = node.id.clone();
        let nid_d = node.id.clone();
        let secs = ts.seconds;
        let ts_id = ts.id.clone();
        let jump_lbl = format!("▶ {} · {}", media_helper::format_timestamp(secs), ts.label);
        sec = sec.child(
            row()
                .w_full()
                .center_y()
                .gap(6.0)
                .px(8.0)
                .py(5.0)
                .radius(5.0)
                .fill(theme::bg_card())
                .border(1.0, theme::border_subtle())
                .child(
                    el()
                        .key(format!("btn_insp_ts_jump_{}", idx))
                        .grow(1.0)
                        .cursor(Cursor::Pointer)
                        .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                            s.jump_to_media_timestamp(&nid_j, secs, tx);
                        })
                        .child(
                            text(jump_lbl)
                                .font(theme::STATUS_TEXT)
                                .color(accent)
                                .truncate(),
                        ),
                )
                .child(
                    el()
                        .key(format!("btn_insp_ts_del_{}", idx))
                        .px(6.0)
                        .py(2.0)
                        .radius(4.0)
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.remove_media_timestamp(&nid_d, &ts_id);
                        })
                        .child(icons::ph(&icons::X, 11.5, theme::text_muted())),
                ),
        );
    }

    sec
}

fn render_audio_subtitles_inspector(s: &AppState, node: &Node, accent: Color) -> El {
    let nid_auto = node.id.clone();
    let nid_load = node.id.clone();
    let nid_cc = node.id.clone();
    let is_pending = s.audio_subtitle_pending.contains(&node.id);
    let err_opt = s.audio_subtitle_errors.get(&node.id).cloned();
    let plain_text = node
        .meta
        .transcript_text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| media_helper::cues_to_plain_text(&node.meta.subtitles));
    let timed_text = media_helper::cues_to_timed_text(&node.meta.subtitles);
    let has_cues = !node.meta.subtitles.is_empty();
    let has_any = has_cues || !plain_text.is_empty();
    let active_idx = s.media_player_for(&node.id).and_then(|p| {
        media_helper::active_subtitle_cue(&node.meta.subtitles, p.time()).map(|(idx, _)| idx)
    });

    let mut box_col = col()
        .key("media_subtitles_section")
        .w_full()
        .padding(8.0)
        .gap(8.0)
        .radius(6.0)
        .fill(theme::bg_card())
        .border(1.0, theme::border_subtle())
        .child(
            row()
                .w_full()
                .center_y()
                .gap(6.0)
                .child(subtle_tag(
                    i18n::t(TextKey::AudioSubtitlesTitle),
                    accent,
                ))
                .child(el().grow(1.0))
                .child(
                    el()
                        .key("btn_insp_cc_on_node")
                        .px(7.0)
                        .py(3.5)
                        .radius(5.0)
                        .fill(if node.meta.show_subtitles_on_card {
                            accent.alpha(0.22)
                        } else {
                            theme::bg_control()
                        })
                        .border(
                            1.0,
                            if node.meta.show_subtitles_on_card {
                                accent.alpha(0.6)
                            } else {
                                theme::border_subtle()
                            },
                        )
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.toggle_audio_subtitles_on_card(&nid_cc);
                        })
                        .child(
                            text(i18n::t(TextKey::AudioShowSubtitlesOnCard))
                                .font(theme::BADGE_TEXT)
                                .color(if node.meta.show_subtitles_on_card {
                                    accent
                                } else {
                                    theme::text_secondary()
                                }),
                        ),
                ),
        )
        .child(
            row()
                .w_full()
                .center_y()
                .gap(6.0)
                .child(
                    el()
                        .key("btn_insp_auto_subtitles")
                        .px(9.0)
                        .py(4.5)
                        .radius(5.0)
                        .fill(theme::bg_control())
                        .border(1.0, theme::border_default())
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.generate_audio_subtitles(&nid_auto);
                        })
                        .child(
                            text(if is_pending {
                                i18n::t(TextKey::AudioGeneratingSubtitles)
                            } else {
                                i18n::t(TextKey::AudioAutoSubtitles)
                            })
                            .font(theme::BADGE_TEXT)
                            .color(accent),
                        ),
                )
                .child(
                    el()
                        .key("btn_insp_load_subtitles")
                        .px(8.0)
                        .py(4.5)
                        .radius(5.0)
                        .fill(theme::bg_control())
                        .border(1.0, theme::border_subtle())
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press_tx(move |_: &mut AppState, tx: &mut Tx| {
                            let opts = FileDialogOptions {
                                title: "Load Subtitle File".into(),
                                ..Default::default()
                            }
                            .filter("Subtitles", &["srt", "vtt", "txt"]);
                            let target_id = nid_load.clone();
                            tx.open_file(opts, move |s: &mut AppState, res, _| {
                                if let Ok(Some(path)) = res {
                                    s.ingest_subtitle_file_into_node(&target_id, &path);
                                }
                            });
                        })
                        .child(
                            text(i18n::t(TextKey::AudioLoadSubtitles))
                                .font(theme::BADGE_TEXT)
                                .color(theme::text_secondary()),
                        ),
                )
                .child(el().grow(1.0)),
        );

    if let Some(err) = err_opt {
        box_col = box_col.child(
            text(err)
                .font(theme::BADGE_TEXT)
                .color(theme::accent_danger())
                .w_full()
                .wrap(),
        );
    }

    if has_any {
        let plain_copied = s.is_recently_copied("audio_sub_insp");
        let timed_copied = s.is_recently_copied("audio_sub_timed_insp");
        let plain_payload = plain_text.clone();
        let mut copy_row = row()
            .w_full()
            .center_y()
            .gap(6.0)
            .child(
                el()
                    .key("btn_insp_copy_subtitles")
                    .px(8.0)
                    .py(3.5)
                    .radius(5.0)
                    .fill(theme::bg_control())
                    .border(1.0, theme::border_subtle())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                        tx.copy(plain_payload.clone());
                        s.mark_copied("audio_sub_insp", "Copied subtitle text");
                    })
                    .child(
                        text(if plain_copied {
                            i18n::t(TextKey::ReaderCopied)
                        } else {
                            i18n::t(TextKey::AudioCopyText)
                        })
                        .font(theme::BADGE_TEXT)
                        .color(if plain_copied {
                            accent
                        } else {
                            theme::text_primary()
                        }),
                    ),
            );
        if has_cues && !timed_text.is_empty() {
            let timed_payload = timed_text.clone();
            copy_row = copy_row.child(
                el()
                    .key("btn_insp_copy_subtitles_timed")
                    .px(8.0)
                    .py(3.5)
                    .radius(5.0)
                    .fill(theme::bg_control())
                    .border(1.0, theme::border_subtle())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                        tx.copy(timed_payload.clone());
                        s.mark_copied("audio_sub_timed_insp", "Copied timestamped subtitles");
                    })
                    .child(
                        text(if timed_copied {
                            i18n::t(TextKey::ReaderCopied)
                        } else {
                            i18n::t(TextKey::AudioCopyTimed)
                        })
                        .font(theme::BADGE_TEXT)
                        .color(if timed_copied {
                            accent
                        } else {
                            theme::text_secondary()
                        }),
                    ),
            );
        }
        box_col = box_col.child(copy_row);

        let mut list_col = col()
            .w_full()
            .max_h(180.0)
            .gap(4.0)
            .scroll(ScrollSpec { stick_bottom: false });
        if has_cues {
            for (idx, cue) in node.meta.subtitles.iter().enumerate() {
                let is_cur = active_idx == Some(idx);
                let nid_seek = node.id.clone();
                let secs = cue.start;
                list_col = list_col.child(
                    row()
                        .key(format!("insp_sub_cue_{}", idx))
                        .w_full()
                        .px(6.0)
                        .py(4.0)
                        .gap(6.0)
                        .radius(4.0)
                        .fill(if is_cur {
                            accent.alpha(0.18)
                        } else {
                            theme::bg_input()
                        })
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.seek_media(&nid_seek, secs);
                        })
                        .child(
                            text(media_helper::format_timestamp(cue.start))
                                .font(theme::BADGE_TEXT)
                                .color(accent),
                        )
                        .child(
                            text(&cue.text)
                                .font(theme::STATUS_TEXT)
                                .color(if is_cur {
                                    theme::text_primary()
                                } else {
                                    theme::text_secondary()
                                })
                                .grow(1.0)
                                .wrap(),
                        ),
                );
            }
        } else {
            list_col = list_col.child(
                text(plain_text)
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_secondary())
                    .w_full()
                    .wrap()
                    .selectable(true),
            );
        }
        box_col = box_col.child(list_col);
    }

    box_col
}

fn render_calendar_inspector(s: &AppState, node: &Node) -> El {
    let accent = theme::kind_accent(NodeKind::Calendar);
    let (today_y, today_m, today_d) = media_helper::today_ymd();
    let today_str = media_helper::format_ymd(today_y, today_m, today_d);
    let cy = node.meta.calendar_year.unwrap_or(today_y);
    let cm = node.meta.calendar_month.unwrap_or(today_m);
    let selected_date = s
        .selected_calendar_date
        .get(&node.id)
        .cloned()
        .unwrap_or_else(|| today_str.clone());
    let mode = node.meta.calendar_view_mode;

    let mode_m_id = node.id.clone();
    let mode_w_id = node.id.clone();
    let prev_id = node.id.clone();
    let next_id = node.id.clone();
    let wk_prev_id = node.id.clone();
    let wk_next_id = node.id.clone();
    let day_note_id = node.id.clone();
    let day_note_sub_id = node.id.clone();
    let add_submit_id = node.id.clone();
    let add_btn_id = node.id.clone();

    let mut sec = col()
        .key("calendar_inspector_section")
        .w_full()
        .gap(10.0)
        .child(
            row()
                .padding(2.0)
                .gap(3.0)
                .radius(5.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_subtle())
                .child(
                    el()
                        .key("btn_insp_cal_mode_month")
                        .px(9.0)
                        .py(4.0)
                        .radius(4.0)
                        .fill(if mode == CalendarViewMode::Month {
                            theme::bg_control()
                        } else {
                            Color::TRANSPARENT
                        })
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.set_calendar_view_mode(&mode_m_id, CalendarViewMode::Month);
                        })
                        .child(
                            text(i18n::t(TextKey::CalModeMonth))
                                .font(theme::BADGE_TEXT)
                                .color(if mode == CalendarViewMode::Month {
                                    theme::text_primary()
                                } else {
                                    theme::text_muted()
                                }),
                        ),
                )
                .child(
                    el()
                        .key("btn_insp_cal_mode_week")
                        .px(9.0)
                        .py(4.0)
                        .radius(4.0)
                        .fill(if mode == CalendarViewMode::Week {
                            theme::bg_control()
                        } else {
                            Color::TRANSPARENT
                        })
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.set_calendar_view_mode(&mode_w_id, CalendarViewMode::Week);
                        })
                        .child(
                            text(i18n::t(TextKey::CalModeWeek))
                                .font(theme::BADGE_TEXT)
                                .color(if mode == CalendarViewMode::Week {
                                    theme::text_primary()
                                } else {
                                    theme::text_muted()
                                }),
                        ),
                ),
        );

    if mode == CalendarViewMode::Week {
        let anchor_mon = node
            .meta
            .calendar_week_start
            .clone()
            .unwrap_or_else(|| media_helper::monday_of_week(&selected_date));
        let week = media_helper::week_dates(&anchor_mon);
        let (sy, sm, sd) = media_helper::parse_ymd(&week[0].0).unwrap_or((cy, cm, 1));
        let (ey, em, ed) = media_helper::parse_ymd(&week[6].0).unwrap_or((sy, sm, sd));
        let range_label = format!(
            "{} {} – {} {}, {}",
            i18n::month_short(sm),
            sd,
            i18n::month_short(em),
            ed,
            ey
        );

        sec = sec.child(
            row()
                .w_full()
                .center_y()
                .gap(6.0)
                .child(
                    el()
                        .key("btn_insp_cal_week_prev")
                        .px(8.0)
                        .py(4.0)
                        .radius(5.0)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.calendar_step_week(&wk_prev_id, -1);
                        })
                        .child(icons::ph(&icons::CARET_LEFT, 13.0, theme::text_primary())),
                )
                .child(el().grow(1.0).center().child(
                    text(range_label)
                        .font(theme::BODY_TEXT)
                        .color(theme::text_primary()),
                ))
                .child(
                    el()
                        .key("btn_insp_cal_week_next")
                        .px(8.0)
                        .py(4.0)
                        .radius(5.0)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.calendar_step_week(&wk_next_id, 1);
                        })
                        .child(icons::ph(&icons::CARET_RIGHT, 13.0, theme::text_primary())),
                ),
        );

        let mut days_col = col().w_full().gap(4.0);
        for (wd_idx, (date_str, _wd_name, day_num)) in week.into_iter().enumerate() {
            let is_sel = selected_date == date_str;
            let day_note = node
                .meta
                .calendar_day_notes
                .get(&date_str)
                .map(String::as_str)
                .unwrap_or("")
                .trim();
            let evt_cnt = node
                .meta
                .calendar_events
                .iter()
                .filter(|e| e.date == date_str)
                .count();
            let nid = node.id.clone();
            let dp = date_str.clone();
            days_col = days_col.child(
                row()
                    .key(format!("insp_cal_week_day_{}", date_str))
                    .w_full()
                    .px(8.0)
                    .py(6.0)
                    .gap(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(if is_sel {
                        theme::bg_control()
                    } else {
                        theme::bg_input()
                    })
                    .border(
                        1.0,
                        if is_sel {
                            accent
                        } else {
                            theme::border_subtle()
                        },
                    )
                    .cursor(Cursor::Pointer)
                    .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                        s.calendar_select_date(&nid, dp.clone());
                        tx.focus_when_shown("input_cal_day_note", false);
                    })
                    .child(
                        text(format!("{} {:02}", i18n::weekday_short(wd_idx), day_num))
                            .font(theme::BADGE_TEXT)
                            .color(if is_sel {
                                accent
                            } else {
                                theme::text_primary()
                            }),
                    )
                    .child(
                        text(if !day_note.is_empty() {
                            day_note
                        } else {
                            ""
                        })
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_secondary())
                        .grow(1.0)
                        .truncate(),
                    )
                    .child(
                        text(if evt_cnt > 0 {
                            format!("{} evt", evt_cnt)
                        } else {
                            String::new()
                        })
                        .font(theme::BADGE_TEXT)
                        .color(accent),
                    ),
            );
        }
        sec = sec.child(days_col);
    } else {
        sec = sec.child(
            row()
                .w_full()
                .center_y()
                .gap(6.0)
                .child(
                    el()
                        .key("btn_insp_cal_prev")
                        .px(8.0)
                        .py(4.0)
                        .radius(5.0)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.calendar_step_month(&prev_id, -1);
                        })
                        .child(icons::ph(&icons::CARET_LEFT, 13.0, theme::text_primary())),
                )
                .child(el().grow(1.0).center().child(
                    text(format!("{} {}", i18n::month_full(cm), cy))
                        .font(theme::BODY_TEXT)
                        .color(theme::text_primary()),
                ))
                .child(
                    el()
                        .key("btn_insp_cal_next")
                        .px(8.0)
                        .py(4.0)
                        .radius(5.0)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.calendar_step_month(&next_id, 1);
                        })
                        .child(icons::ph(&icons::CARET_RIGHT, 13.0, theme::text_primary())),
                ),
        );

        // Weekday row
        let mut wk_row = row().w_full().gap(3.0);
        for wd_idx in 0..7 {
            wk_row = wk_row.child(
                el().grow(1.0).center().child(
                    text(i18n::weekday_narrow(wd_idx))
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_muted()),
                ),
            );
        }
        sec = sec.child(wk_row);

        // Month grid
        let offset = media_helper::weekday_of_first(cy, cm);
        let dim = media_helper::days_in_month(cy, cm);
        let num_rows = (offset + dim + 6) / 7;
        let mut grid = col().w_full().gap(3.0);
        for r in 0..num_rows {
            let mut r_el = row().w_full().gap(3.0);
            for c in 0..7 {
                let slot = r * 7 + c;
                if slot < offset || slot >= offset + dim {
                    r_el = r_el.child(el().grow(1.0).h(26.0));
                } else {
                    let day = slot - offset + 1;
                    let date_str = media_helper::format_ymd(cy, cm, day);
                    let has_evt = node.meta.calendar_events.iter().any(|e| e.date == date_str);
                    let has_note = node
                        .meta
                        .calendar_day_notes
                        .get(&date_str)
                        .is_some_and(|t| !t.trim().is_empty());
                    let has_mark = has_evt || has_note;
                    let is_sel = selected_date == date_str;
                    let is_today = today_str == date_str;
                    let nid = node.id.clone();
                    let dp = date_str.clone();
                    r_el = r_el.child(
                        el()
                            .key(format!("insp_cal_day_{}", date_str))
                            .grow(1.0)
                            .h(26.0)
                            .radius(5.0)
                            .fill(if is_sel {
                                accent.alpha(0.24)
                            } else if has_mark {
                                accent.alpha(0.1)
                            } else {
                                theme::bg_card()
                            })
                            .border(
                                1.0,
                                if is_sel {
                                    accent
                                } else if is_today {
                                    theme::border_default()
                                } else {
                                    theme::border_subtle()
                                },
                            )
                            .center()
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.calendar_select_date(&nid, dp.clone());
                            })
                            .child(
                                text(if has_mark {
                                    format!("{}•", day)
                                } else {
                                    format!("{}", day)
                                })
                                .font(theme::BADGE_TEXT)
                                .color(if is_sel || has_mark {
                                    accent
                                } else {
                                    theme::text_secondary()
                                }),
                            ),
                    );
                }
            }
            grid = grid.child(r_el);
        }
        sec = sec.child(grid);
    }

    // Daily Note editor for selected_date (available in both Month and Week mode)
    let mut day_note_hdr = row()
        .w_full()
        .center_y()
        .child(subtle_tag(&selected_date, accent))
        .child(el().grow(1.0));
    if !s.cal_day_note_edit.text.trim().is_empty() {
        let clr_id = node.id.clone();
        let clr_date = selected_date.clone();
        day_note_hdr = day_note_hdr.child(
            el()
                .key("btn_clear_cal_day_note")
                .px(6.0)
                .py(2.0)
                .radius(4.0)
                .fill(theme::bg_control())
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.set_calendar_day_note(&clr_id, clr_date.clone(), String::new());
                })
                .child(icons::ph(&icons::X, 11.0, theme::text_muted())),
        );
    }
    sec = sec.child(
        col()
            .w_full()
            .padding(8.0)
            .gap(6.0)
            .radius(6.0)
            .fill(theme::bg_input())
            .border(1.0, accent.alpha(0.35))
            .child(day_note_hdr)
            .child(
                text_input(&s.cal_day_note_edit, theme::BODY_TEXT)
                    .key("input_cal_day_note")
                    .label("Daily calendar note")
                    .w_full()
                    .min_h(52.0)
                    .color(theme::text_primary())
                    .caret(accent, accent.alpha(0.25))
                    .placeholder(i18n::t(TextKey::CalDayNotePlaceholder), theme::text_muted())
                    .on_edit(|s: &mut AppState| &mut s.cal_day_note_edit)
                    .on_input_change(move |s: &mut AppState, _| {
                        s.sync_calendar_day_note_edit(&day_note_id);
                    })
                    .on_submit(move |s: &mut AppState, _| {
                        s.cal_day_note_edit.insert("\n");
                        s.sync_calendar_day_note_edit(&day_note_sub_id);
                    }),
            ),
    );

    // Add Event form
    sec = sec
        .child(
            row()
                .w_full()
                .h(32.0)
                .gap(6.0)
                .center_y()
                .child(
                    el()
                        .grow(1.0)
                        .h_full()
                        .px(8.0)
                        .center_y()
                        .radius(5.0)
                        .fill(theme::bg_input())
                        .border(1.0, theme::border_subtle())
                        .child(
                            text_input(&s.cal_event_date_edit, theme::STATUS_TEXT)
                                .key("input_cal_date")
                                .label("Event date")
                                .single_line()
                                .w_full()
                                .color(theme::text_primary())
                                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                                .placeholder("YYYY-MM-DD", theme::text_muted())
                                .on_edit(|s: &mut AppState| &mut s.cal_event_date_edit),
                        ),
                )
                .child(
                    el()
                        .w(76.0)
                        .h_full()
                        .px(8.0)
                        .center_y()
                        .radius(5.0)
                        .fill(theme::bg_input())
                        .border(1.0, theme::border_subtle())
                        .child(
                            text_input(&s.cal_event_time_edit, theme::STATUS_TEXT)
                                .key("input_cal_time")
                                .label("Event time")
                                .single_line()
                                .w_full()
                                .color(theme::text_primary())
                                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                                .placeholder("14:00", theme::text_muted())
                                .on_edit(|s: &mut AppState| &mut s.cal_event_time_edit),
                        ),
                ),
        )
        .child(
            row()
                .w_full()
                .h(32.0)
                .gap(6.0)
                .center_y()
                .child(
                    el()
                        .grow(1.0)
                        .h_full()
                        .px(8.0)
                        .center_y()
                        .radius(5.0)
                        .fill(theme::bg_input())
                        .border(1.0, theme::border_subtle())
                        .child(
                            text_input(&s.cal_event_title_edit, theme::STATUS_TEXT)
                                .key("input_cal_title")
                                .label("Event title")
                                .single_line()
                                .w_full()
                                .color(theme::text_primary())
                                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                                .placeholder("Event title...", theme::text_muted())
                                .on_edit(|s: &mut AppState| &mut s.cal_event_title_edit)
                                .on_submit(move |s: &mut AppState, _| {
                                    let d = s.cal_event_date_edit.text.clone();
                                    let tm = if s.cal_event_time_edit.text.trim().is_empty() {
                                        None
                                    } else {
                                        Some(s.cal_event_time_edit.text.clone())
                                    };
                                    let title = s.cal_event_title_edit.text.clone();
                                    let notes = s.cal_event_notes_edit.text.clone();
                                    s.add_calendar_event(&add_submit_id, d, tm, title, notes);
                                    s.cal_event_time_edit = rune::TextEdit::new("");
                                    s.cal_event_title_edit = rune::TextEdit::new("");
                                    s.cal_event_notes_edit = rune::TextEdit::new("");
                                }),
                        ),
                )
                .child(
                    el()
                        .key("btn_add_cal_event")
                        .h_full()
                        .px(10.0)
                        .center()
                        .radius(5.0)
                        .fill(accent.alpha(0.2))
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            let d = s.cal_event_date_edit.text.clone();
                            let tm = if s.cal_event_time_edit.text.trim().is_empty() {
                                None
                            } else {
                                Some(s.cal_event_time_edit.text.clone())
                            };
                            let title = s.cal_event_title_edit.text.clone();
                            let notes = s.cal_event_notes_edit.text.clone();
                            s.add_calendar_event(&add_btn_id, d, tm, title, notes);
                            s.cal_event_time_edit = rune::TextEdit::new("");
                            s.cal_event_title_edit = rune::TextEdit::new("");
                            s.cal_event_notes_edit = rune::TextEdit::new("");
                        })
                        .child(
                            text(i18n::t(TextKey::CalAddEvent))
                                .font(theme::BADGE_TEXT)
                                .color(accent),
                        ),
                ),
        );

    // Event list
    for (idx, ev) in node.meta.calendar_events.iter().enumerate() {
        let nid_t = node.id.clone();
        let nid_d = node.id.clone();
        let ev_id_t = ev.id.clone();
        let ev_id_d = ev.id.clone();
        let tm_str = ev
            .time
            .as_deref()
            .filter(|t| !t.is_empty())
            .map(|t| format!("{} ", t))
            .unwrap_or_default();
        sec = sec.child(
            row()
                .w_full()
                .center_y()
                .gap(6.0)
                .px(8.0)
                .py(5.0)
                .radius(5.0)
                .fill(if ev.date == selected_date {
                    accent.alpha(0.1)
                } else {
                    theme::bg_card()
                })
                .border(
                    1.0,
                    if ev.date == selected_date {
                        accent.alpha(0.35)
                    } else {
                        theme::border_subtle()
                    },
                )
                .child(
                    el()
                        .key(format!("btn_insp_cal_toggle_{}", idx))
                        .px(4.0)
                        .py(2.0)
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.toggle_calendar_event(&nid_t, &ev_id_t);
                        })
                        .child(icons::ph(
                            if ev.completed {
                                &icons::CHECK_SQUARE
                            } else {
                                &icons::SQUARE
                            },
                            13.0,
                            if ev.completed {
                                theme::accent_claude()
                            } else {
                                accent
                            },
                        )),
                )
                .child(
                    text(format!("{} {}{}", ev.date, tm_str, ev.title))
                        .font(theme::STATUS_TEXT)
                        .color(if ev.completed {
                            theme::text_muted()
                        } else {
                            theme::text_primary()
                        })
                        .grow(1.0)
                        .truncate(),
                )
                .child(
                    el()
                        .key(format!("btn_insp_cal_del_{}", idx))
                        .px(6.0)
                        .py(2.0)
                        .radius(4.0)
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.remove_calendar_event(&nid_d, &ev_id_d);
                        })
                        .child(icons::ph(&icons::X, 11.5, theme::text_muted())),
                ),
        );
    }

    sec
}
