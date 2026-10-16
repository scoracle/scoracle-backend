//! These mutate the global clock and therefore require a dedicated, disposable database.
use super::publication::ClaimPublication;
use super::work::{self, ClaimPolicy, Item, TaskKey};
use anyhow::{ensure, Result};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

async fn setup() -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("TEST_DATABASE_URL")?)
        .await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    ensure!(
        database.starts_with("classifier_test_coverage"),
        "requires dedicated classifier_test_coverage database"
    );
    sqlx::raw_sql("UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL;
        TRUNCATE pipeline_work,application_outbox,coverage_assignments,coverage_editions;
        UPDATE coverage_schedule SET enrolled_at=NULL,cutoff='00:00',drain_limit=interval '15 minutes',
            character_limit=interval '6 hours';").execute(&pool).await?;
    Ok(pool)
}

async fn finish(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(
        "UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL;
        DELETE FROM momentum_summaries WHERE sport='ZZ_COVERAGE';
        DELETE FROM character_daily_publications WHERE sport='ZZ_COVERAGE';
        TRUNCATE pipeline_work,application_outbox,coverage_assignments,coverage_editions;",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn enroll(pool: &PgPool) -> Result<()> {
    sqlx::query(
        "UPDATE coverage_schedule SET enabled=true,enrolled_at=clock_timestamp()-interval '1 hour',
        cutoff=(clock_timestamp() AT TIME ZONE timezone-interval '2 seconds')::time",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn tick(pool: &PgPool, seconds: f64) -> Result<String> {
    Ok(
        sqlx::query_scalar("SELECT coverage_tick(clock_timestamp()+make_interval(secs=>$1))")
            .bind(seconds)
            .fetch_one(pool)
            .await?,
    )
}

fn item(stage: &'static str, id: i64, version: &str) -> Item {
    Item {
        stage: TaskKey::new(stage),
        entity_type: "team".into(),
        entity_id: id,
        sport: "ZZ_COVERAGE".into(),
        input_version: Some(version.into()),
        attempts: 0,
        claim_token: None,
    }
}

async fn claim(pool: &PgPool, stage: &'static str) -> Result<Vec<Item>> {
    work::claim_with_policy(pool, TaskKey::new(stage), ClaimPolicy::FIFO, 10).await
}

#[tokio::test]
#[ignore = "requires dedicated migrated classifier_test_coverage database; run serially"]
async fn cutoff_freezes_claims_coalesces_offers_and_recovers_at_deadline() -> Result<()> {
    let pool = setup().await?;
    // Disabled enrollment retains the old queue contract.
    work::enqueue(&pool, &item("rating", 101, "stats-v1")).await?;
    let old = claim(&pool, "rating").await?.remove(0);
    ClaimPublication::begin(&pool, &old)
        .await?
        .unwrap()
        .commit_final()
        .await?;

    work::enqueue(&pool, &item("classifier_acquire", 201, "source-v1")).await?;
    let prep = claim(&pool, "classifier_acquire").await?.remove(0);
    work::enqueue(&pool, &item("classifier", 202, "source-v2")).await?;
    work::enqueue(&pool, &item("rating", 101, "stats-v2")).await?;
    enroll(&pool).await?;
    assert!(claim(&pool, "rating").await?.is_empty());
    assert_eq!(tick(&pool, 0.0).await?, "draining");
    assert!(claim(&pool, "classifier_acquire").await?.is_empty());
    assert!(ClaimPublication::begin(&pool, &prep).await?.is_some());
    assert_eq!(claim(&pool, "classifier").await?.len(), 1);
    assert_eq!(tick(&pool, 0.0).await?, "draining");
    assert_eq!(tick(&pool, 901.0).await?, "characters");
    let assignments: i64 = sqlx::query_scalar("SELECT count(*) FROM coverage_assignments")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        assignments, 6,
        "each active subject gets one assignment for every character"
    );
    assert!(ClaimPublication::begin(&pool, &prep).await?.is_none());
    assert!(claim(&pool, "classifier").await?.is_empty());
    assert!(
        claim(&pool, "momentum").await?.is_empty(),
        "same-edition Scout blocks Analyst"
    );
    let first = claim(&pool, "rating").await?.remove(0);
    assert!(
        claim(&pool, "rating").await?.is_empty(),
        "second host cannot own same job"
    );
    work::enqueue(&pool, &item("rating", 101, "stats-v3")).await?;
    work::enqueue(&pool, &item("rating", 102, "new-subject")).await?;
    assert!(
        claim(&pool, "rating").await?.is_empty(),
        "new subjects wait until next edition"
    );
    let saved: Option<String> = sqlx::query_scalar(
        "SELECT claim_token::text FROM pipeline_work
        WHERE stage='rating' AND entity_id=101",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(saved, first.claim_token);
    ClaimPublication::begin(&pool, &first)
        .await?
        .unwrap()
        .commit_final()
        .await?;
    assert!(
        claim(&pool, "rating").await?.is_empty(),
        "replayed offer cannot reopen completed daily assignment"
    );
    for stage in ["narratives", "vibe", "transfers"] {
        for current in claim(&pool, stage).await? {
            ClaimPublication::begin(&pool, &current)
                .await?
                .unwrap()
                .commit_final()
                .await?;
        }
    }
    let analyst = claim(&pool, "momentum").await?.remove(0);
    assert!(
        claim(&pool, "sigil").await?.is_empty(),
        "Oracle waits for current Analyst"
    );
    assert_eq!(tick(&pool, 902.0).await?, "characters");
    assert_eq!(tick(&pool, 22600.0).await?, "coverage");
    assert!(
        ClaimPublication::begin(&pool, &analyst).await?.is_none(),
        "late host cannot publish"
    );
    let (status, reason): (String, Option<String>) =
        sqlx::query_as("SELECT status,reason FROM coverage_editions")
            .fetch_one(&pool)
            .await?;
    assert_eq!(status, "partial");
    assert_eq!(reason.as_deref(), Some("character_deadline"));
    assert_eq!(
        tick(&pool, 0.0).await?,
        "coverage",
        "duplicate cutoff cannot create another edition"
    );
    assert!(
        claim(&pool, "rating").await?.is_empty(),
        "characters remain parked during coverage"
    );
    let carried: String = sqlx::query_scalar(
        "SELECT input_version FROM pipeline_work
        WHERE stage='rating' AND entity_id=101",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(carried, "stats-v3");
    assert_eq!(claim(&pool, "classifier_acquire").await?.len(), 1);
    finish(&pool).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires dedicated migrated classifier_test_coverage database; run serially"]
async fn transition_waits_for_publication_and_failed_work_cannot_fake_fresh_peers() -> Result<()> {
    let pool = setup().await?;
    work::enqueue(&pool, &item("rating", 101, "stats-v1")).await?;
    enroll(&pool).await?;
    assert_eq!(tick(&pool, 0.0).await?, "draining");
    assert_eq!(tick(&pool, 0.0).await?, "characters");
    let rating = claim(&pool, "rating").await?.remove(0);
    let publication = ClaimPublication::begin(&pool, &rating).await?.unwrap();
    let other = pool.clone();
    let transition = tokio::spawn(async move { tick(&other, 22600.0).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !transition.is_finished(),
        "cutoff waits for atomic publication transaction"
    );
    publication.commit_final().await?;
    assert_eq!(transition.await??, "coverage");
    let completed: String =
        sqlx::query_scalar("SELECT outcome FROM coverage_assignments WHERE stage='rating'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(completed, "completed");

    // Start a fresh isolated cycle to exercise failed dependency behavior.
    sqlx::raw_sql(
        "UPDATE coverage_schedule SET enabled=false;
        TRUNCATE pipeline_work,application_outbox,coverage_assignments,coverage_editions;",
    )
    .execute(&pool)
    .await?;
    work::enqueue(&pool, &item("rating", 101, "stats-v2")).await?;
    enroll(&pool).await?;
    tick(&pool, 0.0).await?;
    tick(&pool, 0.0).await?;
    let rating = claim(&pool, "rating").await?.remove(0);
    work::fail(&pool, &rating, "test exhausted worker", Duration::ZERO, 1).await?;
    sqlx::query("UPDATE pipeline_work SET attempts=5 WHERE stage='rating'")
        .execute(&pool)
        .await?;
    assert_eq!(tick(&pool, 0.0).await?, "characters");
    assert!(
        claim(&pool, "momentum").await?.is_empty(),
        "failed peer cannot be replaced by yesterday's card"
    );
    assert_eq!(tick(&pool, 22600.0).await?, "coverage");
    let failed: String =
        sqlx::query_scalar("SELECT outcome FROM coverage_assignments WHERE stage='rating'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(failed, "failed");
    finish(&pool).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires dedicated migrated classifier_test_coverage database; run serially"]
async fn empty_editions_finish_early_and_outbox_retries_remain_visible() -> Result<()> {
    let pool = setup().await?;
    enroll(&pool).await?;
    assert_eq!(tick(&pool, 0.0).await?, "draining");
    assert_eq!(tick(&pool, 0.0).await?, "characters");
    sqlx::query("INSERT INTO application_outbox(kind,source_stage,source_claim_token,
        entity_type,entity_id,sport) VALUES('vibe_completed','vibe',gen_random_uuid(),'team',101,'NBA')")
        .execute(&pool).await?;
    assert_eq!(tick(&pool, 0.0).await?, "characters");
    sqlx::query("DELETE FROM application_outbox")
        .execute(&pool)
        .await?;
    assert_eq!(tick(&pool, 0.0).await?, "coverage");
    let status: String = sqlx::query_scalar("SELECT status FROM coverage_editions")
        .fetch_one(&pool)
        .await?;
    assert_eq!(status, "complete");
    assert_eq!(tick(&pool, 0.0).await?, "coverage");
    finish(&pool).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires dedicated migrated classifier_test_coverage database; run serially"]
async fn local_midnight_handles_dst_and_missed_windows_without_duplicate_dates() -> Result<()> {
    let pool = setup().await?;
    let (spring, autumn): (f64, f64) = sqlx::query_as(
        "SELECT
        extract(epoch FROM (timestamp '2026-03-09' AT TIME ZONE 'America/Detroit' -
                           timestamp '2026-03-08' AT TIME ZONE 'America/Detroit'))::float8/3600,
        extract(epoch FROM (timestamp '2026-11-02' AT TIME ZONE 'America/Detroit' -
                           timestamp '2026-11-01' AT TIME ZONE 'America/Detroit'))::float8/3600",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!((spring, autumn), (23.0, 25.0));
    sqlx::query(
        "UPDATE coverage_schedule SET enabled=true,enrolled_at='2026-01-01Z',cutoff='00:00'",
    )
    .execute(&pool)
    .await?;
    for instant in [
        "2026-03-08T08:00:00Z",
        "2026-11-01T06:15:00Z",
        "2026-11-01T06:45:00Z",
    ] {
        for _ in 0..4 {
            let _: String = sqlx::query_scalar("SELECT coverage_tick($1::text::timestamptz)")
                .bind(instant)
                .fetch_one(&pool)
                .await?;
        }
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM coverage_editions WHERE edition_date='2026-11-01'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(count, 1);
    // An outage beyond the entire production window reopens without a stale night.
    sqlx::query("SELECT coverage_tick('2026-11-01T13:00:00Z')")
        .execute(&pool)
        .await?;
    let _: String = sqlx::query_scalar("SELECT coverage_tick('2026-11-03T20:00:00Z')")
        .fetch_one(&pool)
        .await?;
    let (status, reason): (String, String) = sqlx::query_as(
        "SELECT status,reason FROM coverage_editions
        WHERE edition_date='2026-11-03'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        (status.as_str(), reason.as_str()),
        ("partial", "controller_missed_window")
    );
    finish(&pool).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires dedicated migrated classifier_test_coverage database; run serially"]
async fn daily_output_requires_the_lease_and_cannot_publish_from_two_transactions() -> Result<()> {
    let pool = setup().await?;
    work::enqueue(&pool, &item("momentum", 101, "daily-v1")).await?;
    enroll(&pool).await?;
    tick(&pool, 0.0).await?;
    tick(&pool, 0.0).await?;
    let insert="INSERT INTO momentum_summaries(entity_type,entity_id,sport,season,blurb,model_version,prompt_version)
        VALUES('team',101,'ZZ_COVERAGE',2026,'Fresh peer readings.','fixture','daily-fresh')";
    assert!(
        sqlx::query(insert).execute(&pool).await.is_err(),
        "direct writer has no queue lease"
    );
    for stage in ["narratives", "vibe", "rating", "transfers"] {
        for current in claim(&pool, stage).await? {
            ClaimPublication::begin(&pool, &current)
                .await?
                .unwrap()
                .commit_final()
                .await?;
        }
    }
    let current = claim(&pool, "momentum").await?.remove(0);
    let mut first = ClaimPublication::begin(&pool, &current).await?.unwrap();
    sqlx::query(insert)
        .execute(&mut **first.transaction())
        .await?;
    first.commit_progress().await?;
    let mut duplicate = ClaimPublication::begin(&pool, &current).await?.unwrap();
    assert!(
        sqlx::query(insert)
            .execute(&mut **duplicate.transaction())
            .await
            .is_err(),
        "a second per-article pass cannot publish another daily card"
    );
    drop(duplicate);
    ClaimPublication::begin(&pool, &current)
        .await?
        .unwrap()
        .commit_final()
        .await?;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM momentum_summaries WHERE sport='ZZ_COVERAGE'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(count, 1);
    finish(&pool).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires dedicated migrated classifier_test_coverage database; run serially"]
async fn fresh_package_uses_local_days_without_a_reporting_week_or_memory_study() -> Result<()> {
    let pool = setup().await?;
    for (instant, hours) in [("2026-03-08T18:00:00Z", 23), ("2026-11-01T18:00:00Z", 25)] {
        let cutoff: i64 =
            sqlx::query_scalar("SELECT extract(epoch FROM $1::text::timestamptz)::bigint")
                .bind(instant)
                .fetch_one(&pool)
                .await?;
        let (assignment, _) = crate::plugins::influencer::prompt::prepare_assignment(
            &pool,
            crate::tools::meta::EntityMeta {
                name: "Cedar".into(),
                entity_type: "team".into(),
                entity_id: 101,
                sport: "ZZ_COVERAGE".into(),
            },
            cutoff,
        )
        .await?;
        assert_eq!(
            assignment.parts.day.end - assignment.parts.day.start,
            hours * 3600
        );
        let packet = assignment.parts.assemble();
        assert!(!packet.contains("HISTORY"));
        assert!(!packet.contains("week"));
    }
    finish(&pool).await?;
    Ok(())
}
