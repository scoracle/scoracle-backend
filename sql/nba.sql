-- Scoracle Data — NBA Schema
-- Owner: NBA product owner
-- Contains: NBA-specific views, stat definitions, triggers, functions, grants
-- Depends on: sql/shared.sql (public tables must exist first)

CREATE SCHEMA IF NOT EXISTS nba;

-- ============================================================================
-- 1. STAT DEFINITIONS
-- ============================================================================

-- NBA player stats
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NBA', 'games_played',       'Games Played',          'player', 'general',    false, false, false,  1),
    ('NBA', 'minutes',            'Minutes Per Game',      'player', 'general',    false, false, true,   2),
    ('NBA', 'pts',                'Points Per Game',       'player', 'scoring',    false, false, true,   3),
    ('NBA', 'reb',                'Rebounds Per Game',      'player', 'rebounding', false, false, true,   4),
    ('NBA', 'ast',                'Assists Per Game',       'player', 'passing',    false, false, true,   5),
    ('NBA', 'stl',                'Steals Per Game',        'player', 'defensive',  false, false, true,   6),
    ('NBA', 'blk',                'Blocks Per Game',        'player', 'defensive',  false, false, true,   7),
    ('NBA', 'oreb',               'Off Rebounds/Game',      'player', 'rebounding', false, false, false,  8),
    ('NBA', 'dreb',               'Def Rebounds/Game',      'player', 'rebounding', false, false, false,  9),
    ('NBA', 'turnover',           'Turnovers Per Game',     'player', 'general',    true,  false, true,  10),
    ('NBA', 'pf',                 'Fouls Per Game',         'player', 'general',    true,  false, false, 11),
    ('NBA', 'plus_minus',         'Plus/Minus',             'player', 'advanced',   false, false, true,  12),
    ('NBA', 'fg_pct',             'Field Goal %',           'player', 'shooting',   false, false, true,  20),
    ('NBA', 'fg3_pct',            'Three-Point %',          'player', 'shooting',   false, false, true,  21),
    ('NBA', 'ft_pct',             'Free Throw %',           'player', 'shooting',   false, false, true,  22),
    ('NBA', 'fgm',                'Field Goals Made',       'player', 'shooting',   false, false, false, 23),
    ('NBA', 'fga',                'Field Goals Attempted',  'player', 'shooting',   false, false, false, 24),
    ('NBA', 'fg3m',               'Three-Pointers Made',    'player', 'shooting',   false, false, false, 25),
    ('NBA', 'fg3a',               'Three-Pointers Att',     'player', 'shooting',   false, false, false, 26),
    ('NBA', 'ftm',                'Free Throws Made',       'player', 'shooting',   false, false, false, 27),
    ('NBA', 'fta',                'Free Throws Attempted',  'player', 'shooting',   false, false, false, 28),
    ('NBA', 'pts_per_36',         'Points Per 36 Min',      'player', 'advanced',   false, true,  true,  30),
    ('NBA', 'reb_per_36',         'Rebounds Per 36 Min',     'player', 'advanced',   false, true,  true,  31),
    ('NBA', 'ast_per_36',         'Assists Per 36 Min',      'player', 'advanced',   false, true,  true,  32),
    ('NBA', 'stl_per_36',         'Steals Per 36 Min',       'player', 'advanced',   false, true,  true,  33),
    ('NBA', 'blk_per_36',         'Blocks Per 36 Min',       'player', 'advanced',   false, true,  true,  34),
    ('NBA', 'true_shooting_pct',  'True Shooting %',        'player', 'advanced',   false, true,  true,  35),
    ('NBA', 'efficiency',         'Efficiency Rating',      'player', 'advanced',   false, true,  true,  36)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- NBA team stats
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NBA', 'wins',          'Wins',             'team', 'standings',  false, false, true,   1),
    ('NBA', 'losses',        'Losses',           'team', 'standings',  true,  false, true,   2),
    ('NBA', 'games_played',  'Games Played',     'team', 'general',    false, false, false,  3),
    ('NBA', 'pts',           'Points Per Game',  'team', 'scoring',    false, false, true,   4),
    ('NBA', 'reb',           'Rebounds Per Game', 'team', 'rebounding', false, false, true,   5),
    ('NBA', 'ast',           'Assists Per Game',  'team', 'passing',    false, false, true,   6),
    ('NBA', 'fg_pct',        'Field Goal %',     'team', 'shooting',   false, false, true,   7),
    ('NBA', 'fg3_pct',       'Three-Point %',    'team', 'shooting',   false, false, true,   8),
    ('NBA', 'ft_pct',        'Free Throw %',     'team', 'shooting',   false, false, false,  9),
    ('NBA', 'win_pct',       'Win Percentage',   'team', 'standings',  false, true,  true,  10)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ============================================================================
