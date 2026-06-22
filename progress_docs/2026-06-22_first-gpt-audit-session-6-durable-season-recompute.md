# First GPT Audit — Session 6: Make deferred season recomputation durable

**Worked:** 2026-06-22 (archbox)

**Plan:** `planning_docs/FIRST-GPT-AUDIT.md`, Session 6

**Depends on:** the deferred-finalize feature (migration `092`, `finalize_fixture(p_recompute)`
+ `recompute_season` + `snapshot_rating_history`). This session makes the dirty-season
tracking around that feature durable.

**Product authority:** wiki `Product Narrative`

## Goal

Process death must not strand a seeded-but-unrecomputed season **invisibly**.

A deferred backfill (`event process --batch`, or the auto-batch path for concluded seasons)
finalizes each fixture with `finalize_fixture(p_recompute = FALSE)` — skipping the expensive
whole-season recompute — then owes the season ONE `recompute_season` + `rating_history`
snapshot at end-of-run. Previously the set of dirty `(sport, season)` pairs lived **only in an
in-memory Python set**: if the process died before the end-of-run drain, the season was left
seeded (so resume state looked complete) but never recomputed, with no durable record that work
remained.

## Decision

Add a tiny durable queue, `season_recompute_needed`, and drive the recompute from its rows
instead of the in-memory set:

- **Mark in the finalize transaction.** When a fixture is finalized with `recompute=FALSE`,
  upsert `(sport, season)` into `season_recompute_needed` in the **same transaction** as
  `finalize_fixture`. A crash before the drain therefore leaves a durable dirty marker.
- **Drain from durable rows.** End-of-run (and the new `recompute-drain` command) read the
  table, not a set — so they pick up this run's deferrals **plus anything an earlier crash left
  behind**. The drain is self-healing: any later `event process` run also clears stranded
  markers.
- **Delete only on full success.** The marker is deleted in the **same transaction** as its
  `recompute_season` + `snapshot_rating_history`, so it clears only when both succeed. A failure
  rolls that transaction back (marker survives) and records `attempts`/`last_error` in a separate
  transaction, without aborting the rest of the drain.
- **Targeted clear.** `event recompute --sport --season` clears that pair's marker atomically
  with its recompute.

`season_recompute_needed` does NOT touch `finalize_fixture`, so migration `101` is safe to apply
while a seeder is mid-run (no cached-plan invalidation — contrast `092`).

## What changed

### `sql/migrations/101_season_recompute_needed.sql` (new)

`CREATE TABLE season_recompute_needed (sport, season, requested_at, last_error, attempts,
PRIMARY KEY (sport, season))`. Additive + idempotent.

### `seed/shared/upsert.py`

Four helpers around the new table:

- `mark_season_recompute_needed(conn, sport, season)` — `INSERT … ON CONFLICT DO NOTHING`.
- `load_dirty_seasons(conn)` — all dirty pairs, oldest `requested_at` first.
- `clear_season_recompute_needed(conn, sport, season)` — delete one marker.
- `record_recompute_failure(conn, sport, season, error)` — bump `attempts`, set `last_error`.

### `seed/services/event/cli.py`

- `_seed_fixture_box_scores` — after a `recompute=FALSE` finalize, calls
  `mark_season_recompute_needed` in the same transaction.
- `_drain_dirty_seasons(conn)` (new) — loads dirty rows; per pair, recompute + snapshot + clear
  in one transaction; on failure records the error and continues. Returns
  `(pairs_seen, players_updated, teams_updated)`.
- `process` — the in-memory `deferred` set is gone; end-of-run calls `_drain_dirty_seasons`.
- `recompute` (`event recompute --sport --season`) — clears the season's marker in the recompute
  transaction.
- `recompute-drain` (new command) — standalone drain of all dirty seasons; the documented
  recovery path after a `process --batch` was killed before its end-of-run drain. No-op when
  nothing is pending.

### Tests — `seed/tests/test_event_recompute.py` (new)

- Deferred finalize marks the dirty season; a live (`recompute=TRUE`) finalize does not.
- Drain clears each marker only on success; an empty queue is a no-op.
- A failed recompute keeps its marker (rolled back), records the failure, and does **not** abort
  the rest of the drain (the other season still drains).

## Maps to the audit's verification

- *Finalize a historical fixture in deferred mode → kill before end-of-run → dirty season remains
  visible* — the mark is committed with finalize; the drain is the only thing that deletes it.
- *Run the drain command; it clears only after success* — `recompute-drain` →
  `_drain_dirty_seasons`, delete in the same txn as recompute+snapshot; failure path keeps the
  row.

## Verification

- `pytest seed/tests/` — **60 passed** (was 55; +5 new cases).
- Migration `101` applied cleanly to an ephemeral throwaway Postgres 18 cluster (isolated from
  prod) and `season_recompute_needed` inspected.
- `event recompute-drain --help` registers; `event` command set =
  `{load-fixtures, process, recompute, recompute-drain}`.

## Not done here (deliberate)

- **No deploy.** Migration `101` is not yet applied to prod and the seeder is not rebuilt/
  restarted — a corpus pipeline was running and a parallel session orchestrates seeding; the
  apply + restart is a coordinated step. The migration is additive and seed-safe (no
  `finalize_fixture` change), so it can be applied without stopping a seed.
- **Backfilling existing stranded seasons** — there is no historical dirty state to migrate; the
  table starts empty and fills as deferred fixtures are finalized going forward.

## Files changed

- `sql/migrations/101_season_recompute_needed.sql` (new)
- `seed/shared/upsert.py`
- `seed/services/event/cli.py`
- `seed/tests/test_event_recompute.py` (new)
- `progress_docs/2026-06-22_first-gpt-audit-session-6-durable-season-recompute.md` (this doc)
