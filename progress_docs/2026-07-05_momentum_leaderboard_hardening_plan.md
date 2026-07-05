# 2026-07-05 - Momentum Leaderboard Hardening Plan

Audit of the migration-128 momentum snapshot dataflow and the DB-first
leaderboard split, plus the plan to make both tight and durable. Companion to
`2026-07-05_momentum_scores_leaderboard_followup.md` (which records what
shipped in `afd16d5`); this doc records what the audit found and what to do
next.

## Verdict

The architecture is right. The dirty-marker -> NOTIFY -> drain -> snapshot ->
read-latest dataflow is the correct shape: event-driven, idempotent, no blind
timers, and the marker handshake is genuinely race-safe. What migration 128 is
missing is a **lifecycle** — nothing bounds the table, nothing paces the
snapshots — and it shipped some dead surface area. Five findings, four of them
one-sitting fixes.

## Verified correct (do not re-litigate)

- **Marker handshake is race-safe.** The drain deletes with
  `WHERE sport = $1 AND last_marked_at = $2`
  (`go/internal/maintenance/maintenance.go:448-450`), so a mark that lands
  mid-refresh survives the delete and is re-drained. No lost updates.
- **The catch-up timer is a true no-op without markers** — it only drains
  pending `momentum_refresh_needed` rows. Matches the design intent exactly.
- **Triggers fire only on the meaningful column** (`UPDATE OF sentiment` /
  `UPDATE OF rating_composite_pct`) with `IS DISTINCT FROM` guards; the
  post-deploy backfill reuses the same dirty-queue path instead of a special
  case (`sql/migrations/128_momentum_scores.sql:260-262`).
- **Roster surface delivers the promise.** Team-scoped player boards echo the
  roster `team_id` (`go/internal/db/db.go:246`), `LIMIT NULL` guarantees full
  roster inclusion (`db.go:334`), and null-rank roster rows sort after scored
  rows.
- **Request-time momentum derivation is fully gone.** Both trending statements
  read `momentum_scores` only. The one remaining request-time heavy path is
  profile `/momentum` (trendsStatement) — intentional: per-entity, 30-min
  cached, the card surface not the hierarchy surface.
- **Transfers cohort params** are wired in the right order ($1-$10 all match
  between handler and statement).

## Findings

### F1 - No retention: unbounded growth, reads degrade with table age

`momentum_scores` is append-only with no pruning anywhere (the cleanup ticker
only purges notifications). Every drain appends a **full-sport snapshot**
(hundreds of rows for NBA). The leaderboard read does
`DISTINCT ON (entity_type, entity_id) ... ORDER BY generated_at DESC` over ALL
history for the sport; Postgres btrees cannot skip-scan, so latency grows
linearly with table age. The "durable DB-first read" quietly becomes a
full-history scan.

**Fix:** two-tier retention (see "Momentum as a historic datapoint" below) —
full resolution for the recent window, one-row-per-entity-per-day beyond it.

### F2 - No debounce: snapshot bursts during game nights

`refresh_momentum_scores` is a heavy aggregate (21d vibe scan + 60d event scan
per sport) and runs once per drained NOTIFY with zero settle time. Finalize
stamps `rating_composite_pct` across the cohort repeatedly on a game night;
each transaction re-marks the sport and each buffered NOTIFY re-drains. Result:
dozens of near-identical snapshots per hour — real refresh cost AND a noisy
historic series.

