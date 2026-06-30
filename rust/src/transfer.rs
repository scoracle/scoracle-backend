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
//! the t4 identity-card framing in the system prompt (the model returns is_rumor AND subject in ONE
//! JSON, exactly as Go does); the standalone embedding-backed `resolve_one` for transfers is a
//! HORIZON refinement — it would restructure the one fused call into two and break Go-machinery
//! parity, so it waits (Plan §1.3 "an improvement, not parity").
//!
//! FAIL CLOSED (the §1.2 invariant): `is_rumor: Option<bool>` — a model timeout, unparseable output,
//! or a verdict that never committed to is_rumor persists an UNKNOWN row (is_rumor NULL), which is
//! NEVER served (every read requires `is_rumor IS TRUE`) and is counted so the team's stage item is
//! re-enqueued for a retry. Only a successful POSITIVE verdict ever becomes a served rumor.
//!
//! THE t4 PROMPT (the single-home change — this is its ONLY home, Go stays frozen at t3 to be
//! retired): t4 adds the **roundup/listicle clause** (a name in a multi-subject roundup / notes
//! column / power ranking / listicle is NOT a live rumor) and strengthens **never-invent-a-fee**
//! (state a fee/bid/figure/stage ONLY when the sources give it — no fabrication, no stage upgrade),
//! with an explicit stage-evidence ladder. This is the L9 false-heat root fix (mistral t3 confirmed
//! an "AFC Notes" roundup as concrete_interest + a fabricated $50m bid → false heat that then fed
//! the narratives draft a phantom). Everything else — loaders, the user prompt, options, persist —
//! is byte-faithful to Go t3 (the parity contract: `bin/transfer_parity` + the Go dump diff only
//! the `system` field, the deliberate t4 divergence).

use crate::harness::{Harness, Parser};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::util::truncate_bytes;
use crate::work::{Item, Stage};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use tracing::warn;

/// Prompt version — the single-home bump from Go's frozen `t3`. t4 = t3 + the roundup/listicle
/// clause + the strengthened never-invent-a-fee/stage instruction (the L9 false-heat root fix).
/// Go's `transferPromptVersion` stays "t3" (to be retired at cutover); this is the only "t4".
pub const TRANSFER_PROMPT_VERSION: &str = "t4";

/// Production vetting temperature (transfer.go uses 0.3). The parity harness overrides to 0.
pub const TRANSFER_TEMPERATURE: f64 = 0.3;

/// Token cap for the JSON verdict. Mirrors transfer.go's NumPredict: 1200.
pub const TRANSFER_NUM_PREDICT: i32 = 1200;

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

