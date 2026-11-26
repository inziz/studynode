/// Compact row shown inline directly inside active `Formula` cards on the canvas.
#[allow(dead_code)]
pub const QUICK_CARD_SYMBOLS: &[(&str, &str)] = &[
    ("∑", "∑"),
    ("∫", "∫"),
    ("√", "√"),
    ("Δ", "Δ"),
    ("π", "π"),
    ("θ", "θ"),
    ("±", "±"),
    ("→", "→"),
    ("≤", "≤"),
    ("≥", "≥"),
    ("²", "²"),
    ("ᵢ", "ᵢ"),
    ("≈", "≈"),
    ("∞", "∞"),
];

#[allow(dead_code)]
pub const GREEK_SYMBOLS: &[&str] = &[
    "α", "β", "γ", "δ", "Δ", "ε", "η", "θ", "λ", "μ", "π", "ρ", "σ", "Σ", "τ", "φ", "ψ", "ω", "Ω",
];

pub const OP_SYMBOLS: &[&str] = &[
    "∑", "∫", "∏", "√", "∂", "∇", "±", "×", "·", "÷", "≈", "≠", "≤", "≥", "→", "⇔", "∈", "∀",
    "∃", "∝", "∞",
];

#[allow(dead_code)]
pub const SCRIPT_SYMBOLS: &[&str] = &[
    "⁻¹", "²", "³", "ⁿ", "₀", "₁", "₂", "ᵢ", "ⱼ", "ₖ", "ₙ", "ₓ",
];

pub const FORMULA_TEMPLATES: &[(&str, &str)] = &[
    ("Sum", "∑ᵢ₌₁ⁿ xᵢ"),
    ("Integral", "∫ₐᵇ f(x) dx"),
    ("Deriv", "d/dx(□)"),
    ("Fraction", "(a + b) / c"),
    ("Hebbian", "Δwᵢⱼ = η · xᵢ · yⱼ"),
    ("Quadratic", "x = (-b ± √(b² - 4ac)) / (2a)"),
];

pub fn is_code_language(lang: Option<&str>) -> bool {
    lang.is_some_and(|l| l.eq_ignore_ascii_case("RUST") || l.eq_ignore_ascii_case("PY"))
}

// Multi-page formula keyboard model

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FormulaKeypadPage {
    #[default]
    Main,
    Func,
    Alpha,
    Greek,
}

