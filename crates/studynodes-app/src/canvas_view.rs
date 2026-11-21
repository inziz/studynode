use rune::{
    Color, Cursor, Cx, El, LineCap, Phase, PointerButton, ScrollSpec, Shape, TextStyle, Tx,
    canvas, col, el, row, text, text_input,
};

use studynodes_graph::bezier::CubicBezier;
use studynodes_graph::coords::{ScreenPos, WorldPos};
use studynodes_graph::model::{Edge, EdgeStatus, Node, NodeKind, PortDirection, PortId};

use crate::state::{AppState, CanvasMode, node_base_size};
use crate::theme;

fn port_world_pos_for(node: &Node, port_id: &PortId) -> WorldPos {
    let (w, h) = node_base_size(node.kind);
    let y_center = node.pos.y + h * 0.5;
    if let Some(port) = node.ports.iter().find(|p| &p.id == port_id) {
        match port.direction {
            PortDirection::Input => WorldPos::new(node.pos.x, y_center),
            PortDirection::Output => WorldPos::new(node.pos.x + w, y_center),
        }
    } else if port_id.as_str() == PortId::DEFAULT_OUT {
        WorldPos::new(node.pos.x + w, y_center)
    } else {
        WorldPos::new(node.pos.x, y_center)
    }
}

