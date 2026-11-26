use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::types::{TranslationOutput, VisionConfig, VisionError, language_display_name};

/// Lightweight source language guesser so free translation endpoints receive a distinct `src|dst` pair.
pub fn detect_source_language(text: &str, target_lang: &str) -> &'static str {
    let target = target_lang.trim().to_ascii_lowercase();

    let has_japanese_kana_or_cjk = text.chars().any(|c| {
        ('\u{3040}'..='\u{30FF}').contains(&c) || ('\u{4E00}'..='\u{9FFF}').contains(&c)
    });
    if has_japanese_kana_or_cjk && target != "ja" {
        return "ja";
    }

    if text.chars().any(|c| matches!(c, 'ñ' | 'Ñ' | '¿' | '¡')) && target != "es" {
        return "es";
    }
    if text.chars().any(|c| matches!(c, 'ã' | 'Ã' | 'õ' | 'Õ' | 'ç' | 'Ç')) && target != "pt" {
        return "pt";
    }
    if text.chars().any(|c| matches!(c, 'ß' | 'ä' | 'ö' | 'ü' | 'Ä' | 'Ö' | 'Ü'))
        && target != "de"
    {
        return "de";
    }

    let lower = format!(" {} ", text.to_ascii_lowercase());
    let score = |markers: &[&str]| -> usize {
        markers.iter().filter(|m| lower.contains(**m)).count()
    };

    let candidates = [
        ("es", score(&[" el ", " la ", " los ", " las ", " que ", " para ", " con ", " una ", " del "])),
        ("fr", score(&[" le ", " la ", " les ", " des ", " une ", " est ", " pour ", " dans ", " avec "])),
        ("de", score(&[" der ", " die ", " das ", " und ", " ist ", " mit ", " für ", " nicht ", " ein "])),
        ("pt", score(&[" não ", " uma ", " para ", " com ", " dos ", " das ", " que ", " por "])),
        ("en", score(&[" the ", " and ", " is ", " of ", " to ", " in ", " that ", " with ", " for "])),
    ];

    let best = candidates
        .iter()
        .filter(|(code, _)| *code != target)
        .max_by_key(|(_, s)| *s)
        .copied();

    match best {
        Some((code, s)) if s > 0 => code,
        _ => {
            if target == "en" {
                "es"
            } else {
                "en"
            }
        }
    }
}

/// Split text cleanly into paragraph/sentence chunks <= `max_chars` without splitting mid-word.
pub fn chunk_text_for_translation(text: &str, max_chars: usize) -> Vec<String> {
    let limit = max_chars.max(40);
    let mut chunks = Vec::new();
    let mut current = String::new();

    for para in text.split('\n') {
        let trimmed = para.trim();
        if trimmed.is_empty() {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            continue;
        }

        if trimmed.chars().count() <= limit {
            if current.is_empty() {
                current.push_str(trimmed);
            } else if current.chars().count() + 1 + trimmed.chars().count() <= limit {
                current.push('\n');
                current.push_str(trimmed);
            } else {
                chunks.push(std::mem::take(&mut current));
                current.push_str(trimmed);
            }
            continue;
        }

        if !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
        }

        let mut line_buf = String::new();
        for word in trimmed.split_whitespace() {
            if line_buf.is_empty() {
                if word.chars().count() > limit {
                    let mut part = String::new();
                    for ch in word.chars() {
                        if part.chars().count() >= limit {
                            chunks.push(std::mem::take(&mut part));
                        }
                        part.push(ch);
                    }
                    if !part.is_empty() {
                        line_buf = part;
                    }
                } else {
                    line_buf.push_str(word);
                }
            } else if line_buf.chars().count() + 1 + word.chars().count() <= limit {
                line_buf.push(' ');
                line_buf.push_str(word);
            } else {
                chunks.push(std::mem::take(&mut line_buf));
                line_buf.push_str(word);
            }
        }
        if !line_buf.is_empty() {
            chunks.push(line_buf);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaTagModel>,
}

#[derive(Deserialize)]
struct OllamaTagModel {
    name: String,
}

#[derive(Serialize)]
struct OllamaGenerateReq<'a> {
    model: &'a str,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaGenerateResp {
    response: String,
}

#[derive(Deserialize)]
struct MyMemoryResp {
    #[serde(rename = "responseData")]
    response_data: Option<MyMemoryData>,
    #[serde(rename = "responseStatus")]
    response_status: Option<u16>,
    #[serde(rename = "responseDetails")]
    response_details: Option<String>,
}

#[derive(Deserialize)]
struct MyMemoryData {
    #[serde(rename = "translatedText")]
    translated_text: Option<String>,
}

#[derive(Deserialize)]
struct GeminiResp {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
}

#[derive(Deserialize)]
struct GeminiContent {
    #[serde(default)]
    parts: Vec<GeminiPart>,
}

#[derive(Deserialize)]
struct GeminiPart {
    text: Option<String>,
}

