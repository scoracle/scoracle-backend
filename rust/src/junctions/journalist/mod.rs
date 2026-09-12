//! The Journalist's `Stage::Narratives` queue handler:
//! - `load_packet_corpus` reads the entity's compiled packets from Postgres.
//! - `build_narratives_prompt` is deterministic.
//! - `parse_narratives` uses a tolerant balanced-brace salvager: a truncated tail drops its last
//!   incomplete object; an empty `{"narratives": []}` is a successful parse -> marker.
//! - `compute_news_impact` deterministically scores the model-selected evidence subset.
//!
//! `NarrativesHandler` is a live queue stage gated by `COGNITION_STAGES`. It is the News hub stage:
//! transfer heat and source freshness are folded here before Vibe and Sigil consume the result.

use crate::corpus::{dedupe_i64, lookup_entity_name};
use crate::harness::{EntityKey, Generation, GenerationCall, Harness, Parser};
use crate::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::story_parts::{mode_storyline, progress_generation, PartItem};
use crate::trajectory::DEFAULT_TRAJECTORY;
use crate::work::{Item, Stage};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use sqlx::{PgPool, Row};
use std::collections::HashSet;
use tracing::{debug, warn};

mod inputs;
pub mod prompt;
pub use crate::junctions::form::narratives_format_schema;
pub use inputs::build_narratives_prompt;
pub use prompt::{NARRATIVES_PROMPT_VERSION, NARRATIVES_SYSTEM_PROMPT};

// ---------------------------------------------------------------------------
// Constants — mirror news_narratives.go.
// ---------------------------------------------------------------------------

/// Output schema version for the parsed narrative document, distinct from the prompt contract.
pub const NARRATIVES_OUTPUT_CONTRACT_VERSION: &str = "narratives-v3-schema";

const NARRATIVES_LEDGER: LedgerSpec = LedgerSpec {
    stage: "narratives",
    lens: "narratives",
    role: Role::NarrativeLogic,
    product_table: "news_summaries",
    output_contract_version: NARRATIVES_OUTPUT_CONTRACT_VERSION,
};

/// Production decode temperature. The deterministic fixture gate pins zero.
pub const NARRATIVES_TEMPERATURE: f64 = 0.6;

/// Output reservation for a large context window. The Journalist may file several storylines,
/// so this is larger than single-read seats.
pub const NARRATIVES_NUM_PREDICT: i32 = 1000;

/// Output reservation for the small context window, including unconstrained JSON overhead.
pub const NARRATIVES_NUM_PREDICT_PACKET: i32 = 900;

/// Pair the context window with an output reservation that leaves room for the prompt.
pub fn narratives_decode_budget(num_ctx: i32) -> (i32, i32) {
    if crate::route::small_voice_window(num_ctx) {
        (num_ctx, NARRATIVES_NUM_PREDICT_PACKET)
    } else {
        (num_ctx, NARRATIVES_NUM_PREDICT)
    }
}

/// Per-article description cap rendered into the prompt.
const DESC_TRUNCATE: usize = 200;

/// Maximum articles in a large-window prompt. The environment can tune the evidence/reply trade.
const DEFAULT_CORPUS_LIMIT: i64 = 40;

/// Article cap inside the small window. Excluded article IDs remain explicit in provenance.
const SMALL_WINDOW_CORPUS_LIMIT: i64 = 8;

fn corpus_limit(num_ctx: i32) -> i64 {
    std::env::var("COGNITION_JOURNALIST_CORPUS_LIMIT")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(if crate::route::small_voice_window(num_ctx) {
            SMALL_WINDOW_CORPUS_LIMIT
        } else {
            DEFAULT_CORPUS_LIMIT
        })
}

/// Vetted-news lookback in seconds. A fresh
/// Editor card also keeps an article in the corpus, so richer newly-enqueued evidence can
/// wake The Journalist even when the source article's `published_at` has aged past this boundary.
const NEWS_LOOKBACK_SECS: f64 = 259_200.0;

// ---------------------------------------------------------------------------
// Types.
// ---------------------------------------------------------------------------

/// Entity whose recent news should be narrated.
#[derive(Clone, Debug)]
pub struct NarrativesReq {
    pub entity_type: String, // "player" | "team"
    pub entity_id: i32,
    pub entity_name: String,
    /// The original-case sport the PROMPT renders (`req.Sport`); the SQL reads upper-case it.
    pub sport: String,
    pub trigger_type: String,
}

/// CorpusItem is one member article of the entity's packet corpus: `title` is the article's
/// headline claim, `description` the rest of its claims, joined. The prompt uses
/// title/description/source; `published_at_epoch` (Unix seconds, NULL when the article has no
/// publish time) feeds the deterministic recency in `compute_news_impact`.
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
    stale_news_ids: Vec<i64>,
    /// Articles inside the lookback window that lost the `COGNITION_JOURNALIST_CORPUS_LIMIT` cut on
    /// `feed_rank`. Restored with the cap in A5 — an excluded article MUST be named somewhere, or
    /// the ledger's evidence accounting silently stops adding up.
    budget_truncated_ids: Vec<i64>,
}

/// Narrative is one grounded storyline — title + body from the model, plus the DETERMINISTIC impact
/// (computed from its cited articles, never the model) and the article ids it cited. Mirrors `Narrative`.
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

/// One model-returned storyline. Defaults make missing optional content tolerant; invalid article
/// arrays make the object unparseable or ungroundable.
#[derive(Clone, Debug, Default, Deserialize)]
struct ModelNarrative {
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    articles: Vec<i32>,
}

/// Salvaged document returned by [`NarrativesParser`].
#[derive(Clone, Debug, Default)]
pub struct ParsedNarratives {
    narratives: Vec<ModelNarrative>,
    /// Best-effort 1-99 busyness verdict; absence persists as NULL.
    card_score: Option<i16>,
    /// Optional entity-level card title settled through the shared title guard.
    headline: Option<String>,
}

impl ParsedNarratives {
    /// Raw `(title, body, cited article numbers)` view used by offline eval before DB grounding.
    pub fn returned(&self) -> impl Iterator<Item = (&str, &str, &[i32])> {
        self.narratives
            .iter()
            .map(|n| (n.title.as_str(), n.body.as_str(), n.articles.as_slice()))
    }

    /// Busyness verdict used by eval and generation.
    pub fn card_score(&self) -> Option<i16> {
        self.card_score
    }

    /// The entity-level card hook, post-guard — for the eval's headline axes and the
    /// generation pipeline.
    pub fn headline(&self) -> Option<&str> {
        self.headline.as_deref()
    }
}

