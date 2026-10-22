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
        TRUNCATE pipeline_work,application_outbox,coverage_assignments,coverage_editions,coverage_drain_obligations; UPDATE coverage_publication SET edition_date=NULL,revision=revision+1;
        INSERT INTO sports(id,display_name,current_season) VALUES('ZZ_COVERAGE','Coverage test',2026)
            ON CONFLICT(id) DO UPDATE SET current_season=2026;
        DELETE FROM team_stats WHERE sport='ZZ_COVERAGE';
        DELETE FROM teams WHERE sport='ZZ_COVERAGE';
        UPDATE coverage_schedule SET enrolled_at=NULL,cutoff='00:00',drain_limit=interval '15 minutes',
            character_limit=interval '6 hours';").execute(&pool).await?;
    Ok(pool)
}

async fn finish(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(
        "UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL;
        DELETE FROM momentum_summaries WHERE sport='ZZ_COVERAGE';
        DELETE FROM character_daily_publications WHERE sport='ZZ_COVERAGE';
        TRUNCATE pipeline_work,application_outbox,coverage_assignments,coverage_editions,coverage_drain_obligations; UPDATE coverage_publication SET edition_date=NULL,revision=revision+1;",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn enroll(pool: &PgPool) -> Result<()> {
    sqlx::query(
        "UPDATE coverage_schedule SET enabled=true,enrolled_at=clock_timestamp()-interval '1 hour',
        collection_cutoff=(clock_timestamp() AT TIME ZONE timezone-interval '2 seconds')::time",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn tick(pool: &PgPool, seconds: f64) -> Result<String> {
    let instant: String =
        sqlx::query_scalar("SELECT GREATEST(clock_timestamp()+make_interval(secs=>$1), (SELECT e.cutoff_at FROM coverage_editions e JOIN coverage_schedule s USING(edition_date) WHERE s.phase<>'coverage'))::text")
            .bind(seconds)
            .fetch_one(pool)
            .await?;
    super::coverage::tick(pool, Some(&instant)).await
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
async fn cutoff_retains_backoff_and_exhausted_decisions_until_they_settle() -> Result<()> {
    let pool = setup().await?;
    work::enqueue(&pool, &item("classifier", 202, "revision-1")).await?;
    enroll(&pool).await?;
    assert_eq!(tick(&pool, 0.0).await?, "draining");
    sqlx::query("UPDATE pipeline_work SET status='failed',attempts=5,available_at=now()+interval '100 years'").execute(&pool).await?;
    assert_eq!(tick(&pool, 86400.0).await?, "draining");
    assert!(claim(&pool, "classifier").await?.is_empty());
    // Explicit operator repair in this fixture; the controller never resets it.
    sqlx::query("UPDATE pipeline_work SET status='pending',attempts=0,available_at=now()")
        .execute(&pool)
        .await?;
    let decision = claim(&pool, "classifier").await?.remove(0);
    ClaimPublication::begin(&pool, &decision)
        .await?
        .unwrap()
        .commit_final()
        .await?;
    let unsettled: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM coverage_drain_obligations WHERE settled_at IS NULL",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(unsettled, 0);
    assert_eq!(tick(&pool, 0.0).await?, "characters");
    finish(&pool).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires dedicated migrated classifier_test_coverage database; run serially"]
async fn failures_remain_blockers_past_six_hours_and_another_midnight() -> Result<()> {
    let pool = setup().await?;
    work::enqueue(&pool, &item("rating", 101, "stats-v1")).await?;
    enroll(&pool).await?;
    tick(&pool, 0.0).await?;
    tick(&pool, 0.0).await?;
    let rating = claim(&pool, "rating").await?.remove(0);
    work::fail(&pool, &rating, "exhausted", Duration::ZERO, 1).await?;
    sqlx::query("UPDATE pipeline_work SET attempts=5 WHERE stage='rating'")
        .execute(&pool)
        .await?;
    for seconds in [22600.0, 86400.0, 172800.0] {
        assert_eq!(tick(&pool, seconds).await?, "characters");
    }
    assert!(claim(&pool, "momentum").await?.is_empty());
    let state: (String, i32) =
        sqlx::query_as("SELECT status,attempts FROM pipeline_work WHERE stage='rating'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(state, ("failed".into(), 5));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM coverage_editions")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 1);
    let published: Option<String> =
        sqlx::query_scalar("SELECT edition_date::text FROM coverage_publication")
            .fetch_one(&pool)
            .await?;
    assert!(published.is_none());
    finish(&pool).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires dedicated migrated classifier_test_coverage database; run serially"]
async fn only_current_edition_outbox_events_delay_completion() -> Result<()> {
    let pool = setup().await?;
    enroll(&pool).await?;
    assert_eq!(tick(&pool, 0.0).await?, "draining");
    assert_eq!(tick(&pool, 0.0).await?, "characters");
    sqlx::query(
        "INSERT INTO application_outbox(kind,source_stage,source_claim_token,
        entity_type,entity_id,sport,edition_date)
        SELECT 'vibe_completed','vibe',gen_random_uuid(),'team',101,'NBA',edition_date
        FROM coverage_schedule",
    )
    .execute(&pool)
    .await?;
    assert_eq!(tick(&pool, 0.0).await?, "characters");
    sqlx::query(
        "INSERT INTO application_outbox(kind,source_stage,source_claim_token,
        entity_type,entity_id,sport,available_at)
        VALUES('vibe_completed','vibe',gen_random_uuid(),'team',999,'NBA',now()+interval '1 day')",
    )
    .execute(&pool)
    .await?;
    sqlx::query("DELETE FROM application_outbox WHERE edition_date IS NOT NULL")
        .execute(&pool)
        .await?;
    assert_eq!(tick(&pool, 0.0).await?, "coverage");
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM application_outbox")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        remaining, 1,
        "unrelated event stays durable without blocking this edition"
    );
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
async fn scout_retries_use_frozen_measurements_even_after_data_ingestion() -> Result<()> {
    use crate::plugins::scout::{prompt::RatingBuild, snapshot};
    let pool = setup().await?;
    sqlx::raw_sql("INSERT INTO sports(id,display_name,current_season) VALUES('ZZ_COVERAGE','Coverage test',2026) ON CONFLICT DO NOTHING;
        INSERT INTO teams(id,sport,name) VALUES(101,'ZZ_COVERAGE','Cedar'),(102,'ZZ_COVERAGE','Maple') ON CONFLICT DO NOTHING;
        INSERT INTO team_stats(team_id,sport,season,rating_score,rating_breakdown)
        VALUES(101,'ZZ_COVERAGE',2026,75,'[{\"measure\":\"corners\",\"label\":\"Corners\",\"value\":20,\"pct\":75,\"in_comp\":true,\"sign\":1,\"cohort\":30}]')
        ON CONFLICT(team_id,sport,season,league_id) DO UPDATE SET rating_score=75,rating_breakdown=EXCLUDED.rating_breakdown;")
        .execute(&pool).await?;
    work::enqueue(&pool, &item("rating", 101, "measured-v1")).await?;
    work::enqueue(&pool, &item("rating", 102, "no-stats")).await?;
    enroll(&pool).await?;
    assert_eq!(tick(&pool, 0.0).await?, "draining");
    assert_eq!(tick(&pool, 0.0).await?, "characters");
    let mut claimed = claim(&pool, "rating").await?;
    let first = claimed.remove(
        claimed
            .iter()
            .position(|item| item.entity_id == 101)
            .unwrap(),
    );
    let no_stats = claimed.pop().unwrap();
    let Some(RatingBuild::Ready(original)) = snapshot::load(&pool, &first, 4096).await? else {
        panic!("measured profile was not frozen");
    };
    assert_eq!(original.parts.profile.values[0].percentile, Some(75.0));
    sqlx::raw_sql(
        "UPDATE team_stats SET rating_score=5 WHERE sport='ZZ_COVERAGE';
        INSERT INTO team_stats(team_id,sport,season,rating_score) VALUES(102,'ZZ_COVERAGE',2026,60);
        UPDATE teams SET name='Changed after cutoff' WHERE sport='ZZ_COVERAGE';
        UPDATE sports SET current_season=2027 WHERE id='ZZ_COVERAGE';",
    )
    .execute(&pool)
    .await?;
    work::fail(&pool, &first, "temporary model failure", Duration::ZERO, 5).await?;
    let retried = claim(&pool, "rating").await?.remove(0);
    let Some(RatingBuild::Ready(frozen)) = snapshot::load(&pool, &retried, 8192).await? else {
        panic!("retry lost its frozen measurements");
    };
    assert_eq!(frozen.parts.season, 2026);
    assert_eq!(frozen.subject.entity_name, "Cedar");
    assert_eq!(frozen.built_prompt, original.built_prompt);
    assert_eq!(frozen.input_hash, original.input_hash);
    assert_eq!(frozen.opts.num_ctx, 8192);
    assert!(matches!(
        snapshot::load(&pool, &no_stats, 4096).await?,
        Some(RatingBuild::NoStats { .. })
    ));
    finish(&pool).await?;
    sqlx::raw_sql(
        "DELETE FROM team_stats WHERE sport='ZZ_COVERAGE';
        DELETE FROM teams WHERE sport='ZZ_COVERAGE';
        UPDATE sports SET current_season=2026 WHERE id='ZZ_COVERAGE';",
    )
    .execute(&pool)
    .await?;
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
    sqlx::query("UPDATE coverage_schedule SET enabled=true,enrolled_at='2026-01-01Z',collection_cutoff='22:00'").execute(&pool).await?;
    for instant in ["2026-03-07T21:59:59-05:00", "2026-10-31T21:59:59-04:00"] {
        let open: bool =
            sqlx::query_scalar("SELECT coverage_collection_open($1::text::timestamptz)")
                .bind(instant)
                .fetch_one(&pool)
                .await?;
        assert!(open);
    }
    let closed: bool =
        sqlx::query_scalar("SELECT coverage_collection_open('2026-10-31T22:00:00-04:00')")
            .fetch_one(&pool)
            .await?;
    assert!(!closed);
    for (instant, expected) in [
        ("2026-10-31T22:00:00-04:00", "draining"),
        ("2026-10-31T22:05:00-04:00", "draining"),
        ("2026-11-01T00:00:00-04:00", "characters"),
    ] {
        let phase: String = sqlx::query_scalar("SELECT coverage_tick($1::text::timestamptz)")
            .bind(instant)
            .fetch_one(&pool)
            .await?;
        assert_eq!(phase, expected);
    }
    let closed: bool =
        sqlx::query_scalar("SELECT coverage_collection_open('2026-11-01T06:45:00Z')")
            .fetch_one(&pool)
            .await?;
    assert!(!closed);
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
    let visible: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qualified_momentum_summaries WHERE sport='ZZ_COVERAGE'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(visible, 0, "candidate must remain hidden");
    for current in claim(&pool, "sigil").await? {
        ClaimPublication::begin(&pool, &current)
            .await?
            .unwrap()
            .commit_final()
            .await?;
    }
    assert_eq!(tick(&pool, 0.0).await?, "coverage");
    let visible: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qualified_momentum_summaries WHERE sport='ZZ_COVERAGE'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(visible, 1, "only the complete edition becomes visible");
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM coverage_publication")
        .fetch_one(&pool)
        .await?;
    sqlx::query("DELETE FROM momentum_summaries WHERE sport='ZZ_COVERAGE'")
        .execute(&pool)
        .await?;
    let next: i64 = sqlx::query_scalar("SELECT revision FROM coverage_publication")
        .fetch_one(&pool)
        .await?;
    assert!(next > revision, "withdrawal changes the cache boundary");
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
        let (assignment, _) = crate::plugins::influencer::fresh::prepare_assignment(
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

#[tokio::test]
#[ignore = "requires dedicated migrated classifier_test_coverage database; run serially"]
async fn people_receive_explicit_empty_reporting_outcomes_without_statistical_jobs() -> Result<()> {
    let pool = setup().await?;
    let person = Item {
        entity_type: "person".into(),
        ..item("narratives", 901, "classifier-spectrum-v1:person")
    };
    work::enqueue(&pool, &person).await?;
    enroll(&pool).await?;
    assert_eq!(tick(&pool, 0.0).await?, "draining");
    assert_eq!(tick(&pool, 0.0).await?, "characters");
    let stages: Vec<String>=sqlx::query_scalar("SELECT stage FROM coverage_assignments WHERE entity_type='person' AND entity_id=901 ORDER BY stage").fetch_all(&pool).await?;
    assert_eq!(stages, vec!["narratives", "transfers", "vibe"]);
    assert!(
        !crate::plugins::analyst::enqueue_momentum_if_needed(&pool, "person", 901, "ZZ_COVERAGE")
            .await?
    );
    assert!(
        !crate::plugins::oracle::enqueue_oracle_if_pillars_settled(
            &pool,
            "person",
            901,
            "ZZ_COVERAGE",
            None
        )
        .await?
    );
    for stage in ["narratives", "vibe", "transfers"] {
        assert!(claim(&pool, stage).await?.is_empty());
    }
    let complete:i64=sqlx::query_scalar("SELECT count(*) FROM coverage_assignments WHERE entity_type='person' AND entity_id=901 AND outcome='completed'").fetch_one(&pool).await?;
    assert_eq!(complete, 3);
    let reactions = crate::harness::registration::build_reactions(pool.clone())?;
    assert_eq!(super::outbox::drain(&pool, &reactions, 10).await?, 0);
    let statistical_jobs:i64=sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE entity_type='person' AND stage IN ('rating','momentum','sigil')").fetch_one(&pool).await?;
    assert_eq!(statistical_jobs, 0);
    assert_eq!(tick(&pool, 0.0).await?, "coverage");
    sqlx::raw_sql("DELETE FROM news_summaries WHERE entity_type='person' AND sport='ZZ_COVERAGE'; DELETE FROM vibe_scores WHERE entity_type='person' AND sport='ZZ_COVERAGE'; DELETE FROM insider_scores WHERE entity_type='person' AND sport='ZZ_COVERAGE';").execute(&pool).await?;
    finish(&pool).await?;
    Ok(())
}
