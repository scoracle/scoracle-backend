# 2026-06-09 — Restore in-season recomputes in finalize_fixture (migration 050)

## Severity: HIGH (regression fix). Caught during the SQL-engine audit.

## What broke
Migration 049 (position durability) rebuilt `finalize_fixture` by extracting it from
canonical `sql/shared.sql` — which had **drifted**. shared.sql's finalize_fixture never
tracked the recompute tail that migrations 017/027/028/029 added, so 049 silently dropped
SIX `PERFORM`s from prod's finalize_fixture, leaving only `recalculate_percentiles`.

**Effect:** during a LIVE season, seeding a fixture refreshed only season percentiles —
NOT the z-rating engine (`compute_rating`/`compute_team_rating`), the per-event starline
(`compute_event_starline`/`compute_team_event_starline`), event percentiles
(`recalculate_event_percentiles`), or per-event rating percentiles
(`recalculate_event_rating_pct`). Ratings/sparkline would have frozen mid-season.

Offseason at the time of the break → no fixtures seeded in the window → no stale data; the
function definition was the only damage.

## Fix (migration 050)
Restored the full recompute tail in `finalize_fixture` (all 7 calls, each scoped to
`v_season` so prior seasons stay frozen) while keeping 049's position-durability fix.
Canonical `shared.sql` now carries the COMPLETE definition, ending the drift.

Tail order (post-aggregation): recalculate_percentiles → recalculate_event_percentiles →
compute_rating → compute_team_rating → compute_event_starline → compute_team_event_starline
→ recalculate_event_rating_pct → REFRESH matviews → mark_fixture_seeded.

## Verification
- All 7 recompute functions confirmed present in prod (text, integer signatures).
- Dry-run (ROLLBACK) compiled; applied → COMMIT. Prod `pg_get_functiondef(finalize_fixture)`
  now shows all 6 restored `PERFORM …(v_sport, v_season)` calls + the 3 position branches.
- DDL only; no data recompute needed (offseason; no in-window seeds).

## Root-cause lesson (for the audit)
The "edit canonical shared.sql + write a migration" pattern is unsafe when shared.sql is
already drifted from the applied migrations — the rating engine (functions + rating_*
columns) lives ONLY in migrations, not in canonical shared.sql. A migration that rebuilds a
function should be derived from the CURRENT prod definition
(`pg_get_functiondef`), not from a possibly-stale canonical file. See the audit for the
broader recommendation to reconcile canonical vs migrations.
