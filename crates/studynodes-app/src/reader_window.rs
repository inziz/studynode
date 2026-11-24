use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rune::{
    AppSpec, Color, Cursor, Cx, El, FileDialogOptions, ImageCache, LineCap, Phase, Pixels,
    ScrollSpec, Shape, SurfaceFit, TextEdit, TextStyle, Titlebar, Tx, Waker, WindowHandle,
    WindowSpec, canvas, col, el, memo, open_window, row, text, text_input,
};
use rune_kit::Markdown;
use studynodes_graph::bezier::CubicBezier;
use studynodes_graph::coords::WorldPos;
use studynodes_graph::model::{CalendarViewMode, NodeId, NodeKind, NodeMeta};
use studynodes_vision::{OcrEnginePreference, OcrMode, SUPPORTED_LANGUAGES};

use crate::doc_reader::{self, DocFormat, PdfLayoutMode};
use crate::formula_math;
use crate::graph_formula;
use crate::i18n::{self, TextKey};
use crate::icons;
use crate::media_helper;
use crate::state::DocViewTab;
use crate::theme;

fn hash_str(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Checklists, Excerpts, and live Media embeds stay inline on canvas rather than popping into separate windows.
pub fn can_pop_out(kind: NodeKind) -> bool {
    !matches!(
        kind,
        NodeKind::Checklist | NodeKind::Excerpt | NodeKind::Video | NodeKind::Audio | NodeKind::Text
    )
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
    pub selected_date: Option<String>,
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
    pub selected_calendar_date: Option<String>,
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
    pub pdf_layout_mode: PdfLayoutMode,
    pub pdf_page_dimensions: Vec<(f32, f32)>,
    pub pdf_rendered_pages: HashMap<usize, (u32, u32, Pixels)>,
    pub pdf_error: Option<String>,
    pub read_only: bool,
    pub ocr_engine: OcrEnginePreference,
    pub target_language: String,
    pub vision_pending: bool,
    pub vision_downloading_models: bool,
    pub neural_models_ready: bool,
    pub vision_error: Option<String>,
    pub ai_summarizing: bool,
    pub llm_last_error: Option<String>,
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
            selected_calendar_date: None,
            section_size: (640.0, 400.0),
            section_children: Vec::new(),
            section_edges: Vec::new(),
            doc_format: DocFormat::Markdown,
            asset_path: None,
            active_image_handle: None,
            doc_view_tab: DocViewTab::Preview,
            pdf_page_index: 0,
            pdf_page_count: 1,
            pdf_zoom: 1.0,
            pdf_layout_mode: PdfLayoutMode::default(),
            pdf_page_dimensions: Vec::new(),
            pdf_rendered_pages: HashMap::new(),
            pdf_error: None,
            read_only: true,
            ocr_engine: OcrEnginePreference::Auto,
            target_language: "es".into(),
            vision_pending: false,
            vision_downloading_models: false,
            neural_models_ready: false,
            vision_error: None,
            ai_summarizing: false,
            llm_last_error: None,
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
    SetPdfLayoutMode(PdfLayoutMode),
    OpenExternal(NodeId),
    ExplainAi(NodeId),
    ClearNodeSummary(NodeId),
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
    ZoomGraphFormula {
        node_id: NodeId,
        factor: f64,
        anchor_norm: Option<(f32, f32)>,
        plot_size: (f32, f32),
    },
    PanGraphFormula {
        node_id: NodeId,
        start_cx: f64,
        start_cy: f64,
        dx: f32,
        dy: f32,
        plot_size: (f32, f32),
        commit: bool,
    },
    ResetGraphFormulaView(NodeId),
    ApplyGraphFormulaKeypad {
        node_id: NodeId,
        action: graph_formula::KeypadAction,
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
    TriggerSectionAiReview(NodeId),
    CaptureSectionScreenshot {
        node_id: Option<NodeId>,
        path: Option<PathBuf>,
    },
    InterpretDrawing {
        node_id: NodeId,
        mode: OcrMode,
    },
    BeginDrawingStroke {
        node_id: NodeId,
        nx: f32,
        ny: f32,
        color_hex: String,
        width: f32,
        eraser: bool,
    },
    ExtendDrawingStroke {
        node_id: NodeId,
        nx: f32,
        ny: f32,
        eraser: bool,
    },
    FinishDrawingStroke(NodeId),
    UndoDrawingStroke(NodeId),
    ClearDrawingStrokes(NodeId),
    CalendarSelectDate {
        node_id: NodeId,
        date: String,
    },
    CalendarStepMonth {
        node_id: NodeId,
        delta: i32,
    },
    CalendarStepWeek {
        node_id: NodeId,
        delta: i32,
    },
    CalendarSetMode {
        node_id: NodeId,
        mode: CalendarViewMode,
    },
    ToggleCalendarEvent {
        node_id: NodeId,
        event_id: String,
    },
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
    pub image_cache: ImageCache,
    pub active_image: Option<(PathBuf, rune::Image)>,
    pub section_images: HashMap<NodeId, (PathBuf, rune::Image)>,
    pub viewport_width: f32,
    pub copied_slot: Option<&'static str>,
    pub drawing_color_hex: String,
    pub drawing_stroke_width: f32,
    pub drawing_eraser: bool,
    pub graph_keypad_mode: graph_formula::KeypadMode,
    pub graph_drag_origin: Option<(NodeId, f64, f64)>,
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
            image_cache: ImageCache::new(Arc::new(|| {})),
            active_image: None,
            section_images: HashMap::new(),
            viewport_width: 960.0,
            copied_slot: None,
            drawing_color_hex: "#F4F3EE".into(),
            drawing_stroke_width: 3.5,
            drawing_eraser: false,
            graph_keypad_mode: graph_formula::KeypadMode::default(),
            graph_drag_origin: None,
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
        } else if snap.doc_format == DocFormat::Image {
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

fn toolbar_icon_chip(
    key: &'static str,
    ico: &'static rune::Icon,
    fg: Color,
    on_press: impl Fn(&mut PopoutReaderState) + 'static,
) -> El {
    el()
        .key(key)
        .size(26.0, 26.0)
        .radius(6.0)
        .hover(theme::hover_subtle())
        .press(theme::press_subtle())
        .center()
        .cursor(Cursor::Pointer)
        .on_press(on_press)
        .child(icons::ph(ico, 13.5, fg))
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
        .child(icons::ph(&icons::SPARKLE, 13.0, theme::accent_claude()))
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
            let sec_ai_id = nid.clone();
            let sec_shot_id = nid.clone();
            let framed = snap.meta.section_framed;
            let slug: String = snap
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
            let shot_name = if slug.trim_matches('_').is_empty() {
                "section_scheme.png".to_string()
            } else {
                format!("{}_scheme.png", slug.trim_matches('_'))
            };
            titlebar = titlebar
                .child(
                    text(i18n::fmt_blocks_count(snap.section_children.len()))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                )
                .child(
                    toolbar_chip(
                        "popout_sec_toggle_frame",
                        if framed {
                            i18n::t(TextKey::CardFrame)
                        } else {
                            i18n::t(TextKey::CardBanner)
                        },
                        theme::text_secondary(),
                        move |s| {
                            s.snapshot.meta.section_framed = !s.snapshot.meta.section_framed;
                            s.send_cmd(ReaderCommand::ToggleSectionFramed(sec_id.clone()));
                        },
                    )
                    .no_window_drag(),
                )
                .child(
                    toolbar_chip(
                        "popout_sec_ai_review",
                        if snap.ai_summarizing {
                            i18n::t(TextKey::ReaderSummarizing)
                        } else {
                            i18n::t(TextKey::PopoutAiReview)
                        },
                        theme::accent_claude(),
                        move |s| {
                            s.snapshot.ai_summarizing = true;
                            s.send_cmd(ReaderCommand::TriggerSectionAiReview(sec_ai_id.clone()));
                        },
                    )
                    .no_window_drag(),
                )
                .child(
                    el()
                        .key("btn_popout_screenshot")
                        .size(26.0, 26.0)
                        .radius(6.0)
                        .fill(theme::bg_control())
                        .border(1.0, theme::border_subtle())
                        .hover(theme::hover_subtle())
                        .press(theme::press_subtle())
                        .center()
                        .cursor(Cursor::Pointer)
                        .no_window_drag()
                        .on_press_tx(move |_s: &mut PopoutReaderState, tx: &mut Tx| {
                            let target_id = sec_shot_id.clone();
                            let opts = FileDialogOptions {
                                title: i18n::t(TextKey::PopoutScreenshot).to_string(),
                                file_name: Some(shot_name.clone()),
                                ..Default::default()
                            }
                            .filter("PNG Image", &["png"]);
                            tx.save_file(opts, move |s: &mut PopoutReaderState, res, _| {
                                if let Ok(Some(dest)) = res {
                                    s.send_cmd(ReaderCommand::CaptureSectionScreenshot {
                                        node_id: Some(target_id.clone()),
                                        path: Some(dest),
                                    });
                                }
                            });
                        })
                        .child(icons::ph(&icons::CAMERA, 13.5, theme::text_secondary())),
                );
        } else {
            let mut switcher = row()
                .key("popout_view_tabs")
                .gap(10.0)
                .center_y()
                .no_window_drag();

            if is_doc_or_note && has_pdf {
                switcher = switcher.child(
                    seg_btn(
                        "popout_tab_pages",
                        i18n::t(TextKey::ReaderTabPages),
                        snap.doc_view_tab == DocViewTab::Pages,
                    )
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
                        if has_pdf && is_ro {
                            i18n::t(TextKey::ReaderTabText)
                        } else {
                            i18n::t(TextKey::ReaderTabPreview)
                        },
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
                    seg_btn(
                        "popout_tab_source",
                        i18n::t(TextKey::ReaderTabSource),
                        snap.doc_view_tab == DocViewTab::Source,
                    )
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
                    if is_ro {
                        i18n::t(TextKey::ReaderEdit)
                    } else {
                        i18n::t(TextKey::ReaderDone)
                    },
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
                text(i18n::t(TextKey::PopoutDockBack))
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_secondary()),
            ),
    );

    let body = if let Some(node_id) = snap.node_id.clone() {
        match snap.node_kind {
            NodeKind::Section => render_popout_section_frame(s, snap),
            NodeKind::Flashcard => render_popout_flashcard(s, snap, &node_id, is_ro),
            NodeKind::Formula => render_popout_formula(s, snap, &node_id, is_ro),
            NodeKind::GraphFormula => render_popout_graph_formula(s, snap, &node_id, is_ro),
            NodeKind::Sequence => render_popout_sequence(s, snap, &node_id, is_ro),
            NodeKind::Image => render_popout_image(s, snap, &node_id, is_ro),
            NodeKind::Calendar => render_popout_calendar(s, snap, &node_id),
            NodeKind::Drawing => render_popout_drawing(s, snap, &node_id),
            _ => {
                let nid_for_title = node_id.clone();
                let nid_for_source = node_id.clone();
                let nid_for_ai = node_id.clone();
                let nid_for_clear = node_id.clone();
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
            render_popout_pdf_surface(s, snap, is_ro)
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
                        .label(i18n::t(TextKey::ReaderTabSource))
                        .w_full()
                        .max_w(740.0)
                        .min_h(520.0)
                        .color(theme::text_primary())
                        .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                        .placeholder(i18n::t(TextKey::ReaderSourcePlaceholder), theme::text_muted())
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
                i18n::t(TextKey::ReaderEmptyReadonlyMd).to_string()
            } else {
                doc_reader::wrap_for_preview(&snap.content, snap.doc_format)
            };

            let md_w = (s.viewport_width - 120.0).clamp(360.0, 740.0);
            let md_hash = hash_str(&md_src);
            let md_version = (
                snap.node_id.clone(),
                md_hash,
                snap.doc_format,
                md_w.round() as i32,
                theme::active_theme(),
                i18n::active_locale(),
                is_ro,
            );
            let task_nid = nid_for_task.clone();
            let md_el = memo("popout_md_preview", md_version, move || {
                let md_style = theme::markdown_style().text_width(md_w);
                let mut md_builder = Markdown::new(&md_src).selectable(true);
                if !is_ro {
                    md_builder = md_builder.on_task(
                        move |s: &mut PopoutReaderState, _block_key, offset, checked| {
                            s.send_cmd(ReaderCommand::ToggleTask {
                                node_id: task_nid.clone(),
                                marker_offset: offset,
                                checked,
                            });
                        },
                    );
                }
                md_builder.render(&md_style)
            });

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
                                text(format!("~{} min", read_min))
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
                            .placeholder("Title...", theme::text_muted())
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
                    if snap.ai_summarizing {
                        i18n::t(TextKey::ReaderSummarizing)
                    } else {
                        i18n::t(TextKey::ReaderSummarize)
                    },
                    theme::accent_claude(),
                    move |s| {
                        s.snapshot.ai_summarizing = true;
                        s.send_cmd(ReaderCommand::ExplainAi(nid_for_ai.clone()));
                    },
                ));
            body_col = body_col.child(top_meta);

            if let Some(err) = &snap.llm_last_error {
                body_col = body_col.child(
                    text(err)
                        .font(theme::STATUS_TEXT)
                        .color(theme::accent_danger())
                        .w_full()
                        .max_w(740.0)
                        .wrap(),
                );
            }

            if let Some(summary) = &snap.summary {
                let clear_nid = nid_for_clear;
                body_col = body_col.child(
                    row()
                        .w_full()
                        .max_w(740.0)
                        .pl(12.0)
                        .py(4.0)
                        .gap(8.0)
                        .center_y()
                        .border_sides([0.0, 0.0, 0.0, 2.0], theme::accent_claude().alpha(0.65))
                        .child(
                            el().grow(1.0).min_w(0.0).child(
                                text(summary.clone())
                                    .font(theme::BODY_TEXT)
                                    .color(theme::text_secondary())
                                    .w_full()
                                    .wrap(),
                            ),
                        )
                        .child(toolbar_icon_chip(
                            "btn_popout_ai_clear",
                            &icons::X,
                            theme::text_muted(),
                            move |s| {
                                s.snapshot.summary = None;
                                s.send_cmd(ReaderCommand::ClearNodeSummary(clear_nid.clone()));
                            },
                        )),
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
                                .label(i18n::t(TextKey::ReaderAppendPlaceholder))
                                .single_line()
                                .w_full()
                                .color(theme::text_primary())
                                .caret(theme::accent_claude(), theme::accent_claude().alpha(0.25))
                                .placeholder(i18n::t(TextKey::ReaderAppendPlaceholder), theme::text_muted())
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
                        "+",
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
                text(i18n::t(TextKey::DashTitle))
                    .font(theme::PANEL_TITLE)
                    .color(theme::text_primary()),
            )
            .child(
                text(i18n::t(TextKey::DashSubtitle))
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

fn popout_pdf_aspect(snap: &ReaderSnapshot, page_idx: usize) -> f32 {
    if let Some(&(w, h)) = snap.pdf_page_dimensions.get(page_idx) {
        if w > 0.0 && h > 0.0 {
            return (w / h).clamp(0.35, 2.5);
        }
    }
    if let Some(&(w, h, _)) = snap.pdf_rendered_pages.get(&page_idx) {
        if w > 0 && h > 0 {
            return (w as f32 / h as f32).clamp(0.35, 2.5);
        }
    }
    8.5 / 11.0
}

fn render_popout_pdf_page_box(
    snap: &ReaderSnapshot,
    page_idx: usize,
    target_w: f32,
    show_caption: bool,
) -> El {
    let ar = popout_pdf_aspect(snap, page_idx);
    let page_surface = if let Some((_, _, px)) = snap.pdf_rendered_pages.get(&page_idx) {
        el()
            .key(format!("popout_pdf_canvas_{}", page_idx))
            .w(target_w)
            .aspect_ratio(ar)
            .radius(4.0)
            .border(1.0, theme::border_default())
            .clip_soft(0.0)
            .pixels_fit(px, SurfaceFit::Fit)
    } else {
        el()
            .key(format!("popout_pdf_canvas_{}", page_idx))
            .w(target_w)
            .aspect_ratio(ar)
            .radius(4.0)
            .fill(theme::bg_card())
            .border(1.0, theme::border_subtle())
            .center()
            .child(
                text(format!("{} · p. {}", i18n::t(TextKey::PdfRenderingPage), page_idx + 1))
                    .font(theme::STATUS_TEXT)
                    .color(theme::text_muted()),
            )
    };

    if show_caption {
        col()
            .gap(4.0)
            .items_center()
            .child(page_surface)
            .child(
                text(format!("p. {}", page_idx + 1))
                    .font(theme::BADGE_TEXT)
                    .color(theme::text_muted()),
            )
    } else {
        page_surface
    }
}

fn render_popout_pdf_surface(s: &PopoutReaderState, snap: &ReaderSnapshot, is_ro: bool) -> El {
    let total_pages = snap.pdf_page_count.max(1);
    let cur_page = snap.pdf_page_index.min(total_pages - 1);
    let cur_zoom = snap.pdf_zoom;
    let mode = snap.pdf_layout_mode;
    let step = if mode == PdfLayoutMode::TwoPage { 2 } else { 1 };

    let pdf_bar = row()
        .key(if is_ro {
            "popout_readonly_toolbar"
        } else {
            "popout_pdf_toolbar"
        })
        .center_y()
        .gap(6.0)
        .px(10.0)
        .py(5.0)
        .radius(6.0)
        .fill(theme::bg_card())
        .border(1.0, theme::border_subtle())
        .child(toolbar_chip(
            "popout_pdf_prev",
            i18n::t(TextKey::PdfPrevPage),
            theme::text_primary(),
            move |s| {
                s.send_cmd(ReaderCommand::SetPdfPage(cur_page.saturating_sub(step)));
            },
        ))
        .child(
            text(i18n::fmt_page_counter(cur_page, total_pages))
                .font(theme::STATUS_TEXT)
                .color(theme::text_secondary()),
        )
        .child(toolbar_chip(
            "popout_pdf_next",
            i18n::t(TextKey::PdfNextPage),
            theme::text_primary(),
            move |s| {
                let next = (cur_page + step).min(total_pages.saturating_sub(1));
                s.send_cmd(ReaderCommand::SetPdfPage(next));
            },
        ))
        .child(el().w(1.0).h(14.0).fill(theme::border_subtle()))
        .child(seg_btn(
            "btn_popout_pdf_mode_continuous",
            i18n::t(TextKey::PdfModeContinuous),
            mode == PdfLayoutMode::Continuous,
        ).on_press(|s: &mut PopoutReaderState| {
            s.snapshot.pdf_layout_mode = PdfLayoutMode::Continuous;
            s.send_cmd(ReaderCommand::SetPdfLayoutMode(PdfLayoutMode::Continuous));
        }))
        .child(seg_btn(
            "btn_popout_pdf_mode_twopage",
            i18n::t(TextKey::PdfModeTwoPage),
            mode == PdfLayoutMode::TwoPage,
        ).on_press(|s: &mut PopoutReaderState| {
            s.snapshot.pdf_layout_mode = PdfLayoutMode::TwoPage;
            s.send_cmd(ReaderCommand::SetPdfLayoutMode(PdfLayoutMode::TwoPage));
        }))
        .child(seg_btn(
            "btn_popout_pdf_mode_single",
            i18n::t(TextKey::PdfModeSingle),
            mode == PdfLayoutMode::SinglePage,
        ).on_press(|s: &mut PopoutReaderState| {
            s.snapshot.pdf_layout_mode = PdfLayoutMode::SinglePage;
            s.send_cmd(ReaderCommand::SetPdfLayoutMode(PdfLayoutMode::SinglePage));
        }))
        .child(el().w(1.0).h(14.0).fill(theme::border_subtle()))
        .child(toolbar_chip(
            "popout_pdf_zout",
            "−",
            theme::text_secondary(),
            move |s| {
                s.send_cmd(ReaderCommand::SetPdfZoom(cur_zoom - 0.25));
            },
        ))
        .child(toolbar_chip(
            "popout_pdf_zoom_reset",
            &format!("{}%", (cur_zoom * 100.0).round() as u32),
            theme::text_muted(),
            |s| {
                s.send_cmd(ReaderCommand::SetPdfZoom(1.0));
            },
        ))
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
        .items_center()
        .child(pdf_bar);

    if total_pages > 1 {
        let last = total_pages - 1;
        let mut jump_pages: Vec<usize> = vec![0];
        if cur_page > 1 {
            jump_pages.push(cur_page - 1);
        }
        jump_pages.push(cur_page);
        if cur_page + 1 < last {
            jump_pages.push(cur_page + 1);
        }
        jump_pages.push(last);
        jump_pages.sort_unstable();
        jump_pages.dedup();

        let mut jump_row = row().gap(4.0).center_y();
        let mut prev_pg: Option<usize> = None;
        for pg in jump_pages {
            if prev_pg.is_some_and(|prev| pg > prev + 1) {
                jump_row = jump_row.child(icons::ph(&icons::DOTS_THREE, 12.0, theme::text_muted()));
            }
            let is_cur = pg == cur_page;
            jump_row = jump_row.child(
                el()
                    .key(format!("popout_pdf_jump_{}", pg))
                    .px(7.0)
                    .py(2.5)
                    .radius(4.0)
                    .fill(if is_cur {
                        theme::accent_claude().alpha(0.22)
                    } else {
                        theme::bg_control()
                    })
                    .hover(theme::hover_subtle())
                    .border(
                        1.0,
                        if is_cur {
                            theme::accent_claude().alpha(0.65)
                        } else {
                            theme::border_subtle()
                        },
                    )
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut PopoutReaderState| {
                        s.send_cmd(ReaderCommand::SetPdfPage(pg));
                    })
                    .child(
                        text(format!("{}", pg + 1))
                            .font(theme::BADGE_TEXT)
                            .color(if is_cur {
                                theme::accent_claude()
                            } else {
                                theme::text_secondary()
                            }),
                    ),
            );
            prev_pg = Some(pg);
        }
        pdf_col = pdf_col.child(jump_row);
    }

    if let Some(err) = &snap.pdf_error {
        return pdf_col.child(
            el()
                .w_full()
                .max_w(740.0)
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
    }

    let base_w = (s.viewport_width - 96.0).clamp(320.0, 820.0);
    let single_w = (base_w * cur_zoom).max(240.0);
    let mut scroll_inner = col().w_full().gap(16.0).items_center().py(4.0);

    match mode {
        PdfLayoutMode::Continuous => {
            let (start, end) = if total_pages <= 12 {
                (0, total_pages - 1)
            } else {
                (cur_page.saturating_sub(2), (cur_page + 6).min(total_pages - 1))
            };
            if start > 0 {
                scroll_inner = scroll_inner.child(toolbar_chip(
                    "popout_pdf_prev_window",
                    i18n::t(TextKey::PdfPrevWindow),
                    theme::text_secondary(),
                    move |s| {
                        s.send_cmd(ReaderCommand::SetPdfPage(start.saturating_sub(1)));
                    },
                ));
            }
            for pg in start..=end {
                scroll_inner = scroll_inner
                    .child(render_popout_pdf_page_box(snap, pg, single_w, total_pages > 1));
            }
            if end + 1 < total_pages {
                scroll_inner = scroll_inner.child(toolbar_chip(
                    "popout_pdf_next_window",
                    i18n::t(TextKey::PdfNextWindow),
                    theme::text_secondary(),
                    move |s| {
                        s.send_cmd(ReaderCommand::SetPdfPage(end + 1));
                    },
                ));
            }
        }
        PdfLayoutMode::TwoPage => {
            let left = (cur_page / 2) * 2;
            let right = left + 1;
            let two_col = right < total_pages && s.viewport_width >= 680.0;
            if two_col {
                let spread_base = ((s.viewport_width - 110.0) * 0.48).clamp(240.0, 620.0);
                let spread_w = (spread_base * cur_zoom).max(200.0);
                scroll_inner = scroll_inner.child(
                    row()
                        .gap(14.0)
                        .center()
                        .child(render_popout_pdf_page_box(snap, left, spread_w, true))
                        .child(render_popout_pdf_page_box(snap, right, spread_w, true)),
                );
            } else {
                scroll_inner = scroll_inner
                    .child(render_popout_pdf_page_box(snap, left, single_w, total_pages > 1));
                if right < total_pages {
                    scroll_inner = scroll_inner
                        .child(render_popout_pdf_page_box(snap, right, single_w, true));
                }
            }
        }
        PdfLayoutMode::SinglePage => {
            scroll_inner = scroll_inner
                .child(render_popout_pdf_page_box(snap, cur_page, single_w, false));
        }
    }

    pdf_col.child(
        col()
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .items_center()
            .scroll(ScrollSpec { stick_bottom: false })
            .child(scroll_inner),
    )
}

fn build_scheme_frame_el(
    snap: &ReaderSnapshot,
    section_images: &HashMap<NodeId, (PathBuf, rune::Image)>,
    avail_w: f32,
    interactive: bool,
) -> (El, f32, f32) {
    let sec_w = snap.section_size.0.max(240.0);
    let sec_h = snap.section_size.1.max(120.0);
    let scale = (avail_w / sec_w).clamp(0.45, 1.35);
    let frame_w = sec_w * scale;
    let frame_h = sec_h * scale;
    let is_dark = theme::active_theme().is_dark();

    let mut wire_shape = Shape::new();
    let grid_step = 48.0 * scale;
    if grid_step >= 12.0 {
        let g_col = if is_dark {
            Color::WHITE.alpha(0.03)
        } else {
            Color::BLACK.alpha(0.04)
        };
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
        .fill(if is_dark {
            Color::WHITE.alpha(0.025)
        } else {
            Color::BLACK.alpha(0.02)
        })
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
            col()
                .w_full()
                .h_full()
                .center()
                .gap(6.0)
                .child(
                    text(i18n::t(TextKey::PopoutEmptySectionTitle))
                        .font(theme::PANEL_TITLE)
                        .color(theme::text_primary()),
                )
                .child(
                    text(i18n::t(TextKey::PopoutEmptySectionDesc))
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
            let is_media_greyed = matches!(child.kind, NodeKind::Video | NodeKind::Audio);

            let card_body = match child.kind {
                NodeKind::Video | NodeKind::Audio => {
                    let plat_badge = child
                        .meta
                        .media_url
                        .as_deref()
                        .map(str::trim)
                        .filter(|u| !u.is_empty())
                        .map(|u| media_helper::classify_media_url(u).label());
                    let mut muted_col = col()
                        .key(format!("popout_sec_media_greyed_{}", child.id.as_str()))
                        .w_full()
                        .grow(1.0)
                        .min_h(0.0)
                        .center()
                        .gap(4.0)
                        .radius(5.0)
                        .fill(if is_dark {
                            theme::hex("#18191B")
                        } else {
                            theme::hex("#D8D9DC")
                        })
                        .border(1.0, theme::border_subtle());
                    if let Some(plat) = plat_badge {
                        muted_col = muted_col.child(
                            text(plat)
                                .font(TextStyle::new(badge_sz, 600.0).mono())
                                .color(theme::text_muted()),
                        );
                    }
                    muted_col.child(
                        text(i18n::t(TextKey::PopoutMediaHidden))
                            .font(TextStyle::new(body_sz * 0.92, 500.0))
                            .color(theme::text_muted())
                            .wrap(),
                    )
                }
                NodeKind::Calendar => render_popout_section_calendar_child(
                    child,
                    body_sz,
                    badge_sz,
                    scale,
                    interactive,
                ),
                NodeKind::Drawing => {
                    let pad_w = (cw - 16.0).max(80.0);
                    let has_ocr = child
                        .meta
                        .extracted_text
                        .as_deref()
                        .is_some_and(|t| !t.trim().is_empty());
                    let pad_h = (ch - if has_ocr { 58.0 } else { 40.0 }).max(48.0);
                    let shape = crate::canvas_view::build_drawing_shape(
                        &child.meta.drawing_strokes,
                        pad_w,
                        pad_h,
                        scale,
                    );
                    let mut d_col = col().w_full().grow(1.0).min_h(0.0).gap(4.0).child(
                        el()
                            .key(format!("popout_sec_drawing_{}", child.id.as_str()))
                            .w(pad_w)
                            .h(pad_h)
                            .radius(4.0)
                            .fill(theme::bg_input())
                            .border(1.0, theme::border_subtle())
                            .child(canvas(shape).absolute(0.0, 0.0).w_full().h_full()),
                    );
                    if let Some(txt) = child
                        .meta
                        .extracted_text
                        .as_deref()
                        .map(str::trim)
                        .filter(|t| !t.is_empty())
                    {
                        d_col = d_col.child(
                            text(txt)
                                .font(TextStyle::new(badge_sz, 500.0))
                                .color(theme::text_secondary())
                                .truncate(),
                        );
                    }
                    d_col
                }
                NodeKind::GraphFormula => {
                    let plot_w = (cw - 16.0).max(80.0);
                    let plot_h = (ch - 52.0).max(48.0);
                    let vp = graph_formula::GraphViewport::from_meta(&child.meta);
                    let plot =
                        graph_formula::build_graph_plot(&child.content, &vp, plot_w, plot_h, scale);
                    col()
                        .w_full()
                        .grow(1.0)
                        .min_h(0.0)
                        .gap(4.0)
                        .child(graph_formula::render_pretty_math(&child.content, body_sz))
                        .child(
                            el()
                                .key(format!("popout_sec_graph_{}", child.id.as_str()))
                                .w(plot_w)
                                .h(plot_h)
                                .radius(4.0)
                                .clip_rounded(4.0)
                                .fill(theme::bg_input())
                                .border(1.0, theme::border_subtle())
                                .child(canvas(plot.shape).absolute(0.0, 0.0).w_full().h_full()),
                        )
                }
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
                            .unwrap_or_else(|| i18n::t(TextKey::PopoutNoAnswer))
                    } else {
                        child.content.trim()
                    };
                    let mut flip_btn = toolbar_chip(
                        "popout_sec_flash_flip",
                        if flipped {
                            "↺"
                        } else {
                            i18n::t(TextKey::CardReveal)
                        },
                        theme::text_primary(),
                        move |s| {
                            if interactive {
                                s.send_cmd(ReaderCommand::FlipFlashcard(flip_id.clone()));
                            }
                        },
                    )
                    .key(format!("popout_sec_flash_flip_{}", child.id.as_str()));
                    if !interactive {
                        flip_btn = el().child(flip_btn);
                    }
                    let mut bar = row().w_full().center_y().gap(4.0).child(flip_btn);
                    if flipped {
                        bar = bar.child(el().grow(1.0));
                        for (pfx, lbl, val, tid) in [
                            (
                                "popout_sec_rate_again_",
                                i18n::t(TextKey::ReaderMasteryAgain),
                                "again",
                                again_id,
                            ),
                            (
                                "popout_sec_rate_good_",
                                i18n::t(TextKey::ReaderMasteryGood),
                                "good",
                                good_id,
                            ),
                            (
                                "popout_sec_rate_easy_",
                                i18n::t(TextKey::ReaderMasteryEasy),
                                "easy",
                                easy_id,
                            ),
                        ] {
                            let mut pill = el()
                                .key(format!("{}{}", pfx, child.id.as_str()))
                                .px(5.0)
                                .py(2.0)
                                .radius(3.0)
                                .fill(theme::bg_control())
                                .border(1.0, theme::border_subtle())
                                .child(
                                    text(lbl)
                                        .font(TextStyle::new(badge_sz, 600.0))
                                        .color(theme::text_primary()),
                                );
                            if interactive {
                                pill = pill.hover(theme::hover_subtle()).cursor(Cursor::Pointer).on_press(
                                    move |s: &mut PopoutReaderState| {
                                        s.send_cmd(ReaderCommand::SetFlashcardMastery {
                                            node_id: tid.clone(),
                                            mastery: val.to_string(),
                                        });
                                    },
                                );
                            }
                            bar = bar.child(pill);
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
                        let mut item_row = row()
                            .key(format!("popout_sec_chk_{}_{}", child.id.as_str(), idx))
                            .w_full()
                            .center_y()
                            .gap(5.0)
                            .px(4.0)
                            .py(2.0)
                            .radius(3.0)
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
                            );
                        if interactive {
                            item_row = item_row
                                .hover(theme::hover_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press(move |s: &mut PopoutReaderState| {
                                    s.send_cmd(ReaderCommand::ToggleChecklistItem {
                                        node_id: chk_id.clone(),
                                        task_index: idx,
                                    });
                                });
                        }
                        list = list.child(item_row);
                    }
                    list
                }
                NodeKind::Sequence => {
                    let seq_id = child.id.clone();
                    let mut next_pill = el()
                        .key(format!("popout_sec_seq_next_{}", child.id.as_str()))
                        .px(6.0)
                        .py(2.0)
                        .radius(3.0)
                        .fill(theme::bg_control())
                        .border(1.0, theme::border_subtle())
                        .child(
                            text(i18n::t(TextKey::PopoutNextStep))
                                .font(TextStyle::new(badge_sz, 600.0))
                                .color(theme::text_primary()),
                        );
                    if interactive {
                        next_pill = next_pill
                            .hover(theme::hover_subtle())
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut PopoutReaderState| {
                                s.send_cmd(ReaderCommand::AddNextSequenceStep(seq_id.clone()));
                            });
                    }
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
                        .child(row().w_full().child(el().grow(1.0)).child(next_pill))
                }
                NodeKind::Image => {
                    if let Some((_, img)) = section_images.get(&child.id) {
                        el().w_full()
                            .grow(1.0)
                            .min_h(0.0)
                            .radius(4.0)
                            .pixels_fit(img.pixels(), SurfaceFit::Fit)
                    } else {
                        text(theme::kind_label(NodeKind::Image))
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

            let card_bg = if is_media_greyed {
                if is_dark {
                    theme::hex("#1C1D1F")
                } else {
                    theme::hex("#E4E5E7")
                }
            } else {
                theme::bg_card()
            };
            let hdr_bg = if is_media_greyed {
                if is_dark {
                    theme::hex("#161719")
                } else {
                    theme::hex("#D6D7DA")
                }
            } else {
                theme::bg_card_header()
            };

            canvas_box = canvas_box.child(
                col()
                    .key(format!("popout_sec_child_{}", child.id.as_str()))
                    .absolute(cx, cy)
                    .size(cw, ch)
                    .radius(6.0)
                    .clip_rounded(6.0)
                    .fill(card_bg)
                    .border(
                        1.0,
                        if is_media_greyed {
                            theme::border_subtle()
                        } else {
                            theme::border_default()
                        },
                    )
                    .child(
                        row()
                            .w_full()
                            .px(8.0)
                            .py(5.0)
                            .gap(5.0)
                            .center_y()
                            .fill(hdr_bg)
                            .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
                            .child(icons::ph(
                                icons::kind_icon(child.kind),
                                badge_sz.max(12.0),
                                if is_media_greyed {
                                    theme::text_muted()
                                } else {
                                    theme::kind_accent(child.kind)
                                },
                            ))
                            .child(
                                el().grow(1.0).min_w(0.0).child(
                                    text(&child.title)
                                        .font(TextStyle::new(title_sz, 600.0))
                                        .color(if is_media_greyed {
                                            theme::text_muted()
                                        } else {
                                            theme::text_primary()
                                        })
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

    (canvas_box, frame_w, frame_h)
}

fn render_popout_section_calendar_child(
    child: &SectionChildSnapshot,
    body_sz: f32,
    badge_sz: f32,
    scale: f32,
    interactive: bool,
) -> El {
    let accent = theme::kind_accent(NodeKind::Calendar);
    let (today_y, today_m, today_d) = media_helper::today_ymd();
    let today_str = media_helper::format_ymd(today_y, today_m, today_d);
    let cy = child.meta.calendar_year.unwrap_or(today_y);
    let cm = child.meta.calendar_month.unwrap_or(today_m);
    let selected_date = child
        .selected_date
        .clone()
        .unwrap_or_else(|| today_str.clone());
    let mode = child.meta.calendar_view_mode;

    let mut cal_col = col()
        .key(format!("popout_sec_cal_{}", child.id.as_str()))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(4.0 * scale);

    if mode == CalendarViewMode::Week {
        let anchor_mon = child
            .meta
            .calendar_week_start
            .clone()
            .unwrap_or_else(|| media_helper::monday_of_week(&selected_date));
        let week = media_helper::week_dates(&anchor_mon);
        let (sy, sm, sd) = media_helper::parse_ymd(&week[0].0).unwrap_or((cy, cm, 1));
        let (ey, em, ed) = media_helper::parse_ymd(&week[6].0).unwrap_or((sy, sm, sd));
        let range = if sy == ey && sm == em {
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
        cal_col = cal_col.child(
            text(range)
                .font(TextStyle::new(badge_sz, 600.0))
                .color(theme::text_primary()),
        );
        let mut wk_row = row().w_full().grow(1.0).min_h(0.0).gap(3.0 * scale);
        for (wd_idx, (date_str, _, day_num)) in week.into_iter().enumerate() {
            let is_sel = selected_date == date_str;
            let day_note = child
                .meta
                .calendar_day_notes
                .get(&date_str)
                .map(String::as_str)
                .unwrap_or("")
                .trim();
            let nid = child.id.clone();
            let d_pick = date_str.clone();
            let mut d_col = col()
                .grow(1.0)
                .h_full()
                .min_h(0.0)
                .padding(3.0 * scale)
                .gap(2.0 * scale)
                .radius(4.0)
                .fill(if is_sel {
                    theme::bg_control()
                } else {
                    theme::bg_input()
                })
                .border(
                    1.0,
                    if is_sel {
                        accent
                    } else {
                        theme::border_subtle()
                    },
                )
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .child(
                            text(i18n::weekday_short(wd_idx))
                                .font(TextStyle::new((badge_sz * 0.85).max(7.0), 500.0).mono())
                                .color(theme::text_muted()),
                        )
                        .child(el().grow(1.0))
                        .child(
                            text(format!("{}", day_num))
                                .font(TextStyle::new(badge_sz, 600.0).mono())
                                .color(if is_sel {
                                    accent
                                } else {
                                    theme::text_primary()
                                }),
                        ),
                );
            if !day_note.is_empty() {
                d_col = d_col.child(
                    text(day_note)
                        .font(TextStyle::new((badge_sz * 0.85).max(7.0), 400.0))
                        .color(theme::text_secondary())
                        .w_full()
                        .wrap(),
                );
            }
            if interactive {
                d_col = d_col.cursor(Cursor::Pointer).on_press(move |s: &mut PopoutReaderState| {
                    s.send_cmd(ReaderCommand::CalendarSelectDate {
                        node_id: nid.clone(),
                        date: d_pick.clone(),
                    });
                });
            }
            wk_row = wk_row.child(d_col);
        }
        return cal_col.child(wk_row);
    }

    cal_col = cal_col.child(
        row()
            .w_full()
            .center_y()
            .child(
                text(format!("{} {}", i18n::month_short(cm), cy))
                    .font(TextStyle::new(badge_sz.max(8.5), 600.0))
                    .color(theme::text_primary()),
            ),
    );
    let mut wk_hdr = row().w_full().gap(2.0 * scale);
    for wd_idx in 0..7 {
        wk_hdr = wk_hdr.child(
            el().grow(1.0).center().child(
                text(i18n::weekday_narrow(wd_idx))
                    .font(TextStyle::new((badge_sz * 0.85).max(7.0), 600.0).mono())
                    .color(theme::text_muted()),
            ),
        );
    }
    cal_col = cal_col.child(wk_hdr);

    let offset = media_helper::weekday_of_first(cy, cm);
    let dim = media_helper::days_in_month(cy, cm);
    let num_rows = (offset + dim + 6) / 7;
    let mut grid = col().w_full().gap(2.0 * scale);
    for r in 0..num_rows {
        let mut r_el = row().w_full().gap(2.0 * scale);
        for c in 0..7 {
            let slot = r * 7 + c;
            if slot < offset || slot >= offset + dim {
                r_el = r_el.child(el().grow(1.0).h((15.0 * scale).clamp(11.0, 22.0)));
            } else {
                let day = slot - offset + 1;
                let date_str = media_helper::format_ymd(cy, cm, day);
                let has_mark = child.meta.calendar_events.iter().any(|e| e.date == date_str)
                    || child
                        .meta
                        .calendar_day_notes
                        .get(&date_str)
                        .is_some_and(|t| !t.trim().is_empty());
                let is_sel = selected_date == date_str;
                let nid = child.id.clone();
                let d_pick = date_str.clone();
                let mut cell = el()
                    .grow(1.0)
                    .h((15.0 * scale).clamp(11.0, 22.0))
                    .radius(3.0)
                    .fill(if is_sel {
                        theme::bg_control()
                    } else if has_mark {
                        accent.alpha(0.14)
                    } else {
                        Color::TRANSPARENT
                    })
                    .border(
                        if is_sel { 1.0 } else { 0.0 },
                        if is_sel { accent } else { Color::TRANSPARENT },
                    )
                    .center()
                    .child(
                        text(format!("{}", day))
                            .font(TextStyle::new((badge_sz * 0.88).max(7.0), 500.0).mono())
                            .color(if is_sel || has_mark {
                                accent
                            } else {
                                theme::text_secondary()
                            }),
                    );
                if interactive {
                    cell = cell.cursor(Cursor::Pointer).on_press(move |s: &mut PopoutReaderState| {
                        s.send_cmd(ReaderCommand::CalendarSelectDate {
                            node_id: nid.clone(),
                            date: d_pick.clone(),
                        });
                    });
                }
                r_el = r_el.child(cell);
            }
        }
        grid = grid.child(r_el);
    }
    cal_col = cal_col.child(grid);

    if let Some(ev) = child
        .meta
        .calendar_events
        .iter()
        .find(|e| e.date == selected_date)
        .or_else(|| child.meta.calendar_events.first())
    {
        cal_col = cal_col.child(
            text(format!("{} · {}", ev.date, ev.title))
                .font(TextStyle::new((body_sz * 0.85).max(7.5), 400.0))
                .color(theme::text_secondary())
                .truncate(),
        );
    }

    cal_col
}

fn render_popout_section_frame(s: &PopoutReaderState, snap: &ReaderSnapshot) -> El {
    let avail_w = (s.viewport_width - 56.0).clamp(320.0, 1280.0);
    let (canvas_box, frame_w, _) = build_scheme_frame_el(snap, &s.section_images, avail_w, true);

    let mut scroll_col = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .padding(18.0)
        .gap(14.0)
        .items_center()
        .scroll(ScrollSpec { stick_bottom: false });

    if snap.ai_summarizing
        || snap
            .summary
            .as_deref()
            .is_some_and(|t| !t.trim().is_empty())
    {
        scroll_col = scroll_col.child(
            col()
                .key("popout_sec_ai_summary")
                .w(frame_w)
                .padding(12.0)
                .gap(6.0)
                .radius(8.0)
                .fill(theme::bg_card())
                .border(1.0, theme::accent_claude().alpha(0.35))
                .child(
                    row()
                        .center_y()
                        .gap(6.0)
                        .child(icons::ph(&icons::SPARKLE, 12.0, theme::accent_claude()))
                        .child(
                            text(i18n::t(TextKey::PopoutAiReview))
                                .font(theme::SECTION_LABEL)
                                .color(theme::accent_claude()),
                        ),
                )
                .child(
                    text(if snap.ai_summarizing {
                        i18n::t(TextKey::ReaderSummarizing)
                    } else {
                        snap.summary.as_deref().unwrap_or("")
                    })
                    .font(theme::BODY_TEXT)
                    .color(theme::text_primary())
                    .w_full()
                    .wrap(),
                ),
        );
    }

    scroll_col.child(canvas_box)
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
                .child(icons::ph(
                    icons::kind_icon(NodeKind::Flashcard),
                    15.0,
                    theme::kind_accent(NodeKind::Flashcard),
                ))
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
                text(if flipped { "↺" } else { i18n::t(TextKey::CardReveal) })
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
        ("btn_popout_flash_rate_again", i18n::t(TextKey::ReaderMasteryAgain), "again"),
        ("btn_popout_flash_rate_good", i18n::t(TextKey::ReaderMasteryGood), "good"),
        ("btn_popout_flash_rate_easy", i18n::t(TextKey::ReaderMasteryEasy), "easy"),
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

fn render_popout_graph_formula(
    s: &PopoutReaderState,
    snap: &ReaderSnapshot,
    node_id: &NodeId,
    is_ro: bool,
) -> El {
    let accent = theme::kind_accent(NodeKind::GraphFormula);
    let plot_w = (s.viewport_width - 120.0).clamp(320.0, 700.0);
    let plot_h = 300.0;
    let vp = graph_formula::GraphViewport::from_meta(&snap.meta);
    let active_content = if is_ro {
        snap.content.as_str()
    } else {
        s.source_edit.text.as_str()
    };
    let plot = graph_formula::build_graph_plot(active_content, &vp, plot_w, plot_h, 1.0);

    let nid_in = node_id.clone();
    let nid_out = node_id.clone();
    let nid_reset = node_id.clone();
    let nid_wheel = node_id.clone();
    let nid_drag = node_id.clone();

    let mut card = col()
        .key("popout_graph_formula_view")
        .w_full()
        .max_w(760.0)
        .padding(22.0)
        .gap(12.0)
        .radius(10.0)
        .fill(theme::bg_card())
        .border(1.0, theme::border_default())
        .child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(icons::ph(
                    icons::kind_icon(NodeKind::GraphFormula),
                    16.0,
                    accent,
                ))
                .child(
                    text(&snap.title)
                        .font(TextStyle::new(17.0, 600.0))
                        .color(theme::text_primary()),
                )
                .child(el().grow(1.0))
                .child(
                    text(graph_formula::format_zoom_badge(&vp))
                        .font(theme::BADGE_TEXT)
                        .color(accent),
                ),
        )
        .child(
            col()
                .w_full()
                .padding(14.0)
                .radius(6.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_subtle())
                .child(graph_formula::render_pretty_math(active_content, 19.0)),
        );

    if let Some(ro) = &plot.readout {
        card = card.child(
            el()
                .w_full()
                .px(14.0)
                .py(8.0)
                .radius(6.0)
                .fill(theme::accent_claude())
                .child(
                    text(ro)
                        .font(TextStyle::new(14.5, 600.0).mono())
                        .color(theme::text_inverted()),
                ),
        );
    }

    if !is_ro {
        let nid_f = node_id.clone();
        card = card.child(
            col()
                .w_full()
                .padding(10.0)
                .radius(6.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_default())
                .child(
                    text_input(&s.source_edit, TextStyle::new(14.5, 500.0).mono().lh(20.0))
                        .key("popout_graph_formula_input")
                        .label("Graphic formula source")
                        .w_full()
                        .min_h(36.0)
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
                        }),
                ),
        );
    }

    let mut plot_box = el()
        .key("popout_graph_formula_plot")
        .w(plot_w)
        .h(plot_h)
        .radius(8.0)
        .clip_rounded(8.0)
        .fill(theme::bg_input())
        .border(1.0, theme::border_default())
        .cursor(Cursor::Grab)
        .on_wheel(move |s: &mut PopoutReaderState, e, tx| {
            let factor = (-e.dy * 0.0022).clamp(-0.35, 0.35).exp() as f64;
            let anchor_norm = if e.x >= 0.0 && e.x <= plot_w && e.y >= 0.0 && e.y <= plot_h {
                Some((e.x / plot_w.max(1.0), e.y / plot_h.max(1.0)))
            } else {
                Some((0.5, 0.5))
            };
            s.send_cmd(ReaderCommand::ZoomGraphFormula {
                node_id: nid_wheel.clone(),
                factor,
                anchor_norm,
                plot_size: (plot_w, plot_h),
            });
            tx.instant();
            true
        })
        .on_drag(move |s: &mut PopoutReaderState, d, tx| match d.phase {
            Phase::Down => {
                let cur_vp = graph_formula::GraphViewport::from_meta(&s.snapshot.meta);
                s.graph_drag_origin = Some((nid_drag.clone(), cur_vp.center_x, cur_vp.center_y));
            }
            Phase::Move => {
                if let Some((orig_id, cx0, cy0)) = s.graph_drag_origin.clone() {
                    if orig_id == nid_drag {
                        s.send_cmd(ReaderCommand::PanGraphFormula {
                            node_id: nid_drag.clone(),
                            start_cx: cx0,
                            start_cy: cy0,
                            dx: d.dx,
                            dy: d.dy,
                            plot_size: (plot_w, plot_h),
                            commit: false,
                        });
                        tx.instant();
                    }
                }
            }
            Phase::Up => {
                if let Some((orig_id, cx0, cy0)) = s.graph_drag_origin.take() {
                    if orig_id == nid_drag {
                        s.send_cmd(ReaderCommand::PanGraphFormula {
                            node_id: nid_drag.clone(),
                            start_cx: cx0,
                            start_cy: cy0,
                            dx: d.dx,
                            dy: d.dy,
                            plot_size: (plot_w, plot_h),
                            commit: true,
                        });
                    }
                }
            }
        })
        .child(canvas(plot.shape).absolute(0.0, 0.0).w_full().h_full());

    for tl in plot.tick_labels {
        let tx_pos = if tl.align_right {
            (tl.x - (tl.text.len() as f32 * 5.8)).clamp(2.0, (plot_w - 22.0).max(2.0))
        } else {
            tl.x.clamp(2.0, (plot_w - 22.0).max(2.0))
        };
        let ty_pos = tl.y.clamp(2.0, (plot_h - 12.0).max(2.0));
        plot_box = plot_box.child(
            el().absolute(tx_pos, ty_pos).child(
                text(tl.text)
                    .font(TextStyle::new(10.0, 450.0).mono())
                    .color(theme::text_muted()),
            ),
        );
    }

    card = card.child(plot_box).child(
        row()
            .w_full()
            .center_y()
            .gap(6.0)
            .child(el().grow(1.0))
            .child(toolbar_chip("btn_popout_graph_in", "+", theme::text_primary(), move |s| {
                s.send_cmd(ReaderCommand::ZoomGraphFormula {
                    node_id: nid_in.clone(),
                    factor: 0.72,
                    anchor_norm: None,
                    plot_size: (plot_w, plot_h),
                });
            }))
            .child(toolbar_chip("btn_popout_graph_out", "−", theme::text_primary(), move |s| {
                s.send_cmd(ReaderCommand::ZoomGraphFormula {
                    node_id: nid_out.clone(),
                    factor: 1.39,
                    anchor_norm: None,
                    plot_size: (plot_w, plot_h),
                });
            }))
            .child(toolbar_chip(
                "btn_popout_graph_reset",
                i18n::t(TextKey::GraphFormulaResetView),
                accent,
                move |s| {
                    s.send_cmd(ReaderCommand::ResetGraphFormulaView(nid_reset.clone()));
                },
            )),
    );

    if !is_ro {
        let mode = s.graph_keypad_mode;
        let rows = match mode {
            graph_formula::KeypadMode::Sci => graph_formula::SCI_KEYPAD_ROWS,
            graph_formula::KeypadMode::Basic => graph_formula::BASIC_KEYPAD_ROWS,
        };
        let mut keypad = col()
            .w_full()
            .gap(4.0)
            .padding(8.0)
            .radius(8.0)
            .fill(theme::bg_chrome())
            .border(1.0, theme::border_subtle());
        for (r_idx, row_btns) in rows.iter().enumerate() {
            let mut r = row().w_full().gap(4.0);
            for btn in *row_btns {
                let nid_k = node_id.clone();
                let act = btn.action;
                r = r.child(
                    el()
                        .key(format!("popout_kp_{r_idx}_{}", btn.id))
                        .grow(1.0)
                        .min_w(0.0)
                        .h(30.0)
                        .radius(5.0)
                        .fill(theme::bg_input())
                        .hover(theme::hover_subtle())
                        .border(1.0, theme::border_subtle())
                        .center()
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut PopoutReaderState| {
                            s.send_cmd(ReaderCommand::ApplyGraphFormulaKeypad {
                                node_id: nid_k.clone(),
                                action: act,
                            });
                        })
                        .child(
                            text(btn.label)
                                .font(TextStyle::new(12.0, 500.0))
                                .color(theme::text_primary()),
                        ),
                );
            }
            keypad = keypad.child(r);
        }
        card = card.child(keypad);
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
                        i18n::t(TextKey::SequenceSpawnNext),
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
                    text(if is_busy {
                        i18n::t(TextKey::ReaderOcrExtracting)
                    } else {
                        i18n::t(TextKey::ReaderOcrExtractText)
                    })
                    .font(theme::BADGE_TEXT)
                    .color(theme::text_inverted()),
                ),
        )
        .child(toolbar_chip(
            "btn_popout_img_ocr_formula",
            i18n::t(TextKey::ReaderOcrFormula),
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
        ocr_row = ocr_row.child(
            el()
                .key("btn_popout_download_neural")
                .px(8.0)
                .py(4.0)
                .radius(6.0)
                .hover(theme::hover_subtle())
                .press(theme::press_subtle())
                .center()
                .cursor(Cursor::Pointer)
                .on_press(|s: &mut PopoutReaderState| {
                    s.snapshot.vision_downloading_models = true;
                    s.send_cmd(ReaderCommand::DownloadNeuralModels);
                })
                .child(if snap.vision_downloading_models {
                    icons::ph(&icons::DOTS_THREE, 13.5, theme::accent_claude())
                } else {
                    row()
                        .center_y()
                        .gap(4.0)
                        .child(icons::ph(&icons::ARROW_DOWN, 12.5, theme::accent_claude()))
                        .child(
                            text("Models")
                                .font(theme::STATUS_TEXT)
                                .color(theme::accent_claude()),
                        )
                }),
        );
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
                                    text(if ocr_copied {
                                        i18n::t(TextKey::ReaderCopied)
                                    } else {
                                        i18n::t(TextKey::ReaderCopy)
                                    })
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
                        .child(toolbar_icon_chip(
                            "btn_popout_ocr_clear",
                            &icons::X,
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
            if is_busy {
                i18n::t(TextKey::ReaderTranslating)
            } else {
                i18n::t(TextKey::ReaderTranslate)
            },
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
                                    text(if tr_copied {
                                        i18n::t(TextKey::ReaderCopied)
                                    } else {
                                        i18n::t(TextKey::ReaderCopy)
                                    })
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
                        .child(toolbar_icon_chip(
                            "btn_popout_translate_clear",
                            &icons::X,
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

fn render_popout_calendar(
    _s: &PopoutReaderState,
    snap: &ReaderSnapshot,
    node_id: &NodeId,
) -> El {
    let accent = theme::kind_accent(NodeKind::Calendar);
    let (today_y, today_m, today_d) = media_helper::today_ymd();
    let today_str = media_helper::format_ymd(today_y, today_m, today_d);
    let cy = snap.meta.calendar_year.unwrap_or(today_y);
    let cm = snap.meta.calendar_month.unwrap_or(today_m);
    let selected_date = snap
        .selected_calendar_date
        .clone()
        .unwrap_or_else(|| today_str.clone());
    let mode = snap.meta.calendar_view_mode;

    let nid_m = node_id.clone();
    let nid_w = node_id.clone();
    let nid_prev = node_id.clone();
    let nid_next = node_id.clone();
    let nid_wprev = node_id.clone();
    let nid_wnext = node_id.clone();

    let mut card = col()
        .key("popout_calendar_view")
        .w_full()
        .max_w(780.0)
        .padding(20.0)
        .gap(14.0)
        .radius(10.0)
        .fill(theme::bg_card())
        .border(1.0, theme::border_default())
        .child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(icons::ph(icons::kind_icon(NodeKind::Calendar), 15.0, accent))
                .child(
                    text(&snap.title)
                        .font(theme::PANEL_TITLE)
                        .color(theme::text_primary()),
                )
                .child(el().grow(1.0))
                .child(
                    row()
                        .padding(2.0)
                        .gap(4.0)
                        .radius(6.0)
                        .fill(theme::bg_input())
                        .border(1.0, theme::border_subtle())
                        .child(seg_btn(
                            "popout_cal_mode_month",
                            i18n::t(TextKey::CalModeMonth),
                            mode == CalendarViewMode::Month,
                        )
                        .on_press(move |s: &mut PopoutReaderState| {
                            s.snapshot.meta.calendar_view_mode = CalendarViewMode::Month;
                            s.send_cmd(ReaderCommand::CalendarSetMode {
                                node_id: nid_m.clone(),
                                mode: CalendarViewMode::Month,
                            });
                        }))
                        .child(seg_btn(
                            "popout_cal_mode_week",
                            i18n::t(TextKey::CalModeWeek),
                            mode == CalendarViewMode::Week,
                        )
                        .on_press(move |s: &mut PopoutReaderState| {
                            s.snapshot.meta.calendar_view_mode = CalendarViewMode::Week;
                            s.send_cmd(ReaderCommand::CalendarSetMode {
                                node_id: nid_w.clone(),
                                mode: CalendarViewMode::Week,
                            });
                        })),
                ),
        );

    if mode == CalendarViewMode::Week {
        let anchor_mon = snap
            .meta
            .calendar_week_start
            .clone()
            .unwrap_or_else(|| media_helper::monday_of_week(&selected_date));
        let week = media_helper::week_dates(&anchor_mon);
        let (sy, sm, sd) = media_helper::parse_ymd(&week[0].0).unwrap_or((cy, cm, 1));
        let (ey, em, ed) = media_helper::parse_ymd(&week[6].0).unwrap_or((sy, sm, sd));
        let range_label = format!(
            "{} {} – {} {}, {}",
            i18n::month_short(sm),
            sd,
            i18n::month_short(em),
            ed,
            ey
        );

        card = card.child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(toolbar_chip(
                    "popout_cal_week_prev",
                    "‹",
                    theme::text_primary(),
                    move |s| {
                        s.send_cmd(ReaderCommand::CalendarStepWeek {
                            node_id: nid_wprev.clone(),
                            delta: -1,
                        });
                    },
                ))
                .child(el().grow(1.0).center().child(
                    text(range_label)
                        .font(theme::BODY_TEXT)
                        .color(theme::text_primary()),
                ))
                .child(toolbar_chip(
                    "popout_cal_week_next",
                    "›",
                    theme::text_primary(),
                    move |s| {
                        s.send_cmd(ReaderCommand::CalendarStepWeek {
                            node_id: nid_wnext.clone(),
                            delta: 1,
                        });
                    },
                )),
        );

        let mut week_grid = row().w_full().gap(8.0);
        for (wd_idx, (date_str, _, day_num)) in week.into_iter().enumerate() {
            let is_sel = selected_date == date_str;
            let day_note = snap
                .meta
                .calendar_day_notes
                .get(&date_str)
                .map(String::as_str)
                .unwrap_or("")
                .trim();
            let nid_pick = node_id.clone();
            let d_pick = date_str.clone();
            let mut d_box = col()
                .key(format!("popout_cal_wk_{}", date_str))
                .grow(1.0)
                .min_h(120.0)
                .padding(8.0)
                .gap(6.0)
                .radius(6.0)
                .fill(if is_sel {
                    theme::bg_control()
                } else {
                    theme::bg_input()
                })
                .border(
                    1.0,
                    if is_sel {
                        accent
                    } else {
                        theme::border_subtle()
                    },
                )
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut PopoutReaderState| {
                    s.snapshot.selected_calendar_date = Some(d_pick.clone());
                    s.send_cmd(ReaderCommand::CalendarSelectDate {
                        node_id: nid_pick.clone(),
                        date: d_pick.clone(),
                    });
                })
                .child(
                    row()
                        .w_full()
                        .center_y()
                        .child(
                            text(i18n::weekday_short(wd_idx))
                                .font(theme::BADGE_TEXT)
                                .color(theme::text_muted()),
                        )
                        .child(el().grow(1.0))
                        .child(
                            text(format!("{}", day_num))
                                .font(theme::CONTROL_TEXT)
                                .color(if is_sel {
                                    accent
                                } else {
                                    theme::text_primary()
                                }),
                        ),
                );
            if !day_note.is_empty() {
                d_box = d_box.child(
                    text(day_note)
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_secondary())
                        .w_full()
                        .wrap(),
                );
            }
            week_grid = week_grid.child(d_box);
        }
        card = card.child(week_grid);
    } else {
        card = card.child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(toolbar_chip(
                    "popout_cal_month_prev",
                    "‹",
                    theme::text_primary(),
                    move |s| {
                        s.send_cmd(ReaderCommand::CalendarStepMonth {
                            node_id: nid_prev.clone(),
                            delta: -1,
                        });
                    },
                ))
                .child(el().grow(1.0).center().child(
                    text(format!("{} {}", i18n::month_full(cm), cy))
                        .font(theme::PANEL_TITLE)
                        .color(theme::text_primary()),
                ))
                .child(toolbar_chip(
                    "popout_cal_month_next",
                    "›",
                    theme::text_primary(),
                    move |s| {
                        s.send_cmd(ReaderCommand::CalendarStepMonth {
                            node_id: nid_next.clone(),
                            delta: 1,
                        });
                    },
                )),
        );

        let mut wk_row = row().w_full().gap(4.0);
        for wd_idx in 0..7 {
            wk_row = wk_row.child(
                el().grow(1.0).center().child(
                    text(i18n::weekday_short(wd_idx))
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_muted()),
                ),
            );
        }
        card = card.child(wk_row);

        let offset = media_helper::weekday_of_first(cy, cm);
        let dim = media_helper::days_in_month(cy, cm);
        let num_rows = (offset + dim + 6) / 7;
        let mut grid = col().w_full().gap(4.0);
        for r in 0..num_rows {
            let mut r_el = row().w_full().gap(4.0);
            for c in 0..7 {
                let slot = r * 7 + c;
                if slot < offset || slot >= offset + dim {
                    r_el = r_el.child(el().grow(1.0).h(44.0));
                } else {
                    let day = slot - offset + 1;
                    let date_str = media_helper::format_ymd(cy, cm, day);
                    let has_mark = snap.meta.calendar_events.iter().any(|e| e.date == date_str)
                        || snap
                            .meta
                            .calendar_day_notes
                            .get(&date_str)
                            .is_some_and(|t| !t.trim().is_empty());
                    let is_sel = selected_date == date_str;
                    let nid_pick = node_id.clone();
                    let d_pick = date_str.clone();
                    r_el = r_el.child(
                        el()
                            .key(format!("popout_cal_day_{}", date_str))
                            .grow(1.0)
                            .h(44.0)
                            .radius(6.0)
                            .fill(if is_sel {
                                theme::bg_control()
                            } else if has_mark {
                                accent.alpha(0.14)
                            } else {
                                theme::bg_input()
                            })
                            .border(
                                1.0,
                                if is_sel {
                                    accent
                                } else {
                                    theme::border_subtle()
                                },
                            )
                            .hover(theme::hover_subtle())
                            .center()
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut PopoutReaderState| {
                                s.snapshot.selected_calendar_date = Some(d_pick.clone());
                                s.send_cmd(ReaderCommand::CalendarSelectDate {
                                    node_id: nid_pick.clone(),
                                    date: d_pick.clone(),
                                });
                            })
                            .child(
                                text(format!("{}", day))
                                    .font(theme::CONTROL_TEXT)
                                    .color(if is_sel || has_mark {
                                        accent
                                    } else {
                                        theme::text_primary()
                                    }),
                            ),
                    );
                }
            }
            grid = grid.child(r_el);
        }
        card = card.child(grid);
    }

    if let Some(note_txt) = snap
        .meta
        .calendar_day_notes
        .get(&selected_date)
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
    {
        card = card.child(
            col()
                .w_full()
                .padding(12.0)
                .gap(4.0)
                .radius(6.0)
                .fill(theme::bg_input())
                .border(1.0, accent.alpha(0.3))
                .child(
                    text(format!("{} · {}", i18n::t(TextKey::CalDayNoteTitle), selected_date))
                        .font(theme::BADGE_TEXT)
                        .color(accent),
                )
                .child(
                    text(note_txt)
                        .font(theme::BODY_TEXT)
                        .color(theme::text_primary())
                        .w_full()
                        .wrap(),
                ),
        );
    }

    if !snap.meta.calendar_events.is_empty() {
        let mut ev_col = col()
            .w_full()
            .gap(6.0)
            .pt(6.0)
            .border_sides([1.0, 0.0, 0.0, 0.0], theme::border_subtle())
            .child(
                text(i18n::t(TextKey::CalEventsTitle))
                    .font(theme::SECTION_LABEL)
                    .color(theme::text_muted()),
            );
        for (idx, ev) in snap.meta.calendar_events.iter().enumerate() {
            let nid_ev = node_id.clone();
            let ev_id = ev.id.clone();
            ev_col = ev_col.child(
                row()
                    .key(format!("popout_cal_evt_{}", idx))
                    .w_full()
                    .px(10.0)
                    .py(6.0)
                    .gap(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme::bg_input())
                    .hover(theme::hover_subtle())
                    .cursor(Cursor::Pointer)
                    .on_press(move |s: &mut PopoutReaderState| {
                        s.send_cmd(ReaderCommand::ToggleCalendarEvent {
                            node_id: nid_ev.clone(),
                            event_id: ev_id.clone(),
                        });
                    })
                    .child(icons::ph(
                        if ev.completed {
                            &icons::CHECK_SQUARE
                        } else {
                            &icons::SQUARE
                        },
                        14.0,
                        if ev.completed {
                            theme::text_muted()
                        } else {
                            accent
                        },
                    ))
                    .child(
                        text(format!("{} · {}", ev.date, ev.title))
                            .font(theme::BODY_TEXT)
                            .color(if ev.completed {
                                theme::text_muted()
                            } else {
                                theme::text_primary()
                            }),
                    ),
            );
        }
        card = card.child(ev_col);
    }

    col()
        .w_full()
        .grow(1.0)
        .padding(24.0)
        .items_center()
        .scroll(ScrollSpec { stick_bottom: false })
        .child(card)
}

fn popout_toggle_chip(
    key: &'static str,
    label: &str,
    active: bool,
    accent: Color,
    action: impl Fn(&mut PopoutReaderState) + 'static,
) -> El {
    el()
        .key(key)
        .px(8.0)
        .py(4.0)
        .radius(5.0)
        .fill(if active {
            accent.alpha(0.18)
        } else {
            theme::bg_control()
        })
        .border(
            1.0,
            if active {
                accent.alpha(0.55)
            } else {
                theme::border_subtle()
            },
        )
        .hover(theme::hover_subtle())
        .cursor(Cursor::Pointer)
        .on_press(action)
        .child(
            text(label)
                .font(theme::BADGE_TEXT)
                .color(if active {
                    accent
                } else {
                    theme::text_secondary()
                }),
        )
}

fn render_popout_drawing(
    s: &PopoutReaderState,
    snap: &ReaderSnapshot,
    node_id: &NodeId,
) -> El {
    let accent = theme::kind_accent(NodeKind::Drawing);
    let pad_w = (s.viewport_width - 96.0).clamp(360.0, 760.0);
    let pad_h = 340.0;
    let shape = crate::canvas_view::build_drawing_shape(&snap.meta.drawing_strokes, pad_w, pad_h, 1.0);

    let drag_id = node_id.clone();
    let undo_id = node_id.clone();
    let clear_id = node_id.clone();
    let txt_id = node_id.clone();
    let eq_id = node_id.clone();
    let spawn_n = node_id.clone();
    let spawn_f = node_id.clone();
    let eraser_active = s.drawing_eraser;

    let mut color_swatches = row().center_y().gap(6.0);
    for (ckey, hex) in crate::inspector_view::DRAWING_COLORS {
        let active = !eraser_active && s.drawing_color_hex.eq_ignore_ascii_case(hex);
        let swatch_color = crate::inspector_view::drawing_swatch_color(hex);
        color_swatches = color_swatches.child(
            el()
                .key(format!("popout_{}", ckey))
                .size(16.0, 16.0)
                .radius(8.0)
                .fill(swatch_color)
                .border(
                    if active { 2.0 } else { 1.0 },
                    if active {
                        theme::text_primary()
                    } else {
                        theme::border_default()
                    },
                )
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut PopoutReaderState| {
                    s.drawing_eraser = false;
                    s.drawing_color_hex = hex.to_string();
                }),
        );
    }

    let widths = [
        ("popout_draw_w_thin", i18n::t(TextKey::DrawingThin), 2.0f32),
        ("popout_draw_w_med", i18n::t(TextKey::DrawingMedium), 3.5f32),
        ("popout_draw_w_thick", i18n::t(TextKey::DrawingThick), 6.0f32),
    ];
    let mut width_row = row().center_y().gap(4.0);
    for (wkey, wlabel, wval) in widths {
        let active = (s.drawing_stroke_width - wval).abs() < 0.4;
        width_row = width_row.child(
            el()
                .key(wkey)
                .px(7.0)
                .py(3.0)
                .radius(5.0)
                .fill(if active {
                    theme::bg_control()
                } else {
                    Color::TRANSPARENT
                })
                .border(
                    1.0,
                    if active {
                        theme::border_strong()
                    } else {
                        theme::border_subtle()
                    },
                )
                .hover(theme::hover_subtle())
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut PopoutReaderState| {
                    s.drawing_stroke_width = wval;
                })
                .child(
                    text(wlabel)
                        .font(theme::BADGE_TEXT)
                        .color(if active {
                            theme::text_primary()
                        } else {
                            theme::text_muted()
                        }),
                ),
        );
    }

    let mut card = col()
        .key("popout_drawing_view")
        .w_full()
        .max_w(800.0)
        .padding(20.0)
        .gap(14.0)
        .radius(10.0)
        .fill(theme::bg_card())
        .border(1.0, theme::border_default())
        .child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(icons::ph(icons::kind_icon(NodeKind::Drawing), 15.0, accent))
                .child(
                    text(&snap.title)
                        .font(theme::PANEL_TITLE)
                        .color(theme::text_primary()),
                )
                .child(el().grow(1.0))
                .child(popout_toggle_chip(
                    "popout_draw_pen",
                    i18n::t(TextKey::DrawingPen),
                    !eraser_active,
                    accent,
                    |s| s.drawing_eraser = false,
                ))
                .child(popout_toggle_chip(
                    "popout_draw_eraser",
                    i18n::t(TextKey::DrawingEraser),
                    eraser_active,
                    accent,
                    |s| s.drawing_eraser = true,
                ))
                .child(toolbar_icon_chip(
                    "popout_draw_undo",
                    &icons::ARROW_CLOCKWISE,
                    theme::text_secondary(),
                    move |s| {
                        s.snapshot.meta.drawing_strokes.pop();
                        s.send_cmd(ReaderCommand::UndoDrawingStroke(undo_id.clone()));
                    },
                ))
                .child(toolbar_icon_chip(
                    "popout_draw_clear",
                    &icons::X,
                    theme::text_muted(),
                    move |s| {
                        s.snapshot.meta.drawing_strokes.clear();
                        s.snapshot.meta.extracted_text = None;
                        s.send_cmd(ReaderCommand::ClearDrawingStrokes(clear_id.clone()));
                    },
                )),
        )
        .child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(color_swatches)
                .child(el().grow(1.0))
                .child(width_row),
        )
        .child({
            let mut pad = el()
                .key("popout_drawing_canvas")
                .w(pad_w)
                .h(pad_h)
                .radius(8.0)
                .fill(theme::bg_input())
                .border(1.0, theme::border_subtle())
                .cursor(Cursor::Crosshair)
                .on_drag(move |s: &mut PopoutReaderState, d, tx| {
                    let nx = (d.x / d.w.max(1.0)).clamp(0.0, 1.0);
                    let ny = (d.y / d.h.max(1.0)).clamp(0.0, 1.0);
                    match d.phase {
                        Phase::Down => {
                            let color_hex = s.drawing_color_hex.clone();
                            let width = s.drawing_stroke_width;
                            let eraser = s.drawing_eraser;
                            crate::state::drawing_begin_stroke(
                                &mut s.snapshot.meta.drawing_strokes,
                                nx,
                                ny,
                                &color_hex,
                                width,
                                eraser,
                            );
                            s.send_cmd(ReaderCommand::BeginDrawingStroke {
                                node_id: drag_id.clone(),
                                nx,
                                ny,
                                color_hex,
                                width,
                                eraser,
                            });
                            tx.instant();
                        }
                        Phase::Move => {
                            let color_hex = s.drawing_color_hex.clone();
                            let width = s.drawing_stroke_width;
                            let eraser = s.drawing_eraser;
                            crate::state::drawing_extend_stroke(
                                &mut s.snapshot.meta.drawing_strokes,
                                nx,
                                ny,
                                &color_hex,
                                width,
                                eraser,
                            );
                            s.send_cmd(ReaderCommand::ExtendDrawingStroke {
                                node_id: drag_id.clone(),
                                nx,
                                ny,
                                eraser,
                            });
                            tx.instant();
                        }
                        Phase::Up => {
                            s.send_cmd(ReaderCommand::FinishDrawingStroke(drag_id.clone()));
                        }
                    }
                })
                .child(canvas(shape).absolute(0.0, 0.0).w_full().h_full());
            if snap.meta.drawing_strokes.is_empty() {
                pad = pad.child(
                    el().absolute(0.0, 0.0).w_full().h_full().center().child(
                        text(i18n::t(TextKey::DrawingEmptyHint))
                            .font(theme::BODY_TEXT)
                            .color(theme::text_muted()),
                    ),
                );
            }
            pad
        })
        .child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(toolbar_chip(
                    "popout_draw_interpret_text",
                    if snap.vision_pending {
                        i18n::t(TextKey::DrawingInterpreting)
                    } else {
                        i18n::t(TextKey::DrawingInterpretText)
                    },
                    accent,
                    move |s| {
                        s.snapshot.vision_pending = true;
                        s.send_cmd(ReaderCommand::InterpretDrawing {
                            node_id: txt_id.clone(),
                            mode: OcrMode::PlainText,
                        });
                    },
                ))
                .child(toolbar_chip(
                    "popout_draw_interpret_formula",
                    i18n::t(TextKey::DrawingInterpretFormula),
                    theme::accent_claude(),
                    move |s| {
                        s.snapshot.vision_pending = true;
                        s.send_cmd(ReaderCommand::InterpretDrawing {
                            node_id: eq_id.clone(),
                            mode: OcrMode::EquationFormula,
                        });
                    },
                )),
        );

    if let Some(err) = &snap.vision_error {
        card = card.child(
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
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        let extracted_owned = extracted.to_string();
        let copied = s.copied_slot == Some("drawing_ocr");
        card = card.child(
            col()
                .key("popout_drawing_ocr_box")
                .w_full()
                .padding(12.0)
                .gap(8.0)
                .radius(8.0)
                .fill(theme::bg_input())
                .border(1.0, accent.alpha(0.35))
                .child(
                    text(i18n::t(TextKey::DrawingRecognizedTitle))
                        .font(theme::SECTION_LABEL)
                        .color(accent),
                )
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
                        .gap(6.0)
                        .child(
                            el()
                                .key("btn_popout_drawing_copy")
                                .px(8.0)
                                .py(4.0)
                                .radius(5.0)
                                .fill(theme::bg_control())
                                .hover(theme::hover_subtle())
                                .cursor(Cursor::Pointer)
                                .on_press_tx(move |s: &mut PopoutReaderState, tx: &mut Tx| {
                                    tx.copy(extracted_owned.clone());
                                    s.copied_slot = Some("drawing_ocr");
                                })
                                .child(
                                    text(if copied {
                                        i18n::t(TextKey::ReaderCopied)
                                    } else {
                                        i18n::t(TextKey::DrawingCopyText)
                                    })
                                    .font(theme::STATUS_TEXT)
                                    .color(if copied {
                                        accent
                                    } else {
                                        theme::text_primary()
                                    }),
                                ),
                        )
                        .child(el().grow(1.0))
                        .child(toolbar_chip(
                            "btn_popout_draw_spawn_note",
                            "+ Note",
                            theme::text_secondary(),
                            move |s| s.send_cmd(ReaderCommand::SpawnNoteFromOcr(spawn_n.clone())),
                        ))
                        .child(toolbar_chip(
                            "btn_popout_draw_spawn_formula",
                            "+ Formula",
                            theme::text_secondary(),
                            move |s| {
                                s.send_cmd(ReaderCommand::SpawnFormulaFromOcr(spawn_f.clone()))
                            },
                        )),
                ),
        );
    }

    col()
        .w_full()
        .grow(1.0)
        .padding(24.0)
        .items_center()
        .scroll(ScrollSpec { stick_bottom: false })
        .child(card)
}

fn render_offscreen_scheme_root(
    snap: &ReaderSnapshot,
    images: &HashMap<NodeId, (PathBuf, rune::Image)>,
    logical_w: f32,
    logical_h: f32,
) -> El {
    let avail_w = (logical_w - 48.0).max(240.0);
    let (canvas_box, frame_w, _) = build_scheme_frame_el(snap, images, avail_w, false);
    col()
        .size(logical_w, logical_h)
        .padding(24.0)
        .gap(12.0)
        .items_center()
        .fill(theme::bg_canvas())
        .child(
            row()
                .w(frame_w)
                .center_y()
                .gap(8.0)
                .child(icons::ph(
                    icons::kind_icon(NodeKind::Section),
                    15.0,
                    theme::kind_accent(NodeKind::Section),
                ))
                .child(
                    text(&snap.title)
                        .font(theme::PANEL_TITLE)
                        .color(theme::text_primary())
                        .truncate(),
                )
                .child(el().grow(1.0))
                .child(
                    text(i18n::fmt_blocks_count(snap.section_children.len()))
                        .font(theme::BADGE_TEXT)
                        .color(theme::text_muted()),
                ),
        )
        .child(canvas_box)
}

/// Export a Section or Canvas scheme snapshot as a high-DPI 2x PNG screenshot.
pub fn render_scheme_to_png(
    snap: &ReaderSnapshot,
    out_path: &std::path::Path,
) -> Result<(u32, u32), String> {
    let cache = ImageCache::new(Arc::new(|| {}));
    let mut images: HashMap<NodeId, (PathBuf, rune::Image)> = HashMap::new();
    for child in &snap.section_children {
        if child.kind == NodeKind::Image {
            if let Some(handle) = &child.image_handle {
                let _ = handle.wait();
                let p = child.asset_path.clone().unwrap_or_default();
                images.insert(child.id.clone(), (p, handle.clone()));
            } else if let Some(path) = &child.asset_path {
                let img = cache.file(path, 1024.0, 2.0);
                let _ = img.wait();
                images.insert(child.id.clone(), (path.clone(), img));
            }
        }
    }

    let sec_w = snap.section_size.0.max(240.0);
    let sec_h = snap.section_size.1.max(120.0);
    let scale = (1280.0 / sec_w).clamp(1.0, 1.35);
    let frame_w = sec_w * scale;
    let frame_h = sec_h * scale;
    let logical_w = (frame_w + 48.0).clamp(400.0, 1920.0);
    let logical_h = (frame_h + 92.0).clamp(280.0, 1440.0);
    let dpi_scale = 2.0f32;
    let pw = (logical_w * dpi_scale).round() as u32;
    let ph = (logical_h * dpi_scale).round() as u32;

    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let is_dark = theme::active_theme().is_dark();
    let snap_gpu = snap.clone();
    let images_gpu = images;
    let out_gpu = out_path.to_path_buf();
    let gpu_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || -> Result<(u32, u32), String> {
        use rune::render::{Gpu, OFFSCREEN_FORMAT, Offscreen, Renderer, write_png};
        let mut rt = rune::Runtime::new(
            (snap_gpu, images_gpu, logical_w, logical_h),
            |(sn, imgs, lw, lh): &(ReaderSnapshot, HashMap<NodeId, (PathBuf, rune::Image)>, f32, f32), _| {
                render_offscreen_scheme_root(sn, imgs, *lw, *lh)
            },
            (logical_w, logical_h),
            rune::Env::new(is_dark, dpi_scale),
        );
        rt.frame(0.0);
        let gpu = Gpu::headless();
        let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        renderer.prepare(
            &gpu.device,
            &gpu.queue,
            &mut rt.text.atlas,
            &rt.instances,
            (pw, ph),
        );
        renderer.upload_images(&gpu.device, &gpu.queue, &mut rt.images);
        let target = Offscreen::new(&gpu.device, pw, ph);
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        let clear = if is_dark {
            [0.06, 0.06, 0.07, 1.0]
        } else {
            [0.95, 0.95, 0.96, 1.0]
        };
        renderer.draw(&mut enc, &target.view, clear);
        target.copy(&mut enc);
        renderer.submit(&gpu.queue, enc);
        let rgba = target.read(&gpu.device);
        write_png(&out_gpu, pw, ph, &rgba).map_err(|e| e.to_string())?;
        Ok((pw, ph))
    }));

    match gpu_res {
        Ok(Ok(dims)) => Ok(dims),
        _ => render_scheme_cpu_fallback(snap, out_path, pw, ph, is_dark),
    }
}

fn render_scheme_cpu_fallback(
    snap: &ReaderSnapshot,
    out_path: &std::path::Path,
    pw: u32,
    ph: u32,
    is_dark: bool,
) -> Result<(u32, u32), String> {
    let w = pw as usize;
    let h = ph as usize;
    let mut buf = vec![0u8; w * h * 4];
    let bg = if is_dark { [18u8, 18, 20, 255] } else { [243u8, 243, 246, 255] };
    for px in buf.chunks_exact_mut(4) {
        px.copy_from_slice(&bg);
    }

    let put = |buf: &mut [u8], x: i32, y: i32, c: [u8; 4]| {
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            let idx = ((y as usize) * w + (x as usize)) * 4;
            buf[idx..idx + 4].copy_from_slice(&c);
        }
    };

    let fill_rect = |buf: &mut [u8], rx: i32, ry: i32, rw: i32, rh: i32, c: [u8; 4]| {
        let x0 = rx.max(0);
        let y0 = ry.max(0);
        let x1 = (rx + rw).min(w as i32);
        let y1 = (ry + rh).min(h as i32);
        for y in y0..y1 {
            for x in x0..x1 {
                put(buf, x, y, c);
            }
        }
    };

    let sec_w = snap.section_size.0.max(240.0);
    let sec_h = snap.section_size.1.max(120.0);
    let margin = 48.0f32;
    let sx = (pw as f32 - margin * 2.0) / sec_w;
    let sy = (ph as f32 - margin * 2.0 - 64.0) / sec_h;
    let s = sx.min(sy).max(0.5);
    let ox = margin;
    let oy = margin + 64.0;

    for child in &snap.section_children {
        let rx = (ox + child.rel_pos.0 * s) as i32;
        let ry = (oy + child.rel_pos.1 * s) as i32;
        let rw = (child.size.0 * s).max(20.0) as i32;
        let rh = (child.size.1 * s).max(20.0) as i32;
        let fill = if matches!(child.kind, NodeKind::Video | NodeKind::Audio) {
            if is_dark { [30, 31, 33, 255] } else { [220, 221, 224, 255] }
        } else if is_dark {
            [34, 34, 38, 255]
        } else {
            [255, 255, 255, 255]
        };
        fill_rect(&mut buf, rx, ry, rw, rh, fill);
        let hdr = if matches!(child.kind, NodeKind::Video | NodeKind::Audio) {
            if is_dark { [24, 25, 27, 255] } else { [208, 209, 212, 255] }
        } else if is_dark {
            [44, 45, 50, 255]
        } else {
            [236, 238, 242, 255]
        };
        fill_rect(&mut buf, rx, ry, rw, (28.0 * s).max(8.0) as i32, hdr);
    }

    rune::render::write_png(out_path, pw, ph, &buf).map_err(|e| e.to_string())?;
    Ok((pw, ph))
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
                        selected_date: None,
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
                        selected_date: None,
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

    #[test]
    fn test_popout_revamped_section_calendar_media_greyed_and_png_screenshot() {
        assert!(can_pop_out(NodeKind::Calendar));
        assert!(can_pop_out(NodeKind::Drawing));

        let dir = tempfile::TempDir::new().unwrap();
        let bridge = Arc::new(Mutex::new(ReaderBridge::default()));
        let sec_snap = {
            let mut b = bridge.lock().unwrap();
            let mut sec_meta = NodeMeta::default();
            sec_meta.section_framed = true;
            let mut cal_meta = NodeMeta::default();
            cal_meta.calendar_year = Some(2026);
            cal_meta.calendar_month = Some(5);
            let mut vid_meta = NodeMeta::default();
            vid_meta.media_url = Some("https://www.youtube.com/watch?v=abc".into());
            let mut draw_meta = NodeMeta::default();
            draw_meta.drawing_strokes.push(studynodes_graph::model::DrawingStroke {
                points: vec![[0.1, 0.2], [0.8, 0.7]],
                color_hex: "#8FA8C8".into(),
                width: 3.0,
            });
            b.snapshot = ReaderSnapshot {
                revision: 1,
                project_title: "Study".into(),
                node_id: Some(NodeId::from_str("sec-revamp")),
                node_kind: NodeKind::Section,
                title: "Physics Scheme".into(),
                meta: sec_meta,
                section_size: (760.0, 520.0),
                section_children: vec![
                    SectionChildSnapshot {
                        id: NodeId::from_str("cal-1"),
                        kind: NodeKind::Calendar,
                        title: "Schedule".into(),
                        content: String::new(),
                        rel_pos: (20.0, 50.0),
                        size: (250.0, 210.0),
                        meta: cal_meta,
                        flipped: false,
                        selected_date: Some("2026-05-12".into()),
                        asset_path: None,
                        image_handle: None,
                    },
                    SectionChildSnapshot {
                        id: NodeId::from_str("draw-1"),
                        kind: NodeKind::Drawing,
                        title: "Freebody Sketch".into(),
                        content: String::new(),
                        rel_pos: (290.0, 50.0),
                        size: (240.0, 200.0),
                        meta: draw_meta,
                        flipped: false,
                        selected_date: None,
                        asset_path: None,
                        image_handle: None,
                    },
                    SectionChildSnapshot {
                        id: NodeId::from_str("vid-1"),
                        kind: NodeKind::Video,
                        title: "Lecture Video".into(),
                        content: String::new(),
                        rel_pos: (20.0, 280.0),
                        size: (250.0, 180.0),
                        meta: vid_meta,
                        flipped: false,
                        selected_date: None,
                        asset_path: None,
                        image_handle: None,
                    },
                    SectionChildSnapshot {
                        id: NodeId::from_str("aud-1"),
                        kind: NodeKind::Audio,
                        title: "Podcast".into(),
                        content: String::new(),
                        rel_pos: (290.0, 280.0),
                        size: (240.0, 140.0),
                        meta: NodeMeta::default(),
                        flipped: false,
                        selected_date: None,
                        asset_path: None,
                        image_handle: None,
                    },
                ],
                read_only: true,
                ..Default::default()
            };
            b.snapshot.clone()
        };

        let mut rt = Runtime::new(
            PopoutReaderState::new(bridge.clone()),
            render_popout_reader,
            (980.0, 860.0),
            Env {
                system_dark: true,
                scale: 2.0,
            },
        );
        for _ in 0..8 {
            rt.frame(1.0 / 60.0);
        }

        let keys: Vec<_> = rt.elements().into_iter().filter_map(|e| e.key).collect();
        assert!(keys.contains(&"popout_sec_ai_review".to_string()));
        assert!(keys.contains(&"btn_popout_screenshot".to_string()));
        assert!(keys.contains(&"popout_sec_cal_cal-1".to_string()));
        assert!(keys.contains(&"popout_sec_drawing_draw-1".to_string()));
        assert!(keys.contains(&"popout_sec_media_greyed_vid-1".to_string()));
        assert!(keys.contains(&"popout_sec_media_greyed_aud-1".to_string()));

        // Trigger AI review button from Section pop-out
        let r = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("popout_sec_ai_review"))
            .unwrap()
            .rect;
        rt.pointer_move(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
        rt.pointer_down(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
        rt.pointer_up(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
        rt.frame(1.0 / 60.0);
        let cmds = std::mem::take(&mut bridge.lock().unwrap().commands);
        assert!(cmds
            .iter()
            .any(|c| matches!(c, ReaderCommand::TriggerSectionAiReview(id) if id.as_str() == "sec-revamp")));

        // Verify high-DPI 2x PNG screenshot export
        let png_path = dir.path().join("physics_scheme.png");
        let (pw, ph) = render_scheme_to_png(&sec_snap, &png_path).expect("render scheme to PNG");
        assert!(pw >= 1200 && ph >= 700, "expected 2x high-DPI PNG, got {}x{}", pw, ph);
        let bytes = std::fs::read(&png_path).unwrap();
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

        // Standalone Calendar & Drawing pop-outs render cleanly
        {
            let mut b = bridge.lock().unwrap();
            b.snapshot = ReaderSnapshot {
                revision: 2,
                project_title: "Study".into(),
                node_id: Some(NodeId::from_str("cal-solo")),
                node_kind: NodeKind::Calendar,
                title: "Exam Schedule".into(),
                ..Default::default()
            };
        }
        rt.state.sync_from_bridge();
        rt.invalidate();
        rt.frame(1.0 / 60.0);
        assert!(rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("popout_calendar_view")));

        {
            let mut b = bridge.lock().unwrap();
            b.snapshot = ReaderSnapshot {
                revision: 3,
                project_title: "Study".into(),
                node_id: Some(NodeId::from_str("draw-solo")),
                node_kind: NodeKind::Drawing,
                title: "Formula Sketch".into(),
                ..Default::default()
            };
        }
        rt.state.sync_from_bridge();
        rt.invalidate();
        rt.frame(1.0 / 60.0);
        assert!(rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("popout_drawing_view")));

        // Draw interactively inside the popped-out Drawing window
        let pad_rect = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("popout_drawing_canvas"))
            .expect("popout_drawing_canvas present")
            .rect;
        let x0 = pad_rect[0] + pad_rect[2] * 0.20;
        let y0 = pad_rect[1] + pad_rect[3] * 0.40;
        let x1 = pad_rect[0] + pad_rect[2] * 0.70;
        let y1 = pad_rect[1] + pad_rect[3] * 0.45;
        rt.pointer_move(x0, y0);
        rt.pointer_down(x0, y0);
        rt.pointer_move(x1, y1);
        rt.pointer_up(x1, y1);
        for _ in 0..4 {
            rt.frame(1.0 / 60.0);
        }

        assert_eq!(rt.state.snapshot.meta.drawing_strokes.len(), 1);
        assert!(rt.state.snapshot.meta.drawing_strokes[0].points.len() >= 2);
        let draw_cmds = std::mem::take(&mut bridge.lock().unwrap().commands);
        assert!(draw_cmds
            .iter()
            .any(|c| matches!(c, ReaderCommand::BeginDrawingStroke { node_id, .. } if node_id.as_str() == "draw-solo")));
        assert!(draw_cmds
            .iter()
            .any(|c| matches!(c, ReaderCommand::ExtendDrawingStroke { node_id, .. } if node_id.as_str() == "draw-solo")));
        assert!(draw_cmds
            .iter()
            .any(|c| matches!(c, ReaderCommand::FinishDrawingStroke(id) if id.as_str() == "draw-solo")));
    }
}
