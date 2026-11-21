use rune::{Color, Cursor, Cx, El, ScrollSpec, TextStyle, col, el, row, text, text_input};

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
            text("✦")
                .font(TextStyle::new(12.0, 700.0).mono())
                .color(theme::accent_claude()),
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

    titlebar = titlebar.child(el().grow(1.0));

    // Unified command input filters existing workspaces AND creates a new workspace on Enter / + New
    let query_raw = s.new_project_input.text.trim().to_string();
    let search_raw = s.project_search_input.text.trim().to_string();
    let effective_query = if !query_raw.is_empty() {
        query_raw.to_lowercase()
    } else {
        search_raw.to_lowercase()
    };

    let filtered: Vec<_> = s
        .recent_projects
        .iter()
        .enumerate()
        .filter(|(_, proj)| {
            effective_query.is_empty()
                || proj.title.to_lowercase().contains(&effective_query)
                || proj.folder_name.to_lowercase().contains(&effective_query)
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
                    .label("Search or create workspace")
                    .single_line()
                    .w_full()
                    .color(theme::text_primary())
                    .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                    .placeholder("Search or create a workspace...", theme::text_muted())
                    .on_edit(|s: &mut AppState| &mut s.new_project_input)
                    .on_submit(|s: &mut AppState, _| {
                        let raw = s.new_project_input.text.trim().to_string();
                        let title = if raw.is_empty() {
                            "New Study Project".to_string()
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
                        "New Study Project".to_string()
                    } else {
                        raw
                    };
                    s.new_project_input = rune::TextEdit::new("");
                    s.create_project(&title);
                })
                .child(
                    text("+ New")
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
            text("Start from:")
                .font(theme::STATUS_TEXT)
                .color(theme::text_muted()),
        )
        .child(starter_link("btn_tpl_blank", "Blank", |s| {
            s.create_project("Untitled Canvas");
        }))
        .child(text("·").font(theme::STATUS_TEXT).color(theme::text_muted()))
        .child(starter_link("btn_tpl_lit", "Literature", |s| {
            s.create_project_from_template("Literature Review", "literature");
        }))
        .child(text("·").font(theme::STATUS_TEXT).color(theme::text_muted()))
        .child(starter_link("btn_tpl_exam", "Exam Prep", |s| {
            s.create_project_from_template("Exam Prep", "exam");
        }))
        .child(text("·").font(theme::STATUS_TEXT).color(theme::text_muted()))
        .child(starter_link("btn_tpl_concept", "Concept Map", |s| {
            s.create_project_from_template("Concept Map", "concept");
        }));

    let mut list_col = col().w_full();

    if s.recent_projects.is_empty() {
        list_col = list_col.child(
            col()
                .w_full()
                .py(28.0)
                .gap(4.0)
                .border_sides([1.0, 0.0, 0.0, 0.0], theme::border_subtle())
                .child(
                    text("No workspaces yet.")
                        .font(theme::BODY_TEXT)
                        .color(theme::text_secondary()),
                )
                .child(
                    text("Type a name above or choose a template to begin.")
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                ),
        );
    } else if filtered.is_empty() {
        list_col = list_col.child(
            col()
                .w_full()
                .py(20.0)
                .gap(4.0)
                .border_sides([1.0, 0.0, 0.0, 0.0], theme::border_subtle())
                .child(
                    text(format!("No existing workspace matches '{}'. Press + New to create it.", query_raw))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                ),
        );
    } else {
        for (row_pos, (idx, proj)) in filtered.into_iter().enumerate() {
            let path_for_open = proj.path.clone();
            let path_for_del_prompt = proj.path.clone();
            let path_for_del_confirm = proj.path.clone();
            let is_open_tab = s
                .tabs
                .iter()
                .any(|t| same_project_path(&t.path, &proj.path));
            let is_confirming_del = s
                .confirm_delete_project
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
                                text("Confirm")
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
                                text("Cancel")
                                    .font(theme::STATUS_TEXT)
                                    .color(theme::text_muted()),
                            ),
                    )
            } else {
                row()
                    .gap(10.0)
                    .pr(4.0)
                    .center_y()
                    .child(
                        text(format!("{} nodes · {}", proj.node_count, proj.last_modified))
                            .font(theme::STATUS_TEXT)
                            .color(theme::text_muted()),
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
                            text("Workspaces")
                                .font(HERO_TITLE)
                                .color(theme::text_primary()),
                        )
                        .child(
                            text("Select a study canvas or start a fresh one.")
                                .font(theme::BODY_TEXT)
                                .color(theme::text_secondary()),
                        ),
                )
                .child(col().w_full().gap(10.0).child(command_bar).child(starter_strip))
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
