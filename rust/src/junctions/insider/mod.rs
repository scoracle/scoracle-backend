//! Transfers stage — team-keyed transfer/trade rumor vetting.
//!
//! Composition (Plan §1.2 + §4): per (team, player) PAIR this composes `extract+validate`
//! (fail-closed Option<bool> is_rumor, JSON mode), the subject same-person test, and the persist.
//! The deterministic parts stay where they belong: `compute_transfer_heat`, the `direction`, and
//! the team relationship are SQL/Postgres (the model never computes the number or the direction);
//! the model ONLY vets: is this a live rumor about THIS exact player, what stage, and a grounded
//! one-line summary. The subject same-person test is realised as the verdict's `subject` field plus
//! the identity-card framing in the system prompt (the model returns is_rumor AND subject in ONE
//! JSON). A standalone embedding-backed single-candidate resolve for transfers
//! was once sketched as a HORIZON refinement (`resolve_one`) — it would restructure the one fused
//! call into two and weaken the fail-closed contract; the unused code was deleted (flow-friction
//! Phase 4), and the idea lives in git history should it ever be wanted.
//!
//! FAIL CLOSED (the §1.2 invariant): `is_rumor: Option<bool>` — a model timeout, unparseable output,
//! or a verdict that never committed to is_rumor persists an UNKNOWN row (is_rumor NULL), which is
//! NEVER served (every read requires `is_rumor IS TRUE`) and is counted so the team's stage item is
//! re-enqueued for a retry. Only a successful POSITIVE verdict ever becomes a served rumor.
//!
//! DEBOUNCE (F3, flow-friction plan 2026-07-12): before each pair's model call the production
//! handler fingerprints the pair's MATERIAL inputs — sorted pair-corpus article ids, the
//! corpus-stable source diversity, and the deterministic relationship; no timestamps, no prose,
//! no recency decay — and skips the GPU call, the insert,
//! and the ledger row when the pair's latest RESOLVED `transfer_rumors` row carries the same
//! `input_hash` (mig 145 reserved the column). UNKNOWN markers never satisfy the gate, so a
//! model-failure retry re-vets ONLY the failed pair: the completed pairs skip on fingerprint
//! instead of burning ~39 redundant GPU calls per team retry. An unchanged pair keeps its previous
//! row serving, which then cools off naturally with its sources — the source-freshness doctrine
//! working, per the 2026-07-12 plan.
//!
//! THE t11 PROMPT keeps the L9 false-heat fixes (roundups are not rumors; never invent a fee or stage)
//! but rewrites the instructions as schema-first rules for smaller local models.

use crate::corpus::{load_transfer_heat, HeatItem};
use crate::harness::{EntityKey, Harness, Parser};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::trajectory::{classify_delta, DEFAULT_TRAJECTORY};
use crate::util::{go_json_string, hash_components, truncate_bytes};
use crate::work::{Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

// This junction's contract with its model — system prompt, contract version, and prompt
// builder — lives in `prompt.rs`, so a change to what this character is asked is a one-file
// diff. Re-exported here so call sites and the ledger keep reading it from the stage module.
pub mod prompt;
pub use prompt::{INSIDER_SCORE_PROMPT_VERSION, INSIDER_SCORE_SYSTEM_PROMPT, TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION, TRANSFER_PROMPT_VERSION, build_insider_score_prompt, build_transfer_identity_adjudication_prompt, build_transfer_prompt, insider_score_format_schema, transfer_identity_adjudication_system_prompt, transfer_system_prompt};

/// Output schema version for transfer adjudication JSON, distinct from the prompt contract.
pub const TRANSFER_OUTPUT_CONTRACT_VERSION: &str = "transfer-verdict-v1";

/// Production vetting temperature.
pub const TRANSFER_TEMPERATURE: f64 = 0.3;

/// Token cap for the JSON verdict.
pub const TRANSFER_NUM_PREDICT: i32 = 900;

/// Corpus + candidate governors.
const TRANSFER_MAX_CORPUS_NEWS: i64 = 12;
/// Default candidate pre-filter (min co-mention articles / 14d).
pub const TRANSFER_DEFAULT_MIN_ARTICLES: i32 = 2;
const TRANSFER_MAX_CANDIDATES: i32 = 40;

// --- Self-pacing against the worker's per-item ceiling -----------------------------------------
//
// This is the only stage whose wall clock is a queue depth rather than one generation: a team item
// is one Mac call per candidate pair, then one more per wire-wrap target. At the 40-candidate cap
// that is ~80 sequential generations, which does not fit inside a 1200s ceiling even on an idle
// GPU — and queueing behind the other five voices is what actually pushed 18 teams over it on
// 2026-07-27.
//
// Being cancelled at the ceiling is not merely slow, it is *biased*: the wrap runs last, so the
// half that never ran was always the same half. Those teams kept producing rumors while their
// `insider_scores` went two days stale. So the budget is split rather than spent first-come —
// the pair loop gets half, the wrap is guaranteed the rest, and whatever is left over is deferred
// to another turn instead of being thrown away.

/// Fraction of the run's budget the pair loop may spend before it stops and defers the remainder.
const TRANSFER_PAIR_BUDGET_FRAC: f64 = 0.50;
/// Fraction at which the wire wrap stops too. The gap below 1.0 is headroom for the autofill
/// refresh and the bookkeeping that follow — this handler must land inside the ceiling, not race
/// it to the line.
const TRANSFER_WRAP_BUDGET_FRAC: f64 = 0.85;
/// How long a deferred team waits before it is claimable again. Claims order by `available_at`, so
/// this puts a big team behind the other pending teams rather than letting it immediately re-take
/// the stage's single in-flight slot and monopolise it round after round.
const TRANSFER_DEFER_DELAY: Duration = Duration::from_secs(60);

/// budget_deadline is the instant at which `frac` of the run's budget is spent, or `None` when the
/// budget is unbounded (`Duration::ZERO` — eval and the one-shot binaries). `None` is what makes an
/// inspection run drive a team to completion however long it takes.
fn budget_deadline(start: Instant, budget: Duration, frac: f64) -> Option<Instant> {
    if budget.is_zero() {
        return None;
    }
    Some(start + budget.mul_f64(frac))
}

/// past reports whether a deadline exists and has arrived. An absent deadline is never past.
fn past(deadline: Option<Instant>) -> bool {
    matches!(deadline, Some(d) if Instant::now() >= d)
}
/// Summary and news-description clip sizes for prompt budget control.
const SUMMARY_TRUNCATE: usize = 240;
const DESC_TRUNCATE: usize = 160;

/// One co-mention candidate player for a team + its identity-card disambiguators. Mirrors
/// `transferCandidate`.
#[derive(Clone, Debug)]
pub struct TransferCandidate {
    pub player_id: i32,
    pub player_name: String,
    pub nationality: String,  // empty when unknown
    pub current_club: String, // canonical current club (player_current_identity)
    pub position: String,
}

/// One corpus news item for the (team, player) pair. Mirrors `newsItem` (the prompt uses
/// title/description/source; the SQL orders by published_at — not needed in Rust).
#[derive(Clone, Debug)]
pub struct NewsItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub source: String,
}

/// The model's JSON verdict (defensively parsed) — the `T` in `Parser<T>`. `is_rumor: Option<bool>`
/// is the fail-closed carrier (Plan §1.2): `None` ⇒ the model never committed ⇒ the UNKNOWN marker,
/// unrepresentable as a served row. Mirrors `transferVerdict`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct TransferVerdict {
    pub is_rumor: Option<bool>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub subject: String, // who the sources are really about (audit trail for discarded impostors)
    #[serde(default, deserialize_with = "null_as_default")]
    pub direction: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub stage: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub summary: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub confidence: f64,
}

fn null_as_default<'de, D, T>(deserializer: D) -> std::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Clone, Debug)]
struct TransferIdentityThreshold {
    min_heat: i16,
    min_deterministic_confidence: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferIdentityAdjudication {
    pub decision: String,
    pub event_type: String,
    #[serde(default)]
    pub confidence: Option<f64>,
    pub old_team_id: Option<i32>,
    pub new_team_id: i32,
    pub reason: String,
    pub evidence_spans: Vec<String>,
}

/// The adjudication reply as a grammar (D-T43): field order matches the contract stated in
/// `transfer_identity_adjudication_system_prompt`, and the enums/bounds live here where they
/// are free instead of in prose. Added 2026-08-18: on free `json_mode` the 3b failed the shape
/// 4/5 times after the seat moved off gemma (Aug 17 `failed_closed` run) — the same model emits
/// the Editor's far larger contract reliably because a schema constrains it. RAW literal, not
/// `json!`, for the same reason as `EDITOR_FORMAT_SCHEMA_RAW`: a `Value` schema reaches Ollama
/// alphabetized.
pub const TRANSFER_IDENTITY_ADJUDICATION_SCHEMA_RAW: &str = r#"{
    "type": "object",
    "properties": {
        "decision": { "type": "string", "enum": ["apply", "reject"] },
        "event_type": { "type": "string", "enum": [
            "transfer", "trade", "loan", "signing", "extension", "rumor", "false_positive"
        ] },
        "old_team_id": { "type": ["integer", "null"] },
        "new_team_id": { "type": "integer" },
        "reason": { "type": "string", "maxLength": 500 },
        "evidence_spans": { "type": "array", "items": { "type": "string", "maxLength": 200 }, "maxItems": 6 }
    },
    "required": ["decision", "event_type", "old_team_id", "new_team_id", "reason", "evidence_spans"]
}"#;

pub struct TransferIdentityAdjudicationParser;

impl Parser<TransferIdentityAdjudication> for TransferIdentityAdjudicationParser {
    fn parse(&self, raw: &str) -> Result<Option<TransferIdentityAdjudication>> {
        let (start, end) = match (raw.find('{'), raw.rfind('}')) {
            (Some(s), Some(e)) if e > s => (s, e),
            _ => return Ok(None),
        };
        let value: serde_json::Value = match serde_json::from_str(&raw[start..=end]) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };
        let Some(obj) = value.as_object() else {
            return Ok(None);
        };
        for key in [
            "decision",
            "event_type",
            "old_team_id",
            "new_team_id",
            "reason",
            "evidence_spans",
        ] {
            if !obj.contains_key(key) {
                return Ok(None);
            }
        }
        let adj: TransferIdentityAdjudication = match serde_json::from_value(value) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };
        if !matches!(adj.decision.as_str(), "apply" | "reject") {
            return Ok(None);
        }
        if !matches!(
            adj.event_type.as_str(),
            "transfer" | "trade" | "loan" | "signing" | "extension" | "rumor" | "false_positive"
        ) {
            return Ok(None);
        }
        if adj
            .confidence
            .is_some_and(|confidence| !(0.0..=1.0).contains(&confidence))
        {
            return Ok(None);
        }
        Ok(Some(adj))
    }
}

/// TransferParser turns the model's JSON reply into a `TransferVerdict`. Fail-closed (`Ok(None)`)
/// only when there is NO JSON object at all or it is unparseable (Go's `!ok` path); a parsed verdict
/// whose `is_rumor` is absent surfaces as `Some(verdict)` with `is_rumor == None`, and the caller
/// routes THAT to the same UNKNOWN marker (Go's `verdict.IsRumor == nil` check). With JSON mode the
/// reply is already JSON; the first-`{`…last-`}` slice defends against any wrapping, mirroring the Go
/// `jsonObjectRE`.
pub struct TransferParser;

impl Parser<TransferVerdict> for TransferParser {
    fn parse(&self, raw: &str) -> Result<Option<TransferVerdict>> {
        let (start, end) = match (raw.find('{'), raw.rfind('}')) {
            (Some(s), Some(e)) if e > s => (s, e),
            _ => return Ok(None), // no JSON object → fail-closed UNKNOWN
        };
        match serde_json::from_str::<TransferVerdict>(&raw[start..=end]) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None), // unparseable → fail-closed UNKNOWN
        }
    }
}

/// How a vetted pair was classified. Drives the per-team tally and the fail-closed retry
/// (any `Unknown` fails the team's stage item).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Rumor,   // is_rumor TRUE — a vetted, served rumor
    Cleared, // is_rumor FALSE — roster/match-report/roundup noise (hidden by the read filter)
    Unknown, // is_rumor NULL — model failure (timeout/unparseable/no-commit); fail-closed, retryable
    Skipped, // no corpus (heat NULL), or unchanged material (the F3 fingerprint gate) — no row written
}

