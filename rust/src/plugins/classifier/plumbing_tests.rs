//! Contract fixtures exercise plumbing, not model correctness.
use super::*;
use crate::harness::config::{Backend, ModelSpec};
use crate::harness::model::{GenerateOptions, GenerateResult, Inference, ResponseFailure};
use crate::harness::plugin::StudioPlugin;
use crate::harness::queue::work::{self, Item};
use async_trait::async_trait;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

pub(crate) struct Model {
    name: &'static str,
    backend: Backend,
    calls: Arc<AtomicUsize>,
    failure: bool,
    changed: bool,
    revision_known: bool,
    tokenizer_known: bool,
    mutate_source: Option<sqlx::PgPool>,
}
impl Model {
    pub(crate) fn new(name: &'static str, backend: Backend) -> Self {
        Self {
            name,
            backend,
            calls: Arc::new(AtomicUsize::new(0)),
            failure: false,
            changed: false,
            revision_known: true,
            tokenizer_known: true,
            mutate_source: None,
        }
    }
}
#[async_trait]
impl Inference for Model {
    async fn prepare(
        &self,
        built: &str,
        options: &GenerateOptions,
    ) -> Result<crate::harness::model::PreparedRequest> {
        // Synthetic byte tokenizer, not a claim about either real transport's tokenizer.
        let rendered = format!("{}{}", options.system.as_deref().unwrap_or(""), built);
        Ok(crate::harness::model::PreparedRequest {
            request: self.request_body(built, options),
            coverage: self
                .tokenizer_known
                .then_some(crate::harness::model::InputCoverage {
                    token_ids: vec![0; rendered.len()],
                    rendered_prompt: rendered,
                    context: options.num_ctx,
                    provider: json!({"fixture":"byte tokenizer"}),
                }),
        })
    }
    async fn revision(&self) -> Result<Option<String>> {
        Ok(self.revision_known.then(|| {
            if self.changed && self.calls.load(Ordering::SeqCst) > 0 {
                "changed".into()
            } else {
                format!("{}-artifact", self.name)
            }
        }))
    }
    fn model(&self) -> &str {
        self.name
    }
    fn request_body(&self, built: &str, options: &GenerateOptions) -> Value {
        // Use the actual transport serializers, with no network or semantic inference.
        crate::plugins::text_generation::bind(
            &ModelSpec {
                backend: self.backend,
                model: self.name.into(),
                base_url: "http://127.0.0.1:1".into(),
                think: Some(false),
            },
            Duration::from_secs(1),
        )
        .unwrap()
        .request_body(built, options)
    }
    async fn generate(
        &self,
        built: &str,
        options: &GenerateOptions,
    ) -> Result<(GenerateResult, Value)> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(pool) = &self.mutate_source {
            sqlx::query("UPDATE news_articles SET title='changed during inference' WHERE id=1")
                .execute(pool)
                .await?;
        }
        if self.failure {
            return Err(ResponseFailure {
                raw_response_body: "{\"error\":\"fixture provider failure\"}".into(),
                error: anyhow::anyhow!("fixture failure"),
            }
            .into());
        }
        let input: Value = serde_json::from_str(built)?;
        let body = input["input"]["body"].as_str().unwrap();
        assert!(body.ends_with("Later correction: no move."));
        let qualifiers = serde_json::from_str::<Value>(SCHEMA)?["qualifiers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| (key.as_str().unwrap().to_string(), Value::Null))
            .collect::<BTreeMap<_, _>>();
        let response = json!({"complete_source_review":true,"extraction_usable":true,
            "measurements":{"presence":{"topic":{"player_move":{"value":0.7,
                "evidence":[{"quote":"possible move","occurrence":0}]}}},"ordinals":{}},
            "claims":[{"evidence":{"quote":body,"occurrence":0},"target_relation":"unknown",
            "target_evidence":null,"kind":"information","time_scope":"unknown",
            "candidate_dimensions":[],"qualifiers":qualifiers}]})
        .to_string();
        Ok((
            GenerateResult {
                response: response.clone(),
                thinking: String::new(),
                model: self.name.into(),
                total_duration: Duration::from_millis(1),
                prompt_eval_count: (built.len() + options.system.as_ref().map_or(0, String::len))
                    as i32,
                eval_count: 100,
                completion_reason: Some("stop".into()),
                raw_response_body: json!({"fixture":response}).to_string(),
            },
            self.request_body(built, options),
        ))
    }
}
pub(crate) fn source() -> Source {
    Source {
        article_id: 1,
        body: "Équipe reported a possible move.\n\nLater correction: no move.".into(),
        source: "Fixture Wire".into(),
        published_at: None,
        query_entities: vec![
            json!({"entity_type":"team","entity_id":1,"sport":"TEST","name":"Équipe","feed_rank":1}),
        ],
        provenance: [
            ("url".into(), json!("https://example.invalid/plumbing")),
            ("title".into(), json!("Fixture source")),
            ("duplicate_of".into(), Value::Null),
        ]
        .into(),
    }
}

