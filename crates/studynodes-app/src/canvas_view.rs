use rune::tok::{self, radius, space, type_};
use rune::{Color, Cx, El, LineCap, Phase, Shape, TextStyle, canvas, col, el, row, text};

use studynodes_graph::bezier::CubicBezier;
use studynodes_graph::coords::{ScreenPos, WorldPos};
use studynodes_graph::model::{EdgeStatus, Node, NodeKind};

use crate::state::{AppState, CanvasMode};

const HEADLINE: TextStyle = TextStyle::new(14.0, 600.0);

pub fn render_canvas_view(s: &AppState, _: &mut Cx<AppState>) -> El {
    let t = tok::theme(true);
    let cam = s.graph.viewport;

    // Palette tokens
    let bg_color = Color { r: 0.08, g: 0.09, b: 0.11, a: 1.0 };
    let grid_color = Color { r: 0.14, g: 0.16, b: 0.20, a: 1.0 };
    let wire_default = Color { r: 0.35, g: 0.38, b: 0.44, a: 0.85 };
    let wire_selected = Color { r: 0.38, g: 0.60, b: 1.0, a: 1.0 };
    let wire_ghost = Color { r: 0.85, g: 0.45, b: 0.95, a: 0.95 }; // Ghost Edge: Neon Violet

    // 1. Build Top Toolbar
    let toolbar = render_top_toolbar(s, t);

    // 2. Build Canvas Vector Geometry (Grid + Bézier Wires)
    let mut canvas_shape = Shape::new();

    // Draw subtle grid lines
    let grid_spacing = 60.0 * cam.zoom;
    if grid_spacing >= 12.0 {
        let offset_x = (cam.offset.x * cam.zoom) % grid_spacing;
        let offset_y = (cam.offset.y * cam.zoom) % grid_spacing;

        let mut x = -offset_x;
        while x < 1920.0 {
            canvas_shape = canvas_shape.line([x, 0.0], [x, 1080.0], 0.5, LineCap::Butt, grid_color);
            x += grid_spacing;
        }

        let mut y = -offset_y;
        while y < 1080.0 {
            canvas_shape = canvas_shape.line([0.0, y], [1920.0, y], 0.5, LineCap::Butt, grid_color);
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
                (wire_selected, 3.0)
            } else if is_ghost {
                (wire_ghost, 2.5)
            } else {
                (wire_default, 1.8)
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
                2.5,
                Color { r: 1.0, g: 0.8, b: 0.2, a: 1.0 }, // Live pulling: Amber
            );
        }
    }

    // 3. Assemble the Interactive Canvas Container
    let mut canvas_container = el()
        .key("graph_canvas")
        .w_full()
        .grow(1.0)
        .fill(bg_color)
        .child(canvas(canvas_shape).w_full().h_full())
        .on_drop_files(|s: &mut AppState, files, _| {
            s.handle_dropped_files(files, ScreenPos::new(400.0, 300.0));
        })
        .on_drag(|s: &mut AppState, d, _| match d.phase {
            Phase::Down => {
                s.canvas_mode = CanvasMode::Panning {
                    start_screen: ScreenPos::new(d.x, d.y),
                    start_offset: s.graph.viewport.offset,
                };
            }
            Phase::Move => {
                if let CanvasMode::Panning {
                    start_screen,
                    start_offset,
                } = s.canvas_mode
                {
                    let dx = d.x - start_screen.x;
                    let dy = d.y - start_screen.y;
                    s.graph.viewport.offset.x = start_offset.x - (dx / s.graph.viewport.zoom);
                    s.graph.viewport.offset.y = start_offset.y - (dy / s.graph.viewport.zoom);
                } else if let CanvasMode::DraggingNode {
                    ref node_id,
                    grab_offset,
                } = s.canvas_mode
                {
                    let cursor_world = s.graph.viewport.screen_to_world(ScreenPos::new(d.x, d.y));
                    if let Some(node) = s.graph.get_node_mut(node_id) {
                        node.pos = WorldPos::new(
                            cursor_world.x - grab_offset.x,
                            cursor_world.y - grab_offset.y,
                        );
                    }
                }
            }
            Phase::Up => {
                s.canvas_mode = CanvasMode::Idle;
            }
        });

    // 4. Mount Edge Annotation Midpoint Pills
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

            let pill = render_edge_pill(edge, mid_screen);
            canvas_container = canvas_container.child(pill);
        }
    }

    // 5. Mount Node Cards
    for node in s.graph.nodes.values() {
        let card = render_node_card(s, node, cam.world_to_screen(node.pos));
        canvas_container = canvas_container.child(card);
    }

    col().w_full().h_full().child(toolbar).child(canvas_container)
}

