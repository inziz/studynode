use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rune::{
    AppSpec, Color, Cursor, Cx, El, FileDialogOptions, ImageCache, LineCap, Pixels, ScrollSpec,
    Shape, SurfaceFit, TextEdit, TextStyle, Titlebar, Tx, Waker, WindowHandle, WindowSpec, canvas,
    col, el, open_window, row, text, text_input,
};
use rune_kit::Markdown;
use studynodes_graph::bezier::CubicBezier;
use studynodes_graph::coords::WorldPos;
use studynodes_graph::model::{NodeId, NodeKind, NodeMeta};
use studynodes_vision::{OcrEnginePreference, OcrMode, SUPPORTED_LANGUAGES};

use crate::doc_reader::{self, DocFormat, render_pdf_page};
use crate::formula_math;
use crate::state::DocViewTab;
use crate::theme;

/// Checklists and Excerpts stay inline on canvas rather than popping into separate windows.
pub fn can_pop_out(kind: NodeKind) -> bool {
    !matches!(kind, NodeKind::Checklist | NodeKind::Excerpt)
}

#[derive(Clone, Debug)]
pub struct SectionChildSnapshot {
    pub id: NodeId,
    pub kind: NodeKind,
    pub title: String,
    pub content: String,
    pub rel_pos: (f32, f32),
    pub size: (f32, f32),
    pub meta: NodeMeta,
    pub flipped: bool,
    pub asset_path: Option<PathBuf>,
    pub image_handle: Option<rune::Image>,
}

#[derive(Clone, Debug)]
pub struct SectionEdgeSnapshot {
    pub from_rel: (f32, f32),
    pub to_rel: (f32, f32),
    pub label: String,
    pub is_ghost: bool,
    pub hide_label: bool,
}

#[derive(Clone)]
pub struct ReaderSnapshot {
    pub revision: u64,
    pub project_title: String,
    pub node_id: Option<NodeId>,
    pub node_kind: NodeKind,
    pub title: String,
    pub content: String,
    pub summary: Option<String>,
    pub meta: NodeMeta,
    pub flashcard_flipped: bool,
    pub section_size: (f32, f32),
    pub section_children: Vec<SectionChildSnapshot>,
    pub section_edges: Vec<SectionEdgeSnapshot>,
    pub doc_format: DocFormat,
    pub asset_path: Option<PathBuf>,
    pub active_image_handle: Option<rune::Image>,
    pub doc_view_tab: DocViewTab,
    pub pdf_page_index: usize,
    pub pdf_page_count: usize,
    pub pdf_zoom: f32,
    pub read_only: bool,
    pub ocr_engine: OcrEnginePreference,
    pub target_language: String,
    pub vision_pending: bool,
    pub vision_downloading_models: bool,
    pub neural_models_ready: bool,
    pub vision_error: Option<String>,
}

impl Default for ReaderSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            project_title: "StudyNodes".into(),
            node_id: None,
            node_kind: NodeKind::Document,
            title: String::new(),
            content: String::new(),
            summary: None,
            meta: NodeMeta::default(),
            flashcard_flipped: false,
            section_size: (640.0, 400.0),
            section_children: Vec::new(),
            section_edges: Vec::new(),
            doc_format: DocFormat::Markdown,
            asset_path: None,
            active_image_handle: None,
            doc_view_tab: DocViewTab::Preview,
            pdf_page_index: 0,
            pdf_page_count: 1,
            pdf_zoom: 1.5,
            read_only: true,
            ocr_engine: OcrEnginePreference::Auto,
            target_language: "es".into(),
            vision_pending: false,
            vision_downloading_models: false,
            neural_models_ready: false,
            vision_error: None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ReaderCommand {
    DockBack,
    WindowClosed,
    SetReadOnly(bool),
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
    FlipFlashcard(NodeId),
    SetFlashcardMastery {
        node_id: NodeId,
        mastery: String,
    },
    UpdateFlashcardAnswer {
        node_id: NodeId,
        answer: String,
    },
    SetFormulaLanguage {
        node_id: NodeId,
        language: String,
    },
    FormatFormula(NodeId),
    InsertFormulaSymbol {
        node_id: NodeId,
        symbol: String,
    },
    AddNextSequenceStep(NodeId),
    ToggleSectionFramed(NodeId),
    ToggleChecklistItem {
        node_id: NodeId,
        task_index: usize,
    },
    IngestImagePath {
        node_id: NodeId,
        path: PathBuf,
    },
    SetOcrEngine(OcrEnginePreference),
    DownloadNeuralModels,
    RunImageOcr {
        node_id: NodeId,
        mode: OcrMode,
    },
    SpawnNoteFromOcr(NodeId),
    SpawnFormulaFromOcr(NodeId),
    SetTargetLanguage(String),
    RunNodeTranslation(NodeId),
    SpawnNoteFromTranslation(NodeId),
    ClearExtractedText(NodeId),
    ClearTranslatedText(NodeId),
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
    pub answer_edit: TextEdit,
    pub append_edit: TextEdit,
    pub pdf_pixels: Pixels,
    pub pdf_rendered_key: Option<(PathBuf, usize, u32)>,
    pub pdf_page_size: (u32, u32),
    pub pdf_error: Option<String>,
    pub image_cache: ImageCache,
    pub active_image: Option<(PathBuf, rune::Image)>,
    pub section_images: HashMap<NodeId, (PathBuf, rune::Image)>,
    pub viewport_width: f32,
    pub copied_slot: Option<&'static str>,
}