/// NarrativesParser runs the tolerant salvager. It returns `Ok(Some(parsed))` for a PARSEABLE
/// document (even an empty array — a legitimate "no storyline this cycle" → marker downstream) and
/// `Err` for a malformed reply with nothing salvageable, which the queue retries. It never returns
/// `Ok(None)`: narratives has
/// no post-model fail-closed marker carried by the parser — the marker decision is made AFTER
/// grounding when zero narratives remain.
pub struct NarrativesParser;

impl Parser<ParsedNarratives> for NarrativesParser {
    fn parse(&self, raw: &str) -> Result<Option<ParsedNarratives>> {
        let (mut narratives, ok) = parse_narratives(raw);
        if !ok {
            // A generation failure must never masquerade as a no-data marker.
            return Err(anyhow!(
                "parse narratives failed (raw={:?})",
                crate::util::truncate(raw, 200)
            ));
        }
        // Every served title and body passes through the shared scrub.
        for n in narratives.iter_mut() {
            n.title = crate::guards::clean_served_prose(&n.title);
            n.body = crate::guards::clean_served_prose(&n.body);
        }
        // Scan only served fields; discarded preamble cannot fail a clean edition.
        for n in &narratives {
            if let Some(p) = crate::guards::first_product_name(&n.title)
                .or_else(|| crate::guards::first_product_name(&n.body))
            {
                tracing::warn!(
                    guard = "product_name",
                    name = p,
                    "narratives edition rejected"
                );
                return Err(anyhow!("narratives: storyline names product {p:?}"));
            }
        }
        // Score and title are best-effort; missing fields never discard grounded prose.
        let card_score = parse_card_score(raw);
        // The entity-level hook is best-effort the same way, then settled through the shared
        // title floor: the tweet contract (140 chars), emphasis stripped, foreign-script and
        // overlong titles dropped rather than failing the edition.
        let headline = crate::guards::settle_title("journalist", parse_headline(raw).as_deref());
        Ok(Some(ParsedNarratives {
            narratives,
            card_score,
            headline,
        }))
    }
}

// ---------------------------------------------------------------------------
// Packet corpus loader.
// ---------------------------------------------------------------------------

/// Storyline corpus lookback.
pub const PACKET_LOOKBACK_HOURS: i64 = 72;
/// Packets read per entity per run. An entity in more than this many live storylines at once is
/// having an extraordinary week; the newest-compiled win and the rest are named as exclusions.
pub const MAX_PACKETS_PER_ENTITY: usize = 5;

/// Load compiled storylines as a `Vec<CorpusItem>` — one item per
/// MEMBER ARTICLE, carrying that article's claims as its text — plus the same `CorpusExclusions`.
/// Everything downstream (the debounce hash, the SIGNALS line, citation grounding, impact scoring,
/// the marker path) therefore works on the packet rail with no change at all, and the model still
/// cites real `news_articles.id`s it can be grounded against. What changes is the material: read
/// FACTS, attributed and contested-marked, in place of headlines and body excerpts.
///
/// The storyline framing (the story, this entity's part in it, what the prior packet said) rides
/// separately, as the returned string — `build_narratives_prompt` renders it above the numbered
/// evidence.
pub async fn load_packet_corpus(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    entity_name: &str,
) -> Result<(Vec<CorpusItem>, CorpusExclusions, String)> {
    use crate::junctions::editor::render;

    let loaded = crate::junctions::editor::packet::load_packets_for_entity(
        pool,
        entity_type,
        entity_id,
        sport,
        PACKET_LOOKBACK_HOURS,
        // One over the cap, so "there were more" is a fact this function KNOWS rather than
        // infers — the extra packet is never rendered, only counted and named.
        MAX_PACKETS_PER_ENTITY as i64 + 1,
    )
    .await?;

    let mut exclusions = CorpusExclusions::default();
    let mut framing = String::new();
    // Article id → its claims, in the order the packets presented them (newest packet first,
    // newest claim first). One entity can appear in several storylines and one article can be a
    // member of only one, so collisions here are rare — but the map keeps the item list unique by
    // article id either way, which is what grounding requires.
    let mut by_article: Vec<(i64, PacketArticle)> = Vec::new();

    for (i, (view, mut part)) in loaded.into_iter().enumerate() {
        if i >= MAX_PACKETS_PER_ENTITY {
            // A5: the packet we did not read is NAMED. Its members are the evidence being
            // excluded, and the exclusions band is where an ungrounded story gets explained.
            exclusions
                .budget_truncated_ids
                .extend(view.claims.iter().map(|c| c.article_id));
            continue;
        }
        // The loader leaves the name blank — it knows the id, the caller knows the name.
        part.name = entity_name.to_string();

        if !framing.is_empty() {
            framing.push('\n');
        }
        framing.push_str(&render::framing(
            &view,
            Some(&part),
            render::Voice::Journalist,
        ));

        for marked in render::mark_contested(&view.claims) {
            let fact = if marked.marked {
                // The contradiction survives INTO the prompt, marked, so the Journalist can write
                // "reports differ" instead of picking a side by accident (T3/D6).
                format!("⇄ {}", marked.claim.fact)
            } else {
                marked.claim.fact.clone()
            };
            match by_article
                .iter_mut()
                .find(|(id, _)| *id == marked.claim.article_id)
            {
                Some((_, art)) => art.facts.push(fact),
                None => by_article.push((
                    marked.claim.article_id,
                    PacketArticle {
                        source: marked.claim.source.clone(),
                        published_at_epoch: marked.claim.published_at,
                        facts: vec![fact],
                    },
                )),
            }
        }
    }

    let corpus: Vec<CorpusItem> = by_article
        .into_iter()
        .map(|(id, art)| {
            // The first fact is the headline and the rest form the body.
            let mut facts = art.facts.into_iter();
            let title = facts.next().unwrap_or_default();
            // The packet rail carries NO bodies. That is the diet: the Editor already read the
            // article, and re-sending its prose is the redundancy this whole rail removes.
            CorpusItem {
                id,
                title,
                description: facts.collect::<Vec<_>>().join(" · "),
                source: art.source,
                published_at_epoch: art.published_at_epoch,
            }
        })
        .collect();

    // A storyline can contain many articles, so cap rendered bytes as well as storyline count.
    // Newest evidence wins and all dropped article IDs remain explicit.
    let (corpus, over_budget) = apply_news_budget(corpus, PACKET_NEWS_BUDGET_CHARS);
    exclusions.budget_truncated_ids.extend(over_budget);

    exclusions.budget_truncated_ids.sort_unstable();
    exclusions.budget_truncated_ids.dedup();
    Ok((corpus, exclusions, framing))
}

/// Rendered-size allowance for numbered evidence inside the small context window.
const PACKET_NEWS_BUDGET_CHARS: usize = 5_000;

