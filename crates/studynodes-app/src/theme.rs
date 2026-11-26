use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU8, Ordering};

use rune::{Color, El, Icon, SurfaceFit, TextStyle, col, el, text};
use rune_kit::MarkdownStyle;
use serde::{Deserialize, Serialize};
use studynodes_graph::model::NodeKind;

/// StudyNodes SVG logo: rounded node outline with an output socket and outgoing Bézier wire.
pub static STUDYNODES_LOGO_ICON: Icon = Icon {
    name: "studynodes-logo",
    view: 24.0,
    body: r#"<rect x="2.5" y="5" width="11.5" height="14" rx="2.5" fill="none" stroke="currentColor" stroke-width="1.75"/><line x1="5.2" y1="9.5" x2="11.3" y2="9.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><line x1="5.2" y1="13.2" x2="9.5" y2="13.2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><circle cx="14" cy="12" r="1.75" fill="currentColor"/><path d="M15.6 12C18.5 12 18.2 7.5 21.5 7.5" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round"/><circle cx="21.5" cy="7.5" r="1.5" fill="currentColor"/>"#,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ThemeId {
    #[default]
    WarmCharcoal,
    Dark,
    #[serde(alias = "NordicMist")]
    Light,
    #[serde(alias = "MidnightSlate")]
    TokyoNight,
    SolarizedDark,
    #[serde(alias = "WarmPaper")]
    SolarizedLight,
}

impl ThemeId {
    pub const fn all() -> [ThemeId; 6] {
        [
            ThemeId::WarmCharcoal,
            ThemeId::Dark,
            ThemeId::Light,
            ThemeId::TokyoNight,
            ThemeId::SolarizedDark,
            ThemeId::SolarizedLight,
        ]
    }

    pub const fn key_suffix(self) -> &'static str {
        match self {
            ThemeId::WarmCharcoal => "warm_charcoal",
            ThemeId::Dark => "dark",
            ThemeId::Light => "light",
            ThemeId::TokyoNight => "tokyo_night",
            ThemeId::SolarizedDark => "solarized_dark",
            ThemeId::SolarizedLight => "solarized_light",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            ThemeId::WarmCharcoal => "Warm Charcoal",
            ThemeId::Dark => "Dark",
            ThemeId::Light => "Light",
            ThemeId::TokyoNight => "Tokyo Night",
            ThemeId::SolarizedDark => "Solarized Dark",
            ThemeId::SolarizedLight => "Solarized Light",
        }
    }

    pub const fn is_dark(self) -> bool {
        matches!(
            self,
            ThemeId::WarmCharcoal
                | ThemeId::Dark
                | ThemeId::TokyoNight
                | ThemeId::SolarizedDark
        )
    }

    const fn as_u8(self) -> u8 {
        match self {
            ThemeId::WarmCharcoal => 0,
            ThemeId::Dark => 1,
            ThemeId::Light => 2,
            ThemeId::TokyoNight => 3,
            ThemeId::SolarizedDark => 4,
            ThemeId::SolarizedLight => 5,
        }
    }

    const fn from_u8(v: u8) -> Self {
        match v {
            1 => ThemeId::Dark,
            2 => ThemeId::Light,
            3 => ThemeId::TokyoNight,
            4 => ThemeId::SolarizedDark,
            5 => ThemeId::SolarizedLight,
            _ => ThemeId::WarmCharcoal,
        }
    }

    /// Swatch preview `(canvas_bg, card_bg, accent)`.
    pub fn preview_colors(self) -> (Color, Color, Color) {
        let p = palette_for(self);
        (hex(p.bg_canvas), hex(p.bg_card), hex(p.accent))
    }
}

static ACTIVE_THEME: AtomicU8 = AtomicU8::new(0);

#[inline]
pub fn active_theme() -> ThemeId {
    ThemeId::from_u8(ACTIVE_THEME.load(Ordering::Relaxed))
}

#[inline]
pub fn set_active_theme(id: ThemeId) {
    ACTIVE_THEME.store(id.as_u8(), Ordering::Relaxed);
}

struct ThemePalette {
    is_dark: bool,
    bg_canvas: &'static str,
    bg_chrome: &'static str,
    bg_sidebar: &'static str,
    bg_card: &'static str,
    bg_card_header: &'static str,
    bg_card_selected: &'static str,
    bg_input: &'static str,
    bg_control: &'static str,
    text_primary: &'static str,
    text_secondary: &'static str,
    text_muted: &'static str,
    text_inverted: &'static str,
    accent: &'static str,
    accent_primary: &'static str,
    accent_subtle: &'static str,
    accent_danger: &'static str,
    wire_default: &'static str,
    wire_selected: &'static str,
}

