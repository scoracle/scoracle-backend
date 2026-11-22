#[tokio::test]
#[ignore = "requires SCORACLE_MEMORY_STUDY_BIN"]
async fn rust_sort_matches_existing_duckdb_for_ties_and_window_edges() -> Result<()> {
    let before = 800000;
    let candidates: Vec<_> = (1..=30)
        .map(|id| {
            let at = match id {
                1 => before,
                2 => before - 7 * 86400 - 1,
                3 => before - 7 * 86400,
                _ => before - id / 2,
            };
            (
                Headline {
                    article_id: id,
                    headline: format!("Report {id}"),
                    publisher: "Wire".into(),
                    published_at: crate::util::utc_timestamp(at),
                    url: format!("https://example.invalid/{id}"),
                    spectrum: json!({"subject":id%3+90,"emotion":id%4+80,"movement":id%5+70}),
                },
                at,
            )
        })
        .collect();
    for character in ["journalist", "influencer", "insider"] {
        let request = json!({"kind":"headline_memory","version":"headline-spectrum-memory-v1",
            "character":character,"from":before-7*86400,"before":before,"limit":20,
            "observations":candidates.iter().map(|(h,at)| json!({"article_id":h.article_id,"reported_at":at,"spectrum":h.spectrum})).collect::<Vec<_>>()});
        let expected: Vec<i64> = crate::tools::memories::run(&request).await?;
        let actual: Vec<i64> = select(character, before, candidates.clone())
            .await?
            .into_iter()
            .map(|h| h.article_id)
            .collect();
        assert_eq!(actual, expected);
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires a disposable classifier_test database via TEST_DATABASE_URL"]
async fn stored_headline_archive_respects_character_day_and_freeze() -> anyhow::Result<()> {
    let pool = crate::harness::dbtest::pool("disposable classifier_test database").await;
    super::super::plumbing_tests::setup_disposable(&pool).await?;
    let mut tx = pool.begin().await?;
    let records = records();
    let subject: EntityMeta = serde_json::from_value(records[0]["subject"].clone())?;
    sqlx::query("INSERT INTO teams(id,sport,name) VALUES(20,'NBA','New York Knicks') ON CONFLICT DO NOTHING").execute(&mut *tx).await?;
    sqlx::query("INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',20,'NBA','new york knicks','name'),('team',20,'NBA','knicks','alias') ON CONFLICT DO NOTHING").execute(&mut *tx).await?;
    let mut spectrum_ids = Vec::new();
    for record in &records {
        let id = record["article_id"].as_i64().unwrap();
        let headline = record["headline"].as_str().unwrap();
        let url = record["url"].as_str().unwrap();
        sqlx::query("INSERT INTO news_articles(id,url_hash,url,title,source,published_at) VALUES($1,$2,$3,$4,$5,$6::text::timestamptz)")
            .bind(id).bind(super::super::hash(url)).bind(url).bind(headline)
            .bind(record["publisher"].as_str().unwrap()).bind(record["published_at"].as_str().unwrap()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO news_article_entities(article_id,entity_type,entity_id,sport) VALUES($1,'team',20,'NBA')").bind(id).execute(&mut *tx).await?;
        let identities: Value =
            sqlx::query_scalar("SELECT classifier_identity_candidates('NBA',$1,'')")
                .bind(headline)
                .fetch_one(&mut *tx)
                .await?;
        let retained = json!({"article_id":id,"body":"","title":headline,"source":record["publisher"],
            "published_at":record["published_at"],"url":url,"query_entities":[subject],"identity_candidates":identities,"evidence_scope":"headline"});
        let native =
            super::super::decision::Measurement::from_receipt(record["receipt"].clone())?;
        let source_id:i64=sqlx::query_scalar("INSERT INTO classifier_sources(article_id,sport,input_hash,body_sha256,source,discovery_version) VALUES($1,'NBA',$2,$3,$4,classifier_discovery_version($1,'NBA')) RETURNING id")
            .bind(id).bind(&native.request_hash).bind(super::super::hash("")).bind(retained).fetch_one(&mut *tx).await?;
        let gate = json!({"coverage":"complete","admitted":super::super::decision::admitted(&native.spectrum)?,
            "characters":super::super::decision::characters(&native.spectrum)?});
        let spectrum_id:i64=sqlx::query_scalar("INSERT INTO classifier_spectra(source_id,article_id,sport,target,model,criteria_version,scrub_version,clean_text,clean_sha256,spectrum,gate,request_hash,receipt,created_at)
            VALUES($1,$2,'NBA',$3,$4,$5,'rss-headline-spectrum-v1',$6,$7,$8,$9,$10,$11,to_timestamp(1792382459)) RETURNING id")
            .bind(source_id).bind(id).bind(json!({"entity_type":"team","entity_id":20,"sport":"NBA"}))
            // Mixed versions preserve pending deliveries and memories across model/prompt cutovers.
            .bind(native.model).bind(match spectrum_ids.len() % 5 {
                0 => super::super::decision::CRITERIA_VERSION,
                1 => "classification-criteria-v12-newsworthy",
                2 => "classification-criteria-v11-targeted",
                3 => "classification-criteria-v10-availability",
                _ => "classification-criteria-v9-headlines",
            }).bind(headline).bind(super::super::hash(headline))
            .bind(json!(native.spectrum)).bind(gate).bind(native.request_hash).bind(native.receipt).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO classifier_source_heads(article_id,sport,source_id) VALUES($1,'NBA',$2)").bind(id).bind(source_id).execute(&mut *tx).await?;
        if spectrum_ids.len() >= 4 {
            sqlx::query("INSERT INTO classifier_spectrum_deliveries(spectrum_id,character) VALUES($1,'journalist')").bind(spectrum_id).execute(&mut *tx).await?;
        }
        spectrum_ids.push(spectrum_id);
    }
    sqlx::query("INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,frozen_at,status)
        VALUES('2026-10-19',to_timestamp(1792382400),to_timestamp(1792383300),to_timestamp(1792382460),'characters')
        ON CONFLICT(edition_date) DO UPDATE SET frozen_at=EXCLUDED.frozen_at").execute(&mut *tx).await?;
    sqlx::query("UPDATE coverage_schedule SET enabled=true,enrolled_at=to_timestamp(1792296000),phase='characters',edition_date='2026-10-19'").execute(&mut *tx).await?;
    sqlx::query("INSERT INTO coverage_assignments(edition_date,stage,entity_type,entity_id,sport,spectrum_ids)
        VALUES('2026-10-19','narratives','team',20,'NBA',$1)").bind(&spectrum_ids[4..]).execute(&mut *tx).await?;
    let fresh = super::super::delivery::load_for_character_on(
        &mut tx,
        crate::harness::skills::journalist::manifest::MANIFEST
            .id
            .as_str(),
        "team",
        20,
        "NBA",
    )
    .await?;
    assert_eq!(fresh.len(), 2);
    assert!(fresh.iter().all(|s| s.context == s.headline));
    let memories = load_on(&mut tx, &subject, &fresh, "journalist").await?;
    assert_eq!(memories.len(), 4);
    sqlx::query("SAVEPOINT reviewed_memory")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO classifier_spectrum_deliveries(spectrum_id,character,status) VALUES($1,'journalist','used')")
        .bind(spectrum_ids[0]).execute(&mut *tx).await?;
    let unread = load_on(&mut tx, &subject, &fresh, "journalist").await?;
    assert_eq!(unread.len(), 3);
    assert!(unread
        .iter()
        .all(|m| m.article_id != records[0]["article_id"].as_i64().unwrap()));
    sqlx::query("ROLLBACK TO SAVEPOINT reviewed_memory")
        .execute(&mut *tx)
        .await?;
    let mut full_packet = fresh.clone();
    full_packet.resize(9, fresh[0].clone());
    assert_eq!(
        load_on(&mut tx, &subject, &full_packet, "journalist")
            .await?
            .len(),
        1
    );
    full_packet.push(fresh[0].clone());
    assert!(load_on(&mut tx, &subject, &full_packet, "journalist")
        .await?
        .is_empty());
    let assignment = crate::harness::skills::journalist::fresh::prepare(
        subject.clone(),
        fresh
            .iter()
            .map(crate::harness::skills::journalist::fresh::CorpusItem::from)
            .collect(),
        &[],
        1792382460,
    )?;
    let packet: Value = serde_json::from_str(
        &crate::harness::skills::journalist::request::prompt(&assignment),
    )?;
    assert_eq!(packet["evidence"].as_array().unwrap().len(), 2);
    assert!(packet.get("memories").is_none());
    for report in packet["evidence"].as_array().unwrap() {
        assert!(report.get("publisher_excerpt").is_none());
        assert!(fresh.iter().any(|s| report["headline"] == s.headline));
    }
    let influencer = crate::harness::skills::influencer::fresh::load(
        &mut tx,
        subject.clone(),
        Day {
            start: 1792296000,
            end: 1792382400,
            cutoff: 1792382460,
        },
    )
    .await?;
    assert!(
        influencer.sources.is_empty(),
        "non-emotional headlines cannot leak from Journalist's selection"
    );
    assert!(memories
        .iter()
        .all(|m| !fresh.iter().any(|s| s.article_id == m.article_id)));
    assert!(memories.iter().all(|m| m.published_at.ends_with('Z')));
    assert!(load_on(&mut tx, &subject, &fresh, "influencer")
        .await?
        .is_empty());
    sqlx::query(
        "UPDATE classifier_spectra SET created_at=to_timestamp(1792382461) WHERE id=$1",
    )
    .bind(spectrum_ids[0])
    .execute(&mut *tx)
    .await?;
    let frozen = load_on(&mut tx, &subject, &fresh, "journalist").await?;
    assert_eq!(frozen.len(), 3);
    assert!(frozen
        .iter()
        .all(|m| m.article_id != records[0]["article_id"].as_i64().unwrap()));
    sqlx::query("INSERT INTO pipeline_work(stage,entity_type,entity_id,sport,input_version)
        VALUES('classifier','article',$1,'NBA',classifier_discovery_version($1,'NBA')) ON CONFLICT DO NOTHING")
        .bind(records[0]["article_id"].as_i64().unwrap()).execute(&mut *tx).await?;
    let offered: i64 = sqlx::query_scalar("SELECT classifier_replay_admission(1)")
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(
        offered, 0,
        "historical replay must leave room for existing runnable work"
    );
    tx.rollback().await?;
    Ok(())
}
