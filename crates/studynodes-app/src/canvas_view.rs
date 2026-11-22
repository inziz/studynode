use rune::{
    Color, Cursor, Cx, El, FileDialogOptions, LineCap, Phase, PointerButton, ScrollSpec, Shape,
    Spring, SurfaceFit, TextStyle, Tx, canvas, col, el, row, text, text_input,
};

use studynodes_graph::bezier::CubicBezier;
use studynodes_graph::coords::{Camera, ScreenPos, WorldPos};
use studynodes_graph::model::{
    CalendarViewMode, DrawingStroke, Edge, EdgeStatus, Node, NodeKind, PortDirection, PortId,
};

use crate::doc_reader::{self, DocFormat, DockPosition};
use crate::formula_math;
use crate::i18n::{self, TextKey};
use crate::media_helper;
use crate::state::{AppState, CanvasMode, node_world_size};
use crate::theme;

const LOW_ZOOM_TITLE_ONLY: f32 = 0.48;
const CULL_MARGIN: f32 = 560.0;

#[inline]
fn is_rect_in_viewport(x: f32, y: f32, w: f32, h: f32, canvas_size: [f32; 2]) -> bool {
    let cw = canvas_size[0].max(240.0);
    let ch = canvas_size[1].max(200.0);
    x + w >= -CULL_MARGIN
        && x <= cw + CULL_MARGIN
        && y + h >= -CULL_MARGIN
        && y <= ch + CULL_MARGIN
}

fn is_node_active_in_interaction(
    s: &AppState,
    node_id: &studynodes_graph::model::NodeId,
) -> bool {
    if s.editing_node_id.as_ref() == Some(node_id) {
        return true;
    }
    match &s.canvas_mode {
        CanvasMode::DraggingNode { node_id: id, .. }
        | CanvasMode::ResizingSection { node_id: id, .. }
        | CanvasMode::ResizingNode { node_id: id, .. }
        | CanvasMode::DraggingWire { from_node: id, .. } => id == node_id,
        CanvasMode::DraggingSection {
            node_id: id,
            children,
            ..
        } => id == node_id || children.iter().any(|(cid, _)| cid == node_id),
        _ => false,
    }
}

fn port_world_pos_for(node: &Node, port_id: &PortId) -> WorldPos {
    let (w, h) = node_world_size(node);
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

fn build_grid_shape(s: &AppState, cam: &Camera) -> Shape {
    let zoom = cam.zoom;
    let mut grid_shape = Shape::new();

    let base_spacing = 48.0 * zoom;
    let major_spacing = base_spacing * 4.0;
    let draw_grid = if base_spacing >= 20.0 {
        Some((base_spacing, 48.0, true))
    } else if major_spacing >= 12.0 {
        Some((major_spacing, 192.0, false))
    } else {
        None
    };

    if let Some((step, world_step, with_minor)) = draw_grid {
        let minor_color = theme::grid_minor();
        let major_color = theme::grid_major();
        let pad = (step * 12.0).clamp(560.0, 960.0);
        let min_x = -pad;
        let min_y = -pad;
        let max_x = s.canvas_size[0].max(400.0) + pad;
        let max_y = s.canvas_size[1].max(300.0) + pad;

        let world_left_idx = ((cam.offset.x - pad / zoom) / world_step).floor() as i32;
        let mut x = (world_left_idx as f32 * world_step - cam.offset.x) * zoom;
        let mut ix = world_left_idx;
        while x < max_x {
            let is_major = !with_minor || ix.rem_euclid(4) == 0;
            let (col, w) = if is_major { (major_color, 0.75) } else { (minor_color, 0.5) };
            grid_shape = grid_shape.line([x, min_y], [x, max_y], w, LineCap::Butt, col);
            x += step;
            ix += 1;
        }

        let world_top_idx = ((cam.offset.y - pad / zoom) / world_step).floor() as i32;
        let mut y = (world_top_idx as f32 * world_step - cam.offset.y) * zoom;
        let mut iy = world_top_idx;
        while y < max_y {
            let is_major = !with_minor || iy.rem_euclid(4) == 0;
            let (col, w) = if is_major { (major_color, 0.75) } else { (minor_color, 0.5) };
            grid_shape = grid_shape.line([min_x, y], [max_x, y], w, LineCap::Butt, col);
            y += step;
            iy += 1;
        }
    }

    grid_shape
}

pub(crate) fn draw_arrow_triangle(
    shape: Shape,
    tip: [f32; 2],
    dir: (f32, f32),
    len: f32,
    half_w: f32,
    color: Color,
) -> Shape {
    let (dx, dy) = dir;
    let px = -dy;
    let py = dx;
    let bx = tip[0] - dx * len;
    let by = tip[1] - dy * len;
    shape.fill(
        vec![
            tip,
            [bx + px * half_w, by + py * half_w],
            [bx - px * half_w, by - py * half_w],
        ],
        color,
    )
}

pub fn build_drawing_shape(strokes: &[DrawingStroke], w: f32, h: f32, scale: f32) -> Shape {
    let mut shape = Shape::new();
    for stroke in strokes {
        if stroke.points.is_empty() {
            continue;
        }
        let color = if !theme::active_theme().is_dark() && stroke.color_hex.eq_ignore_ascii_case("#F4F3EE") {
            theme::text_primary()
        } else if stroke.color_hex.trim().starts_with('#') {
            theme::hex(stroke.color_hex.trim())
        } else {
            theme::text_primary()
        };
        let width = (stroke.width * scale).clamp(1.0, 14.0);
        if stroke.points.len() == 1 {
            let x = stroke.points[0][0] * w;
            let y = stroke.points[0][1] * h;
            shape = shape.stroke(vec![[x - 0.6, y], [x + 0.6, y]], width, color);
        } else {
            let pts: Vec<[f32; 2]> = stroke
                .points
                .iter()
                .map(|p| [p[0] * w, p[1] * h])
                .collect();
            shape = shape.stroke(pts, width, color);
        }
    }
    shape
}

fn build_wire_shape(s: &AppState, cam: &Camera) -> Shape {
    let zoom = cam.zoom;
    let mut wire_shape = Shape::new();
    let socket_r = (4.5 * zoom).clamp(2.2, 8.0);
    let arrow_len = (8.5 * zoom).clamp(4.0, 13.0);
    let arrow_hw = (4.0 * zoom).clamp(2.0, 6.2);
    let animate_wires = s.reader_prefs.animate_wires && zoom >= 0.35;
    let curve_pad = 140.0 * zoom;

    for edge in s.graph.edges.values() {
        if let (Some(from_node), Some(to_node)) = (
            s.graph.get_node(&edge.from.node),
            s.graph.get_node(&edge.to.node),
        ) {
            let from_world = port_world_pos_for(from_node, &edge.from.port);
            let to_world = port_world_pos_for(to_node, &edge.to.port);
            let from_sp = cam.world_to_screen(from_world);
            let to_sp = cam.world_to_screen(to_world);

            let min_x = from_sp.x.min(to_sp.x) - curve_pad;
            let min_y = from_sp.y.min(to_sp.y) - curve_pad;
            let max_x = from_sp.x.max(to_sp.x) + curve_pad;
            let max_y = from_sp.y.max(to_sp.y) + curve_pad;
            if !is_rect_in_viewport(min_x, min_y, max_x - min_x, max_y - min_y, s.canvas_size) {
                continue;
            }

            let chord = (to_sp.x - from_sp.x).hypot(to_sp.y - from_sp.y);
            let seg_count = if zoom < 0.55 || chord < 160.0 { 12 } else { 24 };
            let curve = CubicBezier::wire(from_world, to_world);
            let segments: Vec<[f32; 2]> = curve
                .sample_segments(seg_count)
                .into_iter()
                .map(|wp| {
                    let sp = cam.world_to_screen(wp);
                    [sp.x, sp.y]
                })
                .collect();

            let is_selected = s.selected_edge.as_ref() == Some(&edge.id);
            let is_ghost = edge.status == EdgeStatus::Proposed;

            let (wire_color, wire_w) = if is_selected {
                (theme::wire_selected(), 2.2 * zoom.clamp(0.7, 1.4))
            } else if is_ghost {
                (theme::wire_proposed().alpha(0.82), 2.0 * zoom.clamp(0.7, 1.4))
            } else {
                (theme::wire_default(), 1.6 * zoom.clamp(0.7, 1.4))
            };

            wire_shape = wire_shape.stroke(segments, wire_w, wire_color);

            // Terminal directional arrowhead just outside destination input socket
            let end_dir = curve.tangent(0.98);
            let end_tip = [
                to_sp.x - end_dir.0 * (socket_r + 2.0),
                to_sp.y - end_dir.1 * (socket_r + 2.0),
            ];
            wire_shape = draw_arrow_triangle(
                wire_shape,
                end_tip,
                end_dir,
                arrow_len,
                arrow_hw,
                wire_color,
            );

            if animate_wires {
                // Traveling directional pulse + chevron along the Bézier flow
                let t_anim = 0.08 + s.wire_anim_phase.clamp(0.0, 1.0) * 0.84;
                let alpha_env = (t_anim * 6.0).min((1.0 - t_anim) * 6.0).clamp(0.0, 1.0);
                let pulse_color = if is_selected || is_ghost {
                    theme::accent_claude().alpha(0.92 * alpha_env)
                } else {
                    theme::text_primary().alpha(0.82 * alpha_env)
                };

                let t_tail = (t_anim - 0.07).max(0.02);
                let trail: Vec<[f32; 2]> = (0..=4)
                    .map(|step| {
                        let frac = step as f32 / 4.0;
                        let t = t_tail + (t_anim - t_tail) * frac;
                        let sp = cam.world_to_screen(curve.eval(t));
                        [sp.x, sp.y]
                    })
                    .collect();
                wire_shape =
                    wire_shape.stroke(trail, wire_w * 1.45, pulse_color.alpha(0.55 * alpha_env));

                let pulse_sp = cam.world_to_screen(curve.eval(t_anim));
                let pulse_dir = curve.tangent(t_anim);
                wire_shape = draw_arrow_triangle(
                    wire_shape,
                    [pulse_sp.x, pulse_sp.y],
                    pulse_dir,
                    (8.0 * zoom).clamp(5.0, 12.0),
                    (3.6 * zoom).clamp(2.4, 5.5),
                    pulse_color,
                );
            }
        }
    }

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

            wire_shape = wire_shape.stroke(
                segments,
                2.2 * zoom.clamp(0.7, 1.4),
                theme::accent_claude(),
            );
            let end_sp = cam.world_to_screen(*current_cursor);
            let end_dir = curve.tangent(0.98);
            wire_shape = draw_arrow_triangle(
                wire_shape,
                [end_sp.x, end_sp.y],
                end_dir,
                arrow_len,
                arrow_hw,
                theme::accent_claude(),
            );
        }
    }

    wire_shape
}

fn apply_pan_drag_step(s: &mut AppState, start_offset: WorldPos, dx: f32, dy: f32, tx: &mut Tx) {
    let z = s.graph.viewport.zoom.max(0.01);
    s.graph.viewport.offset.x = start_offset.x - (dx / z);
    s.graph.viewport.offset.y = start_offset.y - (dy / z);
    if s.smooth_zoom_enabled {
        s.sync_live_camera();
        if !s.should_commit_live_camera() {
            tx.live_only();
        } else {
            s.flush_live_camera();
            tx.instant();
        }
    } else {
        s.committed_viewport = s.graph.viewport;
        tx.instant();
    }
}

pub fn render_canvas_view(s: &AppState) -> El {
    let cam = if s.smooth_zoom_enabled {
        s.committed_viewport
    } else {
        s.graph.viewport
    };
    let zoom = cam.zoom;
    let grid_shape = build_grid_shape(s, &cam);
    let wire_shape = build_wire_shape(s, &cam);

    // 2. Interactive Canvas Container (grow(1.0).min_w(0.0) so right-side Reader fits on screen!)
    let mut canvas_container = el()
        .key("graph_canvas")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .clip_soft(0.0)
        .fill(theme::bg_canvas())
        .motion(Spring::NONE)
        .cursor(if s.space_held { Cursor::Grab } else { Cursor::Default })
        .pointer_buttons([PointerButton::Primary, PointerButton::Middle])
        .on_layout_tx(|s: &mut AppState, r, tx| {
            let new_origin = [r[0], r[1]];
            let new_size = [r[2].max(200.0), r[3].max(200.0)];
            let changed = (s.canvas_origin[0] - new_origin[0]).abs() > 0.5
                || (s.canvas_origin[1] - new_origin[1]).abs() > 0.5
                || (s.canvas_size[0] - new_size[0]).abs() > 0.5
                || (s.canvas_size[1] - new_size[1]).abs() > 0.5;
            s.canvas_origin = new_origin;
            s.canvas_size = new_size;
            crate::touchpad_win32::set_canvas_bounds(Some(r));
            if changed {
                tx.instant();
            } else {
                tx.live_only();
            }
        });

    let mut world_stage = el()
        .key("world_stage")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .live_offset(&s.cam_live_pan)
        .live_scale(&s.cam_live_zoom)
        .child(
            canvas(grid_shape)
                .key("canvas_grid_layer")
                .absolute(0.0, 0.0)
                .w_full()
                .h_full(),
        );

    canvas_container = canvas_container
        .on_wheel(|s: &mut AppState, e, tx| {
            if e.mods.ctrl || e.mods.cmd {
                let factor = (e.dy * 0.0014).clamp(-0.28, 0.28).exp();
                s.queue_zoom_at(ScreenPos::new(e.x, e.y), factor, 22.0, Some(tx));
                return true;
            } else if e.mods.shift && e.dx == 0.0 {
                s.pan_live_by(-e.dy, 0.0, Some(tx));
            } else {
                s.pan_live_by(-e.dx, -e.dy, Some(tx));
            }
            true
        })
        .on_magnify(|s: &mut AppState, e, tx| {
            // Windows Ctrl+wheel pinch sends dy>0 for expand, which Rune's pinch_wheel negates.
            let scaled = if cfg!(target_os = "windows") {
                (-e.delta * 0.45).clamp(-0.32, 0.32)
            } else {
                e.delta.clamp(-0.32, 0.32)
            };
            let factor = scaled.exp();
            s.queue_zoom_at(ScreenPos::new(e.x, e.y), factor, 28.0, Some(tx));
            true
        })
        .on_drop_files(|s: &mut AppState, files, _| {
            s.handle_dropped_files(files, ScreenPos::new(400.0, 300.0));
        })
        .on_drag(|s: &mut AppState, d, tx| match d.phase {
            Phase::Down => {
                if s.smooth_zoom_enabled {
                    s.flush_live_camera();
                }
                s.canvas_mode = CanvasMode::Panning {
                    start_screen: ScreenPos::new(d.x, d.y),
                    start_offset: s.graph.viewport.offset,
                };
            }
            Phase::Move => {
                if let CanvasMode::Panning { start_offset, .. } = s.canvas_mode {
                    apply_pan_drag_step(s, start_offset, d.dx, d.dy, tx);
                }
            }
            Phase::Up => {
                if !d.cancelled()
                    && !s.space_held
                    && tx.button() == PointerButton::Primary
                    && d.dx.hypot(d.dy) < 4.0
                {
                    let click_world = s.graph.viewport.screen_to_world(ScreenPos::new(d.x, d.y));
                    let threshold = 12.0 / s.graph.viewport.zoom.max(0.25);
                    let mut hit_edge = None;
                    for edge in s.graph.edges.values() {
                        if let (Some(fn_node), Some(tn_node)) = (
                            s.graph.get_node(&edge.from.node),
                            s.graph.get_node(&edge.to.node),
                        ) {
                            let fw = port_world_pos_for(fn_node, &edge.from.port);
                            let tw = port_world_pos_for(tn_node, &edge.to.port);
                            if CubicBezier::wire(fw, tw).hit_test(click_world, threshold) {
                                hit_edge = Some(edge.id.clone());
                                break;
                            }
                        }
                    }
                    if let Some(eid) = hit_edge {
                        s.select_edge(eid);
                        tx.blur();
                    } else {
                        s.deselect_all();
                        tx.blur();
                    }
                }
                s.flush_live_camera();
                s.canvas_mode = CanvasMode::Idle;
            }
        });

    // 2. Section Title Banners & Group Frames (always behind wires and regular cards)
    let mut section_nodes: Vec<&Node> = s
        .graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Section)
        .collect();
    section_nodes.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    for sec in section_nodes {
        let sp = cam.world_to_screen(sec.pos);
        let (bw, bh) = node_world_size(sec);
        if !is_rect_in_viewport(sp.x, sp.y, bw * zoom, bh * zoom, s.canvas_size)
            && !is_node_active_in_interaction(s, &sec.id)
        {
            continue;
        }
        world_stage = world_stage.child(render_section_node(s, sec, sp, zoom));
    }

    // 2b. Bezier Wires Layer (above Section backdrops, behind regular Study cards)
    world_stage = world_stage.child(
        canvas(wire_shape)
            .key("canvas_wire_layer")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full(),
    );

    // 3. Regular Node Cards (deterministic z-order: unselected sorted by id, selected on top)
    let mut ordered_nodes: Vec<&Node> = s
        .graph
        .nodes
        .values()
        .filter(|n| n.kind != NodeKind::Section)
        .collect();
    ordered_nodes.sort_by(|a, b| {
        s.selected_nodes
            .contains(&a.id)
            .cmp(&s.selected_nodes.contains(&b.id))
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });

    for node in ordered_nodes {
        let sp = cam.world_to_screen(node.pos);
        let (bw, bh) = node_world_size(node);
        if !is_rect_in_viewport(sp.x, sp.y, bw * zoom, bh * zoom, s.canvas_size)
            && !is_node_active_in_interaction(s, &node.id)
        {
            continue;
        }
        let card = render_node_card(s, node, sp, zoom);
        world_stage = world_stage.child(card);
    }

    // 4. Edge Annotation Pills (or compact midpoint click dot when label is hidden)
    if zoom >= 0.45 {
        let mut edges: Vec<&Edge> = s.graph.edges.values().collect();
        edges.sort_by(|a, b| {
            let a_sel = s.selected_edge.as_ref() == Some(&a.id);
            let b_sel = s.selected_edge.as_ref() == Some(&b.id);
            a_sel.cmp(&b_sel).then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        for edge in edges {
            if let (Some(from_node), Some(to_node)) = (
                s.graph.get_node(&edge.from.node),
                s.graph.get_node(&edge.to.node),
            ) {
                let from_world = port_world_pos_for(from_node, &edge.from.port);
                let to_world = port_world_pos_for(to_node, &edge.to.port);
                let curve = CubicBezier::wire(from_world, to_world);
                let mid_screen = cam.world_to_screen(curve.midpoint());
                if !is_rect_in_viewport(
                    mid_screen.x - 80.0,
                    mid_screen.y - 24.0,
                    160.0,
                    48.0,
                    s.canvas_size,
                ) {
                    continue;
                }
                let is_sel = s.selected_edge.as_ref() == Some(&edge.id);
                let label_hidden =
                    (edge.hide_label || s.hide_all_edge_labels) && edge.status != EdgeStatus::Proposed;
                if label_hidden {
                    let dot_edge_id = edge.id.clone();
                    world_stage = world_stage.child(
                        el()
                            .key(format!("edge_pill_{}", edge.id.as_str()))
                            .absolute(mid_screen.x - 6.0, mid_screen.y - 6.0)
                            .size(12.0, 12.0)
                            .radius(6.0)
                            .hover(Color::WHITE.alpha(0.12))
                            .center()
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.select_edge(dot_edge_id.clone());
                            })
                            .child(
                                el()
                                    .size(5.0, 5.0)
                                    .radius(2.5)
                                    .fill(if is_sel {
                                        theme::accent_claude()
                                    } else {
                                        theme::border_strong()
                                    }),
                            ),
                    );
                } else {
                    world_stage = world_stage.child(render_edge_pill(
                        edge,
                        is_sel,
                        mid_screen,
                        zoom,
                    ));
                }
            }
        }
    }

    canvas_container = canvas_container.child(world_stage);

    let dock_pos = s.reader_prefs.dock_position;
    let (dock_x, dock_y, dock_w, dock_h, hud_x, hud_y) =
        floating_overlay_coords(s.canvas_size, dock_pos, s.dock_measured_size);

    if s.show_block_menu {
        canvas_container = canvas_container.child(render_block_menu_popover(
            s, dock_pos, dock_x, dock_y, dock_w, dock_h,
        ));
    }
    if s.show_ai_menu {
        canvas_container = canvas_container.child(render_ai_menu_popover(
            s, dock_pos, dock_x, dock_y, dock_w, dock_h,
        ));
    }
    if let Some(msg) = &s.toast_text {
        canvas_container = canvas_container.child(render_canvas_toast(
            s.canvas_size,
            dock_pos,
            dock_y,
            dock_h,
            msg,
        ));
    }

    canvas_container = canvas_container
        .child(render_canvas_floating_dock(s, dock_x, dock_y))
        .child(render_canvas_viewport_hud(hud_x, hud_y, zoom));

    canvas_container
}

