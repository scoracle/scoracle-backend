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

-- Meta-only player record. Position lives on player_stats / event_box_scores
-- so stats calculations never touch this table (only the FK ties the two).
CREATE TABLE IF NOT EXISTS players (
    id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    name TEXT NOT NULL,
    first_name TEXT,
    last_name TEXT,
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
    -- Per-stats-row position. Provider-raw string. Owned by the stats pipeline
    -- (finalize_fixture picks most-recent non-null from event_box_scores).
    -- recalculate_percentiles() partitions on this — never touches public.players.
    position TEXT,
    stats JSONB NOT NULL DEFAULT '{}',
    percentiles JSONB DEFAULT '{}',
    -- Percentiles partitioned by (position, league/conference scope) — sibling
    -- to `percentiles` (which is sport-wide, position-only). See
    -- recalculate_percentiles() for population logic.
    scoped_percentiles JSONB DEFAULT '{}',
    raw_response JSONB,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (player_id, sport, season, league_id)
);

ALTER TABLE player_stats ADD COLUMN IF NOT EXISTS scoped_percentiles JSONB DEFAULT '{}';
ALTER TABLE player_stats ADD COLUMN IF NOT EXISTS position TEXT;

CREATE INDEX IF NOT EXISTS idx_player_stats_sport_season ON player_stats(sport, season);
CREATE INDEX IF NOT EXISTS idx_player_stats_team ON player_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_player_stats_league ON player_stats(league_id) WHERE league_id > 0;
CREATE INDEX IF NOT EXISTS idx_player_stats_position ON player_stats(sport, position);

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
    -- Percentiles partitioned by league (Football) or conference (NBA/NFL).
    scoped_percentiles JSONB DEFAULT '{}',
    raw_response JSONB,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (team_id, sport, season, league_id)
);

ALTER TABLE team_stats ADD COLUMN IF NOT EXISTS scoped_percentiles JSONB DEFAULT '{}';

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
    -- migration 054: rate-sibling registry (was the triggers' per_X_keys arrays)
    rate_sibling BOOLEAN NOT NULL DEFAULT false,
    rate_base TEXT,
    UNIQUE(sport, key_name, entity_type)
);
COMMENT ON COLUMN public.stat_definitions.rate_sibling IS
    'TRUE = the derived-stats trigger emits <key or rate_base><rate_modes.suffix> siblings.';
COMMENT ON COLUMN public.stat_definitions.rate_base IS
    'Legacy sibling base name when it differs from key_name (turnover→tov, shots_total→shots).';

CREATE INDEX IF NOT EXISTS idx_stat_definitions_sport ON stat_definitions(sport, entity_type);

-- ============================================================================
-- 7a. ENGINE METADATA (migration 054) — rate-mode families, eligibility
-- thresholds, and the shared sibling emitter. Single source for what was
-- previously duplicated across triggers, rating functions, payload blocks,
-- the notify trigger, and the Go catchUpSweep regex.
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.rate_modes (
    sport        TEXT     NOT NULL REFERENCES sports(id),
    mode         TEXT     NOT NULL,   -- rating_modes / payload-block key ('per_36', ...)
    suffix       TEXT     NOT NULL,   -- sibling stat-key suffix ('_per_36', ...)
    denom_key    TEXT     NOT NULL,   -- stats key of the denominator
    formula      TEXT     NOT NULL CHECK (formula IN ('div_mul', 'mul_div', 'div', 'mul')),
    unit         NUMERIC,             -- 36 / 90; NULL for div|mul
    round_digits SMALLINT NOT NULL,
    PRIMARY KEY (sport, mode)
);
COMMENT ON COLUMN public.rate_modes.formula IS
    'Exact ROUND expression order (numeric division is inexact, so order matters at rounding ties): '
    'div_mul = ROUND(v / denom * unit, d) · mul_div = ROUND(v * unit / denom, d) · '
    'div = ROUND(v / denom, d) · mul = ROUND(v * denom, d)';

