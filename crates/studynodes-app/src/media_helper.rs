use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use studynodes_graph::model::SubtitleCue;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaPlatform {
    LocalFile,
    DirectUrl,
    YouTube { video_id: String },
    Spotify,
    Vimeo { video_id: String },
    WebLink,
}

impl MediaPlatform {
    pub fn label(&self) -> &'static str {
        match self {
            Self::LocalFile => "LOCAL FILE",
            Self::DirectUrl => "DIRECT MEDIA",
            Self::YouTube { .. } => "YOUTUBE",
            Self::Spotify => "SPOTIFY",
            Self::Vimeo { .. } => "VIMEO",
            Self::WebLink => "WEB MEDIA",
        }
    }
}

pub fn format_timestamp(secs: f64) -> String {
    let total = if secs.is_finite() && secs >= 0.0 {
        secs.round() as u64
    } else {
        0
    };
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

pub fn parse_timestamp(raw: &str) -> Option<f64> {
    let s = raw.trim().trim_end_matches('s');
    if s.is_empty() {
        return None;
    }
    if !s.contains(':') {
        return s
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.0);
    }
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        2 => {
            let mins = parts[0].trim().parse::<u64>().ok()?;
            let secs = parts[1].trim().parse::<f64>().ok()?;
            if !secs.is_finite() || secs < 0.0 || secs >= 60.0 {
                return None;
            }
            Some((mins as f64) * 60.0 + secs)
        }
        3 => {
            let hours = parts[0].trim().parse::<u64>().ok()?;
            let mins = parts[1].trim().parse::<u64>().ok()?;
            let secs = parts[2].trim().parse::<f64>().ok()?;
            if mins >= 60 || !secs.is_finite() || secs < 0.0 || secs >= 60.0 {
                return None;
            }
            Some((hours as f64) * 3600.0 + (mins as f64) * 60.0 + secs)
        }
        _ => None,
    }
}

fn extract_youtube_id(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    if let Some(idx) = lower.find("youtu.be/") {
        let rest = &url[idx + "youtu.be/".len()..];
        let id = rest.split(['?', '&', '#', '/']).next()?.trim();
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    if lower.contains("youtube.com") || lower.contains("youtube-nocookie.com") {
        if let Some(q_idx) = url.find('?') {
            for pair in url[q_idx + 1..].split('&') {
                if let Some(val) = pair.strip_prefix("v=") {
                    let id = val.split('#').next()?.trim();
                    if !id.is_empty() {
                        return Some(id.to_string());
                    }
                }
            }
        }
        for marker in ["/embed/", "/shorts/", "/live/"] {
            if let Some(idx) = lower.find(marker) {
                let rest = &url[idx + marker.len()..];
                let id = rest.split(['?', '&', '#', '/']).next()?.trim();
                if !id.is_empty() {
                    return Some(id.to_string());
                }
            }
        }
    }
    None
}

fn extract_vimeo_id(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    if !lower.contains("vimeo.com/") {
        return None;
    }
    let idx = lower.find("vimeo.com/")? + "vimeo.com/".len();
    let tail = &url[idx..];
    for seg in tail.split(['/', '?', '&', '#']) {
        if !seg.is_empty() && seg.chars().all(|c| c.is_ascii_digit()) {
            return Some(seg.to_string());
        }
    }
    None
}

pub fn is_direct_playable_media_url(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    if !(u.starts_with("http://") || u.starts_with("https://") || u.starts_with("file://")) {
        return false;
    }
    let path_only = u.split(['?', '#']).next().unwrap_or(&u);
    [
        ".mp4", ".mov", ".m4v", ".webm", ".mkv", ".mp3", ".wav", ".m4a", ".aac", ".flac", ".ogg",
        ".opus", ".aiff",
    ]
    .iter()
    .any(|ext| path_only.ends_with(ext))
}

pub fn classify_media_url(url: &str) -> MediaPlatform {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return MediaPlatform::LocalFile;
    }
    if let Some(video_id) = extract_youtube_id(trimmed) {
        return MediaPlatform::YouTube { video_id };
    }
    if trimmed.to_ascii_lowercase().contains("open.spotify.com")
        || trimmed.to_ascii_lowercase().starts_with("spotify:")
    {
        return MediaPlatform::Spotify;
    }
    if let Some(video_id) = extract_vimeo_id(trimmed) {
        return MediaPlatform::Vimeo { video_id };
    }
    if is_direct_playable_media_url(trimmed) {
        return MediaPlatform::DirectUrl;
    }
    MediaPlatform::WebLink
}

