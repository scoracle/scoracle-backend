//! Fixture Boxscore stage.
//!
//! This stage is fixture-keyed (`entity_type='fixture'`) and fetches completed game
//! box score payloads into `fixture_boxscore_fetches`.
//!
//! # State after mig 230 (2026-08-23): the vendor layer is gone, the public one is not built
//!
//! This seat used to read two PAID providers — balldontlie for NBA/NFL, sportmonks for
//! FOOTBALL — addressed by ids the seeding layer wrote into `provider_fixture_map`. Scott
//! retired that: box scores are public-event facts, so they get read from public sources. The
//! seeder was pruned (no 2026 fixture ever got a mapping), mig 230 dropped the map, and the
//! API tokens are unset. **All three legs were dead, so the vendor code is deleted rather than
//! left looking wired.**
//!
//! What survives here is the SHELL and the SUBSTRATE: the claim/validate/persist path, the
//! provenance ledger, the fingerprint, and the family-independent normalization helpers (each
//! marked `#[allow(dead_code)]` — they await their first parser family, they are not rot).
//! [`select_source`], [`fetch_source`] and [`parse_fetched_boxscore`] are deliberately inert
//! and every fixture takes the honest `no_source` terminal path.
//!
//! The build that fills them in is discovery → retrieval → interpretation (`discover.rs:1`):
//! the Investigator discovers public sources into `boxscore_sources`, retrieval goes through
//! [`crate::fetch::BudgetedFetcher`], and interpretation is a per-family CODE parser.
//!
//! It also does not write `event_box_scores` or `event_team_stats` — promoting a validated
//! fetch into those canonical tables is a deliberate later step, not a side effect.

use crate::stage::StageHandler;
use crate::util::truncate;
use crate::work::{Item, Stage};
use crate::{harness::Harness, util::hash_components};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::StatusCode;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet};
use tracing::warn;

pub const FIXTURE_BOXSCORE_STAGE: &str = "fixture_boxscore";
pub const FIXTURE_BOXSCORE_PARSER_VERSION: &str = "fixture-boxscore-parser-v1";
pub const FIXTURE_BOXSCORE_OUTPUT_CONTRACT_VERSION: &str = "fixture-boxscore-v1";

