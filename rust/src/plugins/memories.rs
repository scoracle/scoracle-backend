//! Shared memory tool. Plugins choose a study, its source tables and scope.
//! Reporting and match-statistic adapters share one DuckDB runner and provenance.
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
    /// Non-empty when the caller narrowed the study to a set of articles it had
    /// already resolved. Recorded so a narrowed study is not mistaken for a
    /// subject-wide one.
    #[serde(default)]
    pub included_articles: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Study {
    pub receipt: Receipt,
    pub findings: Vec<Finding>,
}

/// A dated source observation prepared for articulation, without generated prose.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct History {
    pub publisher: String,
    pub published_at: String,
    pub reported_headline: String,
}

/// Plugin-selected bounds for a compact reporting view. The tool handles
/// source identity, dates and whole-observation budgeting; the plugin may
/// supply its source-admission policy without coupling this tool to a character.
#[derive(Clone, Copy, Debug)]
pub struct ReportingHistory {
    pub lookback_seconds: i64,
    pub max_reports: usize,
    pub budget_bytes: usize,
}

impl ReportingHistory {
    pub async fn load(
        &self,
        pool: &PgPool,
        subject: &EntityMeta,
        before: Option<i64>,
        exclude: &[i64],
    ) -> Result<Option<Study>> {
        let Some(before) = before else {
            return Ok(None);
        };
        Ok(Some(
            reporting(
                pool,
                subject,
                before - self.lookback_seconds,
                before,
                exclude,
                self.max_reports,
            )
            .await?,
        ))
    }

    pub fn select(
        &self,
        study: &Study,
        subject: &EntityMeta,
        before: Option<i64>,
        exclude: &[i64],
        accepts: impl Fn(&Observation) -> bool,
    ) -> Result<Vec<History>> {
        ensure!(&study.receipt.subject == subject, "memory subject mismatch");
        let Some(before) = before else {
            return Ok(Vec::new());
        };
        ensure!(
            study.receipt.before <= before,
            "memory is newer than fresh source"
        );
        let mut selected = Vec::new();
        for report in study.findings.iter().flat_map(|finding| &finding.reports) {
            if selected.len() == self.max_reports {
                break;
            }
            if report.reported_at >= before
                || report.reported_at < before - self.lookback_seconds
                || exclude.contains(&report.article_id)
                || exclude.contains(&report.canonical_id)
                || !accepts(report)
            {
                continue;
            }
            let item = History {
                publisher: report.publisher.clone(),
                published_at: crate::util::utc_timestamp(report.reported_at),
                reported_headline: report.headline.clone(),
            };
            selected.push(item);
            if serde_json::to_vec(&selected)?.len() > self.budget_bytes {
                selected.pop();
            }
        }
        Ok(selected)
    }
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
    reporting_scope(pool, subject, from, before, exclude, limit, &[], None).await
}

/// A plugin-supplied grouping over one loaded observation.
///
/// The shared study does not know what makes two articles the same story. A
/// storyline, a relationship pair and a subject-wide report are three different
/// groupings and each belongs to the plugin that means it. The study emits one
/// canonical article per observation; this function decides what shares a group.
pub type Topic<'a> = &'a (dyn Fn(&Observation) -> Option<String> + Send + Sync);

/// Regroup loaded observations before they are hashed or studied, so the receipt
/// and the findings both cover the grouping that was actually used. Returning
/// `None` leaves the study's one-article-per-observation default in place.
pub fn apply_topics(req: &mut Request, topic: Topic<'_>) {
    for observation in &mut req.observations {
        if let Some(group) = topic(observation) {
            observation.topic = group;
        }
    }
}

/// A study narrowed to articles the caller already resolved.
///
/// `include` replaces the study's own relationship and relationship-predicate
/// lookups: the caller selects candidate articles with whatever vocabulary its
/// domain uses, and the study applies only reporting dates, canonical
/// deduplication, source-name containment and frequency ranking. An empty
/// `include` is the subject-wide study. Requiring both parties' names in a
/// headline, or requiring an allowed predicate, is caller policy and stays with
/// the caller.
pub async fn reporting_scope(
    pool: &PgPool,
    subject: &EntityMeta,
    from: i64,
    before: i64,
    exclude: &[i64],
    limit: usize,
    include: &[i64],
    topic: Option<Topic<'_>>,
) -> Result<Study> {
    ensure!(
        matches!(subject.entity_type.as_str(), "team" | "player") && subject.entity_id > 0,
        "reporting study requires a canonical team or player; person Graph IDs need reconciliation"
    );
    ensure!(
        include.len() <= 20_000,
        "included article set exceeds bound; resolve a narrower set"
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
    let rows: Vec<String> = sqlx::query_scalar(include_str!("memories/reporting.sql"))
        .bind(&subject.sport)
        .bind(&subject.entity_type)
        .bind(subject.entity_id)
        .bind(&subject.name)
        .bind(from)
        .bind(before)
        .bind(exclude)
        .bind(include)
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
    let mut req = Request {
        version: VERSION.into(),
        from,
        before,
        limit,
        per_topic: 2,
        observations,
    };
    // Grouping is applied before hashing so the receipt covers the groups the
    // findings were actually built from, not the study's default grouping.
    if let Some(topic) = topic {
        apply_topics(&mut req, topic);
    }
    let input_hash =
        crate::util::hash_components(&serde_json::to_string(&(subject, include, &req))?);
    let receipt = Receipt {
        version: VERSION.into(),
        subject: subject.clone(),
        from,
        before,
        input_hash,
        captured_at,
        mvcc_snapshot,
        observed_articles: req.observations.len(),
        included_articles: include.len(),
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
