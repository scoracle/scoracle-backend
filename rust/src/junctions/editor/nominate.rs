//! Fixture nomination — the box-score fork (PLAN-one-rail 4.4; the Editor's handle).
//!
//! The news rail CAUSES the stats rail: a read whose verbatim `result_line` parses and whose
//! two team names both resolve exactly (T9 — the same surface-exact path as the resolver,
//! club-kind only) nominates a completed fixture. Code matches it against `fixtures` within
//! ±2 days; the upsert (new row, or status/score correction) trips the EXISTING
//! `fixture_boxscore_enqueue_on_final` trigger, which enqueues the Investigator. This module
//! never enqueues directly — one enqueue seam, owned by the trigger (4.4's law).
//!
//! Identity is the highest-severity failure of the phase, so every gate refuses rather than
//! guesses: an unparseable line nominates nothing; an ambiguous or unresolved team name
//! nominates nothing; two fixtures equally near the anchor nominate nothing (logged). A row
//! this module creates or corrects carries `meta.needs_verification` — the Investigator's
//! validated promotion (4.7) is what clears it.

use super::derive::{parse_result_line, resolve_names, ParsedResult};
use super::{EditorRead, NameMention};
use anyhow::{anyhow, Context, Result};
use serde_json::json;
use sqlx::{PgPool, Row};
use tracing::{info, warn};

/// `page_kind`s that may nominate (§1a): reporting pages and score tables. A roundup or
/// listing mentions many results; nominating from one would fabricate fixtures wholesale.
const NOMINATING_PAGE_KINDS: &[&str] = &["article", "score_table"];

/// What the nomination pass did for one read — logged, and returned for tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NominationOutcome {
    /// No parseable single-result line, wrong page kind, or a team failed to resolve.
    NotNominated(&'static str),
    /// Fixture existed, already completed, scores agree — nothing to write.
    AlreadyCorrect { fixture_id: i32 },
    /// Fixture existed; status and/or scores corrected (trigger enqueues).
    Corrected { fixture_id: i32 },
    /// No fixture within the window; a Scoracle-identity row was created (trigger enqueues).
    Created { fixture_id: i32 },
}

/// nominate_fixture_from_result runs the whole 4.4 chain for one persisted read.
/// Best-effort at the call site (a nomination failure must not re-run the model call), but
/// every SQL step inside is transactional and identity-gated. The time anchor is the
/// article's `COALESCE(published_at, fetched_at)`, read in SQL — this crate never decodes
/// timestamps into Rust (the codebase-wide discipline), and the season label derives from
/// the anchor in SQL too.
pub async fn nominate_fixture_from_result(
    pool: &PgPool,
    sport: &str,
    article_id: i64,
    read: &EditorRead,
) -> Result<NominationOutcome> {
    if !NOMINATING_PAGE_KINDS
        .iter()
        .any(|k| read.page_kind.eq_ignore_ascii_case(k))
    {
        return Ok(NominationOutcome::NotNominated("page_kind"));
    }
    let Some(parsed) = parse_result_line(&read.result_line) else {
        return Ok(NominationOutcome::NotNominated("result_line"));
    };

    // Resolve each parsed team name through the SAME exact-surface path as the resolver, as a
    // club-kind mention (the kind gate then only admits `team` surfaces). One name per call so
    // the name→link association is positional and exact — no re-derivation of nrm() in Rust.
    // Anything short of two distinct unambiguous team links refuses.
    let Some(home_id) = resolve_one_team(pool, sport, &parsed.home).await? else {
        return Ok(NominationOutcome::NotNominated("team_unresolved"));
    };
    let Some(away_id) = resolve_one_team(pool, sport, &parsed.away).await? else {
        return Ok(NominationOutcome::NotNominated("team_unresolved"));
    };
    if home_id == away_id {
        return Ok(NominationOutcome::NotNominated("same_team"));
    }

    upsert_nominated_fixture(pool, sport, article_id, &parsed, home_id, away_id).await
}

/// resolve_one_team resolves a single parsed team name to a team id, or None when the
/// resolver's verdict is anything but one unambiguous team link (unresolved and ambiguous
/// both refuse — a name match alone never merges identities).
async fn resolve_one_team(pool: &PgPool, sport: &str, name: &str) -> Result<Option<i32>> {
    let mention = NameMention {
        name: name.to_string(),
        kind_hint: "club".to_string(),
        descriptor: String::new(),
    };
    let resolved = resolve_names(pool, sport, std::slice::from_ref(&mention)).await?;
    Ok(match resolved.links.as_slice() {
        [link] if link.entity_type == "team" => Some(link.entity_id),
        _ => None,
    })
}