/// The persistable columns derived from a verdict after the deterministic gates.
#[derive(Clone, Debug)]
pub struct TransferRow {
    pub is_rumor: Option<bool>,
    pub direction: Option<String>,
    pub stage: Option<String>,
    pub summary: Option<String>,
    pub attribution: Option<String>,
    pub confidence: Option<f64>,
    pub model: Option<String>,
    pub trigger_payload: String, // JSON text ("{}" or {"subject": …})
}

/// The un-persisted result of vetting one (team, player) pair. The production handler persists
/// the served product row, and the ledger records the prompt/request/evidence envelope.
#[derive(Clone, Debug)]
pub struct TransferPairOutput {
    pub player_id: i32,
    pub model: String,
    pub heat: Option<i16>,
    pub components: String, // heat_components jsonb text
    /// All pair corpus ids returned by compute_transfer_heat before prompt capping.
    pub news_ids: Vec<i64>,
    /// The subset of pair news ids actually rendered into the transfer prompt.
    pub prompted_news_ids: Vec<i64>,
    /// Pair corpus rows excluded by compute_transfer_heat's 14-day freshness boundary.
    pub stale_news_ids: Vec<i64>,
    pub outcome: Outcome,
    /// `None` ⇒ Skipped (no corpus → no row); `Some` for Rumor/Cleared/Unknown.
    pub row: Option<TransferRow>,
    /// The exact user prompt sent (the deterministic parity axis). `None` for Skipped (no call).
    pub built_prompt: Option<String>,
    /// The exact /api/generate wire body (captured by `extract`). `None` for Skipped.
    pub request_body: Option<serde_json::Value>,
    /// Tokens evaluated by Ollama for this call. `None` when no model result was returned.
    pub eval_count: Option<i32>,
    pub wall_ms: Option<u64>,
    /// Evidence retained for the optional post-persist identity adjudication gate. Empty for
    /// skipped/no-corpus pairs.
    pub identity_apply_news: Vec<NewsItem>,
    pub prompt_version: &'static str,
    /// The F3 per-pair debounce fingerprint (persisted on every row, resolved or UNKNOWN).
    /// `None` only for the no-corpus Skipped path (no row to stamp).
    pub input_hash: Option<String>,
}

// ---------------------------------------------------------------------------
// Loaders — byte-for-byte the SQL transfer.go runs (same query ⇒ same rows).
// ---------------------------------------------------------------------------

/// load_candidates returns the team's co-mention candidate players with identity cards — the Rust
/// port of `transfer.go::loadCandidates` (current club from `player_current_identity`; both vetted
/// links required).
///
/// The mig-033 title-proximity gate is GONE (PLAN-one-rail 8.8). It required the team and the
/// player to appear within 50 title characters of each other, and it was a crutch for a regex that
/// scanned headlines: on the packet rail both links come from the Editor having READ the article,
/// and the Editor writes no `title_pos`, so every post-flip pair passed the gate anyway (NULL was
/// the lenient sentinel). Removing it is a no-op for new rows and drops the thinning for the
/// pre-flip tail that still carries positions. What thins the set now is
/// `HAVING count(DISTINCT te.article_id) >= $3` — corroboration across articles, which is the
/// better filter. Replacing proximity with the Editor's `entity_roles` is D-T20
/// (PLAN-character-tuning.md §7a), NOT settled here.
pub async fn load_candidates(
    pool: &PgPool,
    team_id: i32,
    sport: &str,
    min_articles: i32,
) -> Result<Vec<TransferCandidate>> {
    let rows = sqlx::query(
        r#"
        SELECT pe.entity_id, p.name,
               COALESCE(p.nationality, '')                    AS nationality,
               COALESCE(ct.name, '')                          AS current_club,
               COALESCE(NULLIF(pci.position, 'Unknown'), '')  AS position
        FROM news_article_entities te
        JOIN news_article_entities pe
          ON pe.article_id = te.article_id AND pe.sport = te.sport AND pe.entity_type = 'player'
        JOIN news_articles a ON a.id = te.article_id
        JOIN players p ON p.id = pe.entity_id AND p.sport = pe.sport
        LEFT JOIN public.player_current_identity pci ON pci.player_id = p.id AND pci.sport = p.sport
        LEFT JOIN teams ct ON ct.id = pci.team_id AND ct.sport = p.sport
        WHERE te.entity_type = 'team' AND te.entity_id = $1 AND te.sport = $2
          AND a.bucket IS DISTINCT FROM 'non_transfer'
          AND te.created_at > NOW() - INTERVAL '14 days'
        GROUP BY pe.entity_id, p.name, p.nationality, ct.name, pci.position
        HAVING count(DISTINCT te.article_id) >= $3
        ORDER BY max(a.topic_heat) DESC NULLS LAST, count(DISTINCT te.article_id) DESC
        LIMIT $4
        "#,
    )
    .bind(team_id)
    .bind(sport)
    .bind(min_articles)
    .bind(TRANSFER_MAX_CANDIDATES)
    .fetch_all(pool)
    .await
    .context("load candidates")?;

    Ok(rows
        .iter()
        .map(|r| TransferCandidate {
            player_id: r.get("entity_id"),
            player_name: r.get("name"),
            nationality: r.get("nationality"),
            current_club: r.get("current_club"),
            position: r.get("position"),
        })
        .collect())
}

/// compute_pair_heat calls the deterministic `compute_transfer_heat` SQL function (migration 032 —
/// the number stays in Postgres, NEVER the model's). Returns (heat, components-jsonb-text, news_ids);
/// `heat` is `None` when there is no pair corpus (the Skipped short-circuit). Mirrors the
/// `analyzePair` opening query. `components::text` + COALESCE keep the scan null-safe.
pub async fn compute_pair_heat(
    pool: &PgPool,
    team_id: i32,
    player_id: i32,
    sport: &str,
) -> Result<(Option<i16>, String, Vec<i64>)> {
    let row = sqlx::query(
        "SELECT heat, COALESCE(components::text, '{}') AS components, news_ids \
         FROM compute_transfer_heat($1, $2, $3)",
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .fetch_one(pool)
    .await
    .context("compute transfer heat")?;
    let heat: Option<i16> = row.get("heat");
    let components: String = row.get("components");
    let news_ids: Option<Vec<i64>> = row.get("news_ids");
    Ok((heat, components, news_ids.unwrap_or_default()))
}

/// load_pair_news returns the pair's corpus headlines, newest first, capped — the model's grounding.
/// Mirrors `loadPairNews`. `published_at` stays in the ORDER BY (not selected; unused in Rust), so
/// the news order — hence the prompt — is identical to Go.
pub async fn load_pair_news(pool: &PgPool, ids: &[i64]) -> Result<Vec<NewsItem>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        r#"
        SELECT id, title, COALESCE(description, '') AS description, COALESCE(source, '') AS source
        FROM news_articles WHERE id = ANY($1)
        ORDER BY published_at DESC NULLS LAST LIMIT $2
        "#,
    )
    .bind(ids)
    .bind(TRANSFER_MAX_CORPUS_NEWS)
    .fetch_all(pool)
    .await
    .context("load pair news")?;
    Ok(rows
        .iter()
        .map(|r| NewsItem {
            id: r.get("id"),
            title: r.get("title"),
            description: r.get("description"),
            source: r.get("source"),
        })
        .collect())
}

/// How many of the team's live packets may contribute material to one pair's prompt. A pair's
/// articles rarely span more than one storyline; the cap is a ceiling on a pathological team, not
/// a tuning knob.
const PAIR_PACKET_LIMIT: i64 = 5;

/// The packet material for one pair (7.5), keyed by article id: this article's transfer-typed
/// claims, in the packet's order (newest first), contested ones already marked.
type PairPacketFacts = HashMap<i64, Vec<String>>;

/// load_pair_packet_material is the Insider's half of the cutover (7.5): the same pair, read off
/// the compiled storyline instead of the raw article window.
///
/// **What the packet replaces, and what it does NOT.** The pair's IDENTITY stays Postgres's:
/// `compute_transfer_heat` decides the heat, the corpus ids, and therefore the F3 fingerprint —
/// the number is never the model's and never the packet's (§4). What the packet replaces is the
/// MATERIAL: for every article already in the pair's corpus, the Editor's extracted transfer
/// claims stand in for the RSS headline and description. Same articles, same order, same
/// grounding — better evidence. That is the same shape 7.3 gave the Journalist, and it is why the
/// whole adjudication chain below this function is untouched.
///
/// The Insider's slice is the transfer-typed claims — exactly the subset
/// `slice_fingerprints ->> 'transfers'` hashes (E2), so a re-fan and a re-read agree about what
/// moved. Articles the Desk has not assembled (or whose claims are another type) simply carry no
/// overlay and travel on their headline, which is what a headline is for.
///
/// Returns the per-article facts plus the storyline framing of the packets that actually
/// contributed — a packet whose claims all missed this pair frames nothing.
async fn load_pair_packet_material(
    pool: &PgPool,
    team_id: i32,
    team_name: &str,
    sport: &str,
    news_ids: &[i64],
) -> Result<(PairPacketFacts, String)> {
    use crate::junctions::editor::render;

    let mut facts: PairPacketFacts = HashMap::new();
    let mut framing = String::new();
    if news_ids.is_empty() {
        return Ok((facts, framing));
    }
    let wanted: std::collections::HashSet<i64> = news_ids.iter().copied().collect();

    let loaded = crate::junctions::editor::packet::load_packets_for_entity(
        pool,
        "team",
        team_id,
        sport,
        crate::junctions::journalist::PACKET_LOOKBACK_HOURS,
        PAIR_PACKET_LIMIT,
    )
    .await?;

    for (view, mut part) in loaded {
        part.name = team_name.to_string();
        let slice = render::slice_claims(&view.claims, render::Voice::Insider);
        let marked = render::mark_contested(&slice);
        let mut contributed = false;
        for m in marked {
            if !wanted.contains(&m.claim.article_id) {
                continue;
            }
            contributed = true;
            let fact = if m.marked {
                // The contradiction survives into the wire's own prompt (T3/D6). The Insider is
                // the one voice whose whole job is staging a contested claim, so the marker
                // matters most here: "agreement in principle" beside "deal not agreed" is a
                // reason to hold the stage down, not noise to resolve away.
                format!("⇄ {}", m.claim.fact)
            } else {
                m.claim.fact.clone()
            };
            facts.entry(m.claim.article_id).or_default().push(fact);
        }
        if contributed {
            if !framing.is_empty() {
                framing.push('\n');
            }
            framing.push_str(&render::framing(&view, Some(&part), render::Voice::Insider));
        }
    }
    Ok((facts, framing))
}

