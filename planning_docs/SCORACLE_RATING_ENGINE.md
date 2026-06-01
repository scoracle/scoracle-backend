# Scoracle Rating Engine — CANONICAL SPEC (keystone)

**Status:** **LANDED & LOCKED — 2026-05-31.** This is the source of truth for the
Scoracle player rating engine across all sports. Validated on live data: NBA 2025,
Premier League / La Liga / Bundesliga 2025, NFL 2025. The exploration docs
(`COMPOSITE_MATRIX_V2.md`, `FOOTBALL_VOR_EXPLORATION.md`,
`SCARCITY_VALUE_WEIGHTING_LAYER.md`) record the journey; **this doc is the engine.**

---

## 0. The one-line truth

> **Rate every entity positionlessly by the z-score of each de-duped box-score
> datapoint against the whole population. Composite = sum of z (breadth).
> Specialist = peak z + the skill label (irreplaceability). Nothing is weighted.
> Nothing is hand-tuned. The data does all of it.**

The founding intuition — *positionless rating* — turned out to be both the **goal**
and the **mechanism**. The z-score against a positionless population inherently
rewards scarcity (rare skills sit further from the mean), so no scarcity weights, no
replacement baselines, no editorial choices are needed.

---

## 1. Pillars (non-negotiable, all sports)

1. **Event box scores as the base layer.** Bottom-up (event → season), not top-down
   season averages. Public-domain box scores are the *entire* input — the constraint
   that forced this elegance and keeps the system simple and explainable. We give
   derived context; we never import advanced/paywalled data.
