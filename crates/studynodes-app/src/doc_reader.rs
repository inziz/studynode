use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;
use std::process::Command;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReaderMode {
    #[default]
    Integrated,
    CustomExternal,
}

fn default_inspector_width() -> f32 {
    460.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReaderPreferences {
    pub mode: ReaderMode,
    /// Empty string means OS default application; non-empty launches the configured binary.
    pub custom_reader_path: String,
    /// User-adjusted docked right panel width in logical points.
    #[serde(default = "default_inspector_width")]
    pub inspector_width: f32,
}

impl Default for ReaderPreferences {
    fn default() -> Self {
        Self {
            mode: ReaderMode::Integrated,
            custom_reader_path: String::new(),
            inspector_width: default_inspector_width(),
        }
    }
}

pub fn load_preferences(projects_dir: &Path) -> ReaderPreferences {
    let path = projects_dir.join("reader_prefs.json");
    if let Ok(raw) = fs::read_to_string(path) {
        if let Ok(prefs) = serde_json::from_str::<ReaderPreferences>(&raw) {
            return prefs;
        }
    }
    ReaderPreferences::default()
}

pub fn save_preferences(projects_dir: &Path, prefs: &ReaderPreferences) -> Result<(), String> {
    let _ = fs::create_dir_all(projects_dir);
    let path = projects_dir.join("reader_prefs.json");
    let json = serde_json::to_string_pretty(prefs).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

pub fn launch_external_reader(file_path: &Path, custom_reader_path: &str) -> Result<String, String> {
    let custom = custom_reader_path.trim();
    if !custom.is_empty() {
        Command::new(custom)
            .arg(file_path)
            .spawn()
            .map_err(|e| format!("Failed to launch '{}': {}", custom, e))?;
        let exe_name = Path::new(custom)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(custom);
        return Ok(format!("Opened in custom reader '{}'", exe_name));
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", &file_path.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("Failed to launch default Windows viewer: {}", e))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(file_path)
            .spawn()
            .map_err(|e| format!("Failed to open with macOS viewer: {}", e))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(file_path)
            .spawn()
            .map_err(|e| format!("Failed to open with xdg-open: {}", e))?;
    }

    Ok(format!(
        "Opened '{}' in system default reader",
        file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("document")
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocFormat {
    Pdf,
    Markdown,
    Docx,
    Odt,
    Pptx,
    Xlsx,
    Epub,
    Html,
    Rtf,
    Csv,
    Tsv,
    Image,
    Code(&'static str),
    PlainText,
}

pub fn is_supported_image_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "ico" | "svg" | "svgz"
    )
}

pub fn detect_format(path: &Path) -> DocFormat {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "pdf" => DocFormat::Pdf,
        "md" | "markdown" | "mdx" => DocFormat::Markdown,
        "docx" => DocFormat::Docx,
        "odt" => DocFormat::Odt,
        "pptx" => DocFormat::Pptx,
        "xlsx" => DocFormat::Xlsx,
        "epub" => DocFormat::Epub,
        "html" | "htm" | "xhtml" => DocFormat::Html,
        "rtf" => DocFormat::Rtf,
        "csv" => DocFormat::Csv,
        "tsv" => DocFormat::Tsv,
        e if is_supported_image_ext(e) => DocFormat::Image,
        "rs" => DocFormat::Code("rust"),
        "py" => DocFormat::Code("python"),
        "js" => DocFormat::Code("js"),
        "ts" => DocFormat::Code("ts"),
        "tsx" => DocFormat::Code("tsx"),
        "json" => DocFormat::Code("json"),
        "toml" => DocFormat::Code("toml"),
        "yaml" | "yml" => DocFormat::Code("yaml"),
        "sql" => DocFormat::Code("sql"),
        "sh" | "bash" => DocFormat::Code("bash"),
        "css" => DocFormat::Code("css"),
        "xml" => DocFormat::Code("xml"),
        "tex" => DocFormat::Code("latex"),
        _ => DocFormat::PlainText,
    }
}

pub fn format_badge(fmt: DocFormat) -> &'static str {
    match fmt {
        DocFormat::Pdf => "PDF",
        DocFormat::Markdown => "MD",
        DocFormat::Docx => "DOCX",
        DocFormat::Odt => "ODT",
        DocFormat::Pptx => "PPTX",
        DocFormat::Xlsx => "XLSX",
        DocFormat::Epub => "EPUB",
        DocFormat::Html => "HTML",
        DocFormat::Rtf => "RTF",
        DocFormat::Csv => "CSV",
        DocFormat::Tsv => "TSV",
        DocFormat::Image => "IMG",
        DocFormat::Code(_) => "CODE",
        DocFormat::PlainText => "TXT",
    }
}

pub fn wrap_for_preview(content: &str, fmt: DocFormat) -> String {
    match fmt {
        DocFormat::Code(lang) => format!("```{}\n{}\n```", lang, content.trim_end()),
        _ => content.to_string(),
    }
}

#[derive(Clone, Debug)]
pub struct ExtractedDoc {
    pub format: DocFormat,
    pub markdown: String,
    pub page_count: Option<usize>,
}

pub struct RenderedPdfPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub page_count: usize,
}

/// Render a PDF page purely in Rust via `hayro` (`vello_cpu`) into straight RGBA8 pixels.
pub fn render_pdf_page(path: &Path, page_index: usize, scale: f32) -> Result<RenderedPdfPage, String> {
    let bytes = fs::read(path).map_err(|e| format!("Could not read PDF: {}", e))?;
    let pdf = Pdf::new(bytes).map_err(|e| format!("Invalid or encrypted PDF: {:?}", e))?;
    let pages = pdf.pages();
    let page_count = pages.len();
    if page_count == 0 {
        return Err("PDF contains 0 pages".into());
    }

    let clamped_idx = page_index.min(page_count - 1);
    let page = &pages[clamped_idx];
    let scale = scale.clamp(0.5, 3.0);

    let cache = RenderCache::new();
    let interp_settings = InterpreterSettings::default();
    let render_settings = RenderSettings::default();
    let pixmap_settings = PixmapSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: WHITE,
    };

    let pixmap = render(
        page,
        &cache,
        &interp_settings,
        &render_settings,
        &pixmap_settings,
    );

    let mut rgba = Vec::with_capacity(pixmap.data().len() * 4);
    for px in pixmap.data() {
        if px.a == 255 {
            rgba.extend_from_slice(&[px.r, px.g, px.b, 255]);
        } else if px.a == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let a = px.a as u16;
            let unpremul = |c: u8| -> u8 { ((c as u16 * 255 + a / 2) / a).min(255) as u8 };
            rgba.extend_from_slice(&[unpremul(px.r), unpremul(px.g), unpremul(px.b), px.a]);
        }
    }

    Ok(RenderedPdfPage {
        width: u32::from(pixmap.width()),
        height: u32::from(pixmap.height()),
        rgba,
        page_count,
    })
}

