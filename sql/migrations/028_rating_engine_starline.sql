-- 028_rating_engine_starline.sql
--
-- The EVENT-AS-BASE payoff of the rating engine (migration 027): per-event ratings
-- for the dual sparkline ("starline"). The SAME rating_datapoints() definition from
-- 027, applied at the event grain — one source of truth, two grains (season →
-- player_stats in 027; per-event → event_box_scores here).
--
-- Each event's datapoint values are z-scored against the per-event population (all
-- single-game values that season → "how strong was THIS game vs a typical game"),
-- then Composite (flat / NFL category-balanced, identical to the season engine) and
-- Specialist (peak z + label) per event. Feeds a dual sparkline: a breadth line
-- (Composite contribution per game) + an irreplaceable-moment line (Specialist).
--
-- New columns (event_box_scores):
--   rating_composite   NUMERIC  per-event Σz breadth (NFL: facet-balanced)
--   rating_specialist  NUMERIC  per-event peak z over the positive counting set
--   rating_specialty   TEXT     per-event argmax datapoint label
--
-- Functions:
--   compute_event_starline(sport, season)  NEW
--   finalize_fixture(fixture_id)           MODIFIED — also refreshes the starline
--                                          (same per-season cadence the event-
--                                          percentile recompute already uses).
--
-- Lifecycle / cost: finalize_fixture already runs a full per-season event recompute
-- (recalculate_event_percentiles); compute_event_starline rides the same cadence.
-- An incremental per-fixture update (z the new fixture's events against the current
-- season population) is the documented O(events-in-fixture) optimization for the
-- whole event-recompute path. Backfill (3.3s for NBA+NFL 2025 events) is cheap —
-- the LANGUAGE sql rating_datapoints() inlines at event grain.
--
-- Validated read-only against live 2025: Wembanyama's best games top his starline
-- (rim-protection nights); NFL single-game composites surface big-passing QBs.

BEGIN;

ALTER TABLE event_box_scores
    ADD COLUMN IF NOT EXISTS rating_composite  NUMERIC,
    ADD COLUMN IF NOT EXISTS rating_specialist NUMERIC,
    ADD COLUMN IF NOT EXISTS rating_specialty  TEXT;

ALTER TABLE event_team_stats
    ADD COLUMN IF NOT EXISTS rating_composite  NUMERIC,
    ADD COLUMN IF NOT EXISTS rating_specialist NUMERIC,
    ADD COLUMN IF NOT EXISTS rating_specialty  TEXT;

-- ---------------------------------------------------------------------------
-- compute_event_starline — per-event Composite/Specialist for one (sport, season).
-- Mirrors compute_rating (027) at the event grain via the shared rating_datapoints().
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION compute_event_starline(p_sport TEXT, p_season INTEGER)
RETURNS INTEGER
LANGUAGE plpgsql AS $$
DECLARE
    v_updated  INTEGER := 0;
    v_balanced BOOLEAN := (p_sport = 'NFL');
BEGIN
    UPDATE event_box_scores
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL);

    DROP TABLE IF EXISTS _starline_dp;
    CREATE TEMP TABLE _starline_dp (
        event_id BIGINT, label TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT
    ) ON COMMIT DROP;

    -- Every participated event × the shared datapoint definitions.
    INSERT INTO _starline_dp
    SELECT e.id, dp.label, dp.value, dp.in_comp, dp.in_spec, dp.sign, dp.facet
    FROM event_box_scores e
    CROSS JOIN LATERAL rating_datapoints(p_sport, e.stats) dp
    WHERE e.sport = p_sport AND e.season = p_season
      AND (e.minutes_played IS NULL OR e.minutes_played > 0);

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _starline_dp GROUP BY label
    ),
    z AS (
        SELECT d.event_id, d.label, d.in_comp, d.in_spec, d.sign, d.facet,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _starline_dp d JOIN pop p USING (label)
    ),
    comp_flat AS (
        SELECT event_id, SUM(sign * zr) AS composite
        FROM z WHERE in_comp GROUP BY event_id
    ),
    comp_facet AS (
        SELECT event_id, SUM(facet_mean) AS composite
        FROM (
            SELECT event_id, facet, AVG(sign * zr) AS facet_mean
            FROM z WHERE in_comp GROUP BY event_id, facet
        ) fm
        GROUP BY event_id
    ),
    composite AS (
        SELECT event_id, composite FROM comp_flat  WHERE NOT v_balanced
        UNION ALL
        SELECT event_id, composite FROM comp_facet WHERE     v_balanced
    ),
    spec AS (
        SELECT DISTINCT ON (event_id)
               event_id, zr AS specialist, label AS specialty
        FROM z WHERE in_spec
        ORDER BY event_id, zr DESC
    )
    UPDATE event_box_scores e SET
        rating_composite  = ROUND(c.composite,  4),
        rating_specialist = ROUND(s.specialist, 4),
        rating_specialty  = s.specialty
    FROM composite c
    JOIN spec s USING (event_id)
    WHERE e.id = c.event_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    RETURN v_updated;
END;
$$;

-- ---------------------------------------------------------------------------
-- compute_team_event_starline — per-event TEAM Composite/Specialist (flat) from
-- event_team_stats, via the shared rating_datapoints_team() (027).
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION compute_team_event_starline(p_sport TEXT, p_season INTEGER)
RETURNS INTEGER
LANGUAGE plpgsql AS $$
DECLARE
    v_updated INTEGER := 0;
BEGIN
    UPDATE event_team_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL);

    DROP TABLE IF EXISTS _team_starline_dp;
    CREATE TEMP TABLE _team_starline_dp (
        event_id BIGINT, label TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER
    ) ON COMMIT DROP;

    INSERT INTO _team_starline_dp
    SELECT e.id, dp.label, dp.value, dp.in_comp, dp.in_spec, dp.sign
    FROM event_team_stats e
    CROSS JOIN LATERAL rating_datapoints_team(p_sport, e.stats) dp
    WHERE e.sport = p_sport AND e.season = p_season AND e.stats <> '{}'::jsonb;

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_starline_dp GROUP BY label
    ),
    z AS (
        SELECT d.event_id, d.in_comp, d.in_spec, d.sign, d.label,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_starline_dp d JOIN pop p USING (label)
    ),
    composite AS (
        SELECT event_id, SUM(sign * zr) AS composite FROM z WHERE in_comp GROUP BY event_id
    ),
    spec AS (
        SELECT DISTINCT ON (event_id) event_id, zr AS specialist, label AS specialty
        FROM z WHERE in_spec ORDER BY event_id, zr DESC
    )
    UPDATE event_team_stats e SET
        rating_composite  = ROUND(c.composite,  4),
        rating_specialist = ROUND(s.specialist, 4),
        rating_specialty  = s.specialty
    FROM composite c JOIN spec s USING (event_id)
    WHERE e.id = c.event_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    RETURN v_updated;
END;
$$;

-- ---------------------------------------------------------------------------
-- finalize_fixture — 027's body plus a PERFORM compute_event_starline so the
-- per-event sparkline stays fresh in-season (rides the existing event recompute).
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
-- Backfill every existing (sport, season) starline.
-- ---------------------------------------------------------------------------
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM event_box_scores ORDER BY sport, season LOOP
        PERFORM compute_event_starline(r.sport, r.season);
        PERFORM compute_team_event_starline(r.sport, r.season);
    END LOOP;
END $$;

COMMIT;
