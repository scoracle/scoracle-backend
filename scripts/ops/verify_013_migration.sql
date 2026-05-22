-- Verify migration 013_stats_owned_position.sql
-- Run on archbox: psql "$DATABASE_PRIVATE_URL" -f scripts/ops/verify_013_migration.sql
-- Each section prints its own header. All checks should produce non-empty,
-- non-error output.

\echo
\echo === 1. Schema: players should NOT have position/detailed_position ===
SELECT column_name
FROM information_schema.columns
WHERE table_schema = 'public' AND table_name = 'players'
  AND column_name IN ('position', 'detailed_position');
-- Expected: 0 rows.

\echo
\echo === 2. Schema: player_stats + event_box_scores should HAVE position ===
SELECT table_name, column_name, data_type
FROM information_schema.columns
WHERE table_schema = 'public'
  AND table_name IN ('player_stats', 'event_box_scores')
  AND column_name = 'position';
-- Expected: 2 rows.

\echo
\echo === 3. Backfill: player_stats.position distribution per sport ===
SELECT sport,
       COUNT(*) AS total_rows,
       COUNT(position) AS rows_with_position,
       COUNT(DISTINCT position) AS distinct_positions
FROM player_stats
GROUP BY sport
ORDER BY sport;
-- Expected: NBA + FOOTBALL have non-zero rows_with_position from the
-- one-shot UPDATE in the migration. NFL may show 0 if no NBA/FOOT-style
-- backfill applies, depending on what players.position held pre-migration.

\echo
\echo === 4. Position distribution sample (top 15 NBA, top 6 football) ===
(SELECT sport, position, COUNT(*) AS players
 FROM player_stats WHERE sport = 'NBA' AND position IS NOT NULL
 GROUP BY sport, position ORDER BY players DESC LIMIT 15)
UNION ALL
(SELECT sport, position, COUNT(*) AS players
 FROM player_stats WHERE sport = 'FOOTBALL' AND position IS NOT NULL
 GROUP BY sport, position ORDER BY players DESC LIMIT 6);

\echo
\echo === 5. Percentiles: _position_group matches ps.position ===
SELECT sport,
       COUNT(*) FILTER (WHERE percentiles->>'_position_group' = COALESCE(position, 'Unknown')) AS matching,
       COUNT(*) FILTER (WHERE percentiles->>'_position_group' IS NOT NULL
                          AND percentiles->>'_position_group' <> COALESCE(position, 'Unknown')) AS mismatched,
       COUNT(*) FILTER (WHERE percentiles ? '_position_group') AS rows_with_pg,
       COUNT(*) AS total
FROM player_stats
GROUP BY sport
ORDER BY sport;
-- Expected: mismatched = 0 for every sport.

\echo
\echo === 6. Ausar Thompson (56677826) — should be in NBA with a position ===
SELECT player_id, season, league_id, team_id, position,
       percentiles->>'_position_group' AS pg,
       percentiles->>'_sample_size'    AS sample_size
FROM player_stats
WHERE player_id = 56677826 AND sport = 'NBA'
ORDER BY season DESC
LIMIT 3;
-- Expected: position should be a raw BDL code (e.g. 'G-F', 'SF', 'SG').
-- _position_group should equal position.

\echo
\echo === 7. Functions exist and were recreated ===
SELECT n.nspname AS schema, p.proname AS function
FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
WHERE p.proname IN ('recalculate_percentiles', 'finalize_fixture', 'stat_leaders')
ORDER BY n.nspname, p.proname;
-- Expected: recalculate_percentiles (public), finalize_fixture (public),
-- stat_leaders (nba/nfl/football).

\echo
\echo === 8. View shape: nba.player columns (should be stats-only) ===
SELECT column_name, data_type
FROM information_schema.columns
WHERE table_schema = 'nba' AND table_name = 'player'
ORDER BY ordinal_position;
-- Expected: id, season, league_id, team_id, position, stats, percentiles,
-- percentile_metadata, scoped_percentiles, scoped_percentile_metadata,
-- stats_updated_at. No name/first_name/last_name/etc.

\echo
\echo === 9. nba.stat_leaders smoke test — top 5 scorers, current season ===
SELECT * FROM nba.stat_leaders(
    p_season => (SELECT current_season FROM sports WHERE id = 'NBA'),
    p_stat_name => 'pts',
    p_limit => 5
);
-- Expected: 5 rows with rank, player_id, position, team_name, stat_value.

\echo
\echo === 10. Materialized views exist (autofill) ===
SELECT schemaname, matviewname
FROM pg_matviews
WHERE schemaname IN ('nba', 'nfl', 'football')
  AND matviewname = 'autofill_entities'
ORDER BY schemaname;
-- Expected: 3 rows.

\echo
\echo === Done. ===
