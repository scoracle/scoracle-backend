-- 020_composite_purity_and_outcomes.sql
--
-- Composite score becomes "AVG of season per-stat percentile ranks" instead
-- of "AVG of per-event composite scores." Two paired stat-definition flips
-- support the new semantics:
--
--   A) Outcome stats (wins, points, goal_difference, ...) flip into
--      percentile-eligible for teams. They're objective box-score data;
--      including them is honest, not weighting.
--
--   B) Per-X derived stats (passing_yards_per_game, tackles_per_90,
--      pts_per_36, ...) flip OUT of percentile-eligibility for composite
--      purposes. They express the same underlying production as their raw
--      counterparts in a different unit; including both would silently
--      weight that performance 2x. They stay in the percentiles JSONB and
--      remain usable by other consumers (trends per-stat comparisons,
--      profile rate displays). Just not in the composite AVG.
--
--   C) recalculate_event_percentiles function body — replace the season-
--      rollup step. Was: AVG of event composites. Now: AVG of season-level
--      per-stat percentile values, filtered to is_percentile_eligible
--      keys. Event-level composite_score (driving entity_event_scores
--      sparkline) stays unchanged.
--
-- Driving motivation:
--   - Move composite to be cross-season comparable (same partition each
--     year; year-end-frozen for prior seasons via finalize_fixture's
--     per-season scope).
--   - Surface outcome quality (Arsenal won the PL with 82 pts / +43 GD;
--     should rank ahead of mid-table Chelsea with 52 / +7).
--   - Pure data-driven: every eligible data point gets one vote in the
--     AVG. No editorial weighting, no curation by importance.

BEGIN;

-- ===========================================================================
-- Part A — outcome stats become percentile-eligible (team-only)
-- ===========================================================================

UPDATE stat_definitions
   SET is_percentile_eligible = true
 WHERE entity_type = 'team'
   AND key_name IN (
     'wins', 'losses', 'draws', 'points', 'overall_points',
     'goal_difference', 'points_for', 'points_against',
     'point_differential', 'win_pct'
   );

-- Inverse flag for "lower is better" outcome stats so recalculate_percentiles'
-- existing inversion (1 - percent_rank) produces high percentile when value
-- is low.
UPDATE stat_definitions
   SET is_inverse = true
 WHERE entity_type = 'team'
   AND key_name IN ('losses', 'points_against', 'goals_against');

-- ===========================================================================
-- Part B — per-X derived stats become NOT eligible for composite
-- ===========================================================================
--
-- The regex matches the same pattern used by migration 016 when classifying
-- per_game_avg: any derived stat suffixed with _per_(game|36|90|...) is a
-- normalization of a raw counterpart we're already counting. Excluding them
-- from the composite avoids double-counting that underlying production.

UPDATE stat_definitions
   SET is_percentile_eligible = false
 WHERE is_derived = true
   AND key_name ~ '_per_(game|36|90|minute|attempt|carry|reception|target|completion)$';

