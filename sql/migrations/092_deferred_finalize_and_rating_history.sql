-- 092_deferred_finalize_and_rating_history.sql
--
-- Workstream A — fast historical backfill + frozen seasons + a rating ML series.
--
-- Problem: `finalize_fixture` is called once per fixture by the seeder, and its
-- tail re-derives the ENTIRE (sport, season) — percentiles, the z-rating engine,
-- the per-event starline, event percentiles — plus two CONCURRENT matview
-- refreshes. Correct and cheap in steady state (one new game a night), but
-- O(M^2) during bulk historical backfill (M fixtures each pay the whole-season
-- cost). See planning_docs/DEFERRED_PERCENTILE_BACKFILL.md (this is its design,
-- updated for the rating-engine tail added since it was drafted, plus rating_history).
--
-- This migration:
--   1. recompute_season(sport, season)        — the one-pass whole-season tail
--                                                (extracted from finalize_fixture).
--   2. finalize_fixture(fixture_id, p_recompute BOOLEAN DEFAULT TRUE)
--        p_recompute = TRUE  → today's behavior (live per-fixture freshness).
--        p_recompute = FALSE → per-fixture aggregation + mark-seeded only; the
--                              caller runs recompute_season once at the end.
--   3. rating_history                          — append-only per-entity rating
--                                                time-series (the ML "gold").
--   4. snapshot_rating_history(sport, season, trigger) — debounced insert-if-changed.
--
-- Gotcha (per the planning doc): you cannot overload one-arg finalize_fixture(INTEGER)
-- with a two-arg-default version — Postgres raises "function is not unique" on
-- one-arg calls. DROP the one-arg form first; one-arg call sites keep working via
-- the default. Engine rating columns are NEVER renamed here (late-bound PL/pgSQL).

BEGIN;

-- ---------------------------------------------------------------------------
-- 1. recompute_season — the one-pass whole-season derivation.
--    Identical to finalize_fixture's current tail (sql/shared.sql:884-908):
--    pure recompute over current table state, idempotent. Running it ONCE over a
--    complete season yields the same result as running it after every fixture.
--    NOTE: this is the FULL current tail (compute_rating/_team_rating/starlines/
--    event-pct), not the 3-step version in the 2026-05-30 planning doc — the
--    rating engine was folded into finalize_fixture after that doc was written.
--    Deliberately does NOT call recalculate_alltime_ranks (cross-season; runs on
--    the maintenance ticker / once after a full backfill).
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION recompute_season(p_sport TEXT, p_season INTEGER)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
DECLARE
    v_players INTEGER := 0;
    v_teams   INTEGER := 0;
BEGIN
    SELECT rp.players_updated, rp.teams_updated
    INTO v_players, v_teams
    FROM recalculate_percentiles(p_sport, p_season) rp;

    PERFORM recalculate_event_percentiles(p_sport, p_season);
    PERFORM compute_rating(p_sport, p_season);
    PERFORM compute_team_rating(p_sport, p_season);
    PERFORM compute_event_starline(p_sport, p_season);
    PERFORM compute_team_event_starline(p_sport, p_season);
    PERFORM recalculate_event_rating_pct(p_sport, p_season);

    IF    p_sport = 'NBA'      THEN REFRESH MATERIALIZED VIEW CONCURRENTLY nba.autofill_entities;
    ELSIF p_sport = 'NFL'      THEN REFRESH MATERIALIZED VIEW CONCURRENTLY nfl.autofill_entities;
    ELSIF p_sport = 'FOOTBALL' THEN REFRESH MATERIALIZED VIEW CONCURRENTLY football.autofill_entities;
    END IF;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;

-- ---------------------------------------------------------------------------
-- 2. finalize_fixture — per-fixture aggregation + gated whole-season recompute.
--    Step 1 (aggregation) copied verbatim from sql/shared.sql:756-881.
-- ---------------------------------------------------------------------------
DROP FUNCTION IF EXISTS finalize_fixture(INTEGER);

CREATE OR REPLACE FUNCTION finalize_fixture(
    p_fixture_id INTEGER,
    p_recompute  BOOLEAN DEFAULT TRUE
)
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

    -- Reaggregate impacted player season rows from event_box_scores (per-fixture; cheap)
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

    -- Whole-season recompute — gated. Default TRUE = today's live per-fixture
    -- freshness. Bulk historical backfill passes FALSE and runs recompute_season
    -- once at the end (O(M) instead of O(M^2)). Concluded seasons stay FROZEN
    -- until a deliberate recompute. (Cross-season all-time ranks run separately
    -- on the maintenance ticker.)
    IF p_recompute THEN
        SELECT rs.players_updated, rs.teams_updated
        INTO v_players, v_teams
        FROM recompute_season(v_sport, v_season) rs;
    END IF;

    -- Look up final score for each team from event_team_stats.
    SELECT score INTO v_home_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_home_team_id;
    SELECT score INTO v_away_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_away_team_id;

    -- Mark the fixture as seeded (always — keeps get_pending resume state correct
    -- even in deferred mode).
    PERFORM mark_fixture_seeded(p_fixture_id, v_home_score, v_away_score);

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;

