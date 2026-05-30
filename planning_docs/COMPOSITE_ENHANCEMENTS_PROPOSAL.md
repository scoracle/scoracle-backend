# Composite Score — Enhancements Proposal (deltas vs shipped v1)

Date: 2026-05-30
Status: **shipped 2026-05-30** — deltas 1 + 2 live (migrations 025/026); delta 3 deferred per the doc's recommendation. See "Shipped" section at the bottom for the implementation summary.
Builds on (already shipped, migrations 017–024):
- `progress_docs/2026-05-23_event-percentiles-and-composite-score-proposal.md`
- `progress_docs/2026-05-23_event-derivation-proposal.md`

## Why this doc exists

A 2026-05-29/30 design session re-derived the composite pipeline from a stale
local checkout, not realizing the feature had already shipped from the other
machine. Most of what that session "designed" is **already live and more
refined** than the session's version. This doc keeps only the ideas v1 does
**not** already cover, applied to the live design. It is deliberately small.

## What v1 already does (NOT re-proposed)

The shipped four-layer model already covers the bulk of the session's thinking:

| Shipped | Covers the session idea of… |
|---|---|
| Layer 1 — `event_box_scores/event_team_stats.composite_score`, normalized mean=50 (mig 017/018) | per-event composite + a dynamic sparkline |
| Layer 2 — `*_stats.season_composite_score` = AVG of season per-stat percentiles incl. outcome stats (mig 020) | the season "truth" number, cross-season comparable |
| Layer 3 — `*_stats.season_composite_rank` (mig 021) | in-season leaderboard / readable headline |
| Layer 4 — `*_stats.season_composite_rank_alltime` (mig 022–024) | "best season ever recorded" historical leaderboard |
| Frozen-history, in-season-only writes; nightly `recalculate_alltime_ranks` ticker (mig 023/024) | "composites are the source of truth; only in-season computes; previous seasons read-only" |
| Per-X rates dropped from composite (mig 020) | avoiding raw+rate 2× weighting (this was our own caveat) |

So "composite = truth, rank = UX," "frozen previous seasons," and "historical
leaderboard" are **done**. Don't rebuild them.

## Proposed deltas

### 1. Cold-start guard for early-season composites — RECOMMENDED

**Gap in v1:** v1 has no small-sample guard. The composite proposal explicitly
accepts that "an early-season game's score drifts slightly as more events are
added," and relies on season aggregation to dilute outliers. That works at
mid/late season, but in the **first 1–3 games** `season_composite_score`
(layer 2) is built from a tiny sample, so the in-season leaderboard (layer 3)
is volatile — a one-game wonder can top the early board.

