# Composite Matrix v2 — flat, honest, event-built

**Status:** **NBA matrix LOCKED (2026-05-30)** — flat-9, absolute basis. Football
next (per-position matrices; user has an approach to the goalkeeper problem).
NFL after. Supersedes the Game Score direction and the position-relative inputs
explored earlier the same day.

**Author context:** Drafted after a live-data audit of the shipped composite
(migrations 017–026) showed it was effectively a scoring-volume metric, then
iterated interactively against live NBA 2025 data until the board held up.

**Design philosophy (non-negotiables that shaped this):**
- **No weighting.** Every data point is one equal vote. Weighting "opens a can of
  worms and puts too much on me to mess with the data." The matrix self-balances
  *as long as the points stay one-per-concept* — that discipline replaces tuning.
- **Our own metric, not borrowed.** We rejected third-party formulas (Game Score:
  offense-weighted, redundant at the top, craters defenders). The efficiency point
  is the house `shots_to_points` (§3).
- **Absolute, not per-position.** The score answers "best player overall," so each
  stat is ranked against *all* players, not position peers (§2, §4).

---

## The pipeline — event-up, positionless composite, scoped percentile

The desired flow, and the deliberate inversion of v1:

```
event raw box score
  → derived data          efficiency computed + upserted at the EVENT level
                          (shots_to_points, etc. — factor 1)
  → season aggregate      per stat, per player-season
  → per-stat ABSOLUTE     each stat percentiled vs ALL players that season —
    percentile            NO position partition. This is the unit-normalizer
                          (you can't average points with a ratio with +/-).
  → COMPOSITE             unweighted mean of the 9 → POSITIONLESS by construction
  → freeze                locked at season close; joins the permanent reference base
  → composite PERCENTILE  one positionless number, ranked in two scopes:
                            • absolute    — vs all players
                            • by position — the same number, ranked within position
```

**"Percentile" appears at two distinct levels — don't conflate them:**
1. *Inside* the composite, as the **unit-normalizer** — each raw stat → its
   absolute percentile (vs all players) so heterogeneous stats become averageable.
   This is what makes the composite **positionless**.
2. *After* the composite, as the **scoped output** — the finished positionless
   composite is ranked both **absolute** (vs all) and **by position** (same number,
   filtered to position peers, e.g. "best center").

**Position enters only at level 2 — never inside the composite.** A center's
composite reflects how they stack up against *everyone* on each stat; the "best
center" board is just that same positionless number filtered to centers.

**What we're leaving (v1):** season averages → per-stat percentiles **partitioned
by position** → composite built from those. That baked position *into* the
composite, which is why a "best overall" board put rim-runners and keepers on top.
v2 strips position out of the composite and reintroduces it only as a viewing lens.

> **Football/NFL wrinkle (solved — §9):** a purely positionless composite makes
> goalkeepers (and NFL specialists) vanish — ≈0 production outside their role. Fix:
> make the matrix a *superset* with each role's **exclusive** stats (positives +
> negatives); non-participants score 0, so it's a uniform drag that preserves their
> ranks while lifting the specialist. Validated for GKs; extends to QB/K/P. See §9.

---

## 1. Why v1 had to change

Audited live (NBA 2025):

- Season composite correlated **0.90 with scoring volume**, 0.32 with impact. A
  scoring board.
- Root cause: migration `017` overwrote `is_percentile_eligible` from the `unit`
  column, discarding `nba.sql`'s curation (`ON CONFLICT DO NOTHING` never
  reapplies it). Live input set = **13 collinear scoring/shooting : 3 defense : 1
  impact** — an unintended editorial weighting masquerading as "data-driven."
- Two parallel percentile engines per `finalize_fixture`, hidden ordering
  dependency, O(M²) whole-season recompute per fixture.
