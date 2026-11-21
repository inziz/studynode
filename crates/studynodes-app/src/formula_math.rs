/// Compact row shown inline directly inside active `Formula` cards on the canvas.
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

pub const GREEK_SYMBOLS: &[&str] = &[
    "α", "β", "γ", "δ", "Δ", "ε", "η", "θ", "λ", "μ", "π", "ρ", "σ", "Σ", "τ", "φ", "ψ", "ω", "Ω",
];

pub const OP_SYMBOLS: &[&str] = &[
    "∑", "∫", "∏", "√", "∂", "∇", "±", "×", "·", "÷", "≈", "≠", "≤", "≥", "→", "⇔", "∈", "∀",
    "∃", "∝", "∞",
];

pub const SCRIPT_SYMBOLS: &[&str] = &[
    "⁻¹", "²", "³", "ⁿ", "₀", "₁", "₂", "ᵢ", "ⱼ", "ₖ", "ₙ", "ₓ",
];

pub const FORMULA_TEMPLATES: &[(&str, &str)] = &[
    ("Sum", "∑ᵢ₌₁ⁿ xᵢ"),
    ("Integral", "∫ₐᵇ f(x) dx"),
    ("Deriv", "df / dx"),
    ("Fraction", "(a + b) / c"),
    ("Hebbian", "Δwᵢⱼ = η · xᵢ · yⱼ"),
    ("Quadratic", "x = (-b ± √(b² - 4ac)) / 2a"),
];

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
}
