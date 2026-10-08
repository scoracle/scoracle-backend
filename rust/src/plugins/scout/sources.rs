//! Structured personnel changes and attributed availability reports for current evidence.

use anyhow::{ensure, Context, Result};
use sqlx::PgPool;

/// An adjudicated absence, return or withdrawal. Withdrawing an incorrect
/// record does not establish recovery; preserve those events separately.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AvailabilityChange {
    /// `opened` — newly ruled out; `returned` — availability resumed (a real-world outcome);
    /// `reverted` — the RECORD was wrong and has been withdrawn (a correction, never a return).
    pub kind: String,
    /// When the thing that is NEW happened — the apply, the return, or the withdrawal.
    pub date_label: String,
    /// `injury` or `suspension`, the adjudicated enum. Never model prose.
    pub event_kind: String,
    pub player_name: String,
    /// The club the player was at when it happened; `None` when unattached or unresolved.
    pub team_name: Option<String>,
    pub team_id: Option<i32>,
    /// The day the player became unavailable — carried even on a return, because "out Aug 20,
    /// back Aug 30" is the fact, not "back Aug 30".
    pub event_date_label: String,
    /// The prognosis AS REPORTED. Renderable; never ground truth (mig 229).
    pub expected_return_label: Option<String>,
}

/// How many availability lines render before the block starts naming drops instead.
///
/// Four, against personnel's six, and the two budgets are deliberately separate but summed
/// against the same ceiling: the rating prompt lives inside one 4,096-token window, and a
/// deadline-day squad churn plus a treatment-table update must not between them crowd out the
/// datapoints the report is actually built on.
pub(crate) const MAX_AVAILABILITY_LINES: usize = 4;

/// One stored personnel change with full UTC reconciliation dates and resolved
/// names. DuckDB analyzes the snapshot before it becomes model-facing memory.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PersonnelChange {
    /// `applied` — the move is in force; `reverted` — an earlier applied move was undone.
    pub kind: String,
    /// When the identity reconciliation was applied or withdrawn. This is not a signing date.
    pub date_label: String,
    /// The adjudicated event label (`transfer`, `rumor`, …). Never model prose — it is the
    /// Insider's structured `event_type` column.
    pub event_type: Option<String>,
    pub player_name: String,
    pub old_team: Option<String>,
    pub new_team: Option<String>,
    /// Carried so a TEAM read can tell an arrival from a departure by id rather than by
    /// comparing rendered names, which collide across leagues.
    pub old_team_id: Option<i32>,
    pub new_team_id: Option<i32>,
}

/// How many personnel lines the block renders before it starts naming drops instead. Six is
/// ~140 tokens — a deadline-day squad churn cannot crowd out the datapoints inside 4,096.
pub(crate) const MAX_PERSONNEL_LINES: usize = 6;
/// The lookback when this entity has never been read: a first brief still deserves recent
/// personnel facts, but not a year of them.
const PERSONNEL_FIRST_READ_DAYS: i32 = 30;
/// The hard ceiling on the lookback however stale the last read is — an entity nobody has
/// scouted since preseason gets the recent moves, not its whole transfer history.
const PERSONNEL_MAX_DAYS: i32 = 180;

