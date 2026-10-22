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
async fn day_card_contract() {
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
        memories: vec![],
        subject: EntityMeta {
            name: "Club".into(),
            entity_type: "team".into(),
            entity_id: 7,
            sport: "NBA".into(),
        },
        day: prompt::Day {
            start: 1709164700,
            end: 1709251100,
            cutoff: 1709164900,
        },
        sources: vec![
            source(1, "Morgan was hopeful."),
            source(2, "Ellis disagreed. No other supporters were interviewed."),
        ],
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
    let replayed = output_from_receipt(&a, &receipt).unwrap().unwrap();
    assert_eq!(replayed.sentiment, out.sentiment);
    assert_eq!(replayed.vibe_prompt, out.vibe_prompt);
    assert_eq!(replayed.provenance.input_hash, out.provenance.input_hash);
    assert_eq!(replayed.request_body(), out.request_body());
    assert_eq!(model.calls.load(Ordering::Relaxed), 1);
    let mut failed_receipt = receipt.clone();
    failed_receipt["error"] = json!("failed validation");
    assert!(output_from_receipt(&a, &failed_receipt).is_err());
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
    assert!(world["fresh"]["reports"][1]["publisher_text"]
        .as_str()
        .unwrap()
        .ends_with("No other supporters were interviewed."));
    assert!(world["fresh"]["reports"][0]
        .get("classification_id")
        .is_none());
    let mut changed = a.parts.clone();
    changed.day.cutoff += 1;
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
    let mut retained_parts = receipt["input_components"].clone();
    retained_parts["day"] = retained_parts["period"].clone();
    let mut daily: prompt::Parts = serde_json::from_value(retained_parts).unwrap();
    assert!(
        Assignment::from_parts(daily.clone()).is_err(),
        "weekly generation is retired"
    );
    daily.day.end = daily.day.start + 86400;
    let assignment = Assignment::from_parts(daily).unwrap();
    let backend = crate::harness::providers::llama::LlamaClient::new(
        "http://localhost:8080",
        receipt["model_version"].as_str().unwrap(),
        Duration::from_secs(1),
        Some(false),
    )
    .unwrap();
    assert_ne!(assignment.parts.assemble(), receipt["packet"]);
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
        packet["fresh"]["reports"][0]["classifier_world"]["qualified_claims"][0]["target_relation"],
        "unknown"
    );
    assert_ne!(with_world.input_hash, assignment.input_hash);
}

#[tokio::test]
async fn an_incomplete_day_never_calls_the_model() {
    let mut captured = serde_json::from_str::<Value>(include_str!(
        "../../../fixtures/influencer/period-card-v19-smollm3.jsonl"
    ))
    .unwrap()["receipt"]["input_components"]
        .clone();
    captured["day"] = captured["period"].clone();
    let mut parts: prompt::Parts = serde_json::from_value(captured).unwrap();
    parts.day.end = parts.day.start + 86400;
    parts.excluded.push(json!({"article_id":999,"classification_id":999,"in_day":true,"reason":"packet_budget_exceeded"}));
    let model = Model {
        calls: AtomicUsize::new(0),
        reply: "must not be used",
    };
    let (output, receipt) = create(&model, &Assignment::from_parts(parts).unwrap(), 32768)
        .await
        .unwrap();
    assert!(output.is_none());
    assert_eq!(model.calls.load(Ordering::Relaxed), 0);
    assert_eq!(receipt["reason"], "incomplete_reporting_day");
}

#[test]
fn output_prose_cannot_exceed_the_native_budget_contract() {
    let oversized =
        json!({"score":75,"headline":"Hope","body":"x".repeat(prompt::BODY_MAX_CHARS+1)});
    assert!(VibeParser.parse(&oversized.to_string()).is_err());
    assert_eq!(
        prompt::schema()["oneOf"][0]["properties"]["body"]["maxLength"],
        prompt::BODY_MAX_CHARS
    );
}

