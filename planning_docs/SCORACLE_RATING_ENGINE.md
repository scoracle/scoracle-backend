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
   - **(3) Explicit-zero coverage** — the provider must emit the value *including
     zero*, not only-when-nonzero. Check at the EVENT level: if `count(has_key) ==
     count(value>0)`, the provider omits zeros → "absent" is indistinguishable from
     "0" and `COALESCE(...,0)` can't recover the truth → REJECT. (This killed
     `through_balls`: present on exactly its 3,136 nonzero events, absent on 49,943
     zero events.) Verify `stats ? key` coverage, not just nonzero %.
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

## 1.5. Composite aggregation — flat-z vs category-balanced (per-sport, 2026-06-01)

The Composite is `Σ z` (flat) **except where the box score has a structural
phase-skew AND players occupy a single phase** — then it's **category-balanced**:
group datapoints into phase facets, take the **mean of z within each facet**, sum
the facets (equal phase weight). This neutralizes the count asymmetry (a sport that
records 6 defensive stats but 3 offensive ones would otherwise let *recording
granularity* silently weight defense 2×).

**The rule (grounded in player overlap):**
- **Single-phase players → category-balance.** NFL: a corner has zero offensive
  production, a receiver zero defensive. Balancing facets = "a phase of football is a
  phase of football"; each player judged on their one phase at full weight. **Adopted
  for NFL** (offense / defense / special-teams facets). Validated: flat board was
  defense-heavy (top-50 = 7 OFF / 43 DEF; Stafford #4, Puka #41); balanced =
  19 OFF / 31 DEF with Stafford #1, McCaffrey #4, Nacua #7 — reads like a real season.
- **Multi-phase players → flat-z.** NBA & football: every player attacks AND defends
  to some degree. Forcing equal facets there just **rewards whoever touches the most
  facets** — which in football is the all-phase center-back, not the striker.
  Validated: football category-balancing made it WORSE (top-50 attackers 9→4,
  defenders 15→24; Yamal #2→#5, CBs flooded the top). **NBA + football stay flat-z.**
- Category-balancing affects **Composite only**; Specialist is always pure peak-z
  (irreplaceability is phase-agnostic). Football's attacker-vs-grinder tension is
  handled by Specialist (Haaland/Yamal top it), NOT by reshaping Composite.

## 2. The formula

For a sport+season population **P** (positionless: all qualified entities, see
floors §6), for each de-duped datapoint `i`:

```
mean_i = AVG(value_i)            over P
sd_i   = STDDEV_POP(value_i)     over P        -- NULLIF(sd_i, 0) to guard thin stats
z_i(e) = COALESCE( (value_i(e) - mean_i) / sd_i , 0 )      -- non-participant → 0, correct

COMPOSITE(e)  = Σ over all composite datapoints of  z_i(e)        -- breadth → grinders
                 (NFL: category-balanced = Σ over facets of MEAN(z within facet); see §1.5)
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
- **DROPPED permanently: `through_balls`** — NOT a reliable box-score datapoint.
  Traced to source (2026-06-01): SportMonks emits `through-balls` as a match detail
  **only when the value is non-zero** — in raw `event_box_scores` the key is present on
  exactly the 3,136 events where it's >0, and absent on all 49,943 pass-having events
  where it's 0. The provider never sends a zero, so "absent" and "0" are
  indistinguishable, and aggregation can't recover a true season count (a player shown
  with 5 may have had more in un-itemized matches). It's a provider garnish, not a
  stat we own — fails the box-score-honesty principle at the root. Not a seeder bug;
  no fix possible. (Contrast: `passes_accurate` 53k events, `key_passes` 21k,
  `fouls_drawn` 100% — densely/consistently provided, so they qualify.) **General rule
  this established: a datapoint must be provided as an explicit value (incl. zero), not
  only-when-nonzero — else absence masquerades as zero. Verify at the EVENT level
  (`has_key == nonzero_count` is the red flag).**
- Stats-page only: `save_pct`, pass %s, `shot_accuracy`.
- Validation (PL): Composite — Bruno Fernandes, Elliot Anderson, Garner, Senesi,
  Bowen. Specialist — Bruno F (assists 8.90), **Haaland (goals 7.02)**, Tarkowski
  (blocks), Doku (dribbling). GK via position scope.

**NFL** (floor: ≥8 GP; population = league-season; all 17 positions in ONE pool)
- De-dupe: `total_tackles` (not solo/assist splits); drop all `long_*`.
- **"A yard is a yard / a TD is a TD" — including return yardage (revised 2026-06-01):**
  `total_yards` = passing + rushing + receiving **+ kick_return + punt_return yards**;
  `total_touchdowns` = passing + rushing + receiving **+ return TDs**. `return_yards`
  is NO LONGER a standalone datapoint — siloing it created a thin, skew-heavy slot
  that manufactured +14 SD freaks (Chimere Dike's return z 13.97 vs Puka Nacua's best
  6.60), pushing return men to the top of the board. Folding return yards into the
  dense `total_yards` distribution dissolves the distortion — a few hundred return
  yards becomes a small bump, not an outlier. (This is the thin-population caveat §6
  biting; the fix is the same "yards are yards" pillar, applied to ALL yards.)
- Composite z-set: `total_yards, total_touchdowns, receptions, total_tackles,
  tackles_for_loss, defensive_sacks, passes_defended, defensive_interceptions,
  fumbles_recovered, field_goals_made, punts_inside_20,
  −(passing_interceptions + fumbles_lost)`
- Specialist over the positive set → labels total yards / touchdowns / receptions /
  tackles / TFL / sacks / pass defense / interceptions / fumble rec / field goals /
  punting.
- OL stays in the pool (≈0 score — box score doesn't capture them; honest), surfaced
  via position scope. Kickers/punters are pure specialists (low Composite, high
  Specialist) — the GK pattern generalized.
- **Composite is CATEGORY-BALANCED (offense / defense / special-teams facets, each
  the MEAN of its z's, summed) — §1.5.** The box score records ~6 defensive concepts
  vs ~3 offensive (confirmed not over-counted: intra-defense corr mostly <0.55, and no
  missed offensive concept exists — carries/targets/TD-splits all re-skins). Flat-z
  let that recording granularity silently weight defense 2×. Balancing fixes it
  honestly because NFL players are single-phase (corner=0 offense, WR=0 defense), so
  equal facets = equal phases of football. **Validated:** flat top-50 was 7 OFF/43 DEF
  (Stafford #4, Puka #41); balanced = 19 OFF/31 DEF, top = Stafford, Garrett, Marcus
  Jones, McCaffrey, Caleb Williams, Love, Nacua, Maye — reads like a real NFL season.
- Specialist stays pure peak-z (Stafford 100 TDs, Maye/Love 81, Byard 79 INTs).
- Rejected splitting TDs by type to lift RB/WR (0.88–0.97 collinear with yards); the
  real offense/defense imbalance was the facet-count skew, fixed by balancing.
- QBs leading yards is **correct** (most valuable position; salaries + 32 starting
  jobs confirm). Runners/receivers surface via Specialist + position scope.

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

## 9. Build status & execution plan

**Status:** Design **LANDED & LOCKED**. Validated read-only against the live DB
(NBA/PL/La Liga/Bundesliga/NFL 2025). **Nothing built into the pipeline yet** — the
next session is the build. All design decisions are settled; this is execution.

### Execution order (each step is a reviewable unit)

1. **SQL — z-engine functions (Postgres owns the math, per CLAUDE.md).**
   - `compute_rating(sport, season)`: for the qualified population (apply floors §6),
     compute mean+`STDDEV_POP` per de-duped datapoint, then per entity:
     `composite = Σ COALESCE(z_i,0)` over the Composite z-set; `specialist =
     MAX(z_i)` over the production set; `specialty = argmax` label. Guard every term
     `NULLIF(sd,0)`+`COALESCE(z,0)` (thin-stat NULL bug, §6).
   - Per-sport z-sets are fixed in §4. Negatives enter as `−z`.
   - Add columns to `player_stats`: `rating_composite NUMERIC`,
     `rating_specialist NUMERIC`, `rating_specialty TEXT` (keep legacy
     `season_composite_score` lineage or migrate it).
   - Migration numbering continues from `027` (next free; confirm at build time).

2. **Per-event scores (starline).** Event-level z (vs the season population) written to
   `event_box_scores` for the dual sparkline (Composite + Specialist contribution per
   game). Event derivations via `BEFORE INSERT/UPDATE` trigger if any derived input is
   needed (none currently — all rating inputs are raw counting stats).

3. **Lifecycle (freeze + recompute).** In-season: recompute on `finalize_fixture`
   (current season only). Season close: lock; recompute only the cross-season layer on
   rollover (reuse maintenance worker `lastSeenSeason`). Avoid the O(M²) per-fixture
   whole-season pass — see COMPOSITE_MATRIX_V2 §4–5.

4. **Endpoints (thin Go handlers → prepared statements, per CLAUDE.md flow).**
   - `GET /api/v1/{sport}/leaderboard?scope=composite|specialist|<skill>&season=&position=&league=`
   - Starline data on the existing trends/profile-adjacent surface.
   - **Join caveat:** `players` is keyed by `(id, sport)` (ids collide across sports) —
     every join needs `AND p.sport = ps.sport`.
   - Profile/pizza-chart payload stays SEPARATE and unchanged (§5).

5. **Docs:** ENDPOINTS.md, README.md, Swagger annotations (required, per CLAUDE.md).

### Datapoint inclusion is FROZEN for v1 (the three-gate rule, §1.2)
NBA: `pts, reb, ast, stl, blk, fg3m, plus_minus, −turnover, −pf`.
Football: `goals, assists, shots_total, passes_accurate, key_passes,
dribbles_success, duels_won, tackles, interceptions, clearances, blocks,
ball_recovery, −possession_lost, fouls_drawn` + GK `saves, penalties_saved, punches,
good_high_claim`. NFL: §4 (note `total_yards`/`total_touchdowns` INCLUDE return
yardage/TDs — "yards are yards"; no standalone `return_yards` slot).
Specialist = positive counting subset of each.

### Known data limitations (NOT bugs — provider/availability)
- `through_balls` (football) — DROPPED; SportMonks omits zeros (§4). Unusable.
- `fouls_committed` (football) — REJECTED; same omits-zeros trap (gate 3).
- NBA fouls-drawn — does not exist in our box-score feed (only `pf` committed, `fta`).
- NFL `qb_hits` — effectively unseeded (6 nonzero). Excluded.

### Future-data wishlist (slot in cleanly under the gates IF a provider supplies them)
- NBA: a true **fouls-drawn** count (would pair with `−pf` for two-way foul fairness).
- Football: a **zeros-inclusive** fouls feed; a reliable **through-balls/progressive-
  pass** count with explicit zeros (would finally credit line-breaking CBs — Colwill,
  and progressive engines Barco/Enzo).
- NFL: **defensive coverage data** — targeted-against, completions-allowed,
  completion-% / passer-rating allowed (e.g. NextGen Stats). We have NONE of it
  (`receiving_targets` is the OFFENSIVE side). This is THE box-score blind spot: a
  lockdown corner's value is *passes not thrown at him* — events that didn't happen,
  which a box score can't record. `passes_defended`+INT only catch corners who make
  plays ON the ball, not deterrence. A *positive* z for low completion-% allowed would
  finally rate shutdown corners correctly. Charted/advanced data — out of v1 scope.
- General principle: the lockdown-corner archetype (value = absence-of-opportunity) is
  the hardest case for any box-score system; acknowledge the ceiling, don't fake a
  proxy ("low targets = good" also flags benched players — needs a charted denominator).
- Verify any new datapoint with the gate-3 event check: `has_key == nonzero_count` → reject.
