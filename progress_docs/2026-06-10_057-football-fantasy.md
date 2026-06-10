# 057 — Football fantasy (FPL-style) — build-order Phase 4

**Date:** 2026-06-10

## Goals

Football joins the Regular | Fantasy model selector — the last sport without a
fantasy preset. Mechanically the NFL/NBA spine (migration 046): `fantasy_points`
is a derived stat with per-90/per-game rate siblings, ranked by
`recalculate_percentiles`, surfaced through the metadata-driven `public.fantasy_block`.

## Decisions

- **FPL-style, position-dependent, on season totals.** Goal value by position
  group (GK/DEF 6, MID 5, FWD 4), assists 3, GK save credit (1 per 3 saves) +
  penalty saves (5 ea), GK/DEF goals-conceded penalty (−1 per 2), discipline
  deductions (−1 YC, −3 RC, −2 OG, −2 pen miss), and a 2×appearances playing-time
  proxy. `football.fantasy_points(stats, position)` dispatches the group via the
  existing `public.position_group`.
- **Documented approximations** (the NBA DraftKings DD/TD-omission precedent):
  - **Clean sheets** (per-match: shutout while on pitch 60'+) — *omitted*. A season
    `goals_conceded` total can't reconstruct per-match shutouts.
  - **Bonus / BPS** points (per-match judgement) — *omitted*.
  - Appearance points approximated as 2/appearance (the FPL 1-vs-2 sub/60' split is
    per-match). Penalty goals already live inside `goals` → no separate term, no double count.
- **Zero Go changes / no API restart.** The `sparkline` (`public.fantasy_block`) and
  `leaderboard` (`scope='fantasy'` orders by `fantasy_points`) prepared statements are
  already sport-agnostic. Once 057 populates the columns, the running binary serves
  football fantasy immediately — `db.New` prepares nothing new, so no degraded-mode risk.

## What was done

- **`football.fantasy_points(jsonb, text)`** — the position-dependent FPL formula.
- **`football.compute_derived_player_stats`** — emits `fantasy_points` *before*
  `apply_rate_siblings`, so the per_90/per_game siblings fall out of the existing loop.
- **`stat_definitions`** — `fantasy_points` (+ `_per_90`, `_per_game`) rows
  (`category='fantasy'`), and `rate_sibling=TRUE` on `fantasy_points`.
- **Backfill** — `UPDATE player_stats SET stats=stats WHERE sport='FOOTBALL'` re-fires
  the trigger; `recalculate_percentiles` per FOOTBALL season ranks the new keys; the
  milestone NOTIFY trigger is disabled during the bulk recalc.
- Canonical `sql/football.sql` synced to match (function, stat_definitions,
  rate_sibling, trigger).

## Files changed

- `sql/migrations/057_football_fantasy.sql` (new)
- `sql/football.sql` (canonical BASE synced)

## Verification

Local throwaway clone (`scoracle_test`, socket `/tmp/p2pg`), applied via psql:

- Gates green: **5539 nonzero fantasy_points, 5573 ranked, per_90=5545, per_game=5545,
  0 formula drift** (gate 4 asserts every stored value equals
  `football.fantasy_points(stats, position)` — validates the trigger wiring).
- Hand-calc match: GK (ap20, g0, a1, conceded37, saves77, yel0) →
  `2·20 + 3·1 − ⌊37/2⌋ + ⌊77/3⌋ = 40 + 3 − 18 + 25 = 50.00` ✓ (stored 50.00).
- Top fantasy leaders are high-volume attackers + a 21-assist midfielder — FPL-sane.
- `fantasy_block` payload emits all three modes (default/per_game/per_90) with
  `points` + `rank` + `scoped_ranks.position`.
- Sparkline endpoint (`/football/player/154421/sparkline`) returns the fantasy block
  (Haaland: 194 season pts, rank 100, scoped position rank 100).

Note: the local clone has an empty `public.players` meta table, so the *leaderboard*
endpoint returns 0 for every sport/scope locally — verified instead on prod post-deploy
(the Go leaderboard query is untouched; the fantasy board shipped for NBA/NFL in P2b).

## Rollout (pending authorization)

Prod dry-run (COMMIT→ROLLBACK) → `migrate.sh` apply. **No API restart required** (no
Go change, prepared statements unchanged). Frontend cf:deploy flips
`fantasySupported('football')` so the Model selector / Fantasy board / roster column light up.