INSERT INTO public.rate_modes (sport, mode, suffix, denom_key, formula, unit, round_digits) VALUES
    ('NBA',      'per_36',     '_per_36',     'minutes',        'div_mul', 36,   1),
    ('NBA',      'per_season', '_per_season', 'games_played',   'mul',     NULL, 0),
    ('FOOTBALL', 'per_90',     '_per_90',     'minutes_played', 'mul_div', 90,   3),
    ('FOOTBALL', 'per_game',   '_per_game',   'appearances',    'div',     NULL, 3),
    ('NFL',      'per_game',   '_per_game',   'games_played',   'div',     NULL, 2)
ON CONFLICT (sport, mode) DO NOTHING;

CREATE TABLE IF NOT EXISTS public.rating_thresholds (
    sport     TEXT    NOT NULL REFERENCES sports(id),
    stat_key  TEXT    NOT NULL,
    min_value NUMERIC NOT NULL,
    PRIMARY KEY (sport, stat_key)
);
COMMENT ON TABLE public.rating_thresholds IS
    'Rating eligibility gates: a player enters _compute_rating_bundle only when EVERY row '
    'for their sport passes (COALESCE(stats->>stat_key, 0) >= min_value). A sport with no '
    'rows rates nobody.';

INSERT INTO public.rating_thresholds (sport, stat_key, min_value) VALUES
    ('NBA',      'games_played', 30),
    ('NBA',      'minutes',      20),
    ('FOOTBALL', 'appearances',  15),
    ('NFL',      'games_played',  8)
ON CONFLICT (sport, stat_key) DO NOTHING;

-- Shared sibling emitter, called by each sport's compute_derived_player_stats
-- trigger: for every rate_modes row of the sport, emits
-- <COALESCE(rate_base, key_name)><suffix> for every rate_sibling stat present.
CREATE OR REPLACE FUNCTION public.apply_rate_siblings(p_sport TEXT, p_stats JSONB)
RETURNS JSONB
LANGUAGE plpgsql STABLE
AS $$
DECLARE
    result JSONB := p_stats;
    rm     RECORD;
    sk     RECORD;
    denom  NUMERIC;
    v      NUMERIC;
BEGIN
    FOR rm IN
        SELECT suffix, denom_key, formula, unit, round_digits
        FROM public.rate_modes WHERE sport = p_sport ORDER BY mode
    LOOP
        denom := (p_stats->>rm.denom_key)::NUMERIC;
        IF denom IS NOT NULL AND denom > 0 THEN
            FOR sk IN
                SELECT sd.key_name, COALESCE(sd.rate_base, sd.key_name) || rm.suffix AS out_key
                FROM public.stat_definitions sd
                WHERE sd.sport = p_sport AND sd.entity_type = 'player' AND sd.rate_sibling
            LOOP
                IF p_stats ? sk.key_name THEN
                    v := (p_stats->>sk.key_name)::NUMERIC;
                    IF v IS NOT NULL THEN
                        result := result || jsonb_build_object(sk.out_key,
                            CASE rm.formula
                                WHEN 'div_mul' THEN ROUND(v / denom * rm.unit, rm.round_digits)
                                WHEN 'mul_div' THEN ROUND(v * rm.unit / denom, rm.round_digits)
                                WHEN 'div'     THEN ROUND(v / denom, rm.round_digits)
                                WHEN 'mul'     THEN ROUND(v * denom, rm.round_digits)
                            END);
                    END IF;
                END IF;
            END LOOP;
        END IF;
    END LOOP;
    RETURN result;
END;
$$;

-- No web_anon/web_user grants here: the API connects as the db owner, and the
-- PostgREST roles (section 16) are legacy — nothing reads these tables through them.

