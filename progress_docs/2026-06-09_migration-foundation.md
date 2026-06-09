# 2026-06-09 — Migration tracking + fresh-build foundation (audit keystone + quick wins)

Implements the SQL-engine audit's P0 keystone + P1 quick wins, to make the engine a durable,
multi-frontend-ready foundation and prevent another drift regression (the 049→050 incident).

## What was done
- **`public.schema_migrations` tracking table** — migration `051` creates it and backfills
  every already-applied migration (`001`–`051`, incl. the renamed `042a`). Applied state is now
  known, ending the "did this run?" ambiguity that let canonical drift from prod.
- **`sql/migrate.sh`** — forward runner: applies migrations not in `schema_migrations`, in
  lexical order, recording each; idempotent; guards against running on an un-bootstrapped DB
  (so it never re-replays `001+` over live data).
- **`sql/build.sh`** — fresh-env builder: clones the prod **schema only** (incl.
  `schema_migrations`) via `pg_dump`, the reliable way to stand up sandbox/fantasy.scoracle
  (replaying migrations on an empty DB fails on data-dependent gates like 045/046/048).
- **`sql/migrations/052_leaderboard_indexes.sql`** — composite/partial indexes for the
  leaderboard ORDER BYs (`rating_composite`, `rating_specialist`, and a functional index on
  `(stats->>'fantasy_points')::numeric`) + a `team_stats` rating index. Fine at today's scale;
  ready for multi-frontend traffic.
- **Renamed** the duplicate `042_auth_refresh_tokens.sql` → `042a_auth_refresh_tokens.sql`
  (kept `042_rating_modes.sql`, the one referenced everywhere).
- **Docs** — `sql/README-migrations.md` (conventions) + a CLAUDE.md "Migrations & fresh
  environments" section.

## On "Go startup validation" (also a quick win)
Already satisfied — `db.New` → `Ping` → `AfterConnect` → `conn.Prepare` validates every
statement (columns AND functions) at boot and propagates the error, so `main()` exits. A
restart against a drifted schema fails fast rather than degrading. Documented; no new code.

## Verification (prod)
- 051 applied → `schema_migrations` = 52 rows. `migrate.sh` then applied ONLY `052` (skipping
  the 51 already-recorded) → 53 rows; re-run = 0 applied (idempotent). All four leaderboard
  indexes confirmed present (`idx_player_stats_rating_composite/_specialist/_fantasy_points`,
  `idx_team_stats_rating_composite`).

## Deferred (documented in the audit, by Scott's call)
P2/P3 simplicity refactors (centralizing rate-mode families / legacy aliases / eligibility
thresholds into metadata tables) — worthwhile before a 4th sport, not before.
