-- Scoracle Data — Shared Schema
-- Contains: tables, indexes, roles, shared functions, notification infrastructure
-- Sport-specific views, functions, and triggers live in sql/nba.sql, sql/nfl.sql, sql/football.sql
--
-- Apply order: shared.sql first, then sport files in any order.

-- ============================================================================
-- 1. CORE INFRASTRUCTURE
-- ============================================================================

CREATE EXTENSION IF NOT EXISTS unaccent;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO meta (key, value) VALUES
    ('schema_version', '8.0'),
    ('last_full_sync', ''),
    ('last_incremental_sync', '')
ON CONFLICT (key) DO NOTHING;

CREATE TABLE IF NOT EXISTS sports (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    api_base_url TEXT DEFAULT NULL,
    current_season INTEGER NOT NULL,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO sports (id, display_name, current_season) VALUES
    ('NBA', 'NBA Basketball', 2025),
    ('NFL', 'NFL Football', 2025),
    ('FOOTBALL', 'Football (Soccer)', 2025)
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- 2. LEAGUES
-- ============================================================================

CREATE TABLE IF NOT EXISTS leagues (
    id INTEGER PRIMARY KEY,
    sport TEXT NOT NULL REFERENCES sports(id),
    name TEXT NOT NULL,
    country TEXT,
    logo_url TEXT,
    sportmonks_id INTEGER,
    is_benchmark BOOLEAN DEFAULT false,
    is_active BOOLEAN DEFAULT true,
    handicap DECIMAL,
    meta JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_leagues_sport ON leagues(sport);

INSERT INTO leagues (id, sport, name, country, sportmonks_id, is_benchmark) VALUES
    (8,   'FOOTBALL', 'Premier League', 'England', 8,   true),
    (82,  'FOOTBALL', 'Bundesliga',     'Germany', 82,  true),
    (301, 'FOOTBALL', 'Ligue 1',        'France',  301, true),
    (384, 'FOOTBALL', 'Serie A',        'Italy',   384, true),
    (564, 'FOOTBALL', 'La Liga',        'Spain',   564, true)
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- 3. PLAYERS
-- ============================================================================

CREATE TABLE IF NOT EXISTS players (
    id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    name TEXT NOT NULL,
    first_name TEXT,
    last_name TEXT,
    position TEXT,
    detailed_position TEXT,
    nationality TEXT,
    date_of_birth DATE,
    height TEXT,
    weight TEXT,
    photo_url TEXT,
    team_id INTEGER,
    league_id INTEGER,
    search_aliases TEXT[] DEFAULT '{}',
    meta JSONB DEFAULT '{}',
    raw_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, sport)
);

CREATE INDEX IF NOT EXISTS idx_players_sport ON players(sport);
CREATE INDEX IF NOT EXISTS idx_players_team ON players(team_id);
CREATE INDEX IF NOT EXISTS idx_players_name ON players(name);
CREATE INDEX IF NOT EXISTS idx_players_position ON players(sport, position);
CREATE INDEX IF NOT EXISTS idx_players_league ON players(league_id) WHERE league_id IS NOT NULL;

-- ============================================================================
-- 4. PLAYER STATS
-- ============================================================================

CREATE TABLE IF NOT EXISTS player_stats (
    player_id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    season INTEGER NOT NULL,
    league_id INTEGER NOT NULL DEFAULT 0,
    team_id INTEGER,
    stats JSONB NOT NULL DEFAULT '{}',
    percentiles JSONB DEFAULT '{}',
    raw_response JSONB,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (player_id, sport, season, league_id)
);

CREATE INDEX IF NOT EXISTS idx_player_stats_sport_season ON player_stats(sport, season);
CREATE INDEX IF NOT EXISTS idx_player_stats_team ON player_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_player_stats_league ON player_stats(league_id) WHERE league_id > 0;

-- ============================================================================
-- 5. TEAMS
-- ============================================================================

CREATE TABLE IF NOT EXISTS teams (
    id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    name TEXT NOT NULL,
    short_code TEXT,
    country TEXT,
    city TEXT,
    logo_url TEXT,
    league_id INTEGER,
    founded INTEGER,
    venue_name TEXT,
    venue_capacity INTEGER,
    conference TEXT,
    division TEXT,
    search_aliases TEXT[] DEFAULT '{}',
    meta JSONB DEFAULT '{}',
    raw_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, sport)
);

CREATE INDEX IF NOT EXISTS idx_teams_sport ON teams(sport);
CREATE INDEX IF NOT EXISTS idx_teams_name ON teams(name);
CREATE INDEX IF NOT EXISTS idx_teams_search_aliases ON teams USING GIN(search_aliases);
CREATE INDEX IF NOT EXISTS idx_teams_league ON teams(league_id) WHERE league_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_teams_conference ON teams(sport, conference) WHERE conference IS NOT NULL;

