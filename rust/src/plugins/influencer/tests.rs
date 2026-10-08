use super::*;
use crate::harness::model::{GenerateOptions, GenerateResult};
use crate::harness::Parser;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Model {
    calls: AtomicUsize,
    reply: &'static str,
}
#[async_trait]
impl Inference for Model {
    async fn generate(
        &self,
        prompt: &str,
        options: &GenerateOptions,
    ) -> Result<(GenerateResult, Value)> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok((
            GenerateResult {
                response: self.reply.into(),
                thinking: String::new(),
                model: "test".into(),
                total_duration: Duration::from_millis(1),
                prompt_eval_count: 100,
                eval_count: 20,
                completion_reason: Some("stop".into()),
                raw_response_body: self.reply.into(),
            },
            self.request_body(prompt, options),
        ))
    }
    fn model(&self) -> &str {
        "test"
    }
    fn request_body(&self, prompt: &str, options: &GenerateOptions) -> Value {
        json!({"prompt":prompt,"system":options.system,"format":options.format_schema})
    }
}

#[tokio::test]
async fn period_card_contract() {
    let source = |id, text: &str| SourceContext {
        classifier_world: None,
        classification_id: id,
        article_id: id,
        headline: "Club reporting".into(),
        context: text.into(),
        source: "Wire".into(),
        published_at_epoch: Some(1709164800),
    };
    let a = Assignment::from_parts(prompt::Parts {
        subject: EntityMeta {
            name: "Club".into(),
            entity_type: "team".into(),
            entity_id: 7,
            sport: "NBA".into(),
        },
        period: prompt::Period {
            season: 2024,
            week: 1,
            start: 1709164700,
            end: 1709769500,
            cutoff: 1709164900,
        },
        sources: vec![
            source(1, "Morgan was hopeful."),
            source(2, "Ellis disagreed. No other supporters were interviewed."),
        ],
        history: vec![],
        excluded: vec![],
    })
    .unwrap();
    let model = Model {
        calls: AtomicUsize::new(0),
        reply: r#"{"score":50,"headline":"Two speakers differ","body":"Morgan was hopeful.\n\nEllis disagreed. No other supporters were interviewed."}"#,
    };
    let (out, receipt) = create(&model, &a, 4096).await.unwrap();
    let out = out.unwrap();
    assert_eq!(model.calls.load(Ordering::Relaxed), 1);
    assert_eq!(out.sentiment, Some(50));
    assert_eq!(out.provenance.input_ids, vec![1, 2]);
    assert!(out.vibe_prompt.as_ref().unwrap().contains("\n\n"));
    assert_eq!(
        receipt["input_components"]["sources"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let world: Value = serde_json::from_str(&a.parts.assemble()).unwrap();
    assert!(world["FRESH EVIDENCE"][1]["publisher_text"]
        .as_str()
        .unwrap()
        .ends_with("No other supporters were interviewed."));
    assert!(world["FRESH EVIDENCE"][0]
        .get("classification_id")
        .is_none());
    let mut changed = a.parts.clone();
    changed.period.cutoff += 1;
    assert_eq!(
        a.input_hash,
        Assignment::from_parts(changed.clone()).unwrap().input_hash
    );
    changed.sources[1].context.push_str(" A correction.");
    assert_ne!(
        a.input_hash,
        Assignment::from_parts(changed).unwrap().input_hash
    );
    for score in [0, 100] {
        assert!(VibeParser
            .parse(&json!({"score":score,"headline":"Reading","body":"A reading."}).to_string())
            .unwrap()
            .is_some());
    }
    for raw in [
        r#"{"score":101,"headline":"Reading","body":"A reading."}"#,
        r#"{"score":null,"headline":"Reading","body":"A reading."}"#,
        r#"{"headline":"Reading","body":"A reading."}"#,
        r#"{"score":0,"headline":"Reading","body":""}"#,
    ] {
        assert!(VibeParser.parse(raw).is_err());
    }
    assert!(VibeParser
        .parse(r#"{"score":null,"headline":null,"body":null}"#)
        .unwrap()
        .is_none());
    let broken = Model {
        calls: AtomicUsize::new(0),
        reply: "truncated",
    };
    let (out, receipt) = create(&broken, &a, 4096).await.unwrap();
    assert!(out.is_none());
    assert!(receipt.get("error").is_some());
    assert_eq!(receipt["raw_response"], "truncated");
    assert_eq!(broken.calls.load(Ordering::Relaxed), 1);

    // Retained v19 packets remain readable; the new contract invalidates old attempt reuse.
    let retained: Value = serde_json::from_str(include_str!(
        "../../../fixtures/influencer/period-card-v19-smollm3.jsonl"
    ))
    .unwrap();
    let receipt = &retained["receipt"];
    let assignment = Assignment::from_parts(
        serde_json::from_value(receipt["input_components"].clone()).unwrap(),
    )
    .unwrap();
    let backend = crate::harness::providers::ollama::OllamaClient::with_think(
        "http://localhost:11434",
        receipt["model_version"].as_str().unwrap(),
        Duration::from_secs(1),
        Some(false),
    )
    .unwrap();
    assert_eq!(assignment.parts.assemble(), receipt["packet"]);
    assert_ne!(
        prompt::request_hash(&backend, &assignment, 4096).unwrap(),
        receipt["input_hash"]
    );
    let mut changed = assignment.parts.clone();
    changed.sources[0].classifier_world = Some(json!({"qualified_claims":[{
        "target_relation":"unknown", "kind":"withdrawn", "time_scope":"historical",
        "qualifiers":{"speaker":null,"negation":["not confirmed"]}}],"signals_unassessed":true}));
    let with_world = Assignment::from_parts(changed).unwrap();
    let packet: Value = serde_json::from_str(&with_world.parts.assemble()).unwrap();
    assert_eq!(
        packet["FRESH EVIDENCE"][0]["classifier_world"]["qualified_claims"][0]["target_relation"],
        "unknown"
    );
    assert_ne!(with_world.input_hash, assignment.input_hash);
}

#[tokio::test]
#[ignore = "disposable classifier_test database and SCORACLE_MEMORY_STUDY_BIN"]
async fn period_card_flow() -> Result<()> {
    use crate::harness::plugin::StudioPlugin;
    use crate::harness::{config::Backend, dbtest, queue::work};
    use crate::plugins::classifier::{self, plumbing_tests};
    const SPORT: &str = "ZZ_CLASSIFIER_INFLUENCER";
    let pool = dbtest::pool("a disposable classifier_test database").await;
    plumbing_tests::setup_disposable(&pool).await?;
    // Domain fixtures only; source/delivery, queue, outbox and period-card constraints use real migrations.
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS season_weeks(sport text,season integer,week_no integer,
        starts_at timestamptz,ends_at timestamptz,PRIMARY KEY(sport,season,week_no));
        CREATE TABLE IF NOT EXISTS vibe_scores(id bigserial PRIMARY KEY,entity_type text,entity_id integer,sport text,
        trigger_type text,trigger_payload jsonb,sentiment smallint,prompt text,hook text,input_news_ids bigint[],
        model_version text,prompt_version text,input_hash text,week_season integer,week_no integer,
        generated_at timestamptz DEFAULT now());")
        .execute(&pool).await?;
    sqlx::raw_sql(include_str!(
        "../../../../sql/migrations/290_vibe_period_cards.sql"
    ))
    .execute(&pool)
    .await?;
    for table in [
        "pipeline_work",
        "application_outbox",
        "vibe_card_attempts",
        "vibe_scores",
        "season_weeks",
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
    sqlx::query("DELETE FROM news_articles WHERE id BETWEEN 1001 AND 1005")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,'Fictional Influencer',2026) ON CONFLICT DO NOTHING")
        .bind(SPORT).execute(&pool).await?;
    sqlx::query("INSERT INTO teams(id,sport,name) VALUES(11,$1,'Équipe') ON CONFLICT DO NOTHING")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    let current = now();
    let start = current - 86400;
    for week in [1, 2] {
        let begins = start - (2 - week) as i64 * 7 * 86400;
        sqlx::query("INSERT INTO season_weeks VALUES($1,2026,$2,to_timestamp($3::double precision),to_timestamp($4::double precision))")
            .bind(SPORT).bind(week).bind(begins).bind(begins+7*86400).execute(&pool).await?;
    }
    let acquire = classifier::adapter::AcquireHandler::new(
        pool.clone(),
        std::sync::Arc::new(crate::harness::tools::WebBroker::new(0)?),
    );
    let classifier_model = plumbing_tests::Model::new("fictional-classifier", Backend::Ollama);
    let mut measurements = Vec::new();
    for id in 1001..1006_i64 {
        let report = if id == 1004 { 1002 } else { id };
        sqlx::query(
            "INSERT INTO news_articles(id,url,title,source,published_at,full_text,feed_rank)
            VALUES($1,$2,$3,'Fixture Wire',to_timestamp($4::double precision),$5,1)",
        )
        .bind(id)
        .bind(format!("https://example.invalid/vibe/{id}"))
        .bind(format!("Équipe report {report}"))
        .bind(match id {
            1001 => Some(start - 86400),
            1005 => None,
            _ => Some(current - 3600),
        })
        .bind(format!(
            "Report {report}. {}",
            plumbing_tests::source().body
        ))
        .execute(&pool)
        .await?;
        sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport,feed_rank) VALUES($1,'team',11,$2,1)")
            .bind(id).bind(SPORT).execute(&pool).await?;
        sqlx::query("SELECT public.classifier_enqueue_acquisition($1,$2)")
            .bind(id)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        let claim =
            dbtest::claim_one(&pool, classifier::manifest::ACQUIRE_TASK, "vibe source").await;
        acquire.execute(&claim).await?;
        let claim = dbtest::claim_one(&pool, classifier::manifest::TASK, "vibe measurement").await;
        classifier::adapter::execute_model(&pool, &claim, &classifier_model).await?;
        measurements.push(sqlx::query_scalar::<_,i64>("SELECT id FROM classifier_measurements WHERE article_id=$1 ORDER BY id DESC LIMIT 1")
            .bind(id).fetch_one(&pool).await?);
    }
    sqlx::query("UPDATE news_articles SET duplicate_of=1002 WHERE id IN (1003,1004)")
        .execute(&pool)
        .await?;
    // Duplicate flags participate in discovery; reconcile changed metadata before release.
    for id in [1003_i64, 1004] {
        sqlx::query("SELECT public.classifier_enqueue_acquisition($1,$2)")
            .bind(id)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        let claim =
            dbtest::claim_one(&pool, classifier::manifest::ACQUIRE_TASK, "copy source").await;
        acquire.execute(&claim).await?;
        let claim = dbtest::claim_one(&pool, classifier::manifest::TASK, "copy measurement").await;
        classifier::adapter::execute_model(&pool, &claim, &classifier_model).await?;
        measurements[(id - 1001) as usize] = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM classifier_measurements WHERE article_id=$1 ORDER BY id DESC LIMIT 1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await?;
    }
    let plugin = manifest::MANIFEST.id.as_str();
    assert!(load_for_character(&pool, plugin, "team", 11, SPORT)
        .await?
        .is_empty());
    // Only fictional controls are released; Journalists' independent obligations remain held.
    sqlx::query("UPDATE classifier_deliveries SET status='pending',production_eligible=true,reason='fictional_control'
        WHERE measurement_id=ANY($1) AND plugin_id=$2").bind(&measurements).bind(plugin).execute(&pool).await?;
    let model = Model {
        calls: AtomicUsize::new(0),
        reply: r#"{"score":50,"headline":"Fixture reading","body":"Fixture Wire reports uncertainty.\n\nThe late correction remains attached."}"#,
    };
    let claim = dbtest::claim_one(&pool, manifest::TASK, "old reporting period").await;
    assert!(claim
        .input_version
        .as_deref()
        .unwrap()
        .starts_with(classifier::CONTRACT));
    assert!(matches!(
        execute_with_backend(&pool, &model, 4096, &claim).await?,
        PluginOutcome::Deferred { .. }
    ));
    assert_eq!(model.calls.load(Ordering::Relaxed), 1);
    assert_eq!(dbtest::count(&pool, "vibe_scores", SPORT).await, 1);
    let unknown:(String,bool,Option<String>)=sqlx::query_as("SELECT status,production_eligible,reason FROM classifier_deliveries WHERE measurement_id=$1 AND plugin_id=$2")
        .bind(measurements[4]).bind(plugin).fetch_one(&pool).await?;
    assert_eq!(
        unknown,
        (
            "held".into(),
            false,
            Some("unknown_publication_time".into())
        )
    );
    assert!(work::fail(&pool, &claim, "fictional restart", Duration::ZERO, 5).await?);
    let claim = dbtest::claim_one(&pool, manifest::TASK, "current reporting period").await;
    let subject = EntityMeta {
        name: "Équipe".into(),
        entity_type: "team".into(),
        entity_id: 11,
        sport: SPORT.into(),
    };
    let pending = load_for_character(&pool, plugin, "team", 11, SPORT).await?;
    let (assignment, _) =
        prompt::prepare_assignment(&pool, subject, pending.last().unwrap(), now()).await?;
    assert_eq!(assignment.parts.history.len(), 1);
    assert_eq!(
        assignment.parts.sources.len(),
        2,
        "a copy flag cannot erase the changed second report"
    );
    assert_eq!(
        assignment
            .parts
            .excluded
            .iter()
            .filter(|e| e["reason"] == "known_copy")
            .count(),
        1
    );
    let packet: Value = serde_json::from_str(&assignment.parts.assemble())?;
    for report in packet["FRESH EVIDENCE"]
        .as_array()
        .unwrap()
        .iter()
        .chain(packet["RELEVANT HISTORY"].as_array().unwrap())
    {
        assert!(report["publisher_text"]
            .as_str()
            .unwrap()
            .ends_with("Later correction: no move."));
        assert_eq!(
            report["classifier_world"]["qualified_claims"][0]["target_relation"],
            "unknown"
        );
        assert_eq!(report["classifier_world"]["signals_unassessed"], true);
    }
    // Source drift is rejected under the publication transaction.
    let article = assignment.parts.sources[0].article_id;
    sqlx::query("UPDATE news_articles SET title='changed' WHERE id=$1")
        .bind(article)
        .execute(&pool)
        .await?;
    let mut tx = pool.begin().await?;
    assert!(memories::validate(&mut tx, &assignment.parts)
        .await
        .is_err());
    tx.rollback().await?;
    sqlx::query("UPDATE news_articles SET title=$2 WHERE id=$1")
        .bind(article)
        .bind(assignment.parts.sources[0].headline.clone())
        .execute(&pool)
        .await?;
    // Outbox failure rolls back the product, receipts and claim together; attempt is retained.
    sqlx::raw_sql("ALTER TABLE application_outbox ADD CONSTRAINT reject_vibe_fixture CHECK(sport<>'ZZ_CLASSIFIER_INFLUENCER')")
        .execute(&pool).await?;
    assert!(execute_with_backend(&pool, &model, 4096, &claim)
        .await
        .is_err());
    assert_eq!(dbtest::count(&pool, "vibe_scores", SPORT).await, 1);
    assert_eq!(
        load_for_character(&pool, plugin, "team", 11, SPORT)
            .await?
            .len(),
        3
    );
    assert_eq!(dbtest::count(&pool, "pipeline_work", SPORT).await, 1);
    sqlx::raw_sql("ALTER TABLE application_outbox DROP CONSTRAINT reject_vibe_fixture")
        .execute(&pool)
        .await?;
    assert_eq!(
        execute_with_backend(&pool, &model, 4096, &claim).await?,
        PluginOutcome::Committed
    );
    assert_eq!(dbtest::count(&pool, "vibe_scores", SPORT).await, 2);
    assert_eq!(dbtest::count(&pool, "application_outbox", SPORT).await, 1);
    let (used, redundant): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER(WHERE status='used'),count(*) FILTER(WHERE status='redundant')
        FROM classifier_deliveries WHERE measurement_id=ANY($1) AND plugin_id=$2",
    )
    .bind(&measurements)
    .bind(plugin)
    .fetch_one(&pool)
    .await?;
    assert_eq!((used, redundant), (3, 1));
    let calls = model.calls.load(Ordering::Relaxed);
    // An explicit replay of a finished delivery reuses the exact period product.
    sqlx::query("UPDATE classifier_deliveries SET status='pending' WHERE measurement_id=ANY($1) AND plugin_id=$2 AND status IN ('used','redundant')")
        .bind(&measurements[1..4]).bind(plugin).execute(&pool).await?;
    let claim = dbtest::claim_one(&pool, manifest::TASK, "repeat exact period").await;
    assert_eq!(
        execute_with_backend(&pool, &model, 4096, &claim).await?,
        PluginOutcome::Committed
    );
    assert_eq!(model.calls.load(Ordering::Relaxed), calls);
    assert_eq!(dbtest::count(&pool, "vibe_scores", SPORT).await, 2);
    let held:i64=sqlx::query_scalar("SELECT count(*) FROM classifier_deliveries WHERE measurement_id=ANY($1) AND plugin_id='scoracle.character.narrative' AND status='held' AND NOT production_eligible")
        .bind(&measurements).fetch_one(&pool).await?;
    assert_eq!(held, 5);
    Ok(())
}