pub fn timestamp_jump_url(url: &str, seconds: f64) -> String {
    let secs = if seconds.is_finite() && seconds >= 0.0 {
        seconds.round() as u64
    } else {
        0
    };
    let trimmed = url.trim();
    match classify_media_url(trimmed) {
        MediaPlatform::YouTube { video_id } => {
            if secs == 0 {
                format!("https://www.youtube.com/watch?v={}", video_id)
            } else {
                format!("https://www.youtube.com/watch?v={}&t={}s", video_id, secs)
            }
        }
        MediaPlatform::Vimeo { video_id } => {
            if secs == 0 {
                format!("https://vimeo.com/{}", video_id)
            } else {
                format!("https://vimeo.com/{}#t={}s", video_id, secs)
            }
        }
        MediaPlatform::Spotify => {
            let base = trimmed.split('#').next().unwrap_or(trimmed);
            if let Some(q_idx) = base.find('?') {
                let path = &base[..q_idx];
                let pairs: Vec<&str> = base[q_idx + 1..]
                    .split('&')
                    .filter(|p| !p.starts_with("t=") && !p.is_empty())
                    .collect();
                let t_param = format!("t={}", secs);
                let mut query = pairs.join("&");
                if !query.is_empty() {
                    query.push('&');
                }
                query.push_str(&t_param);
                format!("{}?{}", path, query)
            } else {
                format!("{}?t={}", base, secs)
            }
        }
        MediaPlatform::WebLink | MediaPlatform::DirectUrl => {
            let base = trimmed.split('#').next().unwrap_or(trimmed);
            if secs == 0 {
                base.to_string()
            } else {
                format!("{}#t={}", base, secs)
            }
        }
        MediaPlatform::LocalFile => trimmed.to_string(),
    }
}

pub fn is_supported_media_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mp4" | "mov" | "m4v" | "webm" | "mkv" | "mp3" | "wav" | "m4a" | "aac" | "flac" | "ogg"
            | "opus" | "aiff"
    )
}

pub fn is_video_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi"
    )
}

pub fn infer_audio_mime(ext_or_name: &str) -> &'static str {
    let lower = ext_or_name.trim().to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or(&lower);
    match ext {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" | "aac" | "mp4" => "audio/mp4",
        "webm" => "audio/webm",
        "aiff" => "audio/aiff",
        _ => "audio/mpeg",
    }
}

pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 16);
    for &b in input.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push('%');
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0xF) as usize] as char);
        }
    }
    out
}

pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h1 = (bytes[i + 1] as char).to_digit(16);
                let h2 = (bytes[i + 2] as char).to_digit(16);
                if let (Some(a), Some(b)) = (h1, h2) {
                    out.push(((a << 4) | b) as u8);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn spotify_embed_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("spotify:") {
        let mut segs = rest.split(':').filter(|s| !s.is_empty());
        let kind = segs.next()?;
        let id = segs.next()?;
        return Some(format!("https://open.spotify.com/embed/{}/{}", kind, id));
    }
    let lower = trimmed.to_ascii_lowercase();
    let idx = lower.find("open.spotify.com/")?;
    let after = &trimmed[idx + "open.spotify.com/".len()..];
    let path_only = after.split('#').next().unwrap_or(after);
    if path_only.to_ascii_lowercase().starts_with("embed/") {
        return Some(format!("https://open.spotify.com/{}", path_only));
    }
    let clean = path_only.trim_start_matches('/');
    // Strip optional intl-<lang>/ prefix
    let mut parts: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();
    if parts.first().is_some_and(|p| p.starts_with("intl-")) {
        parts.remove(0);
    }
    if parts.len() >= 2 {
        Some(format!(
            "https://open.spotify.com/embed/{}/{}",
            parts[0], parts[1]
        ))
    } else {
        None
    }
}

