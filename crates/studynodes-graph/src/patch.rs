use std::collections::{HashMap, HashSet, VecDeque};
use serde::{Deserialize, Serialize};

use crate::coords::WorldPos;
use crate::graph::StudyGraph;
use crate::model::{
    resolve_trace_color_hex, resolve_trace_color_label, CalendarEvent, CalendarViewMode, Edge,
    EdgeEndpoint, EdgeId, EdgeRelation, MediaTimestamp, Node, NodeId, NodeKind, PortId,
    QuizQuestion,
};
#[cfg(test)]
use crate::model::{EdgeProvenance, EdgeStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SemanticRelation {
    Supports,
    Contradicts,
    DerivedFrom,
    #[default]
    Elaborates,
    PrerequisiteFor,
    ExampleOf,
    Custom,
}

impl<'de> Deserialize<'de> for SemanticRelation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = Option::<String>::deserialize(deserializer)?.unwrap_or_default();
        let norm = raw
            .trim()
            .to_ascii_lowercase()
            .replace(['-', ' '], "_");
        Ok(match norm.as_str() {
            "supports" | "support" | "validates" => SemanticRelation::Supports,
            "contradicts" | "contradict" | "refutes" => SemanticRelation::Contradicts,
            "derived_from" | "derivedfrom" | "derives_from" => SemanticRelation::DerivedFrom,
            "elaborates" | "elaborate" | "relates_to" | "relatesto" | "explains" | "explain"
            | "" => SemanticRelation::Elaborates,
            "prerequisite_for" | "prerequisitefor" | "prerequisite" => {
                SemanticRelation::PrerequisiteFor
            }
            "example_of" | "exampleof" | "example" => SemanticRelation::ExampleOf,
            _ => SemanticRelation::Custom,
        })
    }
}

fn default_confidence() -> f32 {
    0.85
}

fn default_study_kind() -> NodeKind {
    NodeKind::Flashcard
}

fn default_one() -> usize {
    1
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i32, month: u32) -> u32 {
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

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = year as i64 - if month <= 2 { 1 } else { 0 };
    let m = month as i64;
    let d = day as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as u64 + 2) / 5 + (d as u64) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + (doe as i64) - 719_468
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
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

fn format_ymd(year: i32, month: u32, day: u32) -> String {
    format!("{:04}-{:02}-{:02}", year, month.clamp(1, 12), day.clamp(1, 31))
}

fn parse_month_name_token(tok: &str) -> Option<u32> {
    let clean = tok
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_ascii_lowercase();
    match clean.as_str() {
        "jan" | "january" | "ene" | "enero" => Some(1),
        "feb" | "february" | "febrero" => Some(2),
        "mar" | "march" | "marzo" => Some(3),
        "apr" | "april" | "abr" | "abril" => Some(4),
        "may" | "mayo" => Some(5),
        "jun" | "june" | "junio" => Some(6),
        "jul" | "july" | "julio" => Some(7),
        "aug" | "august" | "ago" | "agosto" => Some(8),
        "sep" | "sept" | "september" | "septiembre" => Some(9),
        "oct" | "october" | "octubre" => Some(10),
        "nov" | "november" | "noviembre" => Some(11),
        "dec" | "december" | "dic" | "diciembre" => Some(12),
        _ => None,
    }
}

fn parse_weekday_token(raw: &str) -> Option<i64> {
    let clean = raw.trim().to_ascii_lowercase();
    match clean.as_str() {
        "mon" | "monday" | "lun" | "lunes" => Some(0),
        "tue" | "tues" | "tuesday" | "mar" | "martes" => Some(1),
        "wed" | "wednesday" | "mie" | "mié" | "miercoles" | "miércoles" => Some(2),
        "thu" | "thur" | "thurs" | "thursday" | "jue" | "jueves" => Some(3),
        "fri" | "friday" | "vie" | "viernes" => Some(4),
        "sat" | "saturday" | "sab" | "sáb" | "sabado" | "sábado" => Some(5),
        "sun" | "sunday" | "dom" | "domingo" => Some(6),
        _ => None,
    }
}

fn validate_ymd(y: i32, m: u32, d: u32) -> Option<(i32, u32, u32)> {
    if !(1900..=2999).contains(&y) || !(1..=12).contains(&m) {
        return None;
    }
    let dim = days_in_month(y, m);
    if d == 0 || d > dim {
        return None;
    }
    Some((y, m, d))
}

fn today_epoch_days() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    secs.div_euclid(86_400)
}

pub fn monday_of_week_ymd(ymd: &str) -> Option<String> {
    let mut parts = ymd.trim().split('-');
    let y = parts.next()?.parse::<i32>().ok()?;
    let m = parts.next()?.parse::<u32>().ok()?;
    let d = parts.next()?.parse::<u32>().ok()?;
    let (_, _, _) = validate_ymd(y, m, d)?;
    let days = days_from_civil(y, m, d);
    let wd = (days + 3).rem_euclid(7);
    let (my, mm, md) = civil_from_days(days - wd);
    Some(format_ymd(my, mm, md))
}

/// Format `"YYYY-MM-DD"` as `"YYYY-MM-DD (Weekday) · Current Week (Mon..Sun): ..."` for LLM prompts.
pub fn format_date_context_line(ymd: &str) -> Option<String> {
    const FULL_NAMES: [&str; 7] = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    const SHORT_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let mut parts = ymd.trim().split('-');
    let y = parts.next()?.parse::<i32>().ok()?;
    let m = parts.next()?.parse::<u32>().ok()?;
    let d = parts.next()?.parse::<u32>().ok()?;
    let (_, _, _) = validate_ymd(y, m, d)?;
    let days = days_from_civil(y, m, d);
    let wd = (days + 3).rem_euclid(7) as usize;
    let mon_days = days - wd as i64;
    let week_items: Vec<String> = (0..7)
        .map(|i| {
            let (wy, wm, wdd) = civil_from_days(mon_days + i as i64);
            format!("{} {}", SHORT_NAMES[i], format_ymd(wy, wm, wdd))
        })
        .collect();
    Some(format!(
        "{} ({})\nCurrent Week (Mon-Sun): {}",
        ymd.trim(),
        FULL_NAMES[wd],
        week_items.join(", ")
    ))
}

/// Normalize AI-supplied date strings (`YYYY-MM-DD`, ISO timestamps, `DD/MM/YYYY`, month names,
/// `"today"`, `"tomorrow"`, `"lunes"`, `"Monday"`) into canonical `"YYYY-MM-DD"`.
pub fn normalize_calendar_date(raw: &str, reference_ymd: Option<&str>) -> Option<String> {
    let ref_days = reference_ymd
        .and_then(|r| {
            let mut it = r.trim().split('-');
            let y = it.next()?.parse::<i32>().ok()?;
            let m = it.next()?.parse::<u32>().ok()?;
            let d = it.next()?.parse::<u32>().ok()?;
            validate_ymd(y, m, d).map(|(yy, mm, dd)| days_from_civil(yy, mm, dd))
        })
        .unwrap_or_else(today_epoch_days);

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. Relative English / Spanish day words
    let lower = trimmed.to_ascii_lowercase();
    if matches!(lower.as_str(), "today" | "hoy") {
        let (y, m, d) = civil_from_days(ref_days);
        return Some(format_ymd(y, m, d));
    }
    if matches!(lower.as_str(), "tomorrow" | "mañana" | "manana") {
        let (y, m, d) = civil_from_days(ref_days + 1);
        return Some(format_ymd(y, m, d));
    }
    if matches!(lower.as_str(), "yesterday" | "ayer") {
        let (y, m, d) = civil_from_days(ref_days - 1);
        return Some(format_ymd(y, m, d));
    }

    // 2. Leading ISO or slash date, e.g. "2026-11-23T18:00", "2026/11/5", "23/11/2026"
    let head = trimmed
        .split(['T', ' ', '('])
        .next()
        .unwrap_or(trimmed)
        .trim();
    let sep_nums: Vec<u32> = head
        .split(['-', '/'])
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u32>().ok())
        .collect();
    if sep_nums.len() == 3 {
        let (a, b, c) = (sep_nums[0], sep_nums[1], sep_nums[2]);
        if (1900..=2999).contains(&a) {
            if let Some((y, m, d)) = validate_ymd(a as i32, b, c) {
                return Some(format_ymd(y, m, d));
            }
        } else if (1900..=2999).contains(&c) {
            // Interpret DD/MM/YYYY when a > 12 (else DD/MM/YYYY default)
            if let Some((y, m, d)) = validate_ymd(c as i32, b, a) {
                return Some(format_ymd(y, m, d));
            }
            if let Some((y, m, d)) = validate_ymd(c as i32, a, b) {
                return Some(format_ymd(y, m, d));
            }
        }
    }

    // 3. Weekday name (e.g. "Monday", "next Friday", "este lunes") -> next occurrence [0..6] days from ref
    let is_next_week = lower.contains("next ") || lower.contains("próximo") || lower.contains("proximo");
    for token in trimmed.split(|c: char| !c.is_alphabetic()) {
        if let Some(target_wd) = parse_weekday_token(token) {
            let ref_wd = (ref_days + 3).rem_euclid(7);
            let mut delta = (target_wd - ref_wd).rem_euclid(7);
            if is_next_week && delta == 0 {
                delta = 7;
            }
            let (y, m, d) = civil_from_days(ref_days + delta);
            return Some(format_ymd(y, m, d));
        }
    }

    // 4. Month name + day (+ optional year), e.g. "Nov 23, 2026" or "23 Nov"
    let mut found_month = None;
    let mut nums = Vec::new();
    for tok in trimmed.split_whitespace() {
        let clean = tok.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        if found_month.is_none() {
            if let Some(m) = parse_month_name_token(clean) {
                found_month = Some(m);
                continue;
            }
        }
        if let Ok(n) = clean.parse::<u32>() {
            nums.push(n);
        }
    }
    if let Some(m) = found_month {
        let (ref_y, _, _) = civil_from_days(ref_days);
        let year = nums
            .iter()
            .copied()
            .find(|&n| (1900..=2999).contains(&n))
            .map(|n| n as i32)
            .unwrap_or(ref_y);
        if let Some(day) = nums.iter().copied().find(|&n| (1..=31).contains(&n)) {
            if let Some((y, mm, dd)) = validate_ymd(year, m, day) {
                return Some(format_ymd(y, mm, dd));
            }
        }
    }

    None
}

fn deserialize_node_kind_forgiving<'de, D>(deserializer: D) -> Result<NodeKind, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<String>::deserialize(deserializer)?.unwrap_or_default();
    let norm = raw
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_', ' '], "");
    Ok(match norm.as_str() {
        "flashcard" | "card" => NodeKind::Flashcard,
        "checklist" | "todo" | "tasks" => NodeKind::Checklist,
        "formula" | "equation" | "math" | "code" => NodeKind::Formula,
        "graphformula" | "graphicformula" | "graph" | "plot" | "plotter" | "graphing"
        | "functiongraph" => NodeKind::GraphFormula,
        "excerpt" | "quote" | "citation" => NodeKind::Excerpt,
        "sequence" | "step" => NodeKind::Sequence,
        "section" | "group" | "frame" => NodeKind::Section,
        "video" | "youtube" | "vimeo" => NodeKind::Video,
        "audio" | "music" | "song" | "track" | "spotify" | "podcast" => NodeKind::Audio,
        "calendar" | "schedule" | "planner" | "agenda" => NodeKind::Calendar,
        "drawing" | "sketch" | "whiteboard" | "handwriting" => NodeKind::Drawing,
        "text" | "label" | "heading" | "textbox" => NodeKind::Text,
        "quiz" | "test" | "exam" | "mcq" | "questions" => NodeKind::Quiz,
        "concept" => NodeKind::Concept,
        "llmsynthesis" | "synthesis" => NodeKind::LlmSynthesis,
        "document" | "doc" => NodeKind::Document,
        "image" => NodeKind::Image,
        "note" => NodeKind::Note,
        _ => NodeKind::Flashcard,
    })
}

fn deserialize_string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        Vec(Vec<String>),
        Single(String),
        Null,
    }
    match Option::<StringOrVec>::deserialize(deserializer)? {
        Some(StringOrVec::Vec(v)) => Ok(v),
        Some(StringOrVec::Single(s)) if !s.trim().is_empty() => Ok(vec![s]),
        _ => Ok(Vec::new()),
    }
}

#[derive(Deserialize)]
struct RawQuizQuestion {
    #[serde(default)]
    id: String,
    #[serde(default, alias = "prompt", alias = "q")]
    question: String,
    #[serde(default, alias = "choices", alias = "answers")]
    options: Vec<String>,
    #[serde(default, alias = "correct", alias = "answer_index")]
    correct_index: Option<usize>,
    #[serde(default, alias = "explanation", alias = "solution")]
    answer: String,
    #[serde(default, alias = "hint", deserialize_with = "deserialize_string_or_vec")]
    hints: Vec<String>,
}

impl From<RawQuizQuestion> for QuizQuestion {
    fn from(raw: RawQuizQuestion) -> Self {
        let id = if raw.id.trim().is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            raw.id.trim().to_string()
        };
        let options: Vec<String> = raw
            .options
            .into_iter()
            .map(|o| o.trim().to_string())
            .filter(|o| !o.is_empty())
            .collect();
        let correct_index = raw
            .correct_index
            .filter(|&idx| idx < options.len());
        let hints: Vec<String> = raw
            .hints
            .into_iter()
            .map(|h| h.trim().to_string())
            .filter(|h| !h.is_empty())
            .collect();
        QuizQuestion {
            id,
            question: raw.question.trim().to_string(),
            options,
            correct_index,
            answer: raw.answer.trim().to_string(),
            hints,
        }
    }
}

fn deserialize_quiz_questions_forgiving<'de, D>(
    deserializer: D,
) -> Result<Vec<QuizQuestion>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let items = Option::<Vec<RawQuizQuestion>>::deserialize(deserializer)?.unwrap_or_default();
    Ok(items
        .into_iter()
        .map(QuizQuestion::from)
        .filter(|q| !q.question.is_empty())
        .collect())
}

fn deserialize_opt_quiz_questions_forgiving<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<QuizQuestion>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<Vec<RawQuizQuestion>>::deserialize(deserializer)?;
    Ok(opt.map(|items| {
        items
            .into_iter()
            .map(QuizQuestion::from)
            .filter(|q| !q.question.is_empty())
            .collect()
    }))
}

fn deserialize_seconds_forgiving<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = Option::<serde_json::Value>::deserialize(deserializer)?;
    match val {
        Some(serde_json::Value::Number(n)) => Ok(n.as_f64().unwrap_or(0.0).max(0.0)),
        Some(serde_json::Value::String(s)) => {
            let clean = s.trim().trim_end_matches('s').trim();
            if clean.is_empty() {
                return Ok(0.0);
            }
            if clean.contains(':') {
                let parts: Vec<&str> = clean.split(':').collect();
                let secs = match parts.len() {
                    2 => {
                        let m = parts[0].trim().parse::<f64>().unwrap_or(0.0);
                        let sc = parts[1].trim().parse::<f64>().unwrap_or(0.0);
                        m * 60.0 + sc
                    }
                    3 => {
                        let h = parts[0].trim().parse::<f64>().unwrap_or(0.0);
                        let m = parts[1].trim().parse::<f64>().unwrap_or(0.0);
                        let sc = parts[2].trim().parse::<f64>().unwrap_or(0.0);
                        h * 3600.0 + m * 60.0 + sc
                    }
                    _ => 0.0,
                };
                Ok(if secs.is_finite() { secs.max(0.0) } else { 0.0 })
            } else {
                let parsed = clean.parse::<f64>().unwrap_or(0.0);
                Ok(if parsed.is_finite() { parsed.max(0.0) } else { 0.0 })
            }
        }
        _ => Ok(0.0),
    }
}

