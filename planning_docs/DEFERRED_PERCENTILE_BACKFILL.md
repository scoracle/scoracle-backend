# Deferred Percentile Recompute for Historical Backfill

**Status:** Proposed — ready to execute after the in-flight 2020/2021 football
seeding completes. Do not apply while seeding is running (it redefines
`finalize_fixture`, which the running jobs call per fixture).

**Author context:** Drafted 2026-05-30 while backfilling NBA 2018, NFL 2018,
and FOOTBALL 2020/2021. The football event chains paced at ~2–4 fixtures/min
not because of API limits but because Postgres recomputes whole-season
percentiles on every fixture.

---

## 1. Problem

`event process` calls `finalize_fixture(fixture_id)` once per fixture. The live
definition (`sql/migrations/017_event_percentiles_and_composite.sql:285`) does:

1. **Cheap, per-fixture:** re-aggregate the impacted players/teams from this
   fixture's `event_box_scores` / `event_team_stats` into `player_stats` /
   `team_stats` (`ON CONFLICT DO UPDATE`).
2. **Expensive, whole-season:** `recalculate_percentiles(sport, season)` —
   re-ranks **every** player and team in the season (lines 399–401).
3. **Expensive, whole-season:** `recalculate_event_percentiles(sport, season)` —
   re-ranks **every** event row in the season and rolls `season_composite_score`
   back onto `player_stats`/`team_stats` (line 406).
4. **Expensive:** `REFRESH MATERIALIZED VIEW CONCURRENTLY <sport>.autofill_entities`
   (lines 408–414).
5. **Cheap, per-fixture:** `mark_fixture_seeded(...)` (line 421).

Steps 2–4 are independent of *which* fixture triggered them — they redo the
entire season. So a season of **M** fixtures pays the whole-season cost **M
times**: total work is **O(M²)**. For football that's ~1,826 games re-ranking
the full season ~1,826 times per season.

This is correct and cheap in steady state (one new game a night → one recompute).
It is pathological for backfill.

**Precedent in our own codebase:** migration 023 already decoupled the
*all-time* (cross-season) ranks out of the per-fixture path for exactly this
reason — its header reads "during backfill it's O(seasons²) … Called on a
deliberate cadence (nightly maintenance ticker), NOT per-finalize." This plan
applies the same idea to the *within-season* recompute.

There are **no triggers** on the event tables — all recompute is explicit
function calls, so gating them is purely a matter of not calling them.

---

## 2. Design

Two changes, both keeping derived-stat logic in Postgres (per CLAUDE.md):

### A. Gate the heavy steps behind a parameter (default = current behavior)

Add `p_recompute BOOLEAN DEFAULT TRUE` to `finalize_fixture`. When `TRUE`
(the default), behavior is byte-for-byte identical to today, so the nightly
steady-state cron and the existing one-arg Python call are unaffected. When
`FALSE`, it runs only steps 1 and 5 (per-fixture aggregation + mark seeded) and
skips steps 2–4.

> **Gotcha:** you cannot leave the old one-arg `finalize_fixture(INTEGER)` in
> place alongside a new two-arg-with-default version — Postgres raises
> "function is not unique" on one-arg calls. The migration must
> `DROP FUNCTION IF EXISTS finalize_fixture(INTEGER);` first, then create the
> two-arg form. One-arg call sites keep working via the default.

### B. One-pass season recompute function

Add `recompute_season(p_sport, p_season)` that runs the three whole-season steps
exactly once. This is just the body of steps 2–4 extracted:

```sql
CREATE OR REPLACE FUNCTION recompute_season(p_sport TEXT, p_season INTEGER)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
DECLARE v_players INTEGER := 0; v_teams INTEGER := 0;
BEGIN
    SELECT rp.players_updated, rp.teams_updated INTO v_players, v_teams
    FROM recalculate_percentiles(p_sport, p_season) rp;

    PERFORM recalculate_event_percentiles(p_sport, p_season);

    IF    p_sport = 'NBA'      THEN REFRESH MATERIALIZED VIEW CONCURRENTLY nba.autofill_entities;
    ELSIF p_sport = 'NFL'      THEN REFRESH MATERIALIZED VIEW CONCURRENTLY nfl.autofill_entities;
    ELSIF p_sport = 'FOOTBALL' THEN REFRESH MATERIALIZED VIEW CONCURRENTLY football.autofill_entities;
    END IF;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;
```

