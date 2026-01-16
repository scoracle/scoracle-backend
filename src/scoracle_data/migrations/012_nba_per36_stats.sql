-- Migration: 012_nba_per36_stats
--
-- Add per-36 minute stats to NBA player stats using PostgreSQL generated columns.
-- Per-36 is the industry standard for normalizing NBA player statistics,
-- allowing fair comparison regardless of minutes played.
--
-- Formula: (raw_stat / minutes_total) * 36
-- Minimum threshold: 100 minutes (NULL below threshold to avoid small sample noise)

ALTER TABLE nba_player_stats
ADD COLUMN points_per_36 REAL GENERATED ALWAYS AS (
    CASE WHEN minutes_total >= 100
    THEN ROUND((points_total::numeric / minutes_total) * 36, 1)
    ELSE NULL END
) STORED,
ADD COLUMN rebounds_per_36 REAL GENERATED ALWAYS AS (
    CASE WHEN minutes_total >= 100
    THEN ROUND((total_rebounds::numeric / minutes_total) * 36, 1)
    ELSE NULL END
) STORED,
ADD COLUMN assists_per_36 REAL GENERATED ALWAYS AS (
    CASE WHEN minutes_total >= 100
    THEN ROUND((assists::numeric / minutes_total) * 36, 1)
    ELSE NULL END
) STORED,
ADD COLUMN steals_per_36 REAL GENERATED ALWAYS AS (
    CASE WHEN minutes_total >= 100
    THEN ROUND((steals::numeric / minutes_total) * 36, 1)
    ELSE NULL END
) STORED,
ADD COLUMN blocks_per_36 REAL GENERATED ALWAYS AS (
    CASE WHEN minutes_total >= 100
    THEN ROUND((blocks::numeric / minutes_total) * 36, 1)
    ELSE NULL END
) STORED,
ADD COLUMN turnovers_per_36 REAL GENERATED ALWAYS AS (
    CASE WHEN minutes_total >= 100
    THEN ROUND((turnovers::numeric / minutes_total) * 36, 1)
    ELSE NULL END
) STORED;
