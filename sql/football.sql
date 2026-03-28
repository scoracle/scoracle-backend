-- Scoracle Data — Football (Soccer) Schema
-- Owner: Football product owner
-- Contains: Football-specific views, stat definitions, triggers, functions, grants
-- Depends on: sql/shared.sql (public tables must exist first)

CREATE SCHEMA IF NOT EXISTS football;

-- ============================================================================
-- 1. STAT DEFINITIONS
-- ============================================================================

-- Football player stats
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('FOOTBALL', 'appearances',           'Appearances',          'player', 'general',    false, false, false,  1),
    ('FOOTBALL', 'lineups',               'Starting Lineups',     'player', 'general',    false, false, false,  2),
    ('FOOTBALL', 'minutes_played',        'Minutes Played',       'player', 'general',    false, false, true,   3),
    ('FOOTBALL', 'goals',                 'Goals',                'player', 'scoring',    false, false, true,  10),
    ('FOOTBALL', 'assists',               'Assists',              'player', 'scoring',    false, false, true,  11),
    ('FOOTBALL', 'expected_goals',        'Expected Goals (xG)',  'player', 'scoring',    false, false, true,  12),
    ('FOOTBALL', 'goals_per_90',          'Goals Per 90',         'player', 'scoring',    false, true,  true,  13),
    ('FOOTBALL', 'assists_per_90',        'Assists Per 90',       'player', 'scoring',    false, true,  true,  14),
    ('FOOTBALL', 'shots_total',           'Total Shots',          'player', 'shooting',   false, false, false, 20),
    ('FOOTBALL', 'shots_on_target',       'Shots on Target',      'player', 'shooting',   false, false, false, 21),
    ('FOOTBALL', 'shots_per_90',          'Shots Per 90',         'player', 'shooting',   false, true,  true,  22),
    ('FOOTBALL', 'shot_accuracy',         'Shot Accuracy %',      'player', 'shooting',   false, true,  true,  23),
    ('FOOTBALL', 'passes_total',          'Total Passes',         'player', 'passing',    false, false, false, 30),
    ('FOOTBALL', 'passes_accurate',       'Accurate Passes',      'player', 'passing',    false, false, false, 31),
    ('FOOTBALL', 'key_passes',            'Key Passes',           'player', 'passing',    false, false, true,  32),
    ('FOOTBALL', 'crosses_total',         'Total Crosses',        'player', 'passing',    false, false, false, 33),
    ('FOOTBALL', 'crosses_accurate',      'Accurate Crosses',     'player', 'passing',    false, false, false, 34),
    ('FOOTBALL', 'key_passes_per_90',     'Key Passes Per 90',    'player', 'passing',    false, true,  true,  35),
    ('FOOTBALL', 'pass_accuracy',         'Pass Accuracy %',      'player', 'passing',    false, true,  true,  36),
    ('FOOTBALL', 'tackles',               'Tackles',              'player', 'defensive',  false, false, true,  40),
    ('FOOTBALL', 'interceptions',         'Interceptions',        'player', 'defensive',  false, false, true,  41),
    ('FOOTBALL', 'clearances',            'Clearances',           'player', 'defensive',  false, false, false, 42),
    ('FOOTBALL', 'blocks',                'Blocks',               'player', 'defensive',  false, false, false, 43),
    ('FOOTBALL', 'tackles_per_90',        'Tackles Per 90',       'player', 'defensive',  false, true,  true,  44),
    ('FOOTBALL', 'interceptions_per_90',  'Interceptions/90',     'player', 'defensive',  false, true,  true,  45),
    ('FOOTBALL', 'duels_total',           'Total Duels',          'player', 'duels',      false, false, false, 50),
    ('FOOTBALL', 'duels_won',             'Duels Won',            'player', 'duels',      false, false, false, 51),
    ('FOOTBALL', 'duel_success_rate',     'Duel Success Rate %',  'player', 'duels',      false, true,  true,  52),
    ('FOOTBALL', 'dribbles_attempts',     'Dribble Attempts',     'player', 'dribbling',  false, false, false, 55),
    ('FOOTBALL', 'dribbles_success',      'Successful Dribbles',  'player', 'dribbling',  false, false, false, 56),
    ('FOOTBALL', 'dribble_success_rate',  'Dribble Success %',    'player', 'dribbling',  false, true,  true,  57),
    ('FOOTBALL', 'yellow_cards',          'Yellow Cards',         'player', 'discipline', true,  false, true,  60),
    ('FOOTBALL', 'red_cards',             'Red Cards',            'player', 'discipline', true,  false, true,  61),
    ('FOOTBALL', 'fouls_committed',       'Fouls Committed',      'player', 'discipline', true,  false, false, 62),
    ('FOOTBALL', 'fouls_drawn',           'Fouls Drawn',          'player', 'discipline', false, false, false, 63),
    ('FOOTBALL', 'saves',                 'Saves',                'player', 'goalkeeper', false, false, true,  70),
    ('FOOTBALL', 'goals_conceded',        'Goals Conceded',       'player', 'goalkeeper', true,  false, true,  71),
    ('FOOTBALL', 'goals_conceded_per_90', 'Goals Conceded/90',    'player', 'goalkeeper', true,  true,  true,  72),
    ('FOOTBALL', 'save_pct',              'Save Percentage %',    'player', 'goalkeeper', false, true,  true,  73)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- Football team stats
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('FOOTBALL', 'matches_played',  'Matches Played',       'team', 'standings', false, false, false,  1),
    ('FOOTBALL', 'wins',            'Wins',                 'team', 'standings', false, false, true,   2),
    ('FOOTBALL', 'draws',           'Draws',                'team', 'standings', false, false, false,  3),
    ('FOOTBALL', 'losses',          'Losses',               'team', 'standings', true,  false, true,   4),
    ('FOOTBALL', 'goals_for',       'Goals For',            'team', 'scoring',   false, false, true,   5),
    ('FOOTBALL', 'goals_against',   'Goals Against',        'team', 'scoring',   true,  false, true,   6),
    ('FOOTBALL', 'goal_difference', 'Goal Difference',      'team', 'scoring',   false, false, true,   7),
    ('FOOTBALL', 'points',          'Points',               'team', 'standings', false, false, true,   8),
    ('FOOTBALL', 'overall_points',  'Overall Points',       'team', 'standings', false, false, false,  9),
    ('FOOTBALL', 'position',        'League Position',      'team', 'standings', false, false, false, 10),
    ('FOOTBALL', 'home_played',     'Home Matches',         'team', 'home',      false, false, false, 20),
    ('FOOTBALL', 'home_won',        'Home Wins',            'team', 'home',      false, false, false, 21),
    ('FOOTBALL', 'home_draw',       'Home Draws',           'team', 'home',      false, false, false, 22),
    ('FOOTBALL', 'home_lost',       'Home Losses',          'team', 'home',      false, false, false, 23),
    ('FOOTBALL', 'home_scored',     'Home Goals Scored',    'team', 'home',      false, false, false, 24),
    ('FOOTBALL', 'home_conceded',   'Home Goals Conceded',  'team', 'home',      false, false, false, 25),
    ('FOOTBALL', 'home_points',     'Home Points',          'team', 'home',      false, false, false, 26),
    ('FOOTBALL', 'away_played',     'Away Matches',         'team', 'away',      false, false, false, 30),
    ('FOOTBALL', 'away_won',        'Away Wins',            'team', 'away',      false, false, false, 31),
    ('FOOTBALL', 'away_draw',       'Away Draws',           'team', 'away',      false, false, false, 32),
    ('FOOTBALL', 'away_lost',       'Away Losses',          'team', 'away',      false, false, false, 33),
    ('FOOTBALL', 'away_scored',     'Away Goals Scored',    'team', 'away',      false, false, false, 34),
    ('FOOTBALL', 'away_conceded',   'Away Goals Conceded',  'team', 'away',      false, false, false, 35),
    ('FOOTBALL', 'away_points',     'Away Points',          'team', 'away',      false, false, false, 36)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ============================================================================
