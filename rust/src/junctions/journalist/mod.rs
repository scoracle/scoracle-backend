//! news narratives — the `Stage::Narratives` queue handler. The largest GPU stage, and the one
//! with native Rust value-add: it composes the candle
//! **embed+cluster** primitive (group near-duplicate articles and drop them BEFORE the model call —
//! the dedup the Go pipeline never had) with `route(NarrativeLogic) + extract + persist`.
//!
//! Rust implementation of the news narrative stage:
//! - `load_packet_corpus` reads the entity's compiled packets from Postgres.
//! - `build_narratives_prompt` is deterministic. (n17: the transfer-heat grounding section is
//!   gone — The Insider owns transfer truth end-to-end; heat lines remain vibe's concern only.)
//! - The n13 system prompt is model-neutral and schema-first for smaller local models.
//! - `parse_narratives` uses a tolerant balanced-brace salvager: a truncated tail drops its last
//!   incomplete object; an empty `{"narratives": []}` is a successful parse -> marker.
//! - `compute_news_impact` reproduces the deterministic per-narrative impact (volume + corroboration
//!   + recency) byte-for-byte — like rating's `pctBand`, deterministic stage-shaping mirrored in Rust,
//!     NOT moved to Postgres (it scores a MODEL-selected article subset, so it can't be a pure SQL stat).
//!
//! (The embed+cluster near-duplicate dedup that used to reshape the corpus before the model call
//! left with the embed layer: the packet corpus is compiled claims, deduped upstream by the
//! Editor's read and the worker's exact-title sweep.)
//!
//! `NarrativesHandler` is a live queue stage gated by `COGNITION_STAGES`. It is the News hub stage:
//! transfer heat and source freshness are folded here before Vibe and Sigil consume the result.

use crate::corpus::{
    dedupe_i64, lookup_entity_name,
};
use crate::harness::{EntityKey, Harness, Parser, Provenance};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
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

// This junction's contract with its model — system prompt, contract version, and prompt
// builder — lives in `prompt.rs`, so a change to what this character is asked is a one-file
// diff. Re-exported here so call sites and the ledger keep reading it from the stage module.
pub mod prompt;
pub use prompt::{NARRATIVES_PROMPT_VERSION, NARRATIVES_SYSTEM_PROMPT, build_narratives_prompt, narratives_format_schema};

// ---------------------------------------------------------------------------
// Constants — mirror news_narratives.go.
// ---------------------------------------------------------------------------

/// Output schema version for the parsed narrative document, distinct from the prompt contract.
/// v2-schema: Ollama grammar-constrained decoding (Phase 5) — the shape is enforced by the
/// server, not hoped for by the prompt. v3-schema: required `card_score` (the Journalist's
/// 1-99 busyness verdict, the tarot deck's number) ordered after narratives/buckets.
pub const NARRATIVES_OUTPUT_CONTRACT_VERSION: &str = "narratives-v3-schema";

/// Production decode temperature (`ollama.Generate` in Go). The parity gate pins temp 0 (the
/// deterministic-axes diff); production narrates at 0.6.
pub const NARRATIVES_TEMPERATURE: f64 = 0.6;

/// The Journalist's reservation inside a LARGE window — several multi-sentence narratives; the
/// prompt caps count + body length. Reachable only when `VOICE_NUM_CTX` pins a window above the
/// 4096 packet envelope (`narratives_decode_budget` keys on the window); production runs the
/// packet reservation below. The arithmetic lesson that sized this pair is permanent: a
/// reservation the window cannot hold silently evicts the system prompt mid-generation, and the
/// failure looks like a model that stopped obeying its rules (L9; measured 153/8,899 calls at
/// the old 8192 window). (The legacy `NARRATIVES_NUM_CTX` constant this rode beside — the
/// 16384 window the legacy corpus was sized for — left with the legacy rail.)
pub const NARRATIVES_NUM_PREDICT: i32 = 4000;

/// The Journalist's output reservation on the packet rail (§7's envelope: ≤800, his share 700).
///
/// 4000 was sized for a corpus of twenty article bodies and a narrator asked to cover all of it.
/// The packet rail hands him ONE storyline, already assembled, so the job is to narrate a story
/// rather than to survey a feed — and 700 tokens is a card, not a truncation. The legacy value is
/// untouched beside it: under `RAIL=legacy` this constant is not read at all.
pub const NARRATIVES_NUM_PREDICT_PACKET: i32 = 700;

/// The narratives call's window and output reservation on this rail. Both move together, because
/// the reservation is part of what has to fit inside the window — the failure this pair exists to
/// prevent is a prompt plus a reservation that overflow and silently evict the system prompt.
/// It keys on the WINDOW, not on the rail (Scott, 2026-08-06 — "run them, but run them at 4096").
/// The rail says which corpus the Journalist reads; the window says how much room he has, and a
/// 4,000-token reservation inside a 4,096-token window leaves nothing for the prompt at all. The
/// pairing is arithmetic, so it must follow the number the arithmetic is about.
pub fn narratives_decode_budget(num_ctx: i32) -> (i32, i32) {
    if crate::route::small_voice_window(num_ctx) {
        (num_ctx, NARRATIVES_NUM_PREDICT_PACKET)
    } else {
        (num_ctx, NARRATIVES_NUM_PREDICT)
    }
}

