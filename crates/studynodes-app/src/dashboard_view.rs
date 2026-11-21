use rune::tok::space;
use rune::{
    Color, Cursor, Cx, El, ScrollSpec, TextStyle, col, el, row, text, text_input,
};

use crate::state::AppState;
use crate::theme;

const HERO_TITLE: TextStyle = TextStyle::new(15.5, 600.0).mono();
const PROJECT_TITLE: TextStyle = TextStyle::new(13.5, 500.0);

pub fn render_dashboard(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let insets = cx.chrome_insets();

    // Custom 40px Monochrome Claude Code-style Titlebar
    let titlebar = row()
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
            text("/ visual knowledge workspace")
                .font(theme::STATUS_TEXT)
                .color(theme::text_muted()),
        )
        .child(el().grow(1.0));

    // Start New Study Project Card
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
                        .key("btn_tpl_lit")
                        .px(10.0)
                        .py(4.5)
                        .radius(4.0)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .border(1.0, theme::border_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.create_project("Literature Review");
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
                            s.create_project("Exam Prep");
                        })
                        .child(
                            text("Exam Prep")
                                .font(theme::STATUS_TEXT)
                                .color(theme::text_secondary()),
                        ),
                ),
        );

    // Recent Projects List
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
                    text("Type a name above or pick a template to start.")
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                ),
        );
    } else {
        for (idx, proj) in s.recent_projects.iter().enumerate() {
            let path_clone = proj.path.clone();
            let card = row()
                .key(format!("proj_row_{}_{}", idx, proj.folder_name))
                .w_full()
                .px(16.0)
                .py(12.0)
                .gap(14.0)
                .center_y()
                .radius(6.0)
                .continuous_corners()
                .fill(theme::bg_card())
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .border(1.0, theme::border_subtle())
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.open_project(path_clone.clone());
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
                        .child(
                            text(&proj.title)
                                .font(PROJECT_TITLE)
                                .color(theme::text_primary())
                                .truncate(),
                        )
                        .child(
                            row()
                                .gap(12.0)
                                .child(
                                    text(format!(
                                        "{} nodes · {} links",
                                        proj.node_count, proj.edge_count
                                    ))
                                    .font(theme::STATUS_TEXT)
                                    .color(theme::text_secondary()),
                                )
                                .child(
                                    text(&proj.folder_name)
                                        .font(theme::STATUS_TEXT)
                                        .color(theme::text_muted()),
                                ),
                        ),
                )
                .child(
                    el()
                        .px(10.0)
                        .py(4.5)
                        .radius(4.0)
                        .fill(theme::bg_control())
                        .border(1.0, theme::border_default())
                        .child(
                            text("Open →")
                                .font(theme::STATUS_TEXT)
                                .color(theme::text_primary()),
                        ),
                );

            project_cards = project_cards.child(card);
        }
    }

    let body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .items_center()
        .px(24.0)
        .py(32.0)
        .child(
            col()
                .w_full()
                .max_w(720.0)
                .gap(space::S6)
                .child(new_project_card)
                .child(
                    col()
                        .w_full()
                        .gap(10.0)
                        .child(
                            text("EXISTING WORKSPACES")
                                .font(theme::SECTION_LABEL)
                                .color(theme::text_muted()),
                        )
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
