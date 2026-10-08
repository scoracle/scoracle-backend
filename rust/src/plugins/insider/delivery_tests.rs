//! Fictional native delivery, canonical counterparties and claim-fenced publication.
use super::*;
use crate::harness::{
    config::Backend,
    dbtest,
    model::{GenerateOptions, GenerateResult},
    queue::work,
};
use crate::plugins::classifier::{self, plumbing_tests};
use serde_json::{json, Value};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

struct Voice(AtomicUsize);
#[async_trait]
impl Inference for Voice {
    fn model(&self) -> &str {
        "fictional-insider"
    }
    fn request_body(&self, built: &str, _: &GenerateOptions) -> Value {
        json!({"prompt":built})
    }
    async fn generate(
        &self,
        built: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, Value)> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let packet: Value = serde_json::from_str(built)?;
        let subject = packet["meta"]["name"].as_str().unwrap();
        let report = &packet["fresh"]["reports"][0];
        assert!(report["publisher_excerpt"]
            .as_str()
            .unwrap()
            .ends_with("Later correction: no move."));
        assert_eq!(
            report["classifier_world"]["qualified_claims"][0]["target_relation"],
            "unknown"
        );
        let partner = if subject == "Équipe" {
            "Jordan Sample"
        } else {
            "Équipe"
        };
        let response=json!({"body":format!("{subject}: Fixture Wire denied the possible move; uncertainty remains."),
            "findings":[{"report_index":0,"counterparty":partner,"status":"denied","stage":null,
                "evidence_quote": if subject=="Coach Morgan" {"Coach Morgan denied a possible move for Équipe"} else {"Équipe denied a possible move for Jordan Sample"}}]}).to_string();
        Ok((
            GenerateResult {
                response: response.clone(),
                raw_response_body: response,
                thinking: String::new(),
                model: self.model().into(),
                total_duration: Duration::ZERO,
                prompt_eval_count: 100,
                eval_count: 80,
                completion_reason: Some("stop".into()),
            },
            self.request_body(built, opts),
        ))
    }
}

