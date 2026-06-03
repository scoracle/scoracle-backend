# Rating Datapoint Audit — Scoracle z-engine vs. a "traditional" z-score

**Status:** DRAFT v1 — 2026-06-02. Read-only audit, no engine changes. Pairs with
the canonical spec (`SCORACLE_RATING_ENGINE.md`). Purpose: lay every datapoint on the
table, per sport, for **both players and teams**, and show exactly where Scoracle
diverges from the naive/traditional approach and **why** (the three-gate de-dupe rule).

> **The one contrast this audit measures.** A *traditional* z-score standardizes the
> **whole box-score line** — every column a provider ships — and sums it (or sums a
> hand-weighted subset). Scoracle standardizes only the **de-duped, distinct-concept
> set** that survives the three gates (§1.2 of the spec): (1) correlation < ~0.7 with
> every already-included stat, (2) healthy spread (not sparse-spiky), (3) explicit-zero
> coverage (provider emits 0, not just-when-nonzero). Same operation (z); different,
> disciplined **input set**. This doc is the side-by-side.

Legend for the tables: **C** = in Composite (breadth, Σz), **S** = in Specialist pool
(peak-z), **−** = enters as `−z` (lower is better). "Trad?" = would a conventional
z-score include it. "Verdict" = why we keep / drop / demote.

---

## NBA

**Traditional anchor — the literal "9-category z-score"** (the de-facto standard in
fantasy/analytics): `pts, reb, ast, stl, blk, 3PM, FG%, FT%, turnovers`. Hollinger's
**Game Score / PER** standardize even more (fgm, fga, fta, ftm, oreb, dreb, pf, misses).

### Players (current z-set: 9 datapoints, flat-z)

| Datapoint | Scoracle | Trad 9-cat? | Verdict |
|---|---|---|---|
| Scoring (`pts`) | C, S | ✅ | Keep — core production. |
| Rebounding (`reb`) | C, S | ✅ | Keep — total only; `oreb`/`dreb` are 0.82 collinear re-skins (gate 1). |
| Playmaking (`ast`) | C, S | ✅ | Keep. |
| Steals (`stl`) | C, S | ✅ | Keep. |
| Rim Protection (`blk`) | C, S | ✅ | Keep. |
| 3PT Shooting (`fg3m`) | C, S | ✅ (3PM) | Keep — made, not attempted; distinct from `pts`. |
| On-Court Impact (`plus_minus`) | C | ❌ | **Scoracle-specific** signed-impact term. Not in trad box z. |
| Ball Security (`−turnover`) | C (−) | ✅ | Keep as `−z`. |
| Discipline (`−pf`) | C (−) | ❌ (PER only) | **Scoracle add-back** (2026-06-01); distinct (corr 0.34 vs blk). |
| FG% / FT% / TS% / eFG% | — | ✅ (rates) | **Demoted to stats-page percentile** — tight symmetric dists → ~0 z → no signal. |
| `fga, fgm, fg3a, fta` | — | Game Score | **Dropped** — volume re-skins of `pts` (fga↔pts 0.97), gate 1. |
| `oreb, dreb` | — | Game Score | **Dropped** — collinear with `reb` (gate 1). |
| **`fta` (Foul Drawing / Rim Pressure)** | *MEASURED 2026-06-03* | ❌ (9-cat uses FT%) | Chose `fta` over `ftm` (attempts isolate *drawing contact* from FT conversion — credits poor-FT% rim attackers; `corr(fta,ftm)=0.987` so the swap is a purity win, not a collinearity change). **Player gate-1: `corr(fta,pts)=0.870` (FAILS <0.7).** In the composite breadth sum it's a `pts` re-skin → **player treatment = Specialist + pizza percentile only (`in_spec`, NOT `in_comp`)**. Top-FTA = top scorers (Luka/SGA/Embiid), distinct value isolated to bigs like Giannis (64.6% FT)/Zion (72%). |

### Teams (current z-set: 8 datapoints, flat-z, **no categories yet**)

| Datapoint | Scoracle | Verdict / category target |
|---|---|---|
| Scoring (`pts`) | C, S | → **offense** |
| Playmaking (`ast`) | C, S | → **offense** |
| 3PT Shooting (`fg3m`) | C, S | → **offense** |
| Ball Security (`−turnover`) | C (−) | → **offense** (you turn it over on offense) |
| Rebounding (`reb`) | C, S | → ambiguous (oreb=off, dreb=def); currently total |
| Steals (`stl`) | C, S | → **defense** |
| Rim Protection (`blk`) | C, S | → **defense** |
| Point Differential | C | → **overall/result** (margin — fits neither bucket cleanly) |
| **Gap:** team defense is thin (only `stl`,`blk`). `pts_allowed` is available and a strong team-defense signal — candidate `−Points Allowed` to make the *defense* category real. |

