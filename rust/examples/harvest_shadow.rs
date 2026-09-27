//! Read-only packet-free Harvester replay over a retained publisher corpus.
//!
//! cargo run --example harvest_shadow -- CORPUS.jsonl SUMMARY.json http://127.0.0.1:8019/v1/systemone
//! The summary contains hashes, timings, and advisory choices, never publisher text.
use anyhow::{Context, Result};
use scoracle_cognition::plugins::harvester::{context, Article};
use scoracle_cognition::runtime::providers::system_one::SystemOneClient;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 4,
        "usage: harvest_shadow CORPUS.jsonl SUMMARY.json LAYA_ENDPOINT"
    );
    let model = SystemOneClient::new(args[3].clone())?;
    let mut rows = Vec::new();
    let mut counts = BTreeMap::<String, usize>::new();
    let started = Instant::now();
    for (index, line) in BufReader::new(File::open(&args[1])?).lines().enumerate() {
        let article: Article = serde_json::from_str(&line?)
            .with_context(|| format!("decode corpus line {}", index + 1))?;
        let body_hash = hex::encode(Sha256::digest(article.body.as_bytes()));
        if article.body.trim().is_empty() {
            *counts.entry("missing_local_body".into()).or_default() += 1;
            rows.push(json!({
                "article_id": article.article_id,
                "query_entity": article.hypothesis,
                "state": "missing_local_body",
            }));
            continue;
        }
        let before = Instant::now();
        let result = context::classify(&model, &article).await;
        let elapsed_ms = before.elapsed().as_millis();
        match result {
            Ok(context) => {
                context.verify_against(&article)?;
                *counts.entry("classified".into()).or_default() += 1;
                *counts
                    .entry(format!("entity_{}", context.entity_choice))
                    .or_default() += 1;
                *counts
                    .entry(format!("routes_{}", context.recommended_characters.len()))
                    .or_default() += 1;
                rows.push(json!({
                    "article_id": article.article_id,
                    "query_entity": article.hypothesis,
                    "state": "classified",
                    "body_sha256": body_hash,
                    "context_sha256": hex::encode(Sha256::digest(context.context.text.as_bytes())),
                    "context_bytes": context.context.text.len(),
                    "entity_choice": context.entity_choice,
                    "advisory_characters": context.recommended_characters,
                    "model_revision": context.model_revision,
                    "elapsed_ms": elapsed_ms,
                }));
            }
            Err(error) => {
                *counts.entry("classification_error".into()).or_default() += 1;
                let error_class = if error.chain().any(|cause| cause.is::<reqwest::Error>()) {
                    "transport_or_http"
                } else {
                    "classification_contract"
                };
                rows.push(json!({
                    "article_id": article.article_id,
                    "query_entity": article.hypothesis,
                    "state": "classification_error",
                    "error_class": error_class,
                    "body_sha256": body_hash,
                    "elapsed_ms": elapsed_ms,
                }));
            }
        }
    }
    let summary: Value = json!({
        "contract": context::CONTRACT,
        "read_only": true,
        "source_corpus": args[1],
        "elapsed_ms": started.elapsed().as_millis(),
        "counts": counts,
        "rows": rows,
    });
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])
        .with_context(|| format!("create shadow summary {}", args[2]))?;
    serde_json::to_writer_pretty(&mut out, &summary)?;
    writeln!(out)?;
    println!("{}", serde_json::to_string(&summary["counts"])?);
    anyhow::ensure!(
        counts.get("classified").copied().unwrap_or(0) > 0,
        "no publisher body was classified; check the full Laya endpoint and summary"
    );
    Ok(())
}
