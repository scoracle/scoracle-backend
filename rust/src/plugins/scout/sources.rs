//! Structured personnel changes and attributed availability reports for current evidence.

use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

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

/// Read the Scout's own accepted Harvester sources. These are publisher quotes,
/// never Editor key facts or packet prose. Recheck the retained bytes on read so
/// a later article edit cannot silently become current evidence.
pub async fn load_scout_reports(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<crate::plugins::scout::reports::MarkedClaim>> {
    use crate::plugins::scout::reports::{mark_contested, RenderClaim};

    if entity_type != "player" && entity_type != "team" {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "SELECT c.article_id,c.headline,c.context_text,c.context_start,c.context_end, \
                c.body_sha256,a.title,a.full_text,COALESCE(a.source,'') AS source, \
                EXTRACT(EPOCH FROM COALESCE(a.published_at,a.fetched_at))::bigint AS published_at, \
                d.product_ref->>'source_quote' AS source_quote, \
                d.product_ref->>'source_kind' AS source_kind, \
                availability.kind AS availability_kind, \
                (availability.status='applied' AND availability.reverted_at IS NULL) AS availability_applied, \
                (availability.source_article_id=c.article_id AND availability.sport=c.sport AND \
                 ((c.entity_type='player' AND availability.player_id=c.entity_id) OR \
                  (c.entity_type='team' AND availability.team_id=c.entity_id))) AS availability_entity_matches, \
                (transfer.status='applied' AND transfer.reverted_at IS NULL) AS transfer_applied, \
                (transfer.sport=c.sport AND \
                 ((c.entity_type='player' AND transfer.player_id=c.entity_id) OR \
                  (c.entity_type='team' AND c.entity_id IN (transfer.old_team_id,transfer.new_team_id)))) AS transfer_entity_matches, \
                transfer.evidence->'identity_evidence_article_ids' @> jsonb_build_array(c.article_id) AS transfer_source_matches \
         FROM public.harvester_assignments d \
         JOIN public.harvester_classifications c ON c.id=d.classification_id \
         JOIN public.news_articles a ON a.id=c.article_id \
         LEFT JOIN public.player_availability availability \
           ON availability.id=NULLIF(d.product_ref->>'structured_record_id','')::bigint \
         LEFT JOIN public.transfer_identity_applications transfer \
           ON transfer.id=NULLIF(d.product_ref->>'structured_record_id','')::bigint \
         WHERE d.plugin_id=$1 AND d.status='used' AND c.entity_type=$2 \
           AND c.entity_id=$3 AND c.sport=$4 \
           AND COALESCE(a.published_at,a.fetched_at) >= now()-make_interval(hours=>$5::int) \
         ORDER BY COALESCE(a.published_at,a.fetched_at) DESC,c.article_id DESC \
         LIMIT $6",
    )
    .bind(crate::plugins::scout::manifest::MANIFEST.id.as_str())
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(SCOUT_REPORT_LOOKBACK_HOURS as i32)
    .bind(MAX_SCOUT_CLAIMS as i64)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load Harvester Scout reports {entity_type}/{entity_id}"))?;
    let mut claims = Vec::new();
    for row in rows {
        let article_id: i64 = row.get("article_id");
        let body: Option<String> = row.get("full_text");
        let body =
            body.with_context(|| format!("Scout Harvester body missing for {article_id}"))?;
        let hash: String = row.get("body_sha256");
        let start: i32 = row.get("context_start");
        let end: i32 = row.get("context_end");
        let context: String = row.get("context_text");
        let headline: String = row.get("headline");
        let title: String = row.get("title");
        ensure!(
            headline == title,
            "Scout Harvester headline drift for {article_id}"
        );
        ensure!(
            hex::encode(Sha256::digest(body.as_bytes())) == hash,
            "Scout Harvester body drift for {article_id}"
        );
        ensure!(
            start >= 0
                && end >= start
                && body.get(start as usize..end as usize) == Some(context.as_str()),
            "Scout Harvester opening drift for {article_id}"
        );
        let quote: Option<String> = row.get("source_quote");
        let kind: Option<String> = row.get("source_kind");
        let (Some(quote), Some(kind)) = (quote, kind) else {
            continue;
        };
        ensure!(
            !quote.trim().is_empty() && (headline.contains(&quote) || context.contains(&quote)),
            "Scout Harvester source quote drift for {article_id}"
        );
        ensure!(
            matches!(kind.as_str(), "performance" | "roster" | "availability"),
            "Scout Harvester source kind invalid for {article_id}"
        );
        let story_type = if kind == "availability" {
            let availability_kind: Option<String> = row.get("availability_kind");
            let availability_kind = availability_kind.with_context(|| {
                format!("Scout structured availability missing for {article_id}")
            })?;
            let availability_applied: Option<bool> = row.get("availability_applied");
            let availability_entity_matches: Option<bool> = row.get("availability_entity_matches");
            ensure!(
                availability_applied == Some(true) && availability_entity_matches == Some(true),
                "Scout structured availability no longer applied for {article_id}"
            );
            ensure!(
                matches!(availability_kind.as_str(), "injury" | "suspension"),
                "Scout structured availability kind invalid for {article_id}"
            );
            availability_kind
        } else if kind == "roster" {
            let transfer_applied: Option<bool> = row.get("transfer_applied");
            let transfer_entity_matches: Option<bool> = row.get("transfer_entity_matches");
            let source_matches: Option<bool> = row.get("transfer_source_matches");
            ensure!(
                transfer_applied == Some(true)
                    && transfer_entity_matches == Some(true)
                    && source_matches == Some(true),
                "Scout structured roster evidence no longer applied for {article_id}"
            );
            kind
        } else {
            kind
        };
        claims.push(RenderClaim {
            article_id,
            source: row.get("source"),
            fact: quote,
            published_at: row.get("published_at"),
            story_type,
        });
    }
    Ok(mark_contested(&claims))
}