-- ============================================================================
-- 7b. STAT KEY NORMALIZATION — relocated to Python seeder handlers
-- ============================================================================
--
-- Provider-specific key normalization (e.g. SportMonks `accurate-passes` ->
-- `passes_accurate`, BDL `tov` -> `turnover`) now lives in the Python
-- handlers under seed/services/event/handlers/ as inline `_*_STAT_MAP`
-- consts passed through shared/stat_keys.py::canonicalize().
--
-- Postgres is provider-agnostic: it receives canonical keys and never sees
-- provider-speak. Aggregators, triggers, and views all read canonical keys
-- directly.
--
-- The DROP statements below clean up the old infrastructure in place so
-- an in-place schema reload removes the trigger, function, and mapping
-- table without manual intervention.

DROP TRIGGER IF EXISTS trg_a_normalize_player_stats     ON player_stats;
DROP TRIGGER IF EXISTS trg_a_normalize_team_stats       ON team_stats;
-- Event tables are declared later in this file but the drop must precede
-- DROP FUNCTION since the triggers hold a dependency on it. IF EXISTS
-- guards a first-run when those tables haven't been created yet.
DO $$
BEGIN
    IF to_regclass('event_box_scores') IS NOT NULL THEN
        EXECUTE 'DROP TRIGGER IF EXISTS trg_a_normalize_event_box_scores ON event_box_scores';
    END IF;
    IF to_regclass('event_team_stats') IS NOT NULL THEN
        EXECUTE 'DROP TRIGGER IF EXISTS trg_a_normalize_event_team_stats ON event_team_stats';
    END IF;
END$$;
DROP FUNCTION IF EXISTS normalize_stat_keys();
DROP TABLE    IF EXISTS provider_stat_mappings;

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
    -- Why the latest provider payload failed the completeness contract
    -- (seed/services/event/completeness.py). Distinct from last_seed_error
    -- (transport/processing failures). Cleared by mark_fixture_seeded().
    last_incomplete_reason TEXT,
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
-- 8b. EVENT BOX SCORE TABLES
-- ============================================================================
--
-- The provider maps that used to head this section are GONE with the paid seeding layer:
-- `provider_entity_map` in mig 222, `provider_fixture_map` + `resolve_provider_fixture_id`
-- in mig 230. Box scores are now derived from public sources discovered into
-- `boxscore_sources`, so a fixture no longer needs a third party's id to be readable.
-- Do not reintroduce them here — a fresh environment that creates a table the ledger then
-- drops is exactly the drift `sql/schema/` exists to prevent.

-- Atomic event rows: one player line per fixture
CREATE TABLE IF NOT EXISTS event_box_scores (
    id BIGSERIAL PRIMARY KEY,
    fixture_id INTEGER NOT NULL REFERENCES fixtures(id),
    player_id INTEGER NOT NULL,
    team_id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    season INTEGER NOT NULL,
    league_id INTEGER NOT NULL DEFAULT 0,
    -- Per-game position raw from the provider's stat embed. finalize_fixture
    -- aggregates this into player_stats.position (most-recent non-null wins).
    position TEXT,
    minutes_played NUMERIC,
    stats JSONB NOT NULL DEFAULT '{}',
    raw_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(fixture_id, player_id)
);

ALTER TABLE event_box_scores ADD COLUMN IF NOT EXISTS position TEXT;

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

-- Event-table normalization triggers previously lived here; removed with
-- the rest of the normalization layer (see section 7b). Python handlers
-- canonicalize keys before upsert.
DROP TRIGGER IF EXISTS trg_a_normalize_event_box_scores ON event_box_scores;
DROP TRIGGER IF EXISTS trg_a_normalize_event_team_stats ON event_team_stats;

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
        last_seed_error = NULL,
        last_incomplete_reason = NULL,
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

-- One-pass whole-season derivation: percentiles + the z-rating engine + per-event
-- starline + event percentiles + autofill matview. Extracted from finalize_fixture's
-- tail so bulk historical backfill can run it ONCE per (sport, season) instead of
-- per fixture (O(M) vs O(M^2)). Idempotent. See migration 092 + DEFERRED_PERCENTILE_BACKFILL.md.
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