-- ============================================================================
-- 6. TEAM STATS
-- ============================================================================

CREATE TABLE IF NOT EXISTS team_stats (
    team_id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    season INTEGER NOT NULL,
    league_id INTEGER NOT NULL DEFAULT 0,
    stats JSONB NOT NULL DEFAULT '{}',
    percentiles JSONB DEFAULT '{}',
    raw_response JSONB,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (team_id, sport, season, league_id)
);

CREATE INDEX IF NOT EXISTS idx_team_stats_sport_season ON team_stats(sport, season);
CREATE INDEX IF NOT EXISTS idx_team_stats_league ON team_stats(league_id) WHERE league_id > 0;
CREATE INDEX IF NOT EXISTS idx_team_stats_wins
    ON team_stats (((stats->>'wins')::integer)) WHERE (stats->>'wins') IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_team_stats_points
    ON team_stats (((stats->>'points')::integer)) WHERE (stats->>'points') IS NOT NULL;

-- ============================================================================
-- 7. STAT DEFINITIONS (table only — sport-specific INSERTs in sport files)
-- ============================================================================

CREATE TABLE IF NOT EXISTS stat_definitions (
    id SERIAL PRIMARY KEY,
    sport TEXT NOT NULL REFERENCES sports(id),
    key_name TEXT NOT NULL,
    display_name TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('player', 'team')),
    category TEXT,
    is_inverse BOOLEAN NOT NULL DEFAULT false,
    is_derived BOOLEAN NOT NULL DEFAULT false,
    is_percentile_eligible BOOLEAN NOT NULL DEFAULT false,
    sort_order INTEGER NOT NULL DEFAULT 0,
    UNIQUE(sport, key_name, entity_type)
);

CREATE INDEX IF NOT EXISTS idx_stat_definitions_sport ON stat_definitions(sport, entity_type);

-- ============================================================================
-- 7b. PROVIDER STAT KEY MAPPINGS — maps raw provider keys to canonical names
-- ============================================================================

CREATE TABLE IF NOT EXISTS provider_stat_mappings (
    provider TEXT NOT NULL,
    sport TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('player', 'team')),
    raw_key TEXT NOT NULL,
    canonical_key TEXT NOT NULL,
    PRIMARY KEY (provider, sport, entity_type, raw_key)
);

-- BDL mappings (shared NBA/NFL — these keys appear in both)
INSERT INTO provider_stat_mappings (provider, sport, entity_type, raw_key, canonical_key) VALUES
    ('bdl', 'NBA', 'player', 'tov', 'turnover'),
    ('bdl', 'NBA', 'player', 'gp', 'games_played'),
    ('bdl', 'NBA', 'team', 'tov', 'turnover'),
    ('bdl', 'NBA', 'team', 'gp', 'games_played'),
    ('bdl', 'NBA', 'team', 'w', 'wins'),
    ('bdl', 'NBA', 'team', 'l', 'losses'),
    ('bdl', 'NFL', 'player', 'tov', 'turnover'),
    ('bdl', 'NFL', 'player', 'gp', 'games_played'),
    ('bdl', 'NFL', 'team', 'w', 'wins'),
    ('bdl', 'NFL', 'team', 'l', 'losses')
ON CONFLICT DO NOTHING;

-- SportMonks Football player stat code overrides
INSERT INTO provider_stat_mappings (provider, sport, entity_type, raw_key, canonical_key) VALUES
    ('sportmonks', 'FOOTBALL', 'player', 'passes', 'passes_total'),
    ('sportmonks', 'FOOTBALL', 'player', 'accurate-passes', 'passes_accurate'),
    ('sportmonks', 'FOOTBALL', 'player', 'total-crosses', 'crosses_total'),
    ('sportmonks', 'FOOTBALL', 'player', 'accurate-crosses', 'crosses_accurate'),
    ('sportmonks', 'FOOTBALL', 'player', 'blocked-shots', 'blocks'),
    ('sportmonks', 'FOOTBALL', 'player', 'total-duels', 'duels_total'),
    ('sportmonks', 'FOOTBALL', 'player', 'dribble-attempts', 'dribbles_attempts'),
    ('sportmonks', 'FOOTBALL', 'player', 'successful-dribbles', 'dribbles_success'),
    ('sportmonks', 'FOOTBALL', 'player', 'yellowcards', 'yellow_cards'),
    ('sportmonks', 'FOOTBALL', 'player', 'redcards', 'red_cards'),
    ('sportmonks', 'FOOTBALL', 'player', 'fouls', 'fouls_committed'),
    ('sportmonks', 'FOOTBALL', 'player', 'expected-goals', 'expected_goals')
ON CONFLICT DO NOTHING;