async fn load_stale_pair_news_ids(
    pool: &PgPool,
    team_id: i32,
    player_id: i32,
    sport: &str,
) -> Result<Vec<i64>> {
    let ids = sqlx::query_scalar(
        r#"
        SELECT DISTINCT a.id
        FROM news_articles a
        JOIN news_article_entities te ON te.article_id = a.id AND te.entity_type = 'team'
             AND te.entity_id = $1 AND te.sport = $3
        JOIN news_article_entities pe ON pe.article_id = a.id AND pe.entity_type = 'player'
             AND pe.entity_id = $2 AND pe.sport = $3
        WHERE a.bucket IS DISTINCT FROM 'non_transfer'
          AND a.published_at <= NOW() - INTERVAL '14 days'
        ORDER BY a.id
        "#,
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .fetch_all(pool)
    .await
    .context("load stale pair news ids")?;
    Ok(ids)
}

/// team_relationships batches [`team_relationship`] over one team's whole candidate set — one
/// round trip instead of one query per pair (Phase 2). The correlated subqueries are the same
/// ones the per-pair read runs, so batch and per-pair MUST agree: the relationship is part of
/// the F3 fingerprint.
pub async fn team_relationships(
    pool: &PgPool,
    team_id: i32,
    player_ids: &[i32],
    sport: &str,
) -> Result<HashMap<i32, String>> {
    let rows = sqlx::query(
        r#"
        SELECT p.pid,
               COALESCE((SELECT pci.team_id = $3
                         FROM public.player_current_identity pci
                         WHERE pci.player_id = p.pid AND pci.sport = $2), false) AS is_current,
               COALESCE((SELECT bool_or(ps.team_id = $3)
                         FROM player_stats ps
                         WHERE ps.player_id = p.pid AND ps.sport = $2), false) AS is_ever
        FROM unnest($1::int4[]) AS p(pid)
        "#,
    )
    .bind(player_ids)
    .bind(sport)
    .bind(team_id)
    .fetch_all(pool)
    .await
    .context("team relationships (batch)")?;
    Ok(rows
        .iter()
        .map(|r| {
            let pid: i32 = r.get("pid");
            let is_current: bool = r.get("is_current");
            let is_ever: bool = r.get("is_ever");
            let rel = if is_current {
                "current"
            } else if is_ever {
                "former"
            } else {
                "none"
            };
            (pid, rel.to_string())
        })
        .collect())
}

/// team_relationship classifies the player's deterministic relationship to the team:
/// "current" comes from canonical current identity, while "former" comes from
/// historical player_stats. Drives `direction` and the former-player noise filter — NOT the model's
/// guess. `$1=player, $2=sport, $3=team`.
pub async fn team_relationship(
    pool: &PgPool,
    team_id: i32,
    player_id: i32,
    sport: &str,
) -> Result<String> {
    let row = sqlx::query(
        r#"
        SELECT
            COALESCE((SELECT pci.team_id = $3
                      FROM public.player_current_identity pci
                      WHERE pci.player_id = $1 AND pci.sport = $2), false) AS is_current,
            COALESCE((SELECT bool_or(ps.team_id = $3)
                      FROM player_stats ps
                      WHERE ps.player_id = $1 AND ps.sport = $2), false) AS is_ever
        "#,
    )
    .bind(player_id)
    .bind(sport)
    .bind(team_id)
    .fetch_one(pool)
    .await
    .context("team relationship")?;
    let is_current: bool = row.get("is_current");
    let is_ever: bool = row.get("is_ever");
    Ok(if is_current {
        "current".to_string()
    } else if is_ever {
        "former".to_string()
    } else {
        "none".to_string()
    })
}

/// direction_for maps the relationship to the rumor direction: a current player can only be leaving
/// (outgoing); everyone else would be arriving (incoming). Deterministic — mirrors `directionFor`.
pub fn direction_for(relationship: &str) -> &'static str {
    if relationship == "current" {
        "outgoing"
    } else {
        "incoming"
    }
}

/// primary_source returns the first attributed source in the prompt corpus.
fn primary_source(news: &[NewsItem]) -> String {
    for n in news {
        if !n.source.is_empty() {
            return n.source.clone();
        }
    }
    String::new()
}

/// return_signals: phrases indicating a genuine RETURN move (vs a former player merely mentioned).
/// Mirrors `returnSignals`.
const RETURN_SIGNALS: &[&str] = &[
    "return to",
    "returning to",
    "rejoin",
    "re-sign",
    "resign for",
    "back to",
    "back at",
    "comeback",
    "second spell",
    "reunite",
    "bring back",
    "brings back",
    "volver a",
    "regresar a",
    "vuelve a",
    "regresa a",
    "retour a",
    "retour à",
    "retour au",
    "revient a",
    "revient à",
    "rejoindre",
    "ruckkehr zu",
    "rückkehr zu",
    "kehrt zuruck",
    "kehrt zurück",
    "zuruck zu",
    "zurück zu",
    "ritorno a",
    "torna a",
    "tornare a",
    "regresso ao",
    "retorno ao",
    "volta ao",
    "voltar ao",
    "terugkeer naar",
    "keert terug",
    "terug naar",
];

/// has_return_signal reports whether the pair corpus contains return-move language (lower-cased
/// substring match over title + description). Mirrors `hasReturnSignal`.
fn has_return_signal(news: &[NewsItem]) -> bool {
    let contains = |s: &str| {
        let l = s.to_lowercase();
        RETURN_SIGNALS.iter().any(|kw| l.contains(kw))
    };
    news.iter()
        .any(|n| contains(&n.title) || contains(&n.description))
}

/// build_transfer_prompt assembles the user prompt. It was historically byte-identical to Go's
/// `buildTransferPrompt` (the temp-0 parity axis); the Go transfer STAGE has since been retired
/// (all derivation runs in the Rust harness — see `go/cmd/pipeline/main.go`), so this is now the
/// sole implementation and free to evolve. The byte-fixture tests below still pin the assembly so
/// prompt drift stays deliberate. The "·" separator (U+00B7) and the "—" (U+2014) are significant
/// bytes; at temp 0 a single changed byte changes the model's output.
///
/// The cards, in order: TransferEvidence (Phase 2, t7) — corpus size and source diversity;
/// then the source-reliability card (Phase 4, t9,
/// `source_reliability_for_pair`) — the MEASURED track record of those same sources; then the
/// relational memory card (t8) — the pair's story arc. Evidence is "how much corpus exists";
/// reliability/memory are organic history rather than static source tiers.
#[derive(Clone, Debug, Default)]
pub struct TransferEvidence {
    pub total_articles: usize,
    pub distinct_sources: usize,
    pub best_source: String,
}

