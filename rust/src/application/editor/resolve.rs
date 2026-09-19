use crate::studio::editor::{derive::*, NameMention};
use anyhow::{Context, Result};
use sqlx::{PgConnection, Row};
/// resolve_names is the automatic link path: EXACT match on `nrm()` surfaces, sport-scoped,
/// nothing else (T9 — trigram ranks for review, never writes). `public.nrm()` is called IN SQL
/// on purpose: the database owns the one normalizer, and a Rust copy that drifts from it is the
/// failure mode this single database normalizer avoids.
pub async fn resolve_names(
    conn: &mut PgConnection,
    sport: &str,
    names: &[NameMention],
) -> Result<Resolved> {
    if names.is_empty() {
        return Ok(Resolved::default());
    }
    let raw: Vec<String> = names.iter().map(|n| n.name.clone()).collect();
    let rows = sqlx::query(
        r#"
        SELECT x.name, s.entity_type, s.entity_id, s.sport, s.norm
        FROM unnest($1::text[]) AS x(name)
        JOIN public.entity_name_surfaces s
          ON s.norm = public.nrm(x.name) AND s.sport = $2
        "#,
    )
    .bind(&raw)
    .bind(sport.to_uppercase())
    .fetch_all(conn)
    .await
    .context("resolve editor names against entity_name_surfaces")?;

    let hits: Vec<SurfaceHit> = rows
        .into_iter()
        .map(|r| SurfaceHit {
            name: r.get("name"),
            entity_type: r.get("entity_type"),
            entity_id: r.get("entity_id"),
            sport: r.get("sport"),
            norm: r.get("norm"),
        })
        .collect();
    Ok(group_hits(names, &hits))
}
