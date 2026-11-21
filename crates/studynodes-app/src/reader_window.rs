use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rune::{
    AppSpec, Color, Cursor, Cx, El, ImageCache, Pixels, ScrollSpec, SurfaceFit, TextEdit,
    TextStyle, Titlebar, Tx, Waker, WindowHandle, WindowSpec, col, el, open_window, row, text,
    text_input,
};
use rune_kit::Markdown;
use studynodes_graph::model::NodeId;

use crate::doc_reader::{self, DocFormat, render_pdf_page};
use crate::state::DocViewTab;
use crate::theme;

#[derive(Clone)]
pub struct ReaderSnapshot {
    pub revision: u64,
    pub project_title: String,
    pub node_id: Option<NodeId>,
    pub title: String,
    pub content: String,
    pub summary: Option<String>,
    pub doc_format: DocFormat,
    pub asset_path: Option<PathBuf>,
    pub doc_view_tab: DocViewTab,
    pub pdf_page_index: usize,
    pub pdf_page_count: usize,
    pub pdf_zoom: f32,
}

impl Default for ReaderSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            project_title: "StudyNodes".into(),
            node_id: None,
            title: String::new(),
            content: String::new(),
            summary: None,
            doc_format: DocFormat::Markdown,
            asset_path: None,
            doc_view_tab: DocViewTab::Preview,
            pdf_page_index: 0,
            pdf_page_count: 1,
            pdf_zoom: 1.5,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ReaderCommand {
    DockBack,
    WindowClosed,
    UpdateTitleContent {
        node_id: NodeId,
        title: String,
        content: String,
    },
    ToggleTask {
        node_id: NodeId,
        marker_offset: usize,
        checked: bool,
    },
    AppendText {
        node_id: NodeId,
        text: String,
    },
    SetDocViewTab(DocViewTab),
    SetPdfPage(usize),
    SetPdfZoom(f32),
    OpenExternal(NodeId),
    ExplainAi(NodeId),
}

pub struct ReaderBridge {
    pub snapshot: ReaderSnapshot,
    pub commands: Vec<ReaderCommand>,
    pub main_waker: Option<Waker>,
    pub popout_waker: Option<Waker>,
    pub is_open: bool,
}

impl Default for ReaderBridge {
    fn default() -> Self {
        Self {
            snapshot: ReaderSnapshot::default(),
            commands: Vec::new(),
            main_waker: None,
            popout_waker: None,
            is_open: false,
        }
    }
}

impl ReaderBridge {
    pub fn push_command(&mut self, cmd: ReaderCommand) {
        self.commands.push(cmd);
        if let Some(waker) = &self.main_waker {
            (waker)();
        }
    }

    pub fn wake_popout(&self) {
        if let Some(waker) = &self.popout_waker {
            (waker)();
        }
    }
}

pub struct PopoutReaderState {
    pub bridge: Arc<Mutex<ReaderBridge>>,
    pub applied_revision: u64,
    pub snapshot: ReaderSnapshot,
    pub title_edit: TextEdit,
    pub source_edit: TextEdit,
    pub append_edit: TextEdit,
    pub pdf_pixels: Pixels,
    pub pdf_rendered_key: Option<(PathBuf, usize, u32)>,
    pub pdf_page_size: (u32, u32),
    pub pdf_error: Option<String>,
    pub image_cache: ImageCache,
    pub active_image: Option<(PathBuf, rune::Image)>,
    pub viewport_width: f32,
}

impl Default for PopoutReaderState {
    fn default() -> Self {
        Self {
            bridge: Arc::new(Mutex::new(ReaderBridge::default())),
            applied_revision: 0,
            snapshot: ReaderSnapshot::default(),
            title_edit: TextEdit::new(""),
            source_edit: TextEdit::new(""),
            append_edit: TextEdit::new(""),
            pdf_pixels: Pixels::new(),
            pdf_rendered_key: None,
            pdf_page_size: (0, 0),
            pdf_error: None,
            image_cache: ImageCache::new(Arc::new(|| {})),
            active_image: None,
            viewport_width: 960.0,
        }
    }
}

impl PopoutReaderState {
    pub fn new(bridge: Arc<Mutex<ReaderBridge>>) -> Self {
        let mut s = Self {
            bridge,
            ..Self::default()
        };
        s.sync_from_bridge();
        s
    }

