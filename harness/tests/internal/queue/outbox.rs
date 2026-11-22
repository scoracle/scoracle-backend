use super::*;
use sqlx::PgPool;

const SPORT: &str = "ZZ_OUTBOX_RECOVERY";

async fn pool() -> PgPool {
    crate::harness::dbtest::pool_with_limit(4).await.unwrap()
}

async fn clean(pool: &PgPool) {
    for table in ["application_outbox", "pipeline_work"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE sport = $1"))
            .bind(SPORT)
            .execute(pool)
            .await
            .unwrap();
    }
}

fn reactions(pool: &PgPool) -> ReactionRegistry {
    crate::harness::registration::build_reactions(pool.clone()).unwrap()
}

async fn committed_obligation(pool: &PgPool) -> Event {
    clean(pool).await;
    sqlx::query("INSERT INTO sports (id, display_name, current_season) VALUES ($1, 'Outbox recovery test', 2026) ON CONFLICT DO NOTHING")
        .bind(SPORT).execute(pool).await.unwrap();
    // Recording uses the same production statement as atomic seat publication.
    let item = Item {
        stage: crate::harness::skills::analyst::manifest::TASK,
        entity_type: "team".into(),
        entity_id: 9_600_001,
        sport: SPORT.into(),
        input_version: Some("revision".into()),
        attempts: 0,
        claim_token: Some("00000000-0000-4000-8000-000000000001".into()),
    };
    let mut tx = pool.begin().await.unwrap();
    crate::harness::skills::analyst::record_momentum_completed(&mut tx, &item)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let id = sqlx::query_scalar("SELECT id::text FROM application_outbox WHERE sport = $1")
        .bind(SPORT)
        .fetch_one(pool)
        .await
        .unwrap();
    Event {
        id,
        kind: crate::harness::skills::analyst::MOMENTUM_COMPLETED.into(),
        entity_type: item.entity_type,
        entity_id: item.entity_id as i32,
        sport: item.sport,
        source_input_version: item.input_version,
        attempts: 0,
    }
}

#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn postgres_dispatch_replays_after_dispatch_before_ack_without_a_model_runtime() {
    let pool = crate::harness::dbtest::pool_with_limit(crate::harness::db::MIN_WORKER_CONNECTIONS)
        .await
        .unwrap();
    let mut listener = sqlx::postgres::PgListener::connect_with(&pool)
        .await
        .unwrap();
    listener.listen("pipeline_work_ready").await.unwrap();
    let event = committed_obligation(&pool).await;
    // Simulate dispatch committed, then process death before event acknowledgement.
    let reactions = reactions(&pool);
    reactions.dispatch(&event).await.unwrap();
    let original: (String, String) = sqlx::query_as(
        "SELECT available_at::text, input_version FROM pipeline_work WHERE sport = $1 AND stage = 'sigil'"
    ).bind(SPORT).fetch_one(&pool).await.unwrap();
    drop(event);
    assert_eq!(drain(&pool, &reactions, 100).await.unwrap(), 1);
    let replayed: (String, String) = sqlx::query_as(
        "SELECT available_at::text, input_version FROM pipeline_work WHERE sport = $1 AND stage = 'sigil'"
    ).bind(SPORT).fetch_one(&pool).await.unwrap();
    assert_eq!(
        replayed, original,
        "replay must coalesce without changing FIFO or revision"
    );
    assert_eq!(drain(&pool, &reactions, 100).await.unwrap(), 0);
    clean(&pool).await;
}

