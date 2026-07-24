//! Fixture Boxscore stage.
//!
//! This stage is fixture-keyed (`entity_type='fixture'`) and fetches completed game
//! box score payloads into `fixture_boxscore_fetches`. It does not write
//! `event_box_scores` or `event_team_stats`; those canonical tables stay owned by
//! the existing seeder until provider stat mappings are promoted intentionally.

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
use std::time::Duration;
use tracing::warn;

pub const FIXTURE_BOXSCORE_STAGE: &str = "fixture_boxscore";
pub const FIXTURE_BOXSCORE_PARSER_VERSION: &str = "fixture-boxscore-parser-v1";
pub const FIXTURE_BOXSCORE_OUTPUT_CONTRACT_VERSION: &str = "fixture-boxscore-v1";
const FETCH_TIMEOUT: Duration = Duration::from_secs(25);
const MAX_BDL_PAGES: usize = 20;

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
    provider: String,
    provider_fixture_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SourcePlan {
    pub provider: String,
    pub provider_fixture_id: Option<String>,
    pub source_urls: Vec<String>,
    pub official_url: Option<String>,
}

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
                    &fixture.provider,
                    "not_final",
                    Some(format!("fixture status is {}", fixture.status)),
                ),
            )
            .await?;
            return Ok(());
        }

        let plan = select_source(&fixture);
        if plan.provider == "unsupported" {
            persist_record(
                hx,
                &fixture,
                PersistRecord::terminal(
                    &plan.provider,
                    "not_supported",
                    Some(format!("unsupported sport {}", fixture.sport)),
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
        WITH selected AS (
            SELECT f.*,
                   CASE
                       WHEN f.sport IN ('NBA', 'NFL') THEN 'bdl'
                       WHEN f.sport = 'FOOTBALL' THEN 'sportmonks'
                       ELSE 'unsupported'
                   END AS wanted_provider
            FROM public.fixtures f
            WHERE f.id = $1
        )
        SELECT s.id, s.sport, s.season, COALESCE(s.league_id, 0) AS league_id,
               s.home_team_id, s.away_team_id,
               COALESCE(ht.name, '') AS home_team_name,
               COALESCE(at.name, '') AS away_team_name,
               s.status, s.home_score, s.away_score,
               COALESCE(s.round, '') AS round,
               s.external_id,
               s.wanted_provider AS provider,
               pfm.provider_fixture_id
        FROM selected s
        LEFT JOIN public.teams ht
          ON ht.id = s.home_team_id AND ht.sport = s.sport
        LEFT JOIN public.teams at
          ON at.id = s.away_team_id AND at.sport = s.sport
        LEFT JOIN public.provider_fixture_map pfm
          ON pfm.fixture_id = s.id
         AND pfm.sport = s.sport
         AND pfm.provider = s.wanted_provider
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
        provider: r.get("provider"),
        provider_fixture_id: r.get("provider_fixture_id"),
    }))
}

fn select_source(fixture: &FixtureRow) -> SourcePlan {
    let provider_fixture_id = fixture
        .provider_fixture_id
        .clone()
        .or_else(|| fixture.external_id.map(|id| id.to_string()));

    match fixture.sport.as_str() {
        "NBA" => SourcePlan {
            provider: "bdl".to_string(),
            provider_fixture_id: provider_fixture_id.clone(),
            source_urls: provider_fixture_id
                .as_deref()
                .map(|id| vec![bdl_url("/nba/v1/stats", id)])
                .unwrap_or_default(),
            official_url: None,
        },
        "NFL" => SourcePlan {
            provider: "bdl".to_string(),
            provider_fixture_id: provider_fixture_id.clone(),
            source_urls: provider_fixture_id
                .as_deref()
                .map(|id| {
                    vec![
                        bdl_url("/nfl/v1/stats", id),
                        bdl_url("/nfl/v1/team_stats", id),
                    ]
                })
                .unwrap_or_default(),
            official_url: nfl_gamecenter_url(
                &fixture.away_team_name,
                &fixture.home_team_name,
                fixture.season,
                &fixture.round,
            ),
        },
        "FOOTBALL" => SourcePlan {
            provider: "sportmonks".to_string(),
            provider_fixture_id: provider_fixture_id.clone(),
            source_urls: provider_fixture_id
                .as_deref()
                .map(|id| vec![sportmonks_fixture_url(id)])
                .unwrap_or_default(),
            official_url: None,
        },
        _ => SourcePlan {
            provider: "unsupported".to_string(),
            provider_fixture_id: None,
            source_urls: vec![],
            official_url: None,
        },
    }
}

