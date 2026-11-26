use rune::{
    Color, Cursor, Cx, El, Icon, ScrollSpec, TextEdit, TextStyle, Tx, col, el, icon, row, text,
    text_input,
};

use crate::doc_reader::DockPosition;
use crate::i18n::{self, TextKey, UiLocale};
use crate::icons;
use crate::state::{AppState, SettingsTab};
use crate::theme::{self, ThemeId};

const PAGE_TITLE: TextStyle = TextStyle::new(22.0, 600.0);
const SECTION_TITLE: TextStyle = TextStyle::new(15.5, 600.0);
const ROW_LABEL: TextStyle = TextStyle::new(13.0, 500.0);
const SUBTITLE_TEXT: TextStyle = TextStyle::new(12.0, 400.0).lh(17.5);
const NAV_TEXT_ACTIVE: TextStyle = TextStyle::new(13.0, 550.0);
const NAV_TEXT_IDLE: TextStyle = TextStyle::new(13.0, 450.0);

fn hairline() -> El {
    el().w_full().h(1.0).fill(theme::border_subtle())
}

fn status_badge(label: String, active: bool) -> El {
    let fg = if active {
        theme::accent_claude()
    } else {
        theme::text_muted()
    };
    el()
        .px(9.0)
        .py(3.5)
        .radius(999.0)
        .fill(if active {
            theme::accent_claude().alpha(0.13)
        } else {
            theme::hover_subtle()
        })
        .border(
            1.0,
            if active {
                theme::accent_claude().alpha(0.32)
            } else {
                theme::border_subtle()
            },
        )
        .child(text(label).font(theme::BADGE_TEXT).color(fg))
}

fn section_header(title: &'static str, subtitle: Option<&'static str>, badge: Option<(String, bool)>) -> El {
    let mut left = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(4.0)
        .child(text(title).font(SECTION_TITLE).color(theme::text_primary()));
    if let Some(sub) = subtitle {
        left = left.child(
            text(sub)
                .font(SUBTITLE_TEXT)
                .color(theme::text_muted())
                .wrap(),
        );
    }

    let mut hdr = row().w_full().gap(12.0).items_start().child(left);
    if let Some((b_text, b_active)) = badge {
        hdr = hdr.child(status_badge(b_text, b_active));
    }
    hdr
}

fn setting_row(label: &'static str, control: El) -> El {
    row()
        .w_full()
        .py(13.0)
        .gap(16.0)
        .center_y()
        .child(
            text(label)
                .font(ROW_LABEL)
                .color(theme::text_primary())
                .grow(1.0)
                .min_w(0.0),
        )
        .child(control)
}

fn setting_row_with_desc(label: &'static str, desc: impl Into<String>, control: El) -> El {
    row()
        .w_full()
        .py(13.0)
        .gap(16.0)
        .center_y()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.5)
                .child(text(label).font(ROW_LABEL).color(theme::text_primary()))
                .child(
                    text(desc.into())
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted())
                        .truncate(),
                ),
        )
        .child(control)
}

/// Interactive sliding toggle switch for boolean settings (replaces static pill labels).
fn toggle_switch(
    key: &'static str,
    active: bool,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    row()
        .key(key)
        .size(40.0, 22.0)
        .radius(11.0)
        .pl(if active { 20.0 } else { 2.0 })
        .center_y()
        .fill(if active {
            theme::accent_claude()
        } else {
            theme::bg_control()
        })
        .border(
            1.0,
            if active {
                theme::accent_claude()
            } else {
                theme::border_default()
            },
        )
        .hover(if active {
            theme::accent_claude().alpha(0.9)
        } else {
            theme::hover_subtle()
        })
        .cursor(Cursor::Pointer)
        .on_press(on_press)
        .child(
            el().size(16.0, 16.0)
                .radius(8.0)
                .fill(Color::WHITE)
                .border(0.5, Color::BLACK.alpha(0.18)),
        )
}

/// Interactive sliding toggle switch variant needing `Tx` (e.g. window fullscreen).
fn toggle_switch_tx(
    key: &'static str,
    active: bool,
    on_press: impl Fn(&mut AppState, &mut Tx) + 'static,
) -> El {
    row()
        .key(key)
        .size(40.0, 22.0)
        .radius(11.0)
        .pl(if active { 20.0 } else { 2.0 })
        .center_y()
        .fill(if active {
            theme::accent_claude()
        } else {
            theme::bg_control()
        })
        .border(
            1.0,
            if active {
                theme::accent_claude()
            } else {
                theme::border_default()
            },
        )
        .hover(if active {
            theme::accent_claude().alpha(0.9)
        } else {
            theme::hover_subtle()
        })
        .cursor(Cursor::Pointer)
        .on_press_tx(on_press)
        .child(
            el().size(16.0, 16.0)
                .radius(8.0)
                .fill(Color::WHITE)
                .border(0.5, Color::BLACK.alpha(0.18)),
        )
}

