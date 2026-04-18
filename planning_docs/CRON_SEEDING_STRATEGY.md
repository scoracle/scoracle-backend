# Cron Seeding Strategy

How the daily/weekly seed jobs should be scheduled, and why.

## Providers

| Sport  | Provider     | Long-term plan                          |
|--------|--------------|-----------------------------------------|
| NBA    | BallDontLie  | Switch to webhooks (BDL supports them)  |
| NFL    | BallDontLie  | Switch to webhooks                      |
| Football | SportMonks | Cron-driven polling                     |

Once BDL webhooks are wired, NBA + NFL drop out of the cron schedule
entirely. This doc is primarily about the SportMonks polling loop.

## Timing

Run the SportMonks cron at **23:00 ET daily**. Rationale:

| Match slot                           | Finish (ET)       |
|--------------------------------------|-------------------|
| PL Saturday 12:30 UK                 | ~09:30 ET         |
| PL Saturday 15:00 UK                 | ~12:00 ET         |
| PL Saturday 17:30 UK                 | ~14:30 ET         |
| PL late/UCL (20:00 UK / 21:00 CET)   | **~17:00 ET**     |
| PL Monday / Friday 20:00 UK          | **~17:00 ET**     |

SportMonks typically needs 30–60 min after the final whistle before
lineup / event data fully stabilizes. 5pm ET is on the boundary of
late kickoffs and risks locking in partial payloads. 11pm ET gives
~6h of buffer for the latest European kickoff and leaves room for
SportMonks post-processing.

## Split the pipeline

Running `load-fixtures` and `meta seed` daily re-upserts the same
static data and burns SportMonks quota. Do it weekly.

### Daily job (23:00 ET)

```bash
scoracle-seed event process --sport football --season 2025
```

Drains that day's matches. ~10–30 SportMonks calls on a typical
matchday, 0 on a quiet day. Idempotent — safe to re-run.

### Weekly job (23:00 ET Monday)

```bash
scoracle-seed event load-fixtures football --season 2025
scoracle-seed meta seed football --season 2025
```

Catches postponements, schedule reshuffles, and roster changes.
Larger (~400 SportMonks calls) but rare.

`--league` is intentionally omitted on both: the seeder iterates every
league with a `provider_seasons` row, so one cron entry covers all
configured competitions.

## Seed delay safety net

`event load-fixtures` currently passes `seed_delay_hours=0`. That
makes a fixture eligible the moment kickoff starts — any mistimed
cron fire could process mid-match data.

**Recommended:** bump the football default to `seed_delay_hours=3` in
the football branch of `services/event/cli.py`. Even if the cron runs
early, fixtures stay non-eligible for 3h after kickoff. Belt and
suspenders with the 23:00 ET schedule. Not yet implemented — see
follow-up.

## Why `event process` only picks up today's work

`get_pending_fixtures()` (in `sql/shared.sql`) filters on three
conditions:

1. `status IN ('scheduled', 'completed')` — already-`'seeded'`
   fixtures are skipped forever. A successfully-processed fixture is
   done.
2. `NOW() >= start_time + seed_delay_hours` — future fixtures and
   in-progress matches are skipped.
3. `seed_attempts < 3` — failed fixtures retry up to 3 times, then
   stop.

Result: once a match is processed successfully, it's never touched
again. Daily re-runs are cheap — they only work on newly eligible
matches plus any retryable failures.

## BDL webhooks — future

When we move NBA / NFL off the cron:

- BDL supports webhook subscriptions for game completion events.
- The Go API already has a worker runtime (`go/internal/maintenance`).
- Plan: add an HTTP endpoint (`/webhooks/bdl`) that enqueues a fixture
  process job. Drop the NBA + NFL cron entries.

Not in scope for this doc — capturing here so the cron plan is
designed around a SportMonks-only future.

## Follow-ups

- [ ] Add `seed_delay_hours=3` default for football in
      `services/event/cli.py` `load-fixtures` → `FOOTBALL` branch.
- [ ] Write the cron entries (or systemd timers) once the server host
      is chosen.
- [ ] Wire BDL webhooks into the Go API, retire NBA + NFL cron jobs.
- [ ] Fix the `league_id IS NULL` bug in football team/player upserts
      so league-scoped queries work.
