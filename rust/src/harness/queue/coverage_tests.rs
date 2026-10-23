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
        TRUNCATE coverage_failures,pipeline_work,application_outbox,coverage_assignments,coverage_editions,coverage_drain_obligations; UPDATE coverage_publication SET edition_date=NULL,revision=revision+1;
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
        TRUNCATE coverage_failures,pipeline_work,application_outbox,coverage_assignments,coverage_editions,coverage_drain_obligations; UPDATE coverage_publication SET edition_date=NULL,revision=revision+1;",
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
async fn exhausted_decisions_are_reported_and_release_the_drain_without_retry_reset() -> Result<()>
{
    let pool = setup().await?;
    let offered = item("classifier", 202, "revision-1");
    work::enqueue(&pool, &offered).await?;
    enroll(&pool).await?;
    assert_eq!(tick(&pool, 0.0).await?, "draining");
    for attempt in 1..=work::MAX_ATTEMPTS {
        let decision = claim(&pool, "classifier").await?.remove(0);
        assert_eq!(decision.attempts, attempt - 1);
        assert!(
            work::fail(
                &pool,
                &decision,
                "decision provider unavailable",
                Duration::ZERO,
                work::MAX_ATTEMPTS
            )
            .await?
        );
        work::enqueue(&pool, &offered).await?;
        if attempt < work::MAX_ATTEMPTS {
            assert_eq!(tick(&pool, 0.0).await?, "draining");
        }
    }
    assert!(claim(&pool, "classifier").await?.is_empty());
    let restarted = PgPoolOptions::new()
        .connect(&std::env::var("TEST_DATABASE_URL")?)
        .await?;
    assert_eq!(tick(&restarted, 0.0).await?, "characters");
    assert_eq!(tick(&restarted, 0.0).await?, "coverage");
    let receipt: (String, i32, String) = sqlx::query_as(
        "SELECT kind,attempts,error FROM coverage_failures WHERE sport='ZZ_COVERAGE'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        receipt,
        (
            "decision_exhausted".into(),
            5,
            "decision provider unavailable".into()
        )
    );
    let (status, published): (String, Option<String>) = sqlx::query_as(
        "SELECT e.status,p.edition_date::text FROM coverage_editions e CROSS JOIN coverage_publication p",
    ).fetch_one(&pool).await?;
    assert_eq!(status, "partial");
    assert!(published.is_none());
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT coverage_collection_open()")
            .fetch_one(&pool)
            .await?
    );
    assert_eq!(tick(&restarted, 0.0).await?, "coverage");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM coverage_failures")
            .fetch_one(&pool)
            .await?,
        1
    );
    work::enqueue(&pool, &item("classifier", 202, "revision-2")).await?;
    let revised = claim(&pool, "classifier").await?.remove(0);
    assert_eq!(revised.attempts, 0);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT input_version FROM coverage_failures")
            .fetch_one(&pool)
            .await?,
        "revision-1"
    );
    restarted.close().await;
    finish(&pool).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires dedicated migrated classifier_test_coverage database; run serially"]
