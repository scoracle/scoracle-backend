# 2026-04-24 — Nightly vibe batch cron fix + Railway URL cleanup

## Goals

User noticed the nightly vibe batch hadn't been generating new scores
for several days. Diagnose, fix, and clean up adjacent dead code.

## Decisions

- **Wrap the cron invocation in an env-loading shell script** instead
  of teaching the binary about cron's bare environment. Mirrors the
  existing `cron-scoseed.sh` pattern; one place to look when a cron
  job can't see its config.
- **Drop `RAILWAY_DATABASE_URL` from the resolution chain entirely.**
  We've been fully self-hosted on local Postgres for a while; the
  fallback was misleading (showed up in error messages) and would
  silently let a stale Railway URL win if anyone exported one.

## Accomplishments

### Diagnosis — why the batch wasn't running

`logs/vibe-batch.log` had identical errors at 03:00 ET on
2026-04-21 through 2026-04-24:

```
config load failed
  error="DATABASE_PRIVATE_URL, RAILWAY_DATABASE_URL, or DATABASE_URL must be set"
```

Cron strips the environment to almost nothing — no shell env, no
project `.env`. The binary's `godotenv.Load(".env.local", ".env")`
resolves relative to cron's working directory (`$HOME`, not the repo
root) so it silently missed the file and exited before doing any work.

The crontab fired correctly every night; the binary just couldn't see
its config. (The 84 vibes generated on 2026-04-24 were all from the
real-time milestone path inside the API service, not the batch.)

### Fix

**Commit `afee10e` — wrapper**

- `scripts/hosting/cron-vibe.sh` (new) — `cd` into repo, `source .env`
  + `.env.local` with `set -a`, `exec ./go/bin/vibe "$@"`. Mirrors
  `cron-scoseed.sh` exactly.
- Installed crontab swapped via `crontab -` to point at the wrapper.
- `scripts/hosting/crontab.example` updated so the documented form
  doesn't drift from the live crontab.
- Verified the wrapper survives a fully stripped env
  (`env -i HOME=... PATH=...`) and reaches `batch: all sports complete`
  on a 1-hour test window.

**Commit `69e0b00` — Railway cleanup**

- Resolution chain in `go/internal/config/config.go` and
  `seed/shared/config.py` reduced to `DATABASE_PRIVATE_URL > DATABASE_URL`.
- Error message updated to match.
- `CLAUDE.md`, `.env`, `.env.local` doc comments updated.
- `go test ./internal/config/...` ✓; both binaries rebuild clean.

## Quick reference

**Tomorrow's batch is the proof.** Tail this in the morning:

```
tail -f /home/sheneveld/scoracle-data/logs/vibe-batch.log
```

Expected first successful line will look like:

```
time=2026-04-25T03:00:00... level=INFO msg="batch: starting" sport=FOOTBALL candidates=N
```

**If a cron job fails silently again**, check the log first — cron
exits cleanly even when the wrapped command bails, so the only
evidence is in the configured log file.

## Updated file layout

```
scripts/hosting/cron-vibe.sh           NEW — env-loading wrapper for vibe binary
scripts/hosting/crontab.example        line 51 points at wrapper
go/internal/config/config.go           dropped RAILWAY entry from resolution chain
seed/shared/config.py                  same in Python
CLAUDE.md / .env / .env.local          doc comments updated
```