fn deserialize_opt_pos_forgiving<'de, D>(deserializer: D) -> Result<Option<[f32; 2]>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(v) = val else {
        return Ok(None);
    };
    let coords = match v {
        serde_json::Value::Array(arr) if arr.len() >= 2 => {
            let x = arr[0].as_f64().map(|n| n as f32);
            let y = arr[1].as_f64().map(|n| n as f32);
            x.zip(y)
        }
        serde_json::Value::Object(map) => {
            let x = map.get("x").and_then(|v| v.as_f64()).map(|n| n as f32);
            let y = map.get("y").and_then(|v| v.as_f64()).map(|n| n as f32);
            x.zip(y)
        }
        _ => None,
    };
    Ok(coords
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(x, y)| [x, y]))
}

/// Target position (and optional size) override for a single node inside `PatchOp::ArrangeNodes`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NodePositionSpec {
    #[serde(default, alias = "id", alias = "target")]
    pub node_id: String,
    #[serde(default)]
    pub x: Option<f32>,
    #[serde(default)]
    pub y: Option<f32>,
    #[serde(default, alias = "position", deserialize_with = "deserialize_opt_pos_forgiving")]
    pub pos: Option<[f32; 2]>,
    #[serde(default, alias = "w")]
    pub width: Option<f32>,
    #[serde(default, alias = "h")]
    pub height: Option<f32>,
}

fn deserialize_positions_forgiving<'de, D>(
    deserializer: D,
) -> Result<Vec<NodePositionSpec>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(v) = val else {
        return Ok(Vec::new());
    };
    let mut specs = Vec::new();
    match v {
        serde_json::Value::Array(items) => {
            for item in items {
                if let Ok(spec) = serde_json::from_value::<NodePositionSpec>(item) {
                    if !spec.node_id.trim().is_empty() {
                        specs.push(spec);
                    }
                }
            }
        }
        serde_json::Value::Object(map) => {
            for (k, entry) in map {
                match entry {
                    serde_json::Value::Array(arr) if arr.len() >= 2 => {
                        if let (Some(x), Some(y)) = (
                            arr[0].as_f64().map(|n| n as f32),
                            arr[1].as_f64().map(|n| n as f32),
                        ) {
                            if x.is_finite() && y.is_finite() {
                                specs.push(NodePositionSpec {
                                    node_id: k,
                                    pos: Some([x, y]),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                    serde_json::Value::Object(_) => {
                        if let Ok(mut spec) = serde_json::from_value::<NodePositionSpec>(entry) {
                            if spec.node_id.trim().is_empty() {
                                spec.node_id = k;
                            }
                            if !spec.node_id.trim().is_empty() {
                                specs.push(spec);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    Ok(specs)
}

fn resolve_explicit_pos(
    x: Option<f32>,
    y: Option<f32>,
    pos: Option<[f32; 2]>,
) -> Option<WorldPos> {
    let base = pos.filter(|p| p[0].is_finite() && p[1].is_finite());
    let rx = x.filter(|v| v.is_finite()).or_else(|| base.map(|p| p[0]))?;
    let ry = y.filter(|v| v.is_finite()).or_else(|| base.map(|p| p[1]))?;
    Some(WorldPos::new(rx, ry))
}

fn merge_node_pos(
    cur: WorldPos,
    x: Option<f32>,
    y: Option<f32>,
    pos: Option<[f32; 2]>,
) -> Option<WorldPos> {
    let base = pos.filter(|p| p[0].is_finite() && p[1].is_finite());
    let nx = x.filter(|v| v.is_finite()).or_else(|| base.map(|p| p[0]));
    let ny = y.filter(|v| v.is_finite()).or_else(|| base.map(|p| p[1]));
    if nx.is_none() && ny.is_none() {
        return None;
    }
    Some(WorldPos::new(nx.unwrap_or(cur.x), ny.unwrap_or(cur.y)))
}

fn apply_optional_size(node: &mut Node, width: Option<f32>, height: Option<f32>) -> bool {
    let (cur_w, cur_h) = node.effective_size();
    let nw = width.filter(|v| v.is_finite()).map(|v| v.clamp(80.0, 2400.0));
    let nh = height
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(36.0, 1800.0));
    if nw.is_none() && nh.is_none() {
        return false;
    }
    node.size = (nw.unwrap_or(cur_w), nh.unwrap_or(cur_h));
    true
}

impl From<SemanticRelation> for EdgeRelation {
    fn from(r: SemanticRelation) -> Self {
        match r {
            SemanticRelation::Supports => EdgeRelation::Supports,
            SemanticRelation::Contradicts => EdgeRelation::Contradicts,
            SemanticRelation::DerivedFrom => EdgeRelation::DerivedFrom,
            SemanticRelation::Elaborates => EdgeRelation::RelatesTo,
            SemanticRelation::PrerequisiteFor => EdgeRelation::Custom("prerequisite for".into()),
            SemanticRelation::ExampleOf => EdgeRelation::ExampleOf,
            SemanticRelation::Custom => EdgeRelation::RelatesTo,
        }
    }
}

/// A validated, atomic batch of AI-proposed graph mutations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphPatch {
    /// Short high-level explanation of what this patch accomplishes (shown in UI toast/panel).
    #[serde(default)]
    pub summary: String,
    /// Ordered list of operations. `CreateSynthesisNode` must precede edges referencing its `temp_id`.
    #[serde(default, alias = "operations", alias = "patches")]
    pub ops: Vec<PatchOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PatchOp {
    /// Creates a "Ghost Edge" (status = Proposed, provenance = Llm) between two nodes.
    #[serde(
        alias = "add_edge",
        alias = "create_edge",
        alias = "connect",
        alias = "ProposeConnection"
    )]
    ProposeConnection {
        #[serde(default, alias = "from", alias = "source", alias = "source_node")]
        from_node: String,
        #[serde(default, alias = "to", alias = "target", alias = "target_node")]
        to_node: String,
        #[serde(default)]
        relation: SemanticRelation,
        #[serde(default)]
        label: String,
        #[serde(
            default,
            alias = "suggested_note",
            alias = "description",
            alias = "explanation",
            alias = "reason"
        )]
        note: String,
        #[serde(default, deserialize_with = "deserialize_string_or_vec")]
        evidence: Vec<String>,
        #[serde(default = "default_confidence")]
        confidence: f32,
        #[serde(default, alias = "color_hex")]
        color: Option<String>,
        #[serde(default, alias = "topic", alias = "relevance", alias = "category")]
        color_category: Option<String>,
    },

    /// Enriches or refines an existing edge's annotation without overwriting user-locked notes.
    #[serde(alias = "update_edge", alias = "AnnotateEdge")]
    AnnotateEdge {
        #[serde(default, alias = "id")]
        edge_id: String,
        #[serde(default)]
        relation: Option<SemanticRelation>,
        #[serde(default)]
        label: Option<String>,
        #[serde(
            default,
            alias = "note",
            alias = "annotation",
            alias = "description",
            alias = "explanation"
        )]
        suggested_note: String,
        #[serde(default, alias = "color_hex")]
        color: Option<String>,
        #[serde(default, alias = "topic", alias = "relevance", alias = "category")]
        color_category: Option<String>,
    },

    /// Sets a trace color (relevance or topic preset, `#RRGGBB` hex, or `"auto"`/`"default"` to clear) on an existing edge.
    #[serde(
        alias = "color_edge",
        alias = "set_trace_color",
        alias = "color_trace",
        alias = "SetEdgeColor"
    )]
    SetEdgeColor {
        #[serde(default, alias = "id")]
        edge_id: Option<String>,
        #[serde(default, alias = "from", alias = "source")]
        from_node: Option<String>,
        #[serde(default, alias = "to", alias = "target")]
        to_node: Option<String>,
        #[serde(default, alias = "color_hex")]
        color: String,
        #[serde(default, alias = "topic", alias = "relevance", alias = "color_category")]
        category: Option<String>,
    },

    /// Synthesizes multiple nodes into a new concept/summary node.
    #[serde(
        alias = "create_synthesis",
        alias = "synthesize",
        alias = "CreateSynthesisNode"
    )]
    CreateSynthesisNode {
        #[serde(default, alias = "id", alias = "node_id")]
        temp_id: String,
        #[serde(default)]
        title: String,
        #[serde(
            default,
            alias = "content",
            alias = "body",
            alias = "markdown",
            alias = "summary",
            alias = "text"
        )]
        markdown_body: String,
        #[serde(
            default,
            alias = "sources",
            alias = "source_node",
            deserialize_with = "deserialize_string_or_vec"
        )]
        source_nodes: Vec<String>,
        #[serde(default)]
        x: Option<f32>,
        #[serde(default)]
        y: Option<f32>,
        #[serde(
            default,
            alias = "position",
            deserialize_with = "deserialize_opt_pos_forgiving"
        )]
        pos: Option<[f32; 2]>,
        #[serde(default, alias = "w")]
        width: Option<f32>,
        #[serde(default, alias = "h")]
        height: Option<f32>,
    },

    /// Populates or updates the concise summary field on an existing node.
    #[serde(alias = "summarize_node", alias = "UpdateNodeSummary")]
    UpdateNodeSummary {
        #[serde(default, alias = "id")]
        node_id: String,
        #[serde(default, alias = "content", alias = "text", alias = "note")]
        summary: String,
    },

    /// Spawns a specialized study node (`Flashcard`, `Checklist`, `Formula`, `Excerpt`, `Sequence`, `Section`, `Video`, `Audio`, `Calendar`, `Note`).
    #[serde(
        alias = "create_node",
        alias = "add_node",
        alias = "create_card",
        alias = "CreateStudyNode"
    )]
    CreateStudyNode {
        #[serde(default, alias = "id", alias = "node_id")]
        temp_id: String,
        #[serde(
            default = "default_study_kind",
            deserialize_with = "deserialize_node_kind_forgiving"
        )]
        kind: NodeKind,
        #[serde(default)]
        title: String,
        #[serde(
            default,
            alias = "body",
            alias = "prompt",
            alias = "question",
            alias = "markdown_body",
            alias = "text"
        )]
        content: String,
        #[serde(default, alias = "back")]
        answer: Option<String>,
        #[serde(default, alias = "url", alias = "media", alias = "src")]
        media_url: Option<String>,
        #[serde(
            default,
            alias = "sources",
            alias = "source_node",
            deserialize_with = "deserialize_string_or_vec"
        )]
        source_nodes: Vec<String>,
        #[serde(
            default,
            alias = "questions",
            deserialize_with = "deserialize_quiz_questions_forgiving"
        )]
        quiz_questions: Vec<QuizQuestion>,
        #[serde(default)]
        text_font: Option<String>,
        #[serde(default)]
        text_size: Option<f32>,
        #[serde(default)]
        x: Option<f32>,
        #[serde(default)]
        y: Option<f32>,
        #[serde(
            default,
            alias = "position",
            deserialize_with = "deserialize_opt_pos_forgiving"
        )]
        pos: Option<[f32; 2]>,
        #[serde(default, alias = "w")]
        width: Option<f32>,
        #[serde(default, alias = "h")]
        height: Option<f32>,
    },

    /// Directly edits an existing node's title, body content, flashcard answer, summary, media URL, or quiz questions.
    #[serde(alias = "edit_node", alias = "modify_node", alias = "UpdateNode")]
    UpdateNode {
        #[serde(default, alias = "id")]
        node_id: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default, alias = "body", alias = "text", alias = "markdown_body")]
        content: Option<String>,
        #[serde(default, alias = "back")]
        answer: Option<String>,
        #[serde(default)]
        summary: Option<String>,
        #[serde(default, alias = "url", alias = "src")]
        media_url: Option<String>,
        #[serde(
            default,
            alias = "questions",
            deserialize_with = "deserialize_opt_quiz_questions_forgiving"
        )]
        quiz_questions: Option<Vec<QuizQuestion>>,
    },

    /// Appends a multiple-choice / study question with optional hints to a `Quiz` node (auto-creates one if needed).
    #[serde(
        alias = "add_question",
        alias = "append_quiz_question",
        alias = "AddQuizQuestion"
    )]
    AddQuizQuestion {
        #[serde(default, alias = "quiz_node_id", alias = "id")]
        node_id: Option<String>,
        #[serde(default, alias = "prompt", alias = "q")]
        question: String,
        #[serde(default, alias = "choices", alias = "answers")]
        options: Vec<String>,
        #[serde(default, alias = "correct", alias = "answer_index")]
        correct_index: Option<usize>,
        #[serde(default, alias = "explanation", alias = "solution")]
        answer: String,
        #[serde(default, alias = "hint", deserialize_with = "deserialize_string_or_vec")]
        hints: Vec<String>,
    },

    /// Modifies an existing question (by 0-based `question_index`) or appends a hint on a `Quiz` node.
    #[serde(
        alias = "edit_quiz_question",
        alias = "modify_quiz_question",
        alias = "UpdateQuizQuestion"
    )]
    UpdateQuizQuestion {
        #[serde(default, alias = "quiz_node_id", alias = "id")]
        node_id: String,
        #[serde(default, alias = "index")]
        question_index: usize,
        #[serde(default, alias = "prompt", alias = "q")]
        question: Option<String>,
        #[serde(default, alias = "choices", alias = "answers")]
        options: Option<Vec<String>>,
        #[serde(default, alias = "correct", alias = "answer_index")]
        correct_index: Option<usize>,
        #[serde(default, alias = "explanation", alias = "solution")]
        answer: Option<String>,
        #[serde(default)]
        hints: Option<Vec<String>>,
        #[serde(default)]
        add_hint: Option<String>,
    },

    /// Removes a question at 0-based `question_index` from a `Quiz` node.
    #[serde(alias = "delete_quiz_question", alias = "RemoveQuizQuestion")]
    RemoveQuizQuestion {
        #[serde(default, alias = "quiz_node_id", alias = "id")]
        node_id: String,
        #[serde(default, alias = "index")]
        question_index: usize,
    },

    /// Deletes a node (and cascade-removes its incident edges).
    #[serde(alias = "remove_node", alias = "DeleteNode")]
    DeleteNode {
        #[serde(default, alias = "id")]
        node_id: String,
    },

    /// Moves a node to explicit `(x, y)` / `pos`, relative `(dx, dy)` offset, or anchored relative to another node (`relative_to` + `placement`), optionally resizing it.
    #[serde(
        alias = "reposition_node",
        alias = "set_node_position",
        alias = "position_node",
        alias = "place_node",
        alias = "MoveNode"
    )]
    MoveNode {
        #[serde(default, alias = "id", alias = "target")]
        node_id: String,
        #[serde(default)]
        x: Option<f32>,
        #[serde(default)]
        y: Option<f32>,
        #[serde(
            default,
            alias = "position",
            deserialize_with = "deserialize_opt_pos_forgiving"
        )]
        pos: Option<[f32; 2]>,
        #[serde(default, alias = "delta_x", alias = "offset_x")]
        dx: Option<f32>,
        #[serde(default, alias = "delta_y", alias = "offset_y")]
        dy: Option<f32>,
        #[serde(default, alias = "anchor", alias = "relative_to_node")]
        relative_to: Option<String>,
        #[serde(default, alias = "direction", alias = "side")]
        placement: Option<String>,
        #[serde(default)]
        gap: Option<f32>,
        #[serde(default, alias = "w")]
        width: Option<f32>,
        #[serde(default, alias = "h")]
        height: Option<f32>,
    },

    /// Repositions multiple nodes at once via explicit coordinates (`positions`) and/or an automatic scheme `layout` (`"hierarchy"`, `"flow"`, `"radial"`, `"grid"`, `"row"`, `"column"`).
    #[serde(
        alias = "move_nodes",
        alias = "layout_nodes",
        alias = "reposition_nodes",
        alias = "organize_nodes",
        alias = "auto_layout",
        alias = "ArrangeNodes"
    )]
    ArrangeNodes {
        #[serde(
            default,
            alias = "moves",
            alias = "nodes",
            alias = "items",
            deserialize_with = "deserialize_positions_forgiving"
        )]
        positions: Vec<NodePositionSpec>,
        #[serde(default, alias = "style", alias = "mode", alias = "preset")]
        layout: Option<String>,
        #[serde(default, alias = "targets", deserialize_with = "deserialize_string_or_vec")]
        node_ids: Vec<String>,
        #[serde(default, alias = "root", alias = "center_node")]
        root_node: Option<String>,
        #[serde(default)]
        origin_x: Option<f32>,
        #[serde(default)]
        origin_y: Option<f32>,
        #[serde(default, alias = "h_gap", alias = "gap_x")]
        spacing_x: Option<f32>,
        #[serde(default, alias = "v_gap", alias = "gap_y")]
        spacing_y: Option<f32>,
        #[serde(default, alias = "cols")]
        columns: Option<usize>,
    },

    /// Toggles (or explicitly sets) a Markdown checkbox task at 0-based `task_index` inside a node.
    #[serde(alias = "toggle_task", alias = "check_task", alias = "ToggleChecklistTask")]
    ToggleChecklistTask {
        #[serde(default, alias = "id")]
        node_id: String,
        #[serde(default, alias = "index")]
        task_index: usize,
        #[serde(default)]
        checked: Option<bool>,
    },

    /// Adds a dated event to a Calendar node (targets `node_id`, or first Calendar node, or auto-creates one).
    #[serde(
        alias = "schedule_event",
        alias = "create_calendar_event",
        alias = "AddCalendarEvent"
    )]
    AddCalendarEvent {
        #[serde(default, alias = "calendar_node_id", alias = "id")]
        node_id: Option<String>,
        #[serde(default, alias = "day", alias = "fecha")]
        date: String,
        #[serde(default)]
        time: Option<String>,
        #[serde(default)]
        title: String,
        #[serde(default, alias = "description", alias = "note")]
        notes: String,
        #[serde(default, alias = "linked_node_id")]
        linked_node: Option<String>,
    },

    /// Toggles or sets completion status on a Calendar event matching `event_id_or_title`.
    #[serde(alias = "complete_calendar_event", alias = "ToggleCalendarEvent")]
    ToggleCalendarEvent {
        #[serde(default, alias = "calendar_node_id", alias = "id")]
        node_id: String,
        #[serde(default, alias = "event_id", alias = "title", alias = "event")]
        event_id_or_title: String,
        #[serde(default)]
        completed: Option<bool>,
    },

    /// Removes a Calendar event matching `event_id_or_title`.
    #[serde(alias = "delete_calendar_event", alias = "RemoveCalendarEvent")]
    RemoveCalendarEvent {
        #[serde(default, alias = "calendar_node_id", alias = "id")]
        node_id: String,
        #[serde(default, alias = "event_id", alias = "title", alias = "event")]
        event_id_or_title: String,
    },

    /// Appends a sorted timestamp bookmark to a `Video` or `Audio` node.
    #[serde(
        alias = "add_timestamp",
        alias = "bookmark_timestamp",
        alias = "AddMediaTimestamp"
    )]
    AddMediaTimestamp {
        #[serde(default, alias = "id")]
        node_id: String,
        #[serde(
            default,
            alias = "time",
            alias = "sec",
            deserialize_with = "deserialize_seconds_forgiving"
        )]
        seconds: f64,
        #[serde(default, alias = "note", alias = "title")]
        label: String,
    },

    /// Sets or appends a daily planner note for `date` (`YYYY-MM-DD`) on a `Calendar` node.
    #[serde(
        alias = "update_calendar_day_note",
        alias = "calendar_day_note",
        alias = "set_day_note",
        alias = "add_calendar_note",
        alias = "update_calendar",
        alias = "SetCalendarDayNote"
    )]
    SetCalendarDayNote {
        #[serde(default, alias = "calendar_node_id", alias = "id")]
        node_id: Option<String>,
        #[serde(default, alias = "day", alias = "fecha")]
        date: String,
        #[serde(default, alias = "content", alias = "text", alias = "body")]
        note: String,
        #[serde(default)]
        append: bool,
    },

    /// Switches a `Calendar` node between `"month"` and `"week"` view modes.
    #[serde(
        alias = "switch_calendar_mode",
        alias = "set_calendar_mode",
        alias = "SetCalendarViewMode"
    )]
    SetCalendarViewMode {
        #[serde(default, alias = "calendar_node_id", alias = "id")]
        node_id: Option<String>,
        #[serde(default)]
        mode: String,
    },

    /// Searches public web image repositories (`query`) and downloads `1..=3` matching images as `Image` nodes.
    #[serde(
        alias = "search_image",
        alias = "find_image",
        alias = "find_images",
        alias = "fetch_image",
        alias = "image_search",
        alias = "SearchImages"
    )]
    SearchImages {
        #[serde(default, alias = "q", alias = "prompt", alias = "topic", alias = "keywords")]
        query: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default = "default_one")]
        count: usize,
        #[serde(
            default,
            alias = "sources",
            alias = "source_node",
            deserialize_with = "deserialize_string_or_vec"
        )]
        source_nodes: Vec<String>,
        #[serde(default)]
        x: Option<f32>,
        #[serde(default)]
        y: Option<f32>,
        #[serde(
            default,
            alias = "position",
            deserialize_with = "deserialize_opt_pos_forgiving"
        )]
        pos: Option<[f32; 2]>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingImageSearch {
    pub node_id: NodeId,
    pub query: String,
}