#[tokio::test]
async fn model_swap_preserves_contract_and_failures() {
    let source = source();
    let target = &source.query_entities[0];
    let ollama = Model::new("model-a", Backend::Ollama);
    let compatible = Model::new("model-b", Backend::OpenAi);
    let a = adapter::measure(&ollama, &source, target).await.unwrap();
    let b = adapter::measure(&compatible, &source, target)
        .await
        .unwrap();
    assert_eq!(a["status"], "source_bound_provisional", "{}", a["error"]);
    assert_eq!(b["status"], a["status"]);
    assert_eq!(a["qualification"], b["qualification"]);
    assert_eq!(a["selection"], b["selection"]);
    assert_ne!(a["request_hash"], b["request_hash"]);
    assert_eq!(a["production_eligible"], false);
    assert_eq!(a["measurements"], b["measurements"]);
    let fixed: Measurements = serde_json::from_value(a["measurements"].clone()).unwrap();
    assert_eq!(
        fixed.presence.values().map(BTreeMap::len).sum::<usize>(),
        51
    );
    assert_eq!(fixed.ordinals.len(), 3);
    assert_eq!(fixed.presence["topic"]["player_move"].value, Some(0.7));
    assert_eq!(
        fixed
            .presence
            .values()
            .flat_map(BTreeMap::values)
            .chain(fixed.ordinals.values())
            .filter(|signal| signal.value.is_none() && signal.evidence.is_none())
            .count(),
        53
    );
    let proposed = |value: Value| serde_json::from_value::<MeasurementProposal>(value).unwrap();
    let measured = measurements(
        &source,
        target,
        Some(proposed(json!({"presence":{"topic":{"player_move":{
        "value":0.7,"evidence":[{"quote":"possible move","occurrence":0}]}}},"ordinals":{}}))),
    )
    .unwrap();
    assert_eq!(measured.presence["topic"]["player_move"].value, Some(0.7));
    assert_eq!(
        measured.presence["topic"]["player_move"]
            .evidence
            .as_ref()
            .unwrap()[0]
            .start,
        source.body.find("possible move").unwrap()
    );
    assert!(measured.presence["emotion"]["joy"].value.is_none());
    for invalid in [
        json!({"presence":{"fake":{}},"ordinals":{}}),
        json!({"presence":{"topic":{"player_move":{"value":1.2,"evidence":null}}},"ordinals":{}}),
        json!({"presence":{"topic":{"player_move":{"value":0.7,"evidence":null}}},"ordinals":{}}),
        json!({"presence":{"topic":{"player_move":{"value":0.7,"evidence":[{"quote":"invented","occurrence":0}]}}},"ordinals":{}}),
        json!({"presence":{},"ordinals":{"affect.intensity":{"value":1.5,"evidence":[{"quote":"possible move","occurrence":0}]}}}),
    ] {
        assert!(measurements(&source, target, Some(proposed(invalid))).is_err());
    }
    assert_eq!(a["calibration_status"], "unassessed");
    assert_eq!(a["tokenizer_coverage_verified"], true);
    assert!(
        a["request"]["format"]["properties"]["claims"]["items"]["properties"]
            ["candidate_dimensions"]["items"]["enum"]
            .is_array()
    );
    assert_eq!(a["request"]["options"]["num_ctx"], prompt::NUM_CTX);
    assert_eq!(b["request"]["max_tokens"], prompt::NUM_PREDICT);
    let mut failing = Model::new("failed", Backend::Ollama);
    failing.failure = true;
    let failed = adapter::measure(&failing, &source, target).await.unwrap();
    assert_eq!(failed["status"], "error");
    assert!(failed["qualification"].is_null());
    assert!(failed["raw_response"]
        .as_str()
        .unwrap()
        .contains("fixture provider failure"));
    let mut changed = Model::new("changed", Backend::Ollama);
    changed.changed = true;
    assert!(
        adapter::measure(&changed, &source, target).await.unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("artifact changed")
    );
    let mut oversized = source.clone();
    oversized.body = "Late correction. ".repeat(2000);
    let small = Model::new("bounded", Backend::Ollama);
    let refused = adapter::measure(&small, &oversized, &oversized.query_entities[0])
        .await
        .unwrap();
    assert_eq!(small.calls.load(Ordering::SeqCst), 0);
    assert_eq!(refused["status"], "error");
    assert_eq!(refused["input_coverage"], "not_confirmed");
    let mut unverified = Model::new("no-tokenizer", Backend::Ollama);
    unverified.tokenizer_known = false;
    let refused = adapter::measure(&unverified, &source, target)
        .await
        .unwrap();
    assert_eq!(unverified.calls.load(Ordering::SeqCst), 0);
    assert_eq!(refused["status"], "error");
    assert_eq!(refused["tokenizer_coverage_verified"], false);
    assert!(refused["error"]
        .as_str()
        .unwrap()
        .contains("exact tokenizer"));
}

