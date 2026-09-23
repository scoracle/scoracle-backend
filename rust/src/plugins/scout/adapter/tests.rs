//! Unit and exact-publication tests for the Scout application adapter.

use super::*;
use crate::plugins::scout::cognition::{RatingExclusions, RatingProduct, RATING_PROMPT_VERSION};
use crate::studio::Generation;
fn rating_product(
    skipped_no_stats: bool,
    skipped_unchanged: bool,
    body: Option<&str>,
) -> RatingOutput {
    Generation::uncalled(
        RatingProduct {
            season: 2026,
            skipped_no_stats,
            abstained: false,
            skipped_unchanged,
            body: body.map(str::to_string),
            headline: body.map(|_| "Test Team owns the middle".to_string()),
            notability: body.map(|_| 74),
            notability_components: serde_json::json!({"spread": 74}),
            rating_trajectory: body.map(|_| "rising".to_string()),
            rating_trajectory_label: body.map(|_| "Gaining ground".to_string()),
            rating_trajectory_components: serde_json::json!({"delta": 3.2}),
            input_components: if body.is_some() {
                r#"{"prompt_version":"rating-commentary-v21","rating_id":81}"#.to_string()
            } else {
                "{}".to_string()
            },
            exclusions: RatingExclusions::default(),
        },
        "test-rating-model".to_string(),
        RATING_PROMPT_VERSION,
        Vec::new(),
        body.map(|_| "rating-input-hash".to_string()),
    )
}

#[test]
fn an_abstained_card_publishes_a_marker_instead_of_being_debounced() {
    let mut output = rating_product(false, false, None);
    output.product.abstained = true;
    assert!(matches!(prepare(&output), Prepared::Product(_)));
}

/// Exact publication-contract acceptance against an isolated database containing migrations
/// 256-259. Ordinary test runs compile but ignore these cases; opt in with TEST_DATABASE_URL.
mod postgres_publication_fencing_tests {
    use super::*;
    use crate::application::queue::work;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use std::time::Duration;

    const SPORT: &str = "ZZ_RATING_FENCE";
    const ENTITY_ID: i64 = 9_200_003;
    type StatRow = (
        Option<String>,
        Option<String>,
        Option<i16>,
        Option<String>,
        Option<String>,
        Option<String>,
    );