#[derive(Debug, Default)]
pub struct StagedPatchResult {
    pub new_nodes: Vec<Node>,
    pub proposed_edges: Vec<Edge>,
    pub updated_nodes: Vec<(NodeId, String)>,
    pub moved_nodes: Vec<NodeId>,
    pub nodes_moved: usize,
    pub deleted_nodes: Vec<NodeId>,
    pub calendar_events_changed: usize,
    pub calendar_notes_changed: usize,
    pub last_calendar_target: Option<(NodeId, String)>,
    pub timestamps_added: usize,
    pub edges_colored: usize,
    pub quiz_questions_changed: usize,
    pub pending_image_searches: Vec<PendingImageSearch>,
    pub dropped_ops_count: usize,
}

impl StagedPatchResult {
    pub fn total_changes(&self) -> usize {
        self.new_nodes.len()
            + self.proposed_edges.len()
            + self.updated_nodes.len()
            + self.nodes_moved
            + self.deleted_nodes.len()
            + self.calendar_events_changed
            + self.calendar_notes_changed
            + self.timestamps_added
            + self.edges_colored
            + self.quiz_questions_changed
    }

    pub fn summary_line(&self) -> String {
        let mut parts = Vec::new();
        if !self.new_nodes.is_empty() {
            parts.push(format!("+{} node(s)", self.new_nodes.len()));
        }
        if !self.updated_nodes.is_empty() {
            parts.push(format!("~{} updated", self.updated_nodes.len()));
        }
        if self.nodes_moved > 0 {
            parts.push(format!("{} moved", self.nodes_moved));
        }
        if !self.deleted_nodes.is_empty() {
            parts.push(format!("-{} removed", self.deleted_nodes.len()));
        }
        if !self.proposed_edges.is_empty() {
            parts.push(format!("+{} link(s)", self.proposed_edges.len()));
        }
        if self.edges_colored > 0 {
            parts.push(format!("{} trace(s) colored", self.edges_colored));
        }
        if self.quiz_questions_changed > 0 {
            parts.push(format!("{} quiz Q(s)", self.quiz_questions_changed));
        }
        if self.calendar_events_changed > 0 {
            parts.push(format!("{} calendar event(s)", self.calendar_events_changed));
        }
        if self.calendar_notes_changed > 0 {
            parts.push(format!("{} day note(s)", self.calendar_notes_changed));
        }
        if self.timestamps_added > 0 {
            parts.push(format!("+{} timestamp(s)", self.timestamps_added));
        }
        if !self.pending_image_searches.is_empty() {
            parts.push(format!(
                "{} image search(es)",
                self.pending_image_searches.len()
            ));
        }
        parts.join(" · ")
    }
}

fn centroid_spawn_pos(
    graph: &StudyGraph,
    source_nodes: &[String],
    temp_to_real: &HashMap<String, NodeId>,
    sibling_offset: usize,
    fallback_origin: WorldPos,
) -> WorldPos {
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut valid_count = 0;

    for src in source_nodes {
        if let Some(id) = graph.resolve_node_id(src, temp_to_real) {
            if let Some(n) = graph.nodes.get(&id) {
                sum_x += n.pos.x;
                sum_y += n.pos.y;
                valid_count += 1;
            }
        }
    }

    let col = (sibling_offset % 3) as f32;
    let row = (sibling_offset / 3) as f32;
    let dx = col * 290.0;
    let dy = row * 220.0;
    if valid_count > 0 {
        WorldPos::new(
            sum_x / valid_count as f32 + dx,
            sum_y / valid_count as f32 + 220.0 + dy,
        )
    } else {
        WorldPos::new(fallback_origin.x + dx, fallback_origin.y + dy)
    }
}

