#[tokio::test]
#[ignore = "requires disposable classifier_test_coverage database; run serially"]
async fn availability_routes_freezes_and_settles_without_inventing_a_profile() -> anyhow::Result<()>
{
    use crate::harness::skills::{classifier, scout};
    use crate::harness::{queue::work, skill::Skill};
    let pool = crate::harness::dbtest::pool("disposable availability database").await;
    let db: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        db.starts_with("classifier_test_coverage"),
        "disposable database required"
    );
    sqlx::raw_sql("UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL;
      DELETE FROM pipeline_work WHERE sport='ZZ_AVAIL'; DELETE FROM application_outbox WHERE sport='ZZ_AVAIL';
      DELETE FROM stat_summaries WHERE sport='ZZ_AVAIL'; DELETE FROM news_articles WHERE id=923001;
      DELETE FROM entity_name_surfaces WHERE sport='ZZ_AVAIL';
      INSERT INTO sports(id,display_name,current_season) VALUES('ZZ_AVAIL','Availability fixture',2026) ON CONFLICT DO NOTHING;
      INSERT INTO teams(id,sport,name) VALUES(923001,'ZZ_AVAIL','Cedar FC') ON CONFLICT DO NOTHING;
      INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',923001,'ZZ_AVAIL','cedar fc','name');
      INSERT INTO news_articles(id,url,url_hash,title,source,published_at) VALUES(923001,'https://example.invalid/availability',md5('availability'),'Cedar FC denies injury in availability fixture','Wire',now()-interval '1 minute');
      INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES(923001,'team',923001,'ZZ_AVAIL');
      SELECT classifier_enqueue_admission(923001,'ZZ_AVAIL');").execute(&pool).await?;
    let model = classifier::tests::Model::new()?;
    let handler = classifier::adapter::ClassifierHandler::new(pool.clone(), model.decision()?);
    for item in work::claim(&pool, classifier::manifest::TASK, 1000).await? {
        if item.sport == "ZZ_AVAIL" {
            handler.execute(&item).await?;
        } else {
            work::release(&pool, &item).await?;
        }
    }
    let mut claim = None;
    for item in work::claim(&pool, scout::manifest::TASK, 1000).await? {
        if item.sport == "ZZ_AVAIL" {
            claim = Some(item);
        } else {
            work::release(&pool, &item).await?;
        }
    }
    let claim = claim.ok_or_else(|| anyhow::anyhow!("missing Scout availability claim"))?;
    assert!(claim
        .input_version
        .as_deref()
        .unwrap()
        .starts_with(classifier::AVAILABILITY_CONTRACT));
    let req = scout::request::RatingReq {
        entity_type: "team".into(),
        entity_id: 923001,
        sport: "ZZ_AVAIL".into(),
        season: Some(2026),
        entity_name: "Cedar FC".into(),
        trigger_type: "daily".into(),
    };
    let mut tx = pool.begin().await?;
    let prepared = scout::request::build_rating_request_on(&mut tx, 7900, &req, 0.0, false).await?;
    let scout::request::RatingBuild::NoStats { availability, .. } = prepared else {
        panic!("headline invented a profile")
    };
    assert_eq!(availability.sources.len(), 1);
    tx.rollback().await?;
    let mut publication =
        crate::harness::queue::publication::ClaimPublication::begin(&pool, &claim)
            .await?
            .unwrap();
    classifier::delivery::validate_for_publication(
        publication.transaction(),
        scout::manifest::MANIFEST.id.as_str(),
        "team",
        923001,
        "ZZ_AVAIL",
        &availability.sources,
    )
    .await?;
    availability
        .settle(publication.transaction(), &claim, false)
        .await?;
    publication.commit_final().await?;
    let status:String=sqlx::query_scalar("SELECT d.status FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.sport='ZZ_AVAIL' AND d.character='scout'").fetch_one(&pool).await?;
    assert_eq!(status, "abstained");
    let products: i64 =
        sqlx::query_scalar("SELECT count(*) FROM stat_summaries WHERE sport='ZZ_AVAIL'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(products, 0);
    sqlx::raw_sql("DELETE FROM pipeline_work WHERE sport='ZZ_AVAIL'; DELETE FROM application_outbox WHERE sport='ZZ_AVAIL';").execute(&pool).await?;
    Ok(())
}
