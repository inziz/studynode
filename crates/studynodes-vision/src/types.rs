use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Selectable OCR backend strategy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OcrEnginePreference {
    /// On Windows tries Tier 1 Windows Native OCR first and falls back to Tier 2 Neural OCR;
    /// on macOS/Linux uses Tier 2 Neural OCR directly.
    #[default]
    Auto,
    /// Tier 1: Built-in `Windows.Media.Ocr` WinRT API (0 MB download, Windows 10/11 only).
    WindowsNative,
    /// Tier 2: Cross-platform pure-Rust neural OCR (`ocrs` + `rten`).
    NeuralCrossPlatform,
}

impl OcrEnginePreference {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Auto => "Auto (Tier 1 -> Tier 2)",
            Self::WindowsNative => "Windows Native OCR (Tier 1)",
            Self::NeuralCrossPlatform => "Neural OCR (Tier 2 Cross-Platform)",
        }
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::WindowsNative => "Win OCR",
            Self::NeuralCrossPlatform => "Neural",
        }
    }
}

/// Output formatting mode requested for OCR extraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OcrMode {
    PlainText,
    Markdown,
    EquationFormula,
}

/// Extracted OCR text with provenance metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrOutput {
    pub text: String,
    pub engine_label: String,
    pub lines: Vec<String>,
}

/// Translated text result with target language & backend tag.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationOutput {
    pub translated_text: String,
    pub target_language: String,
    pub backend_label: String,
}

/// On-disk availability of Tier 2 neural OCR weights.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeuralModelStatus {
    pub detection_ready: bool,
    pub recognition_ready: bool,
    pub models_dir: PathBuf,
}

impl NeuralModelStatus {
    pub fn is_ready(&self) -> bool {
        self.detection_ready && self.recognition_ready
    }
}

/// Runtime configuration for local OCR and cross-platform translation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisionConfig {
    pub models_dir: PathBuf,
    pub engine: OcrEnginePreference,
    pub ollama_url: String,
    pub ollama_model: String,
    pub gemini_api_key: Option<String>,
}

impl VisionConfig {
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            engine: OcrEnginePreference::Auto,
            ollama_url: "http://127.0.0.1:11434".into(),
            ollama_model: "qwen2.5:3b".into(),
            gemini_api_key: std::env::var("GEMINI_API_KEY")
                .ok()
                .filter(|k| !k.trim().is_empty()),
        }
    }
}

/// Compact target language presets (`(ISO code, toolbar pill label)`).
pub const SUPPORTED_LANGUAGES: &[(&str, &str)] = &[
    ("en", "EN"),
    ("es", "ES"),
    ("fr", "FR"),
    ("de", "DE"),
    ("pt", "PT"),
    ("ja", "JA"),
];

pub fn language_display_name(code: &str) -> &'static str {
    match code.to_ascii_lowercase().as_str() {
        "en" => "English",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "pt" => "Portuguese",
        "ja" => "Japanese",
        _ => "English",
    }
}

/// Errors returned by Tier 1 / Tier 2 OCR and translation pipelines.
#[derive(Debug, Error)]
pub enum VisionError {
    #[error("Failed to read image: {0}")]
    ImageRead(String),
    #[error("OCR backend unavailable on this platform: {0}")]
    UnsupportedOnPlatform(String),
    #[error("Tier 2 neural OCR models are not downloaded yet ({0})")]
    ModelsNotDownloaded(String),
    #[error("Failed to download neural OCR model: {0}")]
    ModelDownloadFailed(String),
    #[error("OCR extraction failed: {0}")]
    OcrFailed(String),
    #[error("Translation failed: {0}")]
    TranslationFailed(String),
}
