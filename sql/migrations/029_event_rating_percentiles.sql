-- 029_event_rating_percentiles.sql
--
-- Per-event 0-100 percentiles for the rating engine's Composite + Specialist,
-- so the profile "Rating" sparkline can plot Composite and Specialist as 0-100
-- lines on the same axis as the 0-100 vibe line. Mirrors the season ranks in
-- 027 (positionless percent_rank*100 over the per-event z's) and the uniform-
-- [0,100] normalization philosophy of 018.
--
-- The per-event z's themselves (rating_composite / rating_specialist, migration
-- 028) are UNCHANGED; this only adds a derived percentile column beside them.
--
-- New columns (event_box_scores, event_team_stats):
--   rating_composite_pct   NUMERIC  positionless percent_rank*100 of rating_composite (0-100)
--   rating_specialist_pct  NUMERIC  positionless percent_rank*100 of rating_specialist (0-100)
--
-- Functions:
--   recalculate_event_rating_pct(sport, season)  NEW — derives the two pct cols
--                                                 from the live z's.
--   finalize_fixture(fixture_id)                 MODIFIED — 028's body + one
--                                                 PERFORM so the pct rides the
--                                                 same per-season recompute
--                                                 cadence as the starline z's.
--
-- Cost: one extra percent_rank pass per (sport, season) on top of the starline
-- recompute already running in finalize_fixture. Backfill is a single pass over
-- the existing event rows.

BEGIN;

ALTER TABLE event_box_scores
    ADD COLUMN IF NOT EXISTS rating_composite_pct  NUMERIC,
    ADD COLUMN IF NOT EXISTS rating_specialist_pct NUMERIC;

ALTER TABLE event_team_stats
    ADD COLUMN IF NOT EXISTS rating_composite_pct  NUMERIC,
    ADD COLUMN IF NOT EXISTS rating_specialist_pct NUMERIC;

-- ---------------------------------------------------------------------------
-- recalculate_event_rating_pct — derive the 0-100 per-event percentiles from
-- the z's already written by compute_event_starline (028). Positionless, to
-- match the season ranks (compute_rating, 027): percent_rank over the whole
-- (sport, season) event population, ASC so a higher z lands a higher percentile.
-- DNP rows (z IS NULL) keep NULL pct.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION recalculate_event_rating_pct(p_sport TEXT, p_season INTEGER)
RETURNS VOID
LANGUAGE plpgsql AS $$
BEGIN
    -- Player events: clear stale pct, then percent_rank the live z's.
    UPDATE event_box_scores
       SET rating_composite_pct = NULL, rating_specialist_pct = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite_pct IS NOT NULL OR rating_specialist_pct IS NOT NULL);

    WITH ranked AS (
        SELECT id,
               ROUND((percent_rank() OVER (ORDER BY rating_composite  ASC))::numeric * 100, 1) AS cpct,
               ROUND((percent_rank() OVER (ORDER BY rating_specialist ASC))::numeric * 100, 1) AS spct
        FROM event_box_scores
        WHERE sport = p_sport AND season = p_season AND rating_composite IS NOT NULL
    )
    UPDATE event_box_scores e
       SET rating_composite_pct  = r.cpct,
           rating_specialist_pct = r.spct
    FROM ranked r WHERE e.id = r.id;

    -- Team events (same, no position dimension to begin with — already flat).
    UPDATE event_team_stats
       SET rating_composite_pct = NULL, rating_specialist_pct = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite_pct IS NOT NULL OR rating_specialist_pct IS NOT NULL);

    WITH ranked AS (
        SELECT id,
               ROUND((percent_rank() OVER (ORDER BY rating_composite  ASC))::numeric * 100, 1) AS cpct,
               ROUND((percent_rank() OVER (ORDER BY rating_specialist ASC))::numeric * 100, 1) AS spct
        FROM event_team_stats
        WHERE sport = p_sport AND season = p_season AND rating_composite IS NOT NULL
    )
    UPDATE event_team_stats e
       SET rating_composite_pct  = r.cpct,
           rating_specialist_pct = r.spct
    FROM ranked r WHERE e.id = r.id;
