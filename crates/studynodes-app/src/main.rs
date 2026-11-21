#![deny(unsafe_code)]

mod canvas_view;
mod dashboard_view;
mod doc_reader;
mod inspector_view;
mod reader_window;
mod state;
mod theme;
mod touchpad_win32;

use rune::tok::space;
use rune::{App, AppSpec, Cx, El, Titlebar, col, el, row, text};

use crate::canvas_view::{render_canvas_view, render_workspace_topbar};
use crate::dashboard_view::render_dashboard;
use crate::inspector_view::render_inspector;
use crate::state::{AppScreen, AppState, node_base_size};

fn view(s: &AppState, cx: &mut Cx<AppState>) -> El {
    cx.caption_color(theme::text_secondary());
    cx.titlebar_height(40.0);

    if s.screen == AppScreen::Dashboard {
        touchpad_win32::set_canvas_bounds(None);
    }

    let main_content = match s.screen {
        AppScreen::Dashboard => render_dashboard(s, cx),
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
    };

    // Sleek Monochrome 26px Bottom Status Bar (Claude Code-esque)
    let status_bar = row()
        .key("status_bar")
        .w_full()
        .h(26.0)
        .px(12.0)
        .gap(space::S2)
        .center_y()
        .fill(theme::bg_chrome())
        .border_sides([1.0, 0.0, 0.0, 0.0], theme::border_subtle())
        .child(
            text("✦")
                .font(theme::STATUS_TEXT)
                .color(theme::accent_claude()),
        )
        .child(
            text("studynodes")
                .font(theme::STATUS_TEXT)
                .color(theme::text_primary()),
        )
        .child(text("·").font(theme::STATUS_TEXT).color(theme::text_muted()))
        .child(
            text(&s.status_message)
                .font(theme::STATUS_TEXT)
                .color(theme::text_secondary()),
        )
        .child(el().grow(1.0))
        .child(
            text(match s.screen {
                AppScreen::Dashboard => format!("{} workspaces · {} open", s.recent_projects.len(), s.tabs.len()),
                AppScreen::Workspace => {
                    if s.tabs.len() > 1 {
                        format!(
                            "{} tabs · {} nodes · {} links",
                            s.tabs.len(),
                            s.graph.nodes.len(),
                            s.graph.edges.len()
                        )
                    } else {
                        format!(
                            "{} nodes · {} links",
                            s.graph.nodes.len(),
                            s.graph.edges.len()
                        )
                    }
                }
            })
            .font(theme::STATUS_TEXT)
            .color(theme::text_muted()),
        );

    col()
        .key("app_root")
        .w_full()
        .h_full()
        .fill(theme::bg_canvas())
        .child(main_content)
        .child(status_bar)
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

    let mut n1 = studynodes_graph::model::Node::new(
        studynodes_graph::model::NodeKind::Document,
        "Hebbian Learning (1949)",
        studynodes_graph::coords::WorldPos::new(24.0, 56.0),
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
        studynodes_graph::coords::WorldPos::new(470.0, 48.0),
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
            node: id2.clone(),
            port: studynodes_graph::model::PortId::new("in_main"),
        },
    );
    edge.relation = studynodes_graph::model::EdgeRelation::Supports;
    edge.label = "Validates".into();
    edge.note = "Biochemical validation through NMDA activation".into();
    s.graph.add_edge(edge);

    let mut synth = studynodes_graph::model::Node::new(
        studynodes_graph::model::NodeKind::LlmSynthesis,
        "Memory Consolidation",
        studynodes_graph::coords::WorldPos::new(430.0, 350.0),
    );
    synth.size = node_base_size(synth.kind);
    synth.content = "Hebbian plasticity provides the local synaptic substrate for hippocampal-neocortical consolidation.".into();
    let synth_id = synth.id.clone();
    s.graph.add_node(synth);

    let ghost_edge = studynodes_graph::model::Edge::proposed_by_llm(
        studynodes_graph::model::EdgeEndpoint {
            node: id1.clone(),
            port: studynodes_graph::model::PortId::new("out_main"),
        },
        studynodes_graph::model::EdgeEndpoint {
            node: synth_id,
            port: studynodes_graph::model::PortId::new("in_main"),
        },
        studynodes_graph::model::EdgeRelation::DerivedFrom,
        "Cellular substrate",
        "Provides cellular substrate",
    );
    s.graph.add_edge(ghost_edge);

    // Select Document so the right-side Document Reader is open and immediately usable
    s.select_node(id1, false);
    s.save_active_into_tab();
    s.refresh_recent_projects();
    s.status_message = "Loaded 'Cognitive Neuroscience' · 3 nodes, 2 links".into();
}

fn setup_workspace_demo(s: &mut AppState) {
    populate_demo_projects(s);
}

fn setup_dashboard_demo(s: &mut AppState) {
    populate_demo_projects(s);
    s.close_to_dashboard();
}

fn main() {
    let mut spec = AppSpec::new(
        "StudyNodes - Visual Knowledge Canvas",
        (1280.0, 800.0),
        AppState::default,
        view,
    );
    spec.scenes = &[
        ("dashboard", setup_dashboard_demo),
        ("workspace", setup_workspace_demo),
    ];
    spec.setup = Some(|s, waker| {
        touchpad_win32::set_waker(waker.clone());
        s.set_wakers(waker);
    });
    spec.poll = Some(|s, _dt, _tx| {
        touchpad_win32::pump_updates();
        let events = touchpad_win32::take_events();
        let tp_changed = s.apply_touchpad_events(events);
        let bridge_changed = s.poll_reader_bridge();
        tp_changed || bridge_changed
    });
    spec.on_key = Some(|s, ev, tx| {
        if tx.focused().is_none() && ev.key == rune::Key::Text(" ".into()) {
            s.space_held = true;
            return true;
        }
        false
    });

    App::new(spec)
        .titlebar(Titlebar::new(40.0))
        .on_window(|w| touchpad_win32::attach(w))
        .run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Key, KeyEvent, MagnifyEvent, Mods, Runtime, WheelEvent};
    use studynodes_graph::coords::{ScreenPos, WorldPos};
    use studynodes_graph::model::NodeKind;
    use tempfile::TempDir;

    struct TestHarness {
        _dir: TempDir,
        rt: Runtime<AppState>,
    }

    fn make_test_runtime() -> TestHarness {
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
        rt.on_key = Some(|s, ev, tx| {
            if tx.focused().is_none() && ev.key == Key::Text(" ".into()) {
                s.space_held = true;
                return true;
            }
            false
        });
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
}
