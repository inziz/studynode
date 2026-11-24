pub mod client;
pub mod context;
pub mod prompt;

pub use client::{
    API_PROVIDER_PRESETS, AnthropicLlmProvider, ApiProviderPreset, AudioTranscriptSegment,
    AudioTranscriptionRequest, AudioTranscriptionResult, ChatActionResponse, GeminiLlmProvider,
    LlmError, LlmProvider, MockLlmProvider, OpenAiCompatibleLlmProvider,
    extract_gemini_response_text, find_api_provider_preset, is_local_unauthenticated_url,
    normalize_openai_chat_completions_url, parse_chat_action_response,
    resolve_universal_llm_provider, strip_json_fences,
};
pub use context::{ExistingEdgeContext, FocusedNodeContext, LlmContext, NeighborNodeContext};
pub use prompt::{
    CANVAS_AGENT_CHAT_SYSTEM_PROMPT, CHAT_SYSTEM_PROMPT, SUMMARIZE_SYSTEM_PROMPT, SYSTEM_PROMPT,
};

#[cfg(test)]
mod tests {
    use super::*;
    use studynodes_graph::coords::WorldPos;
    use studynodes_graph::model::{Node, NodeKind};
    use studynodes_graph::StudyGraph;

    #[tokio::test]
    async fn test_mock_llm_generates_valid_patch() {
        let mut graph = StudyGraph::new("Test");
        let n1 = Node::new(NodeKind::Document, "Dopamine & Learning", WorldPos::new(0.0, 0.0));
        let n2 = Node::new(NodeKind::Document, "Reward Prediction Error", WorldPos::new(300.0, 0.0));
        let id1 = n1.id.clone();
        let id2 = n2.id.clone();

        graph.add_node(n1);
        graph.add_node(n2);

        let context = LlmContext::build(&graph, &[id1.clone(), id2.clone()], "Find Connections");
        let provider = MockLlmProvider;
        let patch = provider.generate_patch(&context).await.unwrap();

        assert!(!patch.ops.is_empty());
        let result = graph.validate_and_stage_patch(&patch);
        assert!(!result.proposed_edges.is_empty());
        assert!(!result.new_nodes.is_empty());
    }

    #[tokio::test]
    async fn test_mock_llm_generates_study_cards() {
        let mut graph = StudyGraph::new("Study Cards Test");
        let mut doc = Node::new(NodeKind::Document, "Action Potential", WorldPos::new(0.0, 0.0));
        doc.content = "Voltage-gated Na+ channels drive rapid depolarization above -55 mV.".into();
        let doc_id = doc.id.clone();
        graph.add_node(doc);

        let context = LlmContext::build(&graph, &[doc_id], "Generate Study Cards");
        let provider = MockLlmProvider;
        let patch = provider.generate_patch(&context).await.unwrap();
        let result = graph.validate_and_stage_patch(&patch);

        assert_eq!(result.new_nodes.len(), 2);
        assert_eq!(result.new_nodes[0].kind, NodeKind::Flashcard);
        assert!(result.new_nodes[0].meta.answer.is_some());
        assert_eq!(result.new_nodes[1].kind, NodeKind::Checklist);
        assert_eq!(result.proposed_edges.len(), 1);
    }

    #[tokio::test]
    async fn test_mock_llm_summarize_uses_content_and_truncates() {
        let mut graph = StudyGraph::new("Summarize Test");
        let mut note = Node::new(NodeKind::Note, "Bayes Theorem", WorldPos::new(0.0, 0.0));
        note.content = "# Heading\n\nPosterior probability equals likelihood times prior divided by evidence.".into();
        let nid = note.id.clone();
        graph.add_node(note);

        let context = LlmContext::build(&graph, &[nid], "Summarize Node");
        let summary = MockLlmProvider.summarize(&context).await.unwrap();
        assert!(summary.contains("Bayes Theorem"));
        assert!(summary.contains("Posterior probability"));
        assert!(!summary.contains("# Heading"));
    }