-- Finalize a fixture after seeding: per-fixture aggregation + (optional) whole-season
-- recompute via recompute_season() + mark seeded. The single handoff point from the
-- Python seeder to Postgres. p_recompute=FALSE defers the season recompute for bulk
-- historical backfill (caller runs recompute_season once at the end). See migration 092.
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

    -- Whole-season recompute — gated. Default TRUE = live per-fixture freshness;
    -- bulk historical backfill passes FALSE and runs recompute_season() once at the
    -- end (O(M) not O(M^2)). All work is scoped to v_season, so prior (completed)
    -- seasons stay FROZEN until a deliberate recompute. (Lineage: migrations
    -- 017/027/028/029; restored in 050; split into recompute_season in migration 092.)
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

    -- Player percentiles (sport-wide, partitioned by ps.position)
    WITH stat_keys AS (
        SELECT DISTINCT key FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    expanded AS (
        SELECT ps.player_id, COALESCE(ps.position, 'Unknown') AS position,
               sk.key AS stat_key, (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_stats ps CROSS JOIN stat_keys sk
        WHERE ps.sport = p_sport AND ps.season = p_season
          AND ps.stats ? sk.key AND (ps.stats->>sk.key)::numeric != 0
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

    -- ------------------------------------------------------------------
    -- Scoped percentiles (migration 058): MULTI-SCOPE nested {scope: {key: pct}}.
    -- Per-sport cohorts (the LATERAL VALUES are the single source):
    --   players  NFL: position | conference | division | all   (cohorts WITHIN position;
    --                 'all' positionless) · NBA: all | conference · FOOTBALL: all | league
    --   teams    NFL/NBA: conference | division | league(=all, uniform league_id)
    --            FOOTBALL: league (within competition)
    -- The 'all' scope is the positionless baseline the frontend uses for the "All"
    -- option (the template's base pct is within-position). Each player_stats row has
    -- its own league_id in PK → a multi-league player gets distinct scoped values.
    -- ------------------------------------------------------------------

    -- Player scoped percentiles
    WITH stat_keys AS (
        SELECT DISTINCT key FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    player_scope AS (
        SELECT ps.player_id, ps.league_id, sc.scope_name, sc.cohort_key
        FROM player_stats ps
        LEFT JOIN teams t ON t.id = ps.team_id AND t.sport = ps.sport
        CROSS JOIN LATERAL (VALUES
            ('position',   CASE WHEN p_sport='NFL' THEN COALESCE(ps.position,'Unknown') END),
            ('conference', CASE WHEN p_sport IN ('NFL','NBA') THEN COALESCE(ps.position,'Unknown')||'|'||COALESCE(t.conference,'Unknown') END),
            ('division',   CASE WHEN p_sport='NFL' THEN COALESCE(ps.position,'Unknown')||'|'||COALESCE(t.division,'Unknown') END),
            ('league',     CASE WHEN p_sport='FOOTBALL' THEN COALESCE(ps.position,'Unknown')||'|'||ps.league_id::text END),
            ('all',        'ALL')
        ) sc(scope_name, cohort_key)
        WHERE sc.cohort_key IS NOT NULL AND ps.sport = p_sport AND ps.season = p_season
    ),
    expanded AS (
        SELECT psc.player_id, psc.league_id, psc.scope_name, psc.cohort_key,
               sk.key AS stat_key, (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_scope psc
        JOIN player_stats ps ON ps.player_id=psc.player_id AND ps.league_id=psc.league_id
                            AND ps.sport=p_sport AND ps.season=p_season
        CROSS JOIN stat_keys sk
        WHERE ps.stats ? sk.key AND (ps.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT player_id, league_id, scope_name, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY scope_name, cohort_key, stat_key ORDER BY stat_value ASC))::numeric*100,1)
                ELSE round((percent_rank() OVER (PARTITION BY scope_name, cohort_key, stat_key ORDER BY stat_value ASC))::numeric*100,1)
            END AS percentile
        FROM expanded
    ),
    per_scope AS (
        SELECT player_id, league_id, scope_name, jsonb_object_agg(stat_key, percentile) AS scope_pcts
        FROM ranked GROUP BY player_id, league_id, scope_name
    ),
    aggregated AS (
        SELECT player_id, league_id, jsonb_object_agg(scope_name, scope_pcts) AS scoped_json
        FROM per_scope GROUP BY player_id, league_id
    )
    UPDATE player_stats ps SET scoped_percentiles = agg.scoped_json
    FROM aggregated agg WHERE ps.player_id=agg.player_id AND ps.league_id=agg.league_id
                          AND ps.sport=p_sport AND ps.season=p_season;

    -- Team scoped percentiles
    WITH stat_keys AS (
        SELECT DISTINCT key FROM team_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    team_scope AS (
        SELECT ts.team_id, ts.league_id, sc.scope_name, sc.cohort_key
        FROM team_stats ts
        JOIN teams t ON t.id = ts.team_id AND t.sport = ts.sport
        CROSS JOIN LATERAL (VALUES
            ('conference', CASE WHEN p_sport IN ('NFL','NBA') THEN COALESCE(t.conference,'Unknown') END),
            ('division',   CASE WHEN p_sport IN ('NFL','NBA') THEN COALESCE(t.division,'Unknown') END),
            ('league',     ts.league_id::text)
        ) sc(scope_name, cohort_key)
        WHERE sc.cohort_key IS NOT NULL AND ts.sport = p_sport AND ts.season = p_season
    ),
    expanded AS (
        SELECT tsc.team_id, tsc.league_id, tsc.scope_name, tsc.cohort_key,
               sk.key AS stat_key, (ts.stats->>sk.key)::numeric AS stat_value
        FROM team_scope tsc
        JOIN team_stats ts ON ts.team_id=tsc.team_id AND ts.league_id=tsc.league_id
                          AND ts.sport=p_sport AND ts.season=p_season
        CROSS JOIN stat_keys sk
        WHERE ts.stats ? sk.key AND (ts.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT team_id, league_id, scope_name, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY scope_name, cohort_key, stat_key ORDER BY stat_value ASC))::numeric*100,1)
                ELSE round((percent_rank() OVER (PARTITION BY scope_name, cohort_key, stat_key ORDER BY stat_value ASC))::numeric*100,1)
            END AS percentile
        FROM expanded
    ),
    per_scope AS (
        SELECT team_id, league_id, scope_name, jsonb_object_agg(stat_key, percentile) AS scope_pcts
        FROM ranked GROUP BY team_id, league_id, scope_name
    ),
    aggregated AS (
        SELECT team_id, league_id, jsonb_object_agg(scope_name, scope_pcts) AS scoped_json
        FROM per_scope GROUP BY team_id, league_id
    )
    UPDATE team_stats ts SET scoped_percentiles = agg.scoped_json
    FROM aggregated agg WHERE ts.team_id=agg.team_id AND ts.league_id=agg.league_id
                          AND ts.sport=p_sport AND ts.season=p_season;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;



-- fantasy_block — pre-expands fantasy points + percentile rank by rate mode for the
-- sparkline payload (migration 046; mode list from rate_modes since 054). Pure
-- passthrough; NULL when the entity has no fantasy_points. Keys mirror the RateMode
-- vocabulary (default + every registered rate-mode sibling — entities that lack a
-- sibling key simply skip that mode).
CREATE OR REPLACE FUNCTION public.fantasy_block(p_stats jsonb, p_pct jsonb, p_scoped jsonb)
RETURNS jsonb LANGUAGE sql STABLE AS $$
    SELECT CASE WHEN p_stats ? 'fantasy_points' THEN
        jsonb_object_agg(m.mode, m.blk)
    END
    FROM (
        SELECT v.mode,
            CASE WHEN p_stats ? v.key THEN jsonb_build_object(
                'points', (p_stats->>v.key)::numeric,
                'rank',   (p_pct->>v.key)::numeric,
                -- per-cohort fantasy ranks from the nested scoped_percentiles
                -- (migration 058): {scope: pct} for every cohort holding this key.
                'scoped_ranks', (
                    SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>v.key)::numeric)
                                  FILTER (WHERE s.keys ? v.key), '{}'::jsonb)
                    FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
                )
            ) END AS blk
        FROM (
            SELECT 'default'::text AS mode, 'fantasy_points'::text AS key
            UNION
            SELECT DISTINCT rm.mode, 'fantasy_points' || rm.suffix FROM public.rate_modes rm
        ) v
    ) m
    WHERE m.blk IS NOT NULL;
