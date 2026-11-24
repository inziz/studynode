use rune::{
    Color, Cursor, Cx, El, ScrollSpec, TextEdit, TextStyle, Tx, col, el, icon, row, text,
    text_input,
};

use crate::doc_reader::DockPosition;
use crate::i18n::{self, TextKey, UiLocale};
use crate::icons;
use crate::state::AppState;
use crate::theme::{self, ThemeId};

const CARD_TITLE: TextStyle = TextStyle::new(13.5, 600.0);

fn settings_field(
    label: &'static str,
    key: &'static str,
    state: &TextEdit,
    placeholder_text: &'static str,
    on_edit_fn: impl Fn(&mut AppState) -> &mut TextEdit + 'static,
    trailing_action: Option<El>,
) -> El {
    let mut header = row()
        .w_full()
        .center_y()
        .child(
            text(label)
                .font(theme::STATUS_TEXT)
                .color(theme::text_secondary()),
        )
        .child(el().grow(1.0));

    if let Some(action) = trailing_action {
        header = header.child(action);
    }

    col()
        .w_full()
        .gap(5.0)
        .child(header)
        .child(
            el()
                .w_full()
                .px(10.0)
                .py(7.0)
                .radius(7.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_subtle())
                .child(
                    text_input(state, theme::STATUS_TEXT)
                        .key(key)
                        .label(label)
                        .single_line()
                        .w_full()
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder(placeholder_text, theme::text_muted())
                        .on_edit(on_edit_fn)
                        .on_input_change(|s: &mut AppState, _| {
                            s.save_all_api_settings();
                        })
                        .on_submit(|s: &mut AppState, _| {
                            s.save_all_api_settings();
                            s.notify_toast(i18n::t(TextKey::SettingsSavedToast));
                        }),
                ),
        )
}

fn section_card(
    key: &'static str,
    title: &'static str,
    subtitle: &'static str,
    badge_text: String,
    badge_active: bool,
) -> El {
    let badge_color = if badge_active {
        theme::accent_claude()
    } else {
        theme::text_muted()
    };

    col()
        .key(key)
        .w_full()
        .gap(12.0)
        .padding(16.0)
        .radius(10.0)
        .continuous_corners()
        .fill(theme::bg_card())
        .border(1.0, theme::border_subtle())
        .child(
            row()
                .w_full()
                .gap(10.0)
                .center_y()
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(2.0)
                        .child(
                            text(title)
                                .font(CARD_TITLE)
                                .color(theme::text_primary()),
                        )
                        .child(
                            text(subtitle)
                                .font(theme::STATUS_TEXT)
                                .color(theme::text_muted())
                                .wrap(),
                        ),
                )
                .child(
                    el()
                        .px(8.0)
                        .py(3.5)
                        .radius(6.0)
                        .fill(if badge_active {
                            theme::accent_claude().alpha(0.14)
                        } else {
                            theme::hover_subtle()
                        })
                        .border(
                            1.0,
                            if badge_active {
                                theme::accent_claude().alpha(0.35)
                            } else {
                                theme::border_subtle()
                            },
                        )
                        .child(text(badge_text).font(theme::BADGE_TEXT).color(badge_color)),
                ),
        )
}