#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn postgres_dispatch_failure_backs_off_and_recovers_the_durable_event() {
    let pool = pool().await;
    let event = committed_obligation(&pool).await;
    // Fail only this test subject's downstream enqueue, leaving other suites untouched.
    sqlx::raw_sql("CREATE OR REPLACE FUNCTION test_outbox_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.sport = 'ZZ_OUTBOX_RECOVERY' THEN RAISE EXCEPTION 'injected outbox enqueue failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_outbox_failure BEFORE INSERT ON pipeline_work FOR EACH ROW EXECUTE FUNCTION test_outbox_failure();")
        .execute(&pool).await.unwrap();
    let reactions = reactions(&pool);
    let result = drain(&pool, &reactions, 100).await;
    sqlx::raw_sql(
        "DROP TRIGGER test_outbox_failure ON pipeline_work; DROP FUNCTION test_outbox_failure();",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(result.unwrap(), 0);
    let state: (i32, bool, String) = sqlx::query_as(
        "SELECT attempts, available_at > NOW(), last_error FROM application_outbox WHERE id = $1::uuid"
    ).bind(&event.id).fetch_one(&pool).await.unwrap();
    assert_eq!(state.0, 1);
    assert!(state.1);
    assert!(state.2.contains("injected outbox enqueue failure"));
    sqlx::query("UPDATE application_outbox SET available_at = NOW() WHERE id = $1::uuid")
        .bind(&event.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(drain(&pool, &reactions, 100).await.unwrap(), 1);
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE sport = $1 AND stage = 'sigil' AND status = 'pending'")
        .bind(SPORT).fetch_one(&pool).await.unwrap();
    assert_eq!(pending, 1);
    clean(&pool).await;
}

#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn applied_identity_rating_fanout_is_atomic_and_dispatchable() {
    let pool = pool().await;
    clean(&pool).await;
    sqlx::query("INSERT INTO sports (id, display_name, current_season) VALUES ($1, 'Outbox recovery test', 2026) ON CONFLICT DO NOTHING")
        .bind(SPORT).execute(&pool).await.unwrap();
    let item = Item {
        stage: crate::harness::skills::insider::manifest::TASK,
        entity_type: "team".into(),
        entity_id: 9_600_001,
        sport: SPORT.into(),
        input_version: Some("team-revision".into()),
        attempts: 0,
        claim_token: Some("00000000-0000-4000-8000-000000000002".into()),
    };
    let rating_version = "rating:s2026:transfer:77";

    let mut rolled_back = pool.begin().await.unwrap();
    crate::harness::skills::insider::record_transfer_event(
        &mut rolled_back,
        &item,
        crate::harness::skills::scout::TRANSFER_IDENTITY_APPLIED,
        "player",
        9_600_002,
        Some(rating_version),
    )
    .await
    .unwrap();
    rolled_back.rollback().await.unwrap();
    let absent: i64 =
        sqlx::query_scalar("SELECT count(*) FROM application_outbox WHERE sport=$1 AND kind=$2")
            .bind(SPORT)
            .bind(crate::harness::skills::scout::TRANSFER_IDENTITY_APPLIED)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(absent, 0);

    let mut committed = pool.begin().await.unwrap();
    for (entity_type, entity_id) in [
        ("player", 9_600_002),
        ("team", 9_600_001),
        ("team", 9_600_003),
    ] {
        crate::harness::skills::insider::record_transfer_event(
            &mut committed,
            &item,
            crate::harness::skills::scout::TRANSFER_IDENTITY_APPLIED,
            entity_type,
            entity_id,
            Some(rating_version),
        )
        .await
        .unwrap();
    }
    committed.commit().await.unwrap();
    let reactions = reactions(&pool);
    assert_eq!(drain(&pool, &reactions, 100).await.unwrap(), 3);
    let work: Vec<(String, i32, String)> = sqlx::query_as(
        "SELECT entity_type, entity_id, input_version FROM pipeline_work WHERE sport=$1 AND stage='rating' ORDER BY entity_type,entity_id",
    )
    .bind(SPORT)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        work,
        vec![
            ("player".into(), 9_600_002, rating_version.into()),
            ("team".into(), 9_600_001, rating_version.into()),
            ("team".into(), 9_600_003, rating_version.into()),
        ]
    );
    clean(&pool).await;
}

#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn process_crash_dispatch_rehearsal() {
    const CHILD: &str = "SCORACLE_OUTBOX_CRASH";
    if std::env::var_os(CHILD).is_some() {
        let pool = pool().await;
        let mut tx = pool.begin().await.unwrap();
        let id: String =
            sqlx::query_scalar("SELECT id::text FROM application_outbox WHERE sport=$1 FOR UPDATE")
                .bind(SPORT)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        let event = Event {
            id,
            kind: crate::harness::skills::analyst::MOMENTUM_COMPLETED.into(),
            entity_type: "team".into(),
            entity_id: 9_600_001,
            sport: SPORT.into(),
            source_input_version: Some("revision".into()),
            attempts: 0,
        };
        reactions(&pool).dispatch(&event).await.unwrap();
        std::process::exit(86);
    }
    let pool = pool().await;
    committed_obligation(&pool).await;
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "harness::queue::outbox::postgres_recovery_tests::process_crash_dispatch_rehearsal",
            "--ignored",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(86));
    let before: (String, String) =
        sqlx::query_as("SELECT available_at::text,input_version FROM pipeline_work WHERE sport=$1")
            .bind(SPORT)
            .fetch_one(&pool)
            .await
            .unwrap();
    let reactions = reactions(&pool);
    assert_eq!(drain(&pool, &reactions, 1).await.unwrap(), 1);
    let after: (String, String) =
        sqlx::query_as("SELECT available_at::text,input_version FROM pipeline_work WHERE sport=$1")
            .bind(SPORT)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    clean(&pool).await;
}
