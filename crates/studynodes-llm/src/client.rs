use std::time::Duration;

use serde_json::json;
use thiserror::Error;
use studynodes_graph::patch::{GraphPatch, PatchOp, SemanticRelation};

use crate::context::LlmContext;
use crate::prompt::{
    CANVAS_AGENT_CHAT_SYSTEM_PROMPT, CHAT_SYSTEM_PROMPT, SUMMARIZE_SYSTEM_PROMPT, SYSTEM_PROMPT,
};

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("API error: {0}")]
    Api(String),
    #[error("Missing API Key")]
    MissingApiKey,
}

/// Strip optional markdown code fences (` ```json ... ``` ` or ` ``` ... ``` `) and any
/// surrounding prose around a JSON object response.
pub fn strip_json_fences(raw: &str) -> &str {
    let s = raw.trim();
    let unfenced = if let Some(after_open) = s.strip_prefix("```") {
        let body = if let Some(nl) = after_open.find('\n') {
            &after_open[nl + 1..]
        } else {
            after_open.strip_prefix("json").unwrap_or(after_open)
        };
        body.strip_suffix("```").unwrap_or(body).trim()
    } else {
        s
    };
    if let (Some(start), Some(end)) = (unfenced.find('{'), unfenced.rfind('}')) {
        if start <= end {
            return &unfenced[start..=end];
        }
    }
    unfenced
}

fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

fn build_summarize_prompt(context: &LlmContext) -> String {
    let Some(primary) = context.focus_nodes.first() else {
        return "No focused node content provided.".to_string();
    };

    let mut prompt = format!(
        "Title: {}\nKind: {}\n",
        primary.title.trim(),
        primary.kind
    );
    if !primary.tags.is_empty() {
        prompt.push_str(&format!("Tags: {}\n", primary.tags.join(", ")));
    }
    if let Some(lang) = context
        .preferred_language
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        prompt.push_str(&format!("Output Language: Write the summary in {}.\n", lang));
    }
    if !context.neighbor_nodes.is_empty() {
        let neighbors: Vec<&str> = context
            .neighbor_nodes
            .iter()
            .take(8)
            .map(|n| n.title.as_str())
            .collect();
        prompt.push_str(&format!("Connected neighbors: {}\n", neighbors.join(", ")));
    }
    prompt.push_str("\nContent:\n");
    if primary.content.trim().is_empty() {
        prompt.push_str("(empty)");
    } else {
        let truncated: String = primary.content.chars().take(12_000).collect();
        prompt.push_str(&truncated);
    }
    if context.focus_nodes.len() > 1 {
        prompt.push_str("\n\nGrouped Section Cards:\n");
        for child in context.focus_nodes.iter().skip(1).take(16) {
            let body: String = child.content.trim().chars().take(800).collect();
            let snippet = if body.is_empty() {
                "(empty)".to_string()
            } else {
                body
            };
            prompt.push_str(&format!(
                "- {} [{}]: {}\n",
                child.title.trim(),
                child.kind,
                snippet
            ));
        }
    }
    if !context.existing_edges.is_empty() {
        prompt.push_str("\nConnections:\n");
        let title_for = |id: &str| -> String {
            context
                .focus_nodes
                .iter()
                .find(|n| n.node_id == id)
                .map(|n| n.title.clone())
                .or_else(|| {
                    context
                        .neighbor_nodes
                        .iter()
                        .find(|n| n.node_id == id)
                        .map(|n| n.title.clone())
                })
                .unwrap_or_else(|| id.to_string())
        };
        for edge in context.existing_edges.iter().take(16) {
            prompt.push_str(&format!(
                "- {} --[{}]--> {} {}\n",
                title_for(&edge.from),
                edge.relation,
                title_for(&edge.to),
                edge.label
            ));
        }
    }
    prompt
}