/// Load adjudicated current-team reconciliations since the entity's last read. Unlike slow memory, this includes
/// departures, source clubs, and reverts. Only structured facts reach the Scout. Returns the
/// newest rows plus the pre-cap total so exclusions are explicit.
pub async fn load_personnel_changes(
    pool: &PgPool,
    sport: &str,
    entity_type: &str,
    entity_id: i32,
) -> Result<(Vec<PersonnelChange>, usize)> {
    if entity_type != "player" && entity_type != "team" {
        return Ok((Vec::new(), 0));
    }
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<i32>,
        Option<i32>,
    )> = sqlx::query_as(
        r#"
        WITH since AS (
            SELECT greatest(
                       COALESCE(
                           (SELECT max(s.generated_at) FROM public.stat_summaries s
                             WHERE s.entity_type = $2 AND s.entity_id = $3 AND s.sport = $1
                               AND s.body IS NOT NULL),
                           now() - make_interval(days => $4)),
                       now() - make_interval(days => $5)) AS at
        ),
        changes AS (
            SELECT 'applied'::text AS kind, a.applied_at AS at, a.event_type,
                   a.player_id, a.old_team_id, a.new_team_id
              FROM public.transfer_identity_applications a
             WHERE a.sport = $1 AND a.status = 'applied' AND a.reverted_at IS NULL
               AND a.applied_at IS NOT NULL AND a.applied_at > (SELECT at FROM since)
            UNION ALL
            -- A revert is dated by WHEN IT WAS UNDONE: that is the fact that is new since the
            -- last read, whatever the original move's date was.
            SELECT 'reverted'::text, a.reverted_at, a.event_type,
                   a.player_id, a.old_team_id, a.new_team_id
              FROM public.transfer_identity_applications a
             WHERE a.sport = $1 AND a.reverted_at IS NOT NULL
               AND a.reverted_at > (SELECT at FROM since)
        )
        SELECT c.kind,
               to_char(c.at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS date_label,
               c.event_type,
               COALESCE(pl.name, 'a player') AS player_name,
               told.name AS old_team,
               tnew.name AS new_team,
               c.old_team_id,
               c.new_team_id
          FROM changes c
          JOIN public.players pl ON pl.id = c.player_id AND pl.sport = $1
          LEFT JOIN public.teams told ON told.id = c.old_team_id AND told.sport = $1
          LEFT JOIN public.teams tnew ON tnew.id = c.new_team_id AND tnew.sport = $1
         WHERE ($2 = 'player' AND c.player_id = $3)
            OR ($2 = 'team' AND ($3 = c.new_team_id OR $3 = c.old_team_id))
         ORDER BY c.at DESC
        "#,
    )
    .bind(sport)
    .bind(entity_type)
    .bind(entity_id)
    .bind(PERSONNEL_FIRST_READ_DAYS)
    .bind(PERSONNEL_MAX_DAYS)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load personnel changes {entity_type}/{entity_id}"))?;

    let total = rows.len();
    let changes = rows
        .into_iter()
        .take(MAX_PERSONNEL_LINES)
        .map(
            |(
                kind,
                date_label,
                event_type,
                player_name,
                old_team,
                new_team,
                old_team_id,
                new_team_id,
            )| PersonnelChange {
                kind,
                date_label,
                event_type,
                player_name,
                old_team,
                new_team,
                old_team_id,
                new_team_id,
            },
        )
        .collect();
    Ok((changes, total))
}

/// Load adjudicated availability changes since the last read. Three distinct kinds are retained:
/// `opened` (newly ruled out),
/// `returned` (`returned_at` — availability actually resumed, a real-world outcome), and
/// `reverted` (`reverted_at` — the RECORD was wrong, a correction). Rendering a revert as a
/// return would tell the Scout a player is fit when what actually happened is that we withdrew
/// the claim that he was ever hurt.
///
/// The `since` window is the personnel window exactly — same clamp, same first-read floor — so
/// the two halves of one block cannot disagree about what "since our last read" means.
///
/// Returns newest-first plus the TOTAL that qualified, so the renderer names what the cap
/// dropped (the A5 rule) instead of silently truncating.
pub async fn load_availability_changes(
    pool: &PgPool,
    sport: &str,
    entity_type: &str,
    entity_id: i32,
) -> Result<(Vec<AvailabilityChange>, usize)> {
    if entity_type != "player" && entity_type != "team" {
        return Ok((Vec::new(), 0));
    }
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        Option<i32>,
        String,
        Option<String>,
    )> = sqlx::query_as(
        r#"
        WITH since AS (
            SELECT greatest(
                       COALESCE(
                           (SELECT max(s.generated_at) FROM public.stat_summaries s
                             WHERE s.entity_type = $2 AND s.entity_id = $3 AND s.sport = $1
                               AND s.body IS NOT NULL),
                           now() - make_interval(days => $4)),
                       now() - make_interval(days => $5)) AS at
        ),
        changes AS (
            -- Newly ruled out. Dated by the APPLY, not the event: an injury adjudicated today
            -- for a knock last Saturday is new information today.
            SELECT 'opened'::text AS kind, a.applied_at AS at, a.kind AS event_kind,
                   a.player_id, a.team_id, a.event_date, a.expected_return
              FROM public.player_availability a
             WHERE a.sport = $1 AND a.status = 'applied' AND a.reverted_at IS NULL
               AND a.applied_at IS NOT NULL AND a.applied_at > (SELECT at FROM since)
            UNION ALL
            -- Came back. A real-world outcome, and the propensity denominator.
            SELECT 'returned', a.returned_at::timestamptz, a.kind,
                   a.player_id, a.team_id, a.event_date, a.expected_return
              FROM public.player_availability a
             WHERE a.sport = $1 AND a.status = 'applied' AND a.reverted_at IS NULL
               AND a.returned_at IS NOT NULL
               AND a.returned_at > (SELECT at FROM since)::date
            UNION ALL
            -- The record was withdrawn. Dated by WHEN IT WAS UNDONE — that is what is new,
            -- whatever the original event's date was (the personnel read's own convention).
            SELECT 'reverted', a.reverted_at, a.kind,
                   a.player_id, a.team_id, a.event_date, a.expected_return
              FROM public.player_availability a
             WHERE a.sport = $1 AND a.reverted_at IS NOT NULL
               AND a.reverted_at > (SELECT at FROM since)
        )
        SELECT c.kind,
               to_char(c.at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS date_label,
               c.event_kind,
               COALESCE(pl.name, 'a player') AS player_name,
               t.name AS team_name,
               c.team_id,
               to_char(c.event_date, 'YYYY-MM-DD') AS event_date_label,
               CASE WHEN c.expected_return IS NOT NULL
                    THEN to_char(c.expected_return, 'YYYY-MM-DD') END AS expected_return_label
          FROM changes c
          JOIN public.players pl ON pl.id = c.player_id AND pl.sport = $1
          LEFT JOIN public.teams t ON t.id = c.team_id AND t.sport = $1
         WHERE ($2 = 'player' AND c.player_id = $3)
            OR ($2 = 'team' AND c.team_id = $3)
         ORDER BY c.at DESC
        "#,
    )
    .bind(sport)
    .bind(entity_type)
    .bind(entity_id)
    .bind(PERSONNEL_FIRST_READ_DAYS)
    .bind(PERSONNEL_MAX_DAYS)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load availability changes {entity_type}/{entity_id}"))?;

    let total = rows.len();
    let changes = rows
        .into_iter()
        .take(MAX_AVAILABILITY_LINES)
        .map(
            |(
                kind,
                date_label,
                event_kind,
                player_name,
                team_name,
                team_id,
                event_date_label,
                expected_return_label,
            )| AvailabilityChange {
                kind,
                date_label,
                event_kind,
                player_name,
                team_name,
                team_id,
                event_date_label,
                expected_return_label,
            },
        )
        .collect();
    Ok((changes, total))
}