$$;

-- ── Counting-stat templates (migration 047) — per-position counting-stat pizza for
-- NFL offensive skill; NULL elsewhere → frontend z-score pizza fallback. Seeds live in
-- the per-sport file (sql/nfl.sql). ──────────────────────────────────────────────────
-- (rate_base column dropped in 054 — the legacy sibling alias is single-sourced
-- from stat_definitions.rate_base.)
CREATE TABLE IF NOT EXISTS public.stat_templates (
    sport          TEXT    NOT NULL,
    position_group TEXT    NOT NULL,
    stat_key       TEXT    NOT NULL,
    sort_order     INTEGER NOT NULL DEFAULT 0,
    -- Pizza grouping for the Composite card (migration 055). NULL = single
    -- unfaceted pizza (the NFL/NBA fantasy templates); non-NULL = one pizza per
    -- facet, ordered by item sort_order (the football counting-stat pizzas).
    facet          TEXT,
    PRIMARY KEY (sport, position_group, stat_key)
);

-- Raw provider position → template group. NFL offensive skill, NBA (one group),
-- and the four FOOTBALL provider positions map; everything else (NFL defense/OL/
-- special, NULL positions) returns NULL → no template → frontend z-pizza.
CREATE OR REPLACE FUNCTION public.position_group(p_sport TEXT, p_position TEXT)
RETURNS TEXT LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE upper(p_sport)
        WHEN 'NBA' THEN 'ALL'  -- one Fantasy-mode template for all NBA players
        WHEN 'NFL' THEN CASE p_position
            WHEN 'Quarterback' THEN 'quarterback'
            WHEN 'QB'          THEN 'quarterback'
            WHEN 'Running Back' THEN 'running-back'
            WHEN 'Fullback'     THEN 'running-back'
            WHEN 'RB'           THEN 'running-back'
            WHEN 'FB'           THEN 'running-back'
            WHEN 'HB'           THEN 'running-back'
            WHEN 'Wide Receiver' THEN 'receiver'
            WHEN 'Tight End'     THEN 'receiver'
            WHEN 'WR'            THEN 'receiver'
            WHEN 'TE'            THEN 'receiver'
            ELSE NULL
        END
        WHEN 'FOOTBALL' THEN CASE p_position
            WHEN 'Goalkeeper' THEN 'goalkeeper'
            WHEN 'Defender'   THEN 'defender'
            WHEN 'Midfielder' THEN 'midfielder'
            WHEN 'Attacker'   THEN 'attacker'
            ELSE NULL
        END
        ELSE NULL
    END;
