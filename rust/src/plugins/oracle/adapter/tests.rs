//! Exact publication and five-pillar readiness tests for the Oracle application boundary.

use super::*;
use crate::application::queue::work::TaskKey;
use crate::plugins::oracle::cognition::SigilSynthesis;
use crate::studio::Generation;

fn crown(score: Option<i32>) -> SigilOutput {
    Generation::uncalled(
        SigilSynthesis {
            score,
            reading: score.map(|_| "Test Team stand where the five signs meet.".to_string()),
            season: 2026,
            input_components_json: if score.is_some() {
                r#"{"narrative_titles":["Test"]}"#.to_string()
            } else {
                "{}".to_string()
            },
            headline: score.map(|_| "Test Team meet the signs".to_string()),
            convergence: score.map(|_| 67),
            omen: score.map(|_| "crossroads"),
        },
        "test-oracle-model".to_string(),
        crate::plugins::oracle::cognition::ORACLE_PROMPT_VERSION,
        Vec::new(),
        score.map(|_| "oracle-input-hash".to_string()),
    )
}

#[test]
fn readiness_waits_on_five_pillars_and_never_on_sigil_itself() {
    let names: Vec<&str> = PILLAR_STAGES.iter().map(|stage| stage.as_str()).collect();
    assert_eq!(
        names,
        ["narratives", "rating", "vibe", "momentum", "transfers"]
    );
    assert!(!PILLAR_STAGES.contains(&crate::plugins::oracle::manifest::TASK));
}