#[derive(Debug, Clone)]
pub struct ChatActionResponse {
    pub reply: String,
    pub patch: Option<GraphPatch>,
    pub parse_warning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AudioTranscriptionRequest {
    pub bytes: Vec<u8>,
    pub filename: String,
    pub mime_type: String,
    pub language_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioTranscriptSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct AudioTranscriptionResult {
    pub text: String,
    pub segments: Vec<AudioTranscriptSegment>,
}

fn strip_trailing_json_commas(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escaped = false;
    let n = chars.len();

    for i in 0..n {
        let ch = chars[i];
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }
        if ch == ',' {
            let mut j = i + 1;
            while j < n && chars[j].is_whitespace() {
                j += 1;
            }
            if j < n && (chars[j] == '}' || chars[j] == ']') {
                continue;
            }
        }
        out.push(ch);
    }
    out
}

fn normalize_op_value(val: &mut serde_json::Value) {
    let Some(obj) = val.as_object_mut() else {
        return;
    };
    if obj.contains_key("op") {
        if let Some(serde_json::Value::String(op_s)) = obj.get("op").cloned() {
            let clean = op_s.trim().replace('-', "_");
            obj.insert("op".into(), serde_json::Value::String(clean));
        }
        return;
    }
    let alt_key = if obj.contains_key("action") {
        Some("action")
    } else if obj.contains_key("type") {
        Some("type")
    } else {
        None
    };
    if let Some(k) = alt_key {
        if let Some(serde_json::Value::String(s)) = obj.get(k).cloned() {
            let norm = s.trim().replace(['-', ' '], "_");
            obj.insert("op".into(), serde_json::Value::String(norm));
        }
    }
}

fn parse_patch_lenient(candidate: &str) -> Result<(GraphPatch, usize), String> {
    let cleaned = strip_trailing_json_commas(candidate.trim());
    let val: serde_json::Value =
        serde_json::from_str(&cleaned).map_err(|e| format!("Invalid canvas action JSON: {}", e))?;

    let (summary, raw_ops) = match val {
        serde_json::Value::Object(ref map) => {
            let summary = map
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let arr_opt = map
                .get("ops")
                .or_else(|| map.get("actions"))
                .or_else(|| map.get("operations"))
                .or_else(|| map.get("changes"))
                .and_then(|v| v.as_array())
                .cloned();
            let list = if let Some(arr) = arr_opt {
                arr
            } else if map.contains_key("op")
                || map.contains_key("action")
                || map.contains_key("type")
            {
                vec![val.clone()]
            } else {
                return Err("Canvas action JSON object missing \"ops\" array".into());
            };
            (summary, list)
        }
        serde_json::Value::Array(arr) => (String::new(), arr),
        _ => return Err("Expected JSON object or array for canvas actions".into()),
    };

    let mut expanded_items: Vec<serde_json::Value> = Vec::with_capacity(raw_ops.len());
    for mut item in raw_ops {
        normalize_op_value(&mut item);
        let op_name = item
            .get("op")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            op_name.as_str(),
            "set_calendar_week_notes" | "set_week_notes" | "plan_week" | "update_calendar_week"
        ) {
            if let Some(map_obj) = item
                .get("notes")
                .or_else(|| item.get("days"))
                .or_else(|| item.get("day_notes"))
                .and_then(|v| v.as_object())
            {
                let node_id = item.get("node_id").cloned();
                let append = item.get("append").and_then(|v| v.as_bool()).unwrap_or(false);
                if let Some(nid) = &node_id {
                    expanded_items.push(json!({
                        "op": "set_calendar_view_mode",
                        "node_id": nid,
                        "mode": "week"
                    }));
                } else {
                    expanded_items.push(json!({
                        "op": "set_calendar_view_mode",
                        "mode": "week"
                    }));
                }
                for (date_k, note_v) in map_obj {
                    let note_str = note_v.as_str().unwrap_or("");
                    let mut entry = json!({
                        "op": "set_calendar_day_note",
                        "date": date_k,
                        "note": note_str,
                        "append": append
                    });
                    if let Some(nid) = &node_id {
                        entry["node_id"] = nid.clone();
                    }
                    expanded_items.push(entry);
                }
                continue;
            }
        }
        expanded_items.push(item);
    }

    let mut parsed_ops = Vec::new();
    let mut dropped = 0usize;
    let mut last_err = None;
    for item in expanded_items {
        match serde_json::from_value::<studynodes_graph::patch::PatchOp>(item) {
            Ok(op) => parsed_ops.push(op),
            Err(e) => {
                dropped += 1;
                last_err = Some(e.to_string());
            }
        }
    }

    if parsed_ops.is_empty() && dropped > 0 {
        return Err(format!(
            "Could not parse canvas operations: {}",
            last_err.unwrap_or_else(|| "unknown error".into())
        ));
    }

    Ok((
        GraphPatch {
            summary,
            ops: parsed_ops,
        },
        dropped,
    ))
}

struct FencedSpan {
    start: usize,
    end: usize,
    lang: String,
    body_start: usize,
    body_end: usize,
}

fn find_fenced_blocks(raw: &str) -> Vec<FencedSpan> {
    let mut blocks = Vec::new();
    let mut pos = 0usize;
    let bytes = raw.as_bytes();
    let mut active_open: Option<(usize, String, usize)> = None;

    while pos < raw.len() {
        let line_end_rel = bytes[pos..].iter().position(|&b| b == b'\n');
        let line_end = match line_end_rel {
            Some(rel) => pos + rel,
            None => raw.len(),
        };
        let next_pos = if line_end < raw.len() {
            line_end + 1
        } else {
            raw.len()
        };
        let line = raw[pos..line_end].trim_end_matches('\r');
        let trimmed = line.trim();

        if let Some((open_start, lang, body_start)) = active_open.as_ref() {
            if trimmed.starts_with("```") {
                blocks.push(FencedSpan {
                    start: *open_start,
                    end: next_pos,
                    lang: lang.clone(),
                    body_start: *body_start,
                    body_end: pos,
                });
                active_open = None;
            }
        } else if let Some(rest) = trimmed.strip_prefix("```") {
            let lang = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            active_open = Some((pos, lang, next_pos));
        }
        pos = next_pos;
    }

    if let Some((open_start, lang, body_start)) = active_open {
        blocks.push(FencedSpan {
            start: open_start,
            end: raw.len(),
            lang,
            body_start: body_start.min(raw.len()),
            body_end: raw.len(),
        });
    }

    blocks
}

fn find_trailing_json_object(raw: &str) -> Option<(usize, usize)> {
    let trimmed = raw.trim_end();
    if !trimmed.ends_with('}') {
        return None;
    }
    let end = trimmed.len();
    let chars: Vec<(usize, char)> = trimmed.char_indices().collect();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut start_idx = None;

    // Scan forward to find balanced top-level `{ ... }` ending right at `trimmed.len()`.
    let mut stack: Vec<usize> = Vec::new();
    for (byte_idx, ch) in chars {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    stack.push(byte_idx);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    start_idx = stack.pop();
                } else if depth < 0 {
                    depth = 0;
                    stack.clear();
                    start_idx = None;
                }
            }
            _ => {}
        }
    }

    let s = start_idx?;
    let slice = &trimmed[s..end];
    if slice.contains("\"ops\"") || slice.contains("\"actions\"") || slice.contains("\"operations\"") {
        Some((s, end))
    } else {
        None
    }
}

