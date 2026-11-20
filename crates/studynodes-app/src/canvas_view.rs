use rune::tok::space;
use rune::{
    canvas, col, el, row, text, text_input, Color, Cursor, Cx, El, LineCap, Phase,
    PointerButton, Shape, TextStyle,
};

use studynodes_graph::bezier::CubicBezier;
use studynodes_graph::coords::{ScreenPos, WorldPos};
use studynodes_graph::model::{EdgeStatus, Node, NodeKind, PortId};

use crate::state::{AppState, CanvasMode};

const HEADLINE: TextStyle = TextStyle::new(14.0, 700.0);

pub fn render_canvas_view(s: &AppState, _: &mut Cx<AppState>) -> El {
    let t = rune::tok::theme(true);
    let cam = s.graph.viewport;
    let zoom = cam.zoom;

    // Palette tokens (Blender & Antigravity sleek dark canvas)
    let bg_color = Color { r: 0.065, g: 0.075, b: 0.095, a: 1.0 };
    let grid_color = Color { r: 0.12, g: 0.14, b: 0.18, a: 1.0 };
    let wire_default = Color { r: 0.42, g: 0.46, b: 0.55, a: 0.90 };
    let wire_selected = Color { r: 0.38, g: 0.65, b: 1.0, a: 1.0 };
    let wire_ghost = Color { r: 0.85, g: 0.45, b: 0.95, a: 0.95 }; // Ghost Edge: Neon Violet

    // 1. Build Top Toolbar
    let toolbar = render_top_toolbar(s, t);

    // 2. Build Canvas Vector Geometry (Grid + Bézier Wires)
    let mut canvas_shape = Shape::new();

    // Draw subtle grid lines
    let grid_spacing = 60.0 * zoom;
    if grid_spacing >= 12.0 {
        let offset_x = (cam.offset.x * zoom) % grid_spacing;
        let offset_y = (cam.offset.y * zoom) % grid_spacing;

        let mut x = -offset_x;
        while x < 3840.0 {
            canvas_shape = canvas_shape.line([x, 0.0], [x, 2160.0], 0.5, LineCap::Butt, grid_color);
            x += grid_spacing;
        }

        let mut y = -offset_y;
        while y < 2160.0 {
            canvas_shape = canvas_shape.line([0.0, y], [3840.0, y], 0.5, LineCap::Butt, grid_color);
            y += grid_spacing;
        }
    }

    // Draw all connection wires
    for edge in s.graph.edges.values() {
        if let (Some(from_node), Some(to_node)) = (
            s.graph.get_node(&edge.from.node),
            s.graph.get_node(&edge.to.node),
        ) {
            let from_world = from_node
                .port_world_pos(&edge.from.port)
                .unwrap_or(from_node.pos);
            let to_world = to_node
                .port_world_pos(&edge.to.port)
                .unwrap_or(to_node.pos);

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
                (wire_selected, 3.0 * zoom.clamp(0.7, 1.4))
            } else if is_ghost {
                (wire_ghost, 2.5 * zoom.clamp(0.7, 1.4))
            } else {
                (wire_default, 2.0 * zoom.clamp(0.7, 1.4))
            };

            canvas_shape = canvas_shape.stroke(segments, wire_w, wire_color);
        }
    }

    // Draw active wire draft if user is currently pulling a wire
    if let CanvasMode::DraggingWire {
        from_node,
        from_port,
        current_cursor,
    } = &s.canvas_mode
    {
        if let Some(node) = s.graph.get_node(from_node) {
            let from_world = node.port_world_pos(from_port).unwrap_or(node.pos);
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
                2.8 * zoom.clamp(0.7, 1.4),
                Color { r: 1.0, g: 0.82, b: 0.20, a: 1.0 }, // Live pulling: Amber
            );
        }
    }

    // 3. Assemble the Interactive Canvas Container
    // Full simultaneous 2D navigation:
    // - Trackpad 2-finger scroll moves X and Y simultaneously
    // - Ctrl+wheel / Cmd+wheel zooms anchored at cursor
    // - Shift+wheel scrolls horizontally
    // - Trackpad pinch magnifies anchored at cursor
    // - Spacebar + drag pans anywhere
    // - Middle-click drag pans anywhere
    // - Right-click (secondary) drag pans anywhere
    // - Left-click drag on canvas background pans in 2D
    let mut canvas_container = el()
        .key("graph_canvas")
        .w_full()
        .grow(1.0)
        .fill(bg_color)
        .cursor(if s.space_held { Cursor::Grab } else { Cursor::Default })
        .pointer_buttons([PointerButton::Primary, PointerButton::Middle, PointerButton::Secondary])
        .child(canvas(canvas_shape).w_full().h_full())
        .on_key(|s: &mut AppState, ev, _| {
            if let rune::Key::Text(k) = &ev.key {
                if k == " " {
                    s.space_held = true;
                    return true;
                }
            }
            false
        })
        .on_key_up(|s: &mut AppState, ev, _| {
            if let rune::Key::Text(k) = &ev.key {
                if k == " " {
                    s.space_held = false;
                    return true;
                }
            }
            false
        })
        .on_wheel(|s: &mut AppState, e, _| {
            if e.mods.ctrl || e.mods.cmd {
                // Ctrl+scroll zoom anchored at cursor
                let factor = if e.dy > 0.0 { 1.10 } else { 0.90 };
                s.graph.viewport.zoom_at(ScreenPos::new(e.x, e.y), factor);
            } else if e.mods.shift {
                // Shift+wheel: horizontal pan
                s.graph.viewport.pan_by_screen_delta(e.dy, 0.0);
            } else {
                // Simultaneous 2D panning on X and Y!
                s.graph.viewport.pan_by_screen_delta(e.dx, e.dy);
            }
            true
        })
        .on_magnify(|s: &mut AppState, e, _| {
            // Trackpad pinch-to-zoom anchored at cursor
            let factor = (1.0 + e.delta).clamp(0.5, 2.0);
            s.graph.viewport.zoom_at(ScreenPos::new(e.x, e.y), factor);
            true
        })
        .on_drop_files(|s: &mut AppState, files, _| {
            s.handle_dropped_files(files, ScreenPos::new(400.0, 300.0));
        })
        .on_drag(|s: &mut AppState, d, _tx| match d.phase {
            Phase::Down => {
                s.canvas_mode = CanvasMode::Panning {
                    start_screen: ScreenPos::new(d.x, d.y),
                    start_offset: s.graph.viewport.offset,
                };
            }
            Phase::Move => {
                if let CanvasMode::Panning {
                    start_offset,
                    ..
                } = s.canvas_mode
                {
                    let zoom = s.graph.viewport.zoom.max(0.01);
                    s.graph.viewport.offset.x = start_offset.x - (d.dx / zoom);
                    s.graph.viewport.offset.y = start_offset.y - (d.dy / zoom);
                } else if let CanvasMode::DraggingNode {
                    ref node_id,
                    start_pos,
                } = s.canvas_mode
                {
                    let zoom = s.graph.viewport.zoom.max(0.01);
                    if let Some(node) = s.graph.get_node_mut(node_id) {
                        node.pos = WorldPos::new(
                            start_pos.x + (d.dx / zoom),
                            start_pos.y + (d.dy / zoom),
                        );
                    }
                }
            }
            Phase::Up => {
                s.canvas_mode = CanvasMode::Idle;
            }
        });

    // 4. Mount Edge Annotation Midpoint Pills (hidden when zoomed far out)
    if zoom >= 0.55 {
        for edge in s.graph.edges.values() {
            if let (Some(from_node), Some(to_node)) = (
                s.graph.get_node(&edge.from.node),
                s.graph.get_node(&edge.to.node),
            ) {
                let from_world = from_node
                    .port_world_pos(&edge.from.port)
                    .unwrap_or(from_node.pos);
                let to_world = to_node
                    .port_world_pos(&edge.to.port)
                    .unwrap_or(to_node.pos);

                let curve = CubicBezier::wire(from_world, to_world);
                let mid_screen = cam.world_to_screen(curve.midpoint());

                let pill = render_edge_pill(edge, mid_screen, zoom);
                canvas_container = canvas_container.child(pill);
            }
        }
    }

    // 5. Mount Node Cards (scaled by zoom)
    for node in s.graph.nodes.values() {
        let card = render_node_card(s, node, cam.world_to_screen(node.pos), zoom);
        canvas_container = canvas_container.child(card);
    }

    col().w_full().h_full().child(toolbar).child(canvas_container)
}

