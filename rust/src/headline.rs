//! Headlines stage — entity-scoped breaking-news bulletins for the News product.
//!
//! Runs immediately after scrub on the vetted article stream: for each entity with a fresh
//! vetted corpus, ask the local model to identify high-impact, time-sensitive headlines and emit
//! one-sentence blurbs with categories (transfer, injury, coaching, contract, other).
//!
//! This is a News component stage, not a standalone product pillar. It sits before
//! transfers/narratives conceptually but operates as an independent per-entity queue stage
//! (`Stage::Headlines`) so it composes the same `route + extract + persist` primitives as the
//! other handlers. The Go API should package it under News/Meta surfaces.

use crate::harness::{Harness, Parser};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::util::truncate;
use crate::work::{Item, Stage};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use tracing::warn;

/// Prompt version for provenance. Bump when the headline prompt changes.
pub const HEADLINES_PROMPT_VERSION: &str = "h2";

/// Low temperature for a classification/extraction task.
pub const HEADLINES_TEMPERATURE: f64 = 0.3;

/// Enough room for a few one-sentence headlines in JSON.
pub const HEADLINES_NUM_PREDICT: i32 = 1200;

/// Cap the corpus fed to the headline detector (same 72h window as narratives, bounded count).
const MAX_HEADLINE_CORPUS: i64 = 15;

/// Lookback window in seconds (72h, same as narratives/vibe corpus freshness).
const HEADLINES_LOOKBACK_SECS: f64 = 259_200.0;

/// Body truncation for the prompt (snippets are short; cap for token budget).
const DESC_TRUNCATE: usize = 200;

/// System prompt: classify breaking headline news and emit structured JSON.
pub const HEADLINES_SYSTEM_PROMPT: &str = r#"Task: identify breaking headline news about ONE specific sports entity.

Categories: transfer, injury, coaching, contract, other.

Return STRICT JSON only (no markdown fences, no text before or after):
{"headlines": [{"title": "one-sentence breaking-news blurb", "category": "transfer|injury|coaching|contract|other", "article_numbers": [1]}]}

Rules:
- Only flag time-sensitive, high-impact news ABOUT this entity.
- transfer = a current move/interest involving this entity (not background or historical).
- injury = a significant injury or absence reported for this entity.
- coaching = a coaching/managerial change involving this entity.
- contract = a new contract, extension, or contract dispute involving this entity.
- other = any other unexpected, notable event (suspension, retirement, record broken, etc.).
- Do NOT flag routine match reports, general praise, power rankings, listicles, mailbags, notes columns, or passing mentions.
- If no breaking headline is present, return {"headlines": []}.
- Each title must be one concrete sentence naming the key people/clubs and what happened.
- article_numbers refers to the numbered articles in the user prompt (1-indexed)."#;

/// One article in the entity's recent vetted corpus.
#[derive(Clone, Debug)]
pub struct CorpusItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub source: String,
    pub url: String,
    pub published_at_epoch: Option<i64>,
}

/// One headline the model emitted.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct HeadlineVerdict {
    pub title: String,
    pub category: String,
    #[serde(default)]
    pub article_numbers: Vec<i32>,
}

/// Parsed JSON response wrapper.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ParsedHeadlines {
    #[serde(default)]
    pub headlines: Vec<HeadlineVerdict>,
}

/// Fail-closed parser: unparseable JSON → retryable error; empty array is a legitimate
/// "no headlines this cycle" result.
pub struct HeadlinesParser;

impl Parser<ParsedHeadlines> for HeadlinesParser {
    fn parse(&self, raw: &str) -> Result<Option<ParsedHeadlines>> {
        let (start, end) = match (raw.find('{'), raw.rfind('}')) {
            (Some(s), Some(e)) if e > s => (s, e),
            _ => {
                return Err(anyhow::anyhow!(
                    "headlines: no JSON object in response (raw={:?})",
                    truncate(raw, 200)
                ))
            }
        };
        match serde_json::from_str::<ParsedHeadlines>(&raw[start..=end]) {
            Ok(v) => Ok(Some(v)),
            Err(e) => Err(anyhow::anyhow!(
                "headlines: unparseable JSON (raw={:?}): {e}",
                truncate(raw, 200)
            )),
        }
    }
}