-- 2. DERIVED STATS TRIGGERS
-- ============================================================================

-- NBA player: per-36, true shooting %, efficiency
CREATE OR REPLACE FUNCTION nba.compute_derived_player_stats()
RETURNS TRIGGER AS $$
DECLARE
    minutes NUMERIC; pts NUMERIC; reb NUMERIC; ast NUMERIC;
    stl NUMERIC; blk NUMERIC; fga NUMERIC; fgm NUMERIC;
    fta NUMERIC; ftm NUMERIC; turnover NUMERIC; tsa NUMERIC;
BEGIN
    minutes  := (NEW.stats->>'minutes')::NUMERIC;
    pts      := (NEW.stats->>'pts')::NUMERIC;
    reb      := (NEW.stats->>'reb')::NUMERIC;
    ast      := (NEW.stats->>'ast')::NUMERIC;
    stl      := (NEW.stats->>'stl')::NUMERIC;
    blk      := (NEW.stats->>'blk')::NUMERIC;
    fga      := (NEW.stats->>'fga')::NUMERIC;
    fgm      := (NEW.stats->>'fgm')::NUMERIC;
    fta      := (NEW.stats->>'fta')::NUMERIC;
    ftm      := (NEW.stats->>'ftm')::NUMERIC;
    turnover := (NEW.stats->>'turnover')::NUMERIC;

    IF minutes IS NOT NULL AND minutes > 0 THEN
        IF pts IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('pts_per_36', ROUND(pts / minutes * 36, 1)); END IF;
        IF reb IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('reb_per_36', ROUND(reb / minutes * 36, 1)); END IF;
        IF ast IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('ast_per_36', ROUND(ast / minutes * 36, 1)); END IF;
        IF stl IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('stl_per_36', ROUND(stl / minutes * 36, 1)); END IF;
        IF blk IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('blk_per_36', ROUND(blk / minutes * 36, 1)); END IF;
    END IF;

    IF pts IS NOT NULL AND fga IS NOT NULL AND fta IS NOT NULL THEN
        tsa := fga + 0.44 * fta;
        IF tsa > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('true_shooting_pct', ROUND(pts / (2 * tsa) * 100, 1)); END IF;
    END IF;

    IF pts IS NOT NULL AND reb IS NOT NULL AND ast IS NOT NULL AND stl IS NOT NULL AND blk IS NOT NULL
       AND fga IS NOT NULL AND fgm IS NOT NULL AND fta IS NOT NULL AND ftm IS NOT NULL AND turnover IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object('efficiency', ROUND((pts + reb + ast + stl + blk) - ((fga - fgm) + (fta - ftm) + turnover), 1));
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- NBA team: win_pct
CREATE OR REPLACE FUNCTION nba.compute_derived_team_stats()
RETURNS TRIGGER AS $$
DECLARE wins NUMERIC; losses NUMERIC; total NUMERIC;
BEGIN
    wins := (NEW.stats->>'wins')::NUMERIC; losses := (NEW.stats->>'losses')::NUMERIC;
    IF wins IS NOT NULL AND losses IS NOT NULL THEN
        total := wins + losses;
        IF total > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('win_pct', ROUND(wins / total, 3)); END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Triggers on shared tables