    fn send_cmd(&self, cmd: ReaderCommand) {
        if let Ok(mut b) = self.bridge.lock() {
            b.push_command(cmd);
        }
    }

    pub fn sync_from_bridge(&mut self) -> bool {
        let snap_opt = self.bridge.lock().ok().and_then(|b| {
            if b.snapshot.revision != self.applied_revision || self.applied_revision == 0 {
                Some(b.snapshot.clone())
            } else {
                None
            }
        });

        let Some(snap) = snap_opt else {
            return false;
        };

        self.applied_revision = snap.revision;
        if self.title_edit.text != snap.title {
            self.title_edit = TextEdit::new(&snap.title);
        }
        if self.source_edit.text != snap.content {
            self.source_edit = TextEdit::new(&snap.content);
        }

        self.sync_media_for_snapshot(&snap);
        self.snapshot = snap;
        true
    }

    fn sync_media_for_snapshot(&mut self, snap: &ReaderSnapshot) {
        if snap.doc_format == DocFormat::Pdf {
            self.active_image = None;
            if let Some(path) = &snap.asset_path {
                let zoom_bucket = (snap.pdf_zoom * 100.0).round() as u32;
                let key = (path.clone(), snap.pdf_page_index, zoom_bucket);
                if self.pdf_rendered_key.as_ref() != Some(&key) {
                    match render_pdf_page(path, snap.pdf_page_index, snap.pdf_zoom) {
                        Ok(rendered) => {
                            self.pdf_page_size = (rendered.width, rendered.height);
                            self.pdf_pixels
                                .set(rendered.width, rendered.height, rendered.rgba);
                            self.pdf_error = None;
                            self.pdf_rendered_key = Some(key);
                        }
                        Err(e) => {
                            self.pdf_error = Some(e);
                            self.pdf_rendered_key = Some(key);
                        }
                    }
                }
            }
        } else if snap.doc_format == DocFormat::Image {
            self.pdf_rendered_key = None;
            if let Some(path) = &snap.asset_path {
                let already = self.active_image.as_ref().is_some_and(|(p, _)| p == path);
                if !already {
                    let img = self.image_cache.file(path, 1000.0, 2.0);
                    self.active_image = Some((path.clone(), img));
                }
            } else {
                self.active_image = None;
            }
        } else {
            self.pdf_rendered_key = None;
            self.active_image = None;
        }
    }
}

/// Spawn (or focus) the standalone full-view Document Reader window synchronized with `bridge`.
pub fn open_reader_window(bridge: Arc<Mutex<ReaderBridge>>) -> WindowHandle {
    if let Ok(mut b) = bridge.lock() {
        b.is_open = true;
    }

    let initial_state = PopoutReaderState::new(bridge);

    let mut spec = AppSpec::new(
        "StudyNodes — Reader",
        (960.0, 820.0),
        PopoutReaderState::default,
        render_popout_reader,
    );
    spec.setup = Some(|s, waker| {
        s.image_cache = ImageCache::new(waker.clone());
        if let Ok(mut b) = s.bridge.lock() {
            b.popout_waker = Some(waker);
            b.is_open = true;
        }
        s.sync_from_bridge();
    });
    spec.poll = Some(|s, _dt, _tx| s.sync_from_bridge());

    let win_spec = WindowSpec::new("studynodes_reader", spec)
        .state(initial_state)
        .titlebar(Titlebar::new(40.0))
        .on_close(|s, _| {
            if let Ok(mut b) = s.bridge.lock() {
                b.is_open = false;
                b.popout_waker = None;
                b.push_command(ReaderCommand::WindowClosed);
            }
        });

    open_window(win_spec)
}

fn seg_btn(key: &'static str, label: &str, active: bool) -> El {
    el()
        .key(key)
        .px(9.0)
        .py(4.0)
        .radius(3.0)
        .fill(if active {
            theme::bg_card_selected()
        } else {
            Color::TRANSPARENT
        })
        .hover(theme::hover_subtle())
        .border(
            1.0,
            if active {
                theme::border_strong()
            } else {
                Color::TRANSPARENT
            },
        )
        .center()
        .cursor(Cursor::Pointer)
        .child(
            text(label)
                .font(theme::BADGE_TEXT)
                .color(if active {
                    theme::text_primary()
                } else {
                    theme::text_muted()
                }),
        )
}

