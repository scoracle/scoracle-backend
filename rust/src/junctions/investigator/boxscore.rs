//! Fixture Boxscore stage.
//!
//! This stage is fixture-keyed (`entity_type='fixture'`) and fetches completed game
//! box score payloads into `fixture_boxscore_fetches`.
//!
//! # State after mig 230 (2026-08-23): the vendor layer is gone, retrieval is wired
//!
//! This seat used to read two PAID providers — balldontlie for NBA/NFL, sportmonks for
//! FOOTBALL — addressed by ids the seeding layer wrote into `provider_fixture_map`. Scott
//! retired that: box scores are public-event facts, so they get read from public sources. The
//! seeder was pruned (no 2026 fixture ever got a mapping), mig 230 dropped the map, and the
//! API tokens are unset. **All three legs were dead, so the vendor code is deleted rather than
//! left looking wired.**
//!
//! The rebuild is discovery → retrieval → interpretation (`discover.rs:1`), and the three
//! arrive in that order:
//!
//! * **DISCOVERY** — not built. `boxscore_sources` is the registry, and it is EMPTY. Populating
//!   it is the Investigator's model work, routed to `Role::Investigator` on the other host.
//! * **RETRIEVAL** — *built, and this is what changed.* [`select_source`] now reads
//!   `boxscore_sources` and [`fetch_source`] goes through [`crate::fetch::BudgetedFetcher`].
//!   The seat no longer owns an HTTP client, a spacing rule, or a retry: those are the 4.2
//!   substrate's, which was founded for this path and until now only entity discovery ever
//!   used.
//! * **INTERPRETATION** — not built. [`parse_fetched_boxscore`] is still inert; a source's
//!   `parser_family` names a CODE parser and the family-independent normalization helpers
//!   below (each `#[allow(dead_code)]`) are what the first one gets built on.
//!
//! **An empty registry still means `no_source`, and that is the current live behaviour.** The
//! difference is that the emptiness is now the DATA's, not the code's: registering a source is
//! an INSERT, exactly as mig 208 intended ("adding or suspending a source is data, not a
//! deploy"). Nothing here needs to be redeployed to bring the first source online.
//!
//! It also does not write `event_box_scores` or `event_team_stats` — promoting a validated
//! fetch into those canonical tables is a deliberate later step, not a side effect.

use crate::fetch::{BudgetedFetchError, BudgetedFetcher, FetchPolicy};
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

/// The fixture's own facts — which, since mig 230, are the ONLY address a box score has.
///
/// Every field here is a URL-template variable (see [`render_template`]): a public match page
/// is addressed by teams, date, competition and round, so this row IS the discovery query.
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
    /// Kickoff as `YYYY-MM-DD`, rendered by Postgres at UTC.
    ///
    /// A STRING, and formatted server-side, for the same reason the Scout's availability marker
    /// is (`scout/mod.rs`): this crate has no date library, and a date rendered from a local
    /// zone would put a 20:00 kickoff on the wrong calendar day for half the world — which for
    /// a date-keyed match URL is not a rounding error, it is a 404.
    event_date: String,
}

/// One eligible row of `boxscore_sources`, already screened by [`load_sources`].
#[derive(Clone, Debug)]
struct BoxscoreSource {
    id: i64,
    domain: String,
    url_template: Option<String>,
    parser_family: String,
    trust_state: String,
    policy: FetchPolicy,
}

/// Where this fixture's box score will be read from, and under whose budget.
///
/// `provider` is the source DOMAIN now rather than a vendor's brand name — the column it lands
/// in (`fixture_boxscore_fetches.provider`) answers "who told us this", and for a public source
/// that is the host.
#[derive(Clone, Debug)]
pub struct SourcePlan {
    pub provider: String,
    pub provider_fixture_id: Option<String>,
    pub source_urls: Vec<String>,
    pub official_url: Option<String>,
    /// Which CODE parser reads the retrieved page. Empty when nothing resolved.
    pub parser_family: String,
    /// `candidate` until the score-reconciliation gate promotes it; `trusted` after.
    pub trust_state: String,
    /// The registry row id, so provenance can name the source that was used.
    pub source_id: Option<i64>,
    /// The per-domain budget from `boxscore_sources.fetch_policy`.
    pub policy: FetchPolicy,
}