const WARM_CHARCOAL: ThemePalette = ThemePalette {
    is_dark: true,
    bg_canvas: "#0E0E0D",
    bg_chrome: "#141413",
    bg_sidebar: "#151514",
    bg_card: "#1A1A18",
    bg_card_header: "#1F1F1D",
    bg_card_selected: "#222220",
    bg_input: "#111110",
    bg_control: "#232321",
    text_primary: "#ECEBE6",
    text_secondary: "#A1A09A",
    text_muted: "#6E6D68",
    text_inverted: "#0E0E0D",
    accent: "#D97757",
    accent_primary: "#ECEBE6",
    accent_subtle: "#B8B7B0",
    accent_danger: "#C97B72",
    wire_default: "#3D3D39",
    wire_selected: "#ECEBE6",
};

const ANTIGRAVITY_DARK: ThemePalette = ThemePalette {
    is_dark: true,
    bg_canvas: "#111215",
    bg_chrome: "#16181D",
    bg_sidebar: "#15171C",
    bg_card: "#1C1F26",
    bg_card_header: "#22262F",
    bg_card_selected: "#272C36",
    bg_input: "#14161B",
    bg_control: "#252933",
    text_primary: "#E8EAED",
    text_secondary: "#9AA0A6",
    text_muted: "#6C727F",
    text_inverted: "#111215",
    accent: "#8AB4F8",
    accent_primary: "#E8EAED",
    accent_subtle: "#BDC1C6",
    accent_danger: "#F28B82",
    wire_default: "#3C4043",
    wire_selected: "#E8EAED",
};

const ANTIGRAVITY_LIGHT: ThemePalette = ThemePalette {
    is_dark: false,
    bg_canvas: "#F6F8FA",
    bg_chrome: "#FFFFFF",
    bg_sidebar: "#F8F9FA",
    bg_card: "#FFFFFF",
    bg_card_header: "#F1F3F4",
    bg_card_selected: "#E8F0FE",
    bg_input: "#F1F3F4",
    bg_control: "#E8EAED",
    text_primary: "#1F1F1F",
    text_secondary: "#5F6368",
    text_muted: "#80868B",
    text_inverted: "#FFFFFF",
    accent: "#1A73E8",
    accent_primary: "#1F1F1F",
    accent_subtle: "#3C4043",
    accent_danger: "#D93025",
    wire_default: "#DADCE0",
    wire_selected: "#1F1F1F",
};

const TOKYO_NIGHT: ThemePalette = ThemePalette {
    is_dark: true,
    bg_canvas: "#1A1B26",
    bg_chrome: "#16161E",
    bg_sidebar: "#16161E",
    bg_card: "#24283B",
    bg_card_header: "#1F2335",
    bg_card_selected: "#292E42",
    bg_input: "#16161E",
    bg_control: "#292E42",
    text_primary: "#C0CAF5",
    text_secondary: "#A9B1D6",
    text_muted: "#565F89",
    text_inverted: "#15161E",
    accent: "#7AA2F7",
    accent_primary: "#C0CAF5",
    accent_subtle: "#9AA5CE",
    accent_danger: "#F7768E",
    wire_default: "#414868",
    wire_selected: "#C0CAF5",
};

const SOLARIZED_DARK: ThemePalette = ThemePalette {
    is_dark: true,
    bg_canvas: "#002B36",
    bg_chrome: "#00212B",
    bg_sidebar: "#00242E",
    bg_card: "#073642",
    bg_card_header: "#0A3E4C",
    bg_card_selected: "#0E4B5B",
    bg_input: "#00212B",
    bg_control: "#0A3E4C",
    text_primary: "#93A1A1",
    text_secondary: "#839496",
    text_muted: "#586E75",
    text_inverted: "#002B36",
    accent: "#268BD2",
    accent_primary: "#EEE8D5",
    accent_subtle: "#93A1A1",
    accent_danger: "#DC322F",
    wire_default: "#2E545E",
    wire_selected: "#EEE8D5",
};