/// Convert a YouTube, Vimeo, or Spotify media URL into an embeddable HTML `<iframe>` source URL.
pub fn embed_iframe_url(url: &str, start_secs: Option<u64>) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }
    match classify_media_url(trimmed) {
        MediaPlatform::YouTube { video_id } => {
            let mut u = format!(
                "https://www.youtube.com/embed/{}?rel=0&playsinline=1&modestbranding=1",
                video_id
            );
            if let Some(s) = start_secs.filter(|&v| v > 0) {
                u.push_str(&format!("&start={}&autoplay=1", s));
            }
            Some(u)
        }
        MediaPlatform::Vimeo { video_id } => {
            let mut u = format!("https://player.vimeo.com/video/{}", video_id);
            if let Some(s) = start_secs.filter(|&v| v > 0) {
                u.push_str(&format!("#t={}s", s));
            }
            Some(u)
        }
        MediaPlatform::Spotify => spotify_embed_url(trimmed),
        MediaPlatform::LocalFile | MediaPlatform::DirectUrl | MediaPlatform::WebLink => None,
    }
}

fn escape_html_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Build a minimal responsive full-bleed HTML document hosting an `<iframe src="...">`.
pub fn embed_iframe_html(iframe_src: &str, title: &str) -> String {
    let safe_src = escape_html_attr(iframe_src);
    let safe_title = escape_html_attr(if title.trim().is_empty() {
        "Embedded Media"
    } else {
        title.trim()
    });
    format!(
        r#"<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <meta name="referrer" content="strict-origin-when-cross-origin" />
  <style>
    html, body {{ margin: 0; padding: 0; width: 100%; height: 100%; background: #0f1115; overflow: hidden; }}
    iframe {{ width: 100%; height: 100%; border: 0; display: block; }}
  </style>
</head>
<body>
  <iframe src="{safe_src}" title="{safe_title}" referrerpolicy="strict-origin-when-cross-origin" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share; fullscreen" allowfullscreen></iframe>
</body>
</html>"#
    )
}

/// Fetch public oEmbed title for YouTube, Vimeo, or Spotify URLs without any bot-gated player calls.
pub async fn fetch_media_oembed_title(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let endpoint = match classify_media_url(trimmed) {
        MediaPlatform::YouTube { .. } => {
            format!("https://www.youtube.com/oembed?url={}&format=json", percent_encode(trimmed))
        }
        MediaPlatform::Vimeo { .. } => {
            format!("https://vimeo.com/api/oembed.json?url={}", percent_encode(trimmed))
        }
        MediaPlatform::Spotify => {
            format!("https://open.spotify.com/oembed?url={}", percent_encode(trimmed))
        }
        _ => return None,
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .user_agent("StudyNodes/0.1 (Desktop study app)")
        .build()
        .ok()?;
    let resp = client.get(&endpoint).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let val: serde_json::Value = resp.json().await.ok()?;
    val.get("title")
        .and_then(|t| t.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_cue_timestamp(raw: &str) -> Option<f64> {
    let clean = raw
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or("")
        .replace(',', ".");
    if clean.is_empty() {
        return None;
    }
    parse_timestamp(&clean)
}

fn strip_subtitle_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Parse `.srt`, `.vtt`, or plain-text transcripts into ordered `SubtitleCue` segments.
pub fn parse_srt_or_vtt(raw: &str) -> Vec<SubtitleCue> {
    let normalized = raw.trim_start_matches('\u{FEFF}').replace("\r\n", "\n");
    let mut cues = Vec::new();
    for block in normalized.split("\n\n") {
        let lines: Vec<&str> = block
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if lines.is_empty() || lines[0].starts_with("WEBVTT") || lines[0].starts_with("NOTE") {
            continue;
        }
        let Some(arrow_pos) = lines.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        let timing = lines[arrow_pos];
        let Some((lhs, rhs)) = timing.split_once("-->") else {
            continue;
        };
        let Some(start) = parse_cue_timestamp(lhs) else {
            continue;
        };
        let end = parse_cue_timestamp(rhs)
            .unwrap_or(start + 3.5)
            .max(start);
        let txt = lines[arrow_pos + 1..]
            .iter()
            .map(|l| strip_subtitle_markup(l))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !txt.is_empty() {
            cues.push(SubtitleCue { start, end, text: txt });
        }
    }
    cues
}

pub fn cues_to_plain_text(cues: &[SubtitleCue]) -> String {
    cues.iter()
        .map(|c| c.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn cues_to_timed_text(cues: &[SubtitleCue]) -> String {
    cues.iter()
        .filter(|c| !c.text.trim().is_empty())
        .map(|c| format!("[{}] {}", format_timestamp(c.start), c.text.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn active_subtitle_cue(cues: &[SubtitleCue], current_secs: f64) -> Option<(usize, &SubtitleCue)> {
    if cues.is_empty() {
        return None;
    }
    let t = if current_secs.is_finite() {
        current_secs.max(0.0)
    } else {
        0.0
    };
    if let Some((idx, cue)) = cues
        .iter()
        .enumerate()
        .find(|(_, c)| t >= c.start - 0.15 && t <= c.end + 0.15)
    {
        return Some((idx, cue));
    }
    if t <= 0.1 {
        return Some((0, &cues[0]));
    }
    cues.iter()
        .enumerate()
        .rev()
        .find(|(_, c)| c.start <= t)
        .or_else(|| Some((0, &cues[0])))
}

pub fn find_companion_subtitle_file(audio_path: &Path) -> Option<PathBuf> {
    let parent = audio_path.parent()?;
    let stem = audio_path.file_stem()?.to_str()?;
    let full = audio_path.file_name()?.to_str()?;
    for candidate in [
        format!("{}.srt", stem),
        format!("{}.vtt", stem),
        format!("{}.srt", full),
        format!("{}.vtt", full),
    ] {
        let p = parent.join(candidate);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

#[derive(Clone, Debug)]
pub struct DownloadedImage {
    pub bytes: Vec<u8>,
    pub ext: String,
    pub caption: String,
    pub source_url: String,
}

pub fn detect_image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF] {
        Some("jpg")
    } else if bytes.len() >= 6 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

async fn collect_wikimedia_candidates(
    client: &reqwest::Client,
    query: &str,
    out: &mut Vec<(String, String, String)>,
) {
    let enc = percent_encode(query);
    let url = format!(
        "https://commons.wikimedia.org/w/api.php?action=query&generator=search&gsrnamespace=6&gsrsearch={}&gsrlimit=6&prop=imageinfo&iiprop=url|mime&iiurlwidth=1200&format=json",
        enc
    );
    let Ok(resp) = client.get(&url).send().await else {
        return;
    };
    if !resp.status().is_success() {
        return;
    }
    let Ok(val) = resp.json::<serde_json::Value>().await else {
        return;
    };
    if let Some(pages) = val["query"]["pages"].as_object() {
        for page in pages.values() {
            let title = page["title"]
                .as_str()
                .unwrap_or("Wikimedia image")
                .trim_start_matches("File:")
                .to_string();
            if let Some(info) = page["imageinfo"].as_array().and_then(|a| a.first()) {
                let mime = info["mime"].as_str().unwrap_or("");
                if mime.contains("pdf") || mime.contains("tiff") || mime.contains("djvu") {
                    continue;
                }
                let pick = info["thumburl"]
                    .as_str()
                    .or_else(|| info["url"].as_str())
                    .map(str::trim)
                    .filter(|u| !u.is_empty());
                let page_url = info["descriptionurl"]
                    .as_str()
                    .or(pick)
                    .unwrap_or("")
                    .to_string();
                if let Some(img_u) = pick {
                    out.push((img_u.to_string(), title, page_url));
                }
            }
        }
    }
}

async fn collect_wikipedia_candidates(
    client: &reqwest::Client,
    query: &str,
    out: &mut Vec<(String, String, String)>,
) {
    let enc = percent_encode(query);
    let url = format!(
        "https://en.wikipedia.org/w/api.php?action=query&generator=search&gsrsearch={}&gsrlimit=5&prop=pageimages&piprop=thumbnail&pithumbsize=1200&format=json",
        enc
    );
    let Ok(resp) = client.get(&url).send().await else {
        return;
    };
    if !resp.status().is_success() {
        return;
    }
    let Ok(val) = resp.json::<serde_json::Value>().await else {
        return;
    };
    if let Some(pages) = val["query"]["pages"].as_object() {
        for page in pages.values() {
            let title = page["title"].as_str().unwrap_or("Wikipedia image").to_string();
            if let Some(img_u) = page["thumbnail"]["source"].as_str().map(str::trim).filter(|u| !u.is_empty()) {
                out.push((img_u.to_string(), title, img_u.to_string()));
            }
        }
    }
}

async fn collect_openverse_candidates(
    client: &reqwest::Client,
    query: &str,
    out: &mut Vec<(String, String, String)>,
) {
    let enc = percent_encode(query);
    let url = format!(
        "https://api.openverse.org/v1/images/?q={}&page_size=5",
        enc
    );
    let Ok(resp) = client.get(&url).send().await else {
        return;
    };
    if !resp.status().is_success() {
        return;
    }
    let Ok(val) = resp.json::<serde_json::Value>().await else {
        return;
    };
    if let Some(results) = val["results"].as_array() {
        for item in results {
            let title = item["title"].as_str().unwrap_or("Openverse image").to_string();
            let img_u = item["url"]
                .as_str()
                .or_else(|| item["thumbnail"].as_str())
                .map(str::trim)
                .filter(|u| !u.is_empty());
            let src_u = item["foreign_landing_url"]
                .as_str()
                .or(img_u)
                .unwrap_or("")
                .to_string();
            if let Some(u) = img_u {
                out.push((u.to_string(), title, src_u));
            }
        }
    }
}

/// Search public web repositories (Wikimedia Commons, Wikipedia, Openverse) and download valid
/// raster image bytes (`png`/`jpg`/`webp`/`gif`).
pub async fn search_and_download_images(
    query: &str,
    limit: usize,
) -> Result<Vec<DownloadedImage>, String> {
    let clean_q = query.trim();
    if clean_q.is_empty() {
        return Err("Empty image search query".into());
    }
    let wanted = limit.clamp(1, 3);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("StudyNodes/0.1 (Desktop study app; contact@studynodes.local)")
        .build()
        .map_err(|e| e.to_string())?;

    let mut candidates: Vec<(String, String, String)> = Vec::new();
    collect_wikimedia_candidates(&client, clean_q, &mut candidates).await;
    if candidates.len() < wanted {
        collect_wikipedia_candidates(&client, clean_q, &mut candidates).await;
    }
    if candidates.len() < wanted {
        collect_openverse_candidates(&client, clean_q, &mut candidates).await;
    }

    if candidates.is_empty() {
        return Err(format!("No public images found for \"{}\"", clean_q));
    }

    let mut downloaded = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (img_url, title, source_url) in candidates {
        if !seen.insert(img_url.clone()) {
            continue;
        }
        let Ok(resp) = client.get(&img_url).send().await else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(bytes) = resp.bytes().await else {
            continue;
        };
        if bytes.len() > 15 * 1024 * 1024 || bytes.len() < 128 {
            continue;
        }
        let Some(ext) = detect_image_ext(&bytes) else {
            continue;
        };
        downloaded.push(DownloadedImage {
            bytes: bytes.to_vec(),
            ext: ext.to_string(),
            caption: title,
            source_url: if source_url.is_empty() {
                img_url
            } else {
                source_url
            },
        });
        if downloaded.len() >= wanted {
            break;
        }
    }

    if downloaded.is_empty() {
        Err(format!("Could not download usable raster images for \"{}\"", clean_q))
    } else {
        Ok(downloaded)
    }
}

/// Convert UNIX epoch days to civil `(year, month 1..=12, day 1..=31)` (Howard Hinnant algorithm).
pub fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as u32, d as u32)
}

/// Local-approximate current UTC date (`(year, month 1..=12, day 1..=31)`).
pub fn today_ymd() -> (i32, u32, u32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    civil_from_days(secs.div_euclid(86_400))
}

pub fn format_ymd(year: i32, month: u32, day: u32) -> String {
    format!("{:04}-{:02}-{:02}", year, month.clamp(1, 12), day.clamp(1, 31))
}

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Day of week of `(year, month, 1)` where `0 = Mon, 1 = Tue, ..., 6 = Sun`.
pub fn weekday_of_first(year: i32, month: u32) -> u32 {
    let y = year as i64 - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = month as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days_since_unix = era * 146_097 + (doe as i64) - 719_468;
    // 1970-01-01 was Thursday (index 3 when Monday = 0)
    (days_since_unix + 3).rem_euclid(7) as u32
}

pub fn step_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let total = year * 12 + (month.clamp(1, 12) as i32 - 1) + delta;
    let new_year = total.div_euclid(12);
    let new_month = (total.rem_euclid(12) + 1) as u32;
    (new_year, new_month)
}

pub fn parse_ymd(raw: &str) -> Option<(i32, u32, u32)> {
    let trimmed = raw.trim();
    let mut parts = trimmed.split('-');
    let y = parts.next()?.parse::<i32>().ok()?;
    let m = parts.next()?.parse::<u32>().ok()?;
    let d = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) {
        return None;
    }
    let dim = days_in_month(y, m);
    if d == 0 || d > dim {
        return None;
    }
    Some((y, m, d))
}

/// Convert civil `(year, month 1..=12, day 1..=31)` to UNIX epoch days (inverse of `civil_from_days`).
pub fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = year as i64 - if month <= 2 { 1 } else { 0 };
    let m = month as i64;
    let d = day as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as u64 + 2) / 5 + (d as u64) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + (doe as i64) - 719_468
}

pub fn add_days_ymd(date: &str, delta_days: i64) -> String {
    let (y, m, d) = parse_ymd(date).unwrap_or_else(today_ymd);
    let (ny, nm, nd) = civil_from_days(days_from_civil(y, m, d) + delta_days);
    format_ymd(ny, nm, nd)
}

pub fn monday_of_week(date: &str) -> String {
    let (y, m, d) = parse_ymd(date).unwrap_or_else(today_ymd);
    let days = days_from_civil(y, m, d);
    // 1970-01-01 was Thursday (index 3 when Mon = 0 .. Sun = 6)
    let wd = (days + 3).rem_euclid(7);
    let (my, mm, md) = civil_from_days(days - wd);
    format_ymd(my, mm, md)
}

pub fn week_dates(anchor_ymd: &str) -> [(String, &'static str, u32); 7] {
    const NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let mon = monday_of_week(anchor_ymd);
    let (y, m, d) = parse_ymd(&mon).unwrap_or_else(today_ymd);
    let mon_days = days_from_civil(y, m, d);
    std::array::from_fn(|i| {
        let (dy, dm, dd) = civil_from_days(mon_days + i as i64);
        (format_ymd(dy, dm, dd), NAMES[i], dd)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_formatting_and_parsing() {
        assert_eq!(format_timestamp(0.0), "00:00");
        assert_eq!(format_timestamp(75.0), "01:15");
        assert_eq!(format_timestamp(3723.0), "1:02:03");

        assert_eq!(parse_timestamp("90"), Some(90.0));
        assert_eq!(parse_timestamp("01:30"), Some(90.0));
        assert_eq!(parse_timestamp("1:02:03"), Some(3723.0));
        assert_eq!(parse_timestamp("invalid"), None);
    }

    #[test]
    fn test_media_platform_and_timestamp_jump_urls() {
        assert_eq!(
            timestamp_jump_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=5s", 125.0),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=125s"
        );
        assert_eq!(
            timestamp_jump_url("https://youtu.be/dQw4w9WgXcQ", 42.0),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s"
        );
        assert_eq!(
            timestamp_jump_url("https://vimeo.com/123456", 33.0),
            "https://vimeo.com/123456#t=33s"
        );
        assert_eq!(
            timestamp_jump_url("https://open.spotify.com/episode/abc123?si=xyz", 80.0),
            "https://open.spotify.com/episode/abc123?si=xyz&t=80"
        );
    }

    #[test]
    fn test_calendar_date_math_and_leap_years() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2025, 2), 28);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);

        // 2026-11-01 is a Sunday (index 6 in Mon=0..Sun=6)
        assert_eq!(weekday_of_first(2026, 11), 6);
        assert_eq!(step_month(2026, 12, 1), (2027, 1));
        assert_eq!(step_month(2026, 1, -1), (2025, 12));
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn test_week_date_math_and_civil_roundtrip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(days_from_civil(2026, 11, 21)), (2026, 11, 21));
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));

        // 2026-11-21 is Saturday -> Monday of that week is 2026-11-16
        assert_eq!(monday_of_week("2026-11-21"), "2026-11-16");
        assert_eq!(add_days_ymd("2026-11-16", 7), "2026-11-23");
        assert_eq!(add_days_ymd("2026-03-01", -1), "2026-02-28");

        let wk = week_dates("2026-11-21");
        assert_eq!(wk[0], ("2026-11-16".to_string(), "Mon", 16));
        assert_eq!(wk[5], ("2026-11-21".to_string(), "Sat", 21));
        assert_eq!(wk[6], ("2026-11-22".to_string(), "Sun", 22));
    }

    #[test]
    fn test_iframe_embed_urls_subtitles_and_image_sniff() {
        assert_eq!(
            embed_iframe_url("https://youtu.be/dQw4w9WgXcQ", Some(45)).as_deref(),
            Some("https://www.youtube.com/embed/dQw4w9WgXcQ?rel=0&playsinline=1&modestbranding=1&start=45&autoplay=1")
        );
        assert_eq!(
            embed_iframe_url("https://vimeo.com/123456", None).as_deref(),
            Some("https://player.vimeo.com/video/123456")
        );
        assert_eq!(
            embed_iframe_url("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC?si=abc", None).as_deref(),
            Some("https://open.spotify.com/embed/track/4uLU6hMCjMI75M1A2tKUQC?si=abc")
        );
        assert_eq!(
            embed_iframe_url("https://example.com/clip.mp4", None),
            None
        );

        let html = embed_iframe_html("https://www.youtube.com/embed/abc", "Lecture 1");
        assert!(html.contains("<iframe src=\"https://www.youtube.com/embed/abc\""));
        assert!(html.contains("referrerpolicy=\"strict-origin-when-cross-origin\""));

        let srt = "1\n00:00:01,000 --> 00:00:04,000\n<i>First</i> line.\n\n2\n00:00:05,500 --> 00:00:09,000\nSecond line.\n";
        let cues = parse_srt_or_vtt(srt);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text, "First line.");
        assert_eq!(cues_to_plain_text(&cues), "First line. Second line.");
        assert!(cues_to_timed_text(&cues).contains("[00:06] Second line."));
        assert_eq!(active_subtitle_cue(&cues, 2.0).map(|(i, _)| i), Some(0));
        assert_eq!(active_subtitle_cue(&cues, 6.0).map(|(i, _)| i), Some(1));

        assert_eq!(detect_image_ext(b"\x89PNG\r\n\x1a\nxxxx"), Some("png"));
        assert_eq!(detect_image_ext(b"\xFF\xD8\xFF\xE0xxxx"), Some("jpg"));
        assert_eq!(detect_image_ext(b"<html>not image</html>"), None);
    }
}