impl TransferEvidence {
    pub fn from_news(news: &[NewsItem], total_articles: usize, best: &str) -> Self {
        let distinct_sources = news
            .iter()
            .map(|n| n.source.to_lowercase())
            .filter(|s| !s.is_empty())
            .collect::<std::collections::HashSet<_>>()
            .len();
        Self {
            total_articles,
            distinct_sources,
            best_source: best.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Verdict normalization — mirrors normStage / clampConf.
// ---------------------------------------------------------------------------

const VALID_STAGES: &[&str] = &[
    "speculation",
    "concrete_interest",
    "advanced_talks",
    "here_we_go",
];

/// norm_stage lower-cases + underscores the model's stage, defaulting to "speculation" for any
/// out-of-vocabulary value. Mirrors `normStage`.
fn norm_stage(s: &str) -> String {
    let n = s.trim().replace(' ', "_").to_lowercase();
    if VALID_STAGES.contains(&n.as_str()) {
        n
    } else {
        "speculation".to_string()
    }
}

/// clamp_conf bounds confidence to [0, 1]. Mirrors `clampConf`.
fn clamp_conf(c: f64) -> f64 {
    c.clamp(0.0, 1.0)
}

/// row_from_verdict builds the persistable columns from a (post-gate) verdict, mirroring the
/// branching in `transfer.go::persist`:
///
///   * `verdict == None`        → UNKNOWN: is_rumor NULL, direction kept (audit), model NULL.
///   * is_rumor == Some(true)   → a vetted rumor: direction/stage/summary/confidence/model set.
///   * is_rumor == Some(false)  → cleared: is_rumor FALSE + model set, the rest left NULL.
///
/// `model_configured` is the role's configured model (matches Go's `g.ollama.Model()`).
fn row_from_verdict(
    verdict: Option<&TransferVerdict>,
    relationship: &str,
    attribution: Option<&str>,
    model_configured: &str,
) -> (TransferRow, Outcome) {
    let attr = attribution.filter(|a| !a.is_empty()).map(str::to_string);

    // Audit trail: stash who the model judged the sources to be about (even for a discarded
    // impostor), so a cleared same-name link leaves a record. trigger_payload NOT NULL.
    let trigger_payload = verdict
        .map(|v| v.subject.trim())
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::json!({ "subject": s }).to_string())
        .unwrap_or_else(|| "{}".to_string());

    match verdict {
        None => (
            // Model failure → leave is_rumor NULL (unknown). Keep the deterministic direction for
            // the audit row; the read path never surfaces it.
            TransferRow {
                is_rumor: None,
                direction: Some(direction_for(relationship).to_string()),
                stage: None,
                summary: None,
                attribution: attr,
                confidence: None,
                model: None,
                trigger_payload,
            },
            Outcome::Unknown,
        ),
        Some(v) => match v.is_rumor {
            None => (
                // Parsed but never committed to is_rumor → same UNKNOWN marker as no-verdict.
                TransferRow {
                    is_rumor: None,
                    direction: Some(direction_for(relationship).to_string()),
                    stage: None,
                    summary: None,
                    attribution: attr,
                    confidence: None,
                    model: None,
                    trigger_payload,
                },
                Outcome::Unknown,
            ),
            Some(true) => {
                let summary = {
                    let s = v.summary.trim();
                    (!s.is_empty()).then(|| truncate_bytes(s, SUMMARY_TRUNCATE))
                };
                (
                    TransferRow {
                        is_rumor: Some(true),
                        direction: Some(direction_for(relationship).to_string()),
                        stage: Some(norm_stage(&v.stage)),
                        summary,
                        attribution: attr,
                        confidence: Some(clamp_conf(v.confidence)),
                        model: Some(model_configured.to_string()),
                        trigger_payload,
                    },
                    Outcome::Rumor,
                )
            }
            Some(false) => (
                TransferRow {
                    is_rumor: Some(false),
                    direction: None,
                    stage: None,
                    summary: None,
                    attribution: attr,
                    confidence: None,
                    model: Some(model_configured.to_string()),
                    trigger_payload,
                },
                Outcome::Cleared,
            ),
        },
    }
}

// ---------------------------------------------------------------------------
// The per-pair core + the production handler.
// ---------------------------------------------------------------------------

/// build_transfer_input_components is the canonical per-pair debounce pre-image (F3): the sorted
/// pair-corpus article ids (the corpus identity), the corpus-stable source diversity, and the deterministic
/// `relationship` (it drives `direction` and the former-player gate, so an identity flip must
/// re-vet even over a frozen corpus). Same canonical-JSON discipline as
/// `vibe::build_vibe_input_components`.
///
/// Deliberately EXCLUDED: the heat value and the `newest_age_hours`/`recency`/`recent_3d`/
/// `recent_frac` components — ALL are `NOW()`-derived decay that ticks while the corpus stands
/// still (the plan's no-timestamps rule: pure time decay must never re-run the GPU; cooling is
/// served by the source-freshness protocol and the read path's `generated_at` windows);
/// `total_14d`/`volume` (pure functions of the id set / `distinct_sources` — redundant); and the
/// article titles/descriptions/sources (prose — the ids are the identity; a headline edit is not
/// new material).
///
/// INCLUDED (Phase 4): `prompt_version` — so a prompt-contract bump (e.g. t8→t9) changes every
/// pair's fingerprint exactly once, forcing one regen each past the [`pair_unchanged`] debounce
/// even over a frozen corpus. Without it, a quiet pair would keep serving its pre-bump read and
/// never pick up the new card/instructions (the same debounce gap narratives closed at n9). The
/// source-reliability and relational-memory CARDS themselves stay OUT — they ride along when the
/// corpus moves, and their content is `NOW()`-derived measurement that must not tick the GPU.
pub fn build_transfer_input_components(
    news_ids: &[i64],
    heat_components_json: &str,
    relationship: &str,
) -> String {
    let comps: serde_json::Value =
        serde_json::from_str(heat_components_json).unwrap_or(serde_json::Value::Null);
    let distinct_sources = comps
        .get("distinct_sources")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let mut ids: Vec<i64> = news_ids.to_vec();
    ids.sort_unstable();
    let ids_csv = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    format!(
        "{{\"distinct_sources\":{distinct_sources},\"news_ids\":[{ids_csv}],\"prompt_version\":{},\"relationship\":{}}}",
        go_json_string(TRANSFER_PROMPT_VERSION),
        go_json_string(relationship),
    )
}

/// pair_unchanged is the F3 per-pair debounce read: `true` when the pair's LATEST transfer_rumors
/// row is a RESOLVED vetting (`is_rumor IS NOT NULL`) carrying this same `input_hash`. An UNKNOWN
/// marker (is_rumor NULL) never satisfies the gate — after a model failure the retried team item
/// must re-vet the failed pair (while the completed pairs skip on their stamped fingerprints).
/// Legacy rows carry NULL `input_hash`, which never matches ⇒ each pair regenerates once
/// post-deploy, then stamps. `idx_transfer_rumors_pair_recent` covers the read.
pub async fn pair_unchanged(
    pool: &PgPool,
    team_id: i32,
    player_id: i32,
    sport: &str,
    input_hash: &str,
) -> Result<bool> {
    let latest: Option<(Option<String>, Option<bool>)> = sqlx::query_as(
        "SELECT input_hash, is_rumor FROM transfer_rumors \
         WHERE team_id = $1 AND player_id = $2 AND sport = $3 \
         ORDER BY generated_at DESC LIMIT 1",
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("transfer debounce check {team_id}/{player_id}"))?;
    Ok(matches!(latest, Some((Some(h), Some(_))) if h == input_hash))
}

/// PairBuild is the deterministic prefix of `analyze_pair`: everything computed before the model
/// call (heat -> corpus -> relationship -> prompt -> request body). Shared by production and eval
/// paths so inspection artifacts match what production sends. `Skipped` means no pair corpus
/// (heat NULL), so no model call and no row.
pub enum PairBuild {
    Skipped {
        components: String,
        news_ids: Vec<i64>,
    },
    Ready(Box<PairReady>),
}

/// PairReady carries the assembled model inputs plus the deterministic context the
/// post-model gates need (`news` for the former-player return-signal, `relationship` for direction).
/// `request_body` is computed from the SAME backend + opts the
/// call will use, so it can never drift from what is POSTed.
pub struct PairReady {
    pub heat: i16,
    pub components: String,
    pub news_ids: Vec<i64>,
    pub prompted_news_ids: Vec<i64>,
    pub news: Vec<NewsItem>,
    pub relationship: String,
    pub attribution: String,
    pub opts: GenerateOptions,
    pub built_prompt: String,
    pub request_body: serde_json::Value,
    pub model_configured: String,
    /// The F3 per-pair debounce fingerprint over the material inputs
    /// (see [`build_transfer_input_components`]).
    pub input_hash: String,
}

/// load_relational_memory fetches the graph's computed memory card for the pair
/// (`narrative_context_for_pair`, mig 162): prior sealed stories with outcomes, the
/// current story's likelihood/trajectory, recent confirmed moves. `None` = the graph
/// holds no memory for the pair (the prompt renders no section). Model-facing
/// enrichment only — the relational layer is never user-exposed.
pub async fn load_relational_memory(
    pool: &PgPool,
    sport: &str,
    player_id: i32,
    team_id: i32,
) -> Result<Option<String>> {
    let row: (Option<String>,) = sqlx::query_as("SELECT narrative_context_for_pair($1, $2, $3)")
        .bind(sport)
        .bind(player_id)
        .bind(team_id)
        .fetch_one(pool)
        .await
        .context("narrative_context_for_pair")?;
    Ok(row.0)
}

/// load_source_reliability fetches the measured track record of the sources on the pair's
/// live transfer corpus (`source_reliability_for_pair`, mig 178, Phase 4): for each source in
/// the pair's current News headlines, its global per-sport `source_performance` record —
/// reliability N/100, confirmed/tracked base rate, early-call count. `None` = no corpus source
/// has a measured record (the prompt renders no section). Like [`load_relational_memory`], the
/// data + the source→record JOIN live in SQL (the data layer); Rust only SELECTs the finished
/// card and renders it. Model-facing enrichment only, never user-exposed; NOT part of the
/// input_hash (it rides along when the corpus changes, same as the relational memory card).
pub async fn load_source_reliability(
    pool: &PgPool,
    sport: &str,
    player_id: i32,
    team_id: i32,
) -> Result<Option<String>> {
    let row: (Option<String>,) = sqlx::query_as("SELECT source_reliability_for_pair($1, $2, $3)")
        .bind(sport)
        .bind(player_id)
        .bind(team_id)
        .fetch_one(pool)
        .await
        .context("source_reliability_for_pair")?;
    Ok(row.0)
}

/// build_pair_request runs the deterministic prefix: `compute_transfer_heat` (SQL — the number
/// stays Postgres), the pair corpus, then `build_transfer_prompt` with the t5 options and the
/// exact wire body. NO model call — these are the deterministic axes (the L2 finding: the
/// verdict is not a temp-0 parity axis, so the gate needs no GPU). The role is
/// [`Role::TransferLogic`] (The Insider's route seam). `relationship` is the deterministic
/// team relationship, loaded by the caller (the handler batches it per team via
/// [`team_relationships`]; offline paths use the per-pair [`team_relationship`]).
#[allow(clippy::too_many_arguments)]
pub async fn build_pair_request(
    hx: &Harness,
    team_id: i32,
    team_name: &str,
    c: &TransferCandidate,
    sport: &str,
    relationship: String,
    temperature: f64,
) -> Result<PairBuild> {
    let (heat, components, news_ids) =
        compute_pair_heat(&hx.pool, team_id, c.player_id, sport).await?;
    let Some(heat) = heat else {
        return Ok(PairBuild::Skipped {
            components,
            news_ids,
        });
    };

    let mut news = load_pair_news(&hx.pool, &news_ids).await?;
    // 7.5 — the rail decides what these articles SAY, never which articles they are. Under
    // legacy the items keep the headline+description the window query returned; under packet
    // the Editor's transfer claims overlay them, article by article. `prompted_news_ids`,
    // `news_ids`, the attribution and the fingerprint are computed from the same list either
    // way, so the debounce, the evidence card and the identity chain cannot tell the rails
    // apart — which is exactly the property that lets the flip be one env var.
    let packet_framing = {
        let (facts, framing) =
            load_pair_packet_material(&hx.pool, team_id, team_name, sport, &news_ids).await?;
        for n in news.iter_mut() {
            let Some(article_facts) = facts.get(&n.id) else {
                continue; // not assembled, or nothing transfer-typed in it — it keeps its headline
            };
            let mut it = article_facts.iter();
            if let Some(first) = it.next() {
                n.title = first.clone();
                n.description = it.cloned().collect::<Vec<_>>().join(" · ");
            }
        }
        Some(framing).filter(|f| !f.trim().is_empty())
    };
    let prompted_news_ids = news.iter().map(|n| n.id).collect();
    let attribution = primary_source(&news);

    // F3: fingerprint the material inputs now that they are all in hand (corpus ids + stable
    // heat components + relationship) — the handler gates on this BEFORE paying for the GPU.
    let input_hash = hash_components(&build_transfer_input_components(
        &news_ids,
        &components,
        &relationship,
    ));

    let evidence = TransferEvidence::from_news(&news, news_ids.len(), &attribution);
    // Two independent SQL card reads (story arc + source track record) — load concurrently.
    let (memory, source_reliability) = tokio::try_join!(
        load_relational_memory(&hx.pool, sport, c.player_id, team_id),
        load_source_reliability(&hx.pool, sport, c.player_id, team_id),
    )?;
    let built_prompt = build_transfer_prompt(
        team_name,
        c,
        sport,
        &relationship,
        &news,
        &evidence,
        source_reliability.as_deref(),
        memory.as_deref(),
        packet_framing.as_deref(),
    );
    let opts = GenerateOptions {
        system: Some(transfer_system_prompt(sport)),
        temperature: Some(temperature),
        num_predict: TRANSFER_NUM_PREDICT,
        num_ctx: hx.voice_num_ctx,
        json_mode: true,
        format_schema: None,
        format_schema_raw: None,
    };
    let backend = hx.router.for_role(Role::TransferLogic);
    let request_body = backend.request_body(&built_prompt, &opts);
    let model_configured = backend.model().to_string();

    Ok(PairBuild::Ready(Box::new(PairReady {
        heat,
        components,
        news_ids,
        prompted_news_ids,
        news,
        relationship,
        attribution,
        opts,
        built_prompt,
        request_body,
        model_configured,
        input_hash,
    })))
}

/// skipped_pair_output is the no-corpus result: heat NULL ⇒ no model call, no row
/// (Go: `res.Skipped++, return nil`). No fingerprint either — there is no row to stamp.
fn skipped_pair_output(
    hx: &Harness,
    player_id: i32,
    components: String,
    news_ids: Vec<i64>,
) -> TransferPairOutput {
    let model = hx.router.for_role(Role::TransferLogic).model().to_string();
    TransferPairOutput {
        player_id,
        model,
        heat: None,
        components,
        news_ids,
        prompted_news_ids: Vec::new(),
        stale_news_ids: Vec::new(),
        outcome: Outcome::Skipped, // no corpus → no row (Go: res.Skipped++, return nil)
        row: None,
        built_prompt: None,
        request_body: None,
        eval_count: None,
        wall_ms: None,
        identity_apply_news: Vec::new(),
        prompt_version: TRANSFER_PROMPT_VERSION,
        input_hash: None,
    }
}

/// analyze_pair runs the full vetting for one (team, player) pair at the given temperature and
/// returns the un-persisted result (the L11 composition `extract+validate + subject-test + persist`,
/// minus the persist) — `build_pair_request` (deterministic) then `vet_pair` (the model + the
/// gates). No debounce here: the F3 fingerprint gate lives in the production handler, between the
/// builder and `vet_pair` (the same split as vibe's `load_vibe_context`). A generate failure is
/// swallowed into an UNKNOWN output (the fail-closed marker), not propagated. Only a real
/// DB/transport error returns `Err`.
pub async fn analyze_pair(
    hx: &Harness,
    team_id: i32,
    team_name: &str,
    c: &TransferCandidate,
    sport: &str,
    temperature: f64,
) -> Result<TransferPairOutput> {
    // Offline composition loads the relationship per pair; the production handler batches it.
    let relationship = team_relationship(&hx.pool, team_id, c.player_id, sport).await?;
    match build_pair_request(hx, team_id, team_name, c, sport, relationship, temperature).await? {
        PairBuild::Skipped {
            components,
            news_ids,
        } => Ok(skipped_pair_output(hx, c.player_id, components, news_ids)),
        PairBuild::Ready(r) => vet_pair(hx, team_id, c.player_id, sport, *r).await,
    }
}

/// vet_pair is the MODEL half of `analyze_pair`: extract, the deterministic post-model gates, and
/// the row shaping, over an already-built [`PairReady`]. Split out so the production handler can
/// run the F3 fingerprint gate between `build_pair_request` and the GPU call.
pub async fn vet_pair(
    hx: &Harness,
    team_id: i32,
    player_id: i32,
    sport: &str,
    ready: PairReady,
) -> Result<TransferPairOutput> {
    // route(TransferLogic) + extract(TransferParser). A generate transport error → fail-closed
    // UNKNOWN row (Go persists UNKNOWN on a model timeout, then the team item is retried), recording
    // the prompt/body that WAS sent for the parity diff.
    //
    // The stale-pair-news read is audit-only (ledger excluded_evidence), so it rides alongside
    // the model call instead of the build path — a debounce-skipped pair never pays for it,
    // and a generating pair hides it under GPU latency (Phase 2).
    let (extract_result, stale_result) = tokio::join!(
        hx.extract(
            Role::TransferLogic,
            &ready.built_prompt,
            &ready.opts,
            &TransferParser,
        ),
        load_stale_pair_news_ids(&hx.pool, team_id, player_id, sport)
    );
    let stale_news_ids = stale_result?;
    let (verdict, model, built_prompt, request_body, eval_count, wall_ms) = match extract_result {
        Ok(extracted) => (
            extracted.value,
            extracted.model,
            Some(extracted.built_prompt),
            Some(extracted.request_body),
            Some(extracted.eval_count),
            Some(extracted.wall_ms),
        ),
        Err(e) => {
            warn!(team = team_id, player = player_id, error = %e, "transfers: model generate failed; UNKNOWN (fail-closed)");
            (
                None,
                ready.model_configured.clone(),
                Some(ready.built_prompt.clone()),
                Some(ready.request_body.clone()),
                None,
                None,
            )
        }
    };

    // Apply the deterministic gates to a working copy of the verdict (only when committed-positive).
    let mut verdict = verdict;
    if let Some(v) = verdict.as_mut() {
        if v.is_rumor == Some(true) {
            // Former-player gate: a FORMER player is a live rumor ONLY if the corpus signals a
            // RETURN; otherwise the co-mention is historical background / a multi-entity artifact.
            if ready.relationship == "former" && !has_return_signal(&ready.news) {
                v.is_rumor = Some(false);
            }
        }
    }

    let (row, outcome) = row_from_verdict(
        verdict.as_ref(),
        &ready.relationship,
        (!ready.attribution.is_empty()).then_some(ready.attribution.as_str()),
        &ready.model_configured,
    );

    Ok(TransferPairOutput {
        player_id,
        model,
        heat: Some(ready.heat),
        components: ready.components,
        news_ids: ready.news_ids,
        prompted_news_ids: ready.prompted_news_ids,
        stale_news_ids,
        outcome,
        row: Some(row),
        built_prompt,
        request_body,
        eval_count,
        wall_ms,
        identity_apply_news: ready.news,
        prompt_version: TRANSFER_PROMPT_VERSION,
        input_hash: Some(ready.input_hash),
    })
}

fn transfer_components_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!({ "raw": s }))
}

