# 2026-04-21 — Python owns provider knowledge (plus X feed + football team stats)

## Goals

Three things from the same session, landed as three commits.

1. **Fix the X feed** — frontend was getting empty `tweets: []` for every sport.
2. **Pull team-level football box-score data** — possession %, corners, attacks,
   etc. Anything SportMonks publishes per-team that players can't sum to.
3. **Relocate stat-key normalization from Postgres into Python** — settle the
   layering so each service owns one job.

## Decisions

- **Python owns provider knowledge; Postgres is provider-agnostic; Go serves.**
  This was the load-bearing architectural call. The `normalize_stat_keys()`
  trigger and `provider_stat_mappings` table leaked strings like `'sportmonks'`
  and `'bdl'` into the DB. Moved to inline `_*_STAT_MAP` consts per handler,
  applied via a trivial `canonicalize(stats, mapping)` helper.
- **Mappings live inline, not in a shared registry.** Each handler is the
  natural owner of its provider's quirks; a shared `stat_keys/*` directory
  would just be the trigger-table pattern with Python syntax.
- **No backfill.** Existing rows already have canonical keys (the old trigger
  was writing them that way). New fields (`possession_pct`, `corners`, etc.)
  only land on rows seeded after the football commit. Full re-seed is the
  mechanism for populating them.
- **Team-level SportMonks stats overwrite player-accumulated sums.** When
  SportMonks publishes `passes` at team level (425) and player rows sum to a
  different value (410) due to subs or missing-lineup edge cases, the team
  value wins. Player accumulation is the fallback for keys SportMonks doesn't
  publish.
- **Possession % uses AVG over matches, not SUM.** Percentages don't sum.
  Everything else aggregates as SUM.
- **`/2/lists/:id/tweets` does not accept `since_id`.** The X API returns HTTP
  400 once a cursor is saved. Removed the parameter; `ON CONFLICT (id)` on
  `tweets.id` handles dedup for free, so re-pulling 100 tweets per refresh
  is harmless and uses the same one API call.
- **News lookback capped at 12 hours** (was escalating 24→48→168). Protects
  the X token budget and keeps the feed current rather than historical.

## Accomplishments

### Commit 1 — `Fix X feed (drop since_id) and trim news lookback to 12h`

- `go/internal/thirdparty/twitter.go` — stop sending `since_id` to
  `/2/lists/:id/tweets`. The cursor is still persisted for telemetry but
  isn't replayed upstream.
- `go/internal/thirdparty/news.go` — `timeWindows = []int{12}` and the
  Google News `when:` parameter now emits `12h` for sub-day windows.

### Commit 2 — `Pull team-level box-score statistics for football`

- `seed/services/event/handlers/sportmonks_football.py` — added `statistics.type`
  to the fixture include; parse `statistics[]` per team via new
  `_extract_team_statistics()`; overlay onto `team_stats_acc` so SportMonks
  values beat player-row sums where both exist.
- `sql/shared.sql` — registered 12 SportMonks team-entity mappings in
  `provider_stat_mappings` (later relocated to Python in commit 3).
- `sql/football.sql` — 17 new team `stat_definitions` (possession_pct, corners,
  attacks, dangerous_attacks, goal_attempts, hit_woodwork, shots_insidebox,
  shots_outsidebox, successful_headers, ball_safe, goal_kicks, free_kicks,
  throw_ins, penalties, injuries, substitutions, team-level assists). Extended
  `aggregate_team_season()` to roll them up.

### Commit 3 — `Move stat-key normalization from Postgres into Python handlers`

- `seed/shared/stat_keys.py` — new. Single function `canonicalize(stats, mapping)`.
  Mapped key wins; fallback is `'-' → '_'`. Doesn't mutate the input.
- `seed/services/event/handlers/sportmonks_football.py` — inline
  `_PLAYER_STAT_MAP` (12 rows), `_STANDINGS_STAT_MAP` (8 rows),
  `_TEAM_STAT_MAP` (12 rows). Canonicalization applied at the flatten step for
  players, the `_extract_team_statistics()` overlay, the `_parse_standing()`
  and `_extract_league_stats()` paths. `team_stats_acc` accumulates raw codes
  and canonicalizes once with the team map at the end — player and team
  canonicalize differently for the same code.
- `seed/services/event/handlers/bdl_nba.py` — inline `_PLAYER_STAT_MAP` and
  `_TEAM_STAT_MAP` (2/4 rows). Canonicalization applied at box-score flatten,
  team-rollup finalization, `_parse_player_stats`, and `_parse_team_stats`.
- `seed/services/event/handlers/bdl_nfl.py` — identical shape to NBA;
  canonicalization also applied to `_parse_player_stats_flat` which previously
  emitted raw BDL keys.
- `sql/shared.sql` — DROP the `normalize_stat_keys()` function, four
  `trg_a_normalize_*` triggers (player_stats, team_stats, event_box_scores,
  event_team_stats), and the `provider_stat_mappings` table. Kept a
  comment-only block in section 7b pointing readers at Python.

## Quick reference

**canonicalize helper (5 lines):**

```python
def canonicalize(stats: dict, mapping: dict[str, str]) -> dict:
    out = {}
    for raw_key, value in stats.items():
        out[mapping.get(raw_key) or raw_key.replace("-", "_")] = value
    return out
```

**When to add a mapping entry:** only when hyphen-to-underscore isn't enough
(e.g. `yellowcards` → `yellow_cards`, `ball-possession` → `possession_pct`) or
when the canonical name differs from the raw (`tov` → `turnover`). Pure-hyphen
codes like `dangerous-attacks` fall through the fallback and need no entry.

**Player vs team maps diverge** when the same raw code canonicalizes
differently per entity type. SportMonks `passes` → `passes_total` for player
but stays `passes` for team (because the team aggregator reads `passes`).
That divergence was the whole reason for the trigger's `entity_type` lookup;
in Python it's just two separate dicts per handler.

**Re-seeding drill** (for populating new fields after a handler change):

```bash
source .venv/bin/activate
scoracle-seed event load-fixtures football --season 2025   # refresh schedule
# flip already-seeded fixtures back to 'completed' so get_pending sees them
psql "$DB" -c "UPDATE fixtures SET status='completed', seed_attempts=0 \
  WHERE sport='FOOTBALL' AND season=2025 AND status='seeded';"
scoracle-seed event process --sport football --season 2025
```

## Updated file layout

```
seed/shared/stat_keys.py              NEW — canonicalize() helper
seed/services/event/handlers/
  sportmonks_football.py              +3 inline stat maps, 4 canonicalize calls
  bdl_nba.py                          +2 inline stat maps, 4 canonicalize calls
  bdl_nfl.py                          +2 inline stat maps, 3 canonicalize calls
sql/shared.sql                        -150 lines (trigger + table + 42 rows)
sql/football.sql                      +75 lines (team stat_defs + aggregator rollup)
go/internal/thirdparty/
  twitter.go                          -1 query param, -1 branch
  news.go                             12h window, hours-granular `when:`
```