$$;

-- Per-position counting-stat template for one player, pre-expanded by rate mode. NULL
-- when the position has no template (→ frontend z-pizza). `value` = raw counting stat
-- (0 when absent); `pct` = within-position percentile (the percentiles column);
-- `scoped_pct` = {scope: pct} per cohort from scoped_percentiles (migration 058 —
-- the slice re-rank); `facet` = pizza grouping (NULL on unfaceted NFL/NBA templates).
CREATE OR REPLACE FUNCTION public.template_block(p_sport TEXT, p_position TEXT, p_stats JSONB, p_pct JSONB, p_scoped JSONB)
RETURNS JSONB LANGUAGE sql STABLE AS $$
    WITH tmpl AS (
        SELECT t.stat_key,
               -- rate modes read the sibling; the sibling base is the legacy alias from
               -- stat_definitions.rate_base when it differs (e.g. turnover → tov).
               COALESCE(sd.rate_base, t.stat_key) AS rate_base,
               COALESCE(sd.display_name, t.stat_key) AS label,
               t.facet,
               t.sort_order
        FROM public.stat_templates t
        LEFT JOIN public.stat_definitions sd
               ON sd.sport = t.sport AND sd.key_name = t.stat_key AND sd.entity_type = 'player'
        WHERE t.sport = upper(p_sport)
          AND t.position_group = public.position_group(p_sport, p_position)
    ),
    modes(mode, suffix) AS (
        SELECT 'default'::text, ''::text
        UNION
        SELECT DISTINCT rm.mode, rm.suffix FROM public.rate_modes rm
    )
    SELECT jsonb_object_agg(m.mode, m.items)
    FROM (
        SELECT md.mode,
            -- value/pct read the mode sibling, falling back to the base key for
            -- mode-invariant stats (percentages have no per-X sibling — they'd
            -- otherwise zero out in per-X modes).
            (SELECT jsonb_agg(jsonb_build_object(
                'key',   t.stat_key,
                'label', t.label,
                'value', COALESCE((p_stats->>(CASE WHEN md.suffix = '' THEN t.stat_key
                                                   ELSE t.rate_base || md.suffix END))::numeric,
                                  (p_stats->>t.stat_key)::numeric, 0),
                'pct',   COALESCE((p_pct  ->>(CASE WHEN md.suffix = '' THEN t.stat_key
                                                   ELSE t.rate_base || md.suffix END))::numeric,
                                  (p_pct  ->>t.stat_key)::numeric, 0),
                'scoped_pct', (
                    SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>(CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END))::numeric)
                                  FILTER (WHERE s.keys ? (CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END)), '{}'::jsonb)
                    FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
                ),
                'facet', t.facet,
                'sort',  t.sort_order
            ) ORDER BY t.sort_order) FROM tmpl t) AS items
        FROM modes md
        -- emit a mode only when its rate sibling actually exists for this sport
        WHERE EXISTS (SELECT 1 FROM tmpl t
                      WHERE p_stats ? (CASE WHEN md.suffix = '' THEN t.stat_key
                                            ELSE t.rate_base || md.suffix END))
    ) m
    WHERE m.items IS NOT NULL;