/// Extract an optional `canvas_actions` (or `json` containing ops) block from an AI chat response,
/// returning the clean Markdown conversational reply, parsed `GraphPatch`, and any parse warning.
pub fn parse_chat_action_response(raw: &str) -> ChatActionResponse {
    let blocks = find_fenced_blocks(raw);
    let chosen_fence = blocks
        .iter()
        .rfind(|b| {
            matches!(
                b.lang.as_str(),
                "canvas_actions" | "canvas-actions" | "canvas_action" | "canvas-action" | "canvas"
            )
        })
        .or_else(|| {
            blocks.iter().rfind(|b| {
                let body = &raw[b.body_start..b.body_end];
                (b.lang.is_empty() || b.lang == "json")
                    && (body.contains("\"ops\"")
                        || body.contains("\"actions\"")
                        || body.contains("\"operations\"")
                        || (body.contains("\"op\"") && body.contains('{')))
            })
        });

    if let Some(span) = chosen_fence {
        let body = raw[span.body_start..span.body_end].trim();
        let before = raw[..span.start].trim_end();
        let after = raw[span.end..].trim_start();
        let mut reply = before.to_string();
        if !after.is_empty() {
            if !reply.is_empty() {
                reply.push_str("\n\n");
            }
            reply.push_str(after);
        }
        return match parse_patch_lenient(body) {
            Ok((patch, dropped)) => {
                let warn = if dropped > 0 {
                    Some(format!("{} action(s) in response could not be parsed", dropped))
                } else {
                    None
                };
                ChatActionResponse {
                    reply: if reply.is_empty() && !patch.summary.trim().is_empty() {
                        patch.summary.clone()
                    } else {
                        reply
                    },
                    patch: if patch.ops.is_empty() { None } else { Some(patch) },
                    parse_warning: warn,
                }
            }
            Err(err) => ChatActionResponse {
                reply: if reply.is_empty() {
                    raw.trim().to_string()
                } else {
                    reply
                },
                patch: None,
                parse_warning: Some(err),
            },
        };
    }

    if let Some((start, end)) = find_trailing_json_object(raw) {
        let json_slice = &raw[start..end];
        if let Ok((patch, dropped)) = parse_patch_lenient(json_slice) {
            let reply = raw[..start].trim().to_string();
            let warn = if dropped > 0 {
                Some(format!("{} action(s) in response could not be parsed", dropped))
            } else {
                None
            };
            return ChatActionResponse {
                reply: if reply.is_empty() && !patch.summary.trim().is_empty() {
                    patch.summary.clone()
                } else {
                    reply
                },
                patch: if patch.ops.is_empty() { None } else { Some(patch) },
                parse_warning: warn,
            };
        }
    }

    ChatActionResponse {
        reply: raw.trim().to_string(),
        patch: None,
        parse_warning: None,
    }
}

fn build_chat_prompt(
    context: &LlmContext,
    history: &[(String, String)],
    question: &str,
) -> String {
    let mut prompt = String::new();
    if let Some(date) = context
        .current_date
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        if let Some(rich_line) = studynodes_graph::patch::format_date_context_line(date) {
            prompt.push_str(&format!("Current Date: {}\n", rich_line));
        } else {
            prompt.push_str(&format!("Current Date: {}\n", date));
        }
    }
    if let Some(lang) = context
        .preferred_language
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        prompt.push_str(&format!("Preferred Output Language: {}\n\n", lang));
    }

    prompt.push_str("### Project Knowledge Context\n");
    if context.focus_nodes.is_empty() && context.neighbor_nodes.is_empty() {
        prompt.push_str("(No nodes in project yet.)\n");
    } else {
        for n in &context.focus_nodes {
            prompt.push_str(&format!(
                "- **{}** ({}, id: `{}`)\n",
                n.title.trim(),
                n.kind,
                n.node_id
            ));
            if let Some(sum) = n.summary.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                prompt.push_str(&format!("  Summary: {}\n", sum));
            }
            if !n.content.trim().is_empty() {
                let snippet: String = n.content.trim().chars().take(2_500).collect();
                prompt.push_str(&format!("  Content:\n{}\n", snippet));
            }
        }
        if !context.neighbor_nodes.is_empty() {
            prompt.push_str("\nAdditional Project Nodes:\n");
            for nbr in &context.neighbor_nodes {
                match nbr.summary.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    Some(sum) => prompt.push_str(&format!(
                        "- **{}** ({}, id: `{}`): {}\n",
                        nbr.title, nbr.kind, nbr.node_id, sum
                    )),
                    None => prompt.push_str(&format!(
                        "- **{}** ({}, id: `{}`)\n",
                        nbr.title, nbr.kind, nbr.node_id
                    )),
                }
            }
        }
        if !context.existing_edges.is_empty() {
            prompt.push_str("\nConnections:\n");
            for e in context.existing_edges.iter().take(20) {
                prompt.push_str(&format!(
                    "- `{}` --[{} · {}]--> `{}` ({})\n",
                    e.from, e.relation, e.label, e.to, e.note
                ));
            }
        }
    }

    if !history.is_empty() {
        prompt.push_str("\n### Recent Conversation\n");
        let start = history.len().saturating_sub(8);
        for (role, text) in &history[start..] {
            let speaker = if role == "user" { "User" } else { "Assistant" };
            prompt.push_str(&format!("{}: {}\n", speaker, text.trim()));
        }
    }

    prompt.push_str(&format!("\n### User Question\n{}\n", question.trim()));
    prompt
}

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn generate_patch(&self, context: &LlmContext) -> Result<GraphPatch, LlmError>;
    async fn summarize(&self, context: &LlmContext) -> Result<String, LlmError>;
    async fn ask_project_chat(
        &self,
        context: &LlmContext,
        history: &[(String, String)],
        question: &str,
    ) -> Result<String, LlmError> {
        MockLlmProvider
            .ask_project_chat(context, history, question)
            .await
    }
    async fn ask_project_chat_with_actions(
        &self,
        context: &LlmContext,
        history: &[(String, String)],
        question: &str,
    ) -> Result<ChatActionResponse, LlmError> {
        let reply = self.ask_project_chat(context, history, question).await?;
        Ok(parse_chat_action_response(&reply))
    }
    async fn transcribe_audio(
        &self,
        _req: &AudioTranscriptionRequest,
    ) -> Result<AudioTranscriptionResult, LlmError> {
        Err(LlmError::Api(
            "Audio transcription is not supported by this provider".into(),
        ))
    }
}