async fn upsert_nominated_fixture(
    pool: &PgPool,
    sport: &str,
    article_id: i64,
    parsed: &ParsedResult,
    home_id: i32,
    away_id: i32,
) -> Result<NominationOutcome> {
    let mut tx = pool.begin().await.context("begin fixture nomination")?;

    // The anchor: when the article says the game happened. Read in-tx so match and upsert
    // see one value.
    let anchor_exists: Option<bool> =
        sqlx::query_scalar("SELECT true FROM public.news_articles WHERE id = $1")
            .bind(article_id)
            .fetch_optional(&mut *tx)
            .await
            .context("load nomination anchor article")?;
    if anchor_exists.is_none() {
        return Err(anyhow!("article {article_id} vanished before nomination"));
    }

    // Nearest fixture within ±2d of the anchor, either orientation. Two candidates tied to
    // the minute would be a data defect; ORDER BY closeness is deterministic given the ±2d
    // window and one-match-per-pair-per-window reality of league play.
    let row = sqlx::query(
        r#"
        WITH anchor AS (
            SELECT COALESCE(a.published_at, a.fetched_at) AS t
            FROM public.news_articles a WHERE a.id = $4
        )
        SELECT f.id, f.status, f.home_team_id, f.away_team_id, f.home_score, f.away_score
        FROM public.fixtures f, anchor
        WHERE f.sport = $1
          AND ((f.home_team_id = $2 AND f.away_team_id = $3)
            OR (f.home_team_id = $3 AND f.away_team_id = $2))
          AND f.start_time BETWEEN anchor.t - interval '2 days'
                               AND anchor.t + interval '2 days'
        ORDER BY abs(extract(epoch FROM (f.start_time - anchor.t)))
        LIMIT 1
        "#,
    )
    .bind(sport)
    .bind(home_id)
    .bind(away_id)
    .bind(article_id)
    .fetch_optional(&mut *tx)
    .await
    .context("match nominated fixture")?;

    let meta_patch = json!({
        "needs_verification": true,
        "nominated_by": "editor",
        "article_id": article_id,
    });

    let outcome = match row {
        Some(row) => {
            let fixture_id: i32 = row.get("id");
            let status: String = row.get("status");
            let f_home: i32 = row.get("home_team_id");
            // Orientation-aware scores: if the fixture stores the reversed pairing, the
            // parsed home score belongs to the fixture's away side.
            let (want_home, want_away) = if f_home == home_id {
                (parsed.home_score as i32, parsed.away_score as i32)
            } else {
                (parsed.away_score as i32, parsed.home_score as i32)
            };
            let cur_home: Option<i32> = row.get("home_score");
            let cur_away: Option<i32> = row.get("away_score");
            let is_final = status == "completed" || status == "seeded";
            if is_final && cur_home == Some(want_home) && cur_away == Some(want_away) {
                NominationOutcome::AlreadyCorrect { fixture_id }
            } else {
                // A `seeded` fixture whose scores AGREE never regresses to completed (the
                // branch above); disagreement or a non-final status is a correction. The
                // trigger fires off this UPDATE (status/scores in its column list).
                sqlx::query(
                    r#"
                    UPDATE public.fixtures
                    SET status = CASE WHEN status = 'seeded' THEN status ELSE 'completed' END,
                        home_score = $2,
                        away_score = $3,
                        meta = meta || $4::jsonb,
                        updated_at = NOW()
                    WHERE id = $1
                    "#,
                )
                .bind(fixture_id)
                .bind(want_home)
                .bind(want_away)
                .bind(&meta_patch)
                .execute(&mut *tx)
                .await
                .context("correct nominated fixture")?;
                NominationOutcome::Corrected { fixture_id }
            }
        }
        None => {
            // Scoracle identity, not provider identity: external_id NULL. league_id only
            // when both teams agree on one; start_time is the article anchor and season is
            // its starting-year label (July boundary — the measured provider-era
            // convention), both derived in SQL. The Investigator's verified fetch revises
            // start_time.
            let fixture_id: i32 = sqlx::query_scalar(
                r#"
                WITH anchor AS (
                    SELECT COALESCE(a.published_at, a.fetched_at) AS t
                    FROM public.news_articles a WHERE a.id = $7
                )
                INSERT INTO public.fixtures
                    (sport, season, league_id, home_team_id, away_team_id, start_time,
                     status, home_score, away_score, external_id, meta)
                SELECT $1,
                       CASE WHEN EXTRACT(MONTH FROM anchor.t) >= 7
                            THEN EXTRACT(YEAR FROM anchor.t)::int
                            ELSE EXTRACT(YEAR FROM anchor.t)::int - 1 END,
                       CASE WHEN ht.league_id = at.league_id THEN ht.league_id ELSE NULL END,
                       $2, $3, anchor.t, 'completed', $4, $5, NULL, $6::jsonb
                FROM anchor, public.teams ht, public.teams at
                WHERE ht.id = $2 AND ht.sport = $1 AND at.id = $3 AND at.sport = $1
                RETURNING id
                "#,
            )
            .bind(sport)
            .bind(home_id)
            .bind(away_id)
            .bind(parsed.home_score as i32)
            .bind(parsed.away_score as i32)
            .bind(&meta_patch)
            .bind(article_id)
            .fetch_one(&mut *tx)
            .await
            .context("insert nominated fixture")?;
            NominationOutcome::Created { fixture_id }
        }
    };

    tx.commit().await.context("commit fixture nomination")?;

    match &outcome {
        NominationOutcome::Created { fixture_id } => info!(
            fixture_id,
            article_id,
            sport,
            home_id,
            away_id,
            home_score = parsed.home_score,
            away_score = parsed.away_score,
            "editor nominated NEW fixture from result_line"
        ),
        NominationOutcome::Corrected { fixture_id } => info!(
            fixture_id,
            article_id, sport, "editor corrected fixture from result_line"
        ),
        NominationOutcome::AlreadyCorrect { .. } | NominationOutcome::NotNominated(_) => {}
    }
    Ok(outcome)
}

/// Best-effort wrapper for the stage handle: nomination failures are logged, never bubbled —
/// the read is already persisted, and re-running a model call over a SQL hiccup is the wrong
/// trade. The Investigator's dead-letter view (Phase 8 gate) is where persistent breakage
/// surfaces.
pub async fn nominate_best_effort(pool: &PgPool, sport: &str, article_id: i64, read: &EditorRead) {
    match nominate_fixture_from_result(pool, sport, article_id, read).await {
        Ok(_) => {}
        Err(e) => warn!(
            article_id,
            sport,
            error = %format!("{e:#}"),
            "fixture nomination failed (read already persisted; continuing)"
        ),
    }
}