$$;

-- Generic player datapoints (migration 055) — EVERY percentile-ranked base stat,
-- default rate mode only: the keys of `percentiles` minus rate-mode siblings,
-- labeled/faceted/sorted from stat_definitions (unregistered keys are dropped).
-- scoped_pct = {scope: pct} per cohort (migration 058). Third sibling to
-- fantasy_block/template_block; NULL when no ranked key has a definition.
CREATE OR REPLACE FUNCTION public.datapoints_block(p_sport TEXT, p_stats JSONB, p_pct JSONB, p_scoped JSONB)
RETURNS JSONB LANGUAGE sql STABLE AS $$
    SELECT jsonb_agg(jsonb_build_object(
               'key',        sd.key_name,
               'label',      sd.display_name,
               'value',      COALESCE((p_stats->>sd.key_name)::numeric, 0),
               'pct',        (p_pct->>sd.key_name)::numeric,
               'scoped_pct', (
                   SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>sd.key_name)::numeric)
                                 FILTER (WHERE s.keys ? sd.key_name), '{}'::jsonb)
                   FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
               ),
               'facet',      sd.category,
               'sort',       sd.sort_order
           ) ORDER BY sd.sort_order, sd.key_name)
    FROM jsonb_object_keys(COALESCE(p_pct, '{}'::jsonb)) AS k(key)
    JOIN public.stat_definitions sd
      ON sd.sport = upper(p_sport) AND sd.entity_type = 'player' AND sd.key_name = k.key
    -- default mode only: drop rate-mode sibling keys (goals_per_90, pts_per_36, …)
    WHERE NOT EXISTS (SELECT 1 FROM public.rate_modes rm
                      WHERE rm.sport = upper(p_sport)
                        AND right(k.key, length(rm.suffix)) = rm.suffix);