-- 2. DERIVED STATS TRIGGERS
-- ============================================================================

-- Football player: per-90 metrics, accuracy rates, GK stats
CREATE OR REPLACE FUNCTION football.compute_derived_player_stats()
RETURNS TRIGGER AS $$
DECLARE
    minutes NUMERIC; goals NUMERIC; assists NUMERIC; key_passes NUMERIC;
    shots_t NUMERIC; shots_on NUMERIC; passes_t NUMERIC; passes_a NUMERIC;
    tackles NUMERIC; intercepts NUMERIC; duels_t NUMERIC; duels_w NUMERIC;
    dribbles_a NUMERIC; dribbles_s NUMERIC; saves NUMERIC; conceded NUMERIC;
BEGIN
    minutes    := (NEW.stats->>'minutes_played')::NUMERIC;
    goals      := (NEW.stats->>'goals')::NUMERIC;
    assists    := (NEW.stats->>'assists')::NUMERIC;
    key_passes := (NEW.stats->>'key_passes')::NUMERIC;
    shots_t    := (NEW.stats->>'shots_total')::NUMERIC;
    shots_on   := (NEW.stats->>'shots_on_target')::NUMERIC;
    passes_t   := (NEW.stats->>'passes_total')::NUMERIC;
    passes_a   := (NEW.stats->>'passes_accurate')::NUMERIC;
    tackles    := (NEW.stats->>'tackles')::NUMERIC;
    intercepts := (NEW.stats->>'interceptions')::NUMERIC;
    duels_t    := (NEW.stats->>'duels_total')::NUMERIC;
    duels_w    := (NEW.stats->>'duels_won')::NUMERIC;
    dribbles_a := (NEW.stats->>'dribbles_attempts')::NUMERIC;
    dribbles_s := (NEW.stats->>'dribbles_success')::NUMERIC;
    saves      := (NEW.stats->>'saves')::NUMERIC;
    conceded   := (NEW.stats->>'goals_conceded')::NUMERIC;

    IF minutes IS NOT NULL AND minutes > 0 THEN
        IF goals IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('goals_per_90', ROUND(goals * 90 / minutes, 3)); END IF;
        IF assists IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('assists_per_90', ROUND(assists * 90 / minutes, 3)); END IF;
        IF key_passes IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('key_passes_per_90', ROUND(key_passes * 90 / minutes, 3)); END IF;
        IF shots_t IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('shots_per_90', ROUND(shots_t * 90 / minutes, 3)); END IF;
        IF tackles IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('tackles_per_90', ROUND(tackles * 90 / minutes, 3)); END IF;
        IF intercepts IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('interceptions_per_90', ROUND(intercepts * 90 / minutes, 3)); END IF;
        IF conceded IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('goals_conceded_per_90', ROUND(conceded * 90 / minutes, 3)); END IF;
    END IF;

    IF shots_t IS NOT NULL AND shots_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('shot_accuracy', ROUND(COALESCE(shots_on, 0) / shots_t * 100, 1)); END IF;
    IF passes_t IS NOT NULL AND passes_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('pass_accuracy', ROUND(COALESCE(passes_a, 0) / passes_t * 100, 1)); END IF;
    IF duels_t IS NOT NULL AND duels_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('duel_success_rate', ROUND(COALESCE(duels_w, 0) / duels_t * 100, 1)); END IF;
    IF dribbles_a IS NOT NULL AND dribbles_a > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('dribble_success_rate', ROUND(COALESCE(dribbles_s, 0) / dribbles_a * 100, 1)); END IF;
    IF saves IS NOT NULL AND conceded IS NOT NULL AND (saves + conceded) > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('save_pct', ROUND(saves / (saves + conceded) * 100, 1));
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Triggers on shared tables
DROP TRIGGER IF EXISTS trg_football_derived_stats ON player_stats;
CREATE TRIGGER trg_football_derived_stats
    BEFORE INSERT OR UPDATE ON player_stats
    FOR EACH ROW WHEN (NEW.sport = 'FOOTBALL')
    EXECUTE FUNCTION football.compute_derived_player_stats();

