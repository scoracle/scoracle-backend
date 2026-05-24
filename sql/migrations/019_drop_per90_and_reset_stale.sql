-- 019_drop_per90_and_reset_stale.sql
--
-- Two fixes to recalculate_event_percentiles, both surfaced by the
-- 2026-05-24 Football Attacker leaderboard audit.
--
-- 1) Drop the football-player per-90 transform at event ranking.
--    The original design (migration 017) normalized cumulative_total
--    values to per-90 inside the event percentile cohort so low-minute
--    appearances would "illuminate underused players." In practice that
--    inverted the leaderboard: <15-min events averaged composite 75,
--    80+-min events averaged 33, near-perfect inverse correlation with
--    playing time. Top starters (Kane, João Pedro at 76-min average)
--    landed at 33-53 because the cohort top was filled with 5-min subs
--    whose one good stat extrapolated to elite per-90 numbers.
--
--    The fix: rank raw cumulative values like NBA/NFL do. Per-90 stays
--    relevant where it belongs — in player_stats.stats as season-rolled
--    derived keys (goals_per_90 etc.) which have the sample size to
--    mean something. Per-event composite becomes "how much did you
--    deliver in THIS game" (raw production), not "what would you have
--    delivered at 90-minute pace." The two answers are orthogonal and
--    both valuable; conflating them in one number was the bug.
--
-- 2) Reset stale composite_score / percentiles when an event no longer
--    has ranked data.
--    The previous function only UPDATEd rows that DID have ranked data.
--    If a fixture got re-seeded with empty / all-zero stats (data
--    correction, seeder bug, etc.) the row's stale composite_score from
--    an earlier state persisted indefinitely. Surfaced via Papa Dame Ba:
--    stats={}, percentiles={}, yet composite_score=99.9 from a prior run.
--    Migration 018's percent_rank pass treated absent rows / NULLs
--    inconsistently, compounding the noise. Each invocation now resets
--    stale state up front so the result reflects current data.
--
-- No schema change. Same function signature, same call site
-- (finalize_fixture). Re-runs across every (sport, season).

BEGIN;

CREATE OR REPLACE FUNCTION recalculate_event_percentiles(
    p_sport TEXT,
    p_season INTEGER
)
RETURNS TABLE (player_events_updated INTEGER, team_events_updated INTEGER) AS $$
DECLARE
    v_player_events INTEGER := 0;
    v_team_events INTEGER := 0;
