//! Exact-claim transfer publication and durable follow-up tests.

use super::*;
use crate::application::queue::work::Stage;
use crate::studio::insider::TransferPairProduct;
use crate::studio::Generation;

fn pair_output(is_rumor: bool) -> (TransferPairOutput, TransferRow) {
    let row = TransferRow {
        is_rumor: Some(is_rumor),
        direction: is_rumor.then(|| "incoming".to_string()),
        stage: is_rumor.then(|| "advanced_talks".to_string()),
        summary: is_rumor.then(|| "Test Wire reports advancing talks.".to_string()),
        attribution: Some("Test Wire".to_string()),
        confidence: is_rumor.then_some(0.82),
        model: Some("test-insider-model".to_string()),
        trigger_payload: r#"{"subject":"Test Player"}"#.to_string(),
    };
    let output = Generation::uncalled(
        TransferPairProduct {
            player_id: postgres_tests::PLAYER_ID,
            subject_type: "player".to_string(),
            heat: Some(82),
            components: r#"{"distinct_sources":2}"#.to_string(),
            news_ids: Vec::new(),
            prompted_news_ids: Vec::new(),
            stale_news_ids: Vec::new(),
            outcome: if is_rumor {
                Outcome::Rumor
            } else {
                Outcome::Cleared
            },
            row: Some(row.clone()),
            identity_apply_news: Vec::new(),
        },
        "test-insider-model".to_string(),
        TRANSFER_PROMPT_VERSION,
        Vec::new(),
        Some("transfer-input-hash".to_string()),
    );
    (output, row)
}

