use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use base64::Engine;
use serde_json::json;

use crate::postprocess::{format_ocr_output, prepare_ocr_rgb};
use crate::types::{OcrMode, OcrOutput, VisionConfig, VisionError};

const MAX_CLOUD_OCR_DIM: u32 = 2048;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedCloudEndpoint {
    Gemini {
        api_key: String,
        model: String,
    },
    Anthropic {
        url: String,
        api_key: String,
        model: String,
    },
    OpenAiCompatible {
        chat_url: String,
        api_key: Option<String>,
        model: String,
    },
}

fn normalize_openai_chat_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        return trimmed.to_string();
    }
    let after_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    if !after_scheme.contains('/') {
        format!("{}/v1/chat/completions", trimmed)
    } else {
        format!("{}/chat/completions", trimmed)
    }
}

fn default_vision_model_for_url(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if lower.contains("api.airforce") || lower.contains("api.openai.com") {
        "gpt-4o-mini"
    } else if lower.contains("openrouter.ai") {
        "openai/gpt-4o-mini"
    } else if lower.contains("groq.com") {
        "meta-llama/llama-4-scout-17b-16e-instruct"
    } else if lower.contains("api.x.ai") {
        "grok-2-vision-1212"
    } else {
        "deepseek-ai/DeepSeek-OCR"
    }
}

/// Resolve Cloud Vision API target cleanly from user settings & environment variables.
pub fn resolve_cloud_endpoint(config: &VisionConfig) -> Result<ResolvedCloudEndpoint, VisionError> {
    let base = config.cloud_vision_base_url.trim();
    let model_cfg = config.cloud_vision_model.trim();
    let effective_key = config
        .effective_cloud_api_key()
        .map(str::trim)
        .filter(|k| !k.is_empty());

    if !base.is_empty() {
        if base.to_ascii_lowercase().contains("anthropic.com") {
            let api_key = effective_key
                .ok_or_else(|| VisionError::OcrFailed("Missing Anthropic API key.".into()))?
                .to_string();
            let trimmed = base.trim_end_matches('/');
            let url = if trimmed.ends_with("/messages") {
                trimmed.to_string()
            } else if trimmed.ends_with("/v1") {
                format!("{}/messages", trimmed)
            } else {
                format!("{}/v1/messages", trimmed)
            };
            let model = if model_cfg.is_empty() {
                "claude-sonnet-4-20250514".to_string()
            } else {
                model_cfg.to_string()
            };
            return Ok(ResolvedCloudEndpoint::Anthropic {
                url,
                api_key,
                model,
            });
        }
        let model = if model_cfg.is_empty() {
            default_vision_model_for_url(base).to_string()
        } else {
            model_cfg.to_string()
        };
        return Ok(ResolvedCloudEndpoint::OpenAiCompatible {
            chat_url: normalize_openai_chat_url(base),
            api_key: effective_key.map(str::to_string),
            model,
        });
    }

    let explicit_cloud_empty = config
        .cloud_vision_api_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .is_none();
    let gemini_present = config
        .gemini_api_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .is_some();

    if let Some(key) = effective_key {
        if key.starts_with("AIza") || (explicit_cloud_empty && gemini_present) {
            let model = if model_cfg.is_empty() {
                "gemini-2.5-flash".to_string()
            } else {
                model_cfg.to_string()
            };
            return Ok(ResolvedCloudEndpoint::Gemini {
                api_key: key.to_string(),
                model,
            });
        }

        if key.starts_with("sk-ant-") {
            let model = if model_cfg.is_empty() {
                "claude-sonnet-4-20250514".to_string()
            } else {
                model_cfg.to_string()
            };
            return Ok(ResolvedCloudEndpoint::Anthropic {
                url: "https://api.anthropic.com/v1/messages".into(),
                api_key: key.to_string(),
                model,
            });
        }

        let (chat_url, default_model) = if key.starts_with("sk-air-") {
            ("https://api.airforce/v1/chat/completions", "gpt-4o-mini")
        } else if key.starts_with("sk-or-") {
            (
                "https://openrouter.ai/api/v1/chat/completions",
                "openai/gpt-4o-mini",
            )
        } else if key.starts_with("gsk_") {
            (
                "https://api.groq.com/openai/v1/chat/completions",
                "meta-llama/llama-4-scout-17b-16e-instruct",
            )
        } else if key.starts_with("xai-") {
            (
                "https://api.x.ai/v1/chat/completions",
                "grok-2-vision-1212",
            )
        } else if key.starts_with("hf_") {
            (
                "https://router.huggingface.co/v1/chat/completions",
                "Qwen/Qwen2.5-VL-72B-Instruct",
            )
        } else if model_cfg.to_ascii_lowercase().contains("deepseek") {
            (
                "https://api.deepinfra.com/v1/openai/chat/completions",
                "deepseek-ai/DeepSeek-OCR",
            )
        } else {
            ("https://api.openai.com/v1/chat/completions", "gpt-4o-mini")
        };

        let model = if model_cfg.is_empty() {
            default_model.to_string()
        } else {
            model_cfg.to_string()
        };

        return Ok(ResolvedCloudEndpoint::OpenAiCompatible {
            chat_url: chat_url.to_string(),
            api_key: Some(key.to_string()),
            model,
        });
    }

    Err(VisionError::OcrFailed(
        "No Cloud Vision API configured. Open Settings next to the OCR engine button to add an API key/endpoint, or switch to PP-OCRv5.".into(),
    ))
}