-- ===========================================================================
-- Part C — recalculate_event_percentiles: season-composite rollup swap
-- ===========================================================================
--
-- Replace the AVG-of-event-composites rollup at the bottom of the function
-- with AVG of season-level percentile values from the percentiles JSONB
-- (written by recalculate_percentiles, which runs earlier in
-- finalize_fixture). Filter to is_percentile_eligible=true keys and skip
-- metadata keys (`_position_group`, `_sample_size`, etc.).
--
-- The event-level percentile + composite computation (player events + team
-- events sections) stays identical to migration 019 — those drive the
-- sparkline-source entity_event_scores. Only the season rollup changes.

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
    -- Reset stale state for the (sport, season) up front.
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
    -- PLAYER EVENTS  (raw-value ranking; per-stat percentile;
    --                 normalize via second-pass percent_rank;
    --                 sparkline source — unchanged from migration 019)
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
    normalized AS (
        SELECT event_id, position_group, pct_only, sample_size,
               ROUND((percent_rank()
                      OVER (PARTITION BY position_group ORDER BY raw_composite ASC))::numeric
                     * 100, 1) AS composite_score
        FROM per_event
        WHERE raw_composite IS NOT NULL
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
    -- TEAM EVENTS  (sparkline source — unchanged from migration 019)
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
    -- Season composite — NEW source: AVG of season-level per-stat
    -- percentile values from the percentiles JSONB.
    --
    -- recalculate_percentiles() ran earlier in finalize_fixture and
    -- wrote percentiles for every numeric stat in the season blob.
    -- We pull those values, drop metadata keys (_position_group,
    -- _sample_size), join stat_definitions to keep only the keys
    -- where is_percentile_eligible=true after the Parts A + B flips
    -- above, and average. One eligible data point = one vote.
    -- ============================================================

    -- Reset season_composite_score so entities that no longer have
    -- eligible data drop to NULL instead of keeping stale values.
    UPDATE player_stats
        SET season_composite_score = NULL
        WHERE sport = p_sport AND season = p_season
          AND season_composite_score IS NOT NULL;

    UPDATE team_stats
        SET season_composite_score = NULL
        WHERE sport = p_sport AND season = p_season
          AND season_composite_score IS NOT NULL;

    UPDATE player_stats ps
        SET season_composite_score = sub.avg_pct
        FROM (
            SELECT ps2.player_id, ps2.league_id,
                   ROUND(AVG((p.value)::numeric)::numeric, 1) AS avg_pct
            FROM player_stats ps2
            CROSS JOIN LATERAL jsonb_each(ps2.percentiles) AS p(key, value)
            JOIN stat_definitions sd
              ON sd.sport = ps2.sport
             AND sd.entity_type = 'player'
             AND sd.key_name = p.key
            WHERE ps2.sport = p_sport
              AND ps2.season = p_season
              AND ps2.percentiles IS NOT NULL
              AND ps2.percentiles <> '{}'::jsonb
              AND jsonb_typeof(p.value) = 'number'
              AND p.key NOT LIKE '\_%'
              AND sd.is_percentile_eligible = true
            GROUP BY ps2.player_id, ps2.league_id
            HAVING COUNT(*) > 0
        ) sub
        WHERE ps.player_id = sub.player_id
          AND ps.sport = p_sport
          AND ps.season = p_season
          AND COALESCE(ps.league_id, 0) = COALESCE(sub.league_id, 0);

    UPDATE team_stats ts
        SET season_composite_score = sub.avg_pct
        FROM (
            SELECT ts2.team_id, ts2.league_id,
                   ROUND(AVG((p.value)::numeric)::numeric, 1) AS avg_pct
            FROM team_stats ts2
            CROSS JOIN LATERAL jsonb_each(ts2.percentiles) AS p(key, value)
            JOIN stat_definitions sd
              ON sd.sport = ts2.sport
             AND sd.entity_type = 'team'
             AND sd.key_name = p.key
            WHERE ts2.sport = p_sport
              AND ts2.season = p_season
              AND ts2.percentiles IS NOT NULL
              AND ts2.percentiles <> '{}'::jsonb
              AND jsonb_typeof(p.value) = 'number'
              AND p.key NOT LIKE '\_%'
              AND sd.is_percentile_eligible = true
            GROUP BY ts2.team_id, ts2.league_id
            HAVING COUNT(*) > 0
        ) sub
        WHERE ts.team_id = sub.team_id
          AND ts.sport = p_sport
          AND ts.season = p_season
          AND COALESCE(ts.league_id, 0) = COALESCE(sub.league_id, 0);

    RETURN QUERY SELECT v_player_events, v_team_events;
END;
$$ LANGUAGE plpgsql;

COMMIT;

-- ===========================================================================
-- Coverage notice — eligibility counts after Parts A + B
-- ===========================================================================
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN
        SELECT sport, entity_type,
               COUNT(*) AS total,
               COUNT(*) FILTER (WHERE is_percentile_eligible) AS eligible
        FROM stat_definitions GROUP BY 1,2 ORDER BY 1,2
    LOOP
        RAISE NOTICE '% / % : %/% eligible', r.sport, r.entity_type, r.eligible, r.total;
    END LOOP;
END$$;
