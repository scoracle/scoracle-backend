use super::*;
use crate::harness::model::{GenerateOptions, GenerateResult};
use crate::harness::Parser;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Model {
    calls: AtomicUsize,
    reply: &'static str,
}
#[async_trait]
impl Inference for Model {
    async fn generate(
        &self,
        prompt: &str,
        options: &GenerateOptions,
    ) -> Result<(GenerateResult, Value)> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok((
            GenerateResult {
                response: self.reply.into(),
                thinking: String::new(),
                model: "test".into(),
                total_duration: Duration::from_millis(1),
                prompt_eval_count: 100,
                eval_count: 20,
                completion_reason: Some("stop".into()),
                raw_response_body: self.reply.into(),
            },
            self.request_body(prompt, options),
        ))
    }
    fn model(&self) -> &str {
        "test"
    }
    fn request_body(&self, prompt: &str, options: &GenerateOptions) -> Value {
        json!({"prompt":prompt,"system":options.system,"format":options.format_schema})
    }
}

#[tokio::test]
async fn day_card_contract() {
    let source = |id, text: &str| SourceContext {
        classifier_world: None,
        classification_id: id,
        article_id: id,
        headline: "Club reporting".into(),
        context: text.into(),
        source: "Wire".into(),
        published_at_epoch: Some(1709164800),
    };
    let a = Assignment::from_parts(request::Parts {
        database_record: None,
        memories: vec![],
        subject: EntityMeta {
            name: "Club".into(),
            entity_type: "team".into(),
            entity_id: 7,
            sport: "NBA".into(),
        },
        day: request::Day {
            start: 1709164700,
            end: 1709251100,
            cutoff: 1709164900,
        },
        sources: vec![
            source(1, "Morgan was hopeful."),
            source(2, "Ellis disagreed. No other supporters were interviewed."),
        ],
        excluded: vec![],
    })
    .unwrap();
    let model = Model {
        calls: AtomicUsize::new(0),
        reply: r#"{"score":50,"headline":"Two speakers differ","body":"Morgan was hopeful.\n\nEllis disagreed. No other supporters were interviewed."}"#,
    };
    let (out, receipt) = create(&model, &a, 4096).await.unwrap();
    let out = out.unwrap();
    assert_eq!(model.calls.load(Ordering::Relaxed), 1);
    assert_eq!(out.sentiment, Some(50));
    let replayed = output_from_receipt(&a, &receipt).unwrap().unwrap();
    assert_eq!(replayed.sentiment, out.sentiment);
    assert_eq!(replayed.vibe_prompt, out.vibe_prompt);
    assert_eq!(replayed.provenance.input_hash, out.provenance.input_hash);
    assert_eq!(replayed.request_body(), out.request_body());
    assert_eq!(model.calls.load(Ordering::Relaxed), 1);
    let mut failed_receipt = receipt.clone();
    failed_receipt["error"] = json!("failed validation");
    assert!(output_from_receipt(&a, &failed_receipt).is_err());
    assert_eq!(out.provenance.input_ids, vec![1, 2]);
    assert!(out.vibe_prompt.as_ref().unwrap().contains("\n\n"));
    assert_eq!(
        receipt["input_components"]["sources"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let prompt = a.parts.assemble();
    let world: Value = serde_json::from_str(&prompt).unwrap();
    let positions: Vec<_> = [
        "task",
        "target_context",
        "target",
        "evidence_context",
        "evidence",
    ]
    .iter()
    .map(|key| prompt.find(&format!("\n  \"{key}\":")).unwrap())
    .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(world.as_object().unwrap().len(), 5);
    assert_eq!(
        world["target_context"],
        crate::tools::prompt::TARGET_CONTEXT
    );
    assert_eq!(world["task"], TASK);
    assert!(!prompt.contains("instructions.target"));
    assert!(world["evidence"]["reports"][1]["publisher_excerpt"]
        .as_str()
        .unwrap()
        .ends_with("No other supporters were interviewed."));
    assert!(world["evidence"]["reports"][0]
        .get("classification_id")
        .is_none());
    for kind in ["team", "player"] {
        let mut parts = a.parts.clone();
        parts.subject.entity_type = kind.into();
        parts.database_record = Some(serde_json::from_value(json!({
            "position": "G", "role": "captain",
            "affiliation_on_record": {"team": "Cedar", "observed_at": "2025-01-01", "source_document_id": 27},
            "location": {"venue": "Pine Ground", "city": "Cedar City"}
        })).unwrap());
        let influencer: Value = serde_json::from_str(&parts.assemble()).unwrap();
        let reports: Vec<_> = parts
            .sources
            .iter()
            .map(crate::harness::skills::journalist::request::CorpusItem::from)
            .collect();
        let journalist: Value =
            serde_json::from_str(&crate::harness::skills::journalist::request::assemble(
                &parts.subject,
                &reports,
                parts.database_record.as_ref(),
            ))
            .unwrap();
        assert_eq!(influencer["target"], journalist["target"]);
        assert_eq!(influencer["evidence"]["reports"], journalist["evidence"]);
        let attrs = &influencer["target"]["recorded_attributes"];
        assert_eq!(attrs["position"], "guard");
        assert_eq!(attrs["role"], "captain");
        assert_eq!(attrs["team_on_record"]["name"], "Cedar");
        assert_eq!(attrs["team_on_record"]["observed_on"], "2025-01-01");
        assert!(attrs["team_on_record"].get("source_document_id").is_none());
        assert_eq!(attrs["home_venue_on_record"], "Pine Ground");
        assert!(!influencer["evidence"].to_string().contains("Pine Ground"));
    }
    let mut changed = a.parts.clone();
    changed.day.cutoff += 1;
    assert_eq!(
        a.input_hash,
        Assignment::from_parts(changed.clone()).unwrap().input_hash
    );
    changed.sources[1].context.push_str(" A correction.");
    assert_ne!(
        a.input_hash,
        Assignment::from_parts(changed).unwrap().input_hash
    );
    for score in [0, 100] {
        assert!(VibeParser
            .parse(&json!({"score":score,"headline":"Reading","body":"A reading."}).to_string())
            .unwrap()
            .is_some());
    }
    for raw in [
        r#"{"score":101,"headline":"Reading","body":"A reading."}"#,
        r#"{"score":null,"headline":"Reading","body":"A reading."}"#,
        r#"{"headline":"Reading","body":"A reading."}"#,
        r#"{"score":0,"headline":"Reading","body":""}"#,
    ] {
        assert!(VibeParser.parse(raw).is_err());
    }
    assert!(VibeParser
        .parse(r#"{"score":null,"headline":null,"body":null}"#)
        .unwrap()
        .is_none());
    let observed_model = Model {
        calls: AtomicUsize::new(0),
        reply: r#"{"score":50,"headline":"First\nSecond","body":"An unsupported interpretation."}"#,
    };
    let archive =
        std::env::temp_dir().join(format!("influencer-observed-{}.jsonl", std::process::id()));
    let (observed, receipt) = crate::harness::rejections::TEST_PATH
        .scope(
            archive.clone(),
            crate::harness::rejections::TEST_OBSERVE.scope(true, create(&observed_model, &a, 4096)),
        )
        .await
        .unwrap();
    assert!(observed.is_some());
    assert!(receipt.get("validation_warning").is_none());
    assert!(receipt.get("error").is_none());
    let replay = crate::harness::rejections::TEST_OBSERVE
        .sync_scope(false, || output_from_receipt(&a, &receipt))
        .unwrap()
        .unwrap();
    assert_eq!(replay.hook.as_deref(), Some("First Second"));
    assert_eq!(observed_model.calls.load(Ordering::Relaxed), 1);
    let broken = Model {
        calls: AtomicUsize::new(0),
        reply: "truncated",
    };
    let (out, receipt) = create(&broken, &a, 4096).await.unwrap();
    assert!(out.is_none());
    assert!(receipt.get("error").is_some());
    assert_eq!(receipt["raw_response"], "truncated");
    assert_eq!(broken.calls.load(Ordering::Relaxed), 1);

    // Retained v19 packets remain readable; the new contract invalidates old attempt reuse.
    let retained: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/influencer/period-card-v19-smollm3.jsonl"
    )))
    .unwrap();
    let receipt = &retained["receipt"];
    let mut retained_parts = receipt["input_components"].clone();
    retained_parts["day"] = retained_parts["period"].clone();
    let mut daily: request::Parts = serde_json::from_value(retained_parts).unwrap();
    assert!(
        Assignment::from_parts(daily.clone()).is_err(),
        "weekly generation is retired"
    );
    daily.day.end = daily.day.start + 86400;
    let assignment = Assignment::from_parts(daily).unwrap();
    let backend = crate::harness::providers::llama::LlamaClient::new(
        "http://localhost:8080",
        receipt["model_version"].as_str().unwrap(),
        Duration::from_secs(1),
        Some(false),
    )
    .unwrap();
    assert_ne!(assignment.parts.assemble(), receipt["packet"]);
    assert_ne!(
        request::request_hash(&backend, &assignment, 4096).unwrap(),
        receipt["input_hash"]
    );
    let mut changed = assignment.parts.clone();
    changed.sources[0].classifier_world = Some(json!({"qualified_claims":[{
        "target_relation":"unknown", "kind":"withdrawn", "time_scope":"historical",
        "qualifiers":{"speaker":null,"negation":["not confirmed"]}}],"signals_unassessed":true}));
    let with_world = Assignment::from_parts(changed).unwrap();
    let packet: Value = serde_json::from_str(&with_world.parts.assemble()).unwrap();
    assert!(packet["evidence"]["reports"][0]
        .get("classifier_world")
        .is_none());
    assert_eq!(
        with_world.parts.sources[0]
            .classifier_world
            .as_ref()
            .unwrap()["qualified_claims"][0]["target_relation"],
        "unknown"
    );
    assert_ne!(with_world.input_hash, assignment.input_hash);
}

