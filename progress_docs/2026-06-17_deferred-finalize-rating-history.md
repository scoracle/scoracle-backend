# Deferred per-fixture recompute + rating_history time-series

**Date:** 2026-06-17
**Scope:** Make historical-season backfill O(M) instead of O(M²); freeze concluded seasons; add an append-only rating time-series for ML.
**Commit:** `65e0022` (origin/main)

## Goal

`event process` calls `finalize_fixture(fixture_id)` once per fixture, and the tail re-derives the
*entire* `(sport, season)` — percentiles + the z-rating engine + per-event starline + event
percentiles + two CONCURRENT matview refreshes. Correct/cheap in steady state (one new game a
night), but **O(M²)** during bulk historical backfill (M fixtures each pay the whole-season cost).
That was capping football backfill at ~2–4 fixtures/min. Implements the long-standing
`planning_docs/DEFERRED_PERCENTILE_BACKFILL.md`, updated for the rating-engine tail added since that
doc was drafted, plus a new `rating_history` series.

## What Was Done

**SQL — `sql/migrations/092_deferred_finalize_and_rating_history.sql` (+ `sql/shared.sql` synced):**
- `recompute_season(sport, season)` — the whole-season tail extracted into one idempotent pass
  (recalculate_percentiles + recalculate_event_percentiles + compute_rating + compute_team_rating +
  both starlines + recalculate_event_rating_pct + autofill matview refresh). Includes the **full
  current** tail, not the stale 3-step version in the 2026-05-30 doc.
- `finalize_fixture(fixture_id, p_recompute BOOLEAN DEFAULT TRUE)` — `TRUE` = unchanged live
  behavior (now composed from `recompute_season`); `FALSE` = per-fixture aggregation + mark-seeded
  only. `DROP FUNCTION IF EXISTS finalize_fixture(INTEGER)` first (Postgres won't overload one-arg
  with a two-arg-default); one-arg callers keep working via the default.
- `rating_history` (append-only) + `snapshot_rating_history(sport, season, trigger)` — debounced
  insert-if-changed per entity (player + team; teams have no `rating_modes` → NULL). The queryable
  rating-score series for ML.

**Python seeder:**
- `event process` auto-defers by lifecycle: `season < sports.current_season` → batch (per-fixture
  `recompute=False`, then one `recompute_season` + `seed` snapshot at the end); current season →
  per-fixture as before. `--batch/--no-batch` overrides. `current_season` resolved + committed up
  front so each fixture's transaction stays a real transaction.
- New `event recompute --sport --season [--alltime] [--no-snapshot]` for interrupted-run resume and
  the deliberate post-backfill cross-season all-time pass.
- `upsert.py`: `finalize_fixture(recompute=)`, `recompute_season()`, `snapshot_rating_history()` wrappers.

**Go — `internal/maintenance/maintenance.go`:** the daily all-time-rank ticker now also writes the
in-season `rating_history` trajectory and stamps a `season_close` row on rollover (best-effort).

## Files Changed

- `sql/migrations/092_deferred_finalize_and_rating_history.sql` (new)
- `sql/shared.sql` — `finalize_fixture` two-arg + `recompute_season` (base kept in sync)
- `seed/services/event/cli.py` — auto-defer in `process`, new `recompute` command
- `seed/shared/upsert.py` — wrappers
- `go/internal/maintenance/maintenance.go` — rating_history snapshots

## Verification

- `go build`/`go vet`/`gofmt` clean; `py_compile` clean; SQL delimiters/BEGIN-COMMIT/migrate.sh-compat checked.
- **Equivalence + determinism proven on a stable complete season (NBA 2018, in one transaction, ROLLBACK):**
  stored (per-fixture) vs `recompute_season` = **0** rating diffs / **0** percentile diffs;
  `recompute_season` vs `recompute_season` = **0** / **0**. So the batch path is byte-identical to the
  per-fixture path *and* deterministic.
- A first equivalence attempt on **football 2019 showed large diffs — a false alarm**: that season was
  being **concurrently seeded by another session** (and is incomplete + multi-league), so the data
  moved under the test. The pure in-transaction jitter there was ~74 rows in the *percentile layer*
  only, with **0 diffs in the displayed rating** — a pre-existing sub-display-threshold tie-break nit
  (candidate future fix: a deterministic tiebreaker in the rank ORDER BY), not introduced here.
- **Validated in production:** the concurrent backfill session re-ran `event process football 2019`
  on the deployed code → batch path → deferred `recompute_season` + `seed` rating_history snapshot.

## Result

Deployed to archbox: migration 092 applied via `sql/migrate.sh`; `bin/scoracle-api` rebuilt (old →
`bin/scoracle-api.bak091`) and `systemctl --user restart scoracle-api` (health + data endpoints 200).
`rating_history` is populating (`in_season` from the ticker, `seed` from backfill). Historical
backfill now runs at provider-API speed with a single end-of-season recompute (O(M)); concluded
seasons stay frozen unless a deliberate `recompute`. Pushed to `origin/main` (`65e0022`).
