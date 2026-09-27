//! Recover missing publisher bodies in a local, read-only Harvester shadow corpus.
//!
//! cargo run --example harvest_acquire_shadow -- INPUT.jsonl OUTPUT.jsonl SUMMARY.json
//! OUTPUT contains publisher text and must stay outside Git. No database or queue is touched.
use anyhow::{Context, Result};
use futures::{stream, StreamExt};
use scoracle_cognition::evidence::fetch::{
    count_words, domain_of, fetch_article, ArticleHttpStatus,
};
use scoracle_cognition::plugins::harvester::Article;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::time::Instant;

const MIN_PUBLISHER_WORDS: usize = 20;
const FETCH_CONCURRENCY: usize = 4;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 4,
        "usage: harvest_acquire_shadow INPUT.jsonl OUTPUT.jsonl SUMMARY.json"
    );
    let articles: Vec<Article> = BufReader::new(File::open(&args[1])?)
        .lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(&line?)
                .with_context(|| format!("decode candidate line {}", index + 1))
        })
        .collect::<Result<_>>()?;
    let query_edges = articles.len();
    let mut canonical = BTreeMap::<i64, Article>::new();
    let mut edge_counts = BTreeMap::<i64, usize>::new();
    for article in &articles {
        *edge_counts.entry(article.article_id).or_default() += 1;
        match canonical.get_mut(&article.article_id) {
            None => {
                canonical.insert(article.article_id, article.clone());
            }
            Some(existing) => {
                anyhow::ensure!(
                    existing.url == article.url
                        && existing.title == article.title
                        && existing.source == article.source,
                    "canonical article {} has conflicting source metadata",
                    article.article_id
                );
                if existing.body.trim().is_empty() {
                    existing.body = article.body.clone();
                } else if !article.body.trim().is_empty() {
                    anyhow::ensure!(
                        existing.body == article.body,
                        "canonical article {} has conflicting retained bodies",
                        article.article_id
                    );
                }
            }
        }
    }
    let canonical_articles = canonical.len();
    let started = Instant::now();
    let results = stream::iter(canonical.into_values())
        .map(|mut article| async move {
            let article_id = article.article_id;
            let (state, final_domain, observed_words, error) = if article.body.trim().is_empty() {
                match fetch_article(&article.url).await {
                    Ok(fetched) if count_words(&fetched.text) >= MIN_PUBLISHER_WORDS => {
                        article.body = fetched.text;
                        (
                            "acquired",
                            fetched.final_domain,
                            count_words(&article.body),
                            None,
                        )
                    }
                    Ok(fetched) => (
                        "low_content",
                        fetched.final_domain,
                        count_words(&fetched.text),
                        None,
                    ),
                    Err(error) => {
                        let status = error
                            .chain()
                            .find_map(|cause| cause.downcast_ref::<ArticleHttpStatus>());
                        let blocked = status.is_some_and(ArticleHttpStatus::is_access_denied);
                        (
                            if blocked {
                                "blocked"
                            } else {
                                "retryable_error"
                            },
                            status.and_then(|status| domain_of(&status.final_url)),
                            0,
                            Some(format!("{error:#}")),
                        )
                    }
                }
            } else {
                (
                    "retained_local_body",
                    None,
                    count_words(&article.body),
                    None,
                )
            };
            let body_hash = if article.body.is_empty() {
                None
            } else {
                Some(hex::encode(Sha256::digest(article.body.as_bytes())))
            };
            let receipt = json!({
                "article_id": article_id,
                "state": state,
                "final_domain": final_domain,
                "body_sha256": body_hash,
                "observed_words": observed_words,
                "error": error,
            });
            (article, receipt)
        })
        .buffer_unordered(FETCH_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    let mut fetched = BTreeMap::new();
    let mut receipts = Vec::with_capacity(results.len());
    for (article, mut receipt) in results {
        let article_id = article.article_id;
        receipt["query_edges"] = json!(edge_counts[&article_id]);
        fetched.insert(article_id, article.body);
        receipts.push(receipt);
    }
    receipts.sort_by_key(|receipt| receipt["article_id"].as_i64());

    let mut corpus = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[2])
            .with_context(|| format!("create shadow corpus {}", args[2]))?,
    );
    for mut article in articles {
        article.body = fetched[&article.article_id].clone();
        serde_json::to_writer(&mut corpus, &article)?;
        writeln!(corpus)?;
    }
    corpus.flush()?;
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for receipt in &receipts {
        let state = receipt["state"].as_str().context("shadow state")?;
        *counts.entry(state.to_string()).or_default() += 1;
    }
    let summary = json!({
        "contract": "harvest-acquisition-shadow-v1",
        "read_only": true,
        "source_corpus": args[1],
        "output_corpus": args[2],
        "query_edges": query_edges,
        "canonical_articles": canonical_articles,
        "elapsed_ms": started.elapsed().as_millis(),
        "counts": counts,
        "rows": receipts,
    });
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[3])
        .with_context(|| format!("create shadow summary {}", args[3]))?;
    serde_json::to_writer_pretty(&mut out, &summary)?;
    writeln!(out)?;
    println!("{}", serde_json::to_string(&summary["counts"])?);
    Ok(())
}
