//! Concrete PostgreSQL reads used to prepare Scout assignments.

use crate::plugins::scout::cognition::{RatingDatapoint, RatingProfile, RatingTrajectory};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sqlx::{PgPool, Row};
use std::collections::HashMap;

/// load_rating_profile reads the entity's rating row for `season` (None = latest). Prefers the
/// unscoped row, falling back to the richest league row (FOOTBALL is league-scoped). Numeric scores
/// are cast to float8 and JSONB to text for sqlx/serde decoding.
/// Returns `None` when there is no rating row at all.
pub async fn load_rating_profile(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: Option<i32>,
) -> Result<Option<RatingProfile>> {
    // Rate modes exist only for player/minutes stats; teams receive an empty object.
    let (id_col, table, pos_select, modes_select) = match entity_type {
        "player" => (
            "player_id",
            "player_stats",
            "COALESCE(position, '')",
            "COALESCE(rating_modes, '{}'::jsonb)::text",
        ),
        "team" => ("team_id", "team_stats", "''::text", "'{}'::text"),
        _ => bail!("unknown entity type {entity_type:?}"),
    };
    // Recompute the player eligibility gate from source stats as well as trusting the stored
    // bundle. This keeps pre-253 rows from exposing percentiles for a one-appearance sample while
    // the additive rating-evidence migration is waiting to deploy. It is the same self-scaling
    // threshold used by migration 253. Team ratings do not use the player participation gate.
    let eligibility_select = if entity_type == "player" {
        r#"
        COALESCE((
            SELECT bool_and(
                COALESCE(NULLIF(player_stats.stats->>rt.stat_key, '')::numeric, 0)
                >= LEAST(
                    rt.min_value,
                    GREATEST(1, ceil(0.5 * COALESCE((
                        SELECT MAX(NULLIF(ps2.stats->>rt.stat_key, '')::numeric)
                        FROM public.player_stats ps2
                        WHERE ps2.sport = $1 AND ps2.season = player_stats.season
                    ), 0)))
                )
            )
            FROM public.rating_thresholds rt
            WHERE rt.sport = $1
        ), FALSE)
        "#
    } else {
        "TRUE"
    };
    // The unscoped row first (NBA/NFL carry league_id 0/NULL), else the richest league row (the
    // most-datapoints row is the main competition — domestic league over a cup).
    let q = format!(
        r#"
        SELECT season, {pos_select},
               rating_score::float8,
               COALESCE(rating_breakdown, '[]'::jsonb)::text,
               COALESCE(rating_scoped_ranks, '{{}}'::jsonb)::text,
               {modes_select}, NULLIF(league_id,0), updated_at::date::text,
               COALESCE((
                   SELECT jsonb_object_agg(sd.display_name, NULLIF(stats->>sd.key_name,'')::numeric)
                   FROM public.stat_definitions sd
                   WHERE sd.sport = $1 AND sd.entity_type = $4
                     AND (sd.key_name IN ('appearances','games_played','matches_played','minutes_played')
                          OR sd.key_name IN (SELECT stat_key FROM public.rating_thresholds WHERE sport=$1))
                     AND NULLIF(stats->>sd.key_name,'') IS NOT NULL
               ), '{{}}'::jsonb),
               {eligibility_select} AS rank_eligible
        FROM public.{table}
        WHERE sport = $1 AND {id_col} = $2 AND ($3::int IS NULL OR season = $3)
        ORDER BY season DESC,
                 (COALESCE(league_id, 0) = 0) DESC,
                 jsonb_array_length(COALESCE(rating_breakdown, '[]'::jsonb)) DESC,
                 COALESCE(league_id, 0) ASC
        LIMIT 1
        "#
    );
    let Some(row) = sqlx::query(&q)
        .bind(sport)
        .bind(entity_id)
        .bind(season)
        .bind(entity_type)
        .fetch_optional(pool)
        .await
        .context("load rating profile")?
    else {
        return Ok(None);
    };

    let season: i32 = row.get(0);
    let position: String = row.get(1);
    let mut composite_score: Option<f64> = row.get(2);
    let breakdown_raw: String = row.get(3);
    let scoped_raw: String = row.get(4);
    let modes_raw: String = row.get(5);

    let mut breakdown: Vec<RatingDatapoint> =
        serde_json::from_str(&breakdown_raw).context("unmarshal rating_breakdown")?;
    // Cohort framing and per-x modes are optional enrichment.
    let mut scoped_ranks: HashMap<String, f64> =
        serde_json::from_str(&scoped_raw).unwrap_or_default();
    let mut rate_modes = parse_rate_modes(&modes_raw);
    let rank_eligible: bool = row.get(9);
    if !rank_eligible {
        composite_score = None;
        scoped_ranks.clear();
        clear_datapoint_ranks(&mut breakdown);
        for datapoints in rate_modes.values_mut() {
            clear_datapoint_ranks(datapoints);
        }
    }

    Ok(Some(RatingProfile {
        league_id: row.get(6),
        observed_at: row.get(7),
        sample: serde_json::from_value(row.get(8)).context("decode rating sample")?,
        entity_type: entity_type.to_string(),
        season,
        position,
        composite_score,
        breakdown,
        scoped_ranks,
        rate_modes,
    }))
}

