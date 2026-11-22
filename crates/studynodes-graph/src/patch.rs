use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::coords::WorldPos;
use crate::graph::StudyGraph;
use crate::model::{
    CalendarEvent, CalendarViewMode, Edge, EdgeEndpoint, EdgeRelation, MediaTimestamp, Node,
    NodeId, NodeKind, PortId,
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
        "excerpt" | "quote" | "citation" => NodeKind::Excerpt,
        "sequence" | "step" => NodeKind::Sequence,
        "section" | "group" | "frame" => NodeKind::Section,
        "video" | "youtube" | "vimeo" => NodeKind::Video,
        "audio" | "music" | "song" | "track" | "spotify" | "podcast" => NodeKind::Audio,
        "calendar" | "schedule" | "planner" | "agenda" => NodeKind::Calendar,
        "drawing" | "sketch" | "whiteboard" | "handwriting" => NodeKind::Drawing,
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
    },

    /// Directly edits an existing node's title, body content, flashcard answer, summary, or media URL.
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
    },

    /// Deletes a node (and cascade-removes its incident edges).
    #[serde(alias = "remove_node", alias = "DeleteNode")]
    DeleteNode {
        #[serde(default, alias = "id")]
        node_id: String,
    },

    /// Moves a node to explicit world `(x, y)` coordinates on the canvas.
    #[serde(alias = "reposition_node", alias = "MoveNode")]
    MoveNode {
        #[serde(default, alias = "id")]
        node_id: String,
        #[serde(default)]
        x: f32,
        #[serde(default)]
        y: f32,
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
    pub deleted_nodes: Vec<NodeId>,
    pub calendar_events_changed: usize,
    pub calendar_notes_changed: usize,
    pub last_calendar_target: Option<(NodeId, String)>,
    pub timestamps_added: usize,
    pub pending_image_searches: Vec<PendingImageSearch>,
    pub dropped_ops_count: usize,
}

impl StagedPatchResult {
    pub fn total_changes(&self) -> usize {
        self.new_nodes.len()
            + self.proposed_edges.len()
            + self.updated_nodes.len()
            + self.deleted_nodes.len()
            + self.calendar_events_changed
            + self.calendar_notes_changed
            + self.timestamps_added
    }

    pub fn summary_line(&self) -> String {
        let mut parts = Vec::new();
        if !self.new_nodes.is_empty() {
            parts.push(format!("+{} node(s)", self.new_nodes.len()));
        }
        if !self.updated_nodes.is_empty() {
            parts.push(format!("~{} updated", self.updated_nodes.len()));
        }
        if !self.deleted_nodes.is_empty() {
            parts.push(format!("-{} removed", self.deleted_nodes.len()));
        }
        if !self.proposed_edges.is_empty() {
            parts.push(format!("+{} link(s)", self.proposed_edges.len()));
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

    let dx = (sibling_offset as f32) * 290.0;
    if valid_count > 0 {
        WorldPos::new(
            sum_x / valid_count as f32 + dx,
            sum_y / valid_count as f32 + 220.0,
        )
    } else {
        WorldPos::new(fallback_origin.x + dx, fallback_origin.y)
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

        for op in &patch.ops {
            match op {
                PatchOp::CreateSynthesisNode {
                    temp_id,
                    title,
                    markdown_body,
                    source_nodes,
                } => {
                    let spawn_pos = centroid_spawn_pos(
                        self,
                        source_nodes,
                        &temp_to_real,
                        created_count,
                        fallback_origin,
                    );
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
                } => {
                    let spawn_pos = centroid_spawn_pos(
                        self,
                        source_nodes,
                        &temp_to_real,
                        created_count,
                        fallback_origin,
                    );
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
                        _ => {}
                    }
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
                            let desc = title.clone().unwrap_or_else(|| node.title.clone());
                            result.updated_nodes.push((nid, desc));
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

                PatchOp::MoveNode { node_id, x, y } => {
                    if let Some(nid) = self.resolve_node_id(node_id, &temp_to_real) {
                        if let Some(node) = self.get_node_mut(&nid) {
                            node.pos = WorldPos::new(*x, *y);
                            let desc = format!("Moved {}", node.title);
                            result.updated_nodes.push((nid, desc));
                        } else {
                            result.dropped_ops_count += 1;
                        }
                    } else {
                        result.dropped_ops_count += 1;
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
                            created_count,
                            fallback_origin,
                        );
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
                            created_count,
                            fallback_origin,
                        );
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
                            created_count,
                            fallback_origin,
                        );
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
                } => {
                    let clean_q = query.trim();
                    if clean_q.is_empty() {
                        result.dropped_ops_count += 1;
                        continue;
                    }
                    let clamped_count = (*count).clamp(1, 3);
                    let mut created_ids = Vec::with_capacity(clamped_count);
                    for i in 0..clamped_count {
                        let spawn_pos = centroid_spawn_pos(
                            self,
                            source_nodes,
                            &temp_to_real,
                            created_count,
                            fallback_origin,
                        );
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

                    let edge = Edge::proposed_by_llm(
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

                    self.add_edge(edge.clone());
                    result.proposed_edges.push(edge);
                }

                PatchOp::AnnotateEdge {
                    edge_id,
                    relation,
                    label,
                    suggested_note,
                } => {
                    let eid = crate::model::EdgeId::from_str(edge_id);
                    if let Some(edge) = self.get_edge_mut(&eid) {
                        if let Some(r) = relation {
                            edge.relation = r.clone().into();
                        }
                        if let Some(l) = label {
                            edge.label = l.clone();
                        }
                        if edge.note.is_empty() {
                            edge.note = suggested_note.clone();
                        } else {
                            edge.note.push_str(&format!("\n[AI Note]: {}", suggested_note));
                        }
                    } else {
                        result.dropped_ops_count += 1;
                    }
                }
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
                },
                PatchOp::ProposeConnection {
                    from_node: id1.clone(),
                    to_node: "tmp_1".into(),
                    relation: SemanticRelation::DerivedFrom,
                    label: "Basis for review".into(),
                    note: "Provides foundation".into(),
                    evidence: vec![],
                    confidence: 0.9,
                },
                PatchOp::ProposeConnection {
                    from_node: "hallucinated_node".into(),
                    to_node: id2.clone(),
                    relation: SemanticRelation::Contradicts,
                    label: "Invalid link".into(),
                    note: "".into(),
                    evidence: vec![],
                    confidence: 0.9,
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
                },
                PatchOp::CreateStudyNode {
                    temp_id: "chk_1".into(),
                    kind: NodeKind::Checklist,
                    title: "Review Steps".into(),
                    content: "- [ ] Derive Goldman equation\n- [ ] Compare AMPA vs NMDA".into(),
                    answer: None,
                    media_url: None,
                    source_nodes: vec![doc_id.clone()],
                },
                PatchOp::ProposeConnection {
                    from_node: doc_id,
                    to_node: "fc_1".into(),
                    relation: SemanticRelation::DerivedFrom,
                    label: "Tests recall".into(),
                    note: "Key exam question".into(),
                    evidence: vec![],
                    confidence: 0.92,
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
                },
                PatchOp::MoveNode {
                    node_id: chk_id.as_str().to_string(),
                    x: 480.0,
                    y: 90.0,
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
}