/// Per-article description cap rendered into the prompt (Go's `truncate(desc, 200)`).
const DESC_TRUNCATE: usize = 200;

/// Ceiling on how many articles reach one Journalist prompt.
///
/// The corpus load was unbounded, which was survivable only because ingest capped each entity at
/// twelve headlines. Once ingest takes Google's page 1 whole, a busy club brings ~100 articles a
/// day and this query would hand every one of them to a 16,384-token context shared by all six
/// voices — failing as silent truncation inside the prompt rather than as a number in a log.
///
/// 40 is deliberately generous against the observed shape: articles render at `DESC_TRUNCATE`
/// (read articles rendered at 900 chars on the legacy rail), so a full 40 with the usual four
/// read was about 4*900 + 36*200 ≈ 11 KB — expansive, and still well clear of the ceiling.
///
/// The exact number is a VOICE decision, not a plumbing one: it trades breadth of evidence against
/// room for the reply, and that trade belongs to the prompt-tuning session. Env-tunable so that
/// session can move it without a rebuild.
const DEFAULT_CORPUS_LIMIT: i64 = 40;

/// The same ceiling inside a SMALL window (4096). Forty articles at up to 900 characters of
/// Editor card each is ~11 KB — around 3,000 tokens, which fits a 16,384 window with room to
/// spare and does not fit a 4,096 one beside a system prompt, a memory block and a reservation.
/// Eight is what the arithmetic leaves, and the excluded articles are still NAMED (A5) through
/// the same `budget_truncated_ids` band the forty-article cut uses.
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

/// The vetted-news lookback window — Go's `NewsLookback = 72 * time.Hour`, in seconds. A fresh
/// Editor card also keeps an article in the corpus, so richer newly-enqueued evidence can
/// wake The Journalist even when the source article's `published_at` has aged past this boundary.
const NEWS_LOOKBACK_SECS: f64 = 259_200.0;

// ---------------------------------------------------------------------------
// Types.
// ---------------------------------------------------------------------------

/// NarrativesReq describes the entity whose recent news to narrate. Mirrors `NarrativesRequest`
/// (the drain path always passes `trigger_type = "periodic"` and a nil trigger map → jsonb `null`).
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
///
/// (The legacy rail's per-article baggage — `url`, `full_text`, the `article_read_*` evidence
/// card and its fingerprint fields — was stripped once `load_packet_corpus` became the only
/// loader: every one of those fields was hardwired to empty/`None` on the packet rail.)
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

/// ModelNarrative is one object the local model returns. `#[serde(default)]` per field mirrors Go's
/// `encoding/json` tolerance of missing fields; an explicit `"articles": null` (or a non-int element)
/// makes serde skip the object at parse — net-identical to Go, which keeps it then drops it in
/// grounding for having no valid article (either way it is excluded).
#[derive(Clone, Debug, Default, Deserialize)]
struct ModelNarrative {
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    articles: Vec<i32>,
}

/// ParsedNarratives is the salvaged document — the `T` the [`NarrativesParser`] yields.
/// `narratives` drives the storyline persist. The n9 `article_buckets` section was removed in n16;
/// The Editor writes `news_articles.bucket` from its own `story_type` now.
#[derive(Clone, Debug, Default)]
pub struct ParsedNarratives {
    narratives: Vec<ModelNarrative>,
    /// The Journalist's n12 busyness verdict, clamped 1-99 at parse. Best-effort: a reply missing
    /// it (pre-n12 salvage, truncated tail) parses to `None`, never a
    /// failure — the row simply persists NULL and the card falls back to the Veil.
    card_score: Option<i16>,
}

impl ParsedNarratives {
    /// Read access for the Phase-3 eval (`eval_tasks::NarrativeTask`): the storylines the model
    /// returned, as `(title, body, cited_article_numbers)` triples in the model's freshest-first
    /// order. This is the RAW returned set — DB grounding (mapping article numbers → real news ids),
    /// impact scoring, and the marker decision all happen downstream and need a pool, so the
    /// narrative-grounding rubric scores the model output directly and offline. The private
    /// `ModelNarrative` DTO stays encapsulated; only this minimal view is exposed.
    pub fn returned(&self) -> impl Iterator<Item = (&str, &str, &[i32])> {
        self.narratives
            .iter()
            .map(|n| (n.title.as_str(), n.body.as_str(), n.articles.as_slice()))
    }