/// The fixture's own facts — which, since mig 230, are the ONLY address a box score has.
///
/// `season`, `home_team_name`, `away_team_name`, `round` and `external_id` are unread while
/// [`select_source`] is inert, and they are kept deliberately: discovery addresses a public
/// page by teams, competition and round, so these are its query, not leftovers.
#[allow(dead_code)]
#[derive(Clone, Debug)]
struct FixtureRow {
    id: i32,
    sport: String,
    season: i32,
    league_id: i32,
    home_team_id: i32,
    away_team_id: i32,
    home_team_name: String,
    away_team_name: String,
    status: String,
    home_score: Option<i32>,
    away_score: Option<i32>,
    round: String,
    external_id: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct SourcePlan {
    pub provider: String,
    pub provider_fixture_id: Option<String>,
    pub source_urls: Vec<String>,
    pub official_url: Option<String>,
}

/// A retrieved document. `value` is unread until the first parser family lands — it is the
/// payload that family will read (mig 230).
#[allow(dead_code)]
#[derive(Debug)]
struct FetchedJson {
    source_url: String,
    final_url: String,
    final_domain: Option<String>,
    value: Value,
    warnings: Vec<String>,
}

#[derive(Debug)]
struct NormalizedBoxscore {
    provider_status: Option<String>,
    score: Value,
    period_scoring: Value,
    team_stats: Value,
    player_stats: Value,
    raw_labels: Value,
}

#[derive(Debug)]
struct PersistRecord {
    provider: String,
    source_url: Option<String>,
    final_url: Option<String>,
    final_domain: Option<String>,
    status: String,
    content_hash: Option<String>,
    score: Value,
    period_scoring: Value,
    team_stats: Value,
    player_stats: Value,
    raw_labels: Value,
    parser_outcome: String,
    last_error: Option<String>,
}

pub struct FixtureBoxscoreHandler;

impl FixtureBoxscoreHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FixtureBoxscoreHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for FixtureBoxscoreHandler {
    fn stage(&self) -> Stage {
        Stage::FixtureBoxscore
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        if item.entity_type != "fixture" {
            return Err(anyhow!(
                "fixture_boxscore requires entity_type='fixture', got {}",
                item.entity_type
            ));
        }

        let fixture_id = item.entity_id_i32()?;
        let Some(fixture) = load_fixture(&hx.pool, fixture_id).await? else {
            return Ok(());
        };

        if !is_final_fixture_status(&fixture.status) {
            persist_record(
                hx,
                &fixture,
                PersistRecord::terminal(
                    "none",
                    "not_final",
                    Some(format!("fixture status is {}", fixture.status)),
                ),
            )
            .await?;
            return Ok(());
        }

        let plan = select_source(&fixture);
        if plan.source_urls.is_empty() {
            // No registered public source for this fixture. Terminal and honest — this is the
            // state of every fixture until the Investigator's discovery arm populates
            // `boxscore_sources` (mig 230). The old branch here tested for the literal provider
            // "unsupported", which only ever meant "sport outside the vendor match".
            persist_record(
                hx,
                &fixture,
                PersistRecord::terminal(
                    &plan.provider,
                    "no_source",
                    Some(format!(
                        "no public source registered for sport {}",
                        fixture.sport
                    )),
                ),
            )
            .await?;
            return Ok(());
        }

        let fetched = match fetch_source(&fixture, &plan).await {
            Ok(f) => f,
            Err(FetchOutcome {
                status,
                source_url,
                final_url,
                final_domain,
                error,
            }) => {
                persist_record(
                    hx,
                    &fixture,
                    PersistRecord::terminal_with_urls(
                        &plan.provider,
                        &status,
                        source_url.as_deref(),
                        final_url.as_deref(),
                        final_domain.as_deref(),
                        Some(error),
                    ),
                )
                .await?;
                return Ok(());
            }
        };

        let normalized = match parse_fetched_boxscore(&fixture, &plan, &fetched) {
            Ok(n) => n,
            Err(ParseOutcome { status, error }) => {
                persist_record(
                    hx,
                    &fixture,
                    PersistRecord::terminal_with_urls(
                        &plan.provider,
                        &status,
                        Some(&fetched.source_url),
                        Some(&fetched.final_url),
                        fetched.final_domain.as_deref(),
                        Some(error),
                    ),
                )
                .await?;
                return Ok(());
            }
        };

        if provider_status_is_not_final(normalized.provider_status.as_deref()) {
            persist_record(
                hx,
                &fixture,
                PersistRecord::terminal_with_urls(
                    &plan.provider,
                    "not_final",
                    Some(&fetched.source_url),
                    Some(&fetched.final_url),
                    fetched.final_domain.as_deref(),
                    Some(format!(
                        "provider status is {}",
                        normalized.provider_status.unwrap_or_default()
                    )),
                ),
            )
            .await?;
            return Ok(());
        }

        if let Err(reason) = validate_normalized(&fixture, &normalized) {
            persist_record(
                hx,
                &fixture,
                PersistRecord::terminal_with_urls(
                    &plan.provider,
                    "validation_failed",
                    Some(&fetched.source_url),
                    Some(&fetched.final_url),
                    fetched.final_domain.as_deref(),
                    Some(reason),
                ),
            )
            .await?;
            return Ok(());
        }

        let payload_for_hash = json!({
            "provider": plan.provider,
            "score": normalized.score,
            "period_scoring": normalized.period_scoring,
            "team_stats": normalized.team_stats,
            "player_stats": normalized.player_stats,
        });
        let content_hash = boxscore_content_hash(&payload_for_hash);
        persist_record(
            hx,
            &fixture,
            PersistRecord {
                provider: plan.provider.clone(),
                source_url: Some(fetched.source_url),
                final_url: Some(fetched.final_url),
                final_domain: fetched.final_domain,
                status: "success".to_string(),
                content_hash: Some(content_hash),
                score: payload_for_hash["score"].clone(),
                period_scoring: payload_for_hash["period_scoring"].clone(),
                team_stats: payload_for_hash["team_stats"].clone(),
                player_stats: payload_for_hash["player_stats"].clone(),
                raw_labels: merge_raw_labels(normalized.raw_labels, fetched.warnings, &plan),
                parser_outcome: "deterministic".to_string(),
                last_error: None,
            },
        )
        .await?;
        Ok(())
    }
}

impl PersistRecord {
    fn terminal(provider: &str, status: &str, error: Option<String>) -> Self {
        Self::terminal_with_urls(provider, status, None, None, None, error)
    }