fn transfer_trigger_payload_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or(serde_json::Value::Null)
}

fn transfer_parser_outcome(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Rumor => "rumor",
        Outcome::Cleared => "cleared",
        Outcome::Unknown => "unknown",
        Outcome::Skipped => "skipped",
    }
}

fn transfer_included_evidence(out: &TransferPairOutput, row: &TransferRow) -> serde_json::Value {
    serde_json::json!({
        "input_news_ids": &out.news_ids,
        "prompted_news_ids": &out.prompted_news_ids,
        "heat": out.heat,
        "heat_components": transfer_components_json(&out.components),
        "identity_apply_news_count": out.identity_apply_news.len(),
        "is_rumor": row.is_rumor,
        "direction": &row.direction,
        "stage": &row.stage,
        "confidence": row.confidence,
        "source_attribution": &row.attribution,
    })
}

fn transfer_excluded_evidence(out: &TransferPairOutput, row: &TransferRow) -> serde_json::Value {
    let mut excluded = Vec::new();
    match out.outcome {
        Outcome::Cleared => excluded.push(serde_json::json!({
            "reason": "model_cleared_pair",
            "trigger_payload": transfer_trigger_payload_json(&row.trigger_payload),
        })),
        Outcome::Unknown => excluded.push(serde_json::json!({
            "reason": "model_unknown_or_generate_failure",
            "trigger_payload": transfer_trigger_payload_json(&row.trigger_payload),
        })),
        _ => {}
    }
    if out.news_ids.len() > out.prompted_news_ids.len() {
        let prompted: std::collections::HashSet<i64> =
            out.prompted_news_ids.iter().copied().collect();
        let dropped_news_ids: Vec<i64> = out
            .news_ids
            .iter()
            .copied()
            .filter(|id| !prompted.contains(id))
            .collect();
        if !dropped_news_ids.is_empty() {
            excluded.push(serde_json::json!({
                "reason": "budget_truncated",
                "dropped_count": dropped_news_ids.len(),
                "dropped_news_ids": dropped_news_ids,
                "limit": TRANSFER_MAX_CORPUS_NEWS,
            }));
        }
    }
    if !out.stale_news_ids.is_empty() {
        excluded.push(serde_json::json!({
            "reason": "stale_news",
            "dropped_count": out.stale_news_ids.len(),
            "dropped_news_ids": &out.stale_news_ids,
            "lookback_days": 14,
        }));
    }
    serde_json::json!(excluded)
}

/// persist_transfer_row writes ONE row to the LIVE transfer_rumors table — the scored rumor, the
/// cleared row, and the UNKNOWN marker, which differ only in the bound values. Mirrors
/// `transfer.go::persist` (generated_at defaults NOW()).
/// `confidence` is bound float8 then cast to the numeric(3,2) column (sqlx has no numeric encode
/// without the decimal feature — the dual of the scrub `::float8` read landmine).
#[allow(clippy::too_many_arguments)]
pub async fn persist_transfer_row(
    pool: &PgPool,
    team_id: i32,
    player_id: i32,
    sport: &str,
    trigger_type: &str,
    out: &TransferPairOutput,
    row: &TransferRow,
) -> Result<i64> {
    let (source_count, source_names, source_latest_epoch, source_oldest_epoch) =
        load_transfer_source_metadata(pool, &out.news_ids).await?;
    let (trajectory, trajectory_components) =
        classify_transfer_trajectory(pool, team_id, player_id, sport, out, row).await?;
    let trajectory_json = trajectory_components.to_string();

    let row = sqlx::query(
        r#"
        INSERT INTO transfer_rumors (
            team_id, player_id, sport, trigger_type, heat, heat_components,
            is_rumor, direction, stage, model_summary, source_attribution, confidence,
            input_news_ids,
            rumor_updated_at, source_count, source_names, source_latest_at, source_oldest_at,
            trajectory, trajectory_components,
            model_version, prompt_version, trigger_payload, input_hash
        ) VALUES (
            $1,$2,$3,$4,$5,$6::jsonb,$7,$8,$9,$10,$11,$12::float8::numeric,$13,
            COALESCE(to_timestamp($14::double precision), NOW()), $15, $16,
            to_timestamp($17::double precision), to_timestamp($18::double precision),
            $19, $20::jsonb,
            $21,$22,$23::jsonb,$24
        )
        RETURNING id
        "#,
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .bind(trigger_type)
    .bind(out.heat)
    .bind(&out.components)
    .bind(row.is_rumor)
    .bind(row.direction.as_deref())
    .bind(row.stage.as_deref())
    .bind(row.summary.as_deref())
    .bind(row.attribution.as_deref())
    .bind(row.confidence)
    .bind(out.news_ids.as_slice())
    .bind(source_latest_epoch)
    .bind(source_count)
    .bind(&source_names)
    .bind(source_latest_epoch)
    .bind(source_oldest_epoch)
    .bind(trajectory)
    .bind(&trajectory_json)
    .bind(row.model.as_deref())
    .bind(out.prompt_version)
    .bind(&row.trigger_payload)
    .bind(out.input_hash.as_deref())
    .fetch_one(pool)
    .await
    .context("persist transfer row")?;
    Ok(row.get("id"))
}

/// bank_transfer_junction_event banks a SERVED transfer verdict as a narrative_event
/// tagged `origin='junction'` (mig 170) — outputs-as-memories at the corpus level. It is
/// WALLED OUT of the numeric feedback loop: both `refresh_typed_links` and the likelihood
/// language input scan `origin='extraction'` only, so the junction can never corroborate
/// itself. Anchored to the pair's OLDEST corpus article (stable across daily re-verdicts,
/// so re-reads UPSERT the same row rather than accumulate). The stage maps to the event
/// vocabulary: here_we_go ⇒ trade_confirmed/confirmed; advanced/concrete ⇒
/// trade_rumor/reported; speculation ⇒ trade_rumor/speculative. Best-effort at the call
/// site — a banking failure must never fail the already-persisted rumor.
async fn bank_transfer_junction_event(
    pool: &PgPool,
    sport: &str,
    player_id: i32,
    team_id: i32,
    stage: &str,
    model: &str,
    news_ids: &[i64],
) -> Result<()> {
    let Some(anchor) = news_ids.iter().copied().min() else {
        return Ok(()); // no corpus article to anchor to — nothing to bank
    };
    let (predicate, confidence) = match stage {
        "here_we_go" => ("trade_confirmed", "confirmed"),
        "advanced_talks" | "concrete_interest" => ("trade_rumor", "reported"),
        _ => ("trade_rumor", "speculative"),
    };
    sqlx::query(
        r#"
        INSERT INTO narrative_events
            (sport, subject_type, subject_id, predicate, object_type, object_id,
             sentiment, confidence, article_id, event_date, source, model_version,
             prompt_version, origin)
        SELECT $1, 'player', $2, $3, 'team', $4,
               NULL, $5, a.id, NOW(), a.source, $6, $7, 'junction'
        FROM news_articles a WHERE a.id = $8
        ON CONFLICT (article_id, sport, subject_type, subject_id, predicate,
                     COALESCE(object_type, ''), COALESCE(object_id, 0), origin)
        DO UPDATE SET confidence = EXCLUDED.confidence, event_date = NOW(),
                      model_version = EXCLUDED.model_version, extracted_at = NOW()
        "#,
    )
    .bind(sport)
    .bind(player_id)
    .bind(predicate)
    .bind(team_id)
    .bind(confidence)
    .bind(model)
    .bind(TRANSFER_PROMPT_VERSION)
    .bind(anchor)
    .execute(pool)
    .await
    .context("bank transfer junction event")?;
    Ok(())
}

async fn load_transfer_source_metadata(
    pool: &PgPool,
    news_ids: &[i64],
) -> Result<(i32, Vec<String>, Option<i64>, Option<i64>)> {
    if news_ids.is_empty() {
        return Ok((0, Vec::new(), None, None));
    }

    let row: (i32, Vec<String>, Option<i64>, Option<i64>) = sqlx::query_as(
        r#"
        SELECT count(id)::int,
               COALESCE(ARRAY(
                   SELECT DISTINCT NULLIF(a2.source, '')
                   FROM news_articles a2
                   WHERE a2.id = ANY($1)
                     AND NULLIF(a2.source, '') IS NOT NULL
                   ORDER BY 1
               ), '{}'::text[]),
               EXTRACT(EPOCH FROM max(COALESCE(published_at, fetched_at)))::bigint,
               EXTRACT(EPOCH FROM min(COALESCE(published_at, fetched_at)))::bigint
        FROM news_articles
        WHERE id = ANY($1)
        "#,
    )
    .bind(news_ids)
    .fetch_one(pool)
    .await
    .context("load transfer source metadata")?;

    Ok(row)
}

async fn classify_transfer_trajectory(
    pool: &PgPool,
    team_id: i32,
    player_id: i32,
    sport: &str,
    out: &TransferPairOutput,
    row: &TransferRow,
) -> Result<(&'static str, serde_json::Value)> {
    let previous: Option<i32> = sqlx::query_scalar(
        r#"
        SELECT heat::int
        FROM transfer_rumors
        WHERE team_id = $1
          AND player_id = $2
          AND sport = $3
          AND heat IS NOT NULL
        ORDER BY generated_at DESC
        LIMIT 1
        "#,
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("classify transfer trajectory {team_id}/{player_id}"))?;

    let current = out.heat.map(i32::from);
    let (trajectory, reason, delta) = if row.is_rumor == Some(false) {
        (
            "cooling_off",
            "cleared",
            previous.zip(current).map(|(p, c)| c - p),
        )
    } else if row.is_rumor != Some(true) {
        (
            DEFAULT_TRAJECTORY,
            "unresolved",
            previous.zip(current).map(|(p, c)| c - p),
        )
    } else {
        let (trajectory, delta_reason, delta) = classify_delta(previous, current);
        let reason = match delta_reason {
            "up" => "heat_up",
            "down" => "heat_down",
            "stable" => "heat_stable",
            other => other,
        };
        (trajectory, reason, delta)
    };

    Ok((
        trajectory,
        serde_json::json!({
            "previous_heat": previous,
            "current_heat": current,
            "heat_delta": delta,
            "reason": reason,
        }),
    ))
}

async fn load_transfer_identity_threshold(
    pool: &PgPool,
    sport: &str,
) -> Result<Option<TransferIdentityThreshold>> {
    let row = sqlx::query(
        r#"
        SELECT min_heat,
               min_deterministic_confidence::float8 AS min_deterministic_confidence
        FROM public.transfer_identity_thresholds
        WHERE sport = $1
        "#,
    )
    .bind(sport)
    .fetch_optional(pool)
    .await
    .context("load transfer identity threshold")?;

    Ok(row.map(|r| TransferIdentityThreshold {
        min_heat: r.get("min_heat"),
        min_deterministic_confidence: r.get("min_deterministic_confidence"),
    }))
}

fn identity_apply_deterministic_score(heat: i16) -> (i16, f64) {
    (heat, f64::from(heat) / 100.0)
}

async fn current_identity_team(
    pool: &PgPool,
    sport: &str,
    player_id: i32,
) -> Result<(Option<i32>, String)> {
    let row = sqlx::query(
        r#"
        SELECT pci.team_id, COALESCE(t.name, '') AS team_name
        FROM public.player_current_identity pci
        LEFT JOIN public.teams t ON t.id = pci.team_id AND t.sport = pci.sport
        WHERE pci.sport = $1 AND pci.player_id = $2
        "#,
    )
    .bind(sport)
    .bind(player_id)
    .fetch_one(pool)
    .await
    .context("load current identity for transfer apply")?;

    Ok((row.get("team_id"), row.get("team_name")))
}

#[allow(clippy::too_many_arguments)]
async fn record_transfer_identity_failure(
    pool: &PgPool,
    sport: &str,
    player_id: i32,
    old_team_id: Option<i32>,
    new_team_id: i32,
    source_rumor_id: i64,
    deterministic_heat: i16,
    deterministic_confidence: f64,
    raw: &str,
    model: &str,
    reason: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        SELECT public.record_transfer_identity_adjudication_failure(
            $1,$2,$3,$4,$5,NULL,$6,$7::float8::numeric,$8,$9,$10,$11
        )
        "#,
    )
    .bind(sport)
    .bind(player_id)
    .bind(old_team_id)
    .bind(new_team_id)
    .bind(source_rumor_id)
    .bind(deterministic_heat)
    .bind(deterministic_confidence)
    .bind(raw)
    .bind(model)
    .bind(TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION)
    .bind(reason)
    .execute(pool)
    .await
    .context("record transfer identity adjudication failure")?;
    Ok(())
}

