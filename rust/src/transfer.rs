//! Transfers stage — the team-keyed transfer/trade rumor vetting, ported from Go (Cutover Step 2).
//!
//! The Go source is the machinery spec. `transfer.go` is the loaders + the deterministic
//! heat/relationship/direction + the prompt, parse, the former-player + grounding gates + persist;
//! `transfer_heat.go` exposes `compute_transfer_heat`, a SQL function (stays Postgres); and
//! `derive.go`'s drainTransfers is the queue driver (team Item → GenerateForTeam, fail-on-Unknown
//! retry).
//!
//! Composition (Plan §1.2 + §4): per (team, player) PAIR this composes `extract+validate`
//! (fail-closed Option<bool> is_rumor, JSON mode), the subject same-person test, and the persist.
//! The deterministic parts stay where they belong: `compute_transfer_heat`, the `direction`, and
//! the team relationship are SQL/Postgres (the model never computes the number or the direction);
//! the model ONLY vets — is this a live rumor about THIS exact player, what stage, a grounded
//! one-line summary. The subject same-person test is realised as the verdict's `subject` field plus
//! the t5 identity-card framing in the system prompt (the model returns is_rumor AND subject in ONE
//! JSON, exactly as Go does); the standalone embedding-backed `resolve_one` for transfers is a
//! HORIZON refinement — it would restructure the one fused call into two and break Go-machinery
//! parity, so it waits (Plan §1.3 "an improvement, not parity").
//!
//! FAIL CLOSED (the §1.2 invariant): `is_rumor: Option<bool>` — a model timeout, unparseable output,
//! or a verdict that never committed to is_rumor persists an UNKNOWN row (is_rumor NULL), which is
//! NEVER served (every read requires `is_rumor IS TRUE`) and is counted so the team's stage item is
//! re-enqueued for a retry. Only a successful POSITIVE verdict ever becomes a served rumor.
//!
//! DEBOUNCE (F3, flow-friction plan 2026-07-12): before each pair's model call the production
//! handler fingerprints the pair's MATERIAL inputs — sorted pair-corpus article ids, the
//! corpus-stable heat components (`distinct_sources`, `tier_weight`), and the deterministic
//! relationship; no timestamps, no prose, no recency decay — and skips the GPU call, the insert,
//! and the ledger row when the pair's latest RESOLVED `transfer_rumors` row carries the same
//! `input_hash` (mig 145 reserved the column). UNKNOWN markers never satisfy the gate, so a
//! model-failure retry re-vets ONLY the failed pair: the completed pairs skip on fingerprint
//! instead of burning ~39 redundant GPU calls per team retry. An unchanged pair keeps its previous
//! row serving, which then cools off naturally with its sources — the source-freshness doctrine
//! working, per the 2026-07-12 plan.
//!
//! THE t5 PROMPT keeps the L9 false-heat fixes (roundups are not rumors; never invent a fee or stage)
//! but rewrites the instructions as schema-first rules for smaller local models.