    async fn pool() -> PgPool {
        let url = std::env::var("TEST_DATABASE_URL").expect(
            "set TEST_DATABASE_URL to an isolated database with migrations 256-259 applied",
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
        sqlx::query("DELETE FROM stat_summaries WHERE sport = $1")
            .bind(SPORT)
            .execute(pool)
            .await
            .expect("clean rating products");
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
        .bind("Rating fencing test")
        .execute(pool)
        .await
        .expect("ensure test sport");
    }

    fn pending(revision: &str) -> Item {
        Item {
            stage: crate::plugins::scout::manifest::TASK,
            entity_type: "team".to_string(),
            entity_id: ENTITY_ID,
            sport: SPORT.to_string(),
            input_version: Some(revision.to_string()),
            attempts: 0,
            claim_token: None,
        }
    }

    async fn claim_one(pool: &PgPool) -> Item {
        let mut claimed = work::claim(pool, crate::plugins::scout::manifest::TASK, 1)
            .await
            .expect("claim rating test row");
        assert_eq!(claimed.len(), 1);
        claimed.remove(0)
    }

    async fn counts(pool: &PgPool) -> (i64, i64, i64) {
        let products = sqlx::query_scalar("SELECT count(*) FROM stat_summaries WHERE sport = $1")
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
        let stale_output = rating_product(false, false, Some("Stale product."));
        assert_eq!(
            commit_claimed(
                &pool,
                &stale,
                SPORT,
                "periodic",
                &serde_json::json!({}),
                &prepare(&stale_output),
            )
            .await
            .unwrap(),
            (PluginOutcome::Superseded, None)
        );
        assert_eq!(counts(&pool).await, (0, 0, 1));

        let current = claim_one(&pool).await;
        assert_eq!(current.input_version.as_deref(), Some("v2"));
        let current_output = rating_product(false, false, Some("Current product."));
        assert_eq!(
            commit_claimed(
                &pool,
                &current,
                SPORT,
                "periodic",
                &serde_json::json!({}),
                &prepare(&current_output),
            )
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

        let stale_output = rating_product(false, false, Some("Stale product."));
        assert_eq!(
            commit_claimed(
                &pool,
                &stale,
                SPORT,
                "periodic",
                &serde_json::json!({}),
                &prepare(&stale_output),
            )
            .await
            .unwrap(),
            (PluginOutcome::Superseded, None)
        );
        assert_eq!(counts(&pool).await, (0, 0, 1));

        let current_output = rating_product(false, false, Some("Current product."));
        assert_eq!(
            commit_claimed(
                &pool,
                &current,
                SPORT,
                "periodic",
                &serde_json::json!({}),
                &prepare(&current_output),
            )
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
        let output = rating_product(false, false, Some("The profile is balanced and climbing."));
        let (outcome, row_id) = commit_claimed(
            &pool,
            &current,
            SPORT,
            "periodic",
            &serde_json::json!({"reason": "nightly"}),
            &prepare(&output),
        )
        .await
        .unwrap();
        assert_eq!(outcome, PluginOutcome::Committed);
        assert!(row_id.is_some());
        assert_eq!(counts(&pool).await, (1, 1, 0));

        let row: StatRow = sqlx::query_as(
            "SELECT body, headline, notability, input_hash, model_version, prompt_version \
             FROM stat_summaries WHERE sport = $1",
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row.0.as_deref(),
            Some("The profile is balanced and climbing.")
        );
        assert_eq!(row.1.as_deref(), Some("Test Team owns the middle"));
        assert_eq!(row.2, Some(74));
        assert_eq!(row.3.as_deref(), Some("rating-input-hash"));
        assert_eq!(row.4.as_deref(), Some("test-rating-model"));
        assert_eq!(row.5.as_deref(), Some(RATING_PROMPT_VERSION));
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
                "rating_completed".into(),
                "rating".into(),
                Some("current".into())
            )
        );
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn no_stats_marker_commits_provenance_and_product_followup() {
        let pool = pool().await;
        clean(&pool).await;

        work::enqueue(&pool, &pending("no-stats")).await.unwrap();
        let current = claim_one(&pool).await;
        let output = rating_product(true, false, None);
        let result = commit_claimed(
            &pool,
            &current,
            SPORT,
            "periodic",
            &serde_json::json!({}),
            &prepare(&output),
        )
        .await
        .unwrap();
        assert_eq!(result.0, PluginOutcome::Committed);
        assert!(result.1.is_some());
        assert_eq!(counts(&pool).await, (1, 1, 0));

        let row: StatRow = sqlx::query_as(
            "SELECT body, headline, notability, input_hash, model_version, prompt_version \
             FROM stat_summaries WHERE sport = $1",
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, None);
        assert_eq!(row.1, None);
        assert_eq!(row.2, None);
        assert_eq!(row.3, None);
        assert_eq!(row.4.as_deref(), Some("test-rating-model"));
        assert_eq!(row.5.as_deref(), Some(RATING_PROMPT_VERSION));
        let kind: String =
            sqlx::query_scalar("SELECT kind FROM application_outbox WHERE sport = $1")
                .bind(SPORT)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(kind, "rating_completed");
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn debounced_claim_commits_oracle_only_followup_without_product() {
        let pool = pool().await;
        clean(&pool).await;

        work::enqueue(&pool, &pending("unchanged")).await.unwrap();
        let current = claim_one(&pool).await;
        assert_eq!(
            commit_claimed(
                &pool,
                &current,
                SPORT,
                "periodic",
                &serde_json::json!({}),
                &Prepared::Debounced,
            )
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
        assert_eq!(kind, "rating_debounced");
        clean(&pool).await;
    }
}
