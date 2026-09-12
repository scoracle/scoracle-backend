//! Transfers stage — team-keyed transfer/trade rumor vetting.
//!
//! Per team/subject pair this composes model extraction, validation, subject matching, and
//! persistence.
//! The deterministic parts stay where they belong: `compute_transfer_heat`, the `direction`, and
//! the team relationship are SQL/Postgres (the model never computes the number or the direction);
//! the model ONLY vets: is this a live rumor about THIS exact player, what stage, and a grounded
//! one-line summary. The subject same-person test is realised as the verdict's `subject` field plus
//! the identity-card framing in the system prompt; both fields come back in one JSON object.
//!
//! FAIL CLOSED (the §1.2 invariant): `is_rumor: Option<bool>` — a model timeout, unparseable output,
//! or a verdict that never committed to is_rumor persists an UNKNOWN row (is_rumor NULL), which is
//! NEVER served (every read requires `is_rumor IS TRUE`) and is counted so the team's stage item is
//! re-enqueued for a retry. Only a successful POSITIVE verdict ever becomes a served rumor.
//!
//! Before each model call, the handler fingerprints MATERIAL pair inputs — sorted corpus IDs, the
//! corpus-stable source diversity, and the deterministic relationship; no timestamps, no prose,
//! no recency decay. A resolved row with the same hash skips generation and persistence. UNKNOWN
//! markers never satisfy the gate, so a
//! model-failure retry re-vets ONLY the failed pair: the completed pairs skip on fingerprint
//! instead of repeating every call in a team batch.

