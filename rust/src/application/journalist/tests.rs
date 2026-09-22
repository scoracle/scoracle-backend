//! Exact publication tests for the Journalist application boundary.

use super::*;
use crate::application::queue::work::Stage;
use crate::studio::journalist::NarrativesProduct;
use crate::studio::Generation;

fn edition(narratives: Vec<Narrative>) -> NarrativesOutput {
    let input_ids = narratives
        .iter()
        .flat_map(|narrative| narrative.input_news_ids.iter().copied())
        .collect();
    Generation::uncalled(
        NarrativesProduct {
            narratives,
            budget_truncated_ids: Vec::new(),
            card_score: Some(71),
            card_score_prev: Some(54),
            headline: Some("Test Team's story moves".to_string()),
        },
        "test-journalist-model".to_string(),
        crate::studio::journalist::NARRATIVES_PROMPT_VERSION,
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
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration;

    const SPORT: &str = "ZZ_NARRATIVES_FENCE";
    const ENTITY_ID: i64 = 9_300_004;
    const STORYLINE_ID: i64 = 9_300_041;
    const ARTICLE_ID: i64 = 9_300_042;
    type NarrativeRow = (String, String, Option<i64>, Option<String>, Option<String>);

    async fn pool() -> PgPool {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("set TEST_DATABASE_URL to an isolated database with migration 260 applied");
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
        sqlx::query("DELETE FROM news_summaries WHERE sport = $1")
            .bind(SPORT)
            .execute(pool)
            .await
            .expect("clean narratives products");
        sqlx::query("DELETE FROM pipeline_work WHERE sport = $1")
            .bind(SPORT)
            .execute(pool)
            .await
            .expect("clean work");
        sqlx::query("DELETE FROM storylines WHERE id = $1")
            .bind(STORYLINE_ID)
            .execute(pool)
            .await
            .expect("clean storyline");
        sqlx::query("DELETE FROM news_articles WHERE id = $1")
            .bind(ARTICLE_ID)
            .execute(pool)
            .await
            .expect("clean article");
        sqlx::query(
            "INSERT INTO sports (id, display_name, current_season) VALUES ($1,$2,2026) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(SPORT)
        .bind("Narratives fencing test")
        .execute(pool)
        .await
        .expect("ensure test sport");
    }

    fn pending(revision: &str) -> Item {
        Item {
            stage: Stage::Narratives,
            entity_type: "team".to_string(),
            entity_id: ENTITY_ID,
            sport: SPORT.to_string(),
            input_version: Some(revision.to_string()),
            attempts: 0,
            claim_token: None,
        }
    }

    async fn claim_one(pool: &PgPool) -> Item {
        let mut claimed = work::claim(pool, Stage::Narratives, 1)
            .await
            .expect("claim narratives test row");
        assert_eq!(claimed.len(), 1);
        claimed.remove(0)
    }

    async fn counts(pool: &PgPool) -> (i64, i64, i64) {
        let products = sqlx::query_scalar("SELECT count(*) FROM news_summaries WHERE sport = $1")
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

    async fn setup_storyline(pool: &PgPool) {
        sqlx::query(
            "INSERT INTO news_articles (id, url_hash, url, source, title, published_at) \
             VALUES ($1,$2,$3,$4,$5,to_timestamp(1700000000))",
        )
        .bind(ARTICLE_ID)
        .bind("zz-narratives-fence-article")
        .bind("https://example.invalid/narratives-fence")
        .bind("BBC")
        .bind("Test Team story moves")
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO storylines (id, sport, title) VALUES ($1,$2,$3)")
            .bind(STORYLINE_ID)
            .bind(SPORT)
            .bind("Test Team story")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO storyline_articles (storyline_id, article_id, attach_method) \
             VALUES ($1,$2,'auto')",
        )
        .bind(STORYLINE_ID)
        .bind(ARTICLE_ID)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO storyline_entities (storyline_id, entity_type, entity_id, sport) \
             VALUES ($1,'team',$2,$3)",
        )
        .bind(STORYLINE_ID)
        .bind(ENTITY_ID as i32)
        .bind(SPORT)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn current_claim_commits_every_row_progression_event_and_completion() {
        let pool = pool().await;
        clean(&pool).await;
        setup_storyline(&pool).await;
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
        assert_eq!(rows[0].2, Some(STORYLINE_ID));
        assert_eq!(rows[0].3.as_deref(), Some("test-journalist-model"));
        assert_eq!(
            rows[0].4.as_deref(),
            Some(crate::studio::journalist::NARRATIVES_PROMPT_VERSION)
        );
        let entry_count: i32 = sqlx::query_scalar(
            "SELECT entry_count FROM storyline_entities \
             WHERE storyline_id = $1 AND entity_type = 'team' AND entity_id = $2 AND sport = $3",
        )
        .bind(STORYLINE_ID)
        .bind(ENTITY_ID as i32)
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(entry_count, 2);
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
    async fn current_claim_commits_one_marker_with_provenance() {
        let pool = pool().await;
        clean(&pool).await;
        work::enqueue(&pool, &pending("marker")).await.unwrap();
        let current = claim_one(&pool).await;
        let output = edition(Vec::new());
        let result = commit_claimed(
            &pool,
            &current,
            SPORT,
            "periodic",
            &serde_json::Value::Null,
            &Prepared::Product(&output),
        )
        .await
        .unwrap();
        assert_eq!(result.0, PluginOutcome::Committed);
        assert_eq!(result.1.len(), 1);
        assert_eq!(counts(&pool).await, (1, 1, 0));
        let row: (Option<String>, Option<String>, Option<i16>, Option<String>) = sqlx::query_as(
            "SELECT narrative_title, body, card_score, input_hash \
             FROM news_summaries WHERE sport = $1",
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, None);
        assert_eq!(row.1, None);
        assert_eq!(row.2, Some(71));
        assert_eq!(row.3.as_deref(), Some("narratives-input-hash"));
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
        let stale_output = edition(Vec::new());
        assert_eq!(
            commit_claimed(
                &pool,
                &stale,
                SPORT,
                "periodic",
                &serde_json::Value::Null,
                &Prepared::Product(&stale_output),
            )
            .await
            .unwrap(),
            (PluginOutcome::Superseded, Vec::new())
        );
        assert_eq!(counts(&pool).await, (0, 0, 1));

        let current = claim_one(&pool).await;
        assert_eq!(current.input_version.as_deref(), Some("v2"));
        let current_output = edition(Vec::new());
        assert_eq!(
            commit_claimed(
                &pool,
                &current,
                SPORT,
                "periodic",
                &serde_json::Value::Null,
                &Prepared::Product(&current_output),
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

        let stale_output = edition(Vec::new());
        assert_eq!(
            commit_claimed(
                &pool,
                &stale,
                SPORT,
                "periodic",
                &serde_json::Value::Null,
                &Prepared::Product(&stale_output),
            )
            .await
            .unwrap(),
            (PluginOutcome::Superseded, Vec::new())
        );
        let current_output = edition(Vec::new());
        assert_eq!(
            commit_claimed(
                &pool,
                &current,
                SPORT,
                "periodic",
                &serde_json::Value::Null,
                &Prepared::Product(&current_output),
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
