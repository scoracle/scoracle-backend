use super::*;

#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL"]
async fn metadata_updates_only_the_requested_sport_for_a_shared_id() -> Result<()> {
    let pool = crate::harness::dbtest::pool("isolated metadata boundary test").await;
    let mut tx = pool.begin().await?;
    let id = 9_699_001;
    let doc = 9_699_002;
    for sport in ["ZZ_META_A", "ZZ_META_B"] {
        sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,$1,2026)")
            .bind(sport)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO teams(id,sport,name,venue_name,logo_url) VALUES($1,$2,'Club','Original Arena','https://example.test/original')").bind(id).bind(sport).execute(&mut *tx).await?;
        sqlx::query(
            "INSERT INTO players(id,sport,name,date_of_birth) VALUES($1,$2,'Player','1999-01-01')",
        )
        .bind(id)
        .bind(sport)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("INSERT INTO source_documents(id,url,retained_excerpt) VALUES($1,'https://example.test/metadata-boundary','Club Player')").bind(doc).execute(&mut *tx).await?;
    publish_team(
        &mut tx,
        id,
        "ZZ_META_A",
        &Some(("New Arena".into(), doc)),
        &Some(("https://example.test/new".into(), doc)),
    )
    .await?;
    publish_player(
        &mut tx,
        id,
        "ZZ_META_A",
        &WikidataItem {
            qid: "QBOUNDARY".into(),
            date_of_birth: Some("+1990-01-01T00:00:00Z".into()),
            source_document_id: doc,
            ..Default::default()
        },
    )
    .await?;
    let teams: Vec<(String, String, String)> =
        sqlx::query_as("SELECT sport,venue_name,logo_url FROM teams WHERE id=$1 ORDER BY sport")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
    assert_eq!(
        teams,
        vec![
            (
                "ZZ_META_A".into(),
                "New Arena".into(),
                "https://example.test/new".into()
            ),
            (
                "ZZ_META_B".into(),
                "Original Arena".into(),
                "https://example.test/original".into()
            )
        ]
    );
    let players: Vec<(String, String)> =
        sqlx::query_as("SELECT sport,date_of_birth::text FROM players WHERE id=$1 ORDER BY sport")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
    assert_eq!(
        players,
        vec![
            ("ZZ_META_A".into(), "1990-01-01".into()),
            ("ZZ_META_B".into(), "1999-01-01".into())
        ]
    );
    tx.rollback().await?;
    Ok(())
}