---

## NFL

**Traditional anchor:** no single public z-score. Convention standardizes the
positional box line — yards, TDs, receptions (PPR), INTs-thrown; sacks, tackles, INTs,
TFL; FG made, punts inside-20. Established composites are **charted/paywalled** (PFF
grades, ESPN QBR, AV) — out of scope by the public-domain pillar.

### Players (current z-set: 12 datapoints, **category-balanced** off/def/special)

| Facet | Datapoint | Scoracle | Verdict |
|---|---|---|---|
| OFF | Total Yards (pass+rush+rec+**return**) | C, S | "A yard is a yard" — return yds folded in (no thin standalone slot). |
| OFF | Touchdowns (all incl. return) | C, S | "A TD is a TD." |
| OFF | Receiving (`receptions`) | C, S | Kept (PPR-style distinct volume). |
| OFF | Giveaways (`−(int_thrown+fum_lost)`) | C (−) | Bundled negative. |
| DEF | Tackling (`total_tackles`) | C, S | Provider total; `solo/assist` splits dropped (gate 1). |
| DEF | Tackles For Loss | C, S | Keep. |
| DEF | Sacks (`defensive_sacks`) | C, S | Keep — scarcity-rich (elite z 7.47). |
| DEF | Pass Defense (`passes_defended`) | C, S | Keep. |
| DEF | Interceptions (`defensive_interceptions`) | C, S | Keep. |
| DEF | Fumble Recovery | C, S | Keep. |
| SPC | Field Goals (`field_goals_made`) | C, S | Keep — kicker specialist. |
| SPC | Punting (`punts_inside_20`) | C, S | Keep — punter specialist. |
| — | `carries, targets, *_yards splits, *_TD splits` | — | **Dropped** — 0.88–0.99 collinear with the totals (gate 1). |
| — | `long_*` (longest play) | — | **Dropped** — single-event noise. |
| — | `qbr / qb_rating` | — | **Demoted to stats page** — rate, ~0 z. |
| — | `qb_hits` | — | **Excluded** — effectively unseeded (6 nonzero), gate 2. |

### Teams (current z-set: 7 datapoints, flat-z, **no categories, no special teams**)

| Datapoint | Scoracle | Verdict / category target |
|---|---|---|
| Total Yards | C, S | → **offense** |
| Giveaways (`−turnovers`) | C (−) | → **offense** |
| Tackling (`total_tackles`) | C, S | → **defense** |
| Sacks | C, S | → **defense** |
| Pass Defense | C, S | → **defense** |
| Interceptions | C, S | → **defense** |
| Point Differential | C | → **overall/result** |
| **GAP — special teams has ZERO datapoints.** To honor a *special-teams* category we must ADD from the available team set: `field_goals_made`, `punts_inside_20`, and/or return yardage. Defense could also gain `tackles_for_loss`, `takeaways`, `fumbles_recovered`; offense could gain `touchdowns` (pass+rush) and `first_downs`. |

---

## FOOTBALL (soccer)

**Traditional anchor:** the FBref-style per-90 stat line, standardized — goals, assists,
xG, shots, key passes, passes completed, dribbles, tackles, interceptions, clearances,
blocks, GK saves. FBref's public "percentile vs. positional peers, per-90" is the
closest established analog — and is essentially the **scoped percentile** the product
already wants.

### Players (current z-set: 18 datapoints, flat-z; GK in the same pool)

| Datapoint | Scoracle | Verdict |
|---|---|---|
| Goalscoring (`goals`), Creation (`assists`), Shooting (`shots_total`) | C, S | Keep. |
| Passing (`passes_accurate`), Key Passes (`key_passes`) | C, S | Keep — `chances_created`⊇`big_chances` deduped to `key_passes`. |
| Dribbling (`dribbles_success`), Duels (`duels_won`) | C, S | Keep — `duels_won`⊇aerials (deduped). |
| Tackling, Interceptions, Clearances, Blocks, Ball Recovery | C, S | Keep — defensive breadth. |
| Drawing Fouls (`fouls_drawn`) | C, S | **Add-back** (2026-06-01) — distinct 0.69 vs duels, 100% coverage. |
| Possession Lost (`−possession_lost`) | C (−) | `possession_lost`⊇dispossessed+turnovers (deduped). |
| GK: Shot-Stopping (`saves`), Penalty Saves, Punching, High Claims | C, S | Role-exclusive → scarcity among keepers (NULL for outfield). |
| `through_balls` | — | **DROPPED permanently** — provider omits zeros (gate 3): key present only on its 3,136 nonzero events. |
| `fouls_committed` | — | **Rejected** — same omits-zeros trap (gate 3). |
| `xG`, pass%, shot%, save%, duel% | — | **Stats-page percentile** — rates/charted, ~0 z. |