**Proposal (this was the user's own idea in-session):** for the first 3 games of
a season, anchor `season_composite_score` to the entity's **prior-season frozen
composite**, phased out continuously, fully released at game 3:

```
prior = entity's prev-season season_composite_score
        ?? prev-season league-average composite      -- rookies / expansion / promoted
        ?? 50                                         -- first season in the DB

games < 3:  season_composite_score = ( (3 − games)·prior + Σ in-season game contributions ) / 3
games ≥ 3:  season_composite_score = (unchanged v1 value)
```

Continuous at game 3 (no jump). Effect: the early-season leaderboard **opens as
last year's standings and morphs into this year's** instead of being noise.

**Where:** a post-step on layer 2 inside `recalculate_event_percentiles` (the
within-season function), applied before layer-3 rank is computed. The prior is a
frozen read — consistent with v1's "previous seasons are a read-only reference."

**Cost:** one extra prior-season lookup + a blend on current-season rows only.
Negligible; stays within the in-season write footprint v1 already pays.

**Open sub-decisions:** (a) blend on the season_composite_score directly (simple)
vs blending the underlying per-stat percentiles; (b) confirm the rookie fallback
(prev-season league average vs flat 50). Recommend (a) + prev-season league avg.

### 2. Cross-position ("absolute") leaderboard for players — OPTIONAL, product call

**Gap in v1:** player ranks are **position-partitioned** — layer 4 ranks "against
EVERY season in the DB, same sport + position for players." So v1 answers "best
point guard / best striker," but **cannot** answer "best player overall,
regardless of position," which the session wanted as a headline leaderboard.
(Teams are already sport-wide, so this is players-only.)

**Proposal:** add a parallel **position-agnostic** rank for players —
`season_composite_rank_absolute` (in-season) and/or `_alltime_absolute`. Keep the
existing position-partitioned ranks as the drill-down lens; absolute becomes the
"best overall" headline board.

**The load-bearing tradeoff — pick the input:**
- *Naive:* re-percentile raw stats with no position partition. **Reintroduces the
  archetype bias position-partitioning was added to avoid** (volume/usage
  archetypes dominate). Not recommended.
- *Recommended:* rank the **existing `season_composite_score` cross-position**
  (no re-percentiling). Because that composite is already built from
  position-relative percentiles, ranking it across all positions yields a
  "most dominant **relative to their own position**" board — an absolute
  leaderboard that stays position-fair in its inputs. Cheap: one extra
  `percent_rank()` over `season_composite_score` without the position partition,
  slotted into `recalculate_alltime_ranks` (+ the in-season rank fn).

**Decision needed:** do you actually want cross-position leaderboards as a
product surface? If yes, take the recommended variant. If the position-scoped
boards are enough, skip this entirely.

### 3. Cross-season per-event rating for the sparkline — DEFER

v1's event composite is normalized within `(sport, season, position)`, so the
sparkline is already dynamic per season. A cross-season ("best games in DB ever")
event rating is marginal on top of that. Park unless a specific UI wants it.

## Mechanics

- **Next migration is `025_*`** — `014`–`024` are taken (`014` is `nfl_per_game`,
  not composite). Any greenfield "014_composite" numbering from the stale session
  is void.
- **Reuse v1 objects**, do not fork them: `recalculate_event_percentiles`
  (layers 1–3, within-season), `recalculate_alltime_ranks(sport, season)`
  (layer 4, nightly), `season_composite_score`, `season_composite_rank`,
  `season_composite_rank_alltime`, and the `AlltimeRankInterval` ticker in
  `go/internal/maintenance/maintenance.go`.
- **API field names already in use:** profile `meta.season_composite_rank` /
  `meta.season_composite_rank_alltime`; trends `entity_season_score_rank` /
  `entity_alltime_score_rank`; sparkline `entity_event_scores`. Any absolute
  variant adds siblings, never renames these.

## Recommendation

1. **Do delta 1 (cold-start guard)** — small, bounded, fixes a real early-season
   weakness v1 explicitly punts on, and it was the session's own best idea.
2. **Decide delta 2 (absolute leaderboard)** as a product question; if yes, use
   the cross-position-rank-of-composite variant (position-fair, cheap).
3. **Defer delta 3.**

Everything else the session produced is already shipped — this doc is the honest
residue worth acting on.

## Shipped — 2026-05-30

### Delta 1 — Cold-start guard (migration 025)

Implemented as Layer-2.5 inside `recalculate_event_percentiles`, running
between layer 2 (season_composite_score AVG) and layer 3 (in-season rank).
Linear blend `α·prior + (1−α)·current` with `α = max(0, (window−games)/window)`,
where `window` is sport-specific per the doc's "proportional ~10%" choice:

  NBA      8 games  (10% of 82)
  NFL      2 games  (10% of 17)
  FOOTBALL 4 games  (10% of 38)

Fallback chain matches the doc's spec: entity's own prev-season composite
(same league_id for football) → prev-season cohort average
(sport + position for players, sport for teams) → 50.0. Lives within the
existing in-season write footprint; prior seasons read-only.

Sub-decision (a) from the doc adopted (blend on `season_composite_score`
directly rather than on per-stat percentiles). Simpler, same effect.

Verification: NBA Centers full-season unchanged (Jokic 81.9 → 81.2 — tiny
ripple from cohort shifts as sparse-event players got pulled to ~50 rather
than ~bottom). Football Attacker top-5 cleaned up further: Yamal, Soler,
Kane, Greenwood, Nusa — all real 28-36-event starters. Backfill ran in
2m47s across all (sport, season).

### Delta 2 — Cross-position absolute leaderboard (migration 026)

Added two player-only columns: `season_composite_rank_absolute` (in-season,
cross-position) and `season_composite_rank_alltime_absolute` (across-all-seasons,
cross-position). Adopted the doc's recommended variant: `percent_rank` of the
existing `season_composite_score` with no `PARTITION BY position` — keeps the
inputs position-fair (the composite is built from position-relative
percentiles) while producing an overall leaderboard.

Functions modified:
- `recalculate_event_percentiles`: Layer-3 absolute step after the existing
  position-partitioned Layer 3, in-season scope.
- `recalculate_alltime_ranks`: Layer-4 absolute step alongside the existing
  position-partitioned Layer 4, same `(sport, season)`-scope semantics
  (NULL → full re-baseline, integer → that season only).

Teams unchanged — their ranks are already sport-wide; "absolute" is N/A.
Profile/trends API expose the new fields as NULL for teams to keep the
contract symmetric.

Verification: NBA in-season absolute top-5 — SGA, Kawhi, Murray, Jokic,
Edwards (right names, ordered by composite). NBA all-time absolute top-5 —
Kyrie 2020, Kawhi 2023, Jokic 2022, Kawhi 2020, Kyrie 2021. Jokic 2025 →
100 within Centers / 99.5 absolute (correctly identifies him as top Center
but not top player overall since SGA's composite is higher). All-time
full rebaseline ran in seconds.

### Delta 3 — Cross-season per-event sparkline rating

Deferred per the doc's recommendation. The per-event composite is already
normalized within `(sport, season, position)` so the sparkline is dynamic
per season; a cross-season "best games ever" event rating is marginal
until a specific UI surface asks for it.

### API surface added

Profile (`/api/v1/{sport}/{entityType}/{id}`):
  `meta.season_composite_rank_absolute`            (players only)
  `meta.season_composite_rank_alltime_absolute`    (players only)

Trends (`/api/v1/{sport}/{entityType}/{id}/trends`):
  `entity_season_score_rank_absolute`              (players only)
  `entity_alltime_score_rank_absolute`             (players only)

Teams receive NULL for the four absolute fields. Existing position-
partitioned ranks (`season_composite_rank`, `season_composite_rank_alltime`)
are unchanged.

### Maintenance ticker

`AlltimeRankInterval` (24h) already handles both position-partitioned and
absolute all-time recomputes — `recalculate_alltime_ranks` was extended in
mig 026 to write both columns in the same pass, so no maintenance worker
changes were needed. Startup full re-baseline + season-rollover detection
also cover both ranks automatically.