    /// The n12 busyness verdict, for the eval's `card_score_*` axes (D-T47 follow-through: a
    /// field the gate cannot see is a field a prompt edit can quietly break — this one was
    /// invisible from n12 until the n17 pass).
    pub fn card_score(&self) -> Option<i16> {
        self.card_score
    }
}

/// NarrativesParser runs the tolerant salvager. It returns `Ok(Some(parsed))` for a PARSEABLE
/// document (even an empty array — a legitimate "no storyline this cycle" → marker downstream) and
/// `Err` for a genuinely malformed/truncated reply with nothing salvageable (Go's `!ok` → a hard
/// error that the queue retries, NOT a silent marker). It never returns `Ok(None)`: narratives has
/// no post-model fail-closed marker carried by the parser — the marker decision is made AFTER
/// grounding (zero grounded narratives), mirroring Go.
pub struct NarrativesParser;

impl Parser<ParsedNarratives> for NarrativesParser {
    fn parse(&self, raw: &str) -> Result<Option<ParsedNarratives>> {
        let (narratives, ok) = parse_narratives(raw);
        if !ok {
            // Go: `return nil, fmt.Errorf("parse narratives failed ...")` — a real generation failure
            // that must retry (NOT a no-data marker). generation_failed must never masquerade as no-data.
            return Err(anyhow!(
                "parse narratives failed (raw={:?})",
                crate::util::truncate(raw, 200)
            ));
        }
        // The eval→guard migration (2026-08-19, DOCTRINE-directing.md): served storyline prose
        // never names a product. Scans the parsed titles+bodies (the served fields), not the raw
        // document — preamble the salvager discards must not fail a clean edition.
        for n in &narratives {
            if let Some(p) = crate::guards::first_product_name(&n.title)
                .or_else(|| crate::guards::first_product_name(&n.body))
            {
                tracing::warn!(guard = "product_name", name = p, "narratives edition rejected");
                return Err(anyhow!("narratives: storyline names product {p:?}"));
            }
        }
        // card_score (n12) is best-effort the same way: missing → None (NULL row → Veil), never
        // a parse failure. The grammar makes it required on the live path; this tolerance covers
        // truncated tails and the offline bins replaying pre-n12 output.
        let card_score = parse_card_score(raw);
        Ok(Some(ParsedNarratives {
            narratives,
            card_score,
        }))
    }
}

// ---------------------------------------------------------------------------
// Corpus loader — the widened net (Cognition Phase 3): every vetted CANONICAL article for the
// entity within the lookback, no transfer-bucket exclusion and no size cap. The scrub novelty gate
// already collapsed reposts (`duplicate_of IS NULL` keeps only originals), so the honest compressor
// runs once at the tip of the spear and narratives sees the full de-duplicated breadth.
// ---------------------------------------------------------------------------

/// How far back the corpus looks. 72h was matched to the legacy narratives news lookback at
/// cutover, so the flip changed WHAT the corpus is made of, not WHEN it starts. (The legacy
/// loaders it was matched to were deleted in the Phase 9 prune; the number stays because 72h is
/// also the storyline window.)
pub const PACKET_LOOKBACK_HOURS: i64 = 72;
/// Packets read per entity per run. An entity in more than this many live storylines at once is
/// having an extraordinary week; the newest-compiled win and the rest are named as exclusions.
pub const MAX_PACKETS_PER_ENTITY: usize = 5;