fn bdl_url(path: &str, game_id: &str) -> String {
    format!("https://api.balldontlie.io{path}?game_ids%5B%5D={game_id}&per_page=100")
}

fn sportmonks_fixture_url(fixture_id: &str) -> String {
    format!(
        "https://api.sportmonks.com/v3/football/fixtures/{fixture_id}?include={}",
        "lineups.player;lineups.details.type;events;scores;participants;statistics.type;state"
    )
}

fn nfl_gamecenter_url(
    away_team_name: &str,
    home_team_name: &str,
    season: i32,
    round: &str,
) -> Option<String> {
    let week = first_int(round)?;
    if !(1..=22).contains(&week) {
        return None;
    }
    let away = nfl_team_slug(away_team_name)?;
    let home = nfl_team_slug(home_team_name)?;
    Some(format!(
        "https://www.nfl.com/games/{away}-at-{home}-{season}-reg-{week}?tab=stats"
    ))
}

fn nfl_team_slug(team_name: &str) -> Option<String> {
    let last = team_name
        .split_whitespace()
        .last()
        .unwrap_or_default()
        .trim_matches(|c: char| !c.is_ascii_alphanumeric());
    if last.is_empty() {
        return None;
    }
    Some(
        last.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect(),
    )
}

fn first_int(s: &str) -> Option<i32> {
    let mut buf = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            buf.push(c);
        } else if !buf.is_empty() {
            break;
        }
    }
    if buf.is_empty() {
        None
    } else {
        buf.parse().ok()
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

async fn fetch_source(
    fixture: &FixtureRow,
    plan: &SourcePlan,
) -> std::result::Result<FetchedJson, FetchOutcome> {
    let Some(provider_fixture_id) = plan.provider_fixture_id.as_deref() else {
        return Err(FetchOutcome::new(
            "not_found",
            plan.source_urls.first().cloned(),
            None,
            None,
            "fixture has no provider fixture id",
        ));
    };

    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("ScoracleBot/1.0 (+https://scoracle.com)")
        .build()
        .map_err(|e| FetchOutcome::new("fetch_failed", None, None, None, e.to_string()))?;

    match plan.provider.as_str() {
        "bdl" => {
            let api_key = std::env::var("BALLDONTLIE_API_KEY").unwrap_or_default();
            if api_key.trim().is_empty() {
                return Err(FetchOutcome::new(
                    "blocked",
                    plan.source_urls.first().cloned(),
                    None,
                    None,
                    "BALLDONTLIE_API_KEY is not configured",
                ));
            }
            fetch_bdl(&client, &fixture.sport, provider_fixture_id, &api_key).await
        }
        "sportmonks" => {
            let token = std::env::var("SPORTMONKS_API_TOKEN").unwrap_or_default();
            if token.trim().is_empty() {
                return Err(FetchOutcome::new(
                    "blocked",
                    plan.source_urls.first().cloned(),
                    None,
                    None,
                    "SPORTMONKS_API_TOKEN is not configured",
                ));
            }
            fetch_sportmonks(&client, provider_fixture_id, &token).await
        }
        _ => Err(FetchOutcome::new(
            "not_supported",
            None,
            None,
            None,
            format!("unsupported provider {}", plan.provider),
        )),
    }
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

async fn fetch_bdl(
    client: &reqwest::Client,
    sport: &str,
    game_id: &str,
    api_key: &str,
) -> std::result::Result<FetchedJson, FetchOutcome> {
    let stats_path = match sport {
        "NBA" => "/nba/v1/stats",
        "NFL" => "/nfl/v1/stats",
        _ => {
            return Err(FetchOutcome::new(
                "not_supported",
                None,
                None,
                None,
                format!("BDL does not support sport {sport}"),
            ))
        }
    };
    let stats = fetch_bdl_pages(client, stats_path, game_id, api_key).await?;
    let mut warnings = Vec::new();
    let mut payload = Map::new();
    payload.insert("stats".to_string(), stats.value);

    let source_url = stats.source_url;
    let final_url = stats.final_url;
    let mut final_domain = stats.final_domain;

    if sport == "NFL" {
        match fetch_bdl_pages(client, "/nfl/v1/team_stats", game_id, api_key).await {
            Ok(team_stats) => {
                if final_domain.is_none() {
                    final_domain = team_stats.final_domain;
                }
                payload.insert("team_stats".to_string(), team_stats.value);
            }
            Err(e) => {
                warnings.push(format!("team_stats fetch skipped: {}", e.error));
                payload.insert("team_stats".to_string(), json!({"data": []}));
            }
        }
    }

    Ok(FetchedJson {
        source_url,
        final_url,
        final_domain,
        value: Value::Object(payload),
        warnings,
    })
}

async fn fetch_bdl_pages(
    client: &reqwest::Client,
    path: &str,
    game_id: &str,
    api_key: &str,
) -> std::result::Result<FetchedJson, FetchOutcome> {
    let source_url = bdl_url(path, game_id);
    let mut cursor: Option<String> = None;
    let mut all = Vec::new();
    let mut final_url = source_url.clone();
    let mut pages = 0usize;

    loop {
        pages += 1;
        if pages > MAX_BDL_PAGES {
            return Err(FetchOutcome::new(
                "fetch_failed",
                Some(source_url),
                Some(final_url.clone()),
                domain_of(&final_url),
                "BDL pagination exceeded safety limit",
            ));
        }

        let mut url =
            reqwest::Url::parse(&format!("https://api.balldontlie.io{path}")).map_err(|e| {
                FetchOutcome::new(
                    "fetch_failed",
                    Some(source_url.clone()),
                    None,
                    None,
                    e.to_string(),
                )
            })?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("game_ids[]", game_id);
            q.append_pair("per_page", "100");
            if let Some(cursor) = cursor.as_deref() {
                q.append_pair("cursor", cursor);
            }
        }

        let resp = client
            .get(url)
            .header("Authorization", api_key)
            .send()
            .await
            .map_err(|e| {
                FetchOutcome::new(
                    "fetch_failed",
                    Some(source_url.clone()),
                    None,
                    None,
                    e.to_string(),
                )
            })?;
        final_url = resp.url().to_string();
        let status = resp.status();
        if !status.is_success() {
            return Err(http_fetch_outcome(status, &source_url, &final_url, "BDL"));
        }
        let value: Value = resp.json().await.map_err(|e| {
            FetchOutcome::new(
                "parse_failed",
                Some(source_url.clone()),
                Some(final_url.clone()),
                domain_of(&final_url),
                e.to_string(),
            )
        })?;
        if let Some(data) = value.get("data").and_then(Value::as_array) {
            all.extend(data.iter().cloned());
        }
        cursor = value
            .get("meta")
            .and_then(|m| m.get("next_cursor"))
            .and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            });
        if cursor.is_none() {
            break;
        }
    }

    Ok(FetchedJson {
        source_url,
        final_url: final_url.clone(),
        final_domain: domain_of(&final_url),
        value: json!({"data": all}),
        warnings: Vec::new(),
    })
}

