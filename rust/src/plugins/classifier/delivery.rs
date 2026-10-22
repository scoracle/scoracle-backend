//! Native character obligations and source-bound reads. No score thresholds.
use super::Source;
use crate::tools::source::SourceContext;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};

const INFLUENCER: &str = crate::plugins::influencer::manifest::MANIFEST.id.as_str();
const SCOUT: &str = crate::plugins::scout::manifest::MANIFEST.id.as_str();
const INSIDER: &str = crate::plugins::insider::manifest::MANIFEST.id.as_str();
const JOURNALIST: &str = crate::plugins::journalist::manifest::MANIFEST.id.as_str();

pub async fn load_for_character(
    pool: &PgPool,
    plugin: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<SourceContext>> {
    load_for_character_on(
        &mut *pool.acquire().await?,
        plugin,
        entity_type,
        entity_id,
        sport,
    )
    .await
}

/// The routing-gate character key for a plugin id.
pub(crate) fn character_key(plugin: &str) -> Result<&'static str> {
    match plugin {
        JOURNALIST => Ok("journalist"),
        INFLUENCER => Ok("influencer"),
        INSIDER => Ok("insider"),
        SCOUT => Ok("scout"),
        _ => anyhow::bail!("unsupported Classifier character"),
    }
}

/// Acknowledge the same obligations that supplied a character's source packet.
/// Call inside its claim-fenced product publication transaction.
pub(crate) async fn settle(
    connection: &mut PgConnection,
    plugin: &str,
    spectrum_ids: &[i64],
    status: &str,
    reason: Option<&str>,
    product_ref: Value,
) -> Result<()> {
    let changed = sqlx::query("UPDATE classifier_spectrum_deliveries SET status=$3,reason=$4,
        product_ref=$5,updated_at=now() WHERE spectrum_id=ANY($1) AND character=$2 AND status='pending'")
        .bind(spectrum_ids).bind(character_key(plugin)?).bind(status).bind(reason).bind(product_ref)
        .execute(connection).await?;
    ensure!(
        changed.rows_affected() == spectrum_ids.len() as u64,
        "Classifier spectrum obligations changed during character work"
    );
    Ok(())
}

/// The retained source behind a spectrum row, for identity validation and publication locks.
pub(crate) async fn load_spectrum_source_on(
    connection: &mut PgConnection,
    spectrum_id: i64,
) -> Result<Source> {
    let source: Value = sqlx::query_scalar("SELECT s.source FROM classifier_spectra x JOIN classifier_sources s ON s.id=x.source_id WHERE x.id=$1")
        .bind(spectrum_id)
        .fetch_one(connection)
        .await
        .context("load spectrum source")?;
    Ok(serde_json::from_value(source)?)
}

pub(crate) async fn load_spectrum_sources_on(
    connection: &mut PgConnection,
    ids: &[i64],
) -> Result<Vec<Source>> {
    let rows: Vec<Value> = sqlx::query_scalar("SELECT s.source FROM classifier_spectra x JOIN classifier_sources s ON s.id=x.source_id WHERE x.id=ANY($1) ORDER BY x.id")
        .bind(ids).fetch_all(connection).await?;
    ensure!(
        rows.len() == ids.len(),
        "missing or duplicate spectrum source"
    );
    rows.into_iter()
        .map(|value| serde_json::from_value(value).map_err(Into::into))
        .collect()
}

/// Pool convenience for the retained source behind a spectrum row.
pub async fn load_spectrum_source(pool: &PgPool, spectrum_id: i64) -> Result<Source> {
    load_spectrum_source_on(&mut *pool.acquire().await?, spectrum_id).await
}

pub(crate) async fn load_for_character_on(
    connection: &mut PgConnection,
    plugin: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<SourceContext>> {
    let character = character_key(plugin)?;
    let rows=sqlx::query("SELECT x.id,x.spectrum,x.gate,x.clean_text,
        floor(EXTRACT(EPOCH FROM (s.source->>'published_at')::timestamptz))::bigint AS published_at_epoch,
        s.source,
        COALESCE(s.discovery_version=public.classifier_discovery_version(s.article_id,s.sport),false)
            AND EXISTS(SELECT 1 FROM classifier_source_heads h WHERE h.article_id=s.article_id AND h.sport=s.sport AND h.source_id=s.id)
            AS current_source
        FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id
        JOIN classifier_sources s ON s.id=x.source_id
        WHERE d.character=$1 AND x.sport=$4
        AND x.target->>'entity_type'=$2 AND x.target->>'entity_id'=$3::text
        AND d.status='pending' AND x.criteria_version IN ($5,'classification-criteria-v9-headlines')
        AND public.coverage_spectrum_selected(d.character,$2,$3::bigint,$4,x.id)
        ORDER BY published_at_epoch DESC NULLS LAST,x.id DESC LIMIT 20001")
        .bind(character).bind(entity_type).bind(entity_id.to_string()).bind(sport).bind(super::decision::CRITERIA_VERSION)
        .fetch_all(&mut *connection).await?;
    ensure!(
        rows.len() <= 20000,
        "Classifier character population exceeds bound"
    );
    // Obsolete receipts stay pending for reconciliation; current evidence can still be read.
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|row| row.get::<bool, _>("current_source"))
        .collect();
    let native = rows
        .iter()
        .map(|row| serde_json::from_value::<Source>(row.get("source")))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let identities = super::identity::candidates_for(connection, sport, &native).await?;
    let mut sources = Vec::with_capacity(rows.len());
    let mut articles = std::collections::HashSet::new();
    for ((row, source), identity) in rows.into_iter().zip(native).zip(identities) {
        let id: i64 = row.get("id");
        ensure!(
            identity == source.provenance["identity_candidates"],
            "Classifier canonical identity changed"
        );
        if !articles.insert(source.article_id) {
            continue;
        }
        ensure!(
            source.provenance["evidence_scope"] == "headline",
            "character delivery must retain headline evidence"
        );
        sources.push(SourceContext {
            classification_id: id,
            article_id: source.article_id,
            headline: source.provenance["title"]
                .as_str()
                .context("source headline")?
                .into(),
            context: row.get("clean_text"),
            source: source.source,
            published_at_epoch: row.get("published_at_epoch"),
            classifier_world: Some(json!({
                "target_identity_resolved": identity.as_array().unwrap().iter().any(|candidate|
                    candidate["entity_type"] == entity_type && candidate["entity_id"] == entity_id),
                "evidence_scope": "headline",
                "url": source.provenance["url"],
                "spectrum": row.get::<Value, _>("spectrum"),
                "gate": row.get::<Value, _>("gate"),
            })),
        });
    }
    Ok(sources)
}

/// Locks follow the scorer's source-before-delivery order and never span inference.
pub async fn validate_for_publication(
    tx: &mut Transaction<'_, Postgres>,
    plugin: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    sources: &[SourceContext],
) -> Result<()> {
    lock_sources(tx, plugin, sport, sources).await?;
    let ids: Vec<_> = sources.iter().map(|s| s.classification_id).collect();
    let native = load_spectrum_sources_on(tx, &ids).await?;
    super::identity::validate_all(tx, sport, &native).await?;
    let current = load_for_character_on(tx, plugin, entity_type, entity_id, sport).await?;
    ensure!(
        sources.iter().all(|s| current.contains(s)),
        "Classifier evidence or delivery changed during articulation"
    );
    Ok(())
}

pub(crate) async fn lock_sources(
    tx: &mut Transaction<'_, Postgres>,
    plugin: &str,
    sport: &str,
    sources: &[SourceContext],
) -> Result<()> {
    if sources.is_empty() {
        return Ok(());
    }
    let character = character_key(plugin)?;
    let articles: Vec<_> = sources.iter().map(|s| s.article_id).collect();
    let ids: Vec<_> = sources.iter().map(|s| s.classification_id).collect();
    sqlx::query("SELECT id FROM news_articles WHERE id=ANY($1) ORDER BY id FOR SHARE NOWAIT")
        .bind(&articles)
        .fetch_all(&mut **tx)
        .await?;
    sqlx::query("SELECT p.article_id FROM harvester_query_provenance p JOIN teams t ON t.id=p.entity_id AND t.sport=p.sport AND p.entity_type='team'
        WHERE p.article_id=ANY($1) AND p.sport=$2 ORDER BY p.article_id,p.entity_id FOR SHARE OF p,t NOWAIT")
        .bind(&articles).bind(sport).fetch_all(&mut **tx).await?;
    sqlx::query(
        "SELECT x.id FROM classifier_spectra x JOIN classifier_sources s ON s.id=x.source_id
        WHERE x.id=ANY($1) ORDER BY x.id FOR SHARE OF x,s NOWAIT",
    )
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await?;
    sqlx::query("SELECT spectrum_id FROM classifier_spectrum_deliveries WHERE spectrum_id=ANY($1) AND character=$2
        ORDER BY spectrum_id FOR UPDATE NOWAIT")
        .bind(&ids).bind(character).fetch_all(&mut **tx).await?;
    Ok(())
}

pub async fn undelivered_count(
    tx: &mut Transaction<'_, Postgres>,
    plugin: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id
        WHERE d.character=$1 AND d.status='pending' AND x.sport=$4
        AND x.target->>'entity_type'=$2 AND x.target->>'entity_id'=$3::text
        AND public.coverage_spectrum_selected(d.character,$2,$3::bigint,$4,x.id)")
        .bind(character_key(plugin)?).bind(entity_type).bind(entity_id.to_string()).bind(sport).fetch_one(&mut **tx).await?)
}