    #[test]
    fn test_strip_json_fences_and_openai_url() {
        let fenced = "```json\n{\"summary\":\"ok\",\"ops\":[]}\n```";
        assert_eq!(strip_json_fences(fenced), "{\"summary\":\"ok\",\"ops\":[]}");

        let plain = "  {\"summary\":\"plain\",\"ops\":[]} ";
        assert_eq!(strip_json_fences(plain), "{\"summary\":\"plain\",\"ops\":[]}");

        let p1 = OpenAiCompatibleLlmProvider::new("sk-test", "https://api.openai.com/v1/", None);
        assert_eq!(
            p1.chat_completions_url(),
            "https://api.openai.com/v1/chat/completions"
        );
        let p2 = OpenAiCompatibleLlmProvider::new(
            "sk-test",
            "https://openrouter.ai/api/v1/chat/completions",
            Some("openai/gpt-4o-mini".into()),
        );
        assert_eq!(
            p2.chat_completions_url(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
    }

    #[tokio::test]
    async fn test_openai_provider_missing_api_key_error() {
        let mut graph = StudyGraph::new("Missing Key");
        let n = Node::new(NodeKind::Note, "Topic", WorldPos::new(0.0, 0.0));
        let id = n.id.clone();
        graph.add_node(n);

        let context = LlmContext::build(&graph, &[id], "Summarize Node");
        let provider = OpenAiCompatibleLlmProvider::new("  ", "https://api.openai.com/v1", None);
        let err = provider.summarize(&context).await.unwrap_err();
        assert!(matches!(err, LlmError::MissingApiKey));
    }

    #[test]
    fn test_llm_context_falls_back_to_extracted_ocr_text() {
        let mut graph = StudyGraph::new("OCR Context");
        let mut img = Node::new(NodeKind::Image, "Whiteboard", WorldPos::new(0.0, 0.0));
        img.meta.extracted_text = Some("E = mc^2".into());
        let img_id = img.id.clone();
        graph.add_node(img);

        let ctx = LlmContext::build(&graph, &[img_id], "Summarize Node");
        assert_eq!(ctx.focus_nodes[0].content, "E = mc^2");

        let mut img2 = Node::new(NodeKind::Image, "Diagram", WorldPos::new(200.0, 0.0));
        img2.content = "Caption notes".into();
        img2.meta.extracted_text = Some("F = ma".into());
        let img2_id = img2.id.clone();
        graph.add_node(img2);

        let ctx2 = LlmContext::build(&graph, &[img2_id], "Summarize Node");
        assert!(ctx2.focus_nodes[0].content.contains("Caption notes"));
        assert!(ctx2.focus_nodes[0].content.contains("[Extracted OCR Text]\nF = ma"));
    }

    #[test]
    fn test_llm_context_preferred_language_serialization() {
        let mut graph = StudyGraph::new("Lang");
        let n = Node::new(NodeKind::Note, "Topic", WorldPos::new(0.0, 0.0));
        let id = n.id.clone();
        graph.add_node(n);

        let ctx = LlmContext::build(&graph, &[id], "Summarize Node")
            .with_preferred_language(Some("  Spanish  ".into()));
        assert_eq!(ctx.preferred_language.as_deref(), Some("Spanish"));
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("\"preferred_language\":\"Spanish\""));

        let ctx_empty = LlmContext::build(&graph, &[], "Summarize Node")
            .with_preferred_language(Some("   ".into()));
        assert_eq!(ctx_empty.preferred_language, None);
    }

    #[tokio::test]
    async fn test_build_project_context_and_mock_chat() {
        let mut graph = StudyGraph::new("Project Chat");
        let mut doc = Node::new(NodeKind::Document, "Synaptic Plasticity", WorldPos::new(0.0, 0.0));
        doc.content = "NMDA receptors act as coincidence detectors.".into();
        let doc_id = doc.id.clone();
        graph.add_node(doc);

        let mut fc = Node::new(NodeKind::Flashcard, "NMDA Gate", WorldPos::new(260.0, 0.0));
        fc.content = "What unblocks Mg2+?".into();
        fc.meta.answer = Some("Postsynaptic depolarization".into());
        graph.add_node(fc);

        let ctx = LlmContext::build_project_context(&graph, &[doc_id], "Project Chat");
        assert_eq!(ctx.focus_nodes.len(), 2);
        assert_eq!(ctx.focus_nodes[0].title, "Synaptic Plasticity");
        assert!(ctx.focus_nodes[1].content.contains("[Answer]: Postsynaptic depolarization"));

        let reply = MockLlmProvider
            .ask_project_chat(&ctx, &[], "How does NMDA gating work?")
            .await
            .unwrap();
        assert!(reply.contains("Synaptic Plasticity"));
        assert!(reply.contains("How does NMDA gating work?"));
    }

    #[tokio::test]
    async fn test_parse_chat_action_response_and_mock_canvas_actions() {
        let raw = r#"I scheduled your review for tomorrow and created a flashcard.

```canvas_actions
{
  "summary": "Scheduled session",
  "ops": [
    {
      "op": "add_calendar_event",
      "date": "2026-11-22",
      "time": "09:30",
      "title": "Review Hebbian Learning",
      "notes": "Morning drill"
    }
  ]
}
```"#;
        let parsed = parse_chat_action_response(raw);
        assert_eq!(
            parsed.reply,
            "I scheduled your review for tomorrow and created a flashcard."
        );
        let patch = parsed.patch.expect("expected canvas_actions patch");
        assert_eq!(patch.ops.len(), 1);

        let mut graph = StudyGraph::new("Mock Chat Actions");
        let n = Node::new(NodeKind::Note, "Topic", WorldPos::new(0.0, 0.0));
        let nid = n.id.clone();
        graph.add_node(n);
        let ctx = LlmContext::build_project_context(&graph, &[nid], "Project Chat")
            .with_current_date(Some("2026-11-21".into()));

        let resp = MockLlmProvider
            .ask_project_chat_with_actions(&ctx, &[], "Schedule a study session on the calendar")
            .await
            .unwrap();
        assert!(resp.patch.is_some());
        let staged = graph.validate_and_stage_patch(resp.patch.as_ref().unwrap());
        assert_eq!(staged.calendar_events_changed, 1);
    }

    #[tokio::test]
    async fn test_forgiving_chat_action_parser_and_week_notes() {
        // Hyphenated fence tag, trailing comma, "action" alias, and one bad op alongside a valid op
        let raw = r#"Added a note for Wednesday and switched to Week view!

```canvas-actions
{
  "summary": "Week note",
  "ops": [
    {
      "action": "set_calendar_day_note",
      "date": "2026-11-25",
      "note": "Linear algebra problem set",
    },
    {
      "op": "unknown_op_should_drop",
      "foo": 123
    }
  ],
}
```"#;
        let parsed = parse_chat_action_response(raw);
        assert_eq!(
            parsed.reply,
            "Added a note for Wednesday and switched to Week view!"
        );
        assert!(parsed.parse_warning.is_some());
        let patch = parsed.patch.expect("valid op survived");
        assert_eq!(patch.ops.len(), 1);

        // Broken canvas_actions surfaces parse_warning instead of silent failure
        let bad = "Done!\n```canvas_actions\n{not json}\n```";
        let bad_parsed = parse_chat_action_response(bad);
        assert!(bad_parsed.patch.is_none());
        assert!(bad_parsed.parse_warning.is_some());

        // Bare JSON object at end with "actions"
        let trailing = "Created a card.\n{\"actions\": [{\"type\": \"create_study_node\", \"kind\": \"Note\", \"title\": \"Recap\", \"content\": \"Summary\"}]}";
        let t_parsed = parse_chat_action_response(trailing);
        assert_eq!(t_parsed.reply, "Created a card.");
        assert_eq!(t_parsed.patch.unwrap().ops.len(), 1);

        // Mock week + daily note request
        let mut graph = StudyGraph::new("Week Mock");
        let ctx = LlmContext::build_project_context(&graph, &[], "Project Chat")
            .with_current_date(Some("2026-11-21".into()));
        let w_resp = MockLlmProvider
            .ask_project_chat_with_actions(&ctx, &[], "Add a week note on my calendar for physics")
            .await
            .unwrap();
        let staged = graph.validate_and_stage_patch(w_resp.patch.as_ref().unwrap());
        assert_eq!(staged.calendar_notes_changed, 1);
    }

    #[tokio::test]
    async fn test_gemini_thought_multipart_week_expansion_search_images_and_audio_tx() {
        let gem_json = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [
                        { "thought": true, "text": "Internal reasoning trace..." },
                        { "text": "Planned your week on the calendar!" },
                        { "text": "```canvas_actions\n{\"summary\":\"Week plan\",\"ops\":[{\"op\":\"set_calendar_week_notes\",\"notes\":{\"Monday\":\"Read Ch 1\",\"Friday\":\"Quiz prep\"}}]}\n```" }
                    ]
                }
            }]
        });
        let extracted = extract_gemini_response_text(&gem_json).expect("gemini parts");
        assert!(!extracted.contains("Internal reasoning trace"));
        let parsed = parse_chat_action_response(&extracted);
        assert_eq!(parsed.reply, "Planned your week on the calendar!");
        let patch = parsed.patch.expect("expanded week ops");
        assert_eq!(patch.ops.len(), 3);

        let mut graph = StudyGraph::new("Image Search & Subtitles");
        let n = Node::new(NodeKind::Note, "Cellular Respiration", WorldPos::new(0.0, 0.0));
        let nid = n.id.clone();
        graph.add_node(n);
        let ctx = LlmContext::build_project_context(&graph, &[nid], "Project Chat")
            .with_current_date(Some("2026-11-21".into()));

        let img_resp = MockLlmProvider
            .ask_project_chat_with_actions(&ctx, &[], "Search for an image diagram of ATP synthase")
            .await
            .unwrap();
        let img_staged = graph.validate_and_stage_patch(img_resp.patch.as_ref().unwrap());
        assert_eq!(img_staged.pending_image_searches.len(), 1);
        assert_eq!(img_staged.proposed_edges.len(), 1);

        let tx = MockLlmProvider
            .transcribe_audio(&AudioTranscriptionRequest {
                bytes: vec![1, 2, 3, 4],
                filename: "lecture01.mp3".into(),
                mime_type: "audio/mpeg".into(),
                language_hint: Some("English".into()),
            })
            .await
            .unwrap();
        assert!(tx.text.contains("lecture01.mp3"));
        assert_eq!(tx.segments.len(), 2);
    }

    #[tokio::test]
    async fn test_section_summarize_includes_grouped_children() {
        let mut graph = StudyGraph::new("Section Summary Test");
        let sec = Node::new(NodeKind::Section, "Cellular Respiration", WorldPos::new(0.0, 0.0));
        let mut c1 = Node::new(NodeKind::Note, "Glycolysis", WorldPos::new(20.0, 40.0));
        c1.content = "Splits glucose into two pyruvate molecules in the cytosol.".into();
        let mut c2 = Node::new(NodeKind::Flashcard, "ATP Yield", WorldPos::new(280.0, 40.0));
        c2.content = "How many net ATP from glycolysis?".into();
        c2.meta.answer = Some("2 net ATP and 2 NADH".into());

        let sec_id = sec.id.clone();
        let c1_id = c1.id.clone();
        let c2_id = c2.id.clone();
        graph.add_node(sec);
        graph.add_node(c1);
        graph.add_node(c2);

        let context = LlmContext::build(
            &graph,
            &[sec_id, c1_id, c2_id],
            "Review and summarize this section and its grouped study nodes",
        );
        let summary = MockLlmProvider.summarize(&context).await.unwrap();
        assert!(summary.contains("Cellular Respiration"));
        assert!(summary.contains("2 cards"));
        assert!(summary.contains("Glycolysis"));
        assert!(summary.contains("ATP Yield"));
    }

    #[test]
    fn test_universal_provider_detection_and_url_normalization() {
        assert_eq!(
            normalize_openai_chat_completions_url("https://api.airforce"),
            "https://api.airforce/v1/chat/completions"
        );
        assert_eq!(
            normalize_openai_chat_completions_url("https://api.airforce/v1/"),
            "https://api.airforce/v1/chat/completions"
        );
        assert_eq!(
            normalize_openai_chat_completions_url("https://api.perplexity.ai"),
            "https://api.perplexity.ai/chat/completions"
        );
        assert!(is_local_unauthenticated_url("http://127.0.0.1:11434/v1"));
        assert!(!is_local_unauthenticated_url("https://api.openai.com/v1"));

        let air = resolve_universal_llm_provider("sk-air-demo", "", None).unwrap();
        assert!(air.1.contains("Airforce"));
        let or = resolve_universal_llm_provider("sk-or-v1-demo", "", None).unwrap();
        assert!(or.1.contains("OpenRouter"));
        let groq = resolve_universal_llm_provider("gsk_demo", "", None).unwrap();
        assert!(groq.1.contains("Groq"));
        let ant = resolve_universal_llm_provider("sk-ant-demo", "", None).unwrap();
        assert!(ant.1.contains("Anthropic"));
        let gem = resolve_universal_llm_provider("AIzaDemoKey", "", None).unwrap();
        assert!(gem.1.contains("Gemini"));
        let custom_ds = resolve_universal_llm_provider("ds-custom", "", Some("deepseek-chat".into())).unwrap();
        assert!(custom_ds.1.contains("DeepSeek"));
        let local = resolve_universal_llm_provider("", "http://127.0.0.1:11434/v1", Some("qwen2.5:3b".into())).unwrap();
        assert!(local.1.contains("Local"));
        assert!(resolve_universal_llm_provider("", "https://api.airforce/v1", None).is_none());

        let ant_p = AnthropicLlmProvider::new("sk-ant-demo", "https://api.anthropic.com/", None);
        assert_eq!(ant_p.messages_url(), "https://api.anthropic.com/v1/messages");
    }
}