pub(crate) fn floating_overlay_coords(
    canvas_size: [f32; 2],
    dock_pos: DockPosition,
    measured_dock: Option<[f32; 2]>,
) -> (f32, f32, f32, f32, f32, f32) {
    let cw = canvas_size[0].max(240.0);
    let ch = canvas_size[1].max(200.0);
    let hud_w = 136.0;
    let hud_h = 34.0;
    let margin = 16.0;

    let mut hud_x = (cw - hud_w - margin).max(margin);
    let hud_y = (ch - hud_h - (margin + 3.0)).max(12.0);

    let (default_w, default_h) = match dock_pos {
        DockPosition::Bottom | DockPosition::Top => (388.0, 40.0),
        DockPosition::Left | DockPosition::Right => (92.0, 240.0),
    };
    let (dock_w, dock_h) = measured_dock
        .filter(|m| m[0] >= 40.0 && m[1] >= 30.0)
        .map(|m| (m[0], m[1]))
        .unwrap_or((default_w, default_h));

    let (dock_x, dock_y) = match dock_pos {
        DockPosition::Bottom => {
            let max_dock_x = (hud_x - 12.0 - dock_w).max(margin);
            let x = ((cw - dock_w) * 0.5).clamp(margin, max_dock_x);
            let y = (ch - dock_h - margin).max(12.0);
            (x, y)
        }
        DockPosition::Top => {
            let x = ((cw - dock_w) * 0.5).clamp(margin, (cw - dock_w - margin).max(margin));
            (x, margin)
        }
        DockPosition::Left => {
            let y = ((ch - dock_h) * 0.5).clamp(margin, (ch - dock_h - margin).max(margin));
            (margin, y)
        }
        DockPosition::Right => {
            let x = (cw - dock_w - margin).max(margin);
            let y = ((ch - dock_h) * 0.5).clamp(margin, (ch - dock_h - margin).max(margin));
            if y + dock_h + 12.0 >= hud_y {
                hud_x = (x - hud_w - 12.0).max(margin);
            }
            (x, y)
        }
    };

    (dock_x, dock_y, dock_w, dock_h, hud_x, hud_y)
}

fn dock_ghost_btn_dir(
    key: &'static str,
    glyph: Option<(&'static str, Color)>,
    label: &str,
    active: bool,
    full_w: bool,
) -> El {
    let mut content = row().center_y().gap(5.0);
    if let Some((g, c)) = glyph {
        content = content.child(text(g).font(theme::CONTROL_TEXT).color(c));
    }
    content = content.child(
        text(label)
            .font(theme::STATUS_TEXT)
            .color(if active {
                theme::text_primary()
            } else {
                theme::text_secondary()
            }),
    );

    let mut btn = el()
        .key(key)
        .h(28.0)
        .px(10.0)
        .radius(8.0)
        .fill(if active {
            theme::hover_subtle()
        } else {
            Color::TRANSPARENT
        })
        .hover(theme::hover_subtle())
        .press(theme::press_subtle())
        .cursor(Cursor::Pointer)
        .no_window_drag();
    btn = if full_w {
        btn.w_full().center_y().child(content)
    } else {
        btn.center().child(content)
    };
    btn
}

pub fn render_workspace_topbar(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let insets = cx.chrome_insets();

    let mut tabs_strip = row()
        .key("workspace_tabs_strip")
        .gap(2.0)
        .center_y()
        .shrink()
        .min_w(0.0)
        .no_window_drag();

    if s.tabs.is_empty() {
        tabs_strip = tabs_strip.child(
            el().shrink().min_w(0.0).px(6.0).child(
                text(&s.graph.title)
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_primary())
                    .truncate(),
            ),
        );
    } else {
        for (idx, tab) in s.tabs.iter().enumerate() {
            let is_active = s.active_tab == Some(idx);
            let title_text = if is_active {
                &s.graph.title
            } else {
                &tab.title
            };

            tabs_strip = tabs_strip.child(
                row()
                    .key(format!("workspace_tab_shell_{}", idx))
                    .h(35.0)
                    .max_w(168.0)
                    .border_sides(
                        [0.0, 0.0, 1.5, 0.0],
                        if is_active {
                            theme::accent_claude()
                        } else {
                            Color::TRANSPARENT
                        },
                    )
                    .hover(theme::hover_subtle())
                    .center_y()
                    .child(
                        row()
                            .key(format!("workspace_tab_{}", idx))
                            .grow(1.0)
                            .min_w(0.0)
                            .h_full()
                            .pl(10.0)
                            .pr(4.0)
                            .gap(6.0)
                            .center_y()
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.switch_tab(idx);
                            })
                            .child(
                                text("●")
                                    .font(TextStyle::new(6.5, 600.0).mono())
                                    .color(if is_active {
                                        theme::accent_claude()
                                    } else {
                                        Color::TRANSPARENT
                                    }),
                            )
                            .child(
                                text(title_text)
                                    .font(theme::STATUS_TEXT)
                                    .color(if is_active {
                                        theme::text_primary()
                                    } else {
                                        theme::text_muted()
                                    })
                                    .truncate(),
                            ),
                    )
                    .child(
                        el()
                            .key(format!("workspace_tab_close_{}", idx))
                            .size(20.0, 22.0)
                            .radius(4.0)
                            .center()
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.close_tab(idx);
                            })
                            .child(
                                text("×")
                                    .font(TextStyle::new(11.0, 500.0).mono())
                                    .color(if is_active {
                                        theme::text_secondary()
                                    } else {
                                        theme::text_muted().alpha(0.6)
                                    }),
                            ),
                    ),
            );
        }

        tabs_strip = tabs_strip.child(
            el()
                .key("btn_new_tab")
                .size(26.0, 26.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.close_to_dashboard();
                })
                .child(
                    text("+")
                        .font(theme::CONTROL_TEXT)
                        .color(theme::text_muted()),
                ),
        );
    }

    let mut topbar = row()
        .key("workspace_topbar")
        .w_full()
        .h(36.0)
        .pl(insets.leading + 8.0)
        .pr(insets.trailing + 10.0)
        .gap(6.0)
        .center_y()
        .fill(theme::bg_canvas())
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
        .window_drag()
        .child(
            el()
                .key("btn_back")
                .size(26.0, 26.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press(|s: &mut AppState| {
                    s.close_to_dashboard();
                })
                .child(
                    text("←")
                        .font(theme::CONTROL_TEXT)
                        .color(theme::text_muted()),
                ),
        )
        .child(tabs_strip)
        .child(el().grow(1.0));

    if s.inspector_popped_out {
        topbar = topbar.child(
            el()
                .key("btn_focus_popout")
                .h(24.0)
                .px(8.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press(|s: &mut AppState| {
                    if let Some(h) = &s.popout_window {
                        h.focus();
                    } else {
                        s.pop_out_reader();
                    }
                })
                .child(
                    text(i18n::t(TextKey::TopbarReaderChip))
                        .font(theme::STATUS_TEXT)
                        .color(theme::accent_claude()),
                ),
        );
    }

    topbar
        .child(
            el()
                .key("btn_workspace_export")
                .size(26.0, 26.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                    let Some(bundle) = &s.bundle else {
                        return;
                    };
                    let project_path = bundle.root_dir.clone();
                    let folder_name = project_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("project")
                        .to_string();
                    let opts = FileDialogOptions {
                        title: i18n::t(TextKey::DashExportBtn).to_string(),
                        file_name: Some(format!("{}.studynodes", folder_name)),
                        ..Default::default()
                    }
                    .filter("StudyNodes Archive", &["studynodes"]);
                    tx.save_file(opts, move |s: &mut AppState, res, _| {
                        if let Ok(Some(dest)) = res {
                            s.export_project_to_archive(&project_path, &dest);
                        }
                    });
                })
                .child(
                    text("↑")
                        .font(theme::CONTROL_TEXT)
                        .color(theme::text_muted()),
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
                .size(26.0, 26.0)
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
                    text("⚙")
                        .font(theme::CONTROL_TEXT)
                        .color(if s.ai_api_configured() {
                            theme::accent_claude()
                        } else {
                            theme::text_muted()
                        }),
                ),
        )
        .child(
            el()
                .key("btn_toggle_panel")
            .size(26.0, 26.0)
            .radius(6.0)
            .hover(theme::hover_subtle())
            .press(theme::press_subtle())
            .center()
            .cursor(Cursor::Pointer)
            .no_window_drag()
            .on_press(|s: &mut AppState| {
                if s.inspector_popped_out {
                    s.dock_reader_back();
                } else {
                    s.inspector_open = !s.inspector_open;
                }
            })
            .child(
                text("◫")
                    .font(theme::CONTROL_TEXT)
                    .color(if s.inspector_open || s.inspector_popped_out {
                        theme::text_primary()
                    } else {
                        theme::text_muted()
                    }),
            ),
    )
}

fn render_canvas_floating_dock(s: &AppState, dock_x: f32, dock_y: f32) -> El {
    let dock_pos = s.reader_prefs.dock_position;
    let vertical = matches!(dock_pos, DockPosition::Left | DockPosition::Right);
    let container = if vertical {
        col()
            .key("canvas_floating_dock")
            .absolute(dock_x, dock_y)
            .padding(8.0)
            .gap(4.0)
    } else {
        row()
            .key("canvas_floating_dock")
            .absolute(dock_x, dock_y)
            .h(40.0)
            .px(8.0)
            .gap(3.0)
            .center_y()
    }
    .on_layout_tx(|s: &mut AppState, r, tx| {
        let measured = [r[2].max(40.0), r[3].max(36.0)];
        let changed = s.dock_measured_size.map_or(true, |prev| {
            (prev[0] - measured[0]).abs() > 0.5 || (prev[1] - measured[1]).abs() > 0.5
        });
        if changed {
            s.dock_measured_size = Some(measured);
            tx.instant();
        } else {
            tx.live_only();
        }
    });

    let sep = || {
        if vertical {
            el().w_full().h(1.0).my(2.0).fill(theme::border_subtle())
        } else {
            el().w(1.0).h(18.0).fill(theme::border_subtle())
        }
    };

    let pos_glyph = match dock_pos {
        DockPosition::Bottom => "↓",
        DockPosition::Top => "↑",
        DockPosition::Left => "←",
        DockPosition::Right => "→",
    };

    container
        .radius(12.0)
        .continuous_corners()
        .fill(theme::bg_chrome().alpha(0.96))
        .border(1.0, theme::border_default())
        .shadow(10.0, 26.0, Color::BLACK.alpha(0.55))
        .child(
            dock_ghost_btn_dir(
                "btn_add_note",
                Some(("▪", theme::text_secondary())),
                i18n::t(TextKey::DockNote),
                false,
                vertical,
            )
            .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                s.show_block_menu = false;
                s.show_ai_menu = false;
                let offset_x = (s.graph.nodes.len() as f32 * 32.0) % 160.0;
                let offset_y = (s.graph.nodes.len() as f32 * 24.0) % 120.0;
                let pos = s
                    .graph
                    .viewport
                    .screen_to_world(ScreenPos::new(200.0 + offset_x, 120.0 + offset_y));
                let id = s.add_note_node(Some(pos));
                tx.focus_when_shown(format!("note_body_{}", id.as_str()), false);
            }),
        )
        .child(
            dock_ghost_btn_dir(
                "btn_add_doc",
                Some(("▤", theme::text_secondary())),
                i18n::t(TextKey::DockDoc),
                false,
                vertical,
            )
            .on_press(|s: &mut AppState| {
                s.show_block_menu = false;
                s.show_ai_menu = false;
                let offset_x = (s.graph.nodes.len() as f32 * 32.0) % 160.0;
                let offset_y = (s.graph.nodes.len() as f32 * 24.0) % 120.0;
                let pos = s
                    .graph
                    .viewport
                    .screen_to_world(ScreenPos::new(240.0 + offset_x, 150.0 + offset_y));
                s.add_document_node(Some(pos));
            }),
        )
        .child(
            dock_ghost_btn_dir(
                "btn_dock_add_image",
                Some(("▣", theme::text_secondary())),
                i18n::t(TextKey::DockImage),
                false,
                vertical,
            )
            .on_press(|s: &mut AppState| {
                s.show_block_menu = false;
                s.show_ai_menu = false;
                s.add_study_node(NodeKind::Image, None);
            }),
        )
        .child(
            dock_ghost_btn_dir(
                "btn_toggle_block_menu",
                None,
                i18n::t(TextKey::DockMore),
                s.show_block_menu,
                vertical,
            )
            .on_press(|s: &mut AppState| {
                s.show_block_menu = !s.show_block_menu;
                s.show_ai_menu = false;
            }),
        )
        .child(sep())
        .child(
            dock_ghost_btn_dir(
                "btn_ai",
                Some(("✦", theme::accent_claude())),
                i18n::t(TextKey::DockAi),
                s.show_ai_menu,
                vertical,
            )
            .on_press(|s: &mut AppState| {
                s.show_ai_menu = !s.show_ai_menu;
                s.show_block_menu = false;
            }),
        )
        .child(
            dock_ghost_btn_dir(
                "btn_ai_open_chat",
                Some(("✦", theme::accent_claude())),
                i18n::t(TextKey::DockChat),
                s.inspector_open && s.ai_panel_open,
                vertical,
            )
            .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                if s.inspector_open && s.ai_panel_open {
                    s.ai_panel_open = false;
                    s.inspector_open = false;
                } else {
                    s.open_ai_chat_panel();
                    tx.focus_when_shown("ai_chat_input", false);
                }
            }),
        )
        .child(sep())
        .child({
            let mut pos_btn = el()
                .key("btn_dock_position")
                .h(28.0)
                .px(8.0)
                .radius(8.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press(|s: &mut AppState| {
                    s.cycle_dock_position();
                });
            if vertical {
                pos_btn = pos_btn.w_full();
            }
            pos_btn.center().child(
                text(format!("◫{}", pos_glyph))
                    .font(theme::BADGE_TEXT)
                    .color(theme::text_secondary()),
            )
        })
}

