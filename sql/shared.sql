-- Scoracle Data — Shared Schema
-- Contains: tables, indexes, roles, shared functions, notification infrastructure
-- Sport-specific views, functions, and triggers live in sql/nba.sql, sql/nfl.sql, sql/football.sql
--
-- Apply order: shared.sql first, then sport files in any order.

-- ============================================================================
-- 1. CORE INFRASTRUCTURE
-- ============================================================================

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
    meta JSONB DEFAULT '{}',
    raw_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, sport)
);

CREATE INDEX IF NOT EXISTS idx_teams_sport ON teams(sport);
CREATE INDEX IF NOT EXISTS idx_teams_name ON teams(name);
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
-- 8. FIXTURES & SCHEDULING
-- ============================================================================

CREATE TABLE IF NOT EXISTS fixtures (
    id SERIAL PRIMARY KEY,
    external_id INTEGER UNIQUE,
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
    CONSTRAINT different_teams CHECK (home_team_id != away_team_id)
);

CREATE INDEX IF NOT EXISTS idx_fixtures_pending_seed
    ON fixtures(sport, status, start_time) WHERE status = 'scheduled' OR status = 'completed';
CREATE INDEX IF NOT EXISTS idx_fixtures_sport_date ON fixtures(sport, start_time);
CREATE INDEX IF NOT EXISTS idx_fixtures_league_date ON fixtures(league_id, start_time) WHERE league_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_fixtures_home_team ON fixtures(home_team_id);
CREATE INDEX IF NOT EXISTS idx_fixtures_away_team ON fixtures(away_team_id);
CREATE INDEX IF NOT EXISTS idx_fixtures_season ON fixtures(season);

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
-- 10. PERCENTILE ARCHIVE
-- ============================================================================

CREATE TABLE IF NOT EXISTS percentile_archive (
    id SERIAL PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id INTEGER NOT NULL,
    sport TEXT NOT NULL,
    season INTEGER NOT NULL,
    stat_category TEXT NOT NULL,
    stat_value REAL,
    percentile REAL,
    rank INTEGER,
    sample_size INTEGER,
    comparison_group TEXT,
    calculated_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_final BOOLEAN DEFAULT false,
    UNIQUE(entity_type, entity_id, sport, season, stat_category, archived_at)
);

CREATE INDEX IF NOT EXISTS idx_percentile_archive_sport_season ON percentile_archive(sport, season);
CREATE INDEX IF NOT EXISTS idx_percentile_archive_entity ON percentile_archive(entity_type, entity_id, sport);
CREATE INDEX IF NOT EXISTS idx_percentile_archive_final ON percentile_archive(sport, season, is_final) WHERE is_final = true;

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
-- 14. NOTIFICATION HELPER FUNCTIONS
-- ============================================================================

CREATE OR REPLACE FUNCTION archive_current_percentiles(p_sport TEXT, p_season INTEGER)
RETURNS VOID AS $$
BEGIN
    INSERT INTO percentile_archive (entity_type, entity_id, sport, season, stat_category, percentile, sample_size, calculated_at)
    SELECT 'player', ps.player_id, ps.sport, ps.season, kv.key,
           (kv.value::text)::real,
           COALESCE((ps.percentiles->>'_sample_size')::integer, 0),
           ps.updated_at
    FROM player_stats ps
    CROSS JOIN LATERAL jsonb_each(ps.percentiles) AS kv(key, value)
    WHERE ps.sport = p_sport AND ps.season = p_season
      AND kv.key NOT LIKE '\_%'
      AND jsonb_typeof(kv.value) = 'number'
    ON CONFLICT (entity_type, entity_id, sport, season, stat_category, archived_at) DO NOTHING;

    INSERT INTO percentile_archive (entity_type, entity_id, sport, season, stat_category, percentile, sample_size, calculated_at)
    SELECT 'team', ts.team_id, ts.sport, ts.season, kv.key,
           (kv.value::text)::real,
           COALESCE((ts.percentiles->>'_sample_size')::integer, 0),
           ts.updated_at
    FROM team_stats ts
    CROSS JOIN LATERAL jsonb_each(ts.percentiles) AS kv(key, value)
    WHERE ts.sport = p_sport AND ts.season = p_season
      AND kv.key NOT LIKE '\_%'
      AND jsonb_typeof(kv.value) = 'number'
    ON CONFLICT (entity_type, entity_id, sport, season, stat_category, archived_at) DO NOTHING;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION detect_percentile_changes(p_fixture_id INTEGER)