/// Deterministic mock provider for offline development, fast testing, and automated agents.
pub struct MockLlmProvider;

#[async_trait::async_trait]
impl LlmProvider for MockLlmProvider {
    async fn generate_patch(&self, context: &LlmContext) -> Result<GraphPatch, LlmError> {
        let mut ops = Vec::new();
        let task_lower = context.task.to_lowercase();

        if task_lower.contains("flashcard") || task_lower.contains("study") {
            if let Some(primary) = context.focus_nodes.first() {
                let answer_text = primary
                    .summary
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| {
                        let first_line = primary
                            .content
                            .lines()
                            .map(str::trim)
                            .find(|l| !l.is_empty() && !l.starts_with('#'))
                            .unwrap_or("Key conceptual mechanism from source material.");
                        let truncated: String = first_line.chars().take(140).collect();
                        truncated
                    });

                ops.push(PatchOp::CreateStudyNode {
                    temp_id: "study_fc_1".into(),
                    kind: studynodes_graph::model::NodeKind::Flashcard,
                    title: format!("Recall: {}", primary.title),
                    content: format!(
                        "What core principle or mechanism does '{}' establish?",
                        primary.title
                    ),
                    answer: Some(answer_text),
                    media_url: None,
                    source_nodes: vec![primary.node_id.clone()],
                });

                ops.push(PatchOp::CreateStudyNode {
                    temp_id: "study_chk_1".into(),
                    kind: studynodes_graph::model::NodeKind::Checklist,
                    title: format!("{} · Study Plan", primary.title),
                    content: format!(
                        "- [ ] Summarize '{}' from memory\n- [ ] Connect empirical evidence\n- [ ] Self-test flashcard recall",
                        primary.title
                    ),
                    answer: None,
                    media_url: None,
                    source_nodes: vec![primary.node_id.clone()],
                });

                ops.push(PatchOp::ProposeConnection {
                    from_node: primary.node_id.clone(),
                    to_node: "study_fc_1".into(),
                    relation: SemanticRelation::DerivedFrom,
                    label: "Tests recall".into(),
                    note: "Auto-generated active recall prompt.".into(),
                    evidence: vec![],
                    confidence: 0.91,
                });

                return Ok(GraphPatch {
                    summary: "Generated active-recall Flashcard & Study Checklist.".into(),
                    ops,
                });
            }
        }

        if context.focus_nodes.len() >= 2 {
            let n1 = &context.focus_nodes[0];
            let n2 = &context.focus_nodes[1];

            // 1. Propose connection between the two focused nodes
            ops.push(PatchOp::ProposeConnection {
                from_node: n1.node_id.clone(),
                to_node: n2.node_id.clone(),
                relation: SemanticRelation::Supports,
                label: "Shared conceptual foundation".into(),
                note: format!(
                    "{} provides theoretical justification for topics explored in {}.",
                    n1.title, n2.title
                ),
                evidence: vec![format!("Overlap identified between {} and {}", n1.title, n2.title)],
                confidence: 0.88,
            });

            // 2. Propose a synthesis node
            ops.push(PatchOp::CreateSynthesisNode {
                temp_id: "tmp_synth_1".into(),
                title: format!("Synthesis: {} & {}", n1.title, n2.title),
                markdown_body: format!(
                    "### Cross-Document Synthesis\n\n- **{}**: {}\n- **{}**: {}\n\n**Key Takeaway**: Both materials intersect on foundational principles.",
                    n1.title,
                    n1.summary.as_deref().unwrap_or("Core document"),
                    n2.title,
                    n2.summary.as_deref().unwrap_or("Related document")
                ),
                source_nodes: vec![n1.node_id.clone(), n2.node_id.clone()],
            });

            ops.push(PatchOp::ProposeConnection {
                from_node: n1.node_id.clone(),
                to_node: "tmp_synth_1".into(),
                relation: SemanticRelation::DerivedFrom,
                label: "Primary source".into(),
                note: "Synthesized source input.".into(),
                evidence: vec![],
                confidence: 0.95,
            });
        } else if let Some(n1) = context.focus_nodes.first() {
            // Single focused node: generate summary if missing
            if n1.summary.is_none() {
                ops.push(PatchOp::UpdateNodeSummary {
                    node_id: n1.node_id.clone(),
                    summary: format!("Key conceptual summary for {}: covers foundational topics.", n1.title),
                });
            }

            // If there are neighbors, propose an elaboration link
            if let Some(nbr) = context.neighbor_nodes.first() {
                ops.push(PatchOp::ProposeConnection {
                    from_node: n1.node_id.clone(),
                    to_node: nbr.node_id.clone(),
                    relation: SemanticRelation::Elaborates,
                    label: "Extends concepts".into(),
                    note: format!("{} deepens the discussion initiated in {}.", nbr.title, n1.title),
                    evidence: vec![],
                    confidence: 0.78,
                });
            }
        }

