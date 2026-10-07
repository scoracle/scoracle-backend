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
    let world: Value = serde_json::from_str(&prompt::assembled_prompt(&a)).unwrap();
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
}

#[tokio::test]
#[ignore = "isolated migrated TEST_DATABASE_URL and SCORACLE_MEMORY_STUDY_BIN"]
async fn period_card_flow() -> Result<()> {
    use crate::harness::{dbtest, queue::work};
    let pool = dbtest::pool("migration 290").await;
    let sport = "ZZ_VIBE_PERIOD";
    dbtest::clean(
        &pool,
        sport,
        &[
            "pipeline_work",
            "application_outbox",
            "vibe_card_attempts",
            "vibe_scores",
            "harvester_classifications",
            "season_weeks",
        ],
        "Vibe period fixture",
    )
    .await;
    sqlx::query(
        "INSERT INTO teams(id,sport,name) VALUES(9690710,$1,'Cedar Comets') ON CONFLICT DO NOTHING",
    )
    .bind(sport)
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO season_weeks(sport,season,week_no,starts_at,ends_at) VALUES($1,2026,1,date_trunc('day',NOW())-INTERVAL '1 day',date_trunc('day',NOW())+INTERVAL '6 days')").bind(sport).execute(&pool).await?;
    for (id,text) in [(96907101,"Morgan said she was hopeful about Cedar Comets."),(96907102,"Ellis said he was apprehensive about Cedar Comets. No other supporters were interviewed.")] {
        sqlx::query("INSERT INTO news_articles(id,url_hash,url,title,source,full_text,published_at,fetched_at)
            VALUES($1,$2,$2,'Cedar Comets reactions','Example Wire',$3,NOW()-INTERVAL '2 hours',NOW()-INTERVAL '1 hour')
            ON CONFLICT(id) DO UPDATE SET full_text=EXCLUDED.full_text,published_at=EXCLUDED.published_at,fetched_at=EXCLUDED.fetched_at")
            .bind(id as i64).bind(format!("https://example.test/period/{id}")).bind(text).execute(&pool).await?;
        sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES($1,'team',9690710,$2) ON CONFLICT DO NOTHING").bind(id as i64).bind(sport).execute(&pool).await?;
        let cid:i64=sqlx::query_scalar("INSERT INTO harvester_classifications(article_id,entity_type,entity_id,sport,contract_version,model_revision,entity_choice,
            body_sha256,headline,model_input_start,model_input_end,model_input_text,context_start,context_end,context_text,distributions,model_provenance,created_at)
            SELECT id,'team',9690710,$2,'fixture','fixture','relevant',encode(sha256(convert_to(full_text,'UTF8')),'hex'),title,
            0,octet_length(full_text),full_text,0,octet_length(full_text),full_text,'{}','{}',NOW()-INTERVAL '1 minute' FROM news_articles WHERE id=$1 RETURNING id")
            .bind(id as i64).bind(sport).fetch_one(&pool).await?;
        sqlx::query("INSERT INTO harvester_assignments(classification_id,plugin_id) VALUES($1,$2)").bind(cid).bind(manifest::MANIFEST.id.as_str()).execute(&pool).await?;
    }
    // A known-copy flag cannot erase the second report's different reaction/qualification.
    sqlx::query("UPDATE news_articles SET duplicate_of=96907101 WHERE id=96907102")
        .execute(&pool)
        .await?;
    let item = dbtest::item(manifest::TASK, sport, 9690710, Some("period-fixture"));
    work::enqueue(&pool, &item).await?;
    let claim = dbtest::claim_one(&pool, manifest::TASK, "period").await;
    let model = Model {
        calls: AtomicUsize::new(0),
        reply: r#"{"score":50,"headline":"Hope and apprehension coexist","body":"Morgan was hopeful.\n\nEllis was apprehensive. No other supporters were interviewed."}"#,
    };
    assert_eq!(
        execute_with_backend(&pool, &model, 4096, &claim).await?,
        PluginOutcome::Committed
    );
    assert_eq!(model.calls.load(Ordering::Relaxed), 1);
    let (score, ids, week): (i16, Vec<i64>, i32) =
        sqlx::query_as("SELECT sentiment,input_news_ids,week_no FROM vibe_scores WHERE sport=$1")
            .bind(sport)
            .fetch_one(&pool)
            .await?;
    assert_eq!((score, ids, week), (50, vec![96907101, 96907102], 1));
    let used:i64=sqlx::query_scalar("SELECT count(*) FROM harvester_assignments d JOIN harvester_classifications c ON c.id=d.classification_id WHERE c.sport=$1 AND d.status='used'").bind(sport).fetch_one(&pool).await?;
    assert_eq!(used, 2);
    // A repeated delivery with unchanged receipts reuses the coherent product.
    sqlx::query("UPDATE harvester_assignments SET status='pending' WHERE classification_id IN (SELECT id FROM harvester_classifications WHERE sport=$1)").bind(sport).execute(&pool).await?;
    work::enqueue(&pool, &item).await?;
    let claim = dbtest::claim_one(&pool, manifest::TASK, "repeat").await;
    assert_eq!(
        execute_with_backend(&pool, &model, 4096, &claim).await?,
        PluginOutcome::Committed
    );
    assert_eq!(model.calls.load(Ordering::Relaxed), 1);
    assert_eq!(dbtest::count(&pool, "vibe_scores", sport).await, 1);
    pool.close().await;
    Ok(())
}
