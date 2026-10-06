use super::prompt::*;
use super::*;
use crate::studio::model::{GenerateOptions, GenerateResult, Inference};
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::Duration;

fn subject() -> Subject {
    Subject {
        entity_id: 7,
        entity_type: "team".into(),
        entity_name: "Northbridge FC".into(),
        sport: "FOOTBALL".into(),
    }
}

fn cards() -> Cards {
    Cards {
        narratives: vec![SynthNarrative {
            title: "A title challenge gathers".into(),
            body: "Northbridge FC have won three matches.".into(),
            impact: 75.0,
            trajectory: "heating_up".into(),
            source_count: 3,
            source_age_days: Some(1),
        }],
        rating: Some(SynthRating {
            body: "Northbridge FC have a strong statistical profile.".into(),
            notability: 82,
            rating_trajectory: "rising".into(),
            rating_trajectory_label: "rising".into(),
        }),
        vibe: Some(SynthVibe {
            sentiment: 72,
            prompt: "Belief is strengthening around Northbridge FC.".into(),
        }),
        momentum: SynthMomentum {
            direction: Some("rising".into()),
            blurb: Some("Northbridge FC are gathering pace.".into()),
            momentum_score: Some(4.0),
            ..Default::default()
        },
        insider: Some(SynthInsider {
            body: "A source reports Northbridge FC are pursuing Vale.".into(),
            score: 70,
            generated_at: Some("2026-10-02".into()),
        }),
    }
}

fn assignment(cards: Cards) -> Assignment {
    let input_components_json = build_synthesis_input_components(&cards);
    Assignment {
        subject: subject(),
        season: 2026,
        cards,
        input_components_json,
        input_hash: "prepared-oracle-hash".into(),
        options: generation_options(0.0, 4096),
    }
}

struct FakeModel {
    response: &'static str,
    fail: bool,
    calls: Mutex<Vec<String>>,
}

#[async_trait]
impl Inference for FakeModel {
    async fn generate(
        &self,
        prompt: &str,
        _: &GenerateOptions,
    ) -> anyhow::Result<(GenerateResult, serde_json::Value)> {
        self.calls.lock().unwrap().push(prompt.to_owned());
        if self.fail {
            anyhow::bail!("model unavailable");
        }
        Ok((
            GenerateResult {
                response: self.response.into(),
                thinking: String::new(),
                model: "test-model".into(),
                total_duration: Duration::from_millis(1),
                prompt_eval_count: 30,
                eval_count: 18,
                completion_reason: Some("stop".into()),
                raw_response_body: "{}".into(),
            },
            serde_json::json!({"sent": true}),
        ))
    }

    fn model(&self) -> &str {
        "test-model"
    }
    fn request_body(&self, _: &str, _: &GenerateOptions) -> serde_json::Value {
        panic!("provenance must use the request actually sent")
    }
}

#[test]
fn five_cards_are_complete_and_each_changes_the_hash() {
    let all = cards();
    assert_eq!(all.readiness(), Readiness::Complete);
    let first = build_synthesis_input_components(&all);
    let mut changed = all;
    changed
        .insider
        .as_mut()
        .unwrap()
        .body
        .push_str(" Further talks followed.");
    assert_ne!(first, build_synthesis_input_components(&changed));
    changed.insider = None;
    assert_eq!(
        changed.readiness(),
        Readiness::Partial {
            missing: vec![Pillar::Insider]
        }
    );
    assert_eq!(Cards::default().readiness(), Readiness::Empty);
}

#[test]
fn convergence_and_omen_follow_directional_cards() {
    let all = cards();
    let pairs = build_pillar_divergence(all.rating.as_ref(), all.vibe.as_ref(), &all.momentum);
    assert_eq!(pillar_convergence(&pairs), Some(100));
    assert_eq!(compute_omen(Some(100), &all.momentum), "ascendant");
    assert_eq!(compute_omen(Some(50), &all.momentum), "crossroads");
}

#[test]
fn score_is_computed_without_a_model_score() {
    assert_eq!(crown_score(&cards()), 75);
}

#[tokio::test]
async fn finished_cards_get_one_model_call_and_no_memory() {
    let model = FakeModel {
        response: r#"{"reading":"Northbridge FC have gathered pace across the five readings."}"#,
        fail: false,
        calls: Mutex::new(Vec::new()),
    };
    let output = create(&Studio::new(&model), &assignment(cards()))
        .await
        .unwrap();
    assert_eq!(output.score, Some(75));
    assert_eq!(output.omen, Some("ascendant"));
    assert_eq!(output.convergence, Some(100));
    assert_eq!(
        output.provenance.input_hash.as_deref(),
        Some("prepared-oracle-hash")
    );
    assert_eq!(model.calls.lock().unwrap().len(), 1);
    let prompt: serde_json::Value = serde_json::from_str(&model.calls.lock().unwrap()[0]).unwrap();
    assert!(prompt.get("memories").is_none());
    assert_eq!(
        prompt["fresh"]["insider"]["body"],
        "A source reports Northbridge FC are pursuing Vale."
    );
}

#[tokio::test]
async fn empty_cards_skip_the_model_but_model_failure_does_not() {
    let model = FakeModel {
        response: "",
        fail: true,
        calls: Mutex::new(Vec::new()),
    };
    let empty = create(&Studio::new(&model), &assignment(Cards::default()))
        .await
        .unwrap();
    assert!(!empty.was_called());
    assert!(model.calls.lock().unwrap().is_empty());
    let error = create(&Studio::new(&model), &assignment(cards()))
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("model unavailable"));
    assert_eq!(model.calls.lock().unwrap().len(), 1);
}