        Ok(GraphPatch {
            summary: format!("AI analysis for task '{}' complete.", context.task),
            ops,
        })
    }

    async fn summarize(&self, context: &LlmContext) -> Result<String, LlmError> {
        let Some(primary) = context.focus_nodes.first() else {
            return Err(LlmError::Api("No focus node provided to summarize".into()));
        };

        if context.focus_nodes.len() > 1 {
            let children: Vec<String> = context
                .focus_nodes
                .iter()
                .skip(1)
                .take(5)
                .map(|child| {
                    let line = child
                        .content
                        .lines()
                        .map(str::trim)
                        .find(|l| !l.is_empty() && !l.starts_with('#'))
                        .unwrap_or("");
                    if line.is_empty() {
                        child.title.clone()
                    } else {
                        let short: String = line.chars().take(48).collect();
                        format!("{} ({})", child.title, short)
                    }
                })
                .collect();
            let raw = format!(
                "{} — Section review covering {} cards: {}",
                primary.title,
                context.focus_nodes.len() - 1,
                children.join("; ")
            );
            let mut out: String = raw.chars().take(260).collect();
            if raw.chars().count() > 260 {
                out = out.trim_end().to_string();
                out.push('…');
            }
            return Ok(out);
        }

        let body_lines: Vec<&str> = primary
            .content
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .take(4)
            .collect();

        if body_lines.is_empty() {
            return Ok(format!(
                "Key conceptual summary for {}: covers foundational topics.",
                primary.title
            ));
        }

        let joined = body_lines.join(" ");
        let raw = format!("{}: {}", primary.title, joined);
        let mut out: String = raw.chars().take(220).collect();
        if raw.chars().count() > 220 {
            out = out.trim_end().to_string();
            out.push('…');
        }
        Ok(out)
    }

    async fn ask_project_chat(
        &self,
        context: &LlmContext,
        _history: &[(String, String)],
        question: &str,
    ) -> Result<String, LlmError> {
        let q = question.trim();
        if context.focus_nodes.is_empty() {
            return Ok(format!(
                "Based on this workspace (currently empty), you asked: \"{}\". Add documents or notes to the canvas, or configure a live API key in ⚙ Settings.",
                q
            ));
        }

        let titles: Vec<String> = context
            .focus_nodes
            .iter()
            .take(4)
            .map(|n| format!("**{}**", n.title))
            .collect();
        let primary = &context.focus_nodes[0];
        let excerpt: String = primary
            .content
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        let snippet: String = if excerpt.is_empty() {
            primary
                .summary
                .clone()
                .unwrap_or_else(|| "Core study material in the active workspace.".into())
        } else {
            excerpt.chars().take(180).collect()
        };

        Ok(format!(
            "Drawing from {} ({} total focus node(s) in context):\n- **{}**: {}\n- Regarding *\"{}\"*: configure a Gemini or OpenAI-compatible API key in **⚙ Settings** for full live conversational reasoning.",
            titles.join(", "),
            context.focus_nodes.len(),
            primary.title,
            snippet,
            q
        ))
    }

    async fn ask_project_chat_with_actions(
        &self,
        context: &LlmContext,
        history: &[(String, String)],
        question: &str,
    ) -> Result<ChatActionResponse, LlmError> {
        let q = question.trim();
        let ql = q.to_ascii_lowercase();
        let date = context
            .current_date
            .clone()
            .unwrap_or_else(|| "2026-11-21".to_string());

        if ql.contains("week")
            && (ql.contains("note") || ql.contains("calendar") || ql.contains("planner"))
        {
            let cal_id = context
                .focus_nodes
                .iter()
                .find(|n| n.kind == "Calendar")
                .map(|n| n.node_id.clone());
            let patch = GraphPatch {
                summary: "Updated calendar Week view with daily note".into(),
                ops: vec![
                    PatchOp::SetCalendarViewMode {
                        node_id: cal_id.clone(),
                        mode: "week".into(),
                    },
                    PatchOp::SetCalendarDayNote {
                        node_id: cal_id,
                        date: date.clone(),
                        note: q.to_string(),
                        append: false,
                    },
                ],
            };
            return Ok(ChatActionResponse {
                reply: format!(
                    "Switched calendar to **Week mode** and added a daily note for **{}**.",
                    date
                ),
                patch: Some(patch),
                parse_warning: None,
            });
        }

        if ql.contains("calendar") || ql.contains("schedule") || ql.contains("agenda") {
            let cal_id = context
                .focus_nodes
                .iter()
                .find(|n| n.kind == "Calendar")
                .map(|n| n.node_id.clone());
            let linked = context
                .focus_nodes
                .iter()
                .find(|n| n.kind != "Calendar")
                .map(|n| n.node_id.clone());
            let patch = GraphPatch {
                summary: "Scheduled study session on Calendar".into(),
                ops: vec![PatchOp::AddCalendarEvent {
                    node_id: cal_id,
                    date: date.clone(),
                    time: Some("18:00".into()),
                    title: format!("Study Session: {}", q.chars().take(48).collect::<String>()),
                    notes: "Added directly by Project AI".into(),
                    linked_node: linked,
                }],
            };
            return Ok(ChatActionResponse {
                reply: format!(
                    "Scheduled a study session on **{} at 18:00** directly on your project calendar.",
                    date
                ),
                patch: Some(patch),
                parse_warning: None,
            });
        }

        if ql.contains("timestamp") {
            if let Some(media) = context
                .focus_nodes
                .iter()
                .find(|n| n.kind == "Video" || n.kind == "Audio")
            {
                let patch = GraphPatch {
                    summary: format!("Added timestamp on {}", media.title),
                    ops: vec![PatchOp::AddMediaTimestamp {
                        node_id: media.node_id.clone(),
                        seconds: 90.0,
                        label: "Key concept".into(),
                    }],
                };
                return Ok(ChatActionResponse {
                    reply: format!(
                        "Bookmarked **01:30 — Key concept** directly on **{}**.",
                        media.title
                    ),
                    patch: Some(patch),
                    parse_warning: None,
                });
            }
        }

        let wants_image_search = (ql.contains("image")
            || ql.contains("imagen")
            || ql.contains("picture")
            || ql.contains("photo")
            || ql.contains("foto")
            || ql.contains("diagram"))
            && (ql.contains("search")
                || ql.contains("find")
                || ql.contains("look up")
                || ql.contains("fetch")
                || ql.contains("busca")
                || ql.contains("show")
                || ql.contains("add"));
        if wants_image_search {
            let source_ids: Vec<String> = context
                .focus_nodes
                .first()
                .map(|n| vec![n.node_id.clone()])
                .unwrap_or_default();
            let patch = GraphPatch {
                summary: format!("Search image for '{}'", q),
                ops: vec![PatchOp::SearchImages {
                    query: q.to_string(),
                    title: Some(format!("Image: {}", q.chars().take(36).collect::<String>())),
                    count: 1,
                    source_nodes: source_ids,
                }],
            };
            return Ok(ChatActionResponse {
                reply: format!("Searching images for **{}** and adding a visual node to the canvas.", q),
                patch: Some(patch),
                parse_warning: None,
            });
        }

        if ql.contains("create") || ql.contains("add") || ql.contains("flashcard") || ql.contains("note") {
            let is_fc = ql.contains("flashcard") || ql.contains("card");
            let kind = if is_fc {
                studynodes_graph::model::NodeKind::Flashcard
            } else {
                studynodes_graph::model::NodeKind::Note
            };
            let source_ids: Vec<String> = context
                .focus_nodes
                .first()
                .map(|n| vec![n.node_id.clone()])
                .unwrap_or_default();
            let title = if is_fc {
                "AI Study Flashcard".to_string()
            } else {
                "AI Study Note".to_string()
            };
            let patch = GraphPatch {
                summary: format!("Created {}", title),
                ops: vec![PatchOp::CreateStudyNode {
                    temp_id: "tmp_chat_node".into(),
                    kind,
                    title: title.clone(),
                    content: format!("Generated from request: {}", q),
                    answer: is_fc.then(|| "Review key concepts from the selected study nodes.".into()),
                    media_url: None,
                    source_nodes: source_ids,
                }],
            };
            return Ok(ChatActionResponse {
                reply: format!("Created **{}** directly on your canvas.", title),
                patch: Some(patch),
                parse_warning: None,
            });
        }

        let reply = self.ask_project_chat(context, history, question).await?;
        Ok(ChatActionResponse {
            reply,
            patch: None,
            parse_warning: None,
        })
    }

    async fn transcribe_audio(
        &self,
        req: &AudioTranscriptionRequest,
    ) -> Result<AudioTranscriptionResult, LlmError> {
        let label = if req.filename.trim().is_empty() {
            "audio clip"
        } else {
            req.filename.trim()
        };
        let segments = vec![
            AudioTranscriptSegment {
                start: 0.0,
                end: 4.5,
                text: format!("Automatic transcript ready for {}.", label),
            },
            AudioTranscriptSegment {
                start: 4.5,
                end: 9.0,
                text: "Key spoken notes captured as timed subtitles.".into(),
            },
        ];
        let full_text = segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        Ok(AudioTranscriptionResult {
            text: full_text,
            segments,
        })
    }
}

