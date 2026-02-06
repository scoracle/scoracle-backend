# Scoracle-Data Backend Restructure — Summary

## Overview

A multi-phase restructure of the scoracle-data backend covering security hardening,
code consolidation, cache performance, and structural improvements. 12 commits total.

---

## Phase 1: Security Hardening

**Commit:** `205c173`

| Fix | File(s) |
|-----|---------|
| Removed hardcoded DB credentials | `run_migration.py` |
| Deleted `.env.local` with production secrets | `.env.local` |
| SQL injection: regex allowlist on `json_path` | `repositories/postgres.py` |
| SQL injection: `_validate_column_name()` for dynamic stat names (6 call sites) | `percentiles/pg_calculator.py` |
| X-Forwarded-For spoofing: only trust `TRUSTED_PROXY_IPS` | `api/rate_limit.py` |
| Rate limiter OOM: capped storage at 10K entries | `api/rate_limit.py` |
| Info leak: sanitized `/health/db` error output | `api/main.py` |
| Info leak: never expose debug exception detail in production | `api/main.py` |
| SSL: auto-append `sslmode=require` to DB URLs | `pg_connection.py` |
| CORS: restricted `allow_headers` from `["*"]` to specific list | `core/config.py` |
| Type safety: ML endpoints use `EntityType` enum | `api/routers/ml.py` |
| Dead import: removed non-existent `close_async_db` | `api/main.py` |

---

## Phase 2: Consolidate — One True Path

### 2a: Canonical Provider Clients
**Commit:** `5620d93`

Created `src/scoracle_data/providers/`:
- `http.py` — shared `BaseApiClient` with `RateLimiter`, retry logic, async context manager
- `balldontlie_nba.py` — NBA client (BallDontLie API)
- `balldontlie_nfl.py` — NFL client (BallDontLie API)
- `sportmonks.py` — Football client (SportMonks API)

### 2b: Canonical Seeders
**Commit:** `605fb15`

Created `src/scoracle_data/seeders/`:
- `common.py` — shared `SeedResult` dataclass
- `seed_nba.py` — `NBASeedRunner` using `BallDontLieNBA`
- `seed_nfl.py` — `NFLSeedRunner` using `BallDontLieNFL`
- `seed_football.py` — `FootballSeedRunner` using `SportMonksClient`

### 2c: Deleted TOML Config System (646 lines)
**Commit:** `34e662f`

Removed `src/scoracle_data/sports/` directory (3 TOML configs, registry loader).
`SPORT_REGISTRY` in `core/types.py` is the single source of truth.

### 2d: Centralized Table Name Lookups
**Commit:** `21dce19`

Added `STATS_TABLE_MAP` and `PROFILE_TABLE_MAP` to `core/types.py`.
Replaced 21 redundant inline dicts across 12 files. Fixed lowercase key bug in `data_loaders.py`.

### 2e: Dead Code Deletion (~3,574 lines)
**Commit:** `929363f`

Deleted: `providers/base.py`, `providers/api_sports.py` (865 lines),
`seeders/generic_seeder.py` (627 lines), `seeders/small_dataset_seeder.py`,
`repositories/base.py`, `sport_configs/` directory, `packages/scoracle-core/`.
Removed `seed-generic` CLI command.

### 2f: Fixed Cross-Sport ML Queries
**Commit:** `54d4582`

Added `_find_player()` / `_find_team()` helpers in `api/routers/ml.py` that search
sport-specific profile tables instead of deprecated UNION views.

---

## Phase 3: Cache Performance

**Commit:** `178a5dc`

| Fix | Impact |
|-----|--------|
| Cache warming key mismatch (`"info"` vs `"profile"`) | Warmed entries now actually get cache hits |
| MD5 cache keys → structured colon-delimited keys | Debuggable, pattern-matchable invalidation |
| `MAX_ENTRIES=10_000` with LRU eviction | Prevents unbounded memory growth |
| Auto-disable L1 when Redis unavailable | L1 only helps over network hop; pointless alone |

---

## Phase 4: Structural Improvements

### 4a: Extract Service Layer from Routers
**Commit:** `52e163c`