async fn fetch_sportmonks(
    client: &reqwest::Client,
    fixture_id: &str,
    token: &str,
) -> std::result::Result<FetchedJson, FetchOutcome> {
    let source_url = sportmonks_fixture_url(fixture_id);
    let mut url = reqwest::Url::parse(&format!(
        "https://api.sportmonks.com/v3/football/fixtures/{fixture_id}"
    ))
    .map_err(|e| {
        FetchOutcome::new(
            "fetch_failed",
            Some(source_url.clone()),
            None,
            None,
            e.to_string(),
        )
    })?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair(
            "include",
            "lineups.player;lineups.details.type;events;scores;participants;statistics.type;state",
        );
        q.append_pair("api_token", token);
    }

    let resp = client.get(url).send().await.map_err(|e| {
        FetchOutcome::new(
            "fetch_failed",
            Some(source_url.clone()),
            None,
            None,
            e.to_string(),
        )
    })?;
    let final_url = sanitize_secret_query(resp.url(), "api_token");
    let status = resp.status();
    if !status.is_success() {
        return Err(http_fetch_outcome(
            status,
            &source_url,
            &final_url,
            "SportMonks",
        ));
    }
    let value: Value = resp.json().await.map_err(|e| {
        FetchOutcome::new(
            "parse_failed",
            Some(source_url.clone()),
            Some(final_url.clone()),
            domain_of(&final_url),
            e.to_string(),
        )
    })?;
    Ok(FetchedJson {
        source_url,
        final_url: final_url.clone(),
        final_domain: domain_of(&final_url),
        value,
        warnings: Vec::new(),
    })
}

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