pub fn extract_gemini_response_text(response_json: &serde_json::Value) -> Option<String> {
    let parts = response_json["candidates"][0]["content"]["parts"].as_array()?;
    let non_thought: Vec<&str> = parts
        .iter()
        .filter(|p| p["thought"].as_bool() != Some(true))
        .filter_map(|p| p["text"].as_str())
        .collect();
    if !non_thought.is_empty() {
        return Some(non_thought.join("\n"));
    }
    let any_text: Vec<&str> = parts.iter().filter_map(|p| p["text"].as_str()).collect();
    if !any_text.is_empty() {
        Some(any_text.join("\n"))
    } else {
        None
    }
}

fn parse_transcription_json_or_text(raw: &str) -> AudioTranscriptionResult {
    let clean = strip_json_fences(raw);
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(clean) {
        let mut segments = Vec::new();
        if let Some(arr) = v.get("segments").and_then(|s| s.as_array()) {
            for item in arr {
                let txt = item
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .trim();
                if txt.is_empty() {
                    continue;
                }
                let start = item
                    .get("start")
                    .and_then(|n| n.as_f64())
                    .unwrap_or(0.0)
                    .max(0.0);
                let end = item
                    .get("end")
                    .and_then(|n| n.as_f64())
                    .unwrap_or(start + 4.0)
                    .max(start);
                segments.push(AudioTranscriptSegment {
                    start,
                    end,
                    text: txt.to_string(),
                });
            }
        }
        let text = v
            .get("text")
            .and_then(|t| t.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                segments
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            });
        if !text.is_empty() || !segments.is_empty() {
            return AudioTranscriptionResult { text, segments };
        }
    }
    AudioTranscriptionResult {
        text: raw.trim().to_string(),
        segments: Vec::new(),
    }
}