/// Segmented option button styled to sit inside a unified segmented control track.
fn segmented_option(
    key: String,
    label: &'static str,
    selected: bool,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    el()
        .key(key)
        .px(11.0)
        .py(5.5)
        .radius(6.5)
        .fill(if selected {
            theme::bg_card()
        } else {
            Color::TRANSPARENT
        })
        .hover(if selected {
            theme::bg_card()
        } else {
            theme::hover_subtle()
        })
        .press(theme::press_subtle())
        .border(
            1.0,
            if selected {
                theme::border_default()
            } else {
                Color::TRANSPARENT
            },
        )
        .center()
        .cursor(Cursor::Pointer)
        .on_press(on_press)
        .child(
            text(label)
                .font(if selected {
                    NAV_TEXT_ACTIVE
                } else {
                    theme::STATUS_TEXT
                })
                .color(if selected {
                    theme::text_primary()
                } else {
                    theme::text_secondary()
                }),
        )
}

fn segmented_track() -> El {
    row()
        .padding(3.0)
        .gap(2.0)
        .radius(9.0)
        .fill(theme::bg_input())
        .border(1.0, theme::border_subtle())
}

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
                .font(ROW_LABEL)
                .color(theme::text_secondary()),
        )
        .child(el().grow(1.0));

    if let Some(action) = trailing_action {
        header = header.child(action);
    }

    col()
        .w_full()
        .gap(6.0)
        .child(header)
        .child(
            el()
                .w_full()
                .px(12.0)
                .py(8.5)
                .radius(8.0)
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

fn clear_field_btn(
    key: &'static str,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    el()
        .key(key)
        .px(7.0)
        .py(2.5)
        .radius(5.0)
        .hover(theme::hover_subtle())
        .cursor(Cursor::Pointer)
        .on_press(on_press)
        .child(
            text(i18n::t(TextKey::ReaderClear))
                .font(theme::BADGE_TEXT)
                .color(theme::text_muted()),
        )
}

fn sidebar_nav_item(
    key: &'static str,
    ico: &'static Icon,
    label: &'static str,
    active: bool,
    tab: SettingsTab,
) -> El {
    row()
        .key(key)
        .w_full()
        .px(11.0)
        .py(8.5)
        .gap(9.0)
        .center_y()
        .radius(8.0)
        .fill(if active {
            theme::bg_control()
        } else {
            Color::TRANSPARENT
        })
        .hover(theme::hover_subtle())
        .press(theme::press_subtle())
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut AppState| {
            s.settings_tab = tab;
        })
        .child(icons::ph(
            ico,
            14.5,
            if active {
                theme::text_primary()
            } else {
                theme::text_muted()
            },
        ))
        .child(
            text(label)
                .font(if active { NAV_TEXT_ACTIVE } else { NAV_TEXT_IDLE })
                .color(if active {
                    theme::text_primary()
                } else {
                    theme::text_secondary()
                })
                .truncate(),
        )
}

