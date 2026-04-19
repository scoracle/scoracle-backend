-- Patch finalize_fixture() to populate fixtures.home_score / away_score from
-- the event_team_stats rows that were just written. Previously it called
-- mark_fixture_seeded(p_fixture_id) with no scores, so fixture-level scores
-- stayed NULL forever.
--
-- Canonical definition lives in sql/shared.sql; keep both in sync.

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
    SELECT f.sport, f.season, COALESCE(f.league_id, 0),
           f.home_team_id, f.away_team_id
    INTO v_sport, v_season, v_league_id, v_home_team_id, v_away_team_id
    FROM fixtures f WHERE f.id = p_fixture_id;

    IF v_sport IS NULL THEN
        RAISE EXCEPTION 'fixture % not found', p_fixture_id;
    END IF;

    IF v_sport = 'NBA' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, updated_at)
        SELECT
            e.player_id, 'NBA', v_season, v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(nba.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id = EXCLUDED.team_id,
            stats = EXCLUDED.stats,
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
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, updated_at)
        SELECT
            e.player_id, 'NFL', v_season, v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(nfl.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id = EXCLUDED.team_id,
            stats = EXCLUDED.stats,
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
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, updated_at)
        SELECT
            e.player_id, 'FOOTBALL', v_season, v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(football.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id = EXCLUDED.team_id,
            stats = EXCLUDED.stats,
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

    SELECT rp.players_updated, rp.teams_updated
    INTO v_players, v_teams
    FROM recalculate_percentiles(v_sport, v_season) rp;

    IF v_sport = 'NBA' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY nba.autofill_entities;
    ELSIF v_sport = 'NFL' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY nfl.autofill_entities;
    ELSIF v_sport = 'FOOTBALL' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY football.autofill_entities;
    END IF;

    -- Look up final scores from event_team_stats (newly fixed).
    SELECT score INTO v_home_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_home_team_id;
    SELECT score INTO v_away_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_away_team_id;

    PERFORM mark_fixture_seeded(p_fixture_id, v_home_score, v_away_score);

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;

-- Backfill existing 'seeded' fixtures whose home_score / away_score is NULL.
UPDATE fixtures f
SET home_score = h.score, updated_at = NOW()
FROM event_team_stats h
WHERE f.id = h.fixture_id
  AND h.team_id = f.home_team_id
  AND f.home_score IS NULL
  AND f.status = 'seeded';

UPDATE fixtures f
SET away_score = a.score, updated_at = NOW()
FROM event_team_stats a
WHERE f.id = a.fixture_id
  AND a.team_id = f.away_team_id
  AND f.away_score IS NULL
  AND f.status = 'seeded';