-- ============================================================================
-- 3. VIEWS (PostgREST surface)
-- ============================================================================

-- Drop legacy views from pre-consolidation (players, player_stats, teams, team_stats)
DROP VIEW IF EXISTS football.players;
DROP VIEW IF EXISTS football.player_stats;
DROP VIEW IF EXISTS football.teams;
DROP VIEW IF EXISTS football.team_stats;

-- Combined player profile + stats (with team and league context)
CREATE OR REPLACE VIEW football.player AS
SELECT
    p.id, p.name, p.first_name, p.last_name, p.position,
    p.detailed_position, p.nationality, p.date_of_birth::text AS date_of_birth,
    p.height, p.weight, p.photo_url, p.team_id, p.league_id,
    CASE WHEN t.id IS NOT NULL THEN json_build_object(
        'id', t.id, 'name', t.name, 'abbreviation', t.short_code,
        'logo_url', t.logo_url, 'country', t.country, 'city', t.city
    ) END AS team,
    CASE WHEN l.id IS NOT NULL THEN json_build_object(
        'id', l.id, 'name', l.name, 'country', l.country, 'logo_url', l.logo_url
    ) END AS league,
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
LEFT JOIN public.leagues l ON l.id = p.league_id
LEFT JOIN public.player_stats ps ON ps.player_id = p.id AND ps.sport = p.sport
WHERE p.sport = 'FOOTBALL';