/// Tailored prompt per OCR extraction mode and model family.
pub fn ocr_prompt_for_mode(mode: OcrMode, is_deepseek_ocr: bool) -> &'static str {
    if is_deepseek_ocr {
        match mode {
            OcrMode::PlainText => {
                "<|grounding|>Extract all readable text faithfully in reading order. Output only the extracted text."
            }
            OcrMode::Markdown => "<|grounding|>Convert the document to clean Markdown.",
            OcrMode::EquationFormula => {
                "<|grounding|>Extract the mathematical formula(s) cleanly. Use LaTeX or Unicode math notation only, without explanation."
            }
        }
    } else {
        match mode {
            OcrMode::PlainText => {
                "Perform accurate OCR on this image. Transcribe all visible text faithfully in reading order, preserving line breaks. Return ONLY the transcribed text without commentary."
            }
            OcrMode::Markdown => {
                "Transcribe this image into clean Markdown preserving headings, bullet lists, tables, and formulas. Return ONLY the Markdown content without commentary."
            }
            OcrMode::EquationFormula => {
                "Extract the mathematical equations and formulas from this image accurately using clean LaTeX or Unicode math symbols. Return ONLY the formula(s) without commentary."
            }
        }
    }
}

fn remove_tagged_span(input: &str, open: &str, close: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find(open) {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + open.len()..];
        if let Some(end_rel) = after_open.find(close) {
            rest = &after_open[end_rel + close.len()..];
        } else {
            rest = after_open;
        }
    }
    out.push_str(rest);
    out
}

/// Strip DeepSeek-OCR grounding/bounding-box markers (`<|ref|>...<|/ref|>`, `<|det|>...<|/det|>`) and outer fences.
pub fn strip_deepseek_grounding_tags(raw: &str) -> String {
    let s = remove_tagged_span(raw, "<|ref|>", "<|/ref|>");
    let s = remove_tagged_span(&s, "<|det|>", "<|/det|>");
    let s = s
        .replace("<|grounding|>", "")
        .replace("<|ref|>", "")
        .replace("<|/ref|>", "")
        .replace("<|det|>", "")
        .replace("<|/det|>", "");

    let trimmed = s.trim();
    if let Some(after_open) = trimmed.strip_prefix("```") {
        if let Some(after_nl) = after_open.find('\n') {
            let body = &after_open[after_nl + 1..];
            if let Some(inner) = body.strip_suffix("```") {
                if !inner.contains("```") {
                    return inner.trim().to_string();
                }
            }
        }
    }
    trimmed.to_string()
}