/// load_packet_corpus is THE corpus loader (7.3). It replaced the vetted-article-window
/// loaders, which were deleted with the rail in the Phase 9 prune. It reads the entity's
/// storylines, compiled, instead of its articles, raw.
///
/// **The shape is deliberately unchanged.** It returns the same `Vec<CorpusItem>` — one item per
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
        framing.push_str(&render::framing(&view, Some(&part), render::Voice::Journalist));

        for marked in render::mark_contested(&view.claims) {
            let fact = if marked.marked {
                // The contradiction survives INTO the prompt, marked, so the Journalist can write
                // "reports differ" instead of picking a side by accident (T3/D6).
                format!("⇄ {}", marked.claim.fact)
            } else {
                marked.claim.fact.clone()
            };
            match by_article.iter_mut().find(|(id, _)| *id == marked.claim.article_id) {
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
            // The first fact is the headline slot and the rest are the body: the same two-part
            // shape `article_context` already renders, so the prompt's news block is byte-shaped
            // exactly as it is on the legacy rail.
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

    // The n20 char budget. MAX_PACKETS_PER_ENTITY bounds how many STORIES are read, but a
    // mega-storyline is one story with a hundred member articles — measured 2026-08-15, the
    // news block alone reached 63 KB (~160 items) inside a 4,096-token window that also holds
    // an ~830-token system prompt, the framing, the memory card and the reply reservation.
    // Everything past the window was silently truncated before the model saw it (11% of
    // editions that day). Items are newest-packet-first, newest-claim-first, so the budget
    // keeps the freshest evidence and the cut articles are NAMED (A5) like every other cut.
    let (corpus, over_budget) = apply_news_budget(corpus, PACKET_NEWS_BUDGET_CHARS);
    exclusions.budget_truncated_ids.extend(over_budget);

    exclusions.budget_truncated_ids.sort_unstable();
    exclusions.budget_truncated_ids.dedup();
    Ok((corpus, exclusions, framing))
}

/// The rendered-size allowance for the numbered news block, in prompt CHARS (≈ tokens×4).
/// The 4,096 window's arithmetic: ~830 tok of system prompt + ~600 of framing (≤5 packets)
/// + ~500 of memory/SIGNALS + ~500 reserved for the reply leaves ~1,500 tok ≈ 6,000 chars
/// of evidence — about 15 packet items, roughly double the legacy small-window article cap.
const PACKET_NEWS_BUDGET_CHARS: usize = 6_000;

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
// Prompt — buildNarrativesPrompt (n9: no per-article relevance tag; the candle novelty gate is the
// compressor now, so narratives sees the widened, canonical-only corpus straight from the loader).
// ---------------------------------------------------------------------------

/// article_context is the model-visible text for one corpus item, rendered AFTER its headline
/// (the caller always writes `[source] title` first). On the packet corpus that text is the
/// article's remaining claims, and it is used only when it actually says something the headline
/// did not.
///
/// **Headline passthrough.** Returning an empty context is a real answer, not a failure: the
/// headline above it is the evidence. What must NOT happen is the legacy-rail behaviour of
/// falling through to an RSS description that is 99.7% the title repeated plus the outlet name,
/// producing `[Sky Sports] Arsenal sign Tzolis — Arsenal sign Tzolis Sky Sports` — wasted prompt
/// budget that read as corroboration the corpus does not have. (The Editor-card and `full_text`
/// branches that used to come first left with the legacy loader: the packet corpus never
/// carries either.)
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

/// load_entity_memory fetches the graph's per-entity memory card
/// (`narrative_context_for_entity`, mig 163). `None` = no memory, no prompt section.
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

/// How many of the entity's own recent card scores feed the n12 prompt as continuity memory —
/// mirrors sigil's `PRIOR_READ_LIMIT` (the Oracle's continuity trail).
const PRIOR_CARD_READS_LIMIT: i64 = 4;

/// The Journalist's own score memory: the latest non-NULL `card_score` (persisted as
/// `card_score_prev` on the new generation — the continuity audit) plus the rendered
/// prompt block.
pub struct PriorCardReads {
    pub latest: i16,
    pub card: String,
}

/// load_prior_card_reads renders the Journalist's OWN recent card scores as a continuity memory
/// block — mirrors sigil's `load_prior_read` (memory, never a reset; the echo-chamber rule).
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
    let trail: Vec<(i16, String)> = sqlx::query_as(
        r#"
        SELECT card_score, to_char(generated_at, 'Mon DD')
        FROM (
            SELECT DISTINCT generated_at, card_score
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

    let mut card = String::from(
        "YOUR PRIOR CARD READS (memory — your own previous card scores; continuity, not new evidence):\n",
    );
    let scores: Vec<String> = trail.iter().map(|(s, d)| format!("{s} ({d})")).collect();
    card.push_str(&format!(
        "Card scores (newest first): {}\n",
        scores.join(" · ")
    ));
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

/// parse_narratives salvages each complete narrative object from the model's response independently,
/// rather than requiring the whole document to be well-formed — LLM length is non-deterministic, so a
/// reply can truncate mid-array or carry one malformed object. It scans every balanced top-level
/// `{...}` inside the `"narratives"` array (respecting strings/escapes), parses each on its own, and
/// keeps the ones that parse. Byte-for-byte Go's `parseNarratives`.
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
                        // Braces are ASCII ⇒ the slice is on char boundaries; from_utf8 mirrors Go's
                        // json.Unmarshal, which also requires valid UTF-8 (a bad slice → skip, as Go's
                        // err != nil does).
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

/// parse_card_score salvages the n12 `card_score` integer the same tolerant way the buckets are
/// salvaged: find the key, skip to its value, parse the leading number, clamp 1-99. `None` for an
/// absent key or a non-numeric value — never a parse failure (pre-n12 replays and truncated tails
/// simply persist NULL → the Veil). A quoted or fractional value is tolerated like the crown's
/// score parse (sigil `parse_crown_score`), minus the "N/100" form the tarot contract never uses.
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

// ---------------------------------------------------------------------------
// Grounding — map article numbers back to the corpus, compute the deterministic per-narrative impact.
// ---------------------------------------------------------------------------

/// ground_narratives maps the model's 1-indexed article numbers back to the corpus, computes the
/// per-narrative impact from ITS articles (never the model), and keeps only narratives with a title,
/// a body, and ≥1 valid article. Byte-for-byte Go's `groundNarratives`. `now_epoch` is the recency
/// reference (Unix seconds), captured once per generation.
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

/// compute_news_impact reproduces Go's deterministic per-narrative impact (0-100): a saturating
/// volume curve + distinct-source corroboration + a freshness bucket, over a narrative's OWN
/// articles. Mirrors `computeNewsImpact`; `now_epoch` replaces Go's implicit `time.Now()` (the
/// recency is hour-bucketed, so sub-second drift is irrelevant — and impact is NOT a parity axis, it
/// is a post-model deterministic score). Returns the score + the transparent components.
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
/// cycle → a NULL-narrative marker (no model call), mirroring Go's early return.
pub enum NarrativesBuild {
    NoCorpus {
        corpus_exclusions: CorpusExclusions,
        /// Hash over the (empty) material inputs — so a quiet entity's marker also debounces
        /// instead of re-marking every cycle.
        input_hash: String,
    },
    Ready(Box<NarrativesReady>),
}

/// The per-article reading fingerprint, as the debounce pre-image has always spelled it. On the
/// packet corpus there is no reading state on the item any more, so every article fingerprints
/// to this constant — which makes `article_readings_hash` a pure function of the article-id set
/// (already in the pre-image as `article_ids`). It is carried anyway, byte-for-byte.
///
/// ⛔ **DO NOT "TIDY" THIS OUT OF THE PRE-IMAGE.** Dropping the term — or changing this string —
/// changes every entity's hash at once, which is EXACTLY a `NARRATIVES_PROMPT_VERSION` bump by
/// another route: one forced regen of the whole fleet. (The Phase 9 demolition preserved the
/// legacy `reading_fingerprint(status, hash, epoch)` format the same way, for the same reason.)
/// The term can be retired for free only by riding the NEXT deliberate n-bump, which forces the
/// one regen anyway.
pub const READING_FINGERPRINT_NONE: &str = "none::0";

/// build_article_reading_input_components — moved verbatim from `article_reader` in 9.1. Sorts
/// by article id so corpus ordering cannot move the hash, then hashes the canonical pairs. See
/// the warning on [`READING_FINGERPRINT_NONE`]: this is a live cache key, not dead legacy code.
pub fn build_article_reading_input_components(items: &[(i64, String)]) -> String {
    let mut pairs = items.to_vec();
    pairs.sort_by_key(|(id, _)| *id);
    let mut out = String::from("[");
    for (i, (id, fp)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('[');
        out.push_str(&id.to_string());
        out.push(',');
        out.push_str(&crate::util::go_json_string(fp));
        out.push(']');
    }
    out.push(']');
    crate::util::hash_components(&out)
}

/// build_narratives_input_components is the canonical debounce pre-image: the `prompt_version` (so a
/// contract bump forces exactly one regen — see below), the vetted corpus article ids (pre-dedup —
/// the material fact is WHAT NEWS EXISTS, not what the embedder kept). (n17: the transfer-heat
/// term is GONE with the heat input itself — the separation pass. The former summary/confidence note:
/// deliberately excluded — derived commentary, not material facts. Same canonical-JSON discipline as
/// `sigil::build_synthesis_input_components`.
///
/// `prompt_version` is folded in (M4 cutover lever): the debounce otherwise keys only on the corpus +
/// heat, so on an n-bump (n15→n16) an entity whose news is unchanged is debounced and NEVER re-runs
/// the new contract. Including the version changes every entity's hash exactly once at cutover → one
/// forced regen each → then it stabilizes.
/// The regen also re-points vibe for free (the narratives handler enqueues vibe post-persist).
pub fn build_narratives_input_components(corpus: &[CorpusItem]) -> String {
    let mut ids: Vec<i64> = corpus.iter().map(|c| c.id).collect();
    ids.sort_unstable();
    let mut out = format!(
        "{{\"prompt_version\":{},\"article_ids\":[",
        crate::util::go_json_string(NARRATIVES_PROMPT_VERSION)
    );
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&id.to_string());
    }
    out.push(']');
    let article_readings: Vec<(i64, String)> = corpus
        .iter()
        .map(|c| (c.id, READING_FINGERPRINT_NONE.to_string()))
        .collect();
    out.push_str(",\"article_readings_hash\":");
    out.push_str(&crate::util::go_json_string(
        &build_article_reading_input_components(&article_readings),
    ));
    out.push('}');
    out
}

/// NarrativesReady carries the assembled model inputs (the parity axes) plus the widened, canonical
/// corpus the grounding maps back to. `request_body` is computed from the SAME backend + opts the call
/// will use, so it can never drift from what is POSTed. (n9: near-duplicate collapse moved to the
/// candle novelty gate, so the corpus here is already the deduplicated breadth — no embed pass.)
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
    /// The latest non-NULL prior `card_score` (n12) — fed to the prompt as memory and persisted
    /// as `card_score_prev` (continuity audit). Prompt-only: NOT part of `input_hash`.
    pub card_score_prev: Option<i16>,
}