/// Extract structured Markdown from all supported standard study & document formats.
pub fn extract_document_markdown(path: &Path) -> Result<ExtractedDoc, String> {
    let format = detect_format(path);
    match format {
        DocFormat::Pdf => extract_pdf(path),
        DocFormat::Markdown => {
            let md = fs::read_to_string(path).map_err(|e| e.to_string())?;
            Ok(ExtractedDoc {
                format,
                markdown: md,
                page_count: None,
            })
        }
        DocFormat::Docx => extract_docx(path),
        DocFormat::Odt => extract_odt(path),
        DocFormat::Pptx => extract_pptx(path),
        DocFormat::Xlsx => extract_xlsx(path),
        DocFormat::Epub => extract_epub(path),
        DocFormat::Html => {
            let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
            Ok(ExtractedDoc {
                format,
                markdown: html_to_markdown(&raw),
                page_count: None,
            })
        }
        DocFormat::Rtf => {
            let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
            Ok(ExtractedDoc {
                format,
                markdown: rtf_to_markdown(&raw),
                page_count: None,
            })
        }
        DocFormat::Csv => {
            let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
            Ok(ExtractedDoc {
                format,
                markdown: delimited_to_markdown(&raw, ','),
                page_count: None,
            })
        }
        DocFormat::Tsv => {
            let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
            Ok(ExtractedDoc {
                format,
                markdown: delimited_to_markdown(&raw, '\t'),
                page_count: None,
            })
        }
        DocFormat::Image => {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Image");
            Ok(ExtractedDoc {
                format,
                markdown: format!("# {}\n\nVisual asset loaded into integrated preview.", name),
                page_count: None,
            })
        }
        DocFormat::Code(_) | DocFormat::PlainText => {
            let text = read_text_lossy(path)?;
            Ok(ExtractedDoc {
                format,
                markdown: text,
                page_count: None,
            })
        }
    }
}

fn read_text_lossy(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn extract_pdf(path: &Path) -> Result<ExtractedDoc, String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read PDF: {}", e))?;
    let page_count = Pdf::new(bytes).ok().map(|p| p.pages().len());

    let extracted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_extract::extract_text(path)
    }));

    let raw_text = match extracted {
        Ok(Ok(txt)) => txt,
        Ok(Err(e)) => {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("PDF");
            return Ok(ExtractedDoc {
                format: DocFormat::Pdf,
                markdown: format!(
                    "# {}\n\n> Could not extract text layer ({}). Use the **Pages** tab to view the visual PDF pages directly.",
                    name, e
                ),
                page_count,
            });
        }
        Err(_) => {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("PDF");
            return Ok(ExtractedDoc {
                format: DocFormat::Pdf,
                markdown: format!(
                    "# {}\n\n> PDF text extractor skipped non-standard stream. Switch to the **Pages** tab for visual rendering.",
                    name
                ),
                page_count,
            });
        }
    };

    let normalized = clean_extracted_text(&raw_text);
    let markdown = if normalized.is_empty() {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("PDF");
        format!(
            "# {}\n\n> No searchable text found (likely a scanned PDF). Switch to the **Pages** tab to inspect rendered pages.",
            name
        )
    } else {
        normalized
    };

    Ok(ExtractedDoc {
        format: DocFormat::Pdf,
        markdown,
        page_count,
    })
}

fn clean_extracted_text(raw: &str) -> String {
    let mut out = Vec::new();
    let mut empty_run = 0usize;
    for line in raw.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            empty_run += 1;
            if empty_run <= 1 && !out.is_empty() {
                out.push(String::new());
            }
        } else {
            empty_run = 0;
            out.push(trimmed.to_string());
        }
    }
    out.join("\n").trim().to_string()
}

fn open_zip(path: &Path) -> Result<zip::ZipArchive<BufReader<File>>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open archive: {}", e))?;
    zip::ZipArchive::new(BufReader::new(file))
        .map_err(|e| format!("Invalid ZIP document archive: {}", e))
}

fn read_zip_entry(archive: &mut zip::ZipArchive<BufReader<File>>, name: &str) -> Result<String, String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|e| format!("Missing '{}' in archive: {}", name, e))?;
    let mut s = String::new();
    entry
        .read_to_string(&mut s)
        .map_err(|e| format!("Failed to read '{}': {}", name, e))?;
    Ok(s)
}

fn resolve_general_ref(e: &quick_xml::events::BytesRef<'_>) -> String {
    if let Ok(Some(ch)) = e.resolve_char_ref() {
        return ch.to_string();
    }
    match e.decode().as_deref() {
        Ok("lt") => "<".into(),
        Ok("gt") => ">".into(),
        Ok("amp") => "&".into(),
        Ok("quot") => "\"".into(),
        Ok("apos") => "'".into(),
        Ok("nbsp") => " ".into(),
        _ => String::new(),
    }
}

fn xml_attr(tag: &quick_xml::events::BytesStart<'_>, target: &[u8]) -> Option<String> {
    for attr in tag.attributes().flatten() {
        let key = attr.key.as_ref();
        if key == target || key.strip_suffix(target).is_some_and(|p| p.ends_with(b":")) {
            if let Ok(val) = attr.normalized_value(quick_xml::XmlVersion::Explicit1_0) {
                return Some(val.into_owned());
            }
        }
    }
    None
}

