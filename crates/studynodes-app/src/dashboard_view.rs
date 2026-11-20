use rune::tok::{self, radius, space, type_};
use rune::{Color, Cx, El, TextStyle, col, el, row, text};

use crate::state::AppState;

const HEADLINE: TextStyle = TextStyle::new(16.0, 600.0);
const SUBTITLE: TextStyle = TextStyle::new(14.0, 500.0);

pub fn render_dashboard(s: &AppState, _: &mut Cx<AppState>) -> El {
    let t = tok::theme(true);
    let accent = Color { r: 0.38, g: 0.60, b: 1.0, a: 1.0 }; // Antigravity Neon Blue
    let surface = Color { r: 0.12, g: 0.13, b: 0.16, a: 1.0 };
    let border = Color { r: 0.20, g: 0.22, b: 0.27, a: 1.0 };

    // Header bar
    let header = row()
        .w_full()
        .padding(20.0)
        .gap(space::S3)
        .center()
        .child(
            el()
                .size(36.0, 36.0)
                .radius(radius::MD)
                .fill(accent)
                .center()
                .child(text("✦").font(type_::TITLE).color(Color::WHITE)),
        )
        .child(
            col()
                .gap(2.0)
                .child(text("StudyNodes").font(HEADLINE).color(t.text))
                .child(
                    text("Visual Knowledge & LLM Node Graph")
                        .font(type_::LABEL)
                        .color(t.text_secondary),
                ),
        );

    // New project card
    let new_project_card = col()
        .w_full()
        .padding(20.0)
        .gap(space::S3)
        .radius(radius::LG)
        .fill(surface)
        .border(1.0, border)
        .child(
            text("Start New Study Project")
                .font(HEADLINE)
                .color(t.text),
        )
        .child(
            text("Create an infinite node canvas to connect documents, notes, and AI synthesis.")
                .font(type_::BODY)
                .color(t.text_secondary),
        )
        .child(
            row()
                .gap(space::S3)
                .child(
                    el()
                        .padding(space::S3)
                        .radius(radius::MD)
                        .fill(accent)
                        .on_press(|s: &mut AppState| {
                            let title = if s.new_project_input.trim().is_empty() {
                                "New Study Project".to_string()
                            } else {
                                s.new_project_input.clone()
                            };
                            s.create_project(&title);
                        })
                        .child(
                            text("Create Project")
                                .font(type_::BODY)
                                .color(Color::WHITE),
                        ),
                )
                .child(
                    el()
                        .padding(space::S3)
                        .radius(radius::MD)
                        .fill(t.card)
                        .border(1.0, border)
                        .on_press(|s: &mut AppState| {
                            s.create_project("Literature Review");
                        })
                        .child(
                            text("Quick: Literature Review")
                                .font(type_::BODY)
                                .color(t.text),
                        ),
                )
                .child(
                    el()
                        .padding(space::S3)
                        .radius(radius::MD)
                        .fill(t.card)
                        .border(1.0, border)
                        .on_press(|s: &mut AppState| {
                            s.create_project("Exam Prep");
                        })
                        .child(
                            text("Quick: Exam Prep")
                                .font(type_::BODY)
                                .color(t.text),
                        ),
                ),
        );

    // Recent projects list
    let mut project_cards = col().w_full().gap(space::S3);

    if s.recent_projects.is_empty() {
        project_cards = project_cards.child(
            col()
                .w_full()
                .padding(space::S6)
                .center()
                .gap(space::S2)
                .radius(radius::LG)
                .fill(surface)
                .border(1.0, border)
                .child(
                    text("No study projects found yet.")
                        .font(type_::BODY)
                        .color(t.text_secondary),
                )
                .child(
                    text("Click 'Create Project' above to start your first node map!")
                        .font(type_::LABEL)
                        .color(accent),
                ),
        );
    } else {
        for proj in &s.recent_projects {
            let path_clone = proj.path.clone();
            let card = row()
                .w_full()
                .padding(16.0)
                .gap(16.0)
                .radius(radius::MD)
                .fill(surface)
                .border(1.0, border)
                .child(
                    col()
                        .grow(1.0)
                        .gap(space::S1)
                        .child(text(&proj.title).font(SUBTITLE).color(t.text))
                        .child(
                            row()
                                .gap(space::S3)
                                .child(
                                    text(format!("{} nodes", proj.node_count))
                                        .font(type_::LABEL)
                                        .color(t.text_secondary),
                                )
                                .child(
                                    text(format!("{} connections", proj.edge_count))
                                        .font(type_::LABEL)
                                        .color(t.text_secondary),
                                )
                                .child(
                                    text(&proj.folder_name)
                                        .font(type_::LABEL)
                                        .color(t.text_secondary.alpha(0.7)),
                                ),
                        ),
                )
                .child(
                    el()
                        .padding(space::S2)
                        .radius(radius::SM)
                        .fill(accent.alpha(0.2))
                        .border(1.0, accent)
                        .on_press(move |s: &mut AppState| {
                            s.open_project(path_clone.clone());
                        })
                        .child(text("Open").font(type_::BODY).color(accent)),
                );

            project_cards = project_cards.child(card);
        }
    }

    col()
        .w_full()
        .h_full()
        .padding(32.0)
        .gap(space::S6)
        .fill(t.bg)
        .child(header)
        .child(new_project_card)
        .child(
            col()
                .w_full()
                .gap(space::S3)
                .child(
                    text("Existing Projects")
                        .font(HEADLINE)
                        .color(t.text),
                )
                .child(project_cards),
        )
}