All three functions called here already exist and are idempotent:
- `recalculate_percentiles(sport, season)` — `sql/migrations/013_stats_owned_position.sql:216`
- `recalculate_event_percentiles(sport, season)` — `sql/migrations/026_player_absolute_rank.sql:43`
- The autofill matviews — refreshed today inside `finalize_fixture`.

**All-time (cross-season) ranks stay separate.** `recompute_season` deliberately
does NOT call `recalculate_alltime_ranks(sport)` — that's cross-season and only
needs to run once after *all* historical seasons of a sport are loaded (it's
already decoupled per migration 023/024/026). See the runbook.

---

## 3. Files to change

### `sql/migrations/027_deferred_finalize.sql` (new)

```sql
-- 027_deferred_finalize.sql
-- Make the whole-season percentile recompute inside finalize_fixture optional,
-- and expose it as a standalone one-pass per (sport, season) recompute. Lets
-- historical backfill ingest box scores API-bound and recompute percentiles
-- once at the end instead of O(M^2) per-fixture. Default behavior unchanged.

BEGIN;

DROP FUNCTION IF EXISTS finalize_fixture(INTEGER);

CREATE OR REPLACE FUNCTION finalize_fixture(
    p_fixture_id INTEGER,
    p_recompute  BOOLEAN DEFAULT TRUE
)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
DECLARE
    -- ... same DECLARE block as migration 017 ...
BEGIN
    -- ... identical fixture lookup + per-sport aggregation (steps 1) ...

    IF p_recompute THEN
        SELECT rp.players_updated, rp.teams_updated INTO v_players, v_teams
        FROM recalculate_percentiles(v_sport, v_season) rp;

        PERFORM recalculate_event_percentiles(v_sport, v_season);

        IF    v_sport = 'NBA'      THEN REFRESH MATERIALIZED VIEW CONCURRENTLY nba.autofill_entities;
        ELSIF v_sport = 'NFL'      THEN REFRESH MATERIALIZED VIEW CONCURRENTLY nfl.autofill_entities;
        ELSIF v_sport = 'FOOTBALL' THEN REFRESH MATERIALIZED VIEW CONCURRENTLY football.autofill_entities;
        END IF;
    END IF;

    -- ... identical mark_fixture_seeded block (step 5) ...
    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION recompute_season(p_sport TEXT, p_season INTEGER)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
-- ... body from section 2B ...
$$ LANGUAGE plpgsql;

COMMIT;
```

> Copy the exact step-1/step-5 bodies verbatim from
> `017_event_percentiles_and_composite.sql:285–423`; only wrap steps 2–4 in
> `IF p_recompute THEN ... END IF;`.

### `seed/shared/upsert.py` (~line 317)

```python
def finalize_fixture(conn, fixture_id, recompute: bool = True) -> tuple[int, int]:
    row = conn.execute(
        "SELECT * FROM finalize_fixture(%s, %s)", (fixture_id, recompute)
    ).fetchone()
    return (row["players_updated"], row["teams_updated"]) if row else (0, 0)
```

### `seed/services/event/cli.py`

1. `_seed_fixture_box_scores(conn, fixture, handler, recompute=True)` — thread
   `recompute` through to the `finalize_fixture(conn, fixture.id, recompute)`
   call at line 112.
2. `process` command — add `--defer-percentiles/-D` flag (default off → no
   behavior change). When set:
   - pass `recompute=False` to each `_seed_fixture_box_scores` call;
   - track the distinct `(sport, season)` pairs actually processed;
   - after the loop, run one recompute per pair:
     ```python
     for sport_u, season_i in sorted(processed_keys):
         with conn.transaction():
             conn.execute("SELECT recompute_season(%s, %s)", (sport_u, season_i))
         click.echo(f"Recomputed percentiles: {sport_u} {season_i}")
     ```
3. New `recompute` command for the manual/resume case:
   ```python
   @cli.command("recompute")
   @click.option("--sport", type=click.Choice(["nba","nfl","football"]), required=True)
   @click.option("--season", type=int, required=True)
   @click.option("--alltime", is_flag=True, help="Also refresh cross-season all-time ranks for the sport")
   def recompute(sport, season, alltime):
       """One-pass percentile/composite recompute for a (sport, season)."""
       # SELECT recompute_season(SPORT, season);  [+ recalculate_alltime_ranks(SPORT) if --alltime]
   ```

