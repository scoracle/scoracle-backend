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
async fn period_card_contract() {
    let source = |id, text: &str| SourceContext {
        classifier_world: None,
        classification_id: id,
        article_id: id,
        headline: "Club reporting".into(),
        context: text.into(),
        source: "Wire".into(),
        published_at_epoch: Some(1709164800),
    };
    let a = Assignment::from_parts(prompt::Parts {
        subject: EntityMeta {
            name: "Club".into(),
            entity_type: "team".into(),
            entity_id: 7,
            sport: "NBA".into(),
        },
        period: prompt::Period {
            season: 2024,
            week: 1,
            start: 1709164700,
            end: 1709769500,
            cutoff: 1709164900,
        },
        sources: vec![
            source(1, "Morgan was hopeful."),
            source(2, "Ellis disagreed. No other supporters were interviewed."),
        ],
        history: vec![],
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
    assert_eq!(out.provenance.input_ids, vec![1, 2]);
    assert!(out.vibe_prompt.as_ref().unwrap().contains("\n\n"));
    assert_eq!(
        receipt["input_components"]["sources"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let world: Value = serde_json::from_str(&a.parts.assemble()).unwrap();
    assert!(world["FRESH EVIDENCE"][1]["publisher_text"]
        .as_str()
        .unwrap()
        .ends_with("No other supporters were interviewed."));
    assert!(world["FRESH EVIDENCE"][0]
        .get("classification_id")
        .is_none());
    let mut changed = a.parts.clone();
    changed.period.cutoff += 1;
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
    let retained: Value = serde_json::from_str(include_str!(
        "../../../fixtures/influencer/period-card-v19-smollm3.jsonl"
    ))
    .unwrap();
    let receipt = &retained["receipt"];
    let assignment = Assignment::from_parts(
        serde_json::from_value(receipt["input_components"].clone()).unwrap(),
    )
    .unwrap();
    let backend = crate::harness::providers::llama::LlamaClient::new(
        "http://localhost:8080",
        receipt["model_version"].as_str().unwrap(),
        Duration::from_secs(1),
        Some(false),
    )
    .unwrap();
    assert_eq!(assignment.parts.assemble(), receipt["packet"]);
    assert_ne!(
        prompt::request_hash(&backend, &assignment, 4096).unwrap(),
        receipt["input_hash"]
    );
    let mut changed = assignment.parts.clone();
    changed.sources[0].classifier_world = Some(json!({"qualified_claims":[{
        "target_relation":"unknown", "kind":"withdrawn", "time_scope":"historical",
        "qualifiers":{"speaker":null,"negation":["not confirmed"]}}],"signals_unassessed":true}));
    let with_world = Assignment::from_parts(changed).unwrap();
    let packet: Value = serde_json::from_str(&with_world.parts.assemble()).unwrap();
    assert_eq!(
        packet["FRESH EVIDENCE"][0]["classifier_world"]["qualified_claims"][0]["target_relation"],
        "unknown"
    );
    assert_ne!(with_world.input_hash, assignment.input_hash);
}

#[tokio::test]
async fn an_incomplete_period_never_calls_the_model() {
    let mut parts: prompt::Parts = serde_json::from_value(
        serde_json::from_str::<Value>(include_str!(
            "../../../fixtures/influencer/period-card-v19-smollm3.jsonl"
        ))
        .unwrap()["receipt"]["input_components"]
            .clone(),
    )
    .unwrap();
    parts.excluded.push(json!({"article_id":999,"classification_id":999,"in_period":true,"reason":"packet_budget_exceeded"}));
    let model = Model {
        calls: AtomicUsize::new(0),
        reply: "must not be used",
    };
    let (output, receipt) = create(&model, &Assignment::from_parts(parts).unwrap(), 32768)
        .await
        .unwrap();
    assert!(output.is_none());
    assert_eq!(model.calls.load(Ordering::Relaxed), 0);
    assert_eq!(receipt["reason"], "incomplete_reporting_period");
}
