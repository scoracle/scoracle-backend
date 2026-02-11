-- Scoracle Data — Provider Decoupling v5.5
-- Created: 2026-02-10
-- Purpose: Move provider-specific season ID mappings from hardcoded Python
--          dicts into the database. After this migration, no Python code needs
--          to import PREMIER_LEAGUE_SEASONS or LEAGUES dicts — the DB is the
--          single source of truth for provider IDs.
--
-- The sportmonks_id column on leagues already exists (001_schema.sql:56).
-- This migration adds:
--   1. provider_seasons table: maps (league_id, season_year) -> provider_season_id
--   2. Seed data for Premier League seasons (from PREMIER_LEAGUE_SEASONS dict)
--   3. Helper function to look up provider season ID

-- ============================================================================
-- 1. PROVIDER SEASONS TABLE
-- ============================================================================
-- Maps internal (league_id, season_year) to external provider season IDs.
-- Currently only SportMonks requires this (BDL uses plain year integers).

CREATE TABLE IF NOT EXISTS provider_seasons (
    id SERIAL PRIMARY KEY,
    league_id INTEGER NOT NULL REFERENCES leagues(id),
    season_year INTEGER NOT NULL,
    provider TEXT NOT NULL DEFAULT 'sportmonks',
    provider_season_id INTEGER NOT NULL,
    UNIQUE(league_id, season_year, provider)
);

CREATE INDEX IF NOT EXISTS idx_provider_seasons_lookup
    ON provider_seasons(league_id, season_year);

-- ============================================================================
-- 2. SEED PREMIER LEAGUE SEASONS
-- ============================================================================
-- Data previously hardcoded as PREMIER_LEAGUE_SEASONS in seed_football.py.

INSERT INTO provider_seasons (league_id, season_year, provider, provider_season_id) VALUES
    (1, 2020, 'sportmonks', 17420),
    (1, 2021, 'sportmonks', 18378),
    (1, 2022, 'sportmonks', 19734),
    (1, 2023, 'sportmonks', 21646),
    (1, 2024, 'sportmonks', 23614),
    (1, 2025, 'sportmonks', 25583)
ON CONFLICT (league_id, season_year, provider) DO NOTHING;

-- ============================================================================
-- 3. HELPER FUNCTION: resolve_provider_season_id
-- ============================================================================
-- Replaces _resolve_sportmonks_season_id() in post_match_seeder.py and
-- PREMIER_LEAGUE_SEASONS dict lookups in cli.py.

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
-- 4. DROP PROVIDER-SPECIFIC COLUMN FROM SPORTS TABLE
-- ============================================================================
-- api_base_url is provider-specific and belongs in the provider client code,
-- not the core schema. The column was never queried by any application code.

ALTER TABLE sports ALTER COLUMN api_base_url DROP NOT NULL;
ALTER TABLE sports ALTER COLUMN api_base_url SET DEFAULT NULL;

-- ============================================================================
-- 5. SCHEMA VERSION BUMP
-- ============================================================================

UPDATE meta SET value = '5.5', updated_at = NOW() WHERE key = 'schema_version';