No Go changes — the API only reads the resulting columns/views.

---

## 4. Execution runbook (after current seeding finishes)

```bash
# 0. Confirm no seeding job is mid-run (no event process active).

# 1. Apply the migration.
psql "$DATABASE_URL" -f sql/migrations/027_deferred_finalize.sql

# 2. Editable install picks up the Python changes automatically; just confirm.
cd seed && .venv/bin/scoracle-seed event process --help   # shows --defer-percentiles

# 3. Validate equivalence on an ALREADY-seeded season (proves deferred == per-fixture).
#    Snapshot, recompute once, diff — expect zero rows.
psql "$DATABASE_URL" -c "CREATE TEMP TABLE pct_before AS
  SELECT player_id, percentiles FROM player_stats WHERE sport='NBA' AND season=2018;"
psql "$DATABASE_URL" -c "SELECT recompute_season('NBA', 2018);"
psql "$DATABASE_URL" -c "SELECT count(*) AS diffs FROM player_stats p JOIN pct_before b
  USING (player_id) WHERE p.sport='NBA' AND p.season=2018 AND p.percentiles <> b.percentiles;"
#    diffs = 0 confirms the one-pass result matches the per-fixture result.

# 4. Future backfill — deferred path (the payoff):
SEASON=2019
scoracle-seed event load-fixtures football --season $SEASON
scoracle-seed event process --sport football --season $SEASON --defer-percentiles
#    ^ ingestion is now API-bound; process auto-runs recompute_season once at the end.
#    If a run is interrupted, just re-run process (resumes), then:
scoracle-seed event recompute --sport football --season $SEASON

# 5. After ALL historical seasons of a sport are loaded, refresh cross-season ranks ONCE:
psql "$DATABASE_URL" -c "SELECT recalculate_alltime_ranks('FOOTBALL');"
#    (or: scoracle-seed event recompute --sport football --season <latest> --alltime)
```

---

## 5. Expected payoff

| | Per-fixture (today) | Deferred (proposed) |
|---|---|---|
| Whole-season percentile passes per season | **M** (one per fixture) | **1** |
| Matview refreshes per season | M | 1 |
| Ingestion bottleneck | Postgres recompute | Provider API |
| Asymptotic season cost | O(M²) | O(M) |

For football (M ≈ 1,826/season) that's ~1,800× fewer whole-season passes.
Practically, `process` should run at provider-API speed (like `load-fixtures`),
with a single recompute (seconds–low minutes) at the end.

---

## 6. Risks & notes

- **Stale window:** between deferred ingestion and `recompute_season`, that
  season's `player_stats.percentiles`, event percentiles, composite scores, and
  the autofill matview are stale. For historical backfill the season isn't user-
  facing until recompute, so this is invisible. **Always run the recompute
  before treating a season as done.** `process --defer-percentiles` does it
  automatically; the standalone `recompute` command covers interrupted runs.
- **Resume safety:** `mark_fixture_seeded` still runs per fixture in deferred
  mode, so `get_pending` resume state stays correct even if a run dies before
  the recompute. Re-running `process` then `recompute` is safe and idempotent.
- **No steady-state regression:** default `p_recompute = TRUE` means the nightly
  per-game cron and the existing one-arg call site are unchanged. `--defer-
  percentiles` is strictly opt-in for backfill.
- **Equivalence:** the deferred result is identical to the per-fixture result
  because the season functions are pure recomputes over current table state —
  running them once at the end over complete data yields the same ranks as
  running them after each fixture. Step 3 of the runbook proves this empirically.
- **Rollback:** migration 027 is `CREATE OR REPLACE`. To revert, re-apply
  migration 017's `finalize_fixture` (and `DROP FUNCTION recompute_season`).
  `recompute_season` is additive and harmless to leave in place.

---

## 7. Optional follow-ups (not required)

- Batch `mark_fixture_seeded` + aggregation across N fixtures per transaction to
  cut commit overhead further (ingestion is then almost entirely API wait).
- A `maintenance` CLI group that wraps `recompute_season` +
  `recalculate_alltime_ranks` for the nightly ticker, replacing ad-hoc psql.