COMMENT ON VIEW football.player IS
    'Football player profile with stats. Filter by id, season, league_id. Stats columns are NULL when no stats exist.';

-- Combined team profile + stats (with league context)
CREATE OR REPLACE VIEW football.team AS
SELECT
    t.id, t.name, t.short_code, t.logo_url, t.country, t.city,
    t.founded, t.league_id, t.venue_name, t.venue_capacity,
    CASE WHEN l.id IS NOT NULL THEN json_build_object(
        'id', l.id, 'name', l.name, 'country', l.country, 'logo_url', l.logo_url
    ) END AS league,
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
LEFT JOIN public.leagues l ON l.id = t.league_id
LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
WHERE t.sport = 'FOOTBALL';

COMMENT ON VIEW football.team IS
    'Football team profile with stats. Filter by id, season, league_id. Stats columns are NULL when no stats exist.';

-- Standings (hardcoded Football sort: points, goal difference, goals for)
CREATE OR REPLACE VIEW football.standings AS
SELECT
    ts.team_id, ts.season, ts.league_id,
    t.name AS team_name, t.short_code AS team_abbr, t.logo_url,
    l.name AS league_name, ts.stats,
    (ts.stats->>'points')::integer AS sort_points,
    (ts.stats->>'goal_difference')::integer AS sort_goal_diff
FROM public.team_stats ts
JOIN public.teams t ON t.id = ts.team_id AND t.sport = ts.sport
LEFT JOIN public.leagues l ON l.id = ts.league_id
WHERE ts.sport = 'FOOTBALL';

COMMENT ON VIEW football.standings IS
    'Football standings. Order by sort_points DESC, sort_goal_diff DESC. Filter by season, league_id.';

CREATE OR REPLACE VIEW football.stat_definitions AS
SELECT id, key_name, display_name, entity_type, category,
       is_inverse, is_derived, is_percentile_eligible, sort_order
FROM public.stat_definitions
WHERE sport = 'FOOTBALL';

COMMENT ON VIEW football.stat_definitions IS
    'Football stat registry. Filter by entity_type.';