pub fn parse_gemini_vision_response(json: &serde_json::Value) -> Result<String, VisionError> {
    if let Some(msg) = json["error"]["message"].as_str() {
        return Err(VisionError::OcrFailed(format!("Gemini Vision API: {}", msg)));
    }
    let parts = json["candidates"]
        .get(0)
        .and_then(|c| c["content"]["parts"].as_array())
        .ok_or_else(|| VisionError::OcrFailed("Unexpected response format from Gemini Vision".into()))?;

    let text: String = parts
        .iter()
        .filter_map(|p| p["text"].as_str())
        .collect::<Vec<_>>()
        .join("");
    let cleaned = text.trim().to_string();
    if cleaned.is_empty() {
        return Err(VisionError::OcrFailed(
            "Gemini Vision returned an empty response".into(),
        ));
    }
    Ok(cleaned)
}

pub fn parse_openai_vision_response(json: &serde_json::Value) -> Result<String, VisionError> {
    if let Some(msg) = json["error"]["message"].as_str() {
        return Err(VisionError::OcrFailed(format!("Cloud Vision API: {}", msg)));
    }
    let content = &json["choices"][0]["message"]["content"];
    let raw = if let Some(s) = content.as_str() {
        s.to_string()
    } else if let Some(arr) = content.as_array() {
        arr.iter()
            .filter_map(|item| item.as_str().or_else(|| item["text"].as_str()))
            .collect::<Vec<_>>()
            .join("")
    } else {
        return Err(VisionError::OcrFailed(
            "Unexpected response format from Cloud Vision API".into(),
        ));
    };

    let cleaned = raw.trim().to_string();
    if cleaned.is_empty() {
        return Err(VisionError::OcrFailed(
            "Cloud Vision API returned an empty response".into(),
        ));
    }
    Ok(cleaned)
}

