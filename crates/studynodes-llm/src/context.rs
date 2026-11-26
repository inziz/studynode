use serde::{Deserialize, Serialize};
use studynodes_graph::model::NodeId;
use studynodes_graph::StudyGraph;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusedNodeContext {
    pub node_id: String,
    pub title: String,
    pub kind: String,
    pub content: String,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub pos: [f32; 2],
    #[serde(default)]
    pub size: [f32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborNodeContext {
    pub node_id: String,
    pub title: String,
    pub kind: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub pos: [f32; 2],
    #[serde(default)]
    pub size: [f32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingEdgeContext {
    pub edge_id: String,
    pub from: String,
    pub to: String,
    pub relation: String,
    pub label: String,
    pub note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmContext {
    pub task: String,
    pub focus_nodes: Vec<FocusedNodeContext>,
    pub neighbor_nodes: Vec<NeighborNodeContext>,
    pub existing_edges: Vec<ExistingEdgeContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_date: Option<String>,
}

impl LlmContext {
    /// Attach an optional output language preference (e.g. `"Spanish"`).
    pub fn with_preferred_language(mut self, lang: Option<String>) -> Self {
        self.preferred_language = lang
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        self
    }

    /// Attach the current local calendar date (`YYYY-MM-DD`) so AI scheduling resolves relative dates.
    pub fn with_current_date(mut self, date: Option<String>) -> Self {
        self.current_date = date
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        self
    }

    fn format_node_metadata_suffix(n: &studynodes_graph::model::Node) -> String {
        let mut out = String::new();
        if let Some(url) = n
            .meta
            .media_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            out.push_str(&format!("\n[Media URL]: {}", url));
        }
        if !n.meta.timestamps.is_empty() {
            out.push_str("\n[Timestamps]:");
            for ts in &n.meta.timestamps {
                let total = ts.seconds.round().max(0.0) as u64;
                let h = total / 3600;
                let m = (total % 3600) / 60;
                let s = total % 60;
                let stamp = if h > 0 {
                    format!("{}:{:02}:{:02}", h, m, s)
                } else {
                    format!("{:02}:{:02}", m, s)
                };
                out.push_str(&format!("\n- {} ({}s): {}", stamp, total, ts.label));
            }
        }
        if n.kind == studynodes_graph::model::NodeKind::Calendar {
            let mode_str = match n.meta.calendar_view_mode {
                studynodes_graph::model::CalendarViewMode::Month => "Month",
                studynodes_graph::model::CalendarViewMode::Week => "Week",
            };
            out.push_str(&format!("\n[Calendar View]: {}", mode_str));
            if let (Some(cy), Some(cm)) = (n.meta.calendar_year, n.meta.calendar_month) {
                out.push_str(&format!("\n[Calendar Cursor]: {:04}-{:02}", cy, cm));
            }
            if let Some(ws) = n.meta.calendar_week_start.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                out.push_str(&format!(" (week starting {})", ws));
            }
            if !n.meta.calendar_day_notes.is_empty() {
                out.push_str("\n[Calendar Daily Notes]:");
                for (date, note) in &n.meta.calendar_day_notes {
                    let clean = note.trim().replace('\n', " / ");
                    if !clean.is_empty() {
                        out.push_str(&format!("\n- {}: {}", date, clean));
                    }
                }
            }
        }
        if let Some(tr) = n
            .meta
            .transcript_text
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let clipped: String = tr.chars().take(1_200).collect();
            out.push_str(&format!("\n[Transcript]: {}", clipped));
        }
        if !n.meta.calendar_events.is_empty() {
            out.push_str("\n[Calendar Events]:");
            for ev in &n.meta.calendar_events {
                let mark = if ev.completed { "x" } else { " " };
                let time_str = ev.time.as_deref().unwrap_or("");
                let note_str = if ev.notes.trim().is_empty() {
                    String::new()
                } else {
                    format!(" ({})", ev.notes.trim())
                };
                if time_str.is_empty() {
                    out.push_str(&format!(
                        "\n- [{}] {} — {}{}",
                        mark, ev.date, ev.title, note_str
                    ));
                } else {
                    out.push_str(&format!(
                        "\n- [{}] {} {} — {}{}",
                        mark, ev.date, time_str, ev.title, note_str
                    ));
                }
            }
        }
        if n.kind == studynodes_graph::model::NodeKind::Quiz && !n.meta.quiz_questions.is_empty() {
            out.push_str("\n[Quiz Questions]:");
            for (qi, q) in n.meta.quiz_questions.iter().enumerate() {
                out.push_str(&format!("\n- Q{} (index {}): {}", qi + 1, qi, q.question));
                if !q.options.is_empty() {
                    let opts: Vec<String> = q
                        .options
                        .iter()
                        .enumerate()
                        .map(|(oi, o)| format!("[{}] {}", oi, o))
                        .collect();
                    out.push_str(&format!(" | Options: {}", opts.join(", ")));
                }
                if let Some(ci) = q.correct_index {
                    out.push_str(&format!(" | correct_index: {}", ci));
                }
                if !q.answer.trim().is_empty() {
                    out.push_str(&format!(" | Answer: {}", q.answer.trim()));
                }
                if !q.hints.is_empty() {
                    out.push_str(&format!(" | Hints: {}", q.hints.join(" ; ")));
                }
            }
        }
        out
    }

    /// Build bounded neighborhood context around a set of selected focus nodes.
    pub fn build(graph: &StudyGraph, focus_ids: &[NodeId], task: impl Into<String>) -> Self {
        let mut focus_nodes = Vec::new();
        let mut neighbor_nodes = Vec::new();
        let mut existing_edges = Vec::new();
        let mut neighbor_id_set = std::collections::HashSet::new();
        let mut edge_id_set = std::collections::HashSet::new();

        // 1. Pack focus nodes
        for id in focus_ids {
            if let Some(n) = graph.get_node(id) {
                let extracted = n
                    .meta
                    .extracted_text
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let mut packed_content = match (n.content.trim().is_empty(), extracted) {
                    (true, Some(ocr)) => ocr.to_string(),
                    (false, Some(ocr)) => format!("{}\n\n[Extracted OCR Text]\n{}", n.content, ocr),
                    (_, None) => n.content.clone(),
                };
                packed_content.push_str(&Self::format_node_metadata_suffix(n));

                let (eff_w, eff_h) = n.effective_size();
                focus_nodes.push(FocusedNodeContext {
                    node_id: n.id.as_str().to_string(),
                    title: n.title.clone(),
                    kind: n.kind.display_name().to_string(),
                    content: packed_content,
                    summary: n.summary.clone(),
                    tags: n.tags.clone(),
                    pos: [n.pos.x.round(), n.pos.y.round()],
                    size: [eff_w.round(), eff_h.round()],
                });

                // 2. Discover 1-hop neighbors
                for neighbor_id in graph.neighbors(id) {
                    if !focus_ids.contains(&neighbor_id) && neighbor_id_set.insert(neighbor_id.clone()) {
                        if let Some(nbr) = graph.get_node(&neighbor_id) {
                            let (nw, nh) = nbr.effective_size();
                            neighbor_nodes.push(NeighborNodeContext {
                                node_id: nbr.id.as_str().to_string(),
                                title: nbr.title.clone(),
                                kind: nbr.kind.display_name().to_string(),
                                summary: nbr.summary.clone(),
                                pos: [nbr.pos.x.round(), nbr.pos.y.round()],
                                size: [nw.round(), nh.round()],
                            });
                        }
                    }
                }

                // 3. Collect incident edges
                for edge in graph.incident_edges(id) {
                    if edge_id_set.insert(edge.id.clone()) {
                        existing_edges.push(ExistingEdgeContext {
                            edge_id: edge.id.as_str().to_string(),
                            from: edge.from.node.as_str().to_string(),
                            to: edge.to.node.as_str().to_string(),
                            relation: edge.relation.display_label().to_string(),
                            label: edge.label.clone(),
                            note: edge.note.clone(),
                            color: edge.color_hex.clone(),
                            color_category: edge.color_category.clone(),
                        });
                    }
                }
            }
        }

        Self {
            task: task.into(),
            focus_nodes,
            neighbor_nodes,
            existing_edges,
            preferred_language: None,
            current_date: None,
        }
    }

    /// Build a project-wide context for AI Q&A / chat, prioritizing `selected_ids` while
    /// including the broader project nodes and edges so the AI can answer questions across the canvas.
    pub fn build_project_context(
        graph: &StudyGraph,
        selected_ids: &[NodeId],
        task: impl Into<String>,
    ) -> Self {
        let mut focus_nodes = Vec::new();
        let mut neighbor_nodes = Vec::new();
        let mut packed_ids = std::collections::HashSet::new();

        let pack_node = |n: &studynodes_graph::model::Node, max_chars: usize| -> FocusedNodeContext {
            let extracted = n
                .meta
                .extracted_text
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let answer = n
                .meta
                .answer
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let mut body = n.content.clone();
            if let Some(ans) = answer {
                if !body.is_empty() {
                    body.push_str("\n");
                }
                body.push_str(&format!("[Answer]: {}", ans));
            }
            if let Some(ocr) = extracted {
                if !body.is_empty() {
                    body.push_str("\n");
                }
                body.push_str(&format!("[Extracted OCR]: {}", ocr));
            }
            let extra = Self::format_node_metadata_suffix(n);
            if !extra.is_empty() {
                body.push_str(&extra);
            }
            let truncated: String = body.chars().take(max_chars).collect();
            let (eff_w, eff_h) = n.effective_size();
            FocusedNodeContext {
                node_id: n.id.as_str().to_string(),
                title: n.title.clone(),
                kind: n.kind.display_name().to_string(),
                content: truncated,
                summary: n.summary.clone(),
                tags: n.tags.clone(),
                pos: [n.pos.x.round(), n.pos.y.round()],
                size: [eff_w.round(), eff_h.round()],
            }
        };

        for id in selected_ids {
            if let Some(n) = graph.get_node(id) {
                packed_ids.insert(n.id.clone());
                focus_nodes.push(pack_node(n, 6_000));
            }
        }

        // Always prioritize Calendar nodes in focus_nodes so AI sees schedules & daily notes
        let mut calendars: Vec<_> = graph
            .nodes
            .values()
            .filter(|n| {
                n.kind == studynodes_graph::model::NodeKind::Calendar
                    && !packed_ids.contains(&n.id)
            })
            .collect();
        calendars.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        for cal in calendars {
            packed_ids.insert(cal.id.clone());
            focus_nodes.push(pack_node(cal, 4_000));
        }

        let mut remaining: Vec<_> = graph
            .nodes
            .values()
            .filter(|n| !packed_ids.contains(&n.id))
            .collect();
        remaining.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.id.as_str().cmp(b.id.as_str())));

        for n in remaining {
            if focus_nodes.len() < 12 {
                focus_nodes.push(pack_node(n, 2_400));
            } else {
                let (nw, nh) = n.effective_size();
                neighbor_nodes.push(NeighborNodeContext {
                    node_id: n.id.as_str().to_string(),
                    title: n.title.clone(),
                    kind: n.kind.display_name().to_string(),
                    summary: n.summary.clone(),
                    pos: [n.pos.x.round(), n.pos.y.round()],
                    size: [nw.round(), nh.round()],
                });
            }
        }

        let mut existing_edges: Vec<ExistingEdgeContext> = graph
            .edges
            .values()
            .take(30)
            .map(|edge| ExistingEdgeContext {
                edge_id: edge.id.as_str().to_string(),
                from: edge.from.node.as_str().to_string(),
                to: edge.to.node.as_str().to_string(),
                relation: edge.relation.display_label().to_string(),
                label: edge.label.clone(),
                note: edge.note.clone(),
                color: edge.color_hex.clone(),
                color_category: edge.color_category.clone(),
            })
            .collect();
        existing_edges.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));

        Self {
            task: task.into(),
            focus_nodes,
            neighbor_nodes,
            existing_edges,
            preferred_language: None,
            current_date: None,
        }
    }
}
