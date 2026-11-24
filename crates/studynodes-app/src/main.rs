#![deny(unsafe_code)]

mod canvas_view;
mod dashboard_view;
mod doc_reader;
mod embed_webview;
mod formula_math;
mod i18n;
mod icons;
mod inspector_view;
mod media_helper;
mod project_io;
mod quiz_overlay;
mod reader_window;
mod settings_view;
mod state;
mod theme;
mod touchpad_win32;

use rune::{App, AppSpec, Cx, El, Titlebar, col, row};

use crate::canvas_view::{render_canvas_view, render_workspace_topbar};
use crate::dashboard_view::render_dashboard;
use crate::inspector_view::render_inspector;
use crate::settings_view::render_settings;
use crate::state::{AppScreen, AppState, CanvasMode, node_base_size};

fn view(s: &AppState, cx: &mut Cx<AppState>) -> El {
    cx.caption_color(theme::text_secondary());
    cx.titlebar_height(36.0);

    if s.screen != AppScreen::Workspace || s.active_quiz_overlay.is_some() {
        touchpad_win32::set_canvas_bounds(None);
    }

    let main_content = if s.active_quiz_overlay.is_some() {
        quiz_overlay::render_quiz_popout_overlay(s, cx)
    } else {
        match s.screen {
            AppScreen::Dashboard => render_dashboard(s, cx),
            AppScreen::Settings => render_settings(s, cx),
            AppScreen::Workspace => {
                let topbar = render_workspace_topbar(s, cx);
                let canvas = render_canvas_view(s);
                let mut workspace_body = row().w_full().grow(1.0).min_h(0.0).child(canvas);

                if s.inspector_open && !s.inspector_popped_out {
                    let inspector = render_inspector(s, cx);
                    workspace_body = workspace_body.child(inspector);
                }

                col()
                    .key("workspace_root")
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .child(topbar)
                    .child(workspace_body)
            }
        }
    };

    col()
        .key("app_root")
        .w_full()
        .h_full()
        .fill(theme::bg_canvas())
        .child(main_content)
        .on_key_up(|s: &mut AppState, ev, _| {
            if let rune::Key::Text(k) = &ev.key {
                if k == " " && s.space_held {
                    s.space_held = false;
                    return true;
                }
            }
            false
        })
}

fn populate_demo_projects(s: &mut AppState) {
    let demo_dir = std::env::temp_dir().join("studynodes_scene_demo");
    let _ = std::fs::remove_dir_all(&demo_dir);
    *s = AppState::with_projects_dir(demo_dir);

    // Tab 1: Linear Algebra (background open tab so multi-project tabs are visible)
    s.create_project_from_template("Linear Algebra", "concept");

    // Tab 2: Cognitive Neuroscience (active flagship workspace)
    s.create_project("Cognitive Neuroscience");

    let mut section = studynodes_graph::model::Node::new(
        studynodes_graph::model::NodeKind::Section,
        "Synaptic Plasticity · Core Circuit",
        studynodes_graph::coords::WorldPos::new(8.0, 12.0),
    );
    section.size = (760.0, 534.0);
    section.meta.section_framed = true;
    s.graph.add_node(section);

    let mut n1 = studynodes_graph::model::Node::new(
        studynodes_graph::model::NodeKind::Document,
        "Hebbian Learning (1949)",
        studynodes_graph::coords::WorldPos::new(28.0, 62.0),
    );
    n1.size = node_base_size(n1.kind);
    n1.content = r#"# The Organization of Behavior
*Donald O. Hebb (1949)*

> **Core Postulate:** Cells that fire together wire together when axon A persistently excites cell B.

Repeated co-activation induces structural growth at synaptic boutons, lowering the threshold for future ensemble recall.

## Synaptic Update Rule

```rust
// Δw_ij proportional to correlated pre/post activity
fn hebbian_delta(pre: f32, post: f32, lr: f32) -> f32 {
    lr * pre * post
}
```

## Biological Substrates

| Mechanism | Role | Timescale |
| --- | --- | --- |
| NMDA Receptor | Coincidence detector | `~20 ms` |
| AMPA Insertion | Structural potentiation | `Hours+` |

## Study Checklist
- [x] Connect **Hebbian Rule** to NMDA gating
- [ ] Review hippocampal consolidation notes"#
        .into();
    n1.summary = Some("Foundational postulate linking correlated pre/post-synaptic spiking to lasting synaptic strengthening.".into());
    let id1 = n1.id.clone();
    s.graph.add_node(n1);

    let mut n2 = studynodes_graph::model::Node::new(
        studynodes_graph::model::NodeKind::Note,
        "LTP Mechanism",
        studynodes_graph::coords::WorldPos::new(460.0, 62.0),
    );
    n2.size = node_base_size(n2.kind);
    n2.content = "Long-Term Potentiation validates Hebb's postulate via NMDA coincidence detection and AMPA receptor insertion.".into();
    let id2 = n2.id.clone();
    s.graph.add_node(n2);

    let mut edge = studynodes_graph::model::Edge::new(
        studynodes_graph::model::EdgeEndpoint {
            node: id1.clone(),
            port: studynodes_graph::model::PortId::new("out_main"),
        },
        studynodes_graph::model::EdgeEndpoint {
            node: id2,
            port: studynodes_graph::model::PortId::new("in_main"),
        },
    );
    edge.relation = studynodes_graph::model::EdgeRelation::Supports;
    edge.label = "Validates".into();
    edge.note = "Biochemical validation through NMDA activation".into();
    s.graph.add_edge(edge);

    let mut fc = studynodes_graph::model::Node::new(
        studynodes_graph::model::NodeKind::Flashcard,
        "Coincidence Detector",
        studynodes_graph::coords::WorldPos::new(28.0, 326.0),
    );
    fc.size = node_base_size(fc.kind);
    fc.content = "Which receptor gates Ca2+ influx only during paired pre + post firing?".into();
    fc.meta.answer = Some(
        "The NMDA glutamate receptor: glutamate binds while depolarization expels the Mg2+ pore block."
            .into(),
    );
    fc.meta.mastery = Some("good".into());
    let fc_id = fc.id.clone();
    s.graph.add_node(fc);

    let mut chk = studynodes_graph::model::Node::new(
        studynodes_graph::model::NodeKind::Checklist,
        "Plasticity Study Plan",
        studynodes_graph::coords::WorldPos::new(460.0, 326.0),
    );
    chk.size = node_base_size(chk.kind);
    chk.content = "- [x] Trace Hebbian weight update rule\n- [ ] Contrast AMPA vs NMDA kinetics\n- [ ] Test recall on LTP flashcard".into();
    let chk_id = chk.id.clone();
    s.graph.add_node(chk);

    let ghost_edge = studynodes_graph::model::Edge::proposed_by_llm(
        studynodes_graph::model::EdgeEndpoint {
            node: fc_id,
            port: studynodes_graph::model::PortId::new("out_main"),
        },
        studynodes_graph::model::EdgeEndpoint {
            node: chk_id,
            port: studynodes_graph::model::PortId::new("in_main"),
        },
        studynodes_graph::model::EdgeRelation::DerivedFrom,
        "Active recall",
        "Derived study drill anchored in NMDA coincidence detection",
    );
    s.graph.add_edge(ghost_edge);

    // Select Document so the right-side Document Reader is open and immediately usable
    s.select_node(id1, false);
    s.save_active_into_tab();
    s.refresh_recent_projects();
    s.status_message = format!(
        "Loaded 'Cognitive Neuroscience' · {} nodes, {} links",
        s.graph.nodes.len(),
        s.graph.edges.len()
    );
    s.last_toast_status = s.status_message.clone();
    s.toast_text = None;
    s.toast_expires_at = None;
}

fn setup_workspace_demo(s: &mut AppState) {
    populate_demo_projects(s);
}

fn setup_dashboard_demo(s: &mut AppState) {
    populate_demo_projects(s);
    s.close_to_dashboard();
}

fn setup_settings_demo(s: &mut AppState) {
    populate_demo_projects(s);
    s.open_settings();
}

fn handle_app_key(s: &mut AppState, ev: &rune::KeyEvent, tx: &mut rune::Tx) -> bool {
    if ev.key == rune::Key::Function(11) {
        s.is_fullscreen = !s.is_fullscreen;
        tx.toggle_full_screen();
        return true;
    }
    if s.active_quiz_overlay.is_some() {
        if ev.key == rune::Key::Escape {
            s.quiz_return_to_scheme();
            return true;
        }
        if tx.focused().is_none() {
            if let Some(qid) = s.active_quiz_overlay.clone() {
                match ev.key {
                    rune::Key::Left => {
                        s.quiz_step_question(&qid, -1);
                        return true;
                    }
                    rune::Key::Right => {
                        s.quiz_step_question(&qid, 1);
                        return true;
                    }
                    _ => {}
                }
            }
            return true;
        }
        return false;
    }
    if ev.key == rune::Key::Escape {
        let had_menu = s.show_block_menu || s.show_ai_menu;
        if had_menu {
            s.show_block_menu = false;
            s.show_ai_menu = false;
            s.block_menu_anchor = None;
            s.block_menu_spawn_world = None;
            return true;
        }
        if tx.focused().is_some() {
            tx.blur();
            return true;
        }
    }
    if tx.focused().is_none() {
        if ev.key == rune::Key::Delete && s.screen == AppScreen::Workspace {
            s.delete_selected_or_empty_nodes();
            return true;
        }
        if ev.is_command('z') {
            if ev.mods.shift {
                s.redo_graph();
            } else {
                s.undo_graph();
            }
            return true;
        }
        if ev.is_command('y') {
            s.redo_graph();
            return true;
        }
        if ev.key == rune::Key::Text(" ".into()) {
            s.space_held = true;
            return true;
        }
        if let Some(cur) = s.active_pdf.as_ref().map(|p| p.page_index) {
            let step = if s.reader_prefs.pdf_layout_mode == doc_reader::PdfLayoutMode::TwoPage {
                2
            } else {
                1
            };
            match ev.key {
                rune::Key::PageUp => {
                    s.set_pdf_page(cur.saturating_sub(step));
                    return true;
                }
                rune::Key::PageDown => {
                    s.set_pdf_page(cur + step);
                    return true;
                }
                _ => {}
            }
        }
    }
    false
}