/// NarrativesMaterial is the material phase: the concurrent loads plus the debounce hash. The live
/// handler gates on `input_hash` between this and [`finish_narratives_build`] so a quiet wake never
/// pays the prompt assembly (Phase 2); the parity bins go through [`build_narratives_request`],
/// which composes both phases unchanged.
pub struct NarrativesMaterial {
    pub corpus: Vec<CorpusItem>,
    pub corpus_exclusions: CorpusExclusions,
    /// SHA over [`build_narratives_input_components`] — the debounce key.
    pub input_hash: String,
    /// The storyline framing block, on the packet rail only (7.3). `None` under `RAIL=legacy`,
    /// which is what keeps the legacy prompt byte-identical.
    pub packet_framing: Option<String>,
}

/// load_narratives_material runs the loads and hashes the material inputs. No embed, no prompt.
pub async fn load_narratives_material(
    hx: &Harness,
    req: &NarrativesReq,
) -> Result<NarrativesMaterial> {
    let sport_up = req.sport.to_uppercase();

    // n17: the transfer-heat load is GONE (the separation pass — The Insider owns transfer
    // truth end-to-end, and the Journalist files transfer stories from the corpus like any other
    // story). The rail decides WHAT the corpus is (7.1/7.3) — resolved once at boot, carried on
    // the harness, never re-read here.
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

    // The debounce keys on the material fact — what vetted, canonical news exists — AND the
    // prompt_version, so an n-bump forces exactly one regen per entity at cutover
    // (see build_narratives_input_components); otherwise unchanged-corpus entities never run n9.
    // (n17: heat left the components, so heat movement alone no longer re-triggers this stage —
    // the insider-side waker that fires on heat change now lands in the debounce as a no-op.)
    let input_hash = crate::util::hash_components(&build_narratives_input_components(&corpus));

    Ok(NarrativesMaterial {
        corpus,
        corpus_exclusions,
        input_hash,
        packet_framing,
    })
}

