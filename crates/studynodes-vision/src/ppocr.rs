use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use oar_ocr::oarocr::{OAROCR, OAROCRBuilder};

use crate::postprocess::{format_ocr_output, invert_rgb, mean_luminance, prepare_ocr_rgb};
use crate::types::{NeuralModelStatus, OcrMode, OcrOutput, VisionError};

pub const DETECTION_MODEL_URL: &str =
    "https://huggingface.co/PaddlePaddle/PP-OCRv5_mobile_det_onnx/resolve/main/inference.onnx";
pub const RECOGNITION_MODEL_URL: &str =
    "https://huggingface.co/PaddlePaddle/PP-OCRv5_mobile_rec_onnx/resolve/main/inference.onnx";
pub const DICT_URL: &str =
    "https://raw.githubusercontent.com/PaddlePaddle/PaddleOCR/main/ppocr/utils/dict/ppocrv5_dict.txt";

const DETECTION_FILE: &str = "pp-ocrv5_mobile_det.onnx";
const RECOGNITION_FILE: &str = "pp-ocrv5_mobile_rec.onnx";
const DICT_FILE: &str = "ppocrv5_dict.txt";

const MIN_VALID_MODEL_BYTES: u64 = 100_000;
const MIN_VALID_DICT_BYTES: u64 = 10_000;
const MAX_PPOCR_DIM: u32 = 2400;

static CACHED_ENGINE: OnceLock<Mutex<Option<(PathBuf, Arc<OAROCR>)>>> = OnceLock::new();

pub fn ppocr_subdir(models_dir: &Path) -> PathBuf {
    models_dir.join("ppocrv5")
}

fn detection_model_path(models_dir: &Path) -> PathBuf {
    ppocr_subdir(models_dir).join(DETECTION_FILE)
}

fn recognition_model_path(models_dir: &Path) -> PathBuf {
    ppocr_subdir(models_dir).join(RECOGNITION_FILE)
}

fn dict_path(models_dir: &Path) -> PathBuf {
    ppocr_subdir(models_dir).join(DICT_FILE)
}

fn file_looks_valid(path: &Path, min_bytes: u64) -> bool {
    fs::metadata(path)
        .map(|m| m.is_file() && m.len() >= min_bytes)
        .unwrap_or(false)
}

/// Inspect whether local PP-OCRv5 ONNX weights and dictionary are cached locally.
pub fn model_status(models_dir: &Path) -> NeuralModelStatus {
    NeuralModelStatus {
        detection_ready: file_looks_valid(&detection_model_path(models_dir), MIN_VALID_MODEL_BYTES),
        recognition_ready: file_looks_valid(
            &recognition_model_path(models_dir),
            MIN_VALID_MODEL_BYTES,
        ),
        dict_ready: file_looks_valid(&dict_path(models_dir), MIN_VALID_DICT_BYTES),
        models_dir: models_dir.to_path_buf(),
    }
}