fn main() {
    let _ = rune::set_app_icon(include_bytes!("../assets/logo.png"));

    let mut spec = AppSpec::new(
        "StudyNodes - Visual Knowledge Canvas",
        (1280.0, 800.0),
        AppState::default,
        view,
    );
    spec.scenes = &[
        ("dashboard", setup_dashboard_demo),
        ("workspace", setup_workspace_demo),
        ("settings", setup_settings_demo),
    ];
    spec.setup = Some(|s, waker| {
        s.smooth_zoom_enabled = true;
        touchpad_win32::set_waker(waker.clone());
        s.set_wakers(waker);
    });
    spec.poll = Some(|s, _dt, tx| {
        s.smooth_zoom_enabled = true;
        touchpad_win32::pump_updates();
        let events = touchpad_win32::take_events();
        let tp_changed = s.apply_touchpad_events(events);
        let zoom_anim_changed = s.step_smooth_zoom(tx.clock());
        let bridge_changed = s.poll_reader_bridge();
        let vision_changed = s.poll_vision_jobs();
        let llm_changed = s.poll_llm_jobs();
        let media_resolved = s.poll_media_jobs();
        let io_changed = s.poll_background_io();
        let toast_changed = s.sync_toast_and_feedback();
        if !rune::is_headless() {
            embed_webview::sync_embeds(&s.desired_embeds());
        }

        let cam_moving = touchpad_win32::is_active()
            || tp_changed
            || zoom_anim_changed
            || s.is_zoom_animating()
            || !matches!(s.canvas_mode, CanvasMode::Idle);
        let cam_needs_commit = s.should_commit_live_camera();

        let mut anim_changed = false;
        let animate_wires = s.reader_prefs.animate_wires
            && !s.graph.edges.is_empty()
            && !cam_moving
            && !s.cam_live_dirty;
        let playing_media = s.has_playing_media();
        let mut media_tick = false;
        if s.screen == AppScreen::Workspace && !rune::is_headless() {
            if animate_wires {
                s.wire_anim_phase = ((tx.clock() * 0.35).rem_euclid(1.0)) as f32;
                anim_changed = true;
            }
            if playing_media {
                media_tick = true;
            }
            if touchpad_win32::is_active()
                || tp_changed
                || zoom_anim_changed
                || s.is_zoom_animating()
            {
                tx.wake_after(std::time::Duration::ZERO);
            } else if s.cam_live_dirty {
                tx.wake_after(std::time::Duration::from_millis(50));
            } else if animate_wires {
                tx.wake_after(std::time::Duration::from_millis(16));
            } else if playing_media {
                tx.wake_after(std::time::Duration::from_millis(120));
            } else if let Some(due) = s.pending_graph_save_at {
                let wait = due
                    .saturating_duration_since(std::time::Instant::now())
                    .max(std::time::Duration::from_millis(16));
                tx.wake_after(wait);
            } else if s.toast_expires_at.is_some() || s.copied_feedback.is_some() {
                tx.wake_after(std::time::Duration::from_millis(250));
            }
        } else if !rune::is_headless() {
            if let Some(due) = s.pending_graph_save_at {
                let wait = due
                    .saturating_duration_since(std::time::Instant::now())
                    .max(std::time::Duration::from_millis(16));
                tx.wake_after(wait);
            } else if s.toast_expires_at.is_some() || s.copied_feedback.is_some() {
                tx.wake_after(std::time::Duration::from_millis(250));
            }
        }

        let other_changed = bridge_changed
            || vision_changed
            || llm_changed
            || media_resolved
            || io_changed
            || toast_changed
            || anim_changed
            || media_tick;
        let cam_committed = if cam_needs_commit || (other_changed && s.cam_live_dirty) {
            s.flush_live_camera()
        } else {
            false
        };

        if other_changed || cam_committed {
            tx.instant();
            true
        } else if tp_changed || zoom_anim_changed {
            tx.live_only();
            false
        } else {
            false
        }
    });
    spec.on_key = Some(|s, ev, tx| handle_app_key(s, &ev, tx));

    App::new(spec)
        .titlebar(Titlebar::new(36.0))
        .on_window(|w| {
            touchpad_win32::attach(w);
            embed_webview::attach(w);
        })
        .run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Key, KeyEvent, MagnifyEvent, Mods, Runtime, WheelEvent};
    use studynodes_graph::coords::{ScreenPos, WorldPos};
    use studynodes_graph::model::{NodeKind, QuizDifficulty, QuizDisplayMode, QuizQuestion};
    use tempfile::TempDir;

    fn has_element_key(rt: &Runtime<AppState>, key: &str) -> bool {
        rt.elements()
            .into_iter()
            .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
    }

    struct TestHarness {
        _dir: TempDir,
        rt: Runtime<AppState>,
    }

    static TEST_THEME_LOCK: std::sync::RwLock<()> = std::sync::RwLock::new(());

    fn make_test_runtime() -> TestHarness {
        let _guard = TEST_THEME_LOCK.read().unwrap_or_else(|e| e.into_inner());
        let dir = TempDir::new().expect("temp dir for isolated test");
        let env = Env {
            system_dark: true,
            scale: 2.0,
        };
        let mut rt = Runtime::new(
            AppState::with_projects_dir(dir.path().to_path_buf()),
            view,
            (1280.0, 800.0),
            env,
        );
        rt.on_key = Some(|s, ev, tx| handle_app_key(s, &ev, tx));
        rt.frame(0.0);
        TestHarness { _dir: dir, rt }
    }

    fn settle_frames(rt: &mut Runtime<AppState>, count: usize) {
        for _ in 0..count {
            rt.frame(1.0 / 60.0);
        }
    }

    fn element_rect(rt: &Runtime<AppState>, key: &str) -> [f32; 4] {
        rt.elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing element with key '{key}'"))
            .rect
    }

    #[test]
    fn test_dashboard_to_workspace_transition() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        assert_eq!(rt.state.screen, AppScreen::Dashboard);

        rt.state.create_project("Neuroscience 101");
        rt.frame(1.0 / 60.0);

        assert_eq!(rt.state.screen, AppScreen::Workspace);
        assert_eq!(rt.state.graph.title, "Neuroscience 101");
    }

    #[test]
    fn test_node_creation_and_edge_linking() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Cognitive Science");

        let n1 = studynodes_graph::model::Node::new(
            NodeKind::Document,
            "Hebb 1949",
            WorldPos::new(100.0, 100.0),
        );
        let n2 = studynodes_graph::model::Node::new(
            NodeKind::Note,
            "Synaptic Plasticity",
            WorldPos::new(450.0, 100.0),
        );
        let id1 = n1.id.clone();
        let id2 = n2.id.clone();

        rt.state.graph.add_node(n1);
        rt.state.graph.add_node(n2);
        rt.frame(1.0 / 60.0);

        assert_eq!(rt.state.graph.nodes.len(), 2);

        let edge = studynodes_graph::model::Edge::new(
            studynodes_graph::model::EdgeEndpoint {
                node: id1.clone(),
                port: studynodes_graph::model::PortId::new("out_main"),
            },
            studynodes_graph::model::EdgeEndpoint {
                node: id2.clone(),
                port: studynodes_graph::model::PortId::new("in_main"),
            },
        );
        let edge_id = edge.id.clone();
        rt.state.graph.add_edge(edge);
        rt.frame(1.0 / 60.0);

        assert_eq!(rt.state.graph.edges.len(), 1);

        rt.state.select_edge(edge_id.clone());
        assert_eq!(rt.state.selected_edge, Some(edge_id));
    }

    #[test]
    fn test_node_scaling_and_trackpad_navigation() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Scaling Test");

        assert_eq!(rt.state.graph.viewport.zoom, 1.0);
        let n1_id = rt.state.add_note_node(Some(WorldPos::new(100.0, 100.0)));

        rt.state.graph.viewport.pan_by_screen_delta(50.0, -30.0);
        assert!((rt.state.graph.viewport.offset.x - (-50.0)).abs() < 1e-4);
        assert!((rt.state.graph.viewport.offset.y - (30.0)).abs() < 1e-4);

        let anchor = ScreenPos::new(400.0, 300.0);
        let world_at_anchor_before = rt.state.graph.viewport.screen_to_world(anchor);
        rt.state.graph.viewport.zoom_at(anchor, 1.5);
        assert!((rt.state.graph.viewport.zoom - 1.5).abs() < 1e-4);
        let world_at_anchor_after = rt.state.graph.viewport.screen_to_world(anchor);
        assert!((world_at_anchor_before.x - world_at_anchor_after.x).abs() < 1e-3);
        assert!((world_at_anchor_before.y - world_at_anchor_after.y).abs() < 1e-3);

        let node = rt.state.graph.get_node(&n1_id).unwrap();
        let scaled_w = node.size.0 * rt.state.graph.viewport.zoom;
        assert_eq!(scaled_w, 420.0);
    }

    #[test]
    fn test_inline_node_editing_and_deletion() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Interactivity Test");

        let id = rt.state.add_note_node(Some(WorldPos::new(200.0, 200.0)));
        assert_eq!(rt.state.editing_node_id, Some(id.clone()));

        rt.state.stop_editing_node();
        assert_eq!(rt.state.editing_node_id, None);

        rt.state.start_editing_node(id.clone());
        assert_eq!(rt.state.editing_node_id, Some(id.clone()));

        rt.state.canvas_content_edit =
            rune::TextEdit::new("Updated notes directly from card input");
        rt.state.stop_editing_node();
        assert_eq!(rt.state.editing_node_id, None);

        let node = rt.state.graph.get_node(&id).unwrap();
        assert_eq!(node.content, "Updated notes directly from card input");

        rt.state.delete_node(&id);
        assert!(rt.state.graph.get_node(&id).is_none());
    }

    #[test]
    fn test_wire_connection_and_ai_explanation() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("AI Wire Test");

        let id1 = rt.state.add_note_node(Some(WorldPos::new(100.0, 100.0)));
        let id2 = rt.state.add_document_node(Some(WorldPos::new(500.0, 100.0)));

        rt.state.connect_nodes(&id1, &id2);
        assert_eq!(rt.state.graph.edges.len(), 1);

        rt.state.explain_node_ai(&id1);
        let node = rt.state.graph.get_node(&id1).unwrap();
        assert!(node.summary.is_some());
    }

    #[test]
    fn test_click_unselected_note_focuses_and_types_via_real_events() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Real Click & Typing Test");

        let note_id = rt.state.add_note_node(Some(WorldPos::new(120.0, 140.0)));
        rt.state.deselect_all();
        rt.invalidate();
        settle_frames(rt, 3);

        let body_key = format!("node_body_{}", note_id.as_str());
        let r = element_rect(rt, &body_key);
        let (cx, cy) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);

        rt.pointer_move(cx, cy);
        rt.pointer_down(cx, cy);
        rt.pointer_up(cx, cy);
        settle_frames(rt, 4);

        let input_key = format!("note_body_{}", note_id.as_str());
        let input_el = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some(input_key.as_str()))
            .expect("selected note must mount note_body input");
        assert!(
            input_el.focused,
            "clicking unselected note body must transfer focus directly to its text_input"
        );

        rt.key(KeyEvent {
            key: Key::Text("Synaptic gain".into()),
            mods: Mods::default(),
        });
        settle_frames(rt, 3);

        let content = &rt.state.graph.get_node(&note_id).unwrap().content;
        assert!(
            content.contains("Synaptic gain"),
            "expected typed text in node.content, got '{content}'"
        );
    }

    #[test]
    fn test_click_document_opens_onscreen_reader_panel() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Document Reader Test");

        let doc_id = rt.state.add_document_node(Some(WorldPos::new(100.0, 120.0)));
        rt.state.inspector_open = false;
        rt.state.deselect_all();
        rt.invalidate();
        settle_frames(rt, 3);

        let doc_body_key = format!("node_body_{}", doc_id.as_str());
        let r = element_rect(rt, &doc_body_key);
        let (cx, cy) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);

        rt.pointer_move(cx, cy);
        rt.pointer_down(cx, cy);
        rt.pointer_up(cx, cy);
        settle_frames(rt, 35);

        assert!(
            rt.state.inspector_open,
            "clicking a Document card must pop open inspector_open"
        );

        let inspector_r = element_rect(rt, "inspector_panel");
        let canvas_r = element_rect(rt, "graph_canvas");

        assert!(
            inspector_r[0] >= 800.0
                && inspector_r[0] + inspector_r[2] <= 1280.5
                && inspector_r[2] >= 350.0,
            "inspector_panel must sit cleanly on screen: {inspector_r:?}"
        );
        assert!(
            canvas_r[2] < 1000.0,
            "graph_canvas must shrink to make room for inspector_panel: {canvas_r:?}"
        );
    }

    #[test]
    fn test_header_drag_moves_node_independently_of_body() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Header Drag Test");

        let note_id = rt.state.add_note_node(Some(WorldPos::new(150.0, 150.0)));
        rt.invalidate();
        settle_frames(rt, 3);

        let header_key = format!("node_header_{}", note_id.as_str());
        let hr = element_rect(rt, &header_key);
        // Grab the empty right-center area of the header grip
        let (sx, sy) = (hr[0] + hr[2] - 45.0, hr[1] + hr[3] * 0.5);

        rt.pointer_move(sx, sy);
        rt.pointer_down(sx, sy);
        rt.pointer_move(sx + 40.0, sy + 25.0);
        rt.pointer_move(sx + 80.0, sy + 50.0);
        rt.pointer_up(sx + 80.0, sy + 50.0);
        settle_frames(rt, 3);

        let pos = rt.state.graph.get_node(&note_id).unwrap().pos;
        assert!(
            (pos.x - 230.0).abs() < 2.0 && (pos.y - 200.0).abs() < 2.0,
            "expected node pos ~(230, 200), got ({}, {})",
            pos.x,
            pos.y
        );
    }

    #[test]
    fn test_simultaneous_xy_touchpad_pan_and_zoom() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Touchpad 2D Pan & Pinch Test");
        rt.invalidate();
        settle_frames(rt, 2);

        // 1. Reversed DirectManipulation TouchpadEvent path moves X and Y simultaneously
        let changed = rt.state.apply_touchpad_events(vec![
            touchpad_win32::TouchpadEvent::Pan { dx: 60.0, dy: -40.0 },
        ]);
        assert!(changed);
        assert!((rt.state.graph.viewport.offset.x - 60.0).abs() < 1e-4);
        assert!((rt.state.graph.viewport.offset.y - (-40.0)).abs() < 1e-4);

        // 2. Reversed Precision WheelEvent over canvas moves X and Y simultaneously
        let cr = element_rect(rt, "graph_canvas");
        let (cx, cy) = (cr[0] + cr[2] * 0.5, cr[1] + cr[3] * 0.5);
        rt.pointer_move(cx, cy);
        rt.wheel_event(WheelEvent::new(30.0, 20.0).at(cx, cy).precise(true));
        settle_frames(rt, 2);

        assert!(
            (rt.state.graph.viewport.offset.x - 90.0).abs() < 1e-3,
            "expected reversed offset.x 90, got {}",
            rt.state.graph.viewport.offset.x
        );
        assert!(
            (rt.state.graph.viewport.offset.y - (-20.0)).abs() < 1e-3,
            "expected reversed offset.y -20, got {}",
            rt.state.graph.viewport.offset.y
        );

        // 3. Expand zooms IN, pinch zooms OUT (TouchpadEvent + Ctrl+Wheel + MagnifyEvent)
        rt.state.apply_touchpad_events(vec![touchpad_win32::TouchpadEvent::Zoom {
            factor: 1.25,
            x: cx,
            y: cy,
        }]);
        assert!((rt.state.graph.viewport.zoom - 1.25).abs() < 1e-4);

        rt.state.apply_touchpad_events(vec![touchpad_win32::TouchpadEvent::Zoom {
            factor: 0.80,
            x: cx,
            y: cy,
        }]);
        assert!((rt.state.graph.viewport.zoom - 1.0).abs() < 1e-4);

        // Ctrl+Wheel expand (+dy) -> zooms IN
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        rt.wheel_event(WheelEvent::new(0.0, 40.0).at(cx, cy).mods(ctrl));
        settle_frames(rt, 2);
        assert!(
            rt.state.graph.viewport.zoom > 1.0,
            "Ctrl+Wheel +dy must zoom in"
        );

        // MagnifyEvent (pinch_wheel on Windows negates +dy -> delta=-0.20 on expand, +0.20 on pinch)
        let before = rt.state.graph.viewport.zoom;
        let expand_delta = if cfg!(target_os = "windows") { -0.20 } else { 0.20 };
        rt.magnify_event(MagnifyEvent::new(expand_delta).at(cx, cy));
        settle_frames(rt, 2);
        assert!(
            rt.state.graph.viewport.zoom > before,
            "expand gesture must zoom in"
        );

        let expanded = rt.state.graph.viewport.zoom;
        let pinch_delta = if cfg!(target_os = "windows") { 0.20 } else { -0.20 };
        rt.magnify_event(MagnifyEvent::new(pinch_delta).at(cx, cy));
        settle_frames(rt, 2);
        assert!(
            rt.state.graph.viewport.zoom < expanded,
            "pinch gesture must zoom out"
        );
    }

    #[test]
    fn test_short_edge_pill_renders_above_nodes_and_centered() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Edge Layering & Center Test");

        // Place two nodes close together so the connecting edge pill overlaps the short span.
        let id1 = rt.state.add_note_node(Some(WorldPos::new(100.0, 150.0)));
        let id2 = rt.state.add_document_node(Some(WorldPos::new(440.0, 110.0)));
        rt.state.inspector_open = false;
        rt.state.connect_nodes(&id1, &id2);
        let edge_id = rt
            .state
            .graph
            .edges
            .keys()
            .next()
            .expect("connected edge must exist")
            .clone();

        rt.invalidate();
        settle_frames(rt, 3);

        let elements = rt.elements();
        let pos1 = elements
            .iter()
            .position(|e| e.key.as_deref() == Some(id1.as_str()))
            .expect("node 1 element");
        let pos2 = elements
            .iter()
            .position(|e| e.key.as_deref() == Some(id2.as_str()))
            .expect("node 2 element");
        let pill_key = format!("edge_pill_{}", edge_id.as_str());
        let pill_pos = elements
            .iter()
            .position(|e| e.key.as_deref() == Some(pill_key.as_str()))
            .expect("edge pill element");

        assert!(
            pill_pos > pos1 && pill_pos > pos2,
            "edge pill (index {pill_pos}) must render ABOVE both node cards ({pos1}, {pos2})"
        );

        let r1 = elements[pos1].rect;
        let r2 = elements[pos2].rect;
        let pr = elements[pill_pos].rect;
        let wire_mid_x = ((r1[0] + r1[2]) + r2[0]) * 0.5;
        let pill_mid_x = pr[0] + pr[2] * 0.5;
        assert!(
            (pill_mid_x - wire_mid_x).abs() < 18.0,
            "edge pill center ({pill_mid_x}) must sit near wire midpoint ({wire_mid_x})"
        );
    }

    #[test]
    fn test_dashboard_create_and_delete_project() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;

        rt.state.create_project("Temp Workspace");
        let created_path = rt.state.bundle.as_ref().unwrap().root_dir.clone();
        assert!(created_path.exists());
        assert_eq!(rt.state.tabs.len(), 1);

        rt.state.close_to_dashboard();
        assert_eq!(rt.state.screen, AppScreen::Dashboard);
        assert_eq!(rt.state.recent_projects.len(), 1);

        rt.state.delete_project(&created_path);
        assert!(!created_path.exists(), "deleted workspace folder must be removed from disk");
        assert!(rt.state.recent_projects.is_empty());
        assert!(rt.state.tabs.is_empty());
    }

    #[test]
    fn test_multi_project_tabs_switch_and_preserve_state() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;

        rt.state.create_project("Alpha Study");
        let path_a = rt.state.bundle.as_ref().unwrap().root_dir.clone();
        let node_a = rt.state.add_note_node(Some(WorldPos::new(111.0, 222.0)));
        rt.state.graph.viewport.offset = WorldPos::new(75.0, 45.0);

        rt.state.create_project("Beta Study");
        let path_b = rt.state.bundle.as_ref().unwrap().root_dir.clone();
        let node_b = rt.state.add_note_node(Some(WorldPos::new(333.0, 120.0)));
        assert_eq!(rt.state.tabs.len(), 2);
        assert_eq!(rt.state.active_tab, Some(1));
        assert!(rt.state.graph.get_node(&node_b).is_some());
        assert!(rt.state.graph.get_node(&node_a).is_none());

        // Switch back to tab 0 -> restores Alpha Study graph + camera
        rt.state.switch_tab(0);
        assert_eq!(rt.state.active_tab, Some(0));
        assert_eq!(rt.state.graph.title, "Alpha Study");
        assert!(rt.state.graph.get_node(&node_a).is_some());
        assert!((rt.state.graph.viewport.offset.x - 75.0).abs() < 1e-4);

        // Opening an already-open project focuses existing tab instead of duplicating
        rt.state.open_project(path_b);
        assert_eq!(rt.state.tabs.len(), 2);
        assert_eq!(rt.state.active_tab, Some(1));
        assert_eq!(rt.state.graph.title, "Beta Study");

        // Close active tab -> switches cleanly to remaining Alpha Study tab
        rt.state.close_tab(1);
        assert_eq!(rt.state.tabs.len(), 1);
        assert_eq!(rt.state.graph.title, "Alpha Study");
        assert_eq!(rt.state.bundle.as_ref().unwrap().root_dir, path_a);
    }

    #[test]
    fn test_selectable_document_card_page_preview() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Preview Selector Test");

        let doc_id = rt.state.add_document_node(Some(WorldPos::new(80.0, 90.0)));
        rt.state.load_sample_document(&doc_id);
        rt.state.inspector_open = false;
        rt.state.deselect_all();
        rt.invalidate();
        settle_frames(rt, 3);

        let next_key = format!("doc_next_page_{}", doc_id.as_str());
        let nr = element_rect(rt, &next_key);
        let (nx, ny) = (nr[0] + nr[2] * 0.5, nr[1] + nr[3] * 0.5);

        rt.pointer_move(nx, ny);
        rt.pointer_down(nx, ny);
        rt.pointer_up(nx, ny);
        settle_frames(rt, 3);

        assert_eq!(rt.state.graph.get_node(&doc_id).unwrap().preview_page, 1);
        assert!(
            !rt.state.inspector_open,
            "stepping preview page on canvas card must not force-open the reader panel"
        );

        let prev_key = format!("doc_prev_page_{}", doc_id.as_str());
        let pr = element_rect(rt, &prev_key);
        let (px, py) = (pr[0] + pr[2] * 0.5, pr[1] + pr[3] * 0.5);
        rt.pointer_move(px, py);
        rt.pointer_down(px, py);
        rt.pointer_up(px, py);
        settle_frames(rt, 3);
        assert_eq!(rt.state.graph.get_node(&doc_id).unwrap().preview_page, 0);
    }

    #[test]
    fn test_resizable_inspector_and_popout_dock_cycle() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Resize & Popout Test");
        let _doc_id = rt.state.add_document_node(Some(WorldPos::new(80.0, 90.0)));
        rt.invalidate();
        settle_frames(rt, 5);

        let start_w = rt.state.inspector_width;
        let hr = element_rect(rt, "inspector_resize_handle");
        let (hx, hy) = (hr[0] + hr[2] * 0.5, hr[1] + hr[3] * 0.5);

        rt.pointer_move(hx, hy);
        rt.pointer_down(hx, hy);
        rt.pointer_move(hx - 120.0, hy);
        rt.pointer_up(hx - 120.0, hy);
        settle_frames(rt, 3);

        assert!(
            (rt.state.inspector_width - (start_w + 120.0)).abs() < 2.0,
            "dragging splitter left must widen inspector from {} to {}, got {}",
            start_w,
            start_w + 120.0,
            rt.state.inspector_width
        );

        // Pop out hides docked panel and activates standalone bridge state
        rt.state.pop_out_reader();
        settle_frames(rt, 3);
        assert!(rt.state.inspector_popped_out);
        assert!(!rt.state.inspector_open);

        // Simulate standalone window sending DockBack command
        {
            let mut b = rt.state.reader_bridge.lock().unwrap();
            b.push_command(crate::reader_window::ReaderCommand::DockBack);
        }
        assert!(rt.state.poll_reader_bridge());
        assert!(!rt.state.inspector_popped_out);
        assert!(rt.state.inspector_open);
    }

    #[test]
    fn test_reader_readonly_mode_hides_edit_chrome_and_syncs_bridge() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Read-Only Reader Test");
        let doc_id = rt.state.add_document_node(Some(WorldPos::new(80.0, 90.0)));
        rt.state.load_sample_document(&doc_id);
        rt.invalidate();
        settle_frames(rt, 4);

        // Opens directly in Read-Only focus mode with minimal toolbar and hidden edit inputs
        assert!(rt.state.reader_read_only);
        assert!(rt.state.reader_bridge.lock().unwrap().snapshot.read_only);
        let keys_ro: Vec<_> = rt
            .elements()
            .into_iter()
            .filter_map(|e| e.key)
            .collect();
        assert!(keys_ro.contains(&"reader_readonly_toolbar".to_string()));
        assert!(!keys_ro.contains(&"inspector_title".to_string()));
        assert!(!keys_ro.contains(&"btn_doc_ingest".to_string()));
        assert!(!keys_ro.contains(&"append_note".to_string()));
        assert!(!keys_ro.contains(&"btn_delete_doc".to_string()));

        // Clicking '✎ Edit' reveals full editing UI and syncs bridge snapshot
        let r = element_rect(rt, "btn_toggle_readonly");
        let (cx, cy) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
        rt.pointer_move(cx, cy);
        rt.pointer_down(cx, cy);
        rt.pointer_up(cx, cy);
        settle_frames(rt, 3);

        assert!(!rt.state.reader_read_only);
        assert!(!rt.state.reader_bridge.lock().unwrap().snapshot.read_only);
        let keys_edit: Vec<_> = rt
            .elements()
            .into_iter()
            .filter_map(|e| e.key)
            .collect();
        assert!(keys_edit.contains(&"inspector_title".to_string()));
        assert!(keys_edit.contains(&"btn_doc_ingest".to_string()));

        // Pop-out window opens directly in Read-Only mode even if sidebar was in Edit mode
        rt.state.pop_out_reader();
        settle_frames(rt, 2);
        assert!(rt.state.reader_read_only);
        assert!(rt.state.reader_bridge.lock().unwrap().snapshot.read_only);

        // Bridge SetReadOnly(false) toggles Edit mode back from standalone window
        {
            let mut b = rt.state.reader_bridge.lock().unwrap();
            b.push_command(crate::reader_window::ReaderCommand::SetReadOnly(false));
        }
        assert!(rt.state.poll_reader_bridge());
        assert!(!rt.state.reader_read_only);
    }

    #[test]
    fn test_locate_nearest_nodes_recovers_from_far_pan() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Nearest Nodes Test");

        let n1 = rt.state.add_note_node(Some(WorldPos::new(160.0, 140.0)));
        let n2 = rt.state.add_note_node(Some(WorldPos::new(480.0, 220.0)));
        rt.state.inspector_open = false;

        // Pan far into empty void where zero nodes are visible
        rt.state.graph.viewport.offset = WorldPos::new(15000.0, -8000.0);
        rt.state.graph.viewport.zoom = 0.25;
        rt.invalidate();
        settle_frames(rt, 4);

        let n1_visible_while_far = rt
            .elements()
            .into_iter()
            .any(|e| e.key.as_deref() == Some(n1.as_str()) && !e.exiting && e.rect[0] >= -1000.0);
        assert!(
            !n1_visible_while_far,
            "node 1 must start offscreen/culled after far pan"
        );

        // Click topbar 'Nearest' button via real pointer input
        let br = element_rect(rt, "btn_locate_nearest");
        let (bx, by) = (br[0] + br[2] * 0.5, br[1] + br[3] * 0.5);
        rt.pointer_move(bx, by);
        rt.pointer_down(bx, by);
        rt.pointer_up(bx, by);
        settle_frames(rt, 2);

        // Camera state jumps immediately to frame the cluster
        assert!(rt.state.graph.viewport.zoom >= 0.45);
        assert!(rt.state.graph.viewport.offset.x.abs() < 1000.0);

        // Allow Rune's layout transition springs to finish settling rendered element rects
        settle_frames(rt, 45);

        let canvas_r = element_rect(rt, "graph_canvas");
        let r1_near = element_rect(rt, n1.as_str());
        let r2_near = element_rect(rt, n2.as_str());
        assert!(
            r1_near[0] >= canvas_r[0] - 1.0
                && r1_near[0] + r1_near[2] <= canvas_r[0] + canvas_r[2] + 1.0
                && r2_near[0] >= canvas_r[0] - 1.0
                && r2_near[0] + r2_near[2] <= canvas_r[0] + canvas_r[2] + 1.0,
            "both nearest nodes must be centered inside canvas_r={canvas_r:?}, got r1={r1_near:?}, r2={r2_near:?}"
        );

        // Empty-canvas fallback resets cleanly to origin at 100%
        rt.state.delete_node(&n1);
        rt.state.delete_node(&n2);
        rt.state.graph.viewport.offset = WorldPos::new(900.0, 900.0);
        rt.state.graph.viewport.zoom = 2.0;
        assert!(rt.state.locate_nearest_nodes().is_none());
        assert_eq!(rt.state.graph.viewport.offset, WorldPos::ZERO);
        assert!((rt.state.graph.viewport.zoom - 1.0).abs() < 1e-4);
    }

    fn click_key(rt: &mut Runtime<AppState>, key: &str) {
        let r = element_rect(rt, key);
        let cx = r[0] + r[2] * 0.5;
        let cy = r[1] + r[3] * 0.5;
        rt.pointer_move(cx, cy);
        rt.pointer_down(cx, cy);
        rt.pointer_up(cx, cy);
        settle_frames(rt, 3);
    }

    #[test]
    fn test_section_frame_group_drag_and_banner_toggle() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Section Group Test");

        let sec_id = rt
            .state
            .add_study_node(NodeKind::Section, Some(WorldPos::new(40.0, 40.0)));
        let child_inside = rt.state.add_note_node(Some(WorldPos::new(90.0, 110.0)));
        let child_outside = rt.state.add_note_node(Some(WorldPos::new(820.0, 110.0)));
        rt.state.deselect_all();
        rt.state.inspector_open = false;
        rt.invalidate();
        settle_frames(rt, 45);

        let hdr = element_rect(rt, &format!("node_header_{}", sec_id.as_str()));
        let sx = hdr[0] + hdr[2] * 0.25;
        let sy = hdr[1] + hdr[3] * 0.5;
        rt.pointer_move(sx, sy);
        rt.pointer_down(sx, sy);
        rt.pointer_move(sx + 60.0, sy + 40.0);
        rt.pointer_move(sx + 120.0, sy + 70.0);
        rt.pointer_up(sx + 120.0, sy + 70.0);
        settle_frames(rt, 5);

        let sec_pos = rt.state.graph.get_node(&sec_id).unwrap().pos;
        let in_pos = rt.state.graph.get_node(&child_inside).unwrap().pos;
        let out_pos = rt.state.graph.get_node(&child_outside).unwrap().pos;

        assert!((sec_pos.x - 160.0).abs() < 2.0, "sec moved right: {:?}", sec_pos);
        assert!((sec_pos.y - 110.0).abs() < 2.0, "sec moved down: {:?}", sec_pos);
        assert!((in_pos.x - 210.0).abs() < 2.0, "child inside moved together: {:?}", in_pos);
        assert!((in_pos.y - 180.0).abs() < 2.0, "child inside moved together: {:?}", in_pos);
        assert!((out_pos.x - 820.0).abs() < 1e-3, "outside note stayed put: {:?}", out_pos);

        // Toggle section to Title Banner Only mode -> height drops to 44
        settle_frames(rt, 40);
        click_key(rt, &format!("sec_toggle_frame_{}", sec_id.as_str()));
        let sec = rt.state.graph.get_node(&sec_id).unwrap();
        assert!(!sec.meta.section_framed);
        assert!((crate::state::node_world_size(sec).1 - 44.0).abs() < 1e-3);
    }

    #[test]
    fn test_flashcard_flip_and_mastery_rating() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Flashcard Recall Test");
        rt.state.inspector_open = false;

        let fc_id = rt
            .state
            .add_study_node(NodeKind::Flashcard, Some(WorldPos::new(120.0, 120.0)));
        if let Some(fc) = rt.state.graph.get_node_mut(&fc_id) {
            fc.content = "What channels gate LTP?".into();
            fc.meta.answer = Some("NMDA receptors".into());
        }
        rt.state.deselect_all();
        rt.invalidate();
        settle_frames(rt, 45);

        // Reveal answer inline on canvas without popping inspector open
        click_key(rt, &format!("flash_flip_{}", fc_id.as_str()));
        assert!(rt.state.flipped_flashcards.contains(&fc_id));
        assert!(!rt.state.inspector_open);

        // Rate mastery as Good
        settle_frames(rt, 10);
        click_key(rt, &format!("flash_rate_good_{}", fc_id.as_str()));
        let fc = rt.state.graph.get_node(&fc_id).unwrap();
        assert_eq!(fc.meta.mastery.as_deref(), Some("good"));
    }

    #[test]
    fn test_checklist_card_toggles_task_inline() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Checklist Test");

        let chk_id = rt
            .state
            .add_study_node(NodeKind::Checklist, Some(WorldPos::new(100.0, 100.0)));
        if let Some(chk) = rt.state.graph.get_node_mut(&chk_id) {
            chk.content = "- [ ] First item\n- [ ] Second item".into();
        }
        rt.invalidate();
        settle_frames(rt, 45);

        click_key(rt, &format!("check_item_{}_1", chk_id.as_str()));
        let content = &rt.state.graph.get_node(&chk_id).unwrap().content;
        assert!(content.contains("- [ ] First item"));
        assert!(content.contains("- [x] Second item"));
    }

    #[test]
    fn test_excerpt_jumps_to_source_document_page() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Excerpt Jump Test");

        let doc_id = rt.state.add_document_node(Some(WorldPos::new(80.0, 80.0)));
        let lines: String = (1..=20)
            .map(|i| format!("Line {} of multi-page study paper", i))
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(doc) = rt.state.graph.get_node_mut(&doc_id) {
            doc.content = lines;
            doc.preview_page = 2;
        }

        let ex_id = rt
            .state
            .create_excerpt_from_selected_doc(None)
            .expect("excerpt from selected doc");
        {
            let ex = rt.state.graph.get_node(&ex_id).unwrap();
            assert_eq!(ex.meta.source_node.as_ref(), Some(&doc_id));
            assert_eq!(ex.meta.source_page, Some(2));
        }

        // Reset doc preview page back to 0 and deselect everything
        if let Some(doc) = rt.state.graph.get_node_mut(&doc_id) {
            doc.preview_page = 0;
        }
        rt.state.deselect_all();
        rt.state.inspector_open = false;
        rt.invalidate();
        settle_frames(rt, 45);

        // Clicking deep-link pill on Excerpt card jumps to Document page 2 & opens Reader
        click_key(rt, &format!("excerpt_jump_{}", ex_id.as_str()));
        assert!(rt.state.selected_nodes.contains(&doc_id));
        assert_eq!(rt.state.graph.get_node(&doc_id).unwrap().preview_page, 2);
        assert!(rt.state.inspector_open);
    }

    #[test]
    fn test_sequence_next_step_and_block_menu() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Sequence & Block Menu Test");
        rt.state.inspector_open = false;
        rt.invalidate();
        settle_frames(rt, 45);

        // Open + Block palette menu and create a Sequence step
        click_key(rt, "btn_toggle_block_menu");
        assert!(rt.state.show_block_menu);
        settle_frames(rt, 10);

        click_key(rt, "btn_add_sequence");
        assert!(!rt.state.show_block_menu);
        let seq1_id = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Sequence)
            .expect("Step 01 created")
            .id
            .clone();
        settle_frames(rt, 45);

        // Click "+ Next ->" on Step 01 to chain Step 02
        click_key(rt, &format!("seq_next_{}", seq1_id.as_str()));
        let steps: Vec<_> = rt
            .state
            .graph
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Sequence)
            .collect();
        assert_eq!(steps.len(), 2);
        assert!(steps.iter().any(|n| n.meta.step_index == Some(2)));
        assert_eq!(rt.state.graph.edges.len(), 1);
    }

    #[test]
    fn test_wire_layer_aligns_with_canvas_and_node_sockets() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Wire Alignment Test");
        rt.state.inspector_open = false;

        let n1 = rt.state.add_note_node(Some(WorldPos::new(60.0, 120.0)));
        let n2 = rt.state.add_note_node(Some(WorldPos::new(460.0, 220.0)));
        rt.state.connect_nodes(&n1, &n2);
        rt.invalidate();
        settle_frames(rt, 45);

        let canvas_r = element_rect(rt, "graph_canvas");
        let grid_r = element_rect(rt, "canvas_grid_layer");
        let wire_r = element_rect(rt, "canvas_wire_layer");

        for i in 0..4 {
            assert!(
                (grid_r[i] - canvas_r[i]).abs() < 1.0,
                "canvas_grid_layer[{i}]={} must match graph_canvas[{i}]={}",
                grid_r[i],
                canvas_r[i]
            );
            assert!(
                (wire_r[i] - canvas_r[i]).abs() < 1.0,
                "canvas_wire_layer[{i}]={} must match graph_canvas[{i}]={}",
                wire_r[i],
                canvas_r[i]
            );
        }
    }

    #[test]
    fn test_hide_edge_label_and_click_wire_selection() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Hide Edge Label Test");

        let n1 = rt.state.add_note_node(Some(WorldPos::new(40.0, 140.0)));
        let n2 = rt.state.add_note_node(Some(WorldPos::new(480.0, 140.0)));
        rt.state.connect_nodes(&n1, &n2);
        let edge_id = rt.state.graph.edges.keys().next().unwrap().clone();
        if let Some(e) = rt.state.graph.get_edge_mut(&edge_id) {
            e.label = "Supports".into();
        }
        rt.state.select_edge(edge_id.clone());
        rt.invalidate();
        settle_frames(rt, 45);

        let visible_pill_r = element_rect(rt, &format!("edge_pill_{}", edge_id.as_str()));
        assert!(visible_pill_r[2] > 30.0);

        // Hide individual connector label -> becomes compact clickable midpoint dot
        click_key(rt, "btn_edge_hide_label");
        assert!(rt.state.graph.get_edge(&edge_id).unwrap().hide_label);
        rt.state.deselect_all();
        rt.state.inspector_open = false;
        settle_frames(rt, 45);

        let dot_r = element_rect(rt, &format!("edge_pill_{}", edge_id.as_str()));
        assert!(dot_r[2] <= 14.0, "hidden label must collapse to small midpoint dot: {dot_r:?}");

        // Clicking directly along the Bézier curve away from midpoint selects the edge
        let canvas_r = element_rect(rt, "graph_canvas");
        let n1_sz = crate::state::node_base_size(NodeKind::Note);
        let n2_sz = crate::state::node_base_size(NodeKind::Note);
        let from_w = WorldPos::new(40.0 + n1_sz.0, 140.0 + n1_sz.1 * 0.5);
        let to_w = WorldPos::new(480.0, 140.0 + n2_sz.1 * 0.5);
        let wire_pt_world = studynodes_graph::bezier::CubicBezier::wire(from_w, to_w).eval(0.25);
        let wire_pt_screen = rt.state.graph.viewport.world_to_screen(wire_pt_world);
        let click_x = canvas_r[0] + wire_pt_screen.x;
        let click_y = canvas_r[1] + wire_pt_screen.y;
        rt.pointer_move(click_x, click_y);
        rt.pointer_down(click_x, click_y);
        rt.pointer_up(click_x, click_y);
        settle_frames(rt, 45);

        assert_eq!(rt.state.selected_edge.as_ref(), Some(&edge_id));

        // Restore label, then test global Hide All Labels toggle
        click_key(rt, "btn_edge_show_label");
        assert!(!rt.state.graph.get_edge(&edge_id).unwrap().hide_label);
        click_key(rt, "btn_toggle_all_edge_labels");
        assert!(rt.state.hide_all_edge_labels);
        settle_frames(rt, 45);
        let dot_global = element_rect(rt, &format!("edge_pill_{}", edge_id.as_str()));
        assert!(dot_global[2] <= 14.0);
    }

    #[test]
    fn test_formula_inline_symbol_insert_and_shorthand_format() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Formula UX Test");
        rt.state.inspector_open = false;

        let eq_id = rt
            .state
            .add_study_node(NodeKind::Formula, Some(WorldPos::new(100.0, 120.0)));
        if let Some(eq) = rt.state.graph.get_node_mut(&eq_id) {
            eq.content = "E = ".into();
        }
        rt.state.canvas_content_edit = rune::TextEdit::new("E = ");
        rt.state.inspector_content_edit = rune::TextEdit::new("E = ");
        rt.invalidate();
        settle_frames(rt, 45);

        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some(&format!("formula_input_{}", eq_id.as_str()))));

        click_key(rt, &format!("formula_sym_{}_0", eq_id.as_str()));
        assert!(rt.state.graph.get_node(&eq_id).unwrap().content.contains('∑'));

        if let Some(eq) = rt.state.graph.get_node_mut(&eq_id) {
            eq.content = "\\sum \\alpha^2".into();
        }
        rt.state.canvas_content_edit = rune::TextEdit::new("\\sum \\alpha^2");
        rt.state.inspector_content_edit = rune::TextEdit::new("\\sum \\alpha^2");
        rt.invalidate();
        settle_frames(rt, 5);

        click_key(rt, &format!("formula_fmt_{}", eq_id.as_str()));
        assert_eq!(rt.state.graph.get_node(&eq_id).unwrap().content, "∑ α²");
    }

    #[test]
    fn test_image_block_creation_and_popout_restrictions() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Image & Popout Policy Test");
        rt.state.inspector_open = false;
        rt.invalidate();
        settle_frames(rt, 45);

        // Spawn Image block from + Block menu
        click_key(rt, "btn_toggle_block_menu");
        settle_frames(rt, 5);
        click_key(rt, "btn_add_image");
        settle_frames(rt, 10);

        let img_id = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Image)
            .expect("Image block created")
            .id
            .clone();
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("btn_popout_reader")));

        // Checklist must hide pop-out button and refuse pop_out_reader()
        let chk_id = rt
            .state
            .add_study_node(NodeKind::Checklist, Some(WorldPos::new(100.0, 320.0)));
        rt.state.select_node(chk_id, false);
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 45);
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("btn_popout_reader") && !e.exiting));
        rt.state.pop_out_reader();
        assert!(!rt.state.inspector_popped_out);

        // Excerpt must hide pop-out button and refuse pop_out_reader()
        let ex_id = rt
            .state
            .add_study_node(NodeKind::Excerpt, Some(WorldPos::new(400.0, 320.0)));
        rt.state.select_node(ex_id.clone(), false);
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 45);
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("btn_popout_reader") && !e.exiting));
        rt.state.pop_out_reader();
        assert!(!rt.state.inspector_popped_out);

        // Section must pop out and publish its framed children + internal edges to ReaderSnapshot
        let sec_id = rt
            .state
            .add_study_node(NodeKind::Section, Some(WorldPos::new(900.0, 20.0)));
        let fc_id = rt
            .state
            .add_study_node(NodeKind::Flashcard, Some(WorldPos::new(940.0, 80.0)));
        let seq_id = rt
            .state
            .add_study_node(NodeKind::Sequence, Some(WorldPos::new(1240.0, 80.0)));
        rt.state.connect_nodes(&fc_id, &seq_id);
        rt.state.select_node(sec_id.clone(), false);
        rt.state.pop_out_reader();
        assert!(rt.state.inspector_popped_out);

        let snap = rt.state.reader_bridge.lock().unwrap().snapshot.clone();
        assert_eq!(snap.node_kind, NodeKind::Section);
        assert_eq!(snap.node_id.as_ref(), Some(&sec_id));
        assert_eq!(snap.section_children.len(), 2);
        assert_eq!(snap.section_edges.len(), 1);

        // Clicking an Excerpt while Section is popped out must preserve popped-out Section snapshot
        rt.state.select_node(ex_id, false);
        let snap_after_ex = rt.state.reader_bridge.lock().unwrap().snapshot.clone();
        assert_eq!(snap_after_ex.node_kind, NodeKind::Section);
        assert_eq!(snap_after_ex.node_id.as_ref(), Some(&sec_id));

        // Selecting a pop-outable Image switches pop-out window cleanly
        rt.state.select_node(img_id, false);
        assert_eq!(rt.state.reader_bridge.lock().unwrap().snapshot.node_kind, NodeKind::Image);
    }

    fn poll_vision_until_idle(rt: &mut Runtime<AppState>) {
        for _ in 0..80 {
            if rt.state.poll_vision_jobs()
                && rt.state.vision_pending_nodes.is_empty()
                && !rt.state.vision_downloading_models
            {
                break;
            }
            if rt.state.vision_pending_nodes.is_empty() && !rt.state.vision_downloading_models {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        rt.state.poll_vision_jobs();
        rt.invalidate();
        settle_frames(rt, 12);
    }

    #[test]
    fn test_image_ocr_extract_and_spawn_note_and_formula() {
        const ONE_PX_PNG: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];

        let mut th = make_test_runtime();
        let png_path = th._dir.path().join("lecture_slide.png");
        std::fs::write(&png_path, ONE_PX_PNG).expect("write png");

        let rt = &mut th.rt;
        let cb = rune::clipboard::memory();
        rt.use_clipboard(&cb);
        rt.state.create_project("Vision OCR Project");
        rt.state.set_vision_service_override(std::sync::Arc::new(
            studynodes_vision::MockVisionService {
                models_ready: false,
                ..Default::default()
            },
        ));

        let img_id = rt
            .state
            .add_study_node(NodeKind::Image, Some(WorldPos::new(120.0, 120.0)));
        rt.state.ingest_file_into_node(&img_id, &png_path);
        rt.state.select_node(img_id.clone(), false);
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 20);

        // Cycling OCR engine Auto -> LocalPpOcrV5 -> CloudVision reveals/hides btn_download_neural_models,
        // and toggling btn_toggle_vision_settings reveals vision_api_settings_panel.
        assert!(rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("btn_download_neural_models") && !e.exiting));
        click_key(rt, "btn_cycle_ocr_engine");
        assert_eq!(
            rt.state.reader_prefs.ocr_engine,
            studynodes_vision::OcrEnginePreference::LocalPpOcrV5
        );
        assert!(rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("btn_download_neural_models") && !e.exiting));
        click_key(rt, "btn_cycle_ocr_engine");
        assert_eq!(
            rt.state.reader_prefs.ocr_engine,
            studynodes_vision::OcrEnginePreference::CloudVision
        );
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("btn_download_neural_models") && !e.exiting));

        click_key(rt, "btn_toggle_vision_settings");
        settle_frames(rt, 10);
        assert!(rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("vision_api_settings_panel") && !e.exiting));
        click_key(rt, "btn_toggle_vision_settings");
        settle_frames(rt, 10);

        // Verify shared decoded_images cache warms a single handle
        assert_eq!(rt.state.decoded_images.len(), 1);

        // Extract Text (non-blocking worker -> poll -> meta.extracted_text)
        click_key(rt, "btn_img_ocr_text");
        poll_vision_until_idle(rt);

        let img_node = rt.state.graph.get_node(&img_id).unwrap();
        assert_eq!(
            img_node.meta.extracted_text.as_deref(),
            Some("Cells that fire together wire together.")
        );
        assert_eq!(
            img_node.meta.ocr_engine.as_deref(),
            Some("MockVision (Test)")
        );

        // 1-click Copy copies extracted text to clipboard
        click_key(rt, "btn_ocr_copy");
        assert_eq!(
            cb.text().as_deref().map(String::as_str),
            Some("Cells that fire together wire together.")
        );
        assert!(rt.state.is_recently_copied("ocr"));

        // Spawn Note -> linked [derived from] OCR
        click_key(rt, "btn_ocr_spawn_note");
        settle_frames(rt, 10);
        let ocr_note = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Note && n.meta.source_node.as_ref() == Some(&img_id))
            .expect("spawned OCR note");
        assert_eq!(ocr_note.content, "Cells that fire together wire together.");
        assert!(rt.state.graph.edges.values().any(|e| {
            e.from.node == img_id
                && e.to.node == ocr_note.id
                && e.relation == studynodes_graph::model::EdgeRelation::DerivedFrom
                && e.label == "OCR"
        }));

        // Extract Formula -> Spawn Formula -> linked [derived from] Equation OCR
        click_key(rt, "btn_img_ocr_formula");
        poll_vision_until_idle(rt);
        assert_eq!(
            rt.state
                .graph
                .get_node(&img_id)
                .unwrap()
                .meta
                .extracted_text
                .as_deref(),
            Some("Δ wᵢⱼ = η · xᵢ · yⱼ")
        );

        click_key(rt, "btn_ocr_spawn_formula");
        settle_frames(rt, 10);
        let eq_node = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Formula && n.meta.source_node.as_ref() == Some(&img_id))
            .expect("spawned Formula card from OCR");
        assert_eq!(eq_node.content, "Δ wᵢⱼ = η · xᵢ · yⱼ");
        assert!(rt.state.graph.edges.values().any(|e| {
            e.from.node == img_id
                && e.to.node == eq_node.id
                && e.relation == studynodes_graph::model::EdgeRelation::DerivedFrom
                && e.label == "Equation OCR"
        }));

        // Clearing extracted text hides ocr_extracted_preview
        click_key(rt, "btn_ocr_clear");
        settle_frames(rt, 20);
        assert!(rt.state.graph.get_node(&img_id).unwrap().meta.extracted_text.is_none());
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("ocr_extracted_preview") && !e.exiting));
    }

    #[test]
    fn test_node_translation_and_spawn_translated_note() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        let cb = rune::clipboard::memory();
        rt.use_clipboard(&cb);
        rt.state.create_project("Translation Project");
        rt.state.set_vision_service_override(std::sync::Arc::new(
            studynodes_vision::MockVisionService {
                models_ready: true,
                ..Default::default()
            },
        ));

        let mut note_node = studynodes_graph::model::Node::new(
            NodeKind::Note,
            "Hebbian Plasticity",
            WorldPos::new(140.0, 140.0),
        );
        note_node.size = crate::state::node_base_size(NodeKind::Note);
        note_node.content = "Cells that fire together wire together.".into();
        let note_id = note_node.id.clone();
        rt.state.graph.add_node(note_node);
        rt.state.select_node(note_id.clone(), false);
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 20);

        click_key(rt, "lang_fr");
        assert_eq!(rt.state.reader_prefs.target_language, "fr");

        click_key(rt, "btn_translate_node");
        poll_vision_until_idle(rt);

        let note = rt.state.graph.get_node(&note_id).unwrap();
        assert_eq!(
            note.meta.translated_text.as_deref(),
            Some("[FR] Cells that fire together wire together.")
        );
        assert_eq!(note.meta.target_language.as_deref(), Some("fr"));

        // 1-click Copy copies translated text to clipboard
        click_key(rt, "btn_translate_copy");
        assert_eq!(
            cb.text().as_deref().map(String::as_str),
            Some("[FR] Cells that fire together wire together.")
        );
        assert!(rt.state.is_recently_copied("translation"));

        click_key(rt, "btn_translate_spawn_note");
        settle_frames(rt, 10);

        let tr_note = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Note && n.meta.source_node.as_ref() == Some(&note_id))
            .expect("spawned translated note");
        assert_eq!(tr_note.content, "[FR] Cells that fire together wire together.");
        assert!(rt.state.graph.edges.values().any(|e| {
            e.from.node == note_id
                && e.to.node == tr_note.id
                && e.relation == studynodes_graph::model::EdgeRelation::DerivedFrom
                && e.label == "Translation"
        }));
    }

    #[test]
    fn test_image_node_empty_content_hides_empty_document_placeholder() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Empty Image Source Test");

        let mut img_node = studynodes_graph::model::Node::new(
            NodeKind::Image,
            "Lecture Slide",
            WorldPos::new(100.0, 100.0),
        );
        img_node.size = crate::state::node_base_size(NodeKind::Image);
        img_node.content = String::new();
        let img_id = img_node.id.clone();
        rt.state.graph.add_node(img_node);
        rt.state.select_node(img_id, false);
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 30);

        // Read-only mode: no empty-document placeholder when image content is empty
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("empty_doc_placeholder") && !e.exiting));
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("image_caption_markdown") && !e.exiting));

        // Edit mode: still no empty-document placeholder when image content is empty
        click_key(rt, "btn_toggle_readonly");
        settle_frames(rt, 30);
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("empty_doc_placeholder") && !e.exiting));
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("image_caption_markdown") && !e.exiting));
    }

    #[test]
    fn test_low_zoom_renders_title_only_lod() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Low Zoom LOD Test");
        rt.state.inspector_open = false;

        let sec_id = rt
            .state
            .add_study_node(NodeKind::Section, Some(WorldPos::new(60.0, 60.0)));
        let note_id = rt
            .state
            .add_study_node(NodeKind::Flashcard, Some(WorldPos::new(100.0, 120.0)));

        // At zoom = 1.0 -> full card chrome rendered (no node_title_lod_*, sec_toggle_frame_* visible)
        rt.invalidate();
        settle_frames(rt, 20);
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some(&format!("node_title_lod_{}", note_id.as_str()))));
        assert!(rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some(&format!("sec_toggle_frame_{}", sec_id.as_str()))));

        // Zoom out to 0.30 -> title-only LOD enabled, inner card clutter hidden
        rt.state.graph.viewport.zoom = 0.30;
        rt.invalidate();
        settle_frames(rt, 45);
        assert!(rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some(&format!("node_title_lod_{}", note_id.as_str())) && !e.exiting));
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some(&format!("node_body_{}", note_id.as_str())) && !e.exiting));
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some(&format!("node_del_{}", note_id.as_str())) && !e.exiting));
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some(&format!("sec_toggle_frame_{}", sec_id.as_str())) && !e.exiting));
    }

    #[test]
    fn test_settings_page_navigation_and_api_key_persistence() {
        let mut th = make_test_runtime();
        let dir_path = th._dir.path().to_path_buf();
        let rt = &mut th.rt;
        assert_eq!(rt.state.screen, AppScreen::Dashboard);

        click_key(rt, "btn_open_settings");
        settle_frames(rt, 12);
        assert_eq!(rt.state.screen, AppScreen::Settings);
        assert!(rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("settings_ai_section") && !e.exiting));

        rt.state.ai_api_key_edit = rune::TextEdit::new("  sk-demo-test-key  ");
        rt.state.ai_base_url_edit = rune::TextEdit::new("https://openrouter.ai/api/v1");
        rt.state.ai_model_edit = rune::TextEdit::new("openai/gpt-4o-mini");
        rt.state.vision_api_key_edit = rune::TextEdit::new("AIzaDemoVisionKey");
        click_key(rt, "btn_settings_save");
        settle_frames(rt, 12);

        assert_eq!(rt.state.reader_prefs.ai_api_key, "sk-demo-test-key");
        assert_eq!(
            rt.state.reader_prefs.ai_base_url,
            "https://openrouter.ai/api/v1"
        );
        assert_eq!(rt.state.reader_prefs.ai_model, "openai/gpt-4o-mini");
        assert!(rt.state.ai_api_configured());

        let reloaded = crate::doc_reader::load_preferences(&dir_path);
        assert_eq!(reloaded.ai_api_key, "sk-demo-test-key");
        assert_eq!(reloaded.ai_base_url, "https://openrouter.ai/api/v1");
        assert_eq!(reloaded.ai_model, "openai/gpt-4o-mini");
        assert_eq!(reloaded.vision_api_key, "AIzaDemoVisionKey");

        click_key(rt, "btn_settings_back");
        settle_frames(rt, 12);
        assert_eq!(rt.state.screen, AppScreen::Dashboard);

        rt.state.create_project("Settings Roundtrip");
        settle_frames(rt, 12);
        assert_eq!(rt.state.screen, AppScreen::Workspace);

        click_key(rt, "btn_open_settings");
        settle_frames(rt, 12);
        assert_eq!(rt.state.screen, AppScreen::Settings);

        click_key(rt, "btn_settings_back");
        settle_frames(rt, 12);
        assert_eq!(rt.state.screen, AppScreen::Workspace);
    }

    struct TestCustomLlmProvider;

    impl studynodes_llm::LlmProvider for TestCustomLlmProvider {
        fn generate_patch<'life0, 'life1, 'async_trait>(
            &'life0 self,
            _context: &'life1 studynodes_llm::LlmContext,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<studynodes_graph::patch::GraphPatch, studynodes_llm::LlmError>,
                    > + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                Ok(studynodes_graph::patch::GraphPatch {
                    summary: "Custom test patch".into(),
                    ops: vec![],
                })
            })
        }

        fn summarize<'life0, 'life1, 'async_trait>(
            &'life0 self,
            context: &'life1 studynodes_llm::LlmContext,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<String, studynodes_llm::LlmError>>
                    + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            let title = context
                .focus_nodes
                .first()
                .map(|n| n.title.clone())
                .unwrap_or_default();
            Box::pin(async move {
                Ok(format!(
                    "Live API Summary for {}: NMDA gating pairs pre/post spiking.",
                    title
                ))
            })
        }
    }

    fn poll_llm_until_idle(rt: &mut Runtime<AppState>) {
        for _ in 0..80 {
            if rt.state.poll_llm_jobs()
                && rt.state.llm_pending_nodes.is_empty()
                && rt.state.llm_pending_patches == 0
                && !rt.state.ai_chat_pending
                && rt.state.ai_chat_pending_target.is_none()
            {
                break;
            }
            if rt.state.llm_pending_nodes.is_empty()
                && rt.state.llm_pending_patches == 0
                && !rt.state.ai_chat_pending
                && rt.state.ai_chat_pending_target.is_none()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        rt.state.poll_llm_jobs();
        rt.invalidate();
        settle_frames(rt, 12);
    }

    #[test]
    fn test_async_ai_summary_with_custom_provider_and_readonly_display() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        let cb = rune::clipboard::memory();
        rt.use_clipboard(&cb);

        rt.state.create_project("Live AI Summary Test");
        rt.state
            .set_llm_provider_override(std::sync::Arc::new(TestCustomLlmProvider));

        let doc_id = rt
            .state
            .add_document_node(Some(WorldPos::new(140.0, 120.0)));
        rt.state.deselect_all();
        if let Some(doc) = rt.state.graph.get_node_mut(&doc_id) {
            doc.title = "Synaptic Plasticity".into();
            doc.content = "NMDA receptors act as molecular coincidence detectors.".into();
        }
        rt.state.select_node(doc_id.clone(), false);
        rt.state.inspector_open = true;
        rt.state.set_reader_read_only(true);
        rt.invalidate();
        settle_frames(rt, 20);

        click_key(rt, "btn_inspector_more");
        settle_frames(rt, 10);
        click_key(rt, "btn_readonly_summarize");
        poll_llm_until_idle(rt);

        let expected = "Live API Summary for Synaptic Plasticity: NMDA gating pairs pre/post spiking.";
        assert_eq!(
            rt.state
                .graph
                .get_node(&doc_id)
                .and_then(|n| n.summary.as_deref()),
            Some(expected)
        );
        assert!(rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("ai_summary_callout") && !e.exiting));

        click_key(rt, "btn_copy_ai_summary");
        settle_frames(rt, 6);
        assert_eq!(cb.text().as_deref().map(String::as_str), Some(expected));

        click_key(rt, "btn_clear_ai_summary");
        settle_frames(rt, 12);
        assert!(rt
            .state
            .graph
            .get_node(&doc_id)
            .and_then(|n| n.summary.as_ref())
            .is_none());
    }

    #[test]
    fn test_theme_selector_switches_and_persists() {
        let mut th = make_test_runtime();
        let _theme_guard = TEST_THEME_LOCK.write().unwrap_or_else(|e| e.into_inner());
        let dir_path = th._dir.path().to_path_buf();
        let rt = &mut th.rt;

        click_key(rt, "btn_open_settings");
        settle_frames(rt, 12);
        assert!(rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("settings_appearance_section") && !e.exiting));

        click_key(rt, "btn_theme_warm_paper");
        settle_frames(rt, 12);
        assert_eq!(rt.state.reader_prefs.theme, theme::ThemeId::WarmPaper);
        assert_eq!(theme::active_theme(), theme::ThemeId::WarmPaper);

        let reloaded = crate::doc_reader::load_preferences(&dir_path);
        assert_eq!(reloaded.theme, theme::ThemeId::WarmPaper);

        click_key(rt, "btn_theme_warm_charcoal");
        settle_frames(rt, 12);
        assert_eq!(rt.state.reader_prefs.theme, theme::ThemeId::WarmCharcoal);
        assert_eq!(theme::active_theme(), theme::ThemeId::WarmCharcoal);
    }

    #[test]
    fn test_fullscreen_toggle_button_and_f11() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        assert!(!rt.state.is_fullscreen);

        click_key(rt, "btn_toggle_fullscreen");
        settle_frames(rt, 6);
        assert!(rt.state.is_fullscreen);

        rt.key(KeyEvent {
            key: Key::Function(11),
            mods: Mods::default(),
        });
        settle_frames(rt, 6);
        assert!(!rt.state.is_fullscreen);

        rt.state.create_project("Fullscreen Workspace");
        settle_frames(rt, 12);
        click_key(rt, "btn_toggle_fullscreen");
        settle_frames(rt, 6);
        assert!(rt.state.is_fullscreen);
    }

    #[test]
    fn test_i18n_settings_switch_and_persistence() {
        let mut th = make_test_runtime();
        let _lock = TEST_THEME_LOCK.write().unwrap_or_else(|e| e.into_inner());
        let dir_path = th._dir.path().to_path_buf();
        let rt = &mut th.rt;

        click_key(rt, "btn_open_settings");
        settle_frames(rt, 12);
        assert!(rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("btn_locale_es") && !e.exiting));

        click_key(rt, "btn_locale_es");
        settle_frames(rt, 12);
        assert_eq!(rt.state.reader_prefs.ui_locale, i18n::UiLocale::Es);
        assert_eq!(i18n::active_locale(), i18n::UiLocale::Es);
        assert_eq!(i18n::t(i18n::TextKey::DashTitle), "Espacios de trabajo");

        let reloaded = crate::doc_reader::load_preferences(&dir_path);
        assert_eq!(reloaded.ui_locale, i18n::UiLocale::Es);

        click_key(rt, "btn_locale_fr");
        settle_frames(rt, 12);
        assert_eq!(rt.state.reader_prefs.ui_locale, i18n::UiLocale::Fr);
        assert_eq!(i18n::active_locale(), i18n::UiLocale::Fr);
        assert_eq!(i18n::t(i18n::TextKey::DashTitle), "Espaces de travail");

        click_key(rt, "btn_locale_en");
        settle_frames(rt, 12);
        assert_eq!(rt.state.reader_prefs.ui_locale, i18n::UiLocale::En);
        assert_eq!(i18n::active_locale(), i18n::UiLocale::En);
    }

    struct RecordingLangLlmProvider {
        seen_lang: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    }

    impl studynodes_llm::LlmProvider for RecordingLangLlmProvider {
        fn generate_patch<'life0, 'life1, 'async_trait>(
            &'life0 self,
            _context: &'life1 studynodes_llm::LlmContext,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            studynodes_graph::patch::GraphPatch,
                            studynodes_llm::LlmError,
                        >,
                    > + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                Ok(studynodes_graph::patch::GraphPatch {
                    summary: "noop".into(),
                    ops: vec![],
                })
            })
        }

        fn summarize<'life0, 'life1, 'async_trait>(
            &'life0 self,
            context: &'life1 studynodes_llm::LlmContext,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<String, studynodes_llm::LlmError>>
                    + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            if let Ok(mut guard) = self.seen_lang.lock() {
                *guard = context.preferred_language.clone();
            }
            let lang = context
                .preferred_language
                .clone()
                .unwrap_or_else(|| "Source".into());
            Box::pin(async move { Ok(format!("Summary in {}", lang)) })
        }
    }

    #[test]
    fn test_ai_summary_language_toggle_sets_preferred_language() {
        let mut th = make_test_runtime();
        let _lock = TEST_THEME_LOCK.write().unwrap_or_else(|e| e.into_inner());
        let dir_path = th._dir.path().to_path_buf();
        let rt = &mut th.rt;

        let seen_lang = std::sync::Arc::new(std::sync::Mutex::new(None));
        rt.state
            .set_llm_provider_override(std::sync::Arc::new(RecordingLangLlmProvider {
                seen_lang: seen_lang.clone(),
            }));

        click_key(rt, "btn_open_settings");
        settle_frames(rt, 12);
        click_key(rt, "btn_locale_es");
        settle_frames(rt, 8);
        click_key(rt, "btn_toggle_ai_summary_lang");
        settle_frames(rt, 8);
        assert!(rt.state.reader_prefs.ai_summary_follow_ui_language);

        let reloaded = crate::doc_reader::load_preferences(&dir_path);
        assert!(reloaded.ai_summary_follow_ui_language);

        click_key(rt, "btn_settings_back");
        settle_frames(rt, 8);
        rt.state.create_project("Lang Trigger Project");
        let nid = rt.state.add_note_node(Some(WorldPos::new(100.0, 100.0)));
        settle_frames(rt, 10);

        rt.state.explain_node_ai(&nid);
        poll_llm_until_idle(rt);
        assert_eq!(
            seen_lang.lock().unwrap().as_deref(),
            Some("Spanish")
        );

        // Toggle back to source document language
        rt.state.toggle_ai_summary_language_mode();
        assert!(!rt.state.reader_prefs.ai_summary_follow_ui_language);
        rt.state.explain_node_ai(&nid);
        poll_llm_until_idle(rt);
        assert_eq!(*seen_lang.lock().unwrap(), None);

        rt.state.set_ui_locale(i18n::UiLocale::En);
    }

    #[test]
    fn test_canvas_frustum_culling_skips_offscreen_nodes() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;

        rt.state.create_project("Frustum Cull Test");
        let on_screen = rt.state.add_note_node(Some(WorldPos::new(160.0, 140.0)));
        let off_screen = rt
            .state
            .add_note_node(Some(WorldPos::new(10_000.0, 10_000.0)));
        rt.state.deselect_all();
        rt.invalidate();
        settle_frames(rt, 12);

        let mounted_keys: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect();
        assert!(
            mounted_keys.contains(&on_screen.as_str().to_string()),
            "expected on-screen node to be mounted"
        );
        assert!(
            !mounted_keys.contains(&off_screen.as_str().to_string()),
            "expected far off-screen node to be frustum-culled"
        );
    }

    #[test]
    fn test_asset_path_cache_and_doc_preview_cache_invalidation() {
        let mut th = make_test_runtime();
        let dir_path = th._dir.path().to_path_buf();
        let rt = &mut th.rt;

        rt.state.create_project("Cache Invalidation Test");
        let doc_id = rt
            .state
            .add_document_node(Some(WorldPos::new(120.0, 120.0)));
        let sample_md = dir_path.join("cached_notes.md");
        std::fs::write(&sample_md, "# Page One\nAlpha line\n").unwrap();
        rt.state.ingest_file_into_node(&doc_id, &sample_md);

        let hash = rt
            .state
            .graph
            .get_node(&doc_id)
            .and_then(|n| n.asset_hash.clone())
            .expect("ingested asset hash");
        assert!(
            rt.state.asset_path_cache.contains_key(&hash),
            "asset_path_cache should be populated immediately upon ingest"
        );

        let node_v1 = rt.state.graph.get_node(&doc_id).unwrap().clone();
        let pages_v1 = rt.state.doc_preview_pages_for(&node_v1, 5);
        assert_eq!(pages_v1[0].heading, "Page One");

        // Mutate document content and verify doc_preview_cache invalidates cleanly
        rt.state.deselect_all();
        if let Some(doc) = rt.state.graph.get_node_mut(&doc_id) {
            doc.content = "# Updated Heading\nBeta line\n".into();
        }
        let node_v2 = rt.state.graph.get_node(&doc_id).unwrap().clone();
        let pages_v2 = rt.state.doc_preview_pages_for(&node_v2, 5);
        assert_eq!(pages_v2[0].heading, "Updated Heading");
    }

    fn write_minimal_two_page_pdf(path: &std::path::Path) {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"%PDF-1.4\n");
        let off1 = buf.len();
        buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        let off2 = buf.len();
        buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>\nendobj\n");
        let off3 = buf.len();
        buf.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 120] /Contents 4 0 R >>\nendobj\n");
        let off4 = buf.len();
        let s1 = b"0.15 0.20 0.30 rg 10 10 180 100 re f";
        buf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", s1.len()).as_bytes());
        buf.extend_from_slice(s1);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        let off5 = buf.len();
        buf.extend_from_slice(b"5 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 140] /Contents 6 0 R >>\nendobj\n");
        let off6 = buf.len();
        let s2 = b"0.30 0.15 0.20 rg 15 15 170 110 re f";
        buf.extend_from_slice(format!("6 0 obj\n<< /Length {} >>\nstream\n", s2.len()).as_bytes());
        buf.extend_from_slice(s2);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        let xref_off = buf.len();
        buf.extend_from_slice(b"xref\n0 7\n0000000000 65535 f \n");
        for off in [off1, off2, off3, off4, off5, off6] {
            buf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
        }
        buf.extend_from_slice(
            format!(
                "trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                xref_off
            )
            .as_bytes(),
        );
        std::fs::write(path, buf).unwrap();
    }

    #[test]
    fn test_pdf_multipage_async_render_and_layout_modes() {
        let mut th = make_test_runtime();
        let dir_path = th._dir.path().to_path_buf();
        let rt = &mut th.rt;

        let pdf_path = dir_path.join("two_page.pdf");
        write_minimal_two_page_pdf(&pdf_path);

        rt.state.create_project("MultiPage PDF Test");
        let doc_id = rt
            .state
            .add_document_node(Some(WorldPos::new(140.0, 120.0)));
        rt.state.ingest_file_into_node(&doc_id, &pdf_path);
        rt.state.inspector_open = true;

        // Poll background PDF worker until both page 0 and page 1 finish rendering
        for _ in 0..200 {
            rt.state.poll_background_io();
            let ready = rt
                .state
                .active_pdf
                .as_ref()
                .is_some_and(|p| p.page_count == 2 && p.pages.len() >= 2);
            if ready {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        rt.state.poll_background_io();
        rt.invalidate();
        settle_frames(rt, 16);

        let pdf = rt.state.active_pdf.as_ref().expect("active_pdf loaded");
        assert_eq!(pdf.page_count, 2);
        assert_eq!(pdf.page_dimensions.len(), 2);
        assert!(pdf.pages.contains_key(&0));
        assert!(pdf.pages.contains_key(&1));

        // Default Continuous mode mounts both page 0 and page 1 in the inspector scroll stack
        let keys_cont: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect();
        assert!(keys_cont.contains(&"pdf_page_canvas_0".to_string()));
        assert!(keys_cont.contains(&"pdf_page_canvas_1".to_string()));

        // Switch to 2-Page Spread mode
        click_key(rt, "btn_pdf_mode_twopage");
        settle_frames(rt, 10);
        assert_eq!(
            rt.state.reader_prefs.pdf_layout_mode,
            doc_reader::PdfLayoutMode::TwoPage
        );

        // Switch to Single Page mode: only current page (0) is mounted
        click_key(rt, "btn_pdf_mode_single");
        settle_frames(rt, 10);
        assert_eq!(
            rt.state.reader_prefs.pdf_layout_mode,
            doc_reader::PdfLayoutMode::SinglePage
        );
        let keys_single: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect();
        assert!(keys_single.contains(&"pdf_page_canvas_0".to_string()));
        assert!(!keys_single.contains(&"pdf_page_canvas_1".to_string()));
    }

    #[test]
    fn test_project_ai_chat_and_spawn_note() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;

        rt.state.create_project("Biology Chat Project");
        let n1 = rt.state.add_note_node(Some(WorldPos::new(120.0, 120.0)));
        if let Some(node) = rt.state.graph.get_node_mut(&n1) {
            node.title = "Synaptic Plasticity".into();
            node.content = "Long-term potentiation strengthens active synapses.".into();
        }
        rt.invalidate();
        settle_frames(rt, 16);
        let initial_node_count = rt.state.graph.nodes.len();

        // Click "✦ Chat" directly on the floating dock bar (no dropdown required)
        click_key(rt, "btn_ai_open_chat");
        settle_frames(rt, 12);

        assert!(rt.state.inspector_open);
        assert!(rt.state.ai_panel_open);

        // Simulate an error banner in chat and verify the × close button dismisses it
        rt.state.llm_last_error = Some("Network error: request failed".into());
        rt.invalidate();
        settle_frames(rt, 6);
        click_key(rt, "btn_dismiss_ai_error");
        settle_frames(rt, 6);
        assert!(rt.state.llm_last_error.is_none());

        // Type a question into the project AI chat input and send it
        rt.state.ai_chat_input = rune::TextEdit::new("How do the claims and papers relate?");
        rt.invalidate();
        settle_frames(rt, 6);
        click_key(rt, "btn_ai_chat_send");
        settle_frames(rt, 12);

        assert_eq!(rt.state.ai_chat_messages.len(), 2);
        assert_eq!(rt.state.ai_chat_messages[0].role, "user");
        assert_eq!(rt.state.ai_chat_messages[1].role, "assistant");
        assert!(
            rt.state.ai_chat_messages[1]
                .text
                .contains("How do the claims and papers relate?")
        );

        let keys: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect();
        assert!(keys.contains(&"ai_chat_topbar".to_string()));
        assert!(!keys.contains(&"btn_run_ai_synth".to_string()));
        assert!(!keys.contains(&"btn_ai_generate_cards".to_string()));
        assert!(keys.contains(&"ai_chat_msg_0".to_string()));
        assert!(keys.contains(&"ai_chat_msg_1".to_string()));
        assert!(keys.contains(&"ai_chat_md_1".to_string()));
        assert!(keys.contains(&"btn_ai_chat_spawn_note_1".to_string()));

        // Spawn a canvas Note directly from the AI chat response
        click_key(rt, "btn_ai_chat_spawn_note_1");
        settle_frames(rt, 10);
        assert_eq!(rt.state.graph.nodes.len(), initial_node_count + 1);

        // Toggle inline custom Chat API key drawer and save a dedicated Chat API key
        click_key(rt, "btn_ai_chat_toggle_key_drawer");
        settle_frames(rt, 8);
        assert!(rt.state.ai_chat_key_drawer_open);

        rt.state.chat_api_key_edit = rune::TextEdit::new("sk-custom-chat-key-123");
        rt.state.chat_model_edit = rune::TextEdit::new("gpt-4o");
        rt.invalidate();
        settle_frames(rt, 4);
        click_key(rt, "btn_inline_chat_key_save");
        settle_frames(rt, 8);

        assert!(!rt.state.ai_chat_key_drawer_open);
        assert!(rt.state.chat_has_custom_api_key());
        assert_eq!(rt.state.reader_prefs.chat_api_key, "sk-custom-chat-key-123");
        assert_eq!(rt.state.reader_prefs.chat_model, "gpt-4o");
        let (_, provider_label) = rt
            .state
            .resolve_chat_ai_provider()
            .expect("dedicated chat provider");
        assert!(provider_label.contains("gpt-4o"));
    }

    #[test]
    fn test_collections_and_project_export_import() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        let export_dir = TempDir::new().expect("export dir");

        // Create a collection via Dashboard UI
        click_key(rt, "btn_new_collection");
        settle_frames(rt, 6);
        rt.state.new_collection_input = rune::TextEdit::new("Physics");
        rt.invalidate();
        settle_frames(rt, 4);
        click_key(rt, "btn_confirm_new_collection");
        settle_frames(rt, 6);
        assert!(rt.state.all_collections().contains(&"Physics".to_string()));

        // Create two projects
        rt.state.create_project("Quantum Notes");
        let _n_id = rt.state.add_note_node(Some(WorldPos::new(120.0, 140.0)));
        rt.state.canvas_title_edit = rune::TextEdit::new("Wavefunction");
        rt.state.sync_from_canvas_edits();
        let orig_project_id = rt.state.graph.project_id.clone();
        rt.state.flush_graph_save_now();
        let q_path = rt.state.bundle.as_ref().unwrap().root_dir.clone();

        rt.state.create_project("History Essay");
        rt.state.flush_graph_save_now();
        rt.state.close_to_dashboard();
        rt.invalidate();
        settle_frames(rt, 12);

        let q_idx = rt
            .state
            .recent_projects
            .iter()
            .position(|p| p.title == "Quantum Notes")
            .expect("Quantum Notes in recent projects");
        let h_idx = rt
            .state
            .recent_projects
            .iter()
            .position(|p| p.title == "History Essay")
            .expect("History Essay in recent projects");

        // Assign Quantum Notes to "Physics" via inline row picker
        click_key(rt, &format!("btn_assign_col_{}", q_idx));
        settle_frames(rt, 6);
        click_key(rt, &format!("col_pick_{}_0", q_idx));
        settle_frames(rt, 8);
        assert_eq!(
            rt.state.recent_projects[q_idx].collection.as_deref(),
            Some("Physics")
        );

        // Filter Dashboard by Physics collection
        click_key(rt, "col_filter_0");
        settle_frames(rt, 8);
        let keys_phys: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect();
        assert!(keys_phys.iter().any(|k| k.starts_with(&format!("proj_row_{}_", q_idx))));
        assert!(!keys_phys.iter().any(|k| k.starts_with(&format!("proj_row_{}_", h_idx))));

        // Filter Dashboard by Uncategorized
        click_key(rt, "col_filter_uncat");
        settle_frames(rt, 8);
        let keys_uncat: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect();
        assert!(!keys_uncat.iter().any(|k| k.starts_with(&format!("proj_row_{}_", q_idx))));
        assert!(keys_uncat.iter().any(|k| k.starts_with(&format!("proj_row_{}_", h_idx))));

        click_key(rt, "col_filter_all");
        settle_frames(rt, 6);

        // Export Quantum Notes and import it as a new project copy
        let archive_path = export_dir.path().join("quantum.studynodes");
        rt.state.export_project_to_archive(&q_path, &archive_path);
        assert!(archive_path.exists());

        rt.state.import_project_from_archive(&archive_path);
        rt.invalidate();
        settle_frames(rt, 12);
        assert_eq!(rt.state.screen, AppScreen::Workspace);
        assert_eq!(rt.state.graph.title, "Quantum Notes");
        assert_eq!(rt.state.graph.collection.as_deref(), Some("Physics"));
        assert_ne!(rt.state.graph.project_id, orig_project_id);
        assert!(
            rt.state
                .graph
                .nodes
                .values()
                .any(|n| n.title == "Wavefunction")
        );
    }

    #[test]
    fn test_video_audio_timestamps_and_calendar_node() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;

        rt.state.create_project("Media & Calendar Studio");
        rt.invalidate();
        settle_frames(rt, 10);

        // Open '+' block menu and add Video, Audio, and Calendar nodes
        click_key(rt, "btn_toggle_block_menu");
        settle_frames(rt, 8);
        click_key(rt, "btn_add_video");
        settle_frames(rt, 8);

        click_key(rt, "btn_toggle_block_menu");
        settle_frames(rt, 8);
        click_key(rt, "btn_add_audio");
        settle_frames(rt, 8);

        click_key(rt, "btn_toggle_block_menu");
        settle_frames(rt, 8);
        click_key(rt, "btn_add_calendar");
        settle_frames(rt, 8);

        let vid_id = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Video)
            .expect("video node")
            .id
            .clone();
        let aud_id = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Audio)
            .expect("audio node")
            .id
            .clone();
        let cal_id = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Calendar)
            .expect("calendar node")
            .id
            .clone();
        assert_ne!(vid_id, aud_id);

        // Select Video node, save YouTube URL, add timestamp at 01:35, and jump via URL request
        rt.state.select_node(vid_id.clone(), false);
        rt.state.inspector_open = true;
        rt.state.ai_panel_open = false;
        rt.state.media_url_edit =
            rune::TextEdit::new("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        rt.invalidate();
        settle_frames(rt, 6);
        click_key(rt, "btn_save_media_url");
        settle_frames(rt, 6);

        rt.state.media_ts_time_edit = rune::TextEdit::new("01:35");
        rt.state.media_ts_label_edit = rune::TextEdit::new("Key derivation");
        rt.invalidate();
        settle_frames(rt, 4);
        click_key(rt, "btn_add_media_ts");
        settle_frames(rt, 8);

        assert_eq!(rt.state.graph.get_node(&vid_id).unwrap().meta.timestamps.len(), 1);
        click_key(rt, &format!("media_ts_{}_0", vid_id.as_str()));
        settle_frames(rt, 4);
        let embeds_after_ts = rt.state.desired_embeds();
        assert_eq!(embeds_after_ts.len(), 1);
        assert!(embeds_after_ts[0].iframe_src.contains("start=95"));

        // External link button still opens the timestamped YouTube watch URL in browser
        click_key(rt, &format!("media_open_url_{}", vid_id.as_str()));
        settle_frames(rt, 4);
        let urls = rt.take_url_requests();
        assert_eq!(
            urls,
            vec!["https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string()]
        );

        // Select Calendar node, step month, add & toggle a scheduled event
        rt.state.select_node(cal_id.clone(), false);
        rt.invalidate();
        settle_frames(rt, 8);
        let initial_month = rt.state.graph.get_node(&cal_id).unwrap().meta.calendar_month;
        click_key(rt, "btn_insp_cal_next");
        settle_frames(rt, 6);
        assert_ne!(
            rt.state.graph.get_node(&cal_id).unwrap().meta.calendar_month,
            initial_month
        );

        rt.state.cal_event_date_edit = rune::TextEdit::new("2026-11-28");
        rt.state.cal_event_time_edit = rune::TextEdit::new("10:00");
        rt.state.cal_event_title_edit = rune::TextEdit::new("Final Review");
        rt.invalidate();
        settle_frames(rt, 4);
        click_key(rt, "btn_add_cal_event");
        settle_frames(rt, 8);

        assert_eq!(
            rt.state.graph.get_node(&cal_id).unwrap().meta.calendar_events.len(),
            1
        );
        click_key(rt, "btn_insp_cal_toggle_0");
        settle_frames(rt, 6);
        assert!(
            rt.state.graph.get_node(&cal_id).unwrap().meta.calendar_events[0].completed
        );
    }

    #[test]
    fn test_ai_chat_direct_canvas_actions_calendar_and_undo() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;

        rt.state.create_project("AI Canvas Control");
        let n_id = rt.state.add_note_node(Some(WorldPos::new(140.0, 140.0)));
        if let Some(n) = rt.state.graph.get_node_mut(&n_id) {
            n.title = "Linear Algebra".into();
            n.content = "Eigenvalues and eigenvectors review.".into();
        }
        rt.invalidate();
        settle_frames(rt, 12);

        click_key(rt, "btn_ai_open_chat");
        settle_frames(rt, 8);
        assert!(rt.state.ai_chat_canvas_actions);

        rt.state.ai_chat_input =
            rune::TextEdit::new("Schedule a Linear Algebra review on my calendar");
        rt.invalidate();
        settle_frames(rt, 4);
        click_key(rt, "btn_ai_chat_send");
        settle_frames(rt, 12);

        assert_eq!(rt.state.ai_chat_messages.len(), 2);
        assert!(rt.state.ai_chat_messages[1].action_summary.is_some());
        assert!(rt.state.ai_chat_messages[1].can_undo);
        assert_eq!(
            rt.state
                .graph
                .nodes
                .values()
                .filter(|n| n.kind == NodeKind::Calendar)
                .count(),
            1
        );
        let cal_node = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Calendar)
            .unwrap();
        assert_eq!(cal_node.meta.calendar_events.len(), 1);

        // One-click atomic undo in chat
        click_key(rt, "btn_ai_chat_undo_1");
        settle_frames(rt, 8);
        assert!(!rt.state.ai_chat_messages[1].can_undo);
        assert_eq!(
            rt.state
                .graph
                .nodes
                .values()
                .filter(|n| n.kind == NodeKind::Calendar)
                .count(),
            0
        );

        // Keyboard Redo (Cmd/Ctrl+Y) then Undo (Cmd/Ctrl+Z)
        rt.key(KeyEvent {
            key: Key::Command('y'),
            mods: Mods {
                cmd: true,
                ..Mods::default()
            },
        });
        settle_frames(rt, 8);
        assert_eq!(
            rt.state
                .graph
                .nodes
                .values()
                .filter(|n| n.kind == NodeKind::Calendar)
                .count(),
            1
        );

        rt.key(KeyEvent {
            key: Key::Command('z'),
            mods: Mods {
                cmd: true,
                ..Mods::default()
            },
        });
        settle_frames(rt, 8);
        assert_eq!(
            rt.state
                .graph
                .nodes
                .values()
                .filter(|n| n.kind == NodeKind::Calendar)
                .count(),
            0
        );
    }

    #[test]
    fn test_ai_edit_selected_node_syncs_buffers_and_spawns_in_viewport() {
        use studynodes_graph::patch::{GraphPatch, PatchOp};

        let mut th = make_test_runtime();
        let rt = &mut th.rt;

        rt.state.create_project("AI Viewport & Sync Test");
        let n_id = rt.state.add_note_node(Some(WorldPos::new(120.0, 120.0)));
        rt.state.canvas_title_edit = rune::TextEdit::new("Cell Biology");
        rt.state.canvas_content_edit = rune::TextEdit::new("Original text");
        rt.state.sync_from_canvas_edits();
        rt.state.select_node(n_id.clone(), false);
        rt.invalidate();
        settle_frames(rt, 8);

        // AI edits currently selected node by title -> buffers must sync so sync_node_edits doesn't clobber it
        let patch = GraphPatch {
            summary: "Update Cell Biology".into(),
            ops: vec![PatchOp::UpdateNode {
                node_id: "cell biology".into(),
                title: None,
                content: Some("Updated mitochondria notes".into()),
                answer: None,
                summary: None,
                media_url: None,
                quiz_questions: None,
                quiz_mode: None,
                quiz_difficulty: None,
            }],
        };
        let res = rt.state.apply_ai_patch_with_undo(&patch);
        assert_eq!(res.updated_nodes.len(), 1);
        assert_eq!(rt.state.canvas_content_edit.text, "Updated mitochondria notes");
        assert_eq!(rt.state.inspector_content_edit.text, "Updated mitochondria notes");
        rt.state.sync_node_edits();
        assert_eq!(
            rt.state.graph.get_node(&n_id).unwrap().content,
            "Updated mitochondria notes"
        );

        // Pan far away, then apply CreateStudyNode -> must spawn inside the visible viewport
        rt.state.graph.viewport.offset = WorldPos::new(2600.0, 1800.0);
        let spawn_patch = GraphPatch {
            summary: "Add far note".into(),
            ops: vec![PatchOp::CreateStudyNode {
                temp_id: "tmp_far".into(),
                kind: NodeKind::Note,
                title: "Far Viewport Note".into(),
                content: "Visible right here".into(),
                answer: None,
                media_url: None,
                source_nodes: vec![],
                quiz_questions: vec![],
                quiz_mode: None,
                quiz_difficulty: None,
                text_font: None,
                text_size: None,
                x: None,
                y: None,
                pos: None,
                width: None,
                height: None,
            }],
        };
        let res2 = rt.state.apply_ai_patch_with_undo(&spawn_patch);
        assert_eq!(res2.new_nodes.len(), 1);
        let created_pos = res2.new_nodes[0].pos;
        assert!(created_pos.x >= 2600.0);
        assert!(created_pos.y >= 1800.0);
    }

    #[test]
    fn test_calendar_week_mode_day_notes_and_ai_interaction() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;

        rt.state.create_project("Calendar Week Planner");
        rt.invalidate();
        settle_frames(rt, 10);

        click_key(rt, "btn_toggle_block_menu");
        settle_frames(rt, 6);
        click_key(rt, "btn_add_calendar");
        settle_frames(rt, 8);

        let cal_id = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Calendar)
            .expect("calendar node")
            .id
            .clone();

        // Switch Calendar card to Week mode directly on the canvas card
        click_key(rt, &format!("cal_mode_week_{}", cal_id.as_str()));
        settle_frames(rt, 8);
        assert_eq!(
            rt.state.graph.get_node(&cal_id).unwrap().meta.calendar_view_mode,
            studynodes_graph::model::CalendarViewMode::Week
        );
        let (cw, ch) = crate::state::node_world_size(rt.state.graph.get_node(&cal_id).unwrap());
        assert!((cw - 680.0).abs() < 1e-2 && (ch - 200.0).abs() < 1e-2);

        let (ty, tm, td) = media_helper::today_ymd();
        let today_str = media_helper::format_ymd(ty, tm, td);
        let day_key = format!("cal_week_day_{}_{}", cal_id.as_str(), today_str);
        let day_rect = element_rect(rt, &day_key);
        let day_aspect = day_rect[2] / day_rect[3].max(1.0);
        assert!(
            (0.65..=1.35).contains(&day_aspect),
            "week day card should be roughly square (got {}x{}, ratio {:.2})",
            day_rect[2],
            day_rect[3],
            day_aspect
        );
        click_key(rt, &day_key);
        settle_frames(rt, 6);

        rt.state.cal_day_note_edit = rune::TextEdit::new("Complete Lab 3 report");
        rt.state.sync_calendar_day_note_edit(&cal_id);
        rt.invalidate();
        settle_frames(rt, 6);
        assert_eq!(
            rt.state
                .graph
                .get_node(&cal_id)
                .unwrap()
                .meta
                .calendar_day_notes
                .get(&today_str)
                .map(String::as_str),
            Some("Complete Lab 3 report")
        );

        // Open AI Chat and ask AI to update the weekly calendar note
        click_key(rt, "btn_ai_open_chat");
        settle_frames(rt, 8);
        rt.state.ai_chat_input =
            rune::TextEdit::new("Add a week note on my calendar to review organic chemistry");
        rt.invalidate();
        settle_frames(rt, 4);
        click_key(rt, "btn_ai_chat_send");
        settle_frames(rt, 12);

        let note_after_ai = rt
            .state
            .graph
            .get_node(&cal_id)
            .unwrap()
            .meta
            .calendar_day_notes
            .get(&today_str)
            .cloned()
            .unwrap_or_default();
        assert!(note_after_ai.contains("organic chemistry"));
        assert!(rt.state.ai_chat_messages[1].can_undo);

        // One-click Undo restores user's original daily note
        click_key(rt, "btn_ai_chat_undo_1");
        settle_frames(rt, 8);
        assert_eq!(
            rt.state
                .graph
                .get_node(&cal_id)
                .unwrap()
                .meta
                .calendar_day_notes
                .get(&today_str)
                .map(String::as_str),
            Some("Complete Lab 3 report")
        );
    }

    #[test]
    fn test_embedded_direct_video_player_controls_on_card() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;

        rt.state.create_project("Embedded Video Player Test");
        rt.invalidate();
        settle_frames(rt, 10);

        let vid_id = rt
            .state
            .add_study_node(NodeKind::Video, Some(WorldPos::new(140.0, 120.0)));
        rt.state.select_node(vid_id.clone(), false);
        rt.state.inspector_open = true;
        rt.state.ai_panel_open = false;
        rt.state.media_url_edit = rune::TextEdit::new("https://example.com/lecture.mp4");
        rt.invalidate();
        settle_frames(rt, 6);
        click_key(rt, "btn_save_media_url");
        settle_frames(rt, 10);

        assert!(rt.state.media_player_for(&vid_id).is_some());
        let keys: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect();
        let nid = vid_id.as_str();
        for expected in [
            format!("media_play_{}", nid),
            format!("media_back_{}", nid),
            format!("media_fwd_{}", nid),
            format!("media_mute_{}", nid),
            format!("media_rate_{}", nid),
            format!("media_scrub_{}", nid),
            format!("media_mark_{}", nid),
        ] {
            assert!(keys.contains(&expected), "missing player control '{}'", expected);
        }
    }

    #[test]
    fn test_youtube_iframe_embed_and_ai_explain_disabled_on_embeds() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;

        rt.state.create_project("YouTube Iframe & No AI Explain");
        rt.invalidate();
        settle_frames(rt, 10);

        let vid_id = rt
            .state
            .add_study_node(NodeKind::Video, Some(WorldPos::new(140.0, 120.0)));
        rt.state.select_node(vid_id.clone(), false);
        rt.state.inspector_open = true;
        rt.state.ai_panel_open = false;
        rt.state.media_url_edit = rune::TextEdit::new("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        rt.invalidate();
        settle_frames(rt, 6);
        click_key(rt, "btn_save_media_url");
        settle_frames(rt, 10);

        // Iframe embed slot should be rendered instead of LOGIN_REQUIRED API error
        let desired = rt.state.desired_embeds();
        assert_eq!(desired.len(), 1);
        assert!(desired[0].iframe_src.starts_with("https://www.youtube.com/embed/dQw4w9WgXcQ"));

        let keys: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect();
        assert!(keys.contains(&format!("embed_frame_{}", vid_id.as_str())));
        assert!(keys.contains(&format!("media_toggle_embed_{}", vid_id.as_str())));
        assert!(!keys.contains(&"btn_node_ai_explain".to_string()));

        // Clicking more menu also hides AI Explain for Video embed nodes
        click_key(rt, "btn_inspector_more");
        settle_frames(rt, 6);
        let menu_keys: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect();
        assert!(!menu_keys.contains(&"btn_menu_ai_explain".to_string()));
        assert!(menu_keys.contains(&"btn_menu_del_node".to_string()));

        // Direct explain calls are guarded on Video and Audio embed nodes
        rt.state.explain_node_ai(&vid_id);
        assert!(rt.state.graph.get_node(&vid_id).unwrap().summary.is_none());

        // Timestamp jump updates iframe start parameter directly
        rt.state.add_media_timestamp(&vid_id, 95.0, "Chorus".into());
        rt.invalidate();
        settle_frames(rt, 6);
        click_key(rt, "btn_insp_ts_jump_0");
        settle_frames(rt, 6);
        let desired_after = rt.state.desired_embeds();
        assert!(desired_after[0].iframe_src.contains("start=95"));

        // Audio embed also disables AI explain
        let aud_id = rt
            .state
            .add_study_node(NodeKind::Audio, Some(WorldPos::new(520.0, 120.0)));
        rt.state.select_node(aud_id.clone(), false);
        rt.invalidate();
        settle_frames(rt, 8);
        let aud_keys: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect();
        assert!(!aud_keys.contains(&"btn_node_ai_explain".to_string()));
        rt.state.explain_node_ai(&aud_id);
        assert!(rt.state.graph.get_node(&aud_id).unwrap().summary.is_none());
    }

    #[test]
    fn test_audio_automatic_subtitles_on_node_and_details_with_copy() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        let cb = rune::clipboard::memory();
        rt.use_clipboard(&cb);

        rt.state.create_project("Audio Subtitles Test");
        rt.invalidate();
        settle_frames(rt, 10);

        let aud_id = rt
            .state
            .add_study_node(NodeKind::Audio, Some(WorldPos::new(140.0, 140.0)));
        rt.state.select_node(aud_id.clone(), false);
        rt.state.inspector_open = true;
        rt.state.ai_panel_open = false;

        // Simulate automatic subtitle generation completing on the Audio node
        let _ = rt.state.media_resolve_tx.send(crate::state::MediaJobResult::SubtitlesGenerated {
            node_id: aud_id.clone(),
            result: Ok(studynodes_llm::AudioTranscriptionResult {
                text: "Neuroplasticity rewires synapses. Hebbian learning strengthens pathways.".into(),
                segments: vec![
                    studynodes_llm::AudioTranscriptSegment {
                        start: 0.0,
                        end: 3.5,
                        text: "Neuroplasticity rewires synapses.".into(),
                    },
                    studynodes_llm::AudioTranscriptSegment {
                        start: 3.5,
                        end: 8.0,
                        text: "Hebbian learning strengthens pathways.".into(),
                    },
                ],
            }),
        });
        assert!(rt.state.poll_media_jobs());
        rt.invalidate();
        settle_frames(rt, 8);

        let node = rt.state.graph.get_node(&aud_id).unwrap();
        assert!(node.meta.show_subtitles_on_card);
        assert_eq!(node.meta.subtitles.len(), 2);
        assert!(crate::state::node_world_size(node).1 > crate::state::node_base_size(NodeKind::Audio).1);

        let keys: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect();
        assert!(keys.contains(&"media_subtitles_section".to_string()));
        assert!(keys.contains(&"btn_insp_copy_subtitles".to_string()));
        assert!(keys.contains(&"btn_insp_copy_subtitles_timed".to_string()));
        assert!(keys.contains(&format!("media_sub_box_{}", aud_id.as_str())));
        assert!(keys.contains(&format!("media_sub_copy_{}", aud_id.as_str())));

        // Copy directly from Audio node card
        click_key(rt, &format!("media_sub_copy_{}", aud_id.as_str()));
        settle_frames(rt, 6);
        assert_eq!(
            cb.text().as_deref().map(|s| s.as_str()),
            Some("Neuroplasticity rewires synapses. Hebbian learning strengthens pathways.")
        );

        // Copy timed transcript from Inspector Details
        click_key(rt, "btn_insp_copy_subtitles_timed");
        settle_frames(rt, 6);
        let timed_cb = cb.text().unwrap();
        assert!(timed_cb.contains("[00:00] Neuroplasticity rewires synapses."));
        assert!(timed_cb.contains("[00:04] Hebbian learning strengthens pathways."));

        // Toggle showing subtitles on node card off
        click_key(rt, "btn_insp_cc_on_node");
        settle_frames(rt, 8);
        assert!(!rt.state.graph.get_node(&aud_id).unwrap().meta.show_subtitles_on_card);
        let keys_hidden: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect();
        assert!(!keys_hidden.contains(&format!("media_sub_box_{}", aud_id.as_str())));
        assert!(keys_hidden.contains(&"media_subtitles_section".to_string()));
    }

    #[test]
    fn test_ai_image_search_action_and_calendar_date_sync() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;

        rt.state.create_project("AI Image Search & Calendar Sync");
        let cal_id = rt
            .state
            .add_study_node(NodeKind::Calendar, Some(WorldPos::new(100.0, 100.0)));
        rt.invalidate();
        settle_frames(rt, 10);

        // 1. AI updates calendar via relative weekday "tomorrow" -> auto-selects & syncs cal_day_note_edit
        let (ty, tm, td) = media_helper::today_ymd();
        let today_str = media_helper::format_ymd(ty, tm, td);
        let tomorrow_str = media_helper::add_days_ymd(&today_str, 1);

        let cal_patch = studynodes_graph::patch::GraphPatch {
            summary: "Note for tomorrow".into(),
            ops: vec![studynodes_graph::patch::PatchOp::SetCalendarDayNote {
                node_id: Some("calendar".into()),
                date: "tomorrow".into(),
                note: "Finish neuroanatomy quiz".into(),
                append: false,
            }],
        };
        let cal_res = rt.state.apply_ai_patch_with_undo(&cal_patch);
        assert_eq!(cal_res.calendar_notes_changed, 1);
        assert_eq!(
            rt.state.selected_calendar_date.get(&cal_id).map(String::as_str),
            Some(tomorrow_str.as_str())
        );
        assert_eq!(rt.state.cal_day_note_edit.text, "Finish neuroanatomy quiz");

        // 2. AI Chat image search creates Image node + applies downloaded image bytes cleanly
        click_key(rt, "btn_ai_open_chat");
        settle_frames(rt, 8);
        rt.state.ai_chat_input = rune::TextEdit::new("Search for an image of a synapse");
        rt.invalidate();
        settle_frames(rt, 4);
        click_key(rt, "btn_ai_chat_send");
        settle_frames(rt, 10);

        let img_id = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Image)
            .expect("Image node spawned by AI search_images op")
            .id
            .clone();

        // Feed valid minimal 1x1 PNG through the background image search result handler
        let minimal_png: Vec<u8> = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let _ = rt.state.media_resolve_tx.send(crate::state::MediaJobResult::ImageSearched {
            node_id: img_id.clone(),
            query: "synapse".into(),
            result: Ok(media_helper::DownloadedImage {
                bytes: minimal_png,
                ext: "png".into(),
                caption: "Chemical Synapse Diagram".into(),
                source_url: "https://commons.wikimedia.org/wiki/File:Synapse.png".into(),
            }),
        });
        assert!(rt.state.poll_media_jobs());
        assert!(rt.state.graph.get_node(&img_id).unwrap().asset_hash.is_some());
        assert!(rt.state.card_image_previews.contains_key(&img_id));
    }

    #[test]
    fn test_embed_node_resize_and_simplified_media_styling() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;

        rt.state.create_project("Resizable Embed & Clean Media Test");
        rt.invalidate();
        settle_frames(rt, 8);

        // 1. Video YouTube embed node starts compact with a corner resize grip and expands embed_frame
        let vid_id = rt
            .state
            .add_study_node(NodeKind::Video, Some(WorldPos::new(120.0, 100.0)));
        rt.state.select_node(vid_id.clone(), false);
        rt.state.inspector_open = true;
        rt.state.ai_panel_open = false;
        rt.state.media_url_edit = rune::TextEdit::new("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        rt.invalidate();
        settle_frames(rt, 6);
        click_key(rt, "btn_save_media_url");
        settle_frames(rt, 10);

        let (w0, h0) = crate::state::node_world_size(rt.state.graph.get_node(&vid_id).unwrap());
        assert!((w0 - 360.0).abs() < 1e-2 && (h0 - 248.0).abs() < 1e-2);

        let frame_key = format!("embed_frame_{}", vid_id.as_str());
        let grip_key = format!("node_resize_{}", vid_id.as_str());
        let frame_before = element_rect(rt, &frame_key);
        let grip_rect = element_rect(rt, &grip_key);
        let gx = grip_rect[0] + grip_rect[2] * 0.5;
        let gy = grip_rect[1] + grip_rect[3] * 0.5;
        let z = rt.state.graph.viewport.zoom;

        rt.pointer_move(gx, gy);
        rt.pointer_down(gx, gy);
        rt.pointer_move(gx + 140.0 * z, gy + 90.0 * z);
        rt.pointer_up(gx + 140.0 * z, gy + 90.0 * z);
        settle_frames(rt, 8);

        let (w1, h1) = crate::state::node_world_size(rt.state.graph.get_node(&vid_id).unwrap());
        assert!(
            w1 > w0 + 100.0 && h1 > h0 + 60.0,
            "dragging video embed resize grip must enlarge node: ({}, {}) -> ({}, {})",
            w0,
            h0,
            w1,
            h1
        );
        let frame_after = element_rect(rt, &frame_key);
        assert!(
            frame_after[2] > frame_before[2] + 80.0 && frame_after[3] > frame_before[3] + 50.0,
            "embed_frame must grow with resized video node: {:?} -> {:?}",
            frame_before,
            frame_after
        );

        // 2. Audio embed node is also resizable and uses simplified clean card controls
        let aud_id = rt
            .state
            .add_study_node(NodeKind::Audio, Some(WorldPos::new(140.0, 400.0)));
        rt.state.select_node(aud_id.clone(), false);
        rt.state.media_url_edit = rune::TextEdit::new("https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT");
        rt.invalidate();
        settle_frames(rt, 6);
        click_key(rt, "btn_save_media_url");
        settle_frames(rt, 10);

        let aud_grip = format!("node_resize_{}", aud_id.as_str());
        let aud_frame = format!("embed_frame_{}", aud_id.as_str());
        let ag = element_rect(rt, &aud_grip);
        let af0 = element_rect(rt, &aud_frame);
        let ax = ag[0] + ag[2] * 0.5;
        let ay = ag[1] + ag[3] * 0.5;

        rt.pointer_move(ax, ay);
        rt.pointer_down(ax, ay);
        rt.pointer_move(ax + 120.0 * z, ay + 80.0 * z);
        rt.pointer_up(ax + 120.0 * z, ay + 80.0 * z);
        settle_frames(rt, 8);

        let af1 = element_rect(rt, &aud_frame);
        assert!(
            af1[2] > af0[2] + 70.0 && af1[3] > af0[3] + 40.0,
            "audio embed frame must grow when resized: {:?} -> {:?}",
            af0,
            af1
        );

        // Ensure MediaOpenLink only has a single arrow symbol (no duplicate '↗ Abrir enlace ↗')
        let link_label = crate::i18n::t(crate::i18n::TextKey::MediaOpenLink);
        assert_eq!(link_label.matches('↗').count(), 1);
    }

    #[test]
    fn test_smooth_trackpad_zoom_proportional_and_interpolated() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Smooth Trackpad Zoom Test");
        rt.invalidate();
        settle_frames(rt, 4);

        let cr = element_rect(rt, "graph_canvas");
        let (cx, cy) = (cr[0] + cr[2] * 0.5, cr[1] + cr[3] * 0.5);
        rt.pointer_move(cx, cy);

        // 1. Small Ctrl+Wheel trackpad scroll deltas scale proportionally instead of jumping 10%
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        let z0 = rt.state.graph.viewport.zoom;
        rt.wheel_event(WheelEvent::new(0.0, 4.0).at(cx, cy).mods(ctrl).precise(true));
        settle_frames(rt, 2);
        let z1 = rt.state.graph.viewport.zoom;
        assert!(
            z1 > z0 && (z1 - z0) < 0.02,
            "small precise Ctrl+Wheel step should be subtle (<2%), got {} -> {}",
            z0,
            z1
        );

        // 2. Opposite MagnifyEvent steps are exponential/symmetric (in then out returns cleanly)
        rt.state.graph.viewport.zoom = 1.0;
        let expand_delta = if cfg!(target_os = "windows") { -0.25 } else { 0.25 };
        let pinch_delta = -expand_delta;
        rt.magnify_event(MagnifyEvent::new(expand_delta).at(cx, cy));
        settle_frames(rt, 2);
        let mid_zoom = rt.state.graph.viewport.zoom;
        assert!(mid_zoom > 1.05 && mid_zoom < 1.30);
        rt.magnify_event(MagnifyEvent::new(pinch_delta).at(cx, cy));
        settle_frames(rt, 2);
        assert!(
            (rt.state.graph.viewport.zoom - 1.0).abs() < 1e-4,
            "symmetric magnify in/out must return to 1.0, got {}",
            rt.state.graph.viewport.zoom
        );

        // 3. With live smooth zoom enabled, target zoom interpolates smoothly over frames
        //    without jumping straight to the target, preserving anchor world coordinates.
        rt.state.smooth_zoom_enabled = true;
        rt.state.graph.viewport.zoom = 1.0;
        rt.state.graph.viewport.offset = WorldPos::ZERO;
        let anchor = ScreenPos::new(cx, cy);
        let world_anchor_before = rt.state.graph.viewport.screen_to_world(anchor);

        rt.state.queue_zoom_at(anchor, 1.5, 18.0, None);
        assert_eq!(rt.state.graph.viewport.zoom, 1.0);
        assert!(rt.state.is_zoom_animating());

        let mut prev_zoom = rt.state.graph.viewport.zoom;
        for step in 1..=25 {
            let changed = rt.state.step_smooth_zoom(step as f64 * (1.0 / 60.0));
            let cur_zoom = rt.state.graph.viewport.zoom;
            if step == 1 {
                assert!(
                    cur_zoom > 1.0 && cur_zoom < 1.35,
                    "first 16ms frame must glide gradually instead of jumping straight to 1.5 (got {})",
                    cur_zoom
                );
            }
            assert!(cur_zoom >= prev_zoom);
            let world_anchor_now = rt.state.graph.viewport.screen_to_world(anchor);
            assert!(
                (world_anchor_now.x - world_anchor_before.x).abs() < 1e-3
                    && (world_anchor_now.y - world_anchor_before.y).abs() < 1e-3
            );
            prev_zoom = cur_zoom;
            if !changed {
                break;
            }
        }
        // Finish any residual frames so it converges accurately to 1.5
        for step in 26..=60 {
            if !rt.state.step_smooth_zoom(step as f64 * (1.0 / 60.0)) {
                break;
            }
        }
        assert!(
            (rt.state.graph.viewport.zoom - 1.5).abs() < 1e-3,
            "smooth zoom must converge to target 1.5, got {}",
            rt.state.graph.viewport.zoom
        );
        assert!(!rt.state.is_zoom_animating());

        // 4. Verify live GPU zoom path incurs ZERO full view commits mid-gesture
        rt.state.graph.viewport.zoom = 1.0;
        rt.state.graph.viewport.offset = WorldPos::ZERO;
        rt.state.flush_live_camera();
        rt.state.smooth_zoom_enabled = true;
        rt.invalidate();
        rt.frame(1.0 / 120.0);
        rt.check_live_only(false);
        let commits_before = rt.stats.commits;
        let live_events_before = rt.live_only_events();
        let expand_delta = if cfg!(target_os = "windows") { -0.25 } else { 0.25 };
        rt.magnify_event(MagnifyEvent::new(expand_delta).at(cx, cy));
        rt.wheel_event(WheelEvent::new(0.0, 20.0).at(cx, cy).mods(ctrl).precise(true));
        rt.frame(1.0 / 120.0);
        assert_eq!(
            rt.stats.commits, commits_before,
            "magnify and Ctrl+wheel zoom within threshold must use GPU live transforms with 0 view commits"
        );
        assert!(
            rt.live_only_events() > live_events_before,
            "zoom events should record live_only frames"
        );
        assert!(
            rt.state.cam_live_zoom.get() > 1.01,
            "initial live zoom step must begin scaling immediately, got {}",
            rt.state.cam_live_zoom.get()
        );
        let t0 = rt.state.last_zoom_step_clock.unwrap_or(0.0);
        for step in 1..=20 {
            if !rt.state.step_smooth_zoom(t0 + step as f64 * (1.0 / 120.0)) {
                break;
            }
            rt.frame(1.0 / 120.0);
        }
        assert_eq!(
            rt.stats.commits, commits_before,
            "smooth zoom animation frames must stay GPU live-only with 0 view commits"
        );
        assert!(
            rt.state.cam_live_zoom.get() > 1.10,
            "live GPU scale must track smooth zoom target, got {}",
            rt.state.cam_live_zoom.get()
        );
        assert!(rt.state.flush_live_camera());
        assert!((rt.state.committed_viewport.zoom - rt.state.graph.viewport.zoom).abs() < 1e-5);
        assert!((rt.state.cam_live_zoom.get() - 1.0).abs() < 1e-5);

        // 5. Repeated '+' zoom button clicks stay GPU-live (<2.5x, 0 commits) & hide WebView embeds while active
        let vid_id = rt.state.add_study_node(NodeKind::Video, None);
        if let Some(vnode) = rt.state.graph.get_node_mut(&vid_id) {
            vnode.meta.media_url = Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".into());
        }
        rt.state
            .embed_rects
            .insert(vid_id.clone(), [120.0, 120.0, 320.0, 180.0]);
        rt.state.graph.viewport.zoom = 1.0;
        rt.state.graph.viewport.offset = WorldPos::ZERO;
        rt.state.cancel_smooth_zoom();
        rt.state.flush_live_camera();
        rt.state.smooth_zoom_enabled = true;
        rt.invalidate();
        rt.frame(1.0 / 120.0);

        let zin = element_rect(rt, "btn_zoom_in");
        let (bx, by) = (zin[0] + zin[2] * 0.5, zin[1] + zin[3] * 0.5);
        rt.pointer_move(bx, by);
        rt.frame(1.0 / 120.0);
        let commits_before = rt.stats.commits;

        for _ in 0..5 {
            rt.pointer_down(bx, by);
            rt.pointer_up(bx, by);
            rt.frame(1.0 / 120.0);
            assert!(!rt.state.should_commit_live_camera());
        }
        let mut t_click = rt.state.last_zoom_step_clock.unwrap_or(0.0);
        for _ in 0..80 {
            t_click += 1.0 / 120.0;
            if !rt.state.step_smooth_zoom(t_click) {
                break;
            }
            assert!(!rt.state.should_commit_live_camera());
            rt.frame(1.0 / 120.0);
        }
        assert_eq!(
            rt.stats.commits, commits_before,
            "5 repeated '+' button clicks (1.15^5 = 2.01x < 2.50x) must stay GPU live-only with 0 view commits"
        );
        let live_z = rt.state.cam_live_zoom.get();
        assert!(
            live_z > 1.80 && live_z < 2.50,
            "expected live zoom in (1.80, 2.50) after 5x '+', got {}",
            live_z
        );

        let embeds_live = rt.state.desired_embeds();
        let entry_live = embeds_live
            .iter()
            .find(|e| e.node_id == vid_id)
            .expect("YouTube embed must stay registered during live zoom");
        assert!(
            !entry_live.visible,
            "WebView2 embed must hide during live zoom/pan to avoid synchronous COM set_bounds stalls"
        );

        assert!(rt.state.flush_live_camera());
        let embeds_settled = rt.state.desired_embeds();
        let entry_settled = embeds_settled
            .iter()
            .find(|e| e.node_id == vid_id)
            .expect("YouTube embed must remain registered after flush");
        assert!(
            entry_settled.visible,
            "WebView2 embed must become visible again once camera settles"
        );
    }

    #[test]
    fn test_dock_position_cycling_coordinates_and_settings() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Dock Position Test");
        rt.invalidate();
        settle_frames(rt, 8);

        assert_eq!(rt.state.reader_prefs.dock_position, doc_reader::DockPosition::Bottom);
        let dock_rect = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("canvas_floating_dock"))
            .expect("canvas_floating_dock present")
            .rect;
        assert!(
            dock_rect[2] < 430.0,
            "floating dock should hug its items tightly without trailing empty space, got w={}",
            dock_rect[2]
        );
        assert!(rt.state.dock_measured_size.is_some());

        let c_bot = canvas_view::floating_overlay_coords(
            [1280.0, 800.0],
            doc_reader::DockPosition::Bottom,
            None,
        );
        let c_top = canvas_view::floating_overlay_coords(
            [1280.0, 800.0],
            doc_reader::DockPosition::Top,
            None,
        );
        let c_left = canvas_view::floating_overlay_coords(
            [1280.0, 800.0],
            doc_reader::DockPosition::Left,
            None,
        );
        let c_right = canvas_view::floating_overlay_coords(
            [1280.0, 800.0],
            doc_reader::DockPosition::Right,
            None,
        );
        assert!(c_bot.1 > 700.0);
        assert!(c_top.1 < 40.0);
        assert!(c_left.0 < 40.0 && c_left.3 > c_left.2);
        assert!(c_right.0 > 1000.0 && c_right.3 > c_right.2);

        click_key(rt, "btn_dock_position");
        assert_eq!(rt.state.reader_prefs.dock_position, doc_reader::DockPosition::Top);
        click_key(rt, "btn_dock_position");
        assert_eq!(rt.state.reader_prefs.dock_position, doc_reader::DockPosition::Left);
        click_key(rt, "btn_dock_position");
        assert_eq!(rt.state.reader_prefs.dock_position, doc_reader::DockPosition::Right);
        click_key(rt, "btn_dock_position");
        assert_eq!(rt.state.reader_prefs.dock_position, doc_reader::DockPosition::Bottom);

        // Settings screen dock buttons update & persist preference
        rt.state.screen = AppScreen::Settings;
        rt.invalidate();
        settle_frames(rt, 8);
        click_key(rt, "btn_dock_pos_left");
        assert_eq!(rt.state.reader_prefs.dock_position, doc_reader::DockPosition::Left);

        let prefs_loaded = doc_reader::load_preferences(&rt.state.projects_dir);
        assert_eq!(prefs_loaded.dock_position, doc_reader::DockPosition::Left);
    }

    #[test]
    fn test_drawing_node_strokes_interpreter_and_spawn() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        let cb = rune::clipboard::memory();
        rt.use_clipboard(&cb);

        rt.state.create_project("Drawing Node Test");
        rt.state.set_vision_service_override(std::sync::Arc::new(
            studynodes_vision::MockVisionService {
                models_ready: true,
                sample_plain: Some("E = mc^2 handwritten".into()),
                sample_equation: Some("$$E = mc^2$$".into()),
            },
        ));
        rt.invalidate();
        settle_frames(rt, 8);

        click_key(rt, "btn_toggle_block_menu");
        settle_frames(rt, 6);
        click_key(rt, "btn_add_drawing");
        settle_frames(rt, 8);

        let draw_id = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Drawing)
            .expect("Drawing node created")
            .id
            .clone();
        rt.state.select_node(draw_id.clone(), false);
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 8);

        // Drawing node on canvas must only display the drawing preview (no inline canvas tool controls)
        // while Details shows the drawing pad & tools and omits Markdown editor/formatting controls.
        let keys: Vec<_> = rt.elements().into_iter().filter_map(|e| e.key).collect();
        let pad_key = format!("drawing_pad_{}", draw_id.as_str());
        assert!(keys.contains(&pad_key));
        assert!(!keys.contains(&format!("drawing_pen_{}", draw_id.as_str())));
        assert!(!keys.contains(&format!("drawing_eraser_{}", draw_id.as_str())));
        assert!(!keys.contains(&format!("drawing_undo_{}", draw_id.as_str())));
        assert!(!keys.contains(&format!("drawing_clear_{}", draw_id.as_str())));
        assert!(!keys.contains(&format!("drawing_interpret_{}", draw_id.as_str())));
        assert!(keys.contains(&"inspector_drawing_pad".to_string()));
        assert!(keys.contains(&"btn_draw_pen".to_string()));
        assert!(keys.contains(&"btn_draw_undo".to_string()));
        assert!(!keys.contains(&"inspector_content".to_string()));
        assert!(!keys.contains(&"fmt_bullet".to_string()));

        // Draw two strokes, undo one, clear, redraw
        rt.state.begin_drawing_stroke(&draw_id, 0.15, 0.25);
        rt.state.extend_drawing_stroke(&draw_id, 0.45, 0.25);
        rt.state.finish_drawing_stroke(&draw_id);
        rt.state.begin_drawing_stroke(&draw_id, 0.20, 0.60);
        rt.state.extend_drawing_stroke(&draw_id, 0.80, 0.60);
        rt.state.finish_drawing_stroke(&draw_id);
        assert_eq!(rt.state.graph.get_node(&draw_id).unwrap().meta.drawing_strokes.len(), 2);

        rt.state.undo_drawing_stroke(&draw_id);
        assert_eq!(rt.state.graph.get_node(&draw_id).unwrap().meta.drawing_strokes.len(), 1);
        rt.state.clear_drawing_strokes(&draw_id);
        assert_eq!(rt.state.graph.get_node(&draw_id).unwrap().meta.drawing_strokes.len(), 0);

        rt.state.begin_drawing_stroke(&draw_id, 0.20, 0.50);
        rt.state.extend_drawing_stroke(&draw_id, 0.80, 0.50);
        rt.state.finish_drawing_stroke(&draw_id);
        let png_path = rt
            .state
            .rasterize_drawing_to_png(&draw_id)
            .expect("rasterized PNG for drawing");
        assert!(png_path.exists());

        // Simulate Pop-Out drawing commands arriving via ReaderBridge
        {
            let mut b = rt.state.reader_bridge.lock().unwrap();
            b.push_command(reader_window::ReaderCommand::BeginDrawingStroke {
                node_id: draw_id.clone(),
                nx: 0.10,
                ny: 0.10,
                color_hex: "#E07A5F".into(),
                width: 4.0,
                eraser: false,
            });
            b.push_command(reader_window::ReaderCommand::ExtendDrawingStroke {
                node_id: draw_id.clone(),
                nx: 0.90,
                ny: 0.15,
                eraser: false,
            });
            b.push_command(reader_window::ReaderCommand::FinishDrawingStroke(
                draw_id.clone(),
            ));
        }
        assert!(rt.state.poll_reader_bridge());
        let strokes = &rt
            .state
            .graph
            .get_node(&draw_id)
            .unwrap()
            .meta
            .drawing_strokes;
        assert_eq!(strokes.len(), 2);
        assert_eq!(strokes[1].color_hex, "#E07A5F");
        assert!(strokes[1].points.len() >= 2);

        rt.invalidate();
        settle_frames(rt, 8);
        click_key(rt, "btn_draw_interpret_text");
        poll_vision_until_idle(rt);
        assert_eq!(
            rt.state
                .graph
                .get_node(&draw_id)
                .unwrap()
                .meta
                .extracted_text
                .as_deref(),
            Some("E = mc^2 handwritten")
        );

        click_key(rt, "btn_draw_copy_ocr");
        assert_eq!(cb.text().as_deref().map(String::as_str), Some("E = mc^2 handwritten"));

        click_key(rt, "btn_draw_spawn_note");
        click_key(rt, "btn_draw_spawn_formula");
        settle_frames(rt, 6);
        assert!(rt
            .state
            .graph
            .nodes
            .values()
            .any(|n| n.kind == NodeKind::Note && n.content.contains("E = mc^2 handwritten")));
        assert!(rt
            .state
            .graph
            .nodes
            .values()
            .any(|n| n.kind == NodeKind::Formula));
    }

    #[test]
    fn test_video_node_has_no_text_properties() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Video Properties Audit");
        let vid_id = rt
            .state
            .add_study_node(NodeKind::Video, Some(WorldPos::new(100.0, 100.0)));
        assert!(rt.state.graph.get_node(&vid_id).unwrap().content.is_empty());

        rt.state.select_node(vid_id, false);
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 10);

        let keys: Vec<_> = rt.elements().into_iter().filter_map(|e| e.key).collect();
        assert!(keys.contains(&"media_inspector_section".to_string()));
        assert!(!keys.contains(&"inspector_content".to_string()));
        assert!(!keys.contains(&"fmt_bullet".to_string()));
        assert!(!keys.contains(&"fmt_heading".to_string()));
        assert!(!keys.contains(&"fmt_todo".to_string()));
        assert!(!keys.contains(&"fmt_quote".to_string()));
    }

    #[test]
    fn test_section_ai_review_populates_summary_from_children() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Section AI Review");
        rt.state
            .set_llm_provider_override(std::sync::Arc::new(studynodes_llm::MockLlmProvider));

        let sec_id = rt
            .state
            .add_study_node(NodeKind::Section, Some(WorldPos::new(0.0, 0.0)));
        let n1 = rt
            .state
            .add_study_node(NodeKind::Note, Some(WorldPos::new(30.0, 70.0)));
        let n2 = rt
            .state
            .add_study_node(NodeKind::Note, Some(WorldPos::new(320.0, 70.0)));
        rt.state.deselect_all();
        if let Some(sec) = rt.state.graph.get_node_mut(&sec_id) {
            sec.title = "Neural Plasticity Group".into();
            sec.size = (640.0, 420.0);
        }
        if let Some(n) = rt.state.graph.get_node_mut(&n1) {
            n.title = "AMPA Trafficking".into();
            n.content = "GluA1 insertion strengthens postsynaptic currents.".into();
        }
        if let Some(n) = rt.state.graph.get_node_mut(&n2) {
            n.title = "CaMKII Activation".into();
            n.content = "Calcium influx triggers autophosphorylation.".into();
        }
        let edge = studynodes_graph::model::Edge::new(
            studynodes_graph::model::EdgeEndpoint {
                node: n2.clone(),
                port: studynodes_graph::model::PortId::new(studynodes_graph::model::PortId::DEFAULT_OUT),
            },
            studynodes_graph::model::EdgeEndpoint {
                node: n1.clone(),
                port: studynodes_graph::model::PortId::new(studynodes_graph::model::PortId::DEFAULT_IN),
            },
        );
        rt.state.graph.add_edge(edge);

        rt.state.select_node(sec_id.clone(), false);
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 6);

        let shot_btn_rect = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("btn_inspector_section_screenshot"))
            .expect("btn_inspector_section_screenshot present")
            .rect;
        assert!(
            shot_btn_rect[2] <= 32.0 && shot_btn_rect[3] <= 32.0,
            "section screenshot button must be compact icon-only, got {:?}",
            shot_btn_rect
        );

        rt.state.explain_node_ai(&sec_id);
        poll_llm_until_idle(rt);

        let summary = rt
            .state
            .graph
            .get_node(&sec_id)
            .and_then(|s| s.summary.as_deref())
            .expect("Section summary populated");
        assert!(summary.contains("Neural Plasticity Group"));
        assert!(summary.contains("AMPA Trafficking") || summary.contains("CaMKII Activation"));
    }

    #[test]
    fn test_localization_edge_calendar_dock_and_node_defaults() {
        let _guard = TEST_THEME_LOCK.write().unwrap_or_else(|e| e.into_inner());
        i18n::set_active_locale(i18n::UiLocale::En);
        let en_rel = i18n::edge_relation_label(&studynodes_graph::model::EdgeRelation::Supports);
        let en_mon = i18n::month_full(1);
        let en_wd = i18n::weekday_short(0);
        let en_dock = i18n::dock_position_label(doc_reader::DockPosition::Left);
        let en_note = i18n::default_node_title(NodeKind::Note, 1);

        i18n::set_active_locale(i18n::UiLocale::Es);
        let es_rel = i18n::edge_relation_label(&studynodes_graph::model::EdgeRelation::Supports);
        let es_mon = i18n::month_full(1);
        let es_wd = i18n::weekday_short(0);
        let es_dock = i18n::dock_position_label(doc_reader::DockPosition::Left);
        let es_note = i18n::default_node_title(NodeKind::Note, 1);

        i18n::set_active_locale(i18n::UiLocale::Ja);
        let ja_rel = i18n::edge_relation_label(&studynodes_graph::model::EdgeRelation::Supports);
        let ja_mon = i18n::month_full(1);
        let ja_wd = i18n::weekday_short(0);

        i18n::set_active_locale(i18n::UiLocale::En);

        assert_ne!(en_rel, es_rel);
        assert_ne!(en_rel, ja_rel);
        assert_ne!(en_mon, es_mon);
        assert_ne!(en_mon, ja_mon);
        assert_ne!(en_wd, es_wd);
        assert_ne!(en_wd, ja_wd);
        assert_ne!(en_dock, es_dock);
        assert_ne!(en_note, es_note);
    }

    #[test]
    fn test_ai_chat_shift_enter_inserts_newline_and_enter_sends() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("AI Chat Multiline");
        rt.state
            .set_llm_provider_override(std::sync::Arc::new(studynodes_llm::MockLlmProvider));
        rt.state.open_ai_chat_panel();
        rt.invalidate();
        settle_frames(rt, 8);

        let rect = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("ai_chat_input"))
            .expect("ai_chat_input present")
            .rect;
        let cx = rect[0] + rect[2] * 0.5;
        let cy = rect[1] + rect[3] * 0.5;
        rt.pointer_move(cx, cy);
        rt.pointer_down(cx, cy);
        rt.pointer_up(cx, cy);
        settle_frames(rt, 4);

        rt.key(KeyEvent {
            key: Key::Text("Line 1".into()),
            mods: Mods::default(),
        });
        rt.key(KeyEvent {
            key: Key::Enter,
            mods: Mods {
                shift: true,
                ..Mods::default()
            },
        });
        rt.key(KeyEvent {
            key: Key::Text("Line 2".into()),
            mods: Mods::default(),
        });
        settle_frames(rt, 6);

        assert!(
            rt.state.ai_chat_messages.is_empty(),
            "Shift+Enter must not submit AI chat"
        );
        assert_eq!(rt.state.ai_chat_input.text, "Line 1\nLine 2");

        // Plain Enter submits the multiline prompt
        rt.key(KeyEvent {
            key: Key::Enter,
            mods: Mods::default(),
        });
        settle_frames(rt, 8);
        poll_llm_until_idle(rt);
        assert_eq!(rt.state.ai_chat_input.text, "");
        assert!(rt
            .state
            .ai_chat_messages
            .iter()
            .any(|m| m.text == "Line 1\nLine 2"));
    }

    #[test]
    fn test_note_markdown_styling_and_note_flashcard_resizing() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Markdown Note & Resize Test");

        let note_id = rt.state.add_note_node(Some(WorldPos::new(120.0, 140.0)));
        let flash_id = rt
            .state
            .add_study_node(NodeKind::Flashcard, Some(WorldPos::new(460.0, 140.0)));
        rt.state.deselect_all();
        if let Some(n) = rt.state.graph.get_node_mut(&note_id) {
            n.content = "# Key Idea\n- **Bold** concept\n- [ ] Practice `code`".into();
        }
        rt.invalidate();
        settle_frames(rt, 8);

        // Unselected Note renders Markdown headings/lists (parsed without raw `# ` prefix) + both Note & Flashcard expose resize grips
        let els = rt.elements();
        let keys: Vec<_> = els.iter().filter_map(|e| e.key.clone()).collect();
        let texts: Vec<_> = els.iter().filter_map(|e| e.text.as_deref()).collect();
        assert!(
            texts.iter().any(|t| t.trim() == "Key Idea"),
            "expected parsed Markdown heading 'Key Idea' on Note card, got texts={:?}",
            texts
        );
        assert!(keys.contains(&format!("node_resize_{}", note_id.as_str())));
        assert!(keys.contains(&format!("node_resize_{}", flash_id.as_str())));

        // Selecting Note shows inline editor AND Inspector live Markdown preview
        rt.state.select_node(note_id.clone(), false);
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 8);
        let sel_keys: Vec<_> = rt.elements().into_iter().filter_map(|e| e.key).collect();
        assert!(sel_keys.contains(&format!("note_body_{}", note_id.as_str())));
        assert!(sel_keys.contains(&"inspector_content".to_string()));
        assert!(sel_keys.contains(&"note_inspector_md_preview".to_string()));

        // Close inspector and resize both Note and Flashcard by dragging bottom-right grips
        rt.state.inspector_open = false;
        rt.state.deselect_all();
        rt.invalidate();
        settle_frames(rt, 6);

        let (base_nw, base_nh) = state::node_world_size(rt.state.graph.get_node(&note_id).unwrap());
        let (base_fw, base_fh) = state::node_world_size(rt.state.graph.get_node(&flash_id).unwrap());

        for (nid, dx, dy) in [(&note_id, 90.0_f32, 65.0_f32), (&flash_id, 80.0_f32, 55.0_f32)] {
            let grip_key = format!("node_resize_{}", nid.as_str());
            let r = rt
                .elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some(grip_key.as_str()))
                .expect("resize grip present")
                .rect;
            let gx = r[0] + r[2] * 0.5;
            let gy = r[1] + r[3] * 0.5;
            rt.pointer_move(gx, gy);
            rt.pointer_down(gx, gy);
            rt.pointer_move(gx + dx, gy + dy);
            rt.pointer_up(gx + dx, gy + dy);
            settle_frames(rt, 6);
        }

        let (new_nw, new_nh) = state::node_world_size(rt.state.graph.get_node(&note_id).unwrap());
        let (new_fw, new_fh) = state::node_world_size(rt.state.graph.get_node(&flash_id).unwrap());
        assert!(new_nw > base_nw + 50.0 && new_nh > base_nh + 35.0);
        assert!(new_fw > base_fw + 45.0 && new_fh > base_fh + 30.0);
    }

    #[test]
    fn test_universal_api_key_presets_and_cross_feature_fallback() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.open_settings();
        rt.invalidate();
        settle_frames(rt, 8);

        click_key(rt, "btn_ai_preset_airforce");
        settle_frames(rt, 6);
        assert_eq!(rt.state.reader_prefs.ai_base_url, "https://api.airforce/v1");
        assert_eq!(rt.state.reader_prefs.ai_model, "gpt-4o-mini");

        rt.state.ai_api_key_edit = rune::TextEdit::new("sk-air-universal-demo");
        click_key(rt, "btn_settings_save");
        settle_frames(rt, 6);

        let (_, ai_label) = rt
            .state
            .resolve_ai_provider()
            .expect("universal AI summaries provider");
        let (_, chat_label) = rt
            .state
            .resolve_chat_ai_provider()
            .expect("universal fallback chat provider");
        assert!(ai_label.contains("Airforce"));
        assert!(chat_label.contains("Airforce"));
        assert!(
            rt.state.cloud_vision_configured(),
            "universal AI key must also power Cloud Vision when no override key is set"
        );

        // Switching to Auto clears explicit base URL while key prefix auto-routes
        click_key(rt, "btn_ai_preset_auto");
        settle_frames(rt, 6);
        assert_eq!(rt.state.reader_prefs.ai_base_url, "");
        let (_, auto_label) = rt
            .state
            .resolve_ai_provider()
            .expect("prefix auto-detected Airforce provider");
        assert!(auto_label.contains("Airforce"));

        // AI Chat key drawer also exposes Base URL input
        rt.state.create_project("Universal Chat Drawer");
        rt.state.open_ai_chat_panel();
        rt.state.ai_chat_key_drawer_open = true;
        rt.invalidate();
        settle_frames(rt, 6);
        let keys: Vec<_> = rt.elements().into_iter().filter_map(|e| e.key).collect();
        assert!(keys.contains(&"input_inline_chat_api_key".to_string()));
        assert!(keys.contains(&"input_inline_chat_base_url".to_string()));
        assert!(keys.contains(&"input_inline_chat_model".to_string()));
    }

    #[test]
    fn test_per_project_multi_tab_ai_chat_persistence() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state
            .set_llm_provider_override(std::sync::Arc::new(studynodes_llm::MockLlmProvider));

        // Project A: 2 distinct chat tabs
        rt.state.create_project("Project Alpha");
        let path_a = rt.state.bundle.as_ref().unwrap().root_dir.clone();
        rt.state.open_ai_chat_panel();
        rt.invalidate();
        settle_frames(rt, 6);

        rt.state.ai_chat_input = rune::TextEdit::new("Alpha first chat topic");
        rt.state.submit_ai_chat_question();
        poll_llm_until_idle(rt);
        assert_eq!(rt.state.ai_chat_messages.len(), 2);

        rt.invalidate();
        settle_frames(rt, 6);
        click_key(rt, "btn_new_ai_chat_tab");
        settle_frames(rt, 6);
        assert_eq!(rt.state.graph.chat_threads.len(), 2);
        assert!(rt.state.ai_chat_messages.is_empty());

        rt.state.ai_chat_input = rune::TextEdit::new("Alpha second chat topic");
        rt.state.submit_ai_chat_question();
        poll_llm_until_idle(rt);
        assert!(rt
            .state
            .ai_chat_messages
            .iter()
            .any(|m| m.text == "Alpha second chat topic"));

        // Switch back to chat tab 0 inside Project Alpha
        click_key(rt, "ai_chat_tab_0");
        settle_frames(rt, 6);
        assert!(rt
            .state
            .ai_chat_messages
            .iter()
            .any(|m| m.text == "Alpha first chat topic"));
        assert!(!rt
            .state
            .ai_chat_messages
            .iter()
            .any(|m| m.text == "Alpha second chat topic"));

        // Project B starts with its own clean isolated chat
        rt.state.create_project("Project Beta");
        rt.state.open_ai_chat_panel();
        rt.invalidate();
        settle_frames(rt, 6);
        assert_eq!(rt.state.graph.chat_threads.len(), 1);
        assert!(rt.state.ai_chat_messages.is_empty());
        rt.state.ai_chat_input = rune::TextEdit::new("Beta isolated chat");
        rt.state.submit_ai_chat_question();
        poll_llm_until_idle(rt);

        // Close all tabs and reopen Project Alpha from disk (new session persistence)
        rt.state.close_tab(1);
        rt.state.close_tab(0);
        assert!(rt.state.tabs.is_empty());

        rt.state.open_project(path_a);
        rt.state.open_ai_chat_panel();
        rt.invalidate();
        settle_frames(rt, 8);

        assert_eq!(rt.state.graph.chat_threads.len(), 2);
        assert!(rt
            .state
            .ai_chat_messages
            .iter()
            .any(|m| m.text == "Alpha first chat topic"));

        click_key(rt, "ai_chat_tab_1");
        settle_frames(rt, 6);
        assert!(rt
            .state
            .ai_chat_messages
            .iter()
            .any(|m| m.text == "Alpha second chat topic"));

        // Close chat tab 1 via UI close button
        click_key(rt, "btn_close_ai_chat_tab_1");
        settle_frames(rt, 6);
        assert_eq!(rt.state.graph.chat_threads.len(), 1);
        assert!(rt
            .state
            .ai_chat_messages
            .iter()
            .any(|m| m.text == "Alpha first chat topic"));
    }

    #[test]
    fn test_zoom_text_and_markdown_scale_proportionally() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Zoom Text Scaling");

        let note_id = rt.state.add_note_node(Some(WorldPos::new(140.0, 120.0)));
        if let Some(n) = rt.state.graph.get_node_mut(&note_id) {
            n.title = "Scale Note".into();
            n.content = "# Zoom Heading\n\nBody paragraph text for zoom scaling check.".into();
        }
        rt.state.sync_edit_buffers_from_selected();

        let text_id = rt
            .state
            .add_study_node(NodeKind::Text, Some(WorldPos::new(500.0, 120.0)));
        if let Some(n) = rt.state.graph.get_node_mut(&text_id) {
            n.content = "Line one\nLine two\nLine three".into();
            n.meta.text_size = Some(18.0);
        }
        rt.state.sync_edit_buffers_from_selected();

        rt.state.inspector_open = false;
        rt.state.ai_panel_open = false;
        rt.state.deselect_all();
        rt.invalidate();
        settle_frames(rt, 8);

        let header_h_1 = element_rect(rt, &format!("node_header_{}", note_id.as_str()))[3];
        let note_inner_1 = element_rect(rt, &format!("note_md_{}", note_id.as_str()))[3];
        let text_inner_1 = element_rect(rt, &format!("text_body_{}", text_id.as_str()))[3];
        assert!(note_inner_1 > 8.0 && text_inner_1 > 12.0);

        // Live wheel zoom in past 2.5x using Ctrl+wheel
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        for _ in 0..8 {
            rt.wheel_event(WheelEvent::new(0.0, 120.0).at(400.0, 220.0).mods(ctrl));
            rt.frame(1.0 / 60.0);
        }
        let live_z = rt.state.graph.viewport.zoom;
        assert!(live_z > 2.0, "expected zoom > 2.0 after Ctrl+wheel in, got {live_z}");

        let note_inner_live = element_rect(rt, &format!("note_md_{}", note_id.as_str()))[3];
        let text_inner_live = element_rect(rt, &format!("text_body_{}", text_id.as_str()))[3];
        assert!(
            note_inner_live > note_inner_1 * 1.6,
            "markdown text height must scale during live zoom ({note_inner_1} -> {note_inner_live})"
        );
        assert!(
            text_inner_live > text_inner_1 * 1.6,
            "free text height must scale during live zoom ({text_inner_1} -> {text_inner_live})"
        );

        // Commit / settle camera (3.0x explicit): text must keep scaling proportionally and NOT shrink
        rt.state.flush_live_camera();
        rt.state.cancel_smooth_zoom();
        rt.state.graph.viewport.zoom = 3.0;
        rt.invalidate();
        settle_frames(rt, 8);

        let header_h_3 = element_rect(rt, &format!("node_header_{}", note_id.as_str()))[3];
        let note_inner_3 = element_rect(rt, &format!("note_md_{}", note_id.as_str()))[3];
        let text_inner_3 = element_rect(rt, &format!("text_body_{}", text_id.as_str()))[3];
        assert!(
            header_h_3 > header_h_1 * 2.5,
            "card header font/height must scale near 3x ({header_h_1} -> {header_h_3})"
        );
        assert!(
            note_inner_3 > note_inner_1 * 2.2,
            "settled markdown text at 3x must scale proportionally without capping ({note_inner_1} -> {note_inner_3})"
        );
        assert!(
            text_inner_3 > text_inner_1 * 2.3,
            "settled free text at 3x must scale proportionally ({text_inner_1} -> {text_inner_3})"
        );
    }

    #[test]
    fn test_scrollable_note_and_markdown_document_cards() {
        let mut th = make_test_runtime();
        let md_path = th._dir.path().join("long_notes.md");
        let long_md = (0..40)
            .map(|i| format!("- Scrollable bullet point entry number {i:02} with extra study detail"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&md_path, &long_md).expect("write long_notes.md");

        let rt = &mut th.rt;
        rt.state.create_project("Scrollable Cards");

        let note_id = rt.state.add_note_node(Some(WorldPos::new(80.0, 80.0)));
        if let Some(n) = rt.state.graph.get_node_mut(&note_id) {
            n.title = "Long Note".into();
            n.content = long_md.clone();
        }
        rt.state.sync_edit_buffers_from_selected();
        let doc_id = rt
            .state
            .add_document_node(Some(WorldPos::new(420.0, 80.0)));
        rt.state.ingest_file_into_node(&doc_id, &md_path);
        rt.state.sync_edit_buffers_from_selected();
        rt.state.deselect_all();
        rt.state.inspector_open = false;
        rt.invalidate();
        settle_frames(rt, 8);

        let scrolled_content_y = |rt: &Runtime<AppState>, key: &str| -> f32 {
            let cr = element_rect(rt, key);
            rt.elements()
                .into_iter()
                .filter(|e| {
                    !e.exiting
                        && e.key.as_deref() != Some(key)
                        && e.rect[2] > 20.0
                        && e.rect[3] > cr[3]
                        && e.rect[0] >= cr[0] - 2.0
                        && e.rect[0] + e.rect[2] <= cr[0] + cr[2] + 2.0
                })
                .map(|e| e.rect[1])
                .fold(f32::INFINITY, f32::min)
        };

        // Unselected Note card scrolls cleanly via wheel without panning or zooming canvas
        let note_key = format!("node_body_{}", note_id.as_str());
        let note_md_key = format!("note_md_{}", note_id.as_str());
        let nr = element_rect(rt, &note_key);
        let (nx, ny) = (nr[0] + nr[2] * 0.5, nr[1] + nr[3] * 0.5);
        let offset_before = rt.state.graph.viewport.offset;
        let zoom_before_wheel = rt.state.graph.viewport.zoom;
        let note_y_before = element_rect(rt, &note_md_key)[1];

        rt.pointer_move(nx, ny);
        rt.wheel_event(WheelEvent::new(0.0, -240.0).at(nx, ny).precise(true));
        settle_frames(rt, 4);

        assert_eq!(
            rt.state.graph.viewport.offset, offset_before,
            "plain wheel over Note card must scroll Note instead of panning canvas"
        );
        assert!(
            (rt.state.graph.viewport.zoom - zoom_before_wheel).abs() < 1e-4,
            "plain wheel over Note card must not zoom canvas"
        );
        let note_y_after = element_rect(rt, &note_md_key)[1];
        assert!(
            note_y_after < note_y_before - 20.0,
            "Note content should scroll downward on wheel ({note_y_before} -> {note_y_after})"
        );

        // Selected Note keeps scrolling via wheel, while Ctrl+wheel still zooms canvas
        rt.pointer_down(nx, ny);
        rt.pointer_up(nx, ny);
        settle_frames(rt, 4);
        let zoom_before = rt.state.graph.viewport.zoom;
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        rt.wheel_event(WheelEvent::new(0.0, 120.0).at(nx, ny).mods(ctrl));
        settle_frames(rt, 4);
        assert!(
            rt.state.graph.viewport.zoom > zoom_before,
            "Ctrl+wheel over scrollable card must still zoom canvas"
        );

        // Reset zoom and verify .md Document card also scrolls without panning canvas
        rt.state.cancel_smooth_zoom();
        rt.state.graph.viewport.zoom = 1.0;
        rt.state.deselect_all();
        rt.invalidate();
        settle_frames(rt, 6);

        let doc_key = format!("doc_md_scroll_{}", doc_id.as_str());
        let dr = element_rect(rt, &doc_key);
        let (dx, dy) = (dr[0] + dr[2] * 0.5, dr[1] + dr[3] * 0.5);
        let doc_off_before = rt.state.graph.viewport.offset;
        let doc_y_before = scrolled_content_y(rt, &doc_key);

        rt.pointer_move(dx, dy);
        rt.wheel_event(WheelEvent::new(0.0, -240.0).at(dx, dy).precise(true));
        settle_frames(rt, 4);

        assert_eq!(
            rt.state.graph.viewport.offset, doc_off_before,
            "plain wheel over .md Document card must not pan canvas"
        );
        let doc_y_after = scrolled_content_y(rt, &doc_key);
        assert!(
            doc_y_after < doc_y_before - 20.0,
            ".md Document content should scroll downward on wheel ({doc_y_before} -> {doc_y_after})"
        );
    }

    #[test]
    fn test_compact_add_menu_and_right_click_canvas_spawn() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Add Menu Layout");
        let existing_id = rt.state.add_note_node(Some(WorldPos::new(120.0, 120.0)));
        rt.state.deselect_all();
        rt.state.inspector_open = false;
        rt.invalidate();
        settle_frames(rt, 8);

        // Dock toggle opens a wider-than-tall compact menu
        click_key(rt, "btn_toggle_block_menu");
        settle_frames(rt, 6);
        assert!(rt.state.show_block_menu);
        let menu_rect = element_rect(rt, "block_create_menu");
        assert!(
            menu_rect[2] > menu_rect[3],
            "revamped add menu must be wider than tall: {:?}",
            menu_rect
        );
        click_key(rt, "btn_toggle_block_menu");
        settle_frames(rt, 4);
        assert!(!rt.state.show_block_menu);

        // Right-clicking over an existing Note node must NOT open the canvas add menu
        let nr = element_rect(rt, &format!("node_body_{}", existing_id.as_str()));
        let (nx, ny) = (nr[0] + nr[2] * 0.5, nr[1] + nr[3] * 0.5);
        rt.pointer_move(nx, ny);
        rt.secondary_down(nx, ny);
        rt.secondary_up(nx, ny);
        settle_frames(rt, 6);
        assert!(
            !rt.state.show_block_menu,
            "right-clicking directly over a node card must not open the empty-canvas add menu"
        );

        // Right-clicking empty canvas space opens the menu anchored to the clicked point
        let (rx, ry) = (860.0_f32, 460.0_f32);
        rt.pointer_move(rx, ry);
        rt.secondary_down(rx, ry);
        rt.secondary_up(rx, ry);
        settle_frames(rt, 6);
        assert!(
            rt.state.show_block_menu,
            "right-clicking empty canvas space should open add menu"
        );
        let expected_world = rt.state.graph.viewport.screen_to_world(ScreenPos::new(rx, ry));

        click_key(rt, "btn_add_text");
        settle_frames(rt, 6);
        assert!(!rt.state.show_block_menu);
        let spawned = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Text)
            .expect("Text node spawned via context add menu");
        assert!(
            (spawned.pos.x - expected_world.x).abs() < 220.0
                && (spawned.pos.y - expected_world.y).abs() < 160.0,
            "spawned node {:?} should be near right-click world position {:?}",
            spawned.pos,
            expected_world
        );
    }

    #[test]
    fn test_delete_key_deletes_selected_or_empty_nodes() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Delete Key Behavior");

        let pop_id = rt.state.add_note_node(Some(WorldPos::new(80.0, 80.0)));
        if let Some(n) = rt.state.graph.get_node_mut(&pop_id) {
            n.content = "Keep until selected".into();
        }
        let empty_note = rt.state.add_note_node(Some(WorldPos::new(400.0, 80.0)));
        let empty_quiz = rt
            .state
            .add_study_node(NodeKind::Quiz, Some(WorldPos::new(720.0, 80.0)));
        if let Some(n) = rt.state.graph.get_node_mut(&empty_quiz) {
            n.meta.quiz_questions.clear();
        }
        let empty_sec = rt
            .state
            .add_study_node(NodeKind::Section, Some(WorldPos::new(80.0, 360.0)));
        rt.state.sync_edit_buffers_from_selected();
        rt.invalidate();
        settle_frames(rt, 6);

        // 1. Selected populated node is deleted on Supr/Delete without touching empty nodes
        rt.state.select_node(pop_id.clone(), false);
        rt.state.editing_node_id = None;
        rt.invalidate();
        settle_frames(rt, 4);
        rt.key(KeyEvent {
            key: Key::Delete,
            mods: Mods::default(),
        });
        settle_frames(rt, 6);

        assert!(rt.state.graph.get_node(&pop_id).is_none());
        assert!(rt.state.graph.get_node(&empty_note).is_some());
        assert!(rt.state.graph.get_node(&empty_quiz).is_some());
        assert!(rt.state.graph.get_node(&empty_sec).is_some());

        // 2. With nothing selected, Supr/Delete clears empty nodes while preserving populated ones
        let keep_id = rt.state.add_note_node(Some(WorldPos::new(100.0, 100.0)));
        if let Some(n) = rt.state.graph.get_node_mut(&keep_id) {
            n.content = "Do not prune populated note".into();
        }
        rt.state.sync_edit_buffers_from_selected();
        rt.state.deselect_all();
        rt.invalidate();
        settle_frames(rt, 4);
        rt.key(KeyEvent {
            key: Key::Delete,
            mods: Mods::default(),
        });
        settle_frames(rt, 6);

        assert!(rt.state.graph.get_node(&keep_id).is_some());
        assert!(rt.state.graph.get_node(&empty_note).is_none());
        assert!(rt.state.graph.get_node(&empty_quiz).is_none());
        assert!(rt.state.graph.get_node(&empty_sec).is_none());
    }

    #[test]
    fn test_trace_colors_toggle_inspector_and_ai_tool() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Trace Color Project");

        let n1 = rt.state.add_note_node(Some(WorldPos::new(100.0, 120.0)));
        let n2 = rt.state.add_note_node(Some(WorldPos::new(480.0, 120.0)));
        rt.state.connect_nodes(&n1, &n2);
        let eid = rt
            .state
            .graph
            .edges
            .values()
            .next()
            .expect("edge created")
            .id
            .clone();
        rt.state.select_edge(eid.clone());
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 8);

        assert!(rt.state.reader_prefs.enable_trace_colors);
        click_key(rt, "btn_edge_color_critical");
        settle_frames(rt, 6);

        let edge = rt.state.graph.get_edge(&eid).unwrap();
        assert_eq!(edge.color_category.as_deref(), Some("Critical"));
        assert!(edge.color_hex.is_some());

        // Toggle off in Inspector, then toggle back on in Settings
        click_key(rt, "btn_inspector_toggle_trace_colors");
        settle_frames(rt, 6);
        assert!(!rt.state.reader_prefs.enable_trace_colors);

        rt.state.open_settings();
        rt.invalidate();
        settle_frames(rt, 6);
        click_key(rt, "btn_toggle_trace_colors");
        settle_frames(rt, 6);
        assert!(rt.state.reader_prefs.enable_trace_colors);
        rt.state.screen = AppScreen::Workspace;
        rt.invalidate();
        settle_frames(rt, 6);

        // AI tool SetEdgeColor updates color + category cleanly
        let patch_json = serde_json::json!({
            "rationale": "Color trace by synthesis topic",
            "ops": [{
                "op": "set_edge_color",
                "edge_id": eid.as_str(),
                "color": "synthesis",
                "color_category": "Synthesis"
            }]
        });
        let patch: studynodes_graph::patch::GraphPatch =
            serde_json::from_value(patch_json).expect("valid SetEdgeColor patch");
        let res = rt.state.apply_ai_patch_with_undo(&patch);
        assert_eq!(res.edges_colored, 1);
        let updated = rt.state.graph.get_edge(&eid).unwrap();
        assert_eq!(updated.color_category.as_deref(), Some("Synthesis"));
        assert_eq!(
            updated.color_hex.as_deref(),
            studynodes_graph::model::resolve_trace_color_hex("synthesis").as_deref()
        );
    }

    #[test]
    fn test_frameless_and_resizable_image_node() {
        const ONE_PX_PNG: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let mut th = make_test_runtime();
        let png_path = th._dir.path().join("diagram.png");
        std::fs::write(&png_path, ONE_PX_PNG).expect("write diagram.png");

        let rt = &mut th.rt;
        rt.state.create_project("Frameless Image");
        let img_id = rt
            .state
            .add_study_node(NodeKind::Image, Some(WorldPos::new(120.0, 100.0)));
        rt.state.ingest_file_into_node(&img_id, &png_path);
        rt.state.select_node(img_id.clone(), false);
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 12);

        let (w0, h0) = crate::state::node_world_size(rt.state.graph.get_node(&img_id).unwrap());
        let grip = element_rect(rt, &format!("node_resize_{}", img_id.as_str()));
        let (gx, gy) = (grip[0] + grip[2] * 0.5, grip[1] + grip[3] * 0.5);
        let z = rt.state.graph.viewport.zoom;
        rt.pointer_move(gx, gy);
        rt.pointer_down(gx, gy);
        rt.pointer_move(gx + 140.0 * z, gy + 90.0 * z);
        rt.pointer_up(gx + 140.0 * z, gy + 90.0 * z);
        settle_frames(rt, 8);

        let (w1, h1) = crate::state::node_world_size(rt.state.graph.get_node(&img_id).unwrap());
        assert!(
            w1 > w0 + 90.0 && h1 > h0 + 50.0,
            "Image node resize grip must enlarge image box: ({w0},{h0}) -> ({w1},{h1})"
        );

        let header_key = format!("node_header_{}", img_id.as_str());
        assert!(rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some(header_key.as_str()) && !e.exiting));

        click_key(rt, "btn_toggle_image_frame");
        settle_frames(rt, 10);
        assert!(rt.state.graph.get_node(&img_id).unwrap().meta.image_frameless);
        assert!(
            !rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some(header_key.as_str()) && !e.exiting),
            "frameless image mode must hide the card header chrome when image is loaded"
        );
    }

    #[test]
    fn test_frameless_text_node_font_and_size() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Frameless Text Test");
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 8);

        click_key(rt, "btn_toggle_block_menu");
        settle_frames(rt, 6);
        click_key(rt, "btn_add_text");
        settle_frames(rt, 8);

        let txt_id = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Text)
            .expect("Text node added")
            .id
            .clone();

        // Frameless text node must never expose a card header chrome (selected or deselected)
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some(&format!("node_header_{}", txt_id.as_str()))));

        if let Some(n) = rt.state.graph.get_node_mut(&txt_id) {
            n.content = "Typography Heading".into();
        }
        rt.state.sync_edit_buffers_from_selected();
        rt.state.select_node(txt_id.clone(), false);
        rt.invalidate();
        settle_frames(rt, 6);

        click_key(rt, "btn_text_font_serif");
        click_key(rt, "btn_text_size_36");
        settle_frames(rt, 6);

        let node = rt.state.graph.get_node(&txt_id).unwrap();
        assert_eq!(node.meta.text_font.as_deref(), Some("serif"));
        assert_eq!(node.meta.text_size, Some(36.0));
        rt.state.deselect_all();
        rt.invalidate();
        settle_frames(rt, 4);
        assert!(!rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some(&format!("node_header_{}", txt_id.as_str()))));
    }

    #[test]
    fn test_quiz_node_interactive_hints_and_ai_ops() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Quiz Node Test");
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 8);

        click_key(rt, "btn_toggle_block_menu");
        settle_frames(rt, 6);
        click_key(rt, "btn_add_quiz");
        settle_frames(rt, 8);

        let quiz_id = rt
            .state
            .graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Quiz)
            .expect("Quiz node added")
            .id
            .clone();
        assert_eq!(rt.state.graph.get_node(&quiz_id).unwrap().meta.quiz_questions.len(), 1);

        // Add Q2 (index 1) and Q3 (index 2) via Inspector
        rt.state.select_node(quiz_id.clone(), false);
        rt.state.quiz_q_edit = rune::TextEdit::new("What is the powerhouse of the cell?");
        rt.state.quiz_opts_edit = rune::TextEdit::new("Nucleus | Mitochondria | Ribosome");
        rt.state.quiz_ans_edit = rune::TextEdit::new("Mitochondria produce ATP.");
        rt.state.quiz_hint_edit = rune::TextEdit::new("Starts with M\nGenerates ATP");
        rt.invalidate();
        settle_frames(rt, 4);
        click_key(rt, "quiz_new_correct_1");
        click_key(rt, "btn_quiz_add_q");
        settle_frames(rt, 6);

        rt.state.quiz_q_edit = rune::TextEdit::new("Which ion triggers vesicle fusion?");
        rt.state.quiz_opts_edit = rune::TextEdit::new("Calcium | Sodium");
        rt.state.quiz_ans_edit = rune::TextEdit::new("Ca2+ influx.");
        rt.invalidate();
        settle_frames(rt, 4);
        click_key(rt, "btn_quiz_add_q");
        settle_frames(rt, 6);

        assert_eq!(rt.state.graph.get_node(&quiz_id).unwrap().meta.quiz_questions.len(), 3);

        // Append extra hint onto Q2 (index 1) via +H button
        rt.state.quiz_hint_edit = rune::TextEdit::new("Double membrane");
        rt.invalidate();
        settle_frames(rt, 4);
        click_key(rt, "quiz_add_hint_1");
        settle_frames(rt, 6);
        assert_eq!(
            rt.state.graph.get_node(&quiz_id).unwrap().meta.quiz_questions[1]
                .hints
                .len(),
            3
        );

        // Step from Q3 (index 2) back to Q2 (index 1), reveal hint, select option, reveal explanation
        click_key(rt, &format!("quiz_prev_{}", quiz_id.as_str()));
        click_key(rt, &format!("quiz_hint_btn_{}", quiz_id.as_str()));
        click_key(rt, &format!("quiz_opt_{}_1_1", quiz_id.as_str()));
        click_key(rt, &format!("quiz_ans_btn_{}", quiz_id.as_str()));
        settle_frames(rt, 6);

        let sess = rt.state.quiz_sessions.get(&quiz_id).unwrap();
        assert_eq!(sess.active_index, 1);
        assert_eq!(sess.revealed_hints.get(&1), Some(&1));
        assert_eq!(sess.selected_options.get(&1), Some(&1));
        assert!(sess.revealed_answers.contains(&1));

        // AI ops create & remove Quiz questions cleanly (3 + 1 - 1 = 3)
        let ai_patch: studynodes_graph::patch::GraphPatch = serde_json::from_value(serde_json::json!({
            "rationale": "Update quiz deck",
            "ops": [
                {
                    "op": "add_quiz_question",
                    "node_id": quiz_id.as_str(),
                    "question": "Fourth question?",
                    "options": ["Yes", "No"],
                    "correct_index": 0,
                    "answer": "Yes.",
                    "hints": ["Hint 1"]
                },
                {
                    "op": "remove_quiz_question",
                    "node_id": quiz_id.as_str(),
                    "question_index": 0
                }
            ]
        }))
        .expect("valid quiz ops patch");
        let res = rt.state.apply_ai_patch_with_undo(&ai_patch);
        assert!(res.quiz_questions_changed >= 2);
        assert_eq!(rt.state.graph.get_node(&quiz_id).unwrap().meta.quiz_questions.len(), 3);
    }

    #[test]
    fn test_draggable_dock_and_vertical_icon_only_mode() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Draggable Dock");
        rt.state.inspector_open = false;
        rt.state.ai_panel_open = false;
        rt.invalidate();
        settle_frames(rt, 8);

        let handle = element_rect(rt, "dock_drag_handle");
        let (hx, hy) = (handle[0] + handle[2] * 0.5, handle[1] + handle[3] * 0.5);

        // Drag dock to left edge -> auto-snaps vertical DockPosition::Left and removes labels (narrow width)
        rt.pointer_move(hx, hy);
        rt.pointer_down(hx, hy);
        rt.pointer_move(20.0, 360.0);
        rt.pointer_up(20.0, 360.0);
        settle_frames(rt, 10);

        assert_eq!(rt.state.reader_prefs.dock_position, doc_reader::DockPosition::Left);
        let vert_dock = element_rect(rt, "canvas_floating_dock");
        assert!(
            vert_dock[2] < 72.0,
            "vertical dock must drop icon labels so it stays narrow (<72px), got {:?}",
            vert_dock
        );

        // Drag vertical dock back toward middle of canvas -> custom floating horizontal bar
        let vhandle = element_rect(rt, "dock_drag_handle");
        let (vx, vy) = (vhandle[0] + vhandle[2] * 0.5, vhandle[1] + vhandle[3] * 0.5);
        rt.pointer_move(vx, vy);
        rt.pointer_down(vx, vy);
        rt.pointer_move(480.0, 220.0);
        rt.pointer_up(480.0, 220.0);
        settle_frames(rt, 10);

        assert!(
            rt.state.reader_prefs.dock_custom_pos.is_some(),
            "dropping dock away from edges must retain custom floating coordinates"
        );
        let custom_dock = element_rect(rt, "canvas_floating_dock");
        assert!(
            custom_dock[0] > 200.0 && custom_dock[1] < 400.0 && custom_dock[2] > custom_dock[3],
            "custom middle placement should render horizontal dock near drop target: {:?}",
            custom_dock
        );
    }

    #[test]
    fn test_frameless_text_auto_fit_and_fit_button() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Text Auto-Fit");
        let txt_id = rt
            .state
            .add_study_node(NodeKind::Text, Some(WorldPos::new(180.0, 140.0)));
        rt.state.select_node(txt_id.clone(), false);
        rt.state.inspector_open = true;
        rt.invalidate();
        settle_frames(rt, 8);

        let (w0, h0) = crate::state::node_world_size(rt.state.graph.get_node(&txt_id).unwrap());

        if let Some(n) = rt.state.graph.get_node_mut(&txt_id) {
            n.content = "Frameless Heading\nA much longer second line for fit measurement\nThird line".into();
        }
        rt.state.sync_edit_buffers_from_selected();
        rt.invalidate();
        settle_frames(rt, 6);

        click_key(rt, "btn_text_fit_now");
        settle_frames(rt, 6);
        let (w_fit, h_fit) =
            crate::state::node_world_size(rt.state.graph.get_node(&txt_id).unwrap());
        assert!(
            w_fit > w0 + 40.0 || h_fit > h0 + 20.0,
            "Fit-to-content should resize text bounds to content: ({w0},{h0}) -> ({w_fit},{h_fit})"
        );

        click_key(rt, "btn_text_auto_fit");
        settle_frames(rt, 6);
        assert!(rt.state.graph.get_node(&txt_id).unwrap().meta.text_auto_fit);

        click_key(rt, "btn_text_size_36");
        settle_frames(rt, 8);
        let (w_big, h_big) =
            crate::state::node_world_size(rt.state.graph.get_node(&txt_id).unwrap());
        assert!(
            w_big > w_fit + 20.0 || h_big > h_fit + 15.0,
            "Auto-fit should expand text bounds when font size increases: ({w_fit},{h_fit}) -> ({w_big},{h_big})"
        );

        if let Some(n) = rt.state.graph.get_node_mut(&txt_id) {
            n.content = "Hi".into();
        }
        rt.state.sync_edit_buffers_from_selected();
        rt.invalidate();
        settle_frames(rt, 6);
        click_key(rt, &format!("text_fit_{}", txt_id.as_str()));
        settle_frames(rt, 6);
        let (w_short, _) =
            crate::state::node_world_size(rt.state.graph.get_node(&txt_id).unwrap());
        assert!(
            w_short < w_big - 40.0,
            "Canvas Fit pill should shrink bounds to short text: {w_big} -> {w_short}"
        );

        // Manual corner resize turns auto-fit off so user sizing sticks
        let grip = element_rect(rt, &format!("node_resize_{}", txt_id.as_str()));
        let (gx, gy) = (grip[0] + grip[2] * 0.5, grip[1] + grip[3] * 0.5);
        let z = rt.state.graph.viewport.zoom;
        rt.pointer_move(gx, gy);
        rt.pointer_down(gx, gy);
        rt.pointer_move(gx + 100.0 * z, gy + 50.0 * z);
        rt.pointer_up(gx + 100.0 * z, gy + 50.0 * z);
        settle_frames(rt, 6);
        assert!(
            !rt.state.graph.get_node(&txt_id).unwrap().meta.text_auto_fit,
            "manual resize must turn off text_auto_fit"
        );
    }

    #[test]
    fn test_block_menu_square_tiles_and_phosphor_icons() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Square Menu");
        rt.invalidate();
        settle_frames(rt, 8);

        click_key(rt, "btn_toggle_block_menu");
        settle_frames(rt, 6);

        let menu = element_rect(rt, "block_create_menu");
        assert!(
            menu[2] > menu[3],
            "block menu must remain wider than tall: {:?}",
            menu
        );

        for key in [
            "btn_add_menu_note",
            "btn_add_text",
            "btn_add_menu_doc",
            "btn_add_image",
            "btn_add_section",
            "btn_add_drawing",
            "btn_add_flashcard",
            "btn_add_quiz",
            "btn_add_excerpt",
            "btn_add_checklist",
            "btn_add_formula",
            "btn_add_sequence",
            "btn_add_video",
            "btn_add_audio",
            "btn_add_calendar",
        ] {
            let r = rt
                .elements()
                .into_iter()
                .rev()
                .find(|e| {
                    e.key.as_deref() == Some(key)
                        && !e.exiting
                        && e.rect[0] >= menu[0] - 1.0
                        && e.rect[1] >= menu[1] - 1.0
                        && e.rect[0] + e.rect[2] <= menu[0] + menu[2] + 1.0
                        && e.rect[1] + e.rect[3] <= menu[1] + menu[3] + 1.0
                })
                .unwrap_or_else(|| panic!("missing menu tile '{key}' inside block_create_menu"))
                .rect;
            assert!(
                r[2] >= 68.0 && r[3] >= 68.0 && (r[2] - r[3]).abs() <= 24.0,
                "tile '{key}' should be near-square icon-above-label card, got {:?}",
                r
            );
        }
    }

    #[test]
    fn test_dock_auto_orient_toggle_and_corner_behavior() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Dock Corner & Toggle");
        rt.state.inspector_open = false;
        rt.state.ai_panel_open = false;
        rt.invalidate();
        settle_frames(rt, 8);

        let dock0 = element_rect(rt, "canvas_floating_dock");
        assert!(
            dock0[2] < 430.0,
            "horizontal dock must remain compact (<430px), got {:?}",
            dock0
        );

        // Dragging into top-left corner must NOT flip vertical
        let h0 = element_rect(rt, "dock_drag_handle");
        rt.pointer_move(h0[0] + h0[2] * 0.5, h0[1] + h0[3] * 0.5);
        rt.pointer_down(h0[0] + h0[2] * 0.5, h0[1] + h0[3] * 0.5);
        rt.pointer_move(20.0, 48.0);
        rt.pointer_up(20.0, 48.0);
        settle_frames(rt, 8);
        let corner_dock = element_rect(rt, "canvas_floating_dock");
        assert!(
            corner_dock[2] > corner_dock[3],
            "dropping dock in corner band must stay horizontal, got {:?}",
            corner_dock
        );

        // Toggle off automatic vertical orientation via dock button -> mid-left stays horizontal
        click_key(rt, "btn_dock_auto_orient");
        settle_frames(rt, 4);
        assert!(!rt.state.reader_prefs.dock_auto_orient);

        let h1 = element_rect(rt, "dock_drag_handle");
        rt.pointer_move(h1[0] + h1[2] * 0.5, h1[1] + h1[3] * 0.5);
        rt.pointer_down(h1[0] + h1[2] * 0.5, h1[1] + h1[3] * 0.5);
        rt.pointer_move(20.0, 360.0);
        rt.pointer_up(20.0, 360.0);
        settle_frames(rt, 8);
        let manual_edge = element_rect(rt, "canvas_floating_dock");
        assert!(
            manual_edge[2] > manual_edge[3],
            "when dock_auto_orient is off, dragging to side edge must keep horizontal bar: {:?}",
            manual_edge
        );

        // Re-enable in Settings -> dragging to mid-left snaps vertical again
        click_key(rt, "btn_open_settings");
        settle_frames(rt, 6);
        click_key(rt, "btn_toggle_dock_auto_orient");
        settle_frames(rt, 6);
        assert!(rt.state.reader_prefs.dock_auto_orient);
        rt.state.close_settings();
        rt.invalidate();
        settle_frames(rt, 6);

        let h2 = element_rect(rt, "dock_drag_handle");
        rt.pointer_move(h2[0] + h2[2] * 0.5, h2[1] + h2[3] * 0.5);
        rt.pointer_down(h2[0] + h2[2] * 0.5, h2[1] + h2[3] * 0.5);
        rt.pointer_move(20.0, 360.0);
        rt.pointer_up(20.0, 360.0);
        settle_frames(rt, 8);
        assert_eq!(
            rt.state.reader_prefs.dock_position,
            doc_reader::DockPosition::Left
        );
        let vert_dock = element_rect(rt, "canvas_floating_dock");
        assert!(vert_dock[2] < 72.0);
    }

    #[test]
    fn test_ai_chat_tabs_project_style_and_autogrow_composer() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("AI Chat Composer");
        rt.invalidate();
        settle_frames(rt, 8);

        click_key(rt, "btn_ai_open_chat");
        settle_frames(rt, 8);

        let tabs_bar = element_rect(rt, "ai_chat_tabs_bar");
        assert!(
            (tabs_bar[3] - 35.0).abs() <= 2.5,
            "AI chat tab strip must match 35px project tab bar height: {:?}",
            tabs_bar
        );

        click_key(rt, "btn_new_ai_chat_tab");
        settle_frames(rt, 8);
        assert_eq!(rt.state.graph.chat_threads.len(), 2);
        let tab0 = element_rect(rt, "ai_chat_tab_shell_0");
        let tab1 = element_rect(rt, "ai_chat_tab_shell_1");
        assert!((tab0[3] - 35.0).abs() <= 2.5 && (tab1[3] - 35.0).abs() <= 2.5);
        click_key(rt, "ai_chat_tab_0");
        settle_frames(rt, 4);
        assert_eq!(
            rt.state.graph.active_chat_id.as_deref(),
            Some(rt.state.graph.chat_threads[0].id.as_str())
        );

        let bar_before = element_rect(rt, "ai_chat_prompt_bar");
        click_key(rt, "ai_chat_input");
        rt.paste("Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6");
        settle_frames(rt, 18);
        let bar_grown = element_rect(rt, "ai_chat_prompt_bar");
        assert!(
            bar_grown[3] > bar_before[3] + 20.0,
            "adding newlines to AI composer should make prompt bar taller: {:?} -> {:?}",
            bar_before,
            bar_grown
        );

        rt.paste("\nL7\nL8\nL9\nL10\nL11\nL12\nL13\nL14\nL15\nL16\nL17\nL18\nL19\nL20");
        settle_frames(rt, 18);
        let bar_capped = element_rect(rt, "ai_chat_prompt_bar");
        assert!(
            bar_capped[3] <= 240.0,
            "AI composer height growth must cap cleanly instead of growing unbounded: {:?}",
            bar_capped
        );
    }

    #[test]
    fn test_quiz_add_question_real_click_and_typing() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Quiz Real Click Test");
        let quiz_id = rt
            .state
            .add_study_node(NodeKind::Quiz, Some(WorldPos::new(160.0, 140.0)));
        if let Some(n) = rt.state.graph.get_node_mut(&quiz_id) {
            n.meta.quiz_questions.clear();
        }
        rt.state.select_node(quiz_id.clone(), false);
        rt.state.inspector_open = true;
        rt.state.ai_panel_open = false;
        rt.invalidate();
        settle_frames(rt, 8);

        // Empty submit warns cleanly instead of doing nothing silently
        click_key(rt, "btn_quiz_add_q");
        settle_frames(rt, 4);
        assert!(
            rt.state
                .graph
                .get_node(&quiz_id)
                .unwrap()
                .meta
                .quiz_questions
                .is_empty()
        );
        assert!(rt.state.toast_text.is_some());

        // Real pointer clicks + text entry into the 4 bordered Quiz inputs
        click_key(rt, "quiz_q_edit");
        rt.paste("What stores genetic information?");
        settle_frames(rt, 3);

        click_key(rt, "quiz_opts_edit");
        rt.paste("DNA | Lipid | Glucose");
        settle_frames(rt, 3);

        click_key(rt, "quiz_new_correct_0");

        click_key(rt, "quiz_ans_edit");
        rt.paste("DNA encodes hereditary info.");
        settle_frames(rt, 3);

        click_key(rt, "quiz_hint_edit");
        rt.paste("Double helix");
        settle_frames(rt, 3);

        // Re-selecting the same Quiz node preserves active form drafts
        rt.state.select_node(quiz_id.clone(), false);
        assert_eq!(rt.state.quiz_q_edit.text, "What stores genetic information?");

        click_key(rt, "btn_quiz_add_q");
        settle_frames(rt, 6);

        let qs = &rt
            .state
            .graph
            .get_node(&quiz_id)
            .unwrap()
            .meta
            .quiz_questions;
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].question, "What stores genetic information?");
        assert_eq!(
            qs[0].options,
            vec!["DNA".to_string(), "Lipid".to_string(), "Glucose".to_string()]
        );
        assert_eq!(qs[0].correct_index, Some(0));
        assert_eq!(qs[0].answer, "DNA encodes hereditary info.");
        assert_eq!(qs[0].hints, vec!["Double helix".to_string()]);
        assert!(rt.state.quiz_q_edit.text.is_empty());
    }

    #[test]
    fn test_ai_move_and_arrange_scheme_with_undo() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("AI Spatial Scheme");

        let a_id = rt.state.add_note_node(Some(WorldPos::new(100.0, 100.0)));
        let b_id = rt.state.add_note_node(Some(WorldPos::new(100.0, 100.0)));
        if let Some(n) = rt.state.graph.get_node_mut(&a_id) {
            n.title = "Core Theory".into();
        }
        if let Some(n) = rt.state.graph.get_node_mut(&b_id) {
            n.title = "Supporting Evidence".into();
        }
        rt.invalidate();
        settle_frames(rt, 8);

        // 1. Create a node at explicit pos, move existing node A, move B relative to A, and link them
        let patch_json = serde_json::json!({
            "summary": "Build clean scheme layout",
            "ops": [
                {
                    "op": "create_study_node",
                    "temp_id": "tmp_quiz",
                    "kind": "Quiz",
                    "title": "Practice Check",
                    "x": 460.0,
                    "y": 100.0
                },
                {
                    "op": "move_node",
                    "node_id": a_id.as_str(),
                    "x": 120.0,
                    "y": 100.0
                },
                {
                    "op": "move_node",
                    "node_id": b_id.as_str(),
                    "relative_to": a_id.as_str(),
                    "placement": "below",
                    "gap": 80.0
                },
                {
                    "op": "propose_connection",
                    "from_node": a_id.as_str(),
                    "to_node": b_id.as_str(),
                    "relation": "supports",
                    "label": "Evidence"
                },
                {
                    "op": "propose_connection",
                    "from_node": a_id.as_str(),
                    "to_node": "tmp_quiz",
                    "relation": "derived_from",
                    "label": "Test"
                }
            ]
        });
        let patch1: studynodes_graph::patch::GraphPatch = serde_json::from_value(patch_json).unwrap();
        let res1 = rt.state.apply_ai_patch_with_undo(&patch1);
        assert_eq!(res1.dropped_ops_count, 0);
        assert_eq!(res1.new_nodes.len(), 1);
        assert_eq!(res1.nodes_moved, 2);
        let q_id = res1.new_nodes[0].id.clone();

        rt.invalidate();
        settle_frames(rt, 6);
        let ra = element_rect(rt, a_id.as_str());
        let rb = element_rect(rt, b_id.as_str());
        let rq = element_rect(rt, q_id.as_str());
        assert!(rb[1] > ra[1] + ra[3], "B should render below A: A={ra:?}, B={rb:?}");
        assert!(rq[0] > ra[0] + ra[2], "Quiz should render right of A: A={ra:?}, Q={rq:?}");

        let pos_b_before_arrange = rt.state.graph.get_node(&b_id).unwrap().pos;

        // 2. Arrange into hierarchy scheme
        let arrange_json = serde_json::json!({
            "summary": "Arrange as hierarchy",
            "ops": [{
                "op": "arrange_nodes",
                "layout": "hierarchy",
                "root_node": a_id.as_str(),
                "origin_x": 140.0,
                "origin_y": 90.0
            }]
        });
        let patch2: studynodes_graph::patch::GraphPatch = serde_json::from_value(arrange_json).unwrap();
        let res2 = rt.state.apply_ai_patch_with_undo(&patch2);
        assert_eq!(res2.dropped_ops_count, 0);
        assert_eq!(res2.nodes_moved, 3);
        rt.invalidate();
        settle_frames(rt, 6);

        let all_ids = [a_id.clone(), b_id.clone(), q_id.clone()];
        let rects: Vec<[f32; 4]> = all_ids
            .iter()
            .map(|id| element_rect(rt, id.as_str()))
            .collect();
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                let r1 = rects[i];
                let r2 = rects[j];
                let overlap = r1[0] < r2[0] + r2[2] - 1.0
                    && r1[0] + r1[2] > r2[0] + 1.0
                    && r1[1] < r2[1] + r2[3] - 1.0
                    && r1[1] + r1[3] > r2[1] + 1.0;
                assert!(!overlap, "arranged canvas cards overlapped: {:?} vs {:?}", r1, r2);
            }
        }

        // 3. Undo restores exact pre-arrange and pre-patch states
        assert!(rt.state.undo_graph());
        rt.invalidate();
        settle_frames(rt, 4);
        assert_eq!(rt.state.graph.get_node(&b_id).unwrap().pos, pos_b_before_arrange);

        assert!(rt.state.undo_graph());
        rt.invalidate();
        settle_frames(rt, 4);
        assert_eq!(rt.state.graph.nodes.len(), 2);
        assert_eq!(rt.state.graph.get_node(&b_id).unwrap().pos, WorldPos::new(100.0, 100.0));
    }

    #[test]
    fn test_quiz_popout_easy_mode_return_and_resume() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Quiz Popout Easy Test");
        let quiz_id = rt
            .state
            .add_study_node(NodeKind::Quiz, Some(WorldPos::new(160.0, 140.0)));
        if let Some(n) = rt.state.graph.get_node_mut(&quiz_id) {
            n.meta.quiz_questions = vec![
                QuizQuestion {
                    id: "q1".into(),
                    question: "First question?".into(),
                    options: vec!["Alpha".into(), "Beta".into()],
                    correct_index: Some(0),
                    answer: "Alpha is correct.".into(),
                    hints: vec!["Hint for Q1".into()],
                    question_image_hash: None,
                    answer_image_hash: None,
                },
                QuizQuestion {
                    id: "q2".into(),
                    question: "Second question?".into(),
                    options: vec!["Gamma".into(), "Delta".into()],
                    correct_index: Some(1),
                    answer: "Delta is correct.".into(),
                    hints: vec!["Hint for Q2".into()],
                    question_image_hash: None,
                    answer_image_hash: None,
                },
            ];
        }
        // Also add a Video node to verify desired_embeds() is suppressed while overlay covers the window
        let vid_id = rt
            .state
            .add_study_node(NodeKind::Video, Some(WorldPos::new(600.0, 140.0)));
        if let Some(v) = rt.state.graph.get_node_mut(&vid_id) {
            v.meta.media_url = Some("https://youtu.be/dQw4w9WgXcQ".into());
        }
        rt.state
            .embed_rects
            .insert(vid_id.clone(), [120.0, 120.0, 320.0, 180.0]);

        rt.state.select_node(quiz_id.clone(), false);
        rt.state.inspector_open = true;
        rt.state.ai_panel_open = false;
        rt.invalidate();
        settle_frames(rt, 8);

        let opt_0_0 = format!("quiz_opt_{}_0_0", quiz_id.as_str());
        let ans_btn = format!("quiz_ans_btn_{}", quiz_id.as_str());
        let start_btn = format!("quiz_start_btn_{}", quiz_id.as_str());
        assert!(has_element_key(rt, &opt_0_0));
        assert!(has_element_key(rt, &ans_btn));
        assert!(rt.state.desired_embeds().iter().any(|e| e.visible));

        click_key(rt, "btn_quiz_mode_popout");
        click_key(rt, "btn_quiz_diff_easy");
        settle_frames(rt, 6);

        let quiz_node = rt.state.graph.get_node(&quiz_id).unwrap();
        assert_eq!(quiz_node.meta.quiz_mode, QuizDisplayMode::Popout);
        assert_eq!(quiz_node.meta.quiz_difficulty, QuizDifficulty::Easy);
        assert!(has_element_key(rt, &start_btn));
        assert!(!has_element_key(rt, &opt_0_0));

        click_key(rt, &start_btn);
        settle_frames(rt, 6);
        assert_eq!(rt.state.active_quiz_overlay.as_ref(), Some(&quiz_id));
        assert!(has_element_key(rt, "quiz_popout_overlay"));
        assert!(has_element_key(rt, "btn_quiz_overlay_back"));
        assert!(rt.state.desired_embeds().iter().all(|e| !e.visible));

        click_key(rt, "quiz_overlay_opt_0");
        click_key(rt, "quiz_overlay_hint_btn");
        click_key(rt, "btn_quiz_overlay_next");
        settle_frames(rt, 4);
        assert_eq!(rt.state.quiz_sessions.get(&quiz_id).unwrap().active_index, 1);

        // Return to scheme via Back button
        click_key(rt, "btn_quiz_overlay_back");
        settle_frames(rt, 6);
        assert!(rt.state.active_quiz_overlay.is_none());
        assert!(has_element_key(rt, &start_btn));

        // Resume right where we left off
        click_key(rt, &start_btn);
        settle_frames(rt, 6);
        assert_eq!(rt.state.active_quiz_overlay.as_ref(), Some(&quiz_id));
        {
            let sess = rt.state.quiz_sessions.get(&quiz_id).unwrap();
            assert_eq!(sess.active_index, 1);
            assert_eq!(sess.selected_options.get(&0), Some(&0));
            assert_eq!(sess.revealed_hints.get(&0), Some(&1));
        }

        // Escape also returns in Easy mode and keeps progress when resumed again
        rt.key(KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        });
        settle_frames(rt, 6);
        assert!(rt.state.active_quiz_overlay.is_none());

        click_key(rt, &start_btn);
        settle_frames(rt, 6);
        assert_eq!(rt.state.quiz_sessions.get(&quiz_id).unwrap().active_index, 1);

        // Easy mode reset inside overlay clears progress and returns to Q1
        click_key(rt, "btn_quiz_overlay_reset");
        settle_frames(rt, 6);
        {
            let sess = rt.state.quiz_sessions.get(&quiz_id).unwrap();
            assert_eq!(sess.active_index, 0);
            assert!(sess.selected_options.is_empty());
            assert!(sess.revealed_hints.is_empty());
            assert_eq!(sess.status, crate::state::QuizAttemptStatus::InProgress);
        }

        click_key(rt, "quiz_overlay_opt_0");
        click_key(rt, "btn_quiz_overlay_next");
        settle_frames(rt, 4);
        click_key(rt, "quiz_overlay_opt_1");
        settle_frames(rt, 4);
        click_key(rt, "btn_quiz_overlay_finish");
        settle_frames(rt, 6);
        assert_eq!(
            rt.state.quiz_sessions.get(&quiz_id).unwrap().status,
            crate::state::QuizAttemptStatus::Completed
        );

        click_key(rt, "btn_quiz_overlay_done_back");
        settle_frames(rt, 6);
        assert!(rt.state.active_quiz_overlay.is_none());

        // Easy mode reset button on the canvas card clears the completed attempt
        let reset_btn = format!("quiz_reset_btn_{}", quiz_id.as_str());
        assert!(has_element_key(rt, &reset_btn));
        click_key(rt, &reset_btn);
        settle_frames(rt, 6);
        assert!(!rt.state.quiz_sessions.contains_key(&quiz_id));
    }

    #[test]
    fn test_quiz_popout_hard_mode_lock_give_up_fail_and_retry() {
        let mut th = make_test_runtime();
        let rt = &mut th.rt;
        rt.state.create_project("Quiz Popout Hard Test");
        let quiz_id = rt
            .state
            .add_study_node(NodeKind::Quiz, Some(WorldPos::new(160.0, 140.0)));
        if let Some(n) = rt.state.graph.get_node_mut(&quiz_id) {
            n.meta.quiz_questions = vec![
                QuizQuestion {
                    id: "hq1".into(),
                    question: "Hard question 1?".into(),
                    options: vec!["Choice A".into(), "Choice B".into()],
                    correct_index: Some(0),
                    answer: "Choice A explanation.".into(),
                    hints: vec!["Subtle hint".into()],
                    question_image_hash: None,
                    answer_image_hash: None,
                },
                QuizQuestion {
                    id: "hq2".into(),
                    question: "Hard question 2?".into(),
                    options: vec!["Choice C".into(), "Choice D".into()],
                    correct_index: Some(1),
                    answer: "Choice D explanation.".into(),
                    hints: vec![],
                    question_image_hash: None,
                    answer_image_hash: None,
                },
            ];
        }
        rt.state.select_node(quiz_id.clone(), false);
        rt.state.inspector_open = true;
        rt.state.ai_panel_open = false;
        rt.invalidate();
        settle_frames(rt, 8);

        click_key(rt, "btn_quiz_mode_popout");
        click_key(rt, "btn_quiz_diff_hard");
        settle_frames(rt, 6);

        let start_btn = format!("quiz_start_btn_{}", quiz_id.as_str());
        click_key(rt, &start_btn);
        settle_frames(rt, 6);
        assert_eq!(rt.state.active_quiz_overlay.as_ref(), Some(&quiz_id));
        assert!(rt.state.is_quiz_hard_locked());
        assert!(!has_element_key(rt, "btn_quiz_overlay_back"));
        assert!(!has_element_key(rt, "btn_quiz_overlay_reset"));
        assert!(has_element_key(rt, "btn_quiz_overlay_give_up"));
        let give_up_r = element_rect(rt, "btn_quiz_overlay_give_up");
        assert!(
            give_up_r[1] + give_up_r[3] <= 36.0 + 1.0,
            "give up button should sit cleanly inside 36px topbar: {:?}",
            give_up_r
        );

        click_key(rt, "quiz_overlay_opt_0");
        settle_frames(rt, 4);

        // Escape and programmatic return_to_scheme are blocked while Hard-locked
        rt.key(KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        });
        settle_frames(rt, 4);
        assert!(!rt.state.quiz_return_to_scheme());
        assert_eq!(rt.state.active_quiz_overlay.as_ref(), Some(&quiz_id));
        assert!(rt.state.is_quiz_hard_locked());

        // Workspace hotkeys (e.g. Delete) are also blocked while overlay is active
        rt.key(KeyEvent {
            key: Key::Delete,
            mods: Mods::default(),
        });
        settle_frames(rt, 4);
        assert!(rt.state.graph.get_node(&quiz_id).is_some());

        // Give Up marks the quiz Failed and unlocks returning to the scheme
        click_key(rt, "btn_quiz_overlay_give_up");
        settle_frames(rt, 6);
        {
            let sess = rt.state.quiz_sessions.get(&quiz_id).unwrap();
            assert_eq!(sess.status, crate::state::QuizAttemptStatus::Failed);
            assert!(sess.gave_up);
        }
        assert!(!rt.state.is_quiz_hard_locked());
        assert!(has_element_key(rt, "btn_quiz_overlay_try_again"));
        assert!(has_element_key(rt, "btn_quiz_overlay_done_back"));

        click_key(rt, "btn_quiz_overlay_done_back");
        settle_frames(rt, 6);
        assert!(rt.state.active_quiz_overlay.is_none());
        assert_eq!(
            rt.state.quiz_sessions.get(&quiz_id).unwrap().status,
            crate::state::QuizAttemptStatus::Failed
        );

        // Pressing "Try Again" on the canvas card starts a fresh locked attempt
        click_key(rt, &start_btn);
        settle_frames(rt, 6);
        assert_eq!(rt.state.active_quiz_overlay.as_ref(), Some(&quiz_id));
        assert!(rt.state.is_quiz_hard_locked());
        let restarted = rt.state.quiz_sessions.get(&quiz_id).unwrap();
        assert_eq!(restarted.status, crate::state::QuizAttemptStatus::InProgress);
        assert!(!restarted.gave_up);
        assert_eq!(restarted.active_index, 0);
        assert!(restarted.selected_options.is_empty());
    }

    #[test]
    fn test_quiz_question_and_answer_images_canvas_and_popout() {
        const ONE_PX_PNG: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        const ONE_PX_RED_PNG: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D,
            0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];

        let mut th = make_test_runtime();
        let q_png = th._dir.path().join("q_diagram.png");
        let ans_png = th._dir.path().join("ans_diagram.png");
        std::fs::write(&q_png, ONE_PX_PNG).expect("write q_diagram.png");
        std::fs::write(&ans_png, ONE_PX_RED_PNG).expect("write ans_diagram.png");

        let rt = &mut th.rt;
        rt.state.create_project("Quiz Images Test");
        let quiz_id = rt
            .state
            .add_study_node(NodeKind::Quiz, Some(WorldPos::new(140.0, 120.0)));
        if let Some(n) = rt.state.graph.get_node_mut(&quiz_id) {
            n.meta.quiz_questions.clear();
        }
        rt.state.select_node(quiz_id.clone(), false);
        rt.state.inspector_open = true;
        rt.state.ai_panel_open = false;
        rt.invalidate();
        settle_frames(rt, 8);

        // Attach draft question and answer images without typing prompt/explanation text
        assert!(rt.state.quiz_set_draft_image_from_path(false, &q_png));
        assert!(rt.state.quiz_set_draft_image_from_path(true, &ans_png));
        rt.invalidate();
        settle_frames(rt, 8);
        assert!(has_element_key(rt, "quiz_draft_q_img_preview"));
        assert!(has_element_key(rt, "quiz_draft_ans_img_preview"));
        assert!(has_element_key(rt, "btn_quiz_draft_q_img_clear"));
        assert!(has_element_key(rt, "btn_quiz_draft_ans_img_clear"));

        click_key(rt, "btn_quiz_add_q");
        settle_frames(rt, 8);
        assert!(rt.state.quiz_q_img_hash.is_none());
        assert!(rt.state.quiz_ans_img_hash.is_none());

        let (q_hash, ans_hash) = {
            let qs = &rt
                .state
                .graph
                .get_node(&quiz_id)
                .unwrap()
                .meta
                .quiz_questions;
            assert_eq!(qs.len(), 1);
            (
                qs[0].question_image_hash.clone().expect("question image hash"),
                qs[0].answer_image_hash.clone().expect("answer image hash"),
            )
        };
        assert_ne!(q_hash, ans_hash);

        // Inspector card and Regular canvas card display images
        assert!(has_element_key(rt, "quiz_insp_q_img_0"));
        assert!(has_element_key(rt, "quiz_insp_ans_img_0"));
        let canvas_q_img = format!("quiz_q_img_{}_0", quiz_id.as_str());
        let canvas_ans_btn = format!("quiz_ans_btn_{}", quiz_id.as_str());
        let canvas_ans_img = format!("quiz_ans_img_{}_0", quiz_id.as_str());
        assert!(has_element_key(rt, &canvas_q_img));
        assert!(has_element_key(rt, &canvas_ans_btn));
        assert!(!has_element_key(rt, &canvas_ans_img));

        click_key(rt, &canvas_ans_btn);
        settle_frames(rt, 6);
        assert!(has_element_key(rt, &canvas_ans_img));

        // Reset before testing Popout overlay display
        rt.state.quiz_reset_session(&quiz_id);
        click_key(rt, "btn_quiz_mode_popout");
        settle_frames(rt, 6);

        let start_btn = format!("quiz_start_btn_{}", quiz_id.as_str());
        click_key(rt, &start_btn);
        settle_frames(rt, 8);
        assert!(has_element_key(rt, "quiz_overlay_q_img"));
        assert!(has_element_key(rt, "quiz_overlay_ans_btn"));
        assert!(!has_element_key(rt, "quiz_overlay_ans_img"));

        click_key(rt, "quiz_overlay_ans_btn");
        settle_frames(rt, 6);
        assert!(has_element_key(rt, "quiz_overlay_ans_img"));

        click_key(rt, "btn_quiz_overlay_finish");
        settle_frames(rt, 6);
        assert!(has_element_key(rt, "quiz_overlay_rev_q_img_0"));
        assert!(has_element_key(rt, "quiz_overlay_rev_ans_img_0"));

        click_key(rt, "btn_quiz_overlay_done_back");
        settle_frames(rt, 6);
        click_key(rt, "btn_quiz_mode_regular");
        settle_frames(rt, 6);

        // Clear answer image from existing question via inspector button
        click_key(rt, "quiz_ans_img_clear_0");
        settle_frames(rt, 6);
        assert!(
            rt.state
                .graph
                .get_node(&quiz_id)
                .unwrap()
                .meta
                .quiz_questions[0]
                .answer_image_hash
                .is_none()
        );
        assert!(!has_element_key(rt, "quiz_insp_ans_img_0"));

        // Replace question image on existing question and verify orphan pruning preserves active asset
        assert!(rt
            .state
            .quiz_set_question_image_from_path(&quiz_id, 0, false, &ans_png));
        rt.invalidate();
        settle_frames(rt, 6);
        assert_eq!(
            rt.state
                .graph
                .get_node(&quiz_id)
                .unwrap()
                .meta
                .quiz_questions[0]
                .question_image_hash
                .as_deref(),
            Some(ans_hash.as_str())
        );
        let pruned = rt
            .state
            .bundle
            .as_ref()
            .unwrap()
            .prune_orphan_assets(&rt.state.graph)
            .unwrap();
        assert_eq!(pruned, 1);
        assert!(rt.state.asset_path_cache.get(&ans_hash).unwrap().exists());
    }
}