- Per-stat percentiles were **position-partitioned**, so the "absolute" leaderboard
  was really a cross-position ranking of position-relative numbers ("most dominant
  vs your own position") — which is why a great keeper or rim-running center could
  top a "best overall" board. Replaced (§4).

---

## 2. The matrix (NBA) — flat 9, unweighted, absolute

Each data point is percentile-ranked against **all players** in `(sport, season)`
— **no position partition** — including zero values (a `0` is real signal: you
don't block, you don't make threes). The composite is the **unweighted mean** of
the 9 percentiles.

| # | Data point | Concept | Notes |
|---|---|---|---|
| 1 | `pts` | scoring volume | per-game |
| 2 | `reb` | rebounding | total (oreb/dreb not separate votes) |
| 3 | `ast` | playmaking | |
| 4 | `stl` | perimeter defense | kept separate from blk on purpose |
| 5 | `blk` | rim protection | different defensive job than stl |
| 6 | `turnover` | ball security | **inverse** (fewer = better) |
| 7 | `shots_to_points` | scoring efficiency | **= PTS / FGA**, the house metric (§3) |
| 8 | `plus_minus` | on-court impact | only box proxy for unmeasured defense |
| 9 | `fg3m` | floor spacing / shooting | 3-pointers **made** — the counterweight (§2a) |

Defense intentionally carries **2/9** (`stl`+`blk`) — accepted; they're genuinely
different defensive roles (a guard's defense is steals, a big's is blocks;
collapsing them erases that).

### 2a. Why `fg3m`, and why absolute needed it

Going to the absolute basis (§4) put Jokić at #1 (correct) but introduced a
*big-man tilt*: absolute rewards filling every column, so do-everything bigs
(Şengün, Hartenstein at 9.3 ppg, Bam) leapfrogged elite perimeter scorers, and
rim-running centers rode their near-rim efficiency (`shots_to_points`) as an "ace
in the hole." `fg3m` is the structural counterweight — 3-point volume is the one
thing a rim-runner *can't* fake.

Effect (live): **Jokić #1 and Wembanyama #4 held** (modern bigs shoot ~1.7–1.9
threes), while non-shooters were offset (Diabate #35→#86, Şengün/Hartenstein out
of the top 25 — all 0.0 threes) and 3-and-D players got their due (Edwards 13→7,
OG Anunoby 12→8, Maxey 22→9, Curry 45→21, Derrick White 38→22). It draws the real
line between a *skilled modern big* and a *pure rim-runner*.

### 2b. What was dropped vs the candidate set
- **A/TO** — double-docks turnovers (`turnover` already counts them), penalizes
  tempo-pushers, and silently inflated low-usage bigs (who ace A/TO by barely
  touching the ball). Its removal was the biggest single fix to that artifact.
- **TS%** — 0.89 correlated with `shots_to_points`; keeping both double-counts
  efficiency. We keep the house metric.
- **Raw shooting components, shooting %s, oreb/dreb, pf** — the collinear-volume
  mass that made v1 a scoring board. They stay in the per-stat breakdown UI; they
  don't vote.

---

## 3. `shots_to_points` — the house efficiency metric

```
shots_to_points = PTS / FGA          (season: total PTS / total FGA; per event for sparkline)
```

- **Free throws are NOT charged a shot** (no `0.44·FTA`). FT *points* flow into the
  numerator, so drawing-and-converting is pure bonus economy.
- Deliberately **not** TS%. The `0.44·FTA` ("true shot") version equals `2·TS%`
  (0.99 corr). Bare `PTS/FGA` is **0.89** correlated with TS% — distinct signal,
  uniquely rewarding foul-drawing slashers (Barrett, Banchero) that TS% docks.
- Artifact (handled by being 1 of 9): over-rewards low-FGA rim-runners — which is
  exactly why `fg3m` was added as the counterweight.

---

## 4. Architecture — three layers

```
raw event box score
  └─(trigger)→ + derived points (shots_to_points; per sport's efficiency)        [Layer 1]
       └─→ composite = unweighted mean of the 9 ABSOLUTE per-stat percentiles    [Layer 2]
            (frozen at season close)
            └─→ percentiles / ranks = the only recalculable layer                [Layer 3]
                 ├─ in-season rank (current season)
                 ├─ cross-season rank ("all-time" — recompute only on rollover)
                 └─ scopes: absolute (default) · position (filter) · per-36 (rate)
```