fn clear_datapoint_ranks(datapoints: &mut [RatingDatapoint]) {
    for datapoint in datapoints {
        datapoint.z = None;
        datapoint.pct = None;
        datapoint.scoped_pct.clear();
    }
}

/// parse_rate_modes reads `rating_modes` — a per-x bundle per mode (`{"per_36": {"breakdown": [...]}}`),
/// keeping only non-empty breakdowns. A parse error yields no optional modes.
fn parse_rate_modes(raw: &str) -> HashMap<String, Vec<RatingDatapoint>> {
    #[derive(Deserialize)]
    struct ModeWrap {
        #[serde(default)]
        breakdown: Vec<RatingDatapoint>,
    }
    let parsed: HashMap<String, ModeWrap> = match serde_json::from_str(raw) {
        Ok(m) => m,
        Err(_) => return HashMap::new(),
    };
    parsed
        .into_iter()
        .filter(|(_, m)| !m.breakdown.is_empty())
        .map(|(name, m)| (name, m.breakdown))
        .collect()
}

pub async fn load_rating_trajectory(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    profile: &RatingProfile,
) -> Result<RatingTrajectory> {
    if !matches!(entity_type, "player" | "team") {
        bail!("unknown entity type {entity_type:?}");
    }
    // Window, slope and category are computed here so Rust does not keep a second producer.
    // Archbox, 14 days of unchanged rows: 200/200 matched stored slope, series, window and sample.
    let row = sqlx::query(
        r#"
        WITH counted AS (
            SELECT (
                SELECT count(*)::bigint FROM public.event_box_scores e
                WHERE $4 = 'player' AND e.player_id = $1 AND e.sport = $2 AND e.season = $3
                  AND e.rating IS NOT NULL
            ) + (
                SELECT count(*)::bigint FROM public.event_team_stats e
                WHERE $4 = 'team' AND e.team_id = $1 AND e.sport = $2 AND e.season = $3
                  AND e.rating IS NOT NULL
            ) AS events_played
        ),
        windowed AS (
            SELECT events_played,
                   CASE WHEN events_played < 3 THEN 0
                        ELSE least(16, greatest(3, round(events_played::numeric * 0.10)))::int
                   END AS window_size
            FROM counted
        ),
        newest AS (
            SELECT COALESCE(array_agg(rating ORDER BY start_time DESC), ARRAY[]::float8[]) AS ratings
            FROM (
                SELECT rating, start_time
                FROM (
                    SELECT e.rating::float8 AS rating, f.start_time
                    FROM public.event_box_scores e
                    JOIN public.fixtures f ON f.id = e.fixture_id
                    WHERE $4 = 'player' AND e.player_id = $1 AND e.sport = $2 AND e.season = $3
                      AND e.rating IS NOT NULL
                      AND (SELECT window_size FROM windowed) > 0
                    UNION ALL
                    SELECT e.rating::float8, f.start_time
                    FROM public.event_team_stats e
                    JOIN public.fixtures f ON f.id = e.fixture_id
                    WHERE $4 = 'team' AND e.team_id = $1 AND e.sport = $2 AND e.season = $3
                      AND e.rating IS NOT NULL
                      AND (SELECT window_size FROM windowed) > 0
                ) raw
                ORDER BY start_time DESC
                LIMIT (SELECT window_size FROM windowed)
            ) capped
        ),
        sloped AS (
            SELECT w.events_played,
                   w.window_size,
                   COALESCE(cardinality(n.ratings), 0) AS sample_size,
                   n.ratings,
                   (
                     SELECT num / NULLIF(den, 0)
                     FROM (
                       SELECT (cardinality(chrono) - 1)::float8 / 2 AS mean_x,
                              (SELECT avg(y) FROM unnest(chrono) y) AS mean_y,
                              chrono
                       FROM (
                         SELECT array_agg(y ORDER BY ord DESC) AS chrono
                         FROM unnest(n.ratings) WITH ORDINALITY t(y, ord)
                       ) c
                     ) m
                     CROSS JOIN LATERAL (
                       SELECT sum(((ord - 1)::float8 - mean_x) * (y - mean_y)) AS num,
                              sum(((ord - 1)::float8 - mean_x) * ((ord - 1)::float8 - mean_x)) AS den
                       FROM unnest(m.chrono) WITH ORDINALITY u(y, ord)
                     ) a
                   ) AS raw_slope
            FROM windowed w
            CROSS JOIN newest n
        )
        SELECT events_played,
               window_size,
               sample_size,
               CASE WHEN sample_size < 2 OR raw_slope IS NULL THEN NULL
                    ELSE (round((raw_slope * 10)::numeric) / 10)::float8
               END AS slope,
               COALESCE((
                 SELECT jsonb_agg(round((y * 10)::numeric) / 10 ORDER BY ord)
                 FROM unnest(ratings) WITH ORDINALITY t(y, ord)
               ), '[]'::jsonb)::text AS series,
               CASE
                 WHEN events_played < 3 THEN 'sparse_recent_events'
                 WHEN sample_size < 3 THEN 'sparse_z_score_events'
                 ELSE NULL
               END AS reason,
               CASE
                 WHEN events_played < 3 OR sample_size < 3 THEN 'steady'
                 WHEN raw_slope > 0.25 THEN 'rising'
                 WHEN raw_slope < -0.25 THEN 'falling'
                 ELSE 'steady'
               END AS key,
               CASE
                 WHEN events_played < 3 OR sample_size < 3 THEN NULL
                 WHEN raw_slope > 0.25 THEN 'overall scores trending up over recent games'
                 WHEN raw_slope < -0.25 THEN 'overall scores trending down over recent games'
                 ELSE 'overall scores holding steady over recent games'
               END AS label
        FROM sloped
        "#,
    )
    .bind(entity_id)
    .bind(sport)
    .bind(profile.season)
    .bind(entity_type)
    .fetch_one(pool)
    .await
    .with_context(|| format!("load rating trajectory {entity_type}/{entity_id}"))?;

    let events_played: i64 = row.get(0);
    let window_size: i32 = row.get(1);
    let sample_size: i32 = row.get(2);
    let slope: Option<f64> = row.get(3);
    let series: String = row.get(4);
    let reason: Option<String> = row.get(5);
    let key: String = row.get(6);
    let label: Option<String> = row.get(7);
    let recent: serde_json::Value =
        serde_json::from_str(&series).context("decode trajectory series")?;
    let components = if let Some(reason) = reason {
        serde_json::json!({
            "reason": reason,
            "events_played": events_played,
            "window_pct": 0.10,
            "window_size": window_size,
            "sample_size": sample_size,
            "source": "event_rating_z_scores",
            "metrics": ["rating"],
        })
    } else {
        serde_json::json!({
            "source": "event_rating_z_scores",
            "metrics": ["rating"],
            "events_played": events_played,
            "window_pct": 0.10,
            "window_size": window_size,
            "sample_size": sample_size,
            "rating_z_slope": slope,
            "latest_rating_z": recent.as_array().and_then(|rows| rows.first()).cloned(),
            "recent_rating_z": recent,
        })
    };
    // Sparse paths omit window_size and sample_size when no window was opened.
    let components = if events_played < 3 {
        serde_json::json!({
            "reason": "sparse_recent_events",
            "events_played": events_played,
            "window_pct": 0.10,
            "source": "event_rating_z_scores",
            "metrics": ["rating"],
        })
    } else {
        components
    };
    Ok(RatingTrajectory {
        key,
        label,
        components,
    })
}

/// Return the input hash from the entity-season's latest commentary. Take
/// the latest row regardless of nullability; a no-stats marker has a NULL input_hash → None →
/// the next run never wrongly skips against an older real commentary the marker superseded.
pub async fn last_commentary_input_hash(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: i32,
) -> Result<Option<String>> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        r#"
        SELECT input_hash FROM stat_summaries
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4
        ORDER BY generated_at DESC LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("last commentary provenance {entity_type}/{entity_id}"))?;
    Ok(row.and_then(|(hash,)| hash.filter(|h| !h.is_empty())))
}