    fn terminal_with_urls(
        provider: &str,
        status: &str,
        source_url: Option<&str>,
        final_url: Option<&str>,
        final_domain: Option<&str>,
        error: Option<String>,
    ) -> Self {
        Self {
            provider: provider.to_string(),
            source_url: source_url.map(str::to_string),
            final_url: final_url.map(str::to_string),
            final_domain: final_domain.map(str::to_string),
            status: status.to_string(),
            content_hash: None,
            score: json!({}),
            period_scoring: json!([]),
            team_stats: json!([]),
            player_stats: json!([]),
            raw_labels: json!({}),
            parser_outcome: "no_call".to_string(),
            last_error: error,
        }
    }
}

async fn load_fixture(pool: &sqlx::PgPool, fixture_id: i32) -> Result<Option<FixtureRow>> {
    let row = sqlx::query(
        r#"
        -- The vendor CASE that used to head this query (sport → 'bdl'/'sportmonks') and the
        -- LEFT JOIN onto provider_fixture_map both went with mig 230. The fixture's own facts
        -- are the whole input now: a public source is addressed by teams, date and competition,
        -- not by a third party's id.
        SELECT f.id, f.sport, f.season, COALESCE(f.league_id, 0) AS league_id,
               f.home_team_id, f.away_team_id,
               COALESCE(ht.name, '') AS home_team_name,
               COALESCE(at.name, '') AS away_team_name,
               f.status, f.home_score, f.away_score,
               COALESCE(f.round, '') AS round,
               f.external_id
        FROM public.fixtures f
        LEFT JOIN public.teams ht
          ON ht.id = f.home_team_id AND ht.sport = f.sport
        LEFT JOIN public.teams at
          ON at.id = f.away_team_id AND at.sport = f.sport
        WHERE f.id = $1
        "#,
    )
    .bind(fixture_id)
    .fetch_optional(pool)
    .await
    .context("load fixture for fixture_boxscore")?;

    Ok(row.map(|r| FixtureRow {
        id: r.get("id"),
        sport: r.get("sport"),
        season: r.get("season"),
        league_id: r.get("league_id"),
        home_team_id: r.get("home_team_id"),
        away_team_id: r.get("away_team_id"),
        home_team_name: r.get("home_team_name"),
        away_team_name: r.get("away_team_name"),
        status: r.get("status"),
        home_score: r.get("home_score"),
        away_score: r.get("away_score"),
        round: r.get("round"),
        external_id: r.get("external_id"),
    }))
}

/// select_source picks where this fixture's box score will be read from.
///
/// **The paid-provider era ended here (mig 230, 2026-08-23.)** This used to be a `match` on
/// sport that returned one hardcoded vendor per sport — balldontlie for NBA/NFL, sportmonks for
/// FOOTBALL — keyed by an id looked up in `provider_fixture_map`. All three legs of that are
/// gone: the seeding layer that wrote the map was pruned, so no 2026 fixture ever got a mapping;
/// the map itself is dropped; and the API tokens are not configured. Scott's ruling: box scores
/// are public-event facts and get read from public sources.
///
/// The replacement reads `boxscore_sources` — a registry of DISCOVERED public sources carrying
/// their own `url_template`, `parser_family`, `fetch_policy` and `trust_state`. Until the
/// Investigator's discovery arm populates it, this returns an empty plan and every fixture takes
/// the honest `no_source` terminal path rather than pretending a vendor is still there.
fn select_source(_fixture: &FixtureRow) -> SourcePlan {
    SourcePlan {
        provider: "none".to_string(),
        provider_fixture_id: None,
        source_urls: vec![],
        official_url: None,
    }
}

#[derive(Debug)]
struct FetchOutcome {
    status: String,
    source_url: Option<String>,
    final_url: Option<String>,
    final_domain: Option<String>,
    error: String,
}

/// fetch_source retrieves the planned document.
///
/// **It has no implementation right now, and that is the honest state of the seat** (mig 230).
/// The two vendor clients that used to live here read `BALLDONTLIE_API_KEY` and
/// `SPORTMONKS_API_TOKEN`, neither of which is configured, against ids from a table that no
/// longer exists. Leaving them in place would have meant a stage that looks wired and cannot
/// work — the scar tissue Scott asked to cut.
///
/// **The replacement does NOT build its own `reqwest::Client`.** It goes through
/// [`crate::fetch::BudgetedFetcher`], which already enforces, per domain: concurrency 1, a 2s
/// minimum spacing (the 4.2 floor), `429`/`Retry-After` honoured as a hold, a circuit breaker
/// at four consecutive failures, and a `source_documents` provenance row for every retrieval
/// with `cache_ttl` reuse. Its policy is loaded from `boxscore_sources.fetch_policy`. That
/// substrate was FOUNDED for box scores (`fetch.rs`: "founded in Phase 4 (box scores), reused
/// by Phase 5") and then only ever used by entity discovery — this path went direct to the
/// vendors instead. Wiring it back is step one of the public-source build.
///
/// The stated posture travels with it: *"A domain that blocks direct fetch is a domain we skip
/// — never stealth, no browser automation on this path."*
async fn fetch_source(
    _fixture: &FixtureRow,
    plan: &SourcePlan,
) -> std::result::Result<FetchedJson, FetchOutcome> {
    Err(FetchOutcome::new(
        "no_source",
        plan.source_urls.first().cloned(),
        None,
        None,
        "no public source is registered for this fixture (boxscore_sources is empty)",
    ))
}

impl FetchOutcome {
    fn new(
        status: &str,
        source_url: Option<String>,
        final_url: Option<String>,
        final_domain: Option<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            status: status.to_string(),
            source_url,
            final_url,
            final_domain,
            error: error.into(),
        }
    }
}

