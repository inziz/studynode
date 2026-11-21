use rune::{
    Color, Cursor, Cx, El, ScrollSpec, TextStyle, col, el, row, text, text_input,
};

use crate::state::{AppScreen, AppState, same_project_path};
use crate::theme;

const HERO_TITLE: TextStyle = TextStyle::new(15.5, 600.0).mono();
const PROJECT_TITLE: TextStyle = TextStyle::new(13.5, 500.0);
const STAT_VALUE: TextStyle = TextStyle::new(18.0, 600.0).mono();

fn metric_card(label: &str, value: String, hint: &str) -> El {
    col()
        .grow(1.0)
        .min_w(0.0)
        .padding(14.0)
        .gap(4.0)
        .radius(6.0)
        .continuous_corners()
        .fill(theme::bg_card())
        .border(1.0, theme::border_subtle())
        .child(
            text(label)
                .font(theme::SECTION_LABEL)
                .color(theme::text_muted()),
        )
        .child(
            text(value)
                .font(STAT_VALUE)
                .color(theme::text_primary()),
        )
        .child(
            text(hint)
                .font(theme::STATUS_TEXT)
                .color(theme::text_secondary())
                .truncate(),
        )
}

pub fn render_dashboard(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let insets = cx.chrome_insets();

    // Custom 40px Monochrome Claude Code-style Titlebar
    let mut titlebar = row()
        .key("dashboard_topbar")
        .w_full()
        .h(40.0)
        .pl(insets.leading + 14.0)
        .pr(insets.trailing + 14.0)
        .gap(10.0)
        .center_y()
        .fill(theme::bg_chrome())
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
        .window_drag()
        .child(
            text("✦")
                .font(TextStyle::new(12.5, 700.0).mono())
                .color(theme::accent_claude()),
        )
        .child(
            text("studynodes")
                .font(theme::TITLE_BAR)
                .color(theme::text_primary()),
        )
        .child(
            text("/ workspaces")
                .font(theme::STATUS_TEXT)
                .color(theme::text_muted()),
        );

    if !s.tabs.is_empty() {
        let mut open_chips = row().gap(6.0).center_y().shrink().min_w(0.0).no_window_drag();
        for (idx, tab) in s.tabs.iter().enumerate() {
            let is_cur = s.active_tab == Some(idx);
            open_chips = open_chips.child(
                el()
                    .key(format!("dash_open_tab_{}", idx))
                    .h(24.0)
                    .max_w(160.0)
                    .px(9.0)
                    .radius(4.0)
                    .fill(if is_cur {
                        theme::bg_card_selected()
                    } else {
                        theme::bg_control()
                    })
                    .hover(theme::hover_subtle())
                    .border(
                        1.0,
                        if is_cur {
                            theme::border_strong()
                        } else {
                            theme::border_subtle()
                        },
                    )
                    .center_y()
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.switch_tab(idx);
                        s.screen = AppScreen::Workspace;
                    })
                    .child(
                        row()
                            .center_y()
                            .gap(5.0)
                            .child(
                                text("●")
                                    .font(TextStyle::new(7.5, 600.0).mono())
                                    .color(if is_cur {
                                        theme::accent_claude()
                                    } else {
                                        theme::text_muted()
                                    }),
                            )
                            .child(
                                text(&tab.title)
                                    .font(theme::STATUS_TEXT)
                                    .color(theme::text_primary())
                                    .truncate(),
                            ),
                    ),
            );
        }
        titlebar = titlebar.child(open_chips);
    }

    titlebar = titlebar.child(el().grow(1.0));

    let total_workspaces = s.recent_projects.len();
    let total_nodes: usize = s.recent_projects.iter().map(|p| p.node_count).sum();
    let total_docs: usize = s.recent_projects.iter().map(|p| p.doc_count).sum();
    let open_tabs_count = s.tabs.len();

    let stats_strip = row()
        .key("dashboard_stats_strip")
        .w_full()
        .gap(10.0)
        .child(metric_card(
            "WORKSPACES",
            total_workspaces.to_string(),
            "Local .studynodes bundles",
        ))
        .child(metric_card(
            "KNOWLEDGE GRAPH",
            format!("{} nodes", total_nodes),
            &format!(
                "{} {} indexed",
                total_docs,
                if total_docs == 1 {
                    "document"
                } else {
                    "documents"
                }
            ),
        ))
        .child(metric_card(
            "OPEN TABS",
            open_tabs_count.to_string(),
            if open_tabs_count == 0 {
                "Open any workspace below"
            } else {
                "Active in topbar strip"
            },
        ));

    // Create New Study Project Card
    let new_project_card = col()
        .key("dashboard_new_card")
        .w_full()
        .padding(20.0)
        .gap(12.0)
        .radius(6.0)
        .continuous_corners()
        .fill(theme::bg_card())
        .border(1.0, theme::border_default())
        .child(
            col()
                .gap(4.0)
                .child(
                    text("> new workspace")
                        .font(HERO_TITLE)
                        .color(theme::text_primary()),
                )
                .child(
                    text("Create an infinite canvas to connect PDFs, Office docs, Markdown notes, and AI synthesis.")
                        .font(theme::BODY_TEXT)
                        .color(theme::text_secondary())
                        .w_full()
                        .wrap(),
                ),
        )
        .child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(
                    el()
                        .grow(1.0)
                        .h(34.0)
                        .px(12.0)
                        .radius(4.0)
                        .fill(theme::bg_input())
                        .border(1.0, theme::border_default())
                        .center_y()
                        .child(
                            text_input(&s.new_project_input, theme::MONO_BODY)
                                .key("new_project_input")
                                .label("Project name")
                                .single_line()
                                .w_full()
                                .color(theme::text_primary())
                                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                                .placeholder(
                                    "Project name (e.g. Cognitive Neuroscience)...",
                                    theme::text_muted(),
                                )
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
                        .h(34.0)
                        .px(16.0)
                        .radius(4.0)
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
                            text("Create")
                                .font(theme::CONTROL_TEXT)
                                .color(theme::text_inverted()),
                        ),
                ),
        )
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(
                    text("templates:")
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                )
                .child(
                    el()
                        .key("btn_tpl_blank")
                        .px(10.0)
                        .py(4.5)
                        .radius(4.0)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .border(1.0, theme::border_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.create_project("Untitled Canvas");
                        })
                        .child(
                            text("Blank")
                                .font(theme::STATUS_TEXT)
                                .color(theme::text_secondary()),
                        ),
                )
                .child(
                    el()
                        .key("btn_tpl_lit")
                        .px(10.0)
                        .py(4.5)
                        .radius(4.0)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .border(1.0, theme::border_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.create_project_from_template("Literature Review", "literature");
                        })
                        .child(
                            text("Literature Review")
                                .font(theme::STATUS_TEXT)
                                .color(theme::text_secondary()),
                        ),
                )
                .child(
                    el()
                        .key("btn_tpl_exam")
                        .px(10.0)
                        .py(4.5)
                        .radius(4.0)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .border(1.0, theme::border_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.create_project_from_template("Exam Prep", "exam");
                        })
                        .child(
                            text("Exam Prep")
                                .font(theme::STATUS_TEXT)
                                .color(theme::text_secondary()),
                        ),
                )
                .child(
                    el()
                        .key("btn_tpl_concept")
                        .px(10.0)
                        .py(4.5)
                        .radius(4.0)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .border(1.0, theme::border_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.create_project_from_template("Concept Map", "concept");
                        })
                        .child(
                            text("Concept Map")
                                .font(theme::STATUS_TEXT)
                                .color(theme::text_secondary()),
                        ),
                ),
        );

    // Filter existing projects by search query
    let query = s.project_search_input.text.trim().to_lowercase();
    let filtered: Vec<_> = s
        .recent_projects
        .iter()
        .enumerate()
        .filter(|(_, proj)| {
            query.is_empty()
                || proj.title.to_lowercase().contains(&query)
                || proj.folder_name.to_lowercase().contains(&query)
        })
        .collect();

    let mut project_cards = col().w_full().gap(8.0);

    if s.recent_projects.is_empty() {
        project_cards = project_cards.child(
            col()
                .w_full()
                .padding(24.0)
                .center()
                .gap(6.0)
                .radius(6.0)
                .continuous_corners()
                .fill(theme::bg_card())
                .border(1.0, theme::border_subtle())
                .child(
                    text("No study projects yet.")
                        .font(theme::BODY_TEXT)
                        .color(theme::text_secondary()),
                )
                .child(
                    text("Type a name above or pick a starter template to begin.")
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                ),
        );
    } else if filtered.is_empty() {
        project_cards = project_cards.child(
            col()
                .w_full()
                .padding(20.0)
                .center()
                .gap(4.0)
                .radius(6.0)
                .fill(theme::bg_card())
                .border(1.0, theme::border_subtle())
                .child(
                    text(format!("No workspaces matching '{}'.", s.project_search_input.text.trim()))
                        .font(theme::BODY_TEXT)
                        .color(theme::text_secondary()),
                ),
        );
    } else {
        for (idx, proj) in filtered {
            let path_for_open_area = proj.path.clone();
            let path_for_open_btn = proj.path.clone();
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

            let mut left_title_row = row().center_y().gap(8.0).child(
                text(&proj.title)
                    .font(PROJECT_TITLE)
                    .color(theme::text_primary())
                    .truncate(),
            );
            if is_open_tab {
                left_title_row = left_title_row.child(
                    el()
                        .px(6.0)
                        .py(1.5)
                        .radius(3.0)
                        .fill(theme::accent_claude().alpha(0.16))
                        .border(1.0, theme::accent_claude().alpha(0.45))
                        .child(
                            text("OPEN")
                                .font(theme::BADGE_TEXT)
                                .color(theme::accent_claude()),
                        ),
                );
            }

            let left_clickable = row()
                .key(format!("proj_open_area_{}", idx))
                .grow(1.0)
                .min_w(0.0)
                .gap(14.0)
                .center_y()
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.open_project(path_for_open_area.clone());
                })
                .child(
                    text("▤")
                        .font(theme::CONTROL_TEXT)
                        .color(theme::text_secondary()),
                )
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(3.0)
                        .child(left_title_row)
                        .child(
                            row()
                                .gap(12.0)
                                .child(
                                    text(format!(
                                        "{} nodes · {} docs · {} links",
                                        proj.node_count, proj.doc_count, proj.edge_count
                                    ))
                                    .font(theme::STATUS_TEXT)
                                    .color(theme::text_secondary()),
                                )
                                .child(
                                    text(&proj.last_modified)
                                        .font(theme::STATUS_TEXT)
                                        .color(theme::text_muted()),
                                ),
                        ),
                );

            let right_actions = if is_confirming_del {
                row()
                    .gap(6.0)
                    .center_y()
                    .child(
                        el()
                            .key(format!("btn_confirm_del_proj_{}", idx))
                            .px(10.0)
                            .py(5.0)
                            .radius(4.0)
                            .fill(theme::accent_danger().alpha(0.18))
                            .hover(theme::accent_danger().alpha(0.28))
                            .border(1.0, theme::accent_danger().alpha(0.55))
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.delete_project(&path_for_del_confirm);
                            })
                            .child(
                                text("Confirm Delete")
                                    .font(theme::STATUS_TEXT)
                                    .color(theme::accent_danger()),
                            ),
                    )
                    .child(
                        el()
                            .key(format!("btn_cancel_del_proj_{}", idx))
                            .px(9.0)
                            .py(5.0)
                            .radius(4.0)
                            .fill(theme::bg_control())
                            .hover(theme::hover_subtle())
                            .border(1.0, theme::border_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(|s: &mut AppState| {
                                s.confirm_delete_project = None;
                            })
                            .child(
                                text("Cancel")
                                    .font(theme::STATUS_TEXT)
                                    .color(theme::text_secondary()),
                            ),
                    )
            } else {
                row()
                    .gap(6.0)
                    .center_y()
                    .child(
                        el()
                            .key(format!("btn_open_proj_{}", idx))
                            .px(10.0)
                            .py(5.0)
                            .radius(4.0)
                            .fill(theme::bg_control())
                            .hover(theme::hover_subtle())
                            .border(1.0, theme::border_default())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.open_project(path_for_open_btn.clone());
                            })
                            .child(
                                text(if is_open_tab { "Resume →" } else { "Open →" })
                                    .font(theme::STATUS_TEXT)
                                    .color(theme::text_primary()),
                            ),
                    )
                    .child(
                        el()
                            .key(format!("btn_del_proj_{}", idx))
                            .px(9.0)
                            .py(5.0)
                            .radius(4.0)
                            .fill(theme::bg_control())
                            .hover(theme::accent_danger().alpha(0.16))
                            .border(1.0, theme::border_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.confirm_delete_project = Some(path_for_del_prompt.clone());
                            })
                            .child(
                                text("Delete")
                                    .font(theme::STATUS_TEXT)
                                    .color(theme::text_muted()),
                            ),
                    )
            };

            let card = row()
                .key(format!("proj_row_{}_{}", idx, proj.folder_name))
                .w_full()
                .px(16.0)
                .py(12.0)
                .gap(12.0)
                .center_y()
                .radius(6.0)
                .continuous_corners()
                .fill(theme::bg_card())
                .hover(theme::hover_subtle())
                .border(
                    1.0,
                    if is_confirming_del {
                        theme::accent_danger().alpha(0.45)
                    } else {
                        theme::border_subtle()
                    },
                )
                .child(left_clickable)
                .child(right_actions);

            project_cards = project_cards.child(card);
        }
    }

    let workspaces_header = row()
        .w_full()
        .center_y()
        .gap(10.0)
        .child(
            text(format!("EXISTING WORKSPACES ({})", total_workspaces))
                .font(theme::SECTION_LABEL)
                .color(theme::text_muted()),
        )
        .child(el().grow(1.0))
        .child(
            el()
                .w(220.0)
                .h(28.0)
                .px(10.0)
                .radius(4.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_subtle())
                .center_y()
                .child(
                    text_input(&s.project_search_input, theme::STATUS_TEXT)
                        .key("dashboard_search_input")
                        .label("Filter workspaces")
                        .single_line()
                        .w_full()
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder("Filter workspaces...", theme::text_muted())
                        .on_edit(|s: &mut AppState| &mut s.project_search_input),
                ),
        )
        .child(
            el()
                .key("btn_refresh_projects")
                .h(28.0)
                .px(10.0)
                .radius(4.0)
                .fill(theme::bg_control())
                .hover(theme::hover_subtle())
                .border(1.0, theme::border_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.refresh_recent_projects();
                    s.status_message = format!("Refreshed {} workspace(s)", s.recent_projects.len());
                })
                .child(
                    text("Refresh")
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_secondary()),
                ),
        );

    let body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .items_center()
        .px(24.0)
        .py(28.0)
        .child(
            col()
                .w_full()
                .max_w(780.0)
                .gap(20.0)
                .child(stats_strip)
                .child(new_project_card)
                .child(
                    col()
                        .w_full()
                        .gap(10.0)
                        .child(workspaces_header)
                        .child(project_cards),
                ),
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
