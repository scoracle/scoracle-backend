-- Fixtures Schedule Table
-- Version: 010
-- Purpose: Store match schedules for automated post-match stat seeding
--
-- Workflow:
--   1. At season start, load fixture schedule (CSV/JSON) into this table
--   2. Scheduler checks for fixtures where: start_time + seed_delay <= NOW AND status = 'scheduled'
--   3. Post-match seeder updates stats for players/teams from that fixture
--   4. Status updated to 'seeded' after successful processing

-- ============================================================================
-- FIXTURES TABLE
-- ============================================================================

CREATE TABLE IF NOT EXISTS fixtures (
    id SERIAL PRIMARY KEY,

    -- External ID from API-Sports (if available)
    external_id INTEGER UNIQUE,

    -- Sport and league context
    sport_id TEXT NOT NULL REFERENCES sports(id),
    league_id INTEGER REFERENCES leagues(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),

    -- Match participants
    home_team_id INTEGER NOT NULL REFERENCES teams(id),
    away_team_id INTEGER NOT NULL REFERENCES teams(id),

    -- Scheduling
    start_time TIMESTAMPTZ NOT NULL,
    venue_name TEXT,
    round TEXT,  -- e.g., "Week 5", "Matchday 12", "Playoffs Round 1"

    -- Processing status
    status TEXT NOT NULL DEFAULT 'scheduled'
        CHECK (status IN ('scheduled', 'in_progress', 'completed', 'seeded', 'cancelled', 'postponed')),

    -- Seed configuration
    seed_delay_hours INTEGER NOT NULL DEFAULT 4,  -- Hours after start_time to begin seeding

    -- Processing tracking
    seeded_at TIMESTAMPTZ,
    seed_attempts INTEGER DEFAULT 0,
    last_seed_error TEXT,

    -- Match result (populated after completion)
    home_score INTEGER,
    away_score INTEGER,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT different_teams CHECK (home_team_id != away_team_id)
);

-- ============================================================================
-- INDEXES FOR EFFICIENT QUERIES
-- ============================================================================

-- Primary lookup: Find fixtures ready for seeding
CREATE INDEX IF NOT EXISTS idx_fixtures_pending_seed
    ON fixtures(sport_id, status, start_time)
    WHERE status = 'scheduled' OR status = 'completed';

-- Find fixtures by sport and date range
CREATE INDEX IF NOT EXISTS idx_fixtures_sport_date
    ON fixtures(sport_id, start_time);

-- Find fixtures by league (for Football)
CREATE INDEX IF NOT EXISTS idx_fixtures_league_date
    ON fixtures(league_id, start_time)
    WHERE league_id IS NOT NULL;

-- Find fixtures by team
CREATE INDEX IF NOT EXISTS idx_fixtures_home_team ON fixtures(home_team_id);
CREATE INDEX IF NOT EXISTS idx_fixtures_away_team ON fixtures(away_team_id);

-- Find fixtures by season
CREATE INDEX IF NOT EXISTS idx_fixtures_season ON fixtures(season_id);

-- Find fixtures by external ID (for API-Sports sync)
CREATE INDEX IF NOT EXISTS idx_fixtures_external ON fixtures(external_id) WHERE external_id IS NOT NULL;


-- ============================================================================
-- HELPER FUNCTIONS
-- ============================================================================

