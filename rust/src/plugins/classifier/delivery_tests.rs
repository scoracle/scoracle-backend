//! Fictional end-to-end delivery check; no calibration or production promotion.
use super::*;
use crate::harness::{
    config::Backend,
    dbtest,
    model::{GenerateOptions, GenerateResult, Inference},
    queue::work,
    Studio,
};
use crate::plugins::{
    classifier::{self, plumbing_tests},
    journalist::{self, prompt},
};
use crate::tools::meta::EntityMeta;
use anyhow::{ensure, Result};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

struct Voice(AtomicUsize);
#[async_trait]
impl Inference for Voice {
    fn model(&self) -> &str {
        "fictional-journalist"
    }
    fn request_body(&self, built: &str, _: &GenerateOptions) -> serde_json::Value {
        json!({"model":self.model(),"prompt":built})
    }
    async fn generate(
        &self,
        built: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let packet: serde_json::Value = serde_json::from_str(built)?;
        let mut response = serde_json::Map::new();
        for report in packet["fresh"].as_array().unwrap() {
            let text = report["publisher_excerpt"].as_str().unwrap();
            assert!(text.ends_with("Later correction: no move."));
            assert_eq!(
                report["classifier_world"]["qualified_claims"][0]["publisher_text"],
                text
            );
            assert_eq!(
                report["classifier_world"]["qualified_claims"][0]["target_relation"],
                "unknown"
            );
            assert_eq!(report["classifier_world"]["signals_unassessed"], true);
            response.insert(
                report["report_key"].as_str().unwrap().into(),
                json!(format!("Fixture Wire reports: {text}")),
            );
        }
        let response = serde_json::Value::Object(response).to_string();
        Ok((
            GenerateResult {
                response: response.clone(),
                thinking: String::new(),
                model: self.model().into(),
                total_duration: Duration::ZERO,
                prompt_eval_count: 100,
                eval_count: 100,
                completion_reason: Some("stop".into()),
                raw_response_body: response,
            },
            self.request_body(built, opts),
        ))
    }
}