fn parse_fetched_boxscore(
    fixture: &FixtureRow,
    plan: &SourcePlan,
    fetched: &FetchedJson,
) -> std::result::Result<NormalizedBoxscore, ParseOutcome> {
    match plan.provider.as_str() {
        "bdl" if fixture.sport == "NBA" => parse_bdl_nba(fixture, &fetched.value),
        "bdl" if fixture.sport == "NFL" => parse_bdl_nfl(fixture, &fetched.value),
        "sportmonks" if fixture.sport == "FOOTBALL" => parse_sportmonks(fixture, &fetched.value),
        other => Err(ParseOutcome::new(
            "not_supported",
            format!("no parser for provider={other} sport={}", fixture.sport),
        )),
    }
}

fn parse_bdl_nba(
    fixture: &FixtureRow,
    raw: &Value,
) -> std::result::Result<NormalizedBoxscore, ParseOutcome> {
    let rows = raw
        .get("stats")
        .and_then(|v| v.get("data"))
        .and_then(Value::as_array)
        .ok_or_else(|| ParseOutcome::new("parse_failed", "BDL NBA stats.data is not an array"))?;
    if rows.is_empty() {
        return Err(ParseOutcome::new(
            "not_found",
            "BDL NBA returned no stat rows",
        ));
    }

    let mut provider_status = None;
    let mut team_scores: BTreeMap<i32, i32> = BTreeMap::new();
    let mut team_acc: BTreeMap<i32, BTreeMap<String, f64>> = BTreeMap::new();
    let mut players = Vec::new();

    for row in rows {
        let team_id = nested_i32(row, &["team", "id"]).or_else(|| i32_at(row, "team_id"));
        let player_id = nested_i32(row, &["player", "id"]).or_else(|| i32_at(row, "player_id"));
        let Some(team_id) = team_id else { continue };
        let Some(player_id) = player_id else { continue };
        if provider_status.is_none() {
            provider_status = nested_string(row, &["game", "status"]);
        }
        extract_bdl_scores(row, &mut team_scores);
        let stats = extract_numeric_stats(
            row,
            &[
                "id",
                "player",
                "player_id",
                "team",
                "team_id",
                "game",
                "game_id",
                "season",
                "postseason",
                "date",
                "min",
                "minutes",
                "stats",
            ],
            Some("stats"),
        );
        add_stats(&mut team_acc, team_id, &stats);
        players.push(json!({
            "provider_player_id": player_id,
            "player_name": player_name(row.get("player")),
            "provider_team_id": team_id,
            "team_id": team_id,
            "position": row.pointer("/player/position").and_then(Value::as_str),
            "minutes": parse_minutes(row.get("min").or_else(|| row.get("minutes"))),
            "stats": stats_to_json(&stats),
            "raw_labels": {"provider": "bdl"}
        }));
    }

    Ok(normalized_from_parts(
        "bdl",
        provider_status,
        fixture,
        team_scores,
        team_acc,
        players,
        json!([]),
        json!({"stat_key_policy": "raw_provider_labels"}),
    ))
}