/// finish_narratives_build is the post-gate phase: the memory-card load plus the
/// prompt/options/wire-body assembly. (n9: no candle embed pass — the corpus arrives already
/// deduplicated from the loader, so this phase is pure assembly.)
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
    // Card-score grounding (n12): the deterministic SIGNALS tally + the Journalist's own prior
    // card reads. Error-swallowed like memory (enrichment, never a generation blocker) and
    // deliberately NOT in the input_hash (the score always moves — hashing it would self-trigger).
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
    let built_prompt = build_narratives_prompt(
        req,
        &corpus,
        memory.as_deref(),
        Some(&score_context),
        packet_framing.as_deref(),
    );
    let (num_ctx, num_predict) = narratives_decode_budget(hx.voice_num_ctx);
    let opts = GenerateOptions {
        system: Some(NARRATIVES_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict,
        num_ctx,
        json_mode: false,
        // Phase 5: grammar-constrained decoding replaces "hopefully JSON" (the failure class
        // the balanced-brace salvager was built for). The Go-parity free-text contract is
        // retired; the salvager stays as the tolerant parse path either way.
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

/// build_narratives_request runs the full deterministic prefix: load the widened vetted corpus, load
/// the transfer heat for grounding, then
/// `build_narratives_prompt` plus the n4 options and the exact wire body. NO model call — these
/// are the deterministic axes (the L2 finding: the storyline grouping is not a temp-0 parity
/// axis). The role is [`Role::NarrativeLogic`] (the news/transfer reasoner — narratives shares it
/// with vibe/transfers). Composition of [`load_narratives_material`] +
/// [`finish_narratives_build`]; the live handler calls the phases directly to debounce between
/// them.
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
pub struct NarrativesOutput {
    pub narratives: Vec<Narrative>,
    /// The configured model; marker rows still carry provenance.
    pub model: String,
    pub prompt_version: &'static str,
    /// The exact prompt + wire body (the deterministic axes). `None` for the no-corpus marker (no call).
    pub built_prompt: Option<String>,
    pub request_body: Option<serde_json::Value>,
    /// Tokens evaluated by Ollama for this call. `None` on no-corpus marker rows.
    pub eval_count: Option<i32>,
    pub wall_ms: Option<u64>,
    /// Corpus articles outside the lookback window (excluded-evidence telemetry). The cap-based
    /// `budget_truncated` is back with the corpus cap (A5); `stale_news` is no longer the only
    /// exclusion left.
    pub stale_news_ids: Vec<i64>,
    /// Corpus articles inside the window that lost the `feed_rank` cut on
    /// `COGNITION_JOURNALIST_CORPUS_LIMIT` (A5).
    pub budget_truncated_ids: Vec<i64>,
    /// The debounce key this generation was built from (Phase 1); persisted on every row of the
    /// generation so the next cycle's gate has something to compare against.
    pub input_hash: String,
    /// The Journalist's n12 card score — generation-level (persisted on EVERY row, marker
    /// included: a quiet week gets the Journalist's own low number). `None` only on the
    /// no-corpus marker (no model call → the Veil) or a tolerated pre-n12/truncated reply.
    pub card_score: Option<i16>,
    /// The prior generation's card score (the memory line's value) — the continuity audit,
    /// mirroring `sigil_synthesis.previous_score`. Audit-only, never served.
    pub card_score_prev: Option<i16>,
}

impl NarrativesOutput {
    /// provenance lifts the moat fields into the shared `Provenance` envelope. The row-level
    /// `input_news_ids` are still bound per narrative because each grounded storyline cites a
    /// different subset; `input_hash` is generation-level (the Phase 1 debounce key).
    fn provenance(&self) -> Provenance {
        let mut ids = Vec::new();
        for n in &self.narratives {
            ids.extend(n.input_news_ids.iter().copied());
        }
        Provenance {
            model_version: self.model.clone(),
            prompt_version: self.prompt_version,
            input_ids: dedupe_i64(ids),
            input_hash: Some(self.input_hash.clone()),
            trigger_payload: None,
        }
    }
}

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
            // The NULL-narrative marker. Go sets Model = a.ollama.Model() even here.
            let model = hx.router.for_role(Role::NarrativeLogic).model().to_string();
            return Ok(NarrativesOutput {
                narratives: Vec::new(),
                model,
                prompt_version: NARRATIVES_PROMPT_VERSION,
                built_prompt: None,
                request_body: None,
                eval_count: None,
                wall_ms: None,
                stale_news_ids: corpus_exclusions.stale_news_ids,
                budget_truncated_ids: corpus_exclusions.budget_truncated_ids,
                input_hash,
                // No corpus → no call → no verdict: NULL binds and the card draws the Veil.
                card_score: None,
                card_score_prev: None,
            });
        }
        NarrativesBuild::Ready(r) => *r,
    };

    // route(NarrativeLogic) + extract(NarrativesParser). A malformed/unsalvageable reply surfaces as
    // the parser's Err → the item fails + backs off (Go's parse failure → retry), never a marker.
    let extracted = hx
        .extract(
            Role::NarrativeLogic,
            &ready.built_prompt,
            &ready.opts,
            &NarrativesParser,
        )
        .await?;
    let parsed = extracted.value.ok_or_else(|| {
        anyhow!("narratives: parser returned None (NarrativesParser signals failure via Err)")
    })?;

    let narratives = ground_narratives(&parsed.narratives, &ready.corpus, now_epoch);

    Ok(NarrativesOutput {
        narratives,
        model: extracted.model,
        prompt_version: NARRATIVES_PROMPT_VERSION,
        built_prompt: Some(extracted.built_prompt),
        request_body: Some(extracted.request_body),
        eval_count: Some(extracted.eval_count),
        wall_ms: Some(extracted.wall_ms),
        stale_news_ids: ready.corpus_exclusions.stale_news_ids,
        budget_truncated_ids: ready.corpus_exclusions.budget_truncated_ids,
        input_hash: ready.input_hash,
        card_score: parsed.card_score,
        card_score_prev: ready.card_score_prev,
    })
}