fn render_top_toolbar(s: &AppState, _t: rune::tok::Theme) -> El {
    let bar_bg = Color { r: 0.08, g: 0.09, b: 0.12, a: 1.0 };
    let border = Color { r: 0.16, g: 0.18, b: 0.24, a: 1.0 };

    // Amber accent for Note
    let note_color = Color { r: 0.95, g: 0.68, b: 0.12, a: 1.0 };
    let note_bg = Color { r: 0.95, g: 0.68, b: 0.12, a: 0.14 };

    // Blue accent for Document
    let doc_color = Color { r: 0.35, g: 0.65, b: 1.0, a: 1.0 };
    let doc_bg = Color { r: 0.25, g: 0.55, b: 0.95, a: 0.14 };

    // Violet accent for AI Discover
    let ai_color = Color { r: 0.55, g: 0.30, b: 0.90, a: 1.0 };

    row()
        .w_full()
        .px(14.0)
        .py(7.0)
        .gap(space::S3)
        .center()
        .fill(bar_bg)
        .border(1.0, border)
        // Back to Projects button
        .child(
            el()
                .px(10.0)
                .py(5.0)
                .radius(6.0)
                .fill(Color { r: 0.13, g: 0.15, b: 0.19, a: 1.0 })
                .border(1.0, Color { r: 0.22, g: 0.25, b: 0.32, a: 1.0 })
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.close_to_dashboard();
                })
                .child(
                    text("← Projects")
                        .font(TextStyle::new(12.5, 500.0))
                        .color(Color { r: 0.85, g: 0.88, b: 0.94, a: 1.0 }),
                ),
        )
        // Subtle vertical separator
        .child(
            el()
                .size(1.0, 18.0)
                .fill(Color { r: 0.20, g: 0.22, b: 0.28, a: 1.0 }),
        )
        // Project Title
        .child(
            text(&s.graph.title)
                .font(HEADLINE)
                .color(Color::WHITE),
        )
        // Graph Stats Pill
        .child(
            row()
                .px(8.0)
                .py(3.0)
                .radius(10.0)
                .fill(Color { r: 0.13, g: 0.15, b: 0.20, a: 0.85 })
                .border(1.0, Color { r: 0.20, g: 0.23, b: 0.30, a: 0.6 })
                .child(
                    text(format!("{} nodes  •  {} links", s.graph.nodes.len(), s.graph.edges.len()))
                        .font(TextStyle::new(11.0, 500.0))
                        .color(Color { r: 0.60, g: 0.65, b: 0.75, a: 1.0 }),
                ),
        )
        // Active Status Pill (if status is not standard Ready)
        .child(if !s.status_message.is_empty() && s.status_message != "Ready" {
            row()
                .px(8.0)
                .py(3.0)
                .radius(10.0)
                .fill(Color { r: 0.10, g: 0.22, b: 0.38, a: 0.7 })
                .border(1.0, Color { r: 0.25, g: 0.48, b: 0.80, a: 0.7 })
                .child(
                    text(&s.status_message)
                        .font(TextStyle::new(11.0, 500.0))
                        .color(Color { r: 0.65, g: 0.82, b: 1.0, a: 1.0 }),
                )
        } else {
            el()
        })
        .child(el().grow(1.0)) // Spacer
        // Clean Zoom Controls (Grouped segmented pill)
        .child(
            row()
                .center()
                .radius(6.0)
                .fill(Color { r: 0.11, g: 0.13, b: 0.17, a: 1.0 })
                .border(1.0, Color { r: 0.20, g: 0.22, b: 0.28, a: 1.0 })
                .padding(2.0)
                .gap(1.0)
                .child(
                    el()
                        .px(8.0)
                        .py(4.0)
                        .radius(4.0)
                        .fill(Color { r: 0.15, g: 0.17, b: 0.22, a: 1.0 })
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.zoom = (s.graph.viewport.zoom * 0.85).max(0.2);
                        })
                        .child(text("−").font(TextStyle::new(13.0, 600.0)).color(Color { r: 0.85, g: 0.88, b: 0.94, a: 1.0 })),
                )
                .child(
                    el()
                        .px(9.0)
                        .py(4.0)
                        .radius(4.0)
                        .fill(Color { r: 0.12, g: 0.14, b: 0.18, a: 1.0 })
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.zoom = 1.0;
                            s.graph.viewport.offset = WorldPos::ZERO;
                        })
                        .child(
                            text(format!("{}%", (s.graph.viewport.zoom * 100.0).round() as u32))
                                .font(TextStyle::new(11.5, 500.0))
                                .color(Color { r: 0.72, g: 0.76, b: 0.85, a: 1.0 }),
                        ),
                )
                .child(
                    el()
                        .px(8.0)
                        .py(4.0)
                        .radius(4.0)
                        .fill(Color { r: 0.15, g: 0.17, b: 0.22, a: 1.0 })
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.zoom = (s.graph.viewport.zoom * 1.15).min(3.0);
                        })
                        .child(text("+").font(TextStyle::new(13.0, 600.0)).color(Color { r: 0.85, g: 0.88, b: 0.94, a: 1.0 })),
                )
                .child(
                    el()
                        .px(8.0)
                        .py(4.0)
                        .radius(4.0)
                        .fill(Color { r: 0.15, g: 0.17, b: 0.22, a: 1.0 })
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.zoom = 1.0;
                            s.graph.viewport.offset = WorldPos::ZERO;
                        })
                        .child(text("Fit").font(TextStyle::new(11.0, 500.0)).color(Color { r: 0.65, g: 0.70, b: 0.80, a: 1.0 })),
                ),
        )
        // 2D Pan directional controls (Grouped navigation pill)
        .child(
            row()
                .center()
                .radius(6.0)
                .fill(Color { r: 0.11, g: 0.13, b: 0.17, a: 1.0 })
                .border(1.0, Color { r: 0.20, g: 0.22, b: 0.28, a: 1.0 })
                .padding(2.0)
                .gap(1.0)
                .child(
                    el()
                        .px(7.0)
                        .py(4.0)
                        .radius(4.0)
                        .fill(Color { r: 0.15, g: 0.17, b: 0.22, a: 1.0 })
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.pan_by_screen_delta(120.0, 0.0);
                        })
                        .child(text("◄").font(TextStyle::new(10.0, 600.0)).color(Color { r: 0.75, g: 0.78, b: 0.85, a: 1.0 })),
                )
                .child(
                    el()
                        .px(7.0)
                        .py(4.0)
                        .radius(4.0)
                        .fill(Color { r: 0.15, g: 0.17, b: 0.22, a: 1.0 })
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.pan_by_screen_delta(0.0, 120.0);
                        })
                        .child(text("▲").font(TextStyle::new(10.0, 600.0)).color(Color { r: 0.75, g: 0.78, b: 0.85, a: 1.0 })),
                )
                .child(
                    el()
                        .px(7.0)
                        .py(4.0)
                        .radius(4.0)
                        .fill(Color { r: 0.15, g: 0.17, b: 0.22, a: 1.0 })
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.pan_by_screen_delta(0.0, -120.0);
                        })
                        .child(text("▼").font(TextStyle::new(10.0, 600.0)).color(Color { r: 0.75, g: 0.78, b: 0.85, a: 1.0 })),
                )
                .child(
                    el()
                        .px(7.0)
                        .py(4.0)
                        .radius(4.0)
                        .fill(Color { r: 0.15, g: 0.17, b: 0.22, a: 1.0 })
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.pan_by_screen_delta(-120.0, 0.0);
                        })
                        .child(text("►").font(TextStyle::new(10.0, 600.0)).color(Color { r: 0.75, g: 0.78, b: 0.85, a: 1.0 })),
                )
                .child(
                    el()
                        .px(7.0)
                        .py(4.0)
                        .radius(4.0)
                        .fill(Color { r: 0.15, g: 0.17, b: 0.22, a: 1.0 })
                        .cursor(Cursor::Pointer)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.offset = WorldPos::ZERO;
                        })
                        .child(text("⊙").font(TextStyle::new(10.0, 600.0)).color(Color { r: 0.75, g: 0.78, b: 0.85, a: 1.0 })),
                ),
        )
        // Separator
        .child(
            el()
                .size(1.0, 18.0)
                .fill(Color { r: 0.20, g: 0.22, b: 0.28, a: 1.0 }),
        )
        // Add Note Button (Amber)
        .child(
            el()
                .px(11.0)
                .py(5.0)
                .radius(6.0)
                .fill(note_bg)
                .border(1.0, Color { r: note_color.r, g: note_color.g, b: note_color.b, a: 0.80 })
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    let offset_x = (s.graph.nodes.len() as f32 * 32.0) % 192.0;
                    let offset_y = (s.graph.nodes.len() as f32 * 24.0) % 144.0;
                    let center_world = s.graph.viewport.screen_to_world(ScreenPos::new(480.0 + offset_x, 220.0 + offset_y));
                    s.add_note_node(Some(center_world));
                })
                .child(
                    row()
                        .center()
                        .gap(space::S1)
                        .child(text("✎").font(TextStyle::new(12.0, 600.0)).color(note_color))
                        .child(text("+ Note").font(TextStyle::new(12.5, 600.0)).color(note_color)),
                ),
        )
        // Add Document Button (Blue)
        .child(
            el()
                .px(11.0)
                .py(5.0)
                .radius(6.0)
                .fill(doc_bg)
                .border(1.0, Color { r: doc_color.r, g: doc_color.g, b: doc_color.b, a: 0.80 })
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    let offset_x = (s.graph.nodes.len() as f32 * 32.0) % 192.0;
                    let offset_y = (s.graph.nodes.len() as f32 * 24.0) % 144.0;
                    let center_world = s.graph.viewport.screen_to_world(ScreenPos::new(520.0 + offset_x, 260.0 + offset_y));
                    s.add_document_node(Some(center_world));
                })
                .child(
                    row()
                        .center()
                        .gap(space::S1)
                        .child(text("📄").font(TextStyle::new(12.0, 600.0)).color(doc_color))
                        .child(text("+ Document").font(TextStyle::new(12.5, 600.0)).color(doc_color)),
                ),
        )
        // AI Discover Connections (Violet Primary)
        .child(
            el()
                .px(13.0)
                .py(5.0)
                .radius(6.0)
                .fill(ai_color)
                .shadow(2.0, 8.0, Color { r: 0.45, g: 0.18, b: 0.80, a: 0.40 })
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.ai_panel_open = true;
                    s.status_message = "Analyzing graph for latent connections...".into();
                })
                .child(
                    text("✦ AI Discover")
                        .font(TextStyle::new(12.5, 600.0))
                        .color(Color::WHITE),
                ),
        )
}

