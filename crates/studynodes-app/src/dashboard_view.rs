use rune::{
    Color, Cursor, Cx, El, FileDialogOptions, ScrollSpec, TextStyle, Tx, col, el, icon, row,
    text, text_input,
};

use crate::i18n::{self, TextKey};
use crate::state::{AppScreen, AppState, same_project_path};
use crate::theme;

const HERO_TITLE: TextStyle = TextStyle::new(22.0, 600.0);
const ROW_TITLE: TextStyle = TextStyle::new(14.0, 500.0);

fn starter_link(
    key: &'static str,
    label: &str,
    action: impl Fn(&mut AppState) + 'static,
) -> El {
    el()
        .key(key)
        .px(7.0)
        .py(3.5)
        .radius(5.0)
        .hover(theme::hover_subtle())
        .press(theme::press_subtle())
        .cursor(Cursor::Pointer)
        .on_press(action)
        .child(text(label).font(theme::STATUS_TEXT).color(theme::text_secondary()))
}

pub fn render_dashboard(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let insets = cx.chrome_insets();

    // Minimal 36px seamless header matching Workspace strip
    let mut titlebar = row()
        .key("dashboard_topbar")
        .w_full()
        .h(36.0)
        .pl(insets.leading + 14.0)
        .pr(insets.trailing + 14.0)
        .gap(10.0)
        .center_y()
        .fill(theme::bg_canvas())
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
        .window_drag()
        .child(
            icon(&theme::STUDYNODES_LOGO_ICON, 17.0).color(theme::accent_claude()),
        )
        .child(
            text("studynodes")
                .font(theme::STATUS_TEXT)
                .color(theme::text_primary()),
        );

    if !s.tabs.is_empty() {
        let mut open_tabs = row().gap(4.0).center_y().shrink().min_w(0.0).no_window_drag();
        for (idx, tab) in s.tabs.iter().enumerate() {
            let is_cur = s.active_tab == Some(idx);
            open_tabs = open_tabs.child(
                row()
                    .key(format!("dash_open_tab_{}", idx))
                    .h(35.0)
                    .max_w(150.0)
                    .px(8.0)
                    .gap(5.0)
                    .center_y()
                    .hover(theme::hover_subtle())
                    .border_sides(
                        [0.0, 0.0, 1.5, 0.0],
                        if is_cur {
                            theme::accent_claude()
                        } else {
                            Color::TRANSPARENT
                        },
                    )
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.switch_tab(idx);
                        s.screen = AppScreen::Workspace;
                    })
                    .child(
                        text("●")
                            .font(TextStyle::new(6.5, 600.0).mono())
                            .color(theme::accent_claude()),
                    )
                    .child(
                        text(&tab.title)
                            .font(theme::STATUS_TEXT)
                            .color(theme::text_secondary())
                            .truncate(),
                    ),
            );
        }
        titlebar = titlebar.child(open_tabs);
    }

    titlebar = titlebar
        .child(el().grow(1.0))
        .child(
            el()
                .key("btn_import_project")
                .h(26.0)
                .px(8.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press_tx(|_s: &mut AppState, tx: &mut Tx| {
                    let opts = FileDialogOptions {
                        title: i18n::t(TextKey::DashImportBtn).to_string(),
                        ..Default::default()
                    }
                    .filter("StudyNodes Archive", &["studynodes", "zip"]);
                    tx.open_file(opts, |s: &mut AppState, res, _| {
                        if let Ok(Some(path)) = res {
                            s.import_project_from_archive(&path);
                        }
                    });
                })
                .child(
                    text(i18n::t(TextKey::DashImportBtn))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_secondary()),
                ),
        )
        .child(
            el()
                .key("btn_toggle_fullscreen")
                .size(26.0, 26.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                    s.is_fullscreen = !s.is_fullscreen;
                    tx.toggle_full_screen();
                })
                .child(
                    text("⛶")
                        .font(theme::CONTROL_TEXT)
                        .color(if s.is_fullscreen {
                            theme::accent_claude()
                        } else {
                            theme::text_muted()
                        }),
                ),
        )
        .child(
            el()
                .key("btn_open_settings")
                .h(26.0)
                .px(8.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press(|s: &mut AppState| {
                    s.open_settings();
                })
                .child(
                    text(i18n::t(TextKey::DashSettingsBtn))
                        .font(theme::STATUS_TEXT)
                        .color(if s.ai_api_configured() {
                            theme::accent_claude()
                        } else {
                            theme::text_secondary()
                        }),
                ),
        );

    // Unified command input filters existing workspaces AND creates a new workspace on Enter / + New
    let query_raw = s.new_project_input.text.trim().to_string();
    let search_raw = s.project_search_input.text.trim().to_string();
    let effective_query = if !query_raw.is_empty() {
        query_raw.to_lowercase()
    } else {
        search_raw.to_lowercase()
    };

    let collections = s.all_collections();

    let filtered: Vec<_> = s
        .recent_projects
        .iter()
        .enumerate()
        .filter(|(_, proj)| {
            let query_ok = effective_query.is_empty()
                || proj.title.to_lowercase().contains(&effective_query)
                || proj.folder_name.to_lowercase().contains(&effective_query);
            let col_ok = match s.active_collection_filter.as_deref() {
                None => true,
                Some("") => proj
                    .collection
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty(),
                Some(col) => proj
                    .collection
                    .as_deref()
                    .is_some_and(|c| c.eq_ignore_ascii_case(col)),
            };
            query_ok && col_ok
        })
        .collect();

    let command_bar = row()
        .key("dashboard_command_bar")
        .w_full()
        .h(42.0)
        .pl(14.0)
        .pr(6.0)
        .gap(8.0)
        .center_y()
        .radius(10.0)
        .continuous_corners()
        .fill(theme::bg_card())
        .border(1.0, theme::border_default())
        .child(
            el().grow(1.0).min_w(0.0).child(
                text_input(&s.new_project_input, theme::BODY_TEXT)
                    .key("new_project_input")
                    .label(i18n::t(TextKey::DashSearchPlaceholder))
                    .single_line()
                    .w_full()
                    .color(theme::text_primary())
                    .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                    .placeholder(i18n::t(TextKey::DashSearchPlaceholder), theme::text_muted())
                    .on_edit(|s: &mut AppState| &mut s.new_project_input)
                    .on_submit(|s: &mut AppState, _| {
                        let raw = s.new_project_input.text.trim().to_string();
                        let title = if raw.is_empty() {
                            i18n::t(TextKey::DashDefaultNewTitle).to_string()
                        } else {
                            raw
                        };
                        s.new_project_input = rune::TextEdit::new("");
                        s.create_project(&title);
                    }),
            ),
        )
        .child(
            el()
                .key("btn_create_project")
                .h(30.0)
                .px(12.0)
                .radius(7.0)
                .fill(theme::accent_primary())
                .hover(theme::accent_primary().alpha(0.88))
                .press(Color::BLACK.alpha(0.15))
                .center()
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    let raw = s.new_project_input.text.trim().to_string();
                    let title = if raw.is_empty() {
                        i18n::t(TextKey::DashDefaultNewTitle).to_string()
                    } else {
                        raw
                    };
                    s.new_project_input = rune::TextEdit::new("");
                    s.create_project(&title);
                })
                .child(
                    text(i18n::t(TextKey::DashNewProjectBtn))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_inverted()),
                ),
        );

    let starter_strip = row()
        .key("dashboard_starters")
        .w_full()
        .center_y()
        .gap(4.0)
        .child(
            text(i18n::t(TextKey::DashStartFrom))
                .font(theme::STATUS_TEXT)
                .color(theme::text_muted()),
        )
        .child(starter_link(
            "btn_tpl_blank",
            i18n::t(TextKey::DashTplBlank),
            |s| {
                s.create_project("Untitled Canvas");
            },
        ))
        .child(text("·").font(theme::STATUS_TEXT).color(theme::text_muted()))
        .child(starter_link(
            "btn_tpl_lit",
            i18n::t(TextKey::DashTplLiterature),
            |s| {
                s.create_project_from_template("Literature Review", "literature");
            },
        ))
        .child(text("·").font(theme::STATUS_TEXT).color(theme::text_muted()))
        .child(starter_link(
            "btn_tpl_exam",
            i18n::t(TextKey::DashTplExamPrep),
            |s| {
                s.create_project_from_template("Exam Prep", "exam");
            },
        ))
        .child(text("·").font(theme::STATUS_TEXT).color(theme::text_muted()))
        .child(starter_link(
            "btn_tpl_concept",
            i18n::t(TextKey::DashTplConceptMap),
            |s| {
                s.create_project_from_template("Concept Map", "concept");
            },
        ));

    let mut col_chips = row().w_full().center_y().gap(6.0);
    let chip_btn = |key: String, label: String, active: bool| {
        el()
            .key(key)
            .px(8.0)
            .py(4.0)
            .radius(6.0)
            .fill(if active {
                theme::accent_claude().alpha(0.18)
            } else {
                theme::bg_card()
            })
            .border(
                1.0,
                if active {
                    theme::accent_claude().alpha(0.65)
                } else {
                    theme::border_subtle()
                },
            )
            .hover(theme::hover_subtle())
            .press(theme::press_subtle())
            .cursor(Cursor::Pointer)
            .child(
                text(label)
                    .font(theme::STATUS_TEXT)
                    .color(if active {
                        theme::accent_claude()
                    } else {
                        theme::text_secondary()
                    }),
            )
    };

    col_chips = col_chips.child(
        chip_btn(
            "col_filter_all".into(),
            i18n::t(TextKey::DashAllCollections).to_string(),
            s.active_collection_filter.is_none(),
        )
        .on_press(|s: &mut AppState| {
            s.active_collection_filter = None;
        }),
    );

    for (ci, col_name) in collections.iter().enumerate() {
        let active = s
            .active_collection_filter
            .as_deref()
            .is_some_and(|c| c.eq_ignore_ascii_case(col_name));
        let target_col = col_name.clone();
        let del_col = col_name.clone();
        col_chips = col_chips.child(
            row()
                .center_y()
                .radius(6.0)
                .fill(if active {
                    theme::accent_claude().alpha(0.18)
                } else {
                    theme::bg_card()
                })
                .border(
                    1.0,
                    if active {
                        theme::accent_claude().alpha(0.65)
                    } else {
                        theme::border_subtle()
                    },
                )
                .child(
                    el()
                        .key(format!("col_filter_{}", ci))
                        .pl(8.0)
                        .pr(4.0)
                        .py(4.0)
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.active_collection_filter = Some(target_col.clone());
                        })
                        .child(
                            text(col_name.clone())
                                .font(theme::STATUS_TEXT)
                                .color(if active {
                                    theme::accent_claude()
                                } else {
                                    theme::text_secondary()
                                }),
                        ),
                )
                .child(
                    el()
                        .key(format!("btn_del_col_{}", ci))
                        .pl(3.0)
                        .pr(6.0)
                        .py(4.0)
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.delete_collection(&del_col);
                        })
                        .child(
                            text("×")
                                .font(theme::BADGE_TEXT)
                                .color(theme::text_muted()),
                        ),
                ),
        );
    }

    col_chips = col_chips
        .child(
            chip_btn(
                "col_filter_uncat".into(),
                i18n::t(TextKey::DashUncategorized).to_string(),
                s.active_collection_filter.as_deref() == Some(""),
            )
            .on_press(|s: &mut AppState| {
                s.active_collection_filter = Some(String::new());
            }),
        )
        .child(el().grow(1.0))
        .child(
            el()
                .key("btn_new_collection")
                .px(8.0)
                .py(4.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .cursor(Cursor::Pointer)
                .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                    s.show_new_collection_input = !s.show_new_collection_input;
                    if s.show_new_collection_input {
                        tx.focus_when_shown("new_collection_input", false);
                    }
                })
                .child(
                    text(i18n::t(TextKey::DashNewCollection))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                ),
        );

    let mut collections_bar = col().key("dashboard_collections_bar").w_full().gap(6.0).child(col_chips);
    if s.show_new_collection_input {
        collections_bar = collections_bar.child(
            row()
                .w_full()
                .h(34.0)
                .pl(10.0)
                .pr(6.0)
                .gap(8.0)
                .center_y()
                .radius(7.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_default())
                .child(
                    text_input(&s.new_collection_input, theme::BODY_TEXT)
                        .key("new_collection_input")
                        .label("New collection name")
                        .single_line()
                        .grow(1.0)
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder(i18n::t(TextKey::DashCollectionPlaceholder), theme::text_muted())
                        .on_edit(|s: &mut AppState| &mut s.new_collection_input)
                        .on_submit(|s: &mut AppState, _| {
                            let name = s.new_collection_input.text.clone();
                            s.create_collection(&name);
                        }),
                )
                .child(
                    el()
                        .key("btn_confirm_new_collection")
                        .px(9.0)
                        .py(4.0)
                        .radius(5.0)
                        .fill(theme::accent_claude())
                        .hover(theme::accent_claude().alpha(0.86))
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            let name = s.new_collection_input.text.clone();
                            s.create_collection(&name);
                        })
                        .child(
                            text("✓")
                                .font(theme::STATUS_TEXT)
                                .color(Color::WHITE),
                        ),
                ),
        );
    }

    let mut list_col = col().w_full();

    if s.recent_projects.is_empty() {
        list_col = list_col.child(
            col()
                .w_full()
                .py(28.0)
                .gap(4.0)
                .border_sides([1.0, 0.0, 0.0, 0.0], theme::border_subtle())
                .child(
                    text(i18n::t(TextKey::DashEmptyTitle))
                        .font(theme::BODY_TEXT)
                        .color(theme::text_secondary()),
                )
                .child(
                    text(i18n::t(TextKey::DashEmptySubtitle))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                ),
        );
    } else if filtered.is_empty() {
        let empty_filter_label = if !query_raw.is_empty() {
            format!("'{}'", query_raw)
        } else if let Some(col_name) = s.active_collection_filter.as_deref() {
            if col_name.is_empty() {
                i18n::t(TextKey::DashUncategorized).to_string()
            } else {
                col_name.to_string()
            }
        } else {
            i18n::t(TextKey::DashEmptyTitle).to_string()
        };
        list_col = list_col.child(
            col()
                .w_full()
                .py(20.0)
                .gap(4.0)
                .border_sides([1.0, 0.0, 0.0, 0.0], theme::border_subtle())
                .child(
                    text(empty_filter_label)
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                ),
        );
    } else {
        for (row_pos, (idx, proj)) in filtered.into_iter().enumerate() {
            let path_for_open = proj.path.clone();
            let path_for_del_prompt = proj.path.clone();
            let path_for_del_confirm = proj.path.clone();
            let path_for_assign = proj.path.clone();
            let path_for_export = proj.path.clone();
            let export_default_name = format!("{}.studynodes", proj.folder_name);
            let is_open_tab = s
                .tabs
                .iter()
                .any(|t| same_project_path(&t.path, &proj.path));
            let is_confirming_del = s
                .confirm_delete_project
                .as_ref()
                .is_some_and(|p| same_project_path(p, &proj.path));
            let is_assigning_col = s
                .assigning_collection_for
                .as_ref()
                .is_some_and(|p| same_project_path(p, &proj.path));

            let mut left_row = row()
                .key(format!("proj_open_area_{}", idx))
                .grow(1.0)
                .min_w(0.0)
                .py(13.0)
                .px(8.0)
                .gap(10.0)
                .center_y()
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.open_project(path_for_open.clone());
                })
                .child(
                    text("▤")
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                )
                .child(
                    text(&proj.title)
                        .font(ROW_TITLE)
                        .color(theme::text_primary())
                        .truncate(),
                );

            if is_open_tab {
                left_row = left_row.child(
                    text("●")
                        .font(TextStyle::new(6.5, 600.0).mono())
                        .color(theme::accent_claude()),
                );
            }

            let right_actions = if is_confirming_del {
                row()
                    .gap(4.0)
                    .pr(4.0)
                    .center_y()
                    .child(
                        el()
                            .key(format!("btn_confirm_del_proj_{}", idx))
                            .px(8.0)
                            .py(4.0)
                            .radius(5.0)
                            .fill(theme::accent_danger().alpha(0.16))
                            .hover(theme::accent_danger().alpha(0.26))
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.delete_project(&path_for_del_confirm);
                            })
                            .child(
                                text(i18n::t(TextKey::DashConfirmDelete))
                                    .font(theme::STATUS_TEXT)
                                    .color(theme::accent_danger()),
                            ),
                    )
                    .child(
                        el()
                            .key(format!("btn_cancel_del_proj_{}", idx))
                            .px(8.0)
                            .py(4.0)
                            .radius(5.0)
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(|s: &mut AppState| {
                                s.confirm_delete_project = None;
                            })
                            .child(
                                text(i18n::t(TextKey::DashCancel))
                                    .font(theme::STATUS_TEXT)
                                    .color(theme::text_muted()),
                            ),
                    )
            } else {
                let col_badge_label = proj
                    .collection
                    .as_deref()
                    .filter(|c| !c.trim().is_empty())
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "+ Col".into());
                row()
                    .gap(8.0)
                    .pr(4.0)
                    .center_y()
                    .child(
                        el()
                            .key(format!("btn_assign_col_{}", idx))
                            .px(7.0)
                            .py(3.0)
                            .radius(5.0)
                            .fill(if proj.collection.is_some() {
                                theme::accent_claude().alpha(0.12)
                            } else {
                                theme::bg_card()
                            })
                            .border(
                                1.0,
                                if is_assigning_col {
                                    theme::accent_claude()
                                } else {
                                    theme::border_subtle()
                                },
                            )
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                let already = s
                                    .assigning_collection_for
                                    .as_ref()
                                    .is_some_and(|p| same_project_path(p, &path_for_assign));
                                s.assigning_collection_for = if already {
                                    None
                                } else {
                                    Some(path_for_assign.clone())
                                };
                            })
                            .child(
                                text(col_badge_label)
                                    .font(theme::BADGE_TEXT)
                                    .color(if proj.collection.is_some() {
                                        theme::accent_claude()
                                    } else {
                                        theme::text_muted()
                                    }),
                            ),
                    )
                    .child(
                        text(format!(
                            "{} · {}",
                            i18n::fmt_nodes_count(proj.node_count),
                            proj.last_modified
                        ))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                    )
                    .child(
                        el()
                            .key(format!("btn_export_proj_{}", idx))
                            .px(7.0)
                            .py(3.0)
                            .radius(5.0)
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press_tx(move |_s: &mut AppState, tx: &mut Tx| {
                                let opts = FileDialogOptions {
                                    title: i18n::t(TextKey::DashExportBtn).to_string(),
                                    file_name: Some(export_default_name.clone()),
                                    ..Default::default()
                                }
                                .filter("StudyNodes Archive", &["studynodes"]);
                                let proj_path = path_for_export.clone();
                                tx.save_file(opts, move |s: &mut AppState, res, _| {
                                    if let Ok(Some(dest)) = res {
                                        s.export_project_to_archive(&proj_path, &dest);
                                    }
                                });
                            })
                            .child(
                                text(i18n::t(TextKey::DashExportBtn))
                                    .font(theme::BADGE_TEXT)
                                    .color(theme::text_secondary()),
                            ),
                    )
                    .child(
                        el()
                            .key(format!("btn_del_proj_{}", idx))
                            .size(24.0, 24.0)
                            .radius(5.0)
                            .hover(theme::hover_subtle())
                            .center()
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.confirm_delete_project = Some(path_for_del_prompt.clone());
                            })
                            .child(
                                text("×")
                                    .font(theme::CONTROL_TEXT)
                                    .color(theme::text_muted()),
                            ),
                    )
            };

            let item_row = row()
                .key(format!("proj_row_{}_{}", idx, proj.folder_name))
                .w_full()
                .center_y()
                .radius(6.0)
                .hover(theme::hover_subtle())
                .border_sides(
                    [if row_pos == 0 { 1.0 } else { 0.0 }, 0.0, 1.0, 0.0],
                    theme::border_subtle(),
                )
                .child(left_row)
                .child(right_actions);

            list_col = list_col.child(item_row);

            if is_assigning_col {
                let path_clear = proj.path.clone();
                let mut picker_row = row()
                    .key(format!("col_picker_row_{}", idx))
                    .w_full()
                    .px(10.0)
                    .py(6.0)
                    .gap(6.0)
                    .center_y()
                    .fill(theme::bg_card())
                    .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
                    .child(
                        el()
                            .key(format!("col_pick_{}_none", idx))
                            .px(7.0)
                            .py(3.0)
                            .radius(4.0)
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.set_project_collection(&path_clear, None);
                            })
                            .child(
                                text(i18n::t(TextKey::DashUncategorized))
                                    .font(theme::BADGE_TEXT)
                                    .color(theme::text_muted()),
                            ),
                    );
                for (ci, c_opt) in collections.iter().enumerate() {
                    let path_pick = proj.path.clone();
                    let col_val = c_opt.clone();
                    let is_current = proj
                        .collection
                        .as_deref()
                        .is_some_and(|c| c.eq_ignore_ascii_case(&col_val));
                    picker_row = picker_row.child(
                        el()
                            .key(format!("col_pick_{}_{}", idx, ci))
                            .px(7.0)
                            .py(3.0)
                            .radius(4.0)
                            .fill(if is_current {
                                theme::accent_claude().alpha(0.18)
                            } else {
                                Color::TRANSPARENT
                            })
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.set_project_collection(&path_pick, Some(col_val.clone()));
                            })
                            .child(
                                text(c_opt.as_str())
                                    .font(theme::BADGE_TEXT)
                                    .color(if is_current {
                                        theme::accent_claude()
                                    } else {
                                        theme::text_secondary()
                                    }),
                            ),
                    );
                }
                list_col = list_col.child(picker_row);
            }
        }
    }

    let body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .items_center()
        .px(24.0)
        .py(48.0)
        .child(
            col()
                .w_full()
                .max_w(620.0)
                .gap(22.0)
                .child(
                    col()
                        .gap(4.0)
                        .child(
                            text(i18n::t(TextKey::DashTitle))
                                .font(HERO_TITLE)
                                .color(theme::text_primary()),
                        )
                        .child(
                            text(i18n::t(TextKey::DashSubtitle))
                                .font(theme::BODY_TEXT)
                                .color(theme::text_secondary()),
                        ),
                )
                .child(
                    col()
                        .w_full()
                        .gap(10.0)
                        .child(command_bar)
                        .child(starter_strip)
                        .child(collections_bar),
                )
                .child(list_col),
        );

    col()
        .key("dashboard_root")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(theme::bg_canvas())
        .child(titlebar)
        .child(body)
}
