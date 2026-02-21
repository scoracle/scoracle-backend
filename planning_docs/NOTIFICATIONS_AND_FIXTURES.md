# Notifications & Post-Match Fixture Seeder

**Last Updated:** 2026-02-20
**Status:** Implemented (Phase 1)

---

## Overview

Two new Go packages implement post-match fixture seeding and percentile-based push notifications:

- **`internal/fixture/`** — Detects completed fixtures, refreshes stats from upstream providers, and recalculates percentiles.
- **`internal/notifications/`** — Detects significant percentile changes after seeding and schedules timezone-aware push notifications to entity followers via FCM.

Ported from Python `PostMatchSeeder` + `SchedulerService`.

---

## Architecture

```
scoracle-ingest fixtures process
        │
        ▼
┌─────────────────────┐
│  fixture.ProcessPending  │ ── groups by (sport, season, league_id)
│  (channel worker pool)   │    N groups × M workers
└──────────┬──────────┘
           │ per group
           ▼
┌─────────────────────┐
│  fixture.SeedFixture     │ ── calls provider handler → seed.Upsert*
│                          │ ── seed.ArchivePercentiles
│                          │ ── seed.RecalculatePercentiles
└──────────┬──────────┘
           │ on success
           ▼
┌─────────────────────┐
│  notifications.Run       │ ── detect → fan-out → schedule → persist
│  (sequential pipeline)   │
└─────────────────────┘

                    ╔═══════════════════════╗
  cmd/api server ─► ║ notifications.StartWorker ║ ── 30s ticker
                    ║ (background goroutine)    ║    claims due → FCM send
                    ╚═══════════════════════╝
```

---

## File Map

### `internal/fixture/`

| File | Purpose |
|------|---------|
| `fixture.go` | Package doc, constants (`defaultMaxFixtures=50`, `defaultMaxRetries=3`), types (`Row`, `Result`, `SchedulerResult`) |
| `query.go` | DB queries: `GetPending`, `GetByID`, `MarkSeeded`, `RecordFailure` |
| `seed.go` | `SeedFixture` orchestrator + per-sport seeders (`seedNBAFixture`, `seedNFLFixture`, `seedFootballFixture`). Calls provider handlers + `seed.Upsert*` directly. Invokes `notifications.Run()` after percentile recalculation. |
| `scheduler.go` | `ProcessPending` with channel-based worker pool, grouping fixtures by `(sport, season, league_id)` |

### `internal/notifications/`

| File | Purpose |
|------|---------|
| `notify.go` | Package doc, constants, types (`Change`, `Follower`, `Pending`) |
| `detect.go` | `DetectChanges()` — reads percentile archive via prepared statement, `isSignificant()` checks milestone crossings + large deltas |
| `schedule.go` | `ScheduleDelivery()` — pure function with timezone-aware random jitter, `isWakingHour()` |
| `store.go` | All notification DB operations: `GetFollowers`, `GetEntityName`, `GetStatDisplayName`, `GetMatchTime`, `InsertPending`, `ClaimDue` (FOR UPDATE SKIP LOCKED), `MarkSent`, `MarkFailed`, `getDeviceTokens` |
| `pipeline.go` | `Run()` orchestrator (detect→fan-out→schedule→persist), `buildMessage()`, `ordinalSuffix()` |
| `dispatch.go` | `StartWorker()` background ticker goroutine, `dispatchBatch()`, `getDeviceTokens()` |
| `sender.go` | `FCMSender` struct — nil-safe placeholder, logs sends. TODO: integrate `firebase.google.com/go/v4/messaging` |

### Schema additions (`schema.sql`)

- **Section 15:** `users`, `user_follows`, `user_devices`, `notifications` tables
- **Section 16:** `archive_current_percentiles()` and `detect_percentile_changes()` Postgres functions

### Modified existing files

