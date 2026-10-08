//! Native character obligations and source-bound reads. No score thresholds.
use super::{adapter::load_measurement_on, Record};
use crate::tools::source::SourceContext;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};

pub const POLICY_VERSION: &str = "journalist-classifier-v1";
pub const INFLUENCER_POLICY_VERSION: &str = "influencer-classifier-v1";
const INFLUENCER: &str = crate::plugins::influencer::manifest::MANIFEST.id.as_str();
pub const INSIDER_POLICY_VERSION: &str = "insider-classifier-v1";
pub const SCOUT_POLICY_VERSION: &str = "scout-classifier-v1";
const SCOUT: &str = crate::plugins::scout::manifest::MANIFEST.id.as_str();
const INSIDER: &str = crate::plugins::insider::manifest::MANIFEST.id.as_str();
const JOURNALIST: &str = crate::plugins::journalist::manifest::MANIFEST.id.as_str();

/// Retain an unresolved obligation until a separately evaluated policy releases it.
/// Replays cannot reopen completed work or revive an older measurement.
pub(crate) async fn record(tx: &mut Transaction<'_, Postgres>, id: i64) -> Result<()> {
    let (source, record, _) = load_measurement_on(tx, id).await?;
    let reason = if record
        .claims
        .iter()
        .any(|c| c.target_relation == "direct_subject")
    {
        "calibration_unassessed"
    } else {
        "target_selection_unresolved"
    };
    for (plugin, policy) in [
        (JOURNALIST, POLICY_VERSION),
        (INFLUENCER, INFLUENCER_POLICY_VERSION),
        (INSIDER, INSIDER_POLICY_VERSION),
        (SCOUT, SCOUT_POLICY_VERSION),
    ] {
        if record.target["entity_type"] == "person" && plugin != INSIDER {
            continue;
        }
        sqlx::query("UPDATE classifier_deliveries d SET status='superseded',reason='newer_measurement',updated_at=now()
        FROM classifier_measurements old,classifier_measurements current
        WHERE current.id=$1 AND old.id=d.measurement_id AND old.id<current.id
        AND d.plugin_id=$2 AND d.status IN ('held','pending')
        AND old.article_id=current.article_id AND old.sport=current.sport
        AND (old.receipt->'target'->>'entity_type',old.receipt->'target'->>'entity_id')=(current.receipt->'target'->>'entity_type',current.receipt->'target'->>'entity_id')")
        .bind(id).bind(plugin).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO classifier_deliveries(measurement_id,plugin_id,policy_version,reason)
        SELECT $1,$2,$3,$4 WHERE NOT EXISTS (
            SELECT 1 FROM classifier_deliveries d JOIN classifier_measurements newer ON newer.id=d.measurement_id
            WHERE d.plugin_id=$2 AND newer.id>$1 AND newer.article_id=$5 AND newer.sport=$6
            AND (newer.receipt->'target'->>'entity_type',newer.receipt->'target'->>'entity_id')=($7::jsonb->>'entity_type',$7::jsonb->>'entity_id')) ON CONFLICT DO NOTHING")
        .bind(id).bind(plugin).bind(policy).bind(reason).bind(source.article_id)
        .bind(record.target["sport"].as_str().context("delivery sport")?)
        .bind(record.target.to_string()).execute(&mut **tx).await?;
    }
    Ok(())
}

/// Presentation omits byte arithmetic, scores and durable IDs; qualifiers stay literal.
fn world(record: &Record) -> Value {
    fn text(spans: &Option<Vec<super::Span>>) -> Option<Vec<&str>> {
        spans
            .as_ref()
            .map(|spans| spans.iter().map(|s| s.quote.as_str()).collect())
    }
    json!({"qualified_claims":record.claims.iter().map(|claim| json!({
        "publisher_text":claim.evidence.quote,"target_relation":claim.target_relation,
        "target_evidence":text(&claim.target_evidence),"kind":claim.kind,"time_scope":claim.time_scope,
        "qualifiers":claim.qualifiers.iter().map(|(key,spans)| (key,text(spans)))
            .collect::<std::collections::BTreeMap<_,_>>()
    })).collect::<Vec<_>>(),"signals_unassessed":true})
}

pub async fn load_for_character(
    pool: &PgPool,
    plugin: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<SourceContext>> {
    load_on(
        &mut *pool.acquire().await?,
        plugin,
        entity_type,
        entity_id,
        sport,
        Population::Pending,
    )
    .await
}

pub async fn load_used(
    pool: &PgPool,
    plugin: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    from: i64,
    before: i64,
) -> Result<Vec<SourceContext>> {
    load_on(
        &mut *pool.acquire().await?,
        plugin,
        entity_type,
        entity_id,
        sport,
        Population::Used(from, before),
    )
    .await
}

/// Accepted reporting for one period, including previously consumed historical evidence.
pub async fn load_accepted(
    connection: &mut PgConnection,
    subject: &crate::tools::meta::EntityMeta,
    from: i64,
    before: i64,
    cutoff: i64,
) -> Result<Vec<SourceContext>> {
    load_on(
        connection,
        INFLUENCER,
        &subject.entity_type,
        subject.entity_id,
        &subject.sport,
        Population::Accepted(from, before, cutoff),
    )
    .await
}

enum Population {
    Pending,
    Used(i64, i64),
    Accepted(i64, i64, i64),
}

async fn load_on(
    connection: &mut PgConnection,
    plugin: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    population: Population,
) -> Result<Vec<SourceContext>> {
    let (mode, from, before, cutoff) = match population {
        Population::Pending => ("pending", None, None, None),
        Population::Used(from, before) => ("used", Some(from), Some(before), None),
        Population::Accepted(from, before, cutoff) => {
            ("accepted", Some(from), Some(before), Some(cutoff))
        }
    };
    let policy = match plugin {
        JOURNALIST => POLICY_VERSION,
        INFLUENCER => INFLUENCER_POLICY_VERSION,
        INSIDER => INSIDER_POLICY_VERSION,
        SCOUT => SCOUT_POLICY_VERSION,
        _ => anyhow::bail!("unsupported Classifier character"),
    };
    let rows=sqlx::query("SELECT m.id,d.policy_version,d.status,d.selection,
        EXTRACT(EPOCH FROM (s.source->>'published_at')::timestamptz)::bigint AS published_at_epoch,
        COALESCE(s.discovery_version=public.classifier_discovery_version(s.article_id,s.sport),false) AS current_source
        FROM classifier_deliveries d JOIN classifier_measurements m ON m.id=d.measurement_id
        JOIN classifier_sources s ON s.id=m.source_id
        WHERE d.plugin_id=$1 AND m.sport=$4 AND m.status='source_bound_provisional'
        AND m.receipt->'target'->>'entity_type'=$2 AND m.receipt->'target'->>'entity_id'=$3::text
        AND d.production_eligible AND (($5='pending' AND d.status='pending')
            OR ($5='used' AND d.status='used' AND d.updated_at>=to_timestamp($6::double precision)
                AND d.updated_at<to_timestamp($7::double precision))
            OR ($5='accepted' AND d.status IN ('pending','used','abstained','redundant')
                AND floor(extract(epoch FROM m.created_at))<=$8
                AND (s.source->>'published_at')::timestamptz>=to_timestamp($6::double precision)
                AND (s.source->>'published_at')::timestamptz<to_timestamp($7::double precision)))
        ORDER BY published_at_epoch DESC NULLS LAST,m.id DESC LIMIT 20001")
        .bind(plugin).bind(entity_type).bind(entity_id.to_string()).bind(sport)
        .bind(mode).bind(from).bind(before).bind(cutoff).fetch_all(&mut *connection).await?;
    ensure!(
        rows.len() <= 20000,
        "Classifier character population exceeds bound"
    );
    let mut sources = Vec::with_capacity(rows.len());
    let mut articles = std::collections::HashSet::new();
    for row in rows {
        ensure!(
            row.get::<String, _>("policy_version") == policy,
            "unsupported character delivery policy"
        );
        ensure!(
            row.get::<String, _>("status") != "pending" || row.get::<bool, _>("current_source"),
            "Classifier delivery source changed; acquisition must reconcile it"
        );
        let id = row.get("id");
        let (source, record, _) = load_measurement_on(connection, id).await?;
        if row.get::<String, _>("status") == "pending" {
            ensure!(
                super::identity::candidates(connection, sport, &source).await?
                    == source.provenance["identity_candidates"],
                "Classifier canonical identity changed"
            );
        }
        if !articles.insert(source.article_id) {
            continue;
        }
        let mut presented = world(&record);
        if plugin == SCOUT {
            presented["selection"] = row
                .get::<Option<Value>, _>("selection")
                .unwrap_or(Value::Null);
        }
        sources.push(SourceContext {
            classification_id: id,
            article_id: source.article_id,
            headline: source.provenance["title"]
                .as_str()
                .context("source headline")?
                .into(),
            context: source.body,
            source: source.source,
            published_at_epoch: row.get("published_at_epoch"),
            classifier_world: Some(presented),
        });
    }
    Ok(sources)
}

/// Locks follow acquisition's source-before-delivery order and never span inference.
pub async fn validate_for_publication(
    tx: &mut Transaction<'_, Postgres>,
    plugin: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    sources: &[SourceContext],
) -> Result<()> {
    lock_sources(tx, plugin, sport, sources).await?;
    for source in sources {
        let (native, _, _) = load_measurement_on(tx, source.classification_id).await?;
        super::identity::validate(tx, sport, &native).await?;
    }
    let current = load_on(
        tx,
        plugin,
        entity_type,
        entity_id,
        sport,
        Population::Pending,
    )
    .await?;
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
        "SELECT m.id FROM classifier_measurements m JOIN classifier_sources s ON s.id=m.source_id
        WHERE m.id=ANY($1) ORDER BY m.id FOR SHARE OF m,s NOWAIT",
    )
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await?;
    sqlx::query("SELECT measurement_id FROM classifier_deliveries WHERE measurement_id=ANY($1) AND plugin_id=$2
        ORDER BY measurement_id FOR UPDATE NOWAIT")
        .bind(&ids).bind(plugin).fetch_all(&mut **tx).await?;
    Ok(())
}

pub async fn undelivered_count(
    tx: &mut Transaction<'_, Postgres>,
    plugin: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM classifier_deliveries d JOIN classifier_measurements m ON m.id=d.measurement_id
        WHERE d.plugin_id=$1 AND d.status='pending' AND d.production_eligible AND m.sport=$4
        AND m.receipt->'target'->>'entity_type'=$2 AND m.receipt->'target'->>'entity_id'=$3::text")
        .bind(plugin).bind(entity_type).bind(entity_id.to_string()).bind(sport).fetch_one(&mut **tx).await?)
}
