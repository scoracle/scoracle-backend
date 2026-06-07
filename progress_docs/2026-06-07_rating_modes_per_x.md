# 2026-06-07 — Per-X rate modes in the rating engine (migration 042)

## Goals
Give the PLAYER rating engine a selectable per-X **rate mode** (NBA per_36 / FB
per_90 / NFL per_game) so the frontend can switch Composite/Specialist between
season totals and a rate-normalized view — chiefly so rookies / injury-shortened
seasons get a fair per-game read. Additive: the default ("total") mode stays
byte-identical; the alternate mode lives in a new `rating_modes` JSONB column.
Teams unchanged (no per-rate derived keys).

## Decisions
- **Default stays current; per-X is opt-in.** Recompute is purely additive — no
  existing composite/rank/leaderboard shifts.
- **DRY engine**: the per-mode pipeline is one helper, `_compute_rating_bundle
  (sport, season, mode)`; `compute_rating` loops modes and routes the bundle
  (`total` → columns, alternates → `rating_modes`).
- **Per-row `rate_key` literals** in `rating_datapoints` (3rd arg `p_rate_mode`)
  — explicit (handles specials `turnover→tov_per_36`, `shots_total→shots_per_90`),
  NULL ⟺ mode-invariant (%, plus_minus, NFL inline-summed rows, sparse FB GK/
  penalty terms with no sibling). NFL SUM rows sum the `_per_game` siblings.
- **In-transaction parity gate** aborts on any drift of the deterministic columns.
- **Latent 039 bug fixed**: `rating_specialty` / breakdown `is_specialty` were
  chosen among z-score ties with NO tiebreaker → flickered on every recompute.
  042 pins a deterministic tiebreaker (`ORDER BY zr DESC, label`) and derives
  `is_specialty` from the same selection. The specialist VALUE (peak zr) is
  unchanged; only the tied LABEL is stabilized (1,313 sparse-player rows).

## Accomplishments
- `sql/migrations/042_rating_modes.sql` — applied to **production**:
  - `ALTER … ADD COLUMN rating_modes JSONB`; 3-arg `rating_datapoints`;
    `_compute_rating_bundle`; rewritten thin `compute_rating`.
  - Recompute over all player (sport, season). Result: **PARITY OK across 39,813
    rows** (composite/ranks/specialist/scoped_ranks/breakdown-sans-isspec all
    byte-identical); `rating_modes` populated for 20,413 rated players (20,410
    differ from total → per-X works); 1,313 benign specialty tie-pins.
  - Rides `finalize_fixture` (already PERFORMs compute_rating) → refreshes on
    each seed; ~2× rating cost but live-season only (prior seasons frozen).
- `go/internal/db/db.go` — `sparkline` statement serves `rating_modes` (player
  branch; NULL for teams), via the existing `row_to_json(season_rating)`. Thin
  passthrough; PREPARE-validated against prod (no degraded-mode risk).

## Verification
- Throwaway local PG: functions compile + run all sports; determinism check =
  0 default drift; the rookie case proven (9-game RB ranks 0.0 by totals, 33.3
  per-game).
- Prod: 3× dry-run (COMMIT→ROLLBACK) iterated the parity gate to green; real
  apply → PARITY OK. Live spot-check: Jokić composite 10.31 (rank 99.6) +
  per_36 7.93 (rank 99.2). 100% of rated players have `rating_modes`.
- `gofmt` + `go build` + `go vet` clean.

## Quick reference
- Alternate mode per sport: `rating_modes->'per_36'|'per_90'|'per_game'` →
  `{composite, composite_rank, specialist, specialist_rank, specialty,
  breakdown, scoped_ranks}`. Default mode = the existing `rating_*` columns.
- Next: frontend rateMode dropdown + card wiring (scoracle-frontend #7).
