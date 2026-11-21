use rune::{Color, TextStyle};
use rune_kit::MarkdownStyle;
use studynodes_graph::model::NodeKind;

/// Parse an sRGB `#rrggbb` / `#rrggbbaa` color into Rune's linear sRGB [`Color`].
#[inline]
pub fn hex(s: &str) -> Color {
    rune::color::hex(s).unwrap_or(Color::BLACK)
}

// Surfaces (Warm-charcoal matte monochrome — Claude Code-esque)
#[inline]
pub fn bg_canvas() -> Color {
    hex("#0E0E0D")
}

#[inline]
pub fn bg_chrome() -> Color {
    hex("#141413")
}

#[inline]
pub fn bg_sidebar() -> Color {
    hex("#151514")
}

#[inline]
pub fn bg_card() -> Color {
    hex("#1A1A18")
}

#[inline]
pub fn bg_card_header() -> Color {
    hex("#1F1F1D")
}

#[inline]
pub fn bg_card_selected() -> Color {
    hex("#222220")
}

#[inline]
pub fn bg_input() -> Color {
    hex("#111110")
}

#[inline]
pub fn bg_control() -> Color {
    hex("#232321")
}

#[inline]
pub fn hover_subtle() -> Color {
    Color::WHITE.alpha(0.05)
}

#[inline]
pub fn press_subtle() -> Color {
    Color::WHITE.alpha(0.09)
}

// Hairline borders
#[inline]
pub fn border_subtle() -> Color {
    Color::WHITE.alpha(0.07)
}

#[inline]
pub fn border_default() -> Color {
    Color::WHITE.alpha(0.13)
}

#[inline]
pub fn border_strong() -> Color {
    Color::WHITE.alpha(0.28)
}

// Typography colors
#[inline]
pub fn text_primary() -> Color {
    hex("#ECEBE6")
}

#[inline]
pub fn text_secondary() -> Color {
    hex("#A1A09A")
}

#[inline]
pub fn text_muted() -> Color {
    hex("#6E6D68")
}

#[inline]
pub fn text_inverted() -> Color {
    hex("#0E0E0D")
}

// Monochrome hierarchy + single restrained Claude Code terracotta accent
#[inline]
pub fn accent_claude() -> Color {
    hex("#D97757")
}

#[inline]
pub fn accent_primary() -> Color {
    hex("#ECEBE6")
}

#[allow(dead_code)]
#[inline]
pub fn accent_subtle() -> Color {
    hex("#B8B7B0")
}

#[inline]
pub fn accent_danger() -> Color {
    hex("#C97B72")
}

// Wires
#[inline]
pub fn wire_default() -> Color {
    hex("#3D3D39")
}

#[inline]
pub fn wire_selected() -> Color {
    hex("#ECEBE6")
}

#[inline]
pub fn wire_proposed() -> Color {
    hex("#D97757")
}

pub fn kind_accent(kind: NodeKind) -> Color {
    match kind {
        NodeKind::Note => hex("#D4D3CE"),
        NodeKind::Document => hex("#ECEBE6"),
        NodeKind::Image => hex("#B8B7B0"),
        NodeKind::LlmSynthesis => accent_claude(),
        NodeKind::Concept => hex("#9A9993"),
        NodeKind::Section => hex("#73726E"),
        NodeKind::Excerpt => accent_claude(),
        NodeKind::Flashcard => hex("#D4D3CE"),
        NodeKind::Formula => hex("#B8B7B0"),
        NodeKind::Checklist => hex("#9A9993"),
        NodeKind::Sequence => hex("#ECEBE6"),
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
    }
}

pub fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Note => "NOTE",
        NodeKind::Document => "DOC",
        NodeKind::Image => "IMAGE",
        NodeKind::LlmSynthesis => "AI SYNTH",
        NodeKind::Concept => "CONCEPT",
        NodeKind::Section => "SECTION",
        NodeKind::Excerpt => "EXCERPT",
        NodeKind::Flashcard => "CARD",
        NodeKind::Formula => "FORMULA",
        NodeKind::Checklist => "CHECKLIST",
        NodeKind::Sequence => "STEP",
    }
}

// Typography tokens (Claude Code terminal-editorial feel)
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

/// Rich Markdown rendering style tuned to the warm-charcoal monochrome Claude Code theme.
pub fn markdown_style() -> MarkdownStyle {
    MarkdownStyle::neutral(true)
        .text(text_primary())
        .muted(text_secondary())
        .link(accent_claude())
        .inline_code_bg(Color::WHITE.alpha(0.08))
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
        .list(24.0, 4.0)
}
