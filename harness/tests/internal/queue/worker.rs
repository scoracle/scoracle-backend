use super::test_support::TEST_MANIFEST;
use super::*;
use crate::harness::queue::work::Item;
use crate::harness::skill::{Skill, SkillOutcome};

struct Parallel {
    pool: PgPool,
    barrier: tokio::sync::Barrier,
    active: std::sync::atomic::AtomicUsize,
    maximum: std::sync::atomic::AtomicUsize,
}
#[async_trait::async_trait]
impl Skill for Parallel {
    fn manifest(&self) -> &'static crate::harness::skill::SkillManifest {
        const MANIFEST: crate::harness::skill::SkillManifest =
            crate::harness::skill::SkillManifest {
                plugin_id: "test",
                id: crate::harness::skill::SkillId::new("test.parallel"),
                task: crate::harness::skills::journalist::manifest::TASK,
                claim_policy: crate::harness::queue::work::ClaimPolicy::FIFO,
                inference_routes: &[],
                resources: crate::harness::skill::ResourceProfile::grouped(2, ("parallel", 2)),
                tools: &[],
            };
        &MANIFEST
    }
    async fn execute(&self, item: &Item) -> Result<SkillOutcome> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum.fetch_max(active, Ordering::SeqCst);
        self.barrier.wait().await;
        let mut tx = self.pool.begin().await?;
        assert!(work::lock_claim(&mut tx, item).await?);
        assert!(work::complete_in_transaction(&mut tx, item).await?);
        tx.commit().await?;
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(SkillOutcome::Committed)
    }
}

#[tokio::test]
#[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
async fn a_single_ready_stage_fills_its_capacity_without_exceeding_it() {
    let pool = fixture("ZZ_PARALLEL").await;
    let handler = Arc::new(Parallel {
        pool: pool.clone(),
        barrier: tokio::sync::Barrier::new(2),
        active: std::sync::atomic::AtomicUsize::new(0),
        maximum: std::sync::atomic::AtomicUsize::new(0),
    });
    for id in 9_600_200..9_600_204 {
        work::enqueue(
            &pool,
            &item(
                crate::harness::skills::journalist::manifest::TASK,
                "ZZ_PARALLEL",
                id,
            ),
        )
        .await
        .unwrap();
    }
    let worker = Worker::new(
        pool,
        vec![crate::harness::skill::tests::test_plugin(vec![
            handler.clone()
        ])],
        crate::harness::queue::outbox::ReactionRegistry::empty(),
        Duration::from_secs(60),
        Duration::from_secs(1800),
        Duration::from_secs(5),
        Duration::ZERO,
        Some(4),
    );
    tokio::time::timeout(
        Duration::from_secs(3),
        worker.drain_all("test", &Pulse::new()),
    )
    .await
    .unwrap();
    assert_eq!(handler.maximum.load(Ordering::SeqCst), 2);
}

async fn fixture(sport: &str) -> PgPool {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES ($1,$1,2026) ON CONFLICT DO NOTHING")
        .bind(sport).execute(&pool).await.unwrap();
    for table in ["pipeline_work", "application_outbox"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE sport=$1"))
            .bind(sport)
            .execute(&pool)
            .await
            .unwrap();
    }
    pool
}

fn item(stage: TaskKey, sport: &str, id: i64) -> Item {
    Item {
        stage,
        sport: sport.into(),
        entity_type: "team".into(),
        entity_id: id,
        input_version: Some("v1".into()),
        attempts: 0,
        claim_token: None,
    }
}

struct Refilling {
    pool: PgPool,
    stage: TaskKey,
    shared: bool,
    order: Arc<StdMutex<Vec<TaskKey>>>,
}