const SOLARIZED_LIGHT: ThemePalette = ThemePalette {
    is_dark: false,
    bg_canvas: "#FDF6E3",
    bg_chrome: "#F5EEDB",
    bg_sidebar: "#F7F0DD",
    bg_card: "#FFFDF5",
    bg_card_header: "#EEE8D5",
    bg_card_selected: "#E6DFC8",
    bg_input: "#EEE8D5",
    bg_control: "#E6DFC8",
    text_primary: "#073642",
    text_secondary: "#586E75",
    text_muted: "#839496",
    text_inverted: "#FDF6E3",
    accent: "#268BD2",
    accent_primary: "#073642",
    accent_subtle: "#586E75",
    accent_danger: "#DC322F",
    wire_default: "#C7C0AE",
    wire_selected: "#073642",
};

#[inline]
const fn palette_for(id: ThemeId) -> &'static ThemePalette {
    match id {
        ThemeId::WarmCharcoal => &WARM_CHARCOAL,
        ThemeId::Dark => &ANTIGRAVITY_DARK,
        ThemeId::Light => &ANTIGRAVITY_LIGHT,
        ThemeId::TokyoNight => &TOKYO_NIGHT,
        ThemeId::SolarizedDark => &SOLARIZED_DARK,
        ThemeId::SolarizedLight => &SOLARIZED_LIGHT,
    }
}

#[inline]
fn current_palette() -> &'static ThemePalette {
    palette_for(active_theme())
}

/// Parse an sRGB `#rrggbb` / `#rrggbbaa` color into Rune's linear sRGB [`Color`].
#[inline]
pub fn hex(s: &str) -> Color {
    rune::color::hex(s).unwrap_or(Color::BLACK)
}

#[inline]
pub fn bg_canvas() -> Color {
    hex(current_palette().bg_canvas)
}

#[inline]
pub fn bg_chrome() -> Color {
    hex(current_palette().bg_chrome)
}

#[allow(dead_code)]
#[inline]
pub fn bg_sidebar() -> Color {
    hex(current_palette().bg_sidebar)
}

#[inline]
pub fn bg_card() -> Color {
    hex(current_palette().bg_card)
}

#[inline]
pub fn bg_card_header() -> Color {
    hex(current_palette().bg_card_header)
}

#[inline]
pub fn bg_card_selected() -> Color {
    hex(current_palette().bg_card_selected)
}

#[inline]
pub fn bg_input() -> Color {
    hex(current_palette().bg_input)
}

#[inline]
pub fn bg_control() -> Color {
    hex(current_palette().bg_control)
}

#[inline]
pub fn hover_subtle() -> Color {
    if current_palette().is_dark {
        Color::WHITE.alpha(0.05)
    } else {
        Color::BLACK.alpha(0.045)
    }
}

#[inline]
pub fn press_subtle() -> Color {
    if current_palette().is_dark {
        Color::WHITE.alpha(0.09)
    } else {
        Color::BLACK.alpha(0.08)
    }
}

#[inline]
pub fn border_subtle() -> Color {
    if current_palette().is_dark {
        Color::WHITE.alpha(0.07)
    } else {
        Color::BLACK.alpha(0.08)
    }
}

#[inline]
pub fn border_default() -> Color {
    if current_palette().is_dark {
        Color::WHITE.alpha(0.13)
    } else {
        Color::BLACK.alpha(0.15)
    }
}

#[inline]
pub fn border_strong() -> Color {
    if current_palette().is_dark {
        Color::WHITE.alpha(0.28)
    } else {
        Color::BLACK.alpha(0.30)
    }
}

#[inline]
pub fn grid_minor() -> Color {
    if current_palette().is_dark {
        Color::WHITE.alpha(0.025)
    } else {
        Color::BLACK.alpha(0.04)
    }
}

#[inline]
pub fn grid_major() -> Color {
    if current_palette().is_dark {
        Color::WHITE.alpha(0.052)
    } else {
        Color::BLACK.alpha(0.085)
    }
}

#[inline]
pub fn text_primary() -> Color {
    hex(current_palette().text_primary)
}

#[inline]
pub fn text_secondary() -> Color {
    hex(current_palette().text_secondary)
}

#[inline]
pub fn text_muted() -> Color {
    hex(current_palette().text_muted)
}

#[inline]
pub fn text_inverted() -> Color {
    hex(current_palette().text_inverted)
}

#[inline]
pub fn accent_claude() -> Color {
    hex(current_palette().accent)
}

#[inline]
pub fn accent_primary() -> Color {
    hex(current_palette().accent_primary)
}

#[allow(dead_code)]
#[inline]
pub fn accent_subtle() -> Color {
    hex(current_palette().accent_subtle)
}

#[inline]
pub fn accent_danger() -> Color {
    hex(current_palette().accent_danger)
}

