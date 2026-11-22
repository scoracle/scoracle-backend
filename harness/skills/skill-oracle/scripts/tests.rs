use super::request::*;
use super::*;
use crate::harness::model::{GenerateOptions, GenerateResult, Inference};
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::Duration;

fn subject() -> Subject {
    Subject {
        database_record: None,
        entity_id: 7,
        entity_type: "team".into(),
        entity_name: "Northbridge FC".into(),
        sport: "FOOTBALL".into(),
    }
}

fn cards() -> Cards {
    Cards {
        narratives: vec![SynthNarrative {
            generated_at: None,
            source_article_ids: vec![],
            title: "A title challenge gathers".into(),
            body: "Northbridge FC have won three matches.".into(),
            impact: 75.0,
            trajectory: "heating_up".into(),
            source_count: 3,
            source_age_days: Some(1),
        }],
        rating: Some(SynthRating {
            generated_at: None,
            body: "Northbridge FC have a strong statistical profile.".into(),
            notability: 82,
            rating_trajectory: "rising".into(),
            rating_trajectory_label: "rising".into(),
        }),
        vibe: Some(SynthVibe {
            generated_at: None,
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
        options: &GenerateOptions,
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
            serde_json::json!({"sent": true, "prompt": prompt, "options": format!("{options:?}")}),
        ))
    }

    fn model(&self) -> &str {
        "test-model"
    }
    fn request_body(&self, _: &str, _: &GenerateOptions) -> serde_json::Value {
        panic!("provenance must use the request actually sent")
    }
}

async fn create_from_reference(
    studio: &Studio<'_>,
    assignment: &Assignment,
) -> Result<SigilOutput> {
    let cards = &assignment.cards;
    let metrics = if cards.readiness() == Readiness::Empty {
        None
    } else {
        let convergence = pillar_convergence(&build_pillar_divergence(
            cards.rating.as_ref(),
            cards.vibe.as_ref(),
            &cards.momentum,
        ));
        Some((
            crown_score(cards),
            convergence,
            compute_omen(convergence, &cards.momentum),
        ))
    };
    articulate(studio, assignment, metrics).await
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
        response: r#"{"reading":"Northbridge FC have gathered pace across the five readings.","headline":"Northbridge gathers pace"}"#,
        fail: false,
        calls: Mutex::new(Vec::new()),
    };
    let output = create_from_reference(&Studio::new(&model), &assignment(cards()))
        .await
        .unwrap();
    assert_eq!(output.score, Some(75));
    assert_eq!(output.omen, Some("ascendant"));
    assert_eq!(output.headline.as_deref(), Some("Northbridge gathers pace"));
    assert_eq!(output.convergence, Some(100));
    assert_eq!(
        output.provenance.input_hash.as_deref(),
        Some("prepared-oracle-hash")
    );
    assert_eq!(model.calls.lock().unwrap().len(), 1);
    let prompt: serde_json::Value = serde_json::from_str(&model.calls.lock().unwrap()[0]).unwrap();
    assert!(prompt.get("memories").is_none());
    assert_eq!(
        prompt["evidence"]["insider"]["body"],
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
    let empty = create_from_reference(&Studio::new(&model), &assignment(Cards::default()))
        .await
        .unwrap();
    assert!(!empty.was_called());
    assert!(model.calls.lock().unwrap().is_empty());
    let error = create_from_reference(&Studio::new(&model), &assignment(cards()))
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("model unavailable"));
    assert_eq!(model.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn empty_cards_and_sql_failure_never_call_the_model() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(50))
        .connect_lazy("postgres://localhost:1/unused")
        .unwrap();
    let model = FakeModel {
        response: "must not be called",
        fail: false,
        calls: Mutex::new(vec![]),
    };
    let empty = create(&pool, &Studio::new(&model), &assignment(Cards::default()))
        .await
        .unwrap();
    assert!(!empty.was_called());
    assert_eq!(
        (empty.score, empty.convergence, empty.omen),
        (None, None, None)
    );
    assert!(create(&pool, &Studio::new(&model), &assignment(cards()))
        .await
        .is_err());
    assert!(model.calls.lock().unwrap().is_empty());
}

pub(super) async fn created_crown(pool: &PgPool) -> SigilOutput {
    let model = FakeModel {
        response: r#"{"reading":"Northbridge FC have gathered pace across the five readings.","headline":"Northbridge gathers pace"}"#,
        fail: false,
        calls: Mutex::new(vec![]),
    };
    create(pool, &Studio::new(&model), &assignment(cards()))
        .await
        .unwrap()
}

include!("../../../tests/internal/oracle/tests.rs");

// Unchanged legacy formulas are SQL parity oracles, never a production fallback.
/// Deterministic cross-pillar direction comparison handed to the model as a decided fact.
#[derive(Clone, Debug, PartialEq)]
pub struct PillarComparison {
    pub label: String,
    pub agree: bool,
}

/// Reduce a value to a direction sign: `None` = not directional (skip the comparison).
fn trajectory_sign(key: &str) -> Option<i8> {
    match key {
        "rising" | "heating_up" => Some(1),
        "falling" | "cooling_off" => Some(-1),
        _ => None,
    }
}

