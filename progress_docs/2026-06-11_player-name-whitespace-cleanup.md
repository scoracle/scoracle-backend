# 075 — Player name whitespace cleanup (trailing nbsp)

**Date:** 2026-06-11

## Goal
Resolve the long-flagged "football player identity" anomaly (player_id 997 surfacing an NFL
name "Will Mallory" on Harry Kane's row), and fix the genuine residual data defect found while
investigating it.

## What was done
- **Investigated the 997 anomaly — already resolved.** The `players` PK is `(id, sport)`, so
  football 997 (Harry Kane) and NFL 997 (Will Mallory) are distinct rows. DB, Go `/meta`,
  leaderboard, and roster all correctly resolve Kane on Bayern (503). The original name-swap
  was a `JOIN players ON id = player_id` missing `AND sport`; every live join now carries the
  `sport` predicate, and the autocomplete bundle (regenerated 2026-06-08) is clean. "Mallory"
  appears **0** times in any football surface. No code change required for the swap itself.
- **Found + fixed the real residual defect:** 153 football players carried a trailing
  non-breaking space (U+00A0) in `players.name` (Harry Kane, Kevin De Bruyne, Jordan
  Henderson, …). Source: `sportmonks_football._parse_player` used `display_name` verbatim;
  only the firstname+lastname fallback path called `.strip()`. The nbsp survived into the DB
  and rendered as a stray space (e.g. share text "Check out Harry Kane 's report").
  - **Seeder fix (forward):** strip `display_name` in `_parse_player`. `str.strip()` removes
    nbsp (it counts as whitespace). + 4 unit tests (`tests/test_sportmonks_football.py`).
  - **Migration 075 (backfill):** `btrim` over {space, nbsp, tab, LF, CR} on every dirty row.
    nbsp was purely leading/trailing here (0 internal), so btrim exactly replicates
    `str.strip()`. Data-only — no function/column change, no recompute, no API restart.

## Result
0 dirty player names remain (was 153). `players.name` for 997 is now `'Harry Kane'`. NBA/NFL
were already clean; the WHERE predicate self-limits to dirty rows, so the migration is
sport-agnostic.

## Follow-ups flagged (not done here)
- **Stale `players.team_id` on transfer:** 1,763 / 8,228 football players (21%) have
  `players.team_id` differing from their latest-season `player_stats.team_id` (Kane =
  Tottenham(6) in `players`, Bayern(503) in stats). `/meta` therefore shows the wrong/old club
  in the header for transferred players. Semantically ambiguous (current-registration vs
  latest-stats-season) — needs a product decision before fixing.

## Verification
Dry-run (COMMIT→ROLLBACK): UPDATE 153, gate OK, rollback left 153 dirty (no commit). Applied
via `./sql/migrate.sh`: UPDATE 153, gate `075 OK`, post-apply dirty count = 0. Seeder tests:
4 passed.

## Downstream (operational — NOT in the migration)
`/meta` reads `players.name` via the **`football.autofill_entities` materialized view**, not
the base table — so the cleaned names only surface after a matview refresh. Ran
`REFRESH MATERIALIZED VIEW CONCURRENTLY football.autofill_entities` + restarted the API to
drop its 5-min response cache; `/meta` then returns `'Harry Kane'`. (Not folded into the
migration: matview refresh is point-in-time, normally fires on the next fixture-ingestion
NOTIFY, and clone-based fresh envs inherit the refreshed state via `build.sh`.) The frontend
autocomplete bundle is regenerated + redeployed separately (frontend repo). NOTE:
`players.meta->>'display_name'` still holds the raw nbsp value — harmless, nothing renders it
(the matview + bundle use `name`); preserved as the raw provider value.
