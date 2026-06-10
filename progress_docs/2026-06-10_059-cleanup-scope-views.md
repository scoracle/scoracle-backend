# 059 — Cleanup after 058 (dead arities + profile-view scope columns)

**Date:** 2026-06-10

## Goal

De-drift after migration 058. Two leftovers:
1. The superseded single-scope block arities `template_block(text,text,jsonb,jsonb)` and
   `team_template_block(text,jsonb,jsonb)` — dead since the 058 API restart (the live
   binary calls the new `p_scoped` forms).
2. The per-sport profile views (`nba`/`nfl`/`football` . `player`/`team`) built a
   `scoped_percentile_metadata` object from FLAT `scoped_percentiles` keys
   (scope_type/scope_id/scope_name/_position_group/_sample_size) that 058's nested
   `{scope:{key:pct}}` format removed → it was always NULL, and the `scoped_percentiles`
   column carried now-pointless `- 'key'` meta-strips.

## What was done

- **Dropped** the two dead block arities.
- **`CREATE OR REPLACE VIEW`** for all six profile views: `scoped_percentiles` now
  exposes the nested object directly; `scoped_percentile_metadata` is rebuilt to surface
  the **list of available cohort scopes** (e.g. `{"scopes":["all","league"]}`) instead of
  the obsolete single-scope identity. Same column names/types/order → no
  prepared-statement change, no API restart.
- Canonical `sql/nba.sql`, `sql/nfl.sql`, `sql/football.sql` synced (the same two columns
  in each of the six views).

Nothing consumes these columns (verified: the frontend reads the sparkline blocks, not
the profile-view scope columns), so this is purely cosmetic de-drifting — no user-facing
change.

## Files changed

- `sql/migrations/059_cleanup_scope_views.sql` (new)
- `sql/nba.sql`, `sql/nfl.sql`, `sql/football.sql` (canonical views synced)

## Verification

Local throwaway clone (at 058): 059 applies clean — gate confirms the dead arities are
gone and `scoped_percentile_metadata.scopes` is populated (1151 NBA players). Football
player view returns `{"scopes":["all","league"]}`; `scoped_percentiles` exposes the
nested `{all, league}` structure.

## Rollout (pending authorization)

Prod dry-run (COMMIT→ROLLBACK) → `migrate.sh` apply. **No API restart** (function-drop +
view-replace; the `sparkline`/profile prepared statements use `row_to_json(p)` and the
new block arities, both unaffected).