impl SourcePlan {
    /// The empty plan — no registered source could serve this fixture.
    fn none() -> Self {
        Self {
            provider: "none".to_string(),
            provider_fixture_id: None,
            source_urls: vec![],
            official_url: None,
            parser_family: String::new(),
            trust_state: String::new(),
            source_id: None,
            policy: FetchPolicy::default(),
        }
    }
}

/// A retrieved document, straight off the budgeted fetcher.
///
/// `body` is TEXT, not `serde_json::Value`, and that is the shape change mig 230's rebuild
/// forced: the vendor era fetched two JSON APIs and could parse eagerly, but a public source is
/// whatever the page is. Which of JSON-LD, an embedded `__NEXT_DATA__`-style blob, or an HTML
/// table this holds is the PARSER FAMILY's question, and deciding it here would put
/// interpretation back inside retrieval — the exact seam `discover.rs:1` draws.
///
/// `body` and `document_id` are unread until the first family lands. They are the payload and
/// the provenance row that proves where it came from.
#[allow(dead_code)]
#[derive(Debug)]
struct FetchedDocument {
    source_url: String,
    final_url: String,
    final_domain: Option<String>,
    /// The `source_documents` row this retrieval landed as (or was reused from).
    document_id: i64,
    body: String,
    from_cache: bool,
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

pub struct FixtureBoxscoreHandler {
    fetcher: BudgetedFetcher,
}

impl FixtureBoxscoreHandler {
    /// Fallible now: the handler owns ONE [`BudgetedFetcher`], the way
    /// `InvestigateEntityHandler` does, and building its client can fail.
    ///
    /// One per handler is the point, not an accident — the per-domain spacing, the circuit
    /// breaker and the "concurrency 1 per domain" lock all live in that instance's ledger. A
    /// fetcher built per fixture would reset every one of them on every call and turn a polite
    /// crawl into an impolite one that merely looked budgeted.
    pub fn new() -> Result<Self> {
        Ok(Self {
            fetcher: BudgetedFetcher::new()?,
        })
    }
}

#[async_trait]
impl StageHandler for FixtureBoxscoreHandler {
    fn stage(&self) -> Stage {
        Stage::FixtureBoxscore
    }

    /// NO slot group, deliberately — `entity.rs:85`'s D-T10 lesson (2026-08-09) applies here
    /// verbatim, and this seat is where it was learned the expensive way. This stage makes ZERO
    /// model calls: discovery is the other arm, and interpretation is a CODE parser. Holding an
    /// `ARCHBOX_SLOTS` slot for pure HTTP work is "the structural mismatch behind the measured
    /// 57h starvation: it queued behind the Editor's drain for a card it never used."
    ///
    /// When the discovery arm lands, ITS model calls ride `Role::Investigator` to the 14B on the
    /// other host, which has its own governor. So this stays `None` even then.
    fn slot_group(&self) -> Option<(&'static str, usize)> {
        None
    }

