# First GPT Audit — Session 3: Live NBA and NFL ingestion

**Worked:** 2026-06-21 (archbox)

**Plan:** `planning_docs/FIRST-GPT-AUDIT.md`, Session 3

**Baseline:** Session 2 (`progress_docs/2026-06-21_first-gpt-audit-session-2-service-paths-release-health.md`)

**Product authority:** wiki `Product Narrative`

## Goal

Make live NBA and NFL ingestion durable without waiting for a webhook system that does not exist
yet, while removing season-rollover edits from the cron path and keeping polling bounded enough to
respect provider quotas.

## Decisions

1. **Polling is the immediate durable path.** BallDontLie webhook support remains optional future
   work; the live system now has cron-driven NBA/NFL ingestion that does not depend on webhooks.
2. **Current season is resolved at runtime from Postgres.** Instead of teaching every seeder CLI
   command a special `"current"` season value, the hosting wrapper asks `sports.current_season`
   directly and injects the concrete year into the existing commands.
3. **Bound schedule refresh; leave completed-fixture drain backlog-capable.** Refresh jobs only
   reload a near-term date window; `event process` still works oldest-pending-first inside the live
   season so missed completed fixtures are eventually drained.
4. **Use seed-delay safety rails until finality hardening lands.** Session 4 will make completeness
   explicit. Until then, fixture rows now carry sport-specific delay hours so the new frequent cron
   runs do not immediately hit in-progress games.

## What changed

### Hosting wrapper (`scripts/hosting/cron-live-fixtures.sh`, new)

- Loads the repo venv + `.env`/`.env.local`, just like the existing cron wrappers.
- Resolves `sports.current_season` through `psql` at runtime.
- Rejects missing or malformed season values before invoking the seeder.
- Serializes NBA/NFL jobs with a shared nonblocking lock because the BDL client throttle is
  process-local.
- Adds one checked-in entry point for:
  - `nba-refresh`
  - `nba-process`
  - `nfl-refresh`
  - `nfl-process`
  - `football-refresh`
  - `football-process`
  - `football-meta`
- Bounds BDL schedule refresh windows:
  - NBA: `today-1d .. today+10d`
  - NFL: `today-3d .. today+21d`
- Caps one drain tick's current-season backlog:
  - NBA: `--max 24`
  - NFL: `--max 20`
- Supports env overrides for verification/tuning:
  - `SCORACLE_TODAY`
  - `SCORACLE_PSQL_BIN`
  - `SCORACLE_SEED_BIN`
  - `NBA_*` / `NFL_*` window and max vars

### Seeder safety (`seed/services/event/cli.py`)

- Replaced hardcoded `seed_delay_hours=0` fixture loads with sport-specific defaults:
  - NBA: `4`
  - NFL: `6`
  - Football: `3`
- This keeps `get_pending_fixtures()` from making a just-started live event eligible for the new
  30-minute polling cadence.

### Cron + docs

- `scripts/hosting/crontab.example`
  - adds daily bounded refresh jobs for NBA and NFL;
  - adds 30-minute completed-fixture drains for NBA and NFL, offset to avoid concurrent BDL runs;
  - uses `CRON_TZ` for schedule interpretation and `TZ` for job-local date calculations;
  - moves football cron lines onto the same current-season wrapper, removing the hardcoded `2025`.
- `scripts/hosting/README.md`
  - documents the new wrapper and the new log files.
- `planning_docs/CRON_SEEDING_STRATEGY.md`
  - updated from the old "NBA/NFL intentionally absent from cron" model to the actual polling
    model now in repo, including expected BDL request patterns and rate-limit behavior.

## Expected call volume / rate-limit behavior

- **BDL client throttle:** `600 req/min` in `seed/shared/bdl_client.py`.
- **Cross-process safety:** all NBA/NFL wrapper jobs share one nonblocking `flock`; an overlapping
  cron tick logs a skip and leaves pending work for the next tick.
- **NBA refresh:** typically one `/nba/v1/games` page for the 12-day window.
- **NFL refresh:** typically one `/nfl/v1/games` page for the 24-day window.
- **NBA process:** one paginated `/nba/v1/stats` fetch per eligible fixture.
- **NFL process:** one paginated `/nfl/v1/stats` fetch plus one `/nfl/v1/team_stats` fetch per
  eligible fixture.