Created `src/scoracle_data/services/`:
- `profiles.py` — `get_player_profile()`, `get_team_profile()` with `psycopg.sql.Identifier`
- `stats.py` — `get_entity_stats()`, `get_available_seasons()` with `psycopg.sql.Identifier`

Routers (`profile.py`, `stats.py`) now delegate to the service layer.
All dynamic table names use parameterized SQL identifiers (no f-strings).

### 4b: Safe Dynamic SQL in Core
**Commit:** `12d3344`

Applied `psycopg.sql.Identifier()` for dynamic table names in:
- `pg_connection.py` (`get_player`, `get_team`, `get_player_stats`, `get_team_stats`)
- `api/routers/stats.py` (`_get_stats`)

### 4c: Migration History Documentation
**Commit:** `f71e1b9`

Created `src/scoracle_data/migrations/MIGRATION_HISTORY.md` documenting numbering
gaps, SQLite-era no-ops, and the active PostgreSQL schema (migrations 007+).

---

## Architecture Decisions

| Decision | Choice | Alternative Rejected |
|----------|--------|---------------------|
| Database | Single Neon DB with sport-prefixed tables | Per-sport databases |
| Providers | Concrete classes per API | Generic `DataProviderProtocol` |
| Config | Python `SPORT_REGISTRY` in `core/types.py` | TOML/YAML files |
| Seeders | Per-sport concrete seed runners | Generic seeder framework |
| Cache keys | Structured `profile:player:123:NBA` | MD5 hashes |
| Dynamic SQL | `psycopg.sql.Identifier()` | f-string table injection |

---

## Remaining Work

### Blocked: Dead Code Sweep (~3,370 lines)

The following files are still imported by legacy CLI seed commands (`cli.py`) and
cannot be deleted until the CLI is rewired to use the new asyncpg-based seed runners:

| File | Lines | Blocked By |
|------|-------|------------|
| `seeders/base.py` | ~1,200 | CLI `cmd_seed` / `cmd_seed_2phase` / `cmd_seed_debug` |
| `seeders/nba_seeder.py` | 606 | CLI `NBASeeder` import |
| `seeders/nfl_seeder.py` | 625 | CLI `NFLSeeder` import |
| `seeders/football_seeder.py` | 939 | CLI `FootballSeeder` import |
| `seeders/utils.py` | 434 | Used by all legacy seeders |
| `query_builder.py` | 368 | Used by all legacy seeders |
| `api_client.py` | 402 | CLI `get_api_client` import |
| `connection.py` | 45 | CLI and scripts `get_db` import |
| `entity_repository.py` | 588 | Package `__init__.py` re-export |

**To unblock:** Rewire CLI seed commands to use `NBASeedRunner` / `NFLSeedRunner` /
`FootballSeedRunner` (requires psycopg → asyncpg migration in the CLI layer).

### Database Cleanup (SQL migrations needed)

- Drop unused materialized views created by migration 009
- Drop deprecated cross-sport UNION views from migration 016
- Drop stale indexes on legacy `players` / `teams` tables

---

## CRITICAL REMINDERS

### 1. Rotate All Neon Database Passwords

Old credentials were committed to git history via `run_migration.py` and `.env.local`.
Even though these files have been modified/deleted, **the passwords are still in git
history** and must be considered compromised.

**Action:** Go to the Neon console and rotate all database passwords immediately.

### 2. Purge Git History

After rotating passwords, purge the old credentials from git history:

```bash
# Install git-filter-repo (if not already installed)
pip install git-filter-repo

# Remove the sensitive files from all history
git filter-repo --invert-paths --path .env.local --path run_migration.py --force

# Force push the cleaned history
git push --force --all
```

**Warning:** This rewrites all commit SHAs. Coordinate with anyone who has cloned
the repo — they will need to re-clone.

### 3. Set `TRUSTED_PROXY_IPS` in Railway

The rate limiter now only trusts `X-Forwarded-For` headers from IPs listed in the
`TRUSTED_PROXY_IPS` environment variable. Without this, all requests will use the
direct connection IP (which is correct for non-proxied setups, but Railway uses a
reverse proxy).

```bash
# In Railway environment variables:
TRUSTED_PROXY_IPS=10.0.0.0/8,172.16.0.0/12
```

---

## Git State

12 commits ahead of `origin/main`. Push manually (HTTPS auth required):

```bash
git push origin main
```