/// Load the entity's recent vetted articles (same 72h window as narratives).
#[allow(clippy::type_complexity)]
pub async fn load_vetted_corpus(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<CorpusItem>> {
    let rows: Vec<(i64, String, Option<String>, String, String, Option<i64>)> = sqlx::query_as(
        r#"
        SELECT a.id,
               a.title,
               a.description,
               COALESCE(a.source, '') AS source,
               a.url,
               EXTRACT(EPOCH FROM a.published_at)::bigint AS published_at_epoch
          FROM news_article_entities nae
          JOIN news_articles a ON a.id = nae.article_id
         WHERE nae.entity_type = $1
           AND nae.entity_id   = $2
           AND nae.sport       = $3
           AND nae.vetted IS TRUE
           AND a.published_at > NOW() - make_interval(secs => $4)
         ORDER BY a.published_at DESC
         LIMIT $5
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(HEADLINES_LOOKBACK_SECS)
    .bind(MAX_HEADLINE_CORPUS)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load headline corpus {entity_type}/{entity_id} ({sport})"))?;

    Ok(rows
        .into_iter()
        .map(
            |(id, title, description, source, url, published_at_epoch)| CorpusItem {
                id,
                title,
                description: description.unwrap_or_default(),
                source,
                url,
                published_at_epoch,
            },
        )
        .collect())
}

/// Look up the entity's display name (player or team).
pub async fn lookup_entity_name(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<String> {
    let query = if entity_type == "player" {
        "SELECT name FROM players WHERE id = $1 AND sport = $2"
    } else {
        "SELECT name FROM teams WHERE id = $1 AND sport = $2"
    };
    let name: String = sqlx::query_scalar(query)
        .bind(entity_id)
        .bind(sport)
        .fetch_one(pool)
        .await
        .with_context(|| format!("lookup {entity_type}/{entity_id} ({sport})"))?;
    if name.is_empty() {
        bail!("empty name for {entity_type}/{entity_id} ({sport})");
    }
    Ok(name)
}

/// Build the user prompt: numbered articles about the entity.
pub fn build_headlines_prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    corpus: &[CorpusItem],
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Recent articles about {entity_type} {entity_name} ({sport}):\n\n"
    ));
    for (i, item) in corpus.iter().enumerate() {
        let n = i + 1;
        b.push_str(&format!(
            "{n}. [{}] {}\n   {}\n\n",
            item.source,
            item.title,
            truncate(&item.description, DESC_TRUNCATE)
        ));
    }
    b.push_str("Identify any breaking headline news. Return JSON only.");
    b
}

/// A headline normalized for persistence, with resolved source info + provenance.
#[derive(Clone, Debug)]
pub struct HeadlineRow {
    pub title: String,
    pub category: String,
    pub source_url: String,
    pub source_name: String,
    pub published_at_epoch: Option<i64>,
    /// Article IDs from the vetted corpus cited by the model for this headline.
    pub input_news_ids: Vec<i64>,
}

/// Generate headlines for one entity. Returns an empty vector when the corpus is empty
/// or the model finds no breaking news (both are no-ops for persistence).
pub async fn generate_headlines(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport_raw: &str,
) -> Result<(Vec<HeadlineRow>, String)> {
    if entity_id <= 0 || entity_name.is_empty() || sport_raw.is_empty() || entity_type.is_empty() {
        bail!("headlines: entity context incomplete");
    }
    let sport = sport_raw.to_uppercase();

    let corpus = load_vetted_corpus(&hx.pool, entity_type, entity_id, &sport).await?;
    if corpus.is_empty() {
        return Ok((Vec::new(), String::new()));
    }

    let prompt = build_headlines_prompt(entity_type, entity_name, sport_raw, &corpus);
    let opts = GenerateOptions {
        system: Some(HEADLINES_SYSTEM_PROMPT.to_string()),
        temperature: Some(HEADLINES_TEMPERATURE),
        num_predict: HEADLINES_NUM_PREDICT,
        json_mode: false,
    };

    let extracted = hx
        .extract(Role::EmotionalNews, &prompt, &opts, &HeadlinesParser)
        .await?;

    let parsed = extracted
        .value
        .ok_or_else(|| anyhow::anyhow!("headlines: parser returned no value"))?;

    let mut rows = Vec::new();
    for verdict in parsed.headlines {
        let category = normalize_category(&verdict.category);
        if category.is_empty() {
            warn!(title = %verdict.title, category = %verdict.category, "headlines: skipping unknown category");
            continue;
        }
        if verdict.title.trim().is_empty() {
            warn!(category = %category, "headlines: skipping empty title");
            continue;
        }

        // Resolve source info + cited article IDs from the first valid cited article,
        // falling back to the newest article when the model cites none or out-of-range ids.
        let cited: Vec<&CorpusItem> = verdict
            .article_numbers
            .iter()
            .filter_map(|n| {
                let idx = (*n).saturating_sub(1) as usize;
                corpus.get(idx)
            })
            .collect();
        let source_item = cited.first().copied().unwrap_or(&corpus[0]);

        rows.push(HeadlineRow {
            title: verdict.title.trim().to_string(),
            category,
            source_url: source_item.url.clone(),
            source_name: source_item.source.clone(),
            published_at_epoch: source_item.published_at_epoch,
            input_news_ids: cited.iter().map(|item| item.id).collect(),
        });
    }

    Ok((dedupe_headline_rows(rows), extracted.model))
}

