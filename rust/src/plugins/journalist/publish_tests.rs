//! Exact publication tests for the Journalist application boundary.

use super::*;
use crate::harness::queue::work;
use crate::harness::Generation;
use crate::plugins::journalist::{Narrative, NarrativesProduct};

fn edition(narratives: Vec<Narrative>) -> NarrativesOutput {
    let input_ids = narratives
        .iter()
        .flat_map(|narrative| narrative.input_news_ids.iter().copied())
        .collect();
    Generation::uncalled(
        NarrativesProduct {
            memory_provenance: json!({}),
            narratives,
            card_score: Some(71),
            headline: Some("Test Team's story moves".to_string()),
        },
        "test-journalist-model".to_string(),
        crate::plugins::journalist::prompt::NARRATIVES_PROMPT_VERSION,
        input_ids,
        Some("narratives-input-hash".to_string()),
    )
}

fn narrative(title: &str, article_id: i64, impact: i32, source: &str) -> Narrative {
    Narrative {
        title: title.to_string(),
        body: format!("{source} reports a grounded development."),
        impact,
        impact_components: json!({"article_count": 1, "distinct_sources": 1}),
        input_news_ids: vec![article_id],
        source_count: 1,
        source_names: vec![source.to_string()],
        source_latest_epoch: Some(1_700_000_000),
        source_oldest_epoch: Some(1_700_000_000),
    }
}

/// Exact publication-contract acceptance against an isolated database containing migration 260.
/// Ordinary test runs compile but ignore these cases; opt in with TEST_DATABASE_URL.
mod postgres_publication_fencing_tests {
    use super::*;
    use crate::harness::dbtest;
    use sqlx::PgPool;
    use std::time::Duration;

    const SPORT: &str = "ZZ_NARRATIVES_FENCE";
    const ENTITY_ID: i64 = 9_300_004;
    const STORYLINE_ID: i64 = 9_300_041;
    const ARTICLE_ID: i64 = 9_300_042;
    type NarrativeRow = (String, String, Option<i64>, Option<String>, Option<String>);

    /// Children before parents: the outbox references the work row.
    const TABLES: &[&str] = &["application_outbox", "news_summaries", "pipeline_work"];

    async fn pool() -> PgPool {
        dbtest::pool("an isolated database with migration 260 applied").await
    }

    async fn clean(pool: &PgPool) {
        dbtest::clean(pool, SPORT, TABLES, "Narratives fencing test").await;
        // The storyline and article blocks key on id, not sport.
        sqlx::query("DELETE FROM storylines WHERE id = $1")
            .bind(STORYLINE_ID)
            .execute(pool)
            .await
            .expect("clean storyline");
        sqlx::query("DELETE FROM news_articles WHERE id BETWEEN $1 AND ($1 + 3)")
            .bind(ARTICLE_ID)
            .execute(pool)
            .await
            .expect("clean article");
    }

    fn pending(revision: &str) -> Item {
        dbtest::item(
            crate::plugins::journalist::manifest::TASK,
            SPORT,
            ENTITY_ID,
            Some(revision),
        )
    }

    async fn claim_one(pool: &PgPool) -> Item {
        dbtest::claim_one(
            pool,
            crate::plugins::journalist::manifest::TASK,
            "narratives test row",
        )
        .await
    }