fn autofill_view_for_sport(sport: &str) -> Result<&'static str> {
    match sport {
        "NBA" => Ok("nba.autofill_entities"),
        "NFL" => Ok("nfl.autofill_entities"),
        "FOOTBALL" => Ok("football.autofill_entities"),
        _ => Err(anyhow!("unsupported sport for autofill refresh: {sport}")),
    }
}

async fn refresh_sport_autofill_concurrently(
    pool: &PgPool,
    sport: &str,
    reason: &str,
) -> Result<()> {
    let view = autofill_view_for_sport(sport)?;
    sqlx::query("SELECT public.request_sport_autofill_refresh($1, $2)")
        .bind(sport)
        .bind(reason)
        .execute(pool)
        .await
        .context("mark sport autofill refreshing")?;

    if let Err(err) = sqlx::query(&format!("REFRESH MATERIALIZED VIEW CONCURRENTLY {view}"))
        .execute(pool)
        .await
    {
        let _ = sqlx::query("SELECT public.fail_sport_autofill_refresh($1, $2)")
            .bind(sport)
            .bind(err.to_string())
            .execute(pool)
            .await;
        return Err(err).context("refresh sport autofill concurrently");
    }

    let total: i32 = match sqlx::query_scalar(&format!("SELECT COUNT(*)::int FROM {view}"))
        .fetch_one(pool)
        .await
    {
        Ok(total) => total,
        Err(err) => {
            let _ = sqlx::query("SELECT public.fail_sport_autofill_refresh($1, $2)")
                .bind(sport)
                .bind(err.to_string())
                .execute(pool)
                .await;
            return Err(err).context("count refreshed sport autofill entities");
        }
    };

    sqlx::query("SELECT public.complete_sport_autofill_refresh($1, $2, $3)")
        .bind(sport)
        .bind(total)
        .bind(reason)
        .execute(pool)
        .await
        .context("complete sport autofill refresh")?;
    Ok(())
}

async fn sport_autofill_refresh_pending(pool: &PgPool, sport: &str) -> Result<bool> {
    let pending: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT status <> 'ready' FROM public.sport_autofill_versions WHERE sport = $1), false)",
    )
    .bind(sport)
    .fetch_one(pool)
    .await
    .context("check sport autofill refresh status")?;
    Ok(pending)
}

/// Returns whether a sport-autofill refresh is wanted: the caller runs ONE refresh per team
/// drain after the pair loop (Phase 2) instead of one heavy `REFRESH MATERIALIZED VIEW` inline
/// per applied pair. The apply path still marks the refresh durably (status <> 'ready') before
/// returning, so a drain that bails before refreshing self-heals via the pending check on the
/// next wake.
#[allow(clippy::too_many_arguments)]
async fn maybe_apply_transfer_identity(
    hx: &Harness,
    team_id: i32,
    team_name: &str,
    c: &TransferCandidate,
    sport: &str,
    heat: i16,
    news: &[NewsItem],
    persisted_rumor_id: i64,
    row: &TransferRow,
    outcome: Outcome,
    threshold: Option<&TransferIdentityThreshold>,
) -> Result<bool> {
    if outcome != Outcome::Rumor || row.is_rumor != Some(true) {
        return Ok(false);
    }
    if row.direction.as_deref() != Some("incoming") {
        return Ok(false);
    }

    let (identity_heat, deterministic_confidence) = identity_apply_deterministic_score(heat);
    let Some(threshold) = threshold else {
        warn!(
            sport,
            "transfers: missing identity threshold config; skipping apply"
        );
        return Ok(false);
    };
    if identity_heat < threshold.min_heat
        || deterministic_confidence < threshold.min_deterministic_confidence
    {
        return Ok(false);
    }

    let (old_team_id, old_team_name) = current_identity_team(&hx.pool, sport, c.player_id).await?;
    if old_team_id == Some(team_id) {
        // Already the current team: refresh only if a previous drain left one pending.
        return sport_autofill_refresh_pending(&hx.pool, sport).await;
    }

    let prompt = build_transfer_identity_adjudication_prompt(
        sport,
        c.player_id,
        &c.player_name,
        old_team_id,
        &old_team_name,
        team_id,
        team_name,
        news,
    );
    let opts = GenerateOptions {
        system: Some(transfer_identity_adjudication_system_prompt(sport)),
        temperature: Some(0.0),
        num_predict: 700,
        // The Editor's size, NOT `route::VOICE_NUM_CTX` — this is the one call in The Insider that
        // runs on `EmotionalNews`, which resolves LOCALLY to gemma3:4b, so the runner it shares is
        // The Editor's and graph's. It sent `num_ctx: 0` (the server default) until 2026-07-26,
        // which made it a dormant copy of the reload bug the voices had: harmless only because
        // identity adjudication fires rarely, and Archbox's reload count was 0 precisely because it
        // had not run. The value must track whatever The Editor asks for; the voices' 16384 here
        // would put that KV allocation on an 8 GB card.
        num_ctx: crate::route::LOCAL_STAGE_NUM_CTX,
        // Grammar-constrained like the Editor, not free json_mode: the 3b that took this seat
        // failed the bare-JSON shape 4/5 times (Aug 17); under a schema it cannot.
        json_mode: false,
        format_schema: Some(
            serde_json::from_str(TRANSFER_IDENTITY_ADJUDICATION_SCHEMA_RAW)
                .expect("TRANSFER_IDENTITY_ADJUDICATION_SCHEMA_RAW is valid JSON (unit-tested)"),
        ),
        format_schema_raw: Some(TRANSFER_IDENTITY_ADJUDICATION_SCHEMA_RAW.to_string()),
    };
    // Identity adjudication is a utility call (no character voice), so it stays on
    // `EmotionalNews` — NOT `TransferLogic`, which is The Insider's voice seam only.
    let backend = hx.router.for_role(Role::EmotionalNews);
    let model_configured = backend.model().to_string();
    let generated = match hx
        .extract(
            Role::EmotionalNews,
            &prompt,
            &opts,
            &TransferIdentityAdjudicationParser,
        )
        .await
    {
        Ok(extracted) => extracted,
        Err(e) => {
            warn!(
                team = team_id,
                player = c.player_id,
                error = %e,
                "transfers: identity adjudication generate failed; fail closed"
            );
            record_transfer_identity_failure(
                &hx.pool,
                sport,
                c.player_id,
                old_team_id,
                team_id,
                persisted_rumor_id,
                identity_heat,
                deterministic_confidence,
                "",
                &model_configured,
                "identity adjudication generate failed",
            )
            .await?;
            return Ok(false);
        }
    };

    let Some(adjudication) = generated.value else {
        record_transfer_identity_failure(
            &hx.pool,
            sport,
            c.player_id,
            old_team_id,
            team_id,
            persisted_rumor_id,
            identity_heat,
            deterministic_confidence,
            // The verbatim reply, NOT "" — a fail-closed row whose raw is empty is
            // undiagnosable from the database (the Aug-17 lesson).
            &generated.raw_response,
            &generated.model,
            "invalid identity adjudication JSON",
        )
        .await?;
        return Ok(false);
    };

    let adjudication_json =
        serde_json::to_value(&adjudication).context("serialize transfer identity adjudication")?;
    let raw = adjudication_json.to_string();
    let result = sqlx::query(
        r#"
        SELECT application_id, override_id, status, reason
        FROM public.apply_transfer_identity_candidate(
            $1,$2,$3,$4,$5,NULL,$6,$7::float8::numeric,$8::jsonb,$9,$10,$11
        )
        "#,
    )
    .bind(sport)
    .bind(c.player_id)
    .bind(old_team_id)
    .bind(team_id)
    .bind(persisted_rumor_id)
    .bind(identity_heat)
    .bind(deterministic_confidence)
    .bind(&raw)
    .bind(&raw)
    .bind(&generated.model)
    .bind(TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION)
    .fetch_one(&hx.pool)
    .await
    .context("apply transfer identity candidate")?;

    let status: String = result.get("status");
    if status == "applied" {
        let application_id: i64 = result.get("application_id");
        let override_id: Option<i64> = result.get("override_id");
        warn!(
            application_id,
            override_id,
            team = team_id,
            player = c.player_id,
            "transfers: applied current identity override"
        );
        // Mark durably now; the caller runs the actual REFRESH once after the pair loop.
        sqlx::query("SELECT public.request_sport_autofill_refresh($1, $2)")
            .bind(sport)
            .bind("applied_transfer_identity")
            .execute(&hx.pool)
            .await
            .context("mark sport autofill refreshing")?;
        // THIS is the threshold: the move just stopped being a rumor and became a roster fact.
        // Tell the Scout, whose brief is about to describe a squad that no longer exists — it is
        // the one seat with no trigger of its own beyond the rating snapshot, so without this it
        // would wait for stats that may never move. Best-effort: an enqueue failure must never
        // undo an adjudication that already committed.
        if let Err(e) = crate::junctions::scout::enqueue_rating_for_applied_transfer(
            &hx.pool,
            sport,
            c.player_id,
            old_team_id,
            Some(team_id),
            application_id,
        )
        .await
        {
            warn!(
                application_id,
                player = c.player_id,
                "transfers: applied move did not reach the Scout: {e:#}"
            );
        }
        return Ok(true);
    }
    Ok(false)
}

