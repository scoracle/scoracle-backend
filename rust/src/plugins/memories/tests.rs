use super::*;

/// TEMP tables shadow the source relations on one isolated connection. Neither
/// this test nor the production study writes the canonical world. The storyline
/// tables are deliberately absent: the shared study must not read them, and the
/// topic is supplied by the caller.
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
    // A caller-resolved candidate set replaces the study's own relationship
    // lookups. The caller owns the predicate vocabulary and the name rule; the
    // study only ranks what it is handed.
    let pair_articles: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT e.article_id FROM narrative_events e
         WHERE e.sport=$1 AND e.origin='extraction' AND e.predicate=$2
         AND e.subject_type='team' AND e.subject_id=$3
         AND e.object_type='player' AND e.object_id=$4
         AND e.event_date>=to_timestamp(100) AND e.event_date<to_timestamp(200)
         AND strpos(' '||public.nrm(a.title)||' ',' test player ')>0
         FROM news_articles a WHERE a.id=e.article_id",
    )
    .bind(&subject.sport)
    .bind("trade_rumor")
    .bind(subject.entity_id)
    .bind(99i32)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(pair_articles, vec![101, 102, 103]);
    let first = reporting_scope(&pool, &subject, 100, 200, &[], 3, &pair_articles, None)
        .await
        .unwrap();
    assert_eq!(first.receipt.included_articles, 3);
    // Three articles, two canonical after the repost folds into 101, one
    // subject-wide group because the study no longer knows about storylines.
    assert_eq!(first.findings.len(), 1);
    assert_eq!(first.findings[0].topic, "article/101");
    assert_eq!(first.findings[0].article_count, 2);
    assert_eq!(first.findings[0].publisher_count, 2);
    assert_eq!(first.findings[0].source_ids, vec![101, 102]);
    // The same study regrouped by a plugin-supplied topic. Grouping is the
    // plugin's decision; the study runs once either way.
    let storyline = |o: &Observation| Some(format!("storyline/{}", o.canonical_id / 100));
    let grouped = reporting_scope(
        &pool,
        &subject,
        100,
        200,
        &[],
        3,
        &pair_articles,
        Some(&storyline),
    )
    .await
    .unwrap();
    assert_ne!(grouped.receipt.input_hash, first.receipt.input_hash);
    assert!(grouped
        .findings
        .iter()
        .all(|f| f.topic.starts_with("storyline/")));
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
    let changed = reporting_scope(&pool, &subject, 100, 200, &[], 3, &pair_articles, None)
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