const fn test_manifest(
    id: &'static str,
    task: TaskKey,
    shared: bool,
) -> crate::harness::skill::SkillManifest {
    crate::harness::skill::SkillManifest {
        plugin_id: "test",
        id: crate::harness::skill::SkillId::new(id),
        task,
        claim_policy: crate::harness::queue::work::ClaimPolicy::FIFO,
        inference_routes: &[],
        resources: crate::harness::skill::ResourceProfile {
            max_in_flight: 1,
            slot_group: if shared {
                Some(("test-shared", 1))
            } else {
                None
            },
            rotation_batch: 1,
        },
        tools: &[],
    }
}

const NARRATIVES_TEST: crate::harness::skill::SkillManifest = test_manifest(
    "test.narratives",
    crate::harness::skills::journalist::manifest::TASK,
    false,
);
const RATING_TEST: crate::harness::skill::SkillManifest = test_manifest(
    "test.rating",
    crate::harness::skills::scout::manifest::TASK,
    false,
);
const NARRATIVES_SHARED_TEST: crate::harness::skill::SkillManifest = test_manifest(
    "test.narratives",
    crate::harness::skills::journalist::manifest::TASK,
    true,
);
const RATING_SHARED_TEST: crate::harness::skill::SkillManifest = test_manifest(
    "test.rating",
    crate::harness::skills::scout::manifest::TASK,
    true,
);

impl Refilling {
    fn manifest(&self) -> &'static crate::harness::skill::SkillManifest {
        match (self.stage, self.shared) {
            (crate::harness::skills::journalist::manifest::TASK, false) => &NARRATIVES_TEST,
            (crate::harness::skills::scout::manifest::TASK, false) => &RATING_TEST,
            (crate::harness::skills::journalist::manifest::TASK, true) => &NARRATIVES_SHARED_TEST,
            (crate::harness::skills::scout::manifest::TASK, true) => &RATING_SHARED_TEST,
            _ => unreachable!("test double stages"),
        }
    }
}

#[async_trait::async_trait]
impl Skill for Refilling {
    fn manifest(&self) -> &'static crate::harness::skill::SkillManifest {
        Refilling::manifest(self)
    }
    async fn execute(&self, item: &Item) -> Result<SkillOutcome> {
        let count = {
            let mut order = self.order.lock().unwrap();
            order.push(self.stage);
            order.iter().filter(|s| **s == self.stage).count()
        };
        let mut tx = self.pool.begin().await?;
        assert!(work::lock_claim(&mut tx, item).await?);
        assert!(work::complete_in_transaction(&mut tx, item).await?);
        // Keep the first stage continuously ready while a later stage waits.
        if self.stage == crate::harness::skills::journalist::manifest::TASK && count < 20 {
            work::enqueue(&mut *tx, item).await?;
        }
        tx.commit().await?;
        Ok(SkillOutcome::Committed)
    }
}

#[tokio::test]
#[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
async fn sustained_inflow_cannot_starve_later_stages_at_global_or_shared_capacity() {
    for shared in [false, true] {
        let sport = if shared {
            "ZZ_FAIR_SHARED"
        } else {
            "ZZ_FAIR_GLOBAL"
        };
        let pool = fixture(sport).await;
        let order = Arc::new(StdMutex::new(Vec::new()));
        let handlers: Vec<std::sync::Arc<dyn Skill>> = [
            crate::harness::skills::journalist::manifest::TASK,
            crate::harness::skills::scout::manifest::TASK,
        ]
        .into_iter()
        .map(|stage| {
            std::sync::Arc::new(Refilling {
                pool: pool.clone(),
                stage,
                shared,
                order: order.clone(),
            }) as std::sync::Arc<dyn Skill>
        })
        .collect();
        for stage in [
            crate::harness::skills::journalist::manifest::TASK,
            crate::harness::skills::scout::manifest::TASK,
        ] {
            work::enqueue(&pool, &item(stage, sport, 9_600_100))
                .await
                .unwrap();
        }
        let worker = Worker::new(
            pool.clone(),
            vec![crate::harness::skill::tests::test_plugin(handlers)],
            crate::harness::queue::outbox::ReactionRegistry::empty(),
            Duration::from_secs(60),
            Duration::from_secs(1800),
            Duration::from_secs(5),
            Duration::ZERO,
            Some(if shared { 2 } else { 1 }),
        );
        tokio::time::timeout(
            Duration::from_secs(5),
            worker.drain_all("test", &Pulse::new()),
        )
        .await
        .unwrap();
        let order = order.lock().unwrap();
        assert_eq!(order.len(), 21);
        assert_eq!(
            order[1],
            crate::harness::skills::scout::manifest::TASK,
            "a continuously ready earlier stage monopolized capacity: {order:?}"
        );
    }
}