/// enqueue_sigil_for_transfer offers the PLAYER a freshly served rumor names to the Oracle's
/// completion barrier — the Phase 5.1 transfer→sigil trigger (the deferred half of the plan's
/// trigger-topology step). Transfer heat is a Sigil pillar and part of its `input_hash` now, so a
/// real change to the served-rumor set flips the hash and the re-run is real work; the Sigil
/// `input_hash` debounce is the second guard, skipping the model call when the served-rumor set is
/// unchanged. The persisted rumor id is the work-row `input_version`, so a done sigil row reopens
/// on each new served rumor and idempotently coalesces to one pending row within a drain. `sport`
/// is upper-cased (matching the news-rail sigil rows' conflict key).
///
/// The TEAM is deliberately absent: it is the entity being drained, so the worker offers it after
/// completing this `transfers` item. Offering it from here would ask the barrier a question this
/// handler's own un-deleted row is guaranteed to answer wrong.
async fn enqueue_sigil_for_transfer(
    hx: &Harness,
    player_id: i32,
    sport: &str,
    rumor_id: i64,
) -> Result<()> {
    let input_version = Some(rumor_id.to_string());
    // Only the PLAYER is offered here. The team is the entity being drained, so the worker offers
    // it after completing the transfers item — asking now, while this handler still holds that row,
    // is the race the barrier's doc comment describes. The player holds no row we own, so it is
    // safe to ask at any point; if one of the player's own pillars is mid-flight the barrier simply
    // declines and that pillar's completion asks again.
    crate::junctions::oracle::enqueue_oracle_if_pillars_settled(
        &hx.pool,
        "player",
        i64::from(player_id),
        sport,
        input_version,
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The Insider's card score (tarot deck, Phase 4) — "the wire wrap". An Oracle-shaped
// end-of-drain call: after the pair verdicts are filed, the Insider wraps each touched
// entity's wire in one number. The pair contract above stays FROZEN — the wrap versions
// independently and never enters any pair's debounce fingerprint.
// ---------------------------------------------------------------------------

/// Output contract for the wire wrap, captured in the cognition ledger.
pub const INSIDER_SCORE_OUTPUT_CONTRACT_VERSION: &str = "insider-score-v1";

/// Vetting-grade temperature: the wrap is a judgment of the board, not creative prose.
pub const INSIDER_SCORE_TEMPERATURE: f64 = 0.3;

/// Token cap for the `{read, score}` reply (mirrors the crown's budget; a 1-2 sentence read
/// plus one integer).
pub const INSIDER_SCORE_NUM_PREDICT: i32 = 1100;

/// The validated wrap reply — read + 1-99 score, clamped at parse.
#[derive(Clone, Debug)]
pub struct InsiderScoreReply {
    pub read: String,
    pub score: i16,
}

/// parse_insider_score_reply mirrors `sigil::parse_crown_reply`: tolerate prose around the
/// object, fold whitespace in the read, clamp the score to the tarot range. `None` = nothing
/// salvageable (a genuine failure the caller retries via the `errored` tally).
fn parse_insider_score_reply(raw: &str) -> Option<InsiderScoreReply> {
    let trimmed = raw.trim();
    let parsed: Option<serde_json::Value> = serde_json::from_str(trimmed).ok().or_else(|| {
        let start = trimmed.find('{')?;
        let end = trimmed.rfind('}')?;
        serde_json::from_str(&trimmed[start..=end]).ok()
    });
    let v = parsed?;
    let read = v.get("read")?.as_str()?.trim();
    let read = read.split_whitespace().collect::<Vec<_>>().join(" ");
    if read.is_empty() {
        return None;
    }
    let s = v.get("score")?;
    let n = if let Some(i) = s.as_i64() {
        i
    } else if let Some(f) = s.as_f64() {
        if !f.is_finite() {
            return None;
        }
        f.round() as i64
    } else if let Some(txt) = s.as_str() {
        txt.split_whitespace().next()?.parse::<i64>().ok()?
    } else {
        return None;
    };
    Some(InsiderScoreReply {
        read,
        score: n.clamp(1, 99) as i16,
    })
}

/// InsiderScoreParser is the wrap's `Parser` plug-in. Like the crown it never returns the
/// fail-closed `Ok(None)` — the wrap's only no-row path is the pre-model empty board; an
/// unparseable reply is a genuine failure → `Err` → the team item's `errored` tally retries it.
struct InsiderScoreParser;

impl Parser<InsiderScoreReply> for InsiderScoreParser {
    fn parse(&self, raw: &str) -> Result<Option<InsiderScoreReply>> {
        match parse_insider_score_reply(raw) {
            Some(r) => Ok(Some(r)),
            None => bail!(
                "insider score: could not parse read+score from response (raw={:?})",
                crate::util::truncate(raw, 200)
            ),
        }
    }
}

/// build_insider_score_input_components is the wrap's debounce pre-image: prompt_version + the
/// sorted active board, one `counterparty:heat:direction:stage` line per rumor — the same
/// component line narratives hashes for its heat facts. Content-keyed, NOT row-id-keyed: a
/// re-vet that lands the same verdict must not re-score the wire. Summary/confidence stay out
/// (derived commentary, same rule as the narratives pre-image).
pub fn build_insider_score_input_components(heat: &[HeatItem]) -> String {
    let mut lines: Vec<String> = heat
        .iter()
        .map(|t| format!("{}:{}:{}:{}", t.counterparty, t.heat, t.direction, t.stage))
        .collect();
    lines.sort();
    let mut out = format!(
        "{{\"prompt_version\":{},\"board\":[",
        go_json_string(INSIDER_SCORE_PROMPT_VERSION)
    );
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&go_json_string(line));
    }
    out.push_str("]}");
    out
}

/// How many of the entity's own recent wraps feed the prompt as continuity memory — mirrors
/// sigil's `PRIOR_READ_LIMIT`.
const PRIOR_INSIDER_READ_LIMIT: i64 = 4;

/// The Insider's own score memory: latest score (persisted as `previous_score` — continuity
/// audit) plus the rendered prompt block.
struct PriorInsiderRead {
    latest: i16,
    card: String,
}