    /// One at a time. The binding constraint is not the card but the 2s per-domain floor in
    /// `FetchPolicy` — with a handful of registered sources, extra concurrency here would just
    /// queue on the fetcher's per-domain lock.
    fn max_in_flight(&self) -> usize {
        1
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

        let plan = select_source(&hx.pool, &fixture).await?;
        if plan.source_urls.is_empty() {
            // No registered public source could serve this fixture. Terminal and honest — and
            // still the state of every fixture, because `boxscore_sources` is empty until the
            // discovery arm populates it. What changed with the retrieval wiring is WHERE the
            // emptiness lives: this is now a query returning no eligible rows, not a function
            // hardcoded to return nothing.
            persist_record(
                hx,
                &fixture,
                PersistRecord::terminal(
                    &plan.provider,
                    "no_source",
                    Some(format!(
                        "no eligible source in boxscore_sources for sport {} league {}",
                        fixture.sport, fixture.league_id
                    )),
                ),
            )
            .await?;
            return Ok(());
        }

        let fetched = match fetch_source(&self.fetcher, &hx.pool, &plan).await {
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
                raw_labels: merge_raw_labels(
                    normalized.raw_labels,
                    fetched.warnings,
                    &plan,
                    fetched.document_id,
                    fetched.from_cache,
                ),
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
               f.external_id,
               -- Rendered here, at UTC, on purpose: see FixtureRow::event_date. Postgres owns
               -- the calendar because this crate has no date library to own it with.
               to_char(f.start_time AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS event_date
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
        // A fixture with no kickoff cannot address a date-keyed page; an empty string renders
        // a template that will simply not resolve, which is the honest outcome.
        event_date: r
            .get::<Option<String>, _>("event_date")
            .unwrap_or_default(),
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
/// The replacement reads `boxscore_sources`, mig 208's registry: sources carry their own
/// `url_template`, `parser_family`, `fetch_policy` and `trust_state`, so bringing one online is
/// an INSERT rather than a deploy. The registry is EMPTY today, so this still resolves nothing
/// and every fixture still takes the honest `no_source` path — but the emptiness is now the
/// data's, which is the whole point of the table.
///
/// Only the FIRST eligible source is planned, not all of them. Fanning out across sources for
/// one fixture would spend several domains' budgets to answer a question the first source
/// answers, and the score-reconciliation gate — not a quorum — is what decides whether the
/// answer is right.
async fn select_source(pool: &sqlx::PgPool, fixture: &FixtureRow) -> Result<SourcePlan> {
    let sources = load_sources(pool, &fixture.sport, fixture.league_id).await?;
    for source in sources {
        let Some(template) = source.url_template.as_deref() else {
            // `discovery = 'search'` — the source is found by query, not by template. That arm
            // is the discovery build; skip rather than guess a URL shape for it.
            continue;
        };
        let urls: Vec<String> = template
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter_map(|line| render_template(line, fixture))
            .collect();
        if urls.is_empty() {
            continue;
        }
        return Ok(SourcePlan {
            provider: source.domain.clone(),
            provider_fixture_id: None,
            official_url: urls.first().cloned(),
            source_urls: urls,
            parser_family: source.parser_family,
            trust_state: source.trust_state,
            source_id: Some(source.id),
            policy: source.policy,
        });
    }
    Ok(SourcePlan::none())
}

/// load_sources returns the eligible registry rows, best first.
///
/// Three screens, each of which is a law this repo already keeps:
///
/// 1. **`suspended` is excluded.** mig 208: a family is "suspended — never deleted — when it
///    misbehaves", so the row must survive the exclusion to carry its own history.
/// 2. **`terms_review` must record a `pass` verdict.** This is the screen that would be easiest
///    to leave out and worst to leave out. `terms_review` is a REAL exercised process — the
///    Wikimedia family "passed the 4.3 terms review with no reservations", and the same review
///    rejected every other keyless family in both D-4 sports. A discovery arm that proposes
///    domains must not be able to make one fetchable merely by inserting it; the right to fetch
///    is a separate, human verdict, and this is where that separation is enforced in CODE.
/// 3. **League scoping.** `league_id IS NULL` means the family serves the whole sport; a set
///    value narrows it to one league. Narrower rows sort first — a Premier League specialist
///    should beat a general football source for a Premier League fixture.
///
/// `trusted` outranks `candidate` because a source that has already reconciled against known
/// final scores is the better first call; candidates stay eligible, because a candidate that is
/// never fetched can never earn promotion.
async fn load_sources(
    pool: &sqlx::PgPool,
    sport: &str,
    league_id: i32,
) -> Result<Vec<BoxscoreSource>> {
    let rows = sqlx::query(
        r#"
        SELECT id, domain, url_template, parser_family, trust_state, fetch_policy
        FROM public.boxscore_sources
        WHERE sport = $1
          AND (league_id IS NULL OR league_id = $2)
          AND trust_state <> 'suspended'
          AND terms_review->>'verdict' = 'pass'
        ORDER BY (trust_state = 'trusted') DESC,
                 (league_id IS NOT NULL) DESC,
                 id
        "#,
    )
    .bind(sport)
    .bind(league_id)
    .fetch_all(pool)
    .await
    .context("load boxscore_sources")?;

    Ok(rows
        .into_iter()
        .map(|r| BoxscoreSource {
            id: r.get("id"),
            domain: r.get("domain"),
            url_template: r.get("url_template"),
            parser_family: r.get("parser_family"),
            trust_state: r.get("trust_state"),
            policy: policy_from_json(&r.get::<Value, _>("fetch_policy")),
        })
        .collect())
}

/// policy_from_json reads `boxscore_sources.fetch_policy` into the fetcher's knobs.
///
/// The 2s floor is NOT applied here — [`FetchPolicy::new`] applies it, so a policy cannot be
/// made faster than the 4.2 law by any route, including a bad row in this table. Absent keys
/// take [`FetchPolicy::default`]'s values rather than zero: an empty `{}` (the column default)
/// must mean "the polite default", never "no spacing and no cache".
fn policy_from_json(raw: &Value) -> FetchPolicy {
    let default = FetchPolicy::default();
    let secs = |key: &str| raw.get(key).and_then(numeric_value).filter(|n| *n >= 0.0);
    FetchPolicy::new(
        secs("min_spacing_secs")
            .map(Duration::from_secs_f64)
            .unwrap_or(default.min_spacing),
        secs("cache_ttl_secs")
            .map(Duration::from_secs_f64)
            .unwrap_or(default.cache_ttl),
    )
}

/// render_template substitutes the fixture's facts into a `url_template`.
///
/// Returns `None` if any placeholder in the template has no value — a URL with a literal
/// `{date}` left in it is a guaranteed 404 that would still spend the domain's budget and count
/// a failure against its circuit breaker. Failing to render is cheaper and truthful.
///
/// The variables are the fixture's own facts, which since mig 230 are the only address a box
/// score has. `_slug` forms exist because public match URLs are overwhelmingly slug-keyed
/// (`/manchester-united-v-arsenal`), and asking every parser family to reinvent that is how
/// families drift apart.
fn render_template(template: &str, fixture: &FixtureRow) -> Option<String> {
    let vars: [(&str, String); 11] = [
        ("{date}", fixture.event_date.clone()),
        ("{season}", fixture.season.to_string()),
        ("{league_id}", fixture.league_id.to_string()),
        ("{home_team_id}", fixture.home_team_id.to_string()),
        ("{away_team_id}", fixture.away_team_id.to_string()),
        ("{home_team}", fixture.home_team_name.clone()),
        ("{away_team}", fixture.away_team_name.clone()),
        ("{home_slug}", slugify(&fixture.home_team_name)),
        ("{away_slug}", slugify(&fixture.away_team_name)),
        ("{round}", fixture.round.clone()),
        (
            "{external_id}",
            fixture.external_id.map(|i| i.to_string()).unwrap_or_default(),
        ),
    ];

    let mut out = template.to_string();
    for (name, value) in &vars {
        if out.contains(name) {
            if value.is_empty() {
                return None;
            }
            out = out.replace(name, value);
        }
    }
    // An unrecognized placeholder is a template bug, not a fetchable URL.
    if out.contains('{') || out.contains('}') {
        return None;
    }
    Some(out)
}

/// slugify renders a team name as the lowercase hyphenated form public URLs use.
///
/// ASCII-folds the handful of accents that actually appear in the five European leagues we
/// serve (Atlético, Beşiktaş, Bayern München) — an unfolded `é` percent-encodes into a URL that
/// most sites will not match.
fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_sep = false;
    for ch in name.chars() {
        let folded = match ch {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => "a",
            'é' | 'è' | 'ê' | 'ë' => "e",
            'í' | 'ì' | 'î' | 'ï' => "i",
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' => "o",
            'ú' | 'ù' | 'û' | 'ü' => "u",
            'ç' => "c",
            'ñ' => "n",
            'ş' => "s",
            'ğ' => "g",
            'ı' => "i",
            'ß' => "ss",
            c if c.is_ascii_alphanumeric() => {
                if pending_sep && !out.is_empty() {
                    out.push('-');
                }
                pending_sep = false;
                out.extend(c.to_lowercase());
                continue;
            }
            _ => {
                pending_sep = true;
                continue;
            }
        };
        if pending_sep && !out.is_empty() {
            out.push('-');
        }
        pending_sep = false;
        out.push_str(folded);
    }
    out
}

#[derive(Debug)]
struct FetchOutcome {
    status: String,
    source_url: Option<String>,
    final_url: Option<String>,
    final_domain: Option<String>,
    error: String,
}

/// fetch_source retrieves the planned document through the budgeted fetcher.
///
/// **It does NOT build its own `reqwest::Client`, and that is the point of this function.** The
/// two vendor clients that used to live here each had their own timeout, their own retry and
/// their own idea of politeness. [`crate::fetch::BudgetedFetcher`] already enforces, per domain:
/// concurrency 1, a 2s minimum spacing (the 4.2 floor), `429`/`Retry-After` honoured as a hold,
/// a circuit breaker at four consecutive failures for 15 minutes, and a `source_documents`
/// provenance row for every retrieval with `cache_ttl` reuse.
///
/// That substrate was FOUNDED for this path — `fetch.rs:516`: "founded in Phase 4 (box scores),
/// reused by Phase 5" — and then only ever used by entity discovery, because this seat went
/// direct to the vendors instead. This function is the wiring-back.
///
/// The stated posture travels with it and is not negotiable: *"A domain that blocks direct
/// fetch is a domain we skip — never stealth, no browser automation on this path."* Hence
/// `DomainSkipped` and a `403` both terminate honestly instead of escalating.
///
/// Candidate URLs are tried in order and the FIRST retrieval wins. A later URL is only reached
/// when an earlier one produced no document, so a source listing several templates costs one
/// fetch in the normal case.
async fn fetch_source(
    fetcher: &BudgetedFetcher,
    pool: &sqlx::PgPool,
    plan: &SourcePlan,
) -> std::result::Result<FetchedDocument, FetchOutcome> {
    let mut warnings: Vec<String> = Vec::new();
    let mut last: Option<FetchOutcome> = None;

    for url in &plan.source_urls {
        match fetcher.fetch(pool, url, &plan.policy).await {
            Ok(doc) => {
                return Ok(FetchedDocument {
                    source_url: url.clone(),
                    final_url: doc.final_url,
                    final_domain: doc.domain,
                    document_id: doc.document_id,
                    body: doc.body,
                    from_cache: doc.from_cache,
                    warnings,
                });
            }
            Err(e) => {
                let outcome = budgeted_fetch_outcome(url, &e);
                warnings.push(outcome.error.clone());
                last = Some(outcome);
            }
        }
    }

    Err(last.unwrap_or_else(|| {
        FetchOutcome::new(
            "no_source",
            None,
            None,
            None,
            "source plan carried no candidate URLs",
        )
    }))
}

/// budgeted_fetch_outcome maps a fetcher error onto this stage's terminal vocabulary.
///
/// `DomainSkipped` becomes `blocked` rather than a retryable failure on purpose: the circuit is
/// already open or the domain asked us to hold, so the correct behaviour is to stop and record
/// why. Re-queueing would be the stage arguing with a budget that exists to stop exactly that.
fn budgeted_fetch_outcome(url: &str, e: &BudgetedFetchError) -> FetchOutcome {
    match e {
        BudgetedFetchError::DomainSkipped { domain, until_secs } => FetchOutcome::new(
            "blocked",
            Some(url.to_string()),
            None,
            Some(domain.clone()),
            format!("domain {domain} held by its budget (retry in {until_secs}s)"),
        ),
        BudgetedFetchError::Http { status, final_url } => http_fetch_outcome(
            StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY),
            url,
            final_url,
            "source",
        ),
        BudgetedFetchError::Other(err) => FetchOutcome::new(
            "fetch_failed",
            Some(url.to_string()),
            None,
            domain_of(url),
            format!("{err:#}"),
        ),
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

/// Live again: [`budgeted_fetch_outcome`] routes every HTTP rejection through this, so the
/// blocked/not_found/fetch_failed split is decided in one place.
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
///
/// Now that retrieval is wired, this is the LAST inert step: a fixture with a registered source
/// reaches here with a real document in hand and stops, recording `not_supported` against the
/// family that has no parser yet. That is a more useful terminal state than the old one — it
/// says "we fetched it and cannot read it" rather than "we never looked".
fn parse_fetched_boxscore(
    fixture: &FixtureRow,
    plan: &SourcePlan,
    _fetched: &FetchedDocument,
) -> std::result::Result<NormalizedBoxscore, ParseOutcome> {
    Err(ParseOutcome::new(
        "not_supported",
        format!(
            "no parser implemented for family '{}' (source={} sport={})",
            plan.parser_family, plan.provider, fixture.sport
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

/// merge_raw_labels records WHO answered alongside WHAT they said.
///
/// The `source` block is the retrieval's provenance: which `boxscore_sources` row was used,
/// which parser family read it, what trust it carried at read time, and the `source_documents`
/// id the bytes landed as. That last one is the link back to the retained page — "sources
/// prove" (mig 205) is only true if the proof is addressable from the record it produced.
///
/// `trust_state` is captured AT READ TIME rather than looked up later on purpose: a source that
/// is trusted today may be suspended next week, and a stored box score has to remember what it
/// was worth when it was taken.
fn merge_raw_labels(
    raw_labels: Value,
    warnings: Vec<String>,
    plan: &SourcePlan,
    document_id: i64,
    from_cache: bool,
) -> Value {
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
    obj.insert(
        "source".to_string(),
        json!({
            "boxscore_source_id": plan.source_id,
            "parser_family": plan.parser_family,
            "trust_state": plan.trust_state,
            "source_document_id": document_id,
            "from_cache": from_cache,
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
            event_date: "2026-08-24".to_string(),
        }
    }

    /// The empty plan is what an empty registry yields, and it must stay recognizable.
    ///
    /// This replaces `no_sport_resolves_a_source_until_the_registry_is_populated`, whose
    /// subject was a function hardcoded to return nothing. `select_source` now needs a
    /// database, so what is unit-testable is the shape it falls back to — and `handle` keys
    /// the entire `no_source` branch off `source_urls.is_empty()`.
    #[test]
    fn the_empty_plan_is_what_no_eligible_source_looks_like() {
        let plan = SourcePlan::none();
        assert!(plan.source_urls.is_empty());
        assert_eq!(plan.provider, "none");
        assert!(plan.provider_fixture_id.is_none());
        assert!(plan.source_id.is_none());
        assert!(plan.parser_family.is_empty());
    }

    /// A template renders from the fixture's own facts — the only address mig 230 left it.
    #[test]
    fn templates_render_from_the_fixtures_own_facts() {
        let f = fixture("NFL");
        assert_eq!(
            render_template("https://x.test/{date}/{home_team_id}-{away_team_id}", &f).unwrap(),
            "https://x.test/2026-08-24/8-14"
        );
        assert_eq!(
            render_template("https://x.test/{home_slug}-v-{away_slug}", &f).unwrap(),
            "https://x.test/buffalo-bills-v-kansas-city-chiefs"
        );
        assert_eq!(
            render_template("https://x.test/{season}/{league_id}/{external_id}", &f).unwrap(),
            "https://x.test/2025/0/12345"
        );
    }

    /// A placeholder with no value must NOT render — a literal `{date}` in a URL is a
    /// guaranteed 404 that still spends the domain's budget and counts against its breaker.
    #[test]
    fn an_unfillable_or_unknown_placeholder_refuses_to_render() {
        let mut f = fixture("FOOTBALL");
        f.event_date = String::new();
        assert!(render_template("https://x.test/{date}/match", &f).is_none());

        f.external_id = None;
        assert!(render_template("https://x.test/{external_id}", &f).is_none());

        // An unrecognized variable is a template bug, not a fetchable URL.
        assert!(render_template("https://x.test/{referee}", &fixture("NBA")).is_none());

        // A template needing nothing it lacks still renders.
        assert_eq!(
            render_template("https://x.test/fixed", &fixture("NBA")).unwrap(),
            "https://x.test/fixed"
        );
    }

    /// The five leagues we serve are full of accents, and an unfolded `é` percent-encodes into
    /// a URL most sites will not match.
    #[test]
    fn slugs_fold_the_accents_the_european_leagues_actually_carry() {
        assert_eq!(slugify("Atlético Madrid"), "atletico-madrid");
        assert_eq!(slugify("Bayern München"), "bayern-munchen");
        assert_eq!(slugify("Beşiktaş"), "besiktas");
        assert_eq!(slugify("Borussia Mönchengladbach"), "borussia-monchengladbach");
        assert_eq!(slugify("Brighton & Hove Albion"), "brighton-hove-albion");
        assert_eq!(slugify("  Leeds   United  "), "leeds-united");
    }

    /// An empty `fetch_policy` (the column default) must mean "the polite default", never
    /// "no spacing and no cache" — and no row may buy its way under the 2s floor.
    #[test]
    fn fetch_policy_defaults_are_polite_and_the_floor_is_unbuyable() {
        let default = FetchPolicy::default();
        let empty = policy_from_json(&json!({}));
        assert_eq!(empty.min_spacing, default.min_spacing);
        assert_eq!(empty.cache_ttl, default.cache_ttl);

        let configured = policy_from_json(&json!({"min_spacing_secs": 30, "cache_ttl_secs": 60}));
        assert_eq!(configured.min_spacing, Duration::from_secs(30));
        assert_eq!(configured.cache_ttl, Duration::from_secs(60));

        // The 4.2 law: FetchPolicy::new floors spacing at 2s whatever the row says.
        let greedy = policy_from_json(&json!({"min_spacing_secs": 0}));
        assert!(greedy.min_spacing >= Duration::from_secs(2));
        let negative = policy_from_json(&json!({"min_spacing_secs": -5}));
        assert!(negative.min_spacing >= Duration::from_secs(2));
    }

    /// A held domain is `blocked` and terminal — never a retry. The budget exists to stop the
    /// stage arguing with it.
    #[test]
    fn a_held_domain_is_blocked_rather_than_retried() {
        let skipped = budgeted_fetch_outcome(
            "https://x.test/a",
            &BudgetedFetchError::DomainSkipped {
                domain: "x.test".to_string(),
                until_secs: 900,
            },
        );
        assert_eq!(skipped.status, "blocked");
        assert_eq!(skipped.final_domain.as_deref(), Some("x.test"));

        // 403 is the "domain blocks direct fetch" case — we skip, never escalate.
        let forbidden = budgeted_fetch_outcome(
            "https://x.test/a",
            &BudgetedFetchError::Http {
                status: 403,
                final_url: "https://x.test/a".to_string(),
            },
        );
        assert_eq!(forbidden.status, "blocked");

        let missing = budgeted_fetch_outcome(
            "https://x.test/a",
            &BudgetedFetchError::Http {
                status: 404,
                final_url: "https://x.test/a".to_string(),
            },
        );
        assert_eq!(missing.status, "not_found");
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