impl Default for PopoutReaderState {
    fn default() -> Self {
        Self {
            bridge: Arc::new(Mutex::new(ReaderBridge::default())),
            applied_revision: 0,
            snapshot: ReaderSnapshot::default(),
            title_edit: TextEdit::new(""),
            source_edit: TextEdit::new(""),
            answer_edit: TextEdit::new(""),
            append_edit: TextEdit::new(""),
            pdf_pixels: Pixels::new(),
            pdf_rendered_key: None,
            pdf_page_size: (0, 0),
            pdf_error: None,
            image_cache: ImageCache::new(Arc::new(|| {})),
            active_image: None,
            section_images: HashMap::new(),
            viewport_width: 960.0,
            copied_slot: None,
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
        if snap.node_id != self.snapshot.node_id {
            self.copied_slot = None;
        }
        if self.title_edit.text != snap.title {
            self.title_edit = TextEdit::new(&snap.title);
        }
        if self.source_edit.text != snap.content {
            self.source_edit = TextEdit::new(&snap.content);
        }
        let ans_str = snap.meta.answer.as_deref().unwrap_or("");
        if self.answer_edit.text != ans_str {
            self.answer_edit = TextEdit::new(ans_str);
        }

        self.sync_media_for_snapshot(&snap);
        self.snapshot = snap;
        true
    }

    fn sync_media_for_snapshot(&mut self, snap: &ReaderSnapshot) {
        if snap.node_kind == NodeKind::Section {
            self.section_images.retain(|id, _| {
                snap.section_children.iter().any(|c| &c.id == id && c.kind == NodeKind::Image)
            });
            for child in &snap.section_children {
                if child.kind == NodeKind::Image {
                    if let Some(handle) = &child.image_handle {
                        let p = child.asset_path.clone().unwrap_or_default();
                        self.section_images.insert(child.id.clone(), (p, handle.clone()));
                    } else if let Some(path) = &child.asset_path {
                        let already = self
                            .section_images
                            .get(&child.id)
                            .is_some_and(|(p, _)| p == path);
                        if !already {
                            let img = self.image_cache.file(path, 1024.0, 2.0);
                            self.section_images.insert(child.id.clone(), (path.clone(), img));
                        }
                    }
                }
            }
        } else {
            self.section_images.clear();
        }
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
            if let Some(handle) = &snap.active_image_handle {
                let p = snap.asset_path.clone().unwrap_or_default();
                self.active_image = Some((p, handle.clone()));
            } else if let Some(path) = &snap.asset_path {
                let already = self.active_image.as_ref().is_some_and(|(p, _)| p == path);
                if !already {
                    let img = self.image_cache.file(path, 1024.0, 2.0);
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
        s.applied_revision = 0;
        if s.snapshot.active_image_handle.is_none() {
            s.active_image = None;
        }
        s.section_images.clear();
        s.sync_from_bridge();
    });
    spec.poll = Some(|s, _dt, _tx| s.sync_from_bridge());

    let win_spec = WindowSpec::new("studynodes_reader", spec)
        .state(initial_state)
        .titlebar(Titlebar::new(36.0))
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
        .px(5.0)
        .py(5.0)
        .border_sides(
            [0.0, 0.0, 1.5, 0.0],
            if active {
                theme::accent_claude()
            } else {
                Color::TRANSPARENT
            },
        )
        .hover(theme::hover_subtle())
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
        .px(8.0)
        .py(4.0)
        .radius(6.0)
        .hover(theme::hover_subtle())
        .press(theme::press_subtle())
        .cursor(Cursor::Pointer)
        .on_press(on_press)
        .child(text(label).font(theme::STATUS_TEXT).color(fg))
}

pub fn render_popout_reader(s: &PopoutReaderState, cx: &mut Cx<PopoutReaderState>) -> El {
    cx.caption_color(theme::text_secondary());
    cx.titlebar_height(36.0);

    let insets = cx.chrome_insets();
    let snap = &s.snapshot;
    let is_ro = snap.read_only;
    let has_pdf = snap.doc_format == DocFormat::Pdf && snap.asset_path.is_some();
    let word_count = snap.content.split_whitespace().count();
    let read_min = (word_count as f32 / 200.0).ceil().max(1.0) as usize;

    let kind_lbl = if snap.node_kind == NodeKind::Document {
        doc_reader::format_badge(snap.doc_format)
    } else {
        theme::kind_label(snap.node_kind)
    };

    let mut titlebar = row()
        .key("popout_titlebar")
        .w_full()
        .h(36.0)
        .pl(insets.leading + 14.0)
        .pr(insets.trailing + 14.0)
        .gap(10.0)
        .center_y()
        .fill(theme::bg_canvas())
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
        .window_drag()
        .child(
            text("✦")
                .font(theme::STATUS_TEXT)
                .color(theme::accent_claude()),
        )
        .child(
            el().shrink().min_w(0.0).child(
                text(if is_ro && snap.node_id.is_some() && !snap.title.is_empty() {
                    &snap.title
                } else {
                    &snap.project_title
                })
                .font(theme::STATUS_TEXT)
                .color(theme::text_primary())
                .truncate(),
            ),
        )
        .child(
            text(kind_lbl)
                .font(theme::BADGE_TEXT)
                .color(theme::text_muted()),
        );

    let is_doc_or_note = matches!(
        snap.node_kind,
        NodeKind::Document | NodeKind::Note | NodeKind::LlmSynthesis | NodeKind::Concept
    );
    if !is_ro && is_doc_or_note {
        titlebar = titlebar.child(
            text(format!("{}w · ~{}m", word_count, read_min))
                .font(theme::BADGE_TEXT)
                .color(theme::text_muted()),
        );
    }

    titlebar = titlebar.child(el().grow(1.0));

    if let Some(nid) = snap.node_id.clone() {
        if snap.node_kind == NodeKind::Section {
            let sec_id = nid.clone();
            let framed = snap.meta.section_framed;
            titlebar = titlebar
                .child(
                    text(format!("{} blocks", snap.section_children.len()))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                )
                .child(
                    toolbar_chip(
                        "popout_sec_toggle_frame",
                        if framed { "▭ Frame" } else { "— Banner" },
                        theme::text_secondary(),
                        move |s| {
                            s.snapshot.meta.section_framed = !s.snapshot.meta.section_framed;
                            s.send_cmd(ReaderCommand::ToggleSectionFramed(sec_id.clone()));
                        },
                    )
                    .no_window_drag(),
                );
        } else {
            let mut switcher = row()
                .key("popout_view_tabs")
                .gap(10.0)
                .center_y()
                .no_window_drag();

            if is_doc_or_note && has_pdf {
                switcher = switcher.child(
                    seg_btn("popout_tab_pages", "Pages", snap.doc_view_tab == DocViewTab::Pages)
                        .on_press(|s: &mut PopoutReaderState| {
                            s.snapshot.doc_view_tab = DocViewTab::Pages;
                            s.send_cmd(ReaderCommand::SetDocViewTab(DocViewTab::Pages));
                        }),
                );
            }

            if is_doc_or_note && (has_pdf || !is_ro) {
                switcher = switcher.child(
                    seg_btn(
                        "popout_tab_preview",
                        if has_pdf && is_ro { "Text" } else { "Preview" },
                        snap.doc_view_tab == DocViewTab::Preview
                            || (!has_pdf && snap.doc_view_tab == DocViewTab::Pages),
                    )
                    .on_press(|s: &mut PopoutReaderState| {
                        s.snapshot.doc_view_tab = DocViewTab::Preview;
                        s.send_cmd(ReaderCommand::SetDocViewTab(DocViewTab::Preview));
                    }),
                );
            }

            if is_doc_or_note && !is_ro {
                switcher = switcher.child(
                    seg_btn("popout_tab_source", "Source", snap.doc_view_tab == DocViewTab::Source)
                        .on_press(|s: &mut PopoutReaderState| {
                            s.snapshot.doc_view_tab = DocViewTab::Source;
                            s.send_cmd(ReaderCommand::SetDocViewTab(DocViewTab::Source));
                        }),
                );
            }

            if is_doc_or_note && (has_pdf || !is_ro) {
                titlebar = titlebar.child(switcher);
            }

            titlebar = titlebar.child(
                toolbar_chip(
                    "btn_popout_toggle_readonly",
                    if is_ro { "Edit" } else { "Done" },
                    if is_ro {
                        theme::accent_claude()
                    } else {
                        theme::text_primary()
                    },
                    move |s: &mut PopoutReaderState| {
                        let next = !s.snapshot.read_only;
                        s.snapshot.read_only = next;
                        if next && s.snapshot.doc_view_tab == DocViewTab::Source {
                            s.snapshot.doc_view_tab = if has_pdf {
                                DocViewTab::Pages
                            } else {
                                DocViewTab::Preview
                            };
                        }
                        s.send_cmd(ReaderCommand::SetReadOnly(next));
                    },
                )
                .no_window_drag(),
            );

            if !is_ro && snap.node_kind == NodeKind::Document {
                titlebar = titlebar.child(
                    toolbar_chip("btn_popout_external", "↗", theme::text_secondary(), move |s| {
                        s.send_cmd(ReaderCommand::OpenExternal(nid.clone()));
                    })
                    .no_window_drag(),
                );
            }
        }
    }

    titlebar = titlebar.child(
        el()
            .key("btn_popout_dock_back")
            .px(8.0)
            .py(4.0)
            .radius(6.0)
            .hover(theme::hover_subtle())
            .press(theme::press_subtle())
            .center()
            .cursor(Cursor::Pointer)
            .no_window_drag()
            .on_press_tx(|s: &mut PopoutReaderState, tx: &mut Tx| {
                s.send_cmd(ReaderCommand::DockBack);
                tx.close_window();
            })
            .child(
                text("◧ Dock")
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_secondary()),
            ),
    );

    let body = if let Some(node_id) = snap.node_id.clone() {
        match snap.node_kind {
            NodeKind::Section => render_popout_section_frame(s, snap),
            NodeKind::Flashcard => render_popout_flashcard(s, snap, &node_id, is_ro),
            NodeKind::Formula => render_popout_formula(s, snap, &node_id, is_ro),
            NodeKind::Sequence => render_popout_sequence(s, snap, &node_id, is_ro),
            NodeKind::Image => render_popout_image(s, snap, &node_id, is_ro),
            _ => {
                let nid_for_title = node_id.clone();
                let nid_for_source = node_id.clone();
                let nid_for_ai = node_id.clone();
                let nid_for_task = node_id.clone();
                let nid_for_append = node_id.clone();

                let effective_tab = if is_ro && snap.doc_view_tab == DocViewTab::Source {
                    if has_pdf {
                        DocViewTab::Pages
                    } else {
                        DocViewTab::Preview
                    }
                } else {
                    snap.doc_view_tab
                };

        let reader_surface = if has_pdf && effective_tab == DocViewTab::Pages {
            let cur_page = snap.pdf_page_index;
            let total_pages = snap.pdf_page_count.max(1);
            let cur_zoom = snap.pdf_zoom;
            let aspect = if s.pdf_page_size.0 > 0 && s.pdf_page_size.1 > 0 {
                Some(s.pdf_page_size.0 as f32 / s.pdf_page_size.1 as f32)
            } else {
                None
            };

            let pdf_bar = row()
                .key(if is_ro {
                    "popout_readonly_toolbar"
                } else {
                    "popout_pdf_toolbar"
                })
                .center_y()
                .gap(6.0)
                .px(8.0)
                .py(5.0)
                .radius(6.0)
                .fill(theme::bg_card())
                .border(1.0, theme::border_subtle())
                .child(toolbar_chip(
                    "popout_pdf_prev",
                    if is_ro { "‹" } else { "‹ Prev" },
                    theme::text_primary(),
                    move |s| {
                        s.send_cmd(ReaderCommand::SetPdfPage(cur_page.saturating_sub(1)));
                    },
                ))
                .child(
                    text(if is_ro {
                        format!("{} / {}", cur_page + 1, total_pages)
                    } else {
                        format!("Page {} / {}", cur_page + 1, total_pages)
                    })
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_secondary()),
                )
                .child(toolbar_chip(
                    "popout_pdf_next",
                    if is_ro { "›" } else { "Next ›" },
                    theme::text_primary(),
                    move |s| {
                        s.send_cmd(ReaderCommand::SetPdfPage(cur_page + 1));
                    },
                ))
                .child(el().w(1.0).h(14.0).fill(theme::border_subtle()))
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
                ));

            let mut pdf_col = col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .gap(10.0)
                .child(if is_ro {
                    row().w_full().center().child(pdf_bar)
                } else {
                    pdf_bar.w_full()
                });

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
                    .max_w(900.0)
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
        } else if effective_tab == DocViewTab::Source {
            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .items_center()
                .scroll(ScrollSpec { stick_bottom: false })
                .child(
                    text_input(&s.source_edit, theme::MONO_BODY)
                        .key("popout_doc_source")
                        .label("Document source")
                        .w_full()
                        .max_w(740.0)
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
                .max_w(740.0)
                .gap(16.0);

            if let Some((_, img)) = &s.active_image {
                if snap.doc_format == DocFormat::Image {
                    preview_col = preview_col.child(
                        el()
                            .key("popout_image_preview")
                            .w_full()
                            .min_h(340.0)
                            .max_h(560.0)
                            .radius(6.0)
                            .pixels_fit(img.pixels(), SurfaceFit::Fit),
                    );
                }
            }

            let md_src = if snap.content.trim().is_empty() {
                "*Empty document. Click **Edit** to ingest or write Markdown.*".to_string()
            } else {
                doc_reader::wrap_for_preview(&snap.content, snap.doc_format)
            };

            let md_w = (s.viewport_width - 120.0).clamp(360.0, 740.0);
            let md_style = theme::markdown_style().text_width(md_w);
            let mut md_builder = Markdown::new(&md_src).selectable(true);
            if !is_ro {
                md_builder = md_builder.on_task(
                    move |s: &mut PopoutReaderState, _block_key, offset, checked| {
                        s.send_cmd(ReaderCommand::ToggleTask {
                            node_id: nid_for_task.clone(),
                            marker_offset: offset,
                            checked,
                        });
                    },
                );
            }
            let md_el = md_builder.render(&md_style);

            preview_col = preview_col.child(md_el);

            let content_box = col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .px(24.0)
                .py(18.0)
                .items_center()
                .scroll(ScrollSpec { stick_bottom: false })
                .child(preview_col);

            if is_ro {
                col()
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .gap(8.0)
                    .items_center()
                    .child(
                        col()
                            .key("popout_readonly_toolbar")
                            .w_full()
                            .max_w(740.0)
                            .gap(4.0)
                            .pt(8.0)
                            .child(
                                text(&snap.title)
                                    .font(TextStyle::new(22.0, 600.0))
                                    .color(theme::text_primary())
                                    .w_full()
                                    .wrap(),
                            )
                            .child(
                                text(format!("~{} min read", read_min))
                                    .font(theme::STATUS_TEXT)
                                    .color(theme::text_muted()),
                            )
                            .child(el().w_full().h(1.0).fill(theme::border_subtle())),
                    )
                    .child(content_box)
            } else {
                content_box
            }
        };

        let mut body_col = col()
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .px(24.0)
            .py(16.0)
            .gap(12.0)
            .items_center()
            .on_layout_tx(|s: &mut PopoutReaderState, r, tx| {
                if r[2] > 100.0 && (s.viewport_width - r[2]).abs() > 6.0 {
                    s.viewport_width = r[2];
                } else {
                    tx.live_only();
                }
            });

        if !is_ro {
            let top_meta = row()
                .w_full()
                .max_w(740.0)
                .center_y()
                .gap(10.0)
                .pb(10.0)
                .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
                .child(
                    el().grow(1.0).min_w(0.0).child(
                        text_input(&s.title_edit, TextStyle::new(20.0, 600.0))
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
                .child(toolbar_chip(
                    "btn_popout_ai",
                    "✦ Summarize",
                    theme::accent_claude(),
                    move |s| {
                        s.send_cmd(ReaderCommand::ExplainAi(nid_for_ai.clone()));
                    },
                ));
            body_col = body_col.child(top_meta);

            if let Some(summary) = &snap.summary {
                body_col = body_col.child(
                    el()
                        .w_full()
                        .max_w(740.0)
                        .pl(12.0)
                        .py(2.0)
                        .border_sides([0.0, 0.0, 0.0, 2.0], theme::accent_claude().alpha(0.65))
                        .child(
                            text(format!("✦ {}", summary))
                                .font(theme::BODY_TEXT)
                                .color(theme::text_secondary())
                                .w_full()
                                .wrap(),
                        ),
                );
            }
        }

        body_col = body_col.child(reader_surface);

        if !is_ro {
            let nid_append_submit = nid_for_append.clone();
            body_col = body_col.child(
                row()
                    .w_full()
                    .max_w(740.0)
                    .pt(8.0)
                    .gap(8.0)
                    .center_y()
                    .border_sides([1.0, 0.0, 0.0, 0.0], theme::border_subtle())
                    .child(
                        el().grow(1.0).child(
                            text_input(&s.append_edit, theme::STATUS_TEXT)
                                .key("popout_append_note")
                                .label("Append note or excerpt")
                                .single_line()
                                .w_full()
                                .color(theme::text_primary())
                                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
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
                    .child(toolbar_chip(
                        "btn_popout_append",
                        "+ Append",
                        theme::text_secondary(),
                        move |s| {
                            let txt = s.append_edit.text.trim().to_string();
                            if !txt.is_empty() {
                                s.append_edit = TextEdit::new("");
                                s.send_cmd(ReaderCommand::AppendText {
                                    node_id: nid_for_append.clone(),
                                    text: txt,
                                });
                            }
                        },
                    )),
            );
        }

        body_col
            }
        }
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
        .on_layout_tx(|s: &mut PopoutReaderState, r, tx| {
            if r[2] > 100.0 && (s.viewport_width - r[2]).abs() > 6.0 {
                s.viewport_width = r[2];
            } else {
                tx.live_only();
            }
        })
        .child(titlebar)
        .child(body)
}

fn render_popout_section_frame(s: &PopoutReaderState, snap: &ReaderSnapshot) -> El {
    let avail_w = (s.viewport_width - 56.0).clamp(320.0, 1280.0);
    let sec_w = snap.section_size.0.max(240.0);
    let sec_h = snap.section_size.1.max(120.0);
    let scale = (avail_w / sec_w).clamp(0.45, 1.35);
    let frame_w = sec_w * scale;
    let frame_h = sec_h * scale;

    let mut wire_shape = Shape::new();
    let grid_step = 48.0 * scale;
    if grid_step >= 12.0 {
        let g_col = Color::WHITE.alpha(0.03);
        let mut gx = grid_step;
        while gx < frame_w {
            wire_shape = wire_shape.line([gx, 0.0], [gx, frame_h], 0.5, LineCap::Butt, g_col);
            gx += grid_step;
        }
        let mut gy = grid_step;
        while gy < frame_h {
            wire_shape = wire_shape.line([0.0, gy], [frame_w, gy], 0.5, LineCap::Butt, g_col);
            gy += grid_step;
        }
    }

    for edge in &snap.section_edges {
        let from_wp = WorldPos::new(edge.from_rel.0 * scale, edge.from_rel.1 * scale);
        let to_wp = WorldPos::new(edge.to_rel.0 * scale, edge.to_rel.1 * scale);
        let curve = CubicBezier::wire(from_wp, to_wp);
        let pts: Vec<[f32; 2]> = curve
            .sample_segments(24)
            .into_iter()
            .map(|p| [p.x, p.y])
            .collect();
        let col = if edge.is_ghost {
            theme::wire_proposed().alpha(0.82)
        } else {
            theme::wire_default()
        };
        wire_shape = wire_shape.stroke(pts, (1.8 * scale).clamp(1.0, 2.5), col);
        let dir = curve.tangent(0.98);
        let tip = [to_wp.x - dir.0 * 6.0, to_wp.y - dir.1 * 6.0];
        wire_shape = crate::canvas_view::draw_arrow_triangle(
            wire_shape,
            tip,
            dir,
            (8.5 * scale).clamp(5.0, 12.0),
            (3.8 * scale).clamp(2.4, 5.5),
            col,
        );
    }

    let mut canvas_box = el()
        .key("popout_section_canvas")
        .size(frame_w, frame_h)
        .radius(8.0)
        .clip_rounded(8.0)
        .fill(Color::WHITE.alpha(0.025))
        .border(1.0, theme::border_default())
        .child(
            canvas(wire_shape)
                .key("popout_section_wire_layer")
                .absolute(0.0, 0.0)
                .w_full()
                .h_full(),
        );

    if snap.section_children.is_empty() {
        canvas_box = canvas_box.child(
            col().w_full().h_full().center().gap(6.0).child(
                text("Empty Section Frame")
                    .font(theme::PANEL_TITLE)
                    .color(theme::text_primary()),
            ).child(
                text("Drag cards inside this section on the canvas to study them together here.")
                    .font(theme::BODY_TEXT)
                    .color(theme::text_secondary()),
            ),
        );
    } else {
        for child in &snap.section_children {
            let cw = (child.size.0 * scale).max(120.0);
            let ch = (child.size.1 * scale).max(76.0);
            let cx = child.rel_pos.0 * scale;
            let cy = child.rel_pos.1 * scale;
            let title_sz = (12.0 * scale).clamp(9.0, 16.0);
            let body_sz = (11.0 * scale).clamp(8.5, 14.5);
            let badge_sz = (9.0 * scale).clamp(7.5, 11.5);

            let card_body = match child.kind {
                NodeKind::Flashcard => {
                    let flip_id = child.id.clone();
                    let again_id = child.id.clone();
                    let good_id = child.id.clone();
                    let easy_id = child.id.clone();
                    let flipped = child.flipped;
                    let face_txt = if flipped {
                        child
                            .meta
                            .answer
                            .as_deref()
                            .filter(|a| !a.trim().is_empty())
                            .unwrap_or("No answer recorded.")
                    } else {
                        child.content.trim()
                    };
                    let mut bar = row().w_full().center_y().gap(4.0).child(
                        toolbar_chip(
                            "popout_sec_flash_flip",
                            if flipped { "↺ Prompt" } else { "Reveal" },
                            theme::text_primary(),
                            move |s| s.send_cmd(ReaderCommand::FlipFlashcard(flip_id.clone())),
                        )
                        .key(format!("popout_sec_flash_flip_{}", child.id.as_str())),
                    );
                    if flipped {
                        bar = bar.child(el().grow(1.0));
                        for (pfx, lbl, val, tid) in [
                            ("popout_sec_rate_again_", "Again", "again", again_id),
                            ("popout_sec_rate_good_", "Good", "good", good_id),
                            ("popout_sec_rate_easy_", "Easy", "easy", easy_id),
                        ] {
                            bar = bar.child(
                                el()
                                    .key(format!("{}{}", pfx, child.id.as_str()))
                                    .px(5.0)
                                    .py(2.0)
                                    .radius(3.0)
                                    .fill(theme::bg_control())
                                    .hover(theme::hover_subtle())
                                    .border(1.0, theme::border_subtle())
                                    .cursor(Cursor::Pointer)
                                    .on_press(move |s: &mut PopoutReaderState| {
                                        s.send_cmd(ReaderCommand::SetFlashcardMastery {
                                            node_id: tid.clone(),
                                            mastery: val.to_string(),
                                        });
                                    })
                                    .child(
                                        text(lbl)
                                            .font(TextStyle::new(badge_sz, 600.0))
                                            .color(theme::text_primary()),
                                    ),
                            );
                        }
                    }
                    col()
                        .w_full()
                        .grow(1.0)
                        .min_h(0.0)
                        .gap(4.0)
                        .child(
                            el().grow(1.0).min_h(0.0).clip_soft(0.0).child(
                                text(face_txt)
                                    .font(TextStyle::new(body_sz, 400.0))
                                    .color(theme::text_primary())
                                    .w_full()
                                    .wrap(),
                            ),
                        )
                        .child(bar)
                }
                NodeKind::Checklist => {
                    let tasks = doc_reader::parse_markdown_tasks(&child.content);
                    let mut list = col().w_full().grow(1.0).min_h(0.0).gap(3.0);
                    for (idx, (done, lbl)) in tasks.iter().take(4).enumerate() {
                        let chk_id = child.id.clone();
                        list = list.child(
                            row()
                                .key(format!("popout_sec_chk_{}_{}", child.id.as_str(), idx))
                                .w_full()
                                .center_y()
                                .gap(5.0)
                                .px(4.0)
                                .py(2.0)
                                .radius(3.0)
                                .hover(theme::hover_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press(move |s: &mut PopoutReaderState| {
                                    s.send_cmd(ReaderCommand::ToggleChecklistItem {
                                        node_id: chk_id.clone(),
                                        task_index: idx,
                                    });
                                })
                                .child(
                                    text(if *done { "[x]" } else { "[ ]" })
                                        .font(TextStyle::new(badge_sz, 600.0).mono())
                                        .color(if *done {
                                            theme::accent_claude()
                                        } else {
                                            theme::text_muted()
                                        }),
                                )
                                .child(
                                    el().grow(1.0).min_w(0.0).child(
                                        text(lbl)
                                            .font(TextStyle::new(body_sz, 400.0))
                                            .color(theme::text_primary())
                                            .truncate(),
                                    ),
                                ),
                        );
                    }
                    list
                }
                NodeKind::Sequence => {
                    let seq_id = child.id.clone();
                    col()
                        .w_full()
                        .grow(1.0)
                        .min_h(0.0)
                        .gap(4.0)
                        .child(
                            el().grow(1.0).min_h(0.0).clip_soft(0.0).child(
                                text(child.content.trim())
                                    .font(TextStyle::new(body_sz, 400.0))
                                    .color(theme::text_secondary())
                                    .w_full()
                                    .wrap(),
                            ),
                        )
                        .child(
                            row().w_full().child(el().grow(1.0)).child(
                                el()
                                    .key(format!("popout_sec_seq_next_{}", child.id.as_str()))
                                    .px(6.0)
                                    .py(2.0)
                                    .radius(3.0)
                                    .fill(theme::bg_control())
                                    .hover(theme::hover_subtle())
                                    .border(1.0, theme::border_subtle())
                                    .cursor(Cursor::Pointer)
                                    .on_press(move |s: &mut PopoutReaderState| {
                                        s.send_cmd(ReaderCommand::AddNextSequenceStep(
                                            seq_id.clone(),
                                        ));
                                    })
                                    .child(
                                        text("+ Next →")
                                            .font(TextStyle::new(badge_sz, 600.0))
                                            .color(theme::text_primary()),
                                    ),
                            ),
                        )
                }
                NodeKind::Image => {
                    if let Some((_, img)) = s.section_images.get(&child.id) {
                        el().w_full()
                            .grow(1.0)
                            .min_h(0.0)
                            .radius(4.0)
                            .pixels_fit(img.pixels(), SurfaceFit::Fit)
                    } else {
                        text("Image block")
                            .font(TextStyle::new(body_sz, 400.0))
                            .color(theme::text_muted())
                    }
                }
                _ => el()
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .clip_soft(0.0)
                    .child(
                        text(if child.content.trim().is_empty() {
                            theme::kind_label(child.kind)
                        } else {
                            child.content.trim()
                        })
                        .font(TextStyle::new(body_sz, 400.0))
                        .color(theme::text_secondary())
                        .w_full()
                        .wrap(),
                    ),
            };

            canvas_box = canvas_box.child(
                col()
                    .key(format!("popout_sec_child_{}", child.id.as_str()))
                    .absolute(cx, cy)
                    .size(cw, ch)
                    .radius(6.0)
                    .clip_rounded(6.0)
                    .fill(theme::bg_card())
                    .border(1.0, theme::border_default())
                    .child(
                        row()
                            .w_full()
                            .px(8.0)
                            .py(5.0)
                            .gap(5.0)
                            .center_y()
                            .fill(theme::bg_card_header())
                            .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
                            .child(
                                text(theme::kind_glyph(child.kind))
                                    .font(TextStyle::new(badge_sz, 600.0).mono())
                                    .color(theme::kind_accent(child.kind)),
                            )
                            .child(
                                el().grow(1.0).min_w(0.0).child(
                                    text(&child.title)
                                        .font(TextStyle::new(title_sz, 600.0))
                                        .color(theme::text_primary())
                                        .truncate(),
                                ),
                            ),
                    )
                    .child(
                        col()
                            .w_full()
                            .grow(1.0)
                            .min_h(0.0)
                            .padding(8.0)
                            .child(card_body),
                    ),
            );
        }

        for edge in &snap.section_edges {
            if !edge.hide_label && !edge.label.is_empty() {
                let from_wp = WorldPos::new(edge.from_rel.0 * scale, edge.from_rel.1 * scale);
                let to_wp = WorldPos::new(edge.to_rel.0 * scale, edge.to_rel.1 * scale);
                let mid = CubicBezier::wire(from_wp, to_wp).midpoint();
                let fsz = (9.5 * scale).clamp(7.5, 11.5);
                let est_w = (edge.label.chars().count() as f32 * fsz * 0.6 + 14.0 * scale)
                    .max(44.0 * scale);
                let est_h = fsz + 7.0 * scale;
                canvas_box = canvas_box.child(
                    el()
                        .absolute(mid.x - est_w * 0.5, mid.y - est_h * 0.5)
                        .px(6.0 * scale)
                        .py(2.5 * scale)
                        .radius(4.0)
                        .fill(if edge.is_ghost {
                            theme::hex("#1C1715")
                        } else {
                            theme::bg_chrome()
                        })
                        .border(
                            1.0,
                            if edge.is_ghost {
                                theme::accent_claude().alpha(0.65)
                            } else {
                                theme::border_default()
                            },
                        )
                        .child(
                            text(&edge.label)
                                .font(TextStyle::new(fsz, 500.0).mono())
                                .color(if edge.is_ghost {
                                    theme::accent_claude()
                                } else {
                                    theme::text_secondary()
                                }),
                        ),
                );
            }
        }
    }

    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .padding(18.0)
        .items_center()
        .scroll(ScrollSpec { stick_bottom: false })
        .child(canvas_box)
}

fn render_popout_flashcard(
    s: &PopoutReaderState,
    snap: &ReaderSnapshot,
    node_id: &NodeId,
    is_ro: bool,
) -> El {
    let flipped = snap.flashcard_flipped;
    let cur_mastery = snap.meta.mastery.as_deref().unwrap_or("new");
    let flip_id = node_id.clone();

    let mut card = col()
        .key("popout_flashcard_view")
        .w_full()
        .max_w(680.0)
        .padding(24.0)
        .gap(16.0)
        .radius(8.0)
        .fill(theme::bg_card())
        .border(1.0, theme::border_default())
        .child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(
                    text(theme::kind_glyph(NodeKind::Flashcard))
                        .font(theme::CONTROL_TEXT)
                        .color(theme::kind_accent(NodeKind::Flashcard)),
                )
                .child(
                    text("FLASHCARD REVIEW")
                        .font(theme::SECTION_LABEL)
                        .color(theme::text_muted()),
                )
                .child(el().grow(1.0))
                .child(
                    el()
                        .px(8.0)
                        .py(3.0)
                        .radius(4.0)
                        .fill(theme::bg_control())
                        .border(1.0, theme::border_subtle())
                        .child(
                            text(cur_mastery.to_uppercase())
                                .font(theme::BADGE_TEXT)
                                .color(if cur_mastery == "again" {
                                    theme::accent_claude()
                                } else {
                                    theme::text_primary()
                                }),
                        ),
                ),
        );

    if is_ro {
        card = card.child(
            text(&snap.title)
                .font(TextStyle::new(18.0, 600.0))
                .color(theme::text_primary()),
        );
    } else {
        let nid_t = node_id.clone();
        card = card.child(
            el()
                .w_full()
                .px(12.0)
                .py(8.0)
                .radius(4.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_default())
                .child(
                    text_input(&s.title_edit, TextStyle::new(16.0, 600.0))
                        .key("popout_doc_title")
                        .label("Flashcard title")
                        .single_line()
                        .w_full()
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .on_edit(|s: &mut PopoutReaderState| &mut s.title_edit)
                        .on_input_change(move |s: &mut PopoutReaderState, _| {
                            let title = s.title_edit.text.clone();
                            let content = s.source_edit.text.clone();
                            s.snapshot.title = title.clone();
                            s.send_cmd(ReaderCommand::UpdateTitleContent {
                                node_id: nid_t.clone(),
                                title,
                                content,
                            });
                        }),
                ),
        );
    }

    card = card.child(
        col()
            .w_full()
            .gap(6.0)
            .padding(16.0)
            .radius(6.0)
            .fill(theme::bg_input())
            .border(1.0, theme::border_subtle())
            .child(
                text(if flipped && is_ro { "ANSWER" } else { "PROMPT" })
                    .font(theme::SECTION_LABEL)
                    .color(if flipped && is_ro {
                        theme::accent_claude()
                    } else {
                        theme::text_muted()
                    }),
            )
            .child(if is_ro {
                let face = if flipped {
                    snap.meta
                        .answer
                        .as_deref()
                        .filter(|a| !a.trim().is_empty())
                        .unwrap_or("No answer recorded yet.")
                } else {
                    snap.content.trim()
                };
                text(face)
                    .font(TextStyle::new(15.0, 400.0).lh(23.0))
                    .color(theme::text_primary())
                    .w_full()
                    .wrap()
            } else {
                let nid_p = node_id.clone();
                text_input(&s.source_edit, theme::BODY_TEXT)
                    .key("popout_doc_source")
                    .label("Flashcard prompt")
                    .w_full()
                    .min_h(90.0)
                    .color(theme::text_primary())
                    .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                    .on_edit(|s: &mut PopoutReaderState| &mut s.source_edit)
                    .on_input_change(move |s: &mut PopoutReaderState, _| {
                        let title = s.title_edit.text.clone();
                        let content = s.source_edit.text.clone();
                        s.snapshot.content = content.clone();
                        s.send_cmd(ReaderCommand::UpdateTitleContent {
                            node_id: nid_p.clone(),
                            title,
                            content,
                        });
                    })
            }),
    );

    if !is_ro {
        let nid_a = node_id.clone();
        card = card.child(
            col()
                .w_full()
                .gap(6.0)
                .padding(16.0)
                .radius(6.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_default())
                .child(
                    text("ANSWER / BACK")
                        .font(theme::SECTION_LABEL)
                        .color(theme::accent_claude()),
                )
                .child(
                    text_input(&s.answer_edit, theme::BODY_TEXT)
                        .key("popout_flashcard_answer")
                        .label("Flashcard answer")
                        .w_full()
                        .min_h(90.0)
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .on_edit(|s: &mut PopoutReaderState| &mut s.answer_edit)
                        .on_input_change(move |s: &mut PopoutReaderState, _| {
                            let answer = s.answer_edit.text.clone();
                            s.snapshot.meta.answer = Some(answer.clone());
                            s.send_cmd(ReaderCommand::UpdateFlashcardAnswer {
                                node_id: nid_a.clone(),
                                answer,
                            });
                        }),
                ),
        );
    }

    let mut action_row = row().w_full().center_y().gap(8.0).child(
        el()
            .key("btn_popout_flash_flip")
            .px(14.0)
            .py(8.0)
            .radius(4.0)
            .fill(if flipped {
                theme::accent_claude().alpha(0.18)
            } else {
                theme::accent_primary()
            })
            .hover(theme::hover_subtle())
            .border(1.0, theme::border_default())
            .cursor(Cursor::Pointer)
            .on_press(move |s: &mut PopoutReaderState| {
                s.snapshot.flashcard_flipped = !s.snapshot.flashcard_flipped;
                s.send_cmd(ReaderCommand::FlipFlashcard(flip_id.clone()));
            })
            .child(
                text(if flipped { "↺ Prompt" } else { "Reveal" })
                    .font(theme::CONTROL_TEXT)
                    .color(if flipped {
                        theme::accent_claude()
                    } else {
                        theme::text_inverted()
                    }),
            ),
    );

    action_row = action_row.child(el().grow(1.0));
    for (key, lbl, val) in [
        ("btn_popout_flash_rate_again", "Again", "again"),
        ("btn_popout_flash_rate_good", "Good", "good"),
        ("btn_popout_flash_rate_easy", "Easy", "easy"),
    ] {
        let nid_r = node_id.clone();
        let active = cur_mastery == val;
        action_row = action_row.child(
            seg_btn(key, lbl, active).on_press(move |s: &mut PopoutReaderState| {
                s.snapshot.meta.mastery = Some(val.to_string());
                s.send_cmd(ReaderCommand::SetFlashcardMastery {
                    node_id: nid_r.clone(),
                    mastery: val.to_string(),
                });
            }),
        );
    }

    col()
        .w_full()
        .grow(1.0)
        .padding(24.0)
        .items_center()
        .scroll(ScrollSpec { stick_bottom: false })
        .child(card.child(action_row))
}

fn render_popout_formula(
    s: &PopoutReaderState,
    snap: &ReaderSnapshot,
    node_id: &NodeId,
    is_ro: bool,
) -> El {
    let cur_lang = snap.meta.language.as_deref().unwrap_or("MATH");
    let mut lang_row = row().w_full().center_y().gap(6.0);
    for (k, lang) in [
        ("popout_lang_math", "MATH"),
        ("popout_lang_rust", "RUST"),
        ("popout_lang_py", "PY"),
        ("popout_lang_chem", "CHEM"),
    ] {
        let nid_l = node_id.clone();
        lang_row = lang_row.child(
            seg_tab_popout(k, lang, cur_lang.eq_ignore_ascii_case(lang)).on_press(
                move |s: &mut PopoutReaderState| {
                    s.snapshot.meta.language = Some(lang.to_string());
                    s.send_cmd(ReaderCommand::SetFormulaLanguage {
                        node_id: nid_l.clone(),
                        language: lang.to_string(),
                    });
                },
            ),
        );
    }
    let nid_fmt = node_id.clone();
    lang_row = lang_row.child(el().grow(1.0)).child(
        toolbar_chip(
            "btn_popout_formula_format",
            "\\tex → ∑ Format",
            theme::accent_claude(),
            move |s| s.send_cmd(ReaderCommand::FormatFormula(nid_fmt.clone())),
        ),
    );

    let mut card = col()
        .key("popout_formula_view")
        .w_full()
        .max_w(720.0)
        .padding(24.0)
        .gap(14.0)
        .radius(8.0)
        .fill(theme::bg_card())
        .border(1.0, theme::border_default())
        .child(
            text(&snap.title)
                .font(TextStyle::new(17.0, 600.0))
                .color(theme::text_primary()),
        )
        .child(lang_row)
        .child(
            col()
                .w_full()
                .min_h(140.0)
                .padding(18.0)
                .radius(6.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_default())
                .child(if is_ro {
                    text(if snap.content.trim().is_empty() {
                        "Empty equation."
                    } else {
                        snap.content.trim()
                    })
                    .font(TextStyle::new(16.0, 500.0).mono().lh(24.0))
                    .color(theme::text_primary())
                    .w_full()
                    .wrap()
                } else {
                    let nid_f = node_id.clone();
                    text_input(&s.source_edit, TextStyle::new(15.0, 500.0).mono().lh(22.0))
                        .key("popout_formula_input")
                        .label("Formula source")
                        .w_full()
                        .min_h(110.0)
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .on_edit(|s: &mut PopoutReaderState| &mut s.source_edit)
                        .on_input_change(move |s: &mut PopoutReaderState, _| {
                            let title = s.title_edit.text.clone();
                            let content = s.source_edit.text.clone();
                            s.snapshot.content = content.clone();
                            s.send_cmd(ReaderCommand::UpdateTitleContent {
                                node_id: nid_f.clone(),
                                title,
                                content,
                            });
                        })
                }),
        );

    if !is_ro {
        let mut sym_rows = col().w_full().gap(6.0);
        for (r_idx, chunk) in formula_math::OP_SYMBOLS.chunks(10).enumerate() {
            let mut r = row().w_full().gap(5.0);
            for (c_idx, sym) in chunk.iter().enumerate() {
                let nid_s = node_id.clone();
                let sym_s = *sym;
                r = r.child(
                    el()
                        .key(format!("popout_sym_{}_{}", r_idx, c_idx))
                        .min_w(32.0)
                        .h(28.0)
                        .px(6.0)
                        .radius(4.0)
                        .fill(theme::bg_control())
                        .hover(theme::hover_subtle())
                        .border(1.0, theme::border_subtle())
                        .center()
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut PopoutReaderState| {
                            s.send_cmd(ReaderCommand::InsertFormulaSymbol {
                                node_id: nid_s.clone(),
                                symbol: sym_s.to_string(),
                            });
                        })
                        .child(
                            text(sym_s)
                                .font(TextStyle::new(13.0, 500.0).mono())
                                .color(theme::text_primary()),
                        ),
                );
            }
            sym_rows = sym_rows.child(r);
        }
        card = card.child(sym_rows);
    }

    col()
        .w_full()
        .grow(1.0)
        .padding(24.0)
        .items_center()
        .scroll(ScrollSpec { stick_bottom: false })
        .child(card)
}

fn render_popout_sequence(
    s: &PopoutReaderState,
    snap: &ReaderSnapshot,
    node_id: &NodeId,
    is_ro: bool,
) -> El {
    let step_num = snap.meta.step_index.unwrap_or(1);
    let seq_id = node_id.clone();

    let card = col()
        .key("popout_sequence_view")
        .w_full()
        .max_w(680.0)
        .padding(24.0)
        .gap(16.0)
        .radius(8.0)
        .fill(theme::bg_card())
        .border(1.0, theme::border_default())
        .child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(
                    text(format!("STEP #{:02}", step_num))
                        .font(TextStyle::new(12.0, 600.0).mono())
                        .color(theme::accent_claude()),
                )
                .child(el().grow(1.0))
                .child(
                    toolbar_chip(
                        "btn_popout_seq_next",
                        "+ Spawn Next Step →",
                        theme::text_primary(),
                        move |s| s.send_cmd(ReaderCommand::AddNextSequenceStep(seq_id.clone())),
                    ),
                ),
        )
        .child(
            text(&snap.title)
                .font(TextStyle::new(18.0, 600.0))
                .color(theme::text_primary()),
        )
        .child(
            col()
                .w_full()
                .min_h(150.0)
                .padding(16.0)
                .radius(6.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_subtle())
                .child(if is_ro {
                    text(if snap.content.trim().is_empty() {
                        "Describe this step..."
                    } else {
                        snap.content.trim()
                    })
                    .font(TextStyle::new(14.0, 400.0).lh(22.0))
                    .color(theme::text_primary())
                    .w_full()
                    .wrap()
                } else {
                    let nid_c = node_id.clone();
                    text_input(&s.source_edit, theme::BODY_TEXT)
                        .key("popout_doc_source")
                        .label("Sequence step details")
                        .w_full()
                        .min_h(120.0)
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .on_edit(|s: &mut PopoutReaderState| &mut s.source_edit)
                        .on_input_change(move |s: &mut PopoutReaderState, _| {
                            let title = s.title_edit.text.clone();
                            let content = s.source_edit.text.clone();
                            s.snapshot.content = content.clone();
                            s.send_cmd(ReaderCommand::UpdateTitleContent {
                                node_id: nid_c.clone(),
                                title,
                                content,
                            });
                        })
                }),
        );

    col()
        .w_full()
        .grow(1.0)
        .padding(24.0)
        .items_center()
        .scroll(ScrollSpec { stick_bottom: false })
        .child(card)
}

fn render_popout_image(
    s: &PopoutReaderState,
    snap: &ReaderSnapshot,
    node_id: &NodeId,
    is_ro: bool,
) -> El {
    let nid_pick = node_id.clone();
    let mut box_col = col()
        .key("popout_image_view")
        .w_full()
        .max_w(740.0)
        .gap(16.0)
        .items_center();

    if !is_ro {
        box_col = box_col.child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(
                    text(&snap.title)
                        .font(TextStyle::new(16.0, 600.0))
                        .color(theme::text_primary()),
                )
                .child(el().grow(1.0))
                .child(
                    el()
                        .key("btn_popout_img_pick")
                        .px(12.0)
                        .py(6.0)
                        .radius(4.0)
                        .fill(theme::accent_primary())
                        .hover(theme::accent_primary().alpha(0.88))
                        .cursor(Cursor::Pointer)
                        .on_press_tx(move |_s: &mut PopoutReaderState, tx: &mut Tx| {
                            let opts = FileDialogOptions {
                                title: "Choose Image File".into(),
                                ..Default::default()
                            }
                            .filter(
                                "Images",
                                &["png", "jpg", "jpeg", "webp", "gif", "bmp", "svg"],
                            );
                            let tid = nid_pick.clone();
                            tx.open_file(opts, move |s: &mut PopoutReaderState, res, _| {
                                if let Ok(Some(path)) = res {
                                    s.send_cmd(ReaderCommand::IngestImagePath {
                                        node_id: tid.clone(),
                                        path,
                                    });
                                }
                            });
                        })
                        .child(
                            text("Choose Image")
                                .font(theme::CONTROL_TEXT)
                                .color(theme::text_inverted()),
                        ),
                ),
        );
    }

    if let Some((_, img)) = &s.active_image {
        let img_h = if let Some([iw, ih]) = img.size() {
            if iw > 0 && ih > 0 {
                (820.0 * (ih as f32 / iw as f32)).clamp(260.0, 520.0)
            } else {
                360.0
            }
        } else {
            360.0
        };
        if let Some(err) = img.error() {
            box_col = box_col.child(
                col()
                    .key("popout_image_preview")
                    .w_full()
                    .h(180.0)
                    .radius(6.0)
                    .fill(theme::bg_input())
                    .border(1.0, theme::accent_danger().alpha(0.45))
                    .center()
                    .child(
                        text(format!("Image error: {}", err))
                            .font(theme::STATUS_TEXT)
                            .color(theme::accent_danger())
                            .wrap(),
                    ),
            );
        } else if img.is_ready() {
            box_col = box_col.child(
                el()
                    .key("popout_image_preview")
                    .w_full()
                    .h(img_h)
                    .radius(6.0)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_default())
                    .pixels_fit(img.pixels(), SurfaceFit::Fit),
            );
        } else {
            box_col = box_col.child(
                el()
                    .key("popout_image_preview")
                    .w_full()
                    .h(img_h)
                    .radius(6.0)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_subtle())
                    .center()
                    .child(
                        text("Loading image...")
                            .font(theme::STATUS_TEXT)
                            .color(theme::text_muted()),
                    ),
            );
        }
    } else {
        box_col = box_col.child(
            col()
                .w_full()
                .h(220.0)
                .radius(6.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_default())
                .center()
                .child(
                    text("No image file loaded yet.")
                        .font(theme::BODY_TEXT)
                        .color(theme::text_muted()),
                ),
        );
    }

    // Minimal 1-row OCR + Engine + Translation strip
    let nid_ocr_txt = node_id.clone();
    let nid_ocr_eq = node_id.clone();
    let nid_spawn_note = node_id.clone();
    let nid_spawn_eq = node_id.clone();
    let nid_clear_ocr = node_id.clone();
    let nid_tr = node_id.clone();
    let nid_spawn_tr = node_id.clone();
    let nid_clear_tr = node_id.clone();
    let engine = snap.ocr_engine;
    let is_busy = snap.vision_pending;
    let local_selected = engine != OcrEnginePreference::CloudVision;

    let mut ocr_row = row()
        .key("popout_image_ocr_panel")
        .w_full()
        .center_y()
        .gap(8.0)
        .child(
            el()
                .key("btn_popout_img_ocr_text")
                .px(11.0)
                .py(5.5)
                .radius(4.0)
                .fill(theme::accent_primary())
                .hover(theme::accent_primary().alpha(0.88))
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut PopoutReaderState| {
                    s.snapshot.vision_pending = true;
                    s.send_cmd(ReaderCommand::RunImageOcr {
                        node_id: nid_ocr_txt.clone(),
                        mode: OcrMode::PlainText,
                    });
                })
                .child(
                    text(if is_busy { "Extracting..." } else { "Extract Text" })
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_inverted()),
                ),
        )
        .child(toolbar_chip(
            "btn_popout_img_ocr_formula",
            "Formula",
            theme::text_primary(),
            move |s| {
                s.snapshot.vision_pending = true;
                s.send_cmd(ReaderCommand::RunImageOcr {
                    node_id: nid_ocr_eq.clone(),
                    mode: OcrMode::EquationFormula,
                });
            },
        ))
        .child(el().grow(1.0))
        .child(toolbar_chip(
            "btn_popout_cycle_ocr_engine",
            engine.short_label(),
            theme::text_secondary(),
            |s| {
                let next = match s.snapshot.ocr_engine {
                    OcrEnginePreference::Auto => OcrEnginePreference::LocalPpOcrV5,
                    OcrEnginePreference::LocalPpOcrV5 => OcrEnginePreference::CloudVision,
                    OcrEnginePreference::CloudVision => OcrEnginePreference::Auto,
                };
                s.snapshot.ocr_engine = next;
                s.send_cmd(ReaderCommand::SetOcrEngine(next));
            },
        ));

    if local_selected && !snap.neural_models_ready {
        ocr_row = ocr_row.child(toolbar_chip(
            "btn_popout_download_neural",
            if snap.vision_downloading_models {
                "..."
            } else {
                "↓ Models"
            },
            theme::accent_claude(),
            |s| {
                s.snapshot.vision_downloading_models = true;
                s.send_cmd(ReaderCommand::DownloadNeuralModels);
            },
        ));
    }

    let mut ocr_card = col()
        .w_full()
        .gap(12.0)
        .pt(12.0)
        .border_sides([1.0, 0.0, 0.0, 0.0], theme::border_subtle())
        .child(ocr_row);

    if let Some(err) = &snap.vision_error {
        ocr_card = ocr_card.child(
            text(err)
                .font(theme::STATUS_TEXT)
                .color(theme::accent_danger())
                .w_full()
                .wrap(),
        );
    }

    if let Some(extracted) = snap
        .meta
        .extracted_text
        .as_deref()
        .filter(|t| !t.trim().is_empty())
    {
        let eng = snap.meta.ocr_engine.as_deref().unwrap_or("OCR");
        let extracted_owned = extracted.to_string();
        let ocr_copied = s.copied_slot == Some("ocr");
        ocr_card = ocr_card.child(
            col()
                .key("popout_ocr_extracted_preview")
                .w_full()
                .gap(8.0)
                .pl(14.0)
                .py(2.0)
                .border_sides([0.0, 0.0, 0.0, 2.0], theme::border_strong())
                .child(
                    text(extracted)
                        .font(theme::BODY_TEXT)
                        .color(theme::text_primary())
                        .w_full()
                        .wrap(),
                )
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .gap(4.0)
                        .child(
                            text(eng)
                                .font(theme::BADGE_TEXT)
                                .color(theme::text_muted()),
                        )
                        .child(el().grow(1.0))
                        .child(
                            el()
                                .key("btn_popout_ocr_copy")
                                .px(7.0)
                                .py(3.5)
                                .radius(5.0)
                                .hover(theme::hover_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press_tx(move |s: &mut PopoutReaderState, tx: &mut Tx| {
                                    tx.copy(extracted_owned.clone());
                                    s.copied_slot = Some("ocr");
                                })
                                .child(
                                    text(if ocr_copied { "Copied ✓" } else { "Copy" })
                                        .font(theme::STATUS_TEXT)
                                        .color(if ocr_copied {
                                            theme::accent_claude()
                                        } else {
                                            theme::text_secondary()
                                        }),
                                ),
                        )
                        .child(toolbar_chip(
                            "btn_popout_ocr_spawn_note",
                            "+ Note",
                            theme::text_secondary(),
                            move |s| {
                                s.send_cmd(ReaderCommand::SpawnNoteFromOcr(nid_spawn_note.clone()));
                            },
                        ))
                        .child(toolbar_chip(
                            "btn_popout_ocr_spawn_formula",
                            "+ Formula",
                            theme::text_secondary(),
                            move |s| {
                                s.send_cmd(ReaderCommand::SpawnFormulaFromOcr(
                                    nid_spawn_eq.clone(),
                                ));
                            },
                        ))
                        .child(toolbar_chip(
                            "btn_popout_ocr_clear",
                            "×",
                            theme::text_muted(),
                            move |s| {
                                s.snapshot.meta.extracted_text = None;
                                s.copied_slot = None;
                                s.send_cmd(ReaderCommand::ClearExtractedText(nid_clear_ocr.clone()));
                            },
                        )),
                ),
        );
    }

    let active_lang = snap.target_language.to_ascii_lowercase();
    let mut lang_strip = row().center_y().gap(6.0);
    for &(code, label) in SUPPORTED_LANGUAGES {
        let key: &'static str = match code {
            "en" => "popout_lang_en",
            "es" => "popout_lang_es",
            "fr" => "popout_lang_fr",
            "de" => "popout_lang_de",
            "pt" => "popout_lang_pt",
            "ja" => "popout_lang_ja",
            _ => "popout_lang_en",
        };
        lang_strip = lang_strip.child(seg_btn(key, label, active_lang == code).on_press(
            move |s: &mut PopoutReaderState| {
                s.snapshot.target_language = code.to_string();
                s.send_cmd(ReaderCommand::SetTargetLanguage(code.to_string()));
            },
        ));
    }
    let lang_bar = row()
        .w_full()
        .pt(8.0)
        .center_y()
        .gap(8.0)
        .border_sides([1.0, 0.0, 0.0, 0.0], theme::border_subtle())
        .child(lang_strip)
        .child(el().grow(1.0))
        .child(toolbar_chip(
            "btn_popout_translate_node",
            if is_busy { "Translating..." } else { "Translate" },
            theme::text_primary(),
            move |s| {
                s.snapshot.vision_pending = true;
                s.send_cmd(ReaderCommand::RunNodeTranslation(nid_tr.clone()));
            },
        ));
    ocr_card = ocr_card.child(lang_bar);

    if let Some(tr) = snap
        .meta
        .translated_text
        .as_deref()
        .filter(|t| !t.trim().is_empty())
    {
        let tr_lang = snap
            .meta
            .target_language
            .as_deref()
            .unwrap_or(&active_lang)
            .to_uppercase();
        let tr_owned = tr.to_string();
        let tr_copied = s.copied_slot == Some("translation");
        ocr_card = ocr_card.child(
            col()
                .key("popout_translated_preview")
                .w_full()
                .gap(8.0)
                .pl(14.0)
                .py(2.0)
                .border_sides([0.0, 0.0, 0.0, 2.0], theme::accent_claude().alpha(0.65))
                .child(
                    text(tr)
                        .font(theme::BODY_TEXT)
                        .color(theme::text_primary())
                        .w_full()
                        .wrap(),
                )
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .gap(4.0)
                        .child(
                            text(tr_lang)
                                .font(theme::BADGE_TEXT)
                                .color(theme::accent_claude()),
                        )
                        .child(el().grow(1.0))
                        .child(
                            el()
                                .key("btn_popout_translate_copy")
                                .px(7.0)
                                .py(3.5)
                                .radius(5.0)
                                .hover(theme::hover_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press_tx(move |s: &mut PopoutReaderState, tx: &mut Tx| {
                                    tx.copy(tr_owned.clone());
                                    s.copied_slot = Some("translation");
                                })
                                .child(
                                    text(if tr_copied { "Copied ✓" } else { "Copy" })
                                        .font(theme::STATUS_TEXT)
                                        .color(if tr_copied {
                                            theme::accent_claude()
                                        } else {
                                            theme::text_secondary()
                                        }),
                                ),
                        )
                        .child(toolbar_chip(
                            "btn_popout_translate_spawn_note",
                            "+ Note",
                            theme::text_secondary(),
                            move |s| {
                                s.send_cmd(ReaderCommand::SpawnNoteFromTranslation(
                                    nid_spawn_tr.clone(),
                                ));
                            },
                        ))
                        .child(toolbar_chip(
                            "btn_popout_translate_clear",
                            "×",
                            theme::text_muted(),
                            move |s| {
                                s.snapshot.meta.translated_text = None;
                                s.copied_slot = None;
                                s.send_cmd(ReaderCommand::ClearTranslatedText(
                                    nid_clear_tr.clone(),
                                ));
                            },
                        )),
                ),
        );
    }

    box_col = box_col.child(ocr_card);

    col()
        .w_full()
        .grow(1.0)
        .padding(24.0)
        .items_center()
        .scroll(ScrollSpec { stick_bottom: false })
        .child(box_col)
}