### Teams (current z-set: 8 datapoints, flat-z, **no categories yet**)

| Datapoint | Scoracle | Verdict / category target |
|---|---|---|
| Goals For (`goals_for`) | C, S | → **attacking** |
| Shooting (`shots_on_target`) | C, S | → **attacking** |
| Creation (`key_passes`) | C, S | → **attacking** |
| Goal Difference | C | → **overall/result** (margin) |
| Possession Lost (`−possession_lost`) | C (−) | → **possession** |
| Tackling (`tackles`) | C, S | → **defense** |
| Interceptions | C, S | → **defense** |
| Clearances | C, S | → **defense** |
| **GAP — "possession" would be just `−possession_lost`.** Available team stats to flesh it out: `accurate_passes`, `possession_pct`, `duels_won`. Attacking could add `assists`, `big_chances_created`, `successful_dribbles`; defense could add `blocked_shots`, `ball_recovery`, `tackles_won`. |

---

## Summary of divergences from "traditional"

**What we DROP that a traditional z-score keeps** (all gate-1 collinearity or
gate-3 coverage):
- NBA: `fga/fgm/fg3a/fta`, `oreb/dreb` (volume re-skins of pts/reb).
- NFL: `carries`, `targets`, per-channel yard/TD splits, `long_*` (re-skins of totals).
- Football: `through_balls`, `fouls_committed` (provider omits zeros — unrecoverable).

**What we DEMOTE to stats-page percentiles** (rates → ~0 z, no ranking signal):
- All shooting/passing/save/duel %s, TS%/eFG%, `qbr`/`qb_rating`, `xG`, `shot_accuracy`.

**What we ADD that a traditional z-score lacks** (distinct value the box score attributes):
- NBA `plus_minus` (on-court impact), `−pf` (discipline).
- Football `fouls_drawn` (contact-drawing aggression).
- Teams: the point/goal **margin** as a result signal.

**Net:** traditional standardizes ~20–60 raw columns and double-counts volume; Scoracle
standardizes a deduped **9 (NBA) / 12 (NFL) / 18 (FOOTBALL)** for players, **8 / 7 / 8**
for teams — every term a distinct concept, every rate moved to the pizza.

---

## Open candidates & gaps this audit surfaces

1. **`fta` (NBA, Foul Drawing / Rim Pressure) — MEASURED 2026-06-03, asymmetric result.**
   - **Teams: `corr(fta,pts)=0.370`** → clean distinct concept → **full include**
     (`in_comp`+`in_spec`, Offense). DECIDED.
   - **Players: `corr(fta,pts)=0.870`** → fails gate-1, a scoring re-skin in the breadth
     sum → **Specialist + pizza percentile only** (`in_spec`, NOT `in_comp`). The
     distinct rim-pressure value (poor-FT% bigs) surfaces as a peak/specialty + slice
     without double-counting `pts`. (Mirror of `plus_minus`: `in_comp` but not `in_spec`.)
   - `corr(fta,ftm)=0.987` — the `ftm`→`fta` swap is a purity win (decouples conversion),
     not a collinearity change.

2. **Team categories — SIMPLIFIED 2026-06-03 to Offense / Defense only (all sports).**
   Special-teams and possession dropped as separate rings → **no new team datapoints
   required beyond `fta`** (no NFL FG/punt terms, no football `accurate_passes`). Every
   existing team term is tagged `offense` or `defense`; the **margin** (Point Diff / Goal
   Diff) gets facet `overall` — a composite contributor shown as a **headline**, not a
   ring. Optional future enrichments (deferred): `−pts_allowed` (NBA defense),
   `touchdowns` (NFL offense), `accurate_passes`/`possession_pct` (football offense).
3. **Adding team datapoints changes the team boards.** Any new term re-ranks the
   validated team Composite. Decide whether categories stay **display-only** (composite
   math frozen, per-category sub-scores added for the pizza) or become a **balanced
   composite** (equal weight per category, re-ranking the boards). See spec §1.5.