fn render_canvas_viewport_hud(hud_x: f32, hud_y: f32, zoom: f32) -> El {
    row()
        .key("canvas_viewport_hud")
        .absolute(hud_x, hud_y)
        .w(136.0)
        .h(34.0)
        .px(5.0)
        .gap(2.0)
        .center_y()
        .radius(10.0)
        .continuous_corners()
        .fill(theme::bg_chrome().alpha(0.95))
        .border(1.0, theme::border_subtle())
        .shadow(6.0, 18.0, Color::BLACK.alpha(0.45))
        .child(
            el()
                .key("btn_locate_nearest")
                .size(26.0, 26.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press(|s: &mut AppState| {
                    s.locate_nearest_nodes();
                })
                .child(
                    text("◎")
                        .font(theme::CONTROL_TEXT)
                        .color(theme::accent_claude()),
                ),
        )
        .child(el().w(1.0).h(14.0).fill(theme::border_subtle()))
        .child(
            el()
                .key("btn_zoom_out")
                .size(24.0, 26.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                    let center = ScreenPos::new(s.canvas_size[0] * 0.5, s.canvas_size[1] * 0.5);
                    s.queue_zoom_at(center, 0.85, 18.0, Some(tx));
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
                .grow(1.0)
                .h(26.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press(|s: &mut AppState| {
                    s.cancel_smooth_zoom();
                    s.graph.viewport.zoom = 1.0;
                    s.graph.viewport.offset = WorldPos::ZERO;
                    s.flush_live_camera();
                })
                .child(
                    text(format!(
                        "{}%",
                        (zoom * 100.0).round() as u32
                    ))
                    .font(theme::BADGE_TEXT)
                    .color(theme::text_secondary()),
                ),
        )
        .child(
            el()
                .key("btn_zoom_in")
                .size(24.0, 26.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                    let center = ScreenPos::new(s.canvas_size[0] * 0.5, s.canvas_size[1] * 0.5);
                    s.queue_zoom_at(center, 1.15, 18.0, Some(tx));
                })
                .child(
                    text("+")
                        .font(theme::CONTROL_TEXT)
                        .color(theme::text_secondary()),
                ),
        )
}

fn render_canvas_toast(
    canvas_size: [f32; 2],
    dock_pos: DockPosition,
    dock_y: f32,
    dock_h: f32,
    msg: &str,
) -> El {
    let est_w = (msg.chars().count() as f32 * 6.8 + 42.0).clamp(140.0, 360.0);
    let tx = ((canvas_size[0] - est_w) * 0.5).max(16.0);
    let ty = match dock_pos {
        DockPosition::Bottom => (dock_y - 38.0).max(12.0),
        DockPosition::Top => (dock_y + dock_h + 10.0).min((canvas_size[1] - 44.0).max(12.0)),
        DockPosition::Left | DockPosition::Right => (canvas_size[1] - 54.0).max(12.0),
    };
    row()
        .key("canvas_toast")
        .absolute(tx, ty)
        .h(28.0)
        .max_w(360.0)
        .px(12.0)
        .gap(6.0)
        .center_y()
        .radius(999.0)
        .fill(theme::bg_chrome().alpha(0.96))
        .border(1.0, theme::border_subtle())
        .shadow(6.0, 16.0, Color::BLACK.alpha(0.45))
        .child(
            text("✦")
                .font(theme::BADGE_TEXT)
                .color(theme::accent_claude()),
        )
        .child(
            text(msg)
                .font(theme::STATUS_TEXT)
                .color(theme::text_primary())
                .truncate(),
        )
}

fn render_node_card(s: &AppState, node: &Node, screen_pos: ScreenPos, zoom: f32) -> El {
    let is_selected = s.selected_nodes.contains(&node.id);
    let (base_w, base_h) = node_world_size(node);
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
        theme::accent_claude()
    } else if is_selected {
        theme::border_strong()
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
    let card_radius = (6.0 * zoom).clamp(3.0, 10.0);
    let socket_r = (4.5 * zoom).clamp(2.2, 8.0);
    let socket_d = socket_r * 2.0;

    let node_id_for_drag = node.id.clone();
    let out_node_id = node.id.clone();
    let del_node_id = node.id.clone();
    let note_click_id = node.id.clone();
    let doc_click_id = node.id.clone();
    let other_click_id = node.id.clone();

    let socket_rim = if is_selected {
        theme::accent_primary().alpha(0.75)
    } else {
        theme::text_muted().alpha(0.75)
    };
    let socket_core = if node.kind == NodeKind::LlmSynthesis {
        theme::accent_claude()
    } else if is_selected {
        theme::accent_primary()
    } else {
        theme::text_secondary()
    };

    // Sockets straddle left & right card boundaries outside the rounded inner clip wrapper
    let in_socket = if is_snap_target {
        let snap_r = socket_r * 1.6;
        let snap_d = snap_r * 2.0;
        el()
            .key(format!("{}_in_socket", node.id.as_str()))
            .absolute(-snap_r, h * 0.5 - snap_r)
            .size(snap_d, snap_d)
            .radius(snap_r)
            .fill(theme::accent_claude().alpha(0.32))
            .border(1.5, theme::accent_claude())
            .center()
            .child(
                el().size(snap_r, snap_r)
                    .radius(snap_r * 0.5)
                    .fill(theme::accent_claude()),
            )
    } else {
        el()
            .key(format!("{}_in_socket", node.id.as_str()))
            .absolute(-socket_r, h * 0.5 - socket_r)
            .size(socket_d, socket_d)
            .radius(socket_r)
            .fill(theme::bg_canvas())
            .border(1.25, socket_rim)
            .center()
            .child(
                el().size(socket_r * 0.72, socket_r * 0.72)
                    .radius(socket_r * 0.36)
                    .fill(socket_core),
            )
    };

    let out_socket = el()
        .key(format!("{}_out_socket", node.id.as_str()))
        .absolute(w - socket_r, h * 0.5 - socket_r)
        .size(socket_d, socket_d)
        .radius(socket_r)
        .fill(theme::bg_canvas())
        .border(1.25, socket_rim)
        .cursor(Cursor::Crosshair)
        .center()
        .child(
            el().size(socket_r * 0.72, socket_r * 0.72)
                .radius(socket_r * 0.36)
                .fill(socket_core),
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
                        if other.id != from_id && other.kind != NodeKind::Section {
                            let (ow, oh) = node_world_size(other);
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

    if zoom < LOW_ZOOM_TITLE_ONLY {
        let drag_id_lod = node_id_for_drag.clone();
        let lod_title_size = (11.5 * (zoom / LOW_ZOOM_TITLE_ONLY).max(0.76)).clamp(9.0, 12.5);
        let lod_title = if node.title.trim().is_empty() {
            theme::kind_label(node.kind)
        } else {
            node.title.as_str()
        };
        return col()
            .key(node.id.as_str())
            .size(w, h)
            .absolute(screen_pos.x, screen_pos.y)
            .radius(card_radius)
            .continuous_corners()
            .fill(card_bg)
            .border(if is_selected || is_snap_target { 1.25 } else { 1.0 }, border_color)
            .child(
                row()
                    .key(format!("node_title_lod_{}", node.id.as_str()))
                    .w_full()
                    .h_full()
                    .clip_rounded(card_radius)
                    .px((7.0 * zoom).clamp(4.0, 10.0))
                    .py((6.0 * zoom).clamp(3.0, 8.0))
                    .gap((4.0 * zoom).clamp(3.0, 6.0))
                    .center()
                    .cursor(Cursor::Grab)
                    .pointer_buttons([PointerButton::Primary, PointerButton::Middle])
                    .on_drag(move |s: &mut AppState, d, tx| match d.phase {
                        Phase::Down => {
                            if tx.button() == PointerButton::Middle || s.space_held {
                                if s.smooth_zoom_enabled {
                                    s.flush_live_camera();
                                }
                                s.canvas_mode = CanvasMode::Panning {
                                    start_screen: ScreenPos::new(d.x, d.y),
                                    start_offset: s.graph.viewport.offset,
                                };
                                return;
                            }
                            s.select_node(drag_id_lod.clone(), false);
                            if let Some(n) = s.graph.get_node(&drag_id_lod) {
                                s.canvas_mode = CanvasMode::DraggingNode {
                                    node_id: drag_id_lod.clone(),
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
                                    apply_pan_drag_step(s, start_offset, d.dx, d.dy, tx);
                                }
                                _ => {}
                            }
                        }
                        Phase::Up => {
                            if !d.cancelled()
                                && matches!(s.canvas_mode, CanvasMode::DraggingNode { .. })
                            {
                                if let Some(bundle) = &s.bundle {
                                    let _ = bundle.save_graph_atomic(&s.graph);
                                }
                            }
                            s.flush_live_camera();
                            s.canvas_mode = CanvasMode::Idle;
                        }
                    })
                    .child(
                        text(theme::kind_glyph(node.kind))
                            .font(TextStyle::new(lod_title_size, 600.0).mono())
                            .color(kind_accent),
                    )
                    .child(
                        el().grow(1.0).min_w(0.0).child(
                            text(lod_title)
                                .font(TextStyle::new(lod_title_size, 600.0))
                                .color(theme::text_primary())
                                .w_full()
                                .wrap(),
                        ),
                    ),
            )
            .child(in_socket)
            .child(out_socket)
            .cache();
    }

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
                    if s.smooth_zoom_enabled {
                        s.flush_live_camera();
                    }
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
                        apply_pan_drag_step(s, start_offset, d.dx, d.dy, tx);
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
                s.flush_live_camera();
                s.canvas_mode = CanvasMode::Idle;
            }
        })
        // Monochrome geometric kind glyph
        .child(
            text(theme::kind_glyph(node.kind))
                .font(TextStyle::new((10.5 * zoom).clamp(7.5, 14.0), 600.0).mono())
                .color(kind_accent),
        )
        // Title (directly editable inline when Note is selected, capped so right-center header stays draggable)
        .child(if is_selected && node.kind == NodeKind::Note {
            el().min_w(40.0 * zoom).max_w(w * 0.55).child(
                text_input(&s.canvas_title_edit, TextStyle::new(title_size, 600.0))
                    .key(format!("note_title_{}", node.id.as_str()))
                    .single_line()
                    .color(theme::text_primary())
                    .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
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
        .child(card_header_meta_pill(node, badge_size, zoom))
        // Subtle delete × button
        .child(
            el()
                .key(format!("node_del_{}", node.id.as_str()))
                .px((5.0 * zoom).clamp(3.0, 7.0))
                .py((1.5 * zoom).clamp(1.0, 4.0))
                .radius(4.0 * zoom)
                .hover(theme::accent_danger().alpha(0.18))
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
                            .radius(4.0 * zoom)
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
                                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
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
                    i18n::t(TextKey::NodeEmptyNote)
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
            let doc_open_btn_id = node.id.clone();
            let prev_page_id = node.id.clone();
            let next_page_id = node.id.clone();
            let doc_fmt = s.node_doc_format(&node.id);
            let fmt_badge = doc_reader::format_badge(doc_fmt);
            let has_pdf_asset =
                doc_fmt == DocFormat::Pdf && s.resolved_node_asset_path(&node.id).is_some();

            let pdf_preview = if has_pdf_asset {
                s.card_pdf_previews.get(&(node.id.clone(), node.preview_page))
            } else {
                None
            };
            let img_preview = if doc_fmt == DocFormat::Image {
                s.card_image_previews.get(&node.id)
            } else {
                None
            };
            let text_pages = if pdf_preview.is_none() && img_preview.is_none() {
                Some(s.doc_preview_pages_for(node, 5))
            } else {
                None
            };

            let (cur_page_idx, total_pages) = if let Some(pdf_p) = pdf_preview {
                let tot = pdf_p.page_count.max(1);
                (node.preview_page.min(tot - 1), tot)
            } else if let Some(ref pages) = text_pages {
                let tot = pages.len().max(1);
                (node.preview_page.min(tot - 1), tot)
            } else {
                (0, 1)
            };

            let page_heading_size = (10.0 * zoom).clamp(7.0, 13.5);
            let page_body_size = (9.0 * zoom).clamp(6.5, 12.5);

            let preview_inner = if let Some(pdf_p) = pdf_preview {
                el().w_full()
                    .h_full()
                    .padding(4.0 * zoom)
                    .pixels_fit(&pdf_p.pixels, SurfaceFit::Fit)
            } else if let Some(img) = img_preview {
                el().w_full()
                    .h_full()
                    .padding(4.0 * zoom)
                    .pixels_fit(img.pixels(), SurfaceFit::Fit)
            } else if let Some(ref pages) = text_pages {
                let page = &pages[cur_page_idx];
                col()
                    .w_full()
                    .h_full()
                    .padding(7.0 * zoom)
                    .gap(3.0 * zoom)
                    .clip_soft(0.0)
                    .child(
                        text(&page.heading)
                            .font(TextStyle::new(page_heading_size, 600.0).mono())
                            .color(theme::text_primary())
                            .truncate(),
                    )
                    .child(el().w_full().h(1.0).fill(theme::border_subtle()))
                    .child(
                        text(&page.body)
                            .font(TextStyle::new(page_body_size, 400.0).lh(page_body_size * 1.32))
                            .color(theme::text_secondary())
                            .w_full()
                            .wrap(),
                    )
            } else {
                el()
            };

            let preview_frame = col()
                .key(format!("node_body_{}", node.id.as_str()))
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .radius(4.0 * zoom)
                .fill(theme::bg_input())
                .hover(theme::hover_subtle())
                .border(1.0, theme::border_subtle())
                .clip_soft(0.0)
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.select_node(doc_click_id.clone(), false);
                    if !s.inspector_popped_out {
                        s.inspector_open = true;
                    }
                    s.ai_panel_open = false;
                    s.status_message = "Opened Document Reader".into();
                })
                .child(preview_inner);

            let footer_strip = row()
                .w_full()
                .center_y()
                .gap(4.0 * zoom)
                .child(
                    el()
                        .px(5.0 * zoom)
                        .py(1.5 * zoom)
                        .radius(3.0 * zoom)
                        .fill(theme::bg_control())
                        .border(1.0, theme::border_default())
                        .child(
                            text(fmt_badge)
                                .font(TextStyle::new(badge_size, 600.0).mono())
                                .color(theme::text_primary()),
                        ),
                )
                .child(
                    row()
                        .center_y()
                        .gap(2.0 * zoom)
                        .child(
                            el()
                                .key(format!("doc_prev_page_{}", node.id.as_str()))
                                .px(5.0 * zoom)
                                .py(1.5 * zoom)
                                .radius(3.0 * zoom)
                                .fill(theme::bg_control())
                                .hover(theme::hover_subtle())
                                .border(1.0, theme::border_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press(move |s: &mut AppState| {
                                    s.step_node_preview_page(&prev_page_id, -1);
                                })
                                .child(
                                    text("‹")
                                        .font(TextStyle::new(badge_size, 600.0).mono())
                                        .color(theme::text_secondary()),
                                ),
                        )
                        .child(
                            text(format!("p.{}/{}", cur_page_idx + 1, total_pages))
                                .font(TextStyle::new(badge_size, 500.0).mono())
                                .color(theme::text_secondary()),
                        )
                        .child(
                            el()
                                .key(format!("doc_next_page_{}", node.id.as_str()))
                                .px(5.0 * zoom)
                                .py(1.5 * zoom)
                                .radius(3.0 * zoom)
                                .fill(theme::bg_control())
                                .hover(theme::hover_subtle())
                                .border(1.0, theme::border_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press(move |s: &mut AppState| {
                                    s.step_node_preview_page(&next_page_id, 1);
                                })
                                .child(
                                    text("›")
                                        .font(TextStyle::new(badge_size, 600.0).mono())
                                        .color(theme::text_secondary()),
                                ),
                        ),
                )
                .child(el().grow(1.0))
                .child(
                    el()
                        .key(format!("doc_open_{}", node.id.as_str()))
                        .px(6.0 * zoom)
                        .py(1.5 * zoom)
                        .radius(3.0 * zoom)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .border(1.0, theme::border_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.select_node(doc_open_btn_id.clone(), false);
                            if !s.inspector_popped_out {
                                s.inspector_open = true;
                            } else if let Some(h) = &s.popout_window {
                                h.focus();
                            }
                            s.ai_panel_open = false;
                            s.status_message = "Opened Document Reader".into();
                        })
                        .child(
                            text(i18n::t(TextKey::CardOpen))
                                .font(TextStyle::new(badge_size, 500.0).mono())
                                .color(theme::text_primary()),
                        ),
                );

            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .clip_soft(0.0)
                .px(8.0 * zoom)
                .py(6.0 * zoom)
                .gap(5.0 * zoom)
                .child(preview_frame)
                .child(footer_strip)
        }
        NodeKind::Image => {
            let pick_id_a = node.id.clone();
            let pick_id_b = node.id.clone();
            let open_id = node.id.clone();
            let click_id_a = other_click_id.clone();
            let click_id_b = other_click_id.clone();
            let make_picker = |pick_id: studynodes_graph::model::NodeId| {
                move |s_ref: &mut AppState, tx: &mut Tx| {
                    s_ref.select_node(pick_id.clone(), false);
                    let opts = FileDialogOptions {
                        title: "Choose Image File".into(),
                        ..Default::default()
                    }
                    .filter(
                        "Images",
                        &["png", "jpg", "jpeg", "webp", "gif", "bmp", "svg"],
                    );
                    let target_id = pick_id.clone();
                    tx.open_file(opts, move |s: &mut AppState, res, _| {
                        if let Ok(Some(path)) = res {
                            s.ingest_file_into_node(&target_id, &path);
                        }
                    });
                }
            };

            if let Some(img) = s.card_image_previews.get(&node.id) {
                col()
                    .key(format!("node_body_{}", node.id.as_str()))
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .clip_soft(0.0)
                    .px(6.0 * zoom)
                    .py(5.0 * zoom)
                    .gap(4.0 * zoom)
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.select_node(click_id_a.clone(), false);
                        if !s.inspector_popped_out {
                            s.inspector_open = true;
                        }
                        s.ai_panel_open = false;
                    })
                    .child(
                        el()
                            .w_full()
                            .grow(1.0)
                            .min_h(0.0)
                            .radius(4.0 * zoom)
                            .fill(theme::bg_input())
                            .border(1.0, theme::border_subtle())
                            .pixels_fit(img.pixels(), SurfaceFit::Fit),
                    )
                    .child(
                        row()
                            .w_full()
                            .center_y()
                            .gap(4.0 * zoom)
                            .child(
                                el()
                                    .key(format!("img_pick_{}", node.id.as_str()))
                                    .px(6.0 * zoom)
                                    .py(2.0 * zoom)
                                    .radius(3.0 * zoom)
                                    .fill(theme::bg_control())
                                    .hover(theme::hover_subtle())
                                    .border(1.0, theme::border_subtle())
                                    .cursor(Cursor::Pointer)
                                    .on_press_tx(make_picker(pick_id_a))
                                    .child(
                                        text("Replace")
                                            .font(TextStyle::new(badge_size, 500.0))
                                            .color(theme::text_secondary()),
                                    ),
                            )
                            .child(el().grow(1.0))
                            .child(
                                el()
                                    .key(format!("img_open_{}", node.id.as_str()))
                                    .px(6.0 * zoom)
                                    .py(2.0 * zoom)
                                    .radius(3.0 * zoom)
                                    .fill(theme::bg_control())
                                    .hover(theme::hover_subtle())
                                    .border(1.0, theme::border_subtle())
                                    .cursor(Cursor::Pointer)
                                    .on_press(move |s: &mut AppState| {
                                        s.select_node(open_id.clone(), false);
                                        if !s.inspector_popped_out {
                                            s.inspector_open = true;
                                        }
                                        s.ai_panel_open = false;
                                    })
                                    .child(
                                        text(i18n::t(TextKey::CardOpen))
                                            .font(TextStyle::new(badge_size, 500.0).mono())
                                            .color(theme::text_primary()),
                                    ),
                            ),
                    )
            } else {
                col()
                    .key(format!("node_body_{}", node.id.as_str()))
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .clip_soft(0.0)
                    .padding(10.0 * zoom)
                    .center()
                    .gap(6.0 * zoom)
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.select_node(click_id_b.clone(), false);
                        if !s.inspector_popped_out {
                            s.inspector_open = true;
                        }
                        s.ai_panel_open = false;
                    })
                    .child(
                        el()
                            .key(format!("img_pick_{}", node.id.as_str()))
                            .px((10.0 * zoom).clamp(6.0, 14.0))
                            .py((5.0 * zoom).clamp(3.0, 8.0))
                            .radius(4.0 * zoom)
                            .fill(theme::bg_control())
                            .hover(theme::hover_subtle())
                            .border(1.0, theme::border_default())
                            .cursor(Cursor::Pointer)
                            .on_press_tx(make_picker(pick_id_b))
                            .child(
                                text(format!("+ {}", i18n::t(TextKey::ReaderIngestImage)))
                                    .font(TextStyle::new(badge_size.max(9.5), 600.0))
                                    .color(theme::text_primary()),
                            ),
                    )
                    .child(
                        text("or drop PNG / JPG / SVG")
                            .font(TextStyle::new((badge_size * 0.9).clamp(7.5, 11.5), 400.0))
                            .color(theme::text_muted()),
                    )
            }
        }
        NodeKind::Flashcard => render_flashcard_body(s, node, body_size, badge_size, zoom),
        NodeKind::Excerpt => render_excerpt_body(s, node, body_size, badge_size, zoom),
        NodeKind::Checklist => render_checklist_body(node, body_size, badge_size, zoom),
        NodeKind::Formula => {
            render_formula_body(s, node, is_selected, body_size, badge_size, zoom)
        }
        NodeKind::Sequence => render_sequence_body(node, body_size, badge_size, zoom),
        NodeKind::Video => render_video_card_body(s, node, body_size, badge_size, zoom),
        NodeKind::Audio => render_audio_card_body(s, node, body_size, badge_size, zoom),
        NodeKind::Calendar => render_calendar_card_body(s, node, body_size, badge_size, zoom),
        NodeKind::Drawing => render_drawing_card_body(s, node, body_size, badge_size, zoom),
        _ => {
            let preview_text = if node.content.trim().is_empty() {
                match node.kind {
                    NodeKind::Image => i18n::t(TextKey::NodeFallbackImage),
                    NodeKind::LlmSynthesis => i18n::t(TextKey::NodeFallbackSynthesis),
                    NodeKind::Concept => i18n::t(TextKey::NodeFallbackConcept),
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
                    if !s.inspector_popped_out {
                        s.inspector_open = true;
                    }
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

    let resize_id = node.id.clone();
    let is_resizable_media = matches!(
        node.kind,
        NodeKind::Video | NodeKind::Audio | NodeKind::Drawing
    );

    // Assemble card shell (no outer drag/press handlers!)
    let mut shell = col()
        .key(node.id.as_str())
        .size(w, h)
        .absolute(screen_pos.x, screen_pos.y)
        .radius(card_radius)
        .continuous_corners()
        .fill(card_bg)
        .border(if is_selected || is_snap_target { 1.25 } else { 1.0 }, border_color)
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
        .child(out_socket);

    if is_resizable_media {
        let grip = (14.0 * zoom).clamp(11.0, 18.0);
        let min_size = match node.kind {
            NodeKind::Video => (240.0, 176.0),
            NodeKind::Drawing => (240.0, 180.0),
            _ => (230.0, 112.0),
        };
        let max_size = match node.kind {
            NodeKind::Video => (1280.0, 860.0),
            NodeKind::Drawing => (960.0, 720.0),
            _ => (900.0, 520.0),
        };
        shell = shell.child(
            el()
                .key(format!("node_resize_{}", node.id.as_str()))
                .absolute((w - grip - 2.0).max(0.0), (h - grip - 2.0).max(0.0))
                .size(grip, grip)
                .radius(3.0)
                .fill(Color::TRANSPARENT)
                .hover(theme::hover_subtle())
                .center()
                .cursor(Cursor::ResizeColumn)
                .on_drag(move |s: &mut AppState, d, tx| match d.phase {
                    Phase::Down => {
                        if let Some(n) = s.graph.get_node(&resize_id) {
                            let sz = node_world_size(n);
                            s.canvas_mode = CanvasMode::ResizingNode {
                                node_id: resize_id.clone(),
                                start_size: sz,
                                min_size,
                                max_size,
                            };
                        }
                    }
                    Phase::Move => {
                        let z = s.graph.viewport.zoom.max(0.01);
                        if let CanvasMode::ResizingNode {
                            node_id,
                            start_size,
                            min_size,
                            max_size,
                        } = &s.canvas_mode
                        {
                            let node_id = node_id.clone();
                            let start_size = *start_size;
                            let min_sz = *min_size;
                            let max_sz = *max_size;
                            if let Some(n) = s.graph.get_node_mut(&node_id) {
                                n.size = (
                                    (start_size.0 + d.dx / z).clamp(min_sz.0, max_sz.0),
                                    (start_size.1 + d.dy / z).clamp(min_sz.1, max_sz.1),
                                );
                            }
                            tx.instant();
                        }
                    }
                    Phase::Up => {
                        if !d.cancelled()
                            && matches!(s.canvas_mode, CanvasMode::ResizingNode { .. })
                        {
                            if let Some(bundle) = &s.bundle {
                                let _ = bundle.save_graph_atomic(&s.graph);
                            }
                        }
                        s.canvas_mode = CanvasMode::Idle;
                    }
                })
                .child(
                    text("⋱")
                        .font(TextStyle::new((badge_size * 0.95).clamp(8.0, 12.0), 600.0))
                        .color(theme::text_muted()),
                ),
        );
    }

    if node.kind == NodeKind::Drawing {
        shell
    } else {
        shell.cache()
    }
}

fn card_header_meta_pill(node: &Node, badge_size: f32, zoom: f32) -> El {
    let badge: Option<(String, Color)> = match node.kind {
        NodeKind::Flashcard => {
            let m = node.meta.mastery.as_deref().unwrap_or("new");
            let col = if m == "again" {
                theme::accent_claude()
            } else if m == "easy" || m == "good" {
                theme::text_primary()
            } else {
                theme::text_muted()
            };
            Some((m.to_uppercase(), col))
        }
        NodeKind::Checklist => {
            let tasks = doc_reader::parse_markdown_tasks(&node.content);
            let done = tasks.iter().filter(|(c, _)| *c).count();
            Some((
                format!("{}/{}", done, tasks.len()),
                if !tasks.is_empty() && done == tasks.len() {
                    theme::accent_claude()
                } else {
                    theme::text_secondary()
                },
            ))
        }
        NodeKind::Formula => Some((
            node.meta
                .language
                .as_deref()
                .unwrap_or("MATH")
                .to_uppercase(),
            theme::text_secondary(),
        )),
        NodeKind::Sequence => Some((
            format!("#{:02}", node.meta.step_index.unwrap_or(1)),
            theme::text_primary(),
        )),
        NodeKind::Video | NodeKind::Audio => {
            let label = if let Some(url) = node
                .meta
                .media_url
                .as_deref()
                .map(str::trim)
                .filter(|u| !u.is_empty())
            {
                let plat = media_helper::classify_media_url(url).label();
                if node.meta.timestamps.is_empty() {
                    plat.to_string()
                } else {
                    format!("{} · {} TS", plat, node.meta.timestamps.len())
                }
            } else if !node.meta.timestamps.is_empty() {
                format!("{} TS", node.meta.timestamps.len())
            } else {
                "MEDIA".into()
            };
            Some((label, theme::text_secondary()))
        }
        NodeKind::Calendar => {
            let done = node.meta.calendar_events.iter().filter(|e| e.completed).count();
            let total = node.meta.calendar_events.len();
            if total == 0 {
                None
            } else {
                Some((
                    format!("{}/{}", done, total),
                    if done == total {
                        theme::accent_claude()
                    } else {
                        theme::text_secondary()
                    },
                ))
            }
        }
        NodeKind::Drawing => {
            if node.meta.drawing_strokes.is_empty() {
                None
            } else {
                Some((
                    format!("{} ✎", node.meta.drawing_strokes.len()),
                    theme::text_secondary(),
                ))
            }
        }
        _ => return el(),
    };

    let inspect_id = node.id.clone();
    let mut r = row().center_y().gap(3.0 * zoom);
    if let Some((label, accent)) = badge {
        r = r.child(
            el()
                .px((5.0 * zoom).clamp(3.0, 7.0))
                .py((1.5 * zoom).clamp(1.0, 3.0))
                .radius(3.0 * zoom)
                .fill(theme::bg_control())
                .border(1.0, theme::border_subtle())
                .child(
                    text(label)
                        .font(TextStyle::new(badge_size, 600.0).mono())
                        .color(accent),
                ),
        );
    }
    r.child(
        el()
            .key(format!("node_inspect_{}", node.id.as_str()))
            .px((4.0 * zoom).clamp(2.5, 6.0))
            .py((1.5 * zoom).clamp(1.0, 3.0))
            .radius(3.0 * zoom)
            .hover(theme::hover_subtle())
            .cursor(Cursor::Pointer)
            .on_press(move |s: &mut AppState| {
                s.select_node(inspect_id.clone(), false);
                if !s.inspector_popped_out {
                    s.inspector_open = true;
                }
                s.ai_panel_open = false;
            })
            .child(
                text("✎")
                    .font(TextStyle::new(badge_size, 600.0))
                    .color(theme::text_muted()),
            ),
    )
}

fn render_flashcard_body(
    s: &AppState,
    node: &Node,
    body_size: f32,
    badge_size: f32,
    zoom: f32,
) -> El {
    let flipped = s.flipped_flashcards.contains(&node.id);
    let select_id = node.id.clone();
    let flip_id = node.id.clone();
    let again_id = node.id.clone();
    let good_id = node.id.clone();
    let easy_id = node.id.clone();

    let eyebrow = if flipped { "ANSWER" } else { "PROMPT" };
    let face_text = if flipped {
        node.meta
            .answer
            .as_deref()
            .filter(|a| !a.trim().is_empty())
            .unwrap_or("No answer set — click ✎ to add answer.")
    } else if node.content.trim().is_empty() {
        "Click ✎ to add recall prompt..."
    } else {
        node.content.trim()
    };

    let cur_mastery = node.meta.mastery.as_deref().unwrap_or("new");
    let mut actions = row().w_full().center_y().gap(4.0 * zoom).child(
        el()
            .key(format!("flash_flip_{}", node.id.as_str()))
            .px((7.0 * zoom).clamp(5.0, 10.0))
            .py((3.0 * zoom).clamp(2.0, 5.0))
            .radius(4.0 * zoom)
            .fill(if flipped {
                theme::accent_claude().alpha(0.18)
            } else {
                theme::bg_control()
            })
            .hover(theme::hover_subtle())
            .border(
                1.0,
                if flipped {
                    theme::accent_claude().alpha(0.50)
                } else {
                    theme::border_default()
                },
            )
            .cursor(Cursor::Pointer)
            .on_press(move |s: &mut AppState| {
                s.toggle_flashcard_flip(&flip_id);
            })
            .child(
                text(if flipped {
                    "↺"
                } else {
                    i18n::t(TextKey::CardReveal)
                })
                .font(TextStyle::new(badge_size, 600.0))
                .color(if flipped {
                    theme::accent_claude()
                } else {
                    theme::text_primary()
                }),
            ),
    );

    actions = actions.child(el().grow(1.0));
    if flipped {
        for (key_prefix, label, rating, active, target_id, accent) in [
            (
                "flash_rate_again_",
                i18n::t(TextKey::ReaderMasteryAgain),
                "again",
                cur_mastery == "again",
                again_id,
                theme::accent_claude(),
            ),
            (
                "flash_rate_good_",
                i18n::t(TextKey::ReaderMasteryGood),
                "good",
                cur_mastery == "good",
                good_id,
                theme::text_primary(),
            ),
            (
                "flash_rate_easy_",
                i18n::t(TextKey::ReaderMasteryEasy),
                "easy",
                cur_mastery == "easy",
                easy_id,
                theme::text_primary(),
            ),
        ] {
            actions = actions.child(
                el()
                    .key(format!("{}{}", key_prefix, node.id.as_str()))
                    .px((5.5 * zoom).clamp(4.0, 8.0))
                    .py((3.0 * zoom).clamp(2.0, 5.0))
                    .radius(4.0 * zoom)
                    .fill(if active {
                        accent.alpha(0.20)
                    } else {
                        theme::bg_control()
                    })
                    .hover(theme::hover_subtle())
                    .border(
                        1.0,
                        if active {
                            accent.alpha(0.55)
                        } else {
                            theme::border_subtle()
                        },
                    )
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.set_flashcard_mastery(&target_id, rating);
                    })
                    .child(
                        text(label)
                            .font(TextStyle::new(badge_size, 600.0))
                            .color(if active { accent } else { theme::text_secondary() }),
                    ),
            );
        }
    }

    col()
        .key(format!("node_body_{}", node.id.as_str()))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .clip_soft(0.0)
        .px(10.0 * zoom)
        .py(8.0 * zoom)
        .gap(5.0 * zoom)
        .on_press(move |s: &mut AppState| {
            s.select_node(select_id.clone(), false);
        })
        .child(
            text(eyebrow)
                .font(TextStyle::new((8.5 * zoom).clamp(7.0, 11.5), 600.0).mono())
                .color(if flipped {
                    theme::accent_claude()
                } else {
                    theme::text_muted()
                }),
        )
        .child(
            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .clip_soft(0.0)
                .child(
                    text(face_text)
                        .font(TextStyle::new(body_size, 400.0).lh(body_size * 1.38))
                        .color(if flipped {
                            theme::text_primary()
                        } else {
                            theme::text_secondary()
                        })
                        .w_full()
                        .wrap(),
                ),
        )
        .child(actions)
}