use crate::harness::{Harness, Parser};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::trajectory::{classify_delta, DEFAULT_TRAJECTORY};
use crate::util::{go_json_float, go_json_string, hash_components, truncate_bytes};
use crate::work::{Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use tracing::{debug, warn};

/// Prompt version for the transfer/trade vetting contract.
pub const TRANSFER_PROMPT_VERSION: &str = "t7"; // t7: computed evidence card + no-credible-source stage ceiling

/// Output schema version for transfer adjudication JSON, distinct from the prompt contract.
pub const TRANSFER_OUTPUT_CONTRACT_VERSION: &str = "transfer-verdict-v1";

/// Production vetting temperature (transfer.go uses 0.3). The parity harness overrides to 0.
pub const TRANSFER_TEMPERATURE: f64 = 0.3;

/// Token cap for the JSON verdict.
pub const TRANSFER_NUM_PREDICT: i32 = 900;

/// Corpus + candidate governors — mirror transfer.go's consts exactly (the parity contract).
const TRANSFER_MAX_CORPUS_NEWS: i64 = 12;
/// Default candidate pre-filter (min co-mention articles / 14d). Mirrors `transferDefaultMinArticles`.
pub const TRANSFER_DEFAULT_MIN_ARTICLES: i32 = 2;
const TRANSFER_MAX_CANDIDATES: i32 = 40;
/// How far apart (title chars) a team and player may be mentioned and still count as a genuine
/// co-mention. Mirrors `comentionProximityChars` / migration 033's gate.
const COMENTION_PROXIMITY_CHARS: i32 = 50;
/// Summary clip (transfer.go: `truncate(s, 240)`) and news-description clip (`truncate(d, 160)`).
const SUMMARY_TRUNCATE: usize = 240;
const DESC_TRUNCATE: usize = 160;

/// transfer_system_prompt is the model-neutral transfer/trade vetting prompt. `noun` is "trade" for
/// NBA/NFL and "transfer" otherwise.
pub fn transfer_system_prompt(sport: &str) -> String {
    let noun = if sport == "NBA" || sport == "NFL" {
        "trade"
    } else {
        "transfer"
    };
    format!(
        r#"Task: decide whether the news reports a current {noun} involving BOTH the named team and the exact player in the identity line.

Use the identity line to disambiguate same-name people. Current club and position are strong tie-breakers. When unsure it is the same person, set is_rumor=false.

Set is_rumor=false when any of these holds:
- The sources are about a different same-name person: owner, president, manager, coach, unrelated figure, or another player at another club.
- The source club, role, or position contradicts the identity line.
- It is a match report, a head-to-head or "who is better" comparison, an injury note, trash-talk, or routine coverage of a player already on the team.
- The player is mentioned only as an opponent/rival, game-plan problem, draft counter, or comparison target.
- The move is old historical/background context from a prior window with no current roster impact.
- A recently completed, finalizing, agreed, or reported trade/transfer involving the named team
  and exact player is still a current move signal; classify it instead of discarding it as historical.
- The player is only one name in a roundup, mailbag, notes column, power ranking, rumor wrap, or listicle. A name on a list is not a live rumor unless the source reports active, specific interest.

When is_rumor=true:
- summary: one tight sentence naming the real counterparties and any fee, bid, pick, or asset compensation explicitly stated by the sources.
- Never estimate, round, or invent money, picks, stage, or deal status.
- Attribute the substance to the strongest named source when available.

Stage ladder:
- speculation = a mention, link, monitoring, or thin report.
- concrete_interest = the source says the club is actively pursuing the player.
- advanced_talks = reported active negotiation.
- here_we_go = agreed or imminent deal.
- If evidence is thin, use speculation.
- The Evidence line is computed, not claimed. A single source, or no credible source, never supports a stage beyond speculation on headline tone alone. advanced_talks and here_we_go need multiple independent credible sources, or one top-tier source explicitly reporting agreement/negotiation.

Return only this JSON object, with every field present:
{{"is_rumor": true|false, "subject": "who the sources are actually about (real name/person, even if NOT this player)", "direction": "incoming"|"outgoing"|"unclear", "stage": "speculation"|"concrete_interest"|"advanced_talks"|"here_we_go", "summary": "one tight sentence: who, which clubs, any fee or picks the sources actually state, attributed to the source", "confidence": 0.0-1.0}}

direction is relative to the named team: incoming = joining the team; outgoing = leaving the team. subject is the person's name only, never the full identity line. If it is not a live {noun} about this exact player, set is_rumor=false and set subject to who the sources are really about."#
    )
}

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

/// Prompt/version for the second, narrower current-identity adjudication gate. The normal transfer
/// vet decides whether a row is a live rumor; this gate decides whether that already-vetted rumor is
/// strong enough to mutate canonical current identity.
pub const TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION: &str = "identity-adjudication-v1";

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

fn transfer_identity_adjudication_system_prompt(sport: &str) -> String {
    let noun = if sport == "NBA" || sport == "NFL" {
        "trade"
    } else {
        "transfer"
    };
    format!(
        r#"Task: adjudicate whether a candidate {noun} should update the player's CURRENT team identity.

Fail closed. You confirm or reject only the proposed IDs; never invent a different player or team ID.

Return only strict JSON with exactly these fields:
{{"decision":"apply|reject","event_type":"transfer|trade|loan|signing|extension|rumor|false_positive","old_team_id":0,"new_team_id":0,"reason":"","evidence_spans":[]}}

Use decision="apply" only when the evidence says the move is complete, agreed, signed, registered, official, or otherwise a current-team fact now.
Use decision="reject" for speculation, interest, monitoring, ambiguity, unclear direction, conflicting sources, missing or contradictory team IDs, historical/background moves, already-current-team contradictions, or false positives.

old_team_id and new_team_id must exactly match the proposed IDs. If old team is unknown, return null for old_team_id."#
    )
}

#[allow(clippy::too_many_arguments)]
fn build_transfer_identity_adjudication_prompt(
    sport: &str,
    player_id: i32,
    player_name: &str,
    current_team_id: Option<i32>,
    current_team_name: &str,
    new_team_id: i32,
    new_team_name: &str,
    news: &[NewsItem],
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Sport: {sport}\nPlayer: {player_name} (id {player_id})\n"
    ));
    b.push_str(&format!(
        "Current identity: team_id={} team_name={}\n",
        current_team_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "null".to_string()),
        if current_team_name.is_empty() {
            "unknown"
        } else {
            current_team_name
        }
    ));
    b.push_str(&format!(
        "Proposed new identity: team_id={new_team_id} team_name={new_team_name}\n"
    ));
    b.push_str("Decide only from the evidence articles and the proposed entity IDs below.\n");
    b.push_str("\nEvidence headlines:\n");
    for n in news {
        b.push_str("- ");
        if !n.source.is_empty() {
            b.push_str(&format!("[{}] ", n.source));
        }
        b.push_str(&n.title);
        if !n.description.is_empty() {
            b.push_str(" — ");
            b.push_str(&truncate_bytes(&n.description, DESC_TRUNCATE));
        }
        b.push('\n');
    }
    b.push_str("\nReturn the strict JSON adjudication now.");
    b
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

/// How a vetted pair was classified — drives the per-team tally (the Go `TransferResult` counts) and
/// the fail-closed retry (any `Unknown` fails the team's stage item).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Rumor,   // is_rumor TRUE — a vetted, served rumor
    Cleared, // is_rumor FALSE — roster/match-report/roundup noise (hidden by the read filter)
    Unknown, // is_rumor NULL — model failure (timeout/unparseable/no-commit); fail-closed, retryable
    Skipped, // no corpus (heat NULL), or unchanged material (the F3 fingerprint gate) — no row written
}

/// The persistable columns derived from a verdict (after the deterministic gates), shared by the
/// production write (→ transfer_rumors) and the shadow write (→ transfer_rumors_shadow) so the two
/// can never drift. Mirrors the branching in `transfer.go::persist`.
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

/// The un-persisted result of vetting one (team, player) pair — everything the production handler
/// (→ transfer_rumors) and the parity harness (→ shadow) need. The twin of `vibe::VibeOutput`.
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