DROP TRIGGER IF EXISTS trg_nba_player_derived_stats ON player_stats;
CREATE TRIGGER trg_nba_player_derived_stats
    BEFORE INSERT OR UPDATE ON player_stats
    FOR EACH ROW WHEN (NEW.sport = 'NBA')
    EXECUTE FUNCTION nba.compute_derived_player_stats();

DROP TRIGGER IF EXISTS trg_nba_team_derived_stats ON team_stats;
CREATE TRIGGER trg_nba_team_derived_stats
    BEFORE INSERT OR UPDATE ON team_stats
    FOR EACH ROW WHEN (NEW.sport = 'NBA')
    EXECUTE FUNCTION nba.compute_derived_team_stats();

-- ============================================================================
-- 3. VIEWS (PostgREST surface)
-- ============================================================================

-- Drop legacy views from pre-consolidation (players, player_stats, teams, team_stats)
DROP VIEW IF EXISTS nba.players;
DROP VIEW IF EXISTS nba.player_stats;
DROP VIEW IF EXISTS nba.teams;
DROP VIEW IF EXISTS nba.team_stats;

-- Combined player profile + stats
CREATE OR REPLACE VIEW nba.player AS
SELECT
    p.id, p.name, p.first_name, p.last_name, p.position,
    p.detailed_position, p.nationality, p.date_of_birth::text AS date_of_birth,
    p.height, p.weight, p.photo_url, p.team_id, p.league_id,
    CASE WHEN t.id IS NOT NULL THEN json_build_object(
        'id', t.id, 'name', t.name, 'abbreviation', t.short_code,
        'logo_url', t.logo_url, 'country', t.country, 'city', t.city,
        'conference', t.conference, 'division', t.division
    ) END AS team,
    ps.season,
    ps.stats,
    ps.percentiles - '_position_group' - '_sample_size' AS percentiles,
    CASE
        WHEN ps.percentiles IS NOT NULL
            AND ps.percentiles->>'_position_group' IS NOT NULL
        THEN jsonb_build_object(
            'position_group', ps.percentiles->>'_position_group',
            'sample_size', COALESCE((ps.percentiles->>'_sample_size')::int, 0)
        )
    END AS percentile_metadata,
    ps.updated_at AS stats_updated_at
FROM public.players p
LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
LEFT JOIN public.player_stats ps ON ps.player_id = p.id AND ps.sport = p.sport
WHERE p.sport = 'NBA';

COMMENT ON VIEW nba.player IS
    'NBA player profile with stats. Filter by id, season. Stats columns are NULL when no stats exist.';

-- Combined team profile + stats
CREATE OR REPLACE VIEW nba.team AS
SELECT
    t.id, t.name, t.short_code, t.logo_url, t.country, t.city,
    t.founded, t.league_id, t.conference, t.division,
    t.venue_name, t.venue_capacity,
    ts.season,
    ts.stats,
    ts.percentiles - '_sample_size' AS percentiles,
    CASE
        WHEN ts.percentiles IS NOT NULL
            AND ts.percentiles->>'_sample_size' IS NOT NULL
        THEN jsonb_build_object(
            'sample_size', COALESCE((ts.percentiles->>'_sample_size')::int, 0)
        )
    END AS percentile_metadata,
    ts.updated_at AS stats_updated_at
FROM public.teams t
LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
WHERE t.sport = 'NBA';

COMMENT ON VIEW nba.team IS
    'NBA team profile with stats. Filter by id, season. Stats columns are NULL when no stats exist.';

-- Standings (hardcoded NBA sort: by wins)
CREATE OR REPLACE VIEW nba.standings AS
SELECT
    ts.team_id, ts.season, ts.league_id,
    t.name AS team_name, t.short_code AS team_abbr, t.logo_url,
    t.conference, t.division, ts.stats,
    ROUND(
        (ts.stats->>'wins')::numeric /
        NULLIF((ts.stats->>'wins')::integer + (ts.stats->>'losses')::integer, 0), 3
    ) AS win_pct
FROM public.team_stats ts
JOIN public.teams t ON t.id = ts.team_id AND t.sport = ts.sport
WHERE ts.sport = 'NBA';

COMMENT ON VIEW nba.standings IS
    'NBA standings. Order by win_pct DESC. Filter by season, conference.';