#[inline]
pub fn wire_default() -> Color {
    hex(current_palette().wire_default)
}

#[inline]
pub fn wire_selected() -> Color {
    hex(current_palette().wire_selected)
}

#[inline]
pub fn wire_proposed() -> Color {
    hex(current_palette().accent)
}

pub fn kind_accent(kind: NodeKind) -> Color {
    if current_palette().is_dark {
        match kind {
            NodeKind::Note => hex("#D4D3CE"),
            NodeKind::Document => text_primary(),
            NodeKind::Image => hex("#B8B7B0"),
            NodeKind::LlmSynthesis => accent_claude(),
            NodeKind::Concept => hex("#9A9993"),
            NodeKind::Section => hex("#73726E"),
            NodeKind::Excerpt => accent_claude(),
            NodeKind::Flashcard => hex("#D4D3CE"),
            NodeKind::Formula => hex("#B8B7B0"),
            NodeKind::GraphFormula => hex("#4DB6AC"),
            NodeKind::Checklist => hex("#9A9993"),
            NodeKind::Sequence => text_primary(),
            NodeKind::Video => hex("#C97B74"),
            NodeKind::Audio => hex("#7FA99B"),
            NodeKind::Calendar => hex("#C8A46D"),
            NodeKind::Drawing => hex("#8FA8C8"),
            NodeKind::Text => text_primary(),
            NodeKind::Quiz => hex("#D9A74A"),
        }
    } else {
        match kind {
            NodeKind::Note => hex("#3E3D39"),
            NodeKind::Document => text_primary(),
            NodeKind::Image => hex("#52504B"),
            NodeKind::LlmSynthesis => accent_claude(),
            NodeKind::Concept => hex("#68655F"),
            NodeKind::Section => hex("#858077"),
            NodeKind::Excerpt => accent_claude(),
            NodeKind::Flashcard => hex("#3E3D39"),
            NodeKind::Formula => hex("#52504B"),
            NodeKind::GraphFormula => hex("#0F766E"),
            NodeKind::Checklist => hex("#68655F"),
            NodeKind::Sequence => text_primary(),
            NodeKind::Video => hex("#A4524B"),
            NodeKind::Audio => hex("#3E7364"),
            NodeKind::Calendar => hex("#9A753B"),
            NodeKind::Drawing => hex("#4F6D94"),
            NodeKind::Text => text_primary(),
            NodeKind::Quiz => hex("#9A753B"),
        }
    }
}

/// Resolve user-selected font family for frameless `Text` canvas nodes.
pub fn text_node_family(font: Option<&str>) -> rune::Family {
    match font.map(str::trim).unwrap_or("sans") {
        "serif" => rune::Family::Named("Georgia, Cambria, Times New Roman, serif"),
        "mono" => rune::Family::Mono,
        "display" => rune::Family::Named("Segoe UI Variable Display, Trebuchet MS, sans-serif"),
        "hand" => rune::Family::Named("Segoe Print, Bradley Hand, Comic Sans MS, cursive"),
        _ => rune::Family::Sans,
    }
}

pub fn kind_label(kind: NodeKind) -> &'static str {
    crate::i18n::kind_label(kind)
}

// Typography tokens (Claude Code terminal-editorial feel)
#[allow(dead_code)]
pub const TITLE_BAR: TextStyle = TextStyle::new(12.5, 600.0).mono();
pub const PANEL_TITLE: TextStyle = TextStyle::new(13.5, 600.0);
pub const SECTION_LABEL: TextStyle = TextStyle::new(10.0, 600.0).mono().tracking(0.6);
pub const BODY_TEXT: TextStyle = TextStyle::new(13.0, 400.0).lh(19.5);
#[allow(dead_code)]
pub const READER_BODY: TextStyle = TextStyle::new(13.0, 400.0).lh(21.0);
pub const MONO_BODY: TextStyle = TextStyle::new(12.0, 400.0).mono().lh(18.5);
pub const BADGE_TEXT: TextStyle = TextStyle::new(10.0, 500.0).mono();
pub const STATUS_TEXT: TextStyle = TextStyle::new(11.0, 400.0).mono();
pub const CONTROL_TEXT: TextStyle = TextStyle::new(11.5, 500.0).mono();