    async fn counts(pool: &PgPool) -> (i64, i64, i64) {
        (
            dbtest::count(pool, "news_summaries", SPORT).await,
            dbtest::count(pool, "application_outbox", SPORT).await,
            dbtest::count(pool, "pipeline_work", SPORT).await,
        )
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn current_claim_commits_every_row_event_and_completion() {
        let pool = pool().await;
        clean(&pool).await;

        work::enqueue(&pool, &pending("current")).await.unwrap();
        let current = claim_one(&pool).await;
        let output = edition(vec![
            narrative("First chapter", ARTICLE_ID, 63, "BBC"),
            narrative("Second chapter", ARTICLE_ID, 72, "ESPN"),
        ]);
        let (outcome, row_ids) = commit_claimed(
            &pool,
            &current,
            SPORT,
            "periodic",
            &json!({"reason":"test"}),
            &Prepared::Product(&output),
            &[],
            &[],
        )
        .await
        .unwrap();
        assert_eq!(outcome, PluginOutcome::Committed);
        assert_eq!(row_ids.len(), 2);
        assert_eq!(counts(&pool).await, (2, 1, 0));

        let generations: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT generated_at) FROM news_summaries WHERE sport = $1",
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(generations, 1);
        let rows: Vec<NarrativeRow> = sqlx::query_as(
            "SELECT narrative_title, body, storyline_id, model_version, prompt_version \
                 FROM news_summaries WHERE sport = $1 ORDER BY id",
        )
        .bind(SPORT)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows[0].0, "First chapter");
        assert_eq!(rows[1].0, "Second chapter");
        assert_eq!(rows[0].2, None);
        assert_eq!(rows[0].3.as_deref(), Some("test-journalist-model"));
        assert_eq!(
            rows[0].4.as_deref(),
            Some(crate::plugins::journalist::prompt::NARRATIVES_PROMPT_VERSION)
        );
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
                "narratives_completed".to_string(),
                "narratives".to_string(),
                Some("current".to_string())
            )
        );
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn debounce_commits_only_the_durable_oracle_obligation() {
        let pool = pool().await;
        clean(&pool).await;
        work::enqueue(&pool, &pending("debounced")).await.unwrap();
        let current = claim_one(&pool).await;
        let result = commit_claimed(
            &pool,
            &current,
            SPORT,
            "periodic",
            &serde_json::Value::Null,
            &Prepared::Debounced,
            &[],
            &[],
        )
        .await
        .unwrap();
        assert_eq!(result, (PluginOutcome::Committed, Vec::new()));
        assert_eq!(counts(&pool).await, (0, 1, 0));
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn revision_arriving_during_execution_fences_every_product_row() {
        let pool = pool().await;
        clean(&pool).await;
        work::enqueue(&pool, &pending("v1")).await.unwrap();
        let stale = claim_one(&pool).await;
        work::enqueue(&pool, &pending("v2")).await.unwrap();
        let stale_output = edition(vec![narrative("Source report", ARTICLE_ID, 32, "Wire")]);
        assert_eq!(
            commit_claimed(
                &pool,
                &stale,
                SPORT,
                "periodic",
                &serde_json::Value::Null,
                &Prepared::Product(&stale_output),
                &[],
                &[],
            )
            .await
            .unwrap(),
            (PluginOutcome::Superseded, Vec::new())
        );
        assert_eq!(counts(&pool).await, (0, 0, 1));

        let current = claim_one(&pool).await;
        assert_eq!(current.input_version.as_deref(), Some("v2"));
        let current_output = edition(vec![narrative("Source report", ARTICLE_ID, 32, "Wire")]);
        assert_eq!(
            commit_claimed(
                &pool,
                &current,
                SPORT,
                "periodic",
                &serde_json::Value::Null,
                &Prepared::Product(&current_output),
                &[],
                &[],
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

        let stale_output = edition(vec![narrative("Source report", ARTICLE_ID, 32, "Wire")]);
        assert_eq!(
            commit_claimed(
                &pool,
                &stale,
                SPORT,
                "periodic",
                &serde_json::Value::Null,
                &Prepared::Product(&stale_output),
                &[],
                &[],
            )
            .await
            .unwrap(),
            (PluginOutcome::Superseded, Vec::new())
        );
        let current_output = edition(vec![narrative("Source report", ARTICLE_ID, 32, "Wire")]);
        assert_eq!(
            commit_claimed(
                &pool,
                &current,
                SPORT,
                "periodic",
                &serde_json::Value::Null,
                &Prepared::Product(&current_output),
                &[],
                &[],
            )
            .await
            .unwrap()
            .0,
            PluginOutcome::Committed
        );
        assert_eq!(counts(&pool).await, (1, 1, 0));
        clean(&pool).await;
    }
}