/// Normalize a model-returned category to the allowed set.
fn normalize_category(raw: &str) -> String {
    match raw.to_lowercase().trim() {
        "transfer" => "transfer".to_string(),
        "injury" => "injury".to_string(),
        "coaching" => "coaching".to_string(),
        "contract" => "contract".to_string(),
        "other" => "other".to_string(),
        _ => String::new(),
    }
}

fn headline_dedupe_key(row: &HeadlineRow) -> String {
    let source_url = row.source_url.trim().to_lowercase();
    if !source_url.is_empty() {
        return format!("url:{source_url}");
    }

    format!(
        "source-title:{}:{}",
        row.source_name.trim().to_lowercase(),
        row.title
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    )
}

fn category_rank(category: &str) -> i32 {
    match category {
        "transfer" | "injury" | "coaching" | "contract" => 2,
        "other" => 1,
        _ => 0,
    }
}

fn merge_article_ids(dst: &mut Vec<i64>, src: &[i64]) {
    let mut seen: HashSet<i64> = dst.iter().copied().collect();
    for id in src {
        if seen.insert(*id) {
            dst.push(*id);
        }
    }
}

fn dedupe_headline_rows(rows: Vec<HeadlineRow>) -> Vec<HeadlineRow> {
    let mut out: Vec<HeadlineRow> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();

    for row in rows {
        let key = headline_dedupe_key(&row);
        if let Some(&idx) = positions.get(&key) {
            if category_rank(&row.category) > category_rank(&out[idx].category) {
                out[idx].category = row.category.clone();
            }
            merge_article_ids(&mut out[idx].input_news_ids, &row.input_news_ids);
            continue;
        }

        positions.insert(key, out.len());
        out.push(row);
    }

    out
}