-- SportMonks Football team standing code overrides
INSERT INTO provider_stat_mappings (provider, sport, entity_type, raw_key, canonical_key) VALUES
    ('sportmonks', 'FOOTBALL', 'team', 'overall-matches-played', 'matches_played'),
    ('sportmonks', 'FOOTBALL', 'team', 'overall-won', 'wins'),
    ('sportmonks', 'FOOTBALL', 'team', 'overall-draw', 'draws'),
    ('sportmonks', 'FOOTBALL', 'team', 'overall-lost', 'losses'),
    ('sportmonks', 'FOOTBALL', 'team', 'overall-goals-for', 'goals_for'),
    ('sportmonks', 'FOOTBALL', 'team', 'overall-goals-against', 'goals_against'),
    ('sportmonks', 'FOOTBALL', 'team', 'home-matches-played', 'home_played'),
    ('sportmonks', 'FOOTBALL', 'team', 'away-matches-played', 'away_played')
ON CONFLICT DO NOTHING;

-- SportMonks Football fixture-level team statistics codes. The codes that
-- need explicit canonical names are listed here; everything else falls back
-- to hyphen->underscore replacement (e.g. `dangerous-attacks` -> `dangerous_attacks`).
INSERT INTO provider_stat_mappings (provider, sport, entity_type, raw_key, canonical_key) VALUES
    ('sportmonks', 'FOOTBALL', 'team', 'ball-possession',                    'possession_pct'),
    ('sportmonks', 'FOOTBALL', 'team', 'yellowcards',                        'yellow_cards'),
    ('sportmonks', 'FOOTBALL', 'team', 'redcards',                           'red_cards'),
    ('sportmonks', 'FOOTBALL', 'team', 'goals-kicks',                        'goal_kicks'),
    ('sportmonks', 'FOOTBALL', 'team', 'throwins',                           'throw_ins'),
    ('sportmonks', 'FOOTBALL', 'team', 'successful-passes',                  'accurate_passes'),
    ('sportmonks', 'FOOTBALL', 'team', 'successful-passes-percentage',       'pass_accuracy'),
    ('sportmonks', 'FOOTBALL', 'team', 'long-passes',                        'long_balls'),
    ('sportmonks', 'FOOTBALL', 'team', 'successful-long-passes',             'long_balls_won'),
    ('sportmonks', 'FOOTBALL', 'team', 'successful-long-passes-percentage',  'long_ball_accuracy'),
    ('sportmonks', 'FOOTBALL', 'team', 'successful-dribbles-percentage',     'dribble_success_rate'),
    ('sportmonks', 'FOOTBALL', 'team', 'shots-blocked',                      'shots_blocked_by_opp')
ON CONFLICT DO NOTHING;

-- ============================================================================
-- 7c. STAT KEY NORMALIZATION TRIGGER
-- Fires BEFORE INSERT OR UPDATE on player_stats and team_stats.
-- Looks up each raw key in provider_stat_mappings, falls back to
-- hyphen-to-underscore replacement. Runs before derived stat triggers
-- (trigger name starts with 'trg_a_' for alphabetical ordering).
-- ============================================================================

CREATE OR REPLACE FUNCTION normalize_stat_keys()
RETURNS TRIGGER AS $$
DECLARE
    raw_stats JSONB := NEW.stats;
    normalized JSONB := '{}';
    rec RECORD;
    mapped_key TEXT;
    v_entity_type TEXT;
BEGIN
    -- Skip if stats is null or empty
    IF raw_stats IS NULL OR raw_stats = '{}'::jsonb THEN
        RETURN NEW;
    END IF;

    IF TG_TABLE_NAME = 'player_stats' OR TG_TABLE_NAME = 'event_box_scores' THEN
        v_entity_type := 'player';
    ELSIF TG_TABLE_NAME = 'team_stats' OR TG_TABLE_NAME = 'event_team_stats' THEN
        v_entity_type := 'team';
    ELSE
        v_entity_type := NULL;
    END IF;

    FOR rec IN SELECT key, value FROM jsonb_each(raw_stats)
    LOOP
        -- Look up canonical key from mappings (any provider for this sport)
        SELECT m.canonical_key INTO mapped_key
        FROM provider_stat_mappings m
        WHERE m.sport = NEW.sport
          AND (v_entity_type IS NULL OR m.entity_type = v_entity_type)
          AND m.raw_key = rec.key
        LIMIT 1;

        -- Fall back to hyphen-to-underscore, then raw key
        mapped_key := COALESCE(mapped_key, replace(rec.key, '-', '_'));
        normalized := normalized || jsonb_build_object(mapped_key, rec.value);
    END LOOP;

    NEW.stats := normalized;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger names start with 'trg_a_' to fire before sport-specific derived
