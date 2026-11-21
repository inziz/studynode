use rune::{
    Color, Cursor, Cx, El, LineCap, Phase, PointerButton, ScrollSpec, Shape, SurfaceFit, TextStyle,
    Tx, canvas, col, el, row, text, text_input,
};

use studynodes_graph::bezier::CubicBezier;
use studynodes_graph::coords::{ScreenPos, WorldPos};
use studynodes_graph::model::{Edge, EdgeStatus, Node, NodeKind, PortDirection, PortId};

use crate::doc_reader::{self, DocFormat};
use crate::state::{AppState, CanvasMode, node_world_size};
use crate::theme;

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

fn build_grid_shape(s: &AppState) -> Shape {
    let cam = s.graph.viewport;
    let zoom = cam.zoom;
    let mut grid_shape = Shape::new();

    let base_spacing = 48.0 * zoom;
    if base_spacing >= 10.0 {
        let minor_color = Color::WHITE.alpha(0.025);
        let major_color = Color::WHITE.alpha(0.052);

        let world_left_idx = (cam.offset.x / 48.0).floor() as i32;
        let offset_x = (cam.offset.x * zoom).rem_euclid(base_spacing);
        let mut x = if offset_x == 0.0 { 0.0 } else { base_spacing - offset_x };
        let mut ix = world_left_idx + if offset_x == 0.0 { 0 } else { 1 };
        while x < 3840.0 {
            let is_major = ix.rem_euclid(4) == 0;
            let (col, w) = if is_major { (major_color, 0.75) } else { (minor_color, 0.5) };
            grid_shape = grid_shape.line([x, 0.0], [x, 2160.0], w, LineCap::Butt, col);
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
            grid_shape = grid_shape.line([0.0, y], [3840.0, y], w, LineCap::Butt, col);
            y += base_spacing;
            iy += 1;
        }
    }

    grid_shape
}

fn build_wire_shape(s: &AppState) -> Shape {
    let cam = s.graph.viewport;
    let zoom = cam.zoom;
    let mut wire_shape = Shape::new();

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
                (theme::wire_selected(), 2.2 * zoom.clamp(0.7, 1.4))
            } else if is_ghost {
                (theme::wire_proposed().alpha(0.82), 2.0 * zoom.clamp(0.7, 1.4))
            } else {
                (theme::wire_default(), 1.6 * zoom.clamp(0.7, 1.4))
            };

            wire_shape = wire_shape.stroke(segments, wire_w, wire_color);
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
        }
    }

    wire_shape
}

pub fn render_canvas_view(s: &AppState) -> El {
    let cam = s.graph.viewport;
    let zoom = cam.zoom;
    let grid_shape = build_grid_shape(s);
    let wire_shape = build_wire_shape(s);

    // 2. Interactive Canvas Container (grow(1.0).min_w(0.0) so right-side Reader fits on screen!)
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
            s.canvas_size = [r[2].max(200.0), r[3].max(200.0)];
            crate::touchpad_win32::set_canvas_bounds(Some(r));
            tx.live_only();
        })
        .child(canvas(grid_shape).w_full().h_full())
        .on_wheel(|s: &mut AppState, e, _| {
            if e.mods.ctrl || e.mods.cmd {
                let factor = if e.dy > 0.0 { 1.10 } else { 0.90 };
                s.graph.viewport.zoom_at(ScreenPos::new(e.x, e.y), factor);
            } else if e.mods.shift && e.dx == 0.0 {
                s.graph.viewport.pan_by_screen_delta(-e.dy, 0.0);
            } else {
                s.graph.viewport.pan_by_screen_delta(-e.dx, -e.dy);
            }
            true
        })
        .on_magnify(|s: &mut AppState, e, _| {
            // Windows Ctrl+wheel pinch sends dy>0 for expand, which Rune's pinch_wheel negates.
            let delta = if cfg!(target_os = "windows") {
                -e.delta
            } else {
                e.delta
            };
            let factor = (1.0 + delta).clamp(0.5, 2.0);
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

    // 2. Section Title Banners & Group Frames (always behind wires and regular cards)
    let mut section_nodes: Vec<&Node> = s
        .graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Section)
        .collect();
    section_nodes.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    for sec in section_nodes {
        canvas_container = canvas_container.child(render_section_node(
            s,
            sec,
            cam.world_to_screen(sec.pos),
            zoom,
        ));
    }

    // 2b. Bezier Wires Layer (above Section backdrops, behind regular Study cards)
    canvas_container = canvas_container.child(canvas(wire_shape).w_full().h_full());

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
        let card = render_node_card(s, node, cam.world_to_screen(node.pos), zoom);
        canvas_container = canvas_container.child(card);
    }

    // 4. Edge Annotation Pills (rendered AFTER nodes so short-wire relation pills NEVER hide behind cards!)
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
                canvas_container = canvas_container.child(render_edge_pill(
                    edge,
                    s.selected_edge.as_ref() == Some(&edge.id),
                    mid_screen,
                    zoom,
                ));
            }
        }
    }

    if s.show_block_menu {
        canvas_container = canvas_container.child(render_block_menu_popover(s));
    }

    canvas_container
}