fn render_top_toolbar(s: &AppState, t: rune::tok::Theme) -> El {
    let accent = Color { r: 0.38, g: 0.60, b: 1.0, a: 1.0 };
    let bar_bg = Color { r: 0.11, g: 0.12, b: 0.15, a: 1.0 };
    let border = Color { r: 0.20, g: 0.22, b: 0.27, a: 1.0 };

    row()
        .w_full()
        .padding(space::S3)
        .gap(space::S3)
        .fill(bar_bg)
        .border(1.0, border)
        .child(
            el()
                .padding(space::S2)
                .radius(radius::SM)
                .fill(t.card)
                .on_press(|s: &mut AppState| {
                    s.close_to_dashboard();
                })
                .child(text("← Projects").font(type_::BODY).color(t.text)),
        )
        .child(
            text(&s.graph.title)
                .font(HEADLINE)
                .color(t.text),
        )
        .child(
            text(format!("({} nodes, {} connections)", s.graph.nodes.len(), s.graph.edges.len()))
                .font(type_::LABEL)
                .color(t.text_secondary),
        )
        .child(el().grow(1.0)) // Spacer
        // Zoom controls
        .child(
            row()
                .gap(space::S1)
                .child(
                    el()
                        .padding(space::S2)
                        .radius(radius::SM)
                        .fill(t.card)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.zoom = (s.graph.viewport.zoom * 0.85).max(0.2);
                        })
                        .child(text("−").font(type_::BODY).color(t.text)),
                )
                .child(
                    el()
                        .padding(space::S2)
                        .radius(radius::SM)
                        .fill(t.card)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.zoom = 1.0;
                            s.graph.viewport.offset = WorldPos::ZERO;
                        })
                        .child(
                            text(format!("{}%", (s.graph.viewport.zoom * 100.0) as u32))
                                .font(type_::BODY)
                                .color(t.text),
                        ),
                )
                .child(
                    el()
                        .padding(space::S2)
                        .radius(radius::SM)
                        .fill(t.card)
                        .on_press(|s: &mut AppState| {
                            s.graph.viewport.zoom = (s.graph.viewport.zoom * 1.15).min(3.0);
                        })
                        .child(text("+").font(type_::BODY).color(t.text)),
                ),
        )
        // Add Note
        .child(
            el()
                .padding(space::S2)
                .radius(radius::SM)
                .fill(Color { r: 0.92, g: 0.70, b: 0.05, a: 0.2 })
                .border(1.0, Color { r: 0.92, g: 0.70, b: 0.05, a: 1.0 })
                .on_press(|s: &mut AppState| {
                    let center_world = s.graph.viewport.screen_to_world(ScreenPos::new(500.0, 300.0));
                    let mut node = studynodes_graph::model::Node::new(
                        NodeKind::Note,
                        format!("Note #{}", s.graph.nodes.len() + 1),
                        center_world,
                    );
                    node.content = "Write study observations or notes here...".into();
                    let id = node.id.clone();
                    s.graph.add_node(node);
                    s.select_node(id, false);
                })
                .child(
                    text("+ Note")
                        .font(type_::BODY)
                        .color(Color { r: 0.92, g: 0.70, b: 0.05, a: 1.0 }),
                ),
        )
        // AI Discover Connections
        .child(
            el()
                .padding(space::S2)
                .radius(radius::SM)
                .fill(accent)
                .on_press(|s: &mut AppState| {
                    s.ai_panel_open = true;
                    s.status_message = "Analyzing graph for latent connections...".into();
                })
                .child(
                    text("✦ AI Discover")
                        .font(type_::BODY)
                        .color(Color::WHITE),
                ),
        )
}