END;
$$;

-- ---------------------------------------------------------------------------
-- finalize_fixture — verbatim copy of migration 028's body, with a single new
-- PERFORM recalculate_event_rating_pct after the starline z recompute so the
-- per-event percentiles stay fresh in-season.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.finalize_fixture(p_fixture_id integer)
 RETURNS TABLE(players_updated integer, teams_updated integer)
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_sport TEXT;
    v_season INTEGER;
    v_league_id INTEGER;
    v_home_team_id INTEGER;
    v_away_team_id INTEGER;
    v_home_score INTEGER;
    v_away_score INTEGER;
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
BEGIN
    SELECT f.sport, f.season, COALESCE(f.league_id, 0),
           f.home_team_id, f.away_team_id
    INTO v_sport, v_season, v_league_id, v_home_team_id, v_away_team_id
    FROM fixtures f WHERE f.id = p_fixture_id;

    IF v_sport IS NULL THEN
        RAISE EXCEPTION 'fixture % not found', p_fixture_id;
    END IF;

    IF v_sport = 'NBA' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id, 'NBA', v_season, v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(nba.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            (array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE e.position IS NOT NULL))[1] AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(EXCLUDED.position, player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id, 'NBA', v_season, v_league_id,
            COALESCE(nba.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION SELECT DISTINCT home_team_id FROM fixtures WHERE id = p_fixture_id
            UNION SELECT DISTINCT away_team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats, updated_at = NOW();

    ELSIF v_sport = 'NFL' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id, 'NFL', v_season, v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(nfl.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            (array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE e.position IS NOT NULL))[1] AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(EXCLUDED.position, player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id, 'NFL', v_season, v_league_id,
            COALESCE(nfl.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION SELECT DISTINCT home_team_id FROM fixtures WHERE id = p_fixture_id
            UNION SELECT DISTINCT away_team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats, updated_at = NOW();

    ELSIF v_sport = 'FOOTBALL' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id, 'FOOTBALL', v_season, v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(football.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            (array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE e.position IS NOT NULL))[1] AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(EXCLUDED.position, player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id, 'FOOTBALL', v_season, v_league_id,
            COALESCE(football.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION SELECT DISTINCT home_team_id FROM fixtures WHERE id = p_fixture_id
            UNION SELECT DISTINCT away_team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats, updated_at = NOW();
    END IF;

    -- Season-level percentile recompute (existing).
    SELECT rp.players_updated, rp.teams_updated
    INTO v_players, v_teams
    FROM recalculate_percentiles(v_sport, v_season) rp;

    -- Event-level percentile recompute (migration 017).
    PERFORM recalculate_event_percentiles(v_sport, v_season);

    -- Rating engine recompute (migration 027 — season Composite/Specialist).
    PERFORM compute_rating(v_sport, v_season);
    PERFORM compute_team_rating(v_sport, v_season);

    -- Starline recompute (migration 028 — per-event Composite/Specialist).
    PERFORM compute_event_starline(v_sport, v_season);
    PERFORM compute_team_event_starline(v_sport, v_season);

    -- Per-event rating percentiles (migration 029 — 0-100 for the sparkline).
    PERFORM recalculate_event_rating_pct(v_sport, v_season);

    IF v_sport = 'NBA' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY nba.autofill_entities;
    ELSIF v_sport = 'NFL' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY nfl.autofill_entities;
    ELSIF v_sport = 'FOOTBALL' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY football.autofill_entities;
    END IF;

    SELECT score INTO v_home_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_home_team_id;
    SELECT score INTO v_away_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_away_team_id;

    PERFORM mark_fixture_seeded(p_fixture_id, v_home_score, v_away_score);

    RETURN QUERY SELECT v_players, v_teams;
END;
$function$;

-- ---------------------------------------------------------------------------
-- Backfill every existing (sport, season).
-- ---------------------------------------------------------------------------
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM event_box_scores ORDER BY sport, season LOOP
        PERFORM recalculate_event_rating_pct(r.sport, r.season);
    END LOOP;
END $$;

COMMIT;