fn render_excerpt_body(
    s: &AppState,
    node: &Node,
    body_size: f32,
    badge_size: f32,
    zoom: f32,
) -> El {
    let select_id = node.id.clone();
    let jump_id = node.id.clone();
    let quote_text = if node.content.trim().is_empty() {
        "\"Quoted source passage...\""
    } else {
        node.content.trim()
    };

    let source_label = match (&node.meta.source_node, node.meta.source_page) {
        (Some(doc_id), page_opt) => {
            let doc_title = s
                .graph
                .get_node(doc_id)
                .map(|d| d.title.as_str())
                .unwrap_or("Source Doc");
            let page = page_opt.unwrap_or(0) + 1;
            format!("▤ {} · p.{} ↗", doc_title, page)
        }
        _ => i18n::t(TextKey::ExcerptNoSource).into(),
    };

    col()
        .key(format!("node_body_{}", node.id.as_str()))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .clip_soft(0.0)
        .px(10.0 * zoom)
        .py(8.0 * zoom)
        .gap(6.0 * zoom)
        .on_press(move |s: &mut AppState| {
            s.select_node(select_id.clone(), false);
        })
        .child(
            row()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .gap(7.0 * zoom)
                .child(
                    el().w((2.5 * zoom).clamp(1.5, 4.0))
                        .h_full()
                        .radius(1.5)
                        .fill(theme::accent_claude().alpha(0.75)),
                )
                .child(
                    el().grow(1.0).min_h(0.0).clip_soft(0.0).child(
                        text(quote_text)
                            .font(TextStyle::new(body_size, 400.0).lh(body_size * 1.38))
                            .color(theme::text_primary())
                            .w_full()
                            .wrap(),
                    ),
                ),
        )
        .child(
            el()
                .key(format!("excerpt_jump_{}", node.id.as_str()))
                .w_full()
                .px(7.0 * zoom)
                .py(3.5 * zoom)
                .radius(4.0 * zoom)
                .fill(theme::bg_control())
                .hover(theme::accent_claude().alpha(0.16))
                .border(1.0, theme::border_subtle())
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.jump_to_excerpt_source(&jump_id);
                })
                .child(
                    text(source_label)
                        .font(TextStyle::new(badge_size, 600.0))
                        .color(if node.meta.source_node.is_some() {
                            theme::accent_claude()
                        } else {
                            theme::text_muted()
                        })
                        .truncate(),
                ),
        )
}

fn render_checklist_body(node: &Node, body_size: f32, badge_size: f32, zoom: f32) -> El {
    let select_id = node.id.clone();
    let tasks = doc_reader::parse_markdown_tasks(&node.content);
    let total = tasks.len();
    let done = tasks.iter().filter(|(c, _)| *c).count();
    let ratio = if total > 0 {
        (done as f32 / total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let track_w = (232.0 * zoom).max(80.0);
    let mut body = col()
        .key(format!("node_body_{}", node.id.as_str()))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .clip_soft(0.0)
        .px(10.0 * zoom)
        .py(7.0 * zoom)
        .gap(4.0 * zoom)
        .on_press(move |s: &mut AppState| {
            s.select_node(select_id.clone(), false);
        })
        .child(
            el().w_full()
                .h((3.0 * zoom).clamp(2.0, 5.0))
                .radius(2.0)
                .fill(theme::bg_input())
                .child(
                    el().w(track_w * ratio)
                        .h_full()
                        .radius(2.0)
                        .fill(if done == total && total > 0 {
                            theme::accent_claude()
                        } else {
                            theme::accent_primary()
                        }),
                ),
        );

    if tasks.is_empty() {
        body = body.child(
            text("No `- [ ]` tasks yet. Click ✎ to add.")
                .font(TextStyle::new(body_size, 400.0))
                .color(theme::text_muted())
                .w_full()
                .wrap(),
        );
        return body;
    }

    for (idx, (checked, label)) in tasks.iter().take(4).enumerate() {
        let toggle_id = node.id.clone();
        let checked_val = *checked;
        body = body.child(
            row()
                .key(format!("check_item_{}_{}", node.id.as_str(), idx))
                .w_full()
                .center_y()
                .gap(5.5 * zoom)
                .px(4.0 * zoom)
                .py(2.0 * zoom)
                .radius(3.5 * zoom)
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.toggle_checklist_item_on_node(&toggle_id, idx);
                })
                .child(
                    el()
                        .size((12.0 * zoom).clamp(9.0, 16.0), (12.0 * zoom).clamp(9.0, 16.0))
                        .radius(3.0 * zoom)
                        .fill(if checked_val {
                            theme::accent_claude().alpha(0.24)
                        } else {
                            theme::bg_input()
                        })
                        .border(
                            1.0,
                            if checked_val {
                                theme::accent_claude()
                            } else {
                                theme::border_default()
                            },
                        )
                        .center()
                        .child(
                            text(if checked_val { "✓" } else { "" })
                                .font(TextStyle::new(badge_size, 700.0))
                                .color(theme::accent_claude()),
                        ),
                )
                .child(
                    el().grow(1.0).min_w(0.0).child(
                        text(label)
                            .font(TextStyle::new(
                                (body_size * 0.92).clamp(8.0, 15.0),
                                400.0,
                            ))
                            .color(if checked_val {
                                theme::text_muted()
                            } else {
                                theme::text_primary()
                            })
                            .truncate(),
                    ),
                ),
        );
    }

    if total > 4 {
        body = body.child(
            text(format!("+{} more in inspector", total - 4))
                .font(TextStyle::new(badge_size, 500.0))
                .color(theme::text_muted()),
        );
    }

    body
}