fn parse_bdl_nfl(
    fixture: &FixtureRow,
    raw: &Value,
) -> std::result::Result<NormalizedBoxscore, ParseOutcome> {
    let rows = raw
        .get("stats")
        .and_then(|v| v.get("data"))
        .and_then(Value::as_array)
        .ok_or_else(|| ParseOutcome::new("parse_failed", "BDL NFL stats.data is not an array"))?;
    if rows.is_empty() {
        return Err(ParseOutcome::new(
            "not_found",
            "BDL NFL returned no stat rows",
        ));
    }

    let mut provider_status = None;
    let mut team_scores: BTreeMap<i32, i32> = BTreeMap::new();
    let mut team_acc: BTreeMap<i32, BTreeMap<String, f64>> = BTreeMap::new();
    let mut players = Vec::new();

    for row in rows {
        let team_id = nested_i32(row, &["team", "id"]).or_else(|| i32_at(row, "team_id"));
        let player_id = nested_i32(row, &["player", "id"]).or_else(|| i32_at(row, "player_id"));
        let Some(team_id) = team_id else { continue };
        let Some(player_id) = player_id else { continue };
        if provider_status.is_none() {
            provider_status = nested_string(row, &["game", "status"]);
        }
        extract_bdl_scores(row, &mut team_scores);
        let stats = extract_numeric_stats(
            row,
            &[
                "id",
                "player",
                "player_id",
                "team",
                "team_id",
                "game",
                "game_id",
                "season",
                "postseason",
                "week",
                "date",
            ],
            None,
        );
        add_stats(&mut team_acc, team_id, &stats);
        players.push(json!({
            "provider_player_id": player_id,
            "player_name": player_name(row.get("player")),
            "provider_team_id": team_id,
            "team_id": team_id,
            "position": row.pointer("/player/position").and_then(Value::as_str),
            "position_abbreviation": row.pointer("/player/position_abbreviation").and_then(Value::as_str),
            "stats": stats_to_json(&stats),
            "raw_labels": {"provider": "bdl"}
        }));
    }

    if let Some(team_rows) = raw
        .get("team_stats")
        .and_then(|v| v.get("data"))
        .and_then(Value::as_array)
    {
        for row in team_rows {
            let Some(team_id) = nested_i32(row, &["team", "id"]) else {
                continue;
            };
            let overrides = extract_numeric_stats(
                row,
                &[
                    "game",
                    "team",
                    "home_away",
                    "possession_time",
                    "third_down_efficiency",
                    "fourth_down_efficiency",
                ],
                None,
            );
            let acc = team_acc.entry(team_id).or_default();
            for (k, v) in overrides {
                acc.insert(k, v);
            }
        }
    }

    Ok(normalized_from_parts(
        "bdl",
        provider_status,
        fixture,
        team_scores,
        team_acc,
        players,
        json!([]),
        json!({"stat_key_policy": "raw_provider_labels"}),
    ))
}

fn parse_sportmonks(
    fixture: &FixtureRow,
    raw: &Value,
) -> std::result::Result<NormalizedBoxscore, ParseOutcome> {
    let data = raw
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| ParseOutcome::new("parse_failed", "SportMonks data is not an object"))?;
    let data = Value::Object(data.clone());

    let provider_status = extract_sportmonks_state(&data);
    let team_scores = extract_sportmonks_scores(&data);
    let period_scoring = extract_sportmonks_period_scoring(&data);
    let mut team_acc = extract_sportmonks_team_statistics(&data);
    let mut players = Vec::new();
    let mut player_team: BTreeMap<i32, i32> = BTreeMap::new();

    let lineups = data
        .get("lineups")
        .and_then(Value::as_array)
        .ok_or_else(|| ParseOutcome::new("parse_failed", "SportMonks lineups is not an array"))?;
    if lineups.is_empty() {
        return Err(ParseOutcome::new(
            "not_found",
            "SportMonks returned no lineup rows",
        ));
    }

    for entry in lineups {
        let team_id = i32_at(entry, "team_id").or_else(|| nested_i32(entry, &["team", "id"]));
        let player_id = i32_at(entry, "player_id").or_else(|| nested_i32(entry, &["player", "id"]));
        let Some(team_id) = team_id else { continue };
        let Some(player_id) = player_id else { continue };

        let raw_stats = flatten_sportmonks_details(entry.get("details"));
        add_stats(&mut team_acc, team_id, &raw_stats);
        player_team.insert(player_id, team_id);
        players.push(json!({
            "provider_player_id": player_id,
            "player_name": sportmonks_player_name(entry.get("player")),
            "provider_team_id": team_id,
            "team_id": team_id,
            "position_id": i32_at(entry, "position_id"),
            "minutes": raw_stats
                .get("minutes_played")
                .or_else(|| raw_stats.get("minutes-played"))
                .copied(),
            "stats": stats_to_json(&raw_stats),
            "raw_labels": {"provider": "sportmonks"}
        }));
    }

    let event_counts = extract_sportmonks_event_counts(data.get("events"));
    for (player_id, counts) in &event_counts {
        if let Some(team_id) = player_team.get(&player_id).copied() {
            add_stats(&mut team_acc, team_id, &counts);
        }
    }
    merge_player_event_counts(&mut players, event_counts);

    Ok(normalized_from_parts(
        "sportmonks",
        provider_status,
        fixture,
        team_scores,
        team_acc,
        players,
        period_scoring,
        json!({"stat_key_policy": "raw_provider_labels"}),
    ))
}

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

