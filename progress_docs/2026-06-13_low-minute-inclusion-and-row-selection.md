# Missing entity data: appearances gate → 10 + multi-league row selection

## Goal
Two "no stats reporting" misses Scott flagged: Roméo Lavia (id 37536874, 2025: 12 apps,
382 min, full stats) and Luka Vušković (id 37657587, played 2,400 min / 6 goals) — both
showing empty profiles.

## Root causes (two distinct)
1. **Rating-eligibility gate.** `public.rating_thresholds` required FOOTBALL `appearances >= 15`
   (applied by `_compute_rating_bundle` 067 + `recalculate_percentiles` 058). It dropped ~5,966
   football player-seasons that HAVE full stats — incl. Roméo (12 < 15) → `rating_composite=NULL`.
2. **Multi-league row selection.** Luka IS rated (league 82, his loan/played club). But the
   `football_profile_page` + `sparkline` reads selected a player's season row with no league
   tiebreak (`ORDER BY season DESC LIMIT 1` / `) u LIMIT 1`), so they surfaced his EMPTY league-8
   (Tottenham parent-club registration) row → looked unrated.

## What was done
- **078** lowered the gate 15→3; a rolled-back impact preview (≥1/≥3/≥5) showed cohort inflation.
- **079** set it to **10 appearances** (Scott: ">25% of a 38-game PL season" — a meaningful
  impact line). Recomputed all football seasons (lock-free, `session_replication_role`). NOT
  parity-preserving by design (the rated cohort grows by ~288 in 2025; existing mid-tier ranks
  shift up modestly). Roméo Lavia now rated (rank 6.6 — low on totals, as expected; per-90 lifts
  standouts). This is Phase 1 of the agreed "gated composite + data-for-everyone" model.
- **db.go** — `football_profile_page` now tiebreaks `… , COALESCE((p.stats->>'appearances')::numeric,0) DESC`
  (prefer the played row); `sparkline` season_rating collapses with `ORDER BY rating_composite DESC
  NULLS LAST LIMIT 1`. Luka now surfaces league 82 (rank 72.3, 15-datapoint breakdown).

## Verification
- `go build`/`vet` clean; API restarted (health 200, no prepared-stmt errors).
- Live: Roméo `…/player/37536874/sparkline` rated rank 6.6; Luka `…/player/37657587/sparkline`
  league 82 rank 72.3 (was the empty league-8 row).

## Next
Phase 2 — data-for-everyone: emit datapoints for sub-10-appearance players (z vs the rated
cohort) with an "unranked · low minutes" state, no rank, zero cohort inflation.
