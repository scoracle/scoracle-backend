-- 096_rated_enum_covering_index.sql  (Optimization Ledger O4)
--
-- The statcommentary nightly enumerate (go/cmd/statcommentary/main.go enumMissing)
-- finds every (entity, season) carrying a rating but no stat_summaries row:
--
--     SELECT 'player', player_id, season FROM player_stats
--      WHERE sport = $1 AND rating_composite_score IS NOT NULL
--      GROUP BY player_id, season
--
-- It currently bitmap-scans idx_player_stats_position by the sport prefix, then
-- heap-fetches ~4.6k candidate rows out of the 1.9 GB player_stats heap only to
-- drop ~2.7k on the `rating_composite_score IS NOT NULL` filter. The existing
-- idx_player_stats_rating_composite is partial on `rating_composite IS NOT NULL`
-- (a DIFFERENT column — the displayed z, not the magnitude score), so the planner
-- can't prove it covers this predicate and won't use it.
--
-- These covering partial indexes carry exactly (sport, season, id) for the rated
-- rows, so the enumerate (filter + GROUP BY) is satisfied index-only — no heap
-- touch on the big table. Partial keeps them tiny (only rated rows).
--
-- CONCURRENTLY is safe inside the migrate.sh runner: it applies files with
-- `psql -f` (no wrapping transaction), so each statement auto-commits. IF NOT
-- EXISTS makes re-runs idempotent; a failed CONCURRENTLY build leaves an INVALID
-- index that a re-run replaces.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_player_stats_rated_enum
    ON public.player_stats (sport, season, player_id)
    WHERE rating_composite_score IS NOT NULL;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_team_stats_rated_enum
    ON public.team_stats (sport, season, team_id)
    WHERE rating_composite_score IS NOT NULL;