### Layer 1 — event box score is the base (factor 1)
Derived stats today are computed **only** at season level
(`compute_derived_player_stats` on `player_stats`/`team_stats`); event rows are
raw-only (live: 0/308k NBA events carry efficiency keys). Add a
`BEFORE INSERT/UPDATE` trigger on `event_box_scores` to derive the efficiency
points per event. Granular base → sparklines, event drill-down, bottom-up build.

### Layer 2 — composite is absolute & frozen (factor 2)
The composite is the source-of-truth number. Computed from **absolute** per-stat
percentiles (vs all players that season, zeros included). Once a season closes it
locks and joins a permanent, growing reference base — never recomputed. (It is
still relative to that season's *field* — a mean of percentiles — but a completed
season's field is final, so the value is stable forever. "Absolute" here means
*not position-partitioned*, not "formula-scale.")

### Layer 3 — only the percentiles/ranks recompute
- **In-season rank:** rank the current season's composites.
- **Cross-season / "all-time":** the growing-context layer. Recompute **only when a
  new season completes** (the pool grew) — not nightly. Frozen seasons' composites
  never change; only their cross-season percentile shifts.
- **Scopes** = views over the same frozen absolute composite:
  - `absolute` (default) — rank across all players.
  - `position` — rank/filter the absolute composite *within* a position ("best
    center by the overall metric"). NOT a re-percentiling within position.
  - `per_36` — rate-normalized variant; the "tomorrow's stars" lens, optional,
    never the baseline.

---

## 5. Recompute lifecycle (kills the O(M²))

| Trigger | Work | Cost |
|---|---|---|
| per fixture (in-season) | derive event points; update affected composites | O(events in fixture) |
| leaderboard read | rank stored composites by requested scope | cheap / cached |
| **season completes (rollover)** | lock season; recompute cross-season percentile across all seasons | once/year |

Replaces the per-fixture whole-season re-percentile, the nightly all-time
recompute, and collapses the two engines into one ingest-time enrichment +
aggregation. Makes the `027` deferred-backfill migration largely moot.

---

## 6. Validation snapshot (NBA 2025, gp≥30, flat-9 absolute)

Top: **Jokić, Dončić, SGA, Wembanyama, Kawhi, Cade, Edwards, OG Anunoby, Maxey,
Durant, Reaves, Bam, Harden, KAT, Mitchell, Murray** — a defensible "best players"
board. Jokić #1 confirms the absolute basis does the right thing at the top; the
big-man tilt is offset by `fg3m` (Diabate down at #86, Hartenstein/Şengün out of
the top 25). Correlation vs scoring volume fell from v1's **0.90 to ~0.7**, impact
(+/-) rose from 0.32.

---

## 7. Open decisions

| # | Item | Status |
|---|---|---|
| 1 | **Absolute vs position basis** | **DECIDED — absolute** (vs all players, zeros included). |
| 2 | **Volume floor** | Likely unnecessary now — `fg3m` self-corrected the low-usage bigs (Diabate #86). Revisit only if a cameo sneaks the top 25. |
| 3 | **Name** | Open. Working: *Scoracle Rating (SR)*; alt *Augur*. "Composite" stays the internal term. |
| 4 | **Football** | **LOCKED (2026-05-31) — TWO-SCORE model: `General` (de-duped breadth matrix → grinders) + `Specialist` (value-matrix VOR *peak* → irreplaceables), shown separately, NEVER summed.** Validated PL/La Liga/Bundesliga (Specialist surfaces Mbappé/Kane/Yamal; General the all-rounders). Full spec in `FOOTBALL_VOR_EXPLORATION.md`. GK via §9 exclusive-stats trick (separate build). Open: GK build, implementation. |
| 5 | **NFL** | **Approach DECIDED — same superset; QB / kicker / punter via exclusive stats incl. negatives (INTs, sacks) (§9).** Open: shared-group balance (rushing / receiving / defense). |
| 6 | **Composite scale** | Stays a 0–100-ish percentile mean (relative-to-field, frozen). No formula scale. |

---

## 8. Implementation sketch (next phase, not yet built)

1. **Absolute percentile pass.** The composite uses **absolute** per-stat
   percentiles (no position partition, zeros included) — a *new* computation,
   distinct from the existing position-partitioned `player_stats.percentiles`
   (those stay for the per-stat breakdown UI: "85th percentile for assists vs
   centers"). v2 wholly replaces the v1 composite + the migration-026
   cross-position-of-position-relative approach.
2. **Event-level derive trigger** on `event_box_scores` (`shots_to_points` first).
3. **Composite build fn:** unweighted mean of the 9 absolute percentiles; rebaseline
   existing seasons once.
4. **Lifecycle:** season-lock + cross-season recompute on rollover (reuse the
   maintenance worker's `lastSeenSeason` rollover detection); drop the nightly
   cadence.
5. **Leaderboard endpoint** (does not exist yet; columns populated, no route):
   `GET /api/v1/{sport}/leaderboard?season=&scope=absolute|position|per36&entity=player|team`
   → prepared statement ranking the frozen composite. **Join caveat:** `players`
   is keyed by `(id, sport)` (ids collide across sports) — every join needs
   `AND p.sport = ps.sport`.
6. **Docs:** ENDPOINTS.md, README, Swagger per CLAUDE.md.

---

## 9. Cross-sport principle — one positionless superset + exclusive stats (incl. negatives)

The positionless composite generalizes to football and NFL via one rule:

> **Put every role's *exclusive* stats — positives AND negatives — into the single
> positionless matrix. Players who don't fill that role score 0 on them, so the
> addition is a uniform drag that preserves everyone else's order; the specialist
> is placed fairly by their own production; the position scope gives them a board.**

This is the mirror of NBA's `fg3m`: there a stat *differentiated within* the field;
here exclusive stats *lift a specialist without disturbing* the field.

**Why it's safe (uniform-drag proof):** with zeros included (§4) and a stat that's
0 for all non-participants, every non-participant ties at percentile 0 on it, so
adding K such stats scales each of their composites by exactly `N/(N+K)` — uniform,
order-preserving. The output is a percentile, so their ranks don't move. A star can
drop from raw ~75 to ~50 and still grade 99th; only the specialist relocates from
the cellar into their fair spot. (Caveat: mid-pack ranks shift slightly as the
specialist takes their rightful place — intended, not a bug.)

**Hard requirement: the stats must be truly exclusive (0 for all non-participants).
Verify per stat in the data.**

### Football — goalkeepers (VALIDATED on live La Liga 2025)
Exclusive GK set (0 of 658 outfielders carry any): `saves` (volume), `save_pct`
(efficiency — the GK "shots-to-points"), `penalties_saved` / `punches` /
`good_high_claim` (the "plus" skills).
**EXCLUDE `goals_conceded`** — team-attributed, not GK-exclusive (518/658
outfielders carry it); it would break uniformity and distort defender ranks. GKs
therefore have clean exclusive *positives* but no clean exclusive *negative*
(`save_pct` carries efficiency instead).

### NFL — QBs and special teamers (same mechanism, cleaner)
- **QB:** exclusive positives (`passing_yards`, `passing_tds`) ride in *with*
  exclusive negatives (`interceptions`, `sacks_taken`, fumbles) — the position
  polices itself; gaudy + turnover-prone ≠ juiced.
- **Kicker / punter:** field goals / punts are theirs alone → straight GK treatment.
- NFL specialists are *cleaner* than GKs because QB/K/P stats are perfectly exclusive.

### What this does NOT solve — shared-stat groups
The exclusive trick handles *pure specialists* (GK, QB, K, P). It does nothing for
stats *shared across positions*:
- Football outfield: attacker vs midfielder vs defender.
- NFL: rushing (QB/RB/WR), receiving (WR/TE/RB), and **defense** (under-measured in
  the box — defenders risk burial under skill-position yardage; the NFL echo of
  football's defenders / NBA's non-scorers).

Balancing those shared groups so the matrix doesn't juice one over another is the
remaining design step for football and NFL — the equivalent of choosing NBA's 9
(with a counterweight where one's needed).
