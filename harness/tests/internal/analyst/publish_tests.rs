use super::*;
use crate::harness::dbtest;
use crate::harness::queue::publication::ClaimPublication;
use crate::harness::queue::work;
use sqlx::PgPool;
use std::time::Duration;

const SPORT: &str = "ZZ_MOMENTUM_FENCE";
const ENTITY_ID: i64 = 9_200_002;
/// Children before parents: the outbox references the work row.
const TABLES: &[&str] = &["application_outbox", "momentum_summaries", "pipeline_work"];

async fn pool() -> PgPool {
    dbtest::pool("an isolated database with migrations 256-258 applied").await
}

async fn clean(pool: &PgPool) {
    dbtest::clean(pool, SPORT, TABLES, "Momentum fencing test").await;
}

fn pending(revision: &str) -> Item {
    dbtest::item(
        crate::harness::skills::analyst::manifest::TASK,
        SPORT,
        ENTITY_ID,
        Some(revision),
    )
}

async fn claim_one(pool: &PgPool) -> Item {
    dbtest::claim_one(
        pool,
        crate::harness::skills::analyst::manifest::TASK,
        "momentum test row",
    )
    .await
}

async fn product(pool: &PgPool) -> Option<MomentumOutput> {
    let adapters = LifecycleAdapters::default();
    create(pool, &Studio::new(&adapters), &lifecycle_assignment(true))
        .await
        .expect("prepare momentum product")
}

async fn counts(pool: &PgPool) -> (i64, i64, i64) {
    (
        dbtest::count(pool, "momentum_summaries", SPORT).await,
        dbtest::count(pool, "application_outbox", SPORT).await,
        dbtest::count(pool, "pipeline_work", SPORT).await,
    )
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
        commit_claimed(&pool, &stale, SPORT, product(&pool).await.as_ref())
            .await
            .unwrap(),
        (SkillOutcome::Superseded, None)
    );
    assert_eq!(counts(&pool).await, (0, 0, 1));

    let current = claim_one(&pool).await;
    assert_eq!(current.input_version.as_deref(), Some("v2"));
    assert_eq!(
        commit_claimed(&pool, &current, SPORT, product(&pool).await.as_ref())
            .await
            .unwrap()
            .0,
        SkillOutcome::Committed
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
    sqlx::query("UPDATE pipeline_work SET updated_at = NOW() - INTERVAL '1 hour' WHERE sport = $1")
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
        commit_claimed(&pool, &stale, SPORT, product(&pool).await.as_ref())
            .await
            .unwrap(),
        (SkillOutcome::Superseded, None)
    );
    assert_eq!(counts(&pool).await, (0, 0, 1));
    assert_eq!(
        commit_claimed(&pool, &current, SPORT, product(&pool).await.as_ref())
            .await
            .unwrap()
            .0,
        SkillOutcome::Committed
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
    let prepared = product(&pool).await;
    let expected_hash = match &prepared {
        Some(output) => output.provenance.input_hash.clone(),
        None => panic!("expected product"),
    };
    let (outcome, row_id) = commit_claimed(&pool, &current, SPORT, prepared.as_ref())
        .await
        .unwrap();
    assert_eq!(outcome, SkillOutcome::Committed);
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
        commit_claimed(&pool, &current, SPORT, None).await.unwrap(),
        (SkillOutcome::Committed, None)
    );
    assert_eq!(counts(&pool).await, (0, 1, 0));
    let kind: String = sqlx::query_scalar("SELECT kind FROM application_outbox WHERE sport = $1")
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
        let prepared = product(&pool).await;
        if phase == "before-commit" {
            let mut publication = ClaimPublication::begin(&pool, &current)
                .await
                .unwrap()
                .expect("current claim");
            if let Some(output) = &prepared {
                persist_momentum_summary(publication.transaction(), &current, SPORT, output)
                    .await
                    .unwrap();
            }
            crate::harness::skills::analyst::record_momentum_completed(
                publication.transaction(),
                &current,
            )
            .await
            .unwrap();
            std::process::exit(86);
        }
        assert_eq!(
            commit_claimed(&pool, &current, SPORT, prepared.as_ref())
                .await
                .unwrap()
                .0,
            SkillOutcome::Committed
        );
        std::process::exit(86);
    }
    let pool = pool().await;
    clean(&pool).await;
    work::enqueue(&pool, &pending("crash")).await.unwrap();
    let crash = |phase: &str| {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "harness::skills::analyst::publish::tests::postgres_publication_fencing_tests::process_crash_publication_rehearsal", "--ignored", "--nocapture"])
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
    let reactions = crate::harness::registration::build_reactions(pool.clone()).unwrap();
    assert_eq!(
        crate::harness::queue::outbox::drain(&pool, &reactions, 1)
            .await
            .unwrap(),
        1
    );
    assert_eq!(counts(&pool).await, (1, 0, 1));
    assert_eq!(
        crate::harness::queue::outbox::drain(&pool, &reactions, 1)
            .await
            .unwrap(),
        0
    );
    clean(&pool).await;
}