-- Stat definitions
CREATE OR REPLACE VIEW nba.stat_definitions AS
SELECT id, key_name, display_name, entity_type, category,
       is_inverse, is_derived, is_percentile_eligible, sort_order
FROM public.stat_definitions
WHERE sport = 'NBA';

COMMENT ON VIEW nba.stat_definitions IS
    'NBA stat registry. Filter by entity_type.';

-- ============================================================================
-- 4. MATERIALIZED VIEW — autofill/search
-- ============================================================================

DROP MATERIALIZED VIEW IF EXISTS nba.autofill_entities;
CREATE MATERIALIZED VIEW nba.autofill_entities AS
    SELECT
        p.id,
        'player'::text AS type,
        p.name,
        p.first_name,
        p.last_name,
        p.position,
        p.detailed_position,
        p.nationality,
        p.date_of_birth::text AS date_of_birth,
        p.height,
        p.weight,
        p.photo_url,
        p.team_id,
        NULL::int AS league_id,
        NULL::text AS league_name,
        t.short_code AS team_abbr,
        t.name AS team_name,
        jsonb_build_array(
            LOWER(p.first_name),
            LOWER(p.last_name),
            LOWER(REPLACE(p.name, ' ', '')),
            LOWER(COALESCE(t.short_code, '')),
            LOWER(COALESCE(t.name, ''))
        ) AS search_tokens,
        jsonb_build_object(
            'display_name', p.name,
            'jersey_number', p.meta->>'jersey_number',
            'draft_year', (p.meta->>'draft_year')::int,
            'draft_pick', (p.meta->>'draft_pick')::int,
            'years_pro', (p.meta->>'years_pro')::int,
            'college', p.meta->>'college'
        ) AS meta
    FROM public.players p
    LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
    WHERE p.sport = 'NBA'
UNION ALL
    SELECT
        t.id,
        'team'::text AS type,
        t.name,
        NULL::text AS first_name,
        NULL::text AS last_name,
        t.conference AS position,
        t.division AS detailed_position,
        t.country AS nationality,
        NULL::text AS date_of_birth,
        NULL::text AS height,
        NULL::text AS weight,
        t.logo_url AS photo_url,
        NULL::int AS team_id,
        NULL::int AS league_id,
        NULL::text AS league_name,
        t.short_code AS team_abbr,
        NULL::text AS team_name,
        jsonb_build_array(
            LOWER(REPLACE(t.name, ' ', '')),
            LOWER(t.short_code),
            LOWER(t.city),
            LOWER(t.country)
        ) AS search_tokens,
        jsonb_build_object(
            'display_name', t.name,
            'abbreviation', t.short_code,
            'city', t.city,
            'country', t.country,
            'conference', t.conference,
            'division', t.division,
            'founded', t.founded,
            'venue_name', t.venue_name,
            'venue_capacity', t.venue_capacity
        ) AS meta
    FROM public.teams t
    WHERE t.sport = 'NBA'
WITH DATA;

CREATE UNIQUE INDEX IF NOT EXISTS idx_nba_autofill_pk
    ON nba.autofill_entities (id, type);

-- ============================================================================
-- 5. RPC FUNCTIONS
-- ============================================================================

CREATE OR REPLACE FUNCTION nba.stat_leaders(
    p_season INTEGER, p_stat_name TEXT,
    p_limit INTEGER DEFAULT 25, p_position TEXT DEFAULT NULL,
    p_league_id INTEGER DEFAULT 0
)
RETURNS TABLE ("rank" BIGINT, player_id INTEGER, name TEXT, "position" TEXT, team_name TEXT, stat_value NUMERIC) AS $$
    SELECT
        ROW_NUMBER() OVER (ORDER BY stat.val DESC) AS rank,
        p.id AS player_id, p.name, p.position, t.name AS team_name, stat.val AS stat_value
    FROM public.player_stats s
    CROSS JOIN LATERAL (SELECT (s.stats->>p_stat_name)::NUMERIC AS val) stat
    JOIN public.players p ON s.player_id = p.id AND s.sport = p.sport
    LEFT JOIN public.teams t ON s.team_id = t.id AND s.sport = t.sport
    WHERE s.sport = 'NBA' AND s.season = p_season AND s.league_id = p_league_id
      AND stat.val IS NOT NULL
      AND (p_position IS NULL OR p.position = p_position)
    ORDER BY stat.val DESC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;