fn toolbar_chip(
    key: &'static str,
    label: &str,
    fg: Color,
    on_press: impl Fn(&mut PopoutReaderState) + 'static,
) -> El {
    el()
        .key(key)
        .px(9.0)
        .py(4.5)
        .radius(4.0)
        .fill(theme::bg_control())
        .hover(theme::hover_subtle())
        .press(theme::press_subtle())
        .border(1.0, theme::border_subtle())
        .cursor(Cursor::Pointer)
        .on_press(on_press)
        .child(text(label).font(theme::STATUS_TEXT).color(fg))
}

pub fn render_popout_reader(s: &PopoutReaderState, cx: &mut Cx<PopoutReaderState>) -> El {
    cx.caption_color(theme::text_secondary());
    cx.titlebar_height(40.0);

    let insets = cx.chrome_insets();
    let snap = &s.snapshot;
    let has_pdf = snap.doc_format == DocFormat::Pdf && snap.asset_path.is_some();
    let word_count = snap.content.split_whitespace().count();
    let read_min = (word_count as f32 / 200.0).ceil().max(1.0) as usize;

    let mut titlebar = row()
        .key("popout_titlebar")
        .w_full()
        .h(40.0)
        .pl(insets.leading + 12.0)
        .pr(insets.trailing + 12.0)
        .gap(8.0)
        .center_y()
        .fill(theme::bg_chrome())
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
        .window_drag()
        .child(
            text("✦")
                .font(theme::CONTROL_TEXT)
                .color(theme::accent_claude()),
        )
        .child(
            text("reader")
                .font(theme::TITLE_BAR)
                .color(theme::text_primary()),
        )
        .child(text("·").font(theme::STATUS_TEXT).color(theme::text_muted()))
        .child(
            el().shrink().min_w(0.0).child(
                text(&snap.project_title)
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_secondary())
                    .truncate(),
            ),
        )
        .child(
            el()
                .px(6.0)
                .py(2.0)
                .radius(3.0)
                .fill(theme::bg_control())
                .border(1.0, theme::border_default())
                .child(
                    text(doc_reader::format_badge(snap.doc_format))
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_primary()),
                ),
        )
        .child(
            text(format!("{}w · ~{}m", word_count, read_min))
                .font(theme::STATUS_TEXT)
                .color(theme::text_muted()),
        )
        .child(el().grow(1.0));

    if let Some(nid) = snap.node_id.clone() {
        let mut switcher = row()
            .key("popout_view_tabs")
            .padding(2.0)
            .gap(2.0)
            .radius(4.0)
            .fill(theme::bg_input())
            .border(1.0, theme::border_subtle())
            .no_window_drag();

        if has_pdf {
            switcher = switcher.child(
                seg_btn("popout_tab_pages", "Pages", snap.doc_view_tab == DocViewTab::Pages)
                    .on_press(|s: &mut PopoutReaderState| {
                        s.snapshot.doc_view_tab = DocViewTab::Pages;
                        s.send_cmd(ReaderCommand::SetDocViewTab(DocViewTab::Pages));
                    }),
            );
        }

        switcher = switcher
            .child(
                seg_btn(
                    "popout_tab_preview",
                    "Preview",
                    snap.doc_view_tab == DocViewTab::Preview
                        || (!has_pdf && snap.doc_view_tab == DocViewTab::Pages),
                )
                .on_press(|s: &mut PopoutReaderState| {
                    s.snapshot.doc_view_tab = DocViewTab::Preview;
                    s.send_cmd(ReaderCommand::SetDocViewTab(DocViewTab::Preview));
                }),
            )
            .child(
                seg_btn("popout_tab_source", "Source", snap.doc_view_tab == DocViewTab::Source)
                    .on_press(|s: &mut PopoutReaderState| {
                        s.snapshot.doc_view_tab = DocViewTab::Source;
                        s.send_cmd(ReaderCommand::SetDocViewTab(DocViewTab::Source));
                    }),
            );

        titlebar = titlebar
            .child(switcher)
            .child(
                el()
                    .key("btn_popout_external")
                    .h(26.0)
                    .px(8.0)
                    .radius(4.0)
                    .fill(theme::bg_control())
                    .hover(theme::hover_subtle())
                    .border(1.0, theme::border_subtle())
                    .center()
                    .cursor(Cursor::Pointer)
                    .no_window_drag()
                    .on_press(move |s: &mut PopoutReaderState| {
                        s.send_cmd(ReaderCommand::OpenExternal(nid.clone()));
                    })
                    .child(
                        text("↗")
                            .font(theme::CONTROL_TEXT)
                            .color(theme::text_primary()),
                    ),
            );
    }

    titlebar = titlebar.child(
        el()
            .key("btn_popout_dock_back")
            .h(26.0)
            .px(10.0)
            .radius(4.0)
            .fill(theme::bg_control())
            .hover(theme::hover_subtle())
            .press(theme::press_subtle())
            .border(1.0, theme::border_default())
            .center()
            .cursor(Cursor::Pointer)
            .no_window_drag()
            .on_press_tx(|s: &mut PopoutReaderState, tx: &mut Tx| {
                s.send_cmd(ReaderCommand::DockBack);
                tx.close_window();
            })
            .child(
                text("◧ Dock Back")
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_primary()),
            ),
    );

    let body = if let Some(node_id) = snap.node_id.clone() {
        let nid_for_title = node_id.clone();
        let nid_for_source = node_id.clone();
        let nid_for_ai = node_id.clone();
        let nid_for_task = node_id.clone();
        let nid_for_append = node_id.clone();

        let top_meta = row()
            .w_full()
            .center_y()
            .gap(10.0)
            .child(
                el()
                    .grow(1.0)
                    .min_w(0.0)
                    .px(12.0)
                    .py(8.0)
                    .radius(4.0)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_default())
                    .child(
                        text_input(&s.title_edit, TextStyle::new(15.0, 600.0))
                            .key("popout_doc_title")
                            .label("Document title")
                            .single_line()
                            .w_full()
                            .color(theme::text_primary())
                            .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                            .placeholder("Document title...", theme::text_muted())
                            .on_edit(|s: &mut PopoutReaderState| &mut s.title_edit)
                            .on_input_change(move |s: &mut PopoutReaderState, _| {
                                let title = s.title_edit.text.clone();
                                let content = s.source_edit.text.clone();
                                s.snapshot.title = title.clone();
                                s.send_cmd(ReaderCommand::UpdateTitleContent {
                                    node_id: nid_for_title.clone(),
                                    title,
                                    content,
                                });
                            }),
                    ),
            )
            .child(
                el()
                    .key("btn_popout_ai")
                    .h(32.0)
                    .px(12.0)
                    .radius(4.0)
                    .fill(theme::accent_claude().alpha(0.14))
                    .hover(theme::accent_claude().alpha(0.22))
                    .border(1.0, theme::accent_claude().alpha(0.45))
                    .center()
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut PopoutReaderState| {
                        s.send_cmd(ReaderCommand::ExplainAi(nid_for_ai.clone()));
                    })
                    .child(
                        text("✦ Summarize")
                            .font(theme::CONTROL_TEXT)
                            .color(theme::text_primary()),
                    ),
            );

        let reader_surface = if has_pdf && snap.doc_view_tab == DocViewTab::Pages {
            let cur_page = snap.pdf_page_index;
            let total_pages = snap.pdf_page_count.max(1);
            let cur_zoom = snap.pdf_zoom;
            let aspect = if s.pdf_page_size.0 > 0 && s.pdf_page_size.1 > 0 {
                Some(s.pdf_page_size.0 as f32 / s.pdf_page_size.1 as f32)
            } else {
                None
            };

            let mut pdf_col = col().w_full().grow(1.0).min_h(0.0).gap(10.0).child(
                row()
                    .w_full()
                    .center_y()
                    .gap(8.0)
                    .padding(8.0)
                    .radius(4.0)
                    .fill(theme::bg_card())
                    .border(1.0, theme::border_subtle())
                    .child(toolbar_chip(
                        "popout_pdf_prev",
                        "‹ Prev",
                        theme::text_primary(),
                        move |s| {
                            s.send_cmd(ReaderCommand::SetPdfPage(cur_page.saturating_sub(1)));
                        },
                    ))
                    .child(
                        text(format!("Page {} / {}", cur_page + 1, total_pages))
                            .font(theme::CONTROL_TEXT)
                            .color(theme::text_secondary()),
                    )
                    .child(toolbar_chip(
                        "popout_pdf_next",
                        "Next ›",
                        theme::text_primary(),
                        move |s| {
                            s.send_cmd(ReaderCommand::SetPdfPage(cur_page + 1));
                        },
                    ))
                    .child(el().grow(1.0))
                    .child(toolbar_chip(
                        "popout_pdf_zout",
                        "−",
                        theme::text_secondary(),
                        move |s| {
                            s.send_cmd(ReaderCommand::SetPdfZoom(cur_zoom - 0.25));
                        },
                    ))
                    .child(
                        text(format!("{}%", (cur_zoom * 100.0).round() as u32))
                            .font(theme::STATUS_TEXT)
                            .color(theme::text_muted()),
                    )
                    .child(toolbar_chip(
                        "popout_pdf_zin",
                        "+",
                        theme::text_secondary(),
                        move |s| {
                            s.send_cmd(ReaderCommand::SetPdfZoom(cur_zoom + 0.25));
                        },
                    )),
            );

            if let Some(err) = &s.pdf_error {
                pdf_col = pdf_col.child(
                    el()
                        .w_full()
                        .padding(14.0)
                        .radius(4.0)
                        .fill(theme::bg_input())
                        .border(1.0, theme::accent_danger().alpha(0.45))
                        .child(
                            text(format!("PDF render note: {}", err))
                                .font(theme::BODY_TEXT)
                                .color(theme::text_secondary())
                                .w_full()
                                .wrap(),
                        ),
                );
            } else {
                let mut page_el = el()
                    .key(format!("popout_pdf_canvas_{}", cur_page))
                    .w_full()
                    .max_w(860.0)
                    .radius(4.0)
                    .border(1.0, theme::border_default())
                    .clip_soft(0.0)
                    .pixels_fit(&s.pdf_pixels, SurfaceFit::Fit);
                if let Some(ar) = aspect {
                    page_el = page_el.aspect_ratio(ar);
                } else {
                    page_el = page_el.min_h(520.0);
                }
                pdf_col = pdf_col.child(
                    col()
                        .w_full()
                        .grow(1.0)
                        .min_h(0.0)
                        .items_center()
                        .scroll(ScrollSpec { stick_bottom: false })
                        .child(page_el),
                );
            }
            pdf_col
        } else if snap.doc_view_tab == DocViewTab::Source {
            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .padding(14.0)
                .radius(4.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_default())
                .scroll(ScrollSpec { stick_bottom: false })
                .child(
                    text_input(&s.source_edit, theme::MONO_BODY)
                        .key("popout_doc_source")
                        .label("Document source")
                        .w_full()
                        .min_h(520.0)
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder("Markdown or raw document text...", theme::text_muted())
                        .on_edit(|s: &mut PopoutReaderState| &mut s.source_edit)
                        .on_input_change(move |s: &mut PopoutReaderState, _| {
                            let title = s.title_edit.text.clone();
                            let content = s.source_edit.text.clone();
                            s.snapshot.content = content.clone();
                            s.send_cmd(ReaderCommand::UpdateTitleContent {
                                node_id: nid_for_source.clone(),
                                title,
                                content,
                            });
                        })
                        .on_submit(move |s: &mut PopoutReaderState, _| {
                            s.source_edit.insert("\n");
                            let title = s.title_edit.text.clone();
                            let content = s.source_edit.text.clone();
                            s.snapshot.content = content.clone();
                            s.send_cmd(ReaderCommand::UpdateTitleContent {
                                node_id: node_id.clone(),
                                title,
                                content,
                            });
                        }),
                )
        } else {
            let mut preview_col = col()
                .w_full()
                .max_w(860.0)
                .gap(12.0);

            if let Some((_, img)) = &s.active_image {
                if snap.doc_format == DocFormat::Image {
                    preview_col = preview_col.child(
                        el()
                            .key("popout_image_preview")
                            .w_full()
                            .min_h(360.0)
                            .max_h(560.0)
                            .radius(4.0)
                            .border(1.0, theme::border_default())
                            .pixels_fit(img.pixels(), SurfaceFit::Fit),
                    );
                }
            }

            let md_src = if snap.content.trim().is_empty() {
                "*Empty document. Ingest a file in the workspace or switch to **Source** to write Markdown.*".to_string()
            } else {
                doc_reader::wrap_for_preview(&snap.content, snap.doc_format)
            };

            let md_w = (s.viewport_width - 120.0).clamp(360.0, 820.0);
            let md_style = theme::markdown_style().text_width(md_w);
            let md_el = Markdown::new(&md_src)
                .selectable(true)
                .on_task(move |s: &mut PopoutReaderState, _block_key, offset, checked| {
                    s.send_cmd(ReaderCommand::ToggleTask {
                        node_id: nid_for_task.clone(),
                        marker_offset: offset,
                        checked,
                    });
                })
                .render(&md_style);

            preview_col = preview_col.child(md_el);

            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .padding(20.0)
                .radius(4.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_default())
                .items_center()
                .scroll(ScrollSpec { stick_bottom: false })
                .child(preview_col)
        };

        let mut body_col = col()
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .padding(18.0)
            .gap(12.0)
            .on_layout_tx(|s: &mut PopoutReaderState, r, tx| {
                if r[2] > 100.0 && (s.viewport_width - r[2]).abs() > 6.0 {
                    s.viewport_width = r[2];
                } else {
                    tx.live_only();
                }
            })
            .child(top_meta);

        if let Some(summary) = &snap.summary {
            body_col = body_col.child(
                el()
                    .w_full()
                    .padding(10.0)
                    .radius(4.0)
                    .fill(theme::bg_card())
                    .border(1.0, theme::accent_claude().alpha(0.35))
                    .child(
                        text(format!("✦ {}", summary))
                            .font(theme::BODY_TEXT)
                            .color(theme::text_primary())
                            .w_full()
                            .wrap(),
                    ),
            );
        }

        let nid_append_submit = nid_for_append.clone();
        body_col = body_col.child(reader_surface).child(
            row()
                .w_full()
                .gap(8.0)
                .child(
                    el()
                        .grow(1.0)
                        .px(10.0)
                        .py(7.0)
                        .radius(4.0)
                        .fill(theme::bg_input())
                        .border(1.0, theme::border_default())
                        .child(
                            text_input(&s.append_edit, theme::CONTROL_TEXT)
                                .key("popout_append_note")
                                .label("Append note or excerpt")
                                .single_line()
                                .w_full()
                                .color(theme::text_primary())
                                .caret(
                                    theme::accent_claude(),
                                    theme::accent_claude().alpha(0.25),
                                )
                                .placeholder("Append note or excerpt...", theme::text_muted())
                                .on_edit(|s: &mut PopoutReaderState| &mut s.append_edit)
                                .on_submit(move |s: &mut PopoutReaderState, _| {
                                    let txt = s.append_edit.text.trim().to_string();
                                    if !txt.is_empty() {
                                        s.append_edit = TextEdit::new("");
                                        s.send_cmd(ReaderCommand::AppendText {
                                            node_id: nid_append_submit.clone(),
                                            text: txt,
                                        });
                                    }
                                }),
                        ),
                )
                .child(
                    el()
                        .key("btn_popout_append")
                        .px(12.0)
                        .py(7.0)
                        .radius(4.0)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .border(1.0, theme::border_default())
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut PopoutReaderState| {
                            let txt = s.append_edit.text.trim().to_string();
                            if !txt.is_empty() {
                                s.append_edit = TextEdit::new("");
                                s.send_cmd(ReaderCommand::AppendText {
                                    node_id: nid_for_append.clone(),
                                    text: txt,
                                });
                            }
                        })
                        .child(
                            text("+ Append")
                                .font(theme::CONTROL_TEXT)
                                .color(theme::text_primary()),
                        ),
                ),
        );

        body_col
    } else {
        col()
            .w_full()
            .grow(1.0)
            .center()
            .gap(8.0)
            .child(
                text("No document selected")
                    .font(theme::PANEL_TITLE)
                    .color(theme::text_primary()),
            )
            .child(
                text("Click any Document or Note node on the StudyNodes canvas to read it full-screen here.")
                    .font(theme::BODY_TEXT)
                    .color(theme::text_secondary()),
            )
    };

    col()
        .key("popout_reader_root")
        .w_full()
        .h_full()
        .fill(theme::bg_canvas())
        .child(titlebar)
        .child(body)
}
