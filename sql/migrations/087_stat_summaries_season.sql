-- 087_stat_summaries_season.sql
--
-- Season-key the stats-rail commentary + add the regenerate signal.
--
-- A stat profile is inherently PER-SEASON (an entity's identity is a season's
-- aggregate). Keying by season lets us:
--   * backfill PREVIOUS seasons once each — they're immutable, so one pass forever;
--   * have the nightly cadence touch ONLY the current season, regenerating an entity
--     when its rating actually moved.
--
-- input_hash is that "did the numbers move?" signal: a hash of input_components
-- (the rating snapshot fed to local model). The nightly pass compares the current hash to
-- the entity-season's last commentary and skips the local model call when unchanged —
-- robust to the spurious player_stats/team_stats row-touches the nightly all-time
-- rank recompute causes (which would make a bare updated_at check regenerate the
-- whole league every night).

BEGIN;

ALTER TABLE stat_summaries ADD COLUMN IF NOT EXISTS season    INTEGER;
ALTER TABLE stat_summaries ADD COLUMN IF NOT EXISTS input_hash TEXT;

-- Reads are latest-generation per (entity, sport, SEASON).
DROP INDEX IF EXISTS idx_stat_summaries_entity_recent;
CREATE INDEX IF NOT EXISTS idx_stat_summaries_entity_recent
    ON stat_summaries(entity_type, entity_id, sport, season, generated_at DESC);

COMMENT ON COLUMN stat_summaries.season IS
    'The season the commentary describes; reads are latest-per-(entity,sport,season). Previous seasons are immutable (backfilled once); the nightly cadence regenerates only the current season, and only when input_hash changes.';
COMMENT ON COLUMN stat_summaries.input_hash IS
    'Hash of input_components (the rating snapshot fed to local model) — the nightly skip-unless-changed signal.';

COMMIT;