-- Function to get fixtures ready for seeding
-- A fixture is ready when:
--   1. Status is 'scheduled' or 'completed' (not yet seeded)
--   2. Current time >= start_time + seed_delay_hours
CREATE OR REPLACE FUNCTION get_pending_fixtures(
    p_sport_id TEXT DEFAULT NULL,
    p_limit INTEGER DEFAULT 50
)
RETURNS TABLE (
    fixture_id INTEGER,
    sport_id TEXT,
    league_id INTEGER,
    season_id INTEGER,
    home_team_id INTEGER,
    away_team_id INTEGER,
    start_time TIMESTAMPTZ,
    seed_delay_hours INTEGER,
    seed_attempts INTEGER
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        f.id,
        f.sport_id,
        f.league_id,
        f.season_id,
        f.home_team_id,
        f.away_team_id,
        f.start_time,
        f.seed_delay_hours,
        f.seed_attempts
    FROM fixtures f
    WHERE (f.status = 'scheduled' OR f.status = 'completed')
      AND NOW() >= f.start_time + (f.seed_delay_hours || ' hours')::INTERVAL
      AND (p_sport_id IS NULL OR f.sport_id = p_sport_id)
    ORDER BY f.start_time ASC
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql;


-- Function to mark a fixture as seeded
CREATE OR REPLACE FUNCTION mark_fixture_seeded(
    p_fixture_id INTEGER,
    p_home_score INTEGER DEFAULT NULL,
    p_away_score INTEGER DEFAULT NULL
)
RETURNS VOID AS $$
BEGIN
    UPDATE fixtures
    SET
        status = 'seeded',
        seeded_at = NOW(),
        home_score = COALESCE(p_home_score, home_score),
        away_score = COALESCE(p_away_score, away_score),
        updated_at = NOW()
    WHERE id = p_fixture_id;
END;
$$ LANGUAGE plpgsql;


-- Function to record a seed failure
CREATE OR REPLACE FUNCTION record_seed_failure(
    p_fixture_id INTEGER,
    p_error_message TEXT
)
RETURNS VOID AS $$
BEGIN
    UPDATE fixtures
    SET
        seed_attempts = seed_attempts + 1,
        last_seed_error = p_error_message,
        updated_at = NOW()
    WHERE id = p_fixture_id;
END;
$$ LANGUAGE plpgsql;


-- ============================================================================
-- SEED SCHEDULE TABLE (for tracking scheduled seed jobs)
-- ============================================================================

CREATE TABLE IF NOT EXISTS seed_schedule (
    id SERIAL PRIMARY KEY,

    -- What to seed
    fixture_id INTEGER REFERENCES fixtures(id) ON DELETE CASCADE,
    sport_id TEXT NOT NULL REFERENCES sports(id),

    -- Scheduling
    scheduled_for TIMESTAMPTZ NOT NULL,

    -- Execution tracking
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    error_message TEXT,

    -- Results
    players_updated INTEGER DEFAULT 0,
    teams_updated INTEGER DEFAULT 0,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_seed_schedule_pending
    ON seed_schedule(scheduled_for)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_seed_schedule_fixture
    ON seed_schedule(fixture_id);


-- ============================================================================
-- VIEWS FOR MONITORING
-- ============================================================================

-- View: Upcoming fixtures to be seeded
CREATE OR REPLACE VIEW v_pending_seeds AS
SELECT
    f.id as fixture_id,
    f.sport_id,
    l.name as league_name,
    ht.name as home_team,
    at.name as away_team,
    f.start_time,
    f.start_time + (f.seed_delay_hours || ' hours')::INTERVAL as seed_eligible_at,
    f.status,
    f.seed_attempts
FROM fixtures f
LEFT JOIN leagues l ON l.id = f.league_id
JOIN teams ht ON ht.id = f.home_team_id
JOIN teams at ON at.id = f.away_team_id
WHERE f.status IN ('scheduled', 'completed')
ORDER BY f.start_time ASC;


-- View: Recent seed activity
CREATE OR REPLACE VIEW v_recent_seeds AS
SELECT
    f.id as fixture_id,
    f.sport_id,
    l.name as league_name,
    ht.name as home_team,
    at.name as away_team,
    f.home_score,
    f.away_score,
    f.seeded_at,
    f.seed_attempts
FROM fixtures f
LEFT JOIN leagues l ON l.id = f.league_id
JOIN teams ht ON ht.id = f.home_team_id
JOIN teams at ON at.id = f.away_team_id
WHERE f.status = 'seeded'
ORDER BY f.seeded_at DESC
LIMIT 50;