2. **De-dupe — the three-gate inclusion rule.** A datapoint earns a **Composite**
   vote ONLY if it passes all three gates. The z-score self-weights for *scarcity*
   but does NOT fix collinearity, sparsity, or coverage gaps — so these gates are
   the human-set method (decided once, applied blind):
   - **(1) Distinct concept** — correlation < ~0.7 with every already-included stat.
     (Kills "13 collinear scoring stats" / carries↔rush_yds 0.99 / targets↔rec 0.99 /
     fga↔pts 0.97 / oreb↔reb 0.82 — all volume re-skins, OUT.)
   - **(2) Healthy spread** — broadly recorded, not sparse-spiky (mostly-zeros makes
     a Composite-sum term behave like a specialist spike). Sparse-but-distinct stats
     may feed **Specialist** (peak rewards spikes) but never the breadth sum.
   - **(3) ~Full coverage** — the key is populated for ~all qualified players, not a
     systematic subset. A missing key + `COALESCE(...,0)` silently docks players whose
     row lacks it. **Verify `stats ? key` coverage, not just nonzero %.**
   Collinear clutter (oreb/dreb under reb; solo/assist under total_tackles; "a yard is
   a yard" → one `total_yards`) is what manufactures false risers/fallers. Kill it.
3. **Positionless base.** Every score answers *"how valuable was this performance,
   regardless of position?"* One pool, every entity vs every other. Position is NEVER
   baked into the base.
4. **Value, not volume — derived by the data, not by us.** The z-score *is* the value
   weighting: a rare skill produces a larger z automatically. Judgment lives only in
   *method* (which datapoints, set once), never per-player.
5. **Two complementary scores, never merged.** Composite (breadth) + Specialist
   (peak). Specialist complements Composite — not derived from it, not summed in.
6. **No weighting, no gating, no hand-picked baselines.** Mean and standard deviation
   are computed, not chosen. (This is the whole breakthrough — see §8 for what it
   replaced.)

---

## 2. The formula

For a sport+season population **P** (positionless: all qualified entities, see
floors §6), for each de-duped datapoint `i`:

```
mean_i = AVG(value_i)            over P
sd_i   = STDDEV_POP(value_i)     over P        -- NULLIF(sd_i, 0) to guard thin stats
z_i(e) = COALESCE( (value_i(e) - mean_i) / sd_i , 0 )      -- non-participant → 0, correct

COMPOSITE(e)  = Σ over all composite datapoints of  z_i(e)        -- breadth → grinders
SPECIALIST(e) = MAX over production datapoints of    z_i(e)        -- peak → difference-makers
specialty(e)  = argmax_i z_i(e)                                    -- the skill label
```

- **Composite** = sum of z over the full de-duped set (counting production +
  signed-impact like `plus_minus` + negatives as `−z`). Rewards breadth.
- **Specialist** = the single highest z over **counting-production** datapoints, with
  the **label** (which stat) attached. Rewards the one most-irreplaceable skill.
  - Under z, the unified Specialist board is **diverse** (peak varies by player) —
    unlike the old p90/p50 version which collapsed to one skill. So a unified
    Specialist leaderboard IS viable now; **per-skill boards + the label** remain the
    richest presentation.
- **Scarcity is automatic.** Rare events are right-skewed → being elite is more SDs
  from the mean. Proven (NFL defenders): elite INT = z 6.07, elite tackle = z 4.05;
  elite sack = z 7.47. No explicit weight applied.

### Datapoint eligibility
- **Composite:** all de-duped datapoints with a meaningful spread — counting
  production, signed-impact (`plus_minus`), and **negatives entered as `−z`**.
- **Specialist:** **counting-production only** (positive, discrete accumulations).
  Exclude signed-impact and negatives ("elite at turnovers" is not a specialty).
- **Rates / efficiency** (`shots_to_points`, shooting %, `save_pct`, `qbr`): excluded
  from BOTH ratings — tight symmetric distributions → ~0 z → no signal — and shown as
  **stats-page percentiles** only (the pizza chart). `shots_to_points` was explicitly
  demoted from a pillar to a stats-page percentile.

### Negatives
Negative-event stats enter Composite as `−z`. For **usage-bundled** negatives
(turnovers, QB interceptions) the baseline may be refined from the flat mean to a
**usage-expected** value (regress the negative on the player's positive/usage stats;
the *excess* is the true negative — the "Cade Cunningham" discovery). Measured to be
a small, edge-refining effect (turnovers are ~0.90 usage-correlated), so the flat
`−z` is the simple default; usage-expected is the documented refinement. Requires a
**position-appropriate usage base** (turnovers↔ball-handling, INTs↔attempts).
**Boundary:** correct confounds the box score *contains* (own usage), never ones it
doesn't record (e.g. receiver drops inflating a QB's INTs — that's paywalled data).
Scoracle values what the official box score attributes; it does not apologize for
players.

---

## 3. Scopes (context layered on the positionless base)

The base is positionless. Scopes re-rank/filter the **same** scores for context —
they never change the base computation:
- **position** — "best center / best keeper / best QB" (this is where GKs, OL, kickers
  get their dedicated view).
- **league** (football), **season**, and the **rate normalizers**: per-36 (NBA),
  per-90 (football), per-game (NFL).

"Best X by position" lives here, NOT in the base. A keeper ranks low on the
positionless base (the box score barely captures keeper value — honest) and surfaces
via the position scope.

---

## 4. Per-sport datapoint sets (validated)

**NBA** (floor: ≥30 GP, ≥20 MPG; population = league-season)
- Composite z-set: `pts, reb, ast, stl, blk, fg3m, plus_minus, −turnover`
- Specialist over: `pts, reb, ast, stl, blk, fg3m` → labels scoring / rebounding /
  playmaking / steals / rim protection / 3pt shooting
- Stats-page only: `shots_to_points`, shooting %s
- Validation: Composite — Wembanyama, Jokić, Luka, SGA, Maxey, Kawhi, Cade.
  Specialist — Wembanyama (rim 6.11), Jokić (playmaking), KPJ (steals), Curry (3pt),
  Luka (scoring). Diverse, labeled, correct.

**NBA add-back (2026-06-01):** `pf` (personal fouls) added to the Composite z-set as
a **negative** (`−z`, defensive discipline). Passes all three gates: distinct
(corr 0.34 vs blk), dense (578 nonzero), full coverage. Validated: top unchanged
(Wemby/Jokić/Luka), sensible nudges (Jimmy Butler +26 for low fouls, Cade −5).
NBA Composite z-set is now: `pts, reb, ast, stl, blk, fg3m, plus_minus, −turnover, −pf`.

**Football** (floor: ≥15 apps; population = top-5 leagues pooled; **GK in the same
pool**)
- De-dupe note: `duels_won` already includes aerials; `possession_lost` ⊇
  dispossessed+turnovers; `chances_created` ⊇ big_chances.
- Composite z-set: `goals, assists, shots_total, passes_accurate, key_passes,
  dribbles_success, duels_won, tackles, interceptions, clearances, blocks,
  ball_recovery, −possession_lost` + GK exclusives `saves, penalties_saved, punches,
  good_high_claim` (uniform drag — outfielders score 0, ranks unmoved).
- Specialist over the positive counting set (GK `saves` etc. included).
- **Add-back (2026-06-01): `fouls_drawn`** — passes all three gates (distinct 0.69
  vs duels; dense, p50≈18; 100% coverage 1679/1679). Rewards contact-drawing
  aggression; helps progressive engines (Barco, Enzo Fernández) who pay in turnovers
  but earn fouls. **ADD to Composite z-set.**
- **BLOCKED on seeder fix: `through_balls`** — distinct (0.59) and dense among present
  rows, BUT only **65% coverage** (1086/1679 have the key). A temporal/positional
  seeding gap, not true zeros: Levi Colwill's full 2024 season (35 apps, 2319 passes —
  elite line-breaking CB) has NO through_balls key, and every top ball-playing CB
  (Dunk, van Hecke, van de Ven) is missing it. Including it now would systematically
  PUNISH ball-playing defenders — the opposite of intent. **Pending: seeder must emit
  `through_balls: 0` explicitly for all players; then it passes gate 3 and goes in
  (Colwill/Barco rise legitimately).**
- Stats-page only: `save_pct`, pass %s, `shot_accuracy`.
- Validation (PL): Composite — Bruno Fernandes, Elliot Anderson, Garner, Senesi,
  Bowen. Specialist — Bruno F (assists 8.90), **Haaland (goals 7.02)**, Tarkowski
  (blocks), Doku (dribbling). GK via position scope.