/// transfer_system_prompt is the t4 system prompt. `noun` is "trade" for NBA/NFL and "transfer"
/// otherwise (mirrors transfer.go). The JSON contract + field order are byte-identical to t3; the
/// added paragraphs are the roundup clause, the strengthened never-invent-a-fee, and the
/// stage-evidence ladder — the only intended divergence from Go (the parity diff localises here).
pub fn transfer_system_prompt(sport: &str) -> String {
    let noun = if sport == "NBA" || sport == "NFL" {
        "trade"
    } else {
        "transfer"
    };
    format!(
        r#"You are the seasoned beat reporter who tracks this team's {noun} market — you know a real move from noise, and you report only what the sources actually say, never inventing a fee, a bid, or a deal.

You are given an IDENTITY line describing ONE specific player (name, nationality, current club, position), the team's relationship to them, and the news. Decide whether the sources genuinely report a LIVE {noun} involving BOTH the named team AND THIS EXACT player — the same human as the identity line, not merely someone who shares the name.

Set is_rumor=false when any of these holds:
- The sources are really about a DIFFERENT person who shares the name — a club president/owner, a manager/coach, an unrelated figure, or a different player at another club (a midfielder named "Florentino" is NOT Florentino Pérez, the Real Madrid president — clear it).
- The current club or position in the sources contradicts the identity line (a different person).
- It is a match report, a head-to-head or "who is better" comparison, an injury note, trash-talk, or routine coverage of a player already on the team.
- The player is mentioned only as an OPPONENT or RIVAL of the team — a game or playoff result, a "how to stop him" / "address the X problem" angle, a defender cast as his "stopper", or a draft pick aimed at countering him. Competing AGAINST a team is not joining it; clear these.
- The move is not live — it already completed, or it is interest from a past window dredged up as background. Only a current, active rumor counts.
- The player is just one name in a multi-subject ROUNDUP, mailbag, notes column, power ranking, rumour wrap, or "X things to watch"/listicle that rattles off many players in passing. A name on a list is NOT a live rumor about THIS player — clear it unless the sources actually report active, specific interest in this exact player (not merely a passing mention among others).

Use the identity line — especially the current club — as the tie-breaker for same-name people. When unsure it is the same person, prefer is_rumor=false.

When it IS a live rumor, make the summary worth reading: one tight sentence that names the real counterparties and any CAPITAL the sources state — a fee ("around $50m", "a £40m bid"), or asset/pick compensation ("picks headed to the Raiders", "a pick swap") — attributed to the single most credible source named (not a list of outlets). Name the names; but state a fee, a bid, a figure, or a stage ONLY when the sources give it in words. If the sources name NO number, your summary names NO number — never estimate, round, or invent one (a fabricated "$50m" is a failure), and never upgrade the stage beyond what the sources actually report.

The stage must match the evidence the sources give: "speculation" for a mention, a link, or a roundup name-drop; "concrete_interest" only when the sources report the club actively pursuing this player; "advanced_talks"/"here_we_go" only for reported active negotiation or an agreed/imminent deal. When the evidence is thin, "speculation".

Reply with ONLY a JSON object, no prose:
{{"is_rumor": true|false, "subject": "who the sources are actually about (real name/person, even if NOT this player)", "direction": "incoming"|"outgoing"|"unclear", "stage": "speculation"|"concrete_interest"|"advanced_talks"|"here_we_go", "summary": "one tight sentence: who, which clubs, any fee or picks the sources actually state, attributed to the source", "confidence": 0.0-1.0}}

direction is relative to the named team: "incoming" = the team is signing the player; "outgoing" = the player is leaving. "subject" is the person's NAME only (e.g. "Darwin Nunez") — never copy the identity-card line. Always return every field, including confidence. If it is not a live {noun} about THIS exact player, set is_rumor=false; still fill "subject" with who the sources are really about."#
    )
}

/// One co-mention candidate player for a team + its identity-card disambiguators. Mirrors
/// `transferCandidate`.
#[derive(Clone, Debug)]
pub struct TransferCandidate {
    pub player_id: i32,
    pub player_name: String,
    pub nationality: String, // empty when unknown
    pub current_club: String, // latest-season club (player_current_team), NOT the stale players.team_id
    pub position: String,
}

/// One corpus news item for the (team, player) pair. Mirrors `newsItem` (the prompt uses
/// title/description/source; the SQL orders by published_at — not needed in Rust).
#[derive(Clone, Debug)]
pub struct NewsItem {
    pub title: String,
    pub description: String,
    pub source: String,
}

/// The model's JSON verdict (defensively parsed) — the `T` in `Parser<T>`. `is_rumor: Option<bool>`
/// is the fail-closed carrier (Plan §1.2): `None` ⇒ the model never committed ⇒ the UNKNOWN marker,
/// unrepresentable as a served row. Mirrors `gemmaTransferVerdict`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct TransferVerdict {
    pub is_rumor: Option<bool>,
    #[serde(default)]
    pub subject: String, // who the sources are really about (audit trail for discarded impostors)
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub stage: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub confidence: f64,
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
    Skipped, // no corpus (heat NULL) — no row written
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
    pub heat: Option<i16>,
    pub components: String, // heat_components jsonb text
    pub news_ids: Vec<i64>,
    pub outcome: Outcome,
    /// `None` ⇒ Skipped (no corpus → no row); `Some` for Rumor/Cleared/Unknown.
    pub row: Option<TransferRow>,
    /// The exact user prompt sent (the deterministic parity axis). `None` for Skipped (no call).
    pub built_prompt: Option<String>,
    /// The exact /api/generate wire body (captured by `extract`). `None` for Skipped.
    pub request_body: Option<serde_json::Value>,
    pub prompt_version: &'static str,
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
/// port of `transfer.go::loadCandidates` (current club from `player_current_team`, position from the
/// latest stats row; both vetted links required; co-mention proximity gate). SQL verbatim.
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
               COALESCE(NULLIF(pos.position, 'Unknown'), '')  AS position
        FROM news_article_entities te
        JOIN news_article_entities pe
          ON pe.article_id = te.article_id AND pe.sport = te.sport AND pe.entity_type = 'player'
        JOIN players p ON p.id = pe.entity_id AND p.sport = pe.sport
        LEFT JOIN public.player_current_team pct ON pct.player_id = p.id AND pct.sport = p.sport
        LEFT JOIN teams ct ON ct.id = pct.team_id AND ct.sport = p.sport
        LEFT JOIN LATERAL (
            SELECT ps.position FROM player_stats ps
            WHERE ps.player_id = p.id AND ps.sport = p.sport
            ORDER BY ps.season DESC NULLS LAST LIMIT 1
        ) pos ON true
        WHERE te.entity_type = 'team' AND te.entity_id = $1 AND te.sport = $2
          AND te.created_at > NOW() - INTERVAL '14 days'
          AND te.vetted IS TRUE
          AND pe.vetted IS TRUE
          AND (te.title_pos IS NULL OR pe.title_pos IS NULL
               OR abs(te.title_pos - pe.title_pos) <= $5)
        GROUP BY pe.entity_id, p.name, p.nationality, ct.name, pos.position
        HAVING count(DISTINCT te.article_id) >= $3
        ORDER BY count(DISTINCT te.article_id) DESC
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
            title: r.get("title"),
            description: r.get("description"),
            source: r.get("source"),
        })
        .collect())
}