fn local_name(bytes: &[u8]) -> String {
    let raw = std::str::from_utf8(bytes).unwrap_or("");
    raw.rsplit(':').next().unwrap_or(raw).to_string()
}

fn extract_docx(path: &Path) -> Result<ExtractedDoc, String> {
    let mut archive = open_zip(path)?;
    let xml = read_zip_entry(&mut archive, "word/document.xml")?;

    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut blocks: Vec<String> = Vec::new();
    let mut para_buf = String::new();
    let mut heading_level: Option<u8> = None;
    let mut is_list = false;
    let mut in_table = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut in_cell = false;
    let mut cell_buf = String::new();
    let mut in_text = false;
    let mut run_bold = false;
    let mut run_italic = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => match local_name(e.name().as_ref()).as_str() {
                "tbl" => {
                    in_table = true;
                    table_rows.clear();
                }
                "tr" if in_table => current_row.clear(),
                "tc" if in_table => {
                    in_cell = true;
                    cell_buf.clear();
                }
                "p" if !in_cell => {
                    para_buf.clear();
                    heading_level = None;
                    is_list = false;
                }
                "p" if in_cell => {
                    if !cell_buf.is_empty() && !cell_buf.ends_with(' ') {
                        cell_buf.push(' ');
                    }
                }
                "r" => {
                    run_bold = false;
                    run_italic = false;
                }
                "t" => in_text = true,
                _ => {}
            },
            Ok(Event::Empty(ref e)) => match local_name(e.name().as_ref()).as_str() {
                "pStyle" if !in_cell => {
                    if let Some(style) = xml_attr(e, b"val") {
                        let lower = style.to_ascii_lowercase();
                        if lower == "title" {
                            heading_level = Some(1);
                        } else if lower == "subtitle" {
                            heading_level = Some(2);
                        } else if let Some(rest) = lower.strip_prefix("heading") {
                            if let Ok(lvl) = rest.trim().parse::<u8>() {
                                heading_level = Some(lvl.clamp(1, 6));
                            }
                        }
                    }
                }
                "numPr" | "ilvl" if !in_cell => is_list = true,
                "b" => run_bold = true,
                "i" => run_italic = true,
                "br" | "cr" => {
                    if in_cell {
                        cell_buf.push(' ');
                    } else {
                        para_buf.push('\n');
                    }
                }
                "tab" => {
                    if in_cell {
                        cell_buf.push(' ');
                    } else {
                        para_buf.push_str("  ");
                    }
                }
                _ => {}
            },
            Ok(Event::Text(e)) if in_text => {
                if let Ok(t) = e.decode() {
                    let s = t.as_ref();
                    if in_cell {
                        cell_buf.push_str(s);
                    } else if !s.trim().is_empty() && (run_bold || run_italic) {
                        if run_bold {
                            para_buf.push_str("**");
                            para_buf.push_str(s);
                            para_buf.push_str("**");
                        } else {
                            para_buf.push('*');
                            para_buf.push_str(s);
                            para_buf.push('*');
                        }
                    } else {
                        para_buf.push_str(s);
                    }
                }
            }
            Ok(Event::GeneralRef(e)) if in_text => {
                let resolved = resolve_general_ref(&e);
                if in_cell {
                    cell_buf.push_str(&resolved);
                } else {
                    para_buf.push_str(&resolved);
                }
            }
            Ok(Event::End(ref e)) => match local_name(e.name().as_ref()).as_str() {
                "t" => in_text = false,
                "r" => {
                    run_bold = false;
                    run_italic = false;
                }
                "p" if !in_cell => {
                    let line = para_buf.trim();
                    if !line.is_empty() {
                        if let Some(lvl) = heading_level {
                            let hashes = "#".repeat(lvl as usize);
                            blocks.push(format!("{} {}", hashes, line));
                        } else if is_list {
                            blocks.push(format!("- {}", line));
                        } else {
                            blocks.push(line.to_string());
                        }
                    }
                }
                "tc" if in_table => {
                    in_cell = false;
                    current_row.push(cell_buf.trim().replace('|', "\\|"));
                }
                "tr" if in_table => {
                    if !current_row.is_empty() {
                        table_rows.push(std::mem::take(&mut current_row));
                    }
                }
                "tbl" => {
                    in_table = false;
                    if let Some(tbl_md) = rows_to_gfm_table(&table_rows) {
                        blocks.push(tbl_md);
                    }
                    table_rows.clear();
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    Ok(ExtractedDoc {
        format: DocFormat::Docx,
        markdown: blocks.join("\n\n"),
        page_count: None,
    })
}

fn extract_odt(path: &Path) -> Result<ExtractedDoc, String> {
    let mut archive = open_zip(path)?;
    let xml = read_zip_entry(&mut archive, "content.xml")?;

    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut blocks = Vec::new();
    let mut buf = String::new();
    let mut heading_lvl: Option<u8> = None;
    let mut in_list_item = false;
    let mut in_block = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => match local_name(e.name().as_ref()).as_str() {
                "list-item" => in_list_item = true,
                "h" => {
                    in_block = true;
                    buf.clear();
                    let lvl = xml_attr(e, b"outline-level")
                        .and_then(|v| v.parse::<u8>().ok())
                        .unwrap_or(1)
                        .clamp(1, 6);
                    heading_lvl = Some(lvl);
                }
                "p" => {
                    in_block = true;
                    buf.clear();
                    heading_lvl = None;
                }
                _ => {}
            },
            Ok(Event::Empty(ref e)) => match local_name(e.name().as_ref()).as_str() {
                "line-break" if in_block => buf.push('\n'),
                "tab" if in_block => buf.push_str("  "),
                "s" if in_block => buf.push(' '),
                _ => {}
            },
            Ok(Event::Text(e)) if in_block => {
                if let Ok(t) = e.decode() {
                    buf.push_str(t.as_ref());
                }
            }
            Ok(Event::GeneralRef(e)) if in_block => {
                buf.push_str(&resolve_general_ref(&e));
            }
            Ok(Event::End(ref e)) => match local_name(e.name().as_ref()).as_str() {
                "h" | "p" => {
                    in_block = false;
                    let text = buf.trim();
                    if !text.is_empty() {
                        if let Some(lvl) = heading_lvl.take() {
                            blocks.push(format!("{} {}", "#".repeat(lvl as usize), text));
                        } else if in_list_item {
                            blocks.push(format!("- {}", text));
                        } else {
                            blocks.push(text.to_string());
                        }
                    }
                }
                "list-item" => in_list_item = false,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    Ok(ExtractedDoc {
        format: DocFormat::Odt,
        markdown: blocks.join("\n\n"),
        page_count: None,
    })
}

fn extract_pptx(path: &Path) -> Result<ExtractedDoc, String> {
    let mut archive = open_zip(path)?;
    let mut slide_names: Vec<(usize, String)> = Vec::new();

    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            if let Ok(name) = entry.name() {
                let name = name.into_owned();
                if let Some(rest) = name.strip_prefix("ppt/slides/slide") {
                    if let Some(num_str) = rest.strip_suffix(".xml") {
                        if let Ok(num) = num_str.parse::<usize>() {
                            slide_names.push((num, name));
                        }
                    }
                }
            }
        }
    }
    slide_names.sort_by_key(|(num, _)| *num);

    let mut sections = Vec::new();
    for (num, name) in &slide_names {
        if let Ok(xml) = read_zip_entry(&mut archive, name) {
            let mut reader = quick_xml::Reader::from_str(&xml);
            let mut paras = Vec::new();
            let mut p_buf = String::new();
            let mut in_t = false;

            loop {
                match reader.read_event() {
                    Ok(Event::Start(ref e)) => match local_name(e.name().as_ref()).as_str() {
                        "p" => p_buf.clear(),
                        "t" => in_t = true,
                        _ => {}
                    },
                    Ok(Event::Text(e)) if in_t => {
                        if let Ok(t) = e.decode() {
                            p_buf.push_str(t.as_ref());
                        }
                    }
                    Ok(Event::GeneralRef(e)) if in_t => {
                        p_buf.push_str(&resolve_general_ref(&e));
                    }
                    Ok(Event::End(ref e)) => match local_name(e.name().as_ref()).as_str() {
                        "t" => in_t = false,
                        "p" => {
                            let line = p_buf.trim();
                            if !line.is_empty() {
                                if paras.is_empty() {
                                    paras.push(format!("### {}", line));
                                } else {
                                    paras.push(format!("- {}", line));
                                }
                            }
                        }
                        _ => {}
                    },
                    Ok(Event::Eof) => break,
                    Err(_) => break,
                    _ => {}
                }
            }

            if !paras.is_empty() {
                sections.push(format!("## Slide {}\n\n{}", num, paras.join("\n")));
            }
        }
    }

    Ok(ExtractedDoc {
        format: DocFormat::Pptx,
        markdown: sections.join("\n\n---\n\n"),
        page_count: Some(slide_names.len()),
    })
}

fn extract_xlsx(path: &Path) -> Result<ExtractedDoc, String> {
    let mut archive = open_zip(path)?;
    let shared_strings = read_zip_entry(&mut archive, "xl/sharedStrings.xml")
        .ok()
        .map(|xml| parse_xlsx_shared_strings(&xml))
        .unwrap_or_default();

    let mut sheet_names: Vec<(usize, String)> = Vec::new();
    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            if let Ok(name) = entry.name() {
                let name = name.into_owned();
                if let Some(rest) = name.strip_prefix("xl/worksheets/sheet") {
                    if let Some(num_str) = rest.strip_suffix(".xml") {
                        if let Ok(num) = num_str.parse::<usize>() {
                            sheet_names.push((num, name));
                        }
                    }
                }
            }
        }
    }
    sheet_names.sort_by_key(|(num, _)| *num);

    let mut sections = Vec::new();
    for (num, name) in &sheet_names {
        if let Ok(xml) = read_zip_entry(&mut archive, name) {
            let rows = parse_xlsx_sheet_rows(&xml, &shared_strings);
            if let Some(table_md) = rows_to_gfm_table(&rows) {
                sections.push(format!("## Sheet {}\n\n{}", num, table_md));
            }
        }
    }

    Ok(ExtractedDoc {
        format: DocFormat::Xlsx,
        markdown: sections.join("\n\n"),
        page_count: Some(sheet_names.len()),
    })
}