fn render_formula_body(
    s: &AppState,
    node: &Node,
    is_selected: bool,
    body_size: f32,
    badge_size: f32,
    zoom: f32,
) -> El {
    let select_id = node.id.clone();
    let is_editing = is_selected && s.editing_node_id.as_ref() == Some(&node.id);
    let formula_text = if node.content.trim().is_empty() {
        "Click to edit equation..."
    } else {
        node.content.trim()
    };

    if is_editing {
        let fmt_id = node.id.clone();
        let mut sym_bar = row().w_full().center_y().gap(2.5 * zoom);
        for (idx, &(label, sym)) in formula_math::QUICK_CARD_SYMBOLS.iter().take(7).enumerate() {
            let sym_nid = node.id.clone();
            sym_bar = sym_bar.child(
                el()
                    .key(format!("formula_sym_{}_{}", node.id.as_str(), idx))
                    .px((4.0 * zoom).clamp(3.0, 6.0))
                    .py((2.0 * zoom).clamp(1.0, 4.0))
                    .radius(3.0 * zoom)
                    .fill(theme::bg_control())
                    .hover(theme::hover_subtle())
                    .border(1.0, theme::border_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.insert_formula_symbol(&sym_nid, sym);
                    })
                    .child(
                        text(label)
                            .font(TextStyle::new(badge_size, 500.0).mono())
                            .color(theme::text_primary()),
                    ),
            );
        }
        sym_bar = sym_bar.child(el().grow(1.0)).child(
            el()
                .key(format!("formula_fmt_{}", node.id.as_str()))
                .px((5.0 * zoom).clamp(3.0, 8.0))
                .py((2.0 * zoom).clamp(1.0, 4.0))
                .radius(3.0 * zoom)
                .fill(theme::accent_claude().alpha(0.16))
                .hover(theme::accent_claude().alpha(0.24))
                .border(1.0, theme::accent_claude().alpha(0.45))
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.format_formula_node(&fmt_id);
                })
                .child(
                    text("\\→∑")
                        .font(TextStyle::new(badge_size, 600.0).mono())
                        .color(theme::accent_claude()),
                ),
        );

        col()
            .key(format!("node_body_{}", node.id.as_str()))
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .clip_soft(0.0)
            .padding(7.0 * zoom)
            .gap(4.5 * zoom)
            .child(
                col()
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_default())
                    .padding(7.0 * zoom)
                    .child(
                        text_input(
                            &s.canvas_content_edit,
                            TextStyle::new(body_size, 500.0).mono().lh(body_size * 1.4),
                        )
                        .key(format!("formula_input_{}", node.id.as_str()))
                        .label("Formula editor")
                        .w_full()
                        .grow(1.0)
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder("Type math or \\alpha, \\sum, -> ...", theme::text_muted())
                        .on_edit(|s: &mut AppState| &mut s.canvas_content_edit)
                        .on_input_change(|s: &mut AppState, _| {
                            s.sync_from_canvas_edits();
                            s.maybe_auto_expand_formula_on_canvas_edit();
                        }),
                    ),
            )
            .child(sym_bar)
    } else {
        col()
            .key(format!("node_body_{}", node.id.as_str()))
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .clip_soft(0.0)
            .padding(8.0 * zoom)
            .cursor(Cursor::Pointer)
            .on_press(move |s: &mut AppState| {
                s.select_node(select_id.clone(), false);
            })
            .child(
                col()
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_subtle())
                    .padding(8.0 * zoom)
                    .child(
                        text(formula_text)
                            .font(TextStyle::new(body_size, 500.0).mono().lh(body_size * 1.42))
                            .color(if node.content.trim().is_empty() {
                                theme::text_muted()
                            } else {
                                theme::text_primary()
                            })
                            .w_full()
                            .wrap(),
                    ),
            )
    }
}

fn render_sequence_body(node: &Node, body_size: f32, badge_size: f32, zoom: f32) -> El {
    let select_id = node.id.clone();
    let seq_id = node.id.clone();
    let step_num = node.meta.step_index.unwrap_or(1);
    let desc = if node.content.trim().is_empty() {
        "Step description..."
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
        .py(7.0 * zoom)
        .gap(5.0 * zoom)
        .on_press(move |s: &mut AppState| {
            s.select_node(select_id.clone(), false);
        })
        .child(
            el().grow(1.0).min_h(0.0).clip_soft(0.0).child(
                text(desc)
                    .font(TextStyle::new(body_size, 400.0).lh(body_size * 1.38))
                    .color(theme::text_secondary())
                    .w_full()
                    .wrap(),
            ),
        )
        .child(
            row()
                .w_full()
                .center_y()
                .child(
                    text(format!("Step {:02}", step_num))
                        .font(TextStyle::new(badge_size, 600.0).mono())
                        .color(theme::text_muted()),
                )
                .child(el().grow(1.0))
                .child(
                    el()
                        .key(format!("seq_next_{}", node.id.as_str()))
                        .px((7.0 * zoom).clamp(5.0, 10.0))
                        .py((3.0 * zoom).clamp(2.0, 5.0))
                        .radius(4.0 * zoom)
                        .fill(theme::bg_control())
                        .hover(theme::accent_claude().alpha(0.18))
                        .border(1.0, theme::border_default())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.add_next_sequence_step(&seq_id);
                        })
                        .child(
                            text(i18n::t(TextKey::CardNextStep))
                                .font(TextStyle::new(badge_size, 600.0))
                                .color(theme::text_primary()),
                        ),
                ),
        )
}

fn render_section_node(s: &AppState, node: &Node, screen_pos: ScreenPos, zoom: f32) -> El {
    let is_selected = s.selected_nodes.contains(&node.id);
    let (base_w, base_h) = node_world_size(node);
    let w = base_w * zoom;
    let h = base_h * zoom;
    let framed = node.meta.section_framed;
    let header_h = if framed {
        (34.0 * zoom).clamp(26.0, 44.0)
    } else {
        h.max(28.0)
    };
    let sec_title_size = (13.5 * zoom).clamp(11.5, 22.0);
    let badge_size = (9.5 * zoom).clamp(7.5, 13.0);

    let sec_drag_id = node.id.clone();
    let sec_toggle_id = node.id.clone();
    let sec_edit_id = node.id.clone();
    let sec_del_id = node.id.clone();
    let sec_resize_id = node.id.clone();

    let border_color = if is_selected {
        theme::border_strong()
    } else {
        theme::border_default()
    };

    let low_zoom_lod = zoom < LOW_ZOOM_TITLE_ONLY;
    let mut header_bar = row()
        .key(format!("node_header_{}", node.id.as_str()))
        .w_full()
        .h(header_h)
        .px(10.0 * zoom)
        .gap(7.0 * zoom)
        .center_y()
        .fill(if is_selected {
            theme::bg_card_selected()
        } else {
            theme::bg_chrome().alpha(0.92)
        })
        .border_sides([0.0, 0.0, 1.0, 0.0], border_color)
        .cursor(Cursor::Grab)
        .pointer_buttons([PointerButton::Primary, PointerButton::Middle])
        .on_drag(move |s: &mut AppState, d, tx| match d.phase {
            Phase::Down => {
                if tx.button() == PointerButton::Middle || s.space_held {
                    if s.smooth_zoom_enabled {
                        s.flush_live_camera();
                    }
                    s.canvas_mode = CanvasMode::Panning {
                        start_screen: ScreenPos::new(d.x, d.y),
                        start_offset: s.graph.viewport.offset,
                    };
                    return;
                }
                s.select_node(sec_drag_id.clone(), false);
                let children = s.capture_section_children(&sec_drag_id);
                if let Some(n) = s.graph.get_node(&sec_drag_id) {
                    s.canvas_mode = CanvasMode::DraggingSection {
                        node_id: sec_drag_id.clone(),
                        start_pos: n.pos,
                        children,
                    };
                }
            }
            Phase::Move => {
                let z = s.graph.viewport.zoom.max(0.01);
                match &s.canvas_mode {
                    CanvasMode::DraggingSection {
                        node_id,
                        start_pos,
                        children,
                    } => {
                        let node_id = node_id.clone();
                        let start_pos = *start_pos;
                        let children = children.clone();
                        let dx_w = d.dx / z;
                        let dy_w = d.dy / z;
                        if let Some(n) = s.graph.get_node_mut(&node_id) {
                            n.pos = WorldPos::new(start_pos.x + dx_w, start_pos.y + dy_w);
                        }
                        for (cid, cpos) in children {
                            if let Some(cn) = s.graph.get_node_mut(&cid) {
                                cn.pos = WorldPos::new(cpos.x + dx_w, cpos.y + dy_w);
                            }
                        }
                    }
                    CanvasMode::Panning { start_offset, .. } => {
                        let start_offset = *start_offset;
                        apply_pan_drag_step(s, start_offset, d.dx, d.dy, tx);
                    }
                    _ => {}
                }
            }
            Phase::Up => {
                if !d.cancelled() && matches!(s.canvas_mode, CanvasMode::DraggingSection { .. }) {
                    if let Some(bundle) = &s.bundle {
                        let _ = bundle.save_graph_atomic(&s.graph);
                    }
                }
                s.flush_live_camera();
                s.canvas_mode = CanvasMode::Idle;
            }
        })
        .child(
            text("▭")
                .font(TextStyle::new(sec_title_size * 0.9, 600.0).mono())
                .color(theme::kind_accent(NodeKind::Section)),
        )
        .child(
            el().grow(1.0).min_w(0.0).child(
                text(&node.title)
                    .font(TextStyle::new(sec_title_size, 600.0))
                    .color(theme::text_primary())
                    .truncate(),
            ),
        );

    if !low_zoom_lod {
        let sec_shot_id = node.id.clone();
        let sec_shot_slug: String = node
            .title
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .take(40)
            .collect();
        let sec_shot_name = if sec_shot_slug.trim_matches('_').is_empty() {
            "section_scheme.png".to_string()
        } else {
            format!("{}_scheme.png", sec_shot_slug.trim_matches('_'))
        };
        header_bar = header_bar
            .child(
                el()
                    .key(format!("sec_toggle_frame_{}", node.id.as_str()))
                    .px(6.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .hover(theme::hover_subtle())
                    .border(1.0, theme::border_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.toggle_section_framed(&sec_toggle_id);
                    })
                    .child(
                        text(if framed {
                            i18n::t(TextKey::CardFrame)
                        } else {
                            i18n::t(TextKey::CardBanner)
                        })
                        .font(TextStyle::new(badge_size, 600.0))
                        .color(theme::text_secondary()),
                    ),
            )
            .child(
                el()
                    .key(format!("sec_screenshot_{}", node.id.as_str()))
                    .px(5.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .hover(theme::hover_subtle())
                    .border(1.0, theme::border_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press_tx(move |_s: &mut AppState, tx: &mut Tx| {
                        let nid = sec_shot_id.clone();
                        let opts = FileDialogOptions {
                            title: i18n::t(TextKey::SectionScreenshotBtn).to_string(),
                            file_name: Some(sec_shot_name.clone()),
                            ..Default::default()
                        }
                        .filter("PNG Image", &["png"]);
                        tx.save_file(opts, move |s: &mut AppState, res, _| {
                            if let Ok(Some(dest)) = res {
                                let _ = s.export_scheme_screenshot(Some(&nid), Some(dest));
                            }
                        });
                    })
                    .child(
                        text("📷")
                            .font(TextStyle::new(badge_size, 600.0))
                            .color(theme::accent_claude()),
                    ),
            )
            .child(
                el()
                    .key(format!("node_inspect_{}", node.id.as_str()))
                    .px(5.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.select_node(sec_edit_id.clone(), false);
                        if !s.inspector_popped_out {
                            s.inspector_open = true;
                        }
                        s.ai_panel_open = false;
                    })
                    .child(
                        text("✎")
                            .font(TextStyle::new(badge_size, 600.0))
                            .color(theme::text_muted()),
                    ),
            )
            .child(
                el()
                    .key(format!("node_del_{}", node.id.as_str()))
                    .px(5.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .hover(theme::accent_danger().alpha(0.18))
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.delete_node(&sec_del_id);
                    })
                    .child(
                        text("×")
                            .font(TextStyle::new((13.0 * zoom).clamp(10.0, 18.0), 600.0))
                            .color(theme::text_muted()),
                    ),
            );
    }

    let radius = (8.0 * zoom).clamp(4.0, 12.0);
    let mut container = el()
        .key(node.id.as_str())
        .size(w, h)
        .absolute(screen_pos.x, screen_pos.y)
        .radius(radius)
        .clip_rounded(radius)
        .fill(if framed {
            Color::WHITE.alpha(0.025)
        } else {
            theme::bg_chrome()
        })
        .border(if is_selected { 1.25 } else { 1.0 }, border_color)
        .child(header_bar);

    if framed && !low_zoom_lod {
        let grip = (16.0 * zoom).clamp(12.0, 22.0);
        container = container.child(
            el()
                .key(format!("sec_resize_{}", node.id.as_str()))
                .absolute((w - grip - 3.0).max(0.0), (h - grip - 3.0).max(0.0))
                .size(grip, grip)
                .radius(3.0)
                .fill(theme::bg_control())
                .hover(theme::accent_claude().alpha(0.35))
                .border(1.0, theme::border_default())
                .center()
                .cursor(Cursor::ResizeColumn)
                .on_drag(move |s: &mut AppState, d, _| match d.phase {
                    Phase::Down => {
                        if let Some(n) = s.graph.get_node(&sec_resize_id) {
                            let sz = node_world_size(n);
                            s.canvas_mode = CanvasMode::ResizingSection {
                                node_id: sec_resize_id.clone(),
                                start_size: sz,
                            };
                        }
                    }
                    Phase::Move => {
                        let z = s.graph.viewport.zoom.max(0.01);
                        if let CanvasMode::ResizingSection {
                            node_id,
                            start_size,
                        } = &s.canvas_mode
                        {
                            let node_id = node_id.clone();
                            let start_size = *start_size;
                            if let Some(n) = s.graph.get_node_mut(&node_id) {
                                n.size = (
                                    (start_size.0 + d.dx / z).clamp(240.0, 2000.0),
                                    (start_size.1 + d.dy / z).clamp(140.0, 1400.0),
                                );
                            }
                        }
                    }
                    Phase::Up => {
                        if !d.cancelled()
                            && matches!(s.canvas_mode, CanvasMode::ResizingSection { .. })
                        {
                            if let Some(bundle) = &s.bundle {
                                let _ = bundle.save_graph_atomic(&s.graph);
                            }
                        }
                        s.canvas_mode = CanvasMode::Idle;
                    }
                })
                .child(
                    text("⋱")
                        .font(TextStyle::new(badge_size, 600.0))
                        .color(theme::text_muted()),
                ),
        );
    }

    container
}

fn render_block_menu_popover(
    s: &AppState,
    dock_pos: DockPosition,
    dock_x: f32,
    dock_y: f32,
    dock_w: f32,
    dock_h: f32,
) -> El {
    let menu_w = 232.0;
    let cw = s.canvas_size[0].max(240.0);
    let ch = s.canvas_size[1].max(200.0);
    let (menu_x, menu_y, max_menu_h) = match dock_pos {
        DockPosition::Bottom => (
            (dock_x + 96.0).clamp(12.0, (cw - menu_w - 12.0).max(12.0)),
            (dock_y - 468.0).max(12.0),
            (dock_y - 24.0).max(200.0),
        ),
        DockPosition::Top => (
            (dock_x + 96.0).clamp(12.0, (cw - menu_w - 12.0).max(12.0)),
            (dock_y + dock_h + 8.0).min((ch - 220.0).max(12.0)),
            (ch - (dock_y + dock_h + 24.0)).max(200.0),
        ),
        DockPosition::Left => (
            (dock_x + dock_w + 8.0).min((cw - menu_w - 12.0).max(12.0)),
            dock_y.clamp(12.0, (ch - 320.0).max(12.0)),
            (ch - 24.0).max(200.0),
        ),
        DockPosition::Right => (
            (dock_x - menu_w - 8.0).max(12.0),
            dock_y.clamp(12.0, (ch - 320.0).max(12.0)),
            (ch - 24.0).max(200.0),
        ),
    };
    let items = [
        (
            "btn_add_section",
            "▭",
            i18n::t(TextKey::BlockSectionTitle),
            i18n::t(TextKey::BlockSectionDesc),
            NodeKind::Section,
        ),
        (
            "btn_add_image",
            "▣",
            i18n::t(TextKey::BlockImageTitle),
            i18n::t(TextKey::BlockImageDesc),
            NodeKind::Image,
        ),
        (
            "btn_add_drawing",
            "✎",
            i18n::t(TextKey::BlockDrawingTitle),
            i18n::t(TextKey::BlockDrawingDesc),
            NodeKind::Drawing,
        ),
        (
            "btn_add_flashcard",
            "◇",
            i18n::t(TextKey::BlockFlashcardTitle),
            i18n::t(TextKey::BlockFlashcardDesc),
            NodeKind::Flashcard,
        ),
        (
            "btn_add_excerpt",
            "❝",
            i18n::t(TextKey::BlockExcerptTitle),
            i18n::t(TextKey::BlockExcerptDesc),
            NodeKind::Excerpt,
        ),
        (
            "btn_add_checklist",
            "☑",
            i18n::t(TextKey::BlockChecklistTitle),
            i18n::t(TextKey::BlockChecklistDesc),
            NodeKind::Checklist,
        ),
        (
            "btn_add_formula",
            "∑",
            i18n::t(TextKey::BlockFormulaTitle),
            i18n::t(TextKey::BlockFormulaDesc),
            NodeKind::Formula,
        ),
        (
            "btn_add_sequence",
            "→",
            i18n::t(TextKey::BlockSequenceTitle),
            i18n::t(TextKey::BlockSequenceDesc),
            NodeKind::Sequence,
        ),
        (
            "btn_add_video",
            "▶",
            i18n::t(TextKey::BlockVideoTitle),
            i18n::t(TextKey::BlockVideoDesc),
            NodeKind::Video,
        ),
        (
            "btn_add_audio",
            "♫",
            i18n::t(TextKey::BlockAudioTitle),
            i18n::t(TextKey::BlockAudioDesc),
            NodeKind::Audio,
        ),
        (
            "btn_add_calendar",
            "▦",
            i18n::t(TextKey::BlockCalendarTitle),
            i18n::t(TextKey::BlockCalendarDesc),
            NodeKind::Calendar,
        ),
    ];

    let mut menu = col()
        .key("block_create_menu")
        .absolute(menu_x, menu_y)
        .w(232.0)
        .max_h(max_menu_h)
        .scroll(ScrollSpec { stick_bottom: false })
        .padding(6.0)
        .gap(2.0)
        .radius(10.0)
        .continuous_corners()
        .fill(theme::bg_chrome().alpha(0.98))
        .border(1.0, theme::border_default())
        .shadow(10.0, 24.0, Color::BLACK.alpha(0.55));

    for (key, glyph, name, desc, kind) in items {
        menu = menu.child(
            row()
                .key(key)
                .w_full()
                .px(8.0)
                .py(6.0)
                .gap(8.0)
                .center_y()
                .radius(6.0)
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.add_study_node(kind, None);
                })
                .child(
                    el().w(18.0).center().child(
                        text(glyph)
                            .font(theme::CONTROL_TEXT)
                            .color(theme::kind_accent(kind)),
                    ),
                )
                .child(
                    col()
                        .grow(1.0)
                        .gap(1.0)
                        .child(
                            text(name)
                                .font(theme::CONTROL_TEXT)
                                .color(theme::text_primary()),
                        )
                        .child(
                            text(desc)
                                .font(theme::BADGE_TEXT)
                                .color(theme::text_muted()),
                        ),
                ),
        );
    }

    menu
}