/// load_tier_map returns the source-credibility weights keyed `"kind:source"` (source lower-cased),
/// for grounded attribution. Mirrors `loadTierMap`. `weight::float8` avoids the numeric→f64 scan
/// landmine (sqlx has no numeric decode without the decimal feature).
pub async fn load_tier_map(pool: &PgPool) -> Result<HashMap<String, f64>> {
    let rows: Vec<(String, String, f64)> =
        sqlx::query_as("SELECT kind, lower(source), weight::float8 FROM source_tiers")
            .fetch_all(pool)
            .await
            .context("load tier map")?;
    Ok(rows
        .into_iter()
        .map(|(kind, source, weight)| (format!("{kind}:{source}"), weight))
        .collect())
}

/// load_candidates returns the team's co-mention candidate players with identity cards — the Rust
/// port of `transfer.go::loadCandidates` (current club from `player_current_identity`; both vetted
/// links required; co-mention proximity gate).
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
          AND te.vetted IS TRUE
          AND pe.vetted IS TRUE
          AND (te.title_pos IS NULL OR pe.title_pos IS NULL
               OR abs(te.title_pos - pe.title_pos) <= $5)
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
    .bind(COMENTION_PROXIMITY_CHARS)
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
          AND te.vetted IS TRUE
          AND pe.vetted IS TRUE
          AND (te.title_pos IS NULL OR pe.title_pos IS NULL
               OR abs(te.title_pos - pe.title_pos) <= $4)
        ORDER BY a.id
        "#,
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .bind(COMENTION_PROXIMITY_CHARS)
    .fetch_all(pool)
    .await
    .context("load stale pair news ids")?;
    Ok(ids)
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

/// best_source returns the highest-credibility source in the corpus and its weight (unknown sources
/// default 0.3) — grounded attribution. Mirrors `bestSource`.
fn best_source(news: &[NewsItem], tiers: &HashMap<String, f64>) -> (String, f64) {
    let mut best = String::new();
    let mut best_w = 0.0_f64;
    for n in news {
        if n.source.is_empty() {
            continue;
        }
        let w = tiers
            .get(&format!("news:{}", n.source.to_lowercase()))
            .copied()
            .unwrap_or(0.3);
        if w > best_w {
            best_w = w;
            best = n.source.clone();
        }
    }
    (best, best_w)
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

/// build_transfer_prompt assembles the user prompt — BYTE-IDENTICAL to `buildTransferPrompt`
/// (the deterministic parity axis). The "·" separator (U+00B7) and the "—" (U+2014) are
/// significant bytes; at temp 0 a single changed byte would change the model's output.
/// TransferEvidence is the computed evidence-quality card (Phase 2, t7): corpus size, source
/// diversity, and best-source credibility, measured in code before the model call. The model
/// grades a claim it is HANDED the evidentiary weight of, instead of inferring "how solid is
/// this" from headline tone — the failure mode behind roundup false-positives and over-staging.
#[derive(Clone, Debug, Default)]
pub struct TransferEvidence {
    pub total_articles: usize,
    pub distinct_sources: usize,
    pub best_source: String,
    pub best_weight: f64,
}

impl TransferEvidence {
    pub fn from_news(news: &[NewsItem], total_articles: usize, best: &str, best_weight: f64) -> Self {
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
            best_weight,
        }
    }
}

