#[tokio::test]
#[ignore = "requires disposable migrated classifier_test_coverage database; run serially"]
async fn publication_retry_reuses_generation_and_fences_stale_claims() -> Result<()> {
    use crate::harness::skills::classifier;
    use crate::harness::queue::work;
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
        SkillOutcome::Superseded
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
        SkillOutcome::Committed
    );
    assert_eq!(model.calls.load(Ordering::Relaxed), 1);
    assert_eq!(count("vibe_scores").await, 1);
    assert_eq!(count("vibe_card_attempts").await, 1);
    assert_eq!(count("application_outbox").await, 1);
    let original: Option<String> = sqlx::query_scalar("SELECT generation_claim_token::text FROM vibe_scores WHERE sport=$1").bind(sport).fetch_one(&pool).await?;
    assert_eq!(original, item.claim_token, "retain the original generation on publication retry");
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
        SkillOutcome::Superseded
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
