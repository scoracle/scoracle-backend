# Session: Seeding Container Separation + Dead Code Cleanup

**Date:** 2026-03-15

## Goals
- Review and revise the seeding container separation plan
- Separate data seeding from the Go API into a dedicated Python service
- Move stat key normalization into Postgres triggers
- Establish clean service boundaries: Python seeds, Postgres normalizes, Go notifies
- Delete legacy FastAPI code
- Clean all dead/unused code left behind by the refactor
- Audit and align schema files with the new architecture

## Decisions Made

| Decision | Rationale |
|----------|-----------|
| Python for seeder (not separate Go binary) | Structural language boundary enforces separation. Python's JSON/HTTP ergonomics reduce boilerplate ~60% for this workload. |
| Fixture-driven seeding model | All seeding triggered by fixture readiness (start_time + delay). No daily full seeds, no in-process scheduler. External cron invokes `scoracle-seed process`. |
| Stat key normalization in Postgres | `provider_stat_mappings` table + `normalize_stat_keys()` trigger. Python inserts raw provider keys. Consistent regardless of inserter. |
| `finalize_fixture()` as single handoff | Python calls one Postgres function when done. Postgres orchestrates percentile recalculation, materialized view refresh, and fixture marking. |
| Enhanced percentile trigger with OLD/NEW | Replaces `notify_milestone_reached()`. Uses OLD vs NEW for delta detection (90/95/99 crossing or delta >= 10). Eliminates `percentile_archive` table from notification flow. |
| Minimal Python dependencies | Only httpx, psycopg[binary,pool], click. No APScheduler, PyYAML, pydantic-settings, or structlog. |
| Frontend handles unit conversion | Heights/weights stored as provider raw values. No backend cm/ft or kg/lbs conversion. |
| Consolidate duplicate `getDeviceTokens()` | Was in both `dispatch.go` and `listener.go`. Now single `GetDeviceTokens()` in `store.go`. |

## Accomplishments

### Created
- `seed/` — Complete Python seeder package (15 modules)
  - `cli.py` — Click CLI: bootstrap-teams, load-fixtures, process, seed-fixture, percentiles
  - `config.py` — Env var loading with DB URL resolution chain
  - `db.py` — psycopg3 connection pool with `psycopg_pool`
  - `models.py` — Canonical dataclasses (Team, Player, PlayerStats, TeamStats, SeedResult)
  - `upsert.py` — INSERT ON CONFLICT functions + `finalize_fixture()` call
  - `fixtures.py` — Fixture schedule management
  - `bdl_client.py` — BDL HTTP client (rate limiting, cursor pagination)
  - `bdl_nba.py` — NBA handler (thin: raw key/value extraction)
  - `bdl_nfl.py` — NFL handler (flat JSON extraction)
  - `sportmonks_client.py` — SportMonks client (rate limiting, 429 backoff)
  - `sportmonks_football.py` — Football handler (squad iteration, nested JSON)
  - `seed_nba.py`, `seed_nfl.py`, `seed_football.py` — Per-sport orchestration
- `seed/Dockerfile` — python:3.13-slim, non-root user, deploy-ready
- `seed/.dockerignore` — Excludes tests, caches, venv
- `seed/railway.toml` — Railway deployment config
- `seed/pyproject.toml` — 3 dependencies: httpx, psycopg[binary,pool], click
- `seed/tests/test_models.py` — Model unit tests

### Updated — SQL
- `sql/shared.sql` — Added:
  - `provider_stat_mappings` table with 30 mapping rows (BDL + SportMonks)
  - `normalize_stat_keys()` trigger function (fires BEFORE INSERT/UPDATE on stats tables)
  - `trg_a_normalize_player_stats` / `trg_a_normalize_team_stats` triggers
  - `upsert_fixture()` function for loading fixture schedules
  - `finalize_fixture()` function (recalc percentiles, refresh per-sport materialized views, mark seeded)
  - `notify_percentile_changed()` trigger replacing `notify_milestone_reached()`
- `sql/shared.sql` — Removed:
  - `percentile_archive` table + 3 indexes
  - `archive_current_percentiles()` function
  - `detect_percentile_changes()` function
  - Section 14 "NOTIFICATION HELPER FUNCTIONS"
- `sql/shared.sql` — Fixed:
  - `finalize_fixture()` now refreshes per-sport materialized views (`nba.autofill_entities`, etc.) instead of non-existent `mv_autofill_entities`