COMMENT ON FUNCTION nba.stat_leaders IS
    'Returns top N NBA players by stat category with positional filtering.';

CREATE OR REPLACE FUNCTION nba.health()
RETURNS json AS $$
    SELECT json_build_object('status', 'ok');
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 6. EVENT -> SEASON AGGREGATION FUNCTIONS
-- ============================================================================

CREATE OR REPLACE FUNCTION nba.aggregate_player_season(
    p_player_id INTEGER,
    p_season INTEGER,
    p_league_id INTEGER DEFAULT 0
)
RETURNS JSONB AS $$
WITH agg AS (
    SELECT
        COUNT(*)::numeric AS gp,
        AVG(minutes_played) AS minutes_avg,
        AVG(NULLIF((stats->>'pts')::numeric, NULL)) AS pts_avg,
        AVG(NULLIF((stats->>'reb')::numeric, NULL)) AS reb_avg,
        AVG(NULLIF((stats->>'ast')::numeric, NULL)) AS ast_avg,
        AVG(NULLIF((stats->>'stl')::numeric, NULL)) AS stl_avg,
        AVG(NULLIF((stats->>'blk')::numeric, NULL)) AS blk_avg,
        AVG(NULLIF((stats->>'turnover')::numeric, NULL)) AS tov_avg,
        AVG(NULLIF((stats->>'pf')::numeric, NULL)) AS pf_avg,
        AVG(NULLIF((stats->>'plus_minus')::numeric, NULL)) AS pm_avg,
        AVG(NULLIF((stats->>'oreb')::numeric, NULL)) AS oreb_avg,
        AVG(NULLIF((stats->>'dreb')::numeric, NULL)) AS dreb_avg,
        AVG(NULLIF((stats->>'fgm')::numeric, NULL)) AS fgm_avg,
        AVG(NULLIF((stats->>'fga')::numeric, NULL)) AS fga_avg,
        AVG(NULLIF((stats->>'fg3m')::numeric, NULL)) AS fg3m_avg,
        AVG(NULLIF((stats->>'fg3a')::numeric, NULL)) AS fg3a_avg,
        AVG(NULLIF((stats->>'ftm')::numeric, NULL)) AS ftm_avg,
        AVG(NULLIF((stats->>'fta')::numeric, NULL)) AS fta_avg,
        SUM(COALESCE((stats->>'fgm')::numeric, 0)) AS fgm_sum,
        SUM(COALESCE((stats->>'fga')::numeric, 0)) AS fga_sum,
        SUM(COALESCE((stats->>'fg3m')::numeric, 0)) AS fg3m_sum,
        SUM(COALESCE((stats->>'fg3a')::numeric, 0)) AS fg3a_sum,
        SUM(COALESCE((stats->>'ftm')::numeric, 0)) AS ftm_sum,
        SUM(COALESCE((stats->>'fta')::numeric, 0)) AS fta_sum
    FROM public.event_box_scores
    WHERE player_id = p_player_id
      AND sport = 'NBA'
      AND season = p_season
      AND league_id = p_league_id
)
SELECT CASE
    WHEN gp = 0 THEN '{}'::jsonb
    ELSE jsonb_strip_nulls(
        jsonb_build_object(
            'games_played', gp::int,
            'minutes', ROUND(minutes_avg, 1),
            'pts', ROUND(pts_avg, 1),
            'reb', ROUND(reb_avg, 1),
            'ast', ROUND(ast_avg, 1),
            'stl', ROUND(stl_avg, 1),
            'blk', ROUND(blk_avg, 1),
            'turnover', ROUND(tov_avg, 1),
            'pf', ROUND(pf_avg, 1),
            'plus_minus', ROUND(pm_avg, 1),
            'oreb', ROUND(oreb_avg, 1),
            'dreb', ROUND(dreb_avg, 1),
            'fgm', ROUND(fgm_avg, 1),
            'fga', ROUND(fga_avg, 1),
            'fg3m', ROUND(fg3m_avg, 1),
            'fg3a', ROUND(fg3a_avg, 1),
            'ftm', ROUND(ftm_avg, 1),
            'fta', ROUND(fta_avg, 1),
            'fg_pct', CASE WHEN fga_sum > 0 THEN ROUND((fgm_sum / fga_sum) * 100, 1) END,
            'fg3_pct', CASE WHEN fg3a_sum > 0 THEN ROUND((fg3m_sum / fg3a_sum) * 100, 1) END,
            'ft_pct', CASE WHEN fta_sum > 0 THEN ROUND((ftm_sum / fta_sum) * 100, 1) END
        )
    )
