use serde_json::json;
use thiserror::Error;
use studynodes_graph::patch::{GraphPatch, PatchOp, SemanticRelation};

use crate::context::LlmContext;
use crate::prompt::SYSTEM_PROMPT;

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

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn generate_patch(&self, context: &LlmContext) -> Result<GraphPatch, LlmError>;
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
}

/// Google Gemini API Provider calling the official Gemini REST endpoint with structured output.
pub struct GeminiLlmProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl GeminiLlmProvider {
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: model.unwrap_or_else(|| "gemini-2.5-flash".to_string()),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for GeminiLlmProvider {
    async fn generate_patch(&self, context: &LlmContext) -> Result<GraphPatch, LlmError> {
        if self.api_key.trim().is_empty() {
            return Err(LlmError::MissingApiKey);
        }

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let context_json = serde_json::to_string_pretty(context)?;
        let request_body = json!({
            "system_instruction": {
                "parts": [{ "text": SYSTEM_PROMPT }]
            },
            "contents": [
                {
                    "parts": [{ "text": context_json }]
                }
            ],
            "generationConfig": {
                "response_mime_type": "application/json"
            }
        });

        let res = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await?;

        if !res.status().is_success() {
            let error_text = res.text().await.unwrap_or_default();
            return Err(LlmError::Api(error_text));
        }

        let response_json: serde_json::Value = res.json().await?;
        let text_content = response_json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| LlmError::Api("Invalid response structure from Gemini API".into()))?;

        let patch: GraphPatch = serde_json::from_str(text_content)?;
        Ok(patch)
    }
}