#[tokio::test]
#[ignore = "requires disposable migrated classifier_test_coverage database; run serially"]
async fn publication_retry_reuses_generation_and_fences_stale_claims() -> Result<()> {
    use crate::harness::queue::work;
    use crate::plugins::classifier;
    let pool = crate::harness::dbtest::pool("disposable publication database").await;
    classifier::plumbing_tests::setup_disposable(&pool).await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    ensure!(
        database.starts_with("classifier_test_coverage"),
        "requires disposable coverage database"
    );
    let sport = "ZZ_VIBE_RETRY";
    sqlx::raw_sql("UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL;
        DELETE FROM application_outbox WHERE sport='ZZ_VIBE_RETRY';
        DELETE FROM pipeline_work WHERE sport='ZZ_VIBE_RETRY';
        DELETE FROM vibe_card_attempts WHERE sport='ZZ_VIBE_RETRY';
        DELETE FROM vibe_scores WHERE sport='ZZ_VIBE_RETRY';
        DELETE FROM news_articles WHERE id=921001;
        DELETE FROM entity_name_surfaces WHERE sport='ZZ_VIBE_RETRY';
        INSERT INTO sports(id,display_name,current_season) VALUES('ZZ_VIBE_RETRY','Vibe retry',2026) ON CONFLICT DO NOTHING;
        INSERT INTO teams(id,sport,name) VALUES(921001,'ZZ_VIBE_RETRY','Cedar FC') ON CONFLICT DO NOTHING;
        INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',921001,'ZZ_VIBE_RETRY','cedar fc','name');
        INSERT INTO news_articles(id,url,url_hash,title,source,published_at) VALUES(921001,'https://example.invalid/vibe-retry',md5('vibe-retry'),'Cedar FC supporters express hope','Fixture Wire',now()-interval '1 minute');
        INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES(921001,'team',921001,'ZZ_VIBE_RETRY');
        SELECT classifier_enqueue_admission(921001,'ZZ_VIBE_RETRY');").execute(&pool).await?;
    let scorer = classifier::tests::Model::new()?;
    let handler = classifier::adapter::ClassifierHandler::new(pool.clone(), scorer.decision()?);
    for item in work::claim(&pool, classifier::manifest::TASK, 1000).await? {
        if item.sport == sport {
            handler.execute(&item).await?;
        } else {
            work::release(&pool, &item).await?;
        }
    }
    let mut selected = None;
    for item in work::claim(&pool, manifest::TASK, 1000).await? {
        if item.sport == sport {
            selected = Some(item);
        } else {
            work::release(&pool, &item).await?;
        }
    }
    let item = selected.context("Vibe fixture claim")?;
    let model = Model {
        calls: AtomicUsize::new(0),
        reply: r#"{"score":75,"headline":"Supporters express hope","body":"Cedar FC supporters express hope."}"#,
    };
    // Fail after inference, inside the real product transaction and migrated constraints.
    sqlx::raw_sql("CREATE OR REPLACE FUNCTION public.test_vibe_publication_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.sport='ZZ_VIBE_RETRY' THEN RAISE EXCEPTION 'injected publication failure'; END IF; RETURN NEW; END $$;
        CREATE TRIGGER test_vibe_publication_failure BEFORE INSERT ON vibe_scores FOR EACH ROW EXECUTE FUNCTION public.test_vibe_publication_failure();").execute(&pool).await?;
    let failed = execute_with_backend(&pool, &model, 7900, &item).await;
    sqlx::raw_sql("DROP TRIGGER test_vibe_publication_failure ON vibe_scores; DROP FUNCTION public.test_vibe_publication_failure();").execute(&pool).await?;
    assert!(format!("{:#}", failed.unwrap_err()).contains("injected publication failure"));
    assert_eq!(model.calls.load(Ordering::Relaxed), 1);
    let count = |table: &'static str| crate::harness::dbtest::count(&pool, table, sport);
    assert_eq!(count("vibe_scores").await, 0);
    assert_eq!(count("application_outbox").await, 0);
    let outcome: String =
        sqlx::query_scalar("SELECT outcome FROM vibe_card_attempts WHERE sport=$1")
            .bind(sport)
            .fetch_one(&pool)
            .await?;
    assert_eq!(outcome, "generated");
    let hash: String =
        sqlx::query_scalar("SELECT input_hash FROM vibe_card_attempts WHERE sport=$1")
            .bind(sport)
            .fetch_one(&pool)
            .await?;
    assert!(reusable_attempt(&pool, &hash, "different-model")
        .await?
        .is_none());
    assert!(
        reusable_attempt(&pool, "different-input-or-contract", model.model())
            .await?
            .is_none()
    );
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.sport=$1 AND d.character='influencer' AND d.status='pending'").bind(sport).fetch_one(&pool).await?;
    assert_eq!(pending, 1);
    work::release(&pool, &item).await?;
    assert_eq!(
        execute_with_backend(&pool, &model, 7900, &item).await?,
        PluginOutcome::Superseded
    );
    assert_eq!(count("vibe_scores").await, 0);
    let mut selected = None;
    for claimed in work::claim(&pool, manifest::TASK, 1000).await? {
        if claimed.sport == sport {
            selected = Some(claimed);
        } else {
            work::release(&pool, &claimed).await?;
        }
    }
    let current = selected.context("reclaimed Vibe")?;
    assert_ne!(current.claim_token, item.claim_token);
    assert_eq!(
        execute_with_backend(&pool, &model, 7900, &current).await?,
        PluginOutcome::Committed
    );
    assert_eq!(model.calls.load(Ordering::Relaxed), 1);
    assert_eq!(count("vibe_scores").await, 1);
    assert_eq!(count("vibe_card_attempts").await, 1);
    assert_eq!(count("application_outbox").await, 1);
    let saved: (String, String, String, String) = sqlx::query_as("SELECT v.trigger_type,a.outcome,d.status,o.kind FROM vibe_card_attempts a JOIN vibe_scores v ON v.id=a.product_id JOIN classifier_spectra x ON x.sport=v.sport JOIN classifier_spectrum_deliveries d ON d.spectrum_id=x.id AND d.character='influencer' JOIN application_outbox o ON o.sport=v.sport WHERE v.sport=$1").bind(sport).fetch_one(&pool).await?;
    assert_eq!(
        saved,
        (
            "periodic".into(),
            "published".into(),
            "used".into(),
            "vibe_completed".into()
        )
    );
    assert_eq!(
        execute_with_backend(&pool, &model, 7900, &current).await?,
        PluginOutcome::Superseded
    );
    assert_eq!(count("vibe_scores").await, 1);
    assert_eq!(count("application_outbox").await, 1);
    sqlx::query("DELETE FROM application_outbox WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    Ok(())
}