pub fn render_canvas_view(s: &AppState) -> El {
    let cam = s.graph.viewport;
    let zoom = cam.zoom;

    // 1. Vector Canvas Layer (Infinite Grid + Bézier Wires)
    let mut canvas_shape = Shape::new();

    let base_spacing = 48.0 * zoom;
    if base_spacing >= 10.0 {
        let minor_color = Color::WHITE.alpha(0.028);
        let major_color = Color::WHITE.alpha(0.055);

        let world_left_idx = (cam.offset.x / 48.0).floor() as i32;
        let offset_x = (cam.offset.x * zoom).rem_euclid(base_spacing);
        let mut x = if offset_x == 0.0 { 0.0 } else { base_spacing - offset_x };
        let mut ix = world_left_idx + if offset_x == 0.0 { 0 } else { 1 };
        while x < 3840.0 {
            let is_major = ix.rem_euclid(4) == 0;
            let (col, w) = if is_major { (major_color, 0.75) } else { (minor_color, 0.5) };
            canvas_shape = canvas_shape.line([x, 0.0], [x, 2160.0], w, LineCap::Butt, col);
            x += base_spacing;
            ix += 1;
        }

        let world_top_idx = (cam.offset.y / 48.0).floor() as i32;
        let offset_y = (cam.offset.y * zoom).rem_euclid(base_spacing);
        let mut y = if offset_y == 0.0 { 0.0 } else { base_spacing - offset_y };
        let mut iy = world_top_idx + if offset_y == 0.0 { 0 } else { 1 };
        while y < 2160.0 {
            let is_major = iy.rem_euclid(4) == 0;
            let (col, w) = if is_major { (major_color, 0.75) } else { (minor_color, 0.5) };
            canvas_shape = canvas_shape.line([0.0, y], [3840.0, y], w, LineCap::Butt, col);
            y += base_spacing;
            iy += 1;
        }
    }

    // Connection wires
    for edge in s.graph.edges.values() {
        if let (Some(from_node), Some(to_node)) = (
            s.graph.get_node(&edge.from.node),
            s.graph.get_node(&edge.to.node),
        ) {
            let from_world = port_world_pos_for(from_node, &edge.from.port);
            let to_world = port_world_pos_for(to_node, &edge.to.port);

            let curve = CubicBezier::wire(from_world, to_world);
            let segments: Vec<[f32; 2]> = curve
                .sample_segments(24)
                .into_iter()
                .map(|wp| {
                    let sp = cam.world_to_screen(wp);
                    [sp.x, sp.y]
                })
                .collect();

            let is_selected = s.selected_edge.as_ref() == Some(&edge.id);
            let is_ghost = edge.status == EdgeStatus::Proposed;

            let (wire_color, wire_w) = if is_selected {
                (theme::accent_blue(), 2.6 * zoom.clamp(0.7, 1.4))
            } else if is_ghost {
                (theme::accent_violet().alpha(0.85), 2.2 * zoom.clamp(0.7, 1.4))
            } else {
                (theme::wire_default(), 1.8 * zoom.clamp(0.7, 1.4))
            };

            canvas_shape = canvas_shape.stroke(segments, wire_w, wire_color);
        }
    }

    // Active wire pulling preview
    if let CanvasMode::DraggingWire {
        from_node,
        from_port,
        current_cursor,
    } = &s.canvas_mode
    {
        if let Some(node) = s.graph.get_node(from_node) {
            let from_world = port_world_pos_for(node, from_port);
            let curve = CubicBezier::wire(from_world, *current_cursor);
            let segments: Vec<[f32; 2]> = curve
                .sample_segments(24)
                .into_iter()
                .map(|wp| {
                    let sp = cam.world_to_screen(wp);
                    [sp.x, sp.y]
                })
                .collect();

            canvas_shape = canvas_shape.stroke(
                segments,
                2.4 * zoom.clamp(0.7, 1.4),
                theme::accent_amber(),
            );
        }
    }

    // 2. Interactive Canvas Container (grow(1.0).min_w(0.0) so right-side Inspector fits on screen!)
    let mut canvas_container = el()
        .key("graph_canvas")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .clip_soft(0.0)
        .fill(theme::bg_canvas())
        .cursor(if s.space_held { Cursor::Grab } else { Cursor::Default })
        .pointer_buttons([PointerButton::Primary, PointerButton::Middle])
        .on_layout_tx(|s: &mut AppState, r, tx| {
            s.canvas_origin = [r[0], r[1]];
            crate::touchpad_win32::set_canvas_bounds(Some(r));
            tx.live_only();
        })
        .child(canvas(canvas_shape).w_full().h_full())
        .on_wheel(|s: &mut AppState, e, _| {
            if e.mods.ctrl || e.mods.cmd {
                let factor = if e.dy > 0.0 { 1.10 } else { 0.90 };
                s.graph.viewport.zoom_at(ScreenPos::new(e.x, e.y), factor);
            } else if e.mods.shift && e.dx == 0.0 {
                s.graph.viewport.pan_by_screen_delta(e.dy, 0.0);
            } else {
                s.graph.viewport.pan_by_screen_delta(e.dx, e.dy);
            }
            true
        })
        .on_magnify(|s: &mut AppState, e, _| {
            let factor = (1.0 + e.delta).clamp(0.5, 2.0);
            s.graph.viewport.zoom_at(ScreenPos::new(e.x, e.y), factor);
            true
        })
        .on_drop_files(|s: &mut AppState, files, _| {
            s.handle_dropped_files(files, ScreenPos::new(400.0, 300.0));
        })
        .on_drag(|s: &mut AppState, d, tx| match d.phase {
            Phase::Down => {
                s.canvas_mode = CanvasMode::Panning {
                    start_screen: ScreenPos::new(d.x, d.y),
                    start_offset: s.graph.viewport.offset,
                };
            }
            Phase::Move => {
                if let CanvasMode::Panning { start_offset, .. } = s.canvas_mode {
                    let z = s.graph.viewport.zoom.max(0.01);
                    s.graph.viewport.offset.x = start_offset.x - (d.dx / z);
                    s.graph.viewport.offset.y = start_offset.y - (d.dy / z);
                }
            }
            Phase::Up => {
                if !d.cancelled()
                    && !s.space_held
                    && tx.button() == PointerButton::Primary
                    && d.dx.hypot(d.dy) < 4.0
                {
                    s.deselect_all();
                    tx.blur();
                }
                s.canvas_mode = CanvasMode::Idle;
            }
        });

    // 4. Edge Annotation Pills (rendered below nodes cleanly)
    if zoom >= 0.55 {
        let mut edges: Vec<&Edge> = s.graph.edges.values().collect();
        edges.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        for edge in edges {
            if let (Some(from_node), Some(to_node)) = (
                s.graph.get_node(&edge.from.node),
                s.graph.get_node(&edge.to.node),
            ) {
                let from_world = port_world_pos_for(from_node, &edge.from.port);
                let to_world = port_world_pos_for(to_node, &edge.to.port);
                let curve = CubicBezier::wire(from_world, to_world);
                let mid_screen = cam.world_to_screen(curve.midpoint());
                canvas_container = canvas_container.child(render_edge_pill(
                    edge,
                    s.selected_edge.as_ref() == Some(&edge.id),
                    mid_screen,
                    zoom,
                ));
            }
        }
    }

    // 5. Node Cards (deterministic z-order: unselected sorted by id, selected on top)
    let mut ordered_nodes: Vec<&Node> = s.graph.nodes.values().collect();
    ordered_nodes.sort_by(|a, b| {
        s.selected_nodes
            .contains(&a.id)
            .cmp(&s.selected_nodes.contains(&b.id))
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });

    for node in ordered_nodes {
        let card = render_node_card(s, node, cam.world_to_screen(node.pos), zoom);
        canvas_container = canvas_container.child(card);
    }

    canvas_container
}