fn extract_bdl_scores(row: &Value, team_scores: &mut BTreeMap<i32, i32>) {
    let Some(game) = row.get("game") else {
        return;
    };
    let home_team_id =
        i32_at(game, "home_team_id").or_else(|| nested_i32(game, &["home_team", "id"]));
    let away_team_id = i32_at(game, "visitor_team_id")
        .or_else(|| i32_at(game, "away_team_id"))
        .or_else(|| nested_i32(game, &["visitor_team", "id"]))
        .or_else(|| nested_i32(game, &["away_team", "id"]));
    if let (Some(team_id), Some(score)) = (home_team_id, i32_at(game, "home_team_score")) {
        team_scores.insert(team_id, score);
    }
    let away_score = i32_at(game, "visitor_team_score").or_else(|| i32_at(game, "away_team_score"));
    if let (Some(team_id), Some(score)) = (away_team_id, away_score) {
        team_scores.insert(team_id, score);
    }
}

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

fn stats_to_json(stats: &BTreeMap<String, f64>) -> Value {
    let mut obj = Map::new();
    for (k, v) in stats {
        obj.insert(k.clone(), json_number(*v));
    }
    Value::Object(obj)
}

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

fn sportmonks_player_name(raw: Option<&Value>) -> Option<String> {
    let raw = raw?.as_object()?;
    raw.get("display_name")
        .and_then(Value::as_str)
        .or_else(|| raw.get("name").and_then(Value::as_str))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn flatten_sportmonks_details(raw: Option<&Value>) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    let Some(details) = raw.and_then(Value::as_array) else {
        return out;
    };
    for detail in details {
        let Some(code) = detail
            .get("type")
            .and_then(|t| t.get("code"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        if let Some(value) = detail
            .get("data")
            .and_then(|d| d.get("value"))
            .and_then(numeric_value)
        {
            out.insert(code.to_string(), value);
        }
    }
    out
}

fn extract_sportmonks_state(data: &Value) -> Option<String> {
    let state = data.get("state")?;
    for key in ["state", "developer_name", "short_name", "name"] {
        if let Some(s) = state.get(key).and_then(Value::as_str) {
            if !s.trim().is_empty() {
                return Some(s.trim().to_string());
            }
        }
    }
    None
}

fn extract_sportmonks_scores(data: &Value) -> BTreeMap<i32, i32> {
    let mut out = BTreeMap::new();
    let Some(scores) = data.get("scores").and_then(Value::as_array) else {
        return out;
    };
    for block in scores {
        let Some(team_id) = i32_at(block, "participant_id") else {
            continue;
        };
        let goals = block
            .get("score")
            .and_then(|s| s.get("goals"))
            .and_then(numeric_value)
            .or_else(|| block.get("score").and_then(numeric_value));
        if let Some(goals) = goals {
            let score = goals as i32;
            let current = out.get(&team_id).copied().unwrap_or(i32::MIN);
            out.insert(team_id, current.max(score));
        }
    }
    out
}

fn extract_sportmonks_period_scoring(data: &Value) -> Value {
    let Some(scores) = data.get("scores").and_then(Value::as_array) else {
        return json!([]);
    };
    Value::Array(
        scores
            .iter()
            .filter_map(|block| {
                let team_id = i32_at(block, "participant_id")?;
                let label = block
                    .get("description")
                    .and_then(Value::as_str)
                    .or_else(|| block.get("type").and_then(Value::as_str))
                    .unwrap_or("score");
                let goals = block
                    .get("score")
                    .and_then(|s| s.get("goals"))
                    .and_then(numeric_value)
                    .or_else(|| block.get("score").and_then(numeric_value))?;
                Some(json!({
                    "team_id": team_id,
                    "label": label,
                    "score": json_number(goals),
                    "raw_description": label
                }))
            })
            .collect(),
    )
}

fn extract_sportmonks_team_statistics(data: &Value) -> BTreeMap<i32, BTreeMap<String, f64>> {
    let mut out: BTreeMap<i32, BTreeMap<String, f64>> = BTreeMap::new();
    let Some(stats) = data.get("statistics").and_then(Value::as_array) else {
        return out;
    };
    for row in stats {
        let Some(team_id) = i32_at(row, "participant_id") else {
            continue;
        };
        let Some(code) = row
            .get("type")
            .and_then(|t| t.get("code"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let Some(value) = row
            .get("data")
            .and_then(|d| d.get("value"))
            .and_then(numeric_value)
        else {
            continue;
        };
        out.entry(team_id)
            .or_default()
            .insert(code.to_string(), value);
    }
    out
}

fn extract_sportmonks_event_counts(raw: Option<&Value>) -> BTreeMap<i32, BTreeMap<String, f64>> {
    let mut out: BTreeMap<i32, BTreeMap<String, f64>> = BTreeMap::new();
    let Some(events) = raw.and_then(Value::as_array) else {
        return out;
    };
    for event in events {
        let Some(player_id) = i32_at(event, "player_id") else {
            continue;
        };
        let Some(type_id) = i32_at(event, "type_id") else {
            continue;
        };
        let key = match type_id {
            14 => Some("goals"),
            16 => Some("penalty_goals"),
            17 => Some("penalties_missed"),
            19 => Some("yellow_cards"),
            20 | 21 => Some("red_cards"),
            _ => None,
        };
        if let Some(key) = key {
            let player = out.entry(player_id).or_default();
            *player.entry(key.to_string()).or_insert(0.0) += 1.0;
        }
        if type_id == 14 {
            if let Some(assister_id) = i32_at(event, "related_player_id") {
                let player = out.entry(assister_id).or_default();
                *player.entry("assists".to_string()).or_insert(0.0) += 1.0;
            }
        }
    }
    out
}

fn merge_player_event_counts(
    players: &mut [Value],
    event_counts: BTreeMap<i32, BTreeMap<String, f64>>,
) {
    for player in players {
        let Some(player_id) = i32_at(player, "provider_player_id") else {
            continue;
        };
        let Some(counts) = event_counts.get(&player_id) else {
            continue;
        };
        let Some(stats) = player.get_mut("stats").and_then(Value::as_object_mut) else {
            continue;
        };
        for (key, value) in counts {
            let current = stats.get(key).and_then(numeric_value).unwrap_or(0.0);
            stats.insert(key.clone(), json_number(current + value));
        }
    }
}

fn i32_at(v: &Value, key: &str) -> Option<i32> {
    v.get(key)
        .and_then(numeric_value)
        .and_then(|n| i32::try_from(n as i64).ok())
}

fn nested_i32(v: &Value, path: &[&str]) -> Option<i32> {
    let mut cur = v;
    for key in path {
        cur = cur.get(*key)?;
    }
    numeric_value(cur).and_then(|n| i32::try_from(n as i64).ok())
}

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

fn domain_of(raw_url: &str) -> Option<String> {
    reqwest::Url::parse(raw_url).ok().and_then(|u| {
        u.host_str()
            .map(|h| h.trim_start_matches("www.").to_lowercase())
    })
}

fn sanitize_secret_query(url: &reqwest::Url, secret_key: &str) -> String {
    let mut clean = url.clone();
    let pairs: Vec<(String, String)> = clean
        .query_pairs()
        .filter(|(k, _)| k != secret_key)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    clean.set_query(None);
    if !pairs.is_empty() {
        let mut q = clean.query_pairs_mut();
        for (k, v) in pairs {
            q.append_pair(&k, &v);
        }
    }
    clean.to_string()
}

pub fn build_fixture_boxscore_input_version(
    sport: &str,
    season: i32,
    league_id: i32,
    home_team_id: i32,
    away_team_id: i32,
    home_score: Option<i32>,
    away_score: Option<i32>,
    provider_pairs: &[(String, String)],
) -> String {
    let mut pairs = provider_pairs.to_vec();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let providers = pairs
        .into_iter()
        .map(|(provider, id)| format!("{provider}={id}"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "fbf1:{sport}:{season}:{league_id}:{home_team_id}:{away_team_id}:{}:{}:{providers}",
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
            provider: if sport == "FOOTBALL" {
                "sportmonks"
            } else {
                "bdl"
            }
            .to_string(),
            provider_fixture_id: Some("12345".to_string()),
        }
    }

    #[test]
    fn source_selection_uses_existing_provider_ids() {
        let plan = select_source(&fixture("NBA"));
        assert_eq!(plan.provider, "bdl");
        assert_eq!(
            plan.source_urls,
            vec!["https://api.balldontlie.io/nba/v1/stats?game_ids%5B%5D=12345&per_page=100"]
        );

        let nfl = select_source(&fixture("NFL"));
        assert_eq!(nfl.source_urls.len(), 2);
        assert_eq!(
            nfl.official_url.as_deref(),
            Some("https://www.nfl.com/games/chiefs-at-bills-2025-reg-9?tab=stats")
        );

        let football = select_source(&fixture("FOOTBALL"));
        assert_eq!(football.provider, "sportmonks");
        assert!(football.source_urls[0].contains("/fixtures/12345"));
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

    #[test]
    fn input_version_is_order_stable() {
        let a = build_fixture_boxscore_input_version(
            "NFL",
            2025,
            0,
            1,
            2,
            Some(10),
            Some(7),
            &[
                ("z".to_string(), "9".to_string()),
                ("bdl".to_string(), "123".to_string()),
            ],
        );
        let b = build_fixture_boxscore_input_version(
            "NFL",
            2025,
            0,
            1,
            2,
            Some(10),
            Some(7),
            &[
                ("bdl".to_string(), "123".to_string()),
                ("z".to_string(), "9".to_string()),
            ],
        );
        assert_eq!(a, b);
        assert_ne!(a, a.replace(":10:", ":11:"));
    }

    #[test]
    fn content_hash_changes_with_payload() {
        let a = boxscore_content_hash(&json!({"score": {"home": 1}}));
        let b = boxscore_content_hash(&json!({"score": {"home": 2}}));
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }

    #[test]
    fn parses_nba_bdl_sample() {
        let raw: Value = serde_json::from_str(include_str!(
            "../fixtures/boxscore_fetch/nba_bdl_sample.json"
        ))
        .unwrap();
        let parsed = parse_bdl_nba(&fixture("NBA"), &raw).unwrap();
        assert_eq!(parsed.provider_status.as_deref(), Some("Final"));
        assert_eq!(parsed.score["home_score"], 110);
        assert_eq!(parsed.score["away_score"], 102);
        assert_eq!(parsed.player_stats.as_array().unwrap().len(), 2);
        validate_normalized(
            &FixtureRow {
                home_score: Some(110),
                away_score: Some(102),
                ..fixture("NBA")
            },
            &parsed,
        )
        .unwrap();
    }

    #[test]
    fn parses_nfl_bdl_sample() {
        let raw: Value = serde_json::from_str(include_str!(
            "../fixtures/boxscore_fetch/nfl_bdl_sample.json"
        ))
        .unwrap();
        let parsed = parse_bdl_nfl(&fixture("NFL"), &raw).unwrap();
        assert_eq!(parsed.provider_status.as_deref(), Some("Final"));
        assert_eq!(parsed.score["home_score"], 28);
        assert_eq!(parsed.score["away_score"], 21);
        assert_eq!(parsed.player_stats.as_array().unwrap().len(), 2);
        validate_normalized(&fixture("NFL"), &parsed).unwrap();
    }

    #[test]
    fn parses_football_sportmonks_sample() {
        let raw: Value = serde_json::from_str(include_str!(
            "../fixtures/boxscore_fetch/football_sportmonks_sample.json"
        ))
        .unwrap();
        let parsed = parse_sportmonks(&fixture("FOOTBALL"), &raw).unwrap();
        assert_eq!(parsed.provider_status.as_deref(), Some("FT"));
        assert_eq!(parsed.score["home_score"], 2);
        assert_eq!(parsed.score["away_score"], 1);
        assert_eq!(parsed.period_scoring.as_array().unwrap().len(), 4);
        assert_eq!(parsed.player_stats.as_array().unwrap().len(), 2);
        assert!(parsed.player_stats[0]["stats"]
            .get("accurate-passes")
            .is_some());
        validate_normalized(
            &FixtureRow {
                home_score: Some(2),
                away_score: Some(1),
                ..fixture("FOOTBALL")
            },
            &parsed,
        )
        .unwrap();
    }
}
