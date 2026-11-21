use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Selectable OCR backend strategy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OcrEnginePreference {
    /// Uses Cloud Vision API when configured, otherwise falls back seamlessly to local PP-OCRv5.
    #[default]
    Auto,
    /// Offline cross-platform PP-OCRv5 ONNX engine (auto-downloads ~21 MB once).
    #[serde(alias = "NeuralCrossPlatform", alias = "WindowsNative")]
    LocalPpOcrV5,
    /// Cloud Vision API (DeepSeek-OCR / Gemini Vision / OpenAI-compatible endpoint).
    CloudVision,
}

impl OcrEnginePreference {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Auto => "Auto (Cloud AI -> PP-OCRv5)",
            Self::LocalPpOcrV5 => "PP-OCRv5 (Local Offline)",
            Self::CloudVision => "Cloud Vision API",
        }
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::LocalPpOcrV5 => "PP-OCRv5",
            Self::CloudVision => "Cloud AI",
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

/// On-disk availability of local PP-OCRv5 ONNX weights and character dictionary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeuralModelStatus {
    pub detection_ready: bool,
    pub recognition_ready: bool,
    pub dict_ready: bool,
    pub models_dir: PathBuf,
}

impl NeuralModelStatus {
    pub fn is_ready(&self) -> bool {
        self.detection_ready && self.recognition_ready && self.dict_ready
    }
}

fn non_empty_env(names: &[&str]) -> Option<String> {
    for name in names {
        if let Ok(val) = std::env::var(name) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Runtime configuration for local OCR, Cloud Vision APIs, and cross-platform translation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisionConfig {
    pub models_dir: PathBuf,
    pub engine: OcrEnginePreference,
    pub ollama_url: String,
    pub ollama_model: String,
    pub gemini_api_key: Option<String>,
    pub cloud_vision_api_key: Option<String>,
    pub cloud_vision_base_url: String,
    pub cloud_vision_model: String,
}

impl VisionConfig {
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            engine: OcrEnginePreference::Auto,
            ollama_url: "http://127.0.0.1:11434".into(),
            ollama_model: "qwen2.5:3b".into(),
            gemini_api_key: non_empty_env(&["GEMINI_API_KEY"]),
            cloud_vision_api_key: non_empty_env(&[
                "VISION_API_KEY",
                "DEEPSEEK_API_KEY",
                "OPENAI_API_KEY",
            ]),
            cloud_vision_base_url: non_empty_env(&["VISION_API_URL"]).unwrap_or_default(),
            cloud_vision_model: non_empty_env(&["VISION_MODEL"]).unwrap_or_default(),
        }
    }

    pub fn effective_cloud_api_key(&self) -> Option<&str> {
        self.cloud_vision_api_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .or_else(|| {
                self.gemini_api_key
                    .as_deref()
                    .map(str::trim)
                    .filter(|k| !k.is_empty())
            })
    }

    pub fn has_cloud_vision(&self) -> bool {
        !self.cloud_vision_base_url.trim().is_empty() || self.effective_cloud_api_key().is_some()
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

/// Errors returned by local PP-OCRv5, Cloud Vision API, and translation pipelines.
#[derive(Debug, Error)]
pub enum VisionError {
    #[error("Failed to read image: {0}")]
    ImageRead(String),
    #[error("OCR backend unavailable on this platform: {0}")]
    UnsupportedOnPlatform(String),
    #[error("PP-OCRv5 local models are not downloaded yet ({0})")]
    ModelsNotDownloaded(String),
    #[error("Failed to download PP-OCRv5 model: {0}")]
    ModelDownloadFailed(String),
    #[error("OCR extraction failed: {0}")]
    OcrFailed(String),
    #[error("Translation failed: {0}")]
    TranslationFailed(String),
}