-- Leagues (Football-specific)
CREATE OR REPLACE VIEW football.leagues AS
SELECT id, name, country, logo_url, is_benchmark, is_active, handicap, meta
FROM public.leagues
WHERE sport = 'FOOTBALL';

COMMENT ON VIEW football.leagues IS
    'Football league metadata. Filter by is_active, is_benchmark.';

-- ============================================================================
-- 4. MATERIALIZED VIEW — autofill/search
-- ============================================================================

DROP MATERIALIZED VIEW IF EXISTS football.autofill_entities;
CREATE MATERIALIZED VIEW football.autofill_entities AS
    -- Players (resolve league from latest player_stats)
    SELECT * FROM (
        SELECT DISTINCT ON (p.id)
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
            ps.league_id,
            l.name AS league_name,
            t.short_code AS team_abbr,
            t.name AS team_name,
            jsonb_build_array(
                LOWER(p.first_name),
                LOWER(p.last_name),
                LOWER(REPLACE(p.name, ' ', '')),
                LOWER(COALESCE(t.short_code, '')),
                LOWER(COALESCE(t.name, '')),
                LOWER(COALESCE(l.name, ''))
            ) AS search_tokens,
            jsonb_build_object(
                'display_name', p.name,
                'jersey_number', p.meta->>'jersey_number',
                'foot', p.meta->>'foot',
                'market_value', (p.meta->>'market_value')::bigint,
                'contract_until', p.meta->>'contract_until'
            ) AS meta
        FROM public.players p
        LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
        LEFT JOIN public.player_stats ps ON ps.player_id = p.id AND ps.sport = p.sport
        LEFT JOIN public.leagues l ON l.id = ps.league_id
        WHERE p.sport = 'FOOTBALL'
        ORDER BY p.id, ps.season DESC NULLS LAST
    ) football_players
UNION ALL
    -- Teams (resolve league from latest team_stats)
    SELECT * FROM (
        SELECT DISTINCT ON (t.id)
            t.id,
            'team'::text AS type,
            t.name,
            NULL::text AS first_name,
            NULL::text AS last_name,
            NULL::text AS position,
            NULL::text AS detailed_position,
            t.country AS nationality,
            NULL::text AS date_of_birth,
            NULL::text AS height,
            NULL::text AS weight,
            t.logo_url AS photo_url,
            NULL::int AS team_id,
            ts.league_id,
            l.name AS league_name,
            t.short_code AS team_abbr,
            NULL::text AS team_name,
            jsonb_build_array(
                LOWER(REPLACE(t.name, ' ', '')),
                LOWER(t.short_code),
                LOWER(t.city),
                LOWER(t.country),
                LOWER(COALESCE(l.name, ''))
            ) AS search_tokens,
            jsonb_build_object(
                'display_name', t.name,
                'abbreviation', t.short_code,
                'city', t.city,
                'country', t.country,
                'founded', t.founded,
                'venue_name', t.venue_name,
                'venue_capacity', t.venue_capacity
            ) AS meta
        FROM public.teams t
        LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
        LEFT JOIN public.leagues l ON l.id = ts.league_id
        WHERE t.sport = 'FOOTBALL'
        ORDER BY t.id, ts.season DESC NULLS LAST
    ) football_teams
WITH DATA;

CREATE UNIQUE INDEX IF NOT EXISTS idx_football_autofill_pk
    ON football.autofill_entities (id, type);

-- ============================================================================
-- 5. RPC FUNCTIONS
-- ============================================================================