mod postgres_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration;

    const SPORT: &str = "ZZ_INSIDER_FENCE";
    const TEAM_ID: i64 = 9_300_010;
    pub(super) const PLAYER_ID: i32 = 9_300_011;

    async fn pool() -> PgPool {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("set TEST_DATABASE_URL to an isolated database with migrations through 261");
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
            .unwrap();
        sqlx::query("DELETE FROM transfer_rumors WHERE sport = $1")
            .bind(SPORT)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM pipeline_work WHERE sport = $1")
            .bind(SPORT)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM players WHERE sport = $1")
            .bind(SPORT)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM teams WHERE sport = $1")
            .bind(SPORT)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO sports (id, display_name, current_season) VALUES ($1,$2,2026) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(SPORT)
        .bind("Insider fencing test")
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO teams (id, sport, name) VALUES ($1,$2,$3)")
            .bind(TEAM_ID as i32)
            .bind(SPORT)
            .bind("Test Team")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO players (id, sport, name) VALUES ($1,$2,$3)")
            .bind(PLAYER_ID)
            .bind(SPORT)
            .bind("Test Player")
            .execute(pool)
            .await
            .unwrap();
    }

    fn pending(revision: &str) -> Item {
        Item {
            stage: Stage::Transfers,
            entity_type: "team".to_string(),
            entity_id: TEAM_ID,
            sport: SPORT.to_string(),
            input_version: Some(revision.to_string()),
            attempts: 0,
            claim_token: None,
        }
    }

    async fn claim_one(pool: &PgPool) -> Item {
        let mut claimed = crate::application::queue::work::claim(pool, Stage::Transfers, 1)
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);
        claimed.remove(0)
    }

    async fn counts(pool: &PgPool) -> (i64, i64, i64) {
        let products = sqlx::query_scalar("SELECT count(*) FROM transfer_rumors WHERE sport=$1")
            .bind(SPORT)
            .fetch_one(pool)
            .await
            .unwrap();
        let events = sqlx::query_scalar("SELECT count(*) FROM application_outbox WHERE sport=$1")
            .bind(SPORT)
            .fetch_one(pool)
            .await
            .unwrap();
        let work = sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE sport=$1")
            .bind(SPORT)
            .fetch_one(pool)
            .await
            .unwrap();
        (products, events, work)
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn current_claim_commits_pair_event_then_final_team_completion() {
        let pool = pool().await;
        clean(&pool).await;
        crate::application::queue::work::enqueue(&pool, &pending("current"))
            .await
            .unwrap();
        let current = claim_one(&pool).await;
        let (output, row) = pair_output(true);
        let id = persist_transfer_row(
            &pool,
            &current,
            TEAM_ID as i32,
            PLAYER_ID,
            SPORT,
            "periodic",
            &output,
            &row,
        )
        .await
        .unwrap()
        .expect("current claim publishes");
        assert!(id > 0);
        assert_eq!(counts(&pool).await, (1, 1, 1));
        assert_eq!(
            complete_claimed(&pool, &current).await.unwrap(),
            PluginOutcome::Committed
        );
        assert_eq!(counts(&pool).await, (1, 2, 0));
        let targets: Vec<(String, i32)> = sqlx::query_as(
            "SELECT entity_type, entity_id FROM application_outbox WHERE sport=$1 ORDER BY entity_type, entity_id",
        )
        .bind(SPORT)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            targets,
            vec![
                ("player".to_string(), PLAYER_ID),
                ("team".to_string(), TEAM_ID as i32)
            ]
        );
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn revision_supersession_publishes_no_pair_or_event() {
        let pool = pool().await;
        clean(&pool).await;
        crate::application::queue::work::enqueue(&pool, &pending("v1"))
            .await
            .unwrap();
        let stale = claim_one(&pool).await;
        crate::application::queue::work::enqueue(&pool, &pending("v2"))
            .await
            .unwrap();
        let (output, row) = pair_output(true);
        assert!(persist_transfer_row(
            &pool,
            &stale,
            TEAM_ID as i32,
            PLAYER_ID,
            SPORT,
            "periodic",
            &output,
            &row,
        )
        .await
        .unwrap()
        .is_none());
        assert_eq!(counts(&pool).await, (0, 0, 1));
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn same_revision_reclaim_gives_only_the_new_lease_publication_rights() {
        let pool = pool().await;
        clean(&pool).await;
        crate::application::queue::work::enqueue(&pool, &pending("same"))
            .await
            .unwrap();
        let stale = claim_one(&pool).await;
        sqlx::query("UPDATE pipeline_work SET updated_at=NOW()-INTERVAL '1 hour' WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await
            .unwrap();
        crate::application::queue::work::requeue_stale(&pool, Duration::from_secs(30 * 60))
            .await
            .unwrap();
        let current = claim_one(&pool).await;
        let (output, row) = pair_output(true);
        assert!(persist_transfer_row(
            &pool,
            &stale,
            TEAM_ID as i32,
            PLAYER_ID,
            SPORT,
            "periodic",
            &output,
            &row,
        )
        .await
        .unwrap()
        .is_none());
        assert!(persist_transfer_row(
            &pool,
            &current,
            TEAM_ID as i32,
            PLAYER_ID,
            SPORT,
            "periodic",
            &output,
            &row,
        )
        .await
        .unwrap()
        .is_some());
        assert_eq!(counts(&pool).await, (1, 1, 1));
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn cleared_pair_is_fenced_but_creates_no_player_oracle_event() {
        let pool = pool().await;
        clean(&pool).await;
        crate::application::queue::work::enqueue(&pool, &pending("cleared"))
            .await
            .unwrap();
        let current = claim_one(&pool).await;
        let (output, row) = pair_output(false);
        assert!(persist_transfer_row(
            &pool,
            &current,
            TEAM_ID as i32,
            PLAYER_ID,
            SPORT,
            "periodic",
            &output,
            &row,
        )
        .await
        .unwrap()
        .is_some());
        assert_eq!(counts(&pool).await, (1, 0, 1));
        assert_eq!(
            complete_claimed(&pool, &current).await.unwrap(),
            PluginOutcome::Committed
        );
        assert_eq!(counts(&pool).await, (1, 1, 0));
        clean(&pool).await;
    }
    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
    async fn partial_pair_survives_restart_and_newer_team_revision() {
        let pool = pool().await;
        clean(&pool).await;
        crate::application::queue::work::enqueue(&pool, &pending("v1"))
            .await
            .unwrap();
        let stale = claim_one(&pool).await;
        let (output, row) = pair_output(true);
        persist_transfer_row(
            &pool,
            &stale,
            TEAM_ID as i32,
            PLAYER_ID,
            SPORT,
            "periodic",
            &output,
            &row,
        )
        .await
        .unwrap()
        .expect("committed partial pair");
        assert_eq!(counts(&pool).await, (1, 1, 1));
        pool.close().await;
        let pool = self::pool().await;
        // A source correction lands while the old team's remaining pairs are absent.
        crate::application::queue::work::enqueue(&pool, &pending("v2"))
            .await
            .unwrap();
        assert!(persist_transfer_row(
            &pool,
            &stale,
            TEAM_ID as i32,
            PLAYER_ID,
            SPORT,
            "periodic",
            &output,
            &row
        )
        .await
        .unwrap()
        .is_none());
        assert_eq!(
            complete_claimed(&pool, &stale).await.unwrap(),
            PluginOutcome::Superseded
        );
        assert_eq!(counts(&pool).await, (1, 1, 1));
        let current = claim_one(&pool).await;
        assert_eq!(current.input_version.as_deref(), Some("v2"));
        assert_eq!(
            complete_claimed(&pool, &current).await.unwrap(),
            PluginOutcome::Committed
        );
        assert_eq!(counts(&pool).await, (1, 2, 0));
        let targets: Vec<(String,i32)> = sqlx::query_as("SELECT entity_type,entity_id FROM application_outbox WHERE sport=$1 ORDER BY entity_type")
            .bind(SPORT).fetch_all(&pool).await.unwrap();
        assert_eq!(
            targets,
            vec![
                ("player".into(), PLAYER_ID),
                ("team".into(), TEAM_ID as i32)
            ]
        );
        clean(&pool).await;
    }
}