fn render_ai_menu_popover(
    s: &AppState,
    dock_pos: DockPosition,
    dock_x: f32,
    dock_y: f32,
    dock_w: f32,
    dock_h: f32,
) -> El {
    let menu_w = 224.0;
    let cw = s.canvas_size[0].max(240.0);
    let ch = s.canvas_size[1].max(200.0);
    let (menu_x, menu_y) = match dock_pos {
        DockPosition::Bottom => (
            (dock_x + 178.0).clamp(12.0, (cw - menu_w - 12.0).max(12.0)),
            (dock_y - 138.0).max(12.0),
        ),
        DockPosition::Top => (
            (dock_x + 178.0).clamp(12.0, (cw - menu_w - 12.0).max(12.0)),
            (dock_y + dock_h + 8.0).min((ch - 140.0).max(12.0)),
        ),
        DockPosition::Left => (
            (dock_x + dock_w + 8.0).min((cw - menu_w - 12.0).max(12.0)),
            (dock_y + 120.0).clamp(12.0, (ch - 140.0).max(12.0)),
        ),
        DockPosition::Right => (
            (dock_x - menu_w - 8.0).max(12.0),
            (dock_y + 120.0).clamp(12.0, (ch - 140.0).max(12.0)),
        ),
    };
    let can_explain_selection = s.selected_nodes.is_empty()
        || s.selected_nodes.iter().any(|id| {
            s.graph
                .get_node(id)
                .is_some_and(|n| !matches!(n.kind, NodeKind::Video | NodeKind::Audio))
        });
    col()
        .key("ai_quick_menu")
        .absolute(menu_x, menu_y)
        .w(224.0)
        .padding(6.0)
        .gap(2.0)
        .radius(10.0)
        .continuous_corners()
        .fill(theme::bg_chrome().alpha(0.98))
        .border(1.0, theme::border_default())
        .shadow(10.0, 24.0, Color::BLACK.alpha(0.55))
        .child(
            row()
                .key("btn_ai_summarize_selected")
                .w_full()
                .px(8.0)
                .py(7.0)
                .gap(8.0)
                .center_y()
                .radius(6.0)
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.show_ai_menu = false;
                    s.summarize_selected_nodes_ai();
                })
                .child(
                    text("✦")
                        .font(theme::CONTROL_TEXT)
                        .color(if can_explain_selection {
                            theme::accent_claude()
                        } else {
                            theme::text_muted()
                        }),
                )
                .child(
                    text(i18n::t(TextKey::AiSummarizeSelected))
                        .font(theme::STATUS_TEXT)
                        .color(if can_explain_selection {
                            theme::text_primary()
                        } else {
                            theme::text_muted()
                        }),
                ),
        )
        .child(
            row()
                .key("btn_run_ai_synth")
                .w_full()
                .px(8.0)
                .py(7.0)
                .gap(8.0)
                .center_y()
                .radius(6.0)
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.show_ai_menu = false;
                    s.run_ai_synthesize_connections();
                })
                .child(
                    text("✦")
                        .font(theme::CONTROL_TEXT)
                        .color(theme::accent_claude()),
                )
                .child(
                    text(i18n::t(TextKey::AiSynthesizeConnections))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_primary()),
                ),
        )
        .child(
            row()
                .key("btn_ai_generate_cards")
                .w_full()
                .px(8.0)
                .py(7.0)
                .gap(8.0)
                .center_y()
                .radius(6.0)
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.show_ai_menu = false;
                    s.generate_study_cards_ai();
                })
                .child(
                    text("◇")
                        .font(theme::CONTROL_TEXT)
                        .color(theme::text_secondary()),
                )
                .child(
                    text(i18n::t(TextKey::AiGenerateStudyCards))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_primary()),
                ),
        )
}

fn render_edge_pill(edge: &Edge, is_selected: bool, screen_pos: ScreenPos, zoom: f32) -> El {
    let edge_id_clone = edge.id.clone();
    let edge_id_for_accept = edge.id.clone();
    let edge_id_for_reject = edge.id.clone();

    let is_ghost = edge.status == EdgeStatus::Proposed;
    // Opaque matte surfaces so neither the wire nor card borders bleed through underneath.
    let pill_bg = if is_ghost {
        theme::hex("#1C1715")
    } else if is_selected {
        theme::bg_control()
    } else {
        theme::bg_chrome()
    };

    let pill_border = if is_ghost {
        theme::accent_claude().alpha(0.65)
    } else if is_selected {
        theme::border_strong()
    } else {
        theme::border_default()
    };

    let font_size = (10.0 * zoom).clamp(7.5, 12.0);
    let rel_lbl = i18n::edge_relation_label(&edge.relation);
    let label_str = if edge.label.is_empty() {
        format!("[{}]", rel_lbl)
    } else {
        format!("[{}] {}", rel_lbl, edge.label)
    };

    let char_w = font_size * 0.60;
    let action_w = if is_ghost { 46.0 * zoom } else { 0.0 };
    let est_w = (label_str.chars().count() as f32 * char_w + 18.0 * zoom + action_w).max(52.0 * zoom);
    let est_h = font_size + 9.0 * zoom;

    let mut content = row()
        .center_y()
        .gap(4.0 * zoom)
        .child(
            text(label_str)
                .font(TextStyle::new(font_size, 500.0).mono())
                .color(if is_ghost {
                    theme::accent_claude()
                } else if is_selected {
                    theme::text_primary()
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
                    .radius(3.0)
                    .fill(theme::accent_primary().alpha(0.14))
                    .border(1.0, theme::border_strong())
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
                            .font(TextStyle::new(font_size, 700.0).mono())
                            .color(theme::text_primary()),
                    ),
            )
            .child(
                el()
                    .px(5.0 * zoom)
                    .py(1.5 * zoom)
                    .radius(3.0)
                    .fill(theme::accent_danger().alpha(0.16))
                    .border(1.0, theme::accent_danger().alpha(0.42))
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
                            .font(TextStyle::new(font_size, 700.0).mono())
                            .color(theme::accent_danger()),
                    ),
            );
    }

    el()
        .key(format!("edge_pill_{}", edge.id.as_str()))
        .absolute(screen_pos.x - est_w * 0.5, screen_pos.y - est_h * 0.5)
        .px(8.0 * zoom)
        .py(3.5 * zoom)
        .radius(4.0)
        .fill(pill_bg)
        .hover(theme::hover_subtle())
        .border(1.0, pill_border)
        .shadow(3.0, 10.0, Color::BLACK.alpha(0.45))
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut AppState| {
            s.select_edge(edge_id_clone.clone());
        })
        .child(content)
        .cache()
}

fn render_card_timestamps_strip(node: &Node, badge_size: f32, zoom: f32) -> Option<El> {
    if node.meta.timestamps.is_empty() {
        return None;
    }
    let mut strip = col().w_full().gap(3.0 * zoom);
    for (idx, ts) in node.meta.timestamps.iter().take(3).enumerate() {
        let nid = node.id.clone();
        let secs = ts.seconds;
        let label = format!("{} · {}", media_helper::format_timestamp(secs), ts.label);
        strip = strip.child(
            el()
                .key(format!("media_ts_{}_{}", node.id.as_str(), idx))
                .w_full()
                .px(6.0 * zoom)
                .py(2.5 * zoom)
                .radius(4.0 * zoom)
                .fill(theme::bg_control())
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                    s.jump_to_media_timestamp(&nid, secs, tx);
                })
                .child(
                    text(label)
                        .font(TextStyle::new(badge_size, 500.0).mono())
                        .color(theme::kind_accent(node.kind))
                        .truncate(),
                ),
        );
    }
    Some(strip)
}

fn render_iframe_embed_slot(
    s: &AppState,
    node: &Node,
    url: &str,
    frame_h: f32,
    badge_size: f32,
    zoom: f32,
    accent: Color,
) -> El {
    let plat = media_helper::classify_media_url(url);
    let collapsed = s.embed_collapsed.contains(&node.id);
    let nid_rect = node.id.clone();
    let nid_reload = node.id.clone();
    let nid_toggle = node.id.clone();
    let nid_open = node.id.clone();

    let mut container = col().w_full().grow(1.0).min_h(0.0).gap(5.0 * zoom);
    if !collapsed {
        container = container.child(
            col()
                .key(format!("embed_frame_{}", node.id.as_str()))
                .w_full()
                .grow(1.0)
                .min_h((64.0 * zoom).min(frame_h * zoom))
                .padding(8.0 * zoom)
                .radius(5.0 * zoom)
                .fill(Color::BLACK.alpha(0.92))
                .border(1.0, theme::border_subtle())
                .on_drawn_layout_tx(move |s: &mut AppState, r, tx| {
                    s.embed_rects.insert(nid_rect.clone(), r);
                    tx.live_only();
                })
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .gap(6.0 * zoom)
                        .child(
                            text(plat.label())
                                .font(TextStyle::new(badge_size, 600.0).mono())
                                .color(accent),
                        )
                        .child(
                            text(i18n::t(TextKey::MediaIframeActive))
                                .font(TextStyle::new(badge_size, 500.0))
                                .color(theme::text_muted()),
                        ),
                )
                .child(el().grow(1.0).center().child(
                    text(url)
                        .font(TextStyle::new((badge_size * 0.92).max(7.5), 400.0))
                        .color(theme::text_muted().alpha(0.65))
                        .truncate(),
                )),
        );
    }

    container.child(
        row()
            .w_full()
            .center_y()
            .gap(4.0 * zoom)
            .pr(16.0 * zoom)
            .child(
                el()
                    .key(format!("media_reload_embed_{}", node.id.as_str()))
                    .px(5.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .border(1.0, theme::border_subtle())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.reload_embed_iframe(&nid_reload);
                    })
                    .child(
                        text("↻")
                            .font(TextStyle::new(badge_size, 600.0))
                            .color(theme::text_secondary()),
                    ),
            )
            .child(
                el()
                    .key(format!("media_toggle_embed_{}", node.id.as_str()))
                    .px(6.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .border(1.0, theme::border_subtle())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.toggle_embed_collapsed(&nid_toggle);
                    })
                    .child(
                        text(if collapsed {
                            i18n::t(TextKey::MediaShowEmbed)
                        } else {
                            i18n::t(TextKey::MediaHideEmbed)
                        })
                        .font(TextStyle::new((badge_size * 0.92).max(7.5), 500.0))
                        .color(theme::text_secondary()),
                    ),
            )
            .child(el().grow(1.0))
            .child(
                el()
                    .key(format!("media_open_url_{}", node.id.as_str()))
                    .px(6.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .border(1.0, theme::border_subtle())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                        s.open_media_external_url(&nid_open, 0.0, tx);
                    })
                    .child(
                        text(i18n::t(TextKey::MediaOpenLink))
                            .font(TextStyle::new((badge_size * 0.94).max(7.5), 500.0))
                            .color(theme::text_secondary()),
                    ),
            ),
    )
}

fn render_audio_card_subtitle_box(
    s: &AppState,
    node: &Node,
    current_secs: Option<f64>,
    badge_size: f32,
    zoom: f32,
    accent: Color,
) -> Option<El> {
    if !node.meta.show_subtitles_on_card {
        return None;
    }
    let is_pending = s.audio_subtitle_pending.contains(&node.id);
    let err_opt = s.audio_subtitle_errors.get(&node.id).cloned();
    let plain_text = node
        .meta
        .transcript_text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| media_helper::cues_to_plain_text(&node.meta.subtitles));
    if !is_pending && err_opt.is_none() && plain_text.is_empty() && node.meta.subtitles.is_empty() {
        return None;
    }

    let copy_key: &'static str = "audio_sub_card";
    let was_copied = s.is_recently_copied(copy_key);
    let copy_payload = if !plain_text.is_empty() {
        plain_text.clone()
    } else {
        media_helper::cues_to_plain_text(&node.meta.subtitles)
    };

    let (cue_time_lbl, active_line) = if is_pending {
        (None, i18n::t(TextKey::AudioGeneratingSubtitles).to_string())
    } else if let Some(ref e) = err_opt {
        (None, e.clone())
    } else if let Some((_, cue)) =
        media_helper::active_subtitle_cue(&node.meta.subtitles, current_secs.unwrap_or(0.0))
    {
        (
            Some(media_helper::format_timestamp(cue.start)),
            cue.text.clone(),
        )
    } else {
        (None, plain_text.clone())
    };

    let mut hdr = row()
        .w_full()
        .center_y()
        .gap(5.0 * zoom)
        .child(
            text(match cue_time_lbl {
                Some(ref ts) => format!("CC · {}", ts),
                None => "CC".to_string(),
            })
            .font(TextStyle::new((badge_size * 0.88).max(7.0), 700.0).mono())
            .color(accent),
        )
        .child(el().grow(1.0));

    if !copy_payload.is_empty() {
        hdr = hdr.child(
            el()
                .key(format!("media_sub_copy_{}", node.id.as_str()))
                .px(5.0 * zoom)
                .py(1.5 * zoom)
                .radius(3.0 * zoom)
                .fill(theme::bg_control())
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                    tx.copy(copy_payload.clone());
                    s.mark_copied(copy_key, "Copied audio subtitles");
                })
                .child(
                    text(if was_copied {
                        i18n::t(TextKey::ReaderCopied)
                    } else {
                        i18n::t(TextKey::ReaderCopy)
                    })
                    .font(TextStyle::new((badge_size * 0.88).max(7.0), 600.0))
                    .color(if was_copied {
                        accent
                    } else {
                        theme::text_secondary()
                    }),
                ),
        );
    }

    Some(
        col()
            .key(format!("media_sub_box_{}", node.id.as_str()))
            .w_full()
            .px(7.0 * zoom)
            .py(5.0 * zoom)
            .gap(3.0 * zoom)
            .radius(4.0 * zoom)
            .fill(theme::bg_input())
            .border(1.0, theme::border_subtle())
            .child(hdr)
            .child(
                text(active_line)
                    .font(TextStyle::new((badge_size * 0.96).max(8.0), 400.0))
                    .color(if err_opt.is_some() {
                        theme::accent_danger()
                    } else {
                        theme::text_primary()
                    })
                    .w_full()
                    .wrap(),
            ),
    )
}