#[tokio::test]
#[ignore = "requires disposable classifier_test database and SCORACLE_MEMORY_STUDY_BIN"]
async fn classifier_insider_identity_and_publication() -> Result<()> {
    const SPORT: &str = "ZZ_CLASSIFIER_INSIDER";
    let pool = dbtest::pool("a disposable classifier_test database").await;
    plumbing_tests::setup_disposable(&pool).await?;
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS insider_scores(id bigserial PRIMARY KEY,sport text,entity_type text,entity_id integer,
        score smallint,previous_score smallint,read text,headline text,model_version text,prompt_version text,input_hash text,generated_at timestamptz DEFAULT now());
        CREATE TABLE IF NOT EXISTS transfer_rumors(id bigserial PRIMARY KEY,team_id integer,player_id integer,sport text,trigger_type text,trigger_payload jsonb,
        heat smallint,heat_components jsonb,is_rumor boolean,direction text,stage text,model_summary text,source_attribution text,input_news_ids bigint[],model_version text,
        prompt_version text,rumor_updated_at timestamptz,source_count integer,source_names text[],source_latest_at timestamptz,source_oldest_at timestamptz,input_hash text,subject_type text);
        CREATE TABLE IF NOT EXISTS transfer_ground_truth(sport text,player_id integer,team_id integer);")
        .execute(&pool).await?;
    for table in [
        "pipeline_work",
        "application_outbox",
        "insider_scores",
        "transfer_rumors",
        "entity_name_surfaces",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE sport=$1"))
            .bind(SPORT)
            .execute(&pool)
            .await?;
    }
    sqlx::query("DELETE FROM harvester_query_provenance WHERE article_id=1011")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM news_articles WHERE id=1011")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,'Fictional Insider',2026) ON CONFLICT DO NOTHING")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO teams(id,sport,name) VALUES(11,$1,'Équipe') ON CONFLICT DO NOTHING")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO players(id,sport,name) VALUES(12,$1,'Jordan Sample') ON CONFLICT DO NOTHING",
    )
    .bind(SPORT)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO persons(id,sport,full_name,kind,team_id) VALUES(13,$1,'Coach Morgan','coach',11) ON CONFLICT DO NOTHING",
    )
    .bind(SPORT)
    .execute(&pool)
    .await?;
    for (kind, id, name, surface) in [
        ("team", 11, "Équipe", "name"),
        ("player", 12, "jordan sample", "name"),
        ("person", 13, "coach morgan", "name"),
        ("player", 12, "j sample", "alias"),
    ] {
        sqlx::query("INSERT INTO entity_name_surfaces(sport,entity_type,entity_id,norm,surface_kind) VALUES($1,$2,$3,public.nrm($4),$5)")
            .bind(SPORT)
            .bind(kind)
            .bind(id)
            .bind(name)
            .bind(surface)
            .execute(&pool)
            .await?;
    }
    sqlx::query("INSERT INTO news_articles(id,url_hash,url,title,source,published_at,full_text,feed_rank) VALUES(1011,md5('https://example.invalid/insider'),'https://example.invalid/insider','Move denied','Fixture Wire',now()-interval '1 hour',$1,1)")
        .bind("Équipe denied a possible move for Jordan Sample in talks. Coach Morgan denied a possible move for Équipe in talks. J Sample is an alias only. Later correction: no move.").execute(&pool).await?;
    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport,feed_rank) VALUES(1011,'team',11,$1,1)")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("SELECT classifier_enqueue_acquisition(1011,$1)")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    let acquire = classifier::adapter::AcquireHandler::new(
        pool.clone(),
        std::sync::Arc::new(crate::harness::tools::WebBroker::new(0)?),
    );
    let claim =
        dbtest::claim_one(&pool, classifier::manifest::ACQUIRE_TASK, "Insider source").await;
    acquire.execute(&claim).await?;
    let claim = dbtest::claim_one(&pool, classifier::manifest::TASK, "Insider measure").await;
    classifier::adapter::execute_model(
        &pool,
        &claim,
        &plumbing_tests::Model::new("fictional-classifier", Backend::Ollama),
    )
    .await?;
    let plugin = manifest::MANIFEST.id.as_str();
    let targets:Vec<String>=sqlx::query_scalar("SELECT m.receipt->'target'->>'entity_type' FROM classifier_deliveries d
        JOIN classifier_measurements m ON m.id=d.measurement_id WHERE m.article_id=1011 AND d.plugin_id=$1 AND d.status='held' ORDER BY 1")
        .bind(plugin).fetch_all(&pool).await?;
    assert_eq!(targets, vec!["person", "player", "team"]);
    sqlx::query("UPDATE classifier_deliveries SET status='pending',production_eligible=true,reason='fictional_control' WHERE plugin_id=$1 AND measurement_id IN (SELECT id FROM classifier_measurements WHERE article_id=1011)")
        .bind(plugin).execute(&pool).await?;
    let voice = Voice(AtomicUsize::new(0));
    for index in 0..3 {
        let claim = dbtest::claim_one(&pool, manifest::TASK, "Insider native target").await;
        if index == 0 {
            let material = prompt::load_material(&pool, &claim).await?;
            let generation = create(
                &Studio::new(&voice),
                &material.subject,
                &material.reports,
                &material.history,
                &material.source_records,
                4096,
            )
            .await?;
            sqlx::query(
                "INSERT INTO entity_name_surfaces(sport,entity_type,entity_id,norm,surface_kind) VALUES($1,'player',99,'jordan sample','name')",
            )
            .bind(SPORT)
            .execute(&pool)
            .await?;
            assert!(
                publish::commit(&pool, &claim, &material, &generation)
                    .await
                    .is_err(),
                "changed/ambiguous identity cannot publish"
            );
            assert_eq!(dbtest::count(&pool, "insider_scores", SPORT).await, 0);
            sqlx::query("DELETE FROM entity_name_surfaces WHERE sport=$1 AND entity_id=99")
                .bind(SPORT)
                .execute(&pool)
                .await?;
            sqlx::raw_sql("ALTER TABLE application_outbox ADD CONSTRAINT reject_insider_fixture CHECK(sport<>'ZZ_CLASSIFIER_INSIDER')").execute(&pool).await?;
            assert!(publish::commit(&pool, &claim, &material, &generation)
                .await
                .is_err());
            assert_eq!(dbtest::count(&pool, "insider_scores", SPORT).await, 0);
            sqlx::raw_sql("ALTER TABLE application_outbox DROP CONSTRAINT reject_insider_fixture")
                .execute(&pool)
                .await?;
            assert!(work::fail(&pool, &claim, "fictional restart", Duration::ZERO, 5).await?);
            let recovered = dbtest::claim_one(&pool, manifest::TASK, "Insider recovery").await;
            assert_eq!(
                execute_with_backend(&pool, &voice, 4096, &recovered).await?,
                PluginOutcome::Committed
            );
        } else {
            assert_eq!(
                execute_with_backend(&pool, &voice, 4096, &claim).await?,
                PluginOutcome::Committed
            );
        }
    }
    assert_eq!(dbtest::count(&pool, "insider_scores", SPORT).await, 2);
    assert_eq!(dbtest::count(&pool, "transfer_rumors", SPORT).await, 2);
    let denied:i64=sqlx::query_scalar("SELECT count(*) FROM transfer_rumors WHERE sport=$1 AND NOT is_rumor AND stage IS NULL AND trigger_type='classifier'")
        .bind(SPORT).fetch_one(&pool).await?;
    assert_eq!(denied, 2, "denials never become active moves");
    assert_eq!(dbtest::count(&pool, "application_outbox", SPORT).await, 2);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pipeline_work WHERE sport=$1 AND stage='transfers'"
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await?,
        0
    );
    let used:i64=sqlx::query_scalar("SELECT count(*) FROM classifier_deliveries d JOIN classifier_measurements m ON m.id=d.measurement_id WHERE m.article_id=1011 AND d.plugin_id=$1 AND d.status='used'")
        .bind(plugin).fetch_one(&pool).await?;
    assert_eq!(used, 3);
    Ok(())
}