-- stat triggers (trg_nba_*, trg_nfl_*, trg_football_*) alphabetically.
DROP TRIGGER IF EXISTS trg_a_normalize_player_stats ON player_stats;
CREATE TRIGGER trg_a_normalize_player_stats
    BEFORE INSERT OR UPDATE OF stats ON player_stats
    FOR EACH ROW
    EXECUTE FUNCTION normalize_stat_keys();

DROP TRIGGER IF EXISTS trg_a_normalize_team_stats ON team_stats;
CREATE TRIGGER trg_a_normalize_team_stats
    BEFORE INSERT OR UPDATE OF stats ON team_stats
    FOR EACH ROW
    EXECUTE FUNCTION normalize_stat_keys();

-- ============================================================================
-- 8. FIXTURES & SCHEDULING
-- ============================================================================

CREATE TABLE IF NOT EXISTS fixtures (
    id SERIAL PRIMARY KEY,
    external_id INTEGER,
    sport TEXT NOT NULL REFERENCES sports(id),
    league_id INTEGER,
    season INTEGER NOT NULL,
    home_team_id INTEGER NOT NULL,
    away_team_id INTEGER NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    venue_name TEXT,
    round TEXT,
    status TEXT NOT NULL DEFAULT 'scheduled'
        CHECK (status IN ('scheduled', 'in_progress', 'completed', 'seeded', 'cancelled', 'postponed')),
    seed_delay_hours INTEGER NOT NULL DEFAULT 4,
    seeded_at TIMESTAMPTZ,
    seed_attempts INTEGER DEFAULT 0,
    last_seed_error TEXT,
    home_score INTEGER,
    away_score INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fixtures_sport_external_id_key UNIQUE (sport, external_id),
    CONSTRAINT different_teams CHECK (home_team_id != away_team_id)
);

CREATE INDEX IF NOT EXISTS idx_fixtures_pending_seed
    ON fixtures(sport, status, start_time) WHERE status = 'scheduled' OR status = 'completed';
CREATE INDEX IF NOT EXISTS idx_fixtures_sport_date ON fixtures(sport, start_time);
CREATE INDEX IF NOT EXISTS idx_fixtures_league_date ON fixtures(league_id, start_time) WHERE league_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_fixtures_home_team ON fixtures(home_team_id);
CREATE INDEX IF NOT EXISTS idx_fixtures_away_team ON fixtures(away_team_id);
CREATE INDEX IF NOT EXISTS idx_fixtures_season ON fixtures(season);

-- Migrate legacy global-unique external_id to sport-scoped uniqueness.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fixtures_external_id_key'
    ) THEN
        ALTER TABLE fixtures DROP CONSTRAINT fixtures_external_id_key;
    END IF;
END;
$$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fixtures_sport_external_id_key'
    ) THEN
        ALTER TABLE fixtures
            ADD CONSTRAINT fixtures_sport_external_id_key UNIQUE (sport, external_id);
    END IF;
END;
$$;

-- ============================================================================
-- 8b. PROVIDER MAPS + EVENT BOX SCORE TABLES
-- ============================================================================

CREATE TABLE IF NOT EXISTS provider_entity_map (
    provider TEXT NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    entity_type TEXT NOT NULL CHECK (entity_type IN ('player', 'team')),
    provider_entity_id TEXT NOT NULL,
    canonical_entity_id INTEGER NOT NULL,
    meta JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (provider, sport, entity_type, provider_entity_id)
);

CREATE INDEX IF NOT EXISTS idx_provider_entity_map_canonical
    ON provider_entity_map(sport, entity_type, canonical_entity_id);

CREATE TABLE IF NOT EXISTS provider_fixture_map (
    provider TEXT NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    provider_fixture_id TEXT NOT NULL,
    fixture_id INTEGER NOT NULL REFERENCES fixtures(id),
    meta JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (provider, sport, provider_fixture_id),
    UNIQUE(fixture_id, provider, sport)
);

CREATE INDEX IF NOT EXISTS idx_provider_fixture_map_fixture
    ON provider_fixture_map(fixture_id);

CREATE OR REPLACE FUNCTION resolve_provider_fixture_id(
    p_fixture_id INTEGER,
    p_provider TEXT,
    p_sport TEXT
)
RETURNS TEXT AS $$
    SELECT provider_fixture_id
    FROM provider_fixture_map
    WHERE fixture_id = p_fixture_id
      AND provider = p_provider
      AND sport = p_sport
    LIMIT 1;
$$ LANGUAGE sql STABLE;

-- Atomic event rows: one player line per fixture
CREATE TABLE IF NOT EXISTS event_box_scores (
    id BIGSERIAL PRIMARY KEY,
    fixture_id INTEGER NOT NULL REFERENCES fixtures(id),
    player_id INTEGER NOT NULL,
    team_id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    season INTEGER NOT NULL,
    league_id INTEGER NOT NULL DEFAULT 0,
    minutes_played NUMERIC,
    stats JSONB NOT NULL DEFAULT '{}',
    raw_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(fixture_id, player_id)
);

