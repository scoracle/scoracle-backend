-- Migration: 013_ml_tables.sql
-- Description: Creates tables for ML features (Transfer Predictor, Vibe Score, Similarity Engine)
-- Date: 2025-01

-- ============================================================================
-- TRANSFER PREDICTOR TABLES
-- ============================================================================

-- Track transfer links/rumors between players and teams
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
    trend_direction VARCHAR(10) DEFAULT 'stable', -- 'up', 'down', 'stable'
    trend_change_7d FLOAT DEFAULT 0,
    is_active BOOLEAN DEFAULT TRUE,
    transfer_completed BOOLEAN DEFAULT FALSE,
    transfer_completed_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);

-- Unique constraint on player-team-sport combination
CREATE UNIQUE INDEX IF NOT EXISTS idx_transfer_links_unique
ON transfer_links(player_id, team_id, sport)
WHERE is_active = TRUE;

-- Index for active transfers by sport
CREATE INDEX IF NOT EXISTS idx_transfer_links_sport_active
ON transfer_links(sport, is_active, current_probability DESC);

-- Index for team transfer targets
CREATE INDEX IF NOT EXISTS idx_transfer_links_team
ON transfer_links(team_id, is_active, current_probability DESC);

-- Store individual mention events for transfer rumors
CREATE TABLE IF NOT EXISTS transfer_mentions (
    id SERIAL PRIMARY KEY,
    transfer_link_id INTEGER REFERENCES transfer_links(id) ON DELETE CASCADE,
    source_type VARCHAR(20) NOT NULL, -- 'news', 'twitter', 'reddit'
    source_name VARCHAR(255),
    source_url VARCHAR(1024),
    source_tier INTEGER NOT NULL DEFAULT 4,
    headline TEXT NOT NULL,
    content_snippet TEXT,
    sentiment_score FLOAT,
    mentioned_at TIMESTAMP NOT NULL,
    engagement_score INTEGER DEFAULT 0, -- likes, retweets, upvotes
    created_at TIMESTAMP DEFAULT NOW()
);

-- Index for mentions by transfer link
CREATE INDEX IF NOT EXISTS idx_transfer_mentions_link
ON transfer_mentions(transfer_link_id, mentioned_at DESC);

-- Index for recent mentions
CREATE INDEX IF NOT EXISTS idx_transfer_mentions_recent
ON transfer_mentions(mentioned_at DESC);

-- Historical transfers for training data
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

-- Index for historical transfers by sport
CREATE INDEX IF NOT EXISTS idx_historical_transfers_sport
ON historical_transfers(sport, transfer_date DESC);

-- Model predictions log for tracking
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

-- Index for predictions by link
CREATE INDEX IF NOT EXISTS idx_transfer_predictions_link
ON transfer_predictions(transfer_link_id, predicted_at DESC);

-- ============================================================================
-- VIBE SCORE (SENTIMENT) TABLES
-- ============================================================================

-- Store vibe scores over time for entities
CREATE TABLE IF NOT EXISTS vibe_scores (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL, -- 'player' or 'team'
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

-- Index for latest vibe score by entity
CREATE INDEX IF NOT EXISTS idx_vibe_scores_entity
ON vibe_scores(entity_type, entity_id, calculated_at DESC);

-- Index for trending vibes by sport
CREATE INDEX IF NOT EXISTS idx_vibe_scores_sport
ON vibe_scores(sport, calculated_at DESC);

-- Store individual sentiment analyses for auditing
CREATE TABLE IF NOT EXISTS sentiment_samples (
    id SERIAL PRIMARY KEY,
    vibe_score_id INTEGER REFERENCES vibe_scores(id) ON DELETE CASCADE,
    source_type VARCHAR(20) NOT NULL, -- 'twitter', 'news', 'reddit'
    text_content TEXT NOT NULL,
    sentiment_label VARCHAR(20) NOT NULL, -- 'positive', 'negative', 'neutral'
    sentiment_score FLOAT NOT NULL, -- -1.0 to 1.0
    sentiment_confidence FLOAT,
    engagement_score INTEGER DEFAULT 0,
    analyzed_at TIMESTAMP DEFAULT NOW()
);

-- Index for samples by vibe score
CREATE INDEX IF NOT EXISTS idx_sentiment_samples_vibe
ON sentiment_samples(vibe_score_id);

-- ============================================================================
-- ENTITY SIMILARITY ENGINE TABLES
-- ============================================================================

-- Store pre-computed embeddings for entities
CREATE TABLE IF NOT EXISTS entity_embeddings (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL, -- 'player' or 'team'
    entity_id INTEGER NOT NULL,
    entity_name VARCHAR(255) NOT NULL,
    sport VARCHAR(20) NOT NULL,
    season VARCHAR(10),
    position VARCHAR(50), -- For position-specific embeddings
    embedding FLOAT[] NOT NULL, -- 64-dim vector
    feature_vector FLOAT[], -- Original normalized features
    feature_names TEXT[], -- Names of features used
    computed_at TIMESTAMP DEFAULT NOW()
);

-- Unique constraint on entity-season combination
CREATE UNIQUE INDEX IF NOT EXISTS idx_entity_embeddings_unique
ON entity_embeddings(entity_type, entity_id, sport, season);

-- Index for embeddings by sport and type
CREATE INDEX IF NOT EXISTS idx_entity_embeddings_sport
ON entity_embeddings(sport, entity_type, season);

-- Pre-computed similarity pairs for fast lookup
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
    rank INTEGER NOT NULL, -- 1, 2, or 3
    computed_at TIMESTAMP DEFAULT NOW()
);