fn render_general_tab(s: &AppState) -> El {
    let mut locale_row = segmented_track();
    for locale in UiLocale::all() {
        let is_selected = s.reader_prefs.ui_locale == locale;
        locale_row = locale_row.child(segmented_option(
            format!("btn_locale_{}", locale.key_suffix()),
            locale.native_name(),
            is_selected,
            move |s: &mut AppState| {
                s.set_ui_locale(locale);
            },
        ));
    }

    let cur_dock_pos = s.reader_prefs.dock_position;
    let mut dock_pos_pills = segmented_track();
    for pos in DockPosition::all() {
        let is_selected = pos == cur_dock_pos;
        dock_pos_pills = dock_pos_pills.child(segmented_option(
            format!("btn_dock_pos_{}", pos.key_suffix()),
            i18n::dock_position_label(pos),
            is_selected,
            move |s: &mut AppState| {
                s.set_dock_position(pos);
            },
        ));
    }

    let dock_auto_on = s.reader_prefs.dock_auto_orient;
    let dm_active = s.reader_prefs.use_direct_manipulation;
    let wire_anim_active = s.reader_prefs.animate_wires;
    let trace_colors_active = s.reader_prefs.enable_trace_colors;
    let auto_updates_on = s.reader_prefs.auto_check_updates;

    col()
        .key("settings_general_section")
        .w_full()
        .child(
            el().pb(14.0).child(section_header(
                i18n::t(TextKey::SettingsTabGeneral),
                Some(i18n::t(TextKey::SettingsHeroSubtitle)),
                None,
            )),
        )
        .child(hairline())
        .child(
            col()
                .w_full()
                .py(14.0)
                .gap(9.0)
                .child(
                    text(i18n::t(TextKey::SettingsLanguageLabel))
                        .font(ROW_LABEL)
                        .color(theme::text_primary()),
                )
                .child(row().child(locale_row)),
        )
        .child(hairline())
        .child(setting_row(
            i18n::t(TextKey::SettingsDockPosition),
            dock_pos_pills,
        ))
        .child(hairline())
        .child(setting_row_with_desc(
            i18n::t(TextKey::SettingsDockAutoOrient),
            if dock_auto_on {
                i18n::t(TextKey::SettingsDockAutoOrientOn)
            } else {
                i18n::t(TextKey::SettingsDockAutoOrientOff)
            },
            toggle_switch("btn_toggle_dock_auto_orient", dock_auto_on, |s| {
                s.toggle_dock_auto_orient();
            }),
        ))
        .child(hairline())
        .child(setting_row_with_desc(
            "Trackpad",
            if dm_active {
                i18n::t(TextKey::SettingsTrackpadDm)
            } else {
                i18n::t(TextKey::SettingsTrackpadWheel)
            },
            toggle_switch("btn_toggle_touchpad_dm", dm_active, |s| {
                s.toggle_direct_manipulation();
            }),
        ))
        .child(hairline())
        .child(setting_row_with_desc(
            "Wire Motion",
            if wire_anim_active {
                i18n::t(TextKey::SettingsWireAnimOn)
            } else {
                i18n::t(TextKey::SettingsWireAnimOff)
            },
            toggle_switch("btn_toggle_animate_wires", wire_anim_active, |s| {
                s.toggle_animate_wires();
            }),
        ))
        .child(hairline())
        .child(setting_row_with_desc(
            "Trace Colors",
            if trace_colors_active {
                i18n::t(TextKey::SettingsTraceColorsOn)
            } else {
                i18n::t(TextKey::SettingsTraceColorsOff)
            },
            toggle_switch("btn_toggle_trace_colors", trace_colors_active, |s| {
                s.toggle_trace_colors();
            }),
        ))
        .child(hairline())
        .child(setting_row_with_desc(
            "Fullscreen",
            if s.is_fullscreen {
                i18n::t(TextKey::SettingsExitFullscreen)
            } else {
                i18n::t(TextKey::SettingsToggleFullscreen)
            },
            toggle_switch_tx("btn_settings_fullscreen", s.is_fullscreen, |s, tx| {
                s.is_fullscreen = !s.is_fullscreen;
                tx.toggle_full_screen();
            }),
        ))
        .child(hairline())
        .child(setting_row_with_desc(
            "Auto-updates",
            if auto_updates_on {
                "Check GitHub Releases automatically on startup"
            } else {
                "Manual update checks only"
            },
            toggle_switch("btn_toggle_auto_updates", auto_updates_on, |s| {
                s.toggle_auto_check_updates();
            }),
        ))
        .child(hairline())
        .child(render_updates_row(s))
}

