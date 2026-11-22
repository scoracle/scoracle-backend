use super::*;

async fn pool() -> PgPool {
    crate::harness::dbtest::pool_with_limit(2)
        .await
        .expect("disposable queue database")
}

fn pending(stage: TaskKey, entity_id: i64, sport: &str, revision: &str) -> Item {
    Item {
        stage,
        entity_type: "team".to_string(),
        entity_id,
        sport: sport.to_string(),
        input_version: Some(revision.to_string()),
        attempts: 0,
        claim_token: None,
    }
}

async fn clean(pool: &PgPool, sport: &str) {
    sqlx::query("DELETE FROM pipeline_work WHERE sport = $1")
        .bind(sport)
        .execute(pool)
        .await
        .expect("clean claim-fencing test rows");
}

async fn complete(pool: &PgPool, item: &Item) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let completed = complete_in_transaction(&mut tx, item).await?;
    tx.commit().await?;
    Ok(completed)
}

async fn one_claim(pool: &PgPool, stage: TaskKey) -> Item {
    let mut items = claim_registered(pool, stage, 1)
        .await
        .expect("claim test row");
    assert_eq!(items.len(), 1, "isolated test stage should have one row");
    items.remove(0)
}

async fn claim_registered(pool: &PgPool, stage: TaskKey, limit: i64) -> Result<Vec<Item>> {
    if matches!(
        stage.as_str(),
        "test_article" | "fixture_boxscore" | "investigate_entity"
    ) {
        return claim(pool, stage, limit).await;
    }
    let policy = crate::harness::fleet::skills()
        .find(|manifest| manifest.task == stage)
        .map(|manifest| manifest.claim_policy)
        .expect("test stage is registered in the first-party fleet");
    claim_with_policy(pool, stage, policy, limit).await
}

#[tokio::test]
#[ignore = "requires isolated migrated TEST_DATABASE_URL"]
async fn competing_workers_receive_notify_and_claim_disjoint_jobs() {
    let first = pool().await;
    let second = pool().await;
    let sport = "ZZ_SHARED_WORKERS";
    clean(&first, sport).await;
    let mut listener_a = sqlx::postgres::PgListener::connect_with(&first)
        .await
        .unwrap();
    let mut listener_b = sqlx::postgres::PgListener::connect_with(&second)
        .await
        .unwrap();
    listener_a.listen("pipeline_work_ready").await.unwrap();
    listener_b.listen("pipeline_work_ready").await.unwrap();
    for id in 9_100_100..9_100_120 {
        enqueue(
            &first,
            &pending(TaskKey::new("test_article"), id, sport, "v1"),
        )
        .await
        .unwrap();
    }
    for listener in [&mut listener_a, &mut listener_b] {
        tokio::time::timeout(Duration::from_secs(2), listener.recv())
            .await
            .unwrap()
            .unwrap();
    }
    let (a, b) = tokio::join!(
        claim(&first, TaskKey::new("test_article"), 10),
        claim(&second, TaskKey::new("test_article"), 10)
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.len(), 10);
    assert_eq!(b.len(), 10);
    for job in &a {
        assert!(!b.iter().any(|other| other.entity_id == job.entity_id));
    }
    assert!(claim(&second, TaskKey::new("test_article"), 1)
        .await
        .unwrap()
        .is_empty());
    for job in a.iter().chain(&b) {
        assert!(complete(&first, job).await.unwrap());
    }
    clean(&first, sport).await;
}

