//! The Journalist creates a grounded edition from a prepared corpus.
//!
//! Studio owns the brief, prompt, tolerant parser, deterministic grounding and impact scoring,
//! and the model session. Packet retrieval, memory loading, debounce, queue ownership, storyline
//! progression, and publication belong to the application adapter.

use crate::studio::model::GenerateOptions;
use crate::studio::{Generation, GenerationCall, Parser, Studio};
use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;

mod brief;
mod inputs;
pub use crate::studio::form::narratives_format_schema;
pub use brief::{CHARACTER, NARRATIVES_PROMPT_VERSION, NARRATIVES_SYSTEM_PROMPT};
pub use inputs::build_narratives_prompt;

/// Output schema version for the parsed narrative document, distinct from the prompt contract.
pub const NARRATIVES_OUTPUT_CONTRACT_VERSION: &str = "narratives-v3-schema";
pub const NARRATIVES_TEMPERATURE: f64 = 0.6;
pub const NARRATIVES_NUM_PREDICT: i32 = 1000;
pub const NARRATIVES_NUM_PREDICT_PACKET: i32 = 900;
const DESC_TRUNCATE: usize = 200;

/// Subject of a Journalist assignment. Durable identifiers and trigger policy stay outside Studio.
#[derive(Clone, Debug)]
pub struct Subject {
    pub entity_type: String,
    pub entity_name: String,
    pub sport: String,
}

/// One prepared article-sized evidence item. Model citations use its position in the assignment;
/// product provenance uses its durable article id.
#[derive(Clone, Debug)]
pub struct CorpusItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub source: String,
    pub published_at_epoch: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct CorpusExclusions {
    pub budget_truncated_ids: Vec<i64>,
}

/// One grounded storyline with deterministic impact and evidence provenance.
#[derive(Clone, Debug)]
pub struct Narrative {
    pub title: String,
    pub body: String,
    pub impact: i32,
    pub impact_components: serde_json::Value,
    pub input_news_ids: Vec<i64>,
    pub source_count: i32,
    pub source_names: Vec<String>,
    pub source_latest_epoch: Option<i64>,
    pub source_oldest_epoch: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ModelNarrative {
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    articles: Vec<i32>,
}

/// Salvaged and surface-validated model document.
#[derive(Clone, Debug, Default)]
pub struct ParsedNarratives {
    narratives: Vec<ModelNarrative>,
    card_score: Option<i16>,
    headline: Option<String>,
}

impl ParsedNarratives {
    pub fn returned(&self) -> impl Iterator<Item = (&str, &str, &[i32])> {
        self.narratives
            .iter()
            .map(|n| (n.title.as_str(), n.body.as_str(), n.articles.as_slice()))
    }

    pub fn card_score(&self) -> Option<i16> {
        self.card_score
    }