$$;

-- Team template block (migration 056) — the team sibling of template_block. Teams
-- have no rate modes, so the block is {'default': items} only; the frontend's
-- templateForMode falls back to 'default' for any requested mode. NULL when the
-- sport has no team template rows → z-pizza fallback. scoped_pct = {scope: pct} per
-- team cohort (conference/division/league, migration 058).
CREATE OR REPLACE FUNCTION public.team_template_block(p_sport TEXT, p_stats JSONB, p_pct JSONB, p_scoped JSONB)
RETURNS JSONB LANGUAGE sql STABLE AS $$
    SELECT jsonb_build_object('default', jsonb_agg(jsonb_build_object(
               'key',   t.stat_key,
               'label', COALESCE(sd.display_name, t.stat_key),
               'value', COALESCE((p_stats->>t.stat_key)::numeric, 0),
               'pct',   COALESCE((p_pct  ->>t.stat_key)::numeric, 0),
               'scoped_pct', (
                   SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>t.stat_key)::numeric)
                                 FILTER (WHERE s.keys ? t.stat_key), '{}'::jsonb)
                   FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
               ),
               'facet', t.facet,
               'sort',  t.sort_order
           ) ORDER BY t.sort_order, t.stat_key))
    FROM public.stat_templates t
    LEFT JOIN public.stat_definitions sd
           ON sd.sport = t.sport AND sd.key_name = t.stat_key AND sd.entity_type = 'team'
    WHERE t.sport = upper(p_sport) AND t.position_group = 'team'
    HAVING count(*) > 0;
$$;

-- Team datapoints block (migration 056) — the team sibling of datapoints_block:
-- every percentile-ranked team stat, labeled/faceted/sorted from stat_definitions
-- (entity_type='team'; the percentiles meta keys — _sample_size etc. — have no
-- definition row and drop out). NO rate-sibling exclusion: teams carry no rate
-- siblings, and NFL team base keys legitimately end in '_per_game'. scoped_pct =
-- {scope: pct} per team cohort (conference/division/league, migration 058).
CREATE OR REPLACE FUNCTION public.team_datapoints_block(p_sport TEXT, p_stats JSONB, p_pct JSONB, p_scoped JSONB)
RETURNS JSONB LANGUAGE sql STABLE AS $$
    SELECT jsonb_agg(jsonb_build_object(
               'key',        sd.key_name,
               'label',      sd.display_name,
               'value',      COALESCE((p_stats->>sd.key_name)::numeric, 0),
               'pct',        (p_pct->>sd.key_name)::numeric,
               'scoped_pct', (
                   SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>sd.key_name)::numeric)
                                 FILTER (WHERE s.keys ? sd.key_name), '{}'::jsonb)
                   FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
               ),
               'facet',      sd.category,
               'sort',       sd.sort_order
           ) ORDER BY sd.sort_order, sd.key_name)
    FROM jsonb_object_keys(COALESCE(p_pct, '{}'::jsonb)) AS k(key)
    JOIN public.stat_definitions sd
      ON sd.sport = upper(p_sport) AND sd.entity_type = 'team' AND sd.key_name = k.key;
$$;

GRANT SELECT ON public.stat_templates TO web_anon, web_user;

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
          -- skip per-rate siblings; the suffix family lives in rate_modes
          AND NOT EXISTS (SELECT 1 FROM public.rate_modes rm WHERE key ~ (rm.suffix || '$'))
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