/// apply_news_budget keeps the corpus prefix whose PROJECTED render cost (the same title +
/// capped-context arithmetic `build_narratives_prompt` spends) fits `budget`, returning the
/// dropped items' ids for the exclusions band. Order is preserved — the caller already sorts
/// newest-first, so the cut is the oldest evidence.
fn apply_news_budget(corpus: Vec<CorpusItem>, budget: usize) -> (Vec<CorpusItem>, Vec<i64>) {
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

/// One member article's claims, while `load_packet_corpus` groups them.
struct PacketArticle {
    source: String,
    published_at_epoch: Option<i64>,
    facts: Vec<String>,
}

// ---------------------------------------------------------------------------
// Prompt inputs.
// ---------------------------------------------------------------------------

/// article_context is the model-visible text for one corpus item, rendered AFTER its headline
/// (the caller always writes `[source] title` first). On the packet corpus that text is the
/// article's remaining claims, and it is used only when it actually says something the headline
/// did not.
/// Returning empty context is valid when the headline already contains every description token.
fn article_context(c: &CorpusItem) -> (&str, usize) {
    if description_adds_nothing(&c.description, &c.title, &c.source) {
        return ("", DESC_TRUNCATE);
    }
    (&c.description, DESC_TRUNCATE)
}

/// description_adds_nothing reports whether the RSS description is just the headline (plus the
/// outlet) restated. Token containment rather than string equality, because Google glues the source
/// on and punctuation drifts between the two fields. Conservative by construction: a description
/// carrying even one word of genuine new content is kept.
fn description_adds_nothing(description: &str, title: &str, source: &str) -> bool {
    let desc: Vec<String> = context_tokens(description);
    if desc.is_empty() {
        return true;
    }
    let mut known: HashSet<String> = context_tokens(title).into_iter().collect();
    known.extend(context_tokens(source));
    desc.iter().all(|t| known.contains(t))
}

fn context_tokens(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Fetch the graph's per-entity memory card. `None` means no prompt section.
/// Model-facing enrichment only — the relational layer is never user-exposed.
pub async fn load_entity_memory(
    pool: &sqlx::PgPool,
    sport: &str,
    entity_type: &str,
    entity_id: i32,
) -> Result<Option<String>> {
    let row: (Option<String>,) = sqlx::query_as("SELECT narrative_context_for_entity($1, $2, $3)")
        .bind(sport)
        .bind(entity_type)
        .bind(entity_id)
        .fetch_one(pool)
        .await
        .context("narrative_context_for_entity")?;
    Ok(row.0)
}

/// Number of recent Journalist card reads used as continuity memory.
const PRIOR_CARD_READS_LIMIT: i64 = 4;

/// Impact-ranked storylines from the previous filing carried as memory.
const PRIOR_STORY_BODY_LIMIT: i64 = 3;
/// Prompt bytes each remembered storyline body may spend (the influencer BODY_TRUNCATE
/// precedent): three truncated bodies ≈ 200 tokens against the 4096 window.
const PRIOR_STORY_BODY_TRUNCATE: usize = 280;

/// The Journalist's own score memory: the latest non-NULL `card_score` (persisted as
/// `card_score_prev` on the new generation — the continuity audit) plus the rendered
/// prompt block.
pub struct PriorCardReads {
    pub latest: i16,
    pub card: String,
}

/// Render the Journalist's own recent filings as prompt-only continuity memory.
/// One generation carries one uniform card_score, so the trail is DISTINCT over `generated_at`.
/// The previous generation's filed shape (storyline count, max impact) rides along: impact is
/// computed post-parse, so it can only ground the NEXT call — this one. `None` for a first-ever
/// scored read. Prompt-only, deliberately NOT part of the input_hash.
pub async fn load_prior_card_reads(
    pool: &sqlx::PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Option<PriorCardReads>> {
    let trail: Vec<(i16, Option<String>, String)> = sqlx::query_as(
        r#"
        SELECT card_score, headline, to_char(generated_at, 'Mon DD')
        FROM (
            SELECT DISTINCT generated_at, card_score, headline
            FROM news_summaries
            WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
              AND card_score IS NOT NULL
        ) g
        ORDER BY generated_at DESC
        LIMIT $4
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(PRIOR_CARD_READS_LIMIT)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load prior card reads {entity_type}/{entity_id}"))?;
    if trail.is_empty() {
        return Ok(None);
    }
    // The latest generation's filed shape — markers count as an honest zero.
    let (storylines, max_impact): (i64, Option<i16>) = sqlx::query_as(
        r#"
        SELECT count(*) FILTER (WHERE body IS NOT NULL), max(impact)
        FROM news_summaries
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
          AND generated_at = (
              SELECT max(generated_at) FROM news_summaries
              WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
          )
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .fetch_one(pool)
    .await
    .with_context(|| format!("load prior generation shape {entity_type}/{entity_id}"))?;

    // Load the last content generation; a marker must not erase the previous filing's memory.
    let prior_stories: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT narrative_title, body
        FROM news_summaries
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
          AND body IS NOT NULL
          AND generated_at = (
              SELECT max(generated_at) FROM news_summaries
              WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
                AND body IS NOT NULL
          )
        ORDER BY impact DESC NULLS LAST
        LIMIT $4
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(PRIOR_STORY_BODY_LIMIT)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load prior storylines {entity_type}/{entity_id}"))?;

    let mut card = String::from(
        "YOUR PRIOR CARD READS (memory — your own previous filings; continuity, not new evidence):\n",
    );
    // Keep the dated headline trail and only the latest score as the numeric anchor.
    for (_, headline, day) in &trail {
        if let Some(h) = headline.as_deref().filter(|h| !h.trim().is_empty()) {
            card.push_str(&format!("Your front page ({day}): {h}\n"));
        }
    }
    card.push_str(&format!("Your latest card score: {}\n", trail[0].0));
    // Previous bodies are truncated reference, explicitly framed as memory rather than evidence.
    for (title, body) in &prior_stories {
        card.push_str(&format!(
            "You previously filed \"{title}\": {}\n",
            crate::util::truncate_bytes(body, PRIOR_STORY_BODY_TRUNCATE)
        ));
    }
    match max_impact {
        Some(m) => card.push_str(&format!(
            "Your previous filing: {storylines} storyline(s), max impact {m}"
        )),
        None => card.push_str(&format!("Your previous filing: {storylines} storyline(s)")),
    }
    Ok(Some(PriorCardReads {
        latest: trail[0].0,
        card,
    }))
}

/// render_signals_line writes the deterministic tally that grounds the card score: post-dedup
/// article count, distinct sources, freshest-article age. Zero new queries — everything comes
/// from the already-loaded corpus (the plan's "already in the corpus vec" guarantee).
fn render_signals_line(corpus: &[CorpusItem], now_epoch: i64) -> String {
    let sources: HashSet<&str> = corpus
        .iter()
        .filter(|c| !c.source.is_empty())
        .map(|c| c.source.as_str())
        .collect();
    let mut line = format!(
        "SIGNALS (deterministic tally for your card score): {} article(s) after dedup · {} distinct source(s)",
        corpus.len(),
        sources.len()
    );
    let freshest = corpus.iter().filter_map(|c| c.published_at_epoch).max();
    if let Some(f) = freshest {
        let age_h = (now_epoch - f).max(0) / 3600;
        if age_h < 48 {
            line.push_str(&format!(" · freshest {age_h}h ago"));
        } else {
            line.push_str(&format!(" · freshest {}d ago", age_h / 24));
        }
    }
    line
}

// ---------------------------------------------------------------------------
// Parse — mirrors parseNarratives (the tolerant balanced-brace salvager).
// ---------------------------------------------------------------------------

/// Salvage each complete narrative object from the model's response independently,
/// rather than requiring the whole document to be well-formed — LLM length is non-deterministic, so a
/// reply can truncate mid-array or carry one malformed object. It scans every balanced top-level
/// `{...}` inside the `"narratives"` array, respecting strings and escapes.
///
/// The bool reports whether the response was PARSEABLE as a narratives document, NOT whether it
/// carried narratives: a cleanly-closed array — including an empty `{"narratives": []}` — is a
/// successful parse with zero narratives (a legitimate no-data outcome → marker), distinct from a
/// malformed/truncated reply (no `"narratives"` key, no `[`, or EOF before the array closed AND
/// nothing salvaged) which is a failure so the work queue retries it.
fn parse_narratives(raw: &str) -> (Vec<ModelNarrative>, bool) {
    let mut out: Vec<ModelNarrative> = Vec::new();
    let Some(key) = raw.find("\"narratives\"") else {
        return (out, false);
    };
    // index of the array '[' relative to `key`, then `s` is the byte slice just AFTER it.
    let Some(lb) = raw.as_bytes()[key..].iter().position(|&b| b == b'[') else {
        return (out, false);
    };
    let s = &raw.as_bytes()[key + lb + 1..];

    let mut depth: i32 = 0;
    let mut start: i64 = -1;
    let mut in_str = false;
    let mut esc = false;
    let mut i = 0usize;
    while i < s.len() {
        let c = s[i];
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' => {
                if depth == 0 {
                    start = i as i64;
                }
                depth += 1;
            }
            b'}' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 && start >= 0 {
                        // Braces are ASCII, so the slice lands on UTF-8 boundaries.
                        if let Ok(txt) = std::str::from_utf8(&s[start as usize..=i]) {
                            if let Ok(n) = serde_json::from_str::<ModelNarrative>(txt) {
                                out.push(n);
                            }
                        }
                        start = -1;
                    }
                }
            }
            b']' if depth == 0 => {
                // Array closed cleanly — a parseable document even when empty (→ marker).
                return (out, true);
            }
            _ => {}
        }
        i += 1;
    }
    // EOF before the array closed: a truncated generation. Succeed only if we salvaged at least one
    // complete narrative from the tail; otherwise it is a real failure (retry), never a no-data marker.
    let ok = !out.is_empty();
    (out, ok)
}

/// Salvage `card_score`, accepting a quoted or fractional leading number and clamping 1-99.
/// Absence or non-numeric input returns `None` rather than failing the edition.
fn parse_card_score(raw: &str) -> Option<i16> {
    let key = raw.find("\"card_score\"")?;
    let rest = &raw[key + "\"card_score\"".len()..];
    let colon = rest.find(':')?;
    let val = rest[colon + 1..].trim_start().trim_start_matches('"');
    let head: String = val
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
        .collect();
    let n = match head.parse::<i64>() {
        Ok(n) => n,
        Err(_) => head.parse::<f64>().ok().filter(|f| f.is_finite())?.round() as i64,
    };
    Some(n.clamp(1, 99) as i16)
}

/// Salvage the entity-level `headline` string with the same
/// tolerance as [`parse_card_score`]: a clean whole-document parse first, then a raw key scan
/// for truncated/prose-wrapped tails. `None` for an absent key or a non-string value — never a
/// parse failure (pre-headline replays simply persist NULL and the card renders without a hook).
/// The guard pass ([`crate::guards::settle_title`]) runs at the call site, not here.
fn parse_headline(raw: &str) -> Option<String> {
    // Whole-document first: the live path is schema-constrained, so this is the common case.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw.trim()) {
        if let Some(h) = v.get("headline").and_then(|h| h.as_str()) {
            let h = h.trim();
            if !h.is_empty() {
                return Some(h.to_string());
            }
        }
        return None;
    }
    // Raw scan fallback: find the key, then parse the JSON string that follows the colon.
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
    for (_, c) in chars {
        if escaped {
            match c {
                'n' | 't' => out.push(' '),
                other => out.push(other),
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            let out = out.trim();
            return (!out.is_empty()).then(|| out.to_string());
        } else {
            out.push(c);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Grounding — map article numbers back to the corpus, compute the deterministic per-narrative impact.
// ---------------------------------------------------------------------------

/// ground_narratives maps the model's 1-indexed article numbers back to the corpus, computes the
/// per-narrative impact from ITS articles (never the model), and keeps only narratives with a title,
/// a body, and at least one valid article. `now_epoch` is the recency reference captured once per
/// generation.
fn ground_narratives(
    parsed: &[ModelNarrative],
    news: &[CorpusItem],
    now_epoch: i64,
) -> Vec<Narrative> {
    let mut out: Vec<Narrative> = Vec::with_capacity(parsed.len());
    for p in parsed {
        let title = p.title.trim();
        let body = p.body.trim();
        if title.is_empty() || body.is_empty() {
            continue;
        }
        // Article numbers are 1-indexed in the prompt; dedupe + bound to range.
        let mut seen: HashSet<usize> = HashSet::with_capacity(p.articles.len());
        let mut subset: Vec<CorpusItem> = Vec::new();
        let mut ids: Vec<i64> = Vec::new();
        for &num in &p.articles {
            if num < 1 {
                continue; // idx < 0
            }
            let idx = (num - 1) as usize;
            if idx >= news.len() {
                continue;
            }
            if !seen.insert(idx) {
                continue; // dup
            }
            subset.push(news[idx].clone());
            ids.push(news[idx].id);
        }
        if subset.is_empty() {
            continue; // ungrounded — can't score or trace it
        }
        let (impact, components) = compute_news_impact(&subset, now_epoch);
        let (source_count, source_names, source_latest_epoch, source_oldest_epoch) =
            source_metadata(&subset);
        out.push(Narrative {
            title: title.to_string(),
            body: body.to_string(),
            impact,
            impact_components: components,
            input_news_ids: ids,
            source_count,
            source_names,
            source_latest_epoch,
            source_oldest_epoch,
        });
    }
    out
}

/// Compute deterministic 0-100 impact from a saturating volume curve, distinct-source
/// corroboration, and a freshness bucket. Returns the score and transparent components.
fn compute_news_impact(news: &[CorpusItem], now_epoch: i64) -> (i32, serde_json::Value) {
    let n = news.len();
    // Volume: saturating curve — a handful of articles is already hot, returns diminish.
    let volume = 60.0_f64 * (1.0 - (-(n as f64) / 5.0).exp());

    // Corroboration: distinct (lower-cased) sources, capped at 25.
    let mut sources: HashSet<String> = HashSet::new();
    for a in news {
        if !a.source.is_empty() {
            sources.insert(a.source.to_lowercase());
        }
    }
    let distinct = sources.len();
    let corroboration = 25.0_f64.min(distinct as f64 * 6.0);

    // Recency: how fresh is the freshest article.
    let mut recency = 0.0_f64;
    let mut newest: Option<i64> = None;
    for a in news {
        if let Some(pa) = a.published_at_epoch {
            if newest.is_none_or(|cur| pa > cur) {
                newest = Some(pa);
            }
        }
    }
    if let Some(newest) = newest {
        let age = now_epoch - newest; // seconds
        if age <= 12 * 3600 {
            recency = 15.0;
        } else if age <= 24 * 3600 {
            recency = 10.0;
        } else if age <= 48 * 3600 {
            recency = 5.0;
        }
    }

    let mut score = (volume + corroboration + recency).round() as i64;
    score = score.clamp(0, 100);
    let components = json!({
        "article_count": n,
        "distinct_sources": distinct,
        "volume": (volume * 10.0).round() / 10.0,
        "corroboration": (corroboration * 10.0).round() / 10.0,
        "recency": recency,
    });
    (score as i32, components)
}

fn source_epoch(item: &CorpusItem) -> Option<i64> {
    item.published_at_epoch
}

fn source_metadata(news: &[CorpusItem]) -> (i32, Vec<String>, Option<i64>, Option<i64>) {
    let mut source_names: Vec<String> = Vec::new();
    let mut seen_sources: HashSet<String> = HashSet::new();
    let mut latest: Option<i64> = None;
    let mut oldest: Option<i64> = None;

    for item in news {
        let source = item.source.trim();
        if !source.is_empty() && seen_sources.insert(source.to_lowercase()) {
            source_names.push(source.to_string());
        }
        if let Some(epoch) = source_epoch(item) {
            if latest.is_none_or(|cur| epoch > cur) {
                latest = Some(epoch);
            }
            if oldest.is_none_or(|cur| epoch < cur) {
                oldest = Some(epoch);
            }
        }
    }

    (news.len() as i32, source_names, latest, oldest)
}

// ---------------------------------------------------------------------------
// The composition: build (deterministic) → generate (model) → ground → persist.
// ---------------------------------------------------------------------------

/// NarrativesBuild is the deterministic prefix of a generation. `NoCorpus` ⇒ no vetted news this
/// cycle → a NULL-narrative marker with no model call.
pub enum NarrativesBuild {
    NoCorpus {
        corpus_exclusions: CorpusExclusions,
        /// Hash over the (empty) material inputs — so a quiet entity's marker also debounces
        /// instead of re-marking every cycle.
        input_hash: String,
    },
    Ready(Box<NarrativesReady>),
}

/// Stable per-article fingerprint retained in the debounce pre-image. Changing or removing this
/// constant forces a full-fleet regeneration and must ride a deliberate prompt-version bump.
pub const READING_FINGERPRINT_NONE: &str = "none::0";

/// Hash canonical article/fingerprint pairs in article-ID order.
pub fn build_article_reading_input_components(items: &[(i64, String)]) -> String {
    let mut pairs = items.to_vec();
    pairs.sort_by_key(|(id, _)| *id);
    crate::util::hash_components(
        &serde_json::to_string(&pairs).expect("article fingerprint tuples serialize"),
    )
}

/// Canonical debounce pre-image from prompt version and sorted corpus article IDs. Including the
/// version makes a contract bump regenerate each entity exactly once.
pub fn build_narratives_input_components(corpus: &[CorpusItem]) -> String {
    let mut ids: Vec<i64> = corpus.iter().map(|c| c.id).collect();
    ids.sort_unstable();
    let article_readings: Vec<(i64, String)> = corpus
        .iter()
        .map(|c| (c.id, READING_FINGERPRINT_NONE.to_string()))
        .collect();
    serde_json::json!({
        "article_ids": ids,
        "article_readings_hash": build_article_reading_input_components(&article_readings),
        "prompt_version": NARRATIVES_PROMPT_VERSION,
    })
    .to_string()
}

/// Assembled model inputs plus the canonical corpus used for grounding. `request_body` comes from
/// the same backend and options as the actual call.
pub struct NarrativesReady {
    /// The numbered corpus the model sees (widened, canonical-only — the loader already excludes
    /// `duplicate_of` reposts the scrub novelty gate suppressed).
    pub corpus: Vec<CorpusItem>,
    pub corpus_exclusions: CorpusExclusions,
    pub opts: GenerateOptions,
    pub built_prompt: String,
    pub request_body: serde_json::Value,
    pub model_configured: String,
    /// SHA over [`build_narratives_input_components`] — the handler's debounce key.
    pub input_hash: String,
    /// Latest prior score, used as prompt-only continuity and persisted for audit.
    pub card_score_prev: Option<i16>,
}

/// Loaded material and its debounce hash. The live handler gates before prompt assembly.
pub struct NarrativesMaterial {
    pub corpus: Vec<CorpusItem>,
    pub corpus_exclusions: CorpusExclusions,
    /// SHA over [`build_narratives_input_components`] — the debounce key.
    pub input_hash: String,
    /// Optional storyline framing block.
    pub packet_framing: Option<String>,
}

/// load_narratives_material runs the loads and hashes the material inputs. No embed, no prompt.
pub async fn load_narratives_material(
    hx: &Harness,
    req: &NarrativesReq,
) -> Result<NarrativesMaterial> {
    let sport_up = req.sport.to_uppercase();

    // The Insider owns transfer truth; the Journalist sees transfer stories through the corpus.
    let (corpus, corpus_exclusions, packet_framing) = {
        let (c, e, f) = load_packet_corpus(
            &hx.pool,
            &req.entity_type,
            req.entity_id,
            &sport_up,
            &req.entity_name,
        )
        .await?;
        (c, e, Some(f))
    };

    // The prompt version makes a contract change invalidate each material hash once.
    let input_hash = crate::util::hash_components(&build_narratives_input_components(&corpus));

    Ok(NarrativesMaterial {
        corpus,
        corpus_exclusions,
        input_hash,
        packet_framing,
    })
}

/// Post-gate memory loads and prompt/options/wire-body assembly.
pub async fn finish_narratives_build(
    hx: &Harness,
    req: &NarrativesReq,
    material: NarrativesMaterial,
    temperature: f64,
) -> Result<NarrativesBuild> {
    let sport_up = req.sport.to_uppercase();
    let NarrativesMaterial {
        corpus,
        corpus_exclusions,
        input_hash,
        packet_framing,
    } = material;

    // No corpus → the NULL-narrative marker path (no model call).
    if corpus.is_empty() {
        return Ok(NarrativesBuild::NoCorpus {
            corpus_exclusions,
            input_hash,
        });
    }

    // Memory-load failure degrades to an unenriched prompt, mirroring the heat
    // error-swallowing above: the corpus is the primary signal, memory is enrichment.
    let memory =
        match load_entity_memory(&hx.pool, &sport_up, &req.entity_type, req.entity_id).await {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    entity_type = %req.entity_type,
                    entity_id = req.entity_id,
                    sport = %sport_up,
                    error = %e,
                    "narratives: relational memory load failed (continuing without memory)"
                );
                None
            }
        };
    // Signals and prior reads are prompt-only enrichment; failures degrade without blocking.
    let prior_reads =
        match load_prior_card_reads(&hx.pool, &req.entity_type, req.entity_id, &sport_up).await {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    entity_type = %req.entity_type,
                    entity_id = req.entity_id,
                    sport = %sport_up,
                    error = %e,
                    "narratives: prior card reads load failed (continuing without)"
                );
                None
            }
        };
    let card_score_prev = prior_reads.as_ref().map(|p| p.latest);
    let mut score_context = render_signals_line(&corpus, now_unix());
    if let Some(p) = &prior_reads {
        score_context.push('\n');
        score_context.push_str(&p.card);
    }
    // Identity card: house records, dated — degrades to absent like memory.
    let identity =
        crate::corpus::load_identity_card(&hx.pool, &req.entity_type, req.entity_id, &req.sport)
            .await
            .unwrap_or_default();
    let built_prompt = build_narratives_prompt(
        req,
        &corpus,
        memory.as_deref(),
        Some(&score_context),
        packet_framing.as_deref(),
        identity.as_deref(),
    );
    let (num_ctx, num_predict) = narratives_decode_budget(hx.voice_num_ctx);
    let opts = GenerateOptions {
        system: Some(NARRATIVES_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict,
        num_ctx,
        json_mode: false,
        // Grammar constrains the live path; the salvager remains for tolerant offline parsing.
        format_schema: Some(narratives_format_schema()),
        format_schema_raw: None,
    };
    let backend = hx.router.for_role(Role::NarrativeLogic);
    let request_body = backend.request_body(&built_prompt, &opts);
    let model_configured = backend.model().to_string();

    Ok(NarrativesBuild::Ready(Box::new(NarrativesReady {
        corpus,
        corpus_exclusions,
        opts,
        built_prompt,
        request_body,
        model_configured,
        input_hash,
        card_score_prev,
    })))
}

/// Build the complete deterministic request without a model call. The live handler invokes the
/// two phases separately so it can debounce between material loading and prompt assembly.
pub async fn build_narratives_request(
    hx: &Harness,
    req: &NarrativesReq,
    temperature: f64,
) -> Result<NarrativesBuild> {
    let material = load_narratives_material(hx, req).await?;
    finish_narratives_build(hx, req, material, temperature).await
}

/// The un-persisted result of one generation. `narratives` empty means a marker row
/// (no corpus, or a real generation that yielded no usable grounded storyline).
#[derive(Clone, Debug)]
pub struct NarrativesProduct {
    pub narratives: Vec<Narrative>,
    /// Corpus articles outside the lookback window (excluded-evidence telemetry). The cap-based
    /// `budget_truncated` is back with the corpus cap (A5); `stale_news` is no longer the only
    /// exclusion left.
    pub stale_news_ids: Vec<i64>,
    /// Corpus articles inside the window that lost the `feed_rank` cut on
    /// `COGNITION_JOURNALIST_CORPUS_LIMIT` (A5).
    pub budget_truncated_ids: Vec<i64>,
    /// Generation-level card score, including called-empty markers. `None` on a no-call marker
    /// or tolerated missing field.
    pub card_score: Option<i16>,
    /// The prior generation's card score (the memory line's value) — the continuity audit,
    /// mirroring `sigil_synthesis.previous_score`. Audit-only, never served.
    pub card_score_prev: Option<i16>,
    /// Generation-level entity title: the same value on every row of the generation,
    /// the called-empty marker included (a quiet week's honest hook is the product). `None`
    /// on the no-corpus marker (no call), a pre-headline reply, or a dropped title.
    pub headline: Option<String>,
}

pub type NarrativesOutput = Generation<NarrativesProduct>;

/// generate_narratives runs the full per-entity generation (the analog of `NewsNarrator.Generate`,
/// minus persistence): `build_narratives_request` → `extract(EmotionalNews)` (the tolerant parse) →
/// `ground_narratives`. The per-entity core the handler (and the parity `--vet` path) drive.
/// `now_epoch` is the recency reference for the impact scoring.
pub async fn generate_narratives(
    hx: &Harness,
    req: &NarrativesReq,
    temperature: f64,
    now_epoch: i64,
) -> Result<NarrativesOutput> {
    let build = build_narratives_request(hx, req, temperature).await?;
    generate_narratives_from_build(hx, build, now_epoch).await
}

/// generate_narratives_from_build finishes a generation from an already-built request — the
/// handler builds ONCE, debounces on the build's `input_hash`, then hands the same build here
/// (no double corpus/heat load). `generate_narratives` stays as the build-and-run composition
/// for the parity/eval paths, which never debounce.
pub async fn generate_narratives_from_build(
    hx: &Harness,
    build: NarrativesBuild,
    now_epoch: i64,
) -> Result<NarrativesOutput> {
    let ready = match build {
        NarrativesBuild::NoCorpus {
            corpus_exclusions,
            input_hash,
        } => {
            // Keep configured-model provenance on the NULL-narrative marker.
            let model = hx.router.for_role(Role::NarrativeLogic).model().to_string();
            return Ok(Generation::uncalled(
                NarrativesProduct {
                    narratives: Vec::new(),
                    stale_news_ids: corpus_exclusions.stale_news_ids,
                    budget_truncated_ids: corpus_exclusions.budget_truncated_ids,
                    // No corpus → no call → no verdict: NULL binds and the card draws the Veil.
                    card_score: None,
                    card_score_prev: None,
                    headline: None,
                },
                model,
                NARRATIVES_PROMPT_VERSION,
                Vec::new(),
                Some(input_hash),
            ));
        }
        NarrativesBuild::Ready(r) => *r,
    };

    // route(NarrativeLogic) + extract(NarrativesParser). A malformed/unsalvageable reply surfaces as
    // the parser's Err → the item fails and backs off, never a marker.
    let extracted = hx
        .extract(
            Role::NarrativeLogic,
            &ready.built_prompt,
            &ready.opts,
            &NarrativesParser,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let model = extracted.model.clone();
    let parsed = extracted.value.ok_or_else(|| {
        anyhow!("narratives: parser returned None (NarrativesParser signals failure via Err)")
    })?;

    let narratives = ground_narratives(&parsed.narratives, &ready.corpus, now_epoch);
    let input_ids = dedupe_i64(
        narratives
            .iter()
            .flat_map(|n| n.input_news_ids.iter().copied())
            .collect(),
    );

    Ok(Generation::called(
        NarrativesProduct {
            narratives,
            stale_news_ids: ready.corpus_exclusions.stale_news_ids,
            budget_truncated_ids: ready.corpus_exclusions.budget_truncated_ids,
            card_score: parsed.card_score,
            card_score_prev: ready.card_score_prev,
            headline: parsed.headline,
        },
        model,
        NARRATIVES_PROMPT_VERSION,
        input_ids,
        Some(ready.input_hash),
        call,
    ))
}

/// One persisted storyline row: the narrative, its classified trajectory, the
/// trajectory_components audit json, and the storyline it progressed (None = unresolved).
type ClassifiedRow<'a> = (&'a Narrative, &'static str, serde_json::Value, Option<i64>);

/// Persist one row per narrative, or a single NULL marker. Rows in one transaction share a
/// generation timestamp. Storyline identity is a fact of the packet corpus: every article
/// the entity participates in, and every article belongs to exactly one storyline — so each
/// narrative's storyline is the mode of its cited articles' storylines, and `classify_delta`
/// anchors on the part's last_impact (storyline_entities), so heating_up / cooling_off survive
/// any re-titling. The Journalist updates parts; it never creates story identity.
pub async fn persist_narratives(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    trigger_type: &str,
    trigger_payload: &serde_json::Value,
    out: &NarrativesOutput,
) -> Result<()> {
    let pool = &hx.pool;
    let prov = &out.provenance;
    let trigger_json = trigger_payload.to_string();

    // NOW() is constant within a transaction, so every row shares one generated_at.
    // The part progression runs in the SAME transaction: the part updates and the rows citing them
    // commit atomically.
    let mut tx = pool.begin().await.context("begin narratives tx")?;

    // The citation → storyline map for everything this generation cites (one query).
    let article_ids: Vec<i64> = out
        .narratives
        .iter()
        .flat_map(|n| n.input_news_ids.iter().copied())
        .collect();
    let storyline_of: std::collections::HashMap<i64, i64> = if article_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        sqlx::query(
            "SELECT article_id, storyline_id FROM storyline_articles WHERE article_id = ANY($1)",
        )
        .bind(&article_ids)
        .fetch_all(&mut *tx)
        .await
        .context("load article storylines")?
        .into_iter()
        .map(|r| (r.get("article_id"), r.get("storyline_id")))
        .collect()
    };

    let items: Vec<PartItem> = out
        .narratives
        .iter()
        .map(|n| {
            let cited: Vec<i64> = n
                .input_news_ids
                .iter()
                .filter_map(|a| storyline_of.get(a).copied())
                .collect();
            PartItem {
                storyline_id: mode_storyline(&cited),
                impact: n.impact,
                source_names: &n.source_names,
            }
        })
        .collect();
    let outcomes = progress_generation(&mut tx, sport, entity_type, entity_id, &items).await?;

    let classified: Vec<ClassifiedRow> = out
        .narratives
        .iter()
        .zip(&outcomes)
        .map(|(n, o)| {
            let reason = match o.delta_reason {
                "up" => "impact_up",
                "down" => "impact_down",
                "stable" => "impact_stable",
                other => other,
            };
            let components = if o.unresolved {
                json!({
                    "previous_impact": serde_json::Value::Null,
                    "current_impact": n.impact,
                    "impact_delta": serde_json::Value::Null,
                    "reason": "storyline_unresolved",
                })
            } else {
                json!({
                    "previous_impact": o.previous_impact,
                    "current_impact": n.impact,
                    "impact_delta": o.impact_delta,
                    "reason": reason,
                    "storyline_id": o.storyline_id,
                })
            };
            (n, o.trajectory, components, o.storyline_id)
        })
        .collect();

    const INSERT: &str = r#"
        INSERT INTO news_summaries (
            entity_type, entity_id, sport, trigger_type, trigger_payload,
            narrative_title, body, impact, impact_components,
            input_news_ids,
            narrative_updated_at, source_count, source_names, source_latest_at, source_oldest_at,
            trajectory, trajectory_components,
            model_version, prompt_version, input_hash, storyline_id,
            card_score, card_score_prev, headline, generated_at
        ) VALUES (
            $1,$2,$3,$4,$5::jsonb, $6,$7,$8,$9::jsonb, $10,
            COALESCE(to_timestamp($11::double precision), NOW()), $12, $13,
            to_timestamp($14::double precision), to_timestamp($15::double precision),
            $16, $17::jsonb,
            $18,$19,$20,$21,
            $22,$23,$24,NOW()
        )
        RETURNING id"#;

    let rows: Vec<Option<ClassifiedRow>> = if classified.is_empty() {
        vec![None]
    } else {
        classified.into_iter().map(Some).collect()
    };

    let mut product_row_ids: Vec<i64> = Vec::with_capacity(rows.len());
    for row in rows {
        let impact_components_json;
        let trajectory_json;
        let empty_names = Vec::<String>::new();
        let title: Option<&str>;
        let body: Option<&str>;
        let impact: Option<i16>;
        let input_news_ids: &Vec<i64>;
        let narrative_updated_at: Option<i64>;
        let source_count: i32;
        let source_names: &Vec<String>;
        let source_latest_at: Option<i64>;
        let source_oldest_at: Option<i64>;
        let trajectory: &str;
        let storyline_id: Option<i64>;
        let context: &str;

        match &row {
            Some((n, row_trajectory, row_trajectory_components, row_storyline_id)) => {
                impact_components_json = n.impact_components.to_string();
                trajectory_json = row_trajectory_components.to_string();
                title = Some(n.title.as_str());
                body = Some(n.body.as_str());
                impact = Some(n.impact as i16);
                input_news_ids = &n.input_news_ids;
                // Keep source_latest_epoch bound twice: narrative_updated_at ($11) and
                // source_latest_at ($14), matching the pre-loop scored path.
                narrative_updated_at = n.source_latest_epoch;
                source_count = n.source_count;
                source_names = &n.source_names;
                source_latest_at = n.source_latest_epoch;
                source_oldest_at = n.source_oldest_epoch;
                trajectory = row_trajectory;
                storyline_id = *row_storyline_id;
                context = "persist narrative row";
            }
            None => {
                impact_components_json = "{}".to_string();
                trajectory_json = "{}".to_string();
                title = None;
                body = None;
                impact = None;
                input_news_ids = &prov.input_ids;
                narrative_updated_at = Option::<i64>::None;
                source_count = 0_i32;
                source_names = &empty_names;
                source_latest_at = Option::<i64>::None;
                source_oldest_at = Option::<i64>::None;
                trajectory = DEFAULT_TRAJECTORY;
                storyline_id = None;
                context = "persist narratives marker";
            }
        }

        let inserted = sqlx::query(INSERT)
            .bind(entity_type)
            .bind(entity_id)
            .bind(sport)
            .bind(trigger_type)
            .bind(&trigger_json)
            .bind(title)
            .bind(body)
            .bind(impact)
            .bind(&impact_components_json)
            .bind(input_news_ids)
            .bind(narrative_updated_at)
            .bind(source_count)
            .bind(source_names)
            .bind(source_latest_at)
            .bind(source_oldest_at)
            .bind(trajectory)
            .bind(&trajectory_json)
            .bind(prov.model_version.as_str())
            .bind(prov.prompt_version)
            .bind(prov.input_hash.as_deref())
            .bind(storyline_id)
            // Generation-level card score, including called-empty markers.
            .bind(out.card_score)
            .bind(out.card_score_prev)
            // Generation-level title, including called-empty markers.
            .bind(out.headline.as_deref())
            .fetch_one(&mut *tx)
            .await
            .context(context)?;
        product_row_ids.push(inserted.get("id"));
    }

    tx.commit().await.context("commit narratives tx")?;
    let narratives: Vec<_> = out
        .narratives
        .iter()
        .map(|n| {
            json!({
                "title": &n.title,
                "input_news_ids": &n.input_news_ids,
                "source_count": n.source_count,
                "source_names": &n.source_names,
                "impact": n.impact,
            })
        })
        .collect();
    let included_evidence = json!({
        "input_news_ids": &out.provenance.input_ids,
        "narratives": narratives,
    });
    let num_ctx = out
        .request_body()
        .and_then(|b| b.pointer("/options/num_ctx"))
        .and_then(|v| v.as_i64())
        .unwrap_or(crate::route::VOICE_NUM_CTX_PACKET as i64) as i32;
    let mut excluded = Vec::new();
    if !out.stale_news_ids.is_empty() {
        excluded.push(json!({
            "reason": "stale_news",
            "dropped_count": out.stale_news_ids.len(),
            "dropped_news_ids": &out.stale_news_ids,
            "lookback_seconds": NEWS_LOOKBACK_SECS,
        }));
    }
    if !out.budget_truncated_ids.is_empty() {
        excluded.push(json!({
            "reason": "budget_truncated",
            "dropped_count": out.budget_truncated_ids.len(),
            "dropped_news_ids": &out.budget_truncated_ids,
            "corpus_limit": corpus_limit(num_ctx),
        }));
    }
    insert_generation_ledger_best_effort(
        pool,
        out,
        NARRATIVES_LEDGER,
        LedgerEvent {
            entity_type,
            entity_id,
            sport,
            pair_entity: None,
            trigger_type,
            trigger_payload: trigger_payload.clone(),
            product_row_ids,
            included_evidence,
            excluded_evidence: json!(excluded),
            context_budget: out.context_budget(json!({
                "num_predict": out.request_body().and_then(|b| b.pointer("/options/num_predict"))
                    .and_then(|v| v.as_i64()).unwrap_or(NARRATIVES_NUM_PREDICT_PACKET as i64),
                "num_ctx": out.request_body().and_then(|b| b.pointer("/options/num_ctx"))
                    .and_then(|v| v.as_i64()).unwrap_or(crate::route::VOICE_NUM_CTX_PACKET as i64),
            })),
            parser_outcome: if !out.was_called() {
                "no_call"
            } else if out.narratives.is_empty() {
                "parsed_empty"
            } else {
                "parsed"
            },
        },
    )
    .await;
    Ok(())
}

/// now_unix is the recency reference for `compute_news_impact` — Unix seconds, no datetime crate.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Handler.
// ---------------------------------------------------------------------------

/// NarrativesHandler drains the durable `narratives` stage: read the vetted corpus, (live) dedup it,
/// group it into storylines with the model, score each deterministically, and persist one
/// news_summaries row per narrative (or a marker). Unlike rating, narratives is a `pipeline_work`
/// stage (`Stage::Narratives`).
pub struct NarrativesHandler;

impl NarrativesHandler {
    pub fn new() -> Self {
        NarrativesHandler
    }
}

impl Default for NarrativesHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for NarrativesHandler {
    fn stage(&self) -> Stage {
        Stage::Narratives
    }

    // Two slots keep a deep narratives queue from taking the group from other voices.
    fn max_in_flight(&self) -> usize {
        2
    }
    fn slot_group(&self) -> Option<(&'static str, usize)> {
        Some(crate::stage::MAC_SLOTS)
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let entity_id = item.entity_id_i32()?;
        // nameOf uses the queue's raw sport value (drainNarratives), as does the prompt's req.Sport.
        let name = lookup_entity_name(&hx.pool, &item.entity_type, entity_id, &item.sport).await?;
        let req = NarrativesReq {
            entity_type: item.entity_type.clone(),
            entity_id,
            entity_name: name,
            sport: item.sport.clone(),
            trigger_type: "periodic".to_string(),
        };
        let sport_up = item.sport.to_uppercase();

        // Load and debounce material before building a prompt or calling the model.
        let material = load_narratives_material(hx, &req).await?;
        let key = EntityKey {
            entity_type: item.entity_type.clone(),
            entity_id,
            sport: sport_up.clone(),
            season: None,
        };
        if hx
            .debounce_unchanged("news_summaries", &key, &material.input_hash)
            .await?
        {
            debug!(
                entity_type = %item.entity_type,
                entity_id,
                sport = %sport_up,
                "narratives: inputs unchanged, skipping generation"
            );
            return Ok(());
        }

        let build = finish_narratives_build(hx, &req, material, NARRATIVES_TEMPERATURE).await?;
        let out = generate_narratives_from_build(hx, build, now_unix()).await?;

        persist_narratives(
            hx,
            &item.entity_type,
            entity_id,
            &sport_up,
            &req.trigger_type,
            &serde_json::Value::Null,
            &out,
        )
        .await?;

        // The Journalist does not wake the Influencer. Packet fan-out is her sole waker; a second
        // input-version source would make the shared work row churn between prefixes.

        Ok(())
    }
}

#[cfg(test)]
mod tests;
