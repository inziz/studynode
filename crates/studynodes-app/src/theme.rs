use rune::{Color, TextStyle};
use studynodes_graph::model::NodeKind;

/// Parse an sRGB `#rrggbb` / `#rrggbbaa` color into Rune's linear sRGB [`Color`].
#[inline]
pub fn hex(s: &str) -> Color {
    rune::color::hex(s).unwrap_or(Color::BLACK)
}

// Surfaces (Antigravity obsidian dark palette)
#[inline]
pub fn bg_canvas() -> Color {
    hex("#0B0C0F")
}

#[inline]
pub fn bg_chrome() -> Color {
    hex("#0F1115")
}

#[inline]
pub fn bg_sidebar() -> Color {
    hex("#111318")
}

#[inline]
pub fn bg_card() -> Color {
    hex("#151820")
}

#[inline]
pub fn bg_card_header() -> Color {
    hex("#191D27")
}

#[inline]
pub fn bg_card_selected() -> Color {
    hex("#181C26")
}

#[inline]
pub fn bg_input() -> Color {
    hex("#0D0F14")
}

#[inline]
pub fn bg_control() -> Color {
    hex("#1A1E28")
}

#[inline]
pub fn hover_subtle() -> Color {
    Color::WHITE.alpha(0.045)
}

#[inline]
pub fn press_subtle() -> Color {
    Color::WHITE.alpha(0.08)
}

// Hairline borders
#[inline]
pub fn border_subtle() -> Color {
    Color::WHITE.alpha(0.06)
}

#[inline]
pub fn border_default() -> Color {
    Color::WHITE.alpha(0.10)
}

#[allow(dead_code)]
#[inline]
pub fn border_strong() -> Color {
    Color::WHITE.alpha(0.18)
}

// Typography colors
#[inline]
pub fn text_primary() -> Color {
    hex("#F2F4F8")
}

#[inline]
pub fn text_secondary() -> Color {
    hex("#98A2B3")
}

#[inline]
pub fn text_muted() -> Color {
    hex("#5E6778")
}

// Luminous, restrained accents
#[inline]
pub fn accent_blue() -> Color {
    hex("#4F8BFF")
}

#[inline]
pub fn accent_amber() -> Color {
    hex("#F5A623")
}

#[inline]
pub fn accent_violet() -> Color {
    hex("#A77BFF")
}

#[inline]
pub fn accent_emerald() -> Color {
    hex("#34D399")
}

#[inline]
pub fn accent_rose() -> Color {
    hex("#F87171")
}

// Wires
#[inline]
pub fn wire_default() -> Color {
    hex("#3B4254")
}

pub fn kind_accent(kind: NodeKind) -> Color {
    match kind {
        NodeKind::Note => accent_amber(),
        NodeKind::Document => accent_blue(),
        NodeKind::Image => accent_emerald(),
        NodeKind::LlmSynthesis => accent_violet(),
        NodeKind::Concept => accent_rose(),
    }
}

#[allow(dead_code)]
pub fn kind_icon(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Note => "✎",
        NodeKind::Document => "📄",
        NodeKind::Image => "🖼",
        NodeKind::LlmSynthesis => "✦",
        NodeKind::Concept => "◆",
    }
}

pub fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Note => "NOTE",
        NodeKind::Document => "DOC",
        NodeKind::Image => "IMAGE",
        NodeKind::LlmSynthesis => "AI SYNTH",
        NodeKind::Concept => "CONCEPT",
    }
}

// Shared typography tokens
pub const TITLE_BAR: TextStyle = TextStyle::new(13.0, 600.0);
pub const PANEL_TITLE: TextStyle = TextStyle::new(14.0, 600.0);
pub const SECTION_LABEL: TextStyle = TextStyle::new(10.5, 700.0).tracking(0.5);
#[allow(dead_code)]
pub const CARD_TITLE: TextStyle = TextStyle::new(13.0, 600.0);
#[allow(dead_code)]
pub const CARD_BODY: TextStyle = TextStyle::new(12.5, 400.0).lh(18.0);
pub const BODY_TEXT: TextStyle = TextStyle::new(13.0, 400.0).lh(19.0);
pub const READER_BODY: TextStyle = TextStyle::new(13.5, 400.0).lh(22.0);
pub const BADGE_TEXT: TextStyle = TextStyle::new(10.5, 600.0);
pub const STATUS_TEXT: TextStyle = TextStyle::new(11.5, 500.0);
pub const CONTROL_TEXT: TextStyle = TextStyle::new(12.0, 500.0);