**Fix:** in-memory min-interval throttle in `drainMomentumRefreshNeeded` (skip
a sport refreshed < ~5 min ago). The marker persists, so the next NOTIFY or
the 15-min catch-up drains it — worst-case staleness after a burst is one
catch-up tick, which is fine for this surface. This is what makes the historic
series one settled datapoint per real change (same philosophy as
`rating_history`'s debounced insert-if-changed).

### F3 - Dead weight shipped in migration 128

1. **`momentum_score` column is written but never read.** Both boards read
   `vibe_slope`/`rating_slope`; the handler only supports `metric=vibe|rating`.
   (`momentum_score` in `rust/src/sigil.rs` is an unrelated Sigil pillar.)
   Additionally its `GREATEST(x, 0)` clamp stores a positive-only average —
   downside momentum is erased, so it cannot serve as an honest historic
   datapoint. **Resolution (user-directed 2026-07-05): keep the column, fix
   the semantics** — see "Momentum as a historic datapoint" below.
2. **The two partial slope indexes are useless for the actual read.**
   `idx_momentum_scores_sport_vibe/rating` order by slope, but the query needs
   latest-per-entity. And `idx_momentum_scores_entity_recent` has `sport` in
   third position, so the sport-scoped read can't range-scan it.
   **Fix:** replace all three with one index:
   `(sport, entity_type, entity_id, generated_at DESC)`.
3. **Read-side `vibe_samples >= 3` / `rating_samples >= 3` filters are dead
   logic** (`db.go:538`, `db.go:593`) — the refresh's `HAVING count(*) >= 3`
   already guarantees them whenever the slope is non-null. Prune for clarity.

### F4 - Schema source-of-truth drift

Commit `afd16d5` shipped migration 128 but `sql/schema/schema.sql` has zero
momentum objects and `sql/schema/schema_migrations.txt` ends at 127.
(Meanwhile the 125-127 folds sit unstaged and those migration files are
untracked.) Fold 128 into schema.sql and append the ledger line before a
fresh-database bootstrap bites.

### F5 - Doc drift on the score's meaning

`go/internal/api/handler/data.go:219-220` (and regenerated swagger) still says
"score is the fitted per-week rise." It is now the newest-minus-oldest delta
over the snapshot window (21d vibe / 60d rating), rounded. If the frontend
renders "+x/wk" units, that is wrong by both unit and magnitude. Fix the
comment, regenerate swagger, check the frontend label.

## Momentum as a historic datapoint (the lightweight appendable score)

Decision direction: we want a concrete, durable, per-snapshot momentum number
we can always come back to. The slot already exists — `momentum_scores.
momentum_score` — the work is semantics + lifecycle, not new machinery.

**Semantics — make it signed.** Drop the `GREATEST(x, 0)` clamp. Both inputs
are already deltas in 0-100-scaled spaces (vibe sentiment 1-100; rating
percentile 0-100), so a signed average of the present components is
unit-coherent and costs nothing extra (computed inside the same refresh):

```sql
round(
    (COALESCE(vibe_slope, 0) + COALESCE(rating_slope, 0))
    / NULLIF(
        (CASE WHEN vibe_slope IS NULL THEN 0 ELSE 1 END)
        + (CASE WHEN rating_slope IS NULL THEN 0 ELSE 1 END),
        0),
    3)
```

Positive = rising, negative = falling, magnitude comparable across entities
and across time. Deterministic SQL, no model call, no new tables.

**Lifecycle — downsample instead of delete.** Retention becomes two-tier:

- **<= 30 days old:** keep every snapshot (full resolution; serves the
  leaderboard read and any near-term diffing).
- **> 30 days old:** keep the LAST row per (sport, entity_type, entity_id,
  day); delete the rest. Bounded at ~365 rows/entity/year worst case — with
  the F2 debounce, far fewer in practice.

This preserves "a datapoint we can always come back to" without unbounded
growth, and the daily-grain series is exactly what a future sparkline or ML
reader wants. Run the downsample in the existing cleanup ticker.

**Read paths (later, not now).** Nothing reads the history yet, and that is
fine — same pattern as `rating_history` (O3: write-only until it has depth).
When wired, the natural readers are `metric=combined` on the momentum board
and a momentum-trajectory series on profile `/momentum`. Do not add a reader
until there is a product surface that wants it.

## Work plan

### Phase 1 - Migration 129 (SQL)

1. Signed `momentum_score` in `refresh_momentum_scores` (formula above).
2. Drop `idx_momentum_scores_sport_vibe`, `idx_momentum_scores_sport_rating`,
   `idx_momentum_scores_entity_recent`; create
   `idx_momentum_scores_read (sport, entity_type, entity_id, generated_at DESC)`.
3. Optional hardening: `pg_try_advisory_lock` inside `refresh_momentum_scores`
   so concurrent drains (NOTIFY listener + catch-up ticker) cannot
   double-append a snapshot.

### Phase 2 - Go maintenance

1. Min-interval throttle in `drainMomentumRefreshNeeded` (in-memory map,
   sport -> last refresh time; single-process, no locking beyond the existing
   single-goroutine access pattern — mirror `lastSeenSeason`).
2. Two-tier downsample DELETE added to the `cleanup` task.

### Phase 3 - Tidy

1. Fold migrations 125-128 (and 129) into `sql/schema/schema.sql`; append
   `schema_migrations.txt` entries.
2. Drop the dead `samples >= 3` read filters in both trending statements.
3. Fix the "fitted per-week rise" doc comment; `swag init` regenerate; add a
   `, ranked.name` tiebreaker to `json_agg(... ORDER BY ranked.rank)` in the
   rating leaderboard so null-rank roster rows render in stable order.

### Phase 4 - Route retirement (frontend-gated)

Once the frontend is fully on the `?board=` rail, delete the dedicated
`/leaderboard/{vibes,sigil,news,transfers,momentum}` routes and the
`/leaderboard/trending` legacy alias (`go/internal/api/server.go:173-178`).
Handlers stay (they are the delegation targets). Halves the swagger surface
and stops splitting the cache by URL. NOT before the frontend migrates.

### Explicitly deferred

- Statement-level triggers (transition tables) to collapse per-row queue
  upserts during bulk pct stamping. Current per-row triggers are correct,
  just chatty; revisit only if finalize timing regresses on game nights.
- Any reader of the momentum history (combined board / profile series) —
  product-gated.

## Verification (per phase)

- `BEGIN; \i sql/migrations/129_*.sql; ROLLBACK;` clean on a live snapshot.
- `EXPLAIN ANALYZE` the trending statements before/after the index swap —
  expect the new index in the plan and latency flat as history accrues.
- Game-night simulation: burst-mark a sport N times, assert one refresh per
  throttle window and marker fully drained afterward.
- `GOCACHE=/tmp/scoracle-go-cache go test ./internal/api ./internal/api/handler ./internal/db ./internal/maintenance`
- Downsample: seed multi-day history, run cleanup, assert last-per-day
  survival beyond 30 days and full resolution inside it.
