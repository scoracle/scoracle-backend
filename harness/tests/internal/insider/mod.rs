#[tokio::test]
#[ignore = "requires a disposable classifier_test database via TEST_DATABASE_URL"]
async fn known_owner_headline_publishes_a_person_card_and_fenced_source_outcome() -> Result<()>
{
    let pool = crate::harness::dbtest::pool("disposable classifier_test database").await;
    crate::harness::skills::classifier::plumbing_tests::setup_disposable(
        &pool,
    )
    .await?;
    let sport = "ZZ_PERSON_CARD";
    sqlx::query(
        "UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL",
    )
    .execute(&pool)
    .await?;
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM news_articles WHERE id=940101")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM insider_scores WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM transfer_rumors WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM application_outbox WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM entity_name_surfaces WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,'Person card',2026) ON CONFLICT DO NOTHING").bind(sport).execute(&pool).await?;
    sqlx::query("INSERT INTO teams(id,sport,name) VALUES(940101,$1,'Cleveland Browns') ON CONFLICT(id,sport) DO NOTHING").bind(sport).execute(&pool).await?;
    sqlx::query("INSERT INTO persons(id,sport,full_name,kind) VALUES(940101,$1,'Jordan Sample','owner') ON CONFLICT(id) DO UPDATE SET sport=EXCLUDED.sport,full_name=EXCLUDED.full_name,kind=EXCLUDED.kind").bind(sport).execute(&pool).await?;
    sqlx::query("INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',940101,$1,nrm('Cleveland Browns'),'name'),('person',940101,$1,nrm('Jordan Sample'),'name')").bind(sport).execute(&pool).await?;
    sqlx::query("INSERT INTO news_articles(id,url,url_hash,title,source,published_at) VALUES(940101,'https://example.invalid/person-card',md5('person-card'),'Cleveland Browns is monitoring Jordan Sample in trade talks; Cleveland Browns consider talks.','Wire',now())").execute(&pool).await?;
    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES(940101,'team',940101,$1)").bind(sport).execute(&pool).await?;
    sqlx::query("SELECT classifier_enqueue_admission(940101,$1)")
        .bind(sport)
        .execute(&pool)
        .await?;
    let scorer = crate::harness::skills::classifier::tests::Model::new()?;
    let handler =
        crate::harness::skills::classifier::adapter::ClassifierHandler::new(
            pool.clone(),
            scorer.decision()?,
        );
    let claim = crate::harness::queue::work::claim(
        &pool,
        crate::harness::skills::classifier::manifest::TASK,
        1,
    )
    .await?
    .remove(0);
    handler.execute(&claim).await?;
    let mut claimed = None;
    for item in crate::harness::queue::work::claim(&pool, manifest::TASK, 100).await? {
        if item.entity_type == "person" && item.entity_id == 940101 && item.sport == sport {
            claimed = Some(item);
        } else {
            crate::harness::queue::work::release(&pool, &item).await?;
        }
    }
    let item = claimed.context("person transfer claim")?;
    let material = fresh::load_material(&pool, &item).await?;
    assert_eq!(material.sources.len(), 1);
    let backend = Fake(Mutex::new(0), None);
    assert!(matches!(
        execute_prepared(&pool, &backend, 32768, &item, material).await?,
        SkillOutcome::Committed
    ));
    let (body, score): (String, i16) = sqlx::query_as("SELECT read, score FROM insider_scores WHERE sport=$1 AND entity_type='person' AND entity_id=940101").bind(sport).fetch_one(&pool).await?;
    assert!(body.contains("Jordan Sample"));
    assert_eq!(score, 10);
    let settled:i64=sqlx::query_scalar("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.sport=$1 AND x.target->>'entity_type'='person' AND d.character='insider' AND d.status='used' AND d.product_ref->>'score_id' IS NOT NULL").bind(sport).fetch_one(&pool).await?;
    assert_eq!(settled, 1);
    let reactions = crate::harness::registration::build_reactions(pool.clone())?;
    assert_eq!(
        crate::harness::queue::outbox::drain(&pool, &reactions, 100).await?,
        1
    );
    let statistics:i64=sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE sport=$1 AND entity_type='person' AND stage IN ('rating','momentum','sigil')").bind(sport).fetch_one(&pool).await?;
    assert_eq!(statistics, 0);
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    Ok(())
}