fn seg_tab_popout(key: &'static str, label: &str, active: bool) -> El {
    seg_btn(key, label, active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn test_popout_reader_readonly_vs_edit_toolbar() {
        let bridge = Arc::new(Mutex::new(ReaderBridge::default()));
        {
            let mut b = bridge.lock().unwrap();
            b.snapshot = ReaderSnapshot {
                revision: 1,
                project_title: "Neuroscience".into(),
                node_id: Some(NodeId::from_str("doc-1")),
                title: "Primary Paper".into(),
                content: "# Sample\nFocused reading paragraph.".into(),
                summary: Some("Summary".into()),
                doc_format: DocFormat::Markdown,
                asset_path: None,
                doc_view_tab: DocViewTab::Preview,
                pdf_page_index: 0,
                pdf_page_count: 1,
                pdf_zoom: 1.5,
                read_only: true,
                ..Default::default()
            };
        }

        let mut rt = Runtime::new(
            PopoutReaderState::new(bridge.clone()),
            render_popout_reader,
            (960.0, 820.0),
            Env {
                system_dark: true,
                scale: 2.0,
            },
        );
        rt.frame(0.0);
        rt.frame(1.0 / 60.0);

        let ro_keys: Vec<_> = rt.elements().into_iter().filter_map(|e| e.key).collect();
        assert!(ro_keys.contains(&"popout_readonly_toolbar".to_string()));
        assert!(ro_keys.contains(&"btn_popout_toggle_readonly".to_string()));
        assert!(ro_keys.contains(&"btn_popout_dock_back".to_string()));
        assert!(!ro_keys.contains(&"popout_doc_title".to_string()));
        assert!(!ro_keys.contains(&"popout_append_note".to_string()));
        assert!(!ro_keys.contains(&"btn_popout_ai".to_string()));
        assert!(!ro_keys.contains(&"popout_tab_source".to_string()));

        // Toggle into Edit mode via button click
        let btn_r = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("btn_popout_toggle_readonly"))
            .unwrap()
            .rect;
        let (bx, by) = (btn_r[0] + btn_r[2] * 0.5, btn_r[1] + btn_r[3] * 0.5);
        rt.pointer_move(bx, by);
        rt.pointer_down(bx, by);
        rt.pointer_up(bx, by);
        rt.frame(1.0 / 60.0);
        rt.frame(1.0 / 60.0);

        assert!(!rt.state.snapshot.read_only);
        let edit_keys: Vec<_> = rt.elements().into_iter().filter_map(|e| e.key).collect();
        assert!(edit_keys.contains(&"popout_doc_title".to_string()));
        assert!(edit_keys.contains(&"popout_append_note".to_string()));
        assert!(edit_keys.contains(&"btn_popout_ai".to_string()));
        assert!(edit_keys.contains(&"popout_tab_source".to_string()));

        let cmds = std::mem::take(&mut bridge.lock().unwrap().commands);
        assert!(matches!(cmds.as_slice(), [ReaderCommand::SetReadOnly(false)]));
    }

    #[test]
    fn test_popout_section_frame_renders_children_and_forwards_interactions() {
        let bridge = Arc::new(Mutex::new(ReaderBridge::default()));
        {
            let mut b = bridge.lock().unwrap();
            let mut sec_meta = NodeMeta::default();
            sec_meta.section_framed = true;
            b.snapshot = ReaderSnapshot {
                revision: 1,
                project_title: "Neuro".into(),
                node_id: Some(NodeId::from_str("sec-1")),
                node_kind: NodeKind::Section,
                title: "Synaptic Circuit".into(),
                meta: sec_meta,
                section_size: (640.0, 400.0),
                section_children: vec![
                    SectionChildSnapshot {
                        id: NodeId::from_str("fc-1"),
                        kind: NodeKind::Flashcard,
                        title: "NMDA Block".into(),
                        content: "What ion blocks NMDA?".into(),
                        rel_pos: (24.0, 56.0),
                        size: (270.0, 185.0),
                        meta: NodeMeta {
                            answer: Some("Mg2+".into()),
                            ..Default::default()
                        },
                        flipped: false,
                        asset_path: None,
                        image_handle: None,
                    },
                    SectionChildSnapshot {
                        id: NodeId::from_str("chk-1"),
                        kind: NodeKind::Checklist,
                        title: "Review Steps".into(),
                        content: "- [ ] Trace LTP\n- [ ] Solve problem".into(),
                        rel_pos: (340.0, 56.0),
                        size: (260.0, 195.0),
                        meta: NodeMeta::default(),
                        flipped: false,
                        asset_path: None,
                        image_handle: None,
                    },
                ],
                section_edges: vec![SectionEdgeSnapshot {
                    from_rel: (294.0, 148.5),
                    to_rel: (340.0, 153.5),
                    label: "[supports]".into(),
                    is_ghost: false,
                    hide_label: false,
                }],
                read_only: true,
                ..Default::default()
            };
        }

        let mut rt = Runtime::new(
            PopoutReaderState::new(bridge.clone()),
            render_popout_reader,
            (960.0, 820.0),
            Env {
                system_dark: true,
                scale: 2.0,
            },
        );
        for _ in 0..8 {
            rt.frame(1.0 / 60.0);
        }

        let keys: Vec<_> = rt.elements().into_iter().filter_map(|e| e.key).collect();
        assert!(keys.contains(&"popout_section_canvas".to_string()));
        assert!(keys.contains(&"popout_section_wire_layer".to_string()));
        assert!(keys.contains(&"popout_sec_child_fc-1".to_string()));
        assert!(keys.contains(&"popout_sec_child_chk-1".to_string()));

        // Click Flashcard Reveal inside Section pop-out
        let flip_r = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("popout_sec_flash_flip_fc-1"))
            .unwrap()
            .rect;
        rt.pointer_move(flip_r[0] + flip_r[2] * 0.5, flip_r[1] + flip_r[3] * 0.5);
        rt.pointer_down(flip_r[0] + flip_r[2] * 0.5, flip_r[1] + flip_r[3] * 0.5);
        rt.pointer_up(flip_r[0] + flip_r[2] * 0.5, flip_r[1] + flip_r[3] * 0.5);
        rt.frame(1.0 / 60.0);

        // Click Checklist task inside Section pop-out
        let chk_r = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("popout_sec_chk_chk-1_0"))
            .unwrap()
            .rect;
        rt.pointer_move(chk_r[0] + chk_r[2] * 0.5, chk_r[1] + chk_r[3] * 0.5);
        rt.pointer_down(chk_r[0] + chk_r[2] * 0.5, chk_r[1] + chk_r[3] * 0.5);
        rt.pointer_up(chk_r[0] + chk_r[2] * 0.5, chk_r[1] + chk_r[3] * 0.5);
        rt.frame(1.0 / 60.0);

        let cmds = std::mem::take(&mut bridge.lock().unwrap().commands);
        assert!(cmds.iter().any(|c| matches!(c, ReaderCommand::FlipFlashcard(id) if id.as_str() == "fc-1")));
        assert!(cmds.iter().any(|c| matches!(c, ReaderCommand::ToggleChecklistItem { node_id, task_index: 0 } if node_id.as_str() == "chk-1")));
    }

    #[test]
    fn test_popout_flashcard_and_formula_preserve_controls() {
        let bridge = Arc::new(Mutex::new(ReaderBridge::default()));
        {
            let mut b = bridge.lock().unwrap();
            b.snapshot = ReaderSnapshot {
                revision: 1,
                project_title: "Study".into(),
                node_id: Some(NodeId::from_str("fc-2")),
                node_kind: NodeKind::Flashcard,
                title: "LTP".into(),
                content: "Which receptor gates Ca2+?".into(),
                meta: NodeMeta {
                    answer: Some("NMDA".into()),
                    mastery: Some("new".into()),
                    ..Default::default()
                },
                read_only: true,
                ..Default::default()
            };
        }

        let mut rt = Runtime::new(
            PopoutReaderState::new(bridge.clone()),
            render_popout_reader,
            (960.0, 820.0),
            Env {
                system_dark: true,
                scale: 2.0,
            },
        );
        for _ in 0..4 {
            rt.frame(1.0 / 60.0);
        }

        let flip_r = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("btn_popout_flash_flip"))
            .unwrap()
            .rect;
        rt.pointer_move(flip_r[0] + flip_r[2] * 0.5, flip_r[1] + flip_r[3] * 0.5);
        rt.pointer_down(flip_r[0] + flip_r[2] * 0.5, flip_r[1] + flip_r[3] * 0.5);
        rt.pointer_up(flip_r[0] + flip_r[2] * 0.5, flip_r[1] + flip_r[3] * 0.5);
        rt.frame(1.0 / 60.0);

        let good_r = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("btn_popout_flash_rate_good"))
            .unwrap()
            .rect;
        rt.pointer_move(good_r[0] + good_r[2] * 0.5, good_r[1] + good_r[3] * 0.5);
        rt.pointer_down(good_r[0] + good_r[2] * 0.5, good_r[1] + good_r[3] * 0.5);
        rt.pointer_up(good_r[0] + good_r[2] * 0.5, good_r[1] + good_r[3] * 0.5);
        rt.frame(1.0 / 60.0);

        let cmds = std::mem::take(&mut bridge.lock().unwrap().commands);
        assert!(cmds.iter().any(|c| matches!(c, ReaderCommand::FlipFlashcard(id) if id.as_str() == "fc-2")));
        assert!(cmds.iter().any(|c| matches!(c, ReaderCommand::SetFlashcardMastery { node_id, mastery } if node_id.as_str() == "fc-2" && mastery == "good")));

        // Switch pop-out snapshot to editable Formula and trigger shorthand format
        {
            let mut b = bridge.lock().unwrap();
            b.snapshot = ReaderSnapshot {
                revision: 2,
                project_title: "Study".into(),
                node_id: Some(NodeId::from_str("eq-1")),
                node_kind: NodeKind::Formula,
                title: "Hebbian Rule".into(),
                content: "\\sum \\alpha^2".into(),
                read_only: false,
                ..Default::default()
            };
        }
        assert!(rt.state.sync_from_bridge());
        rt.invalidate();
        for _ in 0..4 {
            rt.frame(1.0 / 60.0);
        }

        let fmt_r = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("btn_popout_formula_format"))
            .unwrap()
            .rect;
        rt.pointer_move(fmt_r[0] + fmt_r[2] * 0.5, fmt_r[1] + fmt_r[3] * 0.5);
        rt.pointer_down(fmt_r[0] + fmt_r[2] * 0.5, fmt_r[1] + fmt_r[3] * 0.5);
        rt.pointer_up(fmt_r[0] + fmt_r[2] * 0.5, fmt_r[1] + fmt_r[3] * 0.5);
        rt.frame(1.0 / 60.0);

        let cmds2 = std::mem::take(&mut bridge.lock().unwrap().commands);
        assert!(cmds2.iter().any(|c| matches!(c, ReaderCommand::FormatFormula(id) if id.as_str() == "eq-1")));
    }

    #[test]
    fn test_popout_image_ocr_and_translation_commands() {
        let bridge = Arc::new(Mutex::new(ReaderBridge::default()));
        {
            let mut b = bridge.lock().unwrap();
            b.snapshot = ReaderSnapshot {
                revision: 1,
                project_title: "Vision Study".into(),
                node_id: Some(NodeId::from_str("img-1")),
                node_kind: NodeKind::Image,
                title: "Synapse Diagram".into(),
                doc_format: DocFormat::Image,
                meta: NodeMeta {
                    extracted_text: Some("Δ wᵢⱼ = η · xᵢ · yⱼ".into()),
                    ocr_engine: Some("PP-OCRv5 (Local)".into()),
                    translated_text: Some("Las neuronas que disparan juntas...".into()),
                    target_language: Some("es".into()),
                    ..Default::default()
                },
                ocr_engine: OcrEnginePreference::LocalPpOcrV5,
                neural_models_ready: false,
                target_language: "es".into(),
                read_only: true,
                ..Default::default()
            };
        }

        let cb = rune::clipboard::memory();
        let mut rt = Runtime::new(
            PopoutReaderState::new(bridge.clone()),
            render_popout_reader,
            (960.0, 900.0),
            Env {
                system_dark: true,
                scale: 2.0,
            },
        );
        rt.use_clipboard(&cb);
        for _ in 0..6 {
            rt.frame(1.0 / 60.0);
        }

        let click_popout = |rt: &mut Runtime<PopoutReaderState>, key: &str| {
            let r = rt
                .elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some(key))
                .unwrap_or_else(|| panic!("missing popout element '{key}'"))
                .rect;
            rt.pointer_move(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
            rt.pointer_down(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
            rt.pointer_up(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
            rt.frame(1.0 / 60.0);
        };

        click_popout(&mut rt, "btn_popout_ocr_copy");
        assert_eq!(
            cb.text().as_deref().map(String::as_str),
            Some("Δ wᵢⱼ = η · xᵢ · yⱼ")
        );
        click_popout(&mut rt, "btn_popout_translate_copy");
        assert_eq!(
            cb.text().as_deref().map(String::as_str),
            Some("Las neuronas que disparan juntas...")
        );

        for key in [
            "btn_popout_img_ocr_text",
            "btn_popout_download_neural",
            "btn_popout_cycle_ocr_engine",
            "btn_popout_ocr_spawn_note",
            "btn_popout_ocr_clear",
            "popout_lang_ja",
            "btn_popout_translate_node",
        ] {
            click_popout(&mut rt, key);
        }

        let cmds = std::mem::take(&mut bridge.lock().unwrap().commands);
        assert!(cmds.iter().any(|c| matches!(
            c,
            ReaderCommand::RunImageOcr { node_id, mode: OcrMode::PlainText } if node_id.as_str() == "img-1"
        )));
        assert!(cmds.iter().any(|c| matches!(
            c,
            ReaderCommand::SetOcrEngine(OcrEnginePreference::CloudVision)
        )));
        assert!(cmds
            .iter()
            .any(|c| matches!(c, ReaderCommand::DownloadNeuralModels)));
        assert!(cmds
            .iter()
            .any(|c| matches!(c, ReaderCommand::SpawnNoteFromOcr(id) if id.as_str() == "img-1")));
        assert!(cmds
            .iter()
            .any(|c| matches!(c, ReaderCommand::ClearExtractedText(id) if id.as_str() == "img-1")));
        assert!(cmds
            .iter()
            .any(|c| matches!(c, ReaderCommand::SetTargetLanguage(l) if l == "ja")));
        assert!(cmds
            .iter()
            .any(|c| matches!(c, ReaderCommand::RunNodeTranslation(id) if id.as_str() == "img-1")));
    }
}
