//! On-demand, source-bearing memory studies shared by plugin selectors.
//! Postgres exports a consistent bounded slice; the existing Go DuckDB engine
//! computes findings. This module never publishes facts or calls an LLM.
use crate::plugins::meta::EntityMeta;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub mod statistic;

pub const VERSION: &str = "reporting-frequency-v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Observation {
    pub article_id: i64,
    pub canonical_id: i64,
    pub topic: String,
    pub publisher: String,
    pub reported_at: i64,
    pub headline: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub version: String,
    pub from: i64,
    pub before: i64,
    pub limit: usize,
    pub per_topic: usize,
    pub observations: Vec<Observation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub from: i64,
    pub before: i64,
    pub topic: String,
    pub article_count: usize,
    pub publisher_count: usize,
    pub publishers: Vec<PublisherCount>,
    pub source_ids: Vec<i64>,
    pub reports: Vec<Observation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublisherCount {
    pub publisher: String,
    pub articles: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub version: String,
    pub subject: EntityMeta,
    pub from: i64,
    pub before: i64,
    pub input_hash: String,
    pub captured_at: i64,
    pub mvcc_snapshot: String,
    pub observed_articles: usize,
    #[serde(default)]
    pub pair: Option<EntityMeta>,
    #[serde(default)]
    pub predicates: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Study {
    pub receipt: Receipt,
    pub findings: Vec<Finding>,
}

/// Scope is chosen at request time. Bounds are half-open and are reporting
/// dates, not the time at which the described real-world event took place.
pub async fn reporting(
    pool: &PgPool,
    subject: &EntityMeta,
    from: i64,
    before: i64,
    exclude: &[i64],
    limit: usize,
) -> Result<Study> {
    reporting_scope(pool, subject, from, before, exclude, limit, None, &[]).await
}

/// A pair study counts sourced reports selected by the requested relationship
/// predicates, not repeated graph rows. Both named parties must occur in the
/// retained headline; this intentionally trades recall for auditable identity.
pub async fn reporting_scope(
    pool: &PgPool,
    subject: &EntityMeta,
    from: i64,
    before: i64,
    exclude: &[i64],
    limit: usize,
    pair: Option<&EntityMeta>,
    predicates: &[String],
) -> Result<Study> {
    ensure!(
        matches!(subject.entity_type.as_str(), "team" | "player") && subject.entity_id > 0,
        "reporting study requires a canonical team or player; person Graph IDs need reconciliation"
    );
    ensure!(
        pair.is_none_or(|p| matches!(p.entity_type.as_str(), "team" | "player") && p.entity_id > 0),
        "unsupported reporting pair identity"
    );
    ensure!(
        predicates
            .iter()
            .all(|p| crate::plugins::graph::cognition::PREDICATES.contains(&p.as_str())),
        "unknown Graph relationship predicate"
    );
    ensure!(
        pair.is_none_or(|p| p.sport == subject.sport),
        "pair sport mismatch"
    );
    ensure!(
        from < before && (1..=20).contains(&limit),
        "invalid memory scope"
    );
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='15s'")
        .execute(&mut *tx)
        .await?;
    let (mvcc_snapshot, captured_at): (String, i64) = sqlx::query_as(
        "SELECT pg_current_snapshot()::text,floor(extract(epoch FROM transaction_timestamp()))::bigint",
    ).fetch_one(&mut *tx).await?;
    ensure!(
        before <= captured_at,
        "reporting window extends into future"
    );
    let rows: Vec<String> = sqlx::query_scalar(include_str!("reporting.sql"))
        .bind(&subject.sport)
        .bind(&subject.entity_type)
        .bind(subject.entity_id)
        .bind(&subject.name)
        .bind(from)
        .bind(before)
        .bind(exclude)
        .bind(pair.map(|p| p.entity_id))
        .bind(pair.map(|p| p.entity_type.as_str()))
        .bind(pair.map(|p| p.name.as_str()).unwrap_or(""))
        .bind(predicates)
        .fetch_all(&mut *tx)
        .await?;
    ensure!(
        rows.len() <= 20000,
        "memory population exceeds bound; narrow the timeframe"
    );
    let observations = rows
        .iter()
        .map(|r| serde_json::from_str(r))
        .collect::<std::result::Result<Vec<Observation>, _>>()?;
    tx.commit().await?;
    let req = Request {
        version: VERSION.into(),
        from,
        before,
        limit,
        per_topic: 2,
        observations,
    };
    let input_hash =
        crate::util::hash_components(&serde_json::to_string(&(subject, pair, predicates, &req))?);
    let receipt = Receipt {
        version: VERSION.into(),
        subject: subject.clone(),
        from,
        before,
        input_hash,
        captured_at,
        mvcc_snapshot,
        observed_articles: req.observations.len(),
        pair: pair.cloned(),
        predicates: predicates.to_vec(),
    };
    let findings = if req.observations.is_empty() {
        Vec::new()
    } else {
        compute(&req).await?
    };
    Ok(Study { receipt, findings })
}

fn executable() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("SCORACLE_MEMORY_STUDY_BIN") {
        return path.into();
    }
    // Standard release layout: rust/bin/scoracle-cognition and go/bin/helper.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
        {
            let path = root.join("go/bin/scoracle-memory-study");
            if path.is_file() {
                return path;
            }
        }
        if let Some(dir) = exe.parent() {
            let path = dir.join("scoracle-memory-study");
            if path.is_file() {
                return path;
            }
        }
    }
    "scoracle-memory-study".into()
}

