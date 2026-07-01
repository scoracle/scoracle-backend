# D1 — rating_sigil* → rating_peak* wire rename (peak datapoint)

**Date:** 2026-06-18
**Scope:** Rename the peak-skill datapoint's WIRE name off the overloaded "sigil"
(which now means the crown synthesis) to "peak". Deployed both repos.

## What Was Done (backend)
- `db.go`: every `AS rating_sigil*` read-layer alias → `AS rating_peak*` (engine source column
  stays `rating_specialist*` — never physically renamed, per the late-bound PL/pgSQL constraint),
  across all rating-bearing statements (leaderboard, entity_stats, entity_sigil(/rating), roster,
  event series) + the output column lists that reference the aliased names.
- The nested `rating_modes` jsonb keys remapped from `'sigil'/'sigil_rank'/'sigil_score'/'sigil_label'`
  → `'peak'/'peak_rank'/'peak_score'/'peak_label'` (the per-rate-mode peak re-pick).

## Left intentionally as-is (NOT part of the wire rename)
- The leaderboard **scope** alias `IN ('composite','sigil',…)` — a vestigial board-selector value.
- `sigil_synthesis` table, `/sigil` page, `entity_sigil` statement key — the CROWN (correct).
- `divined_sigil` column + the local model `SIGIL:` prompt marker — a SEPARATE column (the divined
  strength label); renaming it is an engine migration, out of D1's wire-field scope.

## Verification
`go build` clean; deployed (restart, healthy). Prod wire: `/rating` top-level
`rating_peak_score=59.3 / rating_peak_label=Playmaking`, nested `per_36 → peak/peak_rank/peak_score/
peak_label`, roster row `rating_peak_score=72.8` (Luka) — zero `sigil` keys remain. `/rating /sigil
/momentum` all 200.

## Result
"sigil" no longer means "peak strength" anywhere on the wire — it now exclusively names the crown.
Closes the last item of the Sigil convergence tail.
