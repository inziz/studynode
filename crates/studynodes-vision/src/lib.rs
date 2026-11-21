#![deny(unsafe_code)]

pub mod neural_ocr;
pub mod postprocess;
pub mod translator;
pub mod types;
#[cfg(target_os = "windows")]
pub mod win_ocr;

use std::path::Path;

pub use postprocess::format_ocr_output;
pub use translator::{chunk_text_for_translation, detect_source_language};
pub use types::{
    NeuralModelStatus, OcrEnginePreference, OcrMode, OcrOutput, SUPPORTED_LANGUAGES,
    TranslationOutput, VisionConfig, VisionError, language_display_name,
};

/// Unified abstraction for local image-to-text OCR, formula extraction, and translation.
pub trait VisionService: Send + Sync {
    fn extract_image(
        &self,
        image_path: &Path,
        mode: OcrMode,
        language_hint: Option<&str>,
    ) -> Result<OcrOutput, VisionError>;

    fn translate(&self, text: &str, target_lang: &str) -> Result<TranslationOutput, VisionError>;

    fn model_status(&self) -> NeuralModelStatus;

    fn download_neural_models(&self) -> Result<NeuralModelStatus, VisionError>;
}

/// Real Tier 1 (Windows Native OCR) + Tier 2 (Cross-Platform Pure-Rust Neural OCR) implementation.
#[derive(Clone, Debug)]
pub struct LocalVisionService {
    pub config: VisionConfig,
}

impl LocalVisionService {
    pub fn new(config: VisionConfig) -> Self {
        Self { config }
    }
}

impl VisionService for LocalVisionService {
    fn extract_image(
        &self,
        image_path: &Path,
        mode: OcrMode,
        language_hint: Option<&str>,
    ) -> Result<OcrOutput, VisionError> {
        match self.config.engine {
            OcrEnginePreference::WindowsNative => {
                #[cfg(target_os = "windows")]
                {
                    win_ocr::extract_text_windows(image_path, mode, language_hint)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = (image_path, mode, language_hint);
                    Err(VisionError::UnsupportedOnPlatform(
                        "Windows Native OCR is only available on Windows. Switch engine to Auto or Neural."
                            .into(),
                    ))
                }
            }
            OcrEnginePreference::NeuralCrossPlatform => neural_ocr::extract_text_neural(
                image_path,
                &self.config.models_dir,
                mode,
                true,
            ),
            OcrEnginePreference::Auto => {
                #[cfg(target_os = "windows")]
                {
                    match win_ocr::extract_text_windows(image_path, mode, language_hint) {
                        Ok(out) => Ok(out),
                        Err(_win_err) => neural_ocr::extract_text_neural(
                            image_path,
                            &self.config.models_dir,
                            mode,
                            true,
                        ),
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = language_hint;
                    neural_ocr::extract_text_neural(
                        image_path,
                        &self.config.models_dir,
                        mode,
                        true,
                    )
                }
            }
        }
    }

    fn translate(&self, text: &str, target_lang: &str) -> Result<TranslationOutput, VisionError> {
        translator::translate_text(text, target_lang, &self.config)
    }

    fn model_status(&self) -> NeuralModelStatus {
        neural_ocr::model_status(&self.config.models_dir)
    }

    fn download_neural_models(&self) -> Result<NeuralModelStatus, VisionError> {
        neural_ocr::ensure_models_downloaded(&self.config.models_dir)
    }
}

/// Deterministic offline mock implementation for automated unit and UI integration tests.
#[derive(Clone, Debug, Default)]
pub struct MockVisionService {
    pub models_ready: bool,
    pub sample_plain: Option<String>,
    pub sample_equation: Option<String>,
}

impl VisionService for MockVisionService {
    fn extract_image(
        &self,
        _image_path: &Path,
        mode: OcrMode,
        _language_hint: Option<&str>,
    ) -> Result<OcrOutput, VisionError> {
        let raw_lines = match mode {
            OcrMode::EquationFormula => {
                vec![self
                    .sample_equation
                    .clone()
                    .unwrap_or_else(|| "\\Delta w_i_j = \\eta \\cdot x_i \\cdot y_j".into())]
            }
            OcrMode::Markdown => vec![
                "• Coincidence detector".into(),
                self.sample_plain
                    .clone()
                    .unwrap_or_else(|| "NMDA receptor expels Mg2+ upon depolarization".into()),
            ],
            OcrMode::PlainText => vec![self
                .sample_plain
                .clone()
                .unwrap_or_else(|| "Cells that fire together wire together.".into())],
        };
        let text = format_ocr_output(&raw_lines, mode);
        Ok(OcrOutput {
            text,
            engine_label: "MockVision (Test)".into(),
            lines: raw_lines,
        })
    }