fn render_video_card_body(
    s: &AppState,
    node: &Node,
    body_size: f32,
    badge_size: f32,
    zoom: f32,
) -> El {
    let click_id = node.id.clone();
    let pick_id = node.id.clone();
    let play_id = node.id.clone();
    let video_tap_id = node.id.clone();
    let back_id = node.id.clone();
    let fwd_id = node.id.clone();
    let mute_id = node.id.clone();
    let rate_id = node.id.clone();
    let mark_id = node.id.clone();
    let scrub_id = node.id.clone();
    let accent = theme::kind_accent(NodeKind::Video);
    let (node_w, _) = crate::state::node_world_size(node);
    let track_w = (node_w * zoom - 20.0 * zoom).max(60.0);

    let mut body = col()
        .key(format!("node_body_{}", node.id.as_str()))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .clip_soft(0.0)
        .px(8.0 * zoom)
        .py(6.0 * zoom)
        .gap(4.0 * zoom)
        .on_press(move |s: &mut AppState| {
            s.select_node(click_id.clone(), false);
            if !s.inspector_popped_out {
                s.inspector_open = true;
            }
            s.ai_panel_open = false;
        });

    if let Some(player) = s.media_player_for(&node.id) {
        let playing = player.is_playing();
        let muted = player.muted();
        let rate = player.rate();
        let dur = player.duration().unwrap_or(0.0);
        let cur = player.time();
        let frac = if dur > 0.0 {
            (cur / dur).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let time_label = if dur > 0.0 {
            format!(
                "{} / {}",
                media_helper::format_timestamp(cur),
                media_helper::format_timestamp(dur)
            )
        } else {
            media_helper::format_timestamp(cur)
        };
        let player_err = match player.state() {
            rune::PlayerState::Failed(msg) => Some(msg),
            _ => None,
        };

        body = body.child(
            el()
                .w_full()
                .grow(1.0)
                .min_h(76.0 * zoom)
                .radius(5.0 * zoom)
                .fill(Color::BLACK)
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.toggle_media_play(&video_tap_id);
                })
                .video(player, SurfaceFit::Fit),
        );

        if let Some(err) = player_err {
            let jump_fail = node.id.clone();
            let retry_fail = node.id.clone();
            body = body.child(
                row()
                    .w_full()
                    .center_y()
                    .gap(4.0 * zoom)
                    .child(
                        text(err)
                            .font(TextStyle::new((badge_size * 0.9).max(7.5), 400.0))
                            .color(theme::accent_danger())
                            .grow(1.0)
                            .truncate(),
                    )
                    .child(
                        el()
                            .key(format!("media_retry_{}", node.id.as_str()))
                            .px(5.0 * zoom)
                            .py(2.0 * zoom)
                            .radius(4.0 * zoom)
                            .fill(theme::bg_control())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.media_players.remove(&retry_fail);
                                let _ = s.ensure_media_player(&retry_fail);
                            })
                            .child(
                                text("↻")
                                    .font(TextStyle::new(badge_size, 600.0))
                                    .color(theme::text_primary()),
                            ),
                    )
                    .child(
                        el()
                            .key(format!("media_open_url_{}", node.id.as_str()))
                            .px(5.0 * zoom)
                            .py(2.0 * zoom)
                            .radius(4.0 * zoom)
                            .fill(theme::bg_control())
                            .border(1.0, theme::border_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                                s.open_media_external_url(&jump_fail, 0.0, tx);
                            })
                            .child(
                                text("↗")
                                    .font(TextStyle::new(badge_size, 600.0))
                                    .color(theme::text_secondary()),
                            ),
                    ),
            );
        }

        // Scrub timeline bar
        body = body.child(
            el()
                .key(format!("media_scrub_{}", node.id.as_str()))
                .w(track_w)
                .h((5.0 * zoom).clamp(3.5, 7.5))
                .radius(3.0)
                .fill(theme::border_subtle())
                .cursor(Cursor::Pointer)
                .on_drag(move |s: &mut AppState, d, tx| match d.phase {
                    Phase::Down | Phase::Move => {
                        let ratio = (d.x / d.w.max(1.0)).clamp(0.0, 1.0) as f64;
                        if let Some(dur_val) = s.media_player_for(&scrub_id).and_then(|p| p.duration()) {
                            if dur_val > 0.0 {
                                s.seek_media(&scrub_id, ratio * dur_val);
                                tx.instant();
                            }
                        }
                    }
                    Phase::Up => {}
                })
                .child(
                    el()
                        .w((track_w * frac).clamp(0.0, track_w))
                        .h_full()
                        .radius(3.0)
                        .fill(accent),
                ),
        );

        let mut controls = row()
            .w_full()
            .center_y()
            .gap(4.0 * zoom)
            .pr(16.0 * zoom)
            .child(
                el()
                    .key(format!("media_play_{}", node.id.as_str()))
                    .px(6.0 * zoom)
                    .py(2.5 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .border(1.0, theme::border_default())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.toggle_media_play(&play_id);
                    })
                    .child(
                        text(if playing { "⏸" } else { "▶" })
                            .font(TextStyle::new(badge_size, 600.0))
                            .color(accent),
                    ),
            )
            .child(
                el()
                    .key(format!("media_back_{}", node.id.as_str()))
                    .px(4.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        let t = s
                            .media_player_for(&back_id)
                            .map(|p| (p.time() - 10.0).max(0.0))
                            .unwrap_or(0.0);
                        s.seek_media(&back_id, t);
                    })
                    .child(
                        text("-10")
                            .font(TextStyle::new((badge_size * 0.9).max(7.5), 500.0).mono())
                            .color(theme::text_secondary()),
                    ),
            )
            .child(
                el()
                    .key(format!("media_fwd_{}", node.id.as_str()))
                    .px(4.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        let t = s
                            .media_player_for(&fwd_id)
                            .map(|p| p.time() + 10.0)
                            .unwrap_or(10.0);
                        s.seek_media(&fwd_id, t);
                    })
                    .child(
                        text("+10")
                            .font(TextStyle::new((badge_size * 0.9).max(7.5), 500.0).mono())
                            .color(theme::text_secondary()),
                    ),
            )
            .child(
                text(time_label)
                    .font(TextStyle::new((badge_size * 0.92).max(7.5), 500.0).mono())
                    .color(theme::text_secondary()),
            )
            .child(el().grow(1.0))
            .child(
                el()
                    .key(format!("media_mute_{}", node.id.as_str()))
                    .px(4.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.toggle_media_mute(&mute_id);
                    })
                    .child(
                        text(if muted { "🔇" } else { "🔊" })
                            .font(TextStyle::new(badge_size, 500.0))
                            .color(theme::text_secondary()),
                    ),
            )
            .child(
                el()
                    .key(format!("media_rate_{}", node.id.as_str()))
                    .px(4.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.cycle_media_rate(&rate_id);
                    })
                    .child(
                        text(format!("{:.2}x", rate))
                            .font(TextStyle::new((badge_size * 0.9).max(7.5), 500.0).mono())
                            .color(theme::text_secondary()),
                    ),
            )
            .child(
                el()
                    .key(format!("media_mark_{}", node.id.as_str()))
                    .px(5.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .border(1.0, theme::border_subtle())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.mark_current_media_timestamp(&mark_id);
                    })
                    .child(
                        text("+ TS")
                            .font(TextStyle::new(badge_size, 600.0).mono())
                            .color(theme::text_primary()),
                    ),
            );

        if node
            .meta
            .media_url
            .as_deref()
            .is_some_and(|u| !u.trim().is_empty())
        {
            let ext_id = node.id.clone();
            controls = controls.child(
                el()
                    .key(format!("media_open_url_{}", node.id.as_str()))
                    .px(4.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .border(1.0, theme::border_subtle())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                        s.jump_to_media_timestamp(&ext_id, 0.0, tx);
                    })
                    .child(
                        text("↗")
                            .font(TextStyle::new(badge_size, 600.0))
                            .color(theme::text_secondary()),
                    ),
            );
        }
        body = body.child(controls);
    } else if let Some(url) = node
        .meta
        .media_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
    {
        if AppState::media_uses_iframe(url) {
            body = body.child(render_iframe_embed_slot(
                s,
                node,
                url,
                148.0,
                badge_size,
                zoom,
                accent,
            ));
        } else {
            let plat = media_helper::classify_media_url(url);
            let jump_id = node.id.clone();
            let frame = col()
                .w_full()
                .grow(1.0)
                .min_h(80.0 * zoom)
                .padding(8.0 * zoom)
                .gap(5.0 * zoom)
                .radius(5.0 * zoom)
                .fill(theme::bg_input())
                .border(1.0, theme::border_subtle())
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .gap(6.0 * zoom)
                        .child(
                            text(plat.label())
                                .font(TextStyle::new(badge_size, 600.0).mono())
                                .color(accent),
                        )
                        .child(el().grow(1.0))
                        .child(
                            text(url)
                                .font(TextStyle::new(badge_size, 400.0))
                                .color(theme::text_muted())
                                .truncate(),
                        ),
                )
                .child(el().grow(1.0))
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .pr(12.0 * zoom)
                        .child(el().grow(1.0))
                        .child(
                            el()
                                .key(format!("media_open_url_{}", node.id.as_str()))
                                .px(7.0 * zoom)
                                .py(3.0 * zoom)
                                .radius(4.0 * zoom)
                                .fill(theme::bg_control())
                                .border(1.0, theme::border_subtle())
                                .hover(theme::hover_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                                    s.open_media_external_url(&jump_id, 0.0, tx);
                                })
                                .child(
                                    text(i18n::t(TextKey::MediaOpenLink))
                                        .font(TextStyle::new(badge_size, 500.0))
                                        .color(theme::text_primary()),
                                ),
                        ),
                );
            body = body.child(frame);
        }
    } else {
        body = body.child(
            col()
                .w_full()
                .grow(1.0)
                .center()
                .gap(6.0 * zoom)
                .child(
                    el()
                        .key(format!("btn_card_ingest_media_{}", node.id.as_str()))
                        .px(9.0 * zoom)
                        .py(5.0 * zoom)
                        .radius(5.0 * zoom)
                        .fill(theme::bg_control())
                        .border(1.0, theme::border_default())
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press_tx(move |s_ref: &mut AppState, tx: &mut Tx| {
                            s_ref.select_node(pick_id.clone(), false);
                            let opts = FileDialogOptions {
                                title: i18n::t(TextKey::MediaIngestFile).to_string(),
                                ..Default::default()
                            }
                            .filter(
                                "Media",
                                &["mp4", "mov", "m4v", "webm", "mkv", "mp3", "wav", "m4a", "flac", "ogg"],
                            );
                            let nid = pick_id.clone();
                            tx.open_file(opts, move |s: &mut AppState, res, _| {
                                if let Ok(Some(path)) = res {
                                    s.ingest_media_into_node(&nid, &path);
                                }
                            });
                        })
                        .child(
                            text(format!("+ {}", i18n::t(TextKey::MediaIngestFile)))
                                .font(TextStyle::new(body_size.clamp(9.0, 13.0), 600.0))
                                .color(theme::text_primary()),
                        ),
                ),
        );
    }

    if let Some(strip) = render_card_timestamps_strip(node, badge_size, zoom) {
        body = body.child(strip);
    }
    body
}

fn render_audio_card_body(
    s: &AppState,
    node: &Node,
    body_size: f32,
    badge_size: f32,
    zoom: f32,
) -> El {
    let click_id = node.id.clone();
    let pick_id = node.id.clone();
    let play_id = node.id.clone();
    let mark_id = node.id.clone();
    let scrub_id = node.id.clone();
    let cc_toggle_id = node.id.clone();
    let cc_auto_id = node.id.clone();
    let accent = theme::kind_accent(NodeKind::Audio);
    let (node_w, _) = crate::state::node_world_size(node);
    let track_w = (node_w * zoom - 28.0 * zoom).max(60.0);
    let has_subtitles = !node.meta.subtitles.is_empty()
        || node
            .meta
            .transcript_text
            .as_deref()
            .is_some_and(|t| !t.trim().is_empty());
    let mut current_secs: Option<f64> = None;

    let mut body = col()
        .key(format!("node_body_{}", node.id.as_str()))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .clip_soft(0.0)
        .px(9.0 * zoom)
        .py(7.0 * zoom)
        .gap(6.0 * zoom)
        .on_press(move |s: &mut AppState| {
            s.select_node(click_id.clone(), false);
            if !s.inspector_popped_out {
                s.inspector_open = true;
            }
            s.ai_panel_open = false;
        });

    if let Some(player) = s.media_player_for(&node.id) {
        let playing = player.is_playing();
        let dur = player.duration().unwrap_or(0.0);
        let cur = player.time();
        current_secs = Some(cur);
        let frac = if dur > 0.0 {
            (cur / dur).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let time_label = if dur > 0.0 {
            format!(
                "{} / {}",
                media_helper::format_timestamp(cur),
                media_helper::format_timestamp(dur)
            )
        } else {
            media_helper::format_timestamp(cur)
        };
        let mut ctrl_row = row()
            .w_full()
            .center_y()
            .gap(6.0 * zoom)
            .pr(14.0 * zoom)
            .child(
                el()
                    .key(format!("media_play_{}", node.id.as_str()))
                    .px(7.0 * zoom)
                    .py(3.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .border(1.0, theme::border_default())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.toggle_media_play(&play_id);
                    })
                    .child(
                        text(if playing { "⏸" } else { "▶" })
                            .font(TextStyle::new(badge_size, 600.0))
                            .color(accent),
                    ),
            )
            .child(
                text(time_label)
                    .font(TextStyle::new((badge_size * 0.94).max(7.5), 500.0).mono())
                    .color(theme::text_secondary()),
            )
            .child(el().grow(1.0));
        if has_subtitles {
            ctrl_row = ctrl_row.child(
                el()
                    .key(format!("media_cc_toggle_{}", node.id.as_str()))
                    .px(5.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(if node.meta.show_subtitles_on_card {
                        accent.alpha(0.18)
                    } else {
                        theme::bg_control()
                    })
                    .border(
                        1.0,
                        if node.meta.show_subtitles_on_card {
                            accent.alpha(0.55)
                        } else {
                            theme::border_subtle()
                        },
                    )
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.toggle_audio_subtitles_on_card(&cc_toggle_id);
                    })
                    .child(
                        text("CC")
                            .font(TextStyle::new((badge_size * 0.9).max(7.5), 600.0).mono())
                            .color(if node.meta.show_subtitles_on_card {
                                accent
                            } else {
                                theme::text_muted()
                            }),
                    ),
            );
        } else {
            let sub_pending = s.audio_subtitle_pending.contains(&node.id);
            ctrl_row = ctrl_row.child(
                el()
                    .key(format!("media_auto_sub_{}", node.id.as_str()))
                    .px(5.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_control())
                    .border(1.0, theme::border_subtle())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.generate_audio_subtitles(&cc_auto_id);
                    })
                    .child(
                        text(if sub_pending { "…" } else { "✦ CC" })
                            .font(TextStyle::new((badge_size * 0.88).max(7.0), 500.0).mono())
                            .color(theme::text_secondary()),
                    ),
            );
        }
        ctrl_row = ctrl_row.child(
            el()
                .key(format!("media_mark_{}", node.id.as_str()))
                .px(5.0 * zoom)
                .py(2.0 * zoom)
                .radius(4.0 * zoom)
                .fill(theme::bg_control())
                .border(1.0, theme::border_subtle())
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.mark_current_media_timestamp(&mark_id);
                })
                .child(
                    text("+ TS")
                        .font(TextStyle::new(badge_size, 600.0).mono())
                        .color(theme::text_primary()),
                ),
        );
        body = body.child(ctrl_row).child(
            el()
                .key(format!("media_scrub_{}", node.id.as_str()))
                .w(track_w)
                .h((4.5 * zoom).clamp(3.0, 6.5))
                .radius(3.0)
                .fill(theme::border_subtle())
                .cursor(Cursor::Pointer)
                .on_drag(move |s: &mut AppState, d, tx| match d.phase {
                    Phase::Down | Phase::Move => {
                        let ratio = (d.x / d.w.max(1.0)).clamp(0.0, 1.0) as f64;
                        if let Some(dur_val) = s.media_player_for(&scrub_id).and_then(|p| p.duration()) {
                            if dur_val > 0.0 {
                                s.seek_media(&scrub_id, ratio * dur_val);
                                tx.instant();
                            }
                        }
                    }
                    Phase::Up => {}
                })
                .child(
                    el()
                        .w((track_w * frac).clamp(0.0, track_w))
                        .h_full()
                        .radius(3.0)
                        .fill(accent),
                ),
        );
    } else if let Some(url) = node
        .meta
        .media_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
    {
        if AppState::media_uses_iframe(url) {
            body = body.child(render_iframe_embed_slot(
                s,
                node,
                url,
                84.0,
                badge_size,
                zoom,
                accent,
            ));
        } else {
            let plat = media_helper::classify_media_url(url);
            let jump_id = node.id.clone();
            body = body.child(
                row()
                    .w_full()
                    .center_y()
                    .gap(6.0 * zoom)
                    .padding(6.0 * zoom)
                    .pr(16.0 * zoom)
                    .radius(4.0 * zoom)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_subtle())
                    .child(
                        text(plat.label())
                            .font(TextStyle::new(badge_size, 600.0).mono())
                            .color(accent),
                    )
                    .child(el().grow(1.0))
                    .child(
                        el()
                            .key(format!("media_open_url_{}", node.id.as_str()))
                            .px(7.0 * zoom)
                            .py(2.5 * zoom)
                            .radius(4.0 * zoom)
                            .fill(theme::bg_control())
                            .border(1.0, theme::border_subtle())
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                                s.open_media_external_url(&jump_id, 0.0, tx);
                            })
                            .child(
                                text(i18n::t(TextKey::MediaOpenLink))
                                    .font(TextStyle::new(badge_size, 500.0))
                                    .color(theme::text_primary()),
                            ),
                    ),
            );
        }
    } else {
        body = body.child(
            col()
                .w_full()
                .grow(1.0)
                .center()
                .child(
                    el()
                        .key(format!("btn_card_ingest_media_{}", node.id.as_str()))
                        .px(9.0 * zoom)
                        .py(4.5 * zoom)
                        .radius(5.0 * zoom)
                        .fill(theme::bg_control())
                        .border(1.0, theme::border_default())
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press_tx(move |s_ref: &mut AppState, tx: &mut Tx| {
                            s_ref.select_node(pick_id.clone(), false);
                            let opts = FileDialogOptions {
                                title: i18n::t(TextKey::MediaIngestFile).to_string(),
                                ..Default::default()
                            }
                            .filter("Audio", &["mp3", "wav", "m4a", "flac", "ogg", "aac", "opus"]);
                            let nid = pick_id.clone();
                            tx.open_file(opts, move |s: &mut AppState, res, _| {
                                if let Ok(Some(path)) = res {
                                    s.ingest_media_into_node(&nid, &path);
                                }
                            });
                        })
                        .child(
                            text(format!("+ {}", i18n::t(TextKey::MediaIngestFile)))
                                .font(TextStyle::new(body_size.clamp(9.0, 12.5), 600.0))
                                .color(theme::text_primary()),
                        ),
                ),
        );
    }

    if let Some(sub_box) =
        render_audio_card_subtitle_box(s, node, current_secs, badge_size, zoom, accent)
    {
        body = body.child(sub_box);
    }

    if let Some(strip) = render_card_timestamps_strip(node, badge_size, zoom) {
        body = body.child(strip);
    }
    body
}

