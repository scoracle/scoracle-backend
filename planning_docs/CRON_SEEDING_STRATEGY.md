# Cron Seeding Strategy

How the daily/weekly seed jobs should be scheduled, and why.

## Providers

| Sport    | Provider     | Current mechanism | Long-term plan                         |
|----------|--------------|-------------------|----------------------------------------|
| NBA      | BallDontLie  | Cron polling      | Switch to webhooks (BDL supports them) |
| NFL      | BallDontLie  | Cron polling      | Switch to webhooks                     |
| Football | SportMonks   | Cron polling      | Keep polling                           |

Until BDL webhooks exist, NBA and NFL must be polled as well. The checked-in
cron wrapper is `scripts/hosting/cron-live-fixtures.sh`; it resolves
`sports.current_season` from Postgres at runtime so season rollover does not
depend on editing crontab.

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

For BallDontLie, use:

- **Schedule refresh:** daily in the morning ET.
- **Completed-event drain:** every 30 minutes, offset NBA and NFL so the
  shared API key is not exercised concurrently.

The wrapper also takes a shared nonblocking BDL lock because the client throttle
is process-local. An overlapping tick exits cleanly instead of multiplying the
request rate. `event process` exits on provider `429` without incrementing
`seed_attempts`, so the next cron tick resumes instead of exhausting retries.

## Split the pipeline

Running every schedule, process, and meta action on the same cadence would
re-upsert too much static data and burn quota. Split them by what actually
changes.

### NBA / NFL daily schedule refresh

```bash
scripts/hosting/cron-live-fixtures.sh nba-refresh
scripts/hosting/cron-live-fixtures.sh nfl-refresh
```

These wrap:

```bash
scoracle-seed event load-fixtures nba --season <sports.current_season> --from-date <yesterday> --to-date <today+10d>
scoracle-seed event load-fixtures nfl --season <sports.current_season> --from-date <today-3d> --to-date <today+21d>
```

The window keeps refresh bounded: no whole-season reload on every daily tick.
At 100 fixtures per page, these windows are typically a single `/games`
request per sport.

### NBA / NFL completed-event drain

```bash
scripts/hosting/cron-live-fixtures.sh nba-process
scripts/hosting/cron-live-fixtures.sh nfl-process
```

These wrap:

```bash
scoracle-seed event process --sport nba --season <sports.current_season> --max 24
scoracle-seed event process --sport nfl --season <sports.current_season> --max 20
```

Expected request volume per eligible fixture:

- NBA: one `/nba/v1/stats` paginated fetch for the game box score.
- NFL: one `/nfl/v1/stats` paginated fetch plus one `/nfl/v1/team_stats`
  fetch for team-only aggregates.

The fixture max caps one tick's BDL volume if backlog accumulates.

### Daily job (23:00 ET)

```bash
scripts/hosting/cron-live-fixtures.sh football-process
```

Drains that day's matches. ~10–30 SportMonks calls on a typical
matchday, 0 on a quiet day. Idempotent — safe to re-run.

### Weekly job (23:00 ET Monday)

```bash
scripts/hosting/cron-live-fixtures.sh football-refresh
scripts/hosting/cron-live-fixtures.sh football-meta
```

Catches postponements, schedule reshuffles, and roster changes.
Larger (~400 SportMonks calls) but rare.

`--league` is intentionally omitted on both: the seeder iterates every
league with a `provider_seasons` row, so one cron entry covers all
configured competitions.

## Seed delay safety net

`event load-fixtures` now stamps a sport-specific `seed_delay_hours`:

- NBA: `4`
- NFL: `6`
- Football: `3`

This is still only a timing guard; Session 4 hardens finality/completeness.
For now it prevents the new polling cadence from burning retries on live games.

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

Result: once a match is processed successfully, it's never touched again.
Repeated polling only works on newly eligible matches plus retryable failures.

## BDL webhooks — future

When we move NBA / NFL off the cron:

- BDL supports webhook subscriptions for game completion events.
- The Go API already has a worker runtime (`go/internal/maintenance`).
- Plan: add an HTTP endpoint (`/webhooks/bdl`) that enqueues a fixture
  process job. Drop the NBA + NFL cron entries.

Not in scope for this doc — capturing here so the cron plan is
designed around a SportMonks-only future.

## Follow-ups

- [ ] Validate the chosen refresh/process cadence against the active BDL plan's
      quota before installation on production.
- [ ] Wire BDL webhooks into the Go API, retire NBA + NFL cron jobs.
- [ ] Fix the `league_id IS NULL` bug in football team/player upserts
      so league-scoped queries work.