fn render_node_card(s: &AppState, node: &Node, screen_pos: ScreenPos, zoom: f32) -> El {
    let is_selected = s.selected_nodes.contains(&node.id);

    // Dynamic zoom-scaled sizes
    let (base_w, base_h) = match node.kind {
        NodeKind::Note => (280.0, 170.0),
        NodeKind::Document => (260.0, 125.0),
        NodeKind::Image => (240.0, 110.0),
        NodeKind::LlmSynthesis => (260.0, 130.0),
        NodeKind::Concept => (240.0, 110.0),
    };

    let w = base_w * zoom;
    let h = base_h * zoom;

    // Check if this node is currently a snap target for a pulled wire
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

    // Sleek dark slate background (#181a20)
    let card_bg = Color { r: 0.094, g: 0.102, b: 0.125, a: 0.98 };

    let border_color = if is_snap_target {
        Color { r: 1.0, g: 0.85, b: 0.20, a: 1.0 } // Gold glow on snap target
    } else if is_selected {
        Color { r: 0.38, g: 0.65, b: 1.0, a: 1.0 }
    } else {
        Color { r: 0.18, g: 0.20, b: 0.26, a: 1.0 }
    };

    // Distinct colored header banner & port colors:
    // Amber for Note, Blue for Document, Emerald for Image, Violet for AI Synthesis, Pink for Concept
    let (header_bg, port_color, icon, _kind_badge) = match node.kind {
        NodeKind::Note => (
            Color { r: 0.62, g: 0.38, b: 0.05, a: 0.95 },
            Color { r: 0.95, g: 0.68, b: 0.12, a: 1.0 },
            "✎",
            "NOTE",
        ),
        NodeKind::Document => (
            Color { r: 0.12, g: 0.30, b: 0.62, a: 0.95 },
            Color { r: 0.28, g: 0.58, b: 0.98, a: 1.0 },
            "📄",
            "DOC",
        ),
        NodeKind::Image => (
            Color { r: 0.05, g: 0.38, b: 0.25, a: 0.95 },
            Color { r: 0.10, g: 0.78, b: 0.52, a: 1.0 },
            "🖼",
            "IMAGE",
        ),
        NodeKind::LlmSynthesis => (
            Color { r: 0.40, g: 0.16, b: 0.62, a: 0.95 },
            Color { r: 0.70, g: 0.45, b: 0.98, a: 1.0 },
            "✦",
            "AI SYNTH",
        ),
        NodeKind::Concept => (
            Color { r: 0.60, g: 0.14, b: 0.38, a: 0.95 },
            Color { r: 0.92, g: 0.32, b: 0.62, a: 1.0 },
            "💡",
            "CONCEPT",
        ),
    };

    let node_id_clone = node.id.clone();
    let node_id_for_drag = node.id.clone();
    let out_node_id = node.id.clone();
    let del_node_id = node.id.clone();
    let note_click_id = node.id.clone();
    let doc_click_id = node.id.clone();

    // Typography sizes scaled by zoom
    let title_size = (13.0 * zoom).clamp(8.5, 20.0);
    let body_size = (11.5 * zoom).clamp(7.5, 16.0);
    let badge_size = (9.0 * zoom).clamp(6.5, 13.0);
    let socket_lbl_size = (10.0 * zoom).clamp(7.0, 13.0);

    let card_radius = (10.0 * zoom).clamp(5.0, 14.0);
    let banner_radius = (card_radius - 1.0).max(2.0);

    // Socket circle dimensions scaled with zoom
    let socket_r = (5.0 * zoom).clamp(3.0, 9.0);
    let socket_d = socket_r * 2.0;

    // Input socket circle protruding on left edge
    let in_socket = if is_snap_target {
        let snap_r = socket_r * 1.6;
        let snap_d = snap_r * 2.0;
        el()
            .key(format!("{}_in_socket", node.id.as_str()))
            .absolute(-snap_r, h * 0.5 - snap_r)
            .size(snap_d, snap_d)
            .radius(snap_r)
            .fill(Color { r: 1.0, g: 0.85, b: 0.20, a: 0.40 })
            .border(2.0, Color { r: 1.0, g: 0.85, b: 0.20, a: 1.0 })
            .center()
            .child(el().size(snap_r, snap_r).radius(snap_r * 0.5).fill(Color { r: 1.0, g: 0.85, b: 0.20, a: 1.0 }))
    } else {
        el()
            .key(format!("{}_in_socket", node.id.as_str()))
            .absolute(-socket_r, h * 0.5 - socket_r)
            .size(socket_d, socket_d)
            .radius(socket_r)
            .fill(Color { r: 0.094, g: 0.102, b: 0.125, a: 1.0 })
            .border(1.5, port_color)
            .center()
            .child(el().size(socket_r * 0.8, socket_r * 0.8).radius(socket_r * 0.4).fill(port_color))
    };

    // Output socket circle protruding on right edge (Interactive wire pulling)
    let out_socket = el()
        .key(format!("{}_out_socket", node.id.as_str()))
        .absolute(w - socket_r, h * 0.5 - socket_r)
        .size(socket_d, socket_d)
        .radius(socket_r)
        .fill(Color { r: 0.094, g: 0.102, b: 0.125, a: 1.0 })
        .border(1.5, port_color)
        .cursor(Cursor::Crosshair)
        .center()
        .child(el().size(socket_r * 0.8, socket_r * 0.8).radius(socket_r * 0.4).fill(port_color))
        .on_drag(move |s: &mut AppState, d, _| match d.phase {
            Phase::Down => {
                if let Some(node) = s.graph.get_node(&out_node_id) {
                    let start_world = node
                        .port_world_pos(&PortId::new(PortId::DEFAULT_OUT))
                        .unwrap_or(node.pos);
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
                        let from_world = node.port_world_pos(from_port).unwrap_or(node.pos);
                        let zoom = s.graph.viewport.zoom.max(0.01);
                        let cursor_world = WorldPos::new(
                            from_world.x + (d.dx / zoom),
                            from_world.y + (d.dy / zoom),
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
                            let (ow, oh) = other.size;
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

    // 1. Build Header Banner: Simplified! Only [Icon] [Title] and subtle [✕] delete button!
    let header_bar = row()
        .w_full()
        .corners([banner_radius, banner_radius, 0.0, 0.0])
        .px(8.0 * zoom)
        .py(5.0 * zoom)
        .gap(space::S2)
        .center()
        .fill(header_bg)
        .border_sides([0.0, 0.0, 1.0, 0.0], Color { r: 0.0, g: 0.0, b: 0.0, a: 0.35 })
        // Icon indicator
        .child(
            el()
                .px(4.0 * zoom)
                .py(1.5 * zoom)
                .radius(3.5)
                .fill(Color { r: 0.0, g: 0.0, b: 0.0, a: 0.25 })
                .child(text(icon).font(TextStyle::new(title_size, 700.0)).color(Color::WHITE)),
        )
        // Title: If selected note, editable inline text_input; otherwise crisp text
        .child(if is_selected && node.kind == NodeKind::Note {
            text_input(&s.edit_node_title, TextStyle::new(title_size, 700.0))
                .single_line()
                .color(Color::WHITE)
                .caret(Color::WHITE, Color::WHITE.alpha(0.35))
                .on_edit(|s: &mut AppState| &mut s.edit_node_title)
                .on_input_change(|s: &mut AppState, _| {
                    s.sync_node_edits();
                })
        } else {
            text(&node.title)
                .font(TextStyle::new(title_size, 700.0))
                .color(Color::WHITE)
        })
        .child(el().grow(1.0))
        // Subtle ✕ Delete button
        .child(
            el()
                .px((5.0 * zoom).clamp(4.0, 7.0))
                .py((2.0 * zoom).clamp(2.0, 5.0))
                .radius((3.0 * zoom).clamp(2.0, 5.0))
                .fill(Color { r: 0.0, g: 0.0, b: 0.0, a: 0.25 })
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.delete_node(&del_node_id);
                })
                .child(
                    text("✕")
                        .font(TextStyle::new((10.0 * zoom).clamp(8.0, 13.0), 600.0))
                        .color(Color { r: 1.0, g: 1.0, b: 1.0, a: 0.70 }),
                ),
        );

    // 2. Build Card Body specialized by node kind:
    let card_body = match node.kind {
        NodeKind::Note => {
            // Note nodes: Always live text_input when selected, or clickable text that selects!
            if is_selected {
                col()
                    .w_full()
                    .grow(1.0)
                    .px(8.0 * zoom)
                    .py(6.0 * zoom)
                    .gap(space::S1)
                    .child(
                        el()
                            .w_full()
                            .grow(1.0)
                            .radius((4.0 * zoom).clamp(2.0, 6.0))
                            .fill(Color { r: 0.07, g: 0.08, b: 0.10, a: 1.0 })
                            .border(1.0, Color { r: 0.25, g: 0.35, b: 0.55, a: 0.75 })
                            .px(6.0 * zoom)
                            .py(5.0 * zoom)
                            .child(
                                text_input(&s.edit_node_content, TextStyle::new(body_size, 400.0).lh(body_size * 1.35))
                                    .w_full()
                                    .color(Color::WHITE)
                                    .caret(port_color, port_color.alpha(0.35))
                                    .placeholder("Type your notes directly here...", Color::WHITE.alpha(0.35))
                                    .on_edit(|s: &mut AppState| &mut s.edit_node_content)
                                    .on_input_change(|s: &mut AppState, _| {
                                        s.sync_node_edits();
                                    })
                                    .on_submit(|s: &mut AppState, _| {
                                        s.edit_node_content.insert("\n");
                                        s.sync_node_edits();
                                    }),
                            ),
                    )
            } else {
                let note_content = if node.content.trim().is_empty() {
                    "Click to write notes..."
                } else {
                    node.content.trim()
                };
                col()
                    .w_full()
                    .grow(1.0)
                    .px(8.0 * zoom)
                    .py(6.0 * zoom)
                    .cursor(Cursor::Text)
                    .on_press(move |s: &mut AppState| {
                        s.select_node(note_click_id.clone(), false);
                    })
                    .child(
                        text(note_content)
                            .font(TextStyle::new(body_size, 400.0).lh(body_size * 1.35))
                            .color(if node.content.trim().is_empty() {
                                Color::WHITE.alpha(0.35)
                            } else {
                                Color { r: 0.88, g: 0.90, b: 0.95, a: 0.95 }
                            }),
                    )
            }
        }
        NodeKind::Document => {
            // Document nodes: Clean and compact on canvas! Never overflows pages of text!
            // Clicking selects the node and immediately pops open the full reader in the right panel.
            let word_count = node.content.split_whitespace().count();
            col()
                .w_full()
                .grow(1.0)
                .px(10.0 * zoom)
                .py(8.0 * zoom)
                .gap(space::S2)
                .center()
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.select_node(doc_click_id.clone(), false);
                    s.inspector_open = true;
                    s.status_message = "Opened Document Reader in right panel".into();
                })
                .child(
                    row()
                        .center()
                        .gap(space::S2)
                        .child(
                            el()
                                .px(6.0 * zoom)
                                .py(2.0 * zoom)
                                .radius(3.0)
                                .fill(Color { r: 0.15, g: 0.35, b: 0.75, a: 0.35 })
                                .child(
                                    text("📄 DOCUMENT")
                                        .font(TextStyle::new(badge_size, 700.0))
                                        .color(Color { r: 0.40, g: 0.70, b: 1.0, a: 1.0 }),
                                ),
                        )
                        .child(
                            text(format!("{} words", word_count))
                                .font(TextStyle::new(socket_lbl_size, 500.0))
                                .color(Color { r: 0.55, g: 0.60, b: 0.70, a: 0.85 }),
                        ),
                )
                .child(
                    el()
                        .px(8.0 * zoom)
                        .py(4.0 * zoom)
                        .radius(4.0)
                        .fill(Color { r: 0.12, g: 0.25, b: 0.50, a: 0.40 })
                        .border(1.0, Color { r: 0.25, g: 0.50, b: 0.90, a: 0.65 })
                        .child(
                            text("Click to open reader →")
                                .font(TextStyle::new(badge_size, 600.0))
                                .color(Color { r: 0.55, g: 0.78, b: 1.0, a: 1.0 }),
                        ),
                )
        }
        _ => {
            // Image, AI Synthesis, Concept: Compact preview
            let preview_text = if node.content.trim().is_empty() {
                match node.kind {
                    NodeKind::Image => "Visual image asset.",
                    NodeKind::LlmSynthesis => "AI synthesis node.",
                    NodeKind::Concept => "High-level concept cluster.",
                    _ => "",
                }
            } else {
                node.content.trim()
            };
            col()
                .w_full()
                .grow(1.0)
                .px(8.0 * zoom)
                .py(6.0 * zoom)
                .center()
                .child(
                    text(preview_text)
                        .font(TextStyle::new(body_size, 400.0))
                        .color(Color { r: 0.75, g: 0.78, b: 0.85, a: 0.90 }),
                )
        }
    };

    // 3. Assemble the Node Card Container
    col()
        .key(node.id.as_str())
        .size(w, h)
        .absolute(screen_pos.x, screen_pos.y)
        .radius(card_radius)
        .fill(card_bg)
        .border(if is_selected { 1.5 } else { 1.0 }, border_color)
        .shadow(4.0, 14.0, Color { r: 0.0, g: 0.0, b: 0.0, a: 0.45 })
        .cursor(if s.space_held { Cursor::Grab } else { Cursor::Default })
        .pointer_buttons([PointerButton::Primary, PointerButton::Middle, PointerButton::Secondary])
        .on_press(move |s: &mut AppState| {
            s.select_node(node_id_clone.clone(), false);
        })
        .on_drag(move |s: &mut AppState, d, tx| match d.phase {
            Phase::Down => {
                // If middle-click, right-click (secondary), or spacebar is held, PAN the canvas!
                if tx.button() == PointerButton::Middle
                    || tx.button() == PointerButton::Secondary
                    || s.space_held
                {
                    s.canvas_mode = CanvasMode::Panning {
                        start_screen: ScreenPos::new(d.x, d.y),
                        start_offset: s.graph.viewport.offset,
                    };
                    return;
                }
                s.select_node(node_id_for_drag.clone(), false);
                if let Some(node) = s.graph.get_node(&node_id_for_drag) {
                    s.canvas_mode = CanvasMode::DraggingNode {
                        node_id: node_id_for_drag.clone(),
                        start_pos: node.pos,
                    };
                }
            }
            Phase::Move => {
                if let CanvasMode::DraggingNode {
                    ref node_id,
                    start_pos,
                } = s.canvas_mode
                {
                    let zoom = s.graph.viewport.zoom.max(0.01);
                    if let Some(node) = s.graph.get_node_mut(node_id) {
                        node.pos = WorldPos::new(
                            start_pos.x + (d.dx / zoom),
                            start_pos.y + (d.dy / zoom),
                        );
                    }
                } else if let CanvasMode::Panning {
                    start_offset,
                    ..
                } = s.canvas_mode
                {
                    let zoom = s.graph.viewport.zoom.max(0.01);
                    s.graph.viewport.offset.x = start_offset.x - (d.dx / zoom);
                    s.graph.viewport.offset.y = start_offset.y - (d.dy / zoom);
                }
            }
            Phase::Up => {
                s.canvas_mode = CanvasMode::Idle;
            }
        })
        .child(header_bar)
        .child(card_body)
        .child(in_socket)
        .child(out_socket)
}

fn render_edge_pill(edge: &studynodes_graph::model::Edge, screen_pos: ScreenPos, zoom: f32) -> El {
    let edge_id_clone = edge.id.clone();
    let edge_id_for_accept = edge.id.clone();
    let edge_id_for_reject = edge.id.clone();

    let is_ghost = edge.status == EdgeStatus::Proposed;
    let pill_bg = if is_ghost {
        Color { r: 0.35, g: 0.15, b: 0.45, a: 0.95 }
    } else {
        Color { r: 0.14, g: 0.16, b: 0.20, a: 0.95 }
    };

    let pill_border = if is_ghost {
        Color { r: 0.85, g: 0.45, b: 0.95, a: 1.0 }
    } else {
        Color { r: 0.28, g: 0.32, b: 0.38, a: 1.0 }
    };

    let font_size = (11.0 * zoom).clamp(8.0, 13.0);

    let mut pill = row()
        .key(edge.id.as_str())
        .absolute(screen_pos.x - 60.0 * zoom, screen_pos.y - 14.0 * zoom)
        .px(7.0 * zoom)
        .py(3.0 * zoom)
        .gap(space::S2)
        .radius((6.0 * zoom).clamp(4.0, 8.0))
        .fill(pill_bg)
        .border(1.0, pill_border)
        .shadow(2.0, 6.0, Color { r: 0.0, g: 0.0, b: 0.0, a: 0.35 })
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut AppState| {
            s.select_edge(edge_id_clone.clone());
        })
        .child(
            text(format!("[{}]", edge.relation.display_label()))
                .font(TextStyle::new(font_size, 600.0))
                .color(if is_ghost {
                    Color { r: 0.95, g: 0.65, b: 1.0, a: 1.0 }
                } else {
                    Color { r: 0.60, g: 0.75, b: 1.0, a: 1.0 }
                }),
        );

    if !edge.label.is_empty() {
        pill = pill.child(
            text(&edge.label)
                .font(TextStyle::new(font_size, 500.0))
                .color(Color::WHITE),
        );
    }

    if is_ghost {
        pill = pill
            .child(
                el()
                    .px(5.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(3.0)
                    .fill(Color { r: 0.10, g: 0.70, b: 0.40, a: 1.0 })
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.graph.accept_edge(&edge_id_for_accept);
                        s.status_message = "Accepted AI connection".into();
                    })
                    .child(text("✓").font(TextStyle::new(font_size, 700.0)).color(Color::WHITE)),
            )
            .child(
                el()
                    .px(5.0 * zoom)
                    .py(2.0 * zoom)
                    .radius(3.0)
                    .fill(Color { r: 0.70, g: 0.20, b: 0.20, a: 1.0 })
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut AppState| {
                        s.graph.reject_edge(&edge_id_for_reject);
                        s.status_message = "Dismissed AI connection".into();
                    })
                    .child(text("✕").font(TextStyle::new(font_size, 700.0)).color(Color::WHITE)),
            );
    }

    pill
}