#[allow(dead_code)] // parser-family substrate (mig 230)
fn http_fetch_outcome(
    status: StatusCode,
    source_url: &str,
    final_url: &str,
    label: &str,
) -> FetchOutcome {
    let stage_status = match status.as_u16() {
        401 | 403 | 429 => "blocked",
        404 => "not_found",
        _ => "fetch_failed",
    };
    FetchOutcome::new(
        stage_status,
        Some(source_url.to_string()),
        Some(final_url.to_string()),
        domain_of(final_url),
        format!("{label} HTTP {}", status.as_u16()),
    )
}

#[derive(Debug)]
struct ParseOutcome {
    status: String,
    error: String,
}

impl ParseOutcome {
    fn new(status: &str, error: impl Into<String>) -> Self {
        Self {
            status: status.to_string(),
            error: error.into(),
        }
    }
}

/// parse_fetched_boxscore turns a retrieved document into the normalized shape.
///
/// The three vendor parsers that used to be dispatched here went with mig 230. Their
/// replacement is a **parser family** — `boxscore_sources.parser_family` names which one a
/// source belongs to, so the model classifies a page into a family and CODE does the
/// extraction. That split is the house doctrine, stated at `discover.rs:1`: interpretation is
/// *"either CODE over structured claims (the preferred path — no model call at all) or
/// [a model] describing a prose page (the fallback)"*.
///
/// The normalization substrate BELOW this function survived deliberately —
/// [`normalized_from_parts`], [`extract_numeric_stats`], [`parse_minutes`], [`stats_to_json`]
/// and friends are family-independent, carry the Go-compatible number formatting, and are what
/// the first family will be built on top of.
fn parse_fetched_boxscore(
    fixture: &FixtureRow,
    plan: &SourcePlan,
    _fetched: &FetchedJson,
) -> std::result::Result<NormalizedBoxscore, ParseOutcome> {
    Err(ParseOutcome::new(
        "not_supported",
        format!(
            "no parser family registered for source={} sport={}",
            plan.provider, fixture.sport
        ),
    ))
}

