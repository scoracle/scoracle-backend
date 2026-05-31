# Football — Value Over Replacement (VOR) — exploration (DEFERRED)

**Status:** **LOCKED (2026-05-31) — two-score model: General (breadth) + Specialist
(scarcity/VOR), shown SEPARATELY, never summed.** Validated on PL / La Liga /
Bundesliga. See "FINAL MODEL" below; earlier exploration retained for rationale.

**Origin:** Surfaced while debugging why every facet-average we built buries elite
goalscorers — Haaland (26 PL goals, Golden Boot) ranked #47 → #124 across every
version. The realization: box-score averaging measures *volume of actions*, but a
striker's worth is in the **scarcity** of what he does, not the count.

## FINAL MODEL — LOCKED (2026-05-31)

Football player rating = **two separate scores, NOT a single blend.** Each answers
a distinct question; both are shown side by side.

**General** (breadth → grinders / all-rounders)
= unweighted mean of percentiles over the de-duped positive datapoints
(`goals, assists, shots_total, passes_accurate, key_passes, dribbles_success,
duels_won, tackles, interceptions, clearances, blocks, ball_recovery`), percentiled
across all five top leagues.

**Specialist** (scarcity / irreplaceability → difference-makers)
= `MAX over datapoints of (value_i × percentile_i)`, min-max scaled 0–100.
`value_i` = the datapoint's scarcity from the value matrix (**p90/p50 tail ratio** —
goals 7.0 … duels 1.8). Surfaces each player's single most-irreplaceable skill.

**Datapoint eligibility — the scarcity pool (cross-sport rule):** Specialist is
computed over **counting-production datapoints only** — discrete accumulations where
elite ≫ replacement (goals, blocks, tackles, assists, yards, sacks…). **Excluded
from scarcity, kept in General only:**
- *rates / efficiency* (shots_to_points, shooting %, pass accuracy) — commodities,
  no spread (NBA shots_to_points scarcity 1.17), so no scarce "production";
- *inverse / bad* stats (turnovers, fouls) — "scarcity of a bad thing" ≠ value;
- *signed / impact* stats (plus_minus) — mean ≈ 0, the ratio is undefined.

General uses the **full** de-duped set (production + efficiency + inverse + impact);
Specialist uses the **production subset**. (NBA scarcity pool = `pts, reb, ast, stl,
blk, fg3m`.)

**Specialty label:** each Specialist score carries the skill that drove it (the
`value × percentile` argmax) plus that skill's rarity — e.g. "Wembanyama — 100,
*rim protection* (3.3×-rare)", "Curry — *shooting* (2.3×)". A single cross-skill
Specialist *number* collapses to the scarcest skill (NBA: top is all rim-protection),
so present **per-skill boards + the label**, not one unified figure.

**Validated & shippable (NBA + Football, 2026-05-31):** both sports behave
identically. The unified Specialist *leaderboard* collapses to the single scarcest
skill (FB all-`goals`, NBA all-`rim protection`), so it is **not** shipped as a
board. What ships:
1. **General leaderboard** — the all-rounders.
2. **Per-skill specialist boards** — recognizable & diverse (FB: Kane goals · Van
   Dijk clearances · Yamal dribbling · Bruno F assists/key-passes · Mbappé shots ·
   Tarkowski blocks · Garner tackles; NBA: Curry shooting · Jokić playmaking ·
   Wembanyama rim protection · KPJ perimeter-D).
3. **Player card** = General + Specialist + specialty label
   ("Haaland — General 57 / Specialist 100, *goals*").

Small-sample floor: NBA ≥20 MPG, Football ≥15 apps (tunable).