pub fn build_transfer_prompt(
    team_name: &str,
    c: &TransferCandidate,
    sport: &str,
    relationship: &str,
    news: &[NewsItem],
    evidence: &TransferEvidence,
) -> String {
    let player_name = &c.player_name;
    let mut b = String::new();
    b.push_str(&format!(
        "Sport: {sport}\nTeam: {team_name}\nPlayer: {player_name}\n"
    ));

    // Identity card — disambiguators that separate same-name people (current club leads).
    let mut ident: Vec<String> = vec![player_name.clone()];
    if !c.nationality.is_empty() {
        ident.push(c.nationality.clone());
    }
    if !c.current_club.is_empty() {
        ident.push(format!("currently at {}", c.current_club));
    } else {
        ident.push("current club unknown".to_string());
    }
    if !c.position.is_empty() {
        ident.push(c.position.clone());
    }
    b.push_str("Identity (the ONE specific player to judge): ");
    b.push_str(&ident.join(" · "));
    b.push('\n');

    match relationship {
        "current" => b.push_str(&format!(
            "Roster status: {player_name} is CURRENTLY on {team_name} — so any move is a DEPARTURE (outgoing). Frame the summary as other clubs' interest in signing them.\n"
        )),
        "former" => b.push_str(&format!(
            "Roster status: {player_name} is a FORMER {team_name} player who has SINCE LEFT. A 'former/ex-{team_name}' mention is just background, NOT a transfer rumor — set is_rumor=false UNLESS the sources genuinely report {player_name} RETURNING to {team_name} (then it is incoming).\n"
        )),
        _ => b.push_str(&format!(
            "Roster status: {player_name} is NOT on {team_name} — so any move is an ARRIVAL (incoming). Frame the summary as {team_name} pursuing them.\n"
        )),
    }

    // Evidence card (t7): computed facts, not model inference. Rendered even when thin —
    // "1 article, 1 source" IS the signal the staging rules key on.
    b.push_str(&format!(
        "Evidence (computed): {} article{}, {} distinct source{}; strongest source: {}.\n",
        evidence.total_articles,
        if evidence.total_articles == 1 { "" } else { "s" },
        evidence.distinct_sources,
        if evidence.distinct_sources == 1 { "" } else { "s" },
        if evidence.best_source.is_empty() {
            "none attributed".to_string()
        } else {
            format!("{} (credibility {:.1})", evidence.best_source, evidence.best_weight)
        }
    ));

    b.push_str("\nNews headlines:\n");
    if news.is_empty() {
        b.push_str("- (none)\n");
    } else {
        for n in news {
            b.push_str("- ");
            if !n.source.is_empty() {
                b.push_str(&format!("[{}] ", n.source));
            }
            b.push_str(&n.title);
            if !n.description.is_empty() {
                b.push_str(" — ");
                b.push_str(&truncate_bytes(&n.description, DESC_TRUNCATE));
            }
            b.push('\n');
        }
    }
    b.push_str("\nReturn the JSON verdict now.");
    b
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
/// pair-corpus article ids (the corpus identity), the corpus-stable heat components
/// (`distinct_sources` + `tier_weight` — the corroboration/credibility facts feeding the grounding
/// guard, which move only when the corpus or the source-tier table moves), and the deterministic
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
    let tier_weight = comps
        .get("tier_weight")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    let mut ids: Vec<i64> = news_ids.to_vec();
    ids.sort_unstable();
    let ids_csv = ids
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"distinct_sources\":{distinct_sources},\"news_ids\":[{ids_csv}],\"relationship\":{},\"tier_weight\":{}}}",
        go_json_string(relationship),
        go_json_float(tier_weight),
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

/// PairBuild is the DETERMINISTIC prefix of `analyze_pair` — everything computed BEFORE the model
/// call (heat → corpus → relationship → prompt → request body). Shared by the production handler
/// AND the parity harness so the dumped `built_prompt`/`request_body` are EXACTLY what production
/// sends (no drift between what we test and what we run). `Skipped` ⇒ no pair corpus (heat NULL),
/// so no model call and no row (Go's `if heat == nil { res.Skipped++; return nil }`).
pub enum PairBuild {
    Skipped {
        components: String,
        news_ids: Vec<i64>,
    },
    Ready(Box<PairReady>),
}

/// PairReady carries the assembled model inputs (the parity axes) plus the deterministic context the
/// post-model gates need (`news` for the former-player return-signal, `best_weight` for the grounding
/// guard, `relationship` for direction). `request_body` is computed from the SAME backend + opts the
/// call will use, so it can never drift from what is POSTed.
pub struct PairReady {
    pub heat: i16,
    pub components: String,
    pub news_ids: Vec<i64>,
    pub prompted_news_ids: Vec<i64>,
    pub stale_news_ids: Vec<i64>,
    pub news: Vec<NewsItem>,
    pub relationship: String,
    pub attribution: String,
    pub best_weight: f64,
    pub opts: GenerateOptions,
    pub built_prompt: String,
    pub request_body: serde_json::Value,
    pub model_configured: String,
    /// The F3 per-pair debounce fingerprint over the material inputs
    /// (see [`build_transfer_input_components`]).
    pub input_hash: String,
}

/// build_pair_request runs the deterministic prefix: `compute_transfer_heat` (SQL — the number
/// stays Postgres), the pair corpus, the deterministic team relationship, then `build_transfer_prompt`
/// with the t5 options and the exact wire body. NO model call — these are the deterministic axes (the L2
/// finding: the verdict is not a temp-0 parity axis, so the gate needs no GPU). The role is
/// [`Role::EmotionalNews`] (the news/transfer reasoner).
pub async fn build_pair_request(
    hx: &Harness,
    team_id: i32,
    team_name: &str,
    c: &TransferCandidate,
    sport: &str,
    tiers: &HashMap<String, f64>,
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

    let (news, stale_news_ids) = tokio::try_join!(
        load_pair_news(&hx.pool, &news_ids),
        load_stale_pair_news_ids(&hx.pool, team_id, c.player_id, sport),
    )?;
    let prompted_news_ids = news.iter().map(|n| n.id).collect();
    // Grounding: credibility attribution comes from the CORPUS, not the model.
    let (attribution, best_weight) = best_source(&news, tiers);
    // Direction + the noise filter key off the deterministic relationship, not the model's text.
    let relationship = team_relationship(&hx.pool, team_id, c.player_id, sport).await?;

    // F3: fingerprint the material inputs now that they are all in hand (corpus ids + stable
    // heat components + relationship) — the handler gates on this BEFORE paying for the GPU.
    let input_hash = hash_components(&build_transfer_input_components(
        &news_ids,
        &components,
        &relationship,
    ));

    let evidence = TransferEvidence::from_news(&news, news_ids.len(), &attribution, best_weight);
    let built_prompt = build_transfer_prompt(team_name, c, sport, &relationship, &news, &evidence);
    let opts = GenerateOptions {
        system: Some(transfer_system_prompt(sport)),
        temperature: Some(temperature),
        num_predict: TRANSFER_NUM_PREDICT,
        num_ctx: 0,
        json_mode: true,
    format_schema: None,
    };
    let backend = hx.router.for_role(Role::EmotionalNews);
    let request_body = backend.request_body(&built_prompt, &opts);
    let model_configured = backend.model().to_string();

    Ok(PairBuild::Ready(Box::new(PairReady {
        heat,
        components,
        news_ids,
        prompted_news_ids,
        stale_news_ids,
        news,
        relationship,
        attribution,
        best_weight,
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
    let model = hx.router.for_role(Role::EmotionalNews).model().to_string();
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
/// gates). NO debounce: the parity harness must always exercise the model, so the F3 fingerprint
/// gate lives in the production handler, BETWEEN the builder and `vet_pair` (the same split as
/// vibe's `load_vibe_context`). Mirrors `transfer.go::analyzePair`. A generate failure is swallowed
/// into an UNKNOWN output (the fail-closed marker), NOT propagated — only a real DB/transport error
/// returns `Err` (the per-team loop counts it as Errored and moves on, exactly as Go does).
pub async fn analyze_pair(
    hx: &Harness,
    team_id: i32,
    team_name: &str,
    c: &TransferCandidate,
    sport: &str,
    tiers: &HashMap<String, f64>,
    temperature: f64,
) -> Result<TransferPairOutput> {
    match build_pair_request(hx, team_id, team_name, c, sport, tiers, temperature).await? {
        PairBuild::Skipped {
            components,
            news_ids,
        } => Ok(skipped_pair_output(hx, c.player_id, components, news_ids)),
        PairBuild::Ready(r) => vet_pair(hx, team_id, c.player_id, *r).await,
    }
}

/// vet_pair is the MODEL half of `analyze_pair`: extract, the deterministic post-model gates, and
/// the row shaping, over an already-built [`PairReady`]. Split out so the production handler can
/// run the F3 fingerprint gate between `build_pair_request` and the GPU call.
pub async fn vet_pair(
    hx: &Harness,
    team_id: i32,
    player_id: i32,
    ready: PairReady,
) -> Result<TransferPairOutput> {
    // route(EmotionalNews) + extract(TransferParser). A generate transport error → fail-closed
    // UNKNOWN row (Go persists UNKNOWN on a model timeout, then the team item is retried), recording
    // the prompt/body that WAS sent for the parity diff.
    let (verdict, model, built_prompt, request_body, eval_count, wall_ms) = match hx
        .extract(
            Role::EmotionalNews,
            &ready.built_prompt,
            &ready.opts,
            &TransferParser,
        )
        .await
    {
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
            // Grounding guard: a claimed rumor with no credible (tier-1/2) source is suspect.
            if v.is_rumor == Some(true) && ready.best_weight < 0.5 {
                v.confidence *= 0.5;
                // Stage ceiling (t7): with no credible source a claim can never persist beyond
                // speculation — the same evidence bar, applied to the stage itself. The model's
                // original stage stays visible in the ledger's raw response.
                if norm_stage(&v.stage) != "speculation" {
                    warn!(
                        player_id = player_id,
                        model_stage = %v.stage,
                        best_weight = ready.best_weight,
                        "transfer: stage clamped to speculation (no credible source)"
                    );
                    v.stage = "speculation".to_string();
                }
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
        stale_news_ids: ready.stale_news_ids,
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
) -> Result<()> {
    if outcome != Outcome::Rumor || row.is_rumor != Some(true) {
        return Ok(());
    }
    if row.direction.as_deref() != Some("incoming") {
        return Ok(());
    }

    let (identity_heat, deterministic_confidence) = identity_apply_deterministic_score(heat);
    let Some(threshold) = load_transfer_identity_threshold(&hx.pool, sport).await? else {
        warn!(
            sport,
            "transfers: missing identity threshold config; skipping apply"
        );
        return Ok(());
    };
    if identity_heat < threshold.min_heat
        || deterministic_confidence < threshold.min_deterministic_confidence
    {
        return Ok(());
    }

    let (old_team_id, old_team_name) = current_identity_team(&hx.pool, sport, c.player_id).await?;
    if old_team_id == Some(team_id) {
        if sport_autofill_refresh_pending(&hx.pool, sport).await? {
            refresh_sport_autofill_concurrently(&hx.pool, sport, "applied_transfer_identity")
                .await?;
        }
        return Ok(());
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
        num_ctx: 0,
        json_mode: true,
    format_schema: None,
    };
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
            return Ok(());
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
            "",
            &generated.model,
            "invalid identity adjudication JSON",
        )
        .await?;
        return Ok(());
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
        refresh_sport_autofill_concurrently(&hx.pool, sport, "applied_transfer_identity").await?;
    }
    Ok(())
}

/// enqueue_sigil_for_transfer re-triggers panel synthesis for the player AND the team a freshly
/// served rumor touches — the Phase 5.1 transfer→sigil trigger (the deferred half of the plan's
/// trigger-topology step). Transfer heat is a Sigil pillar and part of its `input_hash` now, so a
/// real change to the served-rumor set flips the hash and the re-run is real work; the Sigil
/// `input_hash` debounce is the second guard, skipping the model call when the served-rumor set is
/// unchanged. The persisted rumor id is the work-row `input_version`, so a done sigil row reopens
/// on each new served rumor and idempotently coalesces to one pending row within a drain. `sport`
/// is upper-cased (matching the news-rail sigil rows' conflict key), mirroring the rating→sigil
/// `enqueue_sigil` in `bin/statcommentary`.
async fn enqueue_sigil_for_transfer(
    hx: &Harness,
    team_id: i32,
    player_id: i32,
    sport: &str,
    rumor_id: i64,
) -> Result<()> {
    let input_version = Some(rumor_id.to_string());
    for (entity_type, entity_id) in [("player", player_id), ("team", team_id)] {
        let sig = Item {
            stage: Stage::Sigil,
            entity_type: entity_type.to_string(),
            entity_id: i64::from(entity_id),
            sport: sport.to_string(),
            input_version: input_version.clone(),
            attempts: 0,
        };
        crate::work::enqueue(&hx.pool, &sig).await?;
    }
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
        let tiers = load_tier_map(&hx.pool).await?;
        let candidates =
            load_candidates(&hx.pool, team_id, &sport, TRANSFER_DEFAULT_MIN_ARTICLES).await?;

        let mut unknown = 0usize;
        let mut errored = 0usize;
        for c in &candidates {
            // Model-failure UNKNOWN is not an infrastructure error: it is a successful fail-closed
            // row that the `unknown` tally turns into a team retry. DB/build/persist errors are
            // infrastructure failures; keep scanning pairs for visibility, then fail the team item.
            let pair = async {
                let out = match build_pair_request(
                    hx,
                    team_id,
                    &team_name,
                    c,
                    &sport,
                    &tiers,
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
                        vet_pair(hx, team_id, c.player_id, *ready).await?
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
                            role: Role::EmotionalNews.as_str().to_string(),
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
                    if let Some(heat) = out.heat {
                        maybe_apply_transfer_identity(
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
                        )
                        .await?;
                    }
                    // Phase 5.1 (transfer→sigil trigger): a freshly SERVED rumor is a Sigil pillar
                    // now, so re-trigger panel synthesis for the player and the team it touches.
                    // Best-effort — a failed enqueue must NOT fail the already-persisted rumor or
                    // stall the team item (the vibe→sigil gate's new_transfer branch and the next
                    // news event are fallbacks).
                    if row.is_rumor == Some(true) {
                        if let Err(e) = enqueue_sigil_for_transfer(
                            hx,
                            team_id,
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
                Ok(_) => {}
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

        if errored > 0 {
            bail!(
                "transfers: {errored} pair infrastructure/persist error(s) — retrying team {}",
                item.entity_id
            );
        }
        if unknown > 0 {
            bail!(
                "transfers: {unknown} unresolved pair(s) (model failure) — retrying team {}",
                item.entity_id
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(name: &str, nat: &str, club: &str, pos: &str) -> TransferCandidate {
        TransferCandidate {
            player_id: 1,
            player_name: name.to_string(),
            nationality: nat.to_string(),
            current_club: club.to_string(),
            position: pos.to_string(),
        }
    }

    // --- build_transfer_prompt byte-fixtures: the deterministic parity axis. The expected strings
    // are computed by hand from the Rust assembly, so prompt drift fails here (offline, no model).
    // -----------------------------------------------------------------------------------------------

    #[test]
    fn prompt_current_with_full_identity_and_news() {
        let c = cand("Bukayo Saka", "English", "Arsenal", "winger");
        let news = vec![NewsItem {
            id: 1,
            title: "Saka linked with move".to_string(),
            description: "Reports suggest interest.".to_string(),
            source: "BBC".to_string(),
        }];
        let evidence = TransferEvidence::from_news(&news, 1, "BBC", 0.9);
        let p = build_transfer_prompt("Arsenal", &c, "FOOTBALL", "current", &news, &evidence);
        assert_eq!(
            p,
            "Sport: FOOTBALL\nTeam: Arsenal\nPlayer: Bukayo Saka\n\
Identity (the ONE specific player to judge): Bukayo Saka · English · currently at Arsenal · winger\n\
Roster status: Bukayo Saka is CURRENTLY on Arsenal — so any move is a DEPARTURE (outgoing). Frame the summary as other clubs' interest in signing them.\n\
Evidence (computed): 1 article, 1 distinct source; strongest source: BBC (credibility 0.9).\n\
\nNews headlines:\n\
- [BBC] Saka linked with move — Reports suggest interest.\n\
\nReturn the JSON verdict now."
        );
    }

    #[test]
    fn prompt_former_sparse_identity_no_news() {
        // No nationality, unknown club, no position → identity is just name + "current club unknown".
        let c = cand("John Doe", "", "", "");
        let evidence = TransferEvidence::from_news(&[], 0, "", 0.0);
        let p = build_transfer_prompt("Chelsea", &c, "FOOTBALL", "former", &[], &evidence);
        assert_eq!(
            p,
            "Sport: FOOTBALL\nTeam: Chelsea\nPlayer: John Doe\n\
Identity (the ONE specific player to judge): John Doe · current club unknown\n\
Roster status: John Doe is a FORMER Chelsea player who has SINCE LEFT. A 'former/ex-Chelsea' mention is just background, NOT a transfer rumor — set is_rumor=false UNLESS the sources genuinely report John Doe RETURNING to Chelsea (then it is incoming).\n\
Evidence (computed): 0 articles, 0 distinct sources; strongest source: none attributed.\n\
\nNews headlines:\n\
- (none)\n\
\nReturn the JSON verdict now."
        );
    }

    #[test]
    fn prompt_none_sourceless_headline() {
        // relationship "none" (default arm) + a headline with no source (no "[src] " prefix).
        let c = cand("Victor Wembanyama", "French", "Spurs", "center");
        let news = vec![NewsItem {
            id: 1,
            title: "Trade buzz".to_string(),
            description: String::new(),
            source: String::new(),
        }];
        let evidence = TransferEvidence::from_news(&news, 1, "", 0.0);
        let p = build_transfer_prompt("Lakers", &c, "NBA", "none", &news, &evidence);
        assert_eq!(
            p,
            "Sport: NBA\nTeam: Lakers\nPlayer: Victor Wembanyama\n\
Identity (the ONE specific player to judge): Victor Wembanyama · French · currently at Spurs · center\n\
Roster status: Victor Wembanyama is NOT on Lakers — so any move is an ARRIVAL (incoming). Frame the summary as Lakers pursuing them.\n\
Evidence (computed): 1 article, 0 distinct sources; strongest source: none attributed.\n\
\nNews headlines:\n\
- Trade buzz\n\
\nReturn the JSON verdict now."
        );
    }

    // --- TransferParser fail-closed contract (mirrors Go TestParseTransferVerdictFailClosed) ------

    #[test]
    fn parser_fail_closed_on_non_json() {
        assert!(TransferParser.parse("").unwrap().is_none());
        assert!(TransferParser.parse("Sorry, no idea.").unwrap().is_none());
        assert!(TransferParser.parse("{not json").unwrap().is_none());
    }

    #[test]
    fn parser_missing_is_rumor_is_some_with_none_field() {
        // Parsed object that never committed to is_rumor → Some(verdict) with is_rumor == None; the
        // caller routes THAT to the UNKNOWN marker (Go's verdict.IsRumor == nil branch).
        let v = TransferParser
            .parse(r#"{"subject":"Someone","direction":"incoming"}"#)
            .unwrap()
            .expect("a parseable object is Some");
        assert_eq!(v.is_rumor, None);
        assert_eq!(v.subject, "Someone");
    }

    #[test]
    fn parser_accepts_negative_verdict_with_null_prose_fields() {
        let v = TransferParser
            .parse(
                r#"{"is_rumor":false,"subject":"Coby White","direction":"incoming","stage":null,"summary":null,"confidence":1.0}"#,
            )
            .unwrap()
            .expect("null prose fields should not erase committed false verdict");
        assert_eq!(v.is_rumor, Some(false));
        assert_eq!(v.subject, "Coby White");
        assert_eq!(v.direction, "incoming");
        assert_eq!(v.stage, "");
        assert_eq!(v.summary, "");
        assert_eq!(v.confidence, 1.0);
    }

    #[test]
    fn parser_extracts_committed_verdict_from_wrapped_json() {
        let v = TransferParser
            .parse("noise {\"is_rumor\": true, \"stage\": \"concrete_interest\", \"confidence\": 0.8} trailing")
            .unwrap()
            .expect("salvaged from wrapping");
        assert_eq!(v.is_rumor, Some(true));
        assert_eq!(v.stage, "concrete_interest");
        assert!((v.confidence - 0.8).abs() < 1e-9);
    }

    // --- norm_stage / clamp_conf (mirror Go TestNormStageDefaultsToSpeculation / TestClampConfBounds)

    #[test]
    fn norm_stage_normalizes_and_defaults() {
        assert_eq!(norm_stage("Concrete Interest"), "concrete_interest");
        assert_eq!(norm_stage("HERE_WE_GO"), "here_we_go");
        assert_eq!(norm_stage("nonsense"), "speculation");
        assert_eq!(norm_stage(""), "speculation");
    }

    #[test]
    fn clamp_conf_bounds() {
        assert_eq!(clamp_conf(-0.2), 0.0);
        assert_eq!(clamp_conf(1.5), 1.0);
        assert!((clamp_conf(0.73) - 0.73).abs() < 1e-9);
    }

    // --- the deterministic gates / mappings -------------------------------------------------------

    #[test]
    fn has_return_signal_detects_return_language() {
        let yes = vec![NewsItem {
            id: 1,
            title: "Star set to rejoin former club".to_string(),
            description: String::new(),
            source: "X".to_string(),
        }];
        let no = vec![NewsItem {
            id: 2,
            title: "Former player scores against old side".to_string(),
            description: "A routine match report.".to_string(),
            source: "X".to_string(),
        }];
        assert!(has_return_signal(&yes));
        assert!(!has_return_signal(&no));
    }

    #[test]
    fn identity_score_uses_raw_heat_only() {
        let (heat, confidence) = identity_apply_deterministic_score(83);

        assert_eq!(heat, 83);
        assert_eq!(confidence, 0.83);
    }

    #[test]
    fn direction_is_deterministic_from_relationship() {
        assert_eq!(direction_for("current"), "outgoing");
        assert_eq!(direction_for("former"), "incoming");
        assert_eq!(direction_for("none"), "incoming");
    }

    #[test]
    fn transfer_input_components_are_material_only() {
        // A realistic compute_transfer_heat components blob: the decay fields are PRESENT in the
        // input but must be EXCLUDED from the pre-image (heat, newest_age_hours, recency,
        // recent_3d, recent_frac tick with NOW(); total_14d/volume are id-set redundant).
        let comps = r#"{"distinct_sources": 3, "recent_3d": 2, "total_14d": 5,
            "newest_age_hours": 12.3, "tier_weight": 0.9, "volume": 0.6,
            "recency": 0.842, "recent_frac": 0.4}"#;
        assert_eq!(
            build_transfer_input_components(&[9, 4, 7], comps, "current"),
            r#"{"distinct_sources":3,"news_ids":[4,7,9],"relationship":"current","tier_weight":0.9}"#
        );
        // Empty/degenerate components (the defensive path) keep a stable pre-image.
        assert_eq!(
            build_transfer_input_components(&[], "{}", "none"),
            r#"{"distinct_sources":0,"news_ids":[],"relationship":"none","tier_weight":0}"#
        );
    }

    #[test]
    fn transfer_input_hash_ignores_id_order_and_decay_drift() {
        // The SAME corpus observed hours apart: ids identical (different order), every
        // NOW()-derived component moved. The fingerprint must NOT move — pure time decay
        // never re-runs the GPU (cooling is the read path's job).
        let fresh = r#"{"distinct_sources":2,"recent_3d":4,"total_14d":4,"newest_age_hours":1.0,
            "tier_weight":0.5,"volume":0.4,"recency":0.986,"recent_frac":1.0}"#;
        let aged = r#"{"distinct_sources":2,"recent_3d":1,"total_14d":4,"newest_age_hours":26.4,
            "tier_weight":0.5,"volume":0.4,"recency":0.693,"recent_frac":0.25}"#;
        let a = hash_components(&build_transfer_input_components(&[11, 3, 8, 5], fresh, "none"));
        let b = hash_components(&build_transfer_input_components(&[3, 5, 8, 11], aged, "none"));
        assert_eq!(a, b);
    }

    #[test]
    fn transfer_input_hash_moves_on_material_change() {
        let comps = r#"{"distinct_sources":2,"tier_weight":0.5}"#;
        let base = hash_components(&build_transfer_input_components(&[3, 5], comps, "none"));
        // New article in the pair corpus.
        let grown = hash_components(&build_transfer_input_components(&[3, 5, 9], comps, "none"));
        // Deterministic relationship flip (identity applied / provider update).
        let former = hash_components(&build_transfer_input_components(&[3, 5], comps, "former"));
        // Source-tier re-weighting (moves the grounding guard).
        let retiered = hash_components(&build_transfer_input_components(
            &[3, 5],
            r#"{"distinct_sources":2,"tier_weight":0.9}"#,
            "none",
        ));
        assert_ne!(base, grown);
        assert_ne!(base, former);
        assert_ne!(base, retiered);
    }

    #[test]
    fn unknown_marker_keeps_direction_drops_model() {
        // verdict == None → is_rumor NULL, direction kept (audit), model NULL. Mirrors Go persist.
        let (row, outcome) = row_from_verdict(None, "current", Some("BBC"), "mistral:7b");
        assert_eq!(outcome, Outcome::Unknown);
        assert_eq!(row.is_rumor, None);
        assert_eq!(row.direction.as_deref(), Some("outgoing"));
        assert_eq!(row.model, None);
        assert_eq!(row.attribution.as_deref(), Some("BBC"));
        assert_eq!(row.trigger_payload, "{}");
    }

    #[test]
    fn cleared_row_drops_direction_keeps_model_and_subject() {
        let v = TransferVerdict {
            is_rumor: Some(false),
            subject: "Florentino Pérez".to_string(),
            ..Default::default()
        };
        let (row, outcome) = row_from_verdict(Some(&v), "none", None, "mistral:7b");
        assert_eq!(outcome, Outcome::Cleared);
        assert_eq!(row.is_rumor, Some(false));
        assert_eq!(row.direction, None); // cleared keeps no direction (mirrors Go)
        assert_eq!(row.model.as_deref(), Some("mistral:7b"));
        assert_eq!(row.stage, None);
        assert!(row.trigger_payload.contains("Florentino"));
    }

    #[test]
    fn rumor_row_sets_all_vetted_fields() {
        let v = TransferVerdict {
            is_rumor: Some(true),
            subject: "Darwin Nunez".to_string(),
            stage: "advanced talks".to_string(), // normalized → advanced_talks
            summary: "  Liverpool eye Nunez  ".to_string(), // trimmed
            confidence: 1.4,                     // clamped → 1.0
            ..Default::default()
        };
        let (row, outcome) = row_from_verdict(Some(&v), "current", Some("Fabrizio"), "mistral:7b");
        assert_eq!(outcome, Outcome::Rumor);
        assert_eq!(row.is_rumor, Some(true));
        assert_eq!(row.direction.as_deref(), Some("outgoing"));
        assert_eq!(row.stage.as_deref(), Some("advanced_talks"));
        assert_eq!(row.summary.as_deref(), Some("Liverpool eye Nunez"));
        assert_eq!(row.confidence, Some(1.0));
        assert_eq!(row.model.as_deref(), Some("mistral:7b"));
    }

    #[test]
    fn t6_system_prompt_carries_the_false_heat_guards() {
        // The false-heat guards must be present and noun-correct.
        let football = transfer_system_prompt("FOOTBALL");
        assert!(football.contains("current transfer involving BOTH"));
        assert!(football.contains("roundup"));
        assert!(football.contains("recently completed"));
        assert!(football.contains("Never estimate, round, or invent money"));
        let nba = transfer_system_prompt("NBA");
        assert!(nba.contains("current trade involving BOTH")); // noun swap for NBA/NFL
    }

    #[test]
    fn identity_adjudication_parser_accepts_strict_apply_json() {
        let raw = r#"{
            "decision":"apply",
            "event_type":"transfer",
            "old_team_id":18,
            "new_team_id":42,
            "reason":"club announced the signing",
            "evidence_spans":["announced the signing"]
        }"#;
        let adj = TransferIdentityAdjudicationParser
            .parse(raw)
            .unwrap()
            .expect("strict JSON parses");
        assert_eq!(adj.decision, "apply");
        assert_eq!(adj.confidence, None);
        assert_eq!(adj.old_team_id, Some(18));
        assert_eq!(adj.new_team_id, 42);
    }

    #[test]
    fn identity_adjudication_parser_fails_closed_on_invalid_json_or_schema() {
        assert!(TransferIdentityAdjudicationParser
            .parse("not json")
            .unwrap()
            .is_none());
        assert!(TransferIdentityAdjudicationParser
            .parse(r#"{"decision":"apply"}"#)
            .unwrap()
            .is_none());
        assert!(TransferIdentityAdjudicationParser
            .parse(
                r#"{"decision":"apply","event_type":"transfer","confidence":1.4,"old_team_id":1,"new_team_id":2,"reason":"","evidence_spans":[]}"#
            )
            .unwrap()
            .is_none());
        assert!(TransferIdentityAdjudicationParser
            .parse(
                r#"{"decision":"maybe","event_type":"transfer","confidence":0.9,"old_team_id":1,"new_team_id":2,"reason":"","evidence_spans":[]}"#
            )
            .unwrap()
            .is_none());
        assert!(TransferIdentityAdjudicationParser
            .parse(
                r#"{"decision":"manual_review","event_type":"transfer","old_team_id":1,"new_team_id":2,"reason":"","evidence_spans":[]}"#
            )
            .unwrap()
            .is_none());
    }

    #[test]
    fn identity_adjudication_prompt_pins_candidate_ids() {
        let prompt = build_transfer_identity_adjudication_prompt(
            "FOOTBALL",
            7,
            "Example Player",
            Some(18),
            "Old FC",
            42,
            "New FC",
            &[NewsItem {
                id: 1,
                title: "New FC announce Example Player".to_string(),
                description: "The club confirmed the transfer.".to_string(),
                source: "BBC".to_string(),
            }],
        );
        assert!(prompt.contains("Current identity: team_id=18 team_name=Old FC"));
        assert!(prompt.contains("Proposed new identity: team_id=42 team_name=New FC"));
        assert!(prompt.contains("Decide only from the evidence articles"));
        assert!(!prompt.contains("heat"));
        assert!(!prompt.contains("Vetted summary"));
    }
}
