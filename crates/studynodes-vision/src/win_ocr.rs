use std::io::Cursor;
use std::path::Path;

use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapDecoder, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, IRandomAccessStream, InMemoryRandomAccessStream};
use windows::core::HSTRING;

use crate::postprocess::format_ocr_output;
use crate::types::{OcrMode, OcrOutput, VisionError};

const MAX_WINDOWS_OCR_DIM: u32 = 2400;

fn create_engine(language_hint: Option<&str>) -> Result<OcrEngine, VisionError> {
    if let Some(hint) = language_hint.map(str::trim).filter(|h| !h.is_empty()) {
        if let Ok(lang) = Language::CreateLanguage(&HSTRING::from(hint)) {
            if OcrEngine::IsLanguageSupported(&lang).unwrap_or(false) {
                if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&lang) {
                    return Ok(engine);
                }
            }
        }
    }

    if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
        return Ok(engine);
    }

    if let Ok(langs) = OcrEngine::AvailableRecognizerLanguages() {
        for lang in langs {
            if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&lang) {
                return Ok(engine);
            }
        }
    }

    Err(VisionError::UnsupportedOnPlatform(
        "No Windows OCR recognizer language pack is installed".into(),
    ))
}

fn load_software_bitmap(image_path: &Path) -> Result<SoftwareBitmap, VisionError> {
    let mut img = image::open(image_path)
        .map_err(|e| VisionError::ImageRead(format!("{}: {}", image_path.display(), e)))?;

    if img.width() > MAX_WINDOWS_OCR_DIM || img.height() > MAX_WINDOWS_OCR_DIM {
        img = img.resize(
            MAX_WINDOWS_OCR_DIM,
            MAX_WINDOWS_OCR_DIM,
            image::imageops::FilterType::Triangle,
        );
    }

    let mut png_bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| VisionError::ImageRead(format!("PNG encode failed: {}", e)))?;

    let stream = InMemoryRandomAccessStream::new()
        .map_err(|e| VisionError::OcrFailed(format!("InMemoryRandomAccessStream: {}", e)))?;
    let out_stream = stream
        .GetOutputStreamAt(0)
        .map_err(|e| VisionError::OcrFailed(format!("GetOutputStreamAt: {}", e)))?;
    let writer = DataWriter::CreateDataWriter(&out_stream)
        .map_err(|e| VisionError::OcrFailed(format!("DataWriter: {}", e)))?;
    writer
        .WriteBytes(&png_bytes)
        .map_err(|e| VisionError::OcrFailed(format!("WriteBytes: {}", e)))?;
    writer
        .StoreAsync()
        .map_err(|e| VisionError::OcrFailed(format!("StoreAsync: {}", e)))?
        .join()
        .map_err(|e| VisionError::OcrFailed(format!("StoreAsync.join: {}", e)))?;
    writer
        .FlushAsync()
        .map_err(|e| VisionError::OcrFailed(format!("FlushAsync: {}", e)))?
        .join()
        .map_err(|e| VisionError::OcrFailed(format!("FlushAsync.join: {}", e)))?;
    let _ = writer.DetachStream();

    stream
        .Seek(0)
        .map_err(|e| VisionError::OcrFailed(format!("Seek(0): {}", e)))?;
    let ras: IRandomAccessStream = stream.into();
    let decoder = BitmapDecoder::CreateAsync(&ras)
        .map_err(|e| VisionError::OcrFailed(format!("BitmapDecoder::CreateAsync: {}", e)))?
        .join()
        .map_err(|e| VisionError::OcrFailed(format!("BitmapDecoder join: {}", e)))?;
    decoder
        .GetSoftwareBitmapAsync()
        .map_err(|e| VisionError::OcrFailed(format!("GetSoftwareBitmapAsync: {}", e)))?
        .join()
        .map_err(|e| VisionError::OcrFailed(format!("GetSoftwareBitmap join: {}", e)))
}

/// Returns `true` if Windows 10/11 native OCR can construct a recognizer engine.
pub fn is_available() -> bool {
    create_engine(None).is_ok()
}

/// Tier 1: Run offline `Windows.Media.Ocr` WinRT recognition synchronously on a worker thread.
pub fn extract_text_windows(
    image_path: &Path,
    mode: OcrMode,
    language_hint: Option<&str>,
) -> Result<OcrOutput, VisionError> {
    let engine = create_engine(language_hint)?;
    let lang_tag = engine
        .RecognizerLanguage()
        .and_then(|l| l.LanguageTag())
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "os".into());

    let bitmap = load_software_bitmap(image_path)?;
    let ocr_res = engine
        .RecognizeAsync(&bitmap)
        .map_err(|e| VisionError::OcrFailed(format!("RecognizeAsync: {}", e)))?
        .join()
        .map_err(|e| VisionError::OcrFailed(format!("RecognizeAsync join: {}", e)))?;

    let mut lines = Vec::new();
    if let Ok(line_list) = ocr_res.Lines() {
        for line in line_list {
            if let Ok(txt) = line.Text() {
                let s = txt.to_string();
                if !s.trim().is_empty() {
                    lines.push(s);
                }
            }
        }
    }

    if lines.is_empty() {
        if let Ok(full) = ocr_res.Text() {
            for l in full.to_string().lines() {
                if !l.trim().is_empty() {
                    lines.push(l.to_string());
                }
            }
        }
    }

    let formatted = format_ocr_output(&lines, mode);
    Ok(OcrOutput {
        text: formatted,
        engine_label: format!("Tier 1 · Windows OCR ({})", lang_tag),
        lines,
    })
}
