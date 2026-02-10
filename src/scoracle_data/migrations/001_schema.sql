-- Scoracle Data — Unified Schema v5.0
-- Created: 2026-02-09
-- Purpose: Clean-break schema for fresh Neon database.
--
-- Design principles:
--   - 4 unified tables (players, player_stats, teams, team_stats) shared by all sports
--   - Sport-specific data lives in JSONB (stats, meta) — no schema migrations for new stats
--   - Football-specific `leagues` table for multi-league support and future handicaps
--   - ML tables carried forward from v4.0

-- ============================================================================
-- CORE INFRASTRUCTURE
-- ============================================================================

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO meta (key, value) VALUES
    ('schema_version', '5.0'),
    ('last_full_sync', ''),
    ('last_incremental_sync', '')
ON CONFLICT (key) DO NOTHING;

CREATE TABLE IF NOT EXISTS sports (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    api_base_url TEXT NOT NULL,
    current_season INTEGER NOT NULL,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO sports (id, display_name, api_base_url, current_season) VALUES
    ('NBA', 'NBA Basketball', 'https://api.balldontlie.io/v1', 2025),
    ('NFL', 'NFL Football', 'https://api.balldontlie.io/nfl/v1', 2025),
    ('FOOTBALL', 'Football (Soccer)', 'https://api.sportmonks.com/v3/football', 2025)
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- LEAGUES (Football-specific for now)
-- ============================================================================
-- Stores league metadata for multi-league sports. Currently football only.
-- NFL/NBA have one league each — conference/division data lives in teams.meta.
-- The handicap column supports future league-strength weighting for comparisons.

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

-- Seed the top 5 European football leagues
INSERT INTO leagues (id, sport, name, country, sportmonks_id, is_benchmark) VALUES
    (1, 'FOOTBALL', 'Premier League', 'England', 8, true),
    (2, 'FOOTBALL', 'La Liga', 'Spain', 564, true),
    (3, 'FOOTBALL', 'Bundesliga', 'Germany', 82, true),
    (4, 'FOOTBALL', 'Serie A', 'Italy', 384, true),
    (5, 'FOOTBALL', 'Ligue 1', 'France', 301, true)
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- UNIFIED ENTITY TABLES
-- ============================================================================

-- Players — profile/identity data, shared across all sports.
-- Common columns are typed for fast queries. Sport-specific extras go in meta JSONB.
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
    height_cm INTEGER,
    weight_kg INTEGER,
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

-- Player Stats — seasonal performance data, JSONB for sport-specific stats.
-- Primary key: one row per player per sport per season per league.
-- For NBA/NFL, league_id = 0 (single league).
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
CREATE INDEX IF NOT EXISTS idx_player_stats_gin ON player_stats USING gin(stats);

-- Teams — profile/identity data, shared across all sports.
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
    meta JSONB DEFAULT '{}',
    raw_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, sport)
);

CREATE INDEX IF NOT EXISTS idx_teams_sport ON teams(sport);
CREATE INDEX IF NOT EXISTS idx_teams_name ON teams(name);
CREATE INDEX IF NOT EXISTS idx_teams_league ON teams(league_id) WHERE league_id IS NOT NULL;

-- Team Stats — seasonal standings/performance data, JSONB for sport-specific stats.
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
CREATE INDEX IF NOT EXISTS idx_team_stats_gin ON team_stats USING gin(stats);

-- ============================================================================
-- FIXTURES & SCHEDULING
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
    ON fixtures(sport, status, start_time)
    WHERE status = 'scheduled' OR status = 'completed';
CREATE INDEX IF NOT EXISTS idx_fixtures_sport_date ON fixtures(sport, start_time);
CREATE INDEX IF NOT EXISTS idx_fixtures_league_date ON fixtures(league_id, start_time) WHERE league_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_fixtures_home_team ON fixtures(home_team_id);
CREATE INDEX IF NOT EXISTS idx_fixtures_away_team ON fixtures(away_team_id);
CREATE INDEX IF NOT EXISTS idx_fixtures_season ON fixtures(season);