#[allow(dead_code)] // parser-family substrate (mig 230)
fn normalized_from_parts(
    provider: &str,
    provider_status: Option<String>,
    fixture: &FixtureRow,
    team_scores: BTreeMap<i32, i32>,
    team_acc: BTreeMap<i32, BTreeMap<String, f64>>,
    players: Vec<Value>,
    period_scoring: Value,
    raw_labels: Value,
) -> NormalizedBoxscore {
    let score = json!({
        "home_team_id": fixture.home_team_id,
        "away_team_id": fixture.away_team_id,
        "league_id": fixture.league_id,
        "home_score": team_scores.get(&fixture.home_team_id).copied().or(fixture.home_score),
        "away_score": team_scores.get(&fixture.away_team_id).copied().or(fixture.away_score),
        "provider": provider,
    });

    let mut team_ids = BTreeSet::new();
    team_ids.insert(fixture.home_team_id);
    team_ids.insert(fixture.away_team_id);
    for id in team_acc.keys() {
        team_ids.insert(*id);
    }
    for id in team_scores.keys() {
        team_ids.insert(*id);
    }

    let empty_stats = BTreeMap::new();
    let teams: Vec<Value> = team_ids
        .into_iter()
        .map(|team_id| {
            json!({
                "provider_team_id": team_id,
                "team_id": team_id,
                "side": if team_id == fixture.home_team_id { "home" } else if team_id == fixture.away_team_id { "away" } else { "unknown" },
                "score": team_scores.get(&team_id).copied()
                    .or_else(|| if team_id == fixture.home_team_id { fixture.home_score } else if team_id == fixture.away_team_id { fixture.away_score } else { None }),
                "stats": stats_to_json(team_acc.get(&team_id).unwrap_or(&empty_stats)),
                "raw_labels": {"provider": provider}
            })
        })
        .collect();

    NormalizedBoxscore {
        provider_status,
        score,
        period_scoring,
        team_stats: Value::Array(teams),
        player_stats: Value::Array(players),
        raw_labels,
    }
}

fn validate_normalized(
    fixture: &FixtureRow,
    n: &NormalizedBoxscore,
) -> std::result::Result<(), String> {
    let teams = n
        .team_stats
        .as_array()
        .ok_or_else(|| "team_stats is not an array".to_string())?;
    let has_home = teams
        .iter()
        .any(|t| i32_at(t, "team_id") == Some(fixture.home_team_id));
    let has_away = teams
        .iter()
        .any(|t| i32_at(t, "team_id") == Some(fixture.away_team_id));
    if !has_home || !has_away {
        return Err(format!(
            "missing expected teams home={} away={} (has_home={} has_away={})",
            fixture.home_team_id, fixture.away_team_id, has_home, has_away
        ));
    }
    if n.score.get("home_score").and_then(Value::as_i64).is_none()
        || n.score.get("away_score").and_then(Value::as_i64).is_none()
    {
        return Err("missing final score".to_string());
    }
    if !n.player_stats.as_array().is_some_and(|a| !a.is_empty()) {
        return Err("missing player rows".to_string());
    }
    Ok(())
}

fn provider_status_is_not_final(status: Option<&str>) -> bool {
    let Some(status) = status else {
        return false;
    };
    let s = status.trim().to_lowercase();
    if s.is_empty() {
        return false;
    }
    let final_statuses = [
        "final",
        "ft",
        "aet",
        "after extra time",
        "penalties",
        "after penalties",
        "full time",
        "finished",
        "ended",
    ];
    if final_statuses.iter().any(|needle| s.contains(needle)) {
        return false;
    }
    true
}

fn is_final_fixture_status(status: &str) -> bool {
    matches!(status, "completed" | "seeded")
}

#[allow(dead_code)] // parser-family substrate (mig 230)
fn extract_numeric_stats(
    row: &Value,
    skip_keys: &[&str],
    explicit_stats_key: Option<&str>,
) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    if let Some(key) = explicit_stats_key {
        if let Some(obj) = row.get(key).and_then(Value::as_object) {
            for (k, v) in obj {
                if let Some(n) = numeric_value(v) {
                    out.insert(k.clone(), n);
                }
            }
        }
    }
    if let Some(obj) = row.as_object() {
        for (k, v) in obj {
            if skip_keys.iter().any(|skip| skip == k) {
                continue;
            }
            if let Some(n) = numeric_value(v) {
                out.insert(k.clone(), n);
            }
        }
    }
    out
}

