use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;

use crate::postprocess::format_ocr_output;
use crate::types::{NeuralModelStatus, OcrMode, OcrOutput, VisionError};

/// Verified Hugging Face weights compatible with `ocrs 0.14` / `rten 0.28`.
pub const DETECTION_MODEL_URL: &str =
    "https://huggingface.co/robertknight/ocrs/resolve/main/text-detection-ssfbcj81.rten";
pub const RECOGNITION_MODEL_URL: &str =
    "https://huggingface.co/robertknight/ocrs/resolve/main/text-rec-checkpoint-s52qdbqt.rten";

const DETECTION_FILE: &str = "text-detection.rten";
const RECOGNITION_FILE: &str = "text-recognition.rten";
const MIN_VALID_MODEL_BYTES: u64 = 100_000;
const MAX_NEURAL_OCR_DIM: u32 = 2400;

static CACHED_ENGINE: OnceLock<Mutex<Option<(PathBuf, Arc<OcrEngine>)>>> = OnceLock::new();

fn ocrs_subdir(models_dir: &Path) -> PathBuf {
    models_dir.join("ocrs")
}

fn detection_model_path(models_dir: &Path) -> PathBuf {
    ocrs_subdir(models_dir).join(DETECTION_FILE)
}

fn recognition_model_path(models_dir: &Path) -> PathBuf {
    ocrs_subdir(models_dir).join(RECOGNITION_FILE)
}

fn file_looks_valid(path: &Path) -> bool {
    fs::metadata(path)
        .map(|m| m.is_file() && m.len() > MIN_VALID_MODEL_BYTES)
        .unwrap_or(false)
}

/// Inspect whether the Tier 2 pure-Rust neural OCR weights are cached locally.
pub fn model_status(models_dir: &Path) -> NeuralModelStatus {
    NeuralModelStatus {
        detection_ready: file_looks_valid(&detection_model_path(models_dir)),
        recognition_ready: file_looks_valid(&recognition_model_path(models_dir)),
        models_dir: models_dir.to_path_buf(),
    }
}

async fn download_one_model(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
) -> Result<(), VisionError> {
    if file_looks_valid(dest) {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| VisionError::ModelDownloadFailed(format!("{}: {}", parent.display(), e)))?;
    }

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| VisionError::ModelDownloadFailed(format!("GET {}: {}", url, e)))?;
    if !resp.status().is_success() {
        return Err(VisionError::ModelDownloadFailed(format!(
            "HTTP {} downloading {}",
            resp.status(),
            url
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| VisionError::ModelDownloadFailed(format!("Reading {}: {}", url, e)))?;
    if (bytes.len() as u64) < MIN_VALID_MODEL_BYTES {
        return Err(VisionError::ModelDownloadFailed(format!(
            "Downloaded model {} was unexpectedly small ({} bytes)",
            url,
            bytes.len()
        )));
    }

    let tmp_path = dest.with_extension("rten.tmp");
    fs::write(&tmp_path, &bytes)
        .map_err(|e| VisionError::ModelDownloadFailed(format!("{}: {}", tmp_path.display(), e)))?;
    if dest.exists() {
        let _ = fs::remove_file(dest);
    }
    fs::rename(&tmp_path, dest)
        .map_err(|e| VisionError::ModelDownloadFailed(format!("rename {}: {}", dest.display(), e)))?;
    Ok(())
}

/// Download missing Tier 2 detection/recognition `.rten` models (~12 MB total) into `models_dir/ocrs/`.
pub fn ensure_models_downloaded(models_dir: &Path) -> Result<NeuralModelStatus, VisionError> {
    let status = model_status(models_dir);
    if status.is_ready() {
        return Ok(status);
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("StudyNodes/0.1")
        .build()
        .map_err(|e| VisionError::ModelDownloadFailed(e.to_string()))?;

    let det_dest = detection_model_path(models_dir);
    let rec_dest = recognition_model_path(models_dir);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| VisionError::ModelDownloadFailed(format!("tokio runtime: {}", e)))?;

    rt.block_on(async {
        download_one_model(&client, DETECTION_MODEL_URL, &det_dest).await?;
        download_one_model(&client, RECOGNITION_MODEL_URL, &rec_dest).await?;
        Ok::<(), VisionError>(())
    })?;

    Ok(model_status(models_dir))
}

fn load_or_get_engine(models_dir: &Path) -> Result<Arc<OcrEngine>, VisionError> {
    let cache = CACHED_ENGINE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((cached_dir, engine)) = &*guard {
            if cached_dir == models_dir {
                return Ok(Arc::clone(engine));
            }
        }
    }

    let det_path = detection_model_path(models_dir);
    let rec_path = recognition_model_path(models_dir);
    if !file_looks_valid(&det_path) || !file_looks_valid(&rec_path) {
        return Err(VisionError::ModelsNotDownloaded(format!(
            "Expected models under {}",
            ocrs_subdir(models_dir).display()
        )));
    }

    let detection_model = Model::load_file(&det_path)
        .map_err(|e| VisionError::OcrFailed(format!("load {}: {}", det_path.display(), e)))?;
    let recognition_model = Model::load_file(&rec_path)
        .map_err(|e| VisionError::OcrFailed(format!("load {}: {}", rec_path.display(), e)))?;

    let engine = OcrEngine::new(OcrEngineParams {
        detection_model: Some(detection_model),
        recognition_model: Some(recognition_model),
        ..Default::default()
    })
    .map_err(|e| VisionError::OcrFailed(format!("OcrEngine::new: {}", e)))?;

    let arc = Arc::new(engine);
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((models_dir.to_path_buf(), Arc::clone(&arc)));
    }
    Ok(arc)
}