    fn translate(&self, text: &str, target_lang: &str) -> Result<TranslationOutput, VisionError> {
        let code = target_lang.trim().to_ascii_lowercase();
        let translated_text = match code.as_str() {
            "es" => format!("[ES] {}", text.trim()),
            "fr" => format!("[FR] {}", text.trim()),
            "de" => format!("[DE] {}", text.trim()),
            "pt" => format!("[PT] {}", text.trim()),
            "ja" => format!("[JA] {}", text.trim()),
            _ => format!("[{}] {}", code.to_uppercase(), text.trim()),
        };
        Ok(TranslationOutput {
            translated_text,
            target_language: code,
            backend_label: "MockTranslator (Test)".into(),
        })
    }

    fn model_status(&self) -> NeuralModelStatus {
        NeuralModelStatus {
            detection_ready: self.models_ready,
            recognition_ready: self.models_ready,
            models_dir: Path::new(".").to_path_buf(),
        }
    }

    fn download_neural_models(&self) -> Result<NeuralModelStatus, VisionError> {
        Ok(NeuralModelStatus {
            detection_ready: true,
            recognition_ready: true,
            models_dir: Path::new(".").to_path_buf(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_mock_vision_service_modes_and_translation() {
        let svc = MockVisionService {
            models_ready: true,
            ..Default::default()
        };
        let path = Path::new("slide.png");

        let plain = svc.extract_image(path, OcrMode::PlainText, None).unwrap();
        assert_eq!(plain.text, "Cells that fire together wire together.");

        let md = svc.extract_image(path, OcrMode::Markdown, None).unwrap();
        assert!(md.text.contains("- Coincidence detector"));

        let eq = svc
            .extract_image(path, OcrMode::EquationFormula, None)
            .unwrap();
        assert_eq!(eq.text, "Δ wᵢⱼ = η · xᵢ · yⱼ");

        let tr = svc.translate(&plain.text, "es").unwrap();
        assert_eq!(tr.target_language, "es");
        assert!(tr.translated_text.starts_with("[ES]"));
    }

    #[test]
    fn test_format_ocr_output_handles_bullets_and_whitespace() {
        let lines = vec![
            "   ".into(),
            "• First point".into(),
            "▪ Second point".into(),
            "".into(),
            "".into(),
            "Third line".into(),
            "   ".into(),
        ];
        assert_eq!(
            format_ocr_output(&lines, OcrMode::Markdown),
            "- First point\n- Second point\n\nThird line"
        );
        assert_eq!(
            format_ocr_output(&["\\sum \\alpha^2 != 0".into()], OcrMode::EquationFormula),
            "∑ α² ≠ 0"
        );
    }

    #[test]
    fn test_detect_source_language_and_chunking() {
        assert_eq!(
            detect_source_language("The quick brown fox jumps with energy", "es"),
            "en"
        );
        assert_eq!(
            detect_source_language("La sinapsis es para la memoria con plasticidad", "en"),
            "es"
        );
        assert_eq!(detect_source_language("シナプス可塑性", "en"), "ja");

        let long_text = "First paragraph.\n\nSecond paragraph with many words separated nicely to test safe word-boundary chunk splitting behavior cleanly.";
        let chunks = chunk_text_for_translation(long_text, 45);
        assert!(chunks.len() >= 3);
        for c in chunks {
            assert!(c.chars().count() <= 45);
        }
    }

    #[test]
    fn test_neural_model_status_detects_missing_and_valid_files() {
        let dir = tempdir().unwrap();
        let st0 = neural_ocr::model_status(dir.path());
        assert!(!st0.is_ready());

        let sub = dir.path().join("ocrs");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("text-detection.rten"), vec![1u8; 120_000]).unwrap();
        fs::write(sub.join("text-recognition.rten"), vec![2u8; 120_000]).unwrap();

        let st1 = neural_ocr::model_status(dir.path());
        assert!(st1.is_ready());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_windows_ocr_smoke_on_synthetic_png() {
        if !win_ocr::is_available() {
            return;
        }
        let dir = tempdir().unwrap();
        let img_path = dir.path().join("blank.png");
        let img = image::RgbImage::from_pixel(160, 60, image::Rgb([255, 255, 255]));
        img.save(&img_path).unwrap();

        let res = win_ocr::extract_text_windows(&img_path, OcrMode::PlainText, None);
        assert!(
            res.is_ok(),
            "Expected Windows OCR smoke test to run cleanly, got: {:?}",
            res
        );
    }
}
