use std::path::Path;

use crate::types::{OcrMode, VisionError};

/// Normalize an input image to RGB8, upscaling crisp small screenshots and capping oversized ones.
pub fn prepare_ocr_rgb(image_path: &Path, max_dim: u32) -> Result<image::RgbImage, VisionError> {
    let mut img = image::open(image_path)
        .map_err(|e| VisionError::ImageRead(format!("{}: {}", image_path.display(), e)))?;

    let w = img.width();
    let h = img.height();
    if (w < 900 || h < 450) && w.saturating_mul(2) <= max_dim && h.saturating_mul(2) <= max_dim {
        img = img.resize(
            w.saturating_mul(2),
            h.saturating_mul(2),
            image::imageops::FilterType::CatmullRom,
        );
    } else if w > max_dim || h > max_dim {
        img = img.resize(max_dim, max_dim, image::imageops::FilterType::Triangle);
    }

    Ok(img.into_rgb8())
}

/// Average perceptual luminance `[0.0, 255.0]` across sampled pixels.
pub fn mean_luminance(rgb: &image::RgbImage) -> f32 {
    let mut sum = 0.0f64;
    let mut count = 0u64;
    for (_, _, px) in rgb.enumerate_pixels().step_by(4) {
        let luma = 0.2126 * px[0] as f64 + 0.7152 * px[1] as f64 + 0.0722 * px[2] as f64;
        sum += luma;
        count += 1;
    }
    if count == 0 {
        255.0
    } else {
        (sum / count as f64) as f32
    }
}

/// Invert RGB pixel channels so light-on-dark UI screenshots read cleanly as dark-on-light text.
pub fn invert_rgb(rgb: &image::RgbImage) -> image::RgbImage {
    let mut out = rgb.clone();
    for px in out.pixels_mut() {
        px[0] = 255 - px[0];
        px[1] = 255 - px[1];
        px[2] = 255 - px[2];
    }
    out
}

const EQUATION_SHORTHAND_RULES: &[(&str, &str)] = &[
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

fn expand_equation_shorthand(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let bytes = input.as_bytes();

    while i < bytes.len() {
        let rest = &input[i..];
        let mut matched = None;
        for &(pat, rep) in EQUATION_SHORTHAND_RULES {
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

/// Clean and format raw recognized OCR lines according to the requested extraction mode.
pub fn format_ocr_output(lines: &[String], mode: OcrMode) -> String {
    let trimmed_lines: Vec<String> = lines
        .iter()
        .map(|l| l.trim().to_string())
        .collect();

    let first_non_empty = trimmed_lines.iter().position(|l| !l.is_empty());
    let last_non_empty = trimmed_lines.iter().rposition(|l| !l.is_empty());
    let core: &[String] = match (first_non_empty, last_non_empty) {
        (Some(start), Some(end)) if start <= end => &trimmed_lines[start..=end],
        _ => return String::new(),
    };

    match mode {
        OcrMode::PlainText => {
            let mut out = Vec::with_capacity(core.len());
            let mut prev_empty = false;
            for line in core {
                if line.is_empty() {
                    if !prev_empty {
                        out.push(String::new());
                        prev_empty = true;
                    }
                } else {
                    out.push(line.clone());
                    prev_empty = false;
                }
            }
            out.join("\n")
        }
        OcrMode::Markdown => {
            let mut out = Vec::with_capacity(core.len());
            let mut prev_empty = false;
            for line in core {
                if line.is_empty() {
                    if !prev_empty {
                        out.push(String::new());
                        prev_empty = true;
                    }
                    continue;
                }
                prev_empty = false;
                let normalized = if let Some(rest) = line.strip_prefix("• ") {
                    format!("- {}", rest.trim())
                } else if let Some(rest) = line.strip_prefix('•') {
                    format!("- {}", rest.trim())
                } else if let Some(rest) = line.strip_prefix("▪ ") {
                    format!("- {}", rest.trim())
                } else if let Some(rest) = line.strip_prefix('▪') {
                    format!("- {}", rest.trim())
                } else if let Some(rest) = line.strip_prefix("· ") {
                    format!("- {}", rest.trim())
                } else {
                    line.clone()
                };
                out.push(normalized);
            }
            out.join("\n")
        }
        OcrMode::EquationFormula => {
            let joined: Vec<String> = core
                .iter()
                .filter(|l| !l.is_empty())
                .map(|l| expand_equation_shorthand(l))
                .collect();
            joined.join("\n")
        }
    }
}
