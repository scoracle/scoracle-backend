//! Fictional Classifier worlds keep independent identity and structured-record guards.
use super::*;
use crate::harness::plugin::StudioPlugin;
use crate::harness::{
    config::Backend,
    dbtest,
    model::{GenerateOptions, GenerateResult},
    queue::work,
};
use crate::plugins::classifier::{self, plumbing_tests};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Voice {
    calls: AtomicUsize,
    revert: Option<PgPool>,
}
#[async_trait::async_trait]
impl Inference for Voice {
    fn model(&self) -> &str {
        "fictional-scout"
    }
    fn request_body(&self, built: &str, _: &GenerateOptions) -> Value {
        json!({"prompt":built})
    }
    async fn generate(
        &self,
        built: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, Value)> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let packet: Value = serde_json::from_str(built)?;
        assert_eq!(packet["fresh"]["composite"], 65.0);
        let report = &packet["reporting"][0];
        assert!(report["publisher_text"]
            .as_str()
            .unwrap()
            .ends_with("Later correction: no move."));
        assert_eq!(
            report["classifier_world"]["qualified_claims"][0]["target_relation"],
            "unknown"
        );
        assert!(opts.num_ctx >= 32768);
        if let Some(pool) = &self.revert {
            sqlx::query("UPDATE player_availability SET reverted_at=now() WHERE sport='ZZ_CLASSIFIER_SCOUT'")
                .execute(pool).await?;
        }
        let response=json!({"body":"The supplied profile records measured contributions. Fixture Wire reports a late correction."}).to_string();
        Ok((
            GenerateResult {
                response: response.clone(),
                raw_response_body: response,
                thinking: String::new(),
                model: self.model().into(),
                total_duration: Duration::ZERO,
                prompt_eval_count: 100,
                eval_count: 50,
                completion_reason: Some("stop".into()),
            },
            self.request_body(built, opts),
        ))
    }
}