struct PublicationHeld {
    pool: PgPool,
    ready: tokio::sync::mpsc::UnboundedSender<i64>,
    release: [Arc<Notify>; 2],
}
static PUBLICATION_HELD: crate::harness::skill::SkillManifest = {
    let mut manifest = test_manifest("test.publication-held", TaskKey::new("test_article"), false);
    manifest.resources.max_in_flight = 2;
    manifest.resources.rotation_batch = 2;
    manifest
};
#[async_trait::async_trait]
impl Skill for PublicationHeld {
    fn manifest(&self) -> &'static crate::harness::skill::SkillManifest {
        &PUBLICATION_HELD
    }
    async fn execute(&self, item: &Item) -> Result<SkillOutcome> {
        let mut tx = self.pool.begin().await?;
        assert!(work::lock_claim(&mut tx, item).await?);
        self.ready.send(item.entity_id)?;
        self.release[(item.entity_id - 1) as usize].notified().await;
        assert!(work::complete_in_transaction(&mut tx, item).await?);
        tx.commit().await?;
        Ok(SkillOutcome::Committed)
    }
}

#[tokio::test]
#[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
async fn coverage_transition_cannot_stop_polling_an_inflight_publication() {
    let sport = "ZZ_CLAIM_COVERAGE_LOCK";
    let pool = fixture(sport).await;
    let (ready, mut entered) = tokio::sync::mpsc::unbounded_channel();
    let release = [Arc::new(Notify::new()), Arc::new(Notify::new())];
    for id in [1, 2] {
        work::enqueue(&pool, &item(PUBLICATION_HELD.task, sport, id))
            .await
            .unwrap();
    }
    let worker = Worker::new(
        pool.clone(),
        vec![crate::harness::skill::tests::test_plugin(vec![Arc::new(
            PublicationHeld {
                pool: pool.clone(),
                ready,
                release: release.clone(),
            },
        )])],
        crate::harness::queue::outbox::ReactionRegistry::empty(),
        Duration::from_secs(60),
        Duration::from_secs(1800),
        Duration::ZERO,
        Duration::ZERO,
        Some(2),
    );
    let prove = async {
        entered.recv().await.unwrap();
        entered.recv().await.unwrap();
        let mut transition = pool.begin().await.unwrap();
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *transition)
            .await
            .unwrap();
        let controller = tokio::spawn(async move {
            sqlx::query("SELECT pg_advisory_xact_lock(1935896421,1)")
                .execute(&mut *transition)
                .await
                .unwrap();
            transition.commit().await.unwrap();
        });
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND locktype='advisory' AND NOT granted)")
                .bind(pid).fetch_one(&pool).await.unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        release[1].notify_one();
        // Its replacement claim queues behind the controller, which awaits the first publication.
        loop {
            let waiting: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND classid=1935896421 AND objid=1 AND NOT granted")
                .fetch_one(&pool).await.unwrap();
            if waiting >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        release[0].notify_one();
        controller.await.unwrap();
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        let pulse = Pulse::new();
        tokio::join!(worker.drain_all("coverage contention", &pulse), prove);
    })
    .await
    .expect("claiming stopped polling the publication that must release coverage");
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE sport=$1")
        .bind(sport)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
}