/// How many attributed current reports reach the brief. Six matches the
/// personnel cap so news cannot crowd measurements out of the 4,096 window.
const MAX_SCOUT_CLAIMS: usize = 6;
/// Current reporting needs enough room to survive a quiet week between fixtures. This matches
/// the transfer corpus freshness boundary while the claim cap continues to bind prompt size.
const SCOUT_REPORT_LOOKBACK_HOURS: i64 = 14 * 24;

/// Previously accepted native reporting; structured claims remain tied to applied records.
pub async fn load_scout_reports(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<crate::tools::source::SourceContext>> {
    let now = crate::plugins::influencer::now();
    let plugin = super::manifest::MANIFEST.id.as_str();
    let sources = crate::plugins::classifier::delivery::load_used(
        pool,
        plugin,
        entity_type,
        entity_id,
        sport,
        now - SCOUT_REPORT_LOOKBACK_HOURS * 3600,
        now + 1,
    )
    .await?;
    let mut accepted = Vec::new();
    for mut source in sources {
        if !source
            .published_at_epoch
            .is_some_and(|date| date >= now - SCOUT_REPORT_LOOKBACK_HOURS * 3600 && date <= now)
        {
            continue;
        }
        let product:serde_json::Value=sqlx::query_scalar("SELECT product_ref FROM classifier_deliveries WHERE measurement_id=$1 AND plugin_id=$2")
            .bind(source.classification_id).bind(plugin).fetch_one(pool).await?;
        let kind = match product["source_kind"].as_str() {
            Some("performance") => super::delivery::SourceKind::Performance,
            Some("roster") => super::delivery::SourceKind::Roster,
            Some("availability") => super::delivery::SourceKind::Availability,
            _ => anyhow::bail!("Scout accepted source kind unavailable"),
        };
        if kind != super::delivery::SourceKind::Performance {
            let id = product["structured_record_id"]
                .as_i64()
                .context("Scout structured reference missing")?;
            let query = if kind == super::delivery::SourceKind::Availability {
                "SELECT status='applied' AND reverted_at IS NULL,reverted_at IS NOT NULL
                    FROM player_availability WHERE id=$1 AND sport=$2 AND source_article_id=$5
                    AND (($3='player' AND player_id=$4) OR ($3='team' AND team_id=$4))"
            } else {
                "SELECT status='applied' AND reverted_at IS NULL,reverted_at IS NOT NULL
                    FROM transfer_identity_applications WHERE id=$1 AND sport=$2
                    AND evidence->'identity_evidence_article_ids' @> jsonb_build_array($5::bigint)
                    AND (($3='player' AND player_id=$4) OR ($3='team' AND $4 IN (old_team_id,new_team_id)))"
            };
            let (applied, withdrawn): (bool, bool) = sqlx::query_as(query)
                .bind(id)
                .bind(sport)
                .bind(entity_type)
                .bind(entity_id)
                .bind(source.article_id)
                .fetch_optional(pool)
                .await?
                .context("Scout structured ownership changed")?;
            source
                .classifier_world
                .as_mut()
                .context("Scout world missing")?["structured_record"] =
                serde_json::json!({"currently_applied":applied,"withdrawn":withdrawn});
        }
        accepted.push(source);
    }
    ensure!(
        accepted.len() <= MAX_SCOUT_CLAIMS,
        "Scout complete reporting population exceeds limit"
    );
    Ok(accepted)
}