/// Tier 2: Run cross-platform pure-Rust neural OCR (`ocrs` + `rten`) on an image file.
pub fn extract_text_neural(
    image_path: &Path,
    models_dir: &Path,
    mode: OcrMode,
    auto_download: bool,
) -> Result<OcrOutput, VisionError> {
    if !model_status(models_dir).is_ready() {
        if auto_download {
            ensure_models_downloaded(models_dir)?;
        } else {
            return Err(VisionError::ModelsNotDownloaded(format!(
                "Run model download first ({})",
                ocrs_subdir(models_dir).display()
            )));
        }
    }

    let engine = load_or_get_engine(models_dir)?;

    let mut img = image::open(image_path)
        .map_err(|e| VisionError::ImageRead(format!("{}: {}", image_path.display(), e)))?;
    if img.width() > MAX_NEURAL_OCR_DIM || img.height() > MAX_NEURAL_OCR_DIM {
        img = img.resize(
            MAX_NEURAL_OCR_DIM,
            MAX_NEURAL_OCR_DIM,
            image::imageops::FilterType::Triangle,
        );
    }
    let rgb = img.into_rgb8();

    let img_source = ImageSource::from_bytes(rgb.as_raw(), rgb.dimensions())
        .map_err(|e| VisionError::OcrFailed(format!("ImageSource::from_bytes: {:?}", e)))?;
    let ocr_input = engine
        .prepare_input(img_source)
        .map_err(|e| VisionError::OcrFailed(format!("prepare_input: {}", e)))?;

    let word_rects = engine
        .detect_words(&ocr_input)
        .map_err(|e| VisionError::OcrFailed(format!("detect_words: {}", e)))?;
    let line_rects = engine.find_text_lines(&ocr_input, &word_rects);
    let line_texts = engine
        .recognize_text(&ocr_input, &line_rects)
        .map_err(|e| VisionError::OcrFailed(format!("recognize_text: {}", e)))?;

    let lines: Vec<String> = line_texts
        .into_iter()
        .flatten()
        .map(|l| l.to_string())
        .filter(|s| !s.trim().is_empty())
        .collect();

    let formatted = format_ocr_output(&lines, mode);
    Ok(OcrOutput {
        text: formatted,
        engine_label: "Tier 2 · Neural OCR (ocrs)".into(),
        lines,
    })
}