async fn failed_characters_skip_dependents_finish_other_work_and_reopen_without_publication(
) -> Result<()> {
    let pool = setup().await?;
    work::enqueue(&pool, &item("rating", 101, "stats-v1")).await?;
    work::enqueue(&pool, &item("rating", 102, "healthy-v1")).await?;
    enroll(&pool).await?;
    tick(&pool, 0.0).await?;
    tick(&pool, 0.0).await?;
    sqlx::raw_sql("INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,status)
        SELECT edition_date-1,cutoff_at-interval '1 day',drain_deadline-interval '1 day','complete' FROM coverage_editions;
        UPDATE coverage_publication SET edition_date=(SELECT min(edition_date) FROM coverage_editions),revision=revision+1;")
        .execute(&pool).await?;
    let publication: (String, i64) =
        sqlx::query_as("SELECT edition_date::text,revision FROM coverage_publication")
            .fetch_one(&pool)
            .await?;
    let mut ratings = claim(&pool, "rating").await?;
    let healthy = ratings.remove(ratings.iter().position(|i| i.entity_id == 102).unwrap());
    let mut failing = ratings.pop().unwrap();
    for attempt in 1..=work::MAX_ATTEMPTS {
        assert_eq!(failing.attempts, attempt - 1);
        work::fail(
            &pool,
            &failing,
            "unsupported model claim",
            Duration::ZERO,
            work::MAX_ATTEMPTS,
        )
        .await?;
        if attempt < work::MAX_ATTEMPTS {
            assert_eq!(tick(&pool, 0.0).await?, "characters");
            failing = claim(&pool, "rating").await?.remove(0);
        }
    }
    work::enqueue(&pool, &item("rating", 101, "stats-v1")).await?;
    assert_eq!(
        tick(&pool, 0.0).await?,
        "characters",
        "healthy work must finish first"
    );
    assert!(claim(&pool, "momentum").await?.is_empty());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM coverage_failures WHERE entity_id=101")
            .fetch_one(&pool)
            .await?,
        3
    );
    let restarted = PgPoolOptions::new()
        .connect(&std::env::var("TEST_DATABASE_URL")?)
        .await?;
    ClaimPublication::begin(&restarted, &healthy)
        .await?
        .unwrap()
        .commit_final()
        .await?;
    let current = claim(&restarted, "momentum").await?.remove(0);
    assert_eq!(current.entity_id, 102);
    let mut publication_tx = ClaimPublication::begin(&restarted, &current)
        .await?
        .unwrap();
    sqlx::query("INSERT INTO momentum_summaries(entity_type,entity_id,sport,season,blurb,model_version,prompt_version)
        VALUES('team',102,'ZZ_COVERAGE',2026,'Healthy candidate.','fixture','bounded-failures')")
        .execute(&mut **publication_tx.transaction()).await?;
    publication_tx.commit_final().await?;
    let sigil = claim(&restarted, "sigil").await?.remove(0);
    ClaimPublication::begin(&restarted, &sigil)
        .await?
        .unwrap()
        .commit_final()
        .await?;
    assert_eq!(tick(&restarted, 0.0).await?, "coverage");
    assert_eq!(
        sqlx::query_as::<_, (String, i64)>(
            "SELECT edition_date::text,revision FROM coverage_publication"
        )
        .fetch_one(&pool)
        .await?,
        publication
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM qualified_momentum_summaries WHERE sport='ZZ_COVERAGE'"
        )
        .fetch_one(&pool)
        .await?,
        0,
        "partial candidates stay hidden"
    );
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT coverage_collection_open()")
            .fetch_one(&pool)
            .await?
    );
    work::enqueue(&pool, &item("rating", 101, "stats-v1")).await?;
    assert_eq!(sqlx::query_scalar::<_,i32>("SELECT attempts FROM pipeline_work WHERE sport='ZZ_COVERAGE' AND entity_id=101 AND stage='rating'").fetch_one(&pool).await?,5);
    work::enqueue(&pool, &item("rating", 101, "stats-v2")).await?;
    assert_eq!(sqlx::query_scalar::<_,i32>("SELECT attempts FROM pipeline_work WHERE sport='ZZ_COVERAGE' AND entity_id=101 AND stage='rating'").fetch_one(&pool).await?,0);
    assert_eq!(
        sqlx::query_scalar::<_, i32>(
            "SELECT attempts FROM coverage_failures WHERE kind='character_exhausted'"
        )
        .fetch_one(&pool)
        .await?,
        5
    );
    restarted.close().await;
    finish(&pool).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires dedicated migrated classifier_test_coverage database; run serially"]
async fn exhausted_handoffs_stay_reportable_without_stalling_collection() -> Result<()> {
    let pool = setup().await?;
    enroll(&pool).await?;
    tick(&pool, 0.0).await?;
    tick(&pool, 0.0).await?;
    sqlx::query("INSERT INTO application_outbox(kind,source_stage,source_claim_token,entity_type,entity_id,sport,edition_date,attempts,last_error)
        SELECT 'vibe_completed','vibe',gen_random_uuid(),'team',101,'ZZ_COVERAGE',edition_date,4,'handoff unavailable' FROM coverage_schedule")
        .execute(&pool).await?;
    assert_eq!(tick(&pool, 0.0).await?, "characters");
    sqlx::query("UPDATE application_outbox SET attempts=5 WHERE sport='ZZ_COVERAGE'")
        .execute(&pool)
        .await?;
    assert_eq!(tick(&pool, 0.0).await?, "coverage");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM application_outbox WHERE attempts=5")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT kind FROM coverage_failures")
            .fetch_one(&pool)
            .await?,
        "handoff_exhausted"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM coverage_editions")
            .fetch_one(&pool)
            .await?,
        "partial"
    );
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