**Goalkeepers — RESOLVED (2026-05-31):** keepers do NOT fold into the outfield
scarcity pool. Validated: save *volume* isn't scarce (1.35 among keepers — every
starter makes ~90–150 saves) and the real skill (`save_pct`) is a bounded rate, so
keepers can't earn a cross-position Specialist. Since keepers **share no stats with
outfield**, they get their **own within-keeper board**: a keeper General over GK
skills (`save_pct`, `saves`, `good_high_claim`, `penalties_saved`, distribution) +
per-GK-skill specialists (best save% / claimer / penalty-saver). Validated — top 10
= Bounou, de Gea, ter Stegen, Donnarumma, Oblak, Unai Simón, Maignan, Diogo Costa.
Outfield two-score excludes keepers entirely; keepers ranked among keepers. (This
supersedes the earlier "GK spikes the outfield Specialist" idea.)

**Do NOT sum them into an "Overall."** The sum re-introduces the breadth bias
(General drags pure finishers down) — ranking La Liga by General+Specialist dropped
**Mbappé and Lewandowski out of the top 10.** Present the two scores side by side.

**Why two scores (proven, not a cop-out):** breadth is a *mean*, irreplaceability is
a *max*; no weighting reconciles them — sum-VOR and CV / CV² / CV³-weighted means
all bury specialists (Haaland #124–#187); only **max** surfaces them (#4). They are
mathematically distinct questions.

**Validation — Specialist lens, 2025:**
- La Liga: Mbappé, Muriqi, **Yamal**, F. Torres, Budimir, Vinícius, Oyarzabal,
  **Lewandowski** — the elite finishers.
- Bundesliga: **Kane**, Undav, Guirassy, Díaz, Olise, Schick — ditto.
- General lens surfaces the all-rounders (Valverde, Grimaldo, Romero, Coufal).

**Scales to all sports:** NBA and NFL get General + Specialist too. NBA's Specialist
spread is mild (commensurable stats); football/NFL steep. NFL defensive specialists
(shutdown CB, edge rusher) will own *Specialist* — the original NFL hunch.

**Still open:** goalkeepers (the §9 exclusive-stats trick — separate build); the
exact de-duped datapoint set + value measure (p90/p50 vs concentration) are
tunable; implementation (SQL functions, columns, leaderboard endpoint).

## Hybrid direction (the football engine)

Football's rating = **hybrid of the percentile matrix (breadth) + VOR (scarcity)** —
not pure VOR, not the crude `attacking ×2` weighting (dropped).

- **Matrix (breadth) credits the workmen** — all-around defensive/involvement work
  (Bruno G, defenders, do-everything mids). Unweighted percentile composite over
  the non-overlapping datapoints.
- **VOR (scarcity) credits the irreplaceable** — rare, decisive skills (finishing,
  elite creation). Weights *emerge from the data* (replacement level + CV), so this
  is principled scarcity weighting, not a gut knob.
- A **complete** player (elite skill *and* all-around) tops both halves → tops the
  board. That's the sweet spot.

**Two ways to combine (decide next session):**
1. **Two blended scores** — a Matrix/"All-Around" score + a VOR/"Impact" score,
   each 0–100, blended into the headline and shown as sub-scores for transparency.
   Interpretable; the blend share is the one remaining knob.
2. **VOR as facets inside the matrix** — add VOR-of-scarce-skills (goal-VOR,
   assist-VOR, …) as datapoints next to the breadth percentiles. One engine, no
   blend knob; scarcity rides in via the steep VOR distributions. (Mild correlation
   with the raw facets — arguably the point.)

Lean: prototype (1) first; fall back to (2) if the blend share feels like tuning.

## The idea

NBA carries an implicit value-over-replacement notion (VORP) — a player's worth
measured against what a freely-available "replacement-level" player would give at
the same position. Football (and our model) has no equivalent. That gap is exactly
what buries the Haaland archetype.

**Core insight — replacement level varies wildly by skill in football:**
- A **replacement-level goalscorer** (25+ in a top league) essentially *does not
  exist* — elite finishing is the rarest, hardest-to-replace skill in the sport.
- A **replacement-level passer / tackler / duel-winner** is cheap and abundant —
  plenty of players post strong passing and defensive volumes.

So Haaland's 26 goals carry enormous *value over replacement*; a midfielder's 2,000
passes carry little (replaceable). VOR captures what facet-averaging cannot: the
**hardest-to-replace** production is the most valuable.

## Why VOR is the principled answer to "how much to weight"

The whole football fight has been about weighting (defense has more distinct
datapoints — 8 vs ~4 attacking — so how much do we boost attacking?). VOR derives
the weights from data instead of gut: **weight each datapoint by the scarcity /
non-replaceability of elite production in it.** Goalscoring has a steep, long-tailed
distribution (few elites) → high weight. Passing/defending is dense (many competent)
→ low weight. The "2× attacking" hack we're testing now is a crude proxy for what
VOR would do rigorously.

## Proof of concept (2026-05-31, live data — top-5 leagues, 2025)

**Scarcity confirmed — goals/assists are the least-replaceable skills.** CV
(stddev ÷ mean; higher = production concentrated in a few players) across
outfielders, apps≥15:

| skill | CV | max ÷ mean |
|---|---|---|
| goals | **1.27** | 11× |
| assists | **1.13** | 11× |
| dribbles_success | 0.95 | 10× |
| key_passes | 0.82 | 7× |
| interceptions | 0.76 | 5× |
| passes_accurate | 0.69 | 4× |
| tackles | 0.62 | 4× |
| duels_won | **0.50** | 3× |

The skills football tracks in *bulk* (duels, tackles, passes) are the *most
replaceable*; the scarce, decisive skills (goals, assists) are tracked sparsely.
That data-supply imbalance is the root of the breadth-average's failure — and
exactly what VOR corrects.

**Goal-VOR works.** Replacement striker = p25 of attackers = **2.0 goals**
(median striker 4.0). Top goal-VOR, 2025:

| player | goals | goal-VOR | × median |
|---|---|---|---|
| Harry Kane | 29 | 27.0 | 7.3× |
| **Erling Haaland** | 26 | **24.0** | 6.5× |
| Kylian Mbappé | 22 | 20.0 | 5.5× |

VOR puts the elite finishers **on top** — the exact players the facet-average
buried at #47–#124, on the *same data*. (POC is goals-only; full VOR = multi-stat,
position-aware replacement baselines — the deferred build.)