BEGIN
    -- ============================================================
    -- Reset stale state for the (sport, season) up front. Touches
    -- only rows that have something to clear — composite_score IS
    -- NOT NULL filter keeps WAL volume tight. Subsequent UPDATEs
    -- in this function overwrite the rows that DO have ranked data
    -- in their current state.
    -- ============================================================
    UPDATE event_box_scores
        SET composite_score = NULL, percentiles = '{}'::jsonb
        WHERE sport = p_sport AND season = p_season
          AND composite_score IS NOT NULL;

    UPDATE event_team_stats
        SET composite_score = NULL, percentiles = '{}'::jsonb
        WHERE sport = p_sport AND season = p_season
          AND composite_score IS NOT NULL;

    -- ============================================================
    -- PLAYER EVENTS  (raw-value ranking, all sports; no per-90)
    -- ============================================================
    WITH eligible AS (
        SELECT key_name, is_inverse, unit
        FROM stat_definitions
        WHERE sport = p_sport
          AND entity_type = 'player'
          AND is_percentile_eligible = true
    ),
    expanded AS (
        SELECT
            e.id AS event_id,
            COALESCE(e.position, ps.position, 'Unknown') AS position,
            ek.key_name AS stat_key,
            ek.is_inverse,
            (e.stats->>ek.key_name)::numeric AS stat_value
        FROM event_box_scores e
        CROSS JOIN eligible ek
        LEFT JOIN player_stats ps
               ON ps.player_id = e.player_id
              AND ps.sport = e.sport
              AND ps.season = e.season
              AND COALESCE(ps.league_id, 0) = COALESCE(e.league_id, 0)
        WHERE e.sport = p_sport
          AND e.season = p_season
          AND e.stats ? ek.key_name
          AND jsonb_typeof(e.stats -> ek.key_name) = 'number'
          AND (e.stats->>ek.key_name)::numeric != 0
          AND (ek.unit <> 'rate_pct'
               OR (e.stats->>ek.key_name)::numeric BETWEEN 0 AND 100)
    ),
    ranked AS (
        SELECT event_id, position, stat_key,
            CASE WHEN is_inverse
                THEN ROUND((1.0 - percent_rank()
                             OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric
                           * 100, 1)
                ELSE ROUND((percent_rank()
                             OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric
                           * 100, 1)
            END AS percentile,
            COUNT(*) OVER (PARTITION BY position, stat_key) AS sample_size
        FROM expanded
    ),
    per_event AS (
        SELECT event_id,
            MAX(position) AS position_group,
            jsonb_object_agg(stat_key, percentile) AS pct_only,
            MAX(sample_size) AS sample_size,
            ROUND(AVG(percentile), 1) AS raw_composite
        FROM ranked
        GROUP BY event_id
    ),
    -- Migration 018 second-level rank: percent_rank the raw composite
    -- within the position partition so the stored value is uniform in
    -- [0, 100] with mean 50 per partition.
    normalized AS (
        SELECT event_id, position_group, pct_only, sample_size,
               ROUND((percent_rank()
                      OVER (PARTITION BY position_group ORDER BY raw_composite ASC))::numeric
                     * 100, 1) AS composite_score
        FROM per_event
        WHERE raw_composite IS NOT NULL  -- belt-and-suspenders; per_event GROUP BY
                                          -- already excludes events with no ranked stats
    )
    UPDATE event_box_scores ebs
        SET percentiles = nm.pct_only
                          || jsonb_build_object(
                                 '_position_group', nm.position_group,
                                 '_sample_size', nm.sample_size
                             ),
            composite_score = nm.composite_score
        FROM normalized nm
        WHERE ebs.id = nm.event_id;
    GET DIAGNOSTICS v_player_events = ROW_COUNT;

    -- ============================================================
    -- TEAM EVENTS  (raw-value ranking — unchanged from 018)
    -- ============================================================
    WITH eligible AS (
        SELECT key_name, is_inverse, unit
        FROM stat_definitions
        WHERE sport = p_sport
          AND entity_type = 'team'
          AND is_percentile_eligible = true
    ),
    expanded AS (
        SELECT
            e.id AS event_id,
            ek.key_name AS stat_key,
            ek.is_inverse,
            (e.stats->>ek.key_name)::numeric AS stat_value
        FROM event_team_stats e
        CROSS JOIN eligible ek
        WHERE e.sport = p_sport
          AND e.season = p_season
          AND e.stats ? ek.key_name
          AND jsonb_typeof(e.stats -> ek.key_name) = 'number'
          AND (e.stats->>ek.key_name)::numeric != 0
          AND (ek.unit <> 'rate_pct'
               OR (e.stats->>ek.key_name)::numeric BETWEEN 0 AND 100)
    ),
    ranked AS (
        SELECT event_id, stat_key,
            CASE WHEN is_inverse
                THEN ROUND((1.0 - percent_rank()
                             OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric
                           * 100, 1)
                ELSE ROUND((percent_rank()
                             OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric
                           * 100, 1)
            END AS percentile,
            COUNT(*) OVER (PARTITION BY stat_key) AS sample_size
        FROM expanded
    ),
    per_event AS (
        SELECT event_id,
            jsonb_object_agg(stat_key, percentile) AS pct_only,
            MAX(sample_size) AS sample_size,
            ROUND(AVG(percentile), 1) AS raw_composite
        FROM ranked
        GROUP BY event_id
    ),
    normalized AS (
        SELECT event_id, pct_only, sample_size,
               ROUND((percent_rank()
                      OVER (ORDER BY raw_composite ASC))::numeric
                     * 100, 1) AS composite_score
        FROM per_event
        WHERE raw_composite IS NOT NULL
    )
    UPDATE event_team_stats ets
        SET percentiles = nm.pct_only
                          || jsonb_build_object('_sample_size', nm.sample_size),
            composite_score = nm.composite_score
        FROM normalized nm
        WHERE ets.id = nm.event_id;
    GET DIAGNOSTICS v_team_events = ROW_COUNT;

    -- ============================================================
    -- Season composite score rollup — unchanged.
    -- ============================================================
    -- Reset season_composite_score to NULL up front so entities that
    -- now have zero scored events don't keep a stale value. Subsequent
    -- UPDATE writes the new value for entities that DO have events.
    UPDATE player_stats
        SET season_composite_score = NULL
        WHERE sport = p_sport AND season = p_season
          AND season_composite_score IS NOT NULL;

    UPDATE team_stats
        SET season_composite_score = NULL
        WHERE sport = p_sport AND season = p_season
          AND season_composite_score IS NOT NULL;

    UPDATE player_stats ps
        SET season_composite_score = sub.avg_score
        FROM (
            SELECT player_id, league_id, ROUND(AVG(composite_score), 1) AS avg_score
            FROM event_box_scores
            WHERE sport = p_sport AND season = p_season
              AND composite_score IS NOT NULL
            GROUP BY player_id, league_id
        ) sub
        WHERE ps.player_id = sub.player_id
          AND ps.sport = p_sport
          AND ps.season = p_season
          AND COALESCE(ps.league_id, 0) = COALESCE(sub.league_id, 0);

    UPDATE team_stats ts
        SET season_composite_score = sub.avg_score
        FROM (
            SELECT team_id, league_id, ROUND(AVG(composite_score), 1) AS avg_score
            FROM event_team_stats
            WHERE sport = p_sport AND season = p_season
              AND composite_score IS NOT NULL
            GROUP BY team_id, league_id
        ) sub
        WHERE ts.team_id = sub.team_id
          AND ts.sport = p_sport
          AND ts.season = p_season
          AND COALESCE(ts.league_id, 0) = COALESCE(sub.league_id, 0);

    RETURN QUERY SELECT v_player_events, v_team_events;
END;
$$ LANGUAGE plpgsql;

COMMIT;

-- One-time recompute across every (sport, season) so the stored
-- composites reflect raw-value ranking. Skipped here — run via the
-- per-season bash retry loop after migration apply (same approach
-- used for 018 to dodge live-seeder deadlocks).
