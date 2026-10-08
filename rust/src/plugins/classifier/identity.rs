//! Exact canonical names only; aliases and ambiguous surfaces remain unresolved.
use super::Source;
use anyhow::{ensure, Result};
use serde_json::Value;
use sqlx::PgConnection;

pub(crate) async fn candidates(
    connection: &mut PgConnection,
    sport: &str,
    source: &Source,
) -> Result<Value> {
    let found: Value = sqlx::query_scalar("SELECT public.classifier_identity_candidates($1,$2,$3)")
        .bind(sport)
        .bind(source.provenance["title"].as_str().unwrap_or_default())
        .bind(&source.body)
        .fetch_one(connection)
        .await?;
    ensure!(
        found.as_array().is_some_and(|a| a.len() <= 128),
        "canonical identity population exceeds bound"
    );
    Ok(found)
}

pub(crate) async fn validate(
    connection: &mut PgConnection,
    sport: &str,
    source: &Source,
) -> Result<()> {
    // Surface refresh deletes/rebuilds the index. Fail fast rather than reading half a refresh.
    sqlx::query("LOCK TABLE public.entity_name_surfaces IN SHARE MODE NOWAIT")
        .execute(&mut *connection)
        .await?;
    let found = candidates(connection, sport, source).await?;
    for (kind, table) in [
        ("team", "teams"),
        ("player", "players"),
        ("person", "persons"),
    ] {
        let ids: Vec<i32> = found
            .as_array()
            .unwrap()
            .iter()
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
        candidates(connection, sport, source).await? == source.provenance["identity_candidates"],
        "Classifier canonical identities changed; acquisition must reconcile them"
    );
    Ok(())
}