-- Helper: get fixtures ready for seeding
CREATE OR REPLACE FUNCTION get_pending_fixtures(
    p_sport TEXT DEFAULT NULL,
    p_limit INTEGER DEFAULT 50
)
RETURNS TABLE (
    fixture_id INTEGER,
    sport TEXT,
    league_id INTEGER,
    season INTEGER,
    home_team_id INTEGER,
    away_team_id INTEGER,
    start_time TIMESTAMPTZ,
    seed_delay_hours INTEGER,
    seed_attempts INTEGER
) AS $$
BEGIN
    RETURN QUERY
    SELECT f.id, f.sport, f.league_id, f.season,
           f.home_team_id, f.away_team_id, f.start_time,
           f.seed_delay_hours, f.seed_attempts
    FROM fixtures f
    WHERE (f.status = 'scheduled' OR f.status = 'completed')
      AND NOW() >= f.start_time + (f.seed_delay_hours || ' hours')::INTERVAL
      AND (p_sport IS NULL OR f.sport = p_sport)
    ORDER BY f.start_time ASC
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql;

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

-- ============================================================================
-- PERCENTILE ARCHIVE
-- ============================================================================

CREATE TABLE IF NOT EXISTS percentile_archive (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL,
    entity_id INTEGER NOT NULL,
    sport TEXT NOT NULL,
    season INTEGER NOT NULL,
    stat_category VARCHAR(50) NOT NULL,
    stat_value REAL,
    percentile REAL,
    rank INTEGER,
    sample_size INTEGER,
    comparison_group VARCHAR(100),
    calculated_at INTEGER,
    archived_at INTEGER NOT NULL,
    is_final BOOLEAN DEFAULT false,
    UNIQUE(entity_type, entity_id, sport, season, stat_category, archived_at)
);

CREATE INDEX IF NOT EXISTS idx_percentile_archive_sport_season ON percentile_archive(sport, season);
CREATE INDEX IF NOT EXISTS idx_percentile_archive_entity ON percentile_archive(entity_type, entity_id, sport);
CREATE INDEX IF NOT EXISTS idx_percentile_archive_final ON percentile_archive(sport, season, is_final) WHERE is_final = true;

-- ============================================================================
-- ML TABLES
-- ============================================================================

