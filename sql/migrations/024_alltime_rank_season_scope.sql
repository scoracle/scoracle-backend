-- 024_alltime_rank_season_scope.sql
--
-- Refines recalculate_alltime_ranks to honor "previous seasons read-only,
-- current season dynamic." Adds an optional p_season scope:
--
--   recalculate_alltime_ranks(sport, season)  → writes ONLY that season's
--       all-time ranks. The percent_rank window still reads ALL seasons
--       (the full frozen history is the comparison pool), but only the
--       given season's rows are written. Used nightly for the current
--       season — previous seasons are never touched in-season.
--
--   recalculate_alltime_ranks(sport, NULL)     → writes ALL seasons. The
--       full re-baseline. Run on startup (post-deploy consistency) and at
--       season rollover (to fold the just-completed season into the
--       permanent history and re-rank everything against the larger pool).
--
-- This keeps the in-season write footprint tiny (~current-season rows) and
-- makes the "history is a frozen reference" invariant explicit.

BEGIN;

-- The 1-arg version from migration 023 would be ambiguous against the new
-- 2-arg-with-default signature, so drop it first.
DROP FUNCTION IF EXISTS recalculate_alltime_ranks(TEXT);

CREATE OR REPLACE FUNCTION recalculate_alltime_ranks(
    p_sport TEXT,
    p_season INTEGER DEFAULT NULL
)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
DECLARE
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
BEGIN
    -- Null out rows that lost their composite (scoped to p_season if given).
    UPDATE player_stats SET season_composite_rank_alltime = NULL
        WHERE sport = p_sport AND season_composite_score IS NULL
          AND season_composite_rank_alltime IS NOT NULL
          AND (p_season IS NULL OR season = p_season);
    UPDATE team_stats SET season_composite_rank_alltime = NULL
        WHERE sport = p_sport AND season_composite_score IS NULL
          AND season_composite_rank_alltime IS NOT NULL
          AND (p_season IS NULL OR season = p_season);

    -- Window reads ALL seasons for the percentile (full history is the
    -- comparison pool); the outer WHERE limits writes to p_season when given.
    UPDATE player_stats ps SET season_composite_rank_alltime = r.rnk
        FROM (
            SELECT player_id, season, league_id,
                   ROUND((percent_rank() OVER (
                       PARTITION BY COALESCE(position, 'Unknown')
                       ORDER BY season_composite_score ASC
                   ))::numeric * 100, 1) AS rnk
            FROM player_stats
            WHERE sport = p_sport AND season_composite_score IS NOT NULL
        ) r
        WHERE ps.player_id = r.player_id AND ps.sport = p_sport AND ps.season = r.season
          AND COALESCE(ps.league_id, 0) = COALESCE(r.league_id, 0)
          AND (p_season IS NULL OR ps.season = p_season);
    GET DIAGNOSTICS v_players = ROW_COUNT;

    UPDATE team_stats ts SET season_composite_rank_alltime = r.rnk
        FROM (
            SELECT team_id, season, league_id,
                   ROUND((percent_rank() OVER (ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM team_stats
            WHERE sport = p_sport AND season_composite_score IS NOT NULL
        ) r
        WHERE ts.team_id = r.team_id AND ts.sport = p_sport AND ts.season = r.season
          AND COALESCE(ts.league_id, 0) = COALESCE(r.league_id, 0)
          AND (p_season IS NULL OR ts.season = p_season);
    GET DIAGNOSTICS v_teams = ROW_COUNT;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;

COMMIT;
