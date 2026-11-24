//! Parser, deterministic product fields and exact publication lifecycle checks.

use super::super::*;
use super::*;
use crate::harness::model::{GenerateOptions, GenerateResult, Inference};
use crate::harness::skills::analyst::parser::*;
use crate::harness::skills::analyst::request::*;
use crate::harness::Parser;
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::Duration;

#[test]
fn parses_momentum_reply() {
    let parsed = parse_momentum_reply(
        r#"{"blurb":"Form is rising while the mood is calm.","headline":"Kerr finds a spark"}"#,
    )
    .unwrap();
    assert_eq!(parsed.blurb, "Form is rising while the mood is calm.");
}

#[test]
fn rejects_non_current_momentum_shapes() {
    assert!(parse_momentum_reply("SCORE: 3\nREAD: The form is rising.").is_none());
    assert!(parse_momentum_reply("MOMENTUM: rising\nREAD: The form is rising.").is_none());
    assert!(parse_momentum_reply("MOMENTUM READ: The form is rising.").is_none());
    assert!(parse_momentum_reply("**READ:** The form is rising.").is_none());
    assert!(parse_momentum_reply("read: The form is rising.").is_none());
    assert!(parse_momentum_reply("").is_none());
}

#[test]
fn preserves_the_written_text_in_any_script() {
    // Verbatim class from the 2026-08-15 delegation to ministral-3:3b: an Arabic run
    // mid-word in card-facing prose. The reply is well-formed by the JSON contract, so
    // only a content check catches it — reject and let the retry re-roll.
    assert!(parse_momentum_reply(
        r#"{"blurb":"His playmaking has زمنed in Milwaukee.","headline":"Kerr finds a spark"}"#
    )
    .is_some());
    // Latin diacritics are names, not leaks.
    let ok = parse_momentum_reply(
        r#"{"blurb":"Éder Militão's form is falling, and the tape backs the drop.","headline":"Kerr finds a spark"}"#,
    )
    .expect("diacritics must pass");
    assert!(ok.blurb.contains("Militão"));
}

#[test]
fn prose_form_rejects_undeclared_fields() {
    assert!(parse_momentum_reply(
        r#"{"blurb":"The tape is steady.","headline":"Steady","extra":"Extra"}"#
    )
    .is_none());
}

#[test]
fn direction_is_decided_by_band_not_model() {
    // ±MOMENTUM_STEADY_BAND on the ±100-scale momentum_score; None (no durable
    // snapshot) is honestly steady.
    assert_eq!(momentum_direction_from_score(Some(10.0)), "rising");
    assert_eq!(momentum_direction_from_score(Some(9.9)), "steady");
    assert_eq!(momentum_direction_from_score(Some(-9.9)), "steady");
    assert_eq!(momentum_direction_from_score(Some(-10.0)), "falling");
    assert_eq!(momentum_direction_from_score(Some(0.0)), "steady");
    assert_eq!(momentum_direction_from_score(None), "steady");
}

#[test]
fn conviction_is_computed_not_asked_of_the_model() {
    // s11: the ±5 magnitude is derived from the SAME ±100 momentum_score that decides
    // direction, so the pair can never disagree — the failure class the old clamp existed
    // to paper over is now unrepresentable.
    assert_eq!(momentum_conviction_from_score(None), 0);
    assert_eq!(momentum_conviction_from_score(Some(0.0)), 0);
    assert_eq!(momentum_conviction_from_score(Some(4.9)), 0); // flat: under half the band

    // Inside the steady band a real lean still reads as ±1 (the old contract's steady range).
    assert_eq!(momentum_conviction_from_score(Some(5.0)), 1);
    assert_eq!(momentum_conviction_from_score(Some(-9.9)), -1);

    // At and beyond the band the ladder opens up — the range the model never used.
    assert_eq!(momentum_conviction_from_score(Some(10.0)), 1);
    assert_eq!(momentum_conviction_from_score(Some(20.0)), 2);
    assert_eq!(momentum_conviction_from_score(Some(35.0)), 3);
    assert_eq!(momentum_conviction_from_score(Some(55.0)), 4);
    assert_eq!(momentum_conviction_from_score(Some(80.0)), 5);
    assert_eq!(momentum_conviction_from_score(Some(100.0)), 5);
    assert_eq!(momentum_conviction_from_score(Some(-100.0)), -5);
}

#[test]
fn conviction_sign_always_agrees_with_the_decided_direction() {
    // The invariant the whole change buys: one source number, so one story. Sweep the scale.
    for tenths in -1000..=1000 {
        let s = f64::from(tenths) / 10.0;
        let dir = momentum_direction_from_score(Some(s));
        let conv = momentum_conviction_from_score(Some(s));
        match dir {
            "rising" => assert!((1..=5).contains(&conv), "rising got {conv} at score {s}"),
            "falling" => assert!((-5..=-1).contains(&conv), "falling got {conv} at score {s}"),
            _ => assert!((-1..=1).contains(&conv), "steady got {conv} at score {s}"),
        }
    }
}
#[test]
fn only_readings_or_measured_rails_are_material() {
    assert!(MomentumContext::new(2026, None, None, Snapshot::default()).empty());
    assert!(MomentumContext::new(
        2026,
        None,
        None,
        Snapshot {
            momentum_score: Some(1.4),
            ..Snapshot::default()
        },
    )
    .empty());
    assert!(!MomentumContext::new(
        2026,
        None,
        None,
        Snapshot {
            rating_slope: Some(1.4),
            rating_samples: 4,
            rating_window_start: Some("2026-10-01".into()),
            rating_window_end: Some("2026-10-02".into()),
            ..Snapshot::default()
        },
    )
    .empty());
}