    pub fn headline(&self) -> Option<&str> {
        self.headline.as_deref()
    }
}

/// Tolerant parser: a complete empty array is a valid quiet edition; malformed output with no
/// salvageable story is an error so the application retries rather than publishing a marker.
pub struct NarrativesParser;

impl Parser<ParsedNarratives> for NarrativesParser {
    fn parse(&self, raw: &str) -> Result<Option<ParsedNarratives>> {
        let (mut narratives, ok) = parse_narratives(raw);
        if !ok {
            return Err(anyhow!(
                "parse narratives failed (raw={:?})",
                crate::util::truncate(raw, 200)
            ));
        }
        for narrative in &mut narratives {
            narrative.title = crate::studio::guards::clean_served_prose(&narrative.title);
            narrative.body = crate::studio::guards::clean_served_prose(&narrative.body);
        }
        if !narratives.is_empty() {
            let body = narratives
                .iter()
                .map(|n| n.body.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            crate::studio::form::validate_body(&body)?;
        }
        for narrative in &narratives {
            if let Some(product) = crate::studio::guards::first_product_name(&narrative.title)
                .or_else(|| crate::studio::guards::first_product_name(&narrative.body))
            {
                tracing::warn!(
                    guard = "product_name",
                    name = product,
                    "narratives edition rejected"
                );
                return Err(anyhow!("narratives: storyline names product {product:?}"));
            }
        }
        let card_score = parse_card_score(raw);
        crate::studio::form::validate_hook(parse_headline(raw).as_deref())?;
        let headline =
            crate::studio::guards::settle_title("journalist", parse_headline(raw).as_deref());
        Ok(Some(ParsedNarratives {
            narratives,
            card_score,
            headline,
        }))
    }
}

pub fn narratives_decode_budget(num_ctx: i32) -> (i32, i32) {
    if crate::studio::model::small_voice_window(num_ctx) {
        (num_ctx, NARRATIVES_NUM_PREDICT_PACKET)
    } else {
        (num_ctx, NARRATIVES_NUM_PREDICT)
    }
}

pub fn generation_options(temperature: f64, num_ctx: i32) -> GenerateOptions {
    let (num_ctx, num_predict) = narratives_decode_budget(num_ctx);
    GenerateOptions {
        system: Some(NARRATIVES_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict,
        num_ctx,
        json_mode: false,
        format_schema: Some(narratives_format_schema()),
        format_schema_raw: None,
    }
}

fn article_context(item: &CorpusItem) -> (&str, usize) {
    if description_adds_nothing(&item.description, &item.title, &item.source) {
        return ("", DESC_TRUNCATE);
    }
    (&item.description, DESC_TRUNCATE)
}

fn description_adds_nothing(description: &str, title: &str, source: &str) -> bool {
    let description = context_tokens(description);
    if description.is_empty() {
        return true;
    }
    let mut known: HashSet<String> = context_tokens(title).into_iter().collect();
    known.extend(context_tokens(source));
    description.iter().all(|token| known.contains(token))
}

fn context_tokens(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// Bound prepared evidence by its projected prompt cost while retaining at least one item.
pub(crate) fn apply_news_budget(
    corpus: Vec<CorpusItem>,
    budget: usize,
) -> (Vec<CorpusItem>, Vec<i64>) {
    let mut spent = 0usize;
    let mut kept = Vec::with_capacity(corpus.len());
    let mut dropped = Vec::new();
    for item in corpus {
        let (body, cap) = article_context(&item);
        let cost = 8 + item.source.len() + item.title.len() + body.len().min(cap);
        if spent + cost > budget && !kept.is_empty() {
            dropped.push(item.id);
            continue;
        }
        spent += cost;
        kept.push(item);
    }
    (kept, dropped)
}

fn render_signals_line(corpus: &[CorpusItem], now_epoch: i64) -> String {
    let sources: HashSet<&str> = corpus
        .iter()
        .filter(|item| !item.source.is_empty())
        .map(|item| item.source.as_str())
        .collect();
    let mut line = format!(
        "SIGNALS (deterministic tally for your card score): {} article(s) after dedup · {} distinct source(s)",
        corpus.len(),
        sources.len()
    );
    if let Some(freshest) = corpus
        .iter()
        .filter_map(|item| item.published_at_epoch)
        .max()
    {
        let age_hours = (now_epoch - freshest).max(0) / 3600;
        if age_hours < 48 {
            line.push_str(&format!(" · freshest {age_hours}h ago"));
        } else {
            line.push_str(&format!(" · freshest {}d ago", age_hours / 24));
        }
    }
    line
}

fn parse_narratives(raw: &str) -> (Vec<ModelNarrative>, bool) {
    let mut out = Vec::new();
    let Some(key) = raw.find("\"narratives\"") else {
        return (out, false);
    };
    let Some(left_bracket) = raw.as_bytes()[key..].iter().position(|&byte| byte == b'[') else {
        return (out, false);
    };
    let input = &raw.as_bytes()[key + left_bracket + 1..];
    let mut depth = 0_i32;
    let mut start = -1_i64;
    let mut in_string = false;
    let mut escaped = false;
    let mut index = 0usize;
    while index < input.len() {
        let byte = input[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => {
                if depth == 0 {
                    start = index as i64;
                }
                depth += 1;
            }
            b'}' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 && start >= 0 {
                        if let Ok(text) = std::str::from_utf8(&input[start as usize..=index]) {
                            if let Ok(narrative) = serde_json::from_str::<ModelNarrative>(text) {
                                out.push(narrative);
                            }
                        }
                        start = -1;
                    }
                }
            }
            b']' if depth == 0 => return (out, true),
            _ => {}
        }
        index += 1;
    }
    let ok = !out.is_empty();
    (out, ok)
}

fn parse_card_score(raw: &str) -> Option<i16> {
    let key = raw.find("\"card_score\"")?;
    let rest = &raw[key + "\"card_score\"".len()..];
    let colon = rest.find(':')?;
    let value = rest[colon + 1..].trim_start().trim_start_matches('"');
    let head: String = value
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
        .collect();
    let score = match head.parse::<i64>() {
        Ok(score) => score,
        Err(_) => head.parse::<f64>().ok().filter(|f| f.is_finite())?.round() as i64,
    };
    Some(score.clamp(1, 99) as i16)
}

fn parse_headline(raw: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw.trim()) {
        if let Some(headline) = value.get("headline").and_then(|headline| headline.as_str()) {
            let headline = headline.trim();
            if !headline.is_empty() {
                return Some(headline.to_string());
            }
        }
        return None;
    }
    let key = raw.find("\"headline\"")?;
    let rest = &raw[key + "\"headline\"".len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let mut chars = after.char_indices();
    let (_, quote) = chars.next()?;
    if quote != '"' {
        return None;
    }
    let mut out = String::new();
    let mut escaped = false;
    for (_, character) in chars {
        if escaped {
            match character {
                'n' | 't' => out.push(' '),
                other => out.push(other),
            }
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            let out = out.trim();
            return (!out.is_empty()).then(|| out.to_string());
        } else {
            out.push(character);
        }
    }
    None
}

fn ground_narratives(
    parsed: &[ModelNarrative],
    corpus: &[CorpusItem],
    now_epoch: i64,
) -> Vec<Narrative> {
    let mut out = Vec::with_capacity(parsed.len());
    for parsed_narrative in parsed {
        let title = parsed_narrative.title.trim();
        let body = parsed_narrative.body.trim();
        if title.is_empty() || body.is_empty() {
            continue;
        }
        let mut seen = HashSet::with_capacity(parsed_narrative.articles.len());
        let mut subset = Vec::new();
        let mut ids = Vec::new();
        for &number in &parsed_narrative.articles {
            if number < 1 {
                continue;
            }
            let index = (number - 1) as usize;
            if index >= corpus.len() || !seen.insert(index) {
                continue;
            }
            subset.push(corpus[index].clone());
            ids.push(corpus[index].id);
        }
        if subset.is_empty() {
            continue;
        }
        let (impact, impact_components) = compute_news_impact(&subset, now_epoch);
        let (source_count, source_names, source_latest_epoch, source_oldest_epoch) =
            source_metadata(&subset);
        out.push(Narrative {
            title: title.to_string(),
            body: body.to_string(),
            impact,
            impact_components,
            input_news_ids: ids,
            source_count,
            source_names,
            source_latest_epoch,
            source_oldest_epoch,
        });
    }
    out
}

fn compute_news_impact(corpus: &[CorpusItem], now_epoch: i64) -> (i32, serde_json::Value) {
    let count = corpus.len();
    let volume = 60.0_f64 * (1.0 - (-(count as f64) / 5.0).exp());
    let distinct_sources = corpus
        .iter()
        .filter(|item| !item.source.is_empty())
        .map(|item| item.source.to_lowercase())
        .collect::<HashSet<_>>()
        .len();
    let corroboration = 25.0_f64.min(distinct_sources as f64 * 6.0);
    let newest = corpus
        .iter()
        .filter_map(|item| item.published_at_epoch)
        .max();
    let recency = newest.map_or(0.0, |newest| {
        let age = now_epoch - newest;
        if age <= 12 * 3600 {
            15.0
        } else if age <= 24 * 3600 {
            10.0
        } else if age <= 48 * 3600 {
            5.0
        } else {
            0.0
        }
    });
    let score = (volume + corroboration + recency).round().clamp(0.0, 100.0) as i32;
    (
        score,
        json!({
            "article_count": count,
            "distinct_sources": distinct_sources,
            "volume": (volume * 10.0).round() / 10.0,
            "corroboration": (corroboration * 10.0).round() / 10.0,
            "recency": recency,
        }),
    )
}

fn source_metadata(corpus: &[CorpusItem]) -> (i32, Vec<String>, Option<i64>, Option<i64>) {
    let mut source_names = Vec::new();
    let mut seen_sources = HashSet::new();
    let mut latest = None;
    let mut oldest = None;
    for item in corpus {
        let source = item.source.trim();
        if !source.is_empty() && seen_sources.insert(source.to_lowercase()) {
            source_names.push(source.to_string());
        }
        if let Some(epoch) = item.published_at_epoch {
            latest = Some(latest.map_or(epoch, |current: i64| current.max(epoch)));
            oldest = Some(oldest.map_or(epoch, |current: i64| current.min(epoch)));
        }
    }
    (corpus.len() as i32, source_names, latest, oldest)
}

/// Stable per-article fingerprint retained in the debounce pre-image.
pub const READING_FINGERPRINT_NONE: &str = "none::0";

pub fn build_article_reading_input_components(items: &[(i64, String)]) -> String {
    let mut pairs = items.to_vec();
    pairs.sort_by_key(|(id, _)| *id);
    crate::util::hash_components(
        &serde_json::to_string(&pairs).expect("article fingerprint tuples serialize"),
    )
}

pub fn build_narratives_input_components(corpus: &[CorpusItem]) -> String {
    let mut ids: Vec<i64> = corpus.iter().map(|item| item.id).collect();
    ids.sort_unstable();
    let article_readings: Vec<(i64, String)> = corpus
        .iter()
        .map(|item| (item.id, READING_FINGERPRINT_NONE.to_string()))
        .collect();
    serde_json::json!({
        "article_ids": ids,
        "article_readings_hash": build_article_reading_input_components(&article_readings),
        "prompt_version": NARRATIVES_PROMPT_VERSION,
    })
    .to_string()
}

/// The application's complete, prepared creation contract.
#[derive(Clone, Debug)]
pub struct Assignment {
    pub subject: Subject,
    pub corpus: Vec<CorpusItem>,
    pub corpus_exclusions: CorpusExclusions,
    pub memory: Option<String>,
    pub packet_framing: Option<String>,
    pub input_hash: String,
    pub card_score_prev: Option<i16>,
    pub options: GenerateOptions,
}

/// The unpersisted result of one edition. An empty narrative set becomes one marker row.
#[derive(Clone, Debug)]
pub struct NarrativesProduct {
    pub narratives: Vec<Narrative>,
    pub budget_truncated_ids: Vec<i64>,
    pub card_score: Option<i16>,
    pub card_score_prev: Option<i16>,
    pub headline: Option<String>,
}

pub type NarrativesOutput = Generation<NarrativesProduct>;

pub async fn create(
    studio: &Studio<'_>,
    assignment: &Assignment,
    now_epoch: i64,
) -> Result<NarrativesOutput> {
    if assignment.corpus.is_empty() {
        return Ok(Generation::uncalled(
            NarrativesProduct {
                narratives: Vec::new(),
                budget_truncated_ids: assignment.corpus_exclusions.budget_truncated_ids.clone(),
                card_score: None,
                card_score_prev: None,
                headline: None,
            },
            studio.model.model().to_string(),
            NARRATIVES_PROMPT_VERSION,
            Vec::new(),
            Some(assignment.input_hash.clone()),
        ));
    }

    let score_context = render_signals_line(&assignment.corpus, now_epoch);
    let prompt = build_narratives_prompt(
        &assignment.subject,
        &assignment.corpus,
        assignment.memory.as_deref(),
        Some(&score_context),
        assignment.packet_framing.as_deref(),
    );
    let extracted = studio
        .extract(&prompt, &assignment.options, &NarrativesParser)
        .await?;
    let call = GenerationCall::from(&extracted);
    let model = extracted.model.clone();
    let parsed = extracted.value.ok_or_else(|| {
        anyhow!("narratives: parser returned None (NarrativesParser signals failure via Err)")
    })?;
    let narratives = ground_narratives(&parsed.narratives, &assignment.corpus, now_epoch);
    let mut seen = HashSet::new();
    let input_ids = narratives
        .iter()
        .flat_map(|narrative| narrative.input_news_ids.iter().copied())
        .filter(|id| seen.insert(*id))
        .collect();

    Ok(Generation::called(
        NarrativesProduct {
            narratives,
            budget_truncated_ids: assignment.corpus_exclusions.budget_truncated_ids.clone(),
            card_score: parsed.card_score,
            card_score_prev: assignment.card_score_prev,
            headline: parsed.headline,
        },
        model,
        NARRATIVES_PROMPT_VERSION,
        input_ids,
        Some(assignment.input_hash.clone()),
        call,
    ))
}

#[cfg(test)]
mod tests;
