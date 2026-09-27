//! One-article, read-only packet-free Harvester → Journalist smoke trace.
//!
//! cargo run --example harvest_smoke -- \
//!   CORPUS.jsonl ARTICLE_ID TRACE.json LAYA_ENDPOINT [OLLAMA_BASE_URL] [OLLAMA_MODEL]
//!
//! The trace is created once and never overwrites prior evidence. No production database,
//! queue, or publication table is touched.

use anyhow::{Context, Result};
use scoracle_cognition::plugins::harvester::{context, Article};
use scoracle_cognition::plugins::journalist::cognition::{
    self as journalist, Assignment, CorpusExclusions, CorpusItem, Subject,
};
use scoracle_cognition::runtime::providers::ollama::OllamaClient;
use scoracle_cognition::runtime::providers::system_one::SystemOneClient;
use scoracle_cognition::studio::Studio;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn load_article(path: &str, wanted: i64) -> Result<Article> {
    for (index, line) in BufReader::new(File::open(path)?).lines().enumerate() {
        let article: Article = serde_json::from_str(&line?)
            .with_context(|| format!("decode corpus line {}", index + 1))?;
        if article.article_id == wanted {
            return Ok(article);
        }
    }
    anyhow::bail!("article {wanted} was not present in {path}")
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(
        (5..=7).contains(&args.len()),
        "usage: harvest_smoke CORPUS.jsonl ARTICLE_ID TRACE.json LAYA_ENDPOINT [OLLAMA_BASE_URL] [OLLAMA_MODEL]"
    );
    let article_id: i64 = args[2].parse().context("ARTICLE_ID must be an integer")?;
    let article = load_article(&args[1], article_id)?;
    let laya = SystemOneClient::new(args[4].clone())?;
    let harvested = context::classify(&laya, &article).await?;
    harvested.verify_against(&article)?;

    // Exercise a serialization boundary before character preparation. This represents
    // the exact payload a durable adapter stores; byte-range verification prevents a
    // summary or normalization from masquerading as harvested source.
    let stored_bytes = serde_json::to_vec(&harvested)?;
    let stored_sha256 = hex::encode(Sha256::digest(&stored_bytes));
    let stored: context::HarvestContext = serde_json::from_slice(&stored_bytes)?;
    stored.verify_against(&article)?;
    let start = stored.context.start;
    let end = stored.context.end;
    let text = stored.context.text.as_str();
    anyhow::ensure!(
        article.body.get(start..end) == Some(text),
        "stored character context is not the source byte range"
    );

    let model_base = args
        .get(5)
        .map(String::as_str)
        .unwrap_or("http://127.0.0.1:11434");
    let model_name = args.get(6).map(String::as_str).unwrap_or("granite4.2:3b");
    let model = OllamaClient::with_think(
        model_base,
        model_name,
        Duration::from_secs(600),
        Some(false),
    )?;
    let assignment = Assignment {
        subject: Subject {
            entity_type: article.hypothesis.entity_type.clone(),
            entity_name: article.hypothesis.name.clone(),
            sport: article.hypothesis.sport.clone(),
        },
        corpus: vec![CorpusItem {
            id: article.article_id,
            title: article.title.clone(),
            description: String::new(),
            harvested_context: Some(text.to_string()),
            source: article.source.clone(),
            published_at_epoch: None,
        }],
        corpus_exclusions: CorpusExclusions::default(),
        memory: None,
        packet_framing: None,
        input_hash: stored_sha256.clone(),
        card_score_prev: None,
        options: journalist::generation_options(0.0, 4096),
    };
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let output = journalist::create(&Studio::new(&model), &assignment, now).await?;
    let built_prompt = output
        .call
        .as_ref()
        .map(|call| call.built_prompt.as_str())
        .context("Journalist unexpectedly made no model call")?;
    anyhow::ensure!(
        built_prompt.contains(text),
        "Journalist prompt did not contain the full harvested context"
    );

    let narratives = output
        .narratives
        .iter()
        .map(|narrative| {
            json!({
                "title": narrative.title,
                "body": narrative.body,
                "impact": narrative.impact,
                "input_news_ids": narrative.input_news_ids,
                "source_names": narrative.source_names
            })
        })
        .collect::<Vec<_>>();
    let trace = json!({
        "smoke_contract": "harvest-context-v1-readonly-journalist",
        "ingestion": {
            "article_id": article.article_id,
            "headline": article.title,
            "source": article.source,
            "url": article.url,
            "google_feed_rank": article.feed_rank,
            "query_hypothesis": article.hypothesis
        },
        "harvester_context": harvested,
        "storage_boundary": {
            "encoding": "json",
            "sha256": stored_sha256,
            "bytes": stored_bytes.len(),
            "source_byte_range_verified": true
        },
        "character": {
            "plugin_id": scoracle_cognition::plugins::journalist::manifest::MANIFEST.id.as_str(),
            "model": output.provenance.model_version,
            "prompt_version": output.provenance.prompt_version,
            "input_ids": output.provenance.input_ids,
            "full_harvested_context_in_prompt": true,
            "headline": output.headline,
            "card_score": output.card_score,
            "narratives": narratives,
            "wall_ms": output.call.as_ref().and_then(|call| call.wall_ms),
            "eval_count": output.call.as_ref().and_then(|call| call.eval_count)
        }
    });
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[3])
        .with_context(|| format!("create new smoke trace {}", args[3]))?;
    writeln!(file, "{}", serde_json::to_string_pretty(&trace)?)?;
    println!(
        "smoke passed: article {} -> {} advisory route(s) -> {} Journalist narrative(s); trace {}",
        article.article_id,
        trace["harvester_context"]["recommended_characters"]
            .as_array()
            .map_or(0, Vec::len),
        trace["character"]["narratives"]
            .as_array()
            .map_or(0, Vec::len),
        args[3]
    );
    Ok(())
}