fn render_updates_row(s: &AppState) -> El {
    let status_text = match &s.update_status {
        crate::updater::UpdateStatus::Idle => {
            format!("v{} · GitHub Releases", crate::updater::CURRENT_VERSION)
        }
        crate::updater::UpdateStatus::Checking => "Checking for updates...".to_string(),
        crate::updater::UpdateStatus::UpToDate => {
            format!("v{} · Up to date", crate::updater::CURRENT_VERSION)
        }
        crate::updater::UpdateStatus::Available(info) => {
            format!("v{} available", info.version)
        }
        crate::updater::UpdateStatus::Downloading(info) => {
            format!("Downloading v{}...", info.version)
        }
        crate::updater::UpdateStatus::ReadyToRestart(ver) => {
            format!("Updated to v{ver} · Restarting...")
        }
        crate::updater::UpdateStatus::Failed(err) => err.clone(),
    };

    let mut actions = row().gap(6.0).center_y();
    if let crate::updater::UpdateStatus::Available(info) = &s.update_status {
        actions = actions.child(
            el()
                .key("btn_settings_update_install")
                .px(11.0)
                .py(6.0)
                .radius(8.0)
                .fill(theme::accent_claude())
                .hover(theme::accent_claude().alpha(0.88))
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.start_update_download();
                })
                .child(
                    text(format!("Install v{}", info.version))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_inverted()),
                ),
        );
    }

    let is_busy = matches!(
        s.update_status,
        crate::updater::UpdateStatus::Checking | crate::updater::UpdateStatus::Downloading(_)
    );
    actions = actions.child(
        el()
            .key("btn_check_updates")
            .px(11.0)
            .py(6.0)
            .radius(8.0)
            .fill(theme::bg_control())
            .hover(theme::hover_subtle())
            .press(theme::press_subtle())
            .border(1.0, theme::border_default())
            .cursor(Cursor::Pointer)
            .on_press(|s: &mut AppState| {
                s.check_for_updates();
            })
            .child(
                row()
                    .center_y()
                    .gap(5.0)
                    .child(icons::ph(
                        &icons::ARROW_CLOCKWISE,
                        12.5,
                        theme::text_secondary(),
                    ))
                    .child(
                        text(if is_busy { "Checking..." } else { "Check now" })
                            .font(theme::STATUS_TEXT)
                            .color(theme::text_primary()),
                    ),
            ),
    );

    row()
        .w_full()
        .py(13.0)
        .gap(16.0)
        .center_y()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(
                    text("Updates")
                        .font(ROW_LABEL)
                        .color(theme::text_primary()),
                )
                .child(
                    text(status_text)
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted())
                        .truncate(),
                ),
        )
        .child(actions)
}