/// Persist generated headlines to the live `headlines` table with full provenance.
async fn persist_headlines(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    rows: &[HeadlineRow],
    model_version: &str,
) -> Result<()> {
    let entity_id = item.entity_id_i32()?;
    for row in rows {
        sqlx::query(
            r#"
            INSERT INTO headlines (
                sport, entity_type, entity_id,
                title, category, source_url, source_name, published_at,
                input_news_ids, model_version, prompt_version, trigger_type, generated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, to_timestamp($8),
                $9, $10, $11, $12, NOW()
            )
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(sport)
        .bind(&item.entity_type)
        .bind(entity_id)
        .bind(&row.title)
        .bind(&row.category)
        .bind(&row.source_url)
        .bind(&row.source_name)
        .bind(row.published_at_epoch)
        .bind(&row.input_news_ids)
        .bind(model_version)
        .bind(HEADLINES_PROMPT_VERSION)
        .bind("news_spike")
        .execute(pool)
        .await
        .context("persist headline")?;
    }
    Ok(())
}

/// HeadlinesHandler drains the durable `headlines` stage.
pub struct HeadlinesHandler;

impl HeadlinesHandler {
    pub fn new() -> Self {
        HeadlinesHandler
    }
}

impl Default for HeadlinesHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for HeadlinesHandler {
    fn stage(&self) -> Stage {
        Stage::Headlines
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let entity_id = item.entity_id_i32()?;
        let name = lookup_entity_name(&hx.pool, &item.entity_type, entity_id, &item.sport).await?;

        let (rows, model_version) =
            generate_headlines(hx, &item.entity_type, entity_id, &name, &item.sport).await?;

        if !rows.is_empty() {
            let sport = item.sport.to_uppercase();
            persist_headlines(&hx.pool, item, &sport, &rows, &model_version).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_extracts_headlines() {
        let raw = r#"{"headlines": [{"title": "Bowen signs 5-year extension", "category": "contract", "article_numbers": [1]}]}"#;
        let parsed = HeadlinesParser.parse(raw).unwrap().unwrap();
        assert_eq!(parsed.headlines.len(), 1);
        assert_eq!(parsed.headlines[0].title, "Bowen signs 5-year extension");
        assert_eq!(parsed.headlines[0].category, "contract");
        assert_eq!(parsed.headlines[0].article_numbers, vec![1]);
    }

    #[test]
    fn parser_returns_empty_array() {
        let parsed = HeadlinesParser
            .parse("{\"headlines\": []}")
            .unwrap()
            .unwrap();
        assert!(parsed.headlines.is_empty());
    }

    #[test]
    fn parser_fails_on_bad_json() {
        assert!(HeadlinesParser.parse("not json").is_err());
    }

    #[test]
    fn normalize_category_maps_and_rejects() {
        assert_eq!(normalize_category("Transfer"), "transfer");
        assert_eq!(normalize_category("INJURY"), "injury");
        assert_eq!(normalize_category("nonsense"), "");
    }

    #[test]
    fn build_prompt_numbers_articles() {
        let corpus = vec![
            CorpusItem {
                id: 1,
                title: "T1".to_string(),
                description: "D1".to_string(),
                source: "S1".to_string(),
                url: "http://1".to_string(),
                published_at_epoch: Some(1),
            },
            CorpusItem {
                id: 2,
                title: "T2".to_string(),
                description: "D2".to_string(),
                source: "S2".to_string(),
                url: "http://2".to_string(),
                published_at_epoch: Some(2),
            },
        ];
        let p = build_headlines_prompt("player", "Bowen", "football", &corpus);
        assert!(p.contains("1. [S1] T1"));
        assert!(p.contains("2. [S2] T2"));
        assert!(p.contains("Bowen"));
    }

    #[test]
    fn parser_accepts_article_numbers_array() {
        let raw =
            r#"{"headlines": [{"title": "A", "category": "other", "article_numbers": [1, 2]}]}"#;
        let parsed = HeadlinesParser.parse(raw).unwrap().unwrap();
        assert_eq!(parsed.headlines[0].article_numbers, vec![1, 2]);
    }

    #[test]
    fn dedupe_rows_collapses_same_source_url_and_prefers_specific_category() {
        let rows = vec![
            HeadlineRow {
                title: "Sam Kerr signs with Gotham FC after Chelsea exit".to_string(),
                category: "other".to_string(),
                source_url: "https://example.com/kerr".to_string(),
                source_name: "Al Jazeera".to_string(),
                published_at_epoch: Some(10),
                input_news_ids: vec![1],
            },
            HeadlineRow {
                title: "Sam Kerr signs with Gotham FC after Chelsea exit".to_string(),
                category: "transfer".to_string(),
                source_url: " https://example.com/kerr ".to_string(),
                source_name: "Al Jazeera".to_string(),
                published_at_epoch: Some(10),
                input_news_ids: vec![1, 2],
            },
        ];

        let deduped = dedupe_headline_rows(rows);

        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].category, "transfer");
        assert_eq!(deduped[0].input_news_ids, vec![1, 2]);
    }

    #[test]
    fn dedupe_rows_uses_source_and_title_when_url_missing() {
        let rows = vec![
            HeadlineRow {
                title:
                    "Australian football legend Sam Kerr signs with Gotham FC after Chelsea exit"
                        .to_string(),
                category: "other".to_string(),
                source_url: "".to_string(),
                source_name: "Al Jazeera".to_string(),
                published_at_epoch: Some(10),
                input_news_ids: vec![1],
            },
            HeadlineRow {
                title:
                    "Australian   football legend Sam Kerr signs with Gotham FC after Chelsea exit"
                        .to_string(),
                category: "transfer".to_string(),
                source_url: "".to_string(),
                source_name: "al jazeera".to_string(),
                published_at_epoch: Some(10),
                input_news_ids: vec![2],
            },
        ];

        let deduped = dedupe_headline_rows(rows);

        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].category, "transfer");
        assert_eq!(deduped[0].input_news_ids, vec![1, 2]);
    }
}