#[tokio::test]
#[ignore = "requires isolated migrated TEST_DATABASE_URL"]
async fn character_budget_is_four_across_hosts_and_roles() {
    let first = pool().await;
    let second = pool().await;
    let sport = "ZZ_SHARED_BUDGET";
    clean(&first, sport).await;
    for id in 9_100_200..9_100_205 {
        for stage in [
            crate::harness::skills::scout::manifest::TASK,
            crate::harness::skills::influencer::manifest::TASK,
        ] {
            enqueue(&first, &pending(stage, id, sport, "v1"))
                .await
                .unwrap();
        }
    }
    let (a, b) = tokio::join!(
        claim(&first, crate::harness::skills::scout::manifest::TASK, 4),
        claim(
            &second,
            crate::harness::skills::influencer::manifest::TASK,
            4
        )
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.len() + b.len(), 4);
    assert!(
        claim(&first, crate::harness::skills::scout::manifest::TASK, 1)
            .await
            .unwrap()
            .is_empty()
    );
    for job in a.iter().chain(&b) {
        assert!(complete(&first, job).await.unwrap());
    }
    assert!(!claim(
        &second,
        crate::harness::skills::influencer::manifest::TASK,
        1
    )
    .await
    .unwrap()
    .is_empty());
    clean(&first, sport).await;
}

#[tokio::test]
#[ignore = "requires isolated migrated TEST_DATABASE_URL"]
async fn continuous_claims_gate_momentum_on_its_entity_and_handoff() {
    use crate::harness::skills::{analyst, oracle, scout};

    let first = pool().await;
    let second = pool().await;
    let sport = "ZZ_SHARED_DEPENDENCIES";
    clean(&first, sport).await;
    sqlx::query("INSERT INTO sports (id, display_name, current_season) VALUES ($1, 'Shared workers', 2026) ON CONFLICT DO NOTHING")
        .bind(sport).execute(&first).await.unwrap();
    let id = 9_100_130;
    for stage in [
        scout::manifest::TASK,
        analyst::manifest::TASK,
        oracle::manifest::TASK,
    ] {
        enqueue(&first, &pending(stage, id, sport, "v1"))
            .await
            .unwrap();
    }
    assert!(claim_registered(&second, analyst::manifest::TASK, 1)
        .await
        .unwrap()
        .is_empty());

    // Continuous Oracle reads finished cards even while other work is pending.
    let oracle = one_claim(&second, oracle::manifest::TASK).await;
    assert_eq!(oracle.entity_id, id);
    assert!(complete(&second, &oracle).await.unwrap());

    let rating = one_claim(&first, scout::manifest::TASK).await;
    enqueue(
        &first,
        &pending(analyst::manifest::TASK, id + 1, sport, "v1"),
    )
    .await
    .unwrap();
    let independent = one_claim(&second, analyst::manifest::TASK).await;
    assert_eq!(independent.entity_id, id + 1);
    assert!(complete(&second, &independent).await.unwrap());
    let mut tx = first.begin().await.unwrap();
    scout::record_rating_completed(&mut tx, &rating, true)
        .await
        .unwrap();
    assert!(complete_in_transaction(&mut tx, &rating).await.unwrap());
    tx.commit().await.unwrap();
    assert!(claim_registered(&second, analyst::manifest::TASK, 1)
        .await
        .unwrap()
        .is_empty());

    // Isolate dispatch acknowledgement from model/context preparation here.
    sqlx::query("DELETE FROM application_outbox WHERE sport = $1")
        .bind(sport)
        .execute(&first)
        .await
        .unwrap();
    let momentum = one_claim(&second, analyst::manifest::TASK).await;
    assert_eq!(momentum.entity_id, id);
    assert!(complete(&second, &momentum).await.unwrap());
    clean(&first, sport).await;
}