use crate::corpus::{load_transfer_heat, HeatItem};
use crate::harness::{EntityKey, Generation, GenerationCall, Harness, Parser};
use crate::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::trajectory::{classify_delta, DEFAULT_TRAJECTORY};
use crate::util::{hash_components, truncate_bytes};
use crate::work::{Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

mod application;
use application::{
    bank_transfer_junction_event, load_transfer_identity_threshold, maybe_apply_transfer_identity,
    refresh_sport_autofill_concurrently,
};
mod inputs;
pub mod prompt;
pub use crate::junctions::form::insider_score_format_schema;
pub use inputs::build_insider_score_prompt;
pub use prompt::{INSIDER_SCORE_PROMPT_VERSION, INSIDER_SCORE_SYSTEM_PROMPT};
mod verification;
pub use verification::{
    build_transfer_identity_adjudication_prompt, build_transfer_prompt,
    transfer_identity_adjudication_system_prompt, transfer_system_prompt,
    TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION, TRANSFER_PROMPT_VERSION,
    TRANSFER_PROMPT_VERSION_PERSON,
};

/// Output schema version for transfer adjudication JSON, distinct from the prompt contract.
pub const TRANSFER_OUTPUT_CONTRACT_VERSION: &str = "transfer-verdict-v1";

const TRANSFER_LEDGER: LedgerSpec = LedgerSpec {
    stage: "transfers",
    lens: "transfer",
    role: Role::TransferLogic,
    product_table: "transfer_rumors",
    output_contract_version: TRANSFER_OUTPUT_CONTRACT_VERSION,
};

/// Production vetting temperature.
pub const TRANSFER_TEMPERATURE: f64 = 0.3;

/// Token cap for the JSON verdict.
pub const TRANSFER_NUM_PREDICT: i32 = 900;

/// Corpus + candidate governors.
const TRANSFER_MAX_CORPUS_NEWS: i64 = 12;
/// Default candidate pre-filter (min co-mention articles / 14d).
pub const TRANSFER_DEFAULT_MIN_ARTICLES: i32 = 2;
const TRANSFER_MAX_CANDIDATES: i32 = 40;

// Self-pacing against the worker's per-item ceiling. Reserve time for the wire wrap after the
// variable-length pair loop; defer remaining pairs rather than cancelling them.

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

/// Co-mention candidate and identity-card disambiguators. Subjects may be players or coaches.
#[derive(Clone, Debug)]
pub struct TransferCandidate {
    pub player_id: i32,
    pub player_name: String,
    pub nationality: String,  // empty when unknown
    pub current_club: String, // canonical current club (player_current_identity)
    pub position: String,
    /// 'player' → public.players, 'person' → public.persons. Part of the pair key
    /// everywhere downstream — the two id sequences overlap.
    pub subject_type: String,
    /// Person subjects carry their relationship from persons.team_id directly; the
    /// player batch (`team_relationships`) is player-id keyed and must not see
    /// person ids (collision). None for players.
    pub relationship_override: Option<String>,
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

/// Defensively parsed model verdict. `is_rumor: None` becomes an unserved UNKNOWN marker.
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

/// Raw identity-adjudication schema. Field order matches the prompt contract and survives the
/// trip to Ollama unchanged.
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
/// only when there is no JSON object or it is unparseable; a parsed verdict
/// whose `is_rumor` is absent surfaces as `Some(verdict)` with `is_rumor == None`, and the caller
/// routes that to the UNKNOWN marker. The first-`{`…last-`}` slice tolerates response wrapping.
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
    Skipped, // no corpus or unchanged material; no row written
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
pub struct TransferPairProduct {
    pub player_id: i32,
    /// Which table `player_id` names: `player` or `person`.
    pub subject_type: String,
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
    /// Evidence retained for the optional post-persist identity adjudication gate. Empty for
    /// skipped/no-corpus pairs.
    pub identity_apply_news: Vec<NewsItem>,
}

pub type TransferPairOutput = Generation<TransferPairProduct>;

// ---------------------------------------------------------------------------
// Loaders.
// ---------------------------------------------------------------------------

/// Return a team's corroborated co-mention candidates with identity cards.
pub async fn load_candidates(
    pool: &PgPool,
    team_id: i32,
    sport: &str,
    min_articles: i32,
) -> Result<Vec<TransferCandidate>> {
    let rows = sqlx::query(
        r#"
        SELECT * FROM (
            SELECT pe.entity_id, p.name,
                   COALESCE(p.nationality, '')                    AS nationality,
                   COALESCE(ct.name, '')                          AS current_club,
                   COALESCE(NULLIF(pci.position, 'Unknown'), '')  AS position,
                   'player'::text                                 AS subject_type,
                   NULL::text                                     AS relationship_override,
                   max(a.topic_heat)                              AS topic_heat,
                   count(DISTINCT te.article_id)                  AS article_n
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

            UNION ALL

            -- Coach candidates use the same co-mention and corroboration gate.
            -- Executives and agents are not transfer subjects.
            SELECT pe.entity_id, pp.full_name AS name,
                   ''::text                                       AS nationality,
                   COALESCE(ct.name, '')                          AS current_club,
                   'Head Coach'::text                             AS position,
                   'person'::text                                 AS subject_type,
                   CASE WHEN pp.team_id = $1 THEN 'current' ELSE 'none' END
                                                                  AS relationship_override,
                   max(a.topic_heat)                              AS topic_heat,
                   count(DISTINCT te.article_id)                  AS article_n
            FROM news_article_entities te
            JOIN news_article_entities pe
              ON pe.article_id = te.article_id AND pe.sport = te.sport AND pe.entity_type = 'person'
            JOIN news_articles a ON a.id = te.article_id
            JOIN public.persons pp ON pp.id = pe.entity_id AND pp.sport = pe.sport AND pp.kind = 'coach'
            LEFT JOIN teams ct ON ct.id = pp.team_id AND ct.sport = pp.sport
            WHERE te.entity_type = 'team' AND te.entity_id = $1 AND te.sport = $2
              AND a.bucket IS DISTINCT FROM 'non_transfer'
              AND te.created_at > NOW() - INTERVAL '14 days'
            GROUP BY pe.entity_id, pp.full_name, ct.name, pp.team_id
            HAVING count(DISTINCT te.article_id) >= $3
        ) u
        WHERE u.article_n >= $3
        ORDER BY u.topic_heat DESC NULLS LAST, u.article_n DESC
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
            subject_type: r.get("subject_type"),
            relationship_override: r.get("relationship_override"),
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
    subject_type: &str,
) -> Result<(Option<i16>, String, Vec<i64>)> {
    let row = sqlx::query(
        "SELECT heat, COALESCE(components::text, '{}') AS components, news_ids \
         FROM compute_transfer_heat($1, $2, $3, $4)",
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .bind(subject_type)
    .fetch_one(pool)
    .await
    .context("compute transfer heat")?;
    let heat: Option<i16> = row.get("heat");
    let components: String = row.get("components");
    let news_ids: Option<Vec<i64>> = row.get("news_ids");
    Ok((heat, components, news_ids.unwrap_or_default()))
}

/// load_pair_news returns the pair's corpus headlines, newest first, capped — the model's grounding.
/// `published_at` stays in the ordering without entering the model-facing row.
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

/// Load the Editor's transfer claims for the pair's existing corpus. Postgres still decides pair
/// identity, heat, and corpus IDs; the packet replaces only the article text shown to the model.
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

/// Batch [`team_relationship`] over a team's candidate set. Batch and single-pair reads must
/// agree because relationship is part of the material fingerprint.
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

/// Deterministic transfer prompt evidence: corpus size/diversity, source track record, and the
/// pair's relational memory. Byte fixtures pin this assembly.
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
/// branching of the deleted `transfer.go::persist`:
///
///   * `verdict == None`        → UNKNOWN: is_rumor NULL, direction kept (audit), model NULL.
///   * is_rumor == Some(true)   → a vetted rumor: direction/stage/summary/confidence/model set.
///   * is_rumor == Some(false)  → cleared: is_rumor FALSE + model set, the rest left NULL.
///
/// `model_configured` is the role's configured model.
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
                    // The wire line serves AS the transfers board's headline, so it takes the
                    // shared scrub like every served field — found bare in the 08-23 review
                    // pass (trimmed and truncated, never emphasis-stripped: a bolded summary
                    // shipped its asterisks to the board).
                    let s = crate::guards::clean_served_prose(&v.summary);
                    (!s.is_empty()).then(|| truncate_bytes(&s, SUMMARY_TRUNCATE))
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

/// Builds the canonical per-pair debounce pre-image from the sorted corpus ids,
/// source diversity, relationship, and prompt version.
///
/// Time-derived heat, redundant aggregates, prose, and enrichment cards are excluded so
/// decay alone cannot trigger generation. A prompt-version change triggers one regeneration.
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
    serde_json::json!({
        "distinct_sources": distinct_sources,
        "news_ids": ids,
        "prompt_version": TRANSFER_PROMPT_VERSION,
        "relationship": relationship,
    })
    .to_string()
}

/// Returns true when the latest resolved pair row carries this input hash.
/// UNKNOWN and unstamped rows never satisfy the gate.
pub async fn pair_unchanged(
    pool: &PgPool,
    team_id: i32,
    player_id: i32,
    sport: &str,
    subject_type: &str,
    input_hash: &str,
) -> Result<bool> {
    let latest: Option<(Option<String>, Option<bool>)> = sqlx::query_as(
        "SELECT input_hash, is_rumor FROM transfer_rumors \
         WHERE team_id = $1 AND player_id = $2 AND sport = $3 AND subject_type = $4 \
         ORDER BY generated_at DESC LIMIT 1",
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .bind(subject_type)
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
    /// 'player' | 'person' — carried so vet_pair stamps the output without re-deriving.
    pub subject_type: String,
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
    /// Per-pair debounce fingerprint over the material inputs.
    pub input_hash: String,
}

/// Loads the graph's model-facing memory card for the pair.
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

/// Loads the model-facing source track-record card for the pair's live corpus.
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

/// Builds the deterministic pair inputs and exact request body without calling the model.
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
        compute_pair_heat(&hx.pool, team_id, c.player_id, sport, &c.subject_type).await?;
    let Some(heat) = heat else {
        return Ok(PairBuild::Skipped {
            components,
            news_ids,
        });
    };

    let mut news = load_pair_news(&hx.pool, &news_ids).await?;
    // The packet rail may replace article prose, never corpus membership.
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

    // Fingerprint material inputs before the handler decides whether to call the model.
    let input_hash = hash_components(&build_transfer_input_components(
        &news_ids,
        &components,
        &relationship,
    ));

    let evidence = TransferEvidence::from_news(&news, news_ids.len(), &attribution);
    // Two independent SQL card reads (story arc + source track record) — load concurrently.
    // Person subjects skip both: those reads are player-id keyed, and the persons/players
    // id sequences overlap — a coach's id would silently read a same-id player's memory.
    let (memory, source_reliability) = if c.subject_type == "person" {
        (None, None)
    } else {
        tokio::try_join!(
            load_relational_memory(&hx.pool, sport, c.player_id, team_id),
            load_source_reliability(&hx.pool, sport, c.player_id, team_id),
        )?
    };
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
    // Person subjects use the same contract with a separately versioned noun substitution.
    let system = if c.subject_type == "person" {
        transfer_system_prompt(sport).replace("player", "person")
    } else {
        transfer_system_prompt(sport)
    };
    let opts = GenerateOptions {
        system: Some(system),
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
        subject_type: c.subject_type.clone(),
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
    subject_type: &str,
    components: String,
    news_ids: Vec<i64>,
) -> TransferPairOutput {
    let model = hx.router.for_role(Role::TransferLogic).model().to_string();
    let prompt_version = if subject_type == "person" {
        TRANSFER_PROMPT_VERSION_PERSON
    } else {
        TRANSFER_PROMPT_VERSION
    };
    Generation::uncalled(
        TransferPairProduct {
            player_id,
            subject_type: subject_type.to_string(),
            heat: None,
            components,
            news_ids,
            prompted_news_ids: Vec::new(),
            stale_news_ids: Vec::new(),
            outcome: Outcome::Skipped,
            row: None,
            identity_apply_news: Vec::new(),
        },
        model,
        prompt_version,
        Vec::new(),
        None,
    )
}

/// Vets one pair without persisting it. Generate failures become UNKNOWN outputs;
/// database and transport errors are returned.
pub async fn analyze_pair(
    hx: &Harness,
    team_id: i32,
    team_name: &str,
    c: &TransferCandidate,
    sport: &str,
    temperature: f64,
) -> Result<TransferPairOutput> {
    // Offline composition loads the relationship per pair; the production handler batches it.
    // Person subjects carry their own (persons.team_id-derived) — the player relationship
    // read must never see a person id.
    let relationship = match &c.relationship_override {
        Some(r) => r.clone(),
        None => team_relationship(&hx.pool, team_id, c.player_id, sport).await?,
    };
    match build_pair_request(hx, team_id, team_name, c, sport, relationship, temperature).await? {
        PairBuild::Skipped {
            components,
            news_ids,
        } => Ok(skipped_pair_output(
            hx,
            c.player_id,
            &c.subject_type,
            components,
            news_ids,
        )),
        PairBuild::Ready(r) => vet_pair(hx, team_id, c.player_id, sport, *r).await,
    }
}

/// Runs the model and deterministic post-model gates over a built pair request.
pub async fn vet_pair(
    hx: &Harness,
    team_id: i32,
    player_id: i32,
    sport: &str,
    ready: PairReady,
) -> Result<TransferPairOutput> {
    // Load audit-only stale evidence alongside the model call.
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
    let (verdict, model, call) = match extract_result {
        Ok(extracted) => {
            let call = GenerationCall::from(&extracted);
            (extracted.value, extracted.model, call)
        }
        Err(e) => {
            warn!(team = team_id, player = player_id, error = %e, "transfers: model generate failed; UNKNOWN (fail-closed)");
            (
                None,
                ready.model_configured.clone(),
                GenerationCall {
                    built_prompt: ready.built_prompt.clone(),
                    request_body: ready.request_body.clone(),
                    eval_count: None,
                    wall_ms: None,
                },
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

    let prompt_version = if ready.subject_type == "person" {
        TRANSFER_PROMPT_VERSION_PERSON
    } else {
        TRANSFER_PROMPT_VERSION
    };
    let input_ids = ready.news_ids.clone();
    Ok(Generation::called(
        TransferPairProduct {
            player_id,
            subject_type: ready.subject_type,
            heat: Some(ready.heat),
            components: ready.components,
            news_ids: ready.news_ids,
            prompted_news_ids: ready.prompted_news_ids,
            stale_news_ids,
            outcome,
            row: Some(row),
            identity_apply_news: ready.news,
        },
        model,
        prompt_version,
        input_ids,
        Some(ready.input_hash),
        call,
    ))
}

fn transfer_components_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!({ "raw": s }))
}

fn transfer_trigger_payload_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or(serde_json::Value::Null)
}

/// persist_transfer_row writes ONE row to the LIVE transfer_rumors table — the scored rumor, the
/// cleared row, and the UNKNOWN marker, which differ only in the bound values. Mirrors the
/// deleted `transfer.go::persist` (generated_at defaults NOW()).
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
            model_version, prompt_version, trigger_payload, input_hash, subject_type
        ) VALUES (
            $1,$2,$3,$4,$5,$6::jsonb,$7,$8,$9,$10,$11,$12::float8::numeric,$13,
            COALESCE(to_timestamp($14::double precision), NOW()), $15, $16,
            to_timestamp($17::double precision), to_timestamp($18::double precision),
            $19, $20::jsonb,
            $21,$22,$23::jsonb,$24,$25
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
    .bind(out.provenance.prompt_version)
    .bind(&row.trigger_payload)
    .bind(out.provenance.input_hash.as_deref())
    .bind(&out.subject_type)
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
          AND subject_type = $4
          AND heat IS NOT NULL
        ORDER BY generated_at DESC
        LIMIT 1
        "#,
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .bind(&out.subject_type)
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

/// Offers a newly served player rumor to the Oracle's completion barrier.
/// The worker offers the team after completing the transfers item.
async fn enqueue_sigil_for_transfer(
    hx: &Harness,
    player_id: i32,
    sport: &str,
    rumor_id: i64,
) -> Result<()> {
    let input_version = Some(rumor_id.to_string());
    // The team cannot pass the barrier until this handler's work row is complete.
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
// The Insider's card score: one wrap per touched entity after pair verdicts are filed.
// It versions independently and never enters a pair's debounce fingerprint.
// ---------------------------------------------------------------------------

/// Output contract for the wire wrap, captured in the cognition ledger.
pub const INSIDER_SCORE_OUTPUT_CONTRACT_VERSION: &str = "insider-score-v1";

const INSIDER_SCORE_LEDGER: LedgerSpec = LedgerSpec {
    stage: "transfers",
    lens: "insider_score",
    role: Role::TransferLogic,
    product_table: "insider_scores",
    output_contract_version: INSIDER_SCORE_OUTPUT_CONTRACT_VERSION,
};

/// Vetting-grade temperature: the wrap is a judgment of the board, not creative prose.
pub const INSIDER_SCORE_TEMPERATURE: f64 = 0.3;

/// Token cap for the `{read, score}` reply (mirrors the crown's budget; a 1-2 sentence read
/// plus one integer).
// The wire carries several calls, so it needs more than a single-read seat and less than the
// two story seats: most boards are quiet and an honest filing is short.
pub const INSIDER_SCORE_NUM_PREDICT: i32 = 600;

/// Validated read, entity-level headline, and 1-99 score.
#[derive(Clone, Debug)]
pub struct InsiderScoreReply {
    pub read: String,
    /// A hook accepted by the shared title floor, if one is usable.
    pub headline: Option<String>,
    pub score: i16,
}

/// Parses a wrap reply, cleaning served prose and clamping its score to the tarot range.
fn parse_insider_score_reply(raw: &str) -> Option<InsiderScoreReply> {
    let trimmed = raw.trim();
    let parsed: Option<serde_json::Value> = serde_json::from_str(trimmed).ok().or_else(|| {
        let start = trimmed.find('{')?;
        let end = trimmed.rfind('}')?;
        serde_json::from_str(&trimmed[start..=end]).ok()
    });
    let v = parsed?;
    let read = v.get("read")?.as_str()?.trim();
    let read = crate::junctions::form::normalize_body(read);
    let read = crate::guards::clean_served_prose(&read);
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
    // Only titles accepted by the shared floor may persist.
    let headline =
        crate::guards::settle_title("insider", v.get("headline").and_then(|h| h.as_str()));
    Some(InsiderScoreReply {
        read,
        headline,
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
    serde_json::json!({
        "board": lines,
        "prompt_version": INSIDER_SCORE_PROMPT_VERSION,
    })
    .to_string()
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

/// Prompt bytes each remembered read body may spend (the influencer BODY_TRUNCATE precedent):
/// the memory tells the developing story, it never re-files it.
const PRIOR_READ_BODY_TRUNCATE: usize = 280;

/// Renders recent read bodies and the latest score as prompt-only continuity memory.
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
    for (i, (_, read, day)) in rows.iter().enumerate() {
        let Some(body) = read.as_deref().filter(|r| !r.trim().is_empty()) else {
            continue;
        };
        let label = if i == 0 { "Your last read" } else { "Earlier" };
        card.push_str(&format!(
            "{label} ({day}): {}\n",
            crate::util::truncate_bytes(body, PRIOR_READ_BODY_TRUNCATE)
        ));
    }
    card.push_str(&format!("Your latest score: {}", rows[0].0));
    Ok(Some(PriorInsiderRead {
        latest: rows[0].0,
        card,
    }))
}

/// Returns players on the team's recent served rumors, a superset of the active board.
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
        // A never-scored empty wire skips. A previously scored wire files one quiet close;
        // the empty-board hash then debounces further calls until the board revives.
        let has_row = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM insider_scores WHERE entity_type = $1 AND entity_id = $2 AND sport = $3)",
        )
        .bind(entity_type)
        .bind(entity_id)
        .bind(sport)
        .fetch_one(&hx.pool)
        .await?;
        if !has_row {
            return Ok(());
        }
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
    // Identity card: house records, dated — degrades to absent like memory.
    let identity = crate::corpus::load_identity_card(&hx.pool, entity_type, entity_id, sport)
        .await
        .unwrap_or_default();
    let prompt = build_insider_score_prompt(
        entity_name,
        sport,
        entity_type,
        &heat,
        prior.as_ref().map(|p| p.card.as_str()),
        identity.as_deref(),
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
    let call = GenerationCall::from(&extracted);
    let model = extracted.model.clone();
    let reply = extracted.value.ok_or_else(|| {
        anyhow!("insider score: parser returned None (InsiderScoreParser signals failure via Err)")
    })?;
    let previous_score = prior.as_ref().map(|p| p.latest);
    let generation = Generation::called(
        (),
        model,
        INSIDER_SCORE_PROMPT_VERSION,
        Vec::new(),
        Some(input_hash.clone()),
        call,
    );

    let row = sqlx::query(
        r#"
        INSERT INTO insider_scores (
            sport, entity_type, entity_id, score, previous_score, read, headline,
            model_version, prompt_version, input_hash, generated_at
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,NOW())
        RETURNING id
        "#,
    )
    .bind(sport)
    .bind(entity_type)
    .bind(entity_id)
    .bind(reply.score)
    .bind(previous_score)
    .bind(reply.read.as_str())
    .bind(reply.headline.as_deref())
    .bind(generation.provenance.model_version.as_str())
    .bind(generation.provenance.prompt_version)
    .bind(generation.provenance.input_hash.as_deref())
    .fetch_one(&hx.pool)
    .await
    .context("persist insider score")?;
    let row_id: i64 = row.get("id");

    insert_generation_ledger_best_effort(
        &hx.pool,
        &generation,
        INSIDER_SCORE_LEDGER,
        LedgerEvent {
            entity_type,
            entity_id,
            sport,
            pair_entity: None,
            trigger_type: "periodic",
            trigger_payload: serde_json::Value::Null,
            product_row_ids: vec![row_id],
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
            context_budget: generation.context_budget(serde_json::json!({
                "num_predict": INSIDER_SCORE_NUM_PREDICT,
            })),
            parser_outcome: "parsed",
        },
    )
    .await;
    Ok(())
}

/// Drains team-keyed transfers: vet pairs, persist verdicts, and wrap each touched entity.
/// UNKNOWN or infrastructure failures retry the item; resolved unchanged pairs debounce-skip.
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
        // Load relationships and the identity threshold once per team.
        // Player subjects only — person ids collide with player ids, and person
        // relationships arrive on the candidate itself (relationship_override).
        let player_ids: Vec<i32> = candidates
            .iter()
            .filter(|c| c.subject_type == "player")
            .map(|c| c.player_id)
            .collect();
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
                // matching the per-pair COALESCE(false) defaults. Person subjects carry
                // their own relationship (persons.team_id-derived).
                let relationship = c.relationship_override.clone().unwrap_or_else(|| {
                    relationships
                        .get(&c.player_id)
                        .cloned()
                        .unwrap_or_else(|| "none".to_string())
                });
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
                    } => {
                        skipped_pair_output(hx, c.player_id, &c.subject_type, components, news_ids)
                    }
                    PairBuild::Ready(ready) => {
                        // An unchanged resolved pair keeps serving without another model call.
                        if pair_unchanged(
                            &hx.pool,
                            team_id,
                            c.player_id,
                            &sport,
                            &c.subject_type,
                            &ready.input_hash,
                        )
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
                    let included_evidence = serde_json::json!({
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
                    });
                    let mut excluded = Vec::new();
                    let excluded_reason = match out.outcome {
                        Outcome::Cleared => Some("model_cleared_pair"),
                        Outcome::Unknown => Some("model_unknown_or_generate_failure"),
                        _ => None,
                    };
                    if let Some(reason) = excluded_reason {
                        excluded.push(serde_json::json!({
                            "reason": reason,
                            "trigger_payload": transfer_trigger_payload_json(&row.trigger_payload),
                        }));
                    }
                    if out.news_ids.len() > out.prompted_news_ids.len() {
                        let prompted: std::collections::HashSet<_> =
                            out.prompted_news_ids.iter().copied().collect();
                        let dropped_news_ids: Vec<_> = out
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
                    insert_generation_ledger_best_effort(
                        &hx.pool,
                        &out,
                        TRANSFER_LEDGER,
                        LedgerEvent {
                            entity_type: "team",
                            entity_id: team_id,
                            sport: &sport,
                            pair_entity: Some(("player", c.player_id)),
                            trigger_type: "periodic",
                            trigger_payload: transfer_trigger_payload_json(&row.trigger_payload),
                            product_row_ids: vec![persisted_rumor_id],
                            included_evidence,
                            excluded_evidence: serde_json::json!(excluded),
                            context_budget: out.context_budget(serde_json::json!({
                                "num_predict": TRANSFER_NUM_PREDICT,
                            })),
                            parser_outcome: match out.outcome {
                                Outcome::Rumor => "rumor",
                                Outcome::Cleared => "cleared",
                                Outcome::Unknown => "unknown",
                                Outcome::Skipped => "skipped",
                            },
                        },
                    )
                    .await;
                    // Bank served rumors for memory and audit, outside the numeric loop.
                    if row.is_rumor == Some(true) {
                        if let Some(stage) = row.stage.as_deref() {
                            if let Err(e) = bank_transfer_junction_event(
                                &hx.pool,
                                &sport,
                                c.player_id,
                                team_id,
                                stage,
                                &out.provenance.model_version,
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
                    // Best-effort: a newly served rumor should refresh the player's Sigil.
                    if row.is_rumor == Some(true) {
                        if let Err(e) =
                            enqueue_sigil_for_transfer(hx, c.player_id, &sport, persisted_rumor_id)
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

        // Refresh autofill once per team drain after all applied pairs.
        if autofill_refresh_wanted {
            refresh_sport_autofill_concurrently(&hx.pool, &sport, "applied_transfer_identity")
                .await?;
        }

        // Wrap the team, all candidates, and every player on the served board. This runs last
        // under its own budget; dead or unchanged boards skip inside the scorer.
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
        // Defer a progressing partial drain before charging unresolved pairs to the retry ladder.
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