fn topbar_pill_btn(
    key: &'static str,
    prefix_glyph: Option<(&'static str, Color)>,
    label: &'static str,
) -> El {
    let mut content = row().center_y().gap(5.0);
    if let Some((glyph, color)) = prefix_glyph {
        content = content.child(
            text(glyph)
                .font(theme::CONTROL_TEXT)
                .color(color),
        );
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
        .radius(4.0)
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

    let mut tabs_strip = row()
        .key("workspace_tabs_strip")
        .gap(4.0)
        .center_y()
        .shrink()
        .min_w(0.0)
        .no_window_drag();

    if s.tabs.is_empty() {
        tabs_strip = tabs_strip.child(
            el().shrink().min_w(0.0).child(
                text(&s.graph.title)
                    .font(theme::TITLE_BAR)
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
                    .h(26.0)
                    .max_w(162.0)
                    .radius(4.0)
                    .fill(if is_active {
                        theme::bg_card_selected()
                    } else {
                        theme::bg_control()
                    })
                    .border(
                        1.0,
                        if is_active {
                            theme::border_strong()
                        } else {
                            theme::border_subtle()
                        },
                    )
                    .center_y()
                    .child(
                        row()
                            .key(format!("workspace_tab_{}", idx))
                            .grow(1.0)
                            .min_w(0.0)
                            .h_full()
                            .pl(8.0)
                            .pr(4.0)
                            .gap(5.0)
                            .center_y()
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.switch_tab(idx);
                            })
                            .child(
                                text("●")
                                    .font(TextStyle::new(7.0, 600.0).mono())
                                    .color(if is_active {
                                        theme::accent_claude()
                                    } else {
                                        theme::text_muted()
                                    }),
                            )
                            .child(
                                text(title_text)
                                    .font(theme::STATUS_TEXT)
                                    .color(if is_active {
                                        theme::text_primary()
                                    } else {
                                        theme::text_secondary()
                                    })
                                    .truncate(),
                            ),
                    )
                    .child(
                        el()
                            .key(format!("workspace_tab_close_{}", idx))
                            .size(20.0, 24.0)
                            .center()
                            .hover(theme::accent_danger().alpha(0.18))
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.close_tab(idx);
                            })
                            .child(
                                text("×")
                                    .font(TextStyle::new(11.0, 500.0).mono())
                                    .color(theme::text_muted()),
                            ),
                    ),
            );
        }

        tabs_strip = tabs_strip.child(
            el()
                .key("btn_new_tab")
                .size(26.0, 26.0)
                .radius(4.0)
                .fill(theme::bg_control())
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .border(1.0, theme::border_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut AppState| {
                    s.close_to_dashboard();
                })
                .child(
                    text("+")
                        .font(theme::CONTROL_TEXT)
                        .color(theme::text_secondary()),
                ),
        );
    }

    let mut topbar = row()
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
        // Left: concise Back + Multi-Project Tabs
        .child(
            el()
                .key("btn_back")
                .h(26.0)
                .px(8.0)
                .radius(4.0)
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
        .child(tabs_strip)
        // Middle draggable window band spacer
        .child(el().grow(1.0))
        // Right: concise monochrome creation + Claude AI + Zoom + Reader toggle
        .child(
            topbar_pill_btn(
                "btn_add_note",
                Some(("▪", theme::text_secondary())),
                "+ Note",
            )
            .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
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
            topbar_pill_btn(
                "btn_add_doc",
                Some(("▤", theme::text_secondary())),
                "+ Doc",
            )
            .on_press(|s: &mut AppState| {
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
            el()
                .key("btn_toggle_block_menu")
                .h(26.0)
                .px(10.0)
                .radius(4.0)
                .fill(if s.show_block_menu {
                    theme::accent_claude().alpha(0.18)
                } else {
                    theme::bg_control()
                })
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .border(
                    1.0,
                    if s.show_block_menu {
                        theme::accent_claude().alpha(0.55)
                    } else {
                        theme::border_subtle()
                    },
                )
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press(|s: &mut AppState| {
                    s.show_block_menu = !s.show_block_menu;
                })
                .child(
                    text("+ Block ▾")
                        .font(theme::CONTROL_TEXT)
                        .color(if s.show_block_menu {
                            theme::text_primary()
                        } else {
                            theme::text_secondary()
                        }),
                ),
        )
        .child(
            el()
                .key("btn_ai")
                .h(26.0)
                .px(10.0)
                .radius(4.0)
                .fill(if s.ai_panel_open {
                    theme::accent_claude().alpha(0.18)
                } else {
                    theme::bg_control()
                })
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .border(
                    1.0,
                    if s.ai_panel_open {
                        theme::accent_claude().alpha(0.55)
                    } else {
                        theme::border_subtle()
                    },
                )
                .center()
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .on_press(|s: &mut AppState| {
                    s.ai_panel_open = !s.ai_panel_open;
                    if s.ai_panel_open {
                        if !s.inspector_popped_out {
                            s.inspector_open = true;
                        }
                        s.status_message = "✦ AI Synthesis ready".into();
                    }
                })
                .child(
                    row()
                        .center_y()
                        .gap(4.0)
                        .child(
                            text("✦")
                                .font(theme::CONTROL_TEXT)
                                .color(theme::accent_claude()),
                        )
                        .child(
                            text("AI")
                                .font(theme::CONTROL_TEXT)
                                .color(theme::text_primary()),
                        ),
                ),
        )
        // Jump to nearest nodes button (never get lost on infinite canvas)
        .child(
            topbar_pill_btn(
                "btn_locate_nearest",
                Some(("◎", theme::accent_claude())),
                "Nearest",
            )
            .on_press(|s: &mut AppState| {
                s.locate_nearest_nodes();
            }),
        )
        // Compact segmented zoom pill
        .child(
            row()
                .key("zoom_pill")
                .h(26.0)
                .center_y()
                .radius(4.0)
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
        );

    if s.inspector_popped_out {
        topbar = topbar.child(
            topbar_pill_btn("btn_focus_popout", Some(("⧉", theme::accent_claude())), "Reader")
                .on_press(|s: &mut AppState| {
                    if let Some(h) = &s.popout_window {
                        h.focus();
                    } else {
                        s.pop_out_reader();
                    }
                }),
        );
    }

    topbar.child(
        el()
            .key("btn_toggle_panel")
            .size(26.0, 26.0)
            .radius(4.0)
            .fill(if s.inspector_open || s.inspector_popped_out {
                theme::accent_primary().alpha(0.14)
            } else {
                theme::bg_control()
            })
            .hover(theme::hover_subtle())
            .border(
                1.0,
                if s.inspector_open || s.inspector_popped_out {
                    theme::border_strong()
                } else {
                    theme::border_subtle()
                },
            )
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
                text("◧")
                    .font(theme::CONTROL_TEXT)
                    .color(if s.inspector_open || s.inspector_popped_out {
                        theme::text_primary()
                    } else {
                        theme::text_secondary()
                    }),
            ),
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
    let socket_r = (4.5 * zoom).clamp(3.0, 8.0);
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
                Some(doc_reader::split_document_preview_pages(&node.content, 5))
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
                            text("Open →")
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
            if let Some(img) = s.card_image_previews.get(&node.id) {
                col()
                    .key(format!("node_body_{}", node.id.as_str()))
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .clip_soft(0.0)
                    .padding(6.0 * zoom)
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
                        el()
                            .w_full()
                            .grow(1.0)
                            .min_h(0.0)
                            .radius(4.0 * zoom)
                            .fill(theme::bg_input())
                            .border(1.0, theme::border_subtle())
                            .pixels_fit(img.pixels(), SurfaceFit::Fit),
                    )
            } else {
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
                        text("Visual image asset.")
                            .font(TextStyle::new(body_size, 400.0).lh(body_size * 1.4))
                            .color(theme::text_secondary())
                            .w_full()
                            .wrap(),
                    )
            }
        }
        NodeKind::Flashcard => render_flashcard_body(s, node, body_size, badge_size, zoom),
        NodeKind::Excerpt => render_excerpt_body(s, node, body_size, badge_size, zoom),
        NodeKind::Checklist => render_checklist_body(node, body_size, badge_size, zoom),
        NodeKind::Formula => render_formula_body(node, body_size, zoom),
        NodeKind::Sequence => render_sequence_body(node, body_size, badge_size, zoom),
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

    // Assemble card shell (no outer drag/press handlers!)
    col()
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
        .child(out_socket)
}