impl StudyGraph {
    /// Resolve a temp ID, exact node ID, case-insensitive node title, or unique ID prefix.
    pub fn resolve_node_id(
        &self,
        raw: &str,
        temp_to_real: &HashMap<String, NodeId>,
    ) -> Option<NodeId> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(id) = temp_to_real.get(trimmed) {
            return Some(id.clone());
        }
        let exact = NodeId::from_str(trimmed);
        if self.nodes.contains_key(&exact) {
            return Some(exact);
        }
        let mut title_matches: Vec<&NodeId> = self
            .nodes
            .values()
            .filter(|n| n.title.trim().eq_ignore_ascii_case(trimmed))
            .map(|n| &n.id)
            .collect();
        if !title_matches.is_empty() {
            title_matches.sort_by(|a, b| a.as_str().cmp(b.as_str()));
            return Some(title_matches[0].clone());
        }
        if trimmed.len() >= 6 {
            let prefix_matches: Vec<&NodeId> = self
                .nodes
                .keys()
                .filter(|id| id.as_str().starts_with(trimmed))
                .collect();
            if prefix_matches.len() == 1 {
                return Some(prefix_matches[0].clone());
            }
        }
        if matches!(
            trimmed.to_ascii_lowercase().as_str(),
            "calendar" | "calendario" | "schedule" | "agenda" | "planner"
        ) {
            return self.first_calendar_node_id();
        }
        None
    }

    fn first_calendar_node_id(&self) -> Option<NodeId> {
        let mut cals: Vec<&NodeId> = self
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Calendar)
            .map(|n| &n.id)
            .collect();
        cals.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        cals.first().cloned().cloned()
    }

    fn first_quiz_node_id(&self) -> Option<NodeId> {
        let mut quizzes: Vec<&NodeId> = self
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Quiz)
            .map(|n| &n.id)
            .collect();
        quizzes.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        quizzes.first().cloned().cloned()
    }

    fn apply_edge_color_choice(
        edge: &mut Edge,
        color_opt: Option<&str>,
        category_opt: Option<&str>,
    ) -> bool {
        let Some(raw) = color_opt.map(str::trim).filter(|s| !s.is_empty()) else {
            if let Some(cat) = category_opt.map(str::trim).filter(|s| !s.is_empty()) {
                edge.color_category = Some(cat.to_string());
                return true;
            }
            return false;
        };
        if matches!(
            raw.to_ascii_lowercase().as_str(),
            "auto" | "default" | "none" | "clear" | "reset"
        ) {
            edge.color_hex = None;
            edge.color_category = category_opt
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned);
            return true;
        }
        if let Some(hex) = resolve_trace_color_hex(raw) {
            edge.color_hex = Some(hex);
            edge.color_category = category_opt
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .or_else(|| resolve_trace_color_label(raw).map(ToOwned::to_owned));
            return true;
        }
        false
    }

    fn bfs_layout_layers(
        &self,
        targets: &[NodeId],
        root_opt: Option<&NodeId>,
        undirected: bool,
    ) -> Vec<Vec<NodeId>> {
        if targets.is_empty() {
            return Vec::new();
        }
        let target_set: HashSet<NodeId> = targets.iter().cloned().collect();
        let order_index: HashMap<NodeId, usize> = targets
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();

        let mut out_adj: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        let mut all_adj: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        let mut in_deg: HashMap<NodeId, usize> = HashMap::new();
        let mut internal_edges = 0usize;

        for edge in self.edges.values() {
            let f = &edge.from.node;
            let t = &edge.to.node;
            if f != t && target_set.contains(f) && target_set.contains(t) {
                internal_edges += 1;
                out_adj.entry(f.clone()).or_default().push(t.clone());
                *in_deg.entry(t.clone()).or_insert(0) += 1;
                all_adj.entry(f.clone()).or_default().push(t.clone());
                all_adj.entry(t.clone()).or_default().push(f.clone());
            }
        }
        for list in out_adj.values_mut().chain(all_adj.values_mut()) {
            list.sort_by_key(|id| order_index.get(id).copied().unwrap_or(usize::MAX));
            list.dedup();
        }

        if internal_edges == 0 && targets.len() > 3 {
            let mut layers = Vec::new();
            let mut rest: Vec<NodeId> = targets.to_vec();
            if let Some(root) = root_opt.filter(|r| target_set.contains(*r)) {
                rest.retain(|id| id != root);
                layers.push(vec![root.clone()]);
            }
            for chunk in rest.chunks(3) {
                layers.push(chunk.to_vec());
            }
            return layers;
        }

        let adj = if undirected { &all_adj } else { &out_adj };
        let mut visited: HashSet<NodeId> = HashSet::new();
        let mut layers: Vec<Vec<NodeId>> = Vec::new();

        let initial_seeds: Vec<NodeId> = if let Some(root) = root_opt.filter(|r| target_set.contains(*r))
        {
            vec![root.clone()]
        } else if undirected {
            let best = targets
                .iter()
                .max_by_key(|id| {
                    let deg = all_adj.get(*id).map_or(0, Vec::len);
                    let rev_ord = usize::MAX - order_index.get(*id).copied().unwrap_or(0);
                    (deg, rev_ord)
                })
                .cloned()
                .unwrap_or_else(|| targets[0].clone());
            vec![best]
        } else {
            let roots: Vec<NodeId> = targets
                .iter()
                .filter(|id| in_deg.get(*id).copied().unwrap_or(0) == 0)
                .cloned()
                .collect();
            if roots.is_empty() {
                vec![targets[0].clone()]
            } else {
                roots
            }
        };

        let mut queue: VecDeque<(NodeId, usize)> = VecDeque::new();
        for s in initial_seeds {
            if visited.insert(s.clone()) {
                queue.push_back((s, 0));
            }
        }

        let push_bfs = |queue: &mut VecDeque<(NodeId, usize)>,
                        visited: &mut HashSet<NodeId>,
                        layers: &mut Vec<Vec<NodeId>>| {
            while let Some((curr, depth)) = queue.pop_front() {
                while layers.len() <= depth {
                    layers.push(Vec::new());
                }
                layers[depth].push(curr.clone());
                if let Some(nbrs) = adj.get(&curr) {
                    for nxt in nbrs {
                        if visited.insert(nxt.clone()) {
                            queue.push_back((nxt.clone(), depth + 1));
                        }
                    }
                }
            }
        };

        push_bfs(&mut queue, &mut visited, &mut layers);

        // Include any disconnected components deterministically in trailing layers
        let mut leftover_isolated = Vec::new();
        for id in targets {
            if visited.contains(id) {
                continue;
            }
            let has_nbr = adj.get(id).is_some_and(|v| !v.is_empty())
                || all_adj.get(id).is_some_and(|v| !v.is_empty());
            if !has_nbr {
                if visited.insert(id.clone()) {
                    leftover_isolated.push(id.clone());
                }
            } else if visited.insert(id.clone()) {
                let base_depth = layers.len();
                queue.push_back((id.clone(), base_depth));
                push_bfs(&mut queue, &mut visited, &mut layers);
            }
        }
        if !leftover_isolated.is_empty() {
            for chunk in leftover_isolated.chunks(3) {
                layers.push(chunk.to_vec());
            }
        }

        layers
    }

    fn compute_arranged_positions(
        &self,
        targets: &[NodeId],
        layout_norm: &str,
        root_opt: Option<&NodeId>,
        ox: f32,
        oy: f32,
        gap_x: f32,
        gap_y: f32,
        columns: Option<usize>,
    ) -> Option<HashMap<NodeId, WorldPos>> {
        if targets.is_empty() {
            return None;
        }
        let sizes: HashMap<NodeId, (f32, f32)> = targets
            .iter()
            .filter_map(|id| self.nodes.get(id).map(|n| (id.clone(), n.effective_size())))
            .collect();
        let mut placed: HashMap<NodeId, WorldPos> = HashMap::with_capacity(targets.len());

        match layout_norm {
            "hierarchy" | "tree" | "top_down" | "vertical_tree" | "layered" => {
                let layers = self.bfs_layout_layers(targets, root_opt, false);
                let layer_widths: Vec<f32> = layers
                    .iter()
                    .map(|layer| {
                        let sum_w: f32 = layer.iter().map(|id| sizes[id].0).sum();
                        sum_w + gap_x * (layer.len().saturating_sub(1) as f32)
                    })
                    .collect();
                let max_w = layer_widths.iter().copied().fold(0.0f32, f32::max);
                let center_x = ox + max_w * 0.5;
                let mut cur_y = oy;
                for (li, layer) in layers.iter().enumerate() {
                    let mut cur_x = center_x - layer_widths[li] * 0.5;
                    let mut max_h = 0.0f32;
                    for id in layer {
                        let (w, h) = sizes[id];
                        placed.insert(id.clone(), WorldPos::new(cur_x, cur_y));
                        cur_x += w + gap_x;
                        max_h = max_h.max(h);
                    }
                    cur_y += max_h + gap_y;
                }
            }
            "flow" | "horizontal" | "left_to_right" | "sequence" | "timeline" => {
                let layers = self.bfs_layout_layers(targets, root_opt, false);
                let layer_heights: Vec<f32> = layers
                    .iter()
                    .map(|layer| {
                        let sum_h: f32 = layer.iter().map(|id| sizes[id].1).sum();
                        sum_h + gap_y * (layer.len().saturating_sub(1) as f32)
                    })
                    .collect();
                let max_h = layer_heights.iter().copied().fold(0.0f32, f32::max);
                let center_y = oy + max_h * 0.5;
                let mut cur_x = ox;
                for (li, layer) in layers.iter().enumerate() {
                    let mut cur_y = center_y - layer_heights[li] * 0.5;
                    let mut max_w = 0.0f32;
                    for id in layer {
                        let (w, h) = sizes[id];
                        placed.insert(id.clone(), WorldPos::new(cur_x, cur_y));
                        cur_y += h + gap_y;
                        max_w = max_w.max(w);
                    }
                    cur_x += max_w + gap_x;
                }
            }
            "radial" | "mindmap" | "mind_map" | "hub" => {
                let r_box = |id: &NodeId| -> f32 {
                    let (w, h) = sizes[id];
                    0.5 * (w * w + h * h).sqrt()
                };
                let mut layers = self.bfs_layout_layers(targets, root_opt, true);
                if layers.is_empty() {
                    return None;
                }
                let center_id = layers[0].remove(0);
                if !layers[0].is_empty() {
                    let extra = std::mem::take(&mut layers[0]);
                    if layers.len() > 1 {
                        let mut merged = extra;
                        merged.append(&mut layers[1]);
                        layers[1] = merged;
                    } else {
                        layers.push(extra);
                    }
                }
                let (cw, ch) = sizes[&center_id];
                placed.insert(center_id.clone(), WorldPos::new(-cw * 0.5, -ch * 0.5));
                let mut prev_outer = r_box(&center_id);
                let min_ring_gap = gap_x.min(gap_y);

                for ring in layers.into_iter().skip(1) {
                    if ring.is_empty() {
                        continue;
                    }
                    let k = ring.len();
                    let max_ring_r = ring.iter().map(r_box).fold(0.0f32, f32::max);
                    let r_from_prev = prev_outer + max_ring_r + min_ring_gap;
                    let r_from_chord = if k >= 2 {
                        let chord_req = 2.0 * max_ring_r + gap_x;
                        let sin_half = (std::f32::consts::PI / (k as f32)).sin().max(0.05);
                        chord_req / (2.0 * sin_half)
                    } else {
                        0.0
                    };
                    let ring_r = r_from_prev.max(r_from_chord);
                    for (idx, id) in ring.iter().enumerate() {
                        let theta = -std::f32::consts::FRAC_PI_2
                            + (idx as f32) * std::f32::consts::TAU / (k as f32);
                        let cx = ring_r * theta.cos();
                        let cy = ring_r * theta.sin();
                        let (w, h) = sizes[id];
                        placed.insert(id.clone(), WorldPos::new(cx - w * 0.5, cy - h * 0.5));
                    }
                    prev_outer = ring_r + max_ring_r;
                }

                let min_x = placed.values().map(|p| p.x).fold(f32::INFINITY, f32::min);
                let min_y = placed.values().map(|p| p.y).fold(f32::INFINITY, f32::min);
                if min_x.is_finite() && min_y.is_finite() {
                    let shift_x = ox - min_x;
                    let shift_y = oy - min_y;
                    for p in placed.values_mut() {
                        p.x += shift_x;
                        p.y += shift_y;
                    }
                }
            }
            "grid" => {
                let n = targets.len();
                let cols = columns
                    .unwrap_or_else(|| (n as f32).sqrt().ceil() as usize)
                    .clamp(1, n.max(1).min(6));
                let rows = n.div_ceil(cols);
                let mut col_w = vec![0.0f32; cols];
                let mut row_h = vec![0.0f32; rows];
                for (i, id) in targets.iter().enumerate() {
                    let (w, h) = sizes[id];
                    col_w[i % cols] = col_w[i % cols].max(w);
                    row_h[i / cols] = row_h[i / cols].max(h);
                }
                let mut col_x = vec![ox; cols];
                for c in 1..cols {
                    col_x[c] = col_x[c - 1] + col_w[c - 1] + gap_x;
                }
                let mut row_y = vec![oy; rows];
                for r in 1..rows {
                    row_y[r] = row_y[r - 1] + row_h[r - 1] + gap_y;
                }
                for (i, id) in targets.iter().enumerate() {
                    placed.insert(id.clone(), WorldPos::new(col_x[i % cols], row_y[i / cols]));
                }
            }
            "row" => {
                let max_h = targets
                    .iter()
                    .map(|id| sizes[id].1)
                    .fold(0.0f32, f32::max);
                let mut cur_x = ox;
                for id in targets {
                    let (w, h) = sizes[id];
                    placed.insert(id.clone(), WorldPos::new(cur_x, oy + (max_h - h) * 0.5));
                    cur_x += w + gap_x;
                }
            }
            "column" | "stack" => {
                let max_w = targets
                    .iter()
                    .map(|id| sizes[id].0)
                    .fold(0.0f32, f32::max);
                let mut cur_y = oy;
                for id in targets {
                    let (w, h) = sizes[id];
                    placed.insert(id.clone(), WorldPos::new(ox + (max_w - w) * 0.5, cur_y));
                    cur_y += h + gap_y;
                }
            }
            _ => return None,
        }

        Some(placed)
    }

    /// Validates a `GraphPatch` against the current graph topology, resolving temp IDs and
    /// filtering out any hallucinated IDs or duplicate edges.
    pub fn validate_and_stage_patch(&mut self, patch: &GraphPatch) -> StagedPatchResult {
        self.validate_and_stage_patch_full(patch, WorldPos::new(100.0, 100.0), None)
    }

    /// Validates and stages a `GraphPatch`, anchoring new standalone nodes around `fallback_origin`.
    pub fn validate_and_stage_patch_at(
        &mut self,
        patch: &GraphPatch,
        fallback_origin: WorldPos,
    ) -> StagedPatchResult {
        self.validate_and_stage_patch_full(patch, fallback_origin, None)
    }

    /// Validates and stages a `GraphPatch`, anchoring standalone nodes around `fallback_origin` and
    /// resolving relative calendar dates against `reference_date` (`YYYY-MM-DD`).
    pub fn validate_and_stage_patch_full(
        &mut self,
        patch: &GraphPatch,
        fallback_origin: WorldPos,
        reference_date: Option<&str>,
    ) -> StagedPatchResult {
        let mut result = StagedPatchResult::default();
        let mut temp_to_real: HashMap<String, NodeId> = HashMap::new();
        let mut created_count: usize = 0;
        let mut fallback_count: usize = 0;

        for op in &patch.ops {
            match op {
                PatchOp::CreateSynthesisNode {
                    temp_id,
                    title,
                    markdown_body,
                    source_nodes,
                    x,
                    y,
                    pos,
                    width,
                    height,
                } => {
                    let spawn_pos = if let Some(explicit) = resolve_explicit_pos(*x, *y, *pos) {
                        explicit
                    } else {
                        let p = centroid_spawn_pos(
                            self,
                            source_nodes,
                            &temp_to_real,
                            fallback_count,
                            fallback_origin,
                        );
                        fallback_count += 1;
                        p
                    };
                    created_count += 1;

                    let mut node = Node::new(
                        NodeKind::LlmSynthesis,
                        if title.trim().is_empty() {
                            "AI Synthesis"
                        } else {
                            title
                        },
                        spawn_pos,
                    );
                    node.content = markdown_body.clone();
                    apply_optional_size(&mut node, *width, *height);
                    let tid = if temp_id.trim().is_empty() {
                        format!("tmp_auto_{}", created_count)
                    } else {
                        temp_id.clone()
                    };
                    temp_to_real.insert(tid, node.id.clone());

                    self.add_node(node.clone());
                    result.new_nodes.push(node);
                }

                PatchOp::CreateStudyNode {
                    temp_id,
                    kind,
                    title,
                    content,
                    answer,
                    media_url,
                    source_nodes,
                    quiz_questions,
                    text_font,
                    text_size,
                    x,
                    y,
                    pos,
                    width,
                    height,
                } => {
                    let spawn_pos = if let Some(explicit) = resolve_explicit_pos(*x, *y, *pos) {
                        explicit
                    } else {
                        let p = centroid_spawn_pos(
                            self,
                            source_nodes,
                            &temp_to_real,
                            fallback_count,
                            fallback_origin,
                        );
                        fallback_count += 1;
                        p
                    };
                    created_count += 1;

                    let mut node = Node::new(
                        *kind,
                        if title.trim().is_empty() {
                            kind.display_name()
                        } else {
                            title
                        },
                        spawn_pos,
                    );
                    node.content = content.clone();
                    match kind {
                        NodeKind::Flashcard => {
                            node.meta.answer = answer.clone();
                            node.meta.mastery = Some("new".into());
                        }
                        NodeKind::Section => {
                            node.meta.section_framed = true;
                            node.size = (640.0, 400.0);
                        }
                        NodeKind::Formula => {
                            node.meta.language = Some("MATH".into());
                        }
                        NodeKind::GraphFormula => {
                            node.size = (360.0, 320.0);
                        }
                        NodeKind::Sequence => {
                            node.meta.step_index = Some(created_count as u32);
                        }
                        NodeKind::Video => {
                            node.size = (360.0, 248.0);
                            node.meta.media_url = media_url
                                .as_deref()
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(ToOwned::to_owned);
                        }
                        NodeKind::Audio => {
                            node.size = (288.0, 124.0);
                            node.meta.media_url = media_url
                                .as_deref()
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(ToOwned::to_owned);
                        }
                        NodeKind::Calendar => {
                            node.size = (312.0, 286.0);
                        }
                        NodeKind::Text => {
                            node.size = (240.0, 64.0);
                            node.meta.text_font = Some(
                                text_font
                                    .as_deref()
                                    .map(str::trim)
                                    .filter(|s| !s.is_empty())
                                    .unwrap_or("sans")
                                    .to_string(),
                            );
                            node.meta.text_size =
                                Some(text_size.unwrap_or(18.0).clamp(10.0, 96.0));
                        }
                        NodeKind::Quiz => {
                            node.size = (320.0, 260.0);
                            node.meta.quiz_questions = quiz_questions.clone();
                        }
                        _ => {}
                    }
                    apply_optional_size(&mut node, *width, *height);
                    let tid = if temp_id.trim().is_empty() {
                        format!("tmp_auto_{}", created_count)
                    } else {
                        temp_id.clone()
                    };
                    temp_to_real.insert(tid, node.id.clone());

                    self.add_node(node.clone());
                    result.new_nodes.push(node);
                }

                PatchOp::UpdateNodeSummary { node_id, summary } => {
                    if let Some(nid) = self.resolve_node_id(node_id, &temp_to_real) {
                        if let Some(node) = self.get_node_mut(&nid) {
                            node.summary = Some(summary.clone());
                            result.updated_nodes.push((nid, summary.clone()));
                        } else {
                            result.dropped_ops_count += 1;
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::UpdateNode {
                    node_id,
                    title,
                    content,
                    answer,
                    summary,
                    media_url,
                    quiz_questions,
                } => {
                    if let Some(nid) = self.resolve_node_id(node_id, &temp_to_real) {
                        if let Some(node) = self.get_node_mut(&nid) {
                            if let Some(t) = title {
                                node.title = t.clone();
                            }
                            if let Some(c) = content {
                                node.content = c.clone();
                            }
                            if let Some(a) = answer {
                                node.meta.answer = Some(a.clone());
                            }
                            if let Some(s) = summary {
                                node.summary = Some(s.clone());
                            }
                            if let Some(u) = media_url {
                                let tr = u.trim();
                                node.meta.media_url = if tr.is_empty() {
                                    None
                                } else {
                                    Some(tr.to_string())
                                };
                            }
                            if let Some(qs) = quiz_questions {
                                node.meta.quiz_questions = qs.clone();
                                result.quiz_questions_changed += qs.len().max(1);
                            }
                            let desc = title.clone().unwrap_or_else(|| node.title.clone());
                            result.updated_nodes.push((nid, desc));
                        } else {
                            result.dropped_ops_count += 1;
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::AddQuizQuestion {
                    node_id,
                    question,
                    options,
                    correct_index,
                    answer,
                    hints,
                } => {
                    let clean_q = question.trim();
                    if clean_q.is_empty() {
                        result.dropped_ops_count += 1;
                        continue;
                    }
                    let explicit_id = node_id
                        .as_deref()
                        .and_then(|raw| self.resolve_node_id(raw, &temp_to_real))
                        .filter(|nid| {
                            self.nodes
                                .get(nid)
                                .is_some_and(|n| n.kind == NodeKind::Quiz)
                        });
                    let target_id = if let Some(qid) = explicit_id.or_else(|| self.first_quiz_node_id()) {
                        qid
                    } else {
                        let spawn_pos = centroid_spawn_pos(
                            self,
                            &[],
                            &temp_to_real,
                            fallback_count,
                            fallback_origin,
                        );
                        fallback_count += 1;
                        created_count += 1;
                        let mut quiz = Node::new(NodeKind::Quiz, "Quiz", spawn_pos);
                        quiz.size = (320.0, 260.0);
                        let qid = quiz.id.clone();
                        self.add_node(quiz.clone());
                        result.new_nodes.push(quiz);
                        qid
                    };

                    if let Some(node) = self.get_node_mut(&target_id) {
                        let clean_opts: Vec<String> = options
                            .iter()
                            .map(|o| o.trim().to_string())
                            .filter(|o| !o.is_empty())
                            .collect();
                        let clean_correct = correct_index.filter(|&i| i < clean_opts.len());
                        let clean_hints: Vec<String> = hints
                            .iter()
                            .map(|h| h.trim().to_string())
                            .filter(|h| !h.is_empty())
                            .collect();
                        node.meta.quiz_questions.push(QuizQuestion {
                            id: uuid::Uuid::new_v4().to_string(),
                            question: clean_q.to_string(),
                            options: clean_opts,
                            correct_index: clean_correct,
                            answer: answer.trim().to_string(),
                            hints: clean_hints,
                        });
                        result.quiz_questions_changed += 1;
                        result
                            .updated_nodes
                            .push((target_id, format!("Added quiz Q{}", node.meta.quiz_questions.len())));
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::UpdateQuizQuestion {
                    node_id,
                    question_index,
                    question,
                    options,
                    correct_index,
                    answer,
                    hints,
                    add_hint,
                } => {
                    let Some(nid) = self.resolve_node_id(node_id, &temp_to_real) else {
                        result.dropped_ops_count += 1;
                        continue;
                    };
                    if let Some(node) = self.get_node_mut(&nid) {
                        if let Some(q_item) = node.meta.quiz_questions.get_mut(*question_index) {
                            if let Some(q_text) = question.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                                q_item.question = q_text.to_string();
                            }
                            if let Some(opts) = options {
                                q_item.options = opts
                                    .iter()
                                    .map(|o| o.trim().to_string())
                                    .filter(|o| !o.is_empty())
                                    .collect();
                            }
                            if let Some(ci) = correct_index {
                                q_item.correct_index = if *ci < q_item.options.len() {
                                    Some(*ci)
                                } else {
                                    None
                                };
                            }
                            if let Some(ans) = answer {
                                q_item.answer = ans.trim().to_string();
                            }
                            if let Some(hs) = hints {
                                q_item.hints = hs
                                    .iter()
                                    .map(|h| h.trim().to_string())
                                    .filter(|h| !h.is_empty())
                                    .collect();
                            }
                            if let Some(extra) = add_hint.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                                q_item.hints.push(extra.to_string());
                            }
                            result.quiz_questions_changed += 1;
                            result.updated_nodes.push((
                                nid,
                                format!("Updated quiz Q{}", question_index + 1),
                            ));
                        } else {
                            result.dropped_ops_count += 1;
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::RemoveQuizQuestion {
                    node_id,
                    question_index,
                } => {
                    let Some(nid) = self.resolve_node_id(node_id, &temp_to_real) else {
                        result.dropped_ops_count += 1;
                        continue;
                    };
                    if let Some(node) = self.get_node_mut(&nid) {
                        if *question_index < node.meta.quiz_questions.len() {
                            node.meta.quiz_questions.remove(*question_index);
                            result.quiz_questions_changed += 1;
                            result.updated_nodes.push((
                                nid,
                                format!("Removed quiz Q{}", question_index + 1),
                            ));
                        } else {
                            result.dropped_ops_count += 1;
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::DeleteNode { node_id } => {
                    if let Some(nid) = self.resolve_node_id(node_id, &temp_to_real) {
                        if self.remove_node(&nid).is_some() {
                            result.deleted_nodes.push(nid);
                        } else {
                            result.dropped_ops_count += 1;
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::MoveNode {
                    node_id,
                    x,
                    y,
                    pos,
                    dx,
                    dy,
                    relative_to,
                    placement,
                    gap,
                    width,
                    height,
                } => {
                    let Some(nid) = self.resolve_node_id(node_id, &temp_to_real) else {
                        result.dropped_ops_count += 1;
                        continue;
                    };
                    let Some((cur_pos, cur_size)) = self
                        .nodes
                        .get(&nid)
                        .map(|n| (n.pos, n.effective_size()))
                    else {
                        result.dropped_ops_count += 1;
                        continue;
                    };
                    let eff_w = width
                        .filter(|v| v.is_finite())
                        .map(|v| v.clamp(80.0, 2400.0))
                        .unwrap_or(cur_size.0);
                    let eff_h = height
                        .filter(|v| v.is_finite())
                        .map(|v| v.clamp(36.0, 1800.0))
                        .unwrap_or(cur_size.1);
                    let anchor_rect = relative_to
                        .as_deref()
                        .and_then(|raw| self.resolve_node_id(raw, &temp_to_real))
                        .filter(|aid| aid != &nid)
                        .and_then(|aid| {
                            self.nodes
                                .get(&aid)
                                .map(|a| (a.pos, a.effective_size()))
                        });

                    let dx_val = dx.filter(|v| v.is_finite());
                    let dy_val = dy.filter(|v| v.is_finite());
                    let new_pos_opt = if let Some((apos, asz)) = anchor_rect {
                        let g = gap.filter(|v| v.is_finite()).unwrap_or(72.0).clamp(16.0, 400.0);
                        let nx_nudge = dx_val.unwrap_or(0.0);
                        let ny_nudge = dy_val.unwrap_or(0.0);
                        let place_norm = placement
                            .as_deref()
                            .map(|s| s.trim().to_ascii_lowercase());
                        match place_norm.as_deref() {
                            Some("right" | "east" | "next" | "derecha") => Some(WorldPos::new(
                                apos.x + asz.0 + g + nx_nudge,
                                apos.y + ny_nudge,
                            )),
                            Some("left" | "west" | "prev" | "previous" | "izquierda") => {
                                Some(WorldPos::new(
                                    apos.x - eff_w - g + nx_nudge,
                                    apos.y + ny_nudge,
                                ))
                            }
                            Some("below" | "down" | "bottom" | "south" | "under" | "abajo" | "debajo") => {
                                Some(WorldPos::new(
                                    apos.x + nx_nudge,
                                    apos.y + asz.1 + g + ny_nudge,
                                ))
                            }
                            Some("above" | "up" | "top" | "north" | "arriba" | "encima") => {
                                Some(WorldPos::new(
                                    apos.x + nx_nudge,
                                    apos.y - eff_h - g + ny_nudge,
                                ))
                            }
                            _ if dx_val.is_some() || dy_val.is_some() => Some(WorldPos::new(
                                apos.x + nx_nudge,
                                apos.y + ny_nudge,
                            )),
                            _ => Some(WorldPos::new(apos.x + asz.0 + g, apos.y)),
                        }
                    } else if let Some(merged) = merge_node_pos(cur_pos, *x, *y, *pos) {
                        Some(merged)
                    } else if dx_val.is_some() || dy_val.is_some() {
                        Some(WorldPos::new(
                            cur_pos.x + dx_val.unwrap_or(0.0),
                            cur_pos.y + dy_val.unwrap_or(0.0),
                        ))
                    } else {
                        None
                    };

                    if let Some(node) = self.get_node_mut(&nid) {
                        let moved = if let Some(p) = new_pos_opt {
                            node.pos = p;
                            true
                        } else {
                            false
                        };
                        let resized = apply_optional_size(node, *width, *height);
                        if moved || resized {
                            result.nodes_moved += 1;
                            if !result.moved_nodes.contains(&nid) {
                                result.moved_nodes.push(nid);
                            }
                        } else {
                            result.dropped_ops_count += 1;
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::ArrangeNodes {
                    positions,
                    layout,
                    node_ids,
                    root_node,
                    origin_x,
                    origin_y,
                    spacing_x,
                    spacing_y,
                    columns,
                } => {
                    let mut moved_set: HashSet<NodeId> = HashSet::new();

                    if let Some(raw_layout) = layout.as_deref().map(str::trim).filter(|s| !s.is_empty())
                    {
                        let layout_norm = raw_layout.to_ascii_lowercase().replace(['-', ' '], "_");
                        let targets: Vec<NodeId> = if !node_ids.is_empty() {
                            let mut seen = HashSet::new();
                            node_ids
                                .iter()
                                .filter_map(|raw| self.resolve_node_id(raw, &temp_to_real))
                                .filter(|id| seen.insert(id.clone()))
                                .collect()
                        } else {
                            let mut all: Vec<(&NodeId, String)> = self
                                .nodes
                                .values()
                                .filter(|n| n.kind != NodeKind::Section)
                                .map(|n| (&n.id, n.title.to_ascii_lowercase()))
                                .collect();
                            all.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.as_str().cmp(b.0.as_str())));
                            all.into_iter().map(|(id, _)| id.clone()).collect()
                        };

                        let gap_x = spacing_x
                            .filter(|v| v.is_finite())
                            .unwrap_or(80.0)
                            .clamp(24.0, 320.0);
                        let gap_y = spacing_y
                            .filter(|v| v.is_finite())
                            .unwrap_or(90.0)
                            .clamp(24.0, 320.0);
                        let min_existing_x = targets
                            .iter()
                            .filter_map(|id| self.nodes.get(id).map(|n| n.pos.x))
                            .fold(f32::INFINITY, f32::min);
                        let min_existing_y = targets
                            .iter()
                            .filter_map(|id| self.nodes.get(id).map(|n| n.pos.y))
                            .fold(f32::INFINITY, f32::min);
                        let ox = origin_x.filter(|v| v.is_finite()).unwrap_or_else(|| {
                            if min_existing_x.is_finite() {
                                min_existing_x
                            } else {
                                fallback_origin.x
                            }
                        });
                        let oy = origin_y.filter(|v| v.is_finite()).unwrap_or_else(|| {
                            if min_existing_y.is_finite() {
                                min_existing_y
                            } else {
                                fallback_origin.y
                            }
                        });
                        let root_opt = root_node
                            .as_deref()
                            .and_then(|r| self.resolve_node_id(r, &temp_to_real));

                        if let Some(coords) = self.compute_arranged_positions(
                            &targets,
                            &layout_norm,
                            root_opt.as_ref(),
                            ox,
                            oy,
                            gap_x,
                            gap_y,
                            *columns,
                        ) {
                            for (nid, new_pos) in coords {
                                if let Some(node) = self.get_node_mut(&nid) {
                                    node.pos = new_pos;
                                    moved_set.insert(nid);
                                }
                            }
                        }
                    }

                    for spec in positions {
                        let Some(nid) = self.resolve_node_id(&spec.node_id, &temp_to_real) else {
                            result.dropped_ops_count += 1;
                            continue;
                        };
                        if let Some(node) = self.get_node_mut(&nid) {
                            let cur = node.pos;
                            let moved = if let Some(p) =
                                merge_node_pos(cur, spec.x, spec.y, spec.pos)
                            {
                                node.pos = p;
                                true
                            } else {
                                false
                            };
                            let resized = apply_optional_size(node, spec.width, spec.height);
                            if moved || resized {
                                moved_set.insert(nid);
                            } else {
                                result.dropped_ops_count += 1;
                            }
                        } else {
                            result.dropped_ops_count += 1;
                        }
                    }

                    if moved_set.is_empty() {
                        result.dropped_ops_count += 1;
                    } else {
                        let mut sorted_moved: Vec<NodeId> = moved_set.into_iter().collect();
                        sorted_moved.sort_by(|a, b| a.as_str().cmp(b.as_str()));
                        result.nodes_moved += sorted_moved.len();
                        for nid in sorted_moved {
                            if !result.moved_nodes.contains(&nid) {
                                result.moved_nodes.push(nid);
                            }
                        }
                    }
                }

                PatchOp::ToggleChecklistTask {
                    node_id,
                    task_index,
                    checked,
                } => {
                    let Some(nid) = self.resolve_node_id(node_id, &temp_to_real) else {
                        result.dropped_ops_count += 1;
                        continue;
                    };
                    if let Some(node) = self.get_node_mut(&nid) {
                        let mut cur_task = 0usize;
                        let mut changed = false;
                        let mut out_lines = Vec::new();
                        for line in node.content.lines() {
                            let trimmed = line.trim_start();
                            let indent = &line[..line.len() - trimmed.len()];
                            if let Some(rest) = trimmed
                                .strip_prefix("- [ ] ")
                                .or_else(|| trimmed.strip_prefix("* [ ] "))
                            {
                                if cur_task == *task_index {
                                    let mark = if checked.unwrap_or(true) { "x" } else { " " };
                                    out_lines.push(format!("{}- [{}] {}", indent, mark, rest));
                                    changed = true;
                                } else {
                                    out_lines.push(line.to_string());
                                }
                                cur_task += 1;
                            } else if let Some(rest) = trimmed
                                .strip_prefix("- [x] ")
                                .or_else(|| trimmed.strip_prefix("- [X] "))
                                .or_else(|| trimmed.strip_prefix("* [x] "))
                                .or_else(|| trimmed.strip_prefix("* [X] "))
                            {
                                if cur_task == *task_index {
                                    let mark = if checked.unwrap_or(false) { "x" } else { " " };
                                    out_lines.push(format!("{}- [{}] {}", indent, mark, rest));
                                    changed = true;
                                } else {
                                    out_lines.push(line.to_string());
                                }
                                cur_task += 1;
                            } else {
                                out_lines.push(line.to_string());
                            }
                        }
                        if changed {
                            node.content = out_lines.join("\n");
                            result
                                .updated_nodes
                                .push((nid, format!("Updated task #{}", task_index + 1)));
                        } else {
                            result.dropped_ops_count += 1;
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::AddCalendarEvent {
                    node_id,
                    date,
                    time,
                    title,
                    notes,
                    linked_node,
                } => {
                    let Some(clean_date) = normalize_calendar_date(date, reference_date) else {
                        result.dropped_ops_count += 1;
                        continue;
                    };
                    if title.trim().is_empty() {
                        result.dropped_ops_count += 1;
                        continue;
                    }
                    let explicit_id = node_id
                        .as_deref()
                        .and_then(|raw| self.resolve_node_id(raw, &temp_to_real))
                        .filter(|nid| {
                            self.nodes
                                .get(nid)
                                .is_some_and(|n| n.kind == NodeKind::Calendar)
                        });

                    let existing_cal_id = explicit_id.or_else(|| self.first_calendar_node_id());

                    let target_cal_id = if let Some(cid) = existing_cal_id {
                        cid
                    } else {
                        let spawn_pos = centroid_spawn_pos(
                            self,
                            &[],
                            &temp_to_real,
                            fallback_count,
                            fallback_origin,
                        );
                        fallback_count += 1;
                        created_count += 1;
                        let mut cal = Node::new(NodeKind::Calendar, "Study Calendar", spawn_pos);
                        cal.size = (312.0, 286.0);
                        let cid = cal.id.clone();
                        self.add_node(cal.clone());
                        result.new_nodes.push(cal);
                        cid
                    };

                    let resolved_link = linked_node
                        .as_deref()
                        .and_then(|raw| self.resolve_node_id(raw, &temp_to_real));

                    if let Some(cal_node) = self.get_node_mut(&target_cal_id) {
                        let ev = CalendarEvent {
                            id: uuid::Uuid::new_v4().to_string(),
                            date: clean_date.clone(),
                            time: time
                                .as_deref()
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(ToOwned::to_owned),
                            title: title.trim().to_string(),
                            notes: notes.trim().to_string(),
                            completed: false,
                            linked_node: resolved_link,
                        };
                        cal_node.meta.calendar_events.push(ev);
                        cal_node.meta.calendar_events.sort_by(|a, b| {
                            a.date
                                .cmp(&b.date)
                                .then_with(|| a.time.as_deref().unwrap_or("").cmp(b.time.as_deref().unwrap_or("")))
                                .then_with(|| a.title.cmp(&b.title))
                        });
                        let mut d_parts = clean_date.split('-');
                        if let (Some(Ok(yy)), Some(Ok(mm))) = (
                            d_parts.next().map(str::parse::<i32>),
                            d_parts.next().map(str::parse::<u32>),
                        ) {
                            cal_node.meta.calendar_year = Some(yy);
                            cal_node.meta.calendar_month = Some(mm);
                        }
                        cal_node.meta.calendar_week_start = monday_of_week_ymd(&clean_date);
                        result.calendar_events_changed += 1;
                        result.last_calendar_target = Some((target_cal_id, clean_date));
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::ToggleCalendarEvent {
                    node_id,
                    event_id_or_title,
                    completed,
                } => {
                    let cal_id = self
                        .resolve_node_id(node_id, &temp_to_real)
                        .filter(|nid| {
                            self.nodes
                                .get(nid)
                                .is_some_and(|n| n.kind == NodeKind::Calendar)
                        })
                        .or_else(|| self.first_calendar_node_id());
                    let Some(cid) = cal_id else {
                        result.dropped_ops_count += 1;
                        continue;
                    };
                    let needle = event_id_or_title.trim().to_ascii_lowercase();
                    if let Some(cal_node) = self.get_node_mut(&cid) {
                        if let Some(ev) = cal_node.meta.calendar_events.iter_mut().find(|e| {
                            e.id == *event_id_or_title
                                || (!needle.is_empty() && e.title.to_ascii_lowercase().contains(&needle))
                        }) {
                            ev.completed = completed.unwrap_or(!ev.completed);
                            result.calendar_events_changed += 1;
                        } else {
                            result.dropped_ops_count += 1;
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::RemoveCalendarEvent {
                    node_id,
                    event_id_or_title,
                } => {
                    let cal_id = self
                        .resolve_node_id(node_id, &temp_to_real)
                        .filter(|nid| {
                            self.nodes
                                .get(nid)
                                .is_some_and(|n| n.kind == NodeKind::Calendar)
                        })
                        .or_else(|| self.first_calendar_node_id());
                    let Some(cid) = cal_id else {
                        result.dropped_ops_count += 1;
                        continue;
                    };
                    let needle = event_id_or_title.trim().to_ascii_lowercase();
                    if let Some(cal_node) = self.get_node_mut(&cid) {
                        let before = cal_node.meta.calendar_events.len();
                        cal_node.meta.calendar_events.retain(|e| {
                            e.id != *event_id_or_title
                                && (needle.is_empty()
                                    || !e.title.to_ascii_lowercase().contains(&needle))
                        });
                        let removed = before.saturating_sub(cal_node.meta.calendar_events.len());
                        if removed > 0 {
                            result.calendar_events_changed += removed;
                        } else {
                            result.dropped_ops_count += 1;
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::SetCalendarDayNote {
                    node_id,
                    date,
                    note,
                    append,
                } => {
                    let Some(clean_date) = normalize_calendar_date(date, reference_date) else {
                        result.dropped_ops_count += 1;
                        continue;
                    };
                    let explicit_id = node_id
                        .as_deref()
                        .and_then(|raw| self.resolve_node_id(raw, &temp_to_real))
                        .filter(|nid| {
                            self.nodes
                                .get(nid)
                                .is_some_and(|n| n.kind == NodeKind::Calendar)
                        });

                    let existing_cal_id = explicit_id.or_else(|| self.first_calendar_node_id());
                    let target_cal_id = if let Some(cid) = existing_cal_id {
                        cid
                    } else {
                        let spawn_pos = centroid_spawn_pos(
                            self,
                            &[],
                            &temp_to_real,
                            fallback_count,
                            fallback_origin,
                        );
                        fallback_count += 1;
                        created_count += 1;
                        let mut cal = Node::new(NodeKind::Calendar, "Study Calendar", spawn_pos);
                        cal.meta.calendar_view_mode = CalendarViewMode::Week;
                        cal.size = (680.0, 200.0);
                        let cid = cal.id.clone();
                        self.add_node(cal.clone());
                        result.new_nodes.push(cal);
                        cid
                    };

                    if let Some(cal_node) = self.get_node_mut(&target_cal_id) {
                        let new_text = note.trim();
                        if *append && !new_text.is_empty() {
                            let entry = cal_node
                                .meta
                                .calendar_day_notes
                                .entry(clean_date.clone())
                                .or_default();
                            if !entry.trim().is_empty() {
                                entry.push('\n');
                            }
                            entry.push_str(new_text);
                        } else if new_text.is_empty() {
                            cal_node.meta.calendar_day_notes.remove(&clean_date);
                        } else {
                            cal_node
                                .meta
                                .calendar_day_notes
                                .insert(clean_date.clone(), new_text.to_string());
                        }
                        let mut d_parts = clean_date.split('-');
                        if let (Some(Ok(yy)), Some(Ok(mm))) = (
                            d_parts.next().map(str::parse::<i32>),
                            d_parts.next().map(str::parse::<u32>),
                        ) {
                            cal_node.meta.calendar_year = Some(yy);
                            cal_node.meta.calendar_month = Some(mm);
                        }
                        cal_node.meta.calendar_week_start = monday_of_week_ymd(&clean_date);
                        result.calendar_notes_changed += 1;
                        result.last_calendar_target = Some((target_cal_id, clean_date));
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::SetCalendarViewMode { node_id, mode } => {
                    let parsed_mode = if mode.trim().eq_ignore_ascii_case("week")
                        || mode.trim().eq_ignore_ascii_case("weekly")
                    {
                        CalendarViewMode::Week
                    } else {
                        CalendarViewMode::Month
                    };
                    let explicit_id = node_id
                        .as_deref()
                        .and_then(|raw| self.resolve_node_id(raw, &temp_to_real))
                        .filter(|nid| {
                            self.nodes
                                .get(nid)
                                .is_some_and(|n| n.kind == NodeKind::Calendar)
                        });
                    let existing_cal_id = explicit_id.or_else(|| self.first_calendar_node_id());
                    let target_cal_id = if let Some(cid) = existing_cal_id {
                        cid
                    } else {
                        let spawn_pos = centroid_spawn_pos(
                            self,
                            &[],
                            &temp_to_real,
                            fallback_count,
                            fallback_origin,
                        );
                        fallback_count += 1;
                        created_count += 1;
                        let mut cal = Node::new(NodeKind::Calendar, "Study Calendar", spawn_pos);
                        cal.meta.calendar_view_mode = parsed_mode;
                        cal.size = if parsed_mode == CalendarViewMode::Week {
                            (680.0, 200.0)
                        } else {
                            (312.0, 286.0)
                        };
                        let cid = cal.id.clone();
                        self.add_node(cal.clone());
                        result.new_nodes.push(cal);
                        cid
                    };
                    if let Some(cal_node) = self.get_node_mut(&target_cal_id) {
                        cal_node.meta.calendar_view_mode = parsed_mode;
                        cal_node.size = if parsed_mode == CalendarViewMode::Week {
                            (680.0, 200.0)
                        } else {
                            (312.0, 286.0)
                        };
                        if parsed_mode == CalendarViewMode::Week && cal_node.meta.calendar_week_start.is_none() {
                            if let Some(r) = reference_date.and_then(monday_of_week_ymd) {
                                cal_node.meta.calendar_week_start = Some(r);
                            }
                        }
                        result.updated_nodes.push((
                            target_cal_id,
                            format!(
                                "Switched calendar to {} mode",
                                if parsed_mode == CalendarViewMode::Week {
                                    "week"
                                } else {
                                    "month"
                                }
                            ),
                        ));
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::SearchImages {
                    query,
                    title,
                    count,
                    source_nodes,
                    x,
                    y,
                    pos,
                } => {
                    let clean_q = query.trim();
                    if clean_q.is_empty() {
                        result.dropped_ops_count += 1;
                        continue;
                    }
                    let explicit_base = resolve_explicit_pos(*x, *y, *pos);
                    let clamped_count = (*count).clamp(1, 3);
                    let mut created_ids = Vec::with_capacity(clamped_count);
                    for i in 0..clamped_count {
                        let spawn_pos = if let Some(base) = explicit_base {
                            WorldPos::new(base.x + (i as f32) * 300.0, base.y)
                        } else {
                            let p = centroid_spawn_pos(
                                self,
                                source_nodes,
                                &temp_to_real,
                                fallback_count,
                                fallback_origin,
                            );
                            fallback_count += 1;
                            p
                        };
                        created_count += 1;
                        let base_title = title
                            .as_deref()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .unwrap_or(clean_q);
                        let node_title = if clamped_count > 1 {
                            format!("{} ({})", base_title, i + 1)
                        } else {
                            base_title.to_string()
                        };
                        let mut img_node = Node::new(NodeKind::Image, node_title, spawn_pos);
                        img_node.size = (264.0, 200.0);
                        img_node.content = format!("Searching web images for: \"{}\"...", clean_q);
                        let img_id = img_node.id.clone();
                        self.add_node(img_node.clone());
                        result.new_nodes.push(img_node);
                        result.pending_image_searches.push(PendingImageSearch {
                            node_id: img_id.clone(),
                            query: clean_q.to_string(),
                        });
                        created_ids.push(img_id);
                    }
                    if let Some(first_src) = source_nodes
                        .iter()
                        .find_map(|s| self.resolve_node_id(s, &temp_to_real))
                    {
                        for img_id in created_ids {
                            if first_src != img_id {
                                let edge = Edge::proposed_by_llm(
                                    EdgeEndpoint {
                                        node: first_src.clone(),
                                        port: PortId::new(PortId::DEFAULT_OUT),
                                    },
                                    EdgeEndpoint {
                                        node: img_id,
                                        port: PortId::new(PortId::DEFAULT_IN),
                                    },
                                    EdgeRelation::DerivedFrom,
                                    "Visual reference",
                                    format!("Image search for '{}'", clean_q),
                                );
                                self.add_edge(edge.clone());
                                result.proposed_edges.push(edge);
                            }
                        }
                    }
                }

                PatchOp::AddMediaTimestamp {
                    node_id,
                    seconds,
                    label,
                } => {
                    let Some(nid) = self.resolve_node_id(node_id, &temp_to_real) else {
                        result.dropped_ops_count += 1;
                        continue;
                    };
                    if let Some(node) = self.get_node_mut(&nid) {
                        if matches!(node.kind, NodeKind::Video | NodeKind::Audio) {
                            let clean_secs = if seconds.is_finite() && *seconds >= 0.0 {
                                *seconds
                            } else {
                                0.0
                            };
                            let lbl = if label.trim().is_empty() {
                                format!("{}s", clean_secs.round() as u64)
                            } else {
                                label.trim().to_string()
                            };
                            node.meta.timestamps.push(MediaTimestamp {
                                id: uuid::Uuid::new_v4().to_string(),
                                seconds: clean_secs,
                                label: lbl,
                            });
                            node.meta.timestamps.sort_by(|a, b| {
                                a.seconds
                                    .partial_cmp(&b.seconds)
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            });
                            result.timestamps_added += 1;
                        } else {
                            result.dropped_ops_count += 1;
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::ProposeConnection {
                    from_node,
                    to_node,
                    relation,
                    label,
                    note,
                    confidence,
                    color,
                    color_category,
                    ..
                } => {
                    // Discard low confidence proposals
                    if *confidence < 0.5 {
                        result.dropped_ops_count += 1;
                        continue;
                    }

                    let Some(resolved_from) = self.resolve_node_id(from_node, &temp_to_real) else {
                        result.dropped_ops_count += 1;
                        continue;
                    };
                    let Some(resolved_to) = self.resolve_node_id(to_node, &temp_to_real) else {
                        result.dropped_ops_count += 1;
                        continue;
                    };

                    // Verify both endpoints exist and differ
                    if resolved_from == resolved_to {
                        result.dropped_ops_count += 1;
                        continue;
                    }

                    let edge_rel: EdgeRelation = relation.clone().into();

                    // Check deduplication
                    let exists = self.edges.values().any(|e| {
                        (e.from.node == resolved_from && e.to.node == resolved_to)
                            || (e.from.node == resolved_to && e.to.node == resolved_from)
                    });

                    if exists {
                        result.dropped_ops_count += 1;
                        continue;
                    }

                    let mut edge = Edge::proposed_by_llm(
                        EdgeEndpoint {
                            node: resolved_from,
                            port: PortId::new(PortId::DEFAULT_OUT),
                        },
                        EdgeEndpoint {
                            node: resolved_to,
                            port: PortId::new(PortId::DEFAULT_IN),
                        },
                        edge_rel,
                        label.clone(),
                        note.clone(),
                    );
                    if Self::apply_edge_color_choice(
                        &mut edge,
                        color.as_deref(),
                        color_category.as_deref(),
                    ) {
                        result.edges_colored += 1;
                    }

                    self.add_edge(edge.clone());
                    result.proposed_edges.push(edge);
                }

                PatchOp::AnnotateEdge {
                    edge_id,
                    relation,
                    label,
                    suggested_note,
                    color,
                    color_category,
                } => {
                    let eid = EdgeId::from_str(edge_id);
                    if let Some(edge) = self.get_edge_mut(&eid) {
                        if let Some(r) = relation {
                            edge.relation = r.clone().into();
                        }
                        if let Some(l) = label {
                            edge.label = l.clone();
                        }
                        if edge.note.is_empty() {
                            edge.note = suggested_note.clone();
                        } else if !suggested_note.trim().is_empty() {
                            edge.note.push_str(&format!("\n[AI Note]: {}", suggested_note));
                        }
                        if Self::apply_edge_color_choice(
                            edge,
                            color.as_deref(),
                            color_category.as_deref(),
                        ) {
                            result.edges_colored += 1;
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }

                PatchOp::SetEdgeColor {
                    edge_id,
                    from_node,
                    to_node,
                    color,
                    category,
                } => {
                    let target_eid = edge_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(EdgeId::from_str)
                        .filter(|eid| self.edges.contains_key(eid))
                        .or_else(|| {
                            let f = self.resolve_node_id(from_node.as_deref()?, &temp_to_real)?;
                            let t = self.resolve_node_id(to_node.as_deref()?, &temp_to_real)?;
                            self.edges
                                .values()
                                .find(|e| e.from.node == f && e.to.node == t)
                                .or_else(|| {
                                    self.edges
                                        .values()
                                        .find(|e| e.from.node == t && e.to.node == f)
                                })
                                .map(|e| e.id.clone())
                        });
                    if let Some(eid) = target_eid {
                        if let Some(edge) = self.get_edge_mut(&eid) {
                            if Self::apply_edge_color_choice(
                                edge,
                                Some(color.as_str()),
                                category.as_deref(),
                            ) {
                                result.edges_colored += 1;
                            } else {
                                result.dropped_ops_count += 1;
                            }
                        } else {
                            result.dropped_ops_count += 1;
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }
            }
        }

        for created in &mut result.new_nodes {
            if let Some(final_node) = self.nodes.get(&created.id) {
                *created = final_node.clone();
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_staging_with_temp_ids() {
        let mut g = StudyGraph::new("Patch Test");
        let n1 = Node::new(NodeKind::Document, "Paper 1", WorldPos::new(0.0, 0.0));
        let n2 = Node::new(NodeKind::Document, "Paper 2", WorldPos::new(200.0, 0.0));
        let id1 = n1.id.as_str().to_string();
        let id2 = n2.id.as_str().to_string();
        g.add_node(n1);
        g.add_node(n2);

        let patch = GraphPatch {
            summary: "Synthesize and connect papers".into(),
            ops: vec![
                PatchOp::CreateSynthesisNode {
                    temp_id: "tmp_1".into(),
                    title: "Comparative Analysis".into(),
                    markdown_body: "Both papers explore synaptic plasticity...".into(),
                    source_nodes: vec![id1.clone(), id2.clone()],
                    x: None,
                    y: None,
                    pos: None,
                    width: None,
                    height: None,
                },
                PatchOp::ProposeConnection {
                    from_node: id1.clone(),
                    to_node: "tmp_1".into(),
                    relation: SemanticRelation::DerivedFrom,
                    label: "Basis for review".into(),
                    note: "Provides foundation".into(),
                    evidence: vec![],
                    confidence: 0.9,
                    color: None,
                    color_category: None,
                },
                PatchOp::ProposeConnection {
                    from_node: "hallucinated_node".into(),
                    to_node: id2.clone(),
                    relation: SemanticRelation::Contradicts,
                    label: "Invalid link".into(),
                    note: "".into(),
                    evidence: vec![],
                    confidence: 0.9,
                    color: None,
                    color_category: None,
                },
            ],
        };

        let result = g.validate_and_stage_patch(&patch);

        assert_eq!(result.new_nodes.len(), 1);
        assert_eq!(result.proposed_edges.len(), 1);
        assert_eq!(result.dropped_ops_count, 1); // The hallucinated node link was dropped cleanly
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.edges.len(), 1);

        // Verify proposed edge status
        let edge = result.proposed_edges.first().unwrap();
        assert_eq!(edge.status, EdgeStatus::Proposed);
        assert_eq!(edge.provenance, EdgeProvenance::Llm);
    }

    #[test]
    fn test_create_study_node_patch_and_backward_compat_deserialization() {
        let mut g = StudyGraph::new("Study Patch Test");
        let doc = Node::new(NodeKind::Document, "Lecture Notes", WorldPos::new(100.0, 100.0));
        let doc_id = doc.id.as_str().to_string();
        g.add_node(doc);

        let patch = GraphPatch {
            summary: "Generate flashcard & checklist".into(),
            ops: vec![
                PatchOp::CreateStudyNode {
                    temp_id: "fc_1".into(),
                    kind: NodeKind::Flashcard,
                    title: "LTP Trigger".into(),
                    content: "Which receptor unblocks once postsynaptic depolarization expels Mg2+?".into(),
                    answer: Some("NMDA receptor".into()),
                    media_url: None,
                    source_nodes: vec![doc_id.clone()],
                    quiz_questions: vec![],
                    text_font: None,
                    text_size: None,
                    x: None,
                    y: None,
                    pos: None,
                    width: None,
                    height: None,
                },
                PatchOp::CreateStudyNode {
                    temp_id: "chk_1".into(),
                    kind: NodeKind::Checklist,
                    title: "Review Steps".into(),
                    content: "- [ ] Derive Goldman equation\n- [ ] Compare AMPA vs NMDA".into(),
                    answer: None,
                    media_url: None,
                    source_nodes: vec![doc_id.clone()],
                    quiz_questions: vec![],
                    text_font: None,
                    text_size: None,
                    x: None,
                    y: None,
                    pos: None,
                    width: None,
                    height: None,
                },
                PatchOp::ProposeConnection {
                    from_node: doc_id,
                    to_node: "fc_1".into(),
                    relation: SemanticRelation::DerivedFrom,
                    label: "Tests recall".into(),
                    note: "Key exam question".into(),
                    evidence: vec![],
                    confidence: 0.92,
                    color: None,
                    color_category: None,
                },
            ],
        };

        let result = g.validate_and_stage_patch(&patch);
        assert_eq!(result.new_nodes.len(), 2);
        assert_eq!(result.proposed_edges.len(), 1);
        let fc = &result.new_nodes[0];
        assert_eq!(fc.kind, NodeKind::Flashcard);
        assert_eq!(fc.meta.answer.as_deref(), Some("NMDA receptor"));
        assert_eq!(fc.meta.mastery.as_deref(), Some("new"));
        // Second node is spaced to avoid overlap
        assert!((result.new_nodes[1].pos.x - fc.pos.x - 290.0).abs() < 1e-3);

        // Legacy JSON without `preview_page` or `meta` must deserialize cleanly
        let legacy_json = r#"{
            "id": "legacy_1",
            "kind": "Note",
            "title": "Old Note",
            "content": "Legacy body",
            "pos": { "x": 12.0, "y": 34.0 },
            "size": [280.0, 180.0],
            "asset_hash": null,
            "summary": null,
            "tags": [],
            "ports": []
        }"#;
        let deserialized: Node = serde_json::from_str(legacy_json).expect("legacy node json");
        assert_eq!(deserialized.preview_page, 0);
        assert_eq!(deserialized.meta.answer, None);
        assert!(!deserialized.meta.section_framed);
    }

    #[test]
    fn test_forgiving_llm_patch_deserialization() {
        let mut g = StudyGraph::new("Forgiving Patch");
        let n1 = Node::new(NodeKind::Document, "Doc A", WorldPos::new(0.0, 0.0));
        let n2 = Node::new(NodeKind::Note, "Note B", WorldPos::new(300.0, 0.0));
        let id1 = n1.id.as_str().to_string();
        let id2 = n2.id.as_str().to_string();
        g.add_node(n1);
        g.add_node(n2);

        let edge = Edge::new(
            EdgeEndpoint {
                node: NodeId::from_str(&id1),
                port: PortId::new(PortId::DEFAULT_OUT),
            },
            EdgeEndpoint {
                node: NodeId::from_str(&id2),
                port: PortId::new(PortId::DEFAULT_IN),
            },
        );
        let eid = edge.id.as_str().to_string();
        g.add_edge(edge);

        // Simulate real LLM JSON using aliases (`from`/`to`, `note` on `annotate_edge`,
        // `content` on `create_synthesis_node`, lowercase `flashcard`, PascalCase `Supports`,
        // single string `source_nodes`, and omitted `confidence`).
        let llm_json = format!(
            r#"{{
                "summary": "Synthesized and annotated",
                "ops": [
                    {{
                        "op": "annotate_edge",
                        "edge_id": "{eid}",
                        "relation": "Supports",
                        "note": "Explains synaptic plasticity"
                    }},
                    {{
                        "op": "create_study_node",
                        "id": "tmp_fc",
                        "kind": "flashcard",
                        "title": "NMDA Question",
                        "prompt": "What expels Mg2+?",
                        "back": "Postsynaptic depolarization",
                        "source_nodes": "{id1}"
                    }},
                    {{
                        "op": "propose_connection",
                        "from": "{id1}",
                        "to": "tmp_fc",
                        "relation": "DerivedFrom",
                        "label": "Active recall"
                    }},
                    {{
                        "op": "create_synthesis_node",
                        "temp_id": "tmp_syn",
                        "title": "Synthesis",
                        "content": "Combined summary body",
                        "sources": ["{id1}", "{id2}"]
                    }}
                ]
            }}"#
        );

        let patch: GraphPatch =
            serde_json::from_str(&llm_json).expect("forgiving GraphPatch deserialization");
        let staged = g.validate_and_stage_patch(&patch);
        assert_eq!(staged.new_nodes.len(), 2);
        assert_eq!(staged.proposed_edges.len(), 1);
        assert_eq!(
            g.get_edge(&crate::model::EdgeId::from_str(&eid))
                .unwrap()
                .note,
            "Explains synaptic plasticity"
        );
    }

    #[test]
    fn test_direct_canvas_actions_media_calendar_and_snapshot_undo() {
        use crate::command::{CommandHistory, GraphCommand};

        let mut g = StudyGraph::new("Canvas Actions Test");
        let mut history = CommandHistory::new();

        let vid = Node::new(NodeKind::Video, "Neuro Lecture", WorldPos::new(50.0, 50.0));
        let vid_id = vid.id.clone();
        g.add_node(vid);

        let chk = {
            let mut n = Node::new(NodeKind::Checklist, "Tasks", WorldPos::new(400.0, 50.0));
            n.content = "- [ ] Watch intro\n- [ ] Review notes".into();
            n
        };
        let chk_id = chk.id.clone();
        g.add_node(chk);

        let before_nodes = g.nodes.clone();
        let before_edges = g.edges.clone();

        let patch = GraphPatch {
            summary: "Update video, toggle checklist, schedule review, bookmark timestamp".into(),
            ops: vec![
                PatchOp::UpdateNode {
                    node_id: vid_id.as_str().to_string(),
                    title: Some("Neuro Lecture 01".into()),
                    content: Some("Synaptic plasticity lecture".into()),
                    answer: None,
                    summary: None,
                    media_url: Some("https://youtu.be/dQw4w9WgXcQ".into()),
                    quiz_questions: None,
                },
                PatchOp::MoveNode {
                    node_id: chk_id.as_str().to_string(),
                    x: Some(480.0),
                    y: Some(90.0),
                    pos: None,
                    dx: None,
                    dy: None,
                    relative_to: None,
                    placement: None,
                    gap: None,
                    width: None,
                    height: None,
                },
                PatchOp::ToggleChecklistTask {
                    node_id: chk_id.as_str().to_string(),
                    task_index: 0,
                    checked: Some(true),
                },
                PatchOp::AddMediaTimestamp {
                    node_id: vid_id.as_str().to_string(),
                    seconds: 125.0,
                    label: "NMDA receptor gating".into(),
                },
                // Auto-creates a Study Calendar node since none existed
                PatchOp::AddCalendarEvent {
                    node_id: None,
                    date: "2026-11-22".into(),
                    time: Some("18:00".into()),
                    title: "Re-watch LTP section".into(),
                    notes: "Quiz afterwards".into(),
                    linked_node: Some(vid_id.as_str().to_string()),
                },
            ],
        };

        let res = g.validate_and_stage_patch(&patch);
        assert_eq!(res.dropped_ops_count, 0);
        assert_eq!(res.new_nodes.len(), 1); // auto-created Study Calendar
        assert_eq!(res.timestamps_added, 1);
        assert_eq!(res.calendar_events_changed, 1);

        let after_nodes = g.nodes.clone();
        let after_edges = g.edges.clone();
        history.push_applied(GraphCommand::Snapshot {
            before_nodes,
            before_edges,
            after_nodes,
            after_edges,
        });

        let cal_node = g
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Calendar)
            .expect("auto-created calendar");
        let cal_id = cal_node.id.as_str().to_string();
        assert_eq!(cal_node.meta.calendar_events.len(), 1);
        assert_eq!(cal_node.meta.calendar_events[0].title, "Re-watch LTP section");
        assert!(g.get_node(&chk_id).unwrap().content.contains("- [x] Watch intro"));
        assert!((g.get_node(&chk_id).unwrap().pos.x - 480.0).abs() < 1e-3);
        assert_eq!(g.get_node(&vid_id).unwrap().meta.timestamps.len(), 1);

        // Toggle & remove calendar event via patch
        let patch2 = GraphPatch {
            summary: "Complete and remove event".into(),
            ops: vec![
                PatchOp::ToggleCalendarEvent {
                    node_id: cal_id.clone(),
                    event_id_or_title: "Re-watch LTP".into(),
                    completed: Some(true),
                },
                PatchOp::RemoveCalendarEvent {
                    node_id: cal_id.clone(),
                    event_id_or_title: "Re-watch LTP".into(),
                },
            ],
        };
        let res2 = g.validate_and_stage_patch(&patch2);
        assert_eq!(res2.calendar_events_changed, 2);
        assert!(g.get_node(&NodeId::from_str(&cal_id)).unwrap().meta.calendar_events.is_empty());

        // Atomic snapshot undo restores state before patch 1
        assert!(history.undo(&mut g));
        assert_eq!(g.nodes.len(), 2);
        assert!(g.nodes.values().all(|n| n.kind != NodeKind::Calendar));
        assert_eq!(g.get_node(&vid_id).unwrap().title, "Neuro Lecture");
        assert!(g.get_node(&vid_id).unwrap().meta.timestamps.is_empty());
        assert!(g.get_node(&chk_id).unwrap().content.contains("- [ ] Watch intro"));
    }

    #[test]
    fn test_calendar_day_notes_fuzzy_resolution_and_spawn_origin() {
        let mut g = StudyGraph::new("Week Mode & Fuzzy Title");
        let note = Node::new(NodeKind::Note, "Calculus Notes", WorldPos::new(50.0, 50.0));
        let note_id = note.id.clone();
        g.add_node(note);

        // Deserialize string seconds in AddMediaTimestamp + update via title
        let json = r#"{
            "summary": "Week note + title edit + spawn origin",
            "ops": [
                { "op": "update_node", "node_id": "calculus notes", "content": "Updated via title" },
                { "op": "set_calendar_view_mode", "mode": "week" },
                { "op": "set_calendar_day_note", "date": "2026-11-23", "note": "Solve eigenvalues", "append": false },
                { "op": "set_calendar_day_note", "date": "2026-11-23", "note": "Practice proofs", "append": true },
                { "op": "create_study_node", "temp_id": "tmp_fc", "kind": "Flashcard", "title": "Eigenvalue Def", "content": "Av = lv", "answer": "Characteristic scalar" }
            ]
        }"#;
        let patch: GraphPatch = serde_json::from_str(json).unwrap();
        let res = g.validate_and_stage_patch_at(&patch, WorldPos::new(2400.0, 1200.0));
        assert_eq!(res.dropped_ops_count, 0);
        assert_eq!(res.calendar_notes_changed, 2);
        assert_eq!(g.get_node(&note_id).unwrap().content, "Updated via title");

        let cal = g
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Calendar)
            .expect("calendar created");
        assert_eq!(cal.meta.calendar_view_mode, CalendarViewMode::Week);
        assert_eq!(
            cal.meta.calendar_day_notes.get("2026-11-23").map(String::as_str),
            Some("Solve eigenvalues\nPractice proofs")
        );
        let fc = g
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Flashcard)
            .expect("flashcard created");
        assert!((fc.pos.x - 2690.0).abs() < 1e-2);
        assert!((fc.pos.y - 1200.0).abs() < 1e-2);

        // String MM:SS timestamp parsing
        let ts_json = r#"{ "op": "add_media_timestamp", "node_id": "x", "seconds": "01:35", "label": "Mark" }"#;
        let ts_op: PatchOp = serde_json::from_str(ts_json).unwrap();
        if let PatchOp::AddMediaTimestamp { seconds, .. } = ts_op {
            assert!((seconds - 95.0).abs() < 1e-6);
        } else {
            panic!("expected AddMediaTimestamp");
        }
    }

    #[test]
    fn test_calendar_date_normalization_and_search_images_staging() {
        let ref_day = Some("2026-11-21"); // Saturday
        assert_eq!(normalize_calendar_date("2026-11-23T18:00:00Z", ref_day).as_deref(), Some("2026-11-23"));
        assert_eq!(normalize_calendar_date("2026/11/5", ref_day).as_deref(), Some("2026-11-05"));
        assert_eq!(normalize_calendar_date("24/11/2026", ref_day).as_deref(), Some("2026-11-24"));
        assert_eq!(normalize_calendar_date("today", ref_day).as_deref(), Some("2026-11-21"));
        assert_eq!(normalize_calendar_date("tomorrow", ref_day).as_deref(), Some("2026-11-22"));
        assert_eq!(normalize_calendar_date("lunes", ref_day).as_deref(), Some("2026-11-23"));
        assert_eq!(normalize_calendar_date("Nov 25, 2026", ref_day).as_deref(), Some("2026-11-25"));

        let mut g = StudyGraph::new("Relative Calendar + Search Images");
        let note = Node::new(NodeKind::Note, "Cell Biology", WorldPos::new(100.0, 100.0));
        let note_id = note.id.as_str().to_string();
        g.add_node(note);

        let patch_json = format!(
            r#"{{
                "summary": "Schedule Monday + search organelle diagram",
                "ops": [
                    {{ "op": "set_calendar_day_note", "node_id": "calendar", "date": "lunes", "note": "Review mitochondria" }},
                    {{ "op": "search_images", "query": "mitochondria diagram", "title": "Mitochondria Diagram", "count": 2, "source_nodes": ["{}"] }}
                ]
            }}"#,
            note_id
        );
        let patch: GraphPatch = serde_json::from_str(&patch_json).unwrap();
        let res = g.validate_and_stage_patch_full(&patch, WorldPos::new(400.0, 200.0), ref_day);
        assert_eq!(res.dropped_ops_count, 0);
        assert_eq!(res.calendar_notes_changed, 1);
        assert_eq!(res.last_calendar_target.as_ref().map(|(_, d)| d.as_str()), Some("2026-11-23"));
        assert_eq!(res.pending_image_searches.len(), 2);
        assert_eq!(res.proposed_edges.len(), 2);

        let cal = g.nodes.values().find(|n| n.kind == NodeKind::Calendar).unwrap();
        assert_eq!(cal.meta.calendar_year, Some(2026));
        assert_eq!(cal.meta.calendar_month, Some(11));
        assert_eq!(cal.meta.calendar_week_start.as_deref(), Some("2026-11-23"));
        assert_eq!(
            cal.meta.calendar_day_notes.get("2026-11-23").map(String::as_str),
            Some("Review mitochondria")
        );
    }

    #[test]
    fn test_trace_colors_and_quiz_patch_ops() {
        let mut g = StudyGraph::new("Trace Colors & Quiz");
        let a = Node::new(NodeKind::Note, "Topic A", WorldPos::new(0.0, 0.0));
        let b = Node::new(NodeKind::Note, "Topic B", WorldPos::new(300.0, 0.0));
        let aid = a.id.as_str().to_string();
        let bid = b.id.as_str().to_string();
        g.add_node(a);
        g.add_node(b);

        let patch_json = format!(
            r#"{{
                "summary": "Create quiz, color trace, and modify questions",
                "ops": [
                    {{
                        "op": "propose_connection",
                        "from_node": "{aid}",
                        "to_node": "{bid}",
                        "relation": "supports",
                        "label": "Core theorem",
                        "color": "critical",
                        "category": "Relevance: High"
                    }},
                    {{
                        "op": "create_study_node",
                        "temp_id": "tmp_quiz",
                        "kind": "Quiz",
                        "title": "Synapse Check",
                        "quiz_questions": [
                            {{
                                "question": "Which ion enters via NMDA channels?",
                                "options": ["Ca2+", "Cl-", "K+"],
                                "correct_index": 0,
                                "answer": "Ca2+ triggers downstream LTP kinases.",
                                "hints": ["Think second messenger in LTP."]
                            }}
                        ],
                        "source_nodes": ["{aid}"]
                    }},
                    {{
                        "op": "add_quiz_question",
                        "node_id": "tmp_quiz",
                        "question": "What blocks NMDA at rest?",
                        "options": ["Mg2+", "Na+"],
                        "correct_index": 0,
                        "answer": "Extracellular Mg2+.",
                        "hint": "Removed by depolarization"
                    }},
                    {{
                        "op": "update_quiz_question",
                        "node_id": "tmp_quiz",
                        "question_index": 0,
                        "add_hint": "Divalent cation"
                    }}
                ]
            }}"#
        );

        let patch: GraphPatch = serde_json::from_str(&patch_json).unwrap();
        let res = g.validate_and_stage_patch(&patch);
        assert_eq!(res.dropped_ops_count, 0);
        assert_eq!(res.proposed_edges.len(), 1);
        assert_eq!(res.edges_colored, 1);
        assert_eq!(res.new_nodes.len(), 1);

        let edge = g.edges.values().find(|e| e.label == "Core theorem").unwrap();
        assert_eq!(edge.color_hex.as_deref(), Some("#E06C75"));
        assert_eq!(edge.color_category.as_deref(), Some("Relevance: High"));

        // Re-color via set_edge_color using from/to lookup
        let recolor_json = format!(
            r#"{{
                "summary": "Switch trace color by topic",
                "ops": [
                    {{ "op": "set_edge_color", "from_node": "{aid}", "to_node": "{bid}", "color": "theory", "category": "Neuroscience Theory" }},
                    {{ "op": "remove_quiz_question", "node_id": "Synapse Check", "question_index": 1 }}
                ]
            }}"#
        );
        let patch2: GraphPatch = serde_json::from_str(&recolor_json).unwrap();
        let res2 = g.validate_and_stage_patch(&patch2);
        assert_eq!(res2.dropped_ops_count, 0);
        assert_eq!(res2.edges_colored, 1);

        let updated_edge = g.edges.values().find(|e| e.label == "Core theorem").unwrap();
        assert_eq!(updated_edge.color_hex.as_deref(), Some("#5B9BD5"));
        assert_eq!(
            updated_edge.color_category.as_deref(),
            Some("Neuroscience Theory")
        );

        let quiz = g.nodes.values().find(|n| n.kind == NodeKind::Quiz).unwrap();
        assert_eq!(quiz.meta.quiz_questions.len(), 1);
        assert_eq!(quiz.meta.quiz_questions[0].hints.len(), 2);
    }

    fn boxes_overlap(a: &Node, b: &Node) -> bool {
        let (aw, ah) = a.effective_size();
        let (bw, bh) = b.effective_size();
        let ax2 = a.pos.x + aw;
        let ay2 = a.pos.y + ah;
        let bx2 = b.pos.x + bw;
        let by2 = b.pos.y + bh;
        a.pos.x < bx2 - 0.5 && ax2 > b.pos.x + 0.5 && a.pos.y < by2 - 0.5 && ay2 > b.pos.y + 0.5
    }

    fn assert_no_node_overlaps(g: &StudyGraph, ids: &[NodeId], label: &str) {
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let a = g.get_node(&ids[i]).unwrap();
                let b = g.get_node(&ids[j]).unwrap();
                assert!(
                    !boxes_overlap(a, b),
                    "{label}: overlap between '{}' ({:?}, {:?}) and '{}' ({:?}, {:?})",
                    a.title,
                    a.pos,
                    a.effective_size(),
                    b.title,
                    b.pos,
                    b.effective_size()
                );
            }
        }
    }

    #[test]
    fn test_explicit_creation_positions_and_move_node_modes() {
        let mut g = StudyGraph::new("Spatial Create & Move");
        let root = Node::new(NodeKind::Note, "Center Concept", WorldPos::new(200.0, 150.0));
        let root_id = root.id.clone();
        g.add_node(root);

        let json = format!(
            r#"{{
                "summary": "Spawn nodes at exact coordinates and resize section",
                "ops": [
                    {{
                        "op": "create_study_node",
                        "temp_id": "tmp_sec",
                        "kind": "Section",
                        "title": "Overview Frame",
                        "x": 140.0,
                        "y": 90.0,
                        "width": 760.0,
                        "height": 480.0
                    }},
                    {{
                        "op": "create_study_node",
                        "temp_id": "tmp_right",
                        "kind": "Note",
                        "title": "Right Branch",
                        "content": "Placed via [x, y] pos array",
                        "pos": [580.0, 150.0]
                    }},
                    {{
                        "op": "create_synthesis_node",
                        "temp_id": "tmp_syn",
                        "title": "Bottom Synthesis",
                        "markdown_body": "Placed via object pos",
                        "pos": {{ "x": 380.0, "y": 410.0 }},
                        "source_nodes": ["{}", "tmp_right"]
                    }},
                    {{
                        "op": "move_node",
                        "node_id": "{}",
                        "x": 220.0
                    }},
                    {{
                        "op": "move_node",
                        "node_id": "tmp_rel",
                        "x": 10.0
                    }}
                ]
            }}"#,
            root_id.as_str(),
            root_id.as_str()
        );
        let patch: GraphPatch = serde_json::from_str(&json).unwrap();
        let res = g.validate_and_stage_patch(&patch);
        assert_eq!(res.new_nodes.len(), 3);
        assert_eq!(res.nodes_moved, 1);
        assert_eq!(res.dropped_ops_count, 1);

        let sec = &res.new_nodes[0];
        assert!((sec.pos.x - 140.0).abs() < 1e-3 && (sec.pos.y - 90.0).abs() < 1e-3);
        assert!((sec.size.0 - 760.0).abs() < 1e-3 && (sec.size.1 - 480.0).abs() < 1e-3);

        let right = &res.new_nodes[1];
        assert!((right.pos.x - 580.0).abs() < 1e-3 && (right.pos.y - 150.0).abs() < 1e-3);

        let syn = &res.new_nodes[2];
        assert!((syn.pos.x - 380.0).abs() < 1e-3 && (syn.pos.y - 410.0).abs() < 1e-3);

        // Single-axis move updated x while preserving original y (150.0)
        let root_after = g.get_node(&root_id).unwrap();
        assert!((root_after.pos.x - 220.0).abs() < 1e-3);
        assert!((root_after.pos.y - 150.0).abs() < 1e-3);

        // Relative placement (`right`, `below`, `left`, `above`), delta (`dx`/`dy`), and dropping empty MoveNode
        let rel_json = format!(
            r#"{{
                "summary": "Relative & delta positioning",
                "ops": [
                    {{
                        "op": "move_node",
                        "node_id": "Right Branch",
                        "relative_to": "{}",
                        "placement": "below",
                        "gap": 60.0
                    }},
                    {{
                        "op": "move_node",
                        "node_id": "Bottom Synthesis",
                        "dx": 50.0,
                        "dy": -30.0
                    }},
                    {{
                        "op": "move_node",
                        "node_id": "{}"
                    }}
                ]
            }}"#,
            root_id.as_str(),
            root_id.as_str()
        );
        let patch2: GraphPatch = serde_json::from_str(&rel_json).unwrap();
        let res2 = g.validate_and_stage_patch(&patch2);
        assert_eq!(res2.nodes_moved, 2);
        assert_eq!(res2.dropped_ops_count, 1);

        let right_after = g.get_node(&right.id).unwrap();
        // root is at (220, 150) with height 180 -> below + gap 60 = 150 + 180 + 60 = 390
        assert!((right_after.pos.x - 220.0).abs() < 1e-3);
        assert!((right_after.pos.y - 390.0).abs() < 1e-3);

        let syn_after = g.get_node(&syn.id).unwrap();
        assert!((syn_after.pos.x - 430.0).abs() < 1e-3);
        assert!((syn_after.pos.y - 380.0).abs() < 1e-3);

        // Empty move_node did NOT teleport root to (0, 0)
        assert!((g.get_node(&root_id).unwrap().pos.x - 220.0).abs() < 1e-3);
    }

    #[test]
    fn test_arrange_nodes_batch_map_and_scheme_layouts_no_overlap() {
        let mut g = StudyGraph::new("Scheme Layouts");
        let n_root = Node::new(NodeKind::Concept, "Central Nervous System", WorldPos::new(0.0, 0.0));
        let n_a = Node::new(NodeKind::Note, "Brain", WorldPos::new(0.0, 0.0));
        let n_b = Node::new(NodeKind::Note, "Spinal Cord", WorldPos::new(0.0, 0.0));
        let n_c = Node::new(NodeKind::Flashcard, "Reflex Arc", WorldPos::new(0.0, 0.0));
        let n_d = Node::new(NodeKind::Quiz, "Motor Pathways", WorldPos::new(0.0, 0.0));
        let n_e = Node::new(NodeKind::Note, "Cerebellum", WorldPos::new(0.0, 0.0));

        let ids = vec![
            n_root.id.clone(),
            n_a.id.clone(),
            n_b.id.clone(),
            n_c.id.clone(),
            n_d.id.clone(),
            n_e.id.clone(),
        ];
        for n in [n_root, n_a, n_b, n_c, n_d, n_e] {
            g.add_node(n);
        }

        let connect = |g: &mut StudyGraph, from: &NodeId, to: &NodeId| {
            g.add_edge(Edge::new(
                EdgeEndpoint {
                    node: from.clone(),
                    port: PortId::new(PortId::DEFAULT_OUT),
                },
                EdgeEndpoint {
                    node: to.clone(),
                    port: PortId::new(PortId::DEFAULT_IN),
                },
            ));
        };
        connect(&mut g, &ids[0], &ids[1]);
        connect(&mut g, &ids[0], &ids[2]);
        connect(&mut g, &ids[2], &ids[3]);
        connect(&mut g, &ids[1], &ids[4]);
        connect(&mut g, &ids[1], &ids[5]);
        // Cycle back-edge to ensure BFS guard never hangs
        connect(&mut g, &ids[4], &ids[0]);

        for layout in ["hierarchy", "flow", "radial", "grid"] {
            let json = serde_json::json!({
                "summary": format!("Arrange as {layout}"),
                "ops": [{
                    "op": "arrange_nodes",
                    "layout": layout,
                    "root_node": ids[0].as_str(),
                    "origin_x": 100.0,
                    "origin_y": 80.0
                }]
            });
            let patch: GraphPatch = serde_json::from_value(json).unwrap();
            let res = g.validate_and_stage_patch(&patch);
            assert_eq!(res.dropped_ops_count, 0, "layout {layout} dropped ops");
            assert_eq!(res.nodes_moved, ids.len(), "layout {layout} moved count");
            assert_no_node_overlaps(&g, &ids, layout);
        }

        // Batch `positions` map syntax (`move_nodes` alias) + temp_id inside same patch
        let map_json = serde_json::json!({
            "summary": "Explicit coordinate scheme map",
            "ops": [
                {
                    "op": "create_study_node",
                    "temp_id": "tmp_new",
                    "kind": "Note",
                    "title": "New Leaf"
                },
                {
                    "op": "move_nodes",
                    "positions": {
                        (ids[0].as_str()): [500.0, 40.0],
                        (ids[1].as_str()): { "x": 120.0, "y": 300.0, "width": 320.0 },
                        "tmp_new": [860.0, 300.0]
                    }
                }
            ]
        });
        let patch_map: GraphPatch = serde_json::from_value(map_json).unwrap();
        let res_map = g.validate_and_stage_patch(&patch_map);
        assert_eq!(res_map.dropped_ops_count, 0);
        assert_eq!(res_map.nodes_moved, 3);
        let r0 = g.get_node(&ids[0]).unwrap();
        assert!((r0.pos.x - 500.0).abs() < 1e-3 && (r0.pos.y - 40.0).abs() < 1e-3);
        let r1 = g.get_node(&ids[1]).unwrap();
        assert!((r1.pos.x - 120.0).abs() < 1e-3 && (r1.pos.y - 300.0).abs() < 1e-3);
        assert!((r1.size.0 - 320.0).abs() < 1e-3);
        let new_leaf = &res_map.new_nodes[0];
        assert!((new_leaf.pos.x - 860.0).abs() < 1e-3 && (new_leaf.pos.y - 300.0).abs() < 1e-3);
    }
}