fn topbar_pill_btn(
    key: &'static str,
    dot_color: Option<Color>,
    label: &'static str,
) -> El {
    let mut content = row().center_y().gap(6.0);
    if let Some(c) = dot_color {
        content = content.child(el().size(6.0, 6.0).radius(3.0).fill(c));
    }
    content = content.child(
        text(label)
            .font(theme::CONTROL_TEXT)
            .color(theme::text_primary()),
    );

    el()
        .key(key)
        .h(26.0)
        .px(10.0)
        .radius(6.0)
        .fill(theme::bg_control())
        .hover(theme::hover_subtle())
        .press(theme::press_subtle())
        .border(1.0, theme::border_subtle())
        .center()
        .cursor(Cursor::Pointer)
        .no_window_drag()
        .child(content)
}

pub fn render_workspace_topbar(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let insets = cx.chrome_insets();

    row()
        .key("workspace_topbar")
        .w_full()
        .h(40.0)
        .pl(insets.leading + 10.0)
        .pr(insets.trailing + 10.0)
        .gap(8.0)
        .center_y()
        .fill(theme::bg_chrome())
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
        .window_drag()
        // Left: concise Back + Project Title
        .child(
            el()
                .key("btn_back")
                .h(26.0)
                .px(8.0)
                .radius(6.0)
                .fill(theme::bg_control())
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .border(1.0, theme::border_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press(|s: &mut AppState| {
                    s.close_to_dashboard();
                })
                .child(
                    text("←")
                        .font(theme::CONTROL_TEXT)
                        .color(theme::text_secondary()),
                ),
        )
        .child(
            el()
                .shrink()
                .min_w(0.0)
                .child(
                    text(&s.graph.title)
                        .font(theme::TITLE_BAR)
                        .color(theme::text_primary())
                        .truncate(),
                ),
        )
        // Middle draggable window band spacer
        .child(el().grow(1.0))
        // Right: concise creation + AI + Zoom + Inspector toggle
        .child(
            topbar_pill_btn("btn_add_note", Some(theme::accent_amber()), "+ Note").on_press_tx(
                |s: &mut AppState, tx: &mut Tx| {
                    let offset_x = (s.graph.nodes.len() as f32 * 32.0) % 160.0;
                    let offset_y = (s.graph.nodes.len() as f32 * 24.0) % 120.0;
                    let pos = s
                        .graph
                        .viewport
                        .screen_to_world(ScreenPos::new(200.0 + offset_x, 120.0 + offset_y));
                    let id = s.add_note_node(Some(pos));
                    tx.focus_when_shown(format!("note_body_{}", id.as_str()), false);
                },
            ),
        )
        .child(
            topbar_pill_btn("btn_add_doc", Some(theme::accent_blue()), "+ Doc").on_press(
                |s: &mut AppState| {
                    let offset_x = (s.graph.nodes.len() as f32 * 32.0) % 160.0;
                    let offset_y = (s.graph.nodes.len() as f32 * 24.0) % 120.0;
                    let pos = s
                        .graph
                        .viewport
                        .screen_to_world(ScreenPos::new(240.0 + offset_x, 150.0 + offset_y));
                    s.add_document_node(Some(pos));
                },
            ),
        )
        .child(
            el()
                .key("btn_ai")
                .h(26.0)
                .px(10.0)
                .radius(6.0)
                .fill(if s.ai_panel_open {
                    theme::accent_violet().alpha(0.24)
                } else {
                    theme::accent_violet().alpha(0.13)
                })
                .hover(theme::accent_violet().alpha(0.22))
                .press(theme::accent_violet().alpha(0.30))
                .border(1.0, theme::accent_violet().alpha(0.35))
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press(|s: &mut AppState| {
                    s.ai_panel_open = !s.ai_panel_open;
                    if s.ai_panel_open {
                        s.inspector_open = true;
                        s.status_message = "AI Synthesis ready".into();
                    }
                })
                .child(
                    text("✦ AI")
                        .font(theme::CONTROL_TEXT)
                        .color(theme::accent_violet()),
                ),
        )
        // Compact segmented zoom pill
        .child(
            row()
                .key("zoom_pill")
                .h(26.0)
                .center_y()
                .radius(6.0)
                .fill(theme::bg_control())
                .border(1.0, theme::border_subtle())
                .no_window_drag()
                .child(
                    el()
                        .key("btn_zoom_out")
                        .h_full()
                        .px(7.0)
                        .center()
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.zoom = (s.graph.viewport.zoom * 0.85).max(0.2);
                        })
                        .child(
                            text("−")
                                .font(theme::CONTROL_TEXT)
                                .color(theme::text_secondary()),
                        ),
                )
                .child(
                    el()
                        .key("btn_zoom_reset")
                        .h_full()
                        .px(6.0)
                        .center()
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.zoom = 1.0;
                            s.graph.viewport.offset = WorldPos::ZERO;
                        })
                        .child(
                            text(format!(
                                "{}%",
                                (s.graph.viewport.zoom * 100.0).round() as u32
                            ))
                            .font(theme::STATUS_TEXT)
                            .color(theme::text_secondary()),
                        ),
                )
                .child(
                    el()
                        .key("btn_zoom_in")
                        .h_full()
                        .px(7.0)
                        .center()
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.zoom = (s.graph.viewport.zoom * 1.15).min(3.0);
                        })
                        .child(
                            text("+")
                                .font(theme::CONTROL_TEXT)
                                .color(theme::text_secondary()),
                        ),
                ),
        )
        // Sidebar toggle button
        .child(
            el()
                .key("btn_toggle_panel")
                .size(26.0, 26.0)
                .radius(6.0)
                .fill(if s.inspector_open {
                    theme::accent_blue().alpha(0.16)
                } else {
                    theme::bg_control()
                })
                .hover(theme::hover_subtle())
                .border(
                    1.0,
                    if s.inspector_open {
                        theme::accent_blue().alpha(0.35)
                    } else {
                        theme::border_subtle()
                    },
                )
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press(|s: &mut AppState| {
                    s.inspector_open = !s.inspector_open;
                })
                .child(
                    text("◧")
                        .font(theme::CONTROL_TEXT)
                        .color(if s.inspector_open {
                            theme::accent_blue()
                        } else {
                            theme::text_secondary()
                        }),
                ),
        )
}