**Framing:** VOR is to football what `shots_to_points` is to NBA — the house
derivative that defines the sport's value signature. We have all five top leagues,
which is the sample needed for stable replacement baselines.

## Rough approach (to flesh out later)

1. **Define replacement level** per position per stat — e.g. a baseline regular
   starter (~20th-percentile starter, or a "freely available" benchmark).
2. **Value = production − replacement level**, per stat, then aggregate.
3. **Scarcity weighting falls out naturally:** stats with steep top-ends (goals)
   reward elites heavily per unit; flat-distribution stats (passes) reward little.
   This sharpens the long-tail, low-frequency, high-value events (goals) that
   percentile-averaging flattens to "one vote."
4. **Position-aware replacement:** "replacement goals" measured among comparable
   attacking roles, etc. (ties into the position-scope work).

## Open questions
- What *is* replacement level in our data? (lowest regular starter? fixed
  percentile? positional baseline?)
- Per-90 vs totals for the baseline (sub-minutes normalization).
- Does VOR *replace* the percentile matrix, *weight* it, or run *parallel* to it?
- Cross-position comparability — a striker's goal-VOR vs a CB's defensive-VOR on a
  common scale.

## Relationship to the main work
Alternative/complementary engine to `COMPOSITE_MATRIX_V2.md`'s percentile-average.
The matrix answers "how good across facets"; VOR answers "how irreplaceable." For
football specifically VOR may be the *truer* model — it natively values the Haaland
archetype the facet-average structurally can't. Likely lands as either the
**weighting scheme** for the matrix or a **parallel headline metric**. Park until
the football matrix is settled, then evaluate.
