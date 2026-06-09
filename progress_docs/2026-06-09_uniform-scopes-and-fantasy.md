# 2026-06-09 — Uniform per-X scopes (045) + Fantasy points (046)

Two related additions to the PLAYER rating data spine. Both additive; both gated.

## Goals
1. **Uniform scope vocabulary** — every sport speaks Per Season / Per Game / Per-X.
   NBA stored only per-game averages (+ per_36) with no season-total mode; football
   had season totals (+ per_90) but no per-game; NFL already had both. Close the gaps
   so total seasonal value (durability) is expressible (migration 045).
2. **Fantasy points** — box-score-derived fantasy scoring as a first-class derived
   stat, with a per-X rate family, as the spine for the Regular | Fantasy selector and
   the fantasy leaderboard/roster (migration 046).

## Decisions
- **Reuse the 042 per-X machinery.** New modes = new per-row sibling + a `v_modes`
  entry. `rating_datapoints` gains a second per-row literal `rate_key2` (the new
  mode's sibling); the wrapping CASE routes total→raw, per_36/per_90→rate_key, the new
  mode→rate_key2. NFL unchanged (its `total` already = season totals; no snap data → no
  per-x). `_compute_rating_bundle` (043) untouched.
- **NBA per_season = avg × games_played; football per_game = total ÷ appearances** —
  both derivable in the existing BEFORE-trigger loops, no event re-aggregation.
- **Fantasy points live in the DERIVED-STATS TRIGGER**, not the aggregator: it's the
  right home for a derived stat, it backfills by re-firing the trigger (no
  re-aggregation), and adding `'fantasy_points'` to each sport's rate-key array yields
  the rate siblings for free. Presets: PPR (NFL, season totals), DraftKings (NBA,
  per-game; DD/TD bonus omitted — unreconstructable from averages).
- **Fantasy is a points headline, NOT a z-datapoint** — the Specialist peak-z logic is
  untouched; `recalculate_percentiles` ranks `fantasy_points` for free (the headline
  rank). The sparkline payload gains a `fantasy` block via `public.fantasy_block()`,
  pre-expanded by rate mode (pure passthrough, like rating_modes).
- **Notifications skip per-rate SIBLING keys** (`…_per_36/_per_90/_per_game/_per_season`)
  in both `notify_percentile_changed` and the Go `catchUpSweep` — a player elite in
  "Points" shouldn't also fire "Points Per 36"; this silences the fantasy siblings
  while keeping base `fantasy_points` notifiable. The milestone trigger is disabled
  during 046's bulk recalc to avoid a one-time storm.
- **In-transaction PARITY GATE (045)** aborts on any drift of default columns OR the
  existing per_36/per_90/per_game blocks; smoke-asserts the new modes move composites.
  046 smoke-asserts fantasy_points populated + ranked.

## Accomplishments
- `sql/migrations/045_uniform_scopes.sql` — stat_defs + both derived-trigger redefs +
  `rating_datapoints` (rate_key2) + `compute_rating` (v_modes: NBA `+per_season`, FB
  `+per_game`) + backfill (re-fire triggers → recompute) + parity gate.
- `sql/migrations/046_fantasy_points.sql` — `nba/nfl.fantasy_points` + `fantasy_block`
  + stat_defs + trigger fantasy integration + notify sibling-skip + backfill (re-fire
  → `recalculate_percentiles`, milestone trigger disabled) + smoke gate.
- Canonical sync: `sql/nba.sql` (per_season + fantasy), `sql/football.sql` (per_game),
  `sql/nfl.sql` (fantasy), `sql/shared.sql` (`fantasy_block` + notify skip).
- `go/internal/db/db.go` — `sparkline` statement serves the `fantasy` block (player
  branch via `public.fantasy_block`; NULL for teams). Thin passthrough.
- `go/internal/maintenance/maintenance.go` — `catchUpSweep` skips per-rate siblings.

## Verification
- Throwaway local PG (Postgres 18): `rating_datapoints` returns the correct sibling for
  every mode/sport incl. `tov_*`/`shots_*` aliases + mode-invariant rows; real triggers
  produce correct values (`pts_per_season`=2000, `goals_per_game`=0.526,
  `fantasy_points` NBA 54.5 / NFL QB 286 / RB 284, `fp_per_season`=4360, `fp_per_game`=16.82);
  `fantasy_block` shape + NULL-when-absent; notify regex excludes siblings only.
- `gofmt` clean, `go build ./...` + `go vet` clean.
- NOT yet applied to prod — apply order **045 → 046** (046's trigger supersedes 045's;
  reverse clobbers fantasy), each dry-run via COMMIT→ROLLBACK like 042, then deploy Go
  (needs `public.fantasy_block` for PREPARE).

## Quick reference
- New rate modes: `rating_modes->'per_season'` (NBA), `->'per_game'` (FB). Default =
  base columns (NBA per-game / FB+NFL season totals).
- Fantasy: `stats->>'fantasy_points'` (+ `_per_game`/`_per_season`/`_per_36` siblings);
  ranked in `percentiles`/`scoped_percentiles`; served as the sparkline `fantasy` block.
- Frontend uniform labels: NBA Per Season/Per Game/Per 36; FB Per Season/Per Game/Per 90;
  NFL Per Season/Per Game.
