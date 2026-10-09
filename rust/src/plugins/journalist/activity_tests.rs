use super::*;
use crate::harness::{
    model::{GenerateOptions, GenerateResult, Inference},
    Studio,
};
use crate::plugins::journalist::{
    self,
    prompt::{self, Assignment},
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
                response: r#"{"report_1":"Wire reports Cedar won."}"#.into(),
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
    prompt::prepare(
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
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; pure read-only query, no schema needed"]
async fn sql_activity_matches_legacy_formula_and_served_product() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL"))
        .await
        .unwrap();
    let now = 1_790_467_200;
    let publishers = [
        "Wire", "WIRE", " Wire ", "ΣΟΣ", "σος", "İ", "i\u{307}", "Other",
    ];
    let epochs = [
        None,
        Some(now + 1),
        Some(now),
        Some(now - 43200),
        Some(now - 43201),
        Some(now - 86400),
        Some(now - 86401),
        Some(now - 172800),
        Some(now - 172801),
        Some(i64::MIN),
        Some(i64::MAX),
    ];
    let mut cases = 0;
    for count in 0..=32 {
        for epoch in epochs {
            let corpus = (0..count)
                .map(|i| CorpusItem {
                    classifier_world: None,
                    id: i + 1,
                    title: String::new(),
                    context: format!("Report {i}."),
                    source: publishers[i as usize % publishers.len()].into(),
                    published_at_epoch: epoch,
                })
                .collect::<Vec<_>>();
            for clock in [now, i64::MIN, i64::MAX] {
                let actual = load(&pool, &corpus, clock).await.unwrap();
                let expected = reference(&corpus, clock);
                assert_eq!(
                    actual.card_score, expected.card_score,
                    "count={count} epoch={epoch:?} clock={clock}"
                );
                assert_eq!(actual.reports.len(), expected.reports.len());
                for (actual, expected) in actual.reports.iter().zip(expected.reports) {
                    assert_eq!(actual.score, expected.score);
                    assert_eq!(actual.components, expected.components);
                }
                cases += 1;
            }
        }
    }
    // Mixed epochs exercise max(non-null), rather than the first selected report.
    let corpus = epochs
        .iter()
        .enumerate()
        .map(|(i, &epoch)| CorpusItem {
            classifier_world: None,
            id: i as i64 + 1,
            title: String::new(),
            context: format!("Report {i}."),
            source: publishers[i % publishers.len()].into(),
            published_at_epoch: epoch,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        load(&pool, &corpus, now).await.unwrap().card_score,
        reference(&corpus, now).card_score
    );
    let a = assignment(
        vec![CorpusItem {
            classifier_world: None,
            id: 1,
            title: "Cedar result".into(),
            context: "Cedar won.".into(),
            source: "Wire".into(),
            published_at_epoch: Some(now - 3600),
        }],
        now,
    );
    let model = Model(AtomicUsize::new(0));
    let output = journalist::create(&pool, &Studio::new(&model), &a, now, 4096)
        .await
        .unwrap();
    assert_eq!(output.card_score, Some(32));
    assert_eq!(output.narratives[0].impact, 32);
    assert_eq!(
        output.narratives[0].impact_components,
        reference(&a.selected, now).reports[0].components
    );
    assert_eq!(
        output.provenance.input_hash.as_deref(),
        Some(a.input_hash.as_str())
    );
    assert_eq!(model.0.load(Ordering::SeqCst), 1);
    eprintln!("{cases} SQL/legacy parity cases, mixed epochs and one served product verified");
}