### Updated — Go
- `go/internal/listener/listener.go` — Listens on `percentile_changed` channel (was `milestone_reached`), parses richer payload (old/new percentiles), uses shared `GetDeviceTokens()`
- `go/internal/notifications/notify.go` — Stripped to essentials: 2 constants + `Follower` type. Removed 6 dead constants, 2 dead types, 1 dead variable.
- `go/internal/notifications/store.go` — Removed dead `GetMatchTime()` + `InsertPending()`. Added shared `GetDeviceTokens()`.
- `go/internal/notifications/dispatch.go` — Uses shared `GetDeviceTokens()` instead of private duplicate.
- `go/internal/db/db.go` — Removed dead prepared statements (`player_name_lookup`, `team_name_by_id`), dead `Pool.HealthCheck()` method. Kept 5 active statements.
- `go/internal/config/config.go` — Removed `SportRegistry`, table constants, `BDLAPIKey`, `SportMonksAPIToken`, `Debug` field, `IsProduction()` method.
- `go/internal/cache/cache.go` — Removed 3 dead TTL constants. Kept `TTLNews`.
- `go/internal/api/respond/respond.go` — Removed dead `WriteErrorDetail()`.
- `go/internal/maintenance/maintenance.go` — Removed dead `percentile_archive` cleanup code.
- `docker-compose.yml` — Added seed service with `profiles: ["seed"]`

### Updated — Documentation
- `AGENTS.md` — Three-service architecture, Python seeder CLI commands, replaced legacy_fastapi rule with stat normalization rule
- `CLAUDE.md` — Python seeder section, updated codebase layout, build/run commands, "Adding a New Provider" guide
- `README.md` — Architecture diagram, service roles, CLI commands, data sources, "How Seeding Works" section, directory tree

### Cleaned Up
- Deleted `legacy_fastapi/` — 122 files, 1.6 MB
- Deleted `go/cmd/ingest/` — 313 lines (Cobra CLI)
- Deleted `go/internal/seed/` — 507 lines (orchestration + upserts)
- Deleted `go/internal/provider/` — 1,525 lines (provider clients)
- Deleted `go/internal/fixture/` — 651 lines (fixture processing)
- Deleted `go/internal/notifications/pipeline.go` — 103 lines (dead `Run()` pipeline)
- Deleted `go/internal/notifications/detect.go` — 48 lines (dead `DetectChanges()`)
- Deleted `go/internal/notifications/schedule.go` — 36 lines (dead `ScheduleDelivery()`)
- Deleted `go/internal/maintenance/hooks.go` — 34 lines (dead `RefreshMaterializedViews()`)
- **Total Go code removed: ~3,217 lines**
- **Total legacy Python removed: 122 files**

## Quick Reference

```bash
# Python seeder CLI
scoracle-seed bootstrap-teams nba --season 2025
scoracle-seed load-fixtures nba --season 2025
scoracle-seed process --max 50
scoracle-seed seed-fixture --id 42
scoracle-seed percentiles --sport NBA --season 2025

# Docker
docker compose up --build                          # PostgREST + Go API
docker compose run --rm seed process --max 50      # run seeder
docker compose run --rm seed bootstrap-teams nba --season 2025
```

## File Layout After This Session

```
scoracle-data/
+-- seed/                               # NEW: Python seeder
|   +-- pyproject.toml                  # httpx, psycopg[binary,pool], click
|   +-- Dockerfile                      # python:3.13-slim, deploy-ready
|   +-- .dockerignore
|   +-- railway.toml
|   +-- scoracle_seed/                  # 15 modules
|   |   +-- cli.py                      # Click CLI entry point
|   |   +-- config.py, db.py            # Config + DB pool
|   |   +-- models.py, upsert.py        # Canonical types + SQL upserts
|   |   +-- fixtures.py                 # Fixture schedule management
|   |   +-- bdl_client.py, bdl_nba.py, bdl_nfl.py
|   |   +-- sportmonks_client.py, sportmonks_football.py
|   |   +-- seed_nba.py, seed_nfl.py, seed_football.py
|   +-- tests/
+-- sql/shared.sql                      # UPDATED: normalization, finalize_fixture, new trigger
+-- go/
|   +-- cmd/api/                        # KEPT
|   +-- internal/
|   |   +-- api/                        # KEPT (handler/, respond/, middleware)
|   |   +-- cache/                      # KEPT (slimmed: 1 TTL constant)
|   |   +-- config/                     # UPDATED (slimmed: no sport registry)
|   |   +-- db/                         # UPDATED (5 prepared statements)
|   |   +-- listener/                   # UPDATED (percentile_changed channel)
|   |   +-- maintenance/                # KEPT (1 file: maintenance.go)
|   |   +-- notifications/              # SLIMMED (4 files from 7)
|   |   +-- thirdparty/                 # KEPT
+-- docker-compose.yml                  # UPDATED: seed service added
```
