//! bucketlabel — one-shot overnight GPU labeling batch for the F2 bucket classifier
//! (planning_docs/DATA_FLOW_FRICTION_PRUNE_PLAN.md, F2 verify).
//!
//! Labels a stratified-random sample of vetted articles as `transfer` vs `other` using
//! the configured local model (title + description, temperature 0, JSON mode). The
//! labels tune the candle contrastive+keyword fallback threshold and keyword list, and
//! double as validation of the F2 model-emitted bucket-tag prompt itself.
//!
//! Runs nightly-idempotent: the output TSV is the resume state — already-labeled
//! article ids are skipped on re-run, so a crashed run resumes and a completed run
//! no-ops (a few seconds of DB query, zero GPU). Safe to leave on a cron line.
//!
//! SAFETY: like bin/parity and bin/eval, read-only on the live pipeline. NEVER claims
//! `pipeline_work`, NEVER writes a product table. Reads `news_articles` +
//! `news_article_entities`, POSTs to Ollama, writes ONLY the output TSV.
//!
//! Usage (env from .env.local: DATABASE_PRIVATE_URL + OLLAMA_*):
//!   bucketlabel -limit 1500 -out planning_docs/data/bucket_labels.tsv [-throttle-ms 250]

use anyhow::{Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::db;
use scoracle_cognition::ollama::{GenerateOptions, OllamaClient};
use sqlx::Row;
use std::collections::HashSet;
use std::io::Write;
use std::time::Duration;

const SYSTEM_PROMPT: &str = "You are a sports-news classifier. Classify the article as \
exactly one bucket:\n\
- transfer: player personnel/transaction news — transfer or trade rumors, speculation, \
or completed moves; signings and re-signings; free agency; contract extensions, \
negotiations, or qualifying offers; loans; releases/waivers; draft speculation about \
which team acquires a player.\n\
- other: everything else — match reports and recaps, performance analysis, injuries, \
betting/odds, schedules, coaching or front-office news, off-field or lifestyle stories, \
legal news.\n\
Respond with ONLY a JSON object: {\"bucket\": \"transfer\"} or {\"bucket\": \"other\"}.";

#[derive(serde::Deserialize)]
struct BucketReply {
    bucket: String,
}

fn parse_args() -> (i64, String, u64) {
    let mut limit: i64 = 1500;
    let mut out = String::from("planning_docs/data/bucket_labels.tsv");
    let mut throttle_ms: u64 = 250;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-limit" => {
                limit = args[i + 1].parse().expect("-limit takes an integer");
                i += 2;
            }
            "-out" => {
                out = args[i + 1].clone();
                i += 2;
            }
            "-throttle-ms" => {
                throttle_ms = args[i + 1].parse().expect("-throttle-ms takes an integer");
                i += 2;
            }
            other => panic!(
                "unknown arg {other}; usage: bucketlabel -limit N -out PATH [-throttle-ms MS]"
            ),
        }
    }
    (limit, out, throttle_ms)
}

/// already_labeled reads the resume state: article ids in column 1 of an existing
/// output TSV (header line skipped).
fn already_labeled(path: &str) -> HashSet<i64> {
    let mut done = HashSet::new();
    if let Ok(body) = std::fs::read_to_string(path) {
        for line in body.lines().skip(1) {
            if let Some(id) = line.split('\t').next().and_then(|s| s.parse().ok()) {
                done.insert(id);
            }
        }
    }
    done
}

#[tokio::main]
async fn main() -> Result<()> {
    let (limit, out_path, throttle_ms) = parse_args();
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, 2).await?;
    let client = OllamaClient::new(&cfg.ollama_base_url, &cfg.ollama_model, cfg.ollama_timeout)?;

    let done = already_labeled(&out_path);
    eprintln!(
        "bucketlabel: model={} limit={} out={} resume-skip={}",
        client.model(),
        limit,
        out_path,
        done.len()
    );

    // Stratified-random over the population scrub actually buckets: articles with at
    // least one vetted link. md5 ordering is deterministic across runs, so the resume
    // set and the sample line up night over night.
    let rows = sqlx::query(
        "SELECT a.id,
                (SELECT min(nae.sport) FROM news_article_entities nae
                  WHERE nae.article_id = a.id AND nae.vetted IS TRUE) AS sport,
                a.title, coalesce(a.description,'')
           FROM news_articles a
          WHERE EXISTS (SELECT 1 FROM news_article_entities nae
                         WHERE nae.article_id = a.id AND nae.vetted IS TRUE)
          ORDER BY md5(a.id::text)
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&pool)
    .await
    .context("load article sample")?;

    if let Some(dir) = std::path::Path::new(&out_path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    let fresh = done.is_empty();
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out_path)?;
    if fresh {
        writeln!(f, "article_id\tsport\tlabel\tms\ttitle")?;
    }

    let opts = GenerateOptions {
        system: Some(SYSTEM_PROMPT.to_string()),
        temperature: Some(0.0),
        num_predict: 32,
        num_ctx: 0,
        json_mode: true,
    };

    let (mut n_done, mut n_transfer, mut n_other, mut n_unparsed) = (0u32, 0u32, 0u32, 0u32);
    for row in &rows {
        let id: i64 = row.get(0);
        if done.contains(&id) {
            continue;
        }
        let sport: Option<String> = row.get(1);
        let title: String = row.get(2);
        let desc: String = row.get(3);
        let text = if desc.trim().is_empty() {
            title.clone()
        } else {
            format!("{title}. {desc}")
        };
        let prompt = format!("Article:\n{text}\n\nClassify.");

        let (label, ms) = match client.generate(&prompt, &opts).await {
            Ok(gen) => {
                let ms = gen.total_duration.as_millis();
                let label = match serde_json::from_str::<BucketReply>(gen.response.trim()) {
                    Ok(r) if r.bucket == "transfer" => "transfer",
                    Ok(r) if r.bucket == "other" => "other",
                    _ => "unparsed",
                };
                (label, ms)
            }
            Err(e) => {
                eprintln!("bucketlabel: article {id}: generate failed: {e:#}");
                ("error", 0)
            }
        };
        match label {
            "transfer" => n_transfer += 1,
            "other" => n_other += 1,
            _ => n_unparsed += 1,
        }
        writeln!(
            f,
            "{id}\t{}\t{label}\t{ms}\t{}",
            sport.as_deref().unwrap_or(""),
            title.replace('\t', " ").replace('\n', " ")
        )?;
        n_done += 1;
        if n_done % 50 == 0 {
            f.flush()?;
            eprintln!(
                "bucketlabel: {n_done} labeled ({n_transfer} transfer / {n_other} other / {n_unparsed} unparsed-or-error)"
            );
        }
        if throttle_ms > 0 {
            tokio::time::sleep(Duration::from_millis(throttle_ms)).await;
        }
    }
    f.flush()?;
    eprintln!(
        "bucketlabel: DONE — {n_done} new labels ({n_transfer} transfer / {n_other} other / {n_unparsed} unparsed-or-error); {} skipped as already labeled",
        done.len()
    );
    Ok(())
}