#[tokio::test]
async fn an_incomplete_day_never_calls_the_model() {
    let mut captured = serde_json::from_str::<Value>(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/influencer/period-card-v19-smollm3.jsonl"
    )))
    .unwrap()["receipt"]["input_components"]
        .clone();
    captured["day"] = captured["period"].clone();
    let mut parts: request::Parts = serde_json::from_value(captured).unwrap();
    parts.day.end = parts.day.start + 86400;
    parts.excluded.push(json!({"article_id":999,"classification_id":999,"in_day":true,"reason":"packet_budget_exceeded"}));
    let model = Model {
        calls: AtomicUsize::new(0),
        reply: "must not be used",
    };
    let (output, receipt) = create(&model, &Assignment::from_parts(parts).unwrap(), 32768)
        .await
        .unwrap();
    assert!(output.is_none());
    assert_eq!(model.calls.load(Ordering::Relaxed), 0);
    assert_eq!(receipt["reason"], "incomplete_reporting_day");
}

#[test]
fn decodable_prose_is_preserved_at_any_length() {
    let oversized =
        json!({"score":75,"headline":"Hope","body":"x".repeat(request::BODY_MAX_CHARS+1)});
    assert!(VibeParser.parse(&oversized.to_string()).is_ok());
    assert_eq!(
        request::schema()["oneOf"][0]["properties"]["body"]["maxLength"],
        request::BODY_MAX_CHARS
    );
}

include!("../../../tests/internal/influencer/tests.rs");