CREATE INDEX IF NOT EXISTS idx_event_box_scores_player_season
    ON event_box_scores(player_id, sport, season, league_id);
CREATE INDEX IF NOT EXISTS idx_event_box_scores_fixture
    ON event_box_scores(fixture_id);
CREATE INDEX IF NOT EXISTS idx_event_box_scores_team_season
    ON event_box_scores(team_id, sport, season, league_id);

-- Atomic event rows: one team line per fixture
CREATE TABLE IF NOT EXISTS event_team_stats (
    id BIGSERIAL PRIMARY KEY,
    fixture_id INTEGER NOT NULL REFERENCES fixtures(id),
    team_id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    season INTEGER NOT NULL,
    league_id INTEGER NOT NULL DEFAULT 0,
    score INTEGER,
    stats JSONB NOT NULL DEFAULT '{}',
    raw_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(fixture_id, team_id)
);

CREATE INDEX IF NOT EXISTS idx_event_team_stats_team_season
    ON event_team_stats(team_id, sport, season, league_id);
CREATE INDEX IF NOT EXISTS idx_event_team_stats_fixture
    ON event_team_stats(fixture_id);

CREATE OR REPLACE FUNCTION box_score_coverage_report(
    p_sport TEXT,
    p_season INTEGER,
    p_league_id INTEGER DEFAULT 0,
    p_required_keys TEXT[] DEFAULT ARRAY[]::TEXT[]
)
RETURNS TABLE (
    fixture_count INTEGER,
    player_row_count INTEGER,
    team_row_count INTEGER,
    missing_required_keys TEXT[]
) AS $$
WITH fixtures_in_scope AS (
    SELECT id
    FROM fixtures
    WHERE sport = p_sport
      AND season = p_season
      AND COALESCE(league_id, 0) = p_league_id
),
missing_keys AS (
    SELECT DISTINCT req.key_name
    FROM (
        SELECT unnest(p_required_keys) AS key_name
    ) req
    WHERE EXISTS (
        SELECT 1
        FROM event_box_scores ebs
        JOIN fixtures_in_scope fs ON fs.id = ebs.fixture_id
        WHERE ebs.sport = p_sport
          AND ebs.season = p_season
          AND ebs.league_id = p_league_id
          AND NOT (ebs.stats ? req.key_name)
    )
)
SELECT
    (SELECT COUNT(*)::int FROM fixtures_in_scope) AS fixture_count,
    (
        SELECT COUNT(*)::int
        FROM event_box_scores ebs
        JOIN fixtures_in_scope fs ON fs.id = ebs.fixture_id
        WHERE ebs.sport = p_sport
          AND ebs.season = p_season
          AND ebs.league_id = p_league_id
    ) AS player_row_count,
    (
        SELECT COUNT(*)::int
        FROM event_team_stats ets
        JOIN fixtures_in_scope fs ON fs.id = ets.fixture_id
        WHERE ets.sport = p_sport
          AND ets.season = p_season
          AND ets.league_id = p_league_id
    ) AS team_row_count,
    COALESCE((SELECT array_agg(key_name ORDER BY key_name) FROM missing_keys), ARRAY[]::TEXT[]) AS missing_required_keys;
$$ LANGUAGE sql STABLE;

DROP TRIGGER IF EXISTS trg_a_normalize_event_box_scores ON event_box_scores;
CREATE TRIGGER trg_a_normalize_event_box_scores
    BEFORE INSERT OR UPDATE OF stats ON event_box_scores
    FOR EACH ROW
    EXECUTE FUNCTION normalize_stat_keys();

DROP TRIGGER IF EXISTS trg_a_normalize_event_team_stats ON event_team_stats;
CREATE TRIGGER trg_a_normalize_event_team_stats
    BEFORE INSERT OR UPDATE OF stats ON event_team_stats
    FOR EACH ROW
    EXECUTE FUNCTION normalize_stat_keys();

-- ============================================================================
-- 9. PROVIDER SEASONS
-- ============================================================================

CREATE TABLE IF NOT EXISTS provider_seasons (
    id SERIAL PRIMARY KEY,
    league_id INTEGER NOT NULL REFERENCES leagues(id),
    season_year INTEGER NOT NULL,
    provider TEXT NOT NULL DEFAULT 'sportmonks',
    provider_season_id INTEGER NOT NULL,
    UNIQUE(league_id, season_year, provider)
);

CREATE INDEX IF NOT EXISTS idx_provider_seasons_lookup ON provider_seasons(league_id, season_year);

-- ============================================================================
-- 11. USERS & NOTIFICATIONS (platform tables)
-- ============================================================================