-- ---------------------------------------------------------------------------
-- 3. rating_history — append-only per-entity rating snapshots (the ML series).
--    Column types mirror player_stats/team_stats (all NUMERIC; rating_modes is
--    player_stats-only, NULL for teams).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.rating_history (
    id                            BIGSERIAL PRIMARY KEY,
    entity_type                   TEXT    NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id                     INTEGER NOT NULL,
    sport                         TEXT    NOT NULL,
    season                        INTEGER NOT NULL,
    league_id                     INTEGER NOT NULL DEFAULT 0,
    rating_composite              NUMERIC,
    rating_composite_score        NUMERIC,
    rating_composite_rank         NUMERIC,
    rating_specialist             NUMERIC,
    rating_specialist_score       NUMERIC,
    rating_specialist_rank        NUMERIC,
    rating_specialty              TEXT,
    season_composite_rank_alltime NUMERIC,
    rating_modes                  JSONB,
    trigger_type                  TEXT    NOT NULL DEFAULT 'recompute'
        CHECK (trigger_type IN ('seed', 'in_season', 'season_close', 'recompute', 'manual')),
    generated_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_rating_history_entity_recent
    ON public.rating_history (entity_type, entity_id, sport, season, generated_at DESC);

COMMENT ON TABLE public.rating_history IS
    'Append-only per-entity rating time-series. One frozen row per concluded season '
    '(trigger_type seed/season_close) plus in-season trajectory rows (daily / in_season). '
    'Written by snapshot_rating_history(), debounced insert-if-changed. The queryable '
    'rating-score series for future ML. season_composite_rank_alltime may be stale/NULL in '
    'seed-time rows until the post-backfill recalculate_alltime_ranks pass.';

-- ---------------------------------------------------------------------------
-- 4. snapshot_rating_history — append a snapshot per entity for (sport, season),
--    but only when the latest existing snapshot differs (debounce → no redundant
--    identical rows). Returns the number of rows inserted.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION snapshot_rating_history(
    p_sport   TEXT,
    p_season  INTEGER,
    p_trigger TEXT DEFAULT 'recompute'
)
RETURNS INTEGER AS $$
DECLARE
    v_inserted INTEGER := 0;
    v_count    INTEGER;
BEGIN
    -- Players
    INSERT INTO public.rating_history (
        entity_type, entity_id, sport, season, league_id,
        rating_composite, rating_composite_score, rating_composite_rank,
        rating_specialist, rating_specialist_score, rating_specialist_rank, rating_specialty,
        season_composite_rank_alltime, rating_modes, trigger_type)
    SELECT 'player', ps.player_id, ps.sport, ps.season, ps.league_id,
        ps.rating_composite, ps.rating_composite_score, ps.rating_composite_rank,
        ps.rating_specialist, ps.rating_specialist_score, ps.rating_specialist_rank, ps.rating_specialty,
        ps.season_composite_rank_alltime, ps.rating_modes, p_trigger
    FROM player_stats ps
    WHERE ps.sport = p_sport AND ps.season = p_season
      AND ps.rating_composite IS NOT NULL
      AND NOT EXISTS (
          SELECT 1 FROM public.rating_history rh
          WHERE rh.entity_type = 'player' AND rh.entity_id = ps.player_id
            AND rh.sport = ps.sport AND rh.season = ps.season
            AND rh.generated_at = (
                SELECT max(rh2.generated_at) FROM public.rating_history rh2
                WHERE rh2.entity_type = 'player' AND rh2.entity_id = ps.player_id
                  AND rh2.sport = ps.sport AND rh2.season = ps.season)
            AND rh.rating_composite        IS NOT DISTINCT FROM ps.rating_composite
            AND rh.rating_composite_score  IS NOT DISTINCT FROM ps.rating_composite_score
            AND rh.rating_specialist       IS NOT DISTINCT FROM ps.rating_specialist
            AND rh.rating_specialist_score IS NOT DISTINCT FROM ps.rating_specialist_score
      );
    GET DIAGNOSTICS v_count = ROW_COUNT;
    v_inserted := v_inserted + v_count;

    -- Teams (team_stats has no rating_modes column → NULL)
    INSERT INTO public.rating_history (
        entity_type, entity_id, sport, season, league_id,
        rating_composite, rating_composite_score, rating_composite_rank,
        rating_specialist, rating_specialist_score, rating_specialist_rank, rating_specialty,
        season_composite_rank_alltime, rating_modes, trigger_type)
    SELECT 'team', ts.team_id, ts.sport, ts.season, ts.league_id,
        ts.rating_composite, ts.rating_composite_score, ts.rating_composite_rank,
        ts.rating_specialist, ts.rating_specialist_score, ts.rating_specialist_rank, ts.rating_specialty,
        ts.season_composite_rank_alltime, NULL::jsonb, p_trigger
    FROM team_stats ts
    WHERE ts.sport = p_sport AND ts.season = p_season
      AND ts.rating_composite IS NOT NULL
      AND NOT EXISTS (
          SELECT 1 FROM public.rating_history rh
          WHERE rh.entity_type = 'team' AND rh.entity_id = ts.team_id
            AND rh.sport = ts.sport AND rh.season = ts.season
            AND rh.generated_at = (
                SELECT max(rh2.generated_at) FROM public.rating_history rh2
                WHERE rh2.entity_type = 'team' AND rh2.entity_id = ts.team_id
                  AND rh2.sport = ts.sport AND rh2.season = ts.season)
            AND rh.rating_composite        IS NOT DISTINCT FROM ts.rating_composite
            AND rh.rating_composite_score  IS NOT DISTINCT FROM ts.rating_composite_score
            AND rh.rating_specialist       IS NOT DISTINCT FROM ts.rating_specialist
            AND rh.rating_specialist_score IS NOT DISTINCT FROM ts.rating_specialist_score
      );
    GET DIAGNOSTICS v_count = ROW_COUNT;
    v_inserted := v_inserted + v_count;

    RETURN v_inserted;
END;
$$ LANGUAGE plpgsql;

COMMIT;