/// team_relationship classifies the player's deterministic relationship to the team from
/// `player_stats` history: "current" (latest season on the team), "former" (on it in a past season
/// but not now), or "none". Drives `direction` and the former-player noise filter — NOT the model's
/// guess. Mirrors `teamRelationship`. SQL verbatim; `$1=player, $2=sport, $3=team`.
pub async fn team_relationship(
    pool: &PgPool,
    team_id: i32,
    player_id: i32,
    sport: &str,
) -> Result<String> {
    let row = sqlx::query(
        r#"
        SELECT
            COALESCE(bool_or(ps.team_id = $3 AND ps.season = (SELECT MAX(season) FROM player_stats WHERE player_id = $1 AND sport = $2)), false) AS is_current,
            COALESCE(bool_or(ps.team_id = $3), false) AS is_ever
        FROM player_stats ps
        WHERE ps.player_id = $1 AND ps.sport = $2
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
pub fn build_transfer_prompt(
    team_name: &str,
    c: &TransferCandidate,
    sport: &str,
    relationship: &str,
    news: &[NewsItem],
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
    pub news: Vec<NewsItem>,
    pub relationship: String,
    pub attribution: String,
    pub best_weight: f64,
    pub opts: GenerateOptions,
    pub built_prompt: String,
    pub request_body: serde_json::Value,
    pub model_configured: String,
}

/// build_pair_request runs the deterministic prefix: `compute_transfer_heat` (SQL — the number
/// stays Postgres), the pair corpus, the deterministic team relationship, then `build_transfer_prompt`
/// with the t4 options and the exact wire body. NO model call — these are the parity axes (the L2
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

    let news = load_pair_news(&hx.pool, &news_ids).await?;
    // Grounding: credibility attribution comes from the CORPUS, not the model.
    let (attribution, best_weight) = best_source(&news, tiers);
    // Direction + the noise filter key off the deterministic relationship, not the model's text.
    let relationship = team_relationship(&hx.pool, team_id, c.player_id, sport).await?;

    let built_prompt = build_transfer_prompt(team_name, c, sport, &relationship, &news);
    let opts = GenerateOptions {
        system: Some(transfer_system_prompt(sport)),
        temperature: Some(temperature),
        num_predict: TRANSFER_NUM_PREDICT,
        json_mode: true,
    };
    let backend = hx.router.for_role(Role::EmotionalNews);
    let request_body = backend.request_body(&built_prompt, &opts);
    let model_configured = backend.model().to_string();

    Ok(PairBuild::Ready(Box::new(PairReady {
        heat,
        components,
        news_ids,
        news,
        relationship,
        attribution,
        best_weight,
        opts,
        built_prompt,
        request_body,
        model_configured,
    })))
}

/// analyze_pair runs the full vetting for one (team, player) pair at the given temperature and
/// returns the un-persisted result (the L11 composition `extract+validate + subject-test + persist`,
/// minus the persist) — `build_pair_request` (deterministic) then `extract` (the model) then the
/// gates. Shared by the production handler (temp 0.3 → transfer_rumors) and, via the same builder,
/// the parity harness. Mirrors `transfer.go::analyzePair`. A generate failure is swallowed into an
/// UNKNOWN output (the fail-closed marker), NOT propagated — only a real DB/transport error returns
/// `Err` (the per-team loop counts it as Errored and moves on, exactly as Go does).
pub async fn analyze_pair(
    hx: &Harness,
    team_id: i32,
    team_name: &str,
    c: &TransferCandidate,
    sport: &str,
    tiers: &HashMap<String, f64>,
    temperature: f64,
) -> Result<TransferPairOutput> {
    let ready = match build_pair_request(hx, team_id, team_name, c, sport, tiers, temperature).await?
    {
        PairBuild::Skipped {
            components,
            news_ids,
        } => {
            return Ok(TransferPairOutput {
                player_id: c.player_id,
                heat: None,
                components,
                news_ids,
                outcome: Outcome::Skipped, // no corpus → no row (Go: res.Skipped++, return nil)
                row: None,
                built_prompt: None,
                request_body: None,
                prompt_version: TRANSFER_PROMPT_VERSION,
            });
        }
        PairBuild::Ready(r) => *r,
    };

    // route(EmotionalNews) + extract(TransferParser). A generate transport error → fail-closed
    // UNKNOWN row (Go persists UNKNOWN on a model timeout, then the team item is retried), recording
    // the prompt/body that WAS sent for the parity diff.
    let (verdict, built_prompt, request_body) = match hx
        .extract(Role::EmotionalNews, &ready.built_prompt, &ready.opts, &TransferParser)
        .await
    {
        Ok(extracted) => (
            extracted.value,
            Some(extracted.built_prompt),
            Some(extracted.request_body),
        ),
        Err(e) => {
            warn!(team = team_id, player = c.player_id, error = %e, "transfers: model generate failed; UNKNOWN (fail-closed)");
            (
                None,
                Some(ready.built_prompt.clone()),
                Some(ready.request_body.clone()),
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
        player_id: c.player_id,
        heat: Some(ready.heat),
        components: ready.components,
        news_ids: ready.news_ids,
        outcome,
        row: Some(row),
        built_prompt,
        request_body,
        prompt_version: TRANSFER_PROMPT_VERSION,
    })
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
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO transfer_rumors (
            team_id, player_id, sport, trigger_type, heat, heat_components,
            is_rumor, direction, stage, gemma_summary, source_attribution, confidence,
            input_news_ids, model_version, prompt_version, trigger_payload
        ) VALUES ($1,$2,$3,$4,$5,$6::jsonb,$7,$8,$9,$10,$11,$12::float8::numeric,$13,$14,$15,$16::jsonb)
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
    .bind(row.model.as_deref())
    .bind(out.prompt_version)
    .bind(&row.trigger_payload)
    .execute(pool)
    .await
    .context("persist transfer row")?;
    Ok(())
}

/// TransferHandler drains the team-keyed `transfers` stage: load the co-mention candidates and vet
/// each pair, persisting to transfer_rumors. Terminal — it enqueues nothing (the heat it writes is
/// read by vibe/narratives, which the mig-103 trigger enqueues). Any pair that hit a model failure
/// (UNKNOWN) fails the team's item so the queue's backoff re-runs it (the fail-closed retry —
/// mirrors `drainTransfers`). REGISTERED but NOT enabled until the transfers cutover (Step 3): the
/// Go Drainer still owns this stage, so running both would double-claim the one GPU.
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
        let sport = item.sport.to_uppercase();
        let team_name =
            crate::vibe::lookup_entity_name(&hx.pool, &item.entity_type, item.entity_id, &item.sport)
                .await?;
        let tiers = load_tier_map(&hx.pool).await?;
        let candidates =
            load_candidates(&hx.pool, item.entity_id, &sport, TRANSFER_DEFAULT_MIN_ARTICLES).await?;

        let mut unknown = 0usize;
        for c in &candidates {
            // One bad pair (DB/transport error, or a persist failure) must not kill the run — Go
            // counts it as Errored and moves on. The model-failure UNKNOWN is NOT an error here; it
            // is a successful fail-closed row that the `unknown` tally turns into a team retry.
            let pair = async {
                let out = analyze_pair(
                    hx,
                    item.entity_id,
                    &team_name,
                    c,
                    &sport,
                    &tiers,
                    TRANSFER_TEMPERATURE,
                )
                .await?;
                if let Some(row) = &out.row {
                    persist_transfer_row(
                        &hx.pool,
                        item.entity_id,
                        c.player_id,
                        &sport,
                        "periodic",
                        &out,
                        row,
                    )
                    .await?;
                }
                Ok::<Outcome, anyhow::Error>(out.outcome)
            }
            .await;
            match pair {
                Ok(Outcome::Unknown) => unknown += 1,
                Ok(_) => {}
                Err(e) => warn!(
                    team = item.entity_id,
                    player = c.player_id,
                    error = %e,
                    "transfers: pair errored; skipping (one bad pair won't kill the run)"
                ),
            }
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
    // are computed by hand from Go's buildTransferPrompt, so a drift in the Rust assembly fails here
    // (offline, no model) before the live diff ever runs. -------------------------------------------

    #[test]
    fn prompt_current_with_full_identity_and_news() {
        let c = cand("Bukayo Saka", "English", "Arsenal", "winger");
        let news = vec![NewsItem {
            title: "Saka linked with move".to_string(),
            description: "Reports suggest interest.".to_string(),
            source: "BBC".to_string(),
        }];
        let p = build_transfer_prompt("Arsenal", &c, "FOOTBALL", "current", &news);
        assert_eq!(
            p,
            "Sport: FOOTBALL\nTeam: Arsenal\nPlayer: Bukayo Saka\n\
Identity (the ONE specific player to judge): Bukayo Saka · English · currently at Arsenal · winger\n\
Roster status: Bukayo Saka is CURRENTLY on Arsenal — so any move is a DEPARTURE (outgoing). Frame the summary as other clubs' interest in signing them.\n\
\nNews headlines:\n\
- [BBC] Saka linked with move — Reports suggest interest.\n\
\nReturn the JSON verdict now."
        );
    }

    #[test]
    fn prompt_former_sparse_identity_no_news() {
        // No nationality, unknown club, no position → identity is just name + "current club unknown".
        let c = cand("John Doe", "", "", "");
        let p = build_transfer_prompt("Chelsea", &c, "FOOTBALL", "former", &[]);
        assert_eq!(
            p,
            "Sport: FOOTBALL\nTeam: Chelsea\nPlayer: John Doe\n\
Identity (the ONE specific player to judge): John Doe · current club unknown\n\
Roster status: John Doe is a FORMER Chelsea player who has SINCE LEFT. A 'former/ex-Chelsea' mention is just background, NOT a transfer rumor — set is_rumor=false UNLESS the sources genuinely report John Doe RETURNING to Chelsea (then it is incoming).\n\
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
            title: "Trade buzz".to_string(),
            description: String::new(),
            source: String::new(),
        }];
        let p = build_transfer_prompt("Lakers", &c, "NBA", "none", &news);
        assert_eq!(
            p,
            "Sport: NBA\nTeam: Lakers\nPlayer: Victor Wembanyama\n\
Identity (the ONE specific player to judge): Victor Wembanyama · French · currently at Spurs · center\n\
Roster status: Victor Wembanyama is NOT on Lakers — so any move is an ARRIVAL (incoming). Frame the summary as Lakers pursuing them.\n\
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
            title: "Star set to rejoin former club".to_string(),
            description: String::new(),
            source: "X".to_string(),
        }];
        let no = vec![NewsItem {
            title: "Former player scores against old side".to_string(),
            description: "A routine match report.".to_string(),
            source: "X".to_string(),
        }];
        assert!(has_return_signal(&yes));
        assert!(!has_return_signal(&no));
    }

    #[test]
    fn direction_is_deterministic_from_relationship() {
        assert_eq!(direction_for("current"), "outgoing");
        assert_eq!(direction_for("former"), "incoming");
        assert_eq!(direction_for("none"), "incoming");
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
    fn t4_system_prompt_carries_the_single_home_fix() {
        // The two t4 clauses that fix the L9 false-heat root must be present (and noun-correct).
        let football = transfer_system_prompt("FOOTBALL");
        assert!(football.contains("transfer market"));
        assert!(football.contains("ROUNDUP")); // the roundup/listicle clause
        assert!(football.contains("a fabricated \"$50m\" is a failure")); // never-invent-a-fee
        let nba = transfer_system_prompt("NBA");
        assert!(nba.contains("trade market")); // noun swap for NBA/NFL
    }
}