/// Google Gemini API Provider calling the official Gemini REST endpoint.
pub struct GeminiLlmProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl GeminiLlmProvider {
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let resolved_model = model
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "gemini-2.5-flash".to_string());
        Self {
            client: build_http_client(),
            api_key: api_key.into(),
            model: resolved_model,
        }
    }

    async fn post_generate_content(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        json_mode: bool,
    ) -> Result<String, LlmError> {
        if self.api_key.trim().is_empty() {
            return Err(LlmError::MissingApiKey);
        }

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            self.model
        );

        let mut request_body = json!({
            "system_instruction": {
                "parts": [{ "text": system_prompt }]
            },
            "contents": [
                {
                    "parts": [{ "text": user_prompt }]
                }
            ]
        });
        if json_mode {
            request_body["generationConfig"] = json!({
                "response_mime_type": "application/json"
            });
        }

        let res = self
            .client
            .post(&url)
            .header("x-goog-api-key", self.api_key.trim())
            .json(&request_body)
            .send()
            .await?;
        if !res.status().is_success() {
            let status = res.status();
            let error_text = res.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("Gemini HTTP {}: {}", status, error_text)));
        }

        let response_json: serde_json::Value = res.json().await?;
        let text_content = extract_gemini_response_text(&response_json)
            .ok_or_else(|| LlmError::Api("Invalid response structure from Gemini API".into()))?;
        Ok(text_content)
    }

    async fn upload_gemini_file(
        &self,
        bytes: &[u8],
        mime_type: &str,
        display_name: &str,
    ) -> Result<String, LlmError> {
        let start_url = "https://generativelanguage.googleapis.com/upload/v1beta/files";
        let start_res = self
            .client
            .post(start_url)
            .header("x-goog-api-key", self.api_key.trim())
            .header("X-Goog-Upload-Protocol", "resumable")
            .header("X-Goog-Upload-Command", "start")
            .header("X-Goog-Upload-Header-Content-Length", bytes.len().to_string())
            .header("X-Goog-Upload-Header-Content-Type", mime_type)
            .json(&json!({ "file": { "display_name": display_name } }))
            .send()
            .await?;
        if !start_res.status().is_success() {
            let st = start_res.status();
            let err = start_res.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("Gemini upload start HTTP {}: {}", st, err)));
        }
        let upload_url = start_res
            .headers()
            .get("x-goog-upload-url")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| LlmError::Api("Missing Gemini resumable upload URL".into()))?
            .to_string();

        let fin_res = self
            .client
            .post(&upload_url)
            .header("Content-Length", bytes.len().to_string())
            .header("X-Goog-Upload-Offset", "0")
            .header("X-Goog-Upload-Command", "upload, finalize")
            .body(bytes.to_vec())
            .send()
            .await?;
        if !fin_res.status().is_success() {
            let st = fin_res.status();
            let err = fin_res.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("Gemini file upload HTTP {}: {}", st, err)));
        }
        let fin_json: serde_json::Value = fin_res.json().await?;
        fin_json["file"]["uri"]
            .as_str()
            .map(ToString::to_string)
            .ok_or_else(|| LlmError::Api("Missing file URI from Gemini upload".into()))
    }
}

#[async_trait::async_trait]
impl LlmProvider for GeminiLlmProvider {
    async fn generate_patch(&self, context: &LlmContext) -> Result<GraphPatch, LlmError> {
        let context_json = serde_json::to_string_pretty(context)?;
        let raw = self
            .post_generate_content(SYSTEM_PROMPT, &context_json, true)
            .await?;
        let clean = strip_json_fences(&raw);
        let patch: GraphPatch = serde_json::from_str(clean)?;
        Ok(patch)
    }

    async fn summarize(&self, context: &LlmContext) -> Result<String, LlmError> {
        let user_prompt = build_summarize_prompt(context);
        let raw = self
            .post_generate_content(SUMMARIZE_SYSTEM_PROMPT, &user_prompt, false)
            .await?;
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() {
            return Err(LlmError::Api("Gemini returned an empty summary".into()));
        }
        Ok(trimmed)
    }

    async fn ask_project_chat(
        &self,
        context: &LlmContext,
        history: &[(String, String)],
        question: &str,
    ) -> Result<String, LlmError> {
        let user_prompt = build_chat_prompt(context, history, question);
        let raw = self
            .post_generate_content(CHAT_SYSTEM_PROMPT, &user_prompt, false)
            .await?;
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() {
            return Err(LlmError::Api("Gemini returned an empty response".into()));
        }
        Ok(trimmed)
    }

    async fn ask_project_chat_with_actions(
        &self,
        context: &LlmContext,
        history: &[(String, String)],
        question: &str,
    ) -> Result<ChatActionResponse, LlmError> {
        let user_prompt = build_chat_prompt(context, history, question);
        let raw = self
            .post_generate_content(CANVAS_AGENT_CHAT_SYSTEM_PROMPT, &user_prompt, false)
            .await?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(LlmError::Api("Gemini returned an empty response".into()));
        }
        Ok(parse_chat_action_response(trimmed))
    }

    async fn transcribe_audio(
        &self,
        req: &AudioTranscriptionRequest,
    ) -> Result<AudioTranscriptionResult, LlmError> {
        use base64::Engine;
        if self.api_key.trim().is_empty() {
            return Err(LlmError::MissingApiKey);
        }
        let lang_note = req
            .language_hint
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|l| format!(" Preferred UI language hint (keep original spoken language): {}.", l))
            .unwrap_or_default();
        let instruction = format!(
            "Transcribe the spoken audio accurately into timestamped subtitle segments in JSON: {{\"text\": \"full transcript\", \"segments\": [{{\"start\": 0.0, \"end\": 4.5, \"text\": \"segment text\"}}]}}. Keep segments concise (1-2 sentences each) with numeric seconds.{lang_note}"
        );

        let audio_part = if req.bytes.len() <= 18_000_000 {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&req.bytes);
            json!({
                "inline_data": {
                    "mime_type": req.mime_type,
                    "data": b64
                }
            })
        } else {
            let file_uri = self
                .upload_gemini_file(&req.bytes, &req.mime_type, &req.filename)
                .await?;
            json!({
                "file_data": {
                    "mime_type": req.mime_type,
                    "file_uri": file_uri
                }
            })
        };

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            self.model
        );
        let body = json!({
            "contents": [{
                "parts": [
                    audio_part,
                    { "text": instruction }
                ]
            }],
            "generationConfig": {
                "response_mime_type": "application/json"
            }
        });

        let res = self
            .client
            .post(&url)
            .header("x-goog-api-key", self.api_key.trim())
            .json(&body)
            .send()
            .await?;
        if !res.status().is_success() {
            let status = res.status();
            let err_txt = res.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("Gemini HTTP {}: {}", status, err_txt)));
        }
        let response_json: serde_json::Value = res.json().await?;
        let text_content = extract_gemini_response_text(&response_json)
            .ok_or_else(|| LlmError::Api("Invalid transcription response from Gemini API".into()))?;
        Ok(parse_transcription_json_or_text(&text_content))
    }
}