pub fn render_settings(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let insets = cx.chrome_insets();

    let mut topbar = row()
        .key("settings_topbar")
        .w_full()
        .h(36.0)
        .pl(insets.leading + 8.0)
        .pr(insets.trailing + 12.0)
        .gap(8.0)
        .center_y()
        .fill(theme::bg_canvas())
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
        .window_drag()
        .child(
            el()
                .key("btn_settings_back")
                .size(26.0, 26.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press(|s: &mut AppState| {
                    s.close_settings();
                })
                .child(icons::ph(
                    &icons::ARROW_LEFT,
                    15.0,
                    theme::text_secondary(),
                )),
        )
        .child(icon(&theme::STUDYNODES_LOGO_ICON, 17.0).color(theme::accent_claude()))
        .child(
            text(i18n::t(TextKey::SettingsTitle))
                .font(theme::PANEL_TITLE)
                .color(theme::text_primary()),
        )
        .child(el().grow(1.0));

    if let Some(toast) = &s.toast_text {
        topbar = topbar.child(
            text(toast)
                .font(theme::STATUS_TEXT)
                .color(theme::text_secondary()),
        );
    }

    topbar = topbar.child(
        el()
            .key("btn_settings_save")
            .h(26.0)
            .px(12.0)
            .radius(6.0)
            .fill(theme::accent_claude())
            .hover(theme::accent_claude().alpha(0.88))
            .press(Color::BLACK.alpha(0.18))
            .center()
            .cursor(Cursor::Pointer)
            .no_window_drag()
            .on_press(|s: &mut AppState| {
                s.save_all_api_settings();
                s.notify_toast(i18n::t(TextKey::SettingsSavedToast));
            })
            .child(
                text(i18n::t(TextKey::SettingsSave))
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_inverted()),
            ),
    );

    let ai_clear_btn = if !s.ai_api_key_edit.text.trim().is_empty() {
        Some(
            el()
                .key("btn_clear_ai_api_key")
                .px(6.0)
                .py(2.0)
                .radius(4.0)
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.ai_api_key_edit = TextEdit::new("");
                    s.save_all_api_settings();
                })
                .child(
                    text(i18n::t(TextKey::ReaderClear))
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_muted()),
                ),
        )
    } else {
        None
    };

    let follow_ui_lang = s.reader_prefs.ai_summary_follow_ui_language;
    let summary_lang_label = if follow_ui_lang {
        i18n::fmt_summary_lang_ui(s.reader_prefs.ui_locale)
    } else {
        i18n::t(TextKey::SettingsAiSummaryLangSource).to_string()
    };

    let ai_summary_lang_btn = el()
        .key("btn_toggle_ai_summary_lang")
        .w_full()
        .px(10.0)
        .py(7.0)
        .radius(7.0)
        .fill(if follow_ui_lang {
            theme::accent_claude().alpha(0.12)
        } else {
            theme::bg_input()
        })
        .hover(theme::hover_subtle())
        .border(
            1.0,
            if follow_ui_lang {
                theme::accent_claude().alpha(0.55)
            } else {
                theme::border_subtle()
            },
        )
        .cursor(Cursor::Pointer)
        .on_press(|s: &mut AppState| {
            s.toggle_ai_summary_language_mode();
        })
        .child(
            text(summary_lang_label)
                .font(theme::STATUS_TEXT)
                .color(if follow_ui_lang {
                    theme::accent_claude()
                } else {
                    theme::text_primary()
                }),
        );

    let mut preset_col = col().w_full().gap(5.0);
    let cur_base = s.ai_base_url_edit.text.trim();
    for chunk in studynodes_llm::API_PROVIDER_PRESETS.chunks(5) {
        let mut r = row().w_full().gap(5.0);
        for preset in chunk {
            let is_selected = if preset.id == "auto" {
                cur_base.is_empty()
            } else if preset.base_url.is_empty() {
                cur_base.is_empty() && s.ai_model_edit.text.trim() == preset.default_model
            } else {
                cur_base.eq_ignore_ascii_case(preset.base_url)
            };
            let pid = preset.id;
            r = r.child(
                el()
                    .key(format!("btn_ai_preset_{}", pid))
                    .grow(1.0)
                    .min_w(0.0)
                    .px(8.0)
                    .py(5.0)
                    .radius(6.0)
                    .fill(if is_selected {
                        theme::accent_claude().alpha(0.14)
                    } else {
                        theme::bg_input()
                    })
                    .hover(theme::hover_subtle())
                    .border(
                        if is_selected { 1.5 } else { 1.0 },
                        if is_selected {
                            theme::accent_claude()
                        } else {
                            theme::border_subtle()
                        },
                    )
                    .center()
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.apply_ai_provider_preset(pid);
                    })
                    .child(
                        text(preset.name)
                            .font(theme::BADGE_TEXT)
                            .color(if is_selected {
                                theme::accent_claude()
                            } else {
                                theme::text_secondary()
                            })
                            .truncate(),
                    ),
            );
        }
        preset_col = preset_col.child(r);
    }

    let ai_section = section_card(
        "settings_ai_section",
        i18n::t(TextKey::SettingsAiSectionTitle),
        i18n::t(TextKey::SettingsAiSectionSubtitle),
        s.ai_provider_status_label(),
        s.ai_api_configured(),
    )
    .child(preset_col)
    .child(settings_field(
        i18n::t(TextKey::SettingsFieldApiKey),
        "input_ai_api_key",
        &s.ai_api_key_edit,
        "sk-air-... / sk-or-... / gsk_... / sk-ant-... / AIza... / sk-... (or blank for local)",
        |s| &mut s.ai_api_key_edit,
        ai_clear_btn,
    ))
    .child(settings_field(
        i18n::t(TextKey::SettingsFieldBaseUrl),
        "input_ai_base_url",
        &s.ai_base_url_edit,
        "Optional — blank = auto-detect key, or e.g. https://api.airforce/v1",
        |s| &mut s.ai_base_url_edit,
        None,
    ))
    .child(settings_field(
        i18n::t(TextKey::SettingsFieldModel),
        "input_ai_model",
        &s.ai_model_edit,
        "Optional — e.g. gpt-4o-mini, claude-sonnet-4-20250514, gemini-2.5-flash",
        |s| &mut s.ai_model_edit,
        None,
    ))
    .child(ai_summary_lang_btn);

    let chat_clear_btn = if !s.chat_api_key_edit.text.trim().is_empty() {
        Some(
            el()
                .key("btn_clear_chat_api_key")
                .px(6.0)
                .py(2.0)
                .radius(4.0)
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.chat_api_key_edit = TextEdit::new("");
                    s.save_all_api_settings();
                })
                .child(
                    text(i18n::t(TextKey::ReaderClear))
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_muted()),
                ),
        )
    } else {
        None
    };

    let chat_ai_section = section_card(
        "settings_chat_ai_section",
        i18n::t(TextKey::SettingsChatAiSectionTitle),
        i18n::t(TextKey::SettingsChatAiSectionSubtitle),
        s.chat_provider_status_label(),
        s.chat_api_configured(),
    )
    .child(settings_field(
        i18n::t(TextKey::SettingsFieldApiKey),
        "input_chat_api_key",
        &s.chat_api_key_edit,
        "Dedicated Chat API key — AIza... or sk-... (blank = use AI Summaries key)",
        |s| &mut s.chat_api_key_edit,
        chat_clear_btn,
    ))
    .child(settings_field(
        i18n::t(TextKey::SettingsFieldBaseUrl),
        "input_chat_base_url",
        &s.chat_base_url_edit,
        "Optional — blank for Gemini, or https://api.openai.com/v1",
        |s| &mut s.chat_base_url_edit,
        None,
    ))
    .child(settings_field(
        i18n::t(TextKey::SettingsFieldModel),
        "input_chat_model",
        &s.chat_model_edit,
        "Optional — gemini-2.5-pro, claude-sonnet-4, or gpt-4o",
        |s| &mut s.chat_model_edit,
        None,
    ));

    let vision_configured = s.cloud_vision_configured();
    let vision_clear_btn = if !s.vision_api_key_edit.text.trim().is_empty() {
        Some(
            el()
                .key("btn_clear_vision_api_key")
                .px(6.0)
                .py(2.0)
                .radius(4.0)
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.vision_api_key_edit = TextEdit::new("");
                    s.save_all_api_settings();
                })
                .child(
                    text(i18n::t(TextKey::ReaderClear))
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_muted()),
                ),
        )
    } else {
        None
    };

    let vision_section = section_card(
        "settings_vision_section",
        i18n::t(TextKey::SettingsVisionSectionTitle),
        i18n::t(TextKey::SettingsVisionSectionSubtitle),
        if vision_configured {
            i18n::t(TextKey::SettingsReady).to_string()
        } else {
            i18n::t(TextKey::SettingsVisionNotSet).to_string()
        },
        vision_configured,
    )
    .child(settings_field(
        i18n::t(TextKey::SettingsFieldApiKey),
        "input_settings_vision_api_key",
        &s.vision_api_key_edit,
        "Gemini / DeepInfra / SiliconFlow / OpenAI key",
        |s| &mut s.vision_api_key_edit,
        vision_clear_btn,
    ))
    .child(settings_field(
        i18n::t(TextKey::SettingsFieldBaseUrl),
        "input_settings_vision_base_url",
        &s.vision_base_url_edit,
        "Optional — blank for Gemini, or https://.../v1",
        |s| &mut s.vision_base_url_edit,
        None,
    ))
    .child(settings_field(
        i18n::t(TextKey::SettingsFieldModel),
        "input_settings_vision_model",
        &s.vision_model_edit,
        "Optional — gemini-2.5-flash or deepseek-ai/DeepSeek-OCR",
        |s| &mut s.vision_model_edit,
        None,
    ));

    let ollama_section = section_card(
        "settings_ollama_section",
        i18n::t(TextKey::SettingsOllamaSectionTitle),
        i18n::t(TextKey::SettingsOllamaSectionSubtitle),
        i18n::t(TextKey::SettingsLocalHttp).to_string(),
        false,
    )
    .child(settings_field(
        i18n::t(TextKey::SettingsFieldServerUrl),
        "input_ollama_url",
        &s.ollama_url_edit,
        "http://127.0.0.1:11434",
        |s| &mut s.ollama_url_edit,
        None,
    ))
    .child(settings_field(
        i18n::t(TextKey::SettingsFieldModel),
        "input_ollama_model",
        &s.ollama_model_edit,
        "qwen2.5:3b",
        |s| &mut s.ollama_model_edit,
        None,
    ));

    let mut locale_row = row().w_full().gap(6.0);
    for locale in UiLocale::all() {
        let is_selected = s.reader_prefs.ui_locale == locale;
        locale_row = locale_row.child(
            el()
                .key(format!("btn_locale_{}", locale.key_suffix()))
                .grow(1.0)
                .min_w(0.0)
                .px(8.0)
                .py(6.5)
                .radius(7.0)
                .fill(if is_selected {
                    theme::accent_claude().alpha(0.14)
                } else {
                    theme::bg_input()
                })
                .hover(theme::hover_subtle())
                .border(
                    if is_selected { 1.5 } else { 1.0 },
                    if is_selected {
                        theme::accent_claude()
                    } else {
                        theme::border_subtle()
                    },
                )
                .center()
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.set_ui_locale(locale);
                })
                .child(
                    text(locale.native_name())
                        .font(theme::STATUS_TEXT)
                        .color(if is_selected {
                            theme::text_primary()
                        } else {
                            theme::text_secondary()
                        })
                        .truncate(),
                ),
        );
    }

    let mut theme_row = row().w_full().gap(8.0);
    for theme_id in ThemeId::all() {
        let is_selected = s.reader_prefs.theme == theme_id;
        let (swatch_bg, swatch_card, swatch_accent) = theme_id.preview_colors();
        theme_row = theme_row.child(
            col()
                .key(format!("btn_theme_{}", theme_id.key_suffix()))
                .grow(1.0)
                .min_w(0.0)
                .gap(7.0)
                .padding(10.0)
                .radius(8.0)
                .fill(if is_selected {
                    theme::accent_claude().alpha(0.12)
                } else {
                    theme::bg_input()
                })
                .hover(theme::hover_subtle())
                .border(
                    if is_selected { 1.5 } else { 1.0 },
                    if is_selected {
                        theme::accent_claude()
                    } else {
                        theme::border_subtle()
                    },
                )
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.set_theme(theme_id);
                })
                .child(
                    row()
                        .gap(5.0)
                        .center_y()
                        .child(
                            el()
                                .size(12.0, 12.0)
                                .radius(6.0)
                                .fill(swatch_bg)
                                .border(1.0, theme::border_default()),
                        )
                        .child(
                            el()
                                .size(12.0, 12.0)
                                .radius(6.0)
                                .fill(swatch_card)
                                .border(1.0, theme::border_default()),
                        )
                        .child(el().size(12.0, 12.0).radius(6.0).fill(swatch_accent)),
                )
                .child(
                    text(theme_id.display_name())
                        .font(theme::STATUS_TEXT)
                        .color(if is_selected {
                            theme::text_primary()
                        } else {
                            theme::text_secondary()
                        })
                        .truncate(),
                ),
        );
    }

    let dm_active = s.reader_prefs.use_direct_manipulation;
    let wire_anim_active = s.reader_prefs.animate_wires;
    let trace_colors_active = s.reader_prefs.enable_trace_colors;
    let controls_row = row()
        .w_full()
        .gap(8.0)
        .child(
            el()
                .key("btn_toggle_touchpad_dm")
                .grow(1.0)
                .min_w(0.0)
                .px(10.0)
                .py(7.0)
                .radius(7.0)
                .fill(theme::bg_input())
                .hover(theme::hover_subtle())
                .border(1.0, theme::border_subtle())
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.toggle_direct_manipulation();
                })
                .child(
                    text(if dm_active {
                        i18n::t(TextKey::SettingsTrackpadDm)
                    } else {
                        i18n::t(TextKey::SettingsTrackpadWheel)
                    })
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_primary())
                    .truncate(),
                ),
        )
        .child(
            el()
                .key("btn_toggle_animate_wires")
                .px(10.0)
                .py(7.0)
                .radius(7.0)
                .fill(theme::bg_input())
                .hover(theme::hover_subtle())
                .border(1.0, theme::border_subtle())
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.toggle_animate_wires();
                })
                .child(
                    text(if wire_anim_active {
                        i18n::t(TextKey::SettingsWireAnimOn)
                    } else {
                        i18n::t(TextKey::SettingsWireAnimOff)
                    })
                    .font(theme::STATUS_TEXT)
                    .color(if wire_anim_active {
                        theme::text_primary()
                    } else {
                        theme::text_muted()
                    }),
                ),
        )
        .child(
            el()
                .key("btn_toggle_trace_colors")
                .px(10.0)
                .py(7.0)
                .radius(7.0)
                .fill(theme::bg_input())
                .hover(theme::hover_subtle())
                .border(1.0, theme::border_subtle())
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.toggle_trace_colors();
                })
                .child(
                    text(if trace_colors_active {
                        i18n::t(TextKey::SettingsTraceColorsOn)
                    } else {
                        i18n::t(TextKey::SettingsTraceColorsOff)
                    })
                    .font(theme::STATUS_TEXT)
                    .color(if trace_colors_active {
                        theme::text_primary()
                    } else {
                        theme::text_muted()
                    }),
                ),
        )
        .child(
            el()
                .key("btn_settings_fullscreen")
                .px(12.0)
                .py(7.0)
                .radius(7.0)
                .fill(theme::bg_input())
                .hover(theme::hover_subtle())
                .border(1.0, theme::border_subtle())
                .cursor(Cursor::Pointer)
                .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                    s.is_fullscreen = !s.is_fullscreen;
                    tx.toggle_full_screen();
                })
                .child(
                    text(if s.is_fullscreen {
                        i18n::t(TextKey::SettingsExitFullscreen)
                    } else {
                        i18n::t(TextKey::SettingsToggleFullscreen)
                    })
                    .font(theme::STATUS_TEXT)
                    .color(if s.is_fullscreen {
                        theme::accent_claude()
                    } else {
                        theme::text_primary()
                    }),
                ),
        );

    let cur_dock_pos = s.reader_prefs.dock_position;
    let mut dock_pos_pills = row().grow(1.0).gap(6.0);
    for pos in DockPosition::all() {
        let is_selected = pos == cur_dock_pos;
        dock_pos_pills = dock_pos_pills.child(
            el()
                .key(format!("btn_dock_pos_{}", pos.key_suffix()))
                .grow(1.0)
                .min_w(0.0)
                .px(8.0)
                .py(7.0)
                .radius(7.0)
                .fill(if is_selected {
                    theme::accent_claude().alpha(0.14)
                } else {
                    theme::bg_input()
                })
                .hover(theme::hover_subtle())
                .border(
                    if is_selected { 1.5 } else { 1.0 },
                    if is_selected {
                        theme::accent_claude()
                    } else {
                        theme::border_subtle()
                    },
                )
                .center()
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.set_dock_position(pos);
                })
                .child(
                    text(i18n::dock_position_label(pos))
                        .font(theme::BADGE_TEXT)
                        .color(if is_selected {
                            theme::text_primary()
                        } else {
                            theme::text_secondary()
                        })
                        .truncate(),
                ),
        );
    }

    let dock_pos_row = row()
        .w_full()
        .gap(10.0)
        .center_y()
        .child(
            el().w(128.0).child(
                text(i18n::t(TextKey::SettingsDockPosition))
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_secondary()),
            ),
        )
        .child(dock_pos_pills);

    let dock_auto_on = s.reader_prefs.dock_auto_orient;
    let dock_auto_row = row()
        .w_full()
        .gap(10.0)
        .center_y()
        .child(
            el().w(128.0).child(
                text(i18n::t(TextKey::SettingsDockAutoOrient))
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_secondary()),
            ),
        )
        .child(
            el()
                .key("btn_toggle_dock_auto_orient")
                .px(10.0)
                .py(7.0)
                .radius(7.0)
                .fill(theme::bg_input())
                .hover(theme::hover_subtle())
                .border(
                    1.0,
                    if dock_auto_on {
                        theme::accent_claude().alpha(0.55)
                    } else {
                        theme::border_subtle()
                    },
                )
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.toggle_dock_auto_orient();
                })
                .child(
                    row()
                        .center_y()
                        .gap(6.0)
                        .child(icons::ph(
                            &icons::MAGNET,
                            13.0,
                            if dock_auto_on {
                                theme::accent_claude()
                            } else {
                                theme::text_muted()
                            },
                        ))
                        .child(
                            text(if dock_auto_on {
                                i18n::t(TextKey::SettingsDockAutoOrientOn)
                            } else {
                                i18n::t(TextKey::SettingsDockAutoOrientOff)
                            })
                            .font(theme::STATUS_TEXT)
                            .color(if dock_auto_on {
                                theme::text_primary()
                            } else {
                                theme::text_muted()
                            }),
                        ),
                ),
        );

    let appearance_section = section_card(
        "settings_appearance_section",
        i18n::t(TextKey::SettingsAppearanceTitle),
        i18n::t(TextKey::SettingsAppearanceSubtitle),
        format!(
            "{} · {}",
            s.reader_prefs.ui_locale.short_label(),
            s.reader_prefs.theme.display_name(),
        ),
        true,
    )
    .child(locale_row)
    .child(theme_row)
    .child(dock_pos_row)
    .child(dock_auto_row)
    .child(controls_row);

    let body = col()
        .key("settings_scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .items_center()
        .px(24.0)
        .py(24.0)
        .child(
            col()
                .w_full()
                .max_w(620.0)
                .gap(14.0)
                .child(appearance_section)
                .child(ai_section)
                .child(chat_ai_section)
                .child(vision_section)
                .child(ollama_section)
                .child(
                    text(i18n::t(TextKey::SettingsStorageNote))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                ),
        );

    col()
        .key("settings_root")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(theme::bg_canvas())
        .child(topbar)
        .child(body)
}