#[tokio::test]
#[ignore = "requires a disposable classifier_test database via TEST_DATABASE_URL"]
async fn classifier_journalist_delivery_publication_and_recovery() -> Result<()> {
    const SPORT: &str = "ZZ_CLASSIFIER_JOURNALIST";
    let pool = dbtest::pool("a disposable classifier_test database").await;
    plumbing_tests::setup_disposable(&pool).await?;
    // Minimal domain fixture; queue, source/delivery and outbox use their real migrations.
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS news_summaries(
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,entity_type text,entity_id integer,sport text,
        trigger_type text,trigger_payload jsonb,narrative_title text,body text,impact smallint,impact_components jsonb,
        input_news_ids bigint[],narrative_updated_at timestamptz,source_count integer,source_names text[],
        source_latest_at timestamptz,source_oldest_at timestamptz,trajectory text,trajectory_components jsonb,
        model_version text,prompt_version text,input_hash text,storyline_id bigint,card_score smallint,headline text,generated_at timestamptz);
        CREATE TABLE IF NOT EXISTS harvester_classifications(id bigint,article_id bigint,headline text,context_text text,
            contract_version text,model_provenance jsonb,entity_type text,entity_id integer,sport text,created_at timestamptz);
        CREATE TABLE IF NOT EXISTS harvester_assignments(classification_id bigint,plugin_id text,status text);
        CREATE TABLE IF NOT EXISTS storyline_articles(article_id bigint,storyline_id bigint);
        CREATE TABLE IF NOT EXISTS storyline_entities(storyline_id bigint,sport text,entity_type text,entity_id integer);
        CREATE TABLE IF NOT EXISTS narrative_events(article_id bigint,sport text,origin text,subject_type text,subject_id integer,
            object_type text,object_id integer,event_date timestamptz);
        CREATE TABLE IF NOT EXISTS entity_name_surfaces(sport text,entity_type text,entity_id integer,norm text);
        CREATE OR REPLACE FUNCTION public.nrm(text) RETURNS text LANGUAGE sql IMMUTABLE AS $$SELECT lower($1)$$;")
        .execute(&pool).await?;
    for table in ["application_outbox", "news_summaries", "pipeline_work"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE sport=$1"))
            .bind(SPORT)
            .execute(&pool)
            .await?;
    }
    sqlx::query("DELETE FROM harvester_query_provenance WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM news_articles WHERE id BETWEEN 991 AND 995")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,'Fictional delivery',2026) ON CONFLICT DO NOTHING")
        .bind(SPORT).execute(&pool).await?;
    sqlx::query("INSERT INTO teams(id,sport,name) VALUES(11,$1,'Équipe') ON CONFLICT DO NOTHING")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    let acquire = classifier::adapter::AcquireHandler::new(
        pool.clone(),
        std::sync::Arc::new(crate::harness::tools::WebBroker::new(0)?),
    );
    let model = plumbing_tests::Model::new("fictional-classifier", Backend::Ollama);
    let now = journalist::now_unix();
    let mut measurements = Vec::new();
    for id in 991..996 {
        let source = plumbing_tests::source();
        sqlx::query(
            "INSERT INTO news_articles(id,url,title,source,published_at,full_text,feed_rank)
            VALUES($1,$2,$3,'Fixture Wire',to_timestamp($4::double precision),$5,1)",
        )
        .bind(id)
        .bind(format!("https://example.invalid/{id}"))
        .bind(format!("Équipe report {id}"))
        .bind(if id == 995 { None } else { Some(now - 3600) })
        .bind(format!("Report {id}. {}", source.body))
        .execute(&pool)
        .await?;
        sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport,feed_rank) VALUES($1,'team',11,$2,1)")
            .bind(id).bind(SPORT).execute(&pool).await?;
        sqlx::query("SELECT public.classifier_enqueue_acquisition($1,$2)")
            .bind(id)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        let item =
            dbtest::claim_one(&pool, classifier::manifest::ACQUIRE_TASK, "native source").await;
        crate::harness::plugin::StudioPlugin::execute(&acquire, &item).await?;
        let item = dbtest::claim_one(&pool, classifier::manifest::TASK, "native measure").await;
        classifier::adapter::execute_model(&pool, &item, &model).await?;
        measurements.push(sqlx::query_scalar::<_,i64>("SELECT id FROM classifier_measurements WHERE article_id=$1 ORDER BY id DESC LIMIT 1")
            .bind(id).fetch_one(&pool).await?);
    }
    let plugin = journalist::manifest::MANIFEST.id.as_str();
    assert!(
        classifier::delivery::load_for_character(&pool, plugin, "team", 11, SPORT)
            .await?
            .is_empty()
    );
    let held:i64=sqlx::query_scalar("SELECT count(*) FROM classifier_deliveries WHERE plugin_id='scoracle.character.narrative' AND measurement_id=ANY($1) AND status='held' AND NOT production_eligible")
        .bind(&measurements).fetch_one(&pool).await?;
    assert_eq!(
        held, 5,
        "unassessed measurements never automatically release character work"
    );
    assert!(
        sqlx::query("UPDATE classifier_deliveries SET status='pending' WHERE plugin_id='scoracle.character.narrative' AND measurement_id=$1")
            .bind(measurements[0])
            .execute(&pool)
            .await
            .is_err(),
        "database rejects an ineligible pending obligation"
    );
    // Release fictional controls only. This is a plumbing fixture, not a model/policy qualification.
    sqlx::query("UPDATE classifier_deliveries SET status='pending',production_eligible=true,reason='fictional_control' WHERE plugin_id='scoracle.character.narrative' AND measurement_id=ANY($1)")
        .bind(&measurements).execute(&pool).await?;
    let subject = EntityMeta {
        name: "Équipe".into(),
        entity_type: "team".into(),
        entity_id: 11,
        sport: SPORT.into(),
    };
    let item = dbtest::item(
        journalist::manifest::TASK,
        SPORT,
        11,
        Some("classifier-fixture"),
    );
    let claimed =
        dbtest::claim_one(&pool, journalist::manifest::TASK, "Journalist first batch").await;
    assert!(
        claimed
            .input_version
            .as_deref()
            .unwrap()
            .starts_with(classifier::CONTRACT),
        "ready delivery must dispatch atomically"
    );
    sqlx::query("UPDATE classifier_deliveries SET reason=reason,production_eligible=true WHERE plugin_id='scoracle.character.narrative' AND measurement_id=ANY($1)")
        .bind(&measurements).execute(&pool).await?;
    // Unchanged ready updates preserve this exact live lease.
    let token: String = sqlx::query_scalar(
        "SELECT claim_token::text FROM pipeline_work WHERE stage='narratives' AND sport=$1",
    )
    .bind(SPORT)
    .fetch_one(&pool)
    .await?;
    assert_eq!(Some(token), claimed.claim_token);
    let material = prompt::load_narratives_material(&pool, subject.clone(), now).await?;
    assert_eq!(material.assignment.selected.len(), 3);
    assert_eq!(material.sources.len(), 4);
    let voice = Voice(AtomicUsize::new(0));
    let output =
        journalist::create(&pool, &Studio::new(&voice), &material.assignment, now, 4096).await?;
    // Dispatch writers lock delivery before queue; publication must fail fast on a busy delivery.
    let mut locked = pool.begin().await?;
    sqlx::query(
        "SELECT measurement_id FROM classifier_deliveries WHERE measurement_id=$1 FOR UPDATE",
    )
    .bind(material.sources[0].classification_id)
    .fetch_one(&mut *locked)
    .await?;
    assert!(
        tokio::time::timeout(
            Duration::from_secs(1),
            commit_claimed(
                &pool,
                &claimed,
                SPORT,
                "classifier",
                &json!({}),
                &Prepared::Product(&output),
                &material.sources,
                &material.assignment.dispositions,
            )
        )
        .await?
        .is_err(),
        "publication must not deadlock a ready-delivery writer"
    );
    locked.rollback().await?;
    // A changed source and a superseded lease both reject the prepared result.
    let changed_article = material.sources[0].article_id;
    sqlx::query("UPDATE news_articles SET title='changed during voice inference' WHERE id=$1")
        .bind(changed_article)
        .execute(&pool)
        .await?;
    assert!(commit_claimed(
        &pool,
        &claimed,
        SPORT,
        "classifier",
        &json!({}),
        &Prepared::Product(&output),
        &material.sources,
        &material.assignment.dispositions
    )
    .await
    .is_err());
    sqlx::query("UPDATE news_articles SET title=$2 WHERE id=$1")
        .bind(changed_article)
        .bind(format!("Équipe report {changed_article}"))
        .execute(&pool)
        .await?;
    work::enqueue(
        &pool,
        &work::Item {
            input_version: Some("classifier-fixture-new-lease".into()),
            ..item.clone()
        },
    )
    .await?;
    assert_eq!(
        commit_claimed(
            &pool,
            &claimed,
            SPORT,
            "classifier",
            &json!({}),
            &Prepared::Product(&output),
            &material.sources,
            &material.assignment.dispositions
        )
        .await?
        .0,
        PluginOutcome::Superseded
    );
    assert_eq!(dbtest::count(&pool, "news_summaries", SPORT).await, 0);
    let claimed = dbtest::claim_one(
        &pool,
        journalist::manifest::TASK,
        "Journalist current lease",
    )
    .await;
    // Fail product insertion: no receipt completion, partial product or event can escape.
    sqlx::raw_sql("ALTER TABLE news_summaries ADD CONSTRAINT reject_fixture_product CHECK(sport<>'ZZ_CLASSIFIER_JOURNALIST')")
        .execute(&pool).await?;
    assert!(commit_claimed(
        &pool,
        &claimed,
        SPORT,
        "classifier",
        &json!({}),
        &Prepared::Product(&output),
        &material.sources,
        &material.assignment.dispositions
    )
    .await
    .is_err());
    assert_eq!(dbtest::count(&pool, "news_summaries", SPORT).await, 0);
    let ready:i64=sqlx::query_scalar("SELECT count(*) FROM classifier_deliveries WHERE plugin_id='scoracle.character.narrative' AND measurement_id=ANY($1) AND status='pending'")
        .bind(&measurements).fetch_one(&pool).await?;
    assert_eq!(ready, 5);
    sqlx::raw_sql("ALTER TABLE news_summaries DROP CONSTRAINT reject_fixture_product")
        .execute(&pool)
        .await?;
    let (outcome, rows) = commit_claimed(
        &pool,
        &claimed,
        SPORT,
        "classifier",
        &json!({}),
        &Prepared::Product(&output),
        &material.sources,
        &material.assignment.dispositions,
    )
    .await?;
    assert!(matches!(outcome, PluginOutcome::Deferred { .. }));
    assert_eq!(rows.len(), 3);
    assert_eq!(dbtest::count(&pool, "application_outbox", SPORT).await, 0);
    ensure!(work::fail(&pool, &claimed, "fixture worker restart", Duration::ZERO, 5).await?);
    let claimed = dbtest::claim_one(&pool, journalist::manifest::TASK, "Journalist recovery").await;
    let material = prompt::load_narratives_material(&pool, subject.clone(), now).await?;
    assert_eq!(material.assignment.selected.len(), 1);
    let output =
        journalist::create(&pool, &Studio::new(&voice), &material.assignment, now, 4096).await?;
    sqlx::raw_sql("ALTER TABLE application_outbox ADD CONSTRAINT reject_fixture_event CHECK(sport<>'ZZ_CLASSIFIER_JOURNALIST')")
        .execute(&pool).await?;
    assert!(commit_claimed(
        &pool,
        &claimed,
        SPORT,
        "classifier",
        &json!({}),
        &Prepared::Product(&output),
        &material.sources,
        &material.assignment.dispositions
    )
    .await
    .is_err());
    assert_eq!(dbtest::count(&pool, "news_summaries", SPORT).await, 3);
    assert_eq!(dbtest::count(&pool, "pipeline_work", SPORT).await, 1);
    sqlx::raw_sql("ALTER TABLE application_outbox DROP CONSTRAINT reject_fixture_event")
        .execute(&pool)
        .await?;
    assert_eq!(
        commit_claimed(
            &pool,
            &claimed,
            SPORT,
            "classifier",
            &json!({}),
            &Prepared::Product(&output),
            &material.sources,
            &material.assignment.dispositions
        )
        .await?
        .0,
        PluginOutcome::Committed
    );
    assert_eq!(dbtest::count(&pool, "news_summaries", SPORT).await, 4);
    assert_eq!(dbtest::count(&pool, "application_outbox", SPORT).await, 1);
    assert_eq!(dbtest::count(&pool, "pipeline_work", SPORT).await, 0);
    assert_eq!(voice.0.load(Ordering::SeqCst), 2);
    let memory = journalist::memories::load(&pool, &subject, journalist::now_unix()).await?;
    assert_eq!(memory.published_reports.len(), 4);
    let unresolved:(String,bool,Option<String>)=sqlx::query_as("SELECT status,production_eligible,reason FROM classifier_deliveries WHERE plugin_id='scoracle.character.narrative' AND measurement_id=$1")
        .bind(*measurements.last().unwrap()).fetch_one(&pool).await?;
    assert_eq!(
        unresolved,
        (
            "held".into(),
            false,
            Some("unknown_publication_time".into())
        ),
        "unknown dates stay unresolved, never a completed no-evidence decision"
    );
    let mut tx = pool.begin().await?;
    classifier::delivery::record(&mut tx, measurements[0]).await?;
    tx.commit().await?;
    assert!(
        classifier::delivery::load_for_character(&pool, plugin, "team", 11, SPORT)
            .await?
            .is_empty(),
        "replay cannot reopen used evidence"
    );
    Ok(())
}