RETURNS TABLE (
    entity_type TEXT, entity_id INTEGER, sport TEXT, season INTEGER,
    league_id INTEGER, stat_key TEXT, old_percentile REAL, new_percentile REAL,
    sample_size INTEGER
) AS $$
    SELECT 'team'::text, ts.team_id, ts.sport, ts.season, ts.league_id,
           kv.key, pa.percentile, (kv.value::text)::real,
           COALESCE((ts.percentiles->>'_sample_size')::integer, 0)
    FROM fixtures f
    JOIN team_stats ts ON ts.sport = f.sport AND ts.season = f.season
        AND ts.team_id IN (f.home_team_id, f.away_team_id)
    CROSS JOIN LATERAL jsonb_each(ts.percentiles) AS kv(key, value)
    LEFT JOIN LATERAL (
        SELECT pa2.percentile FROM percentile_archive pa2
        WHERE pa2.entity_type = 'team'
          AND pa2.entity_id = ts.team_id AND pa2.sport = ts.sport
          AND pa2.season = ts.season AND pa2.stat_category = kv.key
          AND pa2.is_final = false
        ORDER BY pa2.archived_at DESC LIMIT 1
    ) pa ON true
    WHERE f.id = p_fixture_id
      AND kv.key NOT LIKE '\_%'
      AND jsonb_typeof(kv.value) = 'number'

    UNION ALL

    SELECT 'player'::text, ps.player_id, ps.sport, ps.season, ps.league_id,
           kv.key, pa.percentile, (kv.value::text)::real,
           COALESCE((ps.percentiles->>'_sample_size')::integer, 0)
    FROM fixtures f
    JOIN player_stats ps ON ps.sport = f.sport AND ps.season = f.season
        AND ps.team_id IN (f.home_team_id, f.away_team_id)
    CROSS JOIN LATERAL jsonb_each(ps.percentiles) AS kv(key, value)
    LEFT JOIN LATERAL (
        SELECT pa2.percentile FROM percentile_archive pa2
        WHERE pa2.entity_type = 'player'
          AND pa2.entity_id = ps.player_id AND pa2.sport = ps.sport
          AND pa2.season = ps.season AND pa2.stat_category = kv.key
          AND pa2.is_final = false
        ORDER BY pa2.archived_at DESC LIMIT 1
    ) pa ON true
    WHERE f.id = p_fixture_id
      AND kv.key NOT LIKE '\_%'
      AND jsonb_typeof(kv.value) = 'number';
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 15. LISTEN/NOTIFY — milestone event trigger
-- ============================================================================

CREATE OR REPLACE FUNCTION notify_milestone_reached()
RETURNS TRIGGER AS $$
DECLARE
    pctile_key TEXT;
    pctile_val NUMERIC;
    payload JSONB;
    entity_type TEXT;
    entity_id INTEGER;
BEGIN
    IF TG_TABLE_NAME = 'player_stats' THEN
        entity_type := 'player';
        entity_id := NEW.player_id;
    ELSIF TG_TABLE_NAME = 'team_stats' THEN
        entity_type := 'team';
        entity_id := NEW.team_id;
    ELSE
        RETURN NEW;
    END IF;

    IF NEW.percentiles IS NULL OR NEW.percentiles = '{}'::jsonb THEN
        RETURN NEW;
    END IF;

    FOR pctile_key, pctile_val IN
        SELECT kv.key, (kv.value::text)::numeric
        FROM jsonb_each(NEW.percentiles) AS kv(key, value)
        WHERE kv.key NOT LIKE '\_%'
          AND jsonb_typeof(kv.value) = 'number'
          AND (kv.value::text)::numeric >= 90
    LOOP
        payload := jsonb_build_object(
            'entity_type', entity_type,
            'entity_id', entity_id,
            'sport', NEW.sport,
            'season', NEW.season,
            'stat_key', pctile_key,
            'percentile', pctile_val,
            'ts', extract(epoch from now())::bigint
        );
        PERFORM pg_notify('milestone_reached', payload::text);
    END LOOP;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_milestone_player_stats ON player_stats;
CREATE TRIGGER trg_milestone_player_stats
    AFTER INSERT OR UPDATE OF percentiles ON player_stats
    FOR EACH ROW
    WHEN (NEW.percentiles IS NOT NULL AND NEW.percentiles != '{}'::jsonb)
    EXECUTE FUNCTION notify_milestone_reached();

DROP TRIGGER IF EXISTS trg_milestone_team_stats ON team_stats;
CREATE TRIGGER trg_milestone_team_stats
    AFTER INSERT OR UPDATE OF percentiles ON team_stats
    FOR EACH ROW
    WHEN (NEW.percentiles IS NOT NULL AND NEW.percentiles != '{}'::jsonb)
    EXECUTE FUNCTION notify_milestone_reached();

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