mod postgres_oracle_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration;

    const SPORT: &str = "ZZ_ORACLE_FENCE";
    const ENTITY_ID: i64 = 9_300_005;

    async fn pool() -> PgPool {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("set TEST_DATABASE_URL to an isolated database with migrations through 260");
        PgPoolOptions::new()
            .max_connections(3)
            .connect(&url)
            .await
            .expect("connect TEST_DATABASE_URL")
    }

    async fn clean(pool: &PgPool) {
        sqlx::query("DELETE FROM sigil_synthesis WHERE sport = $1")
            .bind(SPORT)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM pipeline_work WHERE sport = $1")
            .bind(SPORT)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO sports (id, display_name, current_season) VALUES ($1,$2,2026) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(SPORT)
        .bind("Oracle fencing test")
        .execute(pool)
        .await
        .unwrap();
    }

    fn pending(stage: TaskKey, revision: &str) -> Item {
        Item {
            stage,
            entity_type: "team".to_string(),
            entity_id: ENTITY_ID,
            sport: SPORT.to_string(),
            input_version: Some(revision.to_string()),
            attempts: 0,
            claim_token: None,
        }
    }

    async fn claim_one(pool: &PgPool) -> Item {
        let mut claimed = work::claim(pool, crate::plugins::oracle::manifest::TASK, 1)
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);
        claimed.remove(0)
    }

    async fn counts(pool: &PgPool) -> (i64, i64) {
        let products = sqlx::query_scalar("SELECT count(*) FROM sigil_synthesis WHERE sport = $1")
            .bind(SPORT)
            .fetch_one(pool)
            .await
            .unwrap();
        let work = sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE sport = $1")
            .bind(SPORT)
            .fetch_one(pool)
            .await
            .unwrap();
        (products, work)
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn current_claim_commits_crown_and_exact_completion() {
        let pool = pool().await;
        clean(&pool).await;
        work::enqueue(
            &pool,
            &pending(crate::plugins::oracle::manifest::TASK, "current"),
        )
        .await
        .unwrap();
        let current = claim_one(&pool).await;
        let prepared = Prepared::Product {
            output: Box::new(crown(Some(74))),
            previous_score: Some(61),
        };
        assert_eq!(
            commit_claimed(&pool, &current, SPORT, &prepared)
                .await
                .unwrap()
                .0,
            PluginOutcome::Committed
        );
        assert_eq!(counts(&pool).await, (1, 0));
        type CrownRow = (
            Option<i16>,
            Option<i16>,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let row: CrownRow = sqlx::query_as(
            "SELECT score, previous_score, reading, model_version, input_hash \
               FROM sigil_synthesis WHERE sport = $1",
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, Some(74));
        assert_eq!(row.1, Some(61));
        assert!(row.2.as_deref().unwrap().contains("Test Team"));
        assert_eq!(row.3.as_deref(), Some("test-oracle-model"));
        assert_eq!(row.4.as_deref(), Some("oracle-input-hash"));
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn missing_cards_commit_one_marker_without_a_model_voice() {
        let pool = pool().await;
        clean(&pool).await;
        work::enqueue(
            &pool,
            &pending(crate::plugins::oracle::manifest::TASK, "missing"),
        )
        .await
        .unwrap();
        let current = claim_one(&pool).await;
        let prepared = Prepared::Product {
            output: Box::new(crown(None)),
            previous_score: None,
        };
        commit_claimed(&pool, &current, SPORT, &prepared)
            .await
            .unwrap();
        let row: (Option<i16>, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT score, reading, voice_model_version, input_hash \
               FROM sigil_synthesis WHERE sport = $1",
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row, (None, None, None, None));
        assert_eq!(counts(&pool).await, (1, 0));
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn revision_supersession_publishes_nothing_from_stale_oracle() {
        let pool = pool().await;
        clean(&pool).await;
        work::enqueue(
            &pool,
            &pending(crate::plugins::oracle::manifest::TASK, "v1"),
        )
        .await
        .unwrap();
        let stale = claim_one(&pool).await;
        work::enqueue(
            &pool,
            &pending(crate::plugins::oracle::manifest::TASK, "v2"),
        )
        .await
        .unwrap();
        let prepared = Prepared::Product {
            output: Box::new(crown(Some(45))),
            previous_score: None,
        };
        assert_eq!(
            commit_claimed(&pool, &stale, SPORT, &prepared)
                .await
                .unwrap(),
            (PluginOutcome::Superseded, None)
        );
        assert_eq!(counts(&pool).await, (0, 1));
        let current = claim_one(&pool).await;
        assert_eq!(current.input_version.as_deref(), Some("v2"));
        assert_eq!(
            commit_claimed(&pool, &current, SPORT, &prepared)
                .await
                .unwrap()
                .0,
            PluginOutcome::Committed
        );
        assert_eq!(counts(&pool).await, (1, 0));
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn same_revision_reclaim_gives_only_new_lease_publication_rights() {
        let pool = pool().await;
        clean(&pool).await;
        work::enqueue(
            &pool,
            &pending(crate::plugins::oracle::manifest::TASK, "same"),
        )
        .await
        .unwrap();
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
        let prepared = Prepared::Product {
            output: Box::new(crown(Some(68))),
            previous_score: None,
        };
        assert_eq!(
            commit_claimed(&pool, &stale, SPORT, &prepared)
                .await
                .unwrap(),
            (PluginOutcome::Superseded, None)
        );
        commit_claimed(&pool, &current, SPORT, &prepared)
            .await
            .unwrap();
        assert_eq!(counts(&pool).await, (1, 0));
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn unchanged_crown_completes_without_another_product_or_followup() {
        let pool = pool().await;
        clean(&pool).await;
        work::enqueue(
            &pool,
            &pending(crate::plugins::oracle::manifest::TASK, "same-input"),
        )
        .await
        .unwrap();
        let current = claim_one(&pool).await;
        assert_eq!(
            commit_claimed(&pool, &current, SPORT, &Prepared::Debounced)
                .await
                .unwrap(),
            (PluginOutcome::Committed, None)
        );
        assert_eq!(counts(&pool).await, (0, 0));
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn readiness_preserves_pending_retryable_and_terminal_missing_policy() {
        let pool = pool().await;
        clean(&pool).await;
        assert!(super::pillars_settled(&pool, "team", ENTITY_ID, SPORT)
            .await
            .unwrap());

        work::enqueue(
            &pool,
            &pending(crate::plugins::scout::manifest::TASK, "pending"),
        )
        .await
        .unwrap();
        assert!(!super::pillars_settled(&pool, "team", ENTITY_ID, SPORT)
            .await
            .unwrap());

        sqlx::query(
            "UPDATE pipeline_work SET status='failed', attempts=1, \
             available_at=NOW() + INTERVAL '30 seconds' WHERE sport=$1 AND stage='rating'",
        )
        .bind(SPORT)
        .execute(&pool)
        .await
        .unwrap();
        assert!(super::pillars_settled(&pool, "team", ENTITY_ID, SPORT)
            .await
            .unwrap());

        sqlx::query("UPDATE pipeline_work SET attempts=$2 WHERE sport=$1 AND stage='rating'")
            .bind(SPORT)
            .bind(work::MAX_ATTEMPTS)
            .execute(&pool)
            .await
            .unwrap();
        assert!(super::pillars_settled(&pool, "team", ENTITY_ID, SPORT)
            .await
            .unwrap());
        clean(&pool).await;
    }
}