struct Blocked {
    pool: PgPool,
    entered: Arc<Notify>,
    release: Arc<Notify>,
}
#[async_trait::async_trait]
impl Skill for Blocked {
    fn manifest(&self) -> &'static crate::harness::skill::SkillManifest {
        &TEST_MANIFEST
    }
    async fn execute(&self, item: &Item) -> Result<SkillOutcome> {
        self.entered.notify_one();
        self.release.notified().await;
        let mut tx = self.pool.begin().await?;
        assert!(work::lock_claim(&mut tx, item).await?);
        assert!(work::complete_in_transaction(&mut tx, item).await?);
        tx.commit().await?;
        Ok(SkillOutcome::Committed)
    }
}

#[tokio::test]
#[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
async fn handoffs_dispatch_while_a_claimed_handler_cannot_finish() {
    let sport = "ZZ_BUSY_OUTBOX";
    let pool = fixture(sport).await;
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let worker = Worker::new(
        pool.clone(),
        vec![crate::harness::skill::tests::test_plugin(vec![
            std::sync::Arc::new(Blocked {
                pool: pool.clone(),
                entered: entered.clone(),
                release: release.clone(),
            }),
            std::sync::Arc::new(Terminal(pool.clone())),
        ])],
        crate::harness::registration::build_reactions(pool.clone()).unwrap(),
        Duration::from_secs(60),
        Duration::from_secs(1800),
        Duration::from_secs(10),
        Duration::ZERO,
        Some(1),
    );
    work::enqueue(&pool, &item(TaskKey::new("test_article"), sport, 9_600_101))
        .await
        .unwrap();
    let pulse = Pulse::new();
    let drain = worker.drain_all("busy", &pulse);
    let tick = Notify::new();
    let dispatcher = outbox_loop(&pool, &worker.reactions, &tick, &worker.shutdown);
    tokio::pin!(drain, dispatcher);
    let prove = async {
        entered.notified().await;
        // Arrives after the drain began: startup-only dispatch cannot pass.
        let mut source = item(
            crate::harness::skills::analyst::manifest::TASK,
            sport,
            9_600_102,
        );
        source.claim_token = Some("00000000-0000-4000-8000-000000000103".into());
        let mut tx = pool.begin().await.unwrap();
        crate::harness::skills::analyst::record_momentum_completed(&mut tx, &source)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        tick.notified().await;
        let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM application_outbox WHERE sport=$1), (SELECT count(*) FROM pipeline_work WHERE sport=$1 AND stage='sigil' AND status='pending'), (SELECT count(*) FROM pipeline_work WHERE sport=$1 AND stage='test_article' AND status='running')")
            .bind(sport).fetch_one(&pool).await.unwrap();
        assert_eq!(counts, (0, 1, 1));
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::select! {
            _ = &mut drain => panic!("blocked drain ended early"),
            _ = &mut dispatcher => panic!("dispatcher exited"),
            _ = prove => {}
        }
    })
    .await
    .unwrap();
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(5), &mut drain)
        .await
        .unwrap();
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE sport=$1")
        .bind(sport)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
}

struct Terminal(sqlx::PgPool);
static SIGIL_TEST_MANIFEST: crate::harness::skill::SkillManifest =
    crate::harness::skill::SkillManifest {
        plugin_id: "test",
        id: crate::harness::skill::SkillId::new("test.sigil"),
        task: crate::harness::skills::oracle::manifest::TASK,
        claim_policy: crate::harness::queue::work::ClaimPolicy::FIFO,
        inference_routes: &[],
        resources: crate::harness::skill::ResourceProfile::unbounded_batch(1),
        tools: &[],
    };

#[async_trait::async_trait]
impl Skill for Terminal {
    fn manifest(&self) -> &'static crate::harness::skill::SkillManifest {
        &SIGIL_TEST_MANIFEST
    }
    async fn execute(&self, item: &Item) -> Result<SkillOutcome> {
        let mut tx = self.0.begin().await?;
        assert!(work::lock_claim(&mut tx, item).await?);
        assert!(work::complete_in_transaction(&mut tx, item).await?);
        tx.commit().await?;
        Ok(SkillOutcome::Committed)
    }
}

