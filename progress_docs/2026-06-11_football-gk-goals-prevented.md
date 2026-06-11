# 066 — Football goalkeepers: Goals Prevented core

**Date:** 2026-06-11

## Goal

The 065 GK composite (raw saves + distribution + long-ball + High Claims) failed the eye
test — top keepers were High-Claims merchants from mid-table clubs, marquee keepers
mid-pack. Fix it: rate keepers on their three actual jobs, with shot-stopping measured by
*quality*, not volume.

## What was done

- **Shot-Stopping (raw saves) → Goals Prevented** = `saves − (saves + goals_conceded) ×
  league-avg save%`. Save% expressed as goals stopped above an average keeper, weighted
  by shots actually faced — credits quality AND volume (honours the barrage-stopper too).
  league-avg save% is a per-season structural constant, computed + injected into
  `_compute_rating_bundle` (alongside the 064 opponent-possession injection) — no new
  derived field, no backfill.
- **Dropped High Claims** — the keeper's Blocks/Clearances: weakest metric (rel .45,
  value +.13 perverse) and fat-tailed (max z 4.07), so cross-claimers on weak teams
  dominated. Stays fat-tailed even possession-adjusted.
- **GK composite = Goals Prevented + Distribution (pass accuracy) + Long-Ball Accuracy.**

**Documented reliability exception:** Goals Prevented YoY ≈ 0.156 (vs save% 0.079, saves
0.141) — low, because keeper shot-stopping intrinsically regresses (no post-shot xG to
isolate skill). Included anyway on principle: shot-stopping is the keeper's defining job;
measuring it imperfectly beats omitting it.

## Files changed

- `sql/migrations/066_football_gk_goals_prevented.sql` (new)

## Verification

- Clone + prod dry-run green; gate passes (105 keepers on the core 3; GP spread non-
  degenerate, confirming the save% injection).
- Modelled top-10 is credible — Maignan (+10.5 GP), Provedel (+10.2), Donnarumma, Neuer,
  Oblak, Joan García, Sommer; claims-merchants gone. GK rank spread 5.7–92.9.
- Note: Goals Prevented correctly rates reputation-elite-but-mediocre-this-season keepers
  lower (Alisson save% 64.8, Raya 69.8) — data over reputation.
- No API restart; no frontend change (new GK labels render generically). NBA/NFL untouched.

## Result

Goalkeepers are now rated on the three things a keeper actually does — stopping shots
(by quality), short distribution, and long distribution — and the leaderboard finally
matches the eye test where the eye test is right.
