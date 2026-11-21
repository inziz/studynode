#![deny(unsafe_code)]

pub mod cloud_ocr;
pub mod postprocess;
pub mod ppocr;
pub mod translator;
pub mod types;

use std::path::Path;

pub use postprocess::format_ocr_output;
pub use translator::{chunk_text_for_translation, detect_source_language};
pub use types::{
    NeuralModelStatus, OcrEnginePreference, OcrMode, OcrOutput, SUPPORTED_LANGUAGES,
    TranslationOutput, VisionConfig, VisionError, language_display_name,
};

/// Unified abstraction for image-to-text OCR, formula extraction, and translation.
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

/// Real PP-OCRv5 (Local Offline ONNX) + Cloud Vision API (DeepSeek-OCR / Gemini Vision / OpenAI-compatible).
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
        _language_hint: Option<&str>,
    ) -> Result<OcrOutput, VisionError> {
        match self.config.engine {
            OcrEnginePreference::LocalPpOcrV5 => ppocr::extract_text_ppocr(
                image_path,
                &self.config.models_dir,
                mode,
                true,
            ),
            OcrEnginePreference::CloudVision => {
                cloud_ocr::extract_text_cloud(image_path, &self.config, mode)
            }
            OcrEnginePreference::Auto => {
                if self.config.has_cloud_vision() {
                    if let Ok(cloud_out) =
                        cloud_ocr::extract_text_cloud(image_path, &self.config, mode)
                    {
                        if !cloud_out.text.trim().is_empty() {
                            return Ok(cloud_out);
                        }
                    }
                }
                ppocr::extract_text_ppocr(image_path, &self.config.models_dir, mode, true)
            }
        }
    }

    fn translate(&self, text: &str, target_lang: &str) -> Result<TranslationOutput, VisionError> {
        translator::translate_text(text, target_lang, &self.config)
    }

    fn model_status(&self) -> NeuralModelStatus {
        ppocr::model_status(&self.config.models_dir)
    }

    fn download_neural_models(&self) -> Result<NeuralModelStatus, VisionError> {
        ppocr::ensure_models_downloaded(&self.config.models_dir)
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
            dict_ready: self.models_ready,
            models_dir: Path::new(".").to_path_buf(),
        }
    }

    fn download_neural_models(&self) -> Result<NeuralModelStatus, VisionError> {
        Ok(NeuralModelStatus {
            detection_ready: true,
            recognition_ready: true,
            dict_ready: true,
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
        let st0 = ppocr::model_status(dir.path());
        assert!(!st0.is_ready());

        let sub = ppocr::ppocr_subdir(dir.path());
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("pp-ocrv5_mobile_det.onnx"), vec![1u8; 120_000]).unwrap();
        fs::write(sub.join("pp-ocrv5_mobile_rec.onnx"), vec![2u8; 120_000]).unwrap();
        fs::write(sub.join("ppocrv5_dict.txt"), vec![b'a'; 12_000]).unwrap();

        let st1 = ppocr::model_status(dir.path());
        assert!(st1.is_ready());
    }

    #[test]
    fn test_preprocess_upscales_and_inverts_dark_images() {
        let dir = tempdir().unwrap();
        let img_path = dir.path().join("dark_small.png");
        let img = image::RgbImage::from_pixel(200, 100, image::Rgb([16, 16, 20]));
        img.save(&img_path).unwrap();

        let prepared = postprocess::prepare_ocr_rgb(&img_path, 2400).unwrap();
        assert_eq!(prepared.dimensions(), (400, 200));
        assert!(postprocess::mean_luminance(&prepared) < 30.0);

        let inverted = postprocess::invert_rgb(&prepared);
        assert!(postprocess::mean_luminance(&inverted) > 220.0);
    }

    #[test]
    fn test_cloud_endpoint_resolution_and_response_parsing() {
        let base_cfg = VisionConfig {
            models_dir: Path::new(".").to_path_buf(),
            engine: OcrEnginePreference::CloudVision,
            ollama_url: "http://127.0.0.1:11434".into(),
            ollama_model: "qwen2.5:3b".into(),
            gemini_api_key: None,
            cloud_vision_api_key: Some("AIzaSyTestGeminiKey123".into()),
            cloud_vision_base_url: String::new(),
            cloud_vision_model: String::new(),
        };
        assert_eq!(
            cloud_ocr::resolve_cloud_endpoint(&base_cfg).unwrap(),
            cloud_ocr::ResolvedCloudEndpoint::Gemini {
                api_key: "AIzaSyTestGeminiKey123".into(),
                model: "gemini-2.5-flash".into(),
            }
        );

        let custom_cfg = VisionConfig {
            models_dir: Path::new(".").to_path_buf(),
            engine: OcrEnginePreference::CloudVision,
            ollama_url: "http://127.0.0.1:11434".into(),
            ollama_model: "qwen2.5:3b".into(),
            gemini_api_key: None,
            cloud_vision_api_key: Some("sk-test-token".into()),
            cloud_vision_base_url: "http://127.0.0.1:11434/v1/".into(),
            cloud_vision_model: "deepseek-ocr".into(),
        };
        assert_eq!(
            cloud_ocr::resolve_cloud_endpoint(&custom_cfg).unwrap(),
            cloud_ocr::ResolvedCloudEndpoint::OpenAiCompatible {
                chat_url: "http://127.0.0.1:11434/v1/chat/completions".into(),
                api_key: Some("sk-test-token".into()),
                model: "deepseek-ocr".into(),
            }
        );

        let stripped = cloud_ocr::strip_deepseek_grounding_tags(
            "<|ref|>text<|/ref|><|det|>[[12,34,560,78]]<|/det|>Action potential fires at -55 mV.",
        );
        assert_eq!(stripped, "Action potential fires at -55 mV.");

        let gemini_json = serde_json::json!({
            "candidates": [{
                "content": { "parts": [{ "text": "Hodgkin-Huxley model" }] }
            }]
        });
        assert_eq!(
            cloud_ocr::parse_gemini_vision_response(&gemini_json).unwrap(),
            "Hodgkin-Huxley model"
        );

        let openai_json = serde_json::json!({
            "choices": [{
                "message": { "content": "Synaptic plasticity" }
            }]
        });
        assert_eq!(
            cloud_ocr::parse_openai_vision_response(&openai_json).unwrap(),
            "Synaptic plasticity"
        );
    }

    #[ignore = "Downloads & runs PP-OCRv5 models (~21 MB) on the local screenshot fixture"]
    #[test]
    fn test_real_ppocrv5_extraction_on_uploaded_screenshot() {
        let sample_png = Path::new(
            "C:/Users/pelotadewindows11/.gemini/antigravity/brain/c9cdafef-02e7-47fb-85ad-2413450dca49/.user_uploaded/media_1795248497980_f60658b9.png",
        );
        let models_dir = Path::new("c:/Users/pelotadewindows11/Desktop/StudyNodes/study_projects/models");
        assert!(sample_png.exists(), "missing sample png");

        let t0 = std::time::Instant::now();
        let out = ppocr::extract_text_ppocr(sample_png, models_dir, OcrMode::PlainText, true)
            .expect("PP-OCRv5 extraction must succeed cleanly");
        let elapsed = t0.elapsed();
        println!("PP-OCRv5 ({:.2?}):\n{}", elapsed, out.text);
        assert!(
            out.text.contains("Extract Text") || out.text.contains("Extract Formula"),
            "unexpected PP-OCRv5 output: {}",
            out.text
        );
    }
}
