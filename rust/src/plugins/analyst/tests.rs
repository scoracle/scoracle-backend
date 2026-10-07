//! Exercise prepared cognition and actual provenance without application services.

use super::prompt::*;
use super::*;
use crate::harness::model::{GenerateResult, Inference};
use crate::harness::{model::GenerateOptions, Parser};
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

fn assignment() -> Assignment {
    Assignment {
        entity_id: 7,
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
        voice_num_ctx: 4096,
    }
}

// The unchanged Rust oracle keeps parser/model/provenance checks offline;
// the live parity test below exercises the SQL producer and real creation path.
async fn create_from_reference(
    studio: &Studio<'_>,
    assignment: &Assignment,
) -> Result<Option<MomentumOutput>> {
    if assignment.context.empty() {
        return Ok(None);
    }
    let score = assignment.context.snapshot.momentum_score;
    articulate(
        studio,
        assignment,
        (
            momentum_direction_from_score(score).into(),
            momentum_conviction_from_score(score),
        ),
    )
    .await
}

#[tokio::test]
async fn complete_assignment_creates_validated_product_with_actual_provenance() {
    let model = Model::new(
        r#"{"blurb":"Kerr's form and mood both strengthen, within their separate windows."}"#,
    );
    let assignment = assignment();
    let out = create_from_reference(&Studio::new(&model), &assignment)
        .await
        .unwrap()
        .unwrap();
    let calls = model.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].1.num_predict, MOMENTUM_NUM_PREDICT);
    assert_eq!(calls[0].1.num_ctx, 4096);
    assert_eq!(calls[0].1.temperature, Some(MOMENTUM_TEMPERATURE));
    assert!(calls[0].1.format_schema.is_some());
    assert!(calls[0]
        .0
        .contains("Kerr's measured form is gaining ground."));
    assert_eq!(calls[0].1.system.as_deref(), Some(MOMENTUM_SYSTEM_PROMPT));
    assert_eq!(out.direction, "rising");
    assert_eq!(out.score, 2);
    assert_eq!(out.headline.as_deref(), Some("Vale Kerr: current momentum"));
    assert_eq!(out.season, 2026);
    assert_eq!(out.provenance.model_version, "model-that-answered");
    assert_eq!(out.provenance.prompt_version, MOMENTUM_PROMPT_VERSION);
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
async fn empty_material_finishes_without_model_call() {
    let model = Model::new("must not be called");
    let mut assignment = assignment();
    assignment.context = MomentumContext::new(2026, None, None, Snapshot::default());
    assert!(create_from_reference(&Studio::new(&model), &assignment)
        .await
        .unwrap()
        .is_none());
    assert!(model.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn malformed_or_guard_rejected_prose_cannot_become_a_product() {
    for raw in ["SCORE: 5", "READ: The room cools (sentiment=30)."] {
        let model = Model::new(raw);
        assert!(
            create_from_reference(&Studio::new(&model), &assignment())
                .await
                .is_err(),
            "{raw}"
        );
        assert_eq!(model.calls.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn model_failure_propagates_without_retry() {
    let mut model = Model::new("");
    model.fail = true;
    let error = create_from_reference(&Studio::new(&model), &assignment())
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("model unavailable"));
    assert_eq!(model.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn blank_prose_produces_no_product() {
    let model = Model::new(r#"{"blurb":"  "}"#);
    assert!(create_from_reference(&Studio::new(&model), &assignment())
        .await
        .is_err());
    let calls = model.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
}

#[tokio::test]
async fn partial_material_supports_creation_and_missing_measurement_stays_neutral() {
    let model =
        Model::new(r#"{"blurb":"The mood reading is calm; measured movement is unavailable."}"#);
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
    let out = create_from_reference(&Studio::new(&model), &assignment)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(out.direction, "steady");
    assert_eq!(out.score, 0);
    assert_eq!(out.headline.as_deref(), Some("Vale Kerr: current momentum"));
    assert_eq!(
        model.calls.lock().unwrap()[0].1.num_predict,
        MOMENTUM_NUM_PREDICT
    );
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
        .extract("evidence", &GenerateOptions::default(), &Abstain, |_| None)
        .await
        .unwrap();
    assert!(out.value.is_none());
    assert_eq!(out.raw_response, "uncommitted");
}

#[tokio::test]
async fn empty_context_and_sql_failure_never_call_the_model() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(50))
        .connect_lazy("postgres://localhost:1/unused")
        .unwrap();
    let model = Model::new("must not be called");
    let mut empty = assignment();
    // A momentum number alone is not material without a finished reading/rail.
    empty.context = MomentumContext::new(
        2026,
        None,
        None,
        Snapshot {
            momentum_score: Some(80.0),
            ..Snapshot::default()
        },
    );
    assert!(create(&pool, &Studio::new(&model), &empty)
        .await
        .unwrap()
        .is_none());
    assert!(create(&pool, &Studio::new(&model), &assignment())
        .await
        .is_err());
    assert!(model.calls.lock().unwrap().is_empty());
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; pure read-only SQL, no schema needed"]
async fn sql_metrics_match_legacy_thresholds_and_creation() -> Result<()> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL"))
        .await?;
    let mut scores = (-1000..=1000)
        .map(|i| Some(f64::from(i) / 10.0))
        .collect::<Vec<_>>();
    scores.extend([
        None,
        Some(-0.0),
        Some(f64::from_bits(1)),
        Some(-f64::from_bits(1)),
        Some(f64::MIN),
        Some(f64::MAX),
        Some(f64::NEG_INFINITY),
        Some(f64::INFINITY),
        Some(f64::NAN),
    ]);
    // Immediate floating-point neighbors detect accidental decimal rounding.
    for threshold in [5.0_f64, 10.0, 20.0, 35.0, 55.0, 80.0] {
        for bits in [
            threshold.to_bits() - 1,
            threshold.to_bits(),
            threshold.to_bits() + 1,
        ] {
            let value = f64::from_bits(bits);
            scores.extend([Some(value), Some(-value)]);
        }
    }
    for &score in &scores {
        assert_eq!(
            momentum_metrics(&pool, score).await?,
            (
                momentum_direction_from_score(score).into(),
                momentum_conviction_from_score(score)
            ),
            "score={score:?}"
        );
    }
    let mut creations = 0;
    // Every finished-card/trajectory availability combination retains the same
    // material decision, request/options, product and actual-call provenance.
    for mask in 0..8 {
        for score in [
            None,
            Some(-80.0),
            Some(-10.0),
            Some(0.0),
            Some(9.999999999999998),
            Some(10.0),
            Some(80.0),
        ] {
            let mut a = assignment();
            let rating = (mask & 1 != 0).then(|| a.context.rating.clone().unwrap());
            let vibe = (mask & 2 != 0).then(|| a.context.vibe.clone().unwrap());
            let mut snapshot = if mask & 4 != 0 {
                a.context.snapshot.clone()
            } else {
                Snapshot::default()
            };
            snapshot.momentum_score = score;
            a.context = MomentumContext::new(2026, rating, vibe, snapshot);
            let model = Model::new(
                r#"{"blurb":"Kerr's measured form and mood retain their separate windows."}"#,
            );
            let reference_model = Model::new(&model.response);
            let actual = create(&pool, &Studio::new(&model), &a).await?;
            let expected = create_from_reference(&Studio::new(&reference_model), &a).await?;
            assert_eq!(actual.is_none(), expected.is_none());
            let actual_calls = model.calls.lock().unwrap();
            let expected_calls = reference_model.calls.lock().unwrap();
            assert_eq!(actual_calls.len(), expected_calls.len());
            if let (Some(actual), Some(expected)) = (actual, expected) {
                assert_eq!(
                    (actual.direction.as_str(), actual.score),
                    (expected.direction.as_str(), expected.score)
                );
                assert_eq!(actual.blurb, expected.blurb);
                assert_eq!(actual.headline, expected.headline);
                assert_eq!(actual.season, expected.season);
                assert_eq!(actual.input_components_json, expected.input_components_json);
                assert_eq!(actual.provenance.input_hash, expected.provenance.input_hash);
                assert_eq!(actual.provenance.model_version, "model-that-answered");
                assert_eq!(
                    actual.call.as_ref().unwrap().request_body,
                    expected.call.as_ref().unwrap().request_body
                );
                assert_eq!(actual_calls.len(), 1);
                assert_eq!(actual_calls[0].0, expected_calls[0].0);
                assert_eq!(
                    format!("{:?}", actual_calls[0].1),
                    format!("{:?}", expected_calls[0].1)
                );
            }
            creations += 1;
        }
    }
    eprintln!(
        "{} SQL/legacy score cases and {creations} creation/request parity cases verified",
        scores.len()
    );
    Ok(())
}
