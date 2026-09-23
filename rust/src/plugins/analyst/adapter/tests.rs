//! Unit and lifecycle tests for the Analyst application adapter.
//!
//! Split out of `mod.rs` so the stage module reads as the stage and nothing else.
//! `super` resolves to the application adapter while Studio creation remains independently tested.

use super::*;
use crate::application::queue::work::Stage;
use crate::plugins::analyst::cognition::{
    momentum_conviction_from_score, momentum_direction_from_score, parse_momentum_reply,
    MomentumParser, MOMENTUM_PROMPT_VERSION,
};
use crate::studio::model::{GenerateOptions, GenerateResult, Inference};
use crate::studio::Parser;
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::Duration;

#[test]
fn parses_momentum_reply() {
    let parsed = parse_momentum_reply("READ: Form is rising while the mood is calm.").unwrap();
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
fn foreign_script_leak_fails_closed_the_3b_delegation_glitch() {
    // Verbatim class from the 2026-08-15 delegation to ministral-3:3b: an Arabic run
    // mid-word in card-facing prose. The reply is well-formed by the label contract, so
    // only a content check catches it — reject and let the retry re-roll.
    assert!(parse_momentum_reply("READ: His playmaking has زمنed in Milwaukee.").is_none());
    // Latin diacritics are names, not leaks.
    let ok =
        parse_momentum_reply("READ: Éder Militão's form is falling, and the tape backs the drop.")
            .expect("diacritics must pass");
    assert!(ok.blurb.contains("Militão"));
}

#[test]
fn parses_the_s17_headline_line() {
    // s17 (mig 226): HEADLINE after the READ is the contracted position.
    let parsed = parse_momentum_reply(
        "READ: The form is rising and the mood confirms it.\nHEADLINE: Form and mood rise together for Vale",
    )
    .expect("a contracted headline must parse");
    assert_eq!(
        parsed.headline.as_deref(),
        Some("Form and mood rise together for Vale")
    );
    assert_eq!(parsed.blurb, "The form is rising and the mood confirms it.");

    // A missing title does not cost a valid read.
    let bare = parse_momentum_reply("READ: The tape is steady.").unwrap();
    assert!(bare.headline.is_none());

    assert!(parse_momentum_reply(
        "HEADLINE: Kerr holds the line\nREAD: The form is holding while the mood wobbles."
    )
    .is_none());
    assert!(parse_momentum_reply(
        "READ: First sentence.\nHEADLINE: The title\nSome trailing note."
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
fn prompt_carries_a_study_without_predeclaring_the_verdict() {
    let mom = SynthMomentum {
        rating_slope: Some(50.7),
        rating_samples: 4,
        momentum_score: Some(50.7),
        ..SynthMomentum::default()
    };
    let prompt = build_momentum_prompt_from_pillars(
        "player",
        "Test Player",
        "FOOTBALL",
        None,
        None,
        &mom,
        None,
    );
    assert!(prompt.contains("=== DATED TRAJECTORY STUDY ==="));
    assert!(prompt.contains("Statistical form: rose by 50.7 points"));
    assert!(prompt.contains("across 4 rated games"));
    assert!(!prompt.contains("decided upstream"));
    assert!(!prompt.contains("final"));
    // A missing rail is named as unmeasured, never silently converted to flat.
    let empty = build_momentum_prompt_from_pillars(
        "player",
        "Test Player",
        "FOOTBALL",
        None,
        None,
        &SynthMomentum::default(),
        None,
    );
    assert!(empty.contains("Statistical form: not measured."));
    assert!(empty.contains("Reported mood: not measured."));
    assert!(empty.contains("Missing means unmeasured, not flat"));
}

#[test]
fn finished_character_readings_and_one_study_reach_the_prompt() {
    let mom = SynthMomentum {
        rating_slope: Some(-33.7),
        rating_samples: 4,
        vibe_slope: Some(14.0),
        vibe_samples: 11,
        momentum_score: Some(-22.4),
        ..SynthMomentum::default()
    };
    let p = build_momentum_prompt_from_pillars(
        "team",
        "Test Team",
        "FOOTBALL",
        Some(&a_rating()),
        Some(&a_vibe()),
        &mom,
        None,
    );

    assert!(p.contains("=== SCOUT READING ==="));
    assert!(p.contains("Chances created have held their line"));
    assert!(p.contains("=== INFLUENCER READING ==="));
    assert!(p.contains("The room is warm after the cup run"));
    assert!(p.contains("Statistical form: fell by 33.7 points"));
    assert!(p.contains("Reported mood: rose by 14.0 points"));
    assert!(p.contains("over an unavailable date window"));
    assert!(!p.contains("Direction (decided upstream"));
}

#[test]
fn input_components_are_stable_and_sorted() {
    let rating = SynthRating {
        body: "body".to_string(),
        notability: 88,
        rating_trajectory: "rising".to_string(),
        rating_trajectory_label: "Composite rising".to_string(),
    };
    let vibe = SynthVibe {
        sentiment: 62,
        prompt: "Coverage is warmer".to_string(),
    };
    let mom = SynthMomentum {
        rating_slope: Some(1.24),
        rating_samples: 6,
        vibe_slope: Some(-0.04),
        vibe_samples: 4,
        momentum_score: Some(1.19),
        ..SynthMomentum::default()
    };
    let components = build_momentum_input_components_from_pillars(Some(&rating), Some(&vibe), &mom);
    let value: serde_json::Value = serde_json::from_str(&components).unwrap();
    assert_eq!(value["prompt_version"], MOMENTUM_PROMPT_VERSION);
    assert_eq!(value["scout_body"], "body");
    assert_eq!(value["influencer_body"], "Coverage is warmer");
    assert_eq!(value["influencer_sentiment"], 62);
    assert_eq!(value["momentum_rating_slope"], 1.2);
    assert_eq!(value["momentum_vibe_slope"], -0.0);
}

// ── PARTIAL SPREADS ─────────────────────────────────────────────────────────────────────
// The doctrine (Scott, 2026-08-15): "If the Analyst receives no info, it won't have an
// output, but gracefully skip. If it only has vibe instead of rating, then it will build an
// output on that. Work with what we have, don't fabricate, not having something to say is an
// acceptable answer."
//
// The seat ALREADY does exactly this, and that is the problem these tests fix: the behaviour
// rested entirely on `MomentumContext::empty()` being `&&` rather than `||`, and nothing
// asserted it. Every one of the ten momentum fixtures carries BOTH rails, so flipping that
// operator would have deleted the vibe-only read — the whole "build an output on that" half
// of the brief — while the gate stayed green. A rule measured by nothing is advice (or8).
//
// This is the NORMAL path, not an edge case: the DB grows by fetch-and-upsert with no
// bootstrap, so entities arrive with nothing and fill in over weeks.

fn ctx(
    rating: Option<SynthRating>,
    vibe: Option<SynthVibe>,
    snap: SynthMomentum,
) -> MomentumContext {
    MomentumContext {
        season: 2025,
        rating: rating.as_ref().map(form),
        vibe: vibe.as_ref().map(mood),
        snapshot: snapshot(&snap),
        input_components_json: String::new(),
        input_hash: String::new(),
    }
}

fn a_rating() -> SynthRating {
    SynthRating {
        body: "Chances created have held their line.".to_string(),
        notability: 71,
        rating_trajectory: "steady".to_string(),
        rating_trajectory_label: "overall scores holding steady over recent games".to_string(),
    }
}

fn a_vibe() -> SynthVibe {
    SynthVibe {
        sentiment: 64,
        prompt: "The room is warm after the cup run.".to_string(),
    }
}

#[test]
fn only_a_totally_empty_context_is_empty_the_load_bearing_and() {
    // All three absent — the graceful-skip case. The handler returns Ok(()) and writes no
    // row: silence is a valid output, not a failure.
    assert!(ctx(None, None, SynthMomentum::default()).empty());

    // ...and every PARTIAL spread is NOT empty, so the seat proceeds and reads on whatever
    // survived. These four are the assertions that pin `&&`: under `||` all of them flip to
    // `empty()` == true and the seat would fall silent on entities it can genuinely read.
    assert!(
        !ctx(None, Some(a_vibe()), SynthMomentum::default()).empty(),
        "vibe with no rating must still produce a read — the brief's explicit case"
    );
    assert!(
        !ctx(Some(a_rating()), None, SynthMomentum::default()).empty(),
        "rating with no vibe must still produce a read"
    );
    let snap_only = SynthMomentum {
        momentum_score: Some(1.4),
        ..SynthMomentum::default()
    };
    assert!(
        !ctx(None, None, snap_only).empty(),
        "a trajectory snapshot alone is material enough to read"
    );
    assert!(!ctx(Some(a_rating()), Some(a_vibe()), SynthMomentum::default()).empty());
}

#[test]
fn a_vibe_only_context_builds_a_prompt_that_claims_no_form() {
    // A partial spread still supplies the finished card that exists, while saying explicitly
    // that the absent rail and its change are unmeasured.
    let p = build_momentum_prompt_from_pillars(
        "team",
        "Ipswich Town",
        "FOOTBALL",
        None,
        Some(&a_vibe()),
        &SynthMomentum::default(),
        None,
    );
    assert!(p.contains("=== SCOUT READING ===\nNot available."));
    assert!(p.contains("The room is warm after the cup run"));
    assert!(p.contains("Statistical form: not measured."));
    assert!(p.contains("Reported mood: not measured."));
}

/// The blanket digit ban is retired; the precise bookkeeping check replaces it (2026-08-24).
///
/// The old rule rejected any ASCII digit anywhere in the READ — 1,221 drops in three days, and
/// the thing that permanently dead-lettered momentum player 367 at five attempts. It had no test
/// asserting the rejection, so nothing caught its removal. This is that test, for the rule that
/// replaced it: digits in open prose are ordinary sporting evidence, internal numeric field citations are desk notes.
#[test]
fn momentum_allows_digits_in_prose_but_never_a_bookkeeping_citation() {
    let read = |blurb: &str| MomentumParser.parse(&format!("READ: {blurb}"));

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

    // Still rejected: the desk notes pasted into a card.
    for bad in [
        "The slide is real (Mood: 30/100) and nobody is arguing.",
        "The room cools (sentiment=30).",
    ] {
        assert!(
            read(bad).is_err(),
            "bookkeeping citation must still fail: {bad}"
        );
    }

    // A parenthetical WITHOUT a digit is ordinary prose, not a citation.
    let aside = "The slide is real (and nobody is arguing) this week.";
    assert_eq!(read(aside).unwrap().unwrap().blurb, aside);
}

#[test]
fn claim_paragraphs_survive_the_production_parser() {
    let body = "The profile is ordinary. Most skills sit near average. The middle is the story.\n\nOne edge stands out. Finishing leads the supplied profile. That is the exception.\n\nAvailability is limited. Two absences are recorded. Depth matters now.\n\nThe rest is unchanged. The supplied comparison shows no movement. Continuity holds.";
    let raw = format!("READ: {body}\nHEADLINE: Ordinary form holds");
    let parsed = MomentumParser.parse(&raw).unwrap().unwrap();
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
                response: self.response.clone(),
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
        entity_type: "team".into(),
        entity_name: "Test Team".into(),
        sport: "nba".into(),
        context: if material {
            MomentumContext::new(
                2026,
                Some(Form {
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
        memory: material.then(|| "Prepared source memory.".into()),
        voice_num_ctx: 4096,
    }
}

/// Exact publication-contract acceptance against an isolated database containing migrations
/// 256-258. Ordinary test runs compile but ignore these cases; opt in with TEST_DATABASE_URL.
mod postgres_publication_fencing_tests {
    use super::*;
    use crate::application::queue::publication::ClaimPublication;
    use crate::application::queue::work;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use std::time::Duration;

    const SPORT: &str = "ZZ_MOMENTUM_FENCE";
    const ENTITY_ID: i64 = 9_200_002;

    async fn pool() -> PgPool {
        let url = std::env::var("TEST_DATABASE_URL").expect(
            "set TEST_DATABASE_URL to an isolated database with migrations 256-258 applied",
        );
        PgPoolOptions::new()
            .max_connections(3)
            .connect(&url)
            .await
            .expect("connect TEST_DATABASE_URL")
    }

    async fn clean(pool: &PgPool) {
        sqlx::query("DELETE FROM application_outbox WHERE sport = $1")
            .bind(SPORT)
            .execute(pool)
            .await
            .expect("clean outbox");
        sqlx::query("DELETE FROM momentum_summaries WHERE sport = $1")
            .bind(SPORT)
            .execute(pool)
            .await
            .expect("clean momentum products");
        sqlx::query("DELETE FROM pipeline_work WHERE sport = $1")
            .bind(SPORT)
            .execute(pool)
            .await
            .expect("clean work");
        sqlx::query(
            "INSERT INTO sports (id, display_name, current_season) VALUES ($1,$2,2026) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(SPORT)
        .bind("Momentum fencing test")
        .execute(pool)
        .await
        .expect("ensure test sport");
    }

    fn pending(revision: &str) -> Item {
        Item {
            stage: Stage::Momentum,
            entity_type: "team".to_string(),
            entity_id: ENTITY_ID,
            sport: SPORT.to_string(),
            input_version: Some(revision.to_string()),
            attempts: 0,
            claim_token: None,
        }
    }

    async fn claim_one(pool: &PgPool) -> Item {
        let mut claimed = work::claim(pool, Stage::Momentum, 1)
            .await
            .expect("claim momentum test row");
        assert_eq!(claimed.len(), 1);
        claimed.remove(0)
    }

    async fn product() -> Prepared {
        let adapters = LifecycleAdapters {
            response: r#"{"body":"The form is rising and the mood confirms it.","headline":"Test Team gathers force"}"#.into(),
            ..Default::default()
        };
        prepare(&Studio::new(&adapters), &lifecycle_assignment(true))
            .await
            .expect("prepare momentum product")
    }

    async fn counts(pool: &PgPool) -> (i64, i64, i64) {
        let products =
            sqlx::query_scalar("SELECT count(*) FROM momentum_summaries WHERE sport = $1")
                .bind(SPORT)
                .fetch_one(pool)
                .await
                .unwrap();
        let events = sqlx::query_scalar("SELECT count(*) FROM application_outbox WHERE sport = $1")
            .bind(SPORT)
            .fetch_one(pool)
            .await
            .unwrap();
        let work = sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE sport = $1")
            .bind(SPORT)
            .fetch_one(pool)
            .await
            .unwrap();
        (products, events, work)
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn revision_arriving_during_execution_fences_publication() {
        let pool = pool().await;
        clean(&pool).await;

        work::enqueue(&pool, &pending("v1")).await.unwrap();
        let stale = claim_one(&pool).await;
        work::enqueue(&pool, &pending("v2")).await.unwrap();

        assert_eq!(
            commit_claimed(&pool, &stale, SPORT, &product().await)
                .await
                .unwrap(),
            (PluginOutcome::Superseded, None)
        );
        assert_eq!(counts(&pool).await, (0, 0, 1));

        let current = claim_one(&pool).await;
        assert_eq!(current.input_version.as_deref(), Some("v2"));
        assert_eq!(
            commit_claimed(&pool, &current, SPORT, &product().await)
                .await
                .unwrap()
                .0,
            PluginOutcome::Committed
        );
        assert_eq!(counts(&pool).await, (1, 1, 0));
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn reclaimed_same_revision_fences_the_old_worker() {
        let pool = pool().await;
        clean(&pool).await;

        work::enqueue(&pool, &pending("same")).await.unwrap();
        let stale = claim_one(&pool).await;
        sqlx::query(
            "UPDATE pipeline_work SET updated_at = NOW() - INTERVAL '1 hour' WHERE sport = $1",
        )
        .bind(SPORT)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            work::requeue_stale(&pool, Duration::from_secs(30 * 60))
                .await
                .unwrap(),
            1
        );
        let current = claim_one(&pool).await;
        assert_ne!(stale.claim_token, current.claim_token);

        assert_eq!(
            commit_claimed(&pool, &stale, SPORT, &product().await)
                .await
                .unwrap(),
            (PluginOutcome::Superseded, None)
        );
        assert_eq!(counts(&pool).await, (0, 0, 1));
        assert_eq!(
            commit_claimed(&pool, &current, SPORT, &product().await)
                .await
                .unwrap()
                .0,
            PluginOutcome::Committed
        );
        assert_eq!(counts(&pool).await, (1, 1, 0));
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn current_claim_commits_product_provenance_event_and_completion() {
        let pool = pool().await;
        clean(&pool).await;

        work::enqueue(&pool, &pending("current")).await.unwrap();
        let current = claim_one(&pool).await;
        let prepared = product().await;
        let expected_hash = match &prepared {
            Prepared::Product(output) => output.provenance.input_hash.clone(),
            Prepared::NoMaterial => panic!("expected product"),
        };
        let (outcome, row_id) = commit_claimed(&pool, &current, SPORT, &prepared)
            .await
            .unwrap();
        assert_eq!(outcome, PluginOutcome::Committed);
        assert!(row_id.is_some());
        assert_eq!(counts(&pool).await, (1, 1, 0));

        let row: (String, i16, String, String, Option<String>) = sqlx::query_as(
            "SELECT direction, score, model_version, prompt_version, input_hash \
             FROM momentum_summaries WHERE sport = $1",
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, "rising");
        assert_eq!(row.1, 2);
        assert_eq!(row.2, "responding-model");
        assert_eq!(row.3, MOMENTUM_PROMPT_VERSION);
        assert_eq!(row.4, expected_hash);
        let event: (String, String, Option<String>) = sqlx::query_as(
            "SELECT kind, source_stage, source_input_version \
             FROM application_outbox WHERE sport = $1",
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            event,
            (
                "momentum_completed".into(),
                "momentum".into(),
                Some("current".into())
            )
        );
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn no_material_claim_completes_with_followup_and_without_product() {
        let pool = pool().await;
        clean(&pool).await;

        work::enqueue(&pool, &pending("empty")).await.unwrap();
        let current = claim_one(&pool).await;
        assert_eq!(
            commit_claimed(&pool, &current, SPORT, &Prepared::NoMaterial)
                .await
                .unwrap(),
            (PluginOutcome::Committed, None)
        );
        assert_eq!(counts(&pool).await, (0, 1, 0));
        let kind: String =
            sqlx::query_scalar("SELECT kind FROM application_outbox WHERE sport = $1")
                .bind(SPORT)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(kind, "momentum_completed");
        clean(&pool).await;
    }
    // A separate OS process exits without running destructors at each boundary.
    // The parent has no LISTEN connection: recovery must follow durable rows.
    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
    async fn process_crash_publication_rehearsal() {
        const CHILD: &str = "SCORACLE_ANALYST_CRASH_PHASE";
        if let Ok(phase) = std::env::var(CHILD) {
            let pool = pool().await;
            let current = claim_one(&pool).await;
            let prepared = product().await;
            if phase == "before-commit" {
                let mut publication = ClaimPublication::begin(&pool, &current)
                    .await
                    .unwrap()
                    .expect("current claim");
                if let Prepared::Product(output) = &prepared {
                    persist_momentum_summary(publication.transaction(), &current, SPORT, output)
                        .await
                        .unwrap();
                }
                crate::plugins::analyst::adapter::record_momentum_completed(
                    publication.transaction(),
                    &current,
                )
                .await
                .unwrap();
                std::process::exit(86);
            }
            assert_eq!(
                commit_claimed(&pool, &current, SPORT, &prepared)
                    .await
                    .unwrap()
                    .0,
                PluginOutcome::Committed
            );
            std::process::exit(86);
        }
        let pool = pool().await;
        clean(&pool).await;
        work::enqueue(&pool, &pending("crash")).await.unwrap();
        let crash = |phase: &str| {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "plugins::analyst::adapter::tests::postgres_publication_fencing_tests::process_crash_publication_rehearsal", "--ignored", "--nocapture"])
                .env(CHILD, phase).status().unwrap();
            assert_eq!(status.code(), Some(86));
        };
        crash("before-commit");
        assert_eq!(counts(&pool).await, (0, 0, 1));
        sqlx::query("UPDATE pipeline_work SET updated_at=NOW()-INTERVAL '1 hour' WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            work::requeue_stale(&pool, Duration::from_secs(1800))
                .await
                .unwrap(),
            1
        );
        crash("after-commit");
        assert_eq!(counts(&pool).await, (1, 1, 0));
        pool.close().await;
        // Fresh connection pool is the restarted recovery owner. No notification
        // was observed and no model/runtime constructor is available here.
        let pool = self::pool().await;
        let reactions = crate::application::plugins::build_reactions(pool.clone()).unwrap();
        assert_eq!(
            crate::application::queue::outbox::drain(&pool, &reactions, 1)
                .await
                .unwrap(),
            1
        );
        assert_eq!(counts(&pool).await, (1, 0, 1));
        assert_eq!(
            crate::application::queue::outbox::drain(&pool, &reactions, 1)
                .await
                .unwrap(),
            0
        );
        clean(&pool).await;
    }
}
