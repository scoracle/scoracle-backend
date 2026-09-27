//! Headline-only Laya replay over a stratified nightly corpus sample.
//! Read-only: no publisher fetch, queue work, or production gate receipts.
//! Output contains IDs, hashes, and probabilities, never headlines or bodies.
//! cargo run --release --example harvest_headline_calibration -- RUN_ID PER_STRATUM OUTPUT.json LAYA_ENDPOINT
use anyhow::{ensure, Context, Result};
use scoracle_cognition::plugins::harvester::{context, Article};
use scoracle_cognition::runtime::providers::system_one::SystemOneClient;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::Instant;

const SAMPLE_SQL: &str = r#"
WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE id=$1 AND job='pipeline' AND finished_at IS NOT NULL
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport,
           min(p.feed_rank) AS feed_rank
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
     GROUP BY p.article_id,p.entity_type,p.entity_id,p.sport
), old_classification AS (
    SELECT DISTINCT ON (c.article_id,c.entity_type,c.entity_id,c.sport)
           c.article_id,c.entity_type,c.entity_id,c.sport,c.entity_choice
      FROM public.harvester_classifications c JOIN cohort q
        ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
     WHERE c.contract_version='harvest-context-v1'
     ORDER BY c.article_id,c.entity_type,c.entity_id,c.sport,c.created_at DESC,c.id DESC
), candidates AS (
    SELECT q.*,n.title,t.name AS entity_name,
           COALESCE(c.entity_choice,'unclassified') AS old_stratum
      FROM cohort q JOIN public.news_articles n ON n.id=q.article_id
      JOIN public.teams t ON t.id=q.entity_id AND t.sport=q.sport
      LEFT JOIN old_classification c
        ON c.article_id=q.article_id AND c.entity_type=q.entity_type
       AND c.entity_id=q.entity_id AND c.sport=q.sport
     WHERE q.entity_type='team' AND n.duplicate_of IS NULL
), ranked AS (
    SELECT *,row_number() OVER (
        PARTITION BY old_stratum
        ORDER BY md5(article_id::text || ':' || entity_id::text || ':' || sport)
    ) AS sample_rank
      FROM candidates
)
SELECT article_id,entity_type,entity_id,sport,feed_rank,title,entity_name,old_stratum
  FROM ranked WHERE sample_rank<=$2
 ORDER BY old_stratum,sample_rank
"#;

fn probability_bin(p: f64) -> &'static str {
    if p < 0.05 {
        "00-05"
    } else if p < 0.10 {
        "05-10"
    } else if p < 0.20 {
        "10-20"
    } else if p < 0.25 {
        "20-25"
    } else if p < 0.35 {
        "25-35"
    } else if p < 0.50 {
        "35-50"
    } else if p < 0.70 {
        "50-70"
    } else if p < 0.90 {
        "70-90"
    } else {
        "90-100"
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    ensure!(
        args.len() == 5,
        "usage: harvest_headline_calibration RUN_ID PER_STRATUM OUTPUT.json LAYA_ENDPOINT"
    );
    let run_id: i64 = args[1].parse().context("parse run id")?;
    let per_stratum: i64 = args[2].parse().context("parse per-stratum sample size")?;
    ensure!(
        per_stratum > 0 && per_stratum <= 500,
        "sample size outside 1..500"
    );
    let db_url = std::env::var("DATABASE_PRIVATE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("database URL is required")?;
    let pool = PgPool::connect(&db_url).await?;
    let model = SystemOneClient::new(args[4].clone())?;
    let candidates = sqlx::query(SAMPLE_SQL)
        .bind(run_id)
        .bind(per_stratum)
        .fetch_all(&pool)
        .await?;
    ensure!(!candidates.is_empty(), "no nightly candidates found");
    let mut output_rows = Vec::<Value>::with_capacity(candidates.len());
    let mut hist = BTreeMap::<String, usize>::new();
    let mut failures = 0usize;
    let started = Instant::now();
    for row in candidates {
        let article_id: i64 = row.get("article_id");
        let title: String = row.get("title");
        let stratum: String = row.get("old_stratum");
        let article = Article {
            article_id,
            title: title.clone(),
            source: String::new(),
            url: String::new(),
            published_at: None,
            feed_rank: row.get("feed_rank"),
            description: String::new(),
            body: String::new(),
            hypothesis: scoracle_cognition::plugins::harvester::cognition::Hypothesis {
                name: row.get("entity_name"),
                entity_type: row.get("entity_type"),
                entity_id: row.get("entity_id"),
                sport: row.get("sport"),
            },
            baseline: Value::Null,
        };
        let start = Instant::now();
        match context::classify_headline(&model, &article).await {
            Ok(gate) => {
                let p = gate.relevance_probability();
                *hist
                    .entry(format!("{}:{}", stratum, probability_bin(p)))
                    .or_default() += 1;
                output_rows.push(json!({
                    "article_id": article_id,
                    "entity_type": article.hypothesis.entity_type,
                    "entity_id": article.hypothesis.entity_id,
                    "sport": article.hypothesis.sport,
                    "old_body_stratum_not_gold": stratum,
                    "headline_sha256": hex::encode(Sha256::digest(title.as_bytes())),
                    "question_version": scoracle_cognition::plugins::harvester::cognition::RELEVANCE_QUESTIONS,
                    "input_hash": gate.input_hash,
                    "model_revision": gate.model_revision,
                    "laya_choice": gate.response.answers["relevance"].choice,
                    "p_relevant": p,
                    "inference_ms": start.elapsed().as_millis(),
                }));
            }
            Err(error) => {
                failures += 1;
                output_rows.push(json!({
                    "article_id": article_id,
                    "entity_id": article.hypothesis.entity_id,
                    "sport": article.hypothesis.sport,
                    "old_body_stratum_not_gold": stratum,
                    "headline_sha256": hex::encode(Sha256::digest(title.as_bytes())),
                    "error_class": if error.chain().any(|cause| cause.is::<reqwest::Error>()) {
                        "transport_or_http"
                    } else {
                        "classification_contract"
                    },
                }));
            }
        }
    }
    let summary = json!({
        "run_id": run_id,
        "per_stratum": per_stratum,
        "question_version": scoracle_cognition::plugins::harvester::cognition::RELEVANCE_QUESTIONS,
        "read_only": true,
        "labels_are_gold": false,
        "elapsed_ms": started.elapsed().as_millis(),
        "failures": failures,
        "histogram": hist,
        "rows": output_rows,
    });
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[3])?;
    serde_json::to_writer_pretty(&mut output, &summary)?;
    writeln!(output)?;
    println!(
        "{}",
        json!({"sampled": output_rows.len(),"failures":failures,"histogram":hist})
    );
    Ok(())
}