END
FROM agg;
$$ LANGUAGE sql STABLE;

CREATE OR REPLACE FUNCTION nba.aggregate_team_season(
    p_team_id INTEGER,
    p_season INTEGER,
    p_league_id INTEGER DEFAULT 0
)
RETURNS JSONB AS $$
WITH agg AS (
    SELECT
        COUNT(*)::numeric AS gp,
        SUM(CASE WHEN opp.score IS NOT NULL AND score > opp.score THEN 1 ELSE 0 END)::numeric AS wins,
        SUM(CASE WHEN opp.score IS NOT NULL AND score < opp.score THEN 1 ELSE 0 END)::numeric AS losses,
        AVG(NULLIF((stats->>'pts')::numeric, NULL)) AS pts_avg,
        AVG(NULLIF((stats->>'reb')::numeric, NULL)) AS reb_avg,
        AVG(NULLIF((stats->>'ast')::numeric, NULL)) AS ast_avg,
        AVG(NULLIF((stats->>'turnover')::numeric, NULL)) AS tov_avg,
        SUM(COALESCE((stats->>'fgm')::numeric, 0)) AS fgm_sum,
        SUM(COALESCE((stats->>'fga')::numeric, 0)) AS fga_sum,
        SUM(COALESCE((stats->>'fg3m')::numeric, 0)) AS fg3m_sum,
        SUM(COALESCE((stats->>'fg3a')::numeric, 0)) AS fg3a_sum,
        SUM(COALESCE((stats->>'ftm')::numeric, 0)) AS ftm_sum,
        SUM(COALESCE((stats->>'fta')::numeric, 0)) AS fta_sum
    FROM public.event_team_stats ets
    LEFT JOIN public.event_team_stats opp
        ON opp.fixture_id = ets.fixture_id
       AND opp.sport = ets.sport
       AND opp.season = ets.season
       AND opp.league_id = ets.league_id
       AND opp.team_id <> ets.team_id
    WHERE ets.team_id = p_team_id
      AND ets.sport = 'NBA'
      AND ets.season = p_season
      AND ets.league_id = p_league_id
)
SELECT CASE
    WHEN gp = 0 THEN '{}'::jsonb
    ELSE jsonb_strip_nulls(
        jsonb_build_object(
            'games_played', gp::int,
            'wins', wins::int,
            'losses', losses::int,
            'pts', ROUND(pts_avg, 1),
            'reb', ROUND(reb_avg, 1),
            'ast', ROUND(ast_avg, 1),
            'turnover', ROUND(tov_avg, 1),
            'fg_pct', CASE WHEN fga_sum > 0 THEN ROUND((fgm_sum / fga_sum) * 100, 1) END,
            'fg3_pct', CASE WHEN fg3a_sum > 0 THEN ROUND((fg3m_sum / fg3a_sum) * 100, 1) END,
            'ft_pct', CASE WHEN fta_sum > 0 THEN ROUND((ftm_sum / fta_sum) * 100, 1) END
        )
    )
END
FROM agg;
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 7. GRANTS
-- ============================================================================

GRANT USAGE ON SCHEMA nba TO web_anon, web_user;
GRANT SELECT ON ALL TABLES IN SCHEMA nba TO web_anon, web_user;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA nba TO web_anon, web_user;
