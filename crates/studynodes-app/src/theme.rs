use std::sync::atomic::{AtomicU8, Ordering};

use rune::{Color, Icon, TextStyle};
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
    MidnightSlate,
    WarmPaper,
    NordicMist,
}

impl ThemeId {
    pub const fn all() -> [ThemeId; 4] {
        [
            ThemeId::WarmCharcoal,
            ThemeId::MidnightSlate,
            ThemeId::WarmPaper,
            ThemeId::NordicMist,
        ]
    }

    pub const fn key_suffix(self) -> &'static str {
        match self {
            ThemeId::WarmCharcoal => "warm_charcoal",
            ThemeId::MidnightSlate => "midnight_slate",
            ThemeId::WarmPaper => "warm_paper",
            ThemeId::NordicMist => "nordic_mist",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            ThemeId::WarmCharcoal => "Warm Charcoal",
            ThemeId::MidnightSlate => "Midnight Slate",
            ThemeId::WarmPaper => "Warm Paper",
            ThemeId::NordicMist => "Nordic Mist",
        }
    }

    pub const fn is_dark(self) -> bool {
        matches!(self, ThemeId::WarmCharcoal | ThemeId::MidnightSlate)
    }

    const fn as_u8(self) -> u8 {
        match self {
            ThemeId::WarmCharcoal => 0,
            ThemeId::MidnightSlate => 1,
            ThemeId::WarmPaper => 2,
            ThemeId::NordicMist => 3,
        }
    }

    const fn from_u8(v: u8) -> Self {
        match v {
            1 => ThemeId::MidnightSlate,
            2 => ThemeId::WarmPaper,
            3 => ThemeId::NordicMist,
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

const MIDNIGHT_SLATE: ThemePalette = ThemePalette {
    is_dark: true,
    bg_canvas: "#0B0F17",
    bg_chrome: "#101622",
    bg_sidebar: "#111825",
    bg_card: "#161E2E",
    bg_card_header: "#1B2538",
    bg_card_selected: "#202C42",
    bg_input: "#0D121C",
    bg_control: "#1F293D",
    text_primary: "#E6EDF7",
    text_secondary: "#9BA8BF",
    text_muted: "#64738C",
    text_inverted: "#0B0F17",
    accent: "#60A5FA",
    accent_primary: "#E6EDF7",
    accent_subtle: "#B4C2D9",
    accent_danger: "#F87171",
    wire_default: "#34425E",
    wire_selected: "#E6EDF7",
};

const WARM_PAPER: ThemePalette = ThemePalette {
    is_dark: false,
    bg_canvas: "#F4F1EA",
    bg_chrome: "#FAF8F4",
    bg_sidebar: "#F6F3EC",
    bg_card: "#FFFFFF",
    bg_card_header: "#F7F4ED",
    bg_card_selected: "#EFEAE0",
    bg_input: "#EFECE4",
    bg_control: "#E9E4D9",
    text_primary: "#1C1B18",
    text_secondary: "#57544D",
    text_muted: "#858077",
    text_inverted: "#FAF8F4",
    accent: "#C96442",
    accent_primary: "#1C1B18",
    accent_subtle: "#4B4842",
    accent_danger: "#B94A40",
    wire_default: "#C8C2B6",
    wire_selected: "#1C1B18",
};

const NORDIC_MIST: ThemePalette = ThemePalette {
    is_dark: false,
    bg_canvas: "#EEF2F6",
    bg_chrome: "#F8FAFC",
    bg_sidebar: "#F1F5F9",
    bg_card: "#FFFFFF",
    bg_card_header: "#F4F7FA",
    bg_card_selected: "#E6EDF5",
    bg_input: "#E9EFF5",
    bg_control: "#E2E8F0",
    text_primary: "#0F172A",
    text_secondary: "#475569",
    text_muted: "#7B899D",
    text_inverted: "#F8FAFC",
    accent: "#2563EB",
    accent_primary: "#0F172A",
    accent_subtle: "#334155",
    accent_danger: "#DC2626",
    wire_default: "#CBD5E1",
    wire_selected: "#0F172A",
};

#[inline]
const fn palette_for(id: ThemeId) -> &'static ThemePalette {
    match id {
        ThemeId::WarmCharcoal => &WARM_CHARCOAL,
        ThemeId::MidnightSlate => &MIDNIGHT_SLATE,
        ThemeId::WarmPaper => &WARM_PAPER,
        ThemeId::NordicMist => &NORDIC_MIST,
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
            NodeKind::Checklist => hex("#9A9993"),
            NodeKind::Sequence => text_primary(),
            NodeKind::Video => hex("#C97B74"),
            NodeKind::Audio => hex("#7FA99B"),
            NodeKind::Calendar => hex("#C8A46D"),
            NodeKind::Drawing => hex("#8FA8C8"),
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
            NodeKind::Checklist => hex("#68655F"),
            NodeKind::Sequence => text_primary(),
            NodeKind::Video => hex("#A4524B"),
            NodeKind::Audio => hex("#3E7364"),
            NodeKind::Calendar => hex("#9A753B"),
            NodeKind::Drawing => hex("#4F6D94"),
        }
    }
}

pub fn kind_glyph(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Note => "▪",
        NodeKind::Document => "▤",
        NodeKind::Image => "▣",
        NodeKind::LlmSynthesis => "✦",
        NodeKind::Concept => "○",
        NodeKind::Section => "▭",
        NodeKind::Excerpt => "❝",
        NodeKind::Flashcard => "◇",
        NodeKind::Formula => "∑",
        NodeKind::Checklist => "☑",
        NodeKind::Sequence => "→",
        NodeKind::Video => "▶",
        NodeKind::Audio => "♫",
        NodeKind::Calendar => "▦",
        NodeKind::Drawing => "✎",
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

/// Rich Markdown rendering style tuned to the currently active theme.
pub fn markdown_style() -> MarkdownStyle {
    let is_dark = active_theme().is_dark();
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
        .quote(accent_claude().alpha(0.55), 2.0, 12.0, text_secondary())
        .rule(border_default())
        .selection(accent_claude().alpha(0.28))
        .diagram_surface(bg_card(), border_default(), bg_input())
        .body_font(TextStyle::new(13.0, 400.0).lh(20.5))
        .heading_font(1, TextStyle::new(17.0, 600.0).lh(24.0))
        .heading_font(2, TextStyle::new(14.5, 600.0).lh(21.0))
        .heading_font(3, TextStyle::new(13.0, 600.0).lh(19.0))
        .code_block_font(TextStyle::new(11.5, 400.0).mono().lh(17.5))
        .strong_weight(700.0)
        .list(24.0, 4.0)
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