fn parse_xlsx_shared_strings(xml: &str) -> Vec<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut strings = Vec::new();
    let mut cur = String::new();
    let mut in_t = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => match local_name(e.name().as_ref()).as_str() {
                "si" => cur.clear(),
                "t" => in_t = true,
                _ => {}
            },
            Ok(Event::Text(e)) if in_t => {
                if let Ok(t) = e.decode() {
                    cur.push_str(t.as_ref());
                }
            }
            Ok(Event::GeneralRef(e)) if in_t => {
                cur.push_str(&resolve_general_ref(&e));
            }
            Ok(Event::End(ref e)) => match local_name(e.name().as_ref()).as_str() {
                "t" => in_t = false,
                "si" => strings.push(cur.clone()),
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    strings
}

fn parse_xlsx_sheet_rows(xml: &str, shared: &[String]) -> Vec<Vec<String>> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut cur_row: Vec<String> = Vec::new();
    let mut cell_type = String::new();
    let mut val_buf = String::new();
    let mut in_val = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => match local_name(e.name().as_ref()).as_str() {
                "row" => cur_row.clear(),
                "c" => {
                    cell_type = xml_attr(e, b"t").unwrap_or_default();
                    val_buf.clear();
                }
                "v" | "t" => in_val = true,
                _ => {}
            },
            Ok(Event::Text(e)) if in_val => {
                if let Ok(t) = e.decode() {
                    val_buf.push_str(t.as_ref());
                }
            }
            Ok(Event::GeneralRef(e)) if in_val => {
                val_buf.push_str(&resolve_general_ref(&e));
            }
            Ok(Event::End(ref e)) => match local_name(e.name().as_ref()).as_str() {
                "v" | "t" => in_val = false,
                "c" => {
                    let cell = if cell_type == "s" {
                        val_buf
                            .trim()
                            .parse::<usize>()
                            .ok()
                            .and_then(|idx| shared.get(idx))
                            .cloned()
                            .unwrap_or_else(|| val_buf.trim().to_string())
                    } else {
                        val_buf.trim().to_string()
                    };
                    cur_row.push(cell.replace('|', "\\|"));
                }
                "row" => {
                    if !cur_row.is_empty() && rows.len() < 400 {
                        rows.push(std::mem::take(&mut cur_row));
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    rows
}

fn extract_epub(path: &Path) -> Result<ExtractedDoc, String> {
    let mut archive = open_zip(path)?;
    let container_xml = read_zip_entry(&mut archive, "META-INF/container.xml")?;
    let opf_path = parse_epub_container_opf(&container_xml)
        .ok_or_else(|| "EPUB missing rootfile full-path in container.xml".to_string())?;
    let opf_xml = read_zip_entry(&mut archive, &opf_path)?;

    let opf_dir = Path::new(&opf_path)
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();

    let spine_hrefs = parse_epub_opf_spine(&opf_xml);
    let mut chapters = Vec::new();
    for href in &spine_hrefs {
        let full_href = if opf_dir.is_empty() {
            href.clone()
        } else {
            format!("{}/{}", opf_dir, href)
        };
        if let Ok(xhtml) = read_zip_entry(&mut archive, &full_href) {
            let md = html_to_markdown(&xhtml);
            if !md.trim().is_empty() {
                chapters.push(md);
            }
        }
    }

    Ok(ExtractedDoc {
        format: DocFormat::Epub,
        markdown: chapters.join("\n\n---\n\n"),
        page_count: Some(spine_hrefs.len()),
    })
}

fn parse_epub_container_opf(xml: &str) -> Option<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                if local_name(e.name().as_ref()) == "rootfile" {
                    if let Some(full_path) = xml_attr(e, b"full-path") {
                        return Some(full_path);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    None
}

fn parse_epub_opf_spine(xml: &str) -> Vec<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut manifest: HashMap<String, String> = HashMap::new();
    let mut spine_ids: Vec<String> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                match local_name(e.name().as_ref()).as_str() {
                    "item" => {
                        if let (Some(id), Some(href)) = (xml_attr(e, b"id"), xml_attr(e, b"href")) {
                            manifest.insert(id, href);
                        }
                    }
                    "itemref" => {
                        if let Some(idref) = xml_attr(e, b"idref") {
                            spine_ids.push(idref);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    spine_ids
        .into_iter()
        .filter_map(|id| manifest.remove(&id))
        .collect()
}

pub fn html_to_markdown(html: &str) -> String {
    let mut out = String::new();
    let mut chars = html.chars().peekable();
    let mut skip_depth = 0usize;
    let mut in_pre = false;

    while let Some(c) = chars.next() {
        if c == '<' {
            let mut tag_raw = String::new();
            while let Some(&nc) = chars.peek() {
                chars.next();
                if nc == '>' {
                    break;
                }
                tag_raw.push(nc);
            }

            let trimmed = tag_raw.trim();
            if trimmed.starts_with('!') || trimmed.starts_with('?') {
                continue;
            }

            let is_end = trimmed.starts_with('/');
            let name = trimmed
                .trim_start_matches('/')
                .split(|ch: char| ch.is_whitespace() || ch == '/')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();

            match name.as_str() {
                "script" | "style" | "head" | "title" => {
                    if is_end {
                        skip_depth = skip_depth.saturating_sub(1);
                    } else if !trimmed.ends_with('/') {
                        skip_depth += 1;
                    }
                }
                _ if skip_depth > 0 => {}
                "h1" => push_heading_marker(&mut out, is_end, "# "),
                "h2" => push_heading_marker(&mut out, is_end, "## "),
                "h3" => push_heading_marker(&mut out, is_end, "### "),
                "h4" => push_heading_marker(&mut out, is_end, "#### "),
                "h5" | "h6" => push_heading_marker(&mut out, is_end, "##### "),
                "p" | "div" | "section" | "article" | "tr" => {
                    if !out.ends_with("\n\n") && !out.is_empty() {
                        out.push_str("\n\n");
                    }
                }
                "br" => out.push('\n'),
                "hr" => out.push_str("\n\n---\n\n"),
                "li" => {
                    if !is_end {
                        if !out.ends_with('\n') && !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str("- ");
                    } else if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
                "blockquote" => {
                    if !is_end {
                        if !out.ends_with("\n\n") && !out.is_empty() {
                            out.push_str("\n\n");
                        }
                        out.push_str("> ");
                    } else {
                        out.push_str("\n\n");
                    }
                }
                "pre" => {
                    if !is_end {
                        in_pre = true;
                        out.push_str("\n\n```\n");
                    } else {
                        in_pre = false;
                        out.push_str("\n```\n\n");
                    }
                }
                "code" if !in_pre => out.push('`'),
                "strong" | "b" => out.push_str("**"),
                "em" | "i" => out.push('*'),
                _ => {}
            }
        } else if skip_depth > 0 {
            continue;
        } else if c == '&' {
            let mut entity = String::new();
            let mut closed = false;
            while let Some(&nc) = chars.peek() {
                if nc == ';' {
                    chars.next();
                    closed = true;
                    break;
                }
                if entity.len() > 10 || nc.is_whitespace() || nc == '<' {
                    break;
                }
                entity.push(nc);
                chars.next();
            }
            if closed {
                out.push_str(&decode_html_entity(&entity));
            } else {
                out.push('&');
                out.push_str(&entity);
            }
        } else if in_pre {
            out.push(c);
        } else if c == '\r' {
            continue;
        } else if c.is_whitespace() {
            if !out.ends_with([' ', '\n', '\t']) && !out.is_empty() {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }

    clean_extracted_text(&out)
}

fn push_heading_marker(out: &mut String, is_end: bool, prefix: &str) {
    if !is_end {
        if !out.ends_with("\n\n") && !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(prefix);
    } else if !out.ends_with("\n\n") {
        out.push_str("\n\n");
    }
}

fn decode_html_entity(entity: &str) -> String {
    match entity {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" | "#39" | "#x27" => "'".into(),
        "nbsp" => " ".into(),
        "mdash" => "—".into(),
        "ndash" => "–".into(),
        s if s.starts_with("#x") || s.starts_with("#X") => u32::from_str_radix(&s[2..], 16)
            .ok()
            .and_then(char::from_u32)
            .map(|ch| ch.to_string())
            .unwrap_or_default(),
        s if s.starts_with('#') => s[1..]
            .parse::<u32>()
            .ok()
            .and_then(char::from_u32)
            .map(|ch| ch.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

pub fn rtf_to_markdown(rtf: &str) -> String {
    let bytes = rtf.as_bytes();
    let mut i = 0usize;
    let mut depth = 0i32;
    let mut skip_until_depth: Option<i32> = None;
    let mut uc_skip = 1usize;
    let mut depth_uc = Vec::new();
    let mut out = String::new();

    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'{' => {
                depth += 1;
                depth_uc.push(uc_skip);
                i += 1;
            }
            b'}' => {
                if let Some(target) = skip_until_depth {
                    if depth == target {
                        skip_until_depth = None;
                    }
                }
                uc_skip = depth_uc.pop().unwrap_or(1);
                depth = (depth - 1).max(0);
                i += 1;
            }
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                let next = bytes[i];
                match next {
                    b'\\' | b'{' | b'}' => {
                        if skip_until_depth.is_none() {
                            out.push(next as char);
                        }
                        i += 1;
                    }
                    b'*' => {
                        if skip_until_depth.is_none() {
                            skip_until_depth = Some(depth);
                        }
                        i += 1;
                    }
                    b'\'' => {
                        if i + 2 < bytes.len() {
                            if let Ok(hex_str) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                                if let Ok(val) = u8::from_str_radix(hex_str, 16) {
                                    if skip_until_depth.is_none() {
                                        out.push(val as char);
                                    }
                                }
                            }
                            i += 3;
                        } else {
                            i += 1;
                        }
                    }
                    b'~' => {
                        if skip_until_depth.is_none() {
                            out.push(' ');
                        }
                        i += 1;
                    }
                    b'\r' | b'\n' => {
                        if skip_until_depth.is_none() {
                            out.push('\n');
                        }
                        i += 1;
                    }
                    _ if next.is_ascii_alphabetic() => {
                        let start = i;
                        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        let word = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                        let param_start = i;
                        if i < bytes.len() && (bytes[i] == b'-' || bytes[i].is_ascii_digit()) {
                            i += 1;
                            while i < bytes.len() && bytes[i].is_ascii_digit() {
                                i += 1;
                            }
                        }
                        let param = std::str::from_utf8(&bytes[param_start..i])
                            .ok()
                            .and_then(|s| s.parse::<i32>().ok());
                        if i < bytes.len() && bytes[i] == b' ' {
                            i += 1;
                        }

                        if matches!(
                            word,
                            "fonttbl" | "colortbl" | "stylesheet" | "info" | "pict" | "object"
                        ) && skip_until_depth.is_none()
                        {
                            skip_until_depth = Some(depth);
                        } else if word == "uc" {
                            if let Some(n) = param {
                                uc_skip = n.max(0) as usize;
                            }
                        } else if skip_until_depth.is_none() {
                            match word {
                                "par" | "line" => out.push('\n'),
                                "tab" => out.push_str("  "),
                                "u" => {
                                    if let Some(code) = param {
                                        let ucode = if code < 0 {
                                            (code + 65536) as u32
                                        } else {
                                            code as u32
                                        };
                                        if let Some(ch) = char::from_u32(ucode) {
                                            out.push(ch);
                                        }
                                        // Skip `uc_skip` replacement characters.
                                        let mut skipped = 0;
                                        while i < bytes.len() && skipped < uc_skip {
                                            if bytes[i] == b'\\' || bytes[i] == b'{' || bytes[i] == b'}' {
                                                break;
                                            }
                                            i += 1;
                                            skipped += 1;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => i += 1,
                }
            }
            b'\r' | b'\n' => i += 1,
            _ => {
                if skip_until_depth.is_none() {
                    out.push(b as char);
                }
                i += 1;
            }
        }
    }

    clean_extracted_text(&out)
}

pub fn delimited_to_markdown(raw: &str, sep: char) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        rows.push(parse_csv_row(line, sep));
        if rows.len() >= 400 {
            break;
        }
    }
    rows_to_gfm_table(&rows).unwrap_or_default()
}

fn parse_csv_row(line: &str, sep: char) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '"' {
            if in_quotes && chars.peek() == Some(&'"') {
                chars.next();
                cur.push('"');
            } else {
                in_quotes = !in_quotes;
            }
        } else if c == sep && !in_quotes {
            cells.push(cur.trim().replace('|', "\\|"));
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    cells.push(cur.trim().replace('|', "\\|"));
    cells
}

fn rows_to_gfm_table(rows: &[Vec<String>]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if cols == 0 {
        return None;
    }

    let format_row = |row: &[String]| -> String {
        let mut cells = Vec::with_capacity(cols);
        for i in 0..cols {
            cells.push(row.get(i).cloned().unwrap_or_default());
        }
        format!("| {} |", cells.join(" | "))
    };

    let mut lines = Vec::with_capacity(rows.len() + 1);
    lines.push(format_row(&rows[0]));
    lines.push(format!("| {} |", vec!["---"; cols].join(" | ")));
    for row in &rows[1..] {
        lines.push(format_row(row));
    }
    Some(lines.join("\n"))
}

/// Toggle the `task_index`-th Markdown checkbox (`- [ ]` <-> `- [x]`) in-place.
pub fn toggle_markdown_task(src: &str, task_index: usize) -> String {
    let mut seen = 0usize;
    let mut lines: Vec<String> = Vec::new();
    let had_trailing_newline = src.ends_with('\n');

    for line in src.lines() {
        let trimmed = line.trim_start();
        let indent_len = line.len() - trimmed.len();
        let replaced = if let Some(rest) = trimmed
            .strip_prefix("- [ ] ")
            .or_else(|| trimmed.strip_prefix("* [ ] "))
        {
            if seen == task_index {
                seen += 1;
                let marker = &trimmed[..2];
                Some(format!("{}{}[x] {}", &line[..indent_len], marker, rest))
            } else {
                seen += 1;
                None
            }
        } else if let Some(rest) = trimmed
            .strip_prefix("- [x] ")
            .or_else(|| trimmed.strip_prefix("- [X] "))
            .or_else(|| trimmed.strip_prefix("* [x] "))
            .or_else(|| trimmed.strip_prefix("* [X] "))
        {
            if seen == task_index {
                seen += 1;
                let marker = &trimmed[..2];
                Some(format!("{}{}[ ] {}", &line[..indent_len], marker, rest))
            } else {
                seen += 1;
                None
            }
        } else {
            None
        };

        lines.push(replaced.unwrap_or_else(|| line.to_string()));
    }

    let mut out = lines.join("\n");
    if had_trailing_newline && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextPreviewPage {
    pub heading: String,
    pub body: String,
}

fn clean_preview_line(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("```") {
        let lang = trimmed.trim_start_matches('`').trim();
        return if lang.is_empty() {
            "[code]".to_string()
        } else {
            format!("[{}]", lang)
        };
    }
    let without_heading = trimmed.trim_start_matches('#').trim_start();
    let without_quote = without_heading
        .strip_prefix("> ")
        .unwrap_or(without_heading);
    let cleaned = without_quote
        .replace("**", "")
        .replace("__", "")
        .replace('`', "");
    // Strip markdown italic wrappers (`*word*`) without touching leading bullet `* `.
    if let Some(rest) = cleaned.strip_prefix("* ") {
        format!("- {}", rest.replace('*', ""))
    } else {
        cleaned.replace('*', "")
    }
}

/// Split Markdown or extracted document text into selectable preview pages for canvas cards.
pub fn split_document_preview_pages(markdown: &str, max_lines: usize) -> Vec<TextPreviewPage> {
    let max_lines = max_lines.max(4);
    let max_chars = 360usize;

    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    let mut cur_heading = String::new();
    let mut cur_lines: Vec<String> = Vec::new();

    for raw_line in markdown.lines() {
        let trimmed = raw_line.trim();
        if let Some(h) = trimmed
            .strip_prefix("# ")
            .or_else(|| trimmed.strip_prefix("## "))
        {
            if !cur_heading.is_empty() || !cur_lines.is_empty() {
                sections.push((cur_heading.clone(), std::mem::take(&mut cur_lines)));
            }
            cur_heading = clean_preview_line(h);
        } else {
            let cleaned = clean_preview_line(trimmed);
            if !cleaned.is_empty() {
                cur_lines.push(cleaned);
            }
        }
    }
    if !cur_heading.is_empty() || !cur_lines.is_empty() {
        sections.push((cur_heading, cur_lines));
    }

    if sections.is_empty() {
        return vec![TextPreviewPage {
            heading: "Empty Document".into(),
            body: "Click to ingest a PDF, Office doc, or Markdown file.".into(),
        }];
    }

    let mut pages = Vec::new();
    for (sec_idx, (heading, lines)) in sections.into_iter().enumerate() {
        let base_heading = if heading.is_empty() {
            format!("Section {}", sec_idx + 1)
        } else {
            heading
        };

        if lines.is_empty() {
            pages.push(TextPreviewPage {
                heading: base_heading,
                body: "(Section header)".into(),
            });
            continue;
        }

        let mut chunk: Vec<String> = Vec::new();
        let mut chunk_chars = 0usize;
        let mut part = 0usize;

        for line in lines {
            let next_chars = chunk_chars + line.len() + 1;
            if !chunk.is_empty() && (chunk.len() >= max_lines || next_chars > max_chars) {
                let h = if part == 0 {
                    base_heading.clone()
                } else {
                    format!("{} (cont.)", base_heading)
                };
                pages.push(TextPreviewPage {
                    heading: h,
                    body: chunk.join("\n"),
                });
                chunk.clear();
                chunk_chars = 0;
                part += 1;
            }
            chunk_chars += line.len() + 1;
            chunk.push(line);
        }

        if !chunk.is_empty() {
            let h = if part == 0 {
                base_heading
            } else {
                format!("{} (cont.)", base_heading)
            };
            pages.push(TextPreviewPage {
                heading: h,
                body: chunk.join("\n"),
            });
        }
    }

    if pages.is_empty() {
        pages.push(TextPreviewPage {
            heading: "Empty Document".into(),
            body: "Click to ingest a PDF, Office doc, or Markdown file.".into(),
        });
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    fn write_zip_entries(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts = SimpleFileOptions::default();
        for (name, data) in entries {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }

    #[test]
    fn test_extract_docx_odt_pptx_xlsx_epub_html_rtf_csv() {
        let dir = tempdir().unwrap();

        // DOCX
        let docx_path = dir.path().join("notes.docx");
        let docx_xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Neurobiology</w:t></w:r></w:p>
    <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>LTP</w:t></w:r><w:r><w:t> strengthens synapses.</w:t></w:r></w:p>
    <w:tbl><w:tr><w:tc><w:p><w:r><w:t>Receptor</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Ion</w:t></w:r></w:p></w:tc></w:tr>
    <w:tr><w:tc><w:p><w:r><w:t>NMDA</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Ca2+</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
  </w:body>
</w:document>"#;
        write_zip_entries(&docx_path, &[("word/document.xml", docx_xml)]);
        let docx_out = extract_document_markdown(&docx_path).unwrap();
        assert_eq!(docx_out.format, DocFormat::Docx);
        assert!(docx_out.markdown.contains("# Neurobiology"));
        assert!(docx_out.markdown.contains("**LTP** strengthens synapses."));
        assert!(docx_out.markdown.contains("| NMDA | Ca2+ |"));

        // ODT
        let odt_path = dir.path().join("summary.odt");
        let odt_xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
  <office:body><office:text>
    <text:h text:outline-level="2">Memory</text:h>
    <text:list><text:list-item><text:p>Consolidation occurs during sleep</text:p></text:list-item></text:list>
  </office:text></office:body>
</office:document-content>"#;
        write_zip_entries(&odt_path, &[("content.xml", odt_xml)]);
        let odt_out = extract_document_markdown(&odt_path).unwrap();
        assert_eq!(odt_out.format, DocFormat::Odt);
        assert!(odt_out.markdown.contains("## Memory"));
        assert!(odt_out.markdown.contains("- Consolidation occurs during sleep"));

        // PPTX
        let pptx_path = dir.path().join("lecture.pptx");
        let slide1 = br#"<?xml version="1.0" encoding="UTF-8"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Spike Timing</a:t></a:r></a:p><a:p><a:r><a:t>Causal window &lt; 20ms</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld>
</p:sld>"#;
        write_zip_entries(&pptx_path, &[("ppt/slides/slide1.xml", slide1)]);
        let pptx_out = extract_document_markdown(&pptx_path).unwrap();
        assert_eq!(pptx_out.format, DocFormat::Pptx);
        assert!(pptx_out.markdown.contains("## Slide 1"));
        assert!(pptx_out.markdown.contains("Causal window < 20ms"));

        // XLSX
        let xlsx_path = dir.path().join("metrics.xlsx");
        let shared = br#"<?xml version="1.0" encoding="UTF-8"?><sst><si><t>Gene</t></si><si><t>CREB</t></si></sst>"#;
        let sheet1 = br#"<?xml version="1.0" encoding="UTF-8"?><worksheet><sheetData><row><c t="s"><v>0</v></c><c><v>Score</v></c></row><row><c t="s"><v>1</v></c><c><v>98</v></c></row></sheetData></worksheet>"#;
        write_zip_entries(
            &xlsx_path,
            &[
                ("xl/sharedStrings.xml", shared),
                ("xl/worksheets/sheet1.xml", sheet1),
            ],
        );
        let xlsx_out = extract_document_markdown(&xlsx_path).unwrap();
        assert_eq!(xlsx_out.format, DocFormat::Xlsx);
        assert!(xlsx_out.markdown.contains("| CREB | 98 |"));

        // EPUB
        let epub_path = dir.path().join("book.epub");
        let container = br#"<?xml version="1.0"?><container version="1.0"><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#;
        let opf = br#"<?xml version="1.0"?><package><manifest><item id="ch1" href="ch1.xhtml"/></manifest><spine><itemref idref="ch1"/></spine></package>"#;
        let ch1 = br#"<html><body><h1>Chapter One</h1><p>Neural assemblies &amp; rhythms.</p></body></html>"#;
        write_zip_entries(
            &epub_path,
            &[
                ("META-INF/container.xml", container),
                ("OEBPS/content.opf", opf),
                ("OEBPS/ch1.xhtml", ch1),
            ],
        );
        let epub_out = extract_document_markdown(&epub_path).unwrap();
        assert_eq!(epub_out.format, DocFormat::Epub);
        assert!(epub_out.markdown.contains("# Chapter One"));
        assert!(epub_out.markdown.contains("Neural assemblies & rhythms."));

        // RTF
        let rtf_path = dir.path().join("memo.rtf");
        fs::write(&rtf_path, r#"{\rtf1\ansi{\fonttbl{\f0 Arial;}}Synaptic \par plasticity}"#).unwrap();
        let rtf_out = extract_document_markdown(&rtf_path).unwrap();
        assert_eq!(rtf_out.format, DocFormat::Rtf);
        assert!(rtf_out.markdown.contains("Synaptic\nplasticity"));

        // CSV
        let csv_path = dir.path().join("data.csv");
        fs::write(&csv_path, "Model,Context\n\"Gemini, Flash\",1000000\n").unwrap();
        let csv_out = extract_document_markdown(&csv_path).unwrap();
        assert_eq!(csv_out.format, DocFormat::Csv);
        assert!(csv_out.markdown.contains("| Gemini, Flash | 1000000 |"));
    }

    #[test]
    fn test_hayro_renders_minimal_pdf_page() {
        let dir = tempdir().unwrap();
        let pdf_path = dir.path().join("sample.pdf");

        // Build a compact valid PDF 1.4 with accurate byte xref offsets.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"%PDF-1.4\n");
        let off1 = buf.len();
        buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        let off2 = buf.len();
        buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
        let off3 = buf.len();
        buf.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 120] /Contents 4 0 R >>\nendobj\n");
        let off4 = buf.len();
        let stream = b"0.1 0.1 0.1 rg 20 20 160 80 re f";
        buf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", stream.len()).as_bytes());
        buf.extend_from_slice(stream);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        let xref_off = buf.len();
        buf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
        for off in [off1, off2, off3, off4] {
            buf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
        }
        buf.extend_from_slice(format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

        fs::write(&pdf_path, buf).unwrap();
        let rendered = render_pdf_page(&pdf_path, 0, 1.0).unwrap();
        assert_eq!(rendered.page_count, 1);
        assert_eq!(rendered.width, 200);
        assert_eq!(rendered.height, 120);
        assert_eq!(rendered.rgba.len(), 200 * 120 * 4);
    }

    #[test]
    fn test_toggle_markdown_task_flips_checkbox() {
        let md = "- [x] Done\n- [ ] Pending\n";
        let toggled = toggle_markdown_task(md, 1);
        assert_eq!(toggled, "- [x] Done\n- [x] Pending\n");
        let toggled_back = toggle_markdown_task(&toggled, 0);
        assert_eq!(toggled_back, "- [ ] Done\n- [x] Pending\n");
    }

    #[test]
    fn test_split_document_preview_pages() {
        let empty = split_document_preview_pages("   \n ", 6);
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0].heading, "Empty Document");

        let md = "# Intro\nFirst line\nSecond line\n## Methods\nStep 1\nStep 2\nStep 3\nStep 4\nStep 5\n";
        let pages = split_document_preview_pages(md, 4);
        assert!(pages.len() >= 3, "expected at least 3 pages, got {:?}", pages);
        assert_eq!(pages[0].heading, "Intro");
        assert!(pages[0].body.contains("First line"));
        assert_eq!(pages[1].heading, "Methods");
        assert_eq!(pages[2].heading, "Methods (cont.)");
    }
}