-- Transfer Predictor
CREATE TABLE IF NOT EXISTS transfer_links (
    id SERIAL PRIMARY KEY,
    player_id INTEGER NOT NULL,
    player_name VARCHAR(255) NOT NULL,
    player_current_team VARCHAR(255),
    team_id INTEGER NOT NULL,
    team_name VARCHAR(255) NOT NULL,
    sport VARCHAR(20) NOT NULL,
    first_linked_at TIMESTAMP NOT NULL DEFAULT NOW(),
    last_mention_at TIMESTAMP NOT NULL DEFAULT NOW(),
    total_mentions INTEGER DEFAULT 0,
    tier_1_mentions INTEGER DEFAULT 0,
    tier_2_mentions INTEGER DEFAULT 0,
    tier_3_mentions INTEGER DEFAULT 0,
    tier_4_mentions INTEGER DEFAULT 0,
    current_probability FLOAT,
    previous_probability FLOAT,
    trend_direction VARCHAR(10) DEFAULT 'stable',
    trend_change_7d FLOAT DEFAULT 0,
    is_active BOOLEAN DEFAULT TRUE,
    transfer_completed BOOLEAN DEFAULT FALSE,
    transfer_completed_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_transfer_links_unique
    ON transfer_links(player_id, team_id, sport) WHERE is_active = TRUE;
CREATE INDEX IF NOT EXISTS idx_transfer_links_sport_active
    ON transfer_links(sport, is_active, current_probability DESC);
CREATE INDEX IF NOT EXISTS idx_transfer_links_team
    ON transfer_links(team_id, is_active, current_probability DESC);

CREATE TABLE IF NOT EXISTS transfer_mentions (
    id SERIAL PRIMARY KEY,
    transfer_link_id INTEGER REFERENCES transfer_links(id) ON DELETE CASCADE,
    source_type VARCHAR(20) NOT NULL,
    source_name VARCHAR(255),
    source_url VARCHAR(1024),
    source_tier INTEGER NOT NULL DEFAULT 4,
    headline TEXT NOT NULL,
    content_snippet TEXT,
    sentiment_score FLOAT,
    mentioned_at TIMESTAMP NOT NULL,
    engagement_score INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_transfer_mentions_link ON transfer_mentions(transfer_link_id, mentioned_at DESC);
CREATE INDEX IF NOT EXISTS idx_transfer_mentions_recent ON transfer_mentions(mentioned_at DESC);

CREATE TABLE IF NOT EXISTS historical_transfers (
    id SERIAL PRIMARY KEY,
    player_id INTEGER,
    player_name VARCHAR(255) NOT NULL,
    from_team_id INTEGER,
    from_team_name VARCHAR(255),
    to_team_id INTEGER,
    to_team_name VARCHAR(255) NOT NULL,
    sport VARCHAR(20) NOT NULL,
    transfer_date DATE NOT NULL,
    fee_millions FLOAT,
    loan_deal BOOLEAN DEFAULT FALSE,
    rumor_duration_days INTEGER,
    peak_mentions INTEGER,
    final_probability FLOAT,
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_historical_transfers_sport ON historical_transfers(sport, transfer_date DESC);

CREATE TABLE IF NOT EXISTS transfer_predictions (
    id SERIAL PRIMARY KEY,
    transfer_link_id INTEGER REFERENCES transfer_links(id) ON DELETE CASCADE,
    predicted_probability FLOAT NOT NULL,
    confidence_lower FLOAT,
    confidence_upper FLOAT,
    model_version VARCHAR(50) NOT NULL,
    feature_importance JSONB,
    features_snapshot JSONB,
    predicted_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_transfer_predictions_link ON transfer_predictions(transfer_link_id, predicted_at DESC);

-- Vibe Score (Sentiment)
CREATE TABLE IF NOT EXISTS vibe_scores (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL,
    entity_id INTEGER NOT NULL,
    entity_name VARCHAR(255) NOT NULL,
    sport VARCHAR(20) NOT NULL,
    overall_score FLOAT NOT NULL,
    twitter_score FLOAT,
    twitter_sample_size INTEGER DEFAULT 0,
    news_score FLOAT,
    news_sample_size INTEGER DEFAULT 0,
    reddit_score FLOAT,
    reddit_sample_size INTEGER DEFAULT 0,
    total_sample_size INTEGER DEFAULT 0,
    positive_themes TEXT[],
    negative_themes TEXT[],
    calculated_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_vibe_scores_entity ON vibe_scores(entity_type, entity_id, calculated_at DESC);
CREATE INDEX IF NOT EXISTS idx_vibe_scores_sport ON vibe_scores(sport, calculated_at DESC);

CREATE TABLE IF NOT EXISTS sentiment_samples (
    id SERIAL PRIMARY KEY,
    vibe_score_id INTEGER REFERENCES vibe_scores(id) ON DELETE CASCADE,
    source_type VARCHAR(20) NOT NULL,
    text_content TEXT NOT NULL,
    sentiment_label VARCHAR(20) NOT NULL,
    sentiment_score FLOAT NOT NULL,
    sentiment_confidence FLOAT,
    engagement_score INTEGER DEFAULT 0,
    analyzed_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_sentiment_samples_vibe ON sentiment_samples(vibe_score_id);

-- Entity Similarity Engine
CREATE TABLE IF NOT EXISTS entity_embeddings (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL,
    entity_id INTEGER NOT NULL,
    entity_name VARCHAR(255) NOT NULL,
    sport VARCHAR(20) NOT NULL,
    season VARCHAR(10),
    position VARCHAR(50),
    embedding FLOAT[] NOT NULL,
    feature_vector FLOAT[],
    feature_names TEXT[],
    computed_at TIMESTAMP DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_entity_embeddings_unique
    ON entity_embeddings(entity_type, entity_id, sport, season);
CREATE INDEX IF NOT EXISTS idx_entity_embeddings_sport
    ON entity_embeddings(sport, entity_type, season);

CREATE TABLE IF NOT EXISTS entity_similarities (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL,
    entity_id INTEGER NOT NULL,
    entity_name VARCHAR(255) NOT NULL,
    similar_entity_id INTEGER NOT NULL,
    similar_entity_name VARCHAR(255) NOT NULL,
    sport VARCHAR(20) NOT NULL,
    season VARCHAR(10),
    similarity_score FLOAT NOT NULL,
    shared_traits TEXT[],
    key_differences TEXT[],
    rank INTEGER NOT NULL,
    computed_at TIMESTAMP DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_entity_similarities_unique
    ON entity_similarities(entity_type, entity_id, similar_entity_id, sport, season);
CREATE INDEX IF NOT EXISTS idx_entity_similarities_lookup
    ON entity_similarities(entity_type, entity_id, sport, rank);

-- Performance Predictor
CREATE TABLE IF NOT EXISTS performance_predictions (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL,
    entity_id INTEGER NOT NULL,
    entity_name VARCHAR(255) NOT NULL,
    opponent_id INTEGER,
    opponent_name VARCHAR(255),
    game_date DATE NOT NULL,
    sport VARCHAR(20) NOT NULL,
    predictions JSONB NOT NULL,
    confidence_intervals JSONB,
    confidence_score FLOAT,
    context_factors JSONB,
    model_version VARCHAR(50) NOT NULL,
    predicted_at TIMESTAMP DEFAULT NOW(),
    actuals JSONB,
    accuracy_score FLOAT,
    evaluated_at TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_performance_predictions_entity
    ON performance_predictions(entity_type, entity_id, game_date DESC);
CREATE INDEX IF NOT EXISTS idx_performance_predictions_date
    ON performance_predictions(sport, game_date);

CREATE TABLE IF NOT EXISTS prediction_accuracy (
    id SERIAL PRIMARY KEY,
    model_type VARCHAR(50) NOT NULL,
    model_version VARCHAR(50) NOT NULL,
    sport VARCHAR(20) NOT NULL,
    entity_type VARCHAR(10),
    stat_name VARCHAR(50),
    mae FLOAT,
    rmse FLOAT,
    mape FLOAT,
    within_range_pct FLOAT,
    sample_size INTEGER NOT NULL,
    period_start DATE,
    period_end DATE,
    calculated_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_prediction_accuracy_model
    ON prediction_accuracy(model_type, model_version, sport);

-- ML Feature Store
CREATE TABLE IF NOT EXISTS ml_features (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL,
    entity_id INTEGER NOT NULL,
    sport VARCHAR(20) NOT NULL,
    feature_set VARCHAR(50) NOT NULL,
    features JSONB NOT NULL,
    computed_at TIMESTAMP DEFAULT NOW(),
    valid_until TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_ml_features_lookup
    ON ml_features(entity_type, entity_id, sport, feature_set);

-- ML Model Metadata
CREATE TABLE IF NOT EXISTS ml_models (
    id SERIAL PRIMARY KEY,
    model_type VARCHAR(50) NOT NULL,
    model_version VARCHAR(50) NOT NULL,
    sport VARCHAR(20),
    description TEXT,
    training_config JSONB,
    metrics JSONB,
    model_path VARCHAR(500),
    is_active BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP DEFAULT NOW(),
    activated_at TIMESTAMP,
    deactivated_at TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_ml_models_active
    ON ml_models(model_type, sport) WHERE is_active = TRUE;

-- ML Job Tracking
CREATE TABLE IF NOT EXISTS ml_job_runs (
    id SERIAL PRIMARY KEY,
    job_name VARCHAR(100) NOT NULL,
    sport VARCHAR(20),
    started_at TIMESTAMP NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMP,
    status VARCHAR(20) DEFAULT 'running',
    items_processed INTEGER DEFAULT 0,
    items_failed INTEGER DEFAULT 0,
    error_message TEXT,
    metrics JSONB
);

CREATE INDEX IF NOT EXISTS idx_ml_job_runs_recent ON ml_job_runs(job_name, started_at DESC);

-- ============================================================================
-- PERCENTILE CALCULATION FUNCTIONS
-- ============================================================================
-- Postgres-native percentile calculation using percent_rank() window functions.
-- Operates directly on JSONB stats — no data round-trip to Python.
--
-- Usage from Python:
--   SELECT recalculate_percentiles('NBA', 2025, ARRAY['turnovers_per_36']::text[]);

CREATE OR REPLACE FUNCTION recalculate_percentiles(
    p_sport TEXT,
    p_season INTEGER,
    p_inverse_stats TEXT[] DEFAULT ARRAY[]::TEXT[]
)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
DECLARE
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
BEGIN
    -- ========================================================================
    -- PLAYER PERCENTILES (partitioned by position for fair comparison)
    -- ========================================================================
    WITH stat_keys AS (
        -- Discover all numeric stat keys present for this sport/season
        SELECT DISTINCT key
        FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season
          AND jsonb_typeof(val) = 'number'
          AND (val::text)::numeric != 0
    ),
    player_positions AS (
        -- Get position for each player (for position-group partitioning)
        SELECT ps.player_id, COALESCE(p.position, 'Unknown') AS position
        FROM player_stats ps
        JOIN players p ON p.id = ps.player_id AND p.sport = ps.sport
        WHERE ps.sport = p_sport AND ps.season = p_season
    ),
    expanded AS (
        -- Expand JSONB stats into rows: one row per player per stat key
        SELECT
            ps.player_id,
            pp.position,
            sk.key AS stat_key,
            (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_stats ps
        CROSS JOIN stat_keys sk
        JOIN player_positions pp ON pp.player_id = ps.player_id
        WHERE ps.sport = p_sport AND ps.season = p_season
          AND ps.stats ? sk.key
          AND (ps.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        -- Calculate percent_rank within each position group per stat
        SELECT
            player_id,
            position,
            stat_key,
            CASE
                WHEN stat_key = ANY(p_inverse_stats) THEN
                    round((1.0 - percent_rank() OVER (
                        PARTITION BY position, stat_key ORDER BY stat_value ASC
                    ))::numeric * 100, 1)
                ELSE
                    round((percent_rank() OVER (
                        PARTITION BY position, stat_key ORDER BY stat_value ASC
                    ))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY position, stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        -- Re-aggregate into one JSONB object per player
        SELECT
            player_id,
            position,
            max(sample_size) AS max_sample_size,
            jsonb_object_agg(stat_key, percentile)
                || jsonb_build_object(
                    '_position_group', position,
                    '_sample_size', max(sample_size)
                ) AS percentiles_json
        FROM ranked
        GROUP BY player_id, position
    )
    UPDATE player_stats ps
    SET percentiles = agg.percentiles_json,
        updated_at = NOW()
    FROM aggregated agg
    WHERE ps.player_id = agg.player_id
      AND ps.sport = p_sport
      AND ps.season = p_season;

    GET DIAGNOSTICS v_players = ROW_COUNT;

    -- ========================================================================
    -- TEAM PERCENTILES (all teams compared together, no position partitioning)
    -- ========================================================================
    WITH stat_keys AS (
        SELECT DISTINCT key
        FROM team_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season
          AND jsonb_typeof(val) = 'number'
          AND (val::text)::numeric != 0
    ),
    expanded AS (
        SELECT
            ts.team_id,
            sk.key AS stat_key,
            (ts.stats->>sk.key)::numeric AS stat_value
        FROM team_stats ts
        CROSS JOIN stat_keys sk
        WHERE ts.sport = p_sport AND ts.season = p_season
          AND ts.stats ? sk.key
          AND (ts.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT
            team_id,
            stat_key,
            CASE
                WHEN stat_key = ANY(p_inverse_stats) THEN
                    round((1.0 - percent_rank() OVER (
                        PARTITION BY stat_key ORDER BY stat_value ASC
                    ))::numeric * 100, 1)
                ELSE
                    round((percent_rank() OVER (
                        PARTITION BY stat_key ORDER BY stat_value ASC
                    ))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT
            team_id,
            jsonb_object_agg(stat_key, percentile)
                || jsonb_build_object('_sample_size', max(sample_size))
            AS percentiles_json
        FROM ranked
        GROUP BY team_id
    )
    UPDATE team_stats ts
    SET percentiles = agg.percentiles_json,
        updated_at = NOW()
    FROM aggregated agg
    WHERE ts.team_id = agg.team_id
      AND ts.sport = p_sport
      AND ts.season = p_season;

    GET DIAGNOSTICS v_teams = ROW_COUNT;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;