#[allow(dead_code)] // parser-family substrate (mig 230)
fn add_stats(
    acc: &mut BTreeMap<i32, BTreeMap<String, f64>>,
    team_id: i32,
    stats: &BTreeMap<String, f64>,
) {
    let team = acc.entry(team_id).or_default();
    for (k, v) in stats {
        *team.entry(k.clone()).or_insert(0.0) += *v;
    }
}

#[allow(dead_code)] // parser-family substrate (mig 230)
fn stats_to_json(stats: &BTreeMap<String, f64>) -> Value {
    let mut obj = Map::new();
    for (k, v) in stats {
        obj.insert(k.clone(), json_number(*v));
    }
    Value::Object(obj)
}

#[allow(dead_code)] // parser-family substrate (mig 230)
fn json_number(n: f64) -> Value {
    if n.fract() == 0.0 {
        json!(n as i64)
    } else {
        json!(n)
    }
}

fn numeric_value(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

#[allow(dead_code)] // parser-family substrate (mig 230)
fn parse_minutes(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) if s.contains(':') => {
            let mut parts = s.split(':');
            let minutes = parts.next()?.parse::<f64>().ok()?;
            let seconds = parts.next()?.parse::<f64>().ok()?;
            Some(((minutes + seconds / 60.0) * 100.0).round() / 100.0)
        }
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

#[allow(dead_code)] // parser-family substrate (mig 230)
fn player_name(raw: Option<&Value>) -> Option<String> {
    let raw = raw?.as_object()?;
    let first = raw
        .get("first_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let last = raw
        .get("last_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = format!("{first} {last}").trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn i32_at(v: &Value, key: &str) -> Option<i32> {
    v.get(key)
        .and_then(numeric_value)
        .and_then(|n| i32::try_from(n as i64).ok())
}

#[allow(dead_code)] // parser-family substrate (mig 230)
fn nested_i32(v: &Value, path: &[&str]) -> Option<i32> {
    let mut cur = v;
    for key in path {
        cur = cur.get(*key)?;
    }
    numeric_value(cur).and_then(|n| i32::try_from(n as i64).ok())
}

#[allow(dead_code)] // parser-family substrate (mig 230)
fn nested_string(v: &Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for key in path {
        cur = cur.get(*key)?;
    }
    cur.as_str().map(str::to_string)
}

async fn persist_record(hx: &Harness, fixture: &FixtureRow, record: PersistRecord) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO public.fixture_boxscore_fetches (
            fixture_id, sport, provider, source_url, final_url, final_domain,
            status, content_hash, score, period_scoring, team_stats, player_stats,
            raw_labels, model_version, prompt_version, parser_version,
            output_contract_version, parser_outcome, last_error, fetched_at, updated_at
        ) VALUES (
            $1, $2, $3, $4, $5, $6,
            $7, $8, $9::jsonb, $10::jsonb, $11::jsonb, $12::jsonb,
            $13::jsonb, NULL, NULL, $14,
            $15, $16, $17, NOW(), NOW()
        )
        ON CONFLICT (fixture_id) DO UPDATE SET
            sport = EXCLUDED.sport,
            provider = EXCLUDED.provider,
            source_url = EXCLUDED.source_url,
            final_url = EXCLUDED.final_url,
            final_domain = EXCLUDED.final_domain,
            status = EXCLUDED.status,
            content_hash = EXCLUDED.content_hash,
            score = EXCLUDED.score,
            period_scoring = EXCLUDED.period_scoring,
            team_stats = EXCLUDED.team_stats,
            player_stats = EXCLUDED.player_stats,
            raw_labels = EXCLUDED.raw_labels,
            model_version = NULL,
            prompt_version = NULL,
            parser_version = EXCLUDED.parser_version,
            output_contract_version = EXCLUDED.output_contract_version,
            parser_outcome = EXCLUDED.parser_outcome,
            last_error = EXCLUDED.last_error,
            fetched_at = NOW(),
            updated_at = NOW()
        "#,
    )
    .bind(fixture.id)
    .bind(&fixture.sport)
    .bind(&record.provider)
    .bind(record.source_url.as_deref())
    .bind(record.final_url.as_deref())
    .bind(record.final_domain.as_deref())
    .bind(&record.status)
    .bind(record.content_hash.as_deref())
    .bind(&record.score)
    .bind(&record.period_scoring)
    .bind(&record.team_stats)
    .bind(&record.player_stats)
    .bind(&record.raw_labels)
    .bind(FIXTURE_BOXSCORE_PARSER_VERSION)
    .bind(FIXTURE_BOXSCORE_OUTPUT_CONTRACT_VERSION)
    .bind(&record.parser_outcome)
    .bind(record.last_error.as_deref().map(|e| truncate(e, 1000)))
    .execute(&hx.pool)
    .await
    .with_context(|| format!("persist fixture_boxscore {}", fixture.id))?;

    insert_data_fetch_ledger_best_effort(hx, fixture, &record).await;
    Ok(())
}

async fn insert_data_fetch_ledger_best_effort(
    hx: &Harness,
    fixture: &FixtureRow,
    record: &PersistRecord,
) {
    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO public.data_fetch_ledger (
            target_type, target_id, sport, stage, status, source_url, final_url, final_domain,
            content_hash, model_version, prompt_version, output_contract_version,
            parser_outcome, error, generated_at
        ) VALUES (
            'fixture', $1, $2, $3, $4, $5, $6, $7,
            $8, NULL, NULL, $9, $10, $11, NOW()
        )
        "#,
    )
    .bind(fixture.id)
    .bind(&fixture.sport)
    .bind(FIXTURE_BOXSCORE_STAGE)
    .bind(&record.status)
    .bind(record.source_url.as_deref())
    .bind(record.final_url.as_deref())
    .bind(record.final_domain.as_deref())
    .bind(record.content_hash.as_deref())
    .bind(FIXTURE_BOXSCORE_OUTPUT_CONTRACT_VERSION)
    .bind(&record.parser_outcome)
    .bind(record.last_error.as_deref().map(|e| truncate(e, 1000)))
    .execute(&hx.pool)
    .await
    {
        warn!(
            fixture_id = fixture.id,
            status = %record.status,
            error = %e,
            "fixture_boxscore: data_fetch_ledger insert failed (continuing)"
        );
    }
}

