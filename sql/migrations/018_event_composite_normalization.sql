-- 018_event_composite_normalization.sql
--
-- Composite scores from migration 017 were averaging well under 50 in the
-- aggregate (NBA player events ~42, NFL ~36, football ~49, teams ~47).
-- The mechanism: composite is AVG of per-stat percentiles across stats
-- where the player had non-zero values. Stat presence correlates with
-- stat quality — bench players contribute to few stats AND those stats
-- are at the low end of their distributions; star players contribute to
-- many stats AND those stats sit high. So the population mean of raw
-- composites drifts below 50 even though each per-stat percentile
-- distribution averages to 50 by construction.
--
-- Fix (Option B from the discussion): add a second percent_rank pass
-- over composite_score within the same (sport, season, position)
-- partition. The stored composite becomes "percentile rank of this
-- event's raw composite among events at this position" — guarantees
-- mean = 50 per partition by construction, distribution is uniform,
-- outliers self-bound to [0, 100] without clipping. The interpretation
-- shifts from "average of stat-percentiles" to "where this event ranks
-- among events at the position" which is arguably more intuitive for
-- users anyway.
--
-- Pure SQL function change. Same signature, same call site
-- (finalize_fixture). Re-runs across all (sport, season) at the end.

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
    -- PLAYER EVENTS
    -- ============================================================
    --
    -- Same per-stat percentile pipeline as migration 017, with one
    -- added CTE (`normalized`) that percent-ranks the raw composite
    -- within each position partition. The UPDATE writes the
    -- normalized value as composite_score; the per-stat percentiles
    -- JSONB is unchanged from before.
    WITH eligible AS (
        SELECT key_name, is_inverse, unit,
               (p_sport = 'FOOTBALL' AND unit = 'cumulative_total') AS needs_per90
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
            CASE
                WHEN ek.needs_per90
                     AND COALESCE((e.stats->>'minutes_played')::numeric, e.minutes_played, 0) > 0
                THEN (e.stats->>ek.key_name)::numeric * 90.0
                     / COALESCE((e.stats->>'minutes_played')::numeric, e.minutes_played)
                ELSE (e.stats->>ek.key_name)::numeric
            END AS stat_value
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
    -- Second-level rank (migration 018): percent_rank the raw composite
    -- within the position partition so the stored value is uniform in
    -- [0, 100] with mean 50.
    normalized AS (
        SELECT event_id, position_group, pct_only, sample_size,
               ROUND((percent_rank()
                      OVER (PARTITION BY position_group ORDER BY raw_composite ASC))::numeric
                     * 100, 1) AS composite_score
        FROM per_event
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
    -- TEAM EVENTS  (no position partitioning)
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
    -- Second-level rank (migration 018): teams have no position
    -- partition, so the rank is across all team events in the
    -- (sport, season).
    normalized AS (
        SELECT event_id, pct_only, sample_size,
               ROUND((percent_rank()
                      OVER (ORDER BY raw_composite ASC))::numeric
                     * 100, 1) AS composite_score
        FROM per_event
    )
    UPDATE event_team_stats ets
        SET percentiles = nm.pct_only
                          || jsonb_build_object('_sample_size', nm.sample_size),
            composite_score = nm.composite_score
        FROM normalized nm
        WHERE ets.id = nm.event_id;
    GET DIAGNOSTICS v_team_events = ROW_COUNT;

    -- ============================================================
    -- Season composite score rollup — unchanged from migration 017.
    -- It's still AVG of event composite_scores; the meaning shifts
    -- with the underlying composite (now percentile-rank rather than
    -- avg-of-percentiles) but the rollup expression is identical.
    -- ============================================================

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

-- ---------------------------------------------------------------------------
-- One-time recompute across every (sport, season) so the stored composites
-- reflect the new normalization immediately. Same iteration pattern as
-- migration 017's backfill.
-- ---------------------------------------------------------------------------

DO $$
DECLARE
    r RECORD;
    v_p INTEGER;
    v_t INTEGER;
BEGIN
    FOR r IN
        SELECT sport, season FROM (
            SELECT DISTINCT sport, season FROM event_box_scores
            UNION
            SELECT DISTINCT sport, season FROM event_team_stats
        ) all_seasons
        WHERE sport IS NOT NULL AND season IS NOT NULL
        ORDER BY sport, season
    LOOP
        SELECT player_events_updated, team_events_updated
        INTO v_p, v_t
        FROM recalculate_event_percentiles(r.sport, r.season);
        RAISE NOTICE 'normalized %/%: % player events, % team events',
            r.sport, r.season, v_p, v_t;
    END LOOP;
END$$;

-- ---------------------------------------------------------------------------
-- Coverage notice — confirm the per-event mean is now near 50.
-- ---------------------------------------------------------------------------

DO $$
DECLARE
    r RECORD;
BEGIN
    FOR r IN
        SELECT 'event_box_scores' AS tbl, sport,
               COUNT(*) AS total,
               COUNT(*) FILTER (WHERE composite_score IS NOT NULL) AS scored,
               ROUND(AVG(composite_score)::numeric, 1) AS avg_score
        FROM event_box_scores GROUP BY sport
        UNION ALL
        SELECT 'event_team_stats', sport,
               COUNT(*), COUNT(*) FILTER (WHERE composite_score IS NOT NULL),
               ROUND(AVG(composite_score)::numeric, 1)
        FROM event_team_stats GROUP BY sport
        ORDER BY 1, 2
    LOOP
        RAISE NOTICE '% / % : % rows, % scored, avg=%',
            r.tbl, r.sport, r.total, r.scored, r.avg_score;
    END LOOP;
END$$;