async fn try_ollama_translate(
    client: &reqwest::Client,
    text: &str,
    target_lang: &str,
    config: &VisionConfig,
) -> Option<TranslationOutput> {
    let base = config.ollama_url.trim_end_matches('/');
    let tags_url = format!("{}/api/tags", base);
    let probe = client
        .get(&tags_url)
        .timeout(Duration::from_millis(700))
        .send()
        .await
        .ok()?;
    if !probe.status().is_success() {
        return None;
    }

    let tags = probe.json::<OllamaTagsResponse>().await.ok()?;
    if tags.models.is_empty() {
        return None;
    }

    let chosen_model = tags
        .models
        .iter()
        .find(|m| m.name == config.ollama_model || m.name.starts_with(&config.ollama_model))
        .map(|m| m.name.clone())
        .unwrap_or_else(|| tags.models[0].name.clone());

    let lang_name = language_display_name(target_lang);
    let prompt = format!(
        "Translate the following study text into {}.\nPreserve Markdown formatting, code blocks, and mathematical notation strictly.\nOutput ONLY the translated text without commentary.\n\n{}",
        lang_name, text
    );

    let gen_url = format!("{}/api/generate", base);
    let resp = client
        .post(&gen_url)
        .timeout(Duration::from_secs(45))
        .json(&OllamaGenerateReq {
            model: &chosen_model,
            prompt,
            stream: false,
        })
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed = resp.json::<OllamaGenerateResp>().await.ok()?;
    let trimmed = parsed.response.trim().to_string();
    if trimmed.is_empty() {
        return None;
    }

    Some(TranslationOutput {
        translated_text: trimmed,
        target_language: target_lang.to_ascii_lowercase(),
        backend_label: format!("Local Ollama ({})", chosen_model),
    })
}

async fn try_gemini_translate(
    client: &reqwest::Client,
    text: &str,
    target_lang: &str,
    api_key: &str,
) -> Option<TranslationOutput> {
    let lang_name = language_display_name(target_lang);
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-lite:generateContent?key={}",
        api_key.trim()
    );
    let payload = serde_json::json!({
        "contents": [{
            "parts": [{
                "text": format!(
                    "Translate the following text to {}. Preserve Markdown and mathematical formulas. Return only the translation.\n\n{}",
                    lang_name, text
                )
            }]
        }]
    });

    let resp = client
        .post(&url)
        .timeout(Duration::from_secs(20))
        .json(&payload)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed = resp.json::<GeminiResp>().await.ok()?;
    let text_out = parsed
        .candidates
        .first()
        .and_then(|c| c.content.as_ref())
        .and_then(|c| c.parts.first())
        .and_then(|p| p.text.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;

    Some(TranslationOutput {
        translated_text: text_out.to_string(),
        target_language: target_lang.to_ascii_lowercase(),
        backend_label: "Gemini 2.5 Flash-Lite".into(),
    })
}

async fn try_mymemory_translate(
    client: &reqwest::Client,
    text: &str,
    target_lang: &str,
) -> Result<TranslationOutput, String> {
    let target = target_lang.trim().to_ascii_lowercase();
    let src = detect_source_language(text, &target);
    let langpair = format!("{}|{}", src, target);
    let chunks = chunk_text_for_translation(text, 450);

    let mut translated_chunks = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let resp = client
            .get("https://api.mymemory.translated.net/get")
            .query(&[("q", chunk.as_str()), ("langpair", langpair.as_str())])
            .timeout(Duration::from_secs(12))
            .send()
            .await
            .map_err(|e| format!("Network request failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("MyMemory HTTP {}", resp.status()));
        }

        let body = resp
            .json::<MyMemoryResp>()
            .await
            .map_err(|e| format!("Invalid MyMemory JSON: {}", e))?;

        if let Some(status) = body.response_status {
            if status >= 400 {
                return Err(body
                    .response_details
                    .unwrap_or_else(|| format!("MyMemory error status {}", status)));
            }
        }

        let out = body
            .response_data
            .and_then(|d| d.translated_text)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "MyMemory returned empty translation".to_string())?;
        translated_chunks.push(out);
    }

    Ok(TranslationOutput {
        translated_text: translated_chunks.join("\n\n"),
        target_language: target,
        backend_label: format!("MyMemory ({})", langpair.to_uppercase()),
    })
}

/// Cross-platform translation: Local Ollama (offline) -> optional Gemini API -> free MyMemory API.
pub fn translate_text(
    text: &str,
    target_lang: &str,
    config: &VisionConfig,
) -> Result<TranslationOutput, VisionError> {
    let clean = text.trim();
    if clean.is_empty() {
        return Err(VisionError::TranslationFailed(
            "No source text available to translate.".into(),
        ));
    }

    let target = target_lang.trim().to_ascii_lowercase();
    let client = reqwest::Client::builder()
        .user_agent("StudyNodes/0.1")
        .build()
        .map_err(|e| VisionError::TranslationFailed(e.to_string()))?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| VisionError::TranslationFailed(format!("tokio runtime: {}", e)))?;

    rt.block_on(async {
        if let Some(out) = try_ollama_translate(&client, clean, &target, config).await {
            return Ok(out);
        }

        if let Some(key) = config
            .gemini_api_key
            .as_deref()
            .filter(|k| !k.trim().is_empty())
        {
            if let Some(out) = try_gemini_translate(&client, clean, &target, key).await {
                return Ok(out);
            }
        }

        match try_mymemory_translate(&client, clean, &target).await {
            Ok(out) => Ok(out),
            Err(net_err) => Err(VisionError::TranslationFailed(format!(
                "{}. Start local Ollama ({}) with `{}` for offline translation, or check internet connectivity.",
                net_err, config.ollama_url, config.ollama_model
            ))),
        }
    })
}