fn card_header_meta_pill(node: &Node, badge_size: f32, zoom: f32) -> El {
    let (label, accent) = match node.kind {
        NodeKind::Flashcard => {
            let m = node.meta.mastery.as_deref().unwrap_or("new");
            let col = if m == "again" {
                theme::accent_claude()
            } else if m == "easy" || m == "good" {
                theme::text_primary()
            } else {
                theme::text_muted()
            };
            (m.to_uppercase(), col)
        }
        NodeKind::Checklist => {
            let tasks = doc_reader::parse_markdown_tasks(&node.content);
            let done = tasks.iter().filter(|(c, _)| *c).count();
            (
                format!("{}/{}", done, tasks.len()),
                if !tasks.is_empty() && done == tasks.len() {
                    theme::accent_claude()
                } else {
                    theme::text_secondary()
                },
            )
        }
        NodeKind::Formula => (
            node.meta
                .language
                .as_deref()
                .unwrap_or("MATH")
                .to_uppercase(),
            theme::text_secondary(),
        ),
        NodeKind::Sequence => (
            format!("#{:02}", node.meta.step_index.unwrap_or(1)),
            theme::text_primary(),
        ),
        _ => return el(),
    };

    let inspect_id = node.id.clone();
    row()
        .center_y()
        .gap(3.0 * zoom)
        .child(
            el()
                .px((5.0 * zoom).clamp(3.0, 7.0))
                .py((1.5 * zoom).clamp(1.0, 3.0))
                .radius(3.0 * zoom)
                .fill(Color::WHITE.alpha(0.05))
                .border(1.0, theme::border_subtle())
                .child(
                    text(label)
                        .font(TextStyle::new(badge_size, 600.0).mono())
                        .color(accent),
                ),
        )
        .child(
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
                text(if flipped { "↺ Prompt" } else { "Reveal" })
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
                "Again",
                "again",
                cur_mastery == "again",
                again_id,
                theme::accent_claude(),
            ),
            (
                "flash_rate_good_",
                "Good",
                "good",
                cur_mastery == "good",
                good_id,
                theme::text_primary(),
            ),
            (
                "flash_rate_easy_",
                "Easy",
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
        _ => "Unlinked excerpt".into(),
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

fn render_formula_body(node: &Node, body_size: f32, zoom: f32) -> El {
    let select_id = node.id.clone();
    let formula_text = if node.content.trim().is_empty() {
        "y = f(x)"
    } else {
        node.content.trim()
    };

    col()
        .key(format!("node_body_{}", node.id.as_str()))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .clip_soft(0.0)
        .padding(8.0 * zoom)
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
                        .color(theme::text_primary())
                        .w_full()
                        .wrap(),
                ),
        )
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
                            text("+ Next →")
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

    let header_bar = row()
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
                        s.graph.viewport.offset.x = start_offset.x - (d.dx / z);
                        s.graph.viewport.offset.y = start_offset.y - (d.dy / z);
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
        )
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
                    text(if framed { "▭ Frame" } else { "— Banner" })
                        .font(TextStyle::new(badge_size, 600.0))
                        .color(theme::text_secondary()),
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

    if framed {
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

fn render_block_menu_popover(s: &AppState) -> El {
    let menu_x = (s.canvas_size[0] - 252.0).clamp(12.0, 980.0);
    let items = [
        ("btn_add_section", "▭", "Section", "Group frame & title", NodeKind::Section),
        ("btn_add_flashcard", "◇", "Flashcard", "Prompt & flip answer", NodeKind::Flashcard),
        ("btn_add_excerpt", "❝", "Excerpt", "Linked doc page quote", NodeKind::Excerpt),
        ("btn_add_checklist", "☑", "Checklist", "Interactive study tasks", NodeKind::Checklist),
        ("btn_add_formula", "∑", "Formula", "Equation & code box", NodeKind::Formula),
        ("btn_add_sequence", "→", "Sequence", "Numbered causal step", NodeKind::Sequence),
    ];

    let mut menu = col()
        .key("block_create_menu")
        .absolute(menu_x, 8.0)
        .w(238.0)
        .padding(6.0)
        .gap(3.0)
        .radius(6.0)
        .fill(theme::bg_chrome())
        .border(1.0, theme::border_strong())
        .shadow(8.0, 22.0, Color::BLACK.alpha(0.55))
        .child(
            el().px(8.0).py(4.0).child(
                text("STUDY BLOCKS")
                    .font(theme::SECTION_LABEL)
                    .color(theme::text_muted()),
            ),
        );

    for (key, glyph, name, desc, kind) in items {
        menu = menu.child(
            row()
                .key(key)
                .w_full()
                .px(8.0)
                .py(6.0)
                .gap(8.0)
                .center_y()
                .radius(4.0)
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut AppState| {
                    s.add_study_node(kind, None);
                })
                .child(
                    el().w(20.0).center().child(
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
    let label_str = if edge.label.is_empty() {
        format!("[{}]", edge.relation.display_label())
    } else {
        format!("[{}] {}", edge.relation.display_label(), edge.label)
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
}