#[tokio::test]
#[ignore = "requires a disposable classifier_test database via TEST_DATABASE_URL"]
async fn durable_acquisition_swap_reuse_retry_and_claim_fence() -> Result<()> {
    use crate::harness::tools::WebBroker;
    let pool = crate::harness::dbtest::pool("an isolated empty classifier_test database").await;
    setup_disposable(&pool).await?;
    sqlx::query("DELETE FROM pipeline_work WHERE sport='TEST' AND entity_id=1")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM harvester_query_provenance WHERE article_id=1")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM news_articles WHERE id=1")
        .execute(&pool)
        .await?;
    let source = source();
    sqlx::query(
        "INSERT INTO teams(id,sport,name) VALUES(1,'TEST','Équipe') ON CONFLICT DO NOTHING",
    )
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO news_articles(id,url,title,source,full_text) VALUES(1,$1,$2,$3,$4)")
        .bind(source.provenance["url"].as_str())
        .bind(source.provenance["title"].as_str())
        .bind(&source.source)
        .bind(&source.body)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport,feed_rank) VALUES(1,'team',1,'TEST',1)").execute(&pool).await?;
    let acquire = adapter::AcquireHandler::new(pool.clone(), Arc::new(WebBroker::new(0)?));
    let replay =
        || sqlx::query_scalar::<_, i64>("SELECT public.classifier_replay_acquisition(1000)");
    assert_eq!(replay().fetch_one(&pool).await?, 1);
    assert_eq!(replay().fetch_one(&pool).await?, 0);
    let acquire_claim =
        crate::harness::dbtest::claim_one(&pool, manifest::ACQUIRE_TASK, "acquisition").await;
    assert_eq!(replay().fetch_one(&pool).await?, 0);
    let mut rss_transaction = pool.begin().await?;
    sqlx::query("SELECT id FROM news_articles WHERE id=1 FOR UPDATE")
        .fetch_one(&mut *rss_transaction)
        .await?;
    let busy = tokio::time::timeout(Duration::from_secs(1), acquire.execute(&acquire_claim))
        .await?
        .expect_err("busy RSS source must fail promptly for durable retry");
    assert_eq!(
        busy.downcast_ref::<sqlx::Error>()
            .and_then(sqlx::Error::as_database_error)
            .and_then(|error| error.code())
            .as_deref(),
        Some("55P03")
    );
    rss_transaction.rollback().await?;
    work::fail(
        &pool,
        &acquire_claim,
        "fetch unavailable",
        std::time::Duration::from_secs(60),
        1,
    )
    .await?;
    assert_eq!(replay().fetch_one(&pool).await?, 0);
    let preserved: (String, i32, Option<String>) = sqlx::query_as("SELECT status,attempts,last_error FROM pipeline_work WHERE stage='classifier_acquire' AND sport='TEST' AND entity_id=1")
        .fetch_one(&pool).await?;
    assert_eq!(
        preserved,
        ("failed".into(), 1, Some("fetch unavailable".into()))
    );
    sqlx::query("UPDATE news_articles SET title=title || ' revised' WHERE id=1")
        .execute(&pool)
        .await?;
    assert_eq!(replay().fetch_one(&pool).await?, 1);
    let acquire_claim =
        crate::harness::dbtest::claim_one(&pool, manifest::ACQUIRE_TASK, "revised acquisition")
            .await;
    assert_eq!(
        acquire.execute(&acquire_claim).await?,
        crate::harness::plugin::PluginOutcome::Committed
    );
    assert_eq!(replay().fetch_one(&pool).await?, 0);
    let mut classify = crate::harness::dbtest::claim_one(&pool, manifest::TASK, "classifier").await;
    let source_id = classify.input_version.clone().unwrap();
    let a = Model::new("model-a", Backend::Ollama);
    assert_eq!(
        adapter::execute_model(&pool, &classify, &a).await?,
        crate::harness::plugin::PluginOutcome::Committed
    );
    work::enqueue(
        &pool,
        &Item {
            claim_token: None,
            ..classify.clone()
        },
    )
    .await?;
    classify = crate::harness::dbtest::claim_one(&pool, manifest::TASK, "reuse").await;
    adapter::execute_model(&pool, &classify, &a).await?;
    assert_eq!(a.calls.load(Ordering::SeqCst), 1);
    let b = Model::new("model-b", Backend::OpenAi);
    work::enqueue(
        &pool,
        &Item {
            claim_token: None,
            ..classify.clone()
        },
    )
    .await?;
    classify = crate::harness::dbtest::claim_one(&pool, manifest::TASK, "swap").await;
    adapter::execute_model(&pool, &classify, &b).await?;
    assert_eq!(b.calls.load(Ordering::SeqCst), 1);
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id,receipt->>'model' FROM classifier_measurements WHERE article_id=1 AND sport='TEST' ORDER BY id")
            .fetch_all(&pool)
            .await?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].1, "model-a");
    assert_eq!(rows[1].1, "model-b");
    let statuses:Vec<String>=sqlx::query_scalar("SELECT d.status FROM classifier_deliveries d JOIN classifier_measurements m ON m.id=d.measurement_id WHERE m.article_id=1 ORDER BY m.id")
        .fetch_all(&pool).await?;
    assert_eq!(statuses, vec!["superseded", "held"]);
    for (id, _) in rows {
        let (kept, record, measured) = adapter::load_measurement(&pool, id).await?;
        assert_eq!(measured.body_sha256, record.body_sha256);
        assert_eq!(kept.body, source.body);
        validate(&kept, &record)?;
        let original: String =
            sqlx::query_scalar("SELECT receipt::text FROM classifier_measurements WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await?;
        sqlx::query("UPDATE classifier_measurements SET receipt=jsonb_set(receipt,'{qualification,claims,0,target_relation}','\"other_subject\"') WHERE id=$1")
            .bind(id).execute(&pool).await?;
        assert!(
            adapter::load_measurement(&pool, id).await.is_err(),
            "source-valid claims still must match the retained model reply"
        );
        sqlx::query("UPDATE classifier_measurements SET receipt=$2::jsonb WHERE id=$1")
            .bind(id)
            .bind(original)
            .execute(&pool)
            .await?;
    }
    let mut failed = Model::new("model-error", Backend::Ollama);
    failed.failure = true;
    work::enqueue(
        &pool,
        &Item {
            claim_token: None,
            ..classify.clone()
        },
    )
    .await?;
    classify = crate::harness::dbtest::claim_one(&pool, manifest::TASK, "failure").await;
    assert!(adapter::execute_model(&pool, &classify, &failed)
        .await
        .is_err());
    let errors: i64 =
        sqlx::query_scalar("SELECT count(*) FROM classifier_measurements WHERE article_id=1 AND sport='TEST' AND status='error'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(errors, 1);
    assert!(work::fail(&pool, &classify, "fixture failure", Duration::ZERO, 5).await?);
    classify = crate::harness::dbtest::claim_one(&pool, manifest::TASK, "retry").await;
    failed.failure = false;
    adapter::execute_model(&pool, &classify, &failed).await?;
    assert_eq!(failed.calls.load(Ordering::SeqCst), 2);
    let mut unpinned = Model::new("unversioned", Backend::OpenAi);
    unpinned.revision_known = false;
    for _ in 0..2 {
        work::enqueue(
            &pool,
            &Item {
                claim_token: None,
                ..classify.clone()
            },
        )
        .await?;
        classify = crate::harness::dbtest::claim_one(&pool, manifest::TASK, "unversioned").await;
        adapter::execute_model(&pool, &classify, &unpinned).await?;
    }
    assert_eq!(
        unpinned.calls.load(Ordering::SeqCst),
        2,
        "unversioned tags must not reuse measurements"
    );
    let sources: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM classifier_sources WHERE article_id=1 AND sport='TEST'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        sources, 1,
        "model swap and failure must preserve the acquired source"
    );
    let mut overtaken = Model::new("overtaken-source", Backend::Ollama);
    overtaken.mutate_source = Some(pool.clone());
    work::enqueue(
        &pool,
        &Item {
            claim_token: None,
            ..classify.clone()
        },
    )
    .await?;
    classify =
        crate::harness::dbtest::claim_one(&pool, manifest::TASK, "source changes during inference")
            .await;
    assert!(adapter::execute_model(&pool, &classify, &overtaken)
        .await
        .is_err());
    let failed_receipt:String=sqlx::query_scalar("SELECT receipt::text FROM classifier_measurements WHERE receipt->>'model'='overtaken-source'")
        .fetch_one(&pool).await?;
    let failed_receipt: Value = serde_json::from_str(&failed_receipt)?;
    assert_eq!(failed_receipt["status"], "error");
    assert!(!failed_receipt["raw_response"].is_null());
    assert!(failed_receipt["qualification"].is_null());
    work::fail(&pool, &classify, "source changed", Duration::ZERO, 5).await?;
    classify =
        crate::harness::dbtest::claim_one(&pool, manifest::TASK, "changed source retry").await;
    adapter::execute_model(&pool, &classify, &a).await?;
    let refreshed =
        crate::harness::dbtest::claim_one(&pool, manifest::ACQUIRE_TASK, "refresh acquisition")
            .await;
    acquire.execute(&refreshed).await?;
    classify =
        crate::harness::dbtest::claim_one(&pool, manifest::TASK, "refresh classification").await;
    adapter::execute_model(&pool, &classify, &a).await?;
    assert_eq!(
        a.calls.load(Ordering::SeqCst),
        2,
        "metadata changes invalidate source snapshot reuse"
    );
    // Supersede a claimed source revision; stale execution cannot publish or complete its successor.
    work::enqueue(
        &pool,
        &Item {
            claim_token: None,
            ..classify.clone()
        },
    )
    .await?;
    let stale = crate::harness::dbtest::claim_one(&pool, manifest::TASK, "stale").await;
    work::enqueue(
        &pool,
        &Item {
            input_version: Some(format!("0{source_id}")),
            claim_token: None,
            ..stale.clone()
        },
    )
    .await?;
    let stale_model = Model::new("model-stale", Backend::Ollama);
    assert_eq!(
        adapter::execute_model(&pool, &stale, &stale_model).await?,
        crate::harness::plugin::PluginOutcome::Superseded
    );
    let successor = crate::harness::dbtest::claim_one(&pool, manifest::TASK, "successor").await;
    sqlx::query("UPDATE news_articles SET full_text=full_text || ' changed' WHERE id=1")
        .execute(&pool)
        .await?;
    assert_eq!(
        adapter::execute_model(&pool, &successor, &a).await?,
        crate::harness::plugin::PluginOutcome::Committed
    );
    let reacquisition = crate::harness::dbtest::claim_one(
        &pool,
        manifest::ACQUIRE_TASK,
        "changed source reacquisition",
    )
    .await;
    assert_eq!(reacquisition.entity_id, 1);
    assert_eq!(
        a.calls.load(Ordering::SeqCst),
        2,
        "stale source must not call the model"
    );

    Ok(())
}