-- Unique constraint on similarity pairs
CREATE UNIQUE INDEX IF NOT EXISTS idx_entity_similarities_unique
ON entity_similarities(entity_type, entity_id, similar_entity_id, sport, season);

-- Index for fast similarity lookups
CREATE INDEX IF NOT EXISTS idx_entity_similarities_lookup
ON entity_similarities(entity_type, entity_id, sport, rank);

-- ============================================================================
-- PERFORMANCE PREDICTOR TABLES
-- ============================================================================

-- Store performance predictions
CREATE TABLE IF NOT EXISTS performance_predictions (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL, -- 'player' or 'team'
    entity_id INTEGER NOT NULL,
    entity_name VARCHAR(255) NOT NULL,
    opponent_id INTEGER,
    opponent_name VARCHAR(255),
    game_date DATE NOT NULL,
    sport VARCHAR(20) NOT NULL,
    predictions JSONB NOT NULL, -- {"points": 27.5, "rebounds": 8.2, ...}
    confidence_intervals JSONB, -- {"points": [22, 33], ...}
    confidence_score FLOAT,
    context_factors JSONB, -- {"rest_days": 2, "home": true, ...}
    model_version VARCHAR(50) NOT NULL,
    predicted_at TIMESTAMP DEFAULT NOW(),
    -- Actuals (filled in after game)
    actuals JSONB,
    accuracy_score FLOAT,
    evaluated_at TIMESTAMP
);

-- Index for predictions by entity
CREATE INDEX IF NOT EXISTS idx_performance_predictions_entity
ON performance_predictions(entity_type, entity_id, game_date DESC);

-- Index for predictions by game date
CREATE INDEX IF NOT EXISTS idx_performance_predictions_date
ON performance_predictions(sport, game_date);

-- Track model accuracy over time
CREATE TABLE IF NOT EXISTS prediction_accuracy (
    id SERIAL PRIMARY KEY,
    model_type VARCHAR(50) NOT NULL, -- 'transfer', 'performance'
    model_version VARCHAR(50) NOT NULL,
    sport VARCHAR(20) NOT NULL,
    entity_type VARCHAR(10),
    stat_name VARCHAR(50),
    mae FLOAT, -- Mean Absolute Error
    rmse FLOAT, -- Root Mean Squared Error
    mape FLOAT, -- Mean Absolute Percentage Error
    within_range_pct FLOAT, -- % of actuals within predicted range
    sample_size INTEGER NOT NULL,
    period_start DATE,
    period_end DATE,
    calculated_at TIMESTAMP DEFAULT NOW()
);

-- Index for accuracy by model
CREATE INDEX IF NOT EXISTS idx_prediction_accuracy_model
ON prediction_accuracy(model_type, model_version, sport);

-- ============================================================================
-- ML FEATURE STORE
-- ============================================================================

-- Store cached feature vectors for ML models
CREATE TABLE IF NOT EXISTS ml_features (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL,
    entity_id INTEGER NOT NULL,
    sport VARCHAR(20) NOT NULL,
    feature_set VARCHAR(50) NOT NULL, -- 'transfer', 'vibe', 'similarity', 'performance'
    features JSONB NOT NULL,
    computed_at TIMESTAMP DEFAULT NOW(),
    valid_until TIMESTAMP
);

-- Index for feature lookup
CREATE UNIQUE INDEX IF NOT EXISTS idx_ml_features_lookup
ON ml_features(entity_type, entity_id, sport, feature_set);

-- ============================================================================
-- ML MODEL METADATA
-- ============================================================================

-- Track model versions and metadata
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

-- Index for active models
CREATE UNIQUE INDEX IF NOT EXISTS idx_ml_models_active
ON ml_models(model_type, sport)
WHERE is_active = TRUE;

-- ============================================================================
-- SCHEDULED JOB TRACKING
-- ============================================================================

-- Track ML job executions
CREATE TABLE IF NOT EXISTS ml_job_runs (
    id SERIAL PRIMARY KEY,
    job_name VARCHAR(100) NOT NULL,
    sport VARCHAR(20),
    started_at TIMESTAMP NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMP,
    status VARCHAR(20) DEFAULT 'running', -- 'running', 'completed', 'failed'
    items_processed INTEGER DEFAULT 0,
    items_failed INTEGER DEFAULT 0,
    error_message TEXT,
    metrics JSONB
);

-- Index for recent job runs
CREATE INDEX IF NOT EXISTS idx_ml_job_runs_recent
ON ml_job_runs(job_name, started_at DESC);
