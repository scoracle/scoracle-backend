-- 016_stat_unit_metadata.sql
--
-- Add unit + comparable metadata to stat_definitions so the trends payload
-- can stop comparing stat values across mismatched units (the bug where
-- football team_stats.tackles=350 — a season cumulative — was being shown
-- next to a peer baseline of ~60 per match).
--
-- unit    — the kind of number a stat key carries:
--             rate_pct          — already a percentage / rate (compares directly)
--             per_game_avg      — per-game (or per-36 / per-90) average
--             cumulative_total  — season cumulative count; needs divisor to compare
--             special           — standings / aggregate column (wins, points,
--                                 home/away splits, games_played) — not a trend stat
--
-- comparable — convenience flag the trends SQL filters on. True iff the value
--   can be compared between an entity's per-fixture recent values and the
--   peer cohort's season-rolled values, either directly (rate_pct, per_game_avg)
--   or via Phase-B normalization (cumulative_total ÷ games_played for TEAMS,
--   where divisor coverage is 100%).
--
--   For PLAYERS the cumulative_total flag stays false, because (a) divisor
--   coverage is uneven (football player_stats has 0% matches_played coverage),
--   and (b) every cumulative key already has a per-game / per-90 derived
--   sibling that the trends payload can use instead.

BEGIN;

ALTER TABLE stat_definitions
    ADD COLUMN IF NOT EXISTS unit TEXT,
    ADD COLUMN IF NOT EXISTS comparable BOOLEAN NOT NULL DEFAULT false;

-- ---------------------------------------------------------------------------
-- Classify each stat by unit (most specific rule first).
-- ---------------------------------------------------------------------------
UPDATE stat_definitions SET unit = CASE
    -- Standings / aggregate keys that don't make sense as a trend stat.
    -- games_played / matches_played are useful as divisors, not as trends.
    WHEN key_name IN (
        'wins', 'losses', 'draws', 'ties', 'points',
        'overall_points', 'win_pct', 'win_loss',
        'point_differential', 'goal_difference',
        'games_played', 'matches_played',
        'position',  -- league table position rank, not a per-match stat
        'home_won', 'home_lost', 'home_draw',
        'away_won', 'away_lost', 'away_draw',
        'home_played', 'away_played',
        'home_points', 'away_points',
        'home_scored', 'away_scored',
        'home_conceded', 'away_conceded',
        'points_for', 'points_against'
    ) THEN 'special'

    -- Explicit rate / percentage / efficiency keys by name suffix.
    WHEN key_name ~ '(_pct|_percentage|_accuracy|_rate)$' THEN 'rate_pct'
    WHEN key_name IN ('qbr', 'qb_rating', 'rating', 'efficiency') THEN 'rate_pct'

    -- Per-X normalized derived stats.
    WHEN key_name ~ '_per_(game|36|90|minute|attempt|carry|reception|target|completion)$'
        THEN 'per_game_avg'

    -- NBA non-derived: BallDontLie provides per-game averages by API contract.
    WHEN sport = 'NBA' AND is_derived = false THEN 'per_game_avg'

    -- NFL / football non-derived: season cumulative totals.
    WHEN sport IN ('NFL', 'FOOTBALL') AND is_derived = false THEN 'cumulative_total'

    -- Catch-all (any remaining derived stat). Treat as per-game avg — safer
    -- default than cumulative since derived stats are usually rates/per-X.
    ELSE 'per_game_avg'
END;

-- ---------------------------------------------------------------------------
-- Derived comparable flag.
-- ---------------------------------------------------------------------------
UPDATE stat_definitions SET comparable = (
    unit IN ('per_game_avg', 'rate_pct')
    OR (unit = 'cumulative_total' AND entity_type = 'team')
);

-- ---------------------------------------------------------------------------
-- Sanity counts — surface coverage in the migration log so it's reviewable.
-- ---------------------------------------------------------------------------
DO $$
DECLARE
    r RECORD;
BEGIN
    FOR r IN
        SELECT sport, entity_type, unit, COUNT(*) AS n,
               COUNT(*) FILTER (WHERE comparable) AS n_comparable
        FROM stat_definitions
        GROUP BY sport, entity_type, unit
        ORDER BY sport, entity_type, unit
    LOOP
        RAISE NOTICE '% / % / % : % (% comparable)',
            r.sport, r.entity_type, r.unit, r.n, r.n_comparable;
    END LOOP;
END$$;

COMMIT;
