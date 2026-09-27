//! Read-only cohort replay: Google candidates -> publisher acquisition -> Laya -> exact
//! storage boundary -> grouped Journalist and Influencer creation on the local model.
//!
//! cargo run --example harvest_cohort -- \
//!   CANDIDATES.jsonl TRACE.json LAYA_ENDPOINT [OLLAMA_BASE_URL] [OLLAMA_MODEL]

use anyhow::{Context, Result};
use futures::{stream, StreamExt};
use scoracle_cognition::application::plugins::build_harvester;
use scoracle_cognition::evidence::fetch::{count_words, fetch_article};
use scoracle_cognition::plugins::harvester::Article;
use scoracle_cognition::plugins::influencer::cognition::{
    self as influencer, Assignment as VibeAssignment, PacketBlock,
};
use scoracle_cognition::plugins::journalist::cognition::{
    self as journalist, Assignment as JournalistAssignment, CorpusExclusions, CorpusItem, Subject,
};
use scoracle_cognition::runtime::providers::ollama::OllamaClient;
use scoracle_cognition::studio::Studio;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const JOURNALIST_ID: &str = "scoracle.character.narrative";
const INFLUENCER_ID: &str = "scoracle.character.vibe";
const CHARACTER_BATCH_BYTES: usize = 14_000;

fn fingerprint(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    hex::encode(&digest[..16])
}

#[derive(Clone)]
struct Accepted {
    article: Article,
    packet: Value,
}

fn load(path: &str) -> Result<Vec<Article>> {
    BufReader::new(File::open(path)?)
        .lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(&line?)
                .with_context(|| format!("decode candidate line {}", index + 1))
        })
        .collect()
}

fn has_tag(packet: &Value, tag: &str) -> bool {
    packet["character_tags"]
        .as_array()
        .is_some_and(|tags| tags.iter().any(|value| value == tag))
}

fn stored_packet(article: &Article, packet: Value) -> Result<(Value, Value)> {
    let bytes = serde_json::to_vec(&packet)?;
    let sha256 = hex::encode(Sha256::digest(&bytes));
    let decoded: Value = serde_json::from_slice(&bytes)?;
    let verified = if decoded["excerpt"].is_null() {
        None
    } else {
        let start = decoded["excerpt"]["start"]
            .as_u64()
            .context("excerpt start")? as usize;
        let end = decoded["excerpt"]["end"].as_u64().context("excerpt end")? as usize;
        let text = decoded["excerpt"]["text"]
            .as_str()
            .context("excerpt text")?;
        anyhow::ensure!(
            article.body.get(start..end) == Some(text),
            "article {} stored context is not its publisher byte range",
            article.article_id
        );
        Some(true)
    };
    Ok((
        decoded,
        json!({
            "encoding": "json",
            "bytes": bytes.len(),
            "sha256": sha256,
            "publisher_source_byte_range_verified": verified
        }),
    ))
}

