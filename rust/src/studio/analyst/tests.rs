//! Exercise a complete creation and publication without a database, queue, or model server.

use super::*;
use crate::studio::model::{GenerateResult, Inference};
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::Duration;

struct Model {
    response: String,
    fail: bool,
    calls: Mutex<Vec<(String, GenerateOptions)>>,
}

impl Model {
    fn new(response: &str) -> Self {
        Self {
            response: response.into(),
            fail: false,
            calls: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl Inference for Model {
    async fn generate(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)> {
        self.calls
            .lock()
            .unwrap()
            .push((prompt.to_string(), opts.clone()));
        if self.fail {
            anyhow::bail!("model unavailable");
        }
        Ok((
            GenerateResult {
                response: self.response.clone(),
                thinking: "private reasoning, not product prose".into(),
                model: "model-that-answered".into(),
                total_duration: Duration::from_millis(123),
                eval_count: 42,
                prompt_eval_count: 100,
                completion_reason: Some("stop".into()),
                raw_response_body: "{}".into(),
            },
            serde_json::json!({"actual_request": true, "prompt": prompt}),
        ))
    }

    fn model(&self) -> &str {
        "configured-model"
    }

    fn request_body(&self, _: &str, _: &GenerateOptions) -> serde_json::Value {
        panic!("provenance must use the request actually sent")
    }
}

#[derive(Default)]
struct Publications {
    outputs: Mutex<Vec<MomentumOutput>>,
    fail: bool,
}

#[async_trait]
impl Publisher<MomentumSummary> for Publications {
    type Receipt = usize;

    async fn publish(&self, output: &MomentumOutput) -> Result<usize> {
        if self.fail {
            anyhow::bail!("publication unavailable");
        }
        let mut outputs = self.outputs.lock().unwrap();
        outputs.push(output.clone());
        Ok(outputs.len())
    }
}

fn assignment() -> Assignment {
    Assignment {
        entity_type: "player".into(),
        entity_name: "Vale Kerr".into(),
        sport: "nba".into(),
        context: MomentumContext::new(
            2026,
            Some(Form {
                body: "Kerr's measured form is gaining ground.".into(),
                headline: Some("Kerr sharpens the profile".into()),
                season: Some(2026),
                generated_at: Some("2026-09-20".into()),
                input_hash: Some("scout-hash".into()),
            }),
            Some(Mood {
                body: "The feeling around Kerr is warming.".into(),
                headline: Some("Kerr wins the room".into()),
                sentiment: Some(65),
                generated_at: Some("2026-09-20".into()),
                input_hash: Some("influencer-hash".into()),
            }),
            Snapshot {
                rating_slope: Some(20.0),
                rating_samples: 8,
                rating_window_start: Some("2026-08-01".into()),
                rating_window_end: Some("2026-09-20".into()),
                vibe_slope: Some(30.0),
                vibe_samples: 6,
                vibe_window_start: Some("2026-08-30".into()),
                vibe_window_end: Some("2026-09-20".into()),
                momentum_score: Some(25.0),
                generated_at: Some("2026-09-20".into()),
            },
        ),
        memory: Some("A patient playmaker.".into()),
        voice_num_ctx: 4096,
    }
}

#[tokio::test]
async fn complete_assignment_publishes_validated_product_with_actual_provenance() {
    let model = Model::new(
        r#"{"body":"The form is rising while the mood confirms the move.","headline":"Kerr gains ground on both fronts"}"#,
    );
    let publisher = Publications::default();
    let assignment = assignment();
    assert_eq!(
        run(&Studio::new(&model), &assignment, &publisher)
            .await
            .unwrap(),
        Outcome::Published(1)
    );
    let calls = model.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].1.num_predict, 700);
    assert_eq!(calls[0].1.num_ctx, 4096);
    assert_eq!(calls[0].1.temperature, Some(0.3));
    assert_eq!(
        calls[0].1.format_schema,
        Some(crate::studio::form::card_schema(false))
    );
    assert!(calls[0].0.contains(assignment.memory.as_deref().unwrap()));
    assert_eq!(
        calls[0].1.system.as_deref(),
        Some(MOMENTUM_SYSTEM_PROMPT.as_str())
    );
    let outputs = publisher.outputs.lock().unwrap();
    assert_eq!(outputs.len(), 1);
    let out = &outputs[0];
    assert_eq!(out.direction, "rising");
    assert_eq!(out.score, 2);
    assert_eq!(
        out.headline.as_deref(),
        Some("Kerr gains ground on both fronts")
    );
    assert_eq!(out.season, 2026);
    assert_eq!(out.provenance.model_version, "model-that-answered");
    assert_eq!(out.provenance.prompt_version, "momentum-s32");
    assert_eq!(
        out.provenance.input_hash.as_deref(),
        Some(assignment.context.input_hash.as_str())
    );
    assert_eq!(
        out.input_components_json,
        assignment.context.input_components_json
    );
    assert!(!out.blurb.contains("private reasoning"));
    let call = out.call.as_ref().unwrap();
    assert_eq!(call.built_prompt, calls[0].0);
    assert_eq!(
        call.request_body,
        serde_json::json!({"actual_request": true, "prompt": calls[0].0})
    );
    assert_eq!(call.eval_count, Some(42));
    assert_eq!(call.wall_ms, Some(123));
}

#[tokio::test]
async fn empty_material_finishes_without_model_or_publication() {
    let model = Model::new("must not be called");
    let publisher = Publications::default();
    let mut assignment = assignment();
    assignment.context = MomentumContext::new(2026, None, None, Snapshot::default());
    assert_eq!(
        run(&Studio::new(&model), &assignment, &publisher)
            .await
            .unwrap(),
        Outcome::NoMaterial
    );
    assert!(model.calls.lock().unwrap().is_empty());
    assert!(publisher.outputs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn malformed_or_guard_rejected_prose_cannot_reach_publication() {
    for raw in ["SCORE: 5", "READ: The room cools (sentiment=30)."] {
        let model = Model::new(raw);
        let publisher = Publications::default();
        assert!(
            run(&Studio::new(&model), &assignment(), &publisher)
                .await
                .is_err(),
            "{raw}"
        );
        assert_eq!(model.calls.lock().unwrap().len(), 1);
        assert!(publisher.outputs.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn model_failure_propagates_without_retry_or_publication() {
    let mut model = Model::new("");
    model.fail = true;
    let publisher = Publications::default();
    let error = run(&Studio::new(&model), &assignment(), &publisher)
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("model unavailable"));
    assert_eq!(model.calls.lock().unwrap().len(), 1);
    assert!(publisher.outputs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn exhausted_surface_rewrites_never_publish() {
    let model = Model::new(&format!(
        r#"{{"body":"{}","headline":"Kerr gains ground"}}"#,
        "x".repeat(1201)
    ));
    let publisher = Publications::default();
    assert!(run(&Studio::new(&model), &assignment(), &publisher)
        .await
        .is_err());
    let calls = model.calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[2].0.matches("Output correction:").count(), 2);
    assert!(publisher.outputs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn publication_failure_is_not_reported_as_success_or_retried() {
    let model = Model::new("READ: The form is rising.");
    let publisher = Publications {
        fail: true,
        ..Default::default()
    };
    let error = run(&Studio::new(&model), &assignment(), &publisher)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("publication unavailable"));
    assert_eq!(model.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn partial_material_supports_creation_and_missing_measurement_stays_neutral() {
    let model = Model::new("READ: The mood is calm.\nHEADLINE: Another player gains ground");
    let publisher = Publications::default();
    let mut assignment = assignment();
    assignment.context = MomentumContext::new(
        2026,
        None,
        Some(Mood {
            body: "The room is calm.".into(),
            headline: None,
            sentiment: Some(50),
            generated_at: Some("2026-09-20".into()),
            input_hash: Some("vibe-only".into()),
        }),
        Snapshot::default(),
    );
    assignment.voice_num_ctx = 16384;
    run(&Studio::new(&model), &assignment, &publisher)
        .await
        .unwrap();
    let outputs = publisher.outputs.lock().unwrap();
    assert_eq!(outputs[0].direction, "steady");
    assert_eq!(outputs[0].score, 0);
    assert!(outputs[0].headline.is_none());
    assert_eq!(model.calls.lock().unwrap()[0].1.num_predict, 700);
}

#[tokio::test]
async fn parser_abstention_remains_explicit_at_the_shared_session_boundary() {
    struct Abstain;
    impl Parser<()> for Abstain {
        fn parse(&self, _: &str) -> Result<Option<()>> {
            Ok(None)
        }
    }
    let model = Model::new("uncommitted");
    let out = Studio::new(&model)
        .extract("evidence", &GenerateOptions::default(), &Abstain)
        .await
        .unwrap();
    assert!(out.value.is_none());
    assert_eq!(out.raw_response, "uncommitted");
}
