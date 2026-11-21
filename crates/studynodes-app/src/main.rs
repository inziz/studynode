#![deny(unsafe_code)]

mod canvas_view;
mod dashboard_view;
mod doc_reader;
mod inspector_view;
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

            if s.inspector_open {
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
                AppScreen::Dashboard => "workspaces".into(),
                AppScreen::Workspace => format!(
                    "{} nodes · {} links",
                    s.graph.nodes.len(),
                    s.graph.edges.len()
                ),
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

fn setup_workspace_demo(s: &mut AppState) {
    s.create_project("Cognitive Neuroscience");

    let mut n1 = studynodes_graph::model::Node::new(
        studynodes_graph::model::NodeKind::Document,
        "Hebbian Learning (1949)",
        studynodes_graph::coords::WorldPos::new(30.0, 65.0),
    );
    n1.size = node_base_size(n1.kind);
    n1.content = r#"# The Organization of Behavior
*Donald O. Hebb (1949)*

> **Core Postulate:** Cells that fire together wire together when axon A persistently excites cell B.

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
        studynodes_graph::coords::WorldPos::new(490.0, 50.0),
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
        studynodes_graph::coords::WorldPos::new(430.0, 340.0),
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
    s.status_message = "Loaded 'Cognitive Neuroscience' · 3 nodes, 2 links".into();
}

fn main() {
    let mut spec = AppSpec::new(
        "StudyNodes - Visual Knowledge Canvas",
        (1280.0, 800.0),
        AppState::default,
        view,
    );
    spec.scenes = &[("dashboard", |_s| {}), ("workspace", setup_workspace_demo)];
    spec.setup = Some(|s, waker| {
        touchpad_win32::set_waker(waker.clone());
        s.set_image_waker(waker);
    });
    spec.poll = Some(|s, _dt, _tx| {
        touchpad_win32::pump_updates();
        let events = touchpad_win32::take_events();
        s.apply_touchpad_events(events)
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
    use rune::{Env, Key, KeyEvent, Mods, Runtime, WheelEvent};
    use studynodes_graph::coords::{ScreenPos, WorldPos};
    use studynodes_graph::model::NodeKind;

    fn make_test_runtime() -> Runtime<AppState> {
        let env = Env {
            system_dark: true,
            scale: 2.0,
        };
        let mut rt = Runtime::new(AppState::default(), view, (1280.0, 800.0), env);
        rt.on_key = Some(|s, ev, tx| {
            if tx.focused().is_none() && ev.key == Key::Text(" ".into()) {
                s.space_held = true;
                return true;
            }
            false
        });
        rt.frame(0.0);
        rt
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
        let mut rt = make_test_runtime();
        assert_eq!(rt.state.screen, AppScreen::Dashboard);

        rt.state.create_project("Neuroscience 101");
        rt.frame(1.0 / 60.0);

        assert_eq!(rt.state.screen, AppScreen::Workspace);
        assert_eq!(rt.state.graph.title, "Neuroscience 101");
    }

    #[test]
    fn test_node_creation_and_edge_linking() {
        let mut rt = make_test_runtime();
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
        let mut rt = make_test_runtime();
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
        let mut rt = make_test_runtime();
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
        let mut rt = make_test_runtime();
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
        let mut rt = make_test_runtime();
        rt.state.create_project("Real Click & Typing Test");

        let note_id = rt.state.add_note_node(Some(WorldPos::new(120.0, 140.0)));
        rt.state.deselect_all();
        rt.invalidate();
        settle_frames(&mut rt, 3);

        let body_key = format!("node_body_{}", note_id.as_str());
        let r = element_rect(&rt, &body_key);
        let (cx, cy) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);

        rt.pointer_move(cx, cy);
        rt.pointer_down(cx, cy);
        rt.pointer_up(cx, cy);
        settle_frames(&mut rt, 4);

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
        settle_frames(&mut rt, 3);

        let content = &rt.state.graph.get_node(&note_id).unwrap().content;
        assert!(
            content.contains("Synaptic gain"),
            "expected typed text in node.content, got '{content}'"
        );
    }

    #[test]
    fn test_click_document_opens_onscreen_reader_panel() {
        let mut rt = make_test_runtime();
        rt.state.create_project("Document Reader Test");

        let doc_id = rt.state.add_document_node(Some(WorldPos::new(100.0, 120.0)));
        rt.state.inspector_open = false;
        rt.state.deselect_all();
        rt.invalidate();
        settle_frames(&mut rt, 3);

        let doc_body_key = format!("node_body_{}", doc_id.as_str());
        let r = element_rect(&rt, &doc_body_key);
        let (cx, cy) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);

        rt.pointer_move(cx, cy);
        rt.pointer_down(cx, cy);
        rt.pointer_up(cx, cy);
        settle_frames(&mut rt, 35);

        assert!(
            rt.state.inspector_open,
            "clicking a Document card must pop open inspector_open"
        );

        let inspector_r = element_rect(&rt, "inspector_panel");
        let canvas_r = element_rect(&rt, "graph_canvas");

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
        let mut rt = make_test_runtime();
        rt.state.create_project("Header Drag Test");

        let note_id = rt.state.add_note_node(Some(WorldPos::new(150.0, 150.0)));
        rt.invalidate();
        settle_frames(&mut rt, 3);

        let header_key = format!("node_header_{}", note_id.as_str());
        let hr = element_rect(&rt, &header_key);
        // Grab the empty right-center area of the header grip
        let (sx, sy) = (hr[0] + hr[2] - 45.0, hr[1] + hr[3] * 0.5);

        rt.pointer_move(sx, sy);
        rt.pointer_down(sx, sy);
        rt.pointer_move(sx + 40.0, sy + 25.0);
        rt.pointer_move(sx + 80.0, sy + 50.0);
        rt.pointer_up(sx + 80.0, sy + 50.0);
        settle_frames(&mut rt, 3);

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
        let mut rt = make_test_runtime();
        rt.state.create_project("Touchpad 2D Pan Test");
        rt.invalidate();
        settle_frames(&mut rt, 2);

        // 1. DirectManipulation TouchpadEvent path moves X and Y simultaneously
        let changed = rt.state.apply_touchpad_events(vec![
            touchpad_win32::TouchpadEvent::Pan { dx: 60.0, dy: -40.0 },
        ]);
        assert!(changed);
        assert!((rt.state.graph.viewport.offset.x - (-60.0)).abs() < 1e-4);
        assert!((rt.state.graph.viewport.offset.y - 40.0).abs() < 1e-4);

        // 2. Precision WheelEvent over canvas moves X and Y simultaneously too
        let cr = element_rect(&rt, "graph_canvas");
        let (cx, cy) = (cr[0] + cr[2] * 0.5, cr[1] + cr[3] * 0.5);
        rt.pointer_move(cx, cy);
        rt.wheel_event(WheelEvent::new(30.0, 20.0).at(cx, cy).precise(true));
        settle_frames(&mut rt, 2);

        assert!(
            (rt.state.graph.viewport.offset.x - (-90.0)).abs() < 1e-3,
            "expected offset.x -90, got {}",
            rt.state.graph.viewport.offset.x
        );
        assert!(
            (rt.state.graph.viewport.offset.y - 20.0).abs() < 1e-3,
            "expected offset.y 20, got {}",
            rt.state.graph.viewport.offset.y
        );
    }

    #[test]
    fn test_short_edge_pill_renders_above_nodes_and_centered() {
        let mut rt = make_test_runtime();
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
        settle_frames(&mut rt, 3);

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
}