fn narratives_included_evidence(out: &NarrativesOutput) -> serde_json::Value {
    let narratives: Vec<serde_json::Value> = out
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
    json!({
        "input_news_ids": out.provenance().input_ids,
        "narratives": narratives,
    })
}

fn narratives_excluded_evidence(out: &NarrativesOutput) -> serde_json::Value {
    // The cap that actually applied, read off the EXACT wire body — the same discipline as
    // `context_budget` below. The limit is window-derived now, so restating a constant here
    // would misreport the drop on any host that pinned `VOICE_NUM_CTX`.
    let num_ctx = out
        .request_body
        .as_ref()
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
    json!(excluded)
}

fn narratives_parser_outcome(out: &NarrativesOutput) -> &'static str {
    if out.built_prompt.is_none() {
        "no_call"
    } else if out.narratives.is_empty() {
        "parsed_empty"
    } else {
        "parsed"
    }
}

/// One persisted storyline row: the narrative, its classified trajectory, the
/// trajectory_components audit json, and the storyline it progressed (None = unresolved).
type ClassifiedRow<'a> = (&'a Narrative, &'static str, serde_json::Value, Option<i64>);

/// persist_narratives writes ONE news_summaries row per narrative (all sharing the transaction's
/// `NOW()` — a "generation"), or a single NULL-narrative marker row when there is none. Mirrors
/// `news_narratives.go::persist`: `trigger_payload` is the caller's value (the drain passes jsonb
/// `null` — Go marshals the nil trigger map). (The `source_attribution` column — always NULL here
/// — was dropped in mig 139, plan C7.)
///
/// Mig 219 (the narrative_threads collapse): storyline identity is a FACT on the packet rail,
/// not a match. Every corpus article reached this generation through a packet of a storyline
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
    let prov = out.provenance().with_trigger_payload(trigger_payload);
    let trigger_json = prov.trigger_payload_json("null");

    // NOW() is constant within a transaction (transaction_timestamp), so every row of this generation
    // shares one generated_at — Go's `res.GeneratedAt`, without needing a datetime crate to bind it.
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
            card_score, card_score_prev, generated_at
        ) VALUES (
            $1,$2,$3,$4,$5::jsonb, $6,$7,$8,$9::jsonb, $10,
            COALESCE(to_timestamp($11::double precision), NOW()), $12, $13,
            to_timestamp($14::double precision), to_timestamp($15::double precision),
            $16, $17::jsonb,
            $18,$19,$20,$21,
            $22,$23,NOW()
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
            // n12: generation-level card score — the SAME value on every row of the generation
            // (scored storylines AND the called-empty marker); NULL only for no-corpus/pre-n12.
            .bind(out.card_score)
            .bind(out.card_score_prev)
            .fetch_one(&mut *tx)
            .await
            .context(context)?;
        product_row_ids.push(inserted.get("id"));
    }

    tx.commit().await.context("commit narratives tx")?;
    insert_cognition_ledger_best_effort(
        pool,
        CognitionLedgerEntry {
            stage: "narratives".to_string(),
            lens: "narratives".to_string(),
            role: Role::NarrativeLogic.as_str().to_string(),
            entity_type: entity_type.to_string(),
            entity_id,
            sport: sport.to_string(),
            pair_entity_type: None,
            pair_entity_id: None,
            trigger_type: trigger_type.to_string(),
            trigger_payload: trigger_payload.clone(),
            product_table: "news_summaries".to_string(),
            product_row_ids,
            model_version: prov.model_version,
            prompt_version: prov.prompt_version.to_string(),
            output_contract_version: NARRATIVES_OUTPUT_CONTRACT_VERSION.to_string(),
            input_ids: prov.input_ids,
            input_hash: prov.input_hash,
            request_body: out.request_body.clone(),
            built_prompt: out.built_prompt.clone(),
            included_evidence: narratives_included_evidence(out),
            excluded_evidence: narratives_excluded_evidence(out),
            // Read off the EXACT wire body rather than restated from constants: the decode
            // budget is window-scoped (`narratives_decode_budget`), and a ledger that reported
            // the wrong envelope for a call would be the one place a flip was invisible. Falls
            // back to the packet envelope if the body ever lacks options.
            context_budget: json!({
                "num_predict": out.request_body.as_ref().and_then(|b| b.pointer("/options/num_predict"))
                    .and_then(|v| v.as_i64()).unwrap_or(NARRATIVES_NUM_PREDICT_PACKET as i64),
                "num_ctx": out.request_body.as_ref().and_then(|b| b.pointer("/options/num_ctx"))
                    .and_then(|v| v.as_i64()).unwrap_or(crate::route::VOICE_NUM_CTX_PACKET as i64),
                "eval_count": out.eval_count,
                "wall_ms": out.wall_ms,
            }),
            parser_outcome: narratives_parser_outcome(out).to_string(),
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

        // Load material, gate, THEN build (Phase 2 refines Phase 1's build-once-then-gate):
        // narratives was the heaviest GPU stage and regenerated unconditionally every wake
        // cycle. When the material inputs (vetted corpus ids + heat facts) match the latest
        // persisted generation's hash, skip the dedup embed, the model call, AND the insert —
        // readers use max(generated_at), so the previous generation keeps serving, and
        // downstream vibe/sigil see no phantom "new" input. Pre-145 rows carry a NULL hash,
        // which never matches → one regeneration stamps it. The hash keys on the pre-dedup
        // material, so gating before the candle pass changes no debounce semantics.
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

        // Go marshals the nil trigger map → jsonb `null`.
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

        // Phase 3 hand-off: narratives now feeds Vibe (mirroring vibe → momentum). Vibe reads this
        // generation's storylines + the transfer heat, so enqueue it once that material has moved.
        // (The scrub `vetted` trigger no longer enqueues vibe — mig 174.) Transfers routing rides
        // the Editor's `news_articles.bucket` write (from `story_type`, n16) + the mig 175
        // trigger — no bucket write happens in this junction any more.
        //
        // **THE JOURNALIST DOES NOT WAKE THE INFLUENCER (7.6/E3).** She is woken by the packet's
        // `charged` tag through mig 206's subscription fan-out, and may file BEFORE this handler
        // ever runs. An enqueue here would put two writers on one `pipeline_work` row with
        // different `input_version` prefixes (`vibe:` here, `pk:` from the trigger), and
        // `work::enqueue` reopens on any version change — the mig-197 churn loop through a third
        // door. **One waker.** The legacy arm that called `enqueue_vibe_if_needed` was removed
        // with the rail in Phase 9; this comment is what remains of it, because the reason it must
        // not come back is the part worth keeping.

        Ok(())
    }
}

#[cfg(test)]
mod tests;
