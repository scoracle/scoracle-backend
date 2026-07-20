-- 156_stat_matchups.sql
--
-- The stats rail joins the relational layer (Plan - Narrative Graph; "presented, not
-- gatekept" doctrine, 2026-07-19). stat_matchups holds pairwise performance edges —
-- "player A vs team B on stat K" — derived set-based from event_box_scores + fixtures.
-- No LLM anywhere: this is the computed sibling of the news graph's extracted edges,
-- sharing the endpoint conventions (entity_type/entity_id/sport) so one neighborhood
-- query joins narrative history and statistical history for the same pair.
--
-- CORE DOCTRINE — presented, not gatekept: NO minimum-sample existence gate. An n=1
-- monster game gets a row; what makes it safe to serve is the RELIABILITY score
-- (0-100) delivered alongside, so the model decides inclusion AND framing ("he's only
-- played them once — grain of salt"). Magnitude and trust are deliberately orthogonal
-- columns: raw_delta/adj_delta/shrunk_delta carry size, reliability carries belief.
--
--   reliability = round(100 * shrink * recency_factor * consistency_factor)
--     shrink             = n / (n + k)          -- the empirical-Bayes weight itself
--     recency_factor     = 0.5 + 0.5 * recent_share   (meetings in the 2 newest seasons)
--     consistency_factor = 0.5 + 0.5 * dir_share      (meetings on the delta's side of expected)
--
--   expected = baseline_avg * opponent_allow_ratio    -- opponent-adjusted: if team B
--   concedes boards to EVERYONE, that is team B's property, not player A's edge.
--   adj_delta = matchup_avg - expected; shrunk_delta = adj_delta * shrink.
--
-- Count-stat semantics: an absent key is a real ZERO (providers omit zero counts —
-- only 30.8k of 518k FOOTBALL rows carry 'goals'), gated on minutes_played so DNPs and
-- cameos don't dilute baselines. v1 is per-game averages over count stats; per-90
-- normalization and position-aware opponent adjustment are noted refinements.
--
-- Refresh is DELETE + INSERT per (sport, scope, stat_key) in one call — fully derived,
-- fully recomputable, idempotent. Stats only move on game days; cron nightly at 03:30
-- (cron-stat-matchups.sh), offseason runs are cheap no-ops in effect.
--
-- Deploy order: ADDITIVE — nothing reads the table yet; apply any time.

BEGIN;

CREATE TABLE IF NOT EXISTS public.stat_matchups (
    sport           TEXT NOT NULL REFERENCES public.sports(id),
    scope           TEXT NOT NULL DEFAULT 'career',
    stat_key        TEXT NOT NULL,
    subject_type    TEXT NOT NULL,
    subject_id      INTEGER NOT NULL,
    object_type     TEXT NOT NULL,
    object_id       INTEGER NOT NULL,
    n_games         INTEGER NOT NULL,
    baseline_avg    NUMERIC(8,3) NOT NULL,
    matchup_avg     NUMERIC(8,3) NOT NULL,
    expected_avg    NUMERIC(8,3) NOT NULL,
    raw_delta       NUMERIC(8,3) NOT NULL,
    adj_delta       NUMERIC(8,3) NOT NULL,
    shrunk_delta    NUMERIC(8,3) NOT NULL,
    reliability     SMALLINT NOT NULL,
    components      JSONB NOT NULL DEFAULT '{}'::jsonb,
    first_season    INTEGER NOT NULL,
    last_season     INTEGER NOT NULL,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (sport, scope, stat_key, subject_type, subject_id, object_type, object_id),
    CONSTRAINT stat_matchups_scope_check CHECK (scope IN ('career', 'season')),
    CONSTRAINT stat_matchups_subject_type_check CHECK (subject_type IN ('player', 'team')),
    CONSTRAINT stat_matchups_object_type_check CHECK (object_type IN ('player', 'team')),
    CONSTRAINT stat_matchups_no_self_loop_check CHECK (
        NOT (subject_type = object_type AND subject_id = object_id)
    ),
    CONSTRAINT stat_matchups_n_games_check CHECK (n_games >= 1),
    CONSTRAINT stat_matchups_reliability_check CHECK (reliability BETWEEN 0 AND 100)
);

CREATE INDEX IF NOT EXISTS idx_stat_matchups_object
    ON public.stat_matchups (sport, object_type, object_id, stat_key);
-- Retrieval ranking: the context assembler budgets top-K by |shrunk effect|, each row
-- carried WITH its reliability — the gate is context allocation, never existence.
CREATE INDEX IF NOT EXISTS idx_stat_matchups_effect
    ON public.stat_matchups (sport, stat_key, (abs(shrunk_delta)) DESC);

COMMENT ON TABLE public.stat_matchups IS
    'Pairwise performance edges (player-vs-team per stat), the stats rail''s computed '
    'counterpart to narrative_links. Presented, not gatekept: no minimum-sample row '
    'filter — reliability (shrinkage weight x recency x consistency, 0-100) travels '
    'with every row so the model frames weak samples ("only faced them once") instead '
    'of a threshold silently hiding them. Effect size (deltas) and trust (reliability) '
    'are orthogonal on purpose: huge-effect/low-reliability is the grain-of-salt case.';
COMMENT ON COLUMN public.stat_matchups.adj_delta IS
    'matchup_avg minus (baseline_avg x opponent allow-ratio): the opponent-adjusted '
    'edge, so a stat the opponent concedes to everyone is not credited to the player.';
COMMENT ON COLUMN public.stat_matchups.reliability IS
    'How much to believe adj_delta, 0-100: n/(n+k) shrinkage weight discounted by '
    'recency (meetings in the 2 newest seasons) and direction-consistency across '
    'meetings. Breakdown in components. Serve it WITH the fact; never filter by it.';

CREATE OR REPLACE FUNCTION public.refresh_stat_matchups(
    p_sport text,
    p_stat_key text,
    p_scope text DEFAULT 'career',
    p_k numeric DEFAULT 5,
    p_min_minutes numeric DEFAULT 10,
    p_min_baseline_games integer DEFAULT 10,
    OUT rows_written integer,
    OUT subjects_covered integer
) RETURNS record
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_run timestamptz := clock_timestamp();
BEGIN
    IF p_scope <> 'career' THEN
        RAISE EXCEPTION 'refresh_stat_matchups: only career scope is implemented (got %)', p_scope;
    END IF;

    DELETE FROM stat_matchups
    WHERE sport = p_sport AND scope = p_scope AND stat_key = p_stat_key
      AND subject_type = 'player' AND object_type = 'team';

    WITH per_game AS (
        -- One row per qualifying appearance. Absent count-stat key = real zero;
        -- minutes floor keeps DNPs/cameos out of baselines. Malformed values -> 0.
        SELECT b.player_id,
               b.season,
               CASE WHEN b.team_id = f.home_team_id THEN f.away_team_id
                    ELSE f.home_team_id END AS opponent_id,
               CASE WHEN b.stats->>p_stat_key ~ '^-?[0-9]+\.?[0-9]*$'
                    THEN (b.stats->>p_stat_key)::numeric
                    ELSE 0 END AS v
        FROM event_box_scores b
        JOIN fixtures f ON f.id = b.fixture_id AND f.sport = b.sport
        WHERE b.sport = p_sport
          AND b.minutes_played IS NOT NULL
          AND b.minutes_played >= p_min_minutes
    ),
    bounds AS (
        SELECT max(season) AS max_season, avg(v) AS league_avg FROM per_game
    ),
    opp_allow AS (
        -- What each team concedes on this stat, relative to league average.
        SELECT g.opponent_id,
               avg(g.v) / NULLIF((SELECT league_avg FROM bounds), 0) AS allow_ratio
        FROM per_game g
        GROUP BY 1
    ),
    baseline AS (
        SELECT player_id, avg(v) AS base_avg, count(*) AS n_baseline
        FROM per_game
        GROUP BY 1
        HAVING count(*) >= p_min_baseline_games
    ),
    matchup AS (
        SELECT g.player_id, g.opponent_id,
               count(*) AS n,
               avg(g.v) AS m_avg,
               min(g.season) AS first_season,
               max(g.season) AS last_season,
               count(*) FILTER (
                   WHERE g.season >= (SELECT max_season FROM bounds) - 1
               ) AS recent_n
        FROM per_game g
        GROUP BY 1, 2
    ),
    calc AS (
        SELECT m.*, b.base_avg, b.n_baseline,
               COALESCE(oa.allow_ratio, 1) AS allow_ratio,
               b.base_avg * COALESCE(oa.allow_ratio, 1) AS expected,
               m.m_avg - b.base_avg AS raw_delta,
               m.m_avg - b.base_avg * COALESCE(oa.allow_ratio, 1) AS adj_delta,
               m.n / (m.n + p_k) AS shrink,
               0.5 + 0.5 * (m.recent_n::numeric / m.n) AS recency_factor
        FROM matchup m
        JOIN baseline b USING (player_id)
        LEFT JOIN opp_allow oa ON oa.opponent_id = m.opponent_id
    ),
    dir AS (
        -- Direction-consistency: share of the pair's meetings on the same side of
        -- expected as the overall adjusted delta. n=1 trivially scores 1.0 — harmless,
        -- the shrink term already crushes single-meeting reliability.
        SELECT c.player_id, c.opponent_id,
               avg(CASE WHEN sign(g.v - c.expected) = sign(c.adj_delta)
                        THEN 1.0 ELSE 0.0 END) AS dir_share
        FROM calc c
        JOIN per_game g ON g.player_id = c.player_id AND g.opponent_id = c.opponent_id
        WHERE c.adj_delta <> 0
        GROUP BY 1, 2
    )
    INSERT INTO stat_matchups
        (sport, scope, stat_key, subject_type, subject_id, object_type, object_id,
         n_games, baseline_avg, matchup_avg, expected_avg,
         raw_delta, adj_delta, shrunk_delta, reliability, components,
         first_season, last_season, updated_at)
    SELECT p_sport, p_scope, p_stat_key, 'player', c.player_id, 'team', c.opponent_id,
           c.n,
           round(c.base_avg, 3), round(c.m_avg, 3), round(c.expected, 3),
           round(c.raw_delta, 3), round(c.adj_delta, 3),
           round(c.adj_delta * c.shrink, 3),
           GREATEST(0, LEAST(100, round(
               100 * c.shrink
                   * c.recency_factor
                   * (0.5 + 0.5 * COALESCE(d.dir_share, 0.5)))))::smallint,
           jsonb_build_object(
               'n', c.n,
               'n_baseline', c.n_baseline,
               'recent_n', c.recent_n,
               'shrink', round(c.shrink, 3),
               'recency_factor', round(c.recency_factor, 3),
               'dir_share', round(COALESCE(d.dir_share, 0.5), 3),
               'allow_ratio', round(c.allow_ratio, 3),
               'k', p_k,
               'min_minutes', p_min_minutes),
           c.first_season, c.last_season, v_run
    FROM calc c
    LEFT JOIN dir d ON d.player_id = c.player_id AND d.opponent_id = c.opponent_id;

    GET DIAGNOSTICS rows_written = ROW_COUNT;

    SELECT count(DISTINCT subject_id) INTO subjects_covered
    FROM stat_matchups
    WHERE sport = p_sport AND scope = p_scope AND stat_key = p_stat_key;
END;
$$;

COMMENT ON FUNCTION public.refresh_stat_matchups(text, text, text, numeric, numeric, integer) IS
    'Full recompute of player-vs-team matchup edges for one (sport, stat_key): '
    'opponent-adjusted deltas + shrinkage-based reliability, presented not gatekept. '
    'Count-stat semantics (absent key = 0). DELETE + INSERT, idempotent, no model calls.';

INSERT INTO public.schema_migrations(version) VALUES ('156_stat_matchups')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