CREATE TABLE IF NOT EXISTS users (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timezone    TEXT NOT NULL DEFAULT 'UTC',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS user_follows (
    id          SERIAL PRIMARY KEY,
    user_id     UUID NOT NULL REFERENCES users(id),
    entity_type TEXT NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id   INTEGER NOT NULL,
    sport       TEXT NOT NULL REFERENCES sports(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, entity_type, entity_id, sport)
);

CREATE INDEX IF NOT EXISTS idx_user_follows_entity ON user_follows(entity_type, entity_id, sport);
CREATE INDEX IF NOT EXISTS idx_user_follows_user ON user_follows(user_id);

CREATE TABLE IF NOT EXISTS user_devices (
    id          SERIAL PRIMARY KEY,
    user_id     UUID NOT NULL REFERENCES users(id),
    platform    TEXT NOT NULL CHECK (platform IN ('ios', 'android', 'web')),
    token       TEXT NOT NULL,
    is_active   BOOLEAN DEFAULT true,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, token)
);

CREATE TABLE IF NOT EXISTS notifications (
    id              SERIAL PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id),
    entity_type     TEXT NOT NULL,
    entity_id       INTEGER NOT NULL,
    sport           TEXT NOT NULL REFERENCES sports(id),
    fixture_id      INTEGER REFERENCES fixtures(id),
    stat_key        TEXT NOT NULL,
    percentile      NUMERIC NOT NULL,
    message         TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'scheduled'
        CHECK (status IN ('scheduled', 'sending', 'sent', 'failed')),
    scheduled_for   TIMESTAMPTZ NOT NULL,
    sent_at         TIMESTAMPTZ,
    last_error      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_notifications_dispatch
    ON notifications(status, scheduled_for) WHERE status = 'scheduled';
CREATE INDEX IF NOT EXISTS idx_notifications_user
    ON notifications(user_id, created_at DESC);

-- ============================================================================
-- 12. SHARED HELPER FUNCTIONS
-- ============================================================================

CREATE OR REPLACE FUNCTION get_pending_fixtures(
    p_sport TEXT DEFAULT NULL,
    p_limit INTEGER DEFAULT 50,
    p_max_retries INTEGER DEFAULT 3
)
RETURNS TABLE (
    id INTEGER, sport TEXT, league_id INTEGER, season INTEGER,
    home_team_id INTEGER, away_team_id INTEGER, start_time TIMESTAMPTZ,
    seed_delay_hours INTEGER, seed_attempts INTEGER, external_id INTEGER
) AS $$
    SELECT f.id, f.sport, f.league_id, f.season,
           f.home_team_id, f.away_team_id, f.start_time,
           f.seed_delay_hours, f.seed_attempts, f.external_id
    FROM fixtures f
    WHERE (f.status = 'scheduled' OR f.status = 'completed')
      AND NOW() >= f.start_time + (f.seed_delay_hours || ' hours')::INTERVAL
      AND f.seed_attempts < p_max_retries
      AND (p_sport IS NULL OR f.sport = p_sport)
    ORDER BY f.start_time ASC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;

CREATE OR REPLACE FUNCTION mark_fixture_seeded(
    p_fixture_id INTEGER,
    p_home_score INTEGER DEFAULT NULL,
    p_away_score INTEGER DEFAULT NULL
)
RETURNS VOID AS $$
BEGIN
    UPDATE fixtures SET
        status = 'seeded', seeded_at = NOW(),
        home_score = COALESCE(p_home_score, home_score),
        away_score = COALESCE(p_away_score, away_score),
        updated_at = NOW()
    WHERE id = p_fixture_id;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION resolve_provider_season_id(
    p_league_id INTEGER,
    p_season_year INTEGER,
    p_provider TEXT DEFAULT 'sportmonks'
)
RETURNS INTEGER AS $$
    SELECT provider_season_id
    FROM provider_seasons
    WHERE league_id = p_league_id
      AND season_year = p_season_year
      AND provider = p_provider;
$$ LANGUAGE sql STABLE;

-- Upsert a fixture from a provider schedule API
CREATE OR REPLACE FUNCTION upsert_fixture(
    p_external_id INTEGER,
    p_sport TEXT,
    p_league_id INTEGER,
    p_season INTEGER,
    p_home_team_id INTEGER,
    p_away_team_id INTEGER,
    p_start_time TIMESTAMPTZ,
    p_venue_name TEXT DEFAULT NULL,
    p_round TEXT DEFAULT NULL,
    p_seed_delay_hours INTEGER DEFAULT 4
)
RETURNS INTEGER AS $$
    INSERT INTO fixtures (external_id, sport, league_id, season, home_team_id,
                          away_team_id, start_time, venue_name, round, seed_delay_hours)
    VALUES (p_external_id, p_sport, p_league_id, p_season, p_home_team_id,
            p_away_team_id, p_start_time, p_venue_name, p_round, p_seed_delay_hours)
    ON CONFLICT (sport, external_id) DO UPDATE SET
        league_id = COALESCE(EXCLUDED.league_id, fixtures.league_id),
        season = EXCLUDED.season,
        home_team_id = EXCLUDED.home_team_id,
        away_team_id = EXCLUDED.away_team_id,
        start_time = EXCLUDED.start_time,
        venue_name = COALESCE(EXCLUDED.venue_name, fixtures.venue_name),
        round = COALESCE(EXCLUDED.round, fixtures.round),
        seed_delay_hours = EXCLUDED.seed_delay_hours,
        updated_at = NOW()
    RETURNING id;