fn sentiment_sign(sentiment: i32) -> Option<i8> {
    if sentiment >= 60 {
        Some(1)
    } else if sentiment <= 40 {
        Some(-1)
    } else {
        None
    }
}

fn sign_word(s: i8) -> &'static str {
    if s > 0 {
        "positive"
    } else {
        "negative"
    }
}

/// build_pillar_divergence emits one comparison per directional pillar pair that is actually
/// present. Neutral/steady/absent signals produce NO line (a steady lens neither agrees nor
/// disagrees — the system prompt's own convergence rule). Pure and deterministic; the card is
/// prompt-only and derives entirely from values already in the input hash, so it can never
/// trigger a regeneration by itself.
pub fn build_pillar_divergence(
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
) -> Vec<PillarComparison> {
    let mut out = Vec::new();

    // Momentum is the sole direction signal; the Oracle never reads the raw tracker.
    let vibe_sign = vibe.and_then(|v| sentiment_sign(v.sentiment));
    let mom_sign = mom.direction.as_deref().and_then(trajectory_sign);
    // Profile strength: the LEVEL sign (is this an elite or a weak profile), distinct from the
    // direction sign. The classic rails conflict the fixtures measure — "strong profile vs
    // sliding momentum and negative narrative" — is a LEVEL-vs-direction disagreement that
    // direction pairs alone cannot see. (Narrative heating_up/cooling_off is deliberately NOT
    // compared: it measures story intensity, not valence — a negative story heating up must
    // not read as "positive narrative".)
    let strength_sign = rating.and_then(|r| {
        if r.notability >= 70 {
            Some(1i8)
        } else if r.notability <= 35 {
            Some(-1i8)
        } else {
            None
        }
    });
    let strength_word = |s: i8| if s > 0 { "strong" } else { "weak" };
    let mut push = |label: String, a: i8, b: i8| {
        out.push(PillarComparison {
            label,
            agree: (i32::from(a) * i32::from(b)) > 0,
        });
    };

    if let (Some(v), Some(m)) = (vibe_sign, mom_sign) {
        push(
            format!("Vibe ({}) vs Momentum ({})", sign_word(v), sign_word(m)),
            v,
            m,
        );
    }
    if let (Some(s), Some(m)) = (strength_sign, mom_sign) {
        push(
            format!(
                "Profile strength ({}) vs Momentum ({})",
                strength_word(s),
                sign_word(m)
            ),
            s,
            m,
        );
    }
    if let (Some(s), Some(v)) = (strength_sign, vibe_sign) {
        push(
            format!(
                "Profile strength ({}) vs Vibe ({})",
                strength_word(s),
                sign_word(v)
            ),
            s,
            v,
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Deterministic omen and convergence: code decides, the model narrates.
// ---------------------------------------------------------------------------

fn direction_sign(key: &str) -> i32 {
    match key {
        "rising" => 1,
        "falling" => -1,
        _ => 0,
    }
}

/// pillar_convergence turns the deterministic pillar comparisons into a 1-100 agreement number —
/// a computed measurement, not a model opinion. `round(100·agree/total)` floored at 1; `None` when no directional pair
/// exists (a quiet spread has nothing to converge on). The floor matches the DB contract
/// (`sigil_synthesis_convergence_check`: NULL or 1-100) — an all-disagree spread rounds to 0,
/// which the check rejects and which carries no product meaning beyond 1 (anything ≤ 50 is
/// already a crossroads to `compute_omen`, faithfully preserving the panel's soft rule).
pub fn pillar_convergence(comparisons: &[PillarComparison]) -> Option<i32> {
    if comparisons.is_empty() {
        return None;
    }
    let agree = comparisons.iter().filter(|c| c.agree).count();
    Some((((agree as f64 / comparisons.len() as f64) * 100.0).round() as i32).max(1))
}

/// compute_omen decides the reading's direction deterministically:
/// - a split spread (convergence ≤ 50 — half or more of the directional pairs disagree) is a
///   `crossroads` regardless of net direction — the contested arc IS the story;
/// - otherwise Momentum decides alone: positive ⇒
///   `ascendant`, negative ⇒ `waning`, nothing directional ⇒ `steady`.
pub fn compute_omen(convergence: Option<i32>, mom: &SynthMomentum) -> &'static str {
    if let Some(c) = convergence {
        if c <= 50 {
            return "crossroads";
        }
    }
    let net = mom.direction.as_deref().map(direction_sign).unwrap_or(0);
    if net > 0 {
        "ascendant"
    } else if net < 0 {
        "waning"
    } else {
        "steady"
    }
}

fn crown_score(cards: &Cards) -> i32 {
    let mut signals = Vec::new();
    if let Some(rating) = &cards.rating {
        signals.push(rating.notability.clamp(1, 100));
    }
    if let Some(vibe) = &cards.vibe {
        signals.push(vibe.sentiment.clamp(1, 100));
    }
    if let Some(narrative) = cards
        .narratives
        .iter()
        .max_by(|a, b| a.impact.total_cmp(&b.impact))
    {
        signals.push(narrative.impact.round().clamp(1.0, 100.0) as i32);
    }
    if let Some(insider) = &cards.insider {
        signals.push(insider.score.clamp(1, 100));
    }
    if signals.is_empty() {
        50
    } else {
        (signals.iter().sum::<i32>() as f64 / signals.len() as f64).round() as i32
    }
}