fn merge_raw_labels(raw_labels: Value, warnings: Vec<String>, plan: &SourcePlan) -> Value {
    let mut obj = raw_labels.as_object().cloned().unwrap_or_default();
    if !warnings.is_empty() {
        obj.insert("warnings".to_string(), json!(warnings));
    }
    obj.insert(
        "source_urls".to_string(),
        json!({
            "fetched": plan.source_urls,
            "official": plan.official_url,
        }),
    );
    Value::Object(obj)
}

fn boxscore_content_hash(value: &Value) -> String {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    let digest = Sha256::digest(serialized.as_bytes());
    hex::encode(&digest[..16])
}

#[allow(dead_code)] // parser-family substrate (mig 230)
fn domain_of(raw_url: &str) -> Option<String> {
    reqwest::Url::parse(raw_url).ok().and_then(|u| {
        u.host_str()
            .map(|h| h.trim_start_matches("www.").to_lowercase())
    })
}

/// The queue fingerprint for a fixture box-score demand.
///
/// **This MUST stay byte-identical to `public.fixture_boxscore_input_version(integer)`** — the
/// SQL function is what `enqueue_fixture_boxscore` actually stamps onto `pipeline_work`, and
/// this is the Rust mirror. If they disagree, every enqueue reopens a row the other side
/// considers unchanged, and the stage churns.
///
/// `fbf1` → `fbf2` (mig 230): the provider-map leg left the string with the paid seeding layer.
/// The score is what says "this fixture is final and these are its numbers", which is the whole
/// signal the fingerprint needs; a public source is addressed by the fixture, not by a vendor
/// id. The prefix is bumped rather than the string quietly reshaped, so a fingerprint whose
/// MEANING changed cannot be mistaken for one that drifted.
pub fn build_fixture_boxscore_input_version(
    sport: &str,
    season: i32,
    league_id: i32,
    home_team_id: i32,
    away_team_id: i32,
    home_score: Option<i32>,
    away_score: Option<i32>,
) -> String {
    format!(
        "fbf2:{sport}:{season}:{league_id}:{home_team_id}:{away_team_id}:{}:{}",
        home_score.map(|s| s.to_string()).unwrap_or_default(),
        away_score.map(|s| s.to_string()).unwrap_or_default(),
    )
}

