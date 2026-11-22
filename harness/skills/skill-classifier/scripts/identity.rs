//! Stable canonical name sets; ambiguous embedded names remain unresolved.
use super::Source;
use anyhow::{ensure, Result};
use serde_json::Value;
use sqlx::PgConnection;

pub(crate) async fn candidates(
    connection: &mut PgConnection,
    sport: &str,
    source: &Source,
) -> Result<Value> {
    Ok(
        candidates_for(connection, sport, std::slice::from_ref(source))
            .await?
            .remove(0),
    )
}

pub(crate) async fn candidates_for(
    connection: &mut PgConnection,
    sport: &str,
    sources: &[Source],
) -> Result<Vec<Value>> {
    let titles: Vec<_> = sources
        .iter()
        .map(|s| s.provenance["title"].as_str().unwrap_or_default())
        .collect();
    let found: Vec<Value> = sqlx::query_scalar(
        "SELECT public.classifier_identity_candidates($1,title,'')
         FROM unnest($2::text[]) WITH ORDINALITY AS input(title,n) ORDER BY n",
    )
    .bind(sport)
    .bind(titles)
    .fetch_all(connection)
    .await?;
    ensure!(
        found.len() == sources.len()
            && found
                .iter()
                .all(|v| v.as_array().is_some_and(|a| a.len() <= 128)),
        "canonical identity population exceeds bound"
    );
    Ok(found)
}

pub(crate) async fn validate(
    connection: &mut PgConnection,
    sport: &str,
    source: &Source,
) -> Result<()> {
    validate_all(connection, sport, std::slice::from_ref(source)).await
}

pub(crate) async fn validate_all(
    connection: &mut PgConnection,
    sport: &str,
    sources: &[Source],
) -> Result<()> {
    if sources.is_empty() {
        return Ok(());
    }
    // Surface refresh deletes/rebuilds the index; lock once for the complete publication.
    sqlx::query("LOCK TABLE public.entity_name_surfaces IN SHARE MODE NOWAIT")
        .execute(&mut *connection)
        .await?;
    let found = candidates_for(connection, sport, sources).await?;
    for (kind, table) in [
        ("team", "teams"),
        ("player", "players"),
        ("person", "persons"),
    ] {
        let ids: Vec<i32> = found
            .iter()
            .flat_map(|v| v.as_array().unwrap())
            .filter(|v| v["entity_type"] == kind)
            .filter_map(|v| {
                v["entity_id"]
                    .as_i64()
                    .and_then(|id| i32::try_from(id).ok())
            })
            .collect();
        sqlx::query(&format!("SELECT id FROM public.{table} WHERE sport=$1 AND id=ANY($2) ORDER BY id FOR SHARE NOWAIT"))
            .bind(sport).bind(ids).fetch_all(&mut *connection).await?;
    }
    ensure!(
        candidates_for(connection, sport, sources).await? == found
            && found
                .iter()
                .zip(sources)
                .all(|(current, source)| *current == source.provenance["identity_candidates"]),
        "Classifier canonical identities changed; headline scoring must reconcile them"
    );
    Ok(())
}