fn batches(items: &[Accepted], tag: &str) -> Vec<Vec<Accepted>> {
    let mut result = Vec::new();
    let mut current = Vec::new();
    let mut bytes = 0;
    for item in items.iter().filter(|item| has_tag(&item.packet, tag)) {
        let cost = item.packet["headline"].as_str().map_or(0, str::len)
            + item.packet["source"].as_str().map_or(0, str::len)
            + item.packet["excerpt"]["text"].as_str().map_or(0, str::len)
            + 32;
        if !current.is_empty() && bytes + cost > CHARACTER_BATCH_BYTES {
            result.push(std::mem::take(&mut current));
            bytes = 0;
        }
        bytes += cost;
        current.push(item.clone());
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

fn group_by_entity(accepted: &[Accepted]) -> BTreeMap<i32, Vec<Accepted>> {
    let mut grouped = BTreeMap::new();
    for item in accepted {
        grouped
            .entry(item.article.hypothesis.entity_id)
            .or_insert_with(Vec::new)
            .push(item.clone());
    }
    grouped
}

async fn run_journalist(
    studio: &Studio<'_>,
    grouped: &BTreeMap<i32, Vec<Accepted>>,
    now: i64,
) -> Vec<Value> {
    let mut runs = Vec::new();
    for items in grouped.values() {
        for (batch_index, batch) in batches(items, JOURNALIST_ID).into_iter().enumerate() {
            let hypothesis = &batch[0].article.hypothesis;
            let corpus = batch
                .iter()
                .map(|item| CorpusItem {
                    id: item.article.article_id,
                    title: item.article.title.clone(),
                    description: String::new(),
                    harvested_context: item.packet["excerpt"]["text"].as_str().map(str::to_string),
                    source: item.article.source.clone(),
                    published_at_epoch: None,
                })
                .collect::<Vec<_>>();
            let components = journalist::build_narratives_input_components(&corpus);
            let assignment = JournalistAssignment {
                subject: Subject {
                    entity_type: hypothesis.entity_type.clone(),
                    entity_name: hypothesis.name.clone(),
                    sport: hypothesis.sport.clone(),
                },
                corpus: corpus.clone(),
                corpus_exclusions: CorpusExclusions::default(),
                memory: None,
                packet_framing: None,
                input_hash: fingerprint(&components),
                card_score_prev: None,
                options: journalist::generation_options(journalist::NARRATIVES_TEMPERATURE, 8192),
            };
            let started = Instant::now();
            let result = journalist::create(studio, &assignment, now).await;
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            match result {
                Ok(output) => {
                    let prompt = output.call.as_ref().map(|call| call.built_prompt.as_str());
                    let all_context_present = corpus.iter().all(|item| {
                        item.harvested_context.as_deref().is_some_and(|context| {
                            prompt.is_some_and(|value| value.contains(context))
                        })
                    });
                    runs.push(json!({
                        "plugin_id": JOURNALIST_ID,
                        "entity": hypothesis.name,
                        "batch": batch_index + 1,
                        "article_ids": corpus.iter().map(|item| item.id).collect::<Vec<_>>(),
                        "all_harvested_context_present": all_context_present,
                        "model": output.provenance.model_version,
                        "prompt_version": output.provenance.prompt_version,
                        "headline": output.headline,
                        "card_score": output.card_score,
                        "narratives": output.narratives.iter().map(|narrative| json!({
                            "title": narrative.title,
                            "body": narrative.body,
                            "impact": narrative.impact,
                            "input_news_ids": narrative.input_news_ids,
                            "source_names": narrative.source_names
                        })).collect::<Vec<_>>(),
                        "wall_ms": output.call.as_ref().and_then(|call| call.wall_ms),
                        "elapsed_ms": elapsed_ms
                    }));
                }
                Err(error) => runs.push(json!({
                    "plugin_id": JOURNALIST_ID,
                    "entity": hypothesis.name,
                    "batch": batch_index + 1,
                    "article_ids": corpus.iter().map(|item| item.id).collect::<Vec<_>>(),
                    "error": format!("{error:#}"),
                    "elapsed_ms": elapsed_ms
                })),
            }
        }
    }
    runs
}

async fn run_influencer(studio: &Studio<'_>, grouped: &BTreeMap<i32, Vec<Accepted>>) -> Vec<Value> {
    let mut runs = Vec::new();
    for items in grouped.values() {
        for (batch_index, batch) in batches(items, INFLUENCER_ID).into_iter().enumerate() {
            let hypothesis = &batch[0].article.hypothesis;
            let packets = batch
                .iter()
                .map(|item| PacketBlock {
                    packet_id: item.article.article_id,
                    text: format!(
                        "[{}] {} — {}",
                        item.article.source,
                        item.article.title,
                        item.packet["excerpt"]["text"].as_str().unwrap_or_default()
                    ),
                })
                .collect::<Vec<_>>();
            let ids = batch
                .iter()
                .map(|item| item.article.article_id)
                .collect::<Vec<_>>();
            let components =
                json!({"article_ids": ids, "prompt_version": influencer::VIBE_PROMPT_VERSION})
                    .to_string();
            let assignment = VibeAssignment {
                entity_type: hypothesis.entity_type.clone(),
                entity_name: hypothesis.name.clone(),
                sport: hypothesis.sport.clone(),
                packets,
                memory: None,
                previous_score: None,
                input_components_json: components.clone(),
                input_hash: fingerprint(&components),
                options: influencer::generation_options(
                    influencer::VIBE_TEMPERATURE,
                    8192,
                    influencer::VIBE_NUM_PREDICT,
                ),
            };
            let started = Instant::now();
            let result = influencer::create(studio, &assignment).await;
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            match result {
                Ok(output) => runs.push(json!({
                    "plugin_id": INFLUENCER_ID,
                    "entity": hypothesis.name,
                    "batch": batch_index + 1,
                    "article_ids": ids,
                    "model": output.provenance.model_version,
                    "prompt_version": output.provenance.prompt_version,
                    "sentiment": output.sentiment,
                    "hook": output.hook,
                    "body": output.vibe_prompt,
                    "wall_ms": output.call.as_ref().and_then(|call| call.wall_ms),
                    "elapsed_ms": elapsed_ms
                })),
                Err(error) => runs.push(json!({
                    "plugin_id": INFLUENCER_ID,
                    "entity": hypothesis.name,
                    "batch": batch_index + 1,
                    "article_ids": ids,
                    "error": format!("{error:#}"),
                    "elapsed_ms": elapsed_ms
                })),
            }
        }
    }
    runs
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    anyhow::ensure!(
        (4..=6).contains(&args.len()),
        "usage: harvest_cohort CANDIDATES.jsonl TRACE.json LAYA_ENDPOINT [OLLAMA_BASE_URL] [OLLAMA_MODEL]"
    );
    let articles = load(&args[1])?;
    let harvester = build_harvester(args[3].clone())?;
    let started = Instant::now();

    // Acquisition is deliberately uncapped by the retired Editor's per-entity limit.
    // Google ranked these candidates; publisher I/O is still cheaper than either model.
    let acquired = stream::iter(articles.into_iter().enumerate())
        .map(|(index, mut article)| async move {
            if !article.body.trim().is_empty() {
                let words = count_words(&article.body);
                return (
                    index,
                    article,
                    Ok(json!({
                            "source": "retained_publisher_text",
                            "words": words,
                        "elapsed_ms": 0.0
                    })),
                );
            }
            let began = Instant::now();
            let result = fetch_article(&article.url).await;
            match result {
                Ok(fetched) if !fetched.text.trim().is_empty() => {
                    article.body = fetched.text;
                    let words = count_words(&article.body);
                    (
                        index,
                        article,
                        Ok(json!({
                                "source": "publisher_fetch",
                                "final_url": fetched.final_url,
                                "final_domain": fetched.final_domain,
                                "words": words,
                            "elapsed_ms": began.elapsed().as_secs_f64() * 1000.0
                        })),
                    )
                }
                Ok(fetched) => (
                    index,
                    article,
                    Err(json!({
                        "outcome": "publisher_text_error",
                        "error": "publisher fetch returned empty extracted text",
                        "final_url": fetched.final_url,
                        "elapsed_ms": began.elapsed().as_secs_f64() * 1000.0
                    })),
                ),
                Err(error) => (
                    index,
                    article,
                    Err(json!({
                        "outcome": "publisher_fetch_error",
                        "error": format!("{error:#}"),
                        "elapsed_ms": began.elapsed().as_secs_f64() * 1000.0
                    })),
                ),
            }
        })
        .buffer_unordered(4)
        .collect::<Vec<_>>()
        .await;

    let mut rows = Vec::new();
    let mut ready = Vec::new();
    for (index, article, acquisition) in acquired {
        match acquisition {
            Ok(acquisition) => ready.push((index, article, acquisition)),
            Err(failure) => rows.push((
                index,
                json!({
                    "article_id": article.article_id,
                    "entity": article.hypothesis.name,
                    "feed_rank": article.feed_rank,
                    "headline": article.title,
                    "source": article.source,
                    "url": article.url,
                    "outcome": failure["outcome"],
                    "error": failure["error"],
                    "acquisition": failure
                }),
                None,
            )),
        }
    }

    let classified = stream::iter(ready)
        .map(|(index, article, acquisition)| {
            let harvester = &harvester;
            async move {
                let began = Instant::now();
                match harvester.classify(&article).await {
                    Ok(classification) => {
                        let relevant = match classification.relevant() {
                            Ok(value) => value,
                            Err(error) => {
                                return (
                                    index,
                                    json!({
                                        "article_id": article.article_id,
                                        "entity": article.hypothesis.name,
                                        "outcome": "classification_error",
                                        "error": format!("{error:#}"),
                                        "acquisition": acquisition
                                    }),
                                    None,
                                );
                            }
                        };
                        let classify_ms = began.elapsed().as_secs_f64() * 1000.0;
                        let packet = match harvester.compile(&article, classification) {
                            Ok(packet) => packet,
                            Err(error) => {
                                return (
                                    index,
                                    json!({
                                        "article_id": article.article_id,
                                        "entity": article.hypothesis.name,
                                        "outcome": "compile_error",
                                        "error": format!("{error:#}"),
                                        "acquisition": acquisition
                                    }),
                                    None,
                                );
                            }
                        };
                        match stored_packet(&article, packet) {
                            Ok((packet, storage)) => {
                                let accepted = relevant.then(|| Accepted {
                                    article: article.clone(),
                                    packet: packet.clone(),
                                });
                                (
                                    index,
                                    json!({
                                        "article_id": article.article_id,
                                        "entity": article.hypothesis.name,
                                        "feed_rank": article.feed_rank,
                                        "outcome": if relevant { "accepted" } else { "rejected" },
                                        "classify_ms": classify_ms,
                                        "acquisition": acquisition,
                                        "packet": packet,
                                        "storage": storage
                                    }),
                                    accepted,
                                )
                            }
                            Err(error) => (
                                index,
                                json!({
                                    "article_id": article.article_id,
                                    "entity": article.hypothesis.name,
                                    "outcome": "storage_error",
                                    "error": format!("{error:#}"),
                                    "acquisition": acquisition
                                }),
                                None,
                            ),
                        }
                    }
                    Err(error) => (
                        index,
                        json!({
                            "article_id": article.article_id,
                            "entity": article.hypothesis.name,
                            "outcome": "classification_error",
                            "error": format!("{error:#}"),
                            "acquisition": acquisition
                        }),
                        None,
                    ),
                }
            }
        })
        .buffer_unordered(2)
        .collect::<Vec<_>>()
        .await;
    rows.extend(classified);
    rows.sort_by_key(|(index, _, _)| *index);
    let accepted = rows
        .iter()
        .filter_map(|(_, _, accepted)| accepted.clone())
        .collect::<Vec<_>>();
    let article_records = rows.into_iter().map(|(_, row, _)| row).collect::<Vec<_>>();

    let model_base = args
        .get(4)
        .map(String::as_str)
        .unwrap_or("http://127.0.0.1:11434");
    let model_name = args.get(5).map(String::as_str).unwrap_or("granite4.2:3b");
    let model = OllamaClient::with_think(
        model_base,
        model_name,
        Duration::from_secs(600),
        Some(false),
    )?;
    let studio = Studio::new(&model);
    let grouped = group_by_entity(&accepted);
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let mut character_runs = run_journalist(&studio, &grouped, now).await;
    character_runs.extend(run_influencer(&studio, &grouped).await);

    let mut outcomes = BTreeMap::<String, usize>::new();
    let mut tags = BTreeMap::<String, usize>::new();
    for row in &article_records {
        *outcomes
            .entry(row["outcome"].as_str().unwrap_or("unknown").to_string())
            .or_default() += 1;
        if let Some(values) = row["packet"]["character_tags"].as_array() {
            for tag in values.iter().filter_map(Value::as_str) {
                *tags.entry(tag.to_string()).or_default() += 1;
            }
        }
    }
    let trace = json!({
        "cohort_contract": "google-publisher-laya-storage-characters-v1",
        "canonical_candidate_count": article_records.len(),
        "outcomes": outcomes,
        "character_tags": tags,
        "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
        "articles": article_records,
        "character_runs": character_runs
    });
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    writeln!(output, "{}", serde_json::to_string_pretty(&trace)?)?;
    println!(
        "cohort complete: {} canonical candidates, {} materialized accepts, {} character runs; trace {}",
        trace["canonical_candidate_count"],
        accepted.len(),
        trace["character_runs"].as_array().map_or(0, Vec::len),
        args[2]
    );
    Ok(())
}