- On a provider `429`, `event process` exits with code `2` and does **not** increment
  `seed_attempts`, so the next cron tick resumes rather than burning retries.

## Verification

- `pytest seed/tests/test_event_cli.py seed/tests/test_event_bdl_rate_limits.py -q` — passes;
  covers the new sport-specific seed-delay mapping and `429` propagation through NBA/NFL schedule
  and box-score paths.
- Wrapper dry-runs with stubbed `psql` + `scoracle-seed` confirm the exact commands emitted for:
  - `nba-refresh`
  - `nfl-process`
  - `football-meta`
- `bash -n` on `scripts/hosting/cron-live-fixtures.sh` and `crontab.example`-adjacent review.

## Not verified live in this session

- Did not install the new crontab on the host.
- Did not run live provider calls against BDL or SportMonks.
- A direct in-sandbox `psql` check for `sports.current_season` did not return a usable connection,
  so production DB validation remains to be done on the host when the cron is installed.

## To deploy

```bash
crontab scripts/hosting/crontab.example
```

Then tail the first ticks:

```bash
tail -f logs/cron-nba.log
tail -f logs/cron-nfl.log
tail -f logs/cron-football.log
```

## Files changed

- `scripts/hosting/cron-live-fixtures.sh` (new)
- `scripts/hosting/crontab.example`
- `scripts/hosting/README.md`
- `planning_docs/CRON_SEEDING_STRATEGY.md`
- `seed/services/event/cli.py`
- `seed/tests/test_event_cli.py` (new)
- `progress_docs/2026-06-21_first-gpt-audit-session-3-live-nba-nfl-ingestion.md` (this doc)

---

## Finalization — deploy + live verification (2026-06-21, archbox)

The code above shipped in commit `131274d` (entangled with Session 4 on the
shared seeder files). This addendum closes the "not verified live / not deployed"
gaps the original session left open.

### crontab.example regression fixed before deploy

The rewritten `crontab.example` had silently **dropped the nightly Sigil job**
(`cron-vibesynth.sh -mode nightly -limit 150 -throttle-ms 250`, 05:00 ET) that
was present in the live crontab — installing it as-was would have halted the
Sigil backlog drain. Re-added it with a note that audit **Session 12** will later
convert that nightly run into reconciliation/backfill-only. The installed
crontab is now an **exact match** with `crontab.example`, and the example is a
strict superset of the prior live jobs (football's three hardcoded-`2025`
`cron-scoseed.sh` lines replaced by the current-season-aware wrapper; NBA/NFL
polling added; backup/pipeline/statcommentary/tiers/vibesynth all preserved).

### Live verification (the steps the original session skipped)

- **`sports.current_season` resolves live** — NBA/NFL/FOOTBALL all `2025` via the
  wrapper's `psql` path (the in-sandbox check that previously failed).
- **End-to-end `nba-refresh` proven against live BDL** — wrapper → venv → env →
  season resolution → flock → seeder → provider → DB. Bounded window
  `2026-06-20..2026-07-01`; both the primary `seasons[]` and fallback `season`
  param paths returned HTTP 200; `Loaded 0 NBA fixtures` (correct — June
  offseason); idempotent (fixtures NBA/2025 count `1293 → 1293`); exit 0.
- **Wrapper dry-run** of all 7 jobs emits the expected bounded commands.
- **Tests:** `test_event_cli.py` + `test_event_bdl_rate_limits.py` pass; full
  `seed/tests/` suite 40 passed.
- `bash -n` clean on the wrapper; wrapper is executable in git (`100755`);
  `psql`/`flock`/`date` all on the standard cron PATH.

### Deployed

- Backed up the previous crontab to `logs/crontab.backup-20260622T020109Z`
  (90 lines; cronie also wrote `~/.cache/crontab/crontab.bak`).
- Installed `crontab scripts/hosting/crontab.example`; verified `crontab -l`
  matches the example exactly. NBA daily refresh 08:07 ET + 30-min drains, NFL
  daily refresh 08:12 ET + `:15/:45` drains are now live; the BDL flock
  serializes them.

### Left untouched (deliberate)

- `sql/migrations/099_team_rosters.sql` remains untracked — a parallel-session
  artifact (see Session 4 doc); not part of Session 3 and intentionally not
  applied via the bulk runner here.

**Session 3 status: complete and deployed.**