/// Extract text or formulas from an image using a configured Cloud Vision API endpoint.
pub fn extract_text_cloud(
    image_path: &Path,
    config: &VisionConfig,
    mode: OcrMode,
) -> Result<OcrOutput, VisionError> {
    let endpoint = resolve_cloud_endpoint(config)?;
    let rgb = prepare_ocr_rgb(image_path, MAX_CLOUD_OCR_DIM)?;

    let mut png_bytes = Vec::new();
    image::DynamicImage::ImageRgb8(rgb)
        .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| VisionError::ImageRead(format!("PNG encode {}: {}", image_path.display(), e)))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent("StudyNodes/0.1")
        .build()
        .map_err(|e| VisionError::OcrFailed(e.to_string()))?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| VisionError::OcrFailed(format!("tokio runtime: {}", e)))?;

    let (model_label, raw_text) = rt.block_on(async {
        match endpoint {
            ResolvedCloudEndpoint::Gemini { api_key, model } => {
                let prompt = ocr_prompt_for_mode(mode, false);
                let url = format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                    model, api_key
                );
                let payload = json!({
                    "contents": [{
                        "parts": [
                            { "text": prompt },
                            {
                                "inline_data": {
                                    "mime_type": "image/png",
                                    "data": b64
                                }
                            }
                        ]
                    }]
                });

                let resp = client
                    .post(&url)
                    .json(&payload)
                    .send()
                    .await
                    .map_err(|e| VisionError::OcrFailed(format!("Gemini request failed: {}", e)))?;
                let status = resp.status();
                let body_json: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| VisionError::OcrFailed(format!("Gemini HTTP {}: {}", status, e)))?;
                if !status.is_success() {
                    let msg = body_json["error"]["message"]
                        .as_str()
                        .unwrap_or("request rejected");
                    return Err(VisionError::OcrFailed(format!(
                        "Gemini Vision HTTP {}: {}",
                        status, msg
                    )));
                }
                let text = parse_gemini_vision_response(&body_json)?;
                Ok((model, text))
            }
            ResolvedCloudEndpoint::Anthropic {
                url,
                api_key,
                model,
            } => {
                let prompt = ocr_prompt_for_mode(mode, false);
                let payload = json!({
                    "model": model,
                    "max_tokens": 2048,
                    "messages": [{
                        "role": "user",
                        "content": [
                            {
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": "image/png",
                                    "data": b64
                                }
                            },
                            { "type": "text", "text": prompt }
                        ]
                    }]
                });
                let resp = client
                    .post(&url)
                    .header("x-api-key", api_key)
                    .header("anthropic-version", "2023-06-01")
                    .json(&payload)
                    .send()
                    .await
                    .map_err(|e| VisionError::OcrFailed(format!("Anthropic request failed: {}", e)))?;
                let status = resp.status();
                let body_json: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| VisionError::OcrFailed(format!("Anthropic HTTP {}: {}", status, e)))?;
                if !status.is_success() {
                    let msg = body_json["error"]["message"]
                        .as_str()
                        .unwrap_or("request rejected");
                    return Err(VisionError::OcrFailed(format!(
                        "Anthropic Vision HTTP {}: {}",
                        status, msg
                    )));
                }
                let text = body_json["content"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                if text.trim().is_empty() {
                    return Err(VisionError::OcrFailed(
                        "Anthropic Vision returned empty text".into(),
                    ));
                }
                Ok((model, text))
            }
            ResolvedCloudEndpoint::OpenAiCompatible {
                chat_url,
                api_key,
                model,
            } => {
                let is_deepseek = model.to_ascii_lowercase().contains("deepseek-ocr");
                let prompt = ocr_prompt_for_mode(mode, is_deepseek);
                let data_url = format!("data:image/png;base64,{}", b64);
                let payload = json!({
                    "model": model,
                    "temperature": 0.0,
                    "messages": [{
                        "role": "user",
                        "content": [
                            { "type": "text", "text": prompt },
                            { "type": "image_url", "image_url": { "url": data_url } }
                        ]
                    }]
                });

                let mut req = client.post(&chat_url).json(&payload);
                if let Some(key) = api_key {
                    req = req.bearer_auth(key);
                }
                let resp = req.send().await.map_err(|e| {
                    VisionError::OcrFailed(format!("Cloud Vision request failed: {}", e))
                })?;
                let status = resp.status();
                let raw_body = resp
                    .text()
                    .await
                    .map_err(|e| VisionError::OcrFailed(format!("Cloud Vision read failed: {}", e)))?;
                let body_json: serde_json::Value = serde_json::from_str(&raw_body).map_err(|e| {
                    VisionError::OcrFailed(format!("Cloud Vision HTTP {}: {}", status, e))
                })?;
                if !status.is_success() {
                    let msg = body_json["error"]["message"]
                        .as_str()
                        .unwrap_or(&raw_body);
                    return Err(VisionError::OcrFailed(format!(
                        "Cloud Vision HTTP {}: {}",
                        status, msg
                    )));
                }
                let text = parse_openai_vision_response(&body_json)?;
                Ok((model, text))
            }
        }
    })?;

    let cleaned = strip_deepseek_grounding_tags(&raw_text);
    let lines: Vec<String> = cleaned
        .lines()
        .map(|l| l.trim_end().to_string())
        .filter(|l| !l.trim().is_empty())
        .collect();
    let text = match mode {
        OcrMode::EquationFormula => format_ocr_output(&lines, OcrMode::EquationFormula),
        OcrMode::PlainText | OcrMode::Markdown => cleaned.trim().to_string(),
    };

    Ok(OcrOutput {
        text,
        engine_label: format!("Cloud AI ({})", model_label),
        lines,
    })
}
