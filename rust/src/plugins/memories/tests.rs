use super::*;

/// TEMP tables shadow the source relations on one isolated connection. Neither
/// this test nor the production study writes the canonical world.
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and SCORACLE_MEMORY_STUDY_BIN"]
async fn scoped_studies_preserve_frequency_identity_missingness_and_corrections() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    for ddl in [
        "CREATE TEMP TABLE narrative_events(sport text,origin text,article_id bigint,subject_type text,subject_id int,object_type text,object_id int,predicate text,event_date timestamptz)",
        "CREATE TEMP TABLE news_articles(id bigint,title text,source text,published_at timestamptz,duplicate_of bigint)",
        "CREATE TEMP TABLE entity_name_surfaces(sport text,entity_type text,entity_id int,norm text)",
        "CREATE TEMP TABLE storyline_articles(article_id bigint,storyline_id bigint)",
        "CREATE TEMP TABLE storyline_entities(storyline_id bigint,sport text,entity_type text,entity_id int)",
        "CREATE TEMP TABLE fixtures(id int,sport text,home_team_id int,away_team_id int,league_id int,season int,status text,meta jsonb,start_time timestamptz)",
        "CREATE TEMP TABLE event_team_stats(fixture_id int,team_id int,sport text,league_id int,season int,stats jsonb)",
        "CREATE TEMP TABLE stat_definitions(sport text,entity_type text,key_name text,unit text,display_name text)",
    ] { sqlx::query(ddl).execute(&pool).await.unwrap(); }
    sqlx::raw_sql("INSERT INTO news_articles VALUES
        (101,'Test Team linked with Test Player','One',to_timestamp(110),NULL),
        (102,'Test Team and Test Player update','Two',to_timestamp(120),NULL),
        (103,'Test Team and Test Player repost','Three',to_timestamp(130),101),
        (104,'Other Team linked with Other Player','Four',to_timestamp(140),NULL),
        (105,'Test Team and Test Player fresh','One',to_timestamp(200),NULL);
        INSERT INTO narrative_events SELECT 'FOOTBALL','extraction',id,'team',18,'player',99,'trade_rumor',published_at FROM news_articles;
        INSERT INTO narrative_events SELECT * FROM narrative_events WHERE article_id=101;
        INSERT INTO storyline_articles SELECT id,1 FROM news_articles;
        INSERT INTO storyline_entities VALUES(1,'FOOTBALL','team',18);
        INSERT INTO fixtures VALUES
        (1,'FOOTBALL',18,19,8,2026,'completed','{}',to_timestamp(110)),
        (2,'FOOTBALL',19,18,8,2026,'completed','{}',to_timestamp(210));
        INSERT INTO stat_definitions VALUES('FOOTBALL','team','expected_goals_for','cumulative_total','Expected Goals For (xG)');
        INSERT INTO event_team_stats VALUES(1,18,'FOOTBALL',8,2026,'{\"expected_goals_for\":1}'),(2,18,'FOOTBALL',8,2026,'{\"expected_goals_for\":3}');")
        .execute(&pool).await.unwrap();
    let subject = EntityMeta {
        name: "Test Team".into(),
        entity_type: "team".into(),
        entity_id: 18,
        sport: "FOOTBALL".into(),
    };
    let pair = EntityMeta {
        name: "Test Player".into(),
        entity_type: "player".into(),
        entity_id: 99,
        sport: "FOOTBALL".into(),
    };
    let first = reporting_scope(
        &pool,
        &subject,
        100,
        200,
        &[],
        3,
        Some(&pair),
        &["trade_rumor".into()],
    )
    .await
    .unwrap();
    assert_eq!(first.findings.len(), 1);
    assert_eq!(first.findings[0].article_count, 2);
    assert_eq!(first.findings[0].publisher_count, 2);
    assert_eq!(first.findings[0].source_ids, vec![101, 102]);
    let narrower = reporting(&pool, &subject, 115, 200, &[103], 3)
        .await
        .unwrap();
    assert_eq!(narrower.findings[0].source_ids, vec![102]);
    sqlx::query(
        "UPDATE news_articles SET title='Test Team and Test Player corrected report' WHERE id=102",
    )
    .execute(&pool)
    .await
    .unwrap();
    let changed = reporting_scope(
        &pool,
        &subject,
        100,
        200,
        &[],
        3,
        Some(&pair),
        &["trade_rumor".into()],
    )
    .await
    .unwrap();
    assert_ne!(first.receipt.input_hash, changed.receipt.input_hash);
    let stat = statistic::team_matches(
        &pool,
        &subject,
        "expected_goals_for",
        8,
        2026,
        100,
        200,
        300,
    )
    .await
    .unwrap();
    assert_eq!(stat.finding.per_match_change, Some(2.0));
    assert_eq!(stat.finding.percent_change, Some(200.0));
    sqlx::query("DELETE FROM event_team_stats WHERE fixture_id=2")
        .execute(&pool)
        .await
        .unwrap();
    let missing = statistic::team_matches(
        &pool,
        &subject,
        "expected_goals_for",
        8,
        2026,
        100,
        200,
        300,
    )
    .await
    .unwrap();
    assert_ne!(stat.input_hash, missing.input_hash);
    assert_eq!(missing.finding.current.fixtures, 1);
    assert_eq!(missing.finding.current.measured, 0);
    assert_eq!(missing.finding.per_match_change, None);
    pool.close().await;
}
