use super::*;

#[tokio::test]
#[ignore = "requires migrated disposable classifier_test_coverage database; run serially"]
async fn named_people_and_explicit_league_category_reach_the_same_request_builder() -> Result<()> {
    let pool = crate::harness::dbtest::pool("disposable relationship context database").await;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    ensure!(
        database.starts_with("classifier_test_coverage"),
        "requires disposable database"
    );
    sqlx::raw_sql(r#"
        INSERT INTO sports(id,display_name,current_season) VALUES('ZZ_RELATIONS','Relation fixtures',2026);
        INSERT INTO leagues(id,sport,name,meta) VALUES(991901,'ZZ_RELATIONS','Fixture Cup','{"gender":"women"}');
        INSERT INTO teams(id,sport,name,league_id,meta) VALUES
            (991901,'ZZ_RELATIONS','Cedar FC',991901,'{}'),
            (991902,'ZZ_RELATIONS','Maple FC',991901,'{"gender":"men"}');
        INSERT INTO players(id,sport,name) VALUES(991903,'ZZ_RELATIONS','Alex Reed Jr.');
        INSERT INTO persons(id,sport,full_name,kind) VALUES(991904,'ZZ_RELATIONS','Alex Reed','player');
        INSERT INTO team_rosters(sport,season,team_id,player_id,last_seen)
            VALUES('ZZ_RELATIONS',2026,991901,991903,'2026-11-01T12:00:00Z');
        INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES
            ('player',991903,'ZZ_RELATIONS','alex reed jr','name'),
            ('person',991904,'ZZ_RELATIONS','alex reed','name');
    "#).execute(&pool).await?;
    let title = "Alex Reed Jr. may be rested";
    let people = named_people(&pool, title, "ZZ_RELATIONS").await?;
    assert_eq!(people.as_array().unwrap().len(), 1);
    assert_eq!(people[0]["name"], "Alex Reed Jr.");
    assert_eq!(people[0]["team"], "Cedar FC");
    let mut meta = subject_meta(
        &pool,
        &json!({"name":"Cedar FC","entity_type":"team","entity_id":991901,"sport":"ZZ_RELATIONS"}),
    )
    .await?;
    assert_eq!(meta["team_gender"], "women");
    assert_eq!(
        crate::tools::meta::load_team_gender(&pool, 991902, "ZZ_RELATIONS")
            .await?
            .as_deref(),
        Some("men")
    );
    meta["named_people"] = people;
    let request = super::super::request::request(title, "Wire", "ZZ_RELATIONS", &meta)?;
    assert!(request["state"]
        .as_str()
        .unwrap()
        .contains("recorded affiliation: this target (Cedar FC), observed 2026-11-01"));
    assert!(!request.to_string().contains("991903"));
    let both = named_people(&pool, "Alex Reed praises Alex Reed Jr.", "ZZ_RELATIONS").await?;
    assert_eq!(both.as_array().unwrap().len(), 2);
    for suffix in ["Jr.", "Sr.", "II", "III", "IV"] {
        let matched: bool = sqlx::query_scalar(
            "SELECT public.classifier_name_span_matches($1,'alex reed','person')",
        )
        .bind(format!("Alex Reed {suffix} returns"))
        .fetch_one(&pool)
        .await?;
        assert!(!matched, "{suffix}");
    }
    sqlx::raw_sql("DELETE FROM entity_name_surfaces WHERE sport='ZZ_RELATIONS'; DELETE FROM team_rosters WHERE sport='ZZ_RELATIONS'; DELETE FROM persons WHERE sport='ZZ_RELATIONS'; DELETE FROM players WHERE sport='ZZ_RELATIONS'; DELETE FROM teams WHERE sport='ZZ_RELATIONS'; DELETE FROM leagues WHERE sport='ZZ_RELATIONS'; DELETE FROM sports WHERE id='ZZ_RELATIONS';").execute(&pool).await?;
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires migrated disposable classifier_test_coverage database; run serially"]
async fn canonical_team_gender_reaches_the_decision_target() -> Result<()> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("TEST_DATABASE_URL")?)
        .await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    ensure!(
        database.starts_with("classifier_test_coverage"),
        "requires migrated disposable coverage test database"
    );
    sqlx::raw_sql(
        r#"
        INSERT INTO public.teams(id,sport,name,meta) VALUES
            (991801,'FOOTBALL','Cedar United','{"gender":"men"}'),
            (991802,'FOOTBALL','Cedar United','{"gender":"women"}'),
            (991803,'FOOTBALL','Cedar United','{}');
    "#,
    )
    .execute(&pool)
    .await?;
    for (id, gender, target) in [
        (
            991801,
            Some("men"),
            "the men's football team named Cedar United",
        ),
        (
            991802,
            Some("women"),
            "the women's football team named Cedar United",
        ),
        (991803, None, "Cedar United, the FOOTBALL team"),
    ] {
        let meta = subject_meta(
            &pool,
            &json!({"name":"Cedar United","entity_type":"team",
            "entity_id":id,"sport":"FOOTBALL"}),
        )
        .await?;
        let payload = crate::harness::skills::classifier::request::request(
            "Cedar United Women appoint Jo Vale",
            "Fixture Wire",
            "FOOTBALL",
            &meta,
        )?;
        let state: serde_json::Value = serde_json::from_str(payload["state"].as_str().unwrap())?;
        assert_eq!(
            state["target"]["team_category"].as_str(),
            Some(gender.unwrap_or("unknown"))
        );
        for question in payload["questions"].as_object().unwrap().values() {
            assert!(question["instructions"].as_str().unwrap().contains(target));
        }
    }
    sqlx::query("DELETE FROM public.teams WHERE sport='FOOTBALL' AND id BETWEEN 991801 AND 991803")
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}