/// load_prior_insider_read renders the Insider's OWN recent wraps as a continuity memory card —
/// the last read plus a short score trail with dates, mirroring `sigil::load_prior_read`
/// (memory, never a reset; the echo-chamber rule). `None` for a first-ever wrap. Prompt-only,
/// deliberately NOT part of the input_hash.
async fn load_prior_insider_read(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Option<PriorInsiderRead>> {
    let rows: Vec<(i16, Option<String>, String)> = sqlx::query_as(
        r#"
        SELECT score, read, to_char(generated_at, 'Mon DD')
        FROM insider_scores
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
        ORDER BY generated_at DESC
        LIMIT $4
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(PRIOR_INSIDER_READ_LIMIT)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load prior insider read {entity_type}/{entity_id}"))?;
    if rows.is_empty() {
        return Ok(None);
    }
    let mut card = String::new();
    if let Some(read) = rows[0].1.as_deref().filter(|r| !r.trim().is_empty()) {
        card.push_str(&format!("Last read ({}): {}\n", rows[0].2, read));
    }
    let trail: Vec<String> = rows.iter().map(|(s, _, d)| format!("{s} ({d})")).collect();
    card.push_str(&format!(
        "Recent scores (newest first): {}",
        trail.join(" · ")
    ));
    Ok(Some(PriorInsiderRead {
        latest: rows[0].0,
        card,
    }))
}

/// load_wire_touched_players returns the players named on the team's recently SERVED rumors —
/// a cheap SUPERSET of the true active board (none of load_transfer_heat's cooling-off /
/// direction / latest-per-counterparty logic): the wrap's own per-entity board load is the
/// precise gate, so an over-included player simply loads an empty board and skips. This is what
/// keeps a rumored player whose co-mention count faded OUT of the candidate list wrapped while
/// the rumor itself stays live (candidates alone would let their card show a live board with no
/// score).
async fn load_wire_touched_players(
    pool: &PgPool,
    team_id: i32,
    sport: &str,
) -> Result<Vec<(i32, String)>> {
    sqlx::query_as(
        r#"
        SELECT DISTINCT tr.player_id, p.name
        FROM transfer_rumors tr
        JOIN players p ON p.id = tr.player_id AND p.sport = tr.sport
        WHERE tr.team_id = $1 AND tr.sport = $2
          AND tr.is_rumor IS TRUE AND tr.heat > 0
          AND tr.generated_at > NOW() - INTERVAL '7 days'
        "#,
    )
    .bind(team_id)
    .bind(sport)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load wire-touched players team {team_id}"))
}

/// score_insider_entity runs the wire wrap for ONE entity: load the active board, skip when the
/// wire is dead (no call, no row — the Veil comes from the empty rumor board, `insider_scores`
/// deliberately has no marker rows) or unchanged (the board-hash debounce), else one
/// `Role::TransferLogic` call → one `insider_scores` row + one cognition-ledger entry.
async fn score_insider_entity(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport: &str,
) -> Result<()> {
    let heat = load_transfer_heat(&hx.pool, entity_type, entity_id, sport).await?;
    if heat.is_empty() {
        return Ok(());
    }
    let input_hash = hash_components(&build_insider_score_input_components(&heat));
    let key = EntityKey {
        entity_type: entity_type.to_string(),
        entity_id,
        sport: sport.to_string(),
        season: None,
    };
    if hx
        .debounce_unchanged("insider_scores", &key, &input_hash)
        .await?
    {
        debug!(
            entity_type,
            entity_id, "transfers: insider score debounce-skip, board unchanged"
        );
        return Ok(());
    }
    // Memory failure degrades to a first-wrap prompt (enrichment, never a blocker) — and like
    // every memory card it stays OUT of the input_hash.
    let prior = match load_prior_insider_read(&hx.pool, entity_type, entity_id, sport).await {
        Ok(p) => p,
        Err(e) => {
            warn!(
                entity_type,
                entity_id,
                error = %e,
                "transfers: prior insider read load failed (continuing without)"
            );
            None
        }
    };
    let prompt = build_insider_score_prompt(
        entity_name,
        sport,
        entity_type,
        &heat,
        prior.as_ref().map(|p| p.card.as_str()),
    );
    let opts = GenerateOptions {
        system: Some(INSIDER_SCORE_SYSTEM_PROMPT.to_string()),
        temperature: Some(INSIDER_SCORE_TEMPERATURE),
        num_predict: if crate::route::small_voice_window(hx.voice_num_ctx) {
            crate::junctions::oracle::SMALL_WINDOW_NUM_PREDICT
        } else {
            INSIDER_SCORE_NUM_PREDICT
        },
        num_ctx: hx.voice_num_ctx,
        json_mode: false,
        format_schema: Some(insider_score_format_schema()),
        format_schema_raw: None,
    };
    let extracted = hx
        .extract(Role::TransferLogic, &prompt, &opts, &InsiderScoreParser)
        .await?;
    let reply = extracted.value.ok_or_else(|| {
        anyhow!("insider score: parser returned None (InsiderScoreParser signals failure via Err)")
    })?;
    let previous_score = prior.as_ref().map(|p| p.latest);

    let row = sqlx::query(
        r#"
        INSERT INTO insider_scores (
            sport, entity_type, entity_id, score, previous_score, read,
            model_version, prompt_version, input_hash, generated_at
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,NOW())
        RETURNING id
        "#,
    )
    .bind(sport)
    .bind(entity_type)
    .bind(entity_id)
    .bind(reply.score)
    .bind(previous_score)
    .bind(reply.read.as_str())
    .bind(extracted.model.as_str())
    .bind(INSIDER_SCORE_PROMPT_VERSION)
    .bind(input_hash.as_str())
    .fetch_one(&hx.pool)
    .await
    .context("persist insider score")?;
    let row_id: i64 = row.get("id");

    insert_cognition_ledger_best_effort(
        &hx.pool,
        CognitionLedgerEntry {
            stage: "transfers".to_string(),
            lens: "insider_score".to_string(),
            role: Role::TransferLogic.as_str().to_string(),
            entity_type: entity_type.to_string(),
            entity_id,
            sport: sport.to_string(),
            pair_entity_type: None,
            pair_entity_id: None,
            trigger_type: "periodic".to_string(),
            trigger_payload: serde_json::Value::Null,
            product_table: "insider_scores".to_string(),
            product_row_ids: vec![row_id],
            model_version: extracted.model.clone(),
            prompt_version: INSIDER_SCORE_PROMPT_VERSION.to_string(),
            output_contract_version: INSIDER_SCORE_OUTPUT_CONTRACT_VERSION.to_string(),
            input_ids: Vec::new(),
            input_hash: Some(input_hash),
            request_body: Some(extracted.request_body),
            built_prompt: Some(extracted.built_prompt),
            included_evidence: serde_json::json!({
                "active_rumors": heat.len(),
                "board": heat
                    .iter()
                    .map(|t| format!("{}:{}:{}:{}", t.counterparty, t.heat, t.direction, t.stage))
                    .collect::<Vec<_>>(),
                "score": reply.score,
                "previous_score": previous_score,
            }),
            excluded_evidence: serde_json::json!([]),
            context_budget: serde_json::json!({
                "num_predict": INSIDER_SCORE_NUM_PREDICT,
                "eval_count": extracted.eval_count,
                "wall_ms": extracted.wall_ms,
            }),
            parser_outcome: "parsed".to_string(),
        },
    )
    .await;
    Ok(())
}

/// TransferHandler drains the team-keyed `transfers` stage: load the co-mention candidates and vet
/// each pair, persisting to transfer_rumors. Terminal for the transfers stage itself, but a served
/// rumor now re-triggers the downstream `sigil` convergence (Phase 5.1). The vetted-link trigger
/// enqueues transfers before narratives, and the worker drains stages in that order, so fresh heat
/// is available to the narrative/vibe stages in the same wake cycle. Any pair that hit a model
/// failure (UNKNOWN) or an infrastructure/persist error fails the team's item so the queue's backoff
/// re-runs it — and on that re-run the F3 fingerprint gate skips every pair whose material inputs
/// are unchanged since its last RESOLVED vetting, so only the failed pair pays for the retry.
/// After the pair loop the Insider wraps the wire (Phase 4): one debounced card score per touched
/// entity with a live board.
pub struct TransferHandler;

impl TransferHandler {
    pub fn new() -> Self {
        TransferHandler
    }
}

impl Default for TransferHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for TransferHandler {
    fn stage(&self) -> Stage {
        Stage::Transfers
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        if item.entity_type != "team" {
            bail!(
                "transfers: non-team entity {}/{}",
                item.entity_type,
                item.entity_id
            );
        }
        let team_id = item.entity_id_i32()?;
        let sport = item.sport.to_uppercase();
        let team_name =
            crate::corpus::lookup_entity_name(&hx.pool, &item.entity_type, team_id, &item.sport)
                .await?;
        let candidates =
            load_candidates(&hx.pool, team_id, &sport, TRANSFER_DEFAULT_MIN_ARTICLES).await?;
        // Per-team hoists (Phase 2): one batched relationship read for the whole candidate set
        // and one identity-threshold config read, instead of one of each per pair.
        let player_ids: Vec<i32> = candidates.iter().map(|c| c.player_id).collect();
        let (relationships, identity_threshold) = tokio::try_join!(
            team_relationships(&hx.pool, team_id, &player_ids, &sport),
            load_transfer_identity_threshold(&hx.pool, &sport),
        )?;

        let start = Instant::now();
        let pair_deadline = budget_deadline(start, hx.handler_budget, TRANSFER_PAIR_BUDGET_FRAC);
        let wrap_deadline = budget_deadline(start, hx.handler_budget, TRANSFER_WRAP_BUDGET_FRAC);

        let mut unknown = 0usize;
        let mut errored = 0usize;
        let mut vetted = 0usize;
        let mut pairs_deferred = 0usize;
        let mut autofill_refresh_wanted = false;
        for (idx, c) in candidates.iter().enumerate() {
            // Stop on the budget, not on the axe. Everything behind this point is already
            // persisted and will debounce-skip next round, so the pairs left here are the only
            // ones that cost anything to carry forward.
            if past(pair_deadline) {
                pairs_deferred = candidates.len() - idx;
                break;
            }
            // Model-failure UNKNOWN is not an infrastructure error: it is a successful fail-closed
            // row that the `unknown` tally turns into a team retry. DB/build/persist errors are
            // infrastructure failures; keep scanning pairs for visibility, then fail the team item.
            let pair = async {
                // A missing batch row is semantically "none" (no identity row + no stats row),
                // matching the per-pair COALESCE(false) defaults.
                let relationship = relationships
                    .get(&c.player_id)
                    .cloned()
                    .unwrap_or_else(|| "none".to_string());
                let out = match build_pair_request(
                    hx,
                    team_id,
                    &team_name,
                    c,
                    &sport,
                    relationship,
                    TRANSFER_TEMPERATURE,
                )
                .await?
                {
                    PairBuild::Skipped {
                        components,
                        news_ids,
                    } => skipped_pair_output(hx, c.player_id, components, news_ids),
                    PairBuild::Ready(ready) => {
                        // F3: skip the GPU call, the insert, and the ledger row when the pair's
                        // material inputs are unchanged since its latest resolved vetting. The
                        // previous row keeps serving and cools off with its sources.
                        if pair_unchanged(&hx.pool, team_id, c.player_id, &sport, &ready.input_hash)
                            .await?
                        {
                            debug!(
                                team = team_id,
                                player = c.player_id,
                                "transfers: pair debounce-skip, material inputs unchanged"
                            );
                            return Ok(Outcome::Skipped);
                        }
                        vet_pair(hx, team_id, c.player_id, &sport, *ready).await?
                    }
                };
                if let Some(row) = &out.row {
                    let persisted_rumor_id = persist_transfer_row(
                        &hx.pool,
                        team_id,
                        c.player_id,
                        &sport,
                        "periodic",
                        &out,
                        row,
                    )
                    .await?;
                    insert_cognition_ledger_best_effort(
                        &hx.pool,
                        CognitionLedgerEntry {
                            stage: "transfers".to_string(),
                            lens: "transfer".to_string(),
                            role: Role::TransferLogic.as_str().to_string(),
                            entity_type: "team".to_string(),
                            entity_id: team_id,
                            sport: sport.clone(),
                            pair_entity_type: Some("player".to_string()),
                            pair_entity_id: Some(c.player_id),
                            trigger_type: "periodic".to_string(),
                            trigger_payload: transfer_trigger_payload_json(&row.trigger_payload),
                            product_table: "transfer_rumors".to_string(),
                            product_row_ids: vec![persisted_rumor_id],
                            model_version: out.model.clone(),
                            prompt_version: out.prompt_version.to_string(),
                            output_contract_version: TRANSFER_OUTPUT_CONTRACT_VERSION.to_string(),
                            input_ids: out.news_ids.clone(),
                            input_hash: out.input_hash.clone(),
                            request_body: out.request_body.clone(),
                            built_prompt: out.built_prompt.clone(),
                            included_evidence: transfer_included_evidence(&out, row),
                            excluded_evidence: transfer_excluded_evidence(&out, row),
                            context_budget: serde_json::json!({
                                "num_predict": TRANSFER_NUM_PREDICT,
                                "eval_count": out.eval_count,
                                "wall_ms": out.wall_ms,
                            }),
                            parser_outcome: transfer_parser_outcome(out.outcome).to_string(),
                        },
                    )
                    .await;
                    // Outputs-as-memories, corpus half (mig 170): bank a SERVED rumor as
                    // an origin='junction' event — unified event log + audit, walled out
                    // of the numeric loop. Best-effort: never fail the persisted rumor.
                    if row.is_rumor == Some(true) {
                        if let Some(stage) = row.stage.as_deref() {
                            if let Err(e) = bank_transfer_junction_event(
                                &hx.pool,
                                &sport,
                                c.player_id,
                                team_id,
                                stage,
                                &out.model,
                                &out.news_ids,
                            )
                            .await
                            {
                                warn!(
                                    team = team_id,
                                    player = c.player_id,
                                    error = %e,
                                    "transfers: junction-event banking failed (best-effort)"
                                );
                            }
                        }
                    }
                    if let Some(heat) = out.heat {
                        if maybe_apply_transfer_identity(
                            hx,
                            team_id,
                            &team_name,
                            c,
                            &sport,
                            heat,
                            &out.identity_apply_news,
                            persisted_rumor_id,
                            row,
                            out.outcome,
                            identity_threshold.as_ref(),
                        )
                        .await?
                        {
                            autofill_refresh_wanted = true;
                        }
                    }
                    // Phase 5.1 (transfer→sigil trigger): a freshly SERVED rumor is a Sigil pillar
                    // now, so re-trigger panel synthesis for the player and the team it touches.
                    // Best-effort — a failed enqueue must NOT fail the already-persisted rumor or
                    // stall the team item (the vibe→sigil gate's new_transfer branch and the next
                    // news event are fallbacks).
                    if row.is_rumor == Some(true) {
                        if let Err(e) = enqueue_sigil_for_transfer(
                            hx,
                            c.player_id,
                            &sport,
                            persisted_rumor_id,
                        )
                        .await
                        {
                            warn!(
                                team = team_id,
                                player = c.player_id,
                                error = %e,
                                "transfers: sigil re-trigger enqueue failed (best-effort)"
                            );
                        }
                    }
                }
                Ok::<Outcome, anyhow::Error>(out.outcome)
            }
            .await;
            match pair {
                Ok(Outcome::Unknown) => unknown += 1,
                // Rumor/Cleared is a pair that reached a verdict on THIS run — the durable
                // progress the deferral protocol requires. Skipped is a debounce hit or an empty
                // corpus: correct, but it did not move the pair forward, so it does not count.
                Ok(Outcome::Rumor) | Ok(Outcome::Cleared) => vetted += 1,
                Ok(Outcome::Skipped) => {}
                Err(e) => {
                    errored += 1;
                    warn!(
                        team = team_id,
                        player = c.player_id,
                        error = %e,
                        "transfers: pair infrastructure/persist error"
                    );
                }
            }
        }

        // One autofill refresh per team drain (Phase 2), not one heavy REFRESH per applied
        // pair. Runs before the retry bails so an applied identity surfaces promptly; the
        // durable 'refreshing' marker self-heals a missed refresh on the next drain.
        if autofill_refresh_wanted {
            refresh_sport_autofill_concurrently(&hx.pool, &sport, "applied_transfer_identity")
                .await?;
        }

        // The wire wrap (tarot deck, Phase 4): after the pair verdicts are filed, the Insider
        // scores the wire of every entity this drain touches — the team, each candidate player,
        // AND each player named on the team's served board (a rumored player can outlive the
        // co-mention candidate window; their wire stays live and must stay scored) — inline,
        // the `enqueue_sigil_for_transfer` dual-entity pattern without a queue stage. A dead or
        // unchanged board pays nothing (skip inside); a failed wrap rides the existing
        // `errored` retry tally — the persisted rumors stand, the team item retries, every
        // unchanged pair debounce-skips, so only the wrap pays for the retry.
        //
        // Running LAST is why this needed its own budget: on a busy team the pair loop used to
        // spend the whole ceiling and the wrap never ran at all, so the teams with the most
        // transfer news were the ones whose scores went stale.
        let mut wrap_targets: Vec<(&str, i32, String)> = vec![("team", team_id, team_name.clone())];
        let mut seen_players: std::collections::HashSet<i32> = std::collections::HashSet::new();
        for c in &candidates {
            if seen_players.insert(c.player_id) {
                wrap_targets.push(("player", c.player_id, c.player_name.clone()));
            }
        }
        match load_wire_touched_players(&hx.pool, team_id, &sport).await {
            Ok(rumored) => {
                for (player_id, name) in rumored {
                    if seen_players.insert(player_id) {
                        wrap_targets.push(("player", player_id, name));
                    }
                }
            }
            Err(e) => {
                errored += 1;
                warn!(
                    team = team_id,
                    error = %e,
                    "transfers: wire-touched player load failed"
                );
            }
        }
        let mut wraps_deferred = 0usize;
        for (idx, (entity_type, entity_id, entity_name)) in wrap_targets.iter().enumerate() {
            if past(wrap_deadline) {
                wraps_deferred = wrap_targets.len() - idx;
                break;
            }
            if let Err(e) =
                score_insider_entity(hx, entity_type, *entity_id, entity_name, &sport).await
            {
                errored += 1;
                warn!(
                    entity_type,
                    entity_id,
                    error = %e,
                    "transfers: insider score failed"
                );
            }
        }

        let deferred = pairs_deferred + wraps_deferred;
        info!(
            team = team_id,
            candidates = candidates.len(),
            pairs_vetted = vetted,
            pairs_unknown = unknown,
            pairs_deferred,
            wrap_targets = wrap_targets.len(),
            wraps_deferred,
            errored,
            elapsed_s = start.elapsed().as_secs(),
            "transfers: team drain finished"
        );

        // A real infrastructure failure still walks the retry ladder — that is what `attempts` is
        // for, and a broken pool or a failing persist is not something another turn fixes.
        if errored > 0 {
            bail!(
                "transfers: {errored} pair infrastructure/persist error(s) — retrying team {}",
                item.entity_id
            );
        }

        // Out of budget with work left. `vetted > 0` is the progress guarantee `work::defer`
        // demands: a round that resolved nothing must not get a free turn, or a team whose very
        // first pair outruns the whole budget would defer forever. Such a round falls through to
        // the bails below and burns an attempt like any other stuck item.
        //
        // Deferring takes precedence over the `unknown` bail on purpose. Unresolved pairs are
        // retried next round regardless (an UNKNOWN row never satisfies the F3 fingerprint gate),
        // so making the item pay an attempt for them here would penalise a run that was making
        // progress. The round that finally finishes inside its budget is the one that hands any
        // still-unresolved pairs to the retry ladder.
        if deferred > 0 && vetted > 0 {
            let note = format!(
                "deferred: {pairs_deferred} pair(s) + {wraps_deferred} wrap(s) left after {}s",
                start.elapsed().as_secs()
            );
            crate::work::defer(&hx.pool, item, TRANSFER_DEFER_DELAY, &note).await?;
            debug!(team = team_id, %note, "transfers: team deferred to another turn");
            return Ok(());
        }

        if unknown > 0 {
            bail!(
                "transfers: {unknown} unresolved pair(s) (model failure) — retrying team {}",
                item.entity_id
            );
        }
        if deferred > 0 {
            bail!(
                "transfers: out of budget with {deferred} unit(s) left and no pair resolved — \
                 retrying team {}",
                item.entity_id
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
