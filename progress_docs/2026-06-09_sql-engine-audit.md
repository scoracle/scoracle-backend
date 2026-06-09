# 2026-06-09 — SQL Engine Audit

Audit of the rating engine after the uniform-scopes / fantasy / templates / position work.
Lens: durability, optimization, provider-agnosticism, multi-frontend readiness, and
"elegance via simplicity." Intended flow: **raw events seeded → derived stats (composite,
fantasy, percentiles) generated + upserted → payload via Go endpoints.** Season rule:
**prior seasons frozen during a live season; full cross-season re-baseline on completion.**

## Verdict
The engine is **architecturally sound and genuinely elegant at its core** — provider-
agnostic, Postgres-as-serializer, per-sport logic cleanly separated, fantasy integrated as
a first-class derived stat (not bolted on). The season-freeze + cross-season system works
as intended. The real risk is **not the design — it's DRIFT** between canonical `sql/*.sql`
and the migrations, which already caused one production regression (now fixed). Optimization
gaps (indexes, per-fixture cost) are real but dormant at current scale.

---

## P0 — CRITICAL (one already fixed; one structural)

### 0a. [FIXED] finalize_fixture lost its recompute tail (migration 049 → 050)
Migration 049 rebuilt `finalize_fixture` from a **stale** canonical `shared.sql`, silently
dropping 6 in-season recomputes (compute_rating, compute_team_rating, the two starline
recomputes, recalculate_event_percentiles, recalculate_event_rating_pct). In-season this
would have frozen ratings/sparkline. **Fixed in migration 050** (full tail restored, scoped
to v_season; position-durability fix kept; canonical shared.sql now complete). Offseason →
no stale data. Verified on prod.

### 0b. Canonical `sql/*.sql` ↔ migrations DRIFT  (root cause of 0a; blocks fresh builds)
The rating engine — the `rating_*`/`season_composite_*` COLUMNS and the functions
`compute_rating`, `compute_team_rating`, `recalculate_event_percentiles`, the starline
functions, `recalculate_alltime_ranks` — **lives only in migrations, not in canonical
shared.sql**. Consequences:
- A fresh DB built from `shared.sql` + sport files **alone is broken** (missing rating_*
  columns → prepared statements fail). A fresh build REQUIRES `shared.sql` + sport files +
  **all migrations 001→050 in order**. This directly threatens **sandbox.scoracle / a new
  environment**.
- The "edit canonical + write migration" pattern is unsafe while canonical is drifted — a
  migration that rebuilds a function from canonical can revert prod (exactly what 0a was).

**Recommendation (pick one, durably):**
- **(Preferred) Make migrations the single source of truth.** Add a `schema_migrations`
  tracking table + a one-command fresh-build script (apply base + sports + migrations in
  numeric order). Stop hand-editing canonical for engine functions; when a migration
  rebuilds a function, derive it from `pg_get_functiondef(...)` (current prod), not from
  canonical. Treat `shared.sql`/`nba.sql`/… as the BASE only, and say so.
- **(Alternative) Reconcile canonical to be complete** — port every rating_* column +
  engine function into the canonical files so a fresh `shared+sports` build is whole. Higher
  one-time effort; keeps "canonical is truth," but must be policed forever.

The Go API mitigates blast radius poorly today: prepared statements are registered at
connect but only fail on first execution. Add a **startup validation** (execute each
prepared statement once against the schema, or `PREPARE` + `EXPLAIN`) so a missing column
fails fast at boot, not on a user request.

---

## P1 — HIGH (do before scaling / multi-frontend traffic)

### 1a. Duplicate migration number 042
`042_auth_refresh_tokens.sql` + `042_rating_modes.sql`. Harmless TODAY (migrations are
applied by hand, both ran), but it's a latent trap the moment a runner/tracking table is
adopted (0b). Rename the auth one (e.g. `042a_…`) and adopt the tracking table together.

### 1b. Missing indexes for leaderboard ordering
`/leaderboard` orders by `rating_composite` / `rating_specialist` / `(stats->>'fantasy_points')::numeric`
— none indexed. Fine now (~600 NBA / ~1,800 NFL rows/season → a few-thousand-row scan after
the `(sport,season)` filter). Before multi-frontend traffic, add (CONCURRENTLY):
```sql
CREATE INDEX idx_player_stats_rating_composite  ON player_stats(sport, season, rating_composite  DESC) WHERE rating_composite IS NOT NULL;
CREATE INDEX idx_player_stats_rating_specialist ON player_stats(sport, season, rating_specialist DESC) WHERE rating_specialist IS NOT NULL;
CREATE INDEX idx_player_stats_fantasy_points    ON player_stats((( stats->>'fantasy_points')::numeric) DESC) WHERE (stats->>'fantasy_points') IS NOT NULL;
```
(team_stats rating_composite likewise.)