fn render_node_card(s: &AppState, node: &Node, screen_pos: ScreenPos) -> El {
    let t = tok::theme(true);
    let is_selected = s.selected_nodes.contains(&node.id);
    let card_bg = Color { r: 0.13, g: 0.14, b: 0.18, a: 0.96 };
    let border_color = if is_selected {
        Color { r: 0.38, g: 0.60, b: 1.0, a: 1.0 }
    } else {
        Color { r: 0.22, g: 0.24, b: 0.29, a: 1.0 }
    };

    let kind_accent = match node.kind {
        NodeKind::Note => Color { r: 0.92, g: 0.70, b: 0.05, a: 1.0 },
        NodeKind::Document => Color { r: 0.25, g: 0.55, b: 0.95, a: 1.0 },
        NodeKind::Image => Color { r: 0.10, g: 0.75, b: 0.50, a: 1.0 },
        NodeKind::LlmSynthesis => Color { r: 0.70, g: 0.40, b: 0.95, a: 1.0 },
        NodeKind::Concept => Color { r: 0.90, g: 0.30, b: 0.60, a: 1.0 },
    };

    let (w, h) = node.size;
    let node_id_clone = node.id.clone();
    let node_id_for_drag = node.id.clone();

    col()
        .key(node.id.as_str())
        .size(w, h)
        .absolute(screen_pos.x, screen_pos.y)
        .radius(radius::MD)
        .fill(card_bg)
        .border(if is_selected { 2.0 } else { 1.0 }, border_color)
        .on_press(move |s: &mut AppState| {
            s.select_node(node_id_clone.clone(), false);
        })
        .on_drag(move |s: &mut AppState, d, _| match d.phase {
            Phase::Down => {
                s.select_node(node_id_for_drag.clone(), false);
                let cursor_world = s.graph.viewport.screen_to_world(ScreenPos::new(d.x, d.y));
                if let Some(node) = s.graph.get_node(&node_id_for_drag) {
                    let grab_offset = WorldPos::new(
                        cursor_world.x - node.pos.x,
                        cursor_world.y - node.pos.y,
                    );
                    s.canvas_mode = CanvasMode::DraggingNode {
                        node_id: node_id_for_drag.clone(),
                        grab_offset,
                    };
                }
            }
            Phase::Move => {
                if let CanvasMode::DraggingNode {
                    ref node_id,
                    grab_offset,
                } = s.canvas_mode
                {
                    let cursor_world = s.graph.viewport.screen_to_world(ScreenPos::new(d.x, d.y));
                    if let Some(node) = s.graph.get_node_mut(node_id) {
                        node.pos = WorldPos::new(
                            cursor_world.x - grab_offset.x,
                            cursor_world.y - grab_offset.y,
                        );
                    }
                }
            }
            Phase::Up => {
                s.canvas_mode = CanvasMode::Idle;
            }
        })
        // Card Header
        .child(
            row()
                .w_full()
                .padding(space::S2)
                .gap(space::S2)
                .center()
                .child(
                    el()
                        .size(8.0, 8.0)
                        .radius(4.0)
                        .fill(kind_accent),
                )
                .child(
                    text(node.kind.display_name())
                        .font(type_::LABEL)
                        .color(kind_accent),
                )
                .child(
                    text(&node.title)
                        .font(type_::BODY)
                        .color(t.text),
                ),
        )
        // Card Body Preview
        .child(
            col()
                .w_full()
                .grow(1.0)
                .padding(space::S3)
                .gap(space::S1)
                .child(
                    text(if node.content.is_empty() {
                        node.summary.as_deref().unwrap_or("Empty node")
                    } else {
                        &node.content
                    })
                    .font(type_::LABEL)
                    .color(t.text_secondary),
                ),
        )
}

fn render_edge_pill(edge: &studynodes_graph::model::Edge, screen_pos: ScreenPos) -> El {
    let edge_id_clone = edge.id.clone();
    let edge_id_for_accept = edge.id.clone();
    let edge_id_for_reject = edge.id.clone();

    let is_ghost = edge.status == EdgeStatus::Proposed;
    let pill_bg = if is_ghost {
        Color { r: 0.35, g: 0.15, b: 0.45, a: 0.95 }
    } else {
        Color { r: 0.16, g: 0.18, b: 0.22, a: 0.95 }
    };

    let pill_border = if is_ghost {
        Color { r: 0.85, g: 0.45, b: 0.95, a: 1.0 }
    } else {
        Color { r: 0.30, g: 0.34, b: 0.40, a: 1.0 }
    };

    let mut pill = row()
        .key(edge.id.as_str())
        .absolute(screen_pos.x - 60.0, screen_pos.y - 14.0)
        .padding(space::S1)
        .gap(space::S2)
        .radius(radius::SM)
        .fill(pill_bg)
        .border(1.0, pill_border)
        .on_press(move |s: &mut AppState| {
            s.select_edge(edge_id_clone.clone());
        })
        .child(
            text(format!("[{}]", edge.relation.display_label()))
                .font(type_::LABEL)
                .color(if is_ghost {
                    Color { r: 0.95, g: 0.65, b: 1.0, a: 1.0 }
                } else {
                    Color { r: 0.60, g: 0.75, b: 1.0, a: 1.0 }
                }),
        );

    if !edge.label.is_empty() {
        pill = pill.child(
            text(&edge.label)
                .font(type_::LABEL)
                .color(Color::WHITE),
        );
    }

    if is_ghost {
        pill = pill
            .child(
                el()
                    .padding(2.0)
                    .radius(3.0)
                    .fill(Color { r: 0.10, g: 0.70, b: 0.40, a: 1.0 })
                    .on_press(move |s: &mut AppState| {
                        s.graph.accept_edge(&edge_id_for_accept);
                        s.status_message = "Accepted AI connection".into();
                    })
                    .child(text("✓").font(type_::LABEL).color(Color::WHITE)),
            )
            .child(
                el()
                    .padding(2.0)
                    .radius(3.0)
                    .fill(Color { r: 0.70, g: 0.20, b: 0.20, a: 1.0 })
                    .on_press(move |s: &mut AppState| {
                        s.graph.reject_edge(&edge_id_for_reject);
                        s.status_message = "Dismissed AI connection".into();
                    })
                    .child(text("✕").font(type_::LABEL).color(Color::WHITE)),
            );
    }

    pill
}