/// OpenAI-compatible `/chat/completions` provider (OpenAI, DeepSeek, OpenRouter, Groq, local servers).
pub struct OpenAiCompatibleLlmProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl OpenAiCompatibleLlmProvider {
    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: Option<String>,
    ) -> Self {
        let resolved_model = model
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "gpt-4o-mini".to_string());
        Self {
            client: build_http_client(),
            api_key: api_key.into(),
            base_url: base_url.into(),
            model: resolved_model,
        }
    }

    pub fn chat_completions_url(&self) -> String {
        let trimmed = self.base_url.trim().trim_end_matches('/');
        if trimmed.is_empty() {
            "https://api.openai.com/v1/chat/completions".to_string()
        } else if trimmed.ends_with("/chat/completions") {
            trimmed.to_string()
        } else {
            format!("{}/chat/completions", trimmed)
        }
    }

    async fn post_chat_completion(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        json_mode: bool,
    ) -> Result<String, LlmError> {
        if self.api_key.trim().is_empty() {
            return Err(LlmError::MissingApiKey);
        }

        let url = self.chat_completions_url();
        let mut request_body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ]
        });
        if json_mode {
            request_body["response_format"] = json!({ "type": "json_object" });
        }

        let res = self
            .client
            .post(&url)
            .bearer_auth(self.api_key.trim())
            .json(&request_body)
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let error_text = res.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("API HTTP {}: {}", status, error_text)));
        }

        let response_json: serde_json::Value = res.json().await?;
        let msg_content = &response_json["choices"][0]["message"]["content"];
        let text_content = if let Some(s) = msg_content.as_str() {
            s.to_string()
        } else if let Some(arr) = msg_content.as_array() {
            arr.iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            return Err(LlmError::Api(
                "Invalid response structure from OpenAI-compatible API".into(),
            ));
        };
        Ok(text_content)
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiCompatibleLlmProvider {
    async fn generate_patch(&self, context: &LlmContext) -> Result<GraphPatch, LlmError> {
        let context_json = serde_json::to_string_pretty(context)?;
        let raw = self
            .post_chat_completion(SYSTEM_PROMPT, &context_json, true)
            .await?;
        let clean = strip_json_fences(&raw);
        let patch: GraphPatch = serde_json::from_str(clean)?;
        Ok(patch)
    }

    async fn summarize(&self, context: &LlmContext) -> Result<String, LlmError> {
        let user_prompt = build_summarize_prompt(context);
        let raw = self
            .post_chat_completion(SUMMARIZE_SYSTEM_PROMPT, &user_prompt, false)
            .await?;
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() {
            return Err(LlmError::Api("Provider returned an empty summary".into()));
        }
        Ok(trimmed)
    }

    async fn ask_project_chat(
        &self,
        context: &LlmContext,
        history: &[(String, String)],
        question: &str,
    ) -> Result<String, LlmError> {
        let user_prompt = build_chat_prompt(context, history, question);
        let raw = self
            .post_chat_completion(CHAT_SYSTEM_PROMPT, &user_prompt, false)
            .await?;
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() {
            return Err(LlmError::Api("Provider returned an empty response".into()));
        }
        Ok(trimmed)
    }

    async fn ask_project_chat_with_actions(
        &self,
        context: &LlmContext,
        history: &[(String, String)],
        question: &str,
    ) -> Result<ChatActionResponse, LlmError> {
        let user_prompt = build_chat_prompt(context, history, question);
        let raw = self
            .post_chat_completion(CANVAS_AGENT_CHAT_SYSTEM_PROMPT, &user_prompt, false)
            .await?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(LlmError::Api("Provider returned an empty response".into()));
        }
        Ok(parse_chat_action_response(trimmed))
    }

    async fn transcribe_audio(
        &self,
        req: &AudioTranscriptionRequest,
    ) -> Result<AudioTranscriptionResult, LlmError> {
        if self.api_key.trim().is_empty() {
            return Err(LlmError::MissingApiKey);
        }
        let base = self.base_url.trim().trim_end_matches('/');
        let root = if base.is_empty() {
            "https://api.openai.com/v1"
        } else {
            base.strip_suffix("/chat/completions").unwrap_or(base)
        };
        let url = if root.ends_with("/audio/transcriptions") {
            root.to_string()
        } else {
            format!("{}/audio/transcriptions", root)
        };
        let model_name = if root.to_ascii_lowercase().contains("groq.com") {
            "whisper-large-v3-turbo"
        } else {
            "whisper-1"
        };
        let fname = if req.filename.trim().is_empty() {
            "audio.mp3".to_string()
        } else {
            req.filename.clone()
        };
        let file_part = reqwest::multipart::Part::bytes(req.bytes.clone())
            .file_name(fname)
            .mime_str(&req.mime_type)
            .map_err(|e| LlmError::Api(format!("Invalid audio mime type: {}", e)))?;
        let form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", model_name)
            .text("response_format", "verbose_json");

        let res = self
            .client
            .post(&url)
            .bearer_auth(self.api_key.trim())
            .multipart(form)
            .send()
            .await?;
        if !res.status().is_success() {
            let status = res.status();
            let err_txt = res.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("Transcription HTTP {}: {}", status, err_txt)));
        }
        let raw_txt = res.text().await?;
        Ok(parse_transcription_json_or_text(&raw_txt))
    }
}