$$ LANGUAGE sql;

-- Finalize a fixture after seeding: recalculate percentiles, refresh views, mark seeded.
-- This is the single handoff point from the Python seeder to Postgres.
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
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, updated_at)
        SELECT
            e.player_id,
            'NBA',
            v_season,
            v_league_id,
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
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, updated_at)
        SELECT
            e.player_id,
            'NFL',
            v_season,
            v_league_id,
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
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, updated_at)
        SELECT
            e.player_id,
            'FOOTBALL',
            v_season,
            v_league_id,
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

-- ============================================================================
-- 13. PERCENTILE CALCULATION
-- ============================================================================

CREATE OR REPLACE FUNCTION recalculate_percentiles(
    p_sport TEXT,
    p_season INTEGER,
    p_inverse_stats TEXT[] DEFAULT ARRAY[]::TEXT[]
)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
DECLARE
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
    v_inverse TEXT[];
BEGIN
    SELECT array_agg(DISTINCT key_name) INTO v_inverse
    FROM (
        SELECT key_name FROM stat_definitions WHERE sport = p_sport AND is_inverse = true
        UNION
        SELECT unnest(p_inverse_stats)
    ) combined;
    v_inverse := COALESCE(v_inverse, ARRAY[]::TEXT[]);

    -- Player percentiles (partitioned by position)
    WITH stat_keys AS (
        SELECT DISTINCT key FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    player_positions AS (
        SELECT ps.player_id, COALESCE(p.position, 'Unknown') AS position
        FROM player_stats ps JOIN players p ON p.id = ps.player_id AND p.sport = ps.sport
        WHERE ps.sport = p_sport AND ps.season = p_season
    ),
    expanded AS (
        SELECT ps.player_id, pp.position, sk.key AS stat_key, (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_stats ps CROSS JOIN stat_keys sk JOIN player_positions pp ON pp.player_id = ps.player_id
        WHERE ps.sport = p_sport AND ps.season = p_season AND ps.stats ? sk.key AND (ps.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT player_id, position, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY position, stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT player_id, position, max(sample_size) AS max_sample_size,
            jsonb_object_agg(stat_key, percentile) || jsonb_build_object('_position_group', position, '_sample_size', max(sample_size)) AS percentiles_json
        FROM ranked GROUP BY player_id, position
    )
    UPDATE player_stats ps SET percentiles = agg.percentiles_json, updated_at = NOW()
    FROM aggregated agg WHERE ps.player_id = agg.player_id AND ps.sport = p_sport AND ps.season = p_season;
    GET DIAGNOSTICS v_players = ROW_COUNT;

    -- Team percentiles (no position partitioning)
    WITH stat_keys AS (
        SELECT DISTINCT key FROM team_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    expanded AS (
        SELECT ts.team_id, sk.key AS stat_key, (ts.stats->>sk.key)::numeric AS stat_value
        FROM team_stats ts CROSS JOIN stat_keys sk
        WHERE ts.sport = p_sport AND ts.season = p_season AND ts.stats ? sk.key AND (ts.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT team_id, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT team_id, jsonb_object_agg(stat_key, percentile) || jsonb_build_object('_sample_size', max(sample_size)) AS percentiles_json
        FROM ranked GROUP BY team_id
    )
    UPDATE team_stats ts SET percentiles = agg.percentiles_json, updated_at = NOW()
    FROM aggregated agg WHERE ts.team_id = agg.team_id AND ts.sport = p_sport AND ts.season = p_season;
    GET DIAGNOSTICS v_teams = ROW_COUNT;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;



-- ============================================================================
-- 14. LISTEN/NOTIFY — percentile change detection trigger
-- Fires on UPDATE of percentiles column. Uses OLD vs NEW to detect
-- significant changes (milestone crossing at 90/95/99 or delta >= 10).
-- Go listener receives events and dispatches FCM push notifications.
-- ============================================================================

CREATE OR REPLACE FUNCTION notify_percentile_changed()
RETURNS TRIGGER AS $$
DECLARE
    stat_key TEXT;
    new_val NUMERIC;
    old_val NUMERIC;
    entity_type TEXT;
    entity_id INTEGER;
BEGIN
    -- Determine entity type
    IF TG_TABLE_NAME = 'player_stats' THEN
        entity_type := 'player';
        entity_id := NEW.player_id;
    ELSIF TG_TABLE_NAME = 'team_stats' THEN
        entity_type := 'team';
        entity_id := NEW.team_id;
    ELSE
        RETURN NEW;
    END IF;

    -- Skip if percentiles unchanged
    IF OLD.percentiles IS NOT DISTINCT FROM NEW.percentiles THEN
        RETURN NEW;
    END IF;

    -- Check each stat key for significant changes
    FOR stat_key IN
        SELECT key FROM jsonb_each(NEW.percentiles)
        WHERE key NOT LIKE '\_%'
          AND jsonb_typeof(value) = 'number'
    LOOP
        new_val := (NEW.percentiles ->> stat_key)::numeric;
        old_val := COALESCE((OLD.percentiles ->> stat_key)::numeric, 0);

        -- Significant = milestone crossing (90/95/99) OR large delta (>= 10)
        IF (new_val >= 90 AND old_val < 90)
        OR (new_val >= 95 AND old_val < 95)
        OR (new_val >= 99 AND old_val < 99)
        OR (new_val - old_val >= 10) THEN
            PERFORM pg_notify('percentile_changed', json_build_object(
                'entity_type', entity_type,
                'entity_id', entity_id,
                'sport', NEW.sport,
                'season', NEW.season,
                'stat_key', stat_key,
                'old_percentile', old_val,
                'new_percentile', new_val,
                'ts', extract(epoch from now())::bigint
            )::text);
        END IF;
    END LOOP;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Only fire on UPDATE (not INSERT) — we need OLD for delta comparison.
-- First recalculate_percentiles() call UPDATEs rows from percentiles='{}' to
-- actual values, so old_val defaults to 0 via COALESCE.
DROP TRIGGER IF EXISTS trg_milestone_player_stats ON player_stats;
DROP TRIGGER IF EXISTS trg_percentile_changed_player_stats ON player_stats;
CREATE TRIGGER trg_percentile_changed_player_stats
    AFTER UPDATE OF percentiles ON player_stats
    FOR EACH ROW
    WHEN (NEW.percentiles IS NOT NULL AND NEW.percentiles != '{}'::jsonb)
    EXECUTE FUNCTION notify_percentile_changed();

DROP TRIGGER IF EXISTS trg_milestone_team_stats ON team_stats;
DROP TRIGGER IF EXISTS trg_percentile_changed_team_stats ON team_stats;
CREATE TRIGGER trg_percentile_changed_team_stats
    AFTER UPDATE OF percentiles ON team_stats
    FOR EACH ROW
    WHEN (NEW.percentiles IS NOT NULL AND NEW.percentiles != '{}'::jsonb)
    EXECUTE FUNCTION notify_percentile_changed();

-- ============================================================================
-- 16. POSTGREST ROLES
-- ============================================================================

DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'web_anon') THEN
        CREATE ROLE web_anon NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'web_user') THEN
        CREATE ROLE web_user NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'authenticator') THEN
        CREATE ROLE authenticator NOINHERIT LOGIN;
    END IF;
END $$;

GRANT web_anon TO authenticator;
GRANT web_user TO authenticator;

DO $$
DECLARE
    db_owner name;
BEGIN
    SELECT r.rolname INTO db_owner
    FROM pg_database d JOIN pg_roles r ON r.oid = d.datdba
    WHERE d.datname = current_database();
    IF db_owner IS NOT NULL THEN
        EXECUTE format('GRANT web_anon TO %I', db_owner);
        EXECUTE format('GRANT web_user TO %I', db_owner);
    END IF;
END $$;

-- ============================================================================
-- 17. ROW LEVEL SECURITY (platform tables)
-- ============================================================================

ALTER TABLE user_follows ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS user_follows_own ON user_follows;
CREATE POLICY user_follows_own ON user_follows
    FOR ALL TO web_user
    USING (user_id::text = current_setting('request.jwt.claims', true)::json->>'sub')
    WITH CHECK (user_id::text = current_setting('request.jwt.claims', true)::json->>'sub');

ALTER TABLE user_devices ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS user_devices_own ON user_devices;
CREATE POLICY user_devices_own ON user_devices
    FOR ALL TO web_user
    USING (user_id::text = current_setting('request.jwt.claims', true)::json->>'sub')
    WITH CHECK (user_id::text = current_setting('request.jwt.claims', true)::json->>'sub');

ALTER TABLE notifications ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS notifications_own ON notifications;
CREATE POLICY notifications_own ON notifications
    FOR SELECT TO web_user
    USING (user_id::text = current_setting('request.jwt.claims', true)::json->>'sub');