CREATE OR REPLACE FUNCTION football.stat_leaders(
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
    WHERE s.sport = 'FOOTBALL' AND s.season = p_season AND s.league_id = p_league_id
      AND stat.val IS NOT NULL
      AND (p_position IS NULL OR p.position = p_position)
    ORDER BY stat.val DESC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;

COMMENT ON FUNCTION football.stat_leaders IS
    'Returns top N Football players by stat category with positional filtering.';

CREATE OR REPLACE FUNCTION football.health()
RETURNS json AS $$
    SELECT json_build_object('status', 'ok');
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 6. EVENT -> SEASON AGGREGATION FUNCTIONS
-- ============================================================================

CREATE OR REPLACE FUNCTION football.aggregate_player_season(
    p_player_id INTEGER,
    p_season INTEGER,
    p_league_id INTEGER DEFAULT 0
)
RETURNS JSONB AS $$
WITH agg AS (
    SELECT
        COUNT(*)::numeric AS matches_played,
        SUM(COALESCE(minutes_played, 0)) AS minutes_sum,
        SUM(COALESCE((stats->>'goals')::numeric, 0)) AS goals_sum,
        SUM(COALESCE((stats->>'assists')::numeric, 0)) AS assists_sum,
        SUM(COALESCE((stats->>'shots_total')::numeric, 0)) AS shots_sum,
        SUM(COALESCE((stats->>'shots_on_target')::numeric, 0)) AS sot_sum,
        SUM(COALESCE((stats->>'passes_total')::numeric, 0)) AS passes_sum,
        SUM(COALESCE((stats->>'passes_accurate')::numeric, 0)) AS passes_acc_sum,
        SUM(COALESCE((stats->>'tackles')::numeric, 0)) AS tackles_sum,
        SUM(COALESCE((stats->>'interceptions')::numeric, 0)) AS interceptions_sum,
        SUM(COALESCE((stats->>'yellow_cards')::numeric, 0)) AS yellow_sum,
        SUM(COALESCE((stats->>'red_cards')::numeric, 0)) AS red_sum,
        SUM(COALESCE((stats->>'expected_goals')::numeric, 0)) AS xg_sum
    FROM public.event_box_scores
    WHERE player_id = p_player_id
      AND sport = 'FOOTBALL'
      AND season = p_season
      AND league_id = p_league_id
)
SELECT CASE
    WHEN matches_played = 0 THEN '{}'::jsonb
    ELSE jsonb_strip_nulls(
        jsonb_build_object(
            'appearances', matches_played::int,
            'lineups', matches_played::int,
            'minutes_played', CASE WHEN matches_played > 0 THEN ROUND(minutes_sum / matches_played, 1) END,
            'goals', goals_sum::int,
            'assists', assists_sum::int,
            'expected_goals', ROUND(xg_sum, 2),
            'shots_total', shots_sum::int,
            'shots_on_target', sot_sum::int,
            'passes_total', passes_sum::int,
            'passes_accurate', passes_acc_sum::int,
            'tackles', tackles_sum::int,
            'interceptions', interceptions_sum::int,
            'yellow_cards', yellow_sum::int,
            'red_cards', red_sum::int
        )
    )
END
FROM agg;
$$ LANGUAGE sql STABLE;

CREATE OR REPLACE FUNCTION football.aggregate_team_season(
    p_team_id INTEGER,
    p_season INTEGER,
    p_league_id INTEGER DEFAULT 0
)
RETURNS JSONB AS $$
WITH agg AS (
    SELECT
        COUNT(*)::numeric AS matches_played,
        SUM(CASE WHEN opp.score IS NOT NULL AND score > opp.score THEN 1 ELSE 0 END)::numeric AS wins,
        SUM(CASE WHEN opp.score IS NOT NULL AND score < opp.score THEN 1 ELSE 0 END)::numeric AS losses,
        SUM(CASE WHEN opp.score IS NOT NULL AND score = opp.score THEN 1 ELSE 0 END)::numeric AS draws,
        SUM(COALESCE(score, 0))::numeric AS gf_sum,
        SUM(COALESCE(opp.score, 0))::numeric AS ga_sum,
        SUM(
            CASE
                WHEN f.home_team_id = ets.team_id THEN CASE WHEN opp.score IS NOT NULL AND score > opp.score THEN 1 ELSE 0 END
                ELSE 0
            END
        )::numeric AS home_won,
        SUM(
            CASE
                WHEN f.home_team_id = ets.team_id THEN CASE WHEN opp.score IS NOT NULL AND score = opp.score THEN 1 ELSE 0 END
                ELSE 0
            END
        )::numeric AS home_draw,
        SUM(
            CASE
                WHEN f.home_team_id = ets.team_id THEN CASE WHEN opp.score IS NOT NULL AND score < opp.score THEN 1 ELSE 0 END
                ELSE 0
            END
        )::numeric AS home_lost,
        SUM(
            CASE
                WHEN f.away_team_id = ets.team_id THEN CASE WHEN opp.score IS NOT NULL AND score > opp.score THEN 1 ELSE 0 END
                ELSE 0
            END
        )::numeric AS away_won,
        SUM(
            CASE
                WHEN f.away_team_id = ets.team_id THEN CASE WHEN opp.score IS NOT NULL AND score = opp.score THEN 1 ELSE 0 END
                ELSE 0
            END
        )::numeric AS away_draw,
        SUM(
            CASE
                WHEN f.away_team_id = ets.team_id THEN CASE WHEN opp.score IS NOT NULL AND score < opp.score THEN 1 ELSE 0 END
                ELSE 0
            END
        )::numeric AS away_lost,
        SUM(CASE WHEN f.home_team_id = ets.team_id THEN COALESCE(score, 0) ELSE 0 END)::numeric AS home_scored,
        SUM(CASE WHEN f.home_team_id = ets.team_id THEN COALESCE(opp.score, 0) ELSE 0 END)::numeric AS home_conceded,
        SUM(CASE WHEN f.away_team_id = ets.team_id THEN COALESCE(score, 0) ELSE 0 END)::numeric AS away_scored,
        SUM(CASE WHEN f.away_team_id = ets.team_id THEN COALESCE(opp.score, 0) ELSE 0 END)::numeric AS away_conceded,
        SUM(CASE WHEN f.home_team_id = ets.team_id THEN 1 ELSE 0 END)::numeric AS home_played,
        SUM(CASE WHEN f.away_team_id = ets.team_id THEN 1 ELSE 0 END)::numeric AS away_played
    FROM public.event_team_stats ets
    JOIN public.fixtures f ON f.id = ets.fixture_id
    LEFT JOIN public.event_team_stats opp
        ON opp.fixture_id = ets.fixture_id
       AND opp.sport = ets.sport
       AND opp.season = ets.season
       AND opp.league_id = ets.league_id
       AND opp.team_id <> ets.team_id
    WHERE ets.team_id = p_team_id
      AND ets.sport = 'FOOTBALL'
      AND ets.season = p_season
      AND ets.league_id = p_league_id
)
SELECT CASE
    WHEN matches_played = 0 THEN '{}'::jsonb
    ELSE jsonb_strip_nulls(
        jsonb_build_object(
            'matches_played', matches_played::int,
            'wins', wins::int,
            'draws', draws::int,
            'losses', losses::int,
            'goals_for', gf_sum::int,
            'goals_against', ga_sum::int,
            'goal_difference', (gf_sum - ga_sum)::int,
            'points', (wins * 3 + draws)::int,
            'overall_points', (wins * 3 + draws)::int,
            'home_played', home_played::int,
            'home_won', home_won::int,
            'home_draw', home_draw::int,
            'home_lost', home_lost::int,
            'home_scored', home_scored::int,
            'home_conceded', home_conceded::int,
            'home_points', (home_won * 3 + home_draw)::int,
            'away_played', away_played::int,
            'away_won', away_won::int,
            'away_draw', away_draw::int,
            'away_lost', away_lost::int,
            'away_scored', away_scored::int,
            'away_conceded', away_conceded::int,
            'away_points', (away_won * 3 + away_draw)::int
        )
    )
END
FROM agg;
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 7. GRANTS
-- ============================================================================

GRANT USAGE ON SCHEMA football TO web_anon, web_user;
GRANT SELECT ON ALL TABLES IN SCHEMA football TO web_anon, web_user;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA football TO web_anon, web_user;
