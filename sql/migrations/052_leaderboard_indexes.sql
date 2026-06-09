-- ============================================================================
-- 052_leaderboard_indexes.sql
-- Indexes for the leaderboard ORDER BYs (audit P1). The /leaderboard statement sorts a
-- season's players/teams by rating_composite, rating_specialist, or fantasy_points — none
-- of which were indexed. Fine at today's scale (a few thousand rows/season after the
-- (sport,season) filter), but these make the boards index-ordered ahead of multi-frontend
-- traffic. Plain CREATE INDEX (txn-safe, instant on small tables) rather than CONCURRENTLY.
--
-- Apply with: sql/migrate.sh  (or psql -f ...)
-- ============================================================================

BEGIN;

CREATE INDEX IF NOT EXISTS idx_player_stats_rating_composite
    ON public.player_stats (sport, season, rating_composite DESC)
    WHERE rating_composite IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_player_stats_rating_specialist
    ON public.player_stats (sport, season, rating_specialist DESC)
    WHERE rating_specialist IS NOT NULL;

-- Functional index for the Fantasy board (ORDER BY (stats->>'fantasy_points')::numeric).
CREATE INDEX IF NOT EXISTS idx_player_stats_fantasy_points
    ON public.player_stats (sport, season, ((stats->>'fantasy_points')::numeric) DESC)
    WHERE (stats->>'fantasy_points') IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_team_stats_rating_composite
    ON public.team_stats (sport, season, rating_composite DESC)
    WHERE rating_composite IS NOT NULL;

INSERT INTO public.schema_migrations (version) VALUES ('052_leaderboard_indexes')
ON CONFLICT (version) DO NOTHING;

COMMIT;