fn render_appearance_tab(s: &AppState) -> El {
    let mut grid = col().w_full().gap(12.0);
    for chunk in ThemeId::all().chunks(3) {
        let mut r = row().w_full().gap(12.0);
        for &theme_id in chunk {
            let is_selected = s.reader_prefs.theme == theme_id;
            let (swatch_bg, swatch_card, swatch_accent) = theme_id.preview_colors();
            let line_col = if theme_id.is_dark() {
                Color::WHITE.alpha(0.24)
            } else {
                Color::BLACK.alpha(0.20)
            };
            let tag = if theme_id == ThemeId::WarmCharcoal {
                "Default"
            } else if theme_id.is_dark() {
                "Dark"
            } else {
                "Light"
            };

            let preview = col()
                .w_full()
                .h(64.0)
                .radius(7.0)
                .padding(8.0)
                .gap(5.0)
                .fill(swatch_bg)
                .border(1.0, theme::border_subtle())
                .child(
                    row()
                        .w_full()
                        .gap(4.0)
                        .center_y()
                        .child(el().size(6.0, 6.0).radius(3.0).fill(swatch_accent))
                        .child(el().w(28.0).h(4.0).radius(2.0).fill(line_col))
                        .child(el().grow(1.0)),
                )
                .child(
                    row()
                        .w_full()
                        .grow(1.0)
                        .gap(6.0)
                        .child(
                            col()
                                .grow(1.0)
                                .h_full()
                                .radius(5.0)
                                .padding(6.0)
                                .gap(4.0)
                                .fill(swatch_card)
                                .border(1.0, line_col.alpha(0.45))
                                .child(el().w(40.0).h(4.0).radius(2.0).fill(swatch_accent))
                                .child(el().w(58.0).h(3.5).radius(2.0).fill(line_col))
                                .child(el().w(34.0).h(3.5).radius(2.0).fill(line_col.alpha(0.6))),
                        )
                        .child(
                            col()
                                .w(28.0)
                                .h_full()
                                .radius(5.0)
                                .padding(5.0)
                                .gap(4.0)
                                .fill(swatch_card)
                                .border(1.0, line_col.alpha(0.45))
                                .child(el().w_full().h(3.5).radius(2.0).fill(line_col))
                                .child(el().w(12.0).h(3.5).radius(2.0).fill(swatch_accent)),
                        ),
                );

            let mut footer = row()
                .w_full()
                .center_y()
                .gap(6.0)
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(1.0)
                        .child(
                            text(theme_id.display_name())
                                .font(ROW_LABEL)
                                .color(if is_selected {
                                    theme::text_primary()
                                } else {
                                    theme::text_secondary()
                                })
                                .truncate(),
                        )
                        .child(text(tag).font(theme::BADGE_TEXT).color(theme::text_muted())),
                );
            if is_selected {
                footer = footer.child(icons::ph(&icons::CHECK, 14.0, theme::accent_claude()));
            }

            r = r.child(
                col()
                    .key(format!("btn_theme_{}", theme_id.key_suffix()))
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(9.0)
                    .padding(10.0)
                    .radius(10.0)
                    .continuous_corners()
                    .fill(if is_selected {
                        theme::accent_claude().alpha(0.08)
                    } else {
                        theme::bg_card()
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
                    .child(preview)
                    .child(footer),
            );
        }
        grid = grid.child(r);
    }

    col()
        .key("settings_appearance_section")
        .w_full()
        .gap(18.0)
        .child(section_header(
            i18n::t(TextKey::SettingsThemeLabel),
            Some(i18n::t(TextKey::SettingsAppearanceSubtitle)),
            Some((s.reader_prefs.theme.display_name().to_string(), true)),
        ))
        .child(hairline())
        .child(grid)
}

fn render_ai_tab(s: &AppState) -> El {
    let ai_clear_btn = if !s.ai_api_key_edit.text.trim().is_empty() {
        Some(clear_field_btn("btn_clear_ai_api_key", |s| {
            s.ai_api_key_edit = TextEdit::new("");
            s.save_all_api_settings();
        }))
    } else {
        None
    };

    let mut preset_col = col().w_full().gap(6.0);
    let cur_base = s.ai_base_url_edit.text.trim();
    for chunk in studynodes_llm::API_PROVIDER_PRESETS.chunks(5) {
        let mut r = row().w_full().gap(6.0);
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
                    .py(6.5)
                    .radius(7.0)
                    .fill(if is_selected {
                        theme::accent_claude().alpha(0.14)
                    } else {
                        theme::bg_input()
                    })
                    .hover(theme::hover_subtle())
                    .border(
                        1.0,
                        if is_selected {
                            theme::accent_claude().alpha(0.6)
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

    let follow_ui_lang = s.reader_prefs.ai_summary_follow_ui_language;
    let summary_lang_label = if follow_ui_lang {
        i18n::fmt_summary_lang_ui(s.reader_prefs.ui_locale)
    } else {
        i18n::t(TextKey::SettingsAiSummaryLangSource).to_string()
    };

    col()
        .key("settings_ai_section")
        .w_full()
        .gap(16.0)
        .child(section_header(
            i18n::t(TextKey::SettingsAiSectionTitle),
            Some(i18n::t(TextKey::SettingsAiSectionSubtitle)),
            Some((s.ai_provider_status_label(), s.ai_api_configured())),
        ))
        .child(hairline())
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
        .child(hairline())
        .child(setting_row_with_desc(
            i18n::t(TextKey::SettingsLanguageLabel),
            summary_lang_label,
            toggle_switch("btn_toggle_ai_summary_lang", follow_ui_lang, |s| {
                s.toggle_ai_summary_language_mode();
            }),
        ))
        .child(
            text(i18n::t(TextKey::SettingsStorageNote))
                .font(theme::BADGE_TEXT)
                .color(theme::text_muted()),
        )
}

fn render_chat_tab(s: &AppState) -> El {
    let chat_clear_btn = if !s.chat_api_key_edit.text.trim().is_empty() {
        Some(clear_field_btn("btn_clear_chat_api_key", |s| {
            s.chat_api_key_edit = TextEdit::new("");
            s.save_all_api_settings();
        }))
    } else {
        None
    };

    col()
        .key("settings_chat_ai_section")
        .w_full()
        .gap(16.0)
        .child(section_header(
            i18n::t(TextKey::SettingsChatAiSectionTitle),
            Some(i18n::t(TextKey::SettingsChatAiSectionSubtitle)),
            Some((s.chat_provider_status_label(), s.chat_api_configured())),
        ))
        .child(hairline())
        .child(settings_field(
            i18n::t(TextKey::SettingsFieldApiKey),
            "input_chat_api_key",
            &s.chat_api_key_edit,
            "Dedicated Chat API key — AIza... or sk-... (blank = use Universal AI key)",
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
        ))
        .child(
            text(i18n::t(TextKey::SettingsStorageNote))
                .font(theme::BADGE_TEXT)
                .color(theme::text_muted()),
        )
}

fn render_vision_tab(s: &AppState) -> El {
    let vision_configured = s.cloud_vision_configured();
    let vision_clear_btn = if !s.vision_api_key_edit.text.trim().is_empty() {
        Some(clear_field_btn("btn_clear_vision_api_key", |s| {
            s.vision_api_key_edit = TextEdit::new("");
            s.save_all_api_settings();
        }))
    } else {
        None
    };

    let vision_section = col()
        .key("settings_vision_section")
        .w_full()
        .gap(14.0)
        .child(section_header(
            i18n::t(TextKey::SettingsVisionSectionTitle),
            Some(i18n::t(TextKey::SettingsVisionSectionSubtitle)),
            Some((
                if vision_configured {
                    i18n::t(TextKey::SettingsReady).to_string()
                } else {
                    i18n::t(TextKey::SettingsVisionNotSet).to_string()
                },
                vision_configured,
            )),
        ))
        .child(hairline())
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

    let ollama_section = col()
        .key("settings_ollama_section")
        .w_full()
        .gap(14.0)
        .child(section_header(
            i18n::t(TextKey::SettingsOllamaSectionTitle),
            Some(i18n::t(TextKey::SettingsOllamaSectionSubtitle)),
            Some((i18n::t(TextKey::SettingsLocalHttp).to_string(), false)),
        ))
        .child(hairline())
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

    col()
        .w_full()
        .gap(28.0)
        .child(vision_section)
        .child(ollama_section)
        .child(
            text(i18n::t(TextKey::SettingsStorageNote))
                .font(theme::BADGE_TEXT)
                .color(theme::text_muted()),
        )
}

pub fn render_settings(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let insets = cx.chrome_insets();

    let mut topbar = row()
        .key("settings_topbar")
        .w_full()
        .h(38.0)
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

    let active_tab = s.settings_tab;
    let sidebar = col()
        .key("settings_sidebar")
        .w(196.0)
        .gap(3.0)
        .child(sidebar_nav_item(
            "btn_settings_tab_general",
            &icons::SLIDERS_HORIZONTAL,
            i18n::t(TextKey::SettingsTabGeneral),
            active_tab == SettingsTab::General,
            SettingsTab::General,
        ))
        .child(sidebar_nav_item(
            "btn_settings_tab_appearance",
            &icons::PALETTE,
            i18n::t(TextKey::SettingsTabAppearance),
            active_tab == SettingsTab::Appearance,
            SettingsTab::Appearance,
        ))
        .child(sidebar_nav_item(
            "btn_settings_tab_ai",
            &icons::SPARKLE,
            i18n::t(TextKey::SettingsTabAi),
            active_tab == SettingsTab::Ai,
            SettingsTab::Ai,
        ))
        .child(sidebar_nav_item(
            "btn_settings_tab_chat",
            &icons::CHAT_CIRCLE,
            i18n::t(TextKey::SettingsTabChat),
            active_tab == SettingsTab::Chat,
            SettingsTab::Chat,
        ))
        .child(sidebar_nav_item(
            "btn_settings_tab_vision",
            &icons::CAMERA,
            i18n::t(TextKey::SettingsTabVision),
            active_tab == SettingsTab::Vision,
            SettingsTab::Vision,
        ));

    let active_pane = match active_tab {
        SettingsTab::General => render_general_tab(s),
        SettingsTab::Appearance => render_appearance_tab(s),
        SettingsTab::Ai => render_ai_tab(s),
        SettingsTab::Chat => render_chat_tab(s),
        SettingsTab::Vision => render_vision_tab(s),
    };

    let content_col = col()
        .key("settings_scroll")
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .pb(36.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(active_pane);

    let shell = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .items_center()
        .px(32.0)
        .pt(28.0)
        .child(
            col()
                .w_full()
                .max_w(880.0)
                .grow(1.0)
                .min_h(0.0)
                .gap(22.0)
                .child(
                    text(i18n::t(TextKey::SettingsTitle))
                        .font(PAGE_TITLE)
                        .color(theme::text_primary()),
                )
                .child(
                    row()
                        .w_full()
                        .grow(1.0)
                        .min_h(0.0)
                        .gap(36.0)
                        .child(sidebar)
                        .child(content_col),
                ),
        );

    col()
        .key("settings_root")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(theme::bg_canvas())
        .child(topbar)
        .child(shell)
}