impl FormulaKeypadPage {
    pub const fn tag(self) -> &'static str {
        match self {
            FormulaKeypadPage::Main => "main",
            FormulaKeypadPage::Func => "func",
            FormulaKeypadPage::Alpha => "alpha",
            FormulaKeypadPage::Greek => "greek",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormulaKeyZone {
    Side,
    Center,
    Op,
    PageSwitch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormulaKeyAction {
    InsertTemplate(&'static str),
    SmartSquare,
    SmartPower,
    SmartSubscript,
    Newline,
    MoveLeft,
    MoveRight,
    Backspace,
    Clear,
    SetPage(FormulaKeypadPage),
    ToggleRawInput,
    FormatShorthand,
}

#[derive(Clone, Copy, Debug)]
pub struct FormulaKey {
    pub id: &'static str,
    pub label: &'static str,
    pub math_keycap: Option<&'static str>,
    pub zone: FormulaKeyZone,
    pub action: FormulaKeyAction,
}

const fn fk(
    id: &'static str,
    label: &'static str,
    zone: FormulaKeyZone,
    action: FormulaKeyAction,
) -> FormulaKey {
    FormulaKey {
        id,
        label,
        math_keycap: None,
        zone,
        action,
    }
}

const fn fkm(
    id: &'static str,
    label: &'static str,
    math_keycap: &'static str,
    zone: FormulaKeyZone,
    action: FormulaKeyAction,
) -> FormulaKey {
    FormulaKey {
        id,
        label,
        math_keycap: Some(math_keycap),
        zone,
        action,
    }
}

pub const FORMULA_MAIN_ROWS: &[&[FormulaKey]] = &[
    &[
        fk("lp", "(", FormulaKeyZone::Side, FormulaKeyAction::InsertTemplate("(")),
        fk("rp", ")", FormulaKeyZone::Side, FormulaKeyAction::InsertTemplate(")")),
        fk("7", "7", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("7")),
        fk("8", "8", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("8")),
        fk("9", "9", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("9")),
        fk("add", "+", FormulaKeyZone::Op, FormulaKeyAction::InsertTemplate(" + ")),
    ],
    &[
        fkm(
            "frac",
            "□/□",
            "(□)/(□)",
            FormulaKeyZone::Side,
            FormulaKeyAction::InsertTemplate("(□)/(□)"),
        ),
        fkm(
            "sqrt",
            "√□",
            "√(□)",
            FormulaKeyZone::Side,
            FormulaKeyAction::InsertTemplate("√(□)"),
        ),
        fk("4", "4", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("4")),
        fk("5", "5", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("5")),
        fk("6", "6", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("6")),
        fk("sub", "−", FormulaKeyZone::Op, FormulaKeyAction::InsertTemplate(" - ")),
    ],
    &[
        fkm("sq", "□²", "□²", FormulaKeyZone::Side, FormulaKeyAction::SmartSquare),
        fk("x", "x", FormulaKeyZone::Side, FormulaKeyAction::InsertTemplate("x")),
        fk("1", "1", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("1")),
        fk("2", "2", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("2")),
        fk("3", "3", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("3")),
        fk("mul", "×", FormulaKeyZone::Op, FormulaKeyAction::InsertTemplate(" · ")),
    ],
    &[
        fk(
            "func",
            "...",
            FormulaKeyZone::PageSwitch,
            FormulaKeyAction::SetPage(FormulaKeypadPage::Func),
        ),
        fk(
            "abc",
            "ABC",
            FormulaKeyZone::PageSwitch,
            FormulaKeyAction::SetPage(FormulaKeypadPage::Alpha),
        ),
        fk("0", "0", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("0")),
        fk("dot", ".", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate(".")),
        fk("eq", "=", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate(" = ")),
        fk("div", "÷", FormulaKeyZone::Op, FormulaKeyAction::InsertTemplate(" / ")),
    ],
];

pub const FORMULA_FUNC_ROWS: &[&[FormulaKey]] = &[
    &[
        fk("lt", "<", FormulaKeyZone::Side, FormulaKeyAction::InsertTemplate(" < ")),
        fk("gt", ">", FormulaKeyZone::Side, FormulaKeyAction::InsertTemplate(" > ")),
        fk("pi", "π", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("π")),
        fk("inf", "∞", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("∞")),
        fk("deg", "°", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("°")),
        fk("sin", "sin", FormulaKeyZone::Op, FormulaKeyAction::InsertTemplate("sin(□)")),
    ],
    &[
        fkm(
            "int",
            "∫□dx",
            "∫(□)dx",
            FormulaKeyZone::Side,
            FormulaKeyAction::InsertTemplate("∫(□)dx"),
        ),
        fk("fact", "!", FormulaKeyZone::Side, FormulaKeyAction::InsertTemplate("!")),
        fk("e", "e", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("e")),
        fk("ln", "ln", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("ln(□)")),
        fkm(
            "log10",
            "log₁₀",
            "log₁₀",
            FormulaKeyZone::Center,
            FormulaKeyAction::InsertTemplate("log₁₀(□)"),
        ),
        fk("cos", "cos", FormulaKeyZone::Op, FormulaKeyAction::InsertTemplate("cos(□)")),
    ],
    &[
        fkm(
            "deriv",
            "d/dx □",
            "d/dx(□)",
            FormulaKeyZone::Side,
            FormulaKeyAction::InsertTemplate("d/dx(□)"),
        ),
        fkm(
            "abs",
            "|□|",
            "|□|",
            FormulaKeyZone::Side,
            FormulaKeyAction::InsertTemplate("|□|"),
        ),
        fk("exp", "exp", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("exp(□)")),
        fk("sign", "sign", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("sign(□)")),
        fkm(
            "nroot",
            "ⁿ√□",
            "root(n, □)",
            FormulaKeyZone::Center,
            FormulaKeyAction::InsertTemplate("root(□, □)"),
        ),
        fk("tan", "tan", FormulaKeyZone::Op, FormulaKeyAction::InsertTemplate("tan(□)")),
    ],
    &[
        fk(
            "123",
            "123",
            FormulaKeyZone::PageSwitch,
            FormulaKeyAction::SetPage(FormulaKeypadPage::Main),
        ),
        fkm("pow", "□^□", "□^(□)", FormulaKeyZone::Side, FormulaKeyAction::SmartPower),
        fk("sum", "∑", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("∑(□, □, □)")),
        fk("le", "≤", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate(" ≤ ")),
        fk("ge", "≥", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate(" ≥ ")),
        fk("cot", "cot", FormulaKeyZone::Op, FormulaKeyAction::InsertTemplate("cot(□)")),
    ],
];

pub const FORMULA_ALPHA_ROWS: &[&[FormulaKey]] = &[
    &[
        fk("a", "a", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("a")),
        fk("b", "b", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("b")),
        fk("c", "c", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("c")),
        fk("d", "d", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("d")),
        fk("e", "e", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("e")),
        fk("f", "f", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("f")),
    ],
    &[
        fk("g", "g", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("g")),
        fk("h", "h", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("h")),
        fk("i", "i", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("i")),
        fk("j", "j", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("j")),
        fk("k", "k", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("k")),
        fk("l", "l", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("l")),
    ],
    &[
        fk("m", "m", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("m")),
        fk("n", "n", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("n")),
        fk("o", "o", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("o")),
        fk("p", "p", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("p")),
        fk("q", "q", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("q")),
        fk("r", "r", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("r")),
    ],
    &[
        fk("s", "s", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("s")),
        fk("t", "t", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("t")),
        fk("u", "u", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("u")),
        fk("v", "v", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("v")),
        fk("w", "w", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("w")),
        fk("x", "x", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("x")),
    ],
    &[
        fk(
            "123",
            "123",
            FormulaKeyZone::PageSwitch,
            FormulaKeyAction::SetPage(FormulaKeypadPage::Main),
        ),
        fk(
            "func",
            "...",
            FormulaKeyZone::PageSwitch,
            FormulaKeyAction::SetPage(FormulaKeypadPage::Func),
        ),
        fk(
            "greek",
            "αβγ",
            FormulaKeyZone::PageSwitch,
            FormulaKeyAction::SetPage(FormulaKeypadPage::Greek),
        ),
        fk("y", "y", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("y")),
        fk("z", "z", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("z")),
        fkm("sub", "□_□", "□_(□)", FormulaKeyZone::Side, FormulaKeyAction::SmartSubscript),
    ],
];

pub const FORMULA_GREEK_ROWS: &[&[FormulaKey]] = &[
    &[
        fk("alpha", "α", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("α")),
        fk("beta", "β", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("β")),
        fk("gamma", "γ", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("γ")),
        fk("delta", "δ", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("δ")),
        fk("cap_delta", "Δ", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("Δ")),
        fk("eps", "ε", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("ε")),
    ],
    &[
        fk("eta", "η", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("η")),
        fk("theta", "θ", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("θ")),
        fk("lambda", "λ", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("λ")),
        fk("mu", "μ", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("μ")),
        fk("rho", "ρ", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("ρ")),
        fk("sigma", "σ", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("σ")),
    ],
    &[
        fk("cap_sigma", "Σ", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("Σ")),
        fk("phi", "φ", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("φ")),
        fk("psi", "ψ", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("ψ")),
        fk("omega", "ω", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("ω")),
        fk("cap_omega", "Ω", FormulaKeyZone::Center, FormulaKeyAction::InsertTemplate("Ω")),
        fk("pm", "±", FormulaKeyZone::Op, FormulaKeyAction::InsertTemplate(" ± ")),
    ],
    &[
        fk(
            "123",
            "123",
            FormulaKeyZone::PageSwitch,
            FormulaKeyAction::SetPage(FormulaKeypadPage::Main),
        ),
        fk(
            "abc",
            "ABC",
            FormulaKeyZone::PageSwitch,
            FormulaKeyAction::SetPage(FormulaKeypadPage::Alpha),
        ),
        fk("neq", "≠", FormulaKeyZone::Op, FormulaKeyAction::InsertTemplate(" ≠ ")),
        fk("approx", "≈", FormulaKeyZone::Op, FormulaKeyAction::InsertTemplate(" ≈ ")),
        fk("to", "→", FormulaKeyZone::Op, FormulaKeyAction::InsertTemplate(" → ")),
        fk("iff", "⇔", FormulaKeyZone::Op, FormulaKeyAction::InsertTemplate(" ⇔ ")),
    ],
];

pub const fn keypad_rows(page: FormulaKeypadPage) -> &'static [&'static [FormulaKey]] {
    match page {
        FormulaKeypadPage::Main => FORMULA_MAIN_ROWS,
        FormulaKeypadPage::Func => FORMULA_FUNC_ROWS,
        FormulaKeypadPage::Alpha => FORMULA_ALPHA_ROWS,
        FormulaKeypadPage::Greek => FORMULA_GREEK_ROWS,
    }
}

fn is_preceded_by_operand(edit: &rune::TextEdit) -> bool {
    let (a, b) = edit.range();
    if a != b {
        return false;
    }
    if edit.text[b..].starts_with('□') || edit.text[..a].ends_with('□') {
        return false;
    }
    let Some(prev) = edit.text[..a].chars().last() else {
        return false;
    };
    prev.is_alphanumeric()
        || matches!(
            prev,
            ')' | ']'
                | '|'
                | 'π'
                | '¹'
                | '²'
                | '³'
                | '⁰'..='⁹'
                | 'ⁿ'
                | 'ˣ'
                | 'ᵢ'
                | 'ⱼ'
                | 'ₖ'
                | 'ₙ'
                | 'ₓ'
                | '₀'..='₉'
        )
}

pub fn apply_formula_key_to_edit(edit: &mut rune::TextEdit, action: FormulaKeyAction) {
    use crate::graph_formula::{apply_keypad_to_edit, KeypadAction};

    match action {
        FormulaKeyAction::InsertTemplate(tpl) => {
            apply_keypad_to_edit(edit, KeypadAction::InsertTemplate(tpl));
        }
        FormulaKeyAction::SmartSquare => {
            if is_preceded_by_operand(edit) {
                apply_keypad_to_edit(edit, KeypadAction::InsertTemplate("²"));
            } else {
                apply_keypad_to_edit(edit, KeypadAction::InsertTemplate("□²"));
            }
        }
        FormulaKeyAction::SmartPower => {
            if is_preceded_by_operand(edit) {
                apply_keypad_to_edit(edit, KeypadAction::InsertTemplate("^(□)"));
            } else {
                apply_keypad_to_edit(edit, KeypadAction::InsertTemplate("□^(□)"));
            }
        }
        FormulaKeyAction::SmartSubscript => {
            if is_preceded_by_operand(edit) {
                apply_keypad_to_edit(edit, KeypadAction::InsertTemplate("_(□)"));
            } else {
                apply_keypad_to_edit(edit, KeypadAction::InsertTemplate("□_(□)"));
            }
        }
        FormulaKeyAction::Newline => apply_keypad_to_edit(edit, KeypadAction::Newline),
        FormulaKeyAction::MoveLeft => apply_keypad_to_edit(edit, KeypadAction::MoveLeft),
        FormulaKeyAction::MoveRight => apply_keypad_to_edit(edit, KeypadAction::MoveRight),
        FormulaKeyAction::Backspace => apply_keypad_to_edit(edit, KeypadAction::Backspace),
        FormulaKeyAction::Clear => apply_keypad_to_edit(edit, KeypadAction::Clear),
        FormulaKeyAction::SetPage(_)
        | FormulaKeyAction::ToggleRawInput
        | FormulaKeyAction::FormatShorthand => {}
    }
}

/// Longest-match-first LaTeX-style commands (each starting with `\` must end at a non-letter).
const SHORTHAND_RULES: &[(&str, &str)] = &[
    ("\\epsilon", "ε"),
    ("\\partial", "∂"),
    ("\\approx", "≈"),
    ("\\forall", "∀"),
    ("\\exists", "∃"),
    ("\\propto", "∝"),
    ("\\lambda", "λ"),
    ("\\infty", "∞"),
    ("\\times", "×"),
    ("\\nabla", "∇"),
    ("\\alpha", "α"),
    ("\\delta", "δ"),
    ("\\Delta", "Δ"),
    ("\\gamma", "γ"),
    ("\\theta", "θ"),
    ("\\sigma", "σ"),
    ("\\Sigma", "Σ"),
    ("\\omega", "ω"),
    ("\\Omega", "Ω"),
    ("\\beta", "β"),
    ("\\prod", "∏"),
    ("\\sqrt", "√"),
    ("\\cdot", "·"),
    ("\\inf", "∞"),
    ("\\sum", "∑"),
    ("\\int", "∫"),
    ("\\eta", "η"),
    ("\\rho", "ρ"),
    ("\\tau", "τ"),
    ("\\phi", "φ"),
    ("\\psi", "ψ"),
    ("\\neq", "≠"),
    ("\\leq", "≤"),
    ("\\geq", "≥"),
    ("\\div", "÷"),
    ("\\mu", "μ"),
    ("\\pi", "π"),
    ("\\pm", "±"),
    ("\\to", "→"),
    ("\\in", "∈"),
    ("<=>", "⇔"),
    ("->", "→"),
    ("<=", "≤"),
    (">=", "≥"),
    ("!=", "≠"),
    ("+-", "±"),
    ("^-1", "⁻¹"),
    ("^2", "²"),
    ("^3", "³"),
    ("^n", "ⁿ"),
    ("_0", "₀"),
    ("_1", "₁"),
    ("_2", "₂"),
    ("_3", "₃"),
    ("_4", "₄"),
    ("_5", "₅"),
    ("_6", "₆"),
    ("_7", "₇"),
    ("_8", "₈"),
    ("_9", "₉"),
    ("_i", "ᵢ"),
    ("_j", "ⱼ"),
    ("_k", "ₖ"),
    ("_n", "ₙ"),
    ("_x", "ₓ"),
];

/// Expand TeX-style commands, ASCII operator shorthands, and simple sub/superscripts into Unicode math.
pub fn expand_math_shorthand(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let bytes = input.as_bytes();

    while i < bytes.len() {
        let rest = &input[i..];
        let mut matched = None;
        for &(pat, rep) in SHORTHAND_RULES {
            if let Some(after) = rest.strip_prefix(pat) {
                if pat.starts_with('\\') && after.starts_with(|c: char| c.is_ascii_alphabetic()) {
                    continue;
                }
                matched = Some((pat.len(), rep));
                break;
            }
        }

        if let Some((plen, rep)) = matched {
            out.push_str(rep);
            i += plen;
        } else if let Some(ch) = rest.chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }

    out
}

/// Expand the token immediately preceding a trailing ASCII space (`' '`) if it matches a shorthand.
pub fn expand_trailing_command_on_space(input: &str) -> Option<String> {
    let without_space = input.strip_suffix(' ')?;
    if without_space.is_empty() {
        return None;
    }

    let token_start = without_space
        .rfind(char::is_whitespace)
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let prefix = &without_space[..token_start];
    let token = &without_space[token_start..];
    if token.is_empty() {
        return None;
    }

    let expanded_token = expand_math_shorthand(token);
    if expanded_token == token {
        return None;
    }

    Some(format!("{}{} ", prefix, expanded_token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_math_shorthand_and_trailing_space() {
        assert_eq!(expand_math_shorthand("\\sum \\alpha^2"), "∑ α²");
        assert_eq!(
            expand_math_shorthand("\\Delta w_i_j = \\eta \\cdot x_i \\cdot y_j"),
            "Δ wᵢⱼ = η · xᵢ · yⱼ"
        );
        assert_eq!(expand_math_shorthand("a <= b <=> x^-1 != 0"), "a ≤ b ⇔ x⁻¹ ≠ 0");
        assert_eq!(expand_math_shorthand("\\infty \\in R"), "∞ ∈ R");

        assert_eq!(
            expand_trailing_command_on_space("E = \\alpha "),
            Some("E = α ".into())
        );
        assert_eq!(
            expand_trailing_command_on_space("f(x) -> "),
            Some("f(x) → ".into())
        );
        assert_eq!(expand_trailing_command_on_space("plain "), None);
        assert_eq!(expand_trailing_command_on_space("\\alpha"), None);
    }

    #[test]
    fn test_photomath_smart_keypad_and_placeholder_flow() {
        let mut edit = rune::TextEdit::new("");
        apply_formula_key_to_edit(&mut edit, FormulaKeyAction::SmartSquare);
        assert_eq!(edit.text, "□²");
        assert_eq!(edit.selected(), "□");

        apply_formula_key_to_edit(&mut edit, FormulaKeyAction::InsertTemplate("x"));
        assert_eq!(edit.text, "x²");

        edit = rune::TextEdit::new("y");
        apply_formula_key_to_edit(&mut edit, FormulaKeyAction::SmartPower);
        assert_eq!(edit.text, "y^(□)");
        assert_eq!(edit.selected(), "□");
        apply_formula_key_to_edit(&mut edit, FormulaKeyAction::InsertTemplate("3"));
        assert_eq!(edit.text, "y^(3)");

        edit = rune::TextEdit::new("w");
        apply_formula_key_to_edit(&mut edit, FormulaKeyAction::SmartSubscript);
        assert_eq!(edit.text, "w_(□)");
        assert_eq!(edit.selected(), "□");

        edit = rune::TextEdit::new("");
        apply_formula_key_to_edit(&mut edit, FormulaKeyAction::InsertTemplate("(□)/(□)"));
        assert_eq!(edit.text, "(□)/(□)");
        assert_eq!(edit.selected(), "□");
        apply_formula_key_to_edit(&mut edit, FormulaKeyAction::InsertTemplate("a"));
        apply_formula_key_to_edit(&mut edit, FormulaKeyAction::MoveRight);
        apply_formula_key_to_edit(&mut edit, FormulaKeyAction::InsertTemplate("b"));
        assert_eq!(edit.text, "(a)/(b)");
    }
}