- `internal/db/db.go` — 10 new prepared statements
- `internal/config/config.go` — `FCMCredentialsFile` field
- `internal/seed/upsert.go` — `ArchivePercentiles()` function
- `cmd/ingest/main.go` — `fixtures` command with `process` and `seed` subcommands
- `cmd/api/main.go` — Notification dispatch worker startup

---

## Notification Pipeline

Sequential pipeline (no channels — fan-out is CPU-trivial):

1. **Detect** — `detect_percentile_changes()` Postgres function compares current percentiles to archived snapshot. Returns rows where percentile crossed 90th/95th/99th milestone or delta >= ±10 points.
2. **Fan-out** — For each significant change, query `user_follows` to find followers of the affected entity.
3. **Schedule** — `ScheduleDelivery()` assigns a `deliver_at` timestamp within the user's waking hours (09:00–22:00 in their timezone) with random jitter across a 12-hour window to avoid thundering herd.
4. **Persist** — `InsertPending()` writes to `notifications` table with status `pending`.

### Dispatch

`StartWorker()` runs as a goroutine alongside the API server. Every 30 seconds:
1. `ClaimDue()` — `SELECT ... WHERE status='pending' AND deliver_at <= now() FOR UPDATE SKIP LOCKED`
2. For each claimed row: fetch device tokens → `FCMSender.SendMulti()` → mark sent/failed.

### Trigger conditions

- **Milestone crossing:** Percentile enters 90th, 95th, or 99th tier (or exits)
- **Large delta:** Absolute percentile change >= 10 points

---

## Fixture Seeder

### CLI Usage

```bash
# Process all pending fixtures
scoracle-ingest fixtures process --sport NBA --max 10 --workers 2

# Seed a single fixture
scoracle-ingest fixtures seed --id 42

# Skip percentile recalculation
scoracle-ingest fixtures process --skip-percentiles
```

### Phase 1 (current): Full-season fetch

Uses existing provider handlers to fetch full-season data. The `SeedFixture` function calls the same `handler.GetPlayerStats()` / `handler.GetTeamStats()` methods as the bulk seeder, then upserts via `seed.UpsertPlayer`, `seed.UpsertPlayerStats`, `seed.UpsertTeamStats`.

### Phase 2 (future): Targeted per-fixture seeding

Infrastructure supports swapping in team-filtered provider methods (`GetPlayerStatsByTeam(teamID)`) for targeted fetching using `fixture.HomeTeamID` / `fixture.AwayTeamID`. This eliminates redundant API calls for players not involved in the match.

---

## Configuration

| Env Variable | Default | Purpose |
|---|---|---|
| `FIREBASE_CREDENTIALS_FILE` | `""` (disabled) | Path to Firebase service account JSON. When empty, FCM sender is nil (no-op). |

---

## Design Decisions

1. **Sequential pipeline over channels** for notifications — channels don't earn their keep since the fan-out is CPU-trivial.
2. **Channels in fixture scheduler** — genuine concurrency exists when processing N fixture groups with M workers.
3. **Dispatch worker alongside API server** — no separate process to manage; gracefully shuts down on context cancellation.
4. **FCM sender is nil-safe** — `SendMulti` on nil receiver returns nil. No-op when `FIREBASE_CREDENTIALS_FILE` not set.
5. **Percentile archiving in `seed/upsert.go`** — avoids coupling seed → notifications. Notifications reads the archive.
6. **FOR UPDATE SKIP LOCKED** for dispatch claiming — multiple API server instances can run dispatch workers without conflicts.

---

## Future Work

- [ ] Integrate actual Firebase SDK (`firebase.google.com/go/v4/messaging`) in `sender.go`
- [ ] Phase 2 targeted seeding with team-filtered provider methods
- [ ] API endpoints for user notification preferences and device token registration
- [ ] Notification history endpoint for the frontend
- [ ] Rate limiting on FCM sends (Firebase has 500 msgs/second default)
- [ ] Dead letter handling for repeatedly failed notifications