**NFL** (floor: ≥8 GP; population = league-season; all 17 positions in ONE pool)
- De-dupe: `total_tackles` (not solo/assist splits); **`total_yards` = passing +
  rushing + receiving** ("a yard is a yard"; the *way* is a scope); `total_touchdowns`
  likewise; drop all `long_*`.
- Composite z-set: `total_yards, total_touchdowns, receptions, total_tackles,
  tackles_for_loss, defensive_sacks, passes_defended, defensive_interceptions,
  fumbles_recovered, field_goals_made, punts_inside_20, return_yards,
  −(passing_interceptions + fumbles_lost)`
- Specialist over the positive set → labels total yards / touchdowns / receptions /
  tackles / TFL / sacks / pass defense / interceptions / fumble rec / field goals /
  punting / returns.
- OL stays in the pool (≈0 score — box score doesn't capture them; honest), surfaced
  via position scope. Kickers/punters/returners are pure specialists (low Composite,
  high Specialist) — the GK pattern generalized.
- Validation: Composite — Marcus Jones, Myles Garrett, Brian Burns, Stafford (lone
  QB high), Will Anderson, Maxx Crosby, T.J. Watt. Specialist — return men, Garrett
  (sacks), **Byard (INTs)** — the DB-specialist prediction, confirmed.
- QBs leading the yards datapoint is **correct** (most valuable position; salaries +
  32 starting jobs confirm). Runners/receivers surface via Specialist + position scope.

---

## 5. Product surfaces

- **Starline** (per-event): a Composite and a Specialist value per event → dual
  sparkline showing breadth contribution + irreplaceable-moment contribution per game.
- **Leaderboards:** Composite board (all-rounders/grinders) + Specialist board
  (difference-makers) + per-skill specialist boards. Current-season and historical;
  positionless by default, with position/league/season/rate scopes.
- **Profile endpoint is SEPARATE and unchanged:** absolute per-stat percentiles +
  scopes (position/league/season/per-x) → the pizza chart. The rating engine does NOT
  touch the counting-stats payload. Two independent datasets.

---

## 6. Implementation notes & guards

- **Thin-population guard (required):** `NULLIF(sd,0)` + `COALESCE(z,0)` per term, or
  one NULL nulls the whole sum (hit live in NFL). A stat with <~20 participants can
  throw a freak z — apply a sanity floor / minimum-participant gate on the *stat's*
  inclusion (not on players).
- **Floors** (small-sample, on entities): NBA ≥30 GP & ≥20 MPG; football ≥15 apps;
  NFL ≥8 GP. Tunable. Guards peak-z one-game-wonders (e.g. a returner with one big
  return).
- **Scarcity values measured among participants** where a stat is role-exclusive
  (saves among keepers, etc.) — but the *ranking percentile/z stays positionless*.
- **Pipeline:** event box score → (trigger) derive any needed event values → season
  aggregate → z vs positionless population → Composite/Specialist. Frozen at season
  close; only cross-season percentiles recompute (on rollover). See COMPOSITE_MATRIX_V2
  §4–§5 for the freeze/recompute lifecycle and the O(M²) avoidance.
- Reuses existing `season_composite_score` column lineage (name kept, guts rebuilt).
  New: a specialist score + label column; leaderboard endpoint
  (`/api/v1/{sport}/leaderboard?...`) — **join caveat:** `players` keyed by
  `(id, sport)`; every join needs `AND p.sport = ps.sport`.

---

## 7. Why two scores cannot be one (settled)

Breadth is a **mean/sum**; irreplaceability is a **max**. No weighting reconciles
them — every weighted-mean variant (raw, CV, CV², CV³, sum-VOR) buries specialists;
only the **peak** surfaces them. They answer mathematically distinct questions, so
they ship as two transparent numbers, side by side. Do **not** sum into an "Overall"
(re-introduces the breadth bias).

---

## 8. What this engine REPLACED (journey → keystone)

The exploration converged here; these earlier mechanisms are **superseded**:
- ❌ Hand-picked per-stat weights / "no weighting matrix" (NBA flat-9) → z auto-weights.
- ❌ p90/p50 (and p90/p75) explicit scarcity weighting → z does it implicitly; p90/p75
  also measured the wrong axis (spread among doers, blind to the zero-floor).
- ❌ Hand-picked replacement baselines / `production − replacement` ratios (the ∞
  problem for rare events) → z uses the computed mean; always defined.
- ❌ Game Score / borrowed formulas → rejected (offense-weighted).
- ❌ `shots_to_points` as a pillar → demoted to stats-page percentile.
- ❌ Per-position matrices / separate GK board → positionless pool + position scope.
- ❌ `attacking ×2` / category weighting → not needed.

The destination is **simpler** than any waypoint: one operation (z), zero knobs.

---

## 9. Build status

Design **landed & locked**. Nothing built into the live pipeline yet (still
read-only validated against the DB). Next: SQL implementation (event derivations,
z-based Composite/Specialist + label columns, freeze/recompute lifecycle, leaderboard
+ starline endpoints, ENDPOINTS/README/Swagger per CLAUDE.md).
