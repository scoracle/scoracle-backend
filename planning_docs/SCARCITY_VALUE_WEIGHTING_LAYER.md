# Scarcity / Value Weighting — the cross-sport overlay

**Status:** Core principle **LOCKED (2026-05-31)**. Powers the **Specialist** score
(live in the design). Also available as an **optional weighted-General layer**
(deferred — General ships unweighted first, this is the documented upgrade path).

**The thesis (the breakthrough):** *A rating measures **value**, not **volume**.*
Anyone can rank by counting stats. Value comes from weighting each datapoint by how
**scarce / hard-to-replicate** elite production in it is. A goal is worth more than a
pass not by editorial fiat but because elite goalscoring is *rarer* than elite
passing — and the data says exactly how much rarer. This is the unifying idea behind
both scores, applied across all sports.

---

## 1. The scarcity value of a datapoint

For datapoint `i`, over the appropriate population:

```
valueᵢ = p90ᵢ / p50ᵢ        -- "tail ratio": how far the elite (90th pct) sits above the median
```

- High → scarce/valuable (football goals 7.0, NBA blocks 3.33).
- Low → commodity (football duels 1.8, NBA shots_to_points 1.17).
- **Population matters:** football = across all 5 top leagues; NBA = league-season;
  goalkeepers = within keepers (their stats are ≈0 elsewhere).
- Alt measures considered: CV (stddev/mean) — shallower spread, understates the
  elite tail; top-10% share; p90/p33. **p90/p50 chosen** (cleanest elite-vs-typical
  signal). Measure is a tunable knob.

## 2. Eligibility — which datapoints can carry a value

**Only positive counting-production stats** (discrete accumulations where elite ≫
replacement: goals, blocks, tackles, assists, yards, sacks…). **Excluded:**
- **rates / efficiency** (shots_to_points, %s) — bounded, everyone clusters, no
  scarce production (NBA s2p value 1.17);
- **inverse / bad** (turnovers, fouls) — "scarcity of a bad thing" ≠ value;
- **signed / impact** (plus_minus) — mean ≈ 0, ratio undefined (NBA −44).

Excluded stats still contribute to **General** (unweighted) — they're real quality
signals there; they just can't carry a scarcity weight.

## 3. Two ways the overlay is applied

| | formula | aggregation | rewards | status |
|---|---|---|---|---|
| **Specialist** | `MAX over i of (valueᵢ × pctᵢ)` | **peak** | single most-irreplaceable skill | **LIVE** |
| **Weighted General** | `Σ(valueᵢ × pctᵢ) / Σ valueᵢ` | **weighted mean** | breadth of *valuable* contribution | **DEFERRED** |

`pctᵢ` = positionless percent-rank of the player's per-game value of stat i.
Specialist is min-max scaled to [0,100] and carries a **specialty label** (the
argmax stat) + its rarity. (Full Specialist spec in `FOOTBALL_VOR_EXPLORATION.md`.)

## 4. Weighted General — the deferred layer (evidence)

General ships **unweighted** for now (two maximally-distinct scores; zero knobs).
But scarcity-weighting General is validated as a richer alternative when we want it:

- **Does NOT collapse into Specialist** — weighted vs unweighted General corr
  **0.976** (FB) / **0.969** (NBA); it stays "General," only drifting ~0.13 toward
  Specialist (FB Gen↔Spec corr 0.53 → 0.66).
- **Rewards players who are broad AND value-tilted** (live results):
  - Football PL: **Bowen 7→3**, Bruno Fernandes 9→5, Semenyo 19→11 (broad attackers
    whose volume sits in scarce stats); abundant-stat fullbacks (Truffert 10→20) fall.
  - NBA: Şengün 28→12, Derrick White 20→11, Scottie Barnes 15→8, Cade 6→3, Maxey
    8→5 — two-way/complete players rise.

**IMPORTANT — weighted General does NOT make pure specialists "soar" (measured).**
Initial intuition was that Wemby/Gobert would rocket up a weighted General; the data
says **no**: Wemby 4→4 (flat), Gobert 63→73, Curry 25→31, Holmgren 24→35 — the
spiky specialists *fell* or held. Reason: General is a **mean**, so a specialist's
*holes* (zero scoring/playmaking) get up-weighted toward zero right alongside their
elite skill — the mean still counts the gaps. **Only the Specialist score (a PEAK)
makes a one-of-a-kind like Wemby soar** (Specialist 100), because the peak ignores
the holes. So: the Wemby/“most valuable, irreplaceable” intuition is correct, but
it's delivered by **Specialist**, not by weighting General.

**Why weighted General is still the right "value not volume" instinct:** a holding
mid's 2,000 passes and a winger's 8 goals count equally under unweighted breadth;
weighting says the scarce contribution is worth more — which a *value* rating
should. It just expresses as "broad + value-tilted players rise," not "specialists
soar." (NBA's effect is milder than football's — scarcity spread is only ~3× vs ~4×.)

**Why deferred (not wrong, just a tradeoff):**
1. Costs orthogonality — the product's value is two *distinct* lenses; weighting
   pulls General toward Specialist (0.53→0.66).
2. Reintroduces a tuning knob (the value measure) onto General, which is currently
   parameter-free.
3. "Grinder breadth" is the honest meaning of an *unweighted* General; the scarce-
   value story is what Specialist is *for*.

**Design wrinkle to resolve if adopted:** the non-scarcity-eligible General stats
(efficiency, turnover⁻, plus_minus) have no `valueᵢ`. In the validation they were
held at baseline weight 1.0. Options: baseline 1.0 (used) · median value · drop them
from weighted General. Decide at adoption.

## 5. Caveat — scarcity ≠ team-context (a distinct future layer)

Scarcity weighting rewards **value-dense production** (output concentrated in scarce
categories). It is NOT a team-context / opportunity correction. Bowen rises because
his output is value-dense, not because the model adjusts for his weak team. A true
"value over replacement accounting for team strength/role/opportunity" is a
*separate* potential layer (closer to classic VORP). Don't conflate them — this
overlay answers "how scarce is what you produced," not "how much did your situation
help or hurt you."

## 6. Cross-sport scaling

Self-calibrating: each sport's `valueᵢ` is measured from its own distributions.
- NBA: spread mild (blocks 3.33 → s2p 1.17 ≈ 3×) — stats fairly commensurable.
- Football: spread steep (goals 7.0 → duels 1.8 ≈ 4×) — concentrated value.
- NFL (next): expected steepest — a league of specialists; defensive/special-teams
  scarcity is the marquee test of the whole framework.