#[tokio::test]
#[ignore = "requires isolated migrated TEST_DATABASE_URL"]
async fn crashed_claims_consume_retries_and_duplicate_offers_cannot_reset_them() {
    let pool = pool().await;
    let sport = "ZZ_CRASH_BUDGET";
    clean(&pool, sport).await;
    let offered = pending(TaskKey::new("test_article"), 9_100_001, sport, "v1");
    enqueue(&pool, &offered).await.unwrap();
    for attempt in 1..=MAX_ATTEMPTS {
        let current = one_claim(&pool, TaskKey::new("test_article")).await;
        assert_eq!(current.attempts, attempt - 1);
        sqlx::query("UPDATE pipeline_work SET updated_at=now()-interval '1 hour' WHERE sport=$1")
            .bind(sport)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            requeue_stale(&pool, Duration::from_secs(60)).await.unwrap(),
            1
        );
        enqueue(&pool, &offered).await.unwrap();
    }
    assert!(claim(&pool, TaskKey::new("test_article"), 1)
        .await
        .unwrap()
        .is_empty());
    let state: (String, i32) =
        sqlx::query_as("SELECT status,attempts FROM pipeline_work WHERE sport=$1")
            .bind(sport)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, ("failed".into(), 5));
    enqueue(
        &pool,
        &pending(TaskKey::new("test_article"), 9_100_001, sport, "v2"),
    )
    .await
    .unwrap();
    assert_eq!(
        one_claim(&pool, TaskKey::new("test_article"))
            .await
            .attempts,
        0
    );
    clean(&pool, sport).await;
}

#[tokio::test]
#[ignore = "requires isolated migrated TEST_DATABASE_URL"]
async fn reclaimed_lease_fences_every_older_acknowledgement() {
    let pool = pool().await;
    let sport = "ZZ_RUST_STALE_CLAIM";
    clean(&pool, sport).await;

    let offered = pending(TaskKey::new("test_article"), 9_100_001, sport, "v1");
    enqueue(&pool, &offered).await.expect("enqueue v1");
    let old = one_claim(&pool, TaskKey::new("test_article")).await;

    sqlx::query(
        "UPDATE pipeline_work SET updated_at = NOW() - INTERVAL '1 hour' \
         WHERE stage = 'test_article' AND entity_id = $1 AND sport = $2",
    )
    .bind(offered.entity_id)
    .bind(sport)
    .execute(&pool)
    .await
    .expect("age old claim");
    assert_eq!(
        requeue_stale(&pool, Duration::from_secs(30 * 60))
            .await
            .expect("recover old claim"),
        1
    );
    let current = one_claim(&pool, TaskKey::new("test_article")).await;
    assert_ne!(old.claim_token, current.claim_token);

    assert!(!complete(&pool, &old).await.expect("stale complete"));
    assert!(!fail(&pool, &old, "late", Duration::ZERO, MAX_ATTEMPTS)
        .await
        .expect("stale fail"));
    assert!(!defer(&pool, &old, Duration::ZERO, "late")
        .await
        .expect("stale defer"));
    assert!(!release(&pool, &old).await.expect("stale release"));
    assert!(complete(&pool, &current).await.expect("current complete"));

    clean(&pool, sport).await;
}

#[tokio::test]
#[ignore = "requires isolated migrated TEST_DATABASE_URL"]
async fn newer_revision_survives_older_execution_and_becomes_next_claim() {
    let pool = pool().await;
    let sport = "ZZ_RUST_RUNNING_REVISION";
    clean(&pool, sport).await;

    let v1 = pending(
        crate::harness::skills::fixture_boxscore::manifest::TASK,
        9_100_002,
        sport,
        "v1",
    );
    enqueue(&pool, &v1).await.expect("enqueue v1");
    let old = one_claim(
        &pool,
        crate::harness::skills::fixture_boxscore::manifest::TASK,
    )
    .await;

    let v2 = pending(
        crate::harness::skills::fixture_boxscore::manifest::TASK,
        v1.entity_id,
        sport,
        "v2",
    );
    enqueue(&pool, &v2).await.expect("enqueue v2 during v1");
    assert!(!complete(&pool, &old).await.expect("stale v1 complete"));
    assert!(!fail(&pool, &old, "late v1", Duration::ZERO, MAX_ATTEMPTS)
        .await
        .expect("stale v1 fail"));

    let current = one_claim(
        &pool,
        crate::harness::skills::fixture_boxscore::manifest::TASK,
    )
    .await;
    assert_eq!(current.input_version.as_deref(), Some("v2"));
    assert_ne!(old.claim_token, current.claim_token);
    assert!(complete(&pool, &current).await.expect("complete v2"));

    clean(&pool, sport).await;
}
