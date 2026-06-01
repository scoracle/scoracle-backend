# 2026-05-31 — Scoracle Rating Engine: the keystone (z-score positionless)

## Goal
Replace the shipped composite (audited as a scoring-volume metric) with a principled,
bias-free player rating that works across NBA, football, and NFL — from public-domain
box scores only.

## The landing (canonical spec: `planning_docs/SCORACLE_RATING_ENGINE.md`)
One operation rates every entity, all sports:

> **z-score each de-duped box-score datapoint against the positionless population.
> Composite = sum of z (breadth). Specialist = peak z + skill label
> (irreplaceability). No weighting, no gating, no hand-picked baselines.**

The founding intuition — **positionless rating** — turned out to be both the goal and
the mechanism: the z-score vs a positionless population inherently rewards scarcity
(rare skills sit further from the mean), so scarcity weighting is automatic and
needs no human input.

## Key decisions / discoveries (the journey)
- **Audit:** the live composite was ~0.90 correlated with scoring volume (a migration
  blew away curated `is_percentile_eligible` flags); 13 collinear scoring stats : 3
  defense : 1 impact.
- **NBA** forced de-dupe + the realization that volume ≠ value.
- **Football** forced the breakthrough: a breadth-mean buries elite specialists
  (Haaland #47→#124 every variant) because football data is ~2:1 defensive. Pushing
  the *Haaland* (pure specialist) case, not Kane (all-rounder), is what birthed the
  Specialist score — which also saves Curry's value in NBA.
- **Two scores, not one:** breadth is a mean, irreplaceability is a max; no weighting
  reconciles them. Proven across CV / CV² / CV³ / sum-VOR — all bury specialists; only
  peak surfaces them.
- **Scarcity = z, derived not chosen.** Tried p90/p50, p90/p75, replacement-margin —
  all either measured the wrong axis or required hand-picking a baseline (and ∞ for
  rare events). The z-score against the positionless population does it implicitly
  with zero knobs. THE keystone.
- **Negatives** = `−z`; usage-expected refinement (the "Cade Cunningham" excess-
  turnover discovery) for usage-bundled negatives; box-score-attribution boundary
  (no apologizing for players via data we don't have).
- **`shots_to_points`** (the house metric) demoted from pillar to stats-page percentile
  (a rate → ~0 z → no rating signal).
- **NFL** (the marquee test, "nothing but specialists"): positionless across all 17
  positions via the exclusive-stats uniform drag; "a yard is a yard" collapses offense
  to total_yards/total_TDs (the *way* is a scope); QBs lead (correct). Specialist
  surfaces return men, edge rushers, ball-hawk DBs — defense valued as no public NFL
  metric does.

## Validation (live, read-only, 2025)
- NBA Composite: Wembanyama, Jokić, Luka, SGA, Maxey, Kawhi, Cade.
  Specialist: Wemby (rim 6.11), Jokić (playmaking), KPJ (steals), Curry (3pt), Luka.
- PL Composite: Bruno Fernandes, E. Anderson, Garner, Senesi, Bowen.
  Specialist: Bruno F (assists 8.90), Haaland (goals 7.02), Tarkowski (blocks), Doku.
- NFL Composite: Marcus Jones, Garrett, Burns, Stafford, Anderson, Crosby, Watt.
  Specialist: return men, Garrett (sacks), Byard (INTs).

## Guards / open items
- Thin-population: `NULLIF(sd,0)` + `COALESCE(z,0)` (one NULL nulled NFL Composite);
  min-participant floor on a stat's inclusion.
- Entity floors: NBA ≥30GP/≥20MPG, FB ≥15 apps, NFL ≥8GP.
- Profile endpoint stays separate/unchanged (absolute percentiles + pizza chart);
  rating engine is a distinct dataset for starline + leaderboards.

## Quick reference
- Canonical: `planning_docs/SCORACLE_RATING_ENGINE.md`
- Journey/rationale: `COMPOSITE_MATRIX_V2.md`, `FOOTBALL_VOR_EXPLORATION.md`,
  `SCARCITY_VALUE_WEIGHTING_LAYER.md`
- Lifecycle (freeze/recompute, O(M²) avoidance): COMPOSITE_MATRIX_V2 §4–5

## Datapoint inclusion — three-gate rule (2026-06-01)
A stat earns a Composite vote only if: (1) corr <~0.7 with every included stat
(distinct concept — z self-weights scarcity but NOT collinearity), (2) healthy
spread (sparse-spiky → Specialist only, never breadth sum), (3) ~full key coverage
(check `stats ? key`, not just nonzero %; COALESCE-to-0 on a missing key silently
docks players).
- **Added:** NBA `pf` (−z discipline; Butler +26, Cade −5); football `fouls_drawn`
  (100% coverage; lifts progressive engines Barco/Enzo).
- **Rejected (volume re-skins, gate 1):** carries↔rush_yds 0.99, targets↔rec 0.99,
  passing_attempts↔yds 1.0, fga↔pts 0.97, oreb↔reb 0.82, shots_on_target↔shots 0.94.
- **Rejected (data, not principle):** NFL `qb_hits` (only 6 nonzero — unseeded).

## PENDING SEEDER FIX — through_balls (and audit siblings)
`through_balls` is a real value stat (line-breaking creativity) and passes gates 1+2,
but only 65% of football players have the KEY (1086/1679). It's a seeding gap, not
true zeros — elite ball-playing CBs (Colwill's full 2024, Dunk, van de Ven) lack the
key entirely, so including it would punish exactly the players it should reward.
**Fix: seeder must emit `through_balls: 0` explicitly for players with none** (and
audit other sparse-coverage keys the same way). Once coverage ~100%, through_balls
passes gate 3 → add to football Composite.

## Next
Build phase: SQL (event derivations → z Composite/Specialist + label, freeze/
recompute), leaderboard + starline endpoints, ENDPOINTS/README/Swagger. Plus the
seeder coverage fix above. NOT yet built — design only.