/// Stable 64-bit hash helper used to key memoized Markdown renders.
pub(crate) fn hash_str(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Rich Markdown rendering style scaled by `scale` for inspector panels and canvas cards.
pub fn markdown_style_scaled(scale: f32) -> MarkdownStyle {
    let is_dark = active_theme().is_dark();
    let sc = scale.clamp(0.35, 4.5);
    MarkdownStyle::neutral(is_dark)
        .text(text_primary())
        .muted(text_secondary())
        .link(accent_claude())
        .inline_code_bg(if is_dark {
            Color::WHITE.alpha(0.08)
        } else {
            Color::BLACK.alpha(0.06)
        })
        .code_block(bg_input(), border_default())
        .quote(accent_claude().alpha(0.55), 2.0, 12.0 * sc, text_secondary())
        .rule(border_default())
        .selection(accent_claude().alpha(0.28))
        .diagram_surface(bg_card(), border_default(), bg_input())
        .body_font(TextStyle::new(13.0 * sc, 400.0).lh(20.5 * sc))
        .heading_font(1, TextStyle::new(17.0 * sc, 600.0).lh(24.0 * sc))
        .heading_font(2, TextStyle::new(14.5 * sc, 600.0).lh(21.0 * sc))
        .heading_font(3, TextStyle::new(13.0 * sc, 600.0).lh(19.0 * sc))
        .code_block_font(TextStyle::new(11.5 * sc, 400.0).mono().lh(17.5 * sc))
        .strong_weight(700.0)
        .list(24.0 * sc, 4.0 * sc)
}

/// Rich Markdown rendering style tuned to the currently active theme.
pub fn markdown_style() -> MarkdownStyle {
    markdown_style_scaled(1.0)
}

/// Compact zoom-scaled Markdown style for canvas `Note` cards wrapped to `wrap_w`.
pub fn card_markdown_style(zoom: f32, wrap_w: f32) -> MarkdownStyle {
    markdown_style_scaled((zoom * 0.92).clamp(0.40, 4.0)).text_width(wrap_w.max(60.0))
}

/// Compact Markdown style tuned for Project AI Chat messages with highlighted `**bold**` spans.
pub fn chat_markdown_style() -> MarkdownStyle {
    let is_dark = active_theme().is_dark();
    markdown_style()
        .body_font(TextStyle::new(12.5, 400.0).lh(19.0))
        .heading_font(1, TextStyle::new(15.0, 650.0).lh(21.0))
        .heading_font(2, TextStyle::new(13.5, 650.0).lh(19.5))
        .heading_font(3, TextStyle::new(12.5, 650.0).lh(18.5))
        .code_block_font(TextStyle::new(11.2, 400.0).mono().lh(16.8))
        .strong_weight(700.0)
        .highlight_bg(if is_dark {
            accent_claude().alpha(0.18)
        } else {
            accent_claude().alpha(0.14)
        })
        .highlight_fg(Some(text_primary()))
        .list(20.0, 3.0)
}

/// Compute aspect-ratio-preserving height constrained within `[min_h, max_h]`.
pub(crate) fn scaled_image_height(
    img: &rune::Image,
    avail_w: f32,
    min_h: f32,
    max_h: f32,
    fallback_h: f32,
) -> f32 {
    if let Some([iw, ih]) = img.size() {
        if iw > 0 && ih > 0 {
            return (avail_w * (ih as f32 / iw as f32)).clamp(min_h, max_h);
        }
    }
    fallback_h.clamp(min_h, max_h)
}

/// Shared fitted-image container supporting decoded pixels, async loading, and decode errors.
pub(crate) fn render_fitted_image_box(
    key: impl Into<String>,
    img: &rune::Image,
    height: f32,
    radius: f32,
) -> El {
    let key = key.into();
    if let Some(err) = img.error() {
        col()
            .key(key)
            .w_full()
            .h(height.max(64.0))
            .padding(8.0)
            .radius(radius)
            .fill(bg_input())
            .border(1.0, accent_danger().alpha(0.45))
            .center()
            .child(
                text(format!("Image error: {}", err))
                    .font(STATUS_TEXT)
                    .color(accent_danger())
                    .wrap(),
            )
    } else if img.is_ready() {
        el()
            .key(key)
            .w_full()
            .h(height)
            .radius(radius)
            .fill(bg_input())
            .border(1.0, border_subtle())
            .pixels_fit(img.pixels(), SurfaceFit::Fit)
    } else {
        el()
            .key(key)
            .w_full()
            .h(height)
            .radius(radius)
            .fill(bg_input())
            .border(1.0, border_subtle())
            .center()
            .child(
                text("Loading image...")
                    .font(STATUS_TEXT)
                    .color(text_muted()),
            )
    }
}