/// The blanket digit ban is retired; the precise bookkeeping check replaces it (2026-08-24).
///
/// The old rule rejected any ASCII digit anywhere in the READ — 1,221 drops in three days, and
/// the thing that permanently dead-lettered momentum player 367 at five attempts. It had no test
/// asserting the rejection, so nothing caught its removal. This is that test, for the rule that
/// replaced it: digits in open prose are ordinary sporting evidence, internal numeric field citations are desk notes.
#[test]
fn momentum_preserves_digits_and_score_wording() {
    let read = |blurb: &str| {
        MomentumParser::Measured.parse(
            &serde_json::json!({"blurb":blurb,"headline":"The scores tell the story"}).to_string(),
        )
    };

    // Ships now. Under the old rule every one of these burned a finished READ.
    for ok in [
        "Three wins in a row and the room believes again.",
        "3 wins in a row and the room believes again.",
        "He is climbing (4th percentile) against a soft run.",
        "A 14-point climb over 11 samples, and the shape is holding.",
    ] {
        let got = read(ok)
            .expect("digits in prose never fail the card")
            .expect("a reply");
        assert_eq!(got.blurb, ok);
    }

    // Also publish: the desk notes pasted into a card.
    for bad in [
        "The slide is real (Mood: 30/100) and nobody is arguing.",
        "The room cools (sentiment=30).",
    ] {
        assert!(
            read(bad).is_ok(),
            "bookkeeping citation should publish: {bad}"
        );
    }

    // A parenthetical WITHOUT a digit is ordinary prose, not a citation.
    let aside = "The slide is real (and nobody is arguing) this week.";
    assert_eq!(read(aside).unwrap().unwrap().blurb, aside);
}

#[test]
fn claim_paragraphs_survive_the_production_parser() {
    let body = "The profile is ordinary. Most skills sit near average. The middle is the story.\n\nOne edge stands out. Finishing leads the supplied profile. That is the exception.\n\nAvailability is limited. Two absences are recorded. Depth matters now.\n\nThe rest is unchanged. The supplied comparison shows no movement. Continuity holds.";
    let raw = serde_json::json!({"blurb":body, "headline": "The recorded performance"}).to_string();
    let parsed = MomentumParser::Measured.parse(&raw).unwrap().unwrap();
    assert_eq!(parsed.blurb, body);
}

#[derive(Default)]
struct LifecycleAdapters {
    response: String,
    events: Mutex<Vec<&'static str>>,
}

#[async_trait]
impl Inference for LifecycleAdapters {
    async fn generate(
        &self,
        prompt: &str,
        _: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)> {
        self.events.lock().unwrap().push("model");
        Ok((
            GenerateResult {
                response: if self.response.is_empty() {
                    serde_json::json!({"blurb": "The measured profile is gaining ground. The mood is warming alongside it. Both supplied readings point upward.", "headline": "The recorded performance"}).to_string()
                } else {
                    self.response.clone()
                },
                thinking: String::new(),
                model: "responding-model".into(),
                total_duration: Duration::from_millis(12),
                eval_count: 7,
                prompt_eval_count: 11,
                completion_reason: Some("stop".into()),
                raw_response_body: "{}".into(),
            },
            serde_json::json!({"prompt": prompt}),
        ))
    }

    fn model(&self) -> &str {
        "configured-model"
    }

    fn request_body(&self, _: &str, _: &GenerateOptions) -> serde_json::Value {
        unreachable!("successful-call provenance uses the actual request")
    }
}

fn lifecycle_assignment(material: bool) -> Assignment {
    Assignment {
        entity_id: 7,
        entity_type: "team".into(),
        entity_name: "Test Team".into(),
        sport: "nba".into(),
        context: if material {
            MomentumContext::new(
                2026,
                Some(Form {
                    card_score: Some(68.0),
                    body: "The measured profile is gaining ground.".into(),
                    headline: Some("Test Team sharpens its form".into()),
                    season: Some(2026),
                    generated_at: Some("2026-09-20".into()),
                    input_hash: Some("scout-hash".into()),
                }),
                Some(Mood {
                    body: "The mood is warming.".into(),
                    headline: Some("Test Team lifts the room".into()),
                    sentiment: Some(61),
                    generated_at: Some("2026-09-20".into()),
                    input_hash: Some("influencer-hash".into()),
                }),
                Snapshot {
                    momentum_score: Some(25.0),
                    ..Snapshot::default()
                },
            )
        } else {
            MomentumContext::new(2026, None, None, Snapshot::default())
        },
        voice_num_ctx: 4096,
    }
}

/// Exact publication-contract acceptance against an isolated database containing migrations
/// 256-258. Ordinary test runs compile but ignore these cases; opt in with TEST_DATABASE_URL.
#[path = "../../../tests/internal/analyst/publish_tests.rs"]
mod postgres_publication_fencing_tests;