fn render_node_card(s: &AppState, node: &Node, screen_pos: ScreenPos, zoom: f32) -> El {
    let is_selected = s.selected_nodes.contains(&node.id);
    let (base_w, base_h) = node_base_size(node.kind);
    let w = base_w * zoom;
    let h = base_h * zoom;

    let is_snap_target = if let CanvasMode::DraggingWire {
        ref from_node,
        current_cursor,
        ..
    } = s.canvas_mode
    {
        if from_node != &node.id {
            let in_world = WorldPos::new(node.pos.x, node.pos.y + base_h * 0.5);
            current_cursor.distance_to(&in_world) < 65.0
                || (current_cursor.x >= node.pos.x - 20.0
                    && current_cursor.x <= node.pos.x + base_w + 20.0
                    && current_cursor.y >= node.pos.y - 20.0
                    && current_cursor.y <= node.pos.y + base_h + 20.0)
        } else {
            false
        }
    } else {
        false
    };

    let kind_accent = theme::kind_accent(node.kind);
    let border_color = if is_snap_target {
        theme::accent_amber()
    } else if is_selected {
        kind_accent.alpha(0.85)
    } else {
        theme::border_default()
    };

    let card_bg = if is_selected {
        theme::bg_card_selected()
    } else {
        theme::bg_card()
    };

    let title_size = (12.5 * zoom).clamp(8.5, 19.0);
    let body_size = (12.0 * zoom).clamp(8.0, 16.0);
    let badge_size = (9.5 * zoom).clamp(7.0, 13.0);
    let card_radius = (10.0 * zoom).clamp(5.0, 14.0);
    let socket_r = (5.0 * zoom).clamp(3.0, 8.5);
    let socket_d = socket_r * 2.0;

    let node_id_for_drag = node.id.clone();
    let out_node_id = node.id.clone();
    let del_node_id = node.id.clone();
    let note_click_id = node.id.clone();
    let doc_click_id = node.id.clone();
    let other_click_id = node.id.clone();

    // Sockets straddle left & right card boundaries outside the rounded inner clip wrapper
    let in_socket = if is_snap_target {
        let snap_r = socket_r * 1.6;
        let snap_d = snap_r * 2.0;
        el()
            .key(format!("{}_in_socket", node.id.as_str()))
            .absolute(-snap_r, h * 0.5 - snap_r)
            .size(snap_d, snap_d)
            .radius(snap_r)
            .fill(theme::accent_amber().alpha(0.35))
            .border(1.5, theme::accent_amber())
            .center()
            .child(
                el().size(snap_r, snap_r)
                    .radius(snap_r * 0.5)
                    .fill(theme::accent_amber()),
            )
    } else {
        el()
            .key(format!("{}_in_socket", node.id.as_str()))
            .absolute(-socket_r, h * 0.5 - socket_r)
            .size(socket_d, socket_d)
            .radius(socket_r)
            .fill(theme::bg_canvas())
            .border(1.5, kind_accent.alpha(0.85))
            .center()
            .child(
                el().size(socket_r * 0.75, socket_r * 0.75)
                    .radius(socket_r * 0.375)
                    .fill(kind_accent),
            )
    };

    let out_socket = el()
        .key(format!("{}_out_socket", node.id.as_str()))
        .absolute(w - socket_r, h * 0.5 - socket_r)
        .size(socket_d, socket_d)
        .radius(socket_r)
        .fill(theme::bg_canvas())
        .border(1.5, kind_accent.alpha(0.85))
        .cursor(Cursor::Crosshair)
        .center()
        .child(
            el().size(socket_r * 0.75, socket_r * 0.75)
                .radius(socket_r * 0.375)
                .fill(kind_accent),
        )
        .on_drag(move |s: &mut AppState, d, _| match d.phase {
            Phase::Down => {
                if let Some(node) = s.graph.get_node(&out_node_id) {
                    let start_world =
                        port_world_pos_for(node, &PortId::new(PortId::DEFAULT_OUT));
                    s.canvas_mode = CanvasMode::DraggingWire {
                        from_node: out_node_id.clone(),
                        from_port: PortId::new(PortId::DEFAULT_OUT),
                        current_cursor: start_world,
                    };
                }
            }
            Phase::Move => {
                if let CanvasMode::DraggingWire {
                    ref from_node,
                    ref from_port,
                    ..
                } = s.canvas_mode
                {
                    if let Some(node) = s.graph.get_node(from_node) {
                        let from_world = port_world_pos_for(node, from_port);
                        let z = s.graph.viewport.zoom.max(0.01);
                        let cursor_world = WorldPos::new(
                            from_world.x + (d.dx / z),
                            from_world.y + (d.dy / z),
                        );
                        s.canvas_mode = CanvasMode::DraggingWire {
                            from_node: from_node.clone(),
                            from_port: from_port.clone(),
                            current_cursor: cursor_world,
                        };
                    }
                }
            }
            Phase::Up => {
                if let CanvasMode::DraggingWire {
                    ref from_node,
                    current_cursor,
                    ..
                } = s.canvas_mode
                {
                    let from_id = from_node.clone();
                    let mut target_id = None;
                    for other in s.graph.nodes.values() {
                        if other.id != from_id {
                            let (ow, oh) = node_base_size(other.kind);
                            let in_pos = WorldPos::new(other.pos.x, other.pos.y + oh * 0.5);
                            if current_cursor.distance_to(&in_pos) < 65.0
                                || (current_cursor.x >= other.pos.x - 25.0
                                    && current_cursor.x <= other.pos.x + ow + 25.0
                                    && current_cursor.y >= other.pos.y - 25.0
                                    && current_cursor.y <= other.pos.y + oh + 25.0)
                            {
                                target_id = Some(other.id.clone());
                                break;
                            }
                        }
                    }
                    if let Some(tid) = target_id {
                        s.connect_nodes(&from_id, &tid);
                    }
                }
                s.canvas_mode = CanvasMode::Idle;
            }
        });

    // Drag Handle = Header ONLY (leaves card body clicks/inputs completely unblocked!)
    let header_bar = row()
        .key(format!("node_header_{}", node.id.as_str()))
        .w_full()
        .px(10.0 * zoom)
        .py(6.5 * zoom)
        .gap(6.0 * zoom)
        .center_y()
        .fill(theme::bg_card_header())
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
        .cursor(Cursor::Grab)
        .pointer_buttons([PointerButton::Primary, PointerButton::Middle])
        .on_drag(move |s: &mut AppState, d, tx| match d.phase {
            Phase::Down => {
                if tx.button() == PointerButton::Middle || s.space_held {
                    s.canvas_mode = CanvasMode::Panning {
                        start_screen: ScreenPos::new(d.x, d.y),
                        start_offset: s.graph.viewport.offset,
                    };
                    return;
                }
                s.select_node(node_id_for_drag.clone(), false);
                if let Some(n) = s.graph.get_node(&node_id_for_drag) {
                    s.canvas_mode = CanvasMode::DraggingNode {
                        node_id: node_id_for_drag.clone(),
                        start_pos: n.pos,
                    };
                }
            }
            Phase::Move => {
                let z = s.graph.viewport.zoom.max(0.01);
                match &s.canvas_mode {
                    CanvasMode::DraggingNode { node_id, start_pos } => {
                        let node_id = node_id.clone();
                        let start_pos = *start_pos;
                        if let Some(n) = s.graph.get_node_mut(&node_id) {
                            n.pos = WorldPos::new(
                                start_pos.x + (d.dx / z),
                                start_pos.y + (d.dy / z),
                            );
                        }
                    }
                    CanvasMode::Panning { start_offset, .. } => {
                        let start_offset = *start_offset;
                        s.graph.viewport.offset.x = start_offset.x - (d.dx / z);
                        s.graph.viewport.offset.y = start_offset.y - (d.dy / z);
                    }
                    _ => {}
                }
            }
            Phase::Up => {
                if !d.cancelled() && matches!(s.canvas_mode, CanvasMode::DraggingNode { .. }) {
                    if let Some(bundle) = &s.bundle {
                        let _ = bundle.save_graph_atomic(&s.graph);
                    }
                }
                s.canvas_mode = CanvasMode::Idle;
            }
        })
        // Minimal luminous kind dot
        .child(
            el().size((7.0 * zoom).clamp(4.0, 9.0), (7.0 * zoom).clamp(4.0, 9.0))
                .radius(4.0 * zoom)
                .fill(kind_accent),
        )
        // Title (directly editable inline when Note is selected, capped so right-center header stays draggable)
        .child(if is_selected && node.kind == NodeKind::Note {
            el().min_w(40.0 * zoom).max_w(w * 0.55).child(
                text_input(&s.canvas_title_edit, TextStyle::new(title_size, 600.0))
                    .key(format!("note_title_{}", node.id.as_str()))
                    .single_line()
                    .color(theme::text_primary())
                    .caret(kind_accent, kind_accent.alpha(0.25))
                    .on_edit(|s: &mut AppState| &mut s.canvas_title_edit)
                    .on_input_change(|s: &mut AppState, _| {
                        s.sync_from_canvas_edits();
                    }),
            )
        } else {
            el().grow(1.0).min_w(0.0).child(
                text(&node.title)
                    .font(TextStyle::new(title_size, 600.0))
                    .color(theme::text_primary())
                    .truncate(),
            )
        })
        .child(if is_selected && node.kind == NodeKind::Note {
            el().grow(1.0).h_full()
        } else {
            el()
        })
        // Subtle delete × button
        .child(
            el()
                .key(format!("node_del_{}", node.id.as_str()))
                .px((5.0 * zoom).clamp(3.0, 7.0))
                .py((1.5 * zoom).clamp(1.0, 4.0))
                .radius(4.0 * zoom)
                .hover(theme::accent_rose().alpha(0.18))
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.delete_node(&del_node_id);
                })
                .child(
                    text("×")
                        .font(TextStyle::new((12.0 * zoom).clamp(9.0, 15.0), 500.0))
                        .color(theme::text_muted()),
                ),
        );

    // Card Body (dedicated hit-target, zero drag theft, clipped cleanly!)
    let card_body = match node.kind {
        NodeKind::Note => {
            if is_selected {
                let body_min_h = (h - 56.0 * zoom).max(40.0);
                col()
                    .key(format!("node_body_{}", node.id.as_str()))
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .clip_soft(0.0)
                    .px(8.0 * zoom)
                    .py(7.0 * zoom)
                    .child(
                        col()
                            .w_full()
                            .grow(1.0)
                            .min_h(0.0)
                            .radius(6.0 * zoom)
                            .fill(theme::bg_input())
                            .border(1.0, theme::border_default())
                            .px(7.0 * zoom)
                            .py(6.0 * zoom)
                            .scroll(ScrollSpec { stick_bottom: false })
                            .child(
                                text_input(
                                    &s.canvas_content_edit,
                                    TextStyle::new(body_size, 400.0).lh(body_size * 1.42),
                                )
                                .key(format!("note_body_{}", node.id.as_str()))
                                .w_full()
                                .min_h(body_min_h)
                                .color(theme::text_primary())
                                .caret(kind_accent, kind_accent.alpha(0.25))
                                .placeholder("Write your notes...", theme::text_muted())
                                .on_edit(|s: &mut AppState| &mut s.canvas_content_edit)
                                .on_input_change(|s: &mut AppState, _| {
                                    s.sync_from_canvas_edits();
                                })
                                .on_submit(|s: &mut AppState, _| {
                                    s.canvas_content_edit.insert("\n");
                                    s.sync_from_canvas_edits();
                                }),
                            ),
                    )
            } else {
                let is_empty = node.content.trim().is_empty();
                let preview = if is_empty {
                    "Click to write notes..."
                } else {
                    node.content.trim()
                };
                col()
                    .key(format!("node_body_{}", node.id.as_str()))
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .clip_soft(0.0)
                    .px(10.0 * zoom)
                    .py(8.0 * zoom)
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Text)
                    .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                        s.select_node(note_click_id.clone(), false);
                        tx.focus_when_shown(format!("note_body_{}", note_click_id.as_str()), false);
                    })
                    .child(
                        text(preview)
                            .font(TextStyle::new(body_size, 400.0).lh(body_size * 1.42))
                            .color(if is_empty {
                                theme::text_muted()
                            } else {
                                theme::text_secondary()
                            })
                            .w_full()
                            .wrap(),
                    )
            }
        }
        NodeKind::Document => {
            let word_count = node.content.split_whitespace().count();
            col()
                .key(format!("node_body_{}", node.id.as_str()))
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .clip_soft(0.0)
                .px(10.0 * zoom)
                .py(8.0 * zoom)
                .justify_between()
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.select_node(doc_click_id.clone(), false);
                    s.inspector_open = true;
                    s.ai_panel_open = false;
                    s.status_message = "Opened Document Reader".into();
                })
                .child(
                    row()
                        .center_y()
                        .gap(6.0 * zoom)
                        .child(
                            el()
                                .px(6.0 * zoom)
                                .py(2.0 * zoom)
                                .radius(4.0 * zoom)
                                .fill(theme::accent_blue().alpha(0.14))
                                .border(1.0, theme::accent_blue().alpha(0.30))
                                .child(
                                    text("DOC")
                                        .font(TextStyle::new(badge_size, 700.0))
                                        .color(theme::accent_blue()),
                                ),
                        )
                        .child(
                            text(format!("{} words", word_count))
                                .font(TextStyle::new(badge_size, 500.0))
                                .color(theme::text_muted()),
                        ),
                )
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .justify_between()
                        .child(
                            text("Open reader")
                                .font(TextStyle::new(body_size * 0.9, 500.0))
                                .color(theme::accent_blue()),
                        )
                        .child(
                            text("→")
                                .font(TextStyle::new(body_size * 0.9, 600.0))
                                .color(theme::accent_blue()),
                        ),
                )
        }
        _ => {
            let preview_text = if node.content.trim().is_empty() {
                match node.kind {
                    NodeKind::Image => "Visual image asset.",
                    NodeKind::LlmSynthesis => "AI synthesis insight node.",
                    NodeKind::Concept => "High-level concept anchor.",
                    _ => "",
                }
            } else {
                node.content.trim()
            };
            col()
                .key(format!("node_body_{}", node.id.as_str()))
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .clip_soft(0.0)
                .px(10.0 * zoom)
                .py(8.0 * zoom)
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.select_node(other_click_id.clone(), false);
                    s.inspector_open = true;
                    s.ai_panel_open = false;
                })
                .child(
                    text(preview_text)
                        .font(TextStyle::new(body_size, 400.0).lh(body_size * 1.4))
                        .color(theme::text_secondary())
                        .w_full()
                        .wrap(),
                )
        }
    };

    // Assemble card shell (no outer drag/press handlers!)
    col()
        .key(node.id.as_str())
        .size(w, h)
        .absolute(screen_pos.x, screen_pos.y)
        .radius(card_radius)
        .continuous_corners()
        .fill(card_bg)
        .border(if is_selected || is_snap_target { 1.5 } else { 1.0 }, border_color)
        .shadow(6.0, 18.0, Color::BLACK.alpha(0.45))
        .child(
            col()
                .w_full()
                .h_full()
                .clip_rounded(card_radius)
                .continuous_corners()
                .child(header_bar)
                .child(card_body),
        )
        .child(in_socket)
        .child(out_socket)
}