---

## P2 — MEDIUM (durability + simplicity; address when adding a 4th sport)

### 2a. finalize_fixture per-fixture cost
Each fixture finalize runs 7 recomputes + `REFRESH MATERIALIZED VIEW CONCURRENTLY` (autofill).
At full-season bulk re-seed this is heavy. At current cadence (one fixture at a time, live)
it's acceptable. When bulk-seeding, consider a "defer recompute" path: aggregate per fixture,
recompute the season + refresh matviews ONCE at the end of the batch.

### 2b. Accreted complexity in the rating engine (elegance targets, behavior-preserving)
All defensible, but worth centralizing before sport #4:
- **Per-row `rate_key`/`rate_key2` literals** in `rating_datapoints` (50+ rows) — adding a
  rate mode touches every row. Could move (base_key → suffix) to a small map.
- **Legacy aliases** `turnover→tov`, `shots_total→shots` — hardcoded in triggers AND
  re-handled via `stat_templates.rate_base`. A `stat_key_aliases` metadata row would make
  the 1:1 mapping DRY + visible.
- **Hardcoded rate-mode lists** duplicated in `fantasy_block` + `template_block` + the three
  triggers' `per_X_keys`. A `rate_modes(sport, mode, suffix)` table would single-source it.
- **Hardcoded eligibility thresholds** (NBA ≥30g/≥20m, FB ≥15 apps, NFL ≥8g) duplicated in
  the rating bundle. A `rating_thresholds(sport,…)` row centralizes the business rule.

These are NOT bugs; they're "the pattern doesn't scale to N sports as-is." Each new sport
currently means editing several functions. The metadata-table refactors convert that to
data inserts.

---

## P3 — LOW (cleanups)
- `stat_templates.rate_base` is currently unused (all NULL) — kept for a future football
  alias; fine, but document or drop.
- `is_derived` / `is_percentile_eligible` flags on `stat_definitions` are inconsistent
  (inline composites like NFL "Total Yards" aren't registered at all). Cosmetic; document
  that query-time composites are intentionally unregistered.
- `recalcAlltimeRanks` rollover detection is in-memory (`lastSeenSeason`), so an API restart
  forces a full re-baseline (safe, just extra work). Acceptable; note it.
- `notify_percentile_changed` fires per percentile-eligible key; mitigated by the per-rate
  sibling exclusion (046) and `user_follows=0` today. Revisit (debounce/queue) before follows
  scale.

---

## What's GOOD (keep doing this)
- **Provider-agnostic**: the SQL layer reads only canonical stat keys; provider→canonical
  normalization is isolated in Python (`seed/.../stat_keys`). Swapping providers = Python only.
- **Postgres-as-serializer**: Go handlers are thin (validate → cache → prepared stmt →
  passthrough JSON); 100% of JSON shaped in SQL. Exemplary, and exactly right for serving
  fantasy.scoracle / sandbox.scoracle from the same endpoints.
- **Fantasy as a derived stat**: `fantasy_points` computed in the derived trigger → backfills
  by re-fire, auto-gets rate siblings, ranked for free by recalculate_percentiles, served via
  a `fantasy` payload block mirroring `rating_modes`. Clean, extensible (Phase 4 = add one
  `football.fantasy_points` + seeds).
- **Season-freeze + cross-season ranks**: per-(sport,season) recompute keeps prior seasons
  frozen in-season; `recalculate_alltime_ranks` does current-season-only in steady state and
  a full re-baseline on `current_season` rollover. Matches intent.
- **Parity gates** (042/045) prove zero drift on engine upgrades — a strong durability habit.

---

## Recommended action order
1. **0b is the keystone.** Adopt `schema_migrations` + a fresh-build script + a Go startup
   prepared-statement validation; declare migrations the source of truth and stop deriving
   function-rebuild migrations from (drift-prone) canonical. This is what makes
   sandbox/fantasy.scoracle safe to stand up and prevents another 0a.
2. Rename the duplicate 042 (1a) — bundle with the tracking table.
3. Add the leaderboard indexes (1b) before opening multi-frontend traffic.
4. Defer 2a/2b until sport #4 or a bulk-reseed need; they're elegance/scale, not correctness.