#[tokio::test]
#[ignore = "requires disposable classifier_test database"]
async fn classifier_scout_selection_records_and_publication() -> Result<()> {
    const SPORT: &str = "ZZ_CLASSIFIER_SCOUT";
    let pool = dbtest::pool("a disposable classifier_test database").await;
    plumbing_tests::setup_disposable(&pool).await?;
    sqlx::raw_sql("ALTER TABLE application_outbox DROP CONSTRAINT IF EXISTS reject_scout_fixture")
        .execute(&pool)
        .await?;
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS stat_summaries(id bigserial PRIMARY KEY,entity_type text,entity_id integer,sport text,season integer,
        trigger_type text,trigger_payload jsonb,body text,headline text,notability smallint,notability_components jsonb,input_components jsonb,input_hash text,
        model_version text,prompt_version text,generated_at timestamptz,rating_trajectory text,rating_trajectory_label text,rating_trajectory_components jsonb);
        CREATE TABLE IF NOT EXISTS team_stats(team_id integer,sport text,season integer,rating_score double precision,rating_breakdown jsonb,rating_scoped_ranks jsonb,
        league_id integer,updated_at timestamptz,stats jsonb);
        CREATE TABLE IF NOT EXISTS stat_definitions(sport text,entity_type text,key_name text,display_name text);
        CREATE TABLE IF NOT EXISTS rating_thresholds(sport text,stat_key text);
        CREATE TABLE IF NOT EXISTS fixtures(id integer PRIMARY KEY,start_time timestamptz);
        CREATE TABLE IF NOT EXISTS event_team_stats(fixture_id integer,team_id integer,sport text,season integer,rating double precision);
        CREATE TABLE IF NOT EXISTS event_box_scores(fixture_id integer,player_id integer,sport text,season integer,rating double precision);
        CREATE TABLE IF NOT EXISTS transfer_identity_applications(id bigserial PRIMARY KEY,sport text,status text,reverted_at timestamptz,
            evidence jsonb,player_id integer,old_team_id integer,new_team_id integer,applied_at timestamptz);
        CREATE TABLE IF NOT EXISTS player_availability(id bigserial PRIMARY KEY,sport text,status text,reverted_at timestamptz,
            player_id integer,team_id integer,source_article_id bigint,applied_at timestamptz,kind text,event_date date);")
        .execute(&pool).await?;
    for table in [
        "pipeline_work",
        "application_outbox",
        "stat_summaries",
        "team_stats",
        "entity_name_surfaces",
        "player_availability",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE sport=$1"))
            .bind(SPORT)
            .execute(&pool)
            .await?;
    }
    sqlx::query("DELETE FROM harvester_query_provenance WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM news_articles WHERE id BETWEEN 1021 AND 1025")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,'Fictional Scout',2026) ON CONFLICT DO NOTHING")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO teams(id,sport,name) VALUES(11,$1,'Équipe') ON CONFLICT DO NOTHING")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO entity_name_surfaces(sport,entity_type,entity_id,norm,surface_kind) VALUES($1,'team',11,public.nrm('Équipe'),'name')",
    )
    .bind(SPORT)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO team_stats(team_id,sport,season,rating_score,rating_breakdown,rating_scoped_ranks,league_id,updated_at,stats) VALUES(11,$1,2026,65,'[]','{}',0,now(),'{\"games_played\":12}')",
    )
    .bind(SPORT)
    .execute(&pool)
    .await?;
    sqlx::query("DELETE FROM stat_definitions WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO stat_definitions(sport,entity_type,key_name,display_name) VALUES($1,'team','games_played','Games Played')")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    let acquire = classifier::adapter::AcquireHandler::new(
        pool.clone(),
        std::sync::Arc::new(crate::harness::tools::WebBroker::new(0)?),
    );
    let mut ids = Vec::new();
    for article in 1021..1026_i64 {
        sqlx::query("INSERT INTO news_articles(id,url_hash,url,title,source,published_at,full_text,feed_rank) VALUES($1,md5($2),$2,$3,'Fixture Wire',to_timestamp($4::double precision),$5,1)")
            .bind(article).bind(format!("https://example.invalid/scout/{article}"))
            .bind(if article==1023 {"Other Club report".into()} else {format!("Équipe report {article}")})
            .bind(if article==1025 {None} else {Some(crate::plugins::influencer::now()-3600)})
            .bind(if article==1023 {"Other Club reported a possible move. Later correction: no move.".into()} else {plumbing_tests::source().body})
            .execute(&pool).await?;
        sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport,feed_rank) VALUES($1,'team',11,$2,1)")
            .bind(article)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("SELECT classifier_enqueue_acquisition($1,$2)")
            .bind(article)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        let claim =
            dbtest::claim_one(&pool, classifier::manifest::ACQUIRE_TASK, "Scout source").await;
        acquire.execute(&claim).await?;
        let claim = dbtest::claim_one(&pool, classifier::manifest::TASK, "Scout measurement").await;
        classifier::adapter::execute_model(
            &pool,
            &claim,
            &plumbing_tests::Model::new("fictional-classifier", Backend::Ollama),
        )
        .await?;
        ids.push(sqlx::query_scalar::<_,i64>("SELECT id FROM classifier_measurements WHERE article_id=$1 ORDER BY id DESC LIMIT 1").bind(article).fetch_one(&pool).await?);
    }
    let plugin = crate::plugins::scout::manifest::MANIFEST.id.as_str();
    assert!(load_for_character(&pool, plugin, "team", 11, SPORT)
        .await?
        .is_empty());
    assert!(sqlx::query("UPDATE classifier_deliveries SET status='pending',production_eligible=true WHERE measurement_id=$1 AND plugin_id=$2")
        .bind(ids[0]).bind(plugin).execute(&pool).await.is_err(),"a raw score cannot release Scout without an explicit selection");
    assert!(sqlx::query("UPDATE classifier_deliveries SET status='pending',production_eligible=true,selection='{\"kind\":null}' WHERE measurement_id=$1 AND plugin_id=$2")
        .bind(ids[0]).bind(plugin).execute(&pool).await.is_err());
    sqlx::query(
        "INSERT INTO players(id,sport,name) VALUES(12,$1,'Fixture Player') ON CONFLICT DO NOTHING",
    )
    .bind(SPORT)
    .execute(&pool)
    .await?;
    let availability:i64=sqlx::query_scalar("INSERT INTO player_availability(sport,status,player_id,team_id,source_article_id,applied_at,kind,event_date) VALUES($1,'applied',12,11,1022,now(),'injury',current_date) RETURNING id")
        .bind(SPORT).fetch_one(&pool).await?;
    for (id, kind) in ids.iter().zip([
        "performance",
        "availability",
        "performance",
        "roster",
        "performance",
    ]) {
        sqlx::query("UPDATE classifier_deliveries SET status='pending',production_eligible=true,reason='fictional_control',selection=jsonb_build_object('kind',$3::text)
            WHERE measurement_id=$1 AND plugin_id=$2").bind(id).bind(plugin).bind(kind).execute(&pool).await?;
    }
    let voice = Voice {
        calls: AtomicUsize::new(0),
        revert: None,
    };
    let undated = dbtest::claim_one(
        &pool,
        crate::plugins::scout::manifest::TASK,
        "Scout undated source",
    )
    .await;
    assert!(matches!(
        execute_with_backend(&pool, &voice, 4096, &undated).await?,
        PluginOutcome::Deferred { .. }
    ));
    assert_eq!(voice.calls.load(Ordering::SeqCst), 0);
    work::fail(&pool, &undated, "next dated report", Duration::ZERO, 5).await?;
    let claim = dbtest::claim_one(
        &pool,
        crate::plugins::scout::manifest::TASK,
        "Scout first report",
    )
    .await;
    sqlx::raw_sql("ALTER TABLE application_outbox ADD CONSTRAINT reject_scout_fixture CHECK(sport<>'ZZ_CLASSIFIER_SCOUT')").execute(&pool).await?;
    assert!(execute_with_backend(&pool, &voice, 4096, &claim)
        .await
        .is_err());
    assert_eq!(dbtest::count(&pool, "stat_summaries", SPORT).await, 0);
    assert_eq!(
        load_for_character(&pool, plugin, "team", 11, SPORT)
            .await?
            .len(),
        4
    );
    sqlx::raw_sql("ALTER TABLE application_outbox DROP CONSTRAINT reject_scout_fixture")
        .execute(&pool)
        .await?;
    assert!(matches!(
        execute_with_backend(&pool, &voice, 4096, &claim).await?,
        PluginOutcome::Deferred { .. }
    ));
    assert!(work::fail(&pool, &claim, "fictional restart", Duration::ZERO, 5).await?);
    let claim = dbtest::claim_one(
        &pool,
        crate::plugins::scout::manifest::TASK,
        "Scout structured report",
    )
    .await;
    let mutating = Voice {
        calls: AtomicUsize::new(0),
        revert: Some(pool.clone()),
    };
    assert!(
        execute_with_backend(&pool, &mutating, 4096, &claim)
            .await
            .is_err(),
        "an applied record changed during inference cannot publish"
    );
    assert_eq!(dbtest::count(&pool, "stat_summaries", SPORT).await, 1);
    sqlx::query("UPDATE player_availability SET reverted_at=NULL WHERE id=$1")
        .bind(availability)
        .execute(&pool)
        .await?;
    assert!(matches!(
        execute_with_backend(&pool, &voice, 4096, &claim).await?,
        PluginOutcome::Deferred { .. }
    ));
    assert!(work::fail(&pool, &claim, "next Scout source", Duration::ZERO, 5).await?);
    let calls = voice.calls.load(Ordering::SeqCst);
    for index in 0..2 {
        let claim = dbtest::claim_one(
            &pool,
            crate::plugins::scout::manifest::TASK,
            "unresolved Scout source",
        )
        .await;
        let result = execute_with_backend(&pool, &voice, 4096, &claim).await?;
        if index < 1 {
            assert!(matches!(result, PluginOutcome::Deferred { .. }));
            work::fail(&pool, &claim, "next unresolved source", Duration::ZERO, 5).await?;
        } else {
            assert_eq!(result, PluginOutcome::Committed);
        }
    }
    assert_eq!(
        voice.calls.load(Ordering::SeqCst),
        calls,
        "unresolved identity, absent structured record and unknown dates remain uncalled"
    );
    let statuses:Vec<String>=sqlx::query_scalar("SELECT status FROM classifier_deliveries WHERE measurement_id=ANY($1) AND plugin_id=$2 ORDER BY measurement_id")
        .bind(&ids).bind(plugin).fetch_all(&pool).await?;
    assert_eq!(statuses, vec!["used", "used", "held", "held", "held"]);
    let reporting =
        crate::plugins::scout::sources::load_scout_reports(&pool, "team", 11, SPORT).await?;
    assert_eq!(reporting.len(), 2);
    assert!(reporting.iter().all(|s| s.classifier_world.is_some()));
    sqlx::query("UPDATE player_availability SET reverted_at=now() WHERE id=$1")
        .bind(availability)
        .execute(&pool)
        .await?;
    let history =
        crate::plugins::scout::sources::load_scout_reports(&pool, "team", 11, SPORT).await?;
    assert!(history
        .iter()
        .any(
            |source| source.classifier_world.as_ref().unwrap()["structured_record"]["withdrawn"]
                == true
        ));
    assert_eq!(dbtest::count(&pool, "stat_summaries", SPORT).await, 2);
    assert_eq!(dbtest::count(&pool, "application_outbox", SPORT).await, 2);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pipeline_work WHERE sport=$1 AND stage='rating'"
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await?,
        0
    );
    Ok(())
}