pub(crate) async fn setup_disposable(pool: &sqlx::PgPool) -> Result<()> {
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await?;
    ensure!(
        database.starts_with("classifier_test"),
        "use a disposable classifier_test database"
    );
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS schema_migrations(version text PRIMARY KEY);
        CREATE TABLE IF NOT EXISTS teams(id integer,sport text,name text,PRIMARY KEY(id,sport));
        CREATE TABLE IF NOT EXISTS news_articles(id bigint PRIMARY KEY,url text NOT NULL,title text NOT NULL,
            source text,published_at timestamptz,full_text text,duplicate_of bigint,feed_rank integer);
        CREATE TABLE IF NOT EXISTS harvester_query_provenance(article_id bigint REFERENCES news_articles(id),
            entity_type text,entity_id integer,sport text,feed_rank integer,PRIMARY KEY(article_id,entity_type,entity_id,sport));
        CREATE TABLE IF NOT EXISTS sports(id text PRIMARY KEY,display_name text,current_season integer);")
        .execute(pool).await?;
    for (version, migration) in [
        (
            "102_pipeline_work",
            include_str!("../../../../sql/migrations/102_pipeline_work.sql"),
        ),
        (
            "109_pipeline_work_article_stage",
            include_str!("../../../../sql/migrations/109_pipeline_work_article_stage.sql"),
        ),
        (
            "256_pipeline_work_claim_fencing",
            include_str!("../../../../sql/migrations/256_pipeline_work_claim_fencing.sql"),
        ),
        (
            "257_influencer_publication_outbox",
            include_str!("../../../../sql/migrations/257_influencer_publication_outbox.sql"),
        ),
        (
            "258_analyst_publication_outbox",
            include_str!("../../../../sql/migrations/258_analyst_publication_outbox.sql"),
        ),
        (
            "259_scout_publication_outbox",
            include_str!("../../../../sql/migrations/259_scout_publication_outbox.sql"),
        ),
        (
            "260_journalist_publication_outbox",
            include_str!("../../../../sql/migrations/260_journalist_publication_outbox.sql"),
        ),
        (
            "291_classifier_plumbing",
            include_str!("../../../../sql/migrations/291_classifier_plumbing.sql"),
        ),
        (
            "292_classifier_acquisition_intake",
            include_str!("../../../../sql/migrations/292_classifier_acquisition_intake.sql"),
        ),
        (
            "293_classifier_character_delivery",
            include_str!("../../../../sql/migrations/293_classifier_character_delivery.sql"),
        ),
    ] {
        let applied: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=$1)")
                .bind(version)
                .fetch_one(pool)
                .await?;
        // Reapply the new delivery migration while its local implementation is being developed.
        if !applied || version == "293_classifier_character_delivery" {
            sqlx::raw_sql(migration).execute(pool).await?;
        }
    }
    Ok(())
}
