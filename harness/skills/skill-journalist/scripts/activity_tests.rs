use super::*;
use crate::harness::skills::journalist::{self, request::Assignment};
use crate::harness::{
    model::{GenerateOptions, GenerateResult, Inference},
    Studio,
};
use crate::tools::meta::EntityMeta;
use async_trait::async_trait;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

struct Model(AtomicUsize);
#[async_trait]
impl Inference for Model {
    async fn generate(
        &self,
        _: &str,
        _: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok((
            GenerateResult {
                response: r#"{"headline":"Cedar win","paragraphs":[{"body":"Wire reports Cedar won.","report_ids":[1]}]}"#.into(),
                thinking: String::new(),
                model: "fake".into(),
                total_duration: Duration::from_millis(1),
                prompt_eval_count: 10,
                eval_count: 10,
                completion_reason: Some("stop".into()),
                raw_response_body: "{}".into(),
            },
            json!({}),
        ))
    }
    fn model(&self) -> &str {
        "fake"
    }
    fn request_body(&self, _: &str, _: &GenerateOptions) -> serde_json::Value {
        json!({})
    }
}
fn assignment(corpus: Vec<CorpusItem>, now: i64) -> Assignment {
    journalist::fresh::prepare(
        EntityMeta {
            name: "Cedar".into(),
            entity_id: 7,
            entity_type: "team".into(),
            sport: "FOOTBALL".into(),
        },
        corpus,
        &[],
        now,
    )
    .unwrap()
}
#[tokio::test]
async fn empty_selection_and_activity_failure_never_call_the_model() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(50))
        .connect_lazy("postgres://localhost:1/unused")
        .unwrap();
    let model = Model(AtomicUsize::new(0));
    let output = journalist::create(&pool, &Studio::new(&model), &assignment(vec![], 0), 0, 4096)
        .await
        .unwrap();
    assert!(!output.was_called());
    assert!(output.card_score.is_none());
    assert_eq!(model.0.load(Ordering::SeqCst), 0);
    let a = assignment(
        vec![CorpusItem {
            classifier_world: None,
            id: 1,
            title: String::new(),
            context: "Cedar won.".into(),
            source: "Wire".into(),
            published_at_epoch: Some(0),
        }],
        0,
    );
    assert!(journalist::create(&pool, &Studio::new(&model), &a, 0, 4096)
        .await
        .is_err());
    assert_eq!(model.0.load(Ordering::SeqCst), 0);
}
include!("../../../tests/internal/journalist/activity_tests.rs");