pub async fn compute(req: &Request) -> Result<Vec<Finding>> {
    let findings: Vec<Finding> = run(req).await?;
    ensure!(
        findings.len() <= req.limit,
        "unexpected memory result count"
    );
    for f in &findings {
        ensure!(
            f.reports.len() <= req.per_topic
                && f.article_count == f.source_ids.len()
                && f.from == req.from
                && f.before == req.before,
            "invalid memory result shape"
        );
        for r in &f.reports {
            ensure!(
                req.observations.contains(r)
                    && r.reported_at >= req.from
                    && r.reported_at < req.before
                    && r.topic == f.topic
                    && f.source_ids.contains(&r.article_id),
                "memory result changed source observation"
            );
        }
    }
    Ok(findings)
}

pub(super) async fn run<T: Serialize, R: serde::de::DeserializeOwned>(req: &T) -> Result<R> {
    // Bound embedded-engine concurrency independently of the queue's fan-out.
    static SLOTS: std::sync::LazyLock<tokio::sync::Semaphore> =
        std::sync::LazyLock::new(|| tokio::sync::Semaphore::new(2));
    let _slot = SLOTS.acquire().await?;
    let raw = serde_json::to_vec(req)?;
    ensure!(
        raw.len() <= 8 * 1024 * 1024,
        "memory snapshot exceeds byte budget"
    );
    let mut child = tokio::process::Command::new(executable())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("start shared DuckDB memory study; build go/cmd/memory-study")?;
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap().take(8 * 1024 * 1024 + 1);
    let mut stderr = child.stderr.take().unwrap().take(8193);
    let work = async {
        let mut output = Vec::new();
        let mut errors = Vec::new();
        tokio::try_join!(
            async {
                stdin.write_all(&raw).await?;
                drop(stdin);
                Ok::<_, std::io::Error>(())
            },
            stdout.read_to_end(&mut output),
            stderr.read_to_end(&mut errors),
        )?;
        ensure!(
            output.len() <= 8 * 1024 * 1024 && errors.len() <= 8192,
            "memory output exceeds budget"
        );
        let status = child.wait().await?;
        ensure!(
            status.success(),
            "DuckDB memory study failed: {}",
            String::from_utf8_lossy(&errors)
        );
        let findings: R = serde_json::from_slice(&output)?;
        Ok(findings)
    };
    tokio::time::timeout(std::time::Duration::from_secs(25), work)
        .await
        .context("memory study timed out")?
}

#[cfg(test)]
mod tests;