fn render_drawing_card_body(
    _s: &AppState,
    node: &Node,
    _body_size: f32,
    badge_size: f32,
    zoom: f32,
) -> El {
    let (node_w, node_h) = crate::state::node_world_size(node);
    let has_ocr = node
        .meta
        .extracted_text
        .as_deref()
        .is_some_and(|t| !t.trim().is_empty());
    let ocr_reserve = if has_ocr { 24.0 } else { 0.0 };
    let pad_w = ((node_w - 16.0) * zoom).max(96.0 * zoom);
    let pad_h = ((node_h - 48.0 - ocr_reserve) * zoom).max(96.0 * zoom);

    let shape = build_drawing_shape(&node.meta.drawing_strokes, pad_w, pad_h, zoom);
    let select_id = node.id.clone();

    let mut pad = el()
        .key(format!("drawing_pad_{}", node.id.as_str()))
        .w(pad_w)
        .h(pad_h)
        .radius(5.0 * zoom)
        .fill(theme::bg_input())
        .border(1.0, theme::border_subtle())
        .child(canvas(shape).absolute(0.0, 0.0).w_full().h_full());

    if node.meta.drawing_strokes.is_empty() {
        pad = pad.child(
            el().absolute(0.0, 0.0).w_full().h_full().center().child(
                text(i18n::t(TextKey::DrawingEmptyHint))
                    .font(TextStyle::new(badge_size, 400.0))
                    .color(theme::text_muted()),
            ),
        );
    }

    let mut body = col()
        .key(format!("node_body_{}", node.id.as_str()))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .clip_soft(0.0)
        .px(8.0 * zoom)
        .py(6.0 * zoom)
        .gap(4.0 * zoom)
        .on_press(move |s: &mut AppState| {
            s.select_node(select_id.clone(), false);
            if !s.inspector_popped_out {
                s.inspector_open = true;
            }
            s.ai_panel_open = false;
        })
        .child(pad);

    if let Some(txt) = node
        .meta
        .extracted_text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        body = body.child(
            el().w_full()
                .px(6.0 * zoom)
                .py(2.0 * zoom)
                .radius(4.0 * zoom)
                .fill(theme::bg_input())
                .border(1.0, theme::border_subtle())
                .child(
                    text(txt)
                        .font(TextStyle::new((badge_size * 0.92).max(7.5), 500.0))
                        .color(theme::text_primary())
                        .truncate(),
                ),
        );
    }

    body
}

fn render_calendar_card_body(
    s: &AppState,
    node: &Node,
    body_size: f32,
    badge_size: f32,
    zoom: f32,
) -> El {
    let click_id = node.id.clone();
    let prev_id = node.id.clone();
    let next_id = node.id.clone();
    let wk_prev_id = node.id.clone();
    let wk_next_id = node.id.clone();
    let mode_m_id = node.id.clone();
    let mode_w_id = node.id.clone();
    let accent = theme::kind_accent(NodeKind::Calendar);
    let (today_y, today_m, today_d) = media_helper::today_ymd();
    let today_str = media_helper::format_ymd(today_y, today_m, today_d);
    let cy = node.meta.calendar_year.unwrap_or(today_y);
    let cm = node.meta.calendar_month.unwrap_or(today_m);
    let selected_date = s
        .selected_calendar_date
        .get(&node.id)
        .cloned()
        .unwrap_or_else(|| today_str.clone());
    let mode = node.meta.calendar_view_mode;

    let mut body = col()
        .key(format!("node_body_{}", node.id.as_str()))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .clip_soft(0.0)
        .px(8.0 * zoom)
        .py(6.0 * zoom)
        .gap(5.0 * zoom)
        .on_press(move |s: &mut AppState| {
            s.select_node(click_id.clone(), false);
            if !s.inspector_popped_out {
                s.inspector_open = true;
            }
            s.ai_panel_open = false;
        });

    let mode_segmented = row()
        .center_y()
        .padding(1.5 * zoom)
        .gap(2.0 * zoom)
        .radius(4.0 * zoom)
        .fill(theme::bg_input())
        .border(1.0, theme::border_subtle())
        .child(
            el()
                .key(format!("cal_mode_month_{}", node.id.as_str()))
                .px(6.0 * zoom)
                .py(1.5 * zoom)
                .radius(3.0 * zoom)
                .fill(if mode == CalendarViewMode::Month {
                    theme::bg_control()
                } else {
                    Color::TRANSPARENT
                })
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.set_calendar_view_mode(&mode_m_id, CalendarViewMode::Month);
                })
                .child(
                    text(i18n::t(TextKey::CalModeMonth))
                        .font(TextStyle::new((badge_size * 0.9).max(7.5), 600.0))
                        .color(if mode == CalendarViewMode::Month {
                            theme::text_primary()
                        } else {
                            theme::text_muted()
                        }),
                ),
        )
        .child(
            el()
                .key(format!("cal_mode_week_{}", node.id.as_str()))
                .px(6.0 * zoom)
                .py(1.5 * zoom)
                .radius(3.0 * zoom)
                .fill(if mode == CalendarViewMode::Week {
                    theme::bg_control()
                } else {
                    Color::TRANSPARENT
                })
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.set_calendar_view_mode(&mode_w_id, CalendarViewMode::Week);
                })
                .child(
                    text(i18n::t(TextKey::CalModeWeek))
                        .font(TextStyle::new((badge_size * 0.9).max(7.5), 600.0))
                        .color(if mode == CalendarViewMode::Week {
                            theme::text_primary()
                        } else {
                            theme::text_muted()
                        }),
                ),
        );

    if mode == CalendarViewMode::Week {
        let anchor_mon = node
            .meta
            .calendar_week_start
            .clone()
            .unwrap_or_else(|| media_helper::monday_of_week(&selected_date));
        let week = media_helper::week_dates(&anchor_mon);
        let (sy, sm, sd) = media_helper::parse_ymd(&week[0].0).unwrap_or((cy, cm, 1));
        let (ey, em, ed) = media_helper::parse_ymd(&week[6].0).unwrap_or((sy, sm, sd));
        let range_label = if sy == ey && sm == em {
            format!("{} {} – {}", i18n::month_short(sm), sd, ed)
        } else {
            format!(
                "{} {} – {} {}",
                i18n::month_short(sm),
                sd,
                i18n::month_short(em),
                ed
            )
        };

        // Combined single top bar: week range nav + segmented Month | Week switch
        body = body.child(
            row()
                .w_full()
                .center_y()
                .gap(4.0 * zoom)
                .child(
                    el()
                        .key(format!("cal_week_prev_{}", node.id.as_str()))
                        .px(5.0 * zoom)
                        .py(1.5 * zoom)
                        .radius(4.0)
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.calendar_step_week(&wk_prev_id, -1);
                        })
                        .child(
                            text("‹")
                                .font(TextStyle::new(body_size, 600.0))
                                .color(theme::text_secondary()),
                        ),
                )
                .child(
                    text(range_label)
                        .font(TextStyle::new(badge_size.max(8.5), 600.0))
                        .color(theme::text_primary()),
                )
                .child(
                    el()
                        .key(format!("cal_week_next_{}", node.id.as_str()))
                        .px(5.0 * zoom)
                        .py(1.5 * zoom)
                        .radius(4.0)
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.calendar_step_week(&wk_next_id, 1);
                        })
                        .child(
                            text("›")
                                .font(TextStyle::new(body_size, 600.0))
                                .color(theme::text_secondary()),
                        ),
                )
                .child(el().grow(1.0))
                .child(mode_segmented),
        );

        let mut week_row = row().w_full().grow(1.0).min_h(0.0).gap(5.0 * zoom);
        for (wd_idx, (date_str, _wd_name, day_num)) in week.into_iter().enumerate() {
            let is_sel = selected_date == date_str;
            let is_today = today_str == date_str;
            let day_note = node
                .meta
                .calendar_day_notes
                .get(&date_str)
                .map(String::as_str)
                .unwrap_or("")
                .trim();
            let day_events: Vec<(usize, &studynodes_graph::model::CalendarEvent)> = node
                .meta
                .calendar_events
                .iter()
                .enumerate()
                .filter(|(_, e)| e.date == date_str)
                .collect();
            let nid_pick = node.id.clone();
            let date_pick = date_str.clone();
            let focus_key = format!("cal_card_note_input_{}", node.id.as_str());

            let mut day_col = col()
                .key(format!("cal_week_day_{}_{}", node.id.as_str(), date_str))
                .grow(1.0)
                .h_full()
                .min_h(0.0)
                .clip_soft(0.0)
                .padding(5.0 * zoom)
                .gap(3.0 * zoom)
                .radius(5.0 * zoom)
                .fill(if is_sel {
                    theme::bg_control()
                } else {
                    theme::bg_input()
                })
                .border(
                    1.0,
                    if is_sel {
                        accent
                    } else if is_today {
                        theme::border_default()
                    } else {
                        theme::border_subtle()
                    },
                )
                .cursor(Cursor::Pointer)
                .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                    s.select_node(nid_pick.clone(), false);
                    s.calendar_select_date(&nid_pick, date_pick.clone());
                    tx.focus_when_shown(focus_key.clone(), false);
                })
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .child(
                            text(i18n::weekday_short(wd_idx))
                                .font(TextStyle::new((badge_size * 0.88).max(7.0), 500.0).mono())
                                .color(theme::text_muted()),
                        )
                        .child(el().grow(1.0))
                        .child(
                            text(format!("{}", day_num))
                                .font(TextStyle::new((badge_size * 0.96).max(8.0), 600.0).mono())
                                .color(if is_sel || is_today {
                                    accent
                                } else {
                                    theme::text_primary()
                                }),
                        ),
                );

            for (idx, ev) in day_events.into_iter().take(2) {
                let nid_ev = node.id.clone();
                let ev_id = ev.id.clone();
                day_col = day_col.child(
                    el()
                        .key(format!("cal_wk_evt_{}_{}", node.id.as_str(), idx))
                        .w_full()
                        .py(1.0 * zoom)
                        .radius(3.0 * zoom)
                        .hover(theme::hover_subtle())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.toggle_calendar_event(&nid_ev, &ev_id);
                        })
                        .child(
                            text(format!(
                                "{} {}",
                                if ev.completed { "✓" } else { "•" },
                                ev.title
                            ))
                            .font(TextStyle::new((badge_size * 0.85).max(7.0), 500.0))
                            .color(if ev.completed {
                                theme::text_muted()
                            } else {
                                accent
                            })
                            .truncate(),
                        ),
                );
            }

            if !day_note.is_empty() {
                day_col = day_col.child(
                    text(day_note)
                        .font(TextStyle::new((badge_size * 0.86).max(7.0), 400.0))
                        .color(theme::text_secondary())
                        .w_full()
                        .wrap(),
                );
            }

            week_row = week_row.child(day_col);
        }
        body = body.child(week_row);

        // Constant-height compact quick-note bar so squared day cards stay uniform
        let nid_edit = node.id.clone();
        let nid_focus = node.id.clone();
        let focus_note_key = format!("cal_card_note_input_{}", node.id.as_str());
        let bar_h = (25.0 * zoom).clamp(18.0, 32.0);
        if s.selected_nodes.contains(&node.id) {
            body = body.child(
                row()
                    .w_full()
                    .h(bar_h)
                    .px(7.0 * zoom)
                    .gap(6.0 * zoom)
                    .center_y()
                    .radius(4.0 * zoom)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_default())
                    .child(
                        text(selected_date.clone())
                            .font(TextStyle::new((badge_size * 0.88).max(7.5), 600.0).mono())
                            .color(theme::text_muted()),
                    )
                    .child(
                        text_input(
                            &s.cal_day_note_edit,
                            TextStyle::new((badge_size * 0.96).max(8.5), 400.0),
                        )
                        .key(focus_note_key)
                        .label("Day note")
                        .single_line()
                        .grow(1.0)
                        .color(theme::text_primary())
                        .caret(accent, accent.alpha(0.25))
                        .placeholder(i18n::t(TextKey::CalDayNotePlaceholder), theme::text_muted())
                        .on_edit(|s: &mut AppState| &mut s.cal_day_note_edit)
                        .on_input_change(move |s: &mut AppState, _| {
                            s.sync_calendar_day_note_edit(&nid_edit);
                        }),
                    ),
            );
        } else {
            let preview_note = node
                .meta
                .calendar_day_notes
                .get(&selected_date)
                .map(|t| t.trim())
                .filter(|t| !t.is_empty());
            let sel_date_pick = selected_date.clone();
            body = body.child(
                row()
                    .w_full()
                    .h(bar_h)
                    .px(7.0 * zoom)
                    .gap(6.0 * zoom)
                    .center_y()
                    .radius(4.0 * zoom)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_subtle())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                        s.select_node(nid_focus.clone(), false);
                        s.calendar_select_date(&nid_focus, sel_date_pick.clone());
                        tx.focus_when_shown(focus_note_key.clone(), false);
                    })
                    .child(
                        text(selected_date)
                            .font(TextStyle::new((badge_size * 0.88).max(7.5), 600.0).mono())
                            .color(theme::text_muted()),
                    )
                    .child(
                        text(preview_note.unwrap_or_else(|| i18n::t(TextKey::CalDayNotePlaceholder)))
                            .font(TextStyle::new((badge_size * 0.92).max(8.0), 400.0))
                            .color(if preview_note.is_some() {
                                theme::text_secondary()
                            } else {
                                theme::text_muted()
                            })
                            .truncate(),
                    ),
            );
        }
        return body;
    }

    // Combined single Month header row: nav + segmented Month | Week toggle
    body = body.child(
        row()
            .w_full()
            .center_y()
            .gap(4.0 * zoom)
            .child(
                el()
                    .key(format!("cal_prev_{}", node.id.as_str()))
                    .px(5.0 * zoom)
                    .py(1.5 * zoom)
                    .radius(4.0)
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.calendar_step_month(&prev_id, -1);
                    })
                    .child(
                        text("‹")
                            .font(TextStyle::new(body_size, 600.0))
                            .color(theme::text_secondary()),
                    ),
            )
            .child(
                text(format!("{} {}", i18n::month_short(cm), cy))
                    .font(TextStyle::new(badge_size.max(8.5), 600.0))
                    .color(theme::text_primary()),
            )
            .child(
                el()
                    .key(format!("cal_next_{}", node.id.as_str()))
                    .px(5.0 * zoom)
                    .py(1.5 * zoom)
                    .radius(4.0)
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.calendar_step_month(&next_id, 1);
                    })
                    .child(
                        text("›")
                            .font(TextStyle::new(body_size, 600.0))
                            .color(theme::text_secondary()),
                    ),
            )
            .child(el().grow(1.0))
            .child(mode_segmented),
    );

    // Weekday row
    let mut wk_row = row().w_full().gap(2.0 * zoom);
    for wd_idx in 0..7 {
        wk_row = wk_row.child(
            el().grow(1.0).center().child(
                text(i18n::weekday_narrow(wd_idx))
                    .font(TextStyle::new((badge_size * 0.85).clamp(7.0, 10.5), 600.0).mono())
                    .color(theme::text_muted()),
            ),
        );
    }
    body = body.child(wk_row);

    // Day grid
    let offset = media_helper::weekday_of_first(cy, cm);
    let dim = media_helper::days_in_month(cy, cm);
    let total_cells = offset + dim;
    let num_rows = (total_cells + 6) / 7;
    let mut grid = col().w_full().gap(2.0 * zoom);
    for r in 0..num_rows {
        let mut r_el = row().w_full().gap(2.0 * zoom);
        for c in 0..7 {
            let slot = r * 7 + c;
            if slot < offset || slot >= offset + dim {
                r_el = r_el.child(el().grow(1.0).h((19.0 * zoom).clamp(11.0, 26.0)));
            } else {
                let day = slot - offset + 1;
                let date_str = media_helper::format_ymd(cy, cm, day);
                let has_evt = node.meta.calendar_events.iter().any(|e| e.date == date_str);
                let has_note = node
                    .meta
                    .calendar_day_notes
                    .get(&date_str)
                    .is_some_and(|t| !t.trim().is_empty());
                let has_mark = has_evt || has_note;
                let is_sel = selected_date == date_str;
                let is_today = today_str == date_str;
                let nid = node.id.clone();
                let date_pick = date_str.clone();
                r_el = r_el.child(
                    el()
                        .key(format!("cal_day_{}_{}", node.id.as_str(), date_str))
                        .grow(1.0)
                        .h((19.0 * zoom).clamp(11.0, 26.0))
                        .radius(4.0 * zoom)
                        .fill(if is_sel {
                            theme::bg_control()
                        } else if has_mark {
                            accent.alpha(0.12)
                        } else {
                            Color::TRANSPARENT
                        })
                        .border(
                            if is_sel || is_today { 1.0 } else { 0.0 },
                            if is_sel {
                                accent
                            } else if is_today {
                                theme::border_default()
                            } else {
                                Color::TRANSPARENT
                            },
                        )
                        .hover(theme::hover_subtle())
                        .center()
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.calendar_select_date(&nid, date_pick.clone());
                        })
                        .child(
                            text(format!("{}", day))
                                .font(TextStyle::new((badge_size * 0.9).clamp(7.5, 11.0), 500.0).mono())
                                .color(if is_sel || has_mark {
                                    accent
                                } else {
                                    theme::text_secondary()
                                }),
                        ),
                );
            }
        }
        grid = grid.child(r_el);
    }
    body = body.child(grid);

    if let Some(note_txt) = node
        .meta
        .calendar_day_notes
        .get(&selected_date)
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
    {
        body = body.child(
            el()
                .w_full()
                .px(6.0 * zoom)
                .py(2.5 * zoom)
                .radius(4.0 * zoom)
                .fill(theme::bg_input())
                .border(1.0, theme::border_subtle())
                .child(
                    text(note_txt)
                        .font(TextStyle::new((badge_size * 0.9).max(7.5), 400.0))
                        .color(theme::text_secondary())
                        .truncate(),
                ),
        );
    }

    // Compact events preview (selected day first, else upcoming)
    let mut matching: Vec<(usize, &studynodes_graph::model::CalendarEvent)> = node
        .meta
        .calendar_events
        .iter()
        .enumerate()
        .filter(|(_, e)| e.date == selected_date)
        .collect();
    if matching.is_empty() {
        matching = node.meta.calendar_events.iter().enumerate().collect();
    }
    for (idx, ev) in matching.into_iter().take(2) {
        let nid = node.id.clone();
        let ev_id = ev.id.clone();
        let time_prefix = ev
            .time
            .as_deref()
            .filter(|t| !t.is_empty())
            .map(|t| format!("{} ", t))
            .unwrap_or_default();
        body = body.child(
            row()
                .key(format!("cal_evt_toggle_{}_{}", node.id.as_str(), idx))
                .w_full()
                .px(6.0 * zoom)
                .py(2.0 * zoom)
                .gap(5.0 * zoom)
                .center_y()
                .radius(4.0 * zoom)
                .fill(theme::bg_control())
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.toggle_calendar_event(&nid, &ev_id);
                })
                .child(
                    text(if ev.completed { "☑" } else { "☐" })
                        .font(TextStyle::new(badge_size, 600.0))
                        .color(if ev.completed {
                            theme::text_muted()
                        } else {
                            accent
                        }),
                )
                .child(
                    text(format!("{}{} · {}", time_prefix, ev.date, ev.title))
                        .font(TextStyle::new(badge_size, 400.0))
                        .color(if ev.completed {
                            theme::text_muted()
                        } else {
                            theme::text_primary()
                        })
                        .truncate(),
                ),
        );
    }

    body
}