fn render_edge_pill(edge: &Edge, is_selected: bool, screen_pos: ScreenPos, zoom: f32) -> El {
    let edge_id_clone = edge.id.clone();
    let edge_id_for_accept = edge.id.clone();
    let edge_id_for_reject = edge.id.clone();

    let is_ghost = edge.status == EdgeStatus::Proposed;
    let pill_bg = if is_ghost {
        theme::accent_violet().alpha(0.16)
    } else if is_selected {
        theme::accent_blue().alpha(0.16)
    } else {
        theme::bg_chrome()
    };

    let pill_border = if is_ghost {
        theme::accent_violet().alpha(0.55)
    } else if is_selected {
        theme::accent_blue().alpha(0.75)
    } else {
        theme::border_default()
    };

    let font_size = (10.5 * zoom).clamp(8.0, 12.5);
    let label_str = if edge.label.is_empty() {
        format!("[{}]", edge.relation.display_label())
    } else {
        format!("[{}] {}", edge.relation.display_label(), edge.label)
    };

    let mut content = row()
        .center_y()
        .gap(4.0 * zoom)
        .child(
            text(label_str)
                .font(TextStyle::new(font_size, 500.0))
                .color(if is_ghost {
                    theme::accent_violet()
                } else {
                    theme::text_secondary()
                }),
        );

    if is_ghost {
        content = content
            .child(
                el()
                    .px(5.0 * zoom)
                    .py(1.5 * zoom)
                    .radius(4.0)
                    .fill(theme::accent_emerald().alpha(0.18))
                    .border(1.0, theme::accent_emerald().alpha(0.45))
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.graph.accept_edge(&edge_id_for_accept);
                        if let Some(bundle) = &s.bundle {
                            let _ = bundle.save_graph_atomic(&s.graph);
                        }
                        s.status_message = "Accepted AI link".into();
                    })
                    .child(
                        text("✓")
                            .font(TextStyle::new(font_size, 700.0))
                            .color(theme::accent_emerald()),
                    ),
            )
            .child(
                el()
                    .px(5.0 * zoom)
                    .py(1.5 * zoom)
                    .radius(4.0)
                    .fill(theme::accent_rose().alpha(0.16))
                    .border(1.0, theme::accent_rose().alpha(0.40))
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.graph.reject_edge(&edge_id_for_reject);
                        if let Some(bundle) = &s.bundle {
                            let _ = bundle.save_graph_atomic(&s.graph);
                        }
                        s.status_message = "Dismissed AI link".into();
                    })
                    .child(
                        text("×")
                            .font(TextStyle::new(font_size, 700.0))
                            .color(theme::accent_rose()),
                    ),
            );
    }

    el()
        .key(format!("edge_pill_{}", edge.id.as_str()))
        .absolute(screen_pos.x - 68.0 * zoom, screen_pos.y - 12.0 * zoom)
        .px(8.0 * zoom)
        .py(3.5 * zoom)
        .radius(10.0)
        .fill(pill_bg)
        .hover(theme::hover_subtle())
        .border(1.0, pill_border)
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut AppState| {
            s.select_edge(edge_id_clone.clone());
        })
        .child(content)
}
