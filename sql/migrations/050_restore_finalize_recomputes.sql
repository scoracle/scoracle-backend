-- ============================================================================
-- 050_restore_finalize_recomputes.sql
-- RESTORE the in-season derived recomputes that migration 049 inadvertently dropped.
--
-- Regression: 049 rebuilt finalize_fixture from canonical shared.sql, which had drifted —
-- it never tracked the recompute tail added by migrations 017/027/028/029. So 049 silently
-- removed six PERFORMs from prod's finalize_fixture, leaving only recalculate_percentiles.
-- Effect: during a live season, seeding a fixture refreshed season percentiles but NOT the
-- z-rating engine (compute_rating/compute_team_rating), the per-event starline, the event
-- percentiles, or the per-event rating percentiles — i.e. ratings froze mid-season.
--
-- This restores the full tail (all scoped to v_season — prior seasons stay frozen) while
-- keeping 049's position-durability fix. Canonical shared.sql now carries the complete
-- definition, ending the drift that caused this. DDL only (no data change); finalize_fixture
-- REFRESHes matviews CONCURRENTLY so it is not invoked here.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/050_restore_finalize_recomputes.sql
-- ============================================================================

BEGIN;

CREATE OR REPLACE FUNCTION finalize_fixture(p_fixture_id INTEGER)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
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
    -- Look up fixture details
    SELECT f.sport, f.season, COALESCE(f.league_id, 0),
           f.home_team_id, f.away_team_id
    INTO v_sport, v_season, v_league_id, v_home_team_id, v_away_team_id
    FROM fixtures f WHERE f.id = p_fixture_id;

    IF v_sport IS NULL THEN
        RAISE EXCEPTION 'fixture % not found', p_fixture_id;
    END IF;

    -- Reaggregate impacted player season rows from event_box_scores
    IF v_sport = 'NBA' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id,
            'NBA',
            v_season,
            v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(nba.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            COALESCE(
                NULLIF((array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE NULLIF(e.position, '') IS NOT NULL))[1], ''),
                (SELECT NULLIF(pl.meta->>'position_abbreviation', '') FROM players pl WHERE pl.id = e.player_id AND pl.sport = v_sport)
            ) AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(NULLIF(EXCLUDED.position, ''), player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id,
            'NBA',
            v_season,
            v_league_id,
            COALESCE(nba.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION
            SELECT DISTINCT home_team_id AS team_id FROM fixtures WHERE id = p_fixture_id
            UNION
            SELECT DISTINCT away_team_id AS team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats,
            updated_at = NOW();

    ELSIF v_sport = 'NFL' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id,
            'NFL',
            v_season,
            v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(nfl.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            COALESCE(
                NULLIF((array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE NULLIF(e.position, '') IS NOT NULL))[1], ''),
                (SELECT NULLIF(pl.meta->>'position_abbreviation', '') FROM players pl WHERE pl.id = e.player_id AND pl.sport = v_sport)
            ) AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(NULLIF(EXCLUDED.position, ''), player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id,
            'NFL',
            v_season,
            v_league_id,
            COALESCE(nfl.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION
            SELECT DISTINCT home_team_id AS team_id FROM fixtures WHERE id = p_fixture_id
            UNION
            SELECT DISTINCT away_team_id AS team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats,
            updated_at = NOW();

    ELSIF v_sport = 'FOOTBALL' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id,
            'FOOTBALL',
            v_season,
            v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(football.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            COALESCE(
                NULLIF((array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE NULLIF(e.position, '') IS NOT NULL))[1], ''),
                (SELECT NULLIF(pl.meta->>'position_abbreviation', '') FROM players pl WHERE pl.id = e.player_id AND pl.sport = v_sport)
            ) AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(NULLIF(EXCLUDED.position, ''), player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id,
            'FOOTBALL',
            v_season,
            v_league_id,
            COALESCE(football.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION
            SELECT DISTINCT home_team_id AS team_id FROM fixtures WHERE id = p_fixture_id
            UNION
            SELECT DISTINCT away_team_id AS team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats,
            updated_at = NOW();
    END IF;

    -- Recalculate percentiles for the sport/season
    SELECT rp.players_updated, rp.teams_updated
    INTO v_players, v_teams
    FROM recalculate_percentiles(v_sport, v_season) rp;

    -- Derived recomputes — keep the LIVE season fresh on every seed (event
    -- percentiles, the season z-rating engine for players + teams, the per-event
    -- starline, and per-event rating percentiles). All are scoped to v_season, so
    -- prior (completed) seasons are untouched here and stay FROZEN until a deliberate
    -- recompute. (Lineage: migrations 017/027/028/029; restored in migration 050 after
    -- 049 inadvertently dropped them by rebuilding finalize_fixture from a stale shared.sql.)
    PERFORM recalculate_event_percentiles(v_sport, v_season);
    PERFORM compute_rating(v_sport, v_season);
    PERFORM compute_team_rating(v_sport, v_season);
    PERFORM compute_event_starline(v_sport, v_season);
    PERFORM compute_team_event_starline(v_sport, v_season);
    PERFORM recalculate_event_rating_pct(v_sport, v_season);

    -- Refresh per-sport materialized views used by autofill/search
    IF v_sport = 'NBA' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY nba.autofill_entities;
    ELSIF v_sport = 'NFL' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY nfl.autofill_entities;
    ELSIF v_sport = 'FOOTBALL' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY football.autofill_entities;
    END IF;

    -- Look up final score for each team from event_team_stats.
    SELECT score INTO v_home_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_home_team_id;
    SELECT score INTO v_away_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_away_team_id;

    -- Mark the fixture as seeded (with scores if we found them)
    PERFORM mark_fixture_seeded(p_fixture_id, v_home_score, v_away_score);

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;

COMMIT;
