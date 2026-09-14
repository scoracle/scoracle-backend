//! Structured personnel changes and attributed availability reports for current evidence.

use anyhow::{Context, Result};
use sqlx::PgPool;

/// An adjudicated absence, return or withdrawal. Withdrawing an incorrect
/// record does not establish recovery; preserve those events separately.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// One adjudicated personnel change, as the DB describes it — dates already labeled by
/// `to_char` (the `Mon DD` convention the memory card and 7.10's storyline lens use), names
/// resolved, nothing rendered. The sentence is built in code (T2: describe, then derive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonnelChange {
    /// `applied` — the move is in force; `reverted` — an earlier applied move was undone.
    pub kind: String,
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

/// Load adjudicated transfers since the entity's last read. Unlike slow memory, this includes
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
               to_char(c.at, 'Mon DD') AS date_label,
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
               to_char(c.at, 'Mon DD') AS date_label,
               c.event_kind,
               COALESCE(pl.name, 'a player') AS player_name,
               t.name AS team_name,
               c.team_id,
               to_char(c.event_date, 'Mon DD') AS event_date_label,
               CASE WHEN c.expected_return IS NOT NULL
                    THEN to_char(c.expected_return, 'Mon DD') END AS expected_return_label
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

/// How many reported-availability claims reach the brief. Six, matching the personnel cap: the
/// 4,096 window still binds, and a busy treatment table must not crowd out the datapoints the
/// report is actually built on.
const MAX_AVAILABILITY_CLAIMS: usize = 6;

/// Load the Editor's injury/suspension claims for this entity — evidence the Scout weighs rather
/// than adjudicated facts it simply reports.
///
/// `Voice::Scout` selects only injury and suspension claims. `mark_contested` identifies both
/// sides of a contradiction without filtering or deciding it.
pub async fn load_availability_reports(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<crate::junctions::editor::render::MarkedClaim>> {
    use crate::junctions::editor::render::{mark_contested, slice_claims, Voice};

    if entity_type != "player" && entity_type != "team" {
        return Ok(Vec::new());
    }
    let loaded = crate::junctions::editor::packet::load_packets_for_entity(
        pool,
        entity_type,
        entity_id,
        sport,
        crate::junctions::journalist::PACKET_LOOKBACK_HOURS,
        MAX_AVAILABILITY_CLAIMS as i64,
    )
    .await
    .with_context(|| format!("load availability reports {entity_type}/{entity_id}"))?;

    let mut claims = Vec::new();
    for (view, _) in loaded {
        claims.extend(slice_claims(&view.claims, Voice::Scout));
    }
    claims.truncate(MAX_AVAILABILITY_CLAIMS);
    // Contest-marking runs across the WHOLE set, after the merge — two storylines reporting the
    // same knock differently is precisely the pair worth marking, and marking per-packet would
    // miss it.
    Ok(mark_contested(&claims))
}
