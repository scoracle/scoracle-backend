-- 017_event_percentiles_and_composite.sql
--
-- Phase 1 of the per-event percentile + composite score work
-- (proposal: progress_docs/2026-05-23_event-percentiles-and-composite-score-proposal.md).
--
-- For every event row we compute a percentile per eligible stat key
-- against the same-season position-partitioned cohort, then a composite
-- score = unweighted mean of those percentiles. Both land back on the
-- event row alongside the raw stats. Hooks into finalize_fixture() so
-- new fixtures get scored automatically; a one-time backfill at the
-- end of this migration scores everything that already exists.
--
-- The season-rolled tables (player_stats / team_stats) also gain a
-- season_composite_score numeric column populated as AVG of the
-- entity's event composite scores — gives the profile endpoint a
-- single-number headline rating without a per-request aggregation.

BEGIN;

-- ---------------------------------------------------------------------------
-- 1. Schema additions
-- ---------------------------------------------------------------------------

ALTER TABLE event_box_scores
    ADD COLUMN IF NOT EXISTS percentiles JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS composite_score NUMERIC;

ALTER TABLE event_team_stats
    ADD COLUMN IF NOT EXISTS percentiles JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS composite_score NUMERIC;

ALTER TABLE player_stats
    ADD COLUMN IF NOT EXISTS season_composite_score NUMERIC;

ALTER TABLE team_stats
    ADD COLUMN IF NOT EXISTS season_composite_score NUMERIC;

-- Trends will read "last N composite scores for entity X" — these
-- indexes match that access pattern.
CREATE INDEX IF NOT EXISTS idx_event_box_scores_player_composite
    ON event_box_scores(player_id, sport, season)
    INCLUDE (composite_score, minutes_played);
CREATE INDEX IF NOT EXISTS idx_event_team_stats_team_composite
    ON event_team_stats(team_id, sport, season)
    INCLUDE (composite_score);

-- ---------------------------------------------------------------------------
-- 2. Backfill is_percentile_eligible
--
-- This flag has existed in stat_definitions since migration 008 but no
-- function reads it. Activating it now. The rule:
--   eligible if the stat represents per-event production worth ranking
--   on its own. For per-event ranking, even player cumulative_total keys
--   work (each event is one game, so single-game raw counts compare
--   cleanly within a position cohort). What we exclude is "playing time"
--   keys (games_played, minutes, etc.) — being in the game isn't a
--   ranked production stat — and the standings-style 'special' keys.
-- ---------------------------------------------------------------------------

UPDATE stat_definitions SET is_percentile_eligible = (
    unit IN ('per_game_avg', 'rate_pct', 'cumulative_total')
    AND key_name NOT IN (
        'games_played', 'matches_played', 'lineups',
        'minutes_played', 'minutes', 'gp',
        'cumulative_minutes_played',
        'captain', 'substitutions',
        'rating'  -- already a composite; would double-count if included
    )
);

-- ---------------------------------------------------------------------------
-- 3. recalculate_event_percentiles()
--
-- Mirrors recalculate_percentiles() (sql/shared.sql:803) but reads from
-- event_box_scores / event_team_stats instead of the season tables.
-- Per-stat percent_rank() partitioned by (position, stat_key) for
-- players, (stat_key) for teams. Inverse handling via 1 − percent_rank
-- for is_inverse stats so high percentile always = good.
--
-- Football player cumulative_total values get per-90 normalized BEFORE
-- ranking (each event has variable minutes_played; ranking raw counts
-- would penalize subs who played fewer minutes per the same proposal
-- decision that drives the trends entity-recent CTE).
--
-- composite_score is AVG(percentile) across all eligible stats for that
-- event row — unweighted, completely data-driven per the proposal.
--
-- After event rows are scored, season_composite_score on the season
-- tables is updated from AVG of the entity's event scores.
-- ---------------------------------------------------------------------------

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
    WITH eligible AS (
        SELECT key_name, is_inverse, unit,
               -- Football players need per-90 normalization for cumulative
               -- totals; NBA / NFL events are already per-game (1 event = 1
               -- game), and rate_pct / per_game_avg are directly comparable.
               (p_sport = 'FOOTBALL' AND unit = 'cumulative_total') AS needs_per90
        FROM stat_definitions
        WHERE sport = p_sport
          AND entity_type = 'player'
          AND is_percentile_eligible = true
    ),
    expanded AS (
        SELECT
            e.id AS event_id,
            -- Prefer the event row's position; fall back to player_stats;
            -- last resort 'Unknown'. Most NBA/football events have their
            -- own position; NFL events typically don't and rely on the
            -- season row.
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
          -- rate_pct sanity guard: SportMonks emits some per-fixture rate
          -- keys as non-normalized aggregates (values > 100). Drop those
          -- from ranking so we don't pin every event to 99th percentile.
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
            ROUND(AVG(percentile), 1) AS composite_score
        FROM ranked
        GROUP BY event_id
    )
    UPDATE event_box_scores ebs
        SET percentiles = pe.pct_only
                          || jsonb_build_object(
                                 '_position_group', pe.position_group,
                                 '_sample_size', pe.sample_size
                             ),
            composite_score = pe.composite_score
        FROM per_event pe
        WHERE ebs.id = pe.event_id;
    GET DIAGNOSTICS v_player_events = ROW_COUNT;

    -- ============================================================
    -- TEAM EVENTS  (no position partitioning; teams are teams)
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
            ROUND(AVG(percentile), 1) AS composite_score
        FROM ranked
        GROUP BY event_id
    )
    UPDATE event_team_stats ets
        SET percentiles = pe.pct_only
                          || jsonb_build_object('_sample_size', pe.sample_size),
            composite_score = pe.composite_score
        FROM per_event pe
        WHERE ets.id = pe.event_id;
    GET DIAGNOSTICS v_team_events = ROW_COUNT;

    -- ============================================================
    -- Season composite score rollup
    -- ============================================================
    -- AVG over event composite scores. NULL events (e.g. no eligible
    -- non-zero stats for that row) drop out of the average naturally.
    -- Bounded to the same (sport, season) the events were ranked in.

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

-- ---------------------------------------------------------------------------
-- 4. Hook into finalize_fixture()
--
-- Inserts one PERFORM call right after recalculate_percentiles, before
-- the materialized view refresh. The body is otherwise unchanged from
-- the version at sql/shared.sql:630.
-- ---------------------------------------------------------------------------

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

    -- Event-level percentile recompute (NEW — migration 017).
    -- Re-ranks all events for the (sport, season), then rolls
    -- season_composite_score back onto player_stats / team_stats.
    PERFORM recalculate_event_percentiles(v_sport, v_season);

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
$$ LANGUAGE plpgsql;

COMMIT;

-- ---------------------------------------------------------------------------
-- 5. One-time backfill — outside the transaction so each (sport, season)
--    runs in its own short-ish transaction. The function itself wraps
--    each call in implicit transaction context; we just iterate.
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
        RAISE NOTICE 'backfill %/%: % player events, % team events',
            r.sport, r.season, v_p, v_t;
    END LOOP;
END$$;

-- ---------------------------------------------------------------------------
-- 6. Coverage notice
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