#[tokio::test]
#[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
async fn outbox_poll_recovers_obligation_without_listen_or_models() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    let sport = "ZZ_WORKER_REHEARSAL";
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES ($1,$1,2026) ON CONFLICT DO NOTHING")
        .bind(sport).execute(&pool).await.unwrap();
    for table in ["pipeline_work", "application_outbox"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE sport=$1"))
            .bind(sport)
            .execute(&pool)
            .await
            .unwrap();
    }
    let source = Item {
        stage: crate::harness::skills::analyst::manifest::TASK,
        entity_type: "team".into(),
        entity_id: 9_600_002,
        sport: sport.into(),
        input_version: Some("v1".into()),
        attempts: 0,
        claim_token: Some("00000000-0000-4000-8000-000000000002".into()),
    };
    let mut tx = pool.begin().await.unwrap();
    crate::harness::skills::analyst::record_momentum_completed(&mut tx, &source)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let worker = Worker::new(
        pool.clone(),
        vec![crate::harness::skill::tests::test_plugin(vec![
            std::sync::Arc::new(Terminal(pool.clone())),
        ])],
        crate::harness::registration::build_reactions(pool.clone()).unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1800),
        Duration::from_secs(5),
        Duration::from_secs(30),
        Some(1),
    );
    let tick = Notify::new();
    let dispatcher = outbox_loop(&pool, &worker.reactions, &tick, &worker.shutdown);
    let consume = async {
        tick.notified().await;
        worker.tick("outbox", &Pulse::new()).await;
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::select! {
            _ = dispatcher => panic!("dispatcher exited"),
            _ = consume => {}
        }
    })
    .await
    .unwrap();
    let remaining:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM application_outbox WHERE sport=$1)+(SELECT count(*) FROM pipeline_work WHERE sport=$1)")
        .bind(sport).fetch_one(&pool).await.unwrap();
    assert_eq!(remaining, 0);
}

const UNRELATED_TASK: crate::harness::queue::work::TaskKey =
    crate::harness::queue::work::TaskKey::new("test_unrelated_worker");
static UNRELATED_MANIFEST: crate::harness::skill::SkillManifest =
    crate::harness::skill::SkillManifest {
        plugin_id: "test",
        id: crate::harness::skill::SkillId::new("test.unrelated-worker"),
        task: UNRELATED_TASK,
        claim_policy: crate::harness::queue::work::ClaimPolicy::FIFO,
        inference_routes: &[],
        resources: crate::harness::skill::ResourceProfile::unbounded_batch(1),
        tools: &[],
    };

struct Unrelated(PgPool);

#[async_trait::async_trait]
impl Skill for Unrelated {
    fn manifest(&self) -> &'static crate::harness::skill::SkillManifest {
        &UNRELATED_MANIFEST
    }

    async fn execute(&self, item: &Item) -> Result<SkillOutcome> {
        let mut tx = self.0.begin().await?;
        assert!(work::lock_claim(&mut tx, item).await?);
        assert!(work::complete_in_transaction(&mut tx, item).await?);
        tx.commit().await?;
        Ok(SkillOutcome::Committed)
    }
}

#[tokio::test]
#[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
async fn unrelated_task_registers_claims_executes_and_completes_without_kernel_edits() {
    let sport = "ZZ_UNRELATED_PLUGIN";
    let pool = fixture(sport).await;
    work::enqueue(&pool, &item(UNRELATED_TASK, sport, 9_600_103))
        .await
        .unwrap();
    let worker = Worker::new(
        pool.clone(),
        vec![crate::harness::skill::tests::test_plugin(vec![
            std::sync::Arc::new(Unrelated(pool.clone())),
        ])],
        crate::harness::queue::outbox::ReactionRegistry::empty(),
        Duration::from_secs(60),
        Duration::from_secs(1800),
        Duration::from_secs(5),
        Duration::ZERO,
        Some(1),
    );
    worker.drain_all("test", &Pulse::new()).await;
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE sport=$1 AND stage=$2")
            .bind(sport)
            .bind(UNRELATED_TASK.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0);
}