async fn download_one_asset(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    min_bytes: u64,
) -> Result<(), VisionError> {
    if file_looks_valid(dest, min_bytes) {
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
    if (bytes.len() as u64) < min_bytes {
        return Err(VisionError::ModelDownloadFailed(format!(
            "Downloaded asset {} was unexpectedly small ({} bytes)",
            url,
            bytes.len()
        )));
    }

    let tmp_name = format!(
        "{}.tmp",
        dest.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("model")
    );
    let tmp_path = dest.with_file_name(tmp_name);
    fs::write(&tmp_path, &bytes)
        .map_err(|e| VisionError::ModelDownloadFailed(format!("{}: {}", tmp_path.display(), e)))?;
    if dest.exists() {
        let _ = fs::remove_file(dest);
    }
    fs::rename(&tmp_path, dest)
        .map_err(|e| VisionError::ModelDownloadFailed(format!("rename {}: {}", dest.display(), e)))?;
    Ok(())
}

/// Download missing PP-OCRv5 ONNX detection/recognition models and dictionary (~21 MB total).
pub fn ensure_models_downloaded(models_dir: &Path) -> Result<NeuralModelStatus, VisionError> {
    let status = model_status(models_dir);
    if status.is_ready() {
        return Ok(status);
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .user_agent("StudyNodes/0.1")
        .build()
        .map_err(|e| VisionError::ModelDownloadFailed(e.to_string()))?;

    let det_dest = detection_model_path(models_dir);
    let rec_dest = recognition_model_path(models_dir);
    let dict_dest = dict_path(models_dir);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| VisionError::ModelDownloadFailed(format!("tokio runtime: {}", e)))?;

    rt.block_on(async {
        download_one_asset(&client, DETECTION_MODEL_URL, &det_dest, MIN_VALID_MODEL_BYTES).await?;
        download_one_asset(
            &client,
            RECOGNITION_MODEL_URL,
            &rec_dest,
            MIN_VALID_MODEL_BYTES,
        )
        .await?;
        download_one_asset(&client, DICT_URL, &dict_dest, MIN_VALID_DICT_BYTES).await?;
        Ok::<(), VisionError>(())
    })?;

    Ok(model_status(models_dir))
}

fn load_or_get_engine(models_dir: &Path) -> Result<Arc<OAROCR>, VisionError> {
    let cache = CACHED_ENGINE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((cached_dir, engine)) = &*guard {
            if cached_dir == models_dir {
                return Ok(Arc::clone(engine));
            }
        }
    }

    let det = detection_model_path(models_dir);
    let rec = recognition_model_path(models_dir);
    let dict = dict_path(models_dir);
    if !file_looks_valid(&det, MIN_VALID_MODEL_BYTES)
        || !file_looks_valid(&rec, MIN_VALID_MODEL_BYTES)
        || !file_looks_valid(&dict, MIN_VALID_DICT_BYTES)
    {
        return Err(VisionError::ModelsNotDownloaded(format!(
            "Expected PP-OCRv5 models under {}",
            ppocr_subdir(models_dir).display()
        )));
    }

    let engine = OAROCRBuilder::new(det, rec, dict)
        .build()
        .map_err(|e| VisionError::OcrFailed(format!("PP-OCRv5 init: {}", e)))?;

    let arc = Arc::new(engine);
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((models_dir.to_path_buf(), Arc::clone(&arc)));
    }
    Ok(arc)
}

fn run_ppocr_pass(ocr: &OAROCR, rgb: image::RgbImage) -> Result<Vec<String>, VisionError> {
    let mut results = ocr
        .predict_with_rows(vec![rgb])
        .map_err(|e| VisionError::OcrFailed(format!("PP-OCRv5 predict: {}", e)))?;
    let Some(pred) = results.pop() else {
        return Ok(Vec::new());
    };

    let joined = match pred.page.text_by_rows(&pred.row_ids) {
        Ok(text) => text,
        Err(_) => pred.page.all_text().join("\n"),
    };

    Ok(joined
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

fn alnum_len(lines: &[String]) -> usize {
    lines
        .iter()
        .flat_map(|s| s.chars())
        .filter(|c| c.is_alphanumeric())
        .count()
}

/// Run offline cross-platform PP-OCRv5 ONNX OCR on an image file.
pub fn extract_text_ppocr(
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
                "Download PP-OCRv5 models first ({})",
                ppocr_subdir(models_dir).display()
            )));
        }
    }

    let engine = load_or_get_engine(models_dir)?;
    let rgb = prepare_ocr_rgb(image_path, MAX_PPOCR_DIM)?;
    let is_dark = mean_luminance(&rgb) < 115.0;
    let inverted = if is_dark {
        Some(invert_rgb(&rgb))
    } else {
        None
    };

    let mut lines = run_ppocr_pass(&engine, rgb)?;
    if alnum_len(&lines) < 3 {
        if let Some(inv_img) = inverted {
            if let Ok(inv_lines) = run_ppocr_pass(&engine, inv_img) {
                if alnum_len(&inv_lines) > alnum_len(&lines) {
                    lines = inv_lines;
                }
            }
        }
    }

    let formatted = format_ocr_output(&lines, mode);
    Ok(OcrOutput {
        text: formatted,
        engine_label: "PP-OCRv5 (Local)".into(),
        lines,
    })
}