#[allow(dead_code)]
fn input_version_hash_for_tests(version: &str) -> String {
    hash_components(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(sport: &str) -> FixtureRow {
        FixtureRow {
            id: 9901,
            sport: sport.to_string(),
            season: 2025,
            league_id: 0,
            home_team_id: match sport {
                "FOOTBALL" => 1,
                _ => 8,
            },
            away_team_id: match sport {
                "FOOTBALL" => 2,
                _ => 14,
            },
            home_team_name: "Buffalo Bills".to_string(),
            away_team_name: "Kansas City Chiefs".to_string(),
            status: "completed".to_string(),
            home_score: Some(28),
            away_score: Some(21),
            round: "Week 9".to_string(),
            external_id: Some(12345),
        }
    }

    /// Every sport takes the `no_source` path until `boxscore_sources` has rows.
    ///
    /// This replaces `source_selection_uses_existing_provider_ids`, which asserted the two
    /// paid vendors' URL shapes (mig 230 retired them). The assertion that matters now is the
    /// inverse: NOTHING resolves, for any sport, and the seat says so honestly rather than
    /// producing a vendor URL it can no longer fetch.
    #[test]
    fn no_sport_resolves_a_source_until_the_registry_is_populated() {
        for sport in ["NBA", "NFL", "FOOTBALL"] {
            let plan = select_source(&fixture(sport));
            assert!(
                plan.source_urls.is_empty(),
                "{sport} resolved a source from an empty registry"
            );
            assert_eq!(plan.provider, "none");
            assert!(plan.provider_fixture_id.is_none());
        }
    }

    #[test]
    fn terminal_status_helpers_are_closed() {
        assert!(is_final_fixture_status("completed"));
        assert!(is_final_fixture_status("seeded"));
        assert!(!is_final_fixture_status("scheduled"));
        assert!(!provider_status_is_not_final(Some("Final")));
        assert!(!provider_status_is_not_final(Some("FT")));
        assert!(provider_status_is_not_final(Some("In Progress")));
    }

    /// The fingerprint is the fixture's own facts, and it must mirror the SQL function that
    /// actually stamps `pipeline_work`.
    ///
    /// This replaces `input_version_is_order_stable`, whose whole subject was sorting the
    /// provider pairs that mig 230 removed. What has to hold now: the `fbf2` prefix (so a
    /// re-issued fingerprint is legible as a MEANING change, not drift), and that the score —
    /// the signal that says the fixture is final with these numbers — still moves it.
    #[test]
    fn input_version_mirrors_the_sql_fingerprint() {
        let v = build_fixture_boxscore_input_version("NFL", 2025, 0, 1, 2, Some(10), Some(7));
        assert_eq!(v, "fbf2:NFL:2025:0:1:2:10:7");

        // A changed score is a changed fixture: the row must reopen.
        assert_ne!(
            v,
            build_fixture_boxscore_input_version("NFL", 2025, 0, 1, 2, Some(11), Some(7))
        );

        // An unscored fixture renders empty legs rather than the word "None".
        assert_eq!(
            build_fixture_boxscore_input_version("NBA", 2026, 4, 8, 14, None, None),
            "fbf2:NBA:2026:4:8:14::"
        );
    }

    #[test]
    fn content_hash_changes_with_payload() {
        let a = boxscore_content_hash(&json!({"score": {"home": 1}}));
        let b = boxscore_content_hash(&json!({"score": {"home": 2}}));
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}
