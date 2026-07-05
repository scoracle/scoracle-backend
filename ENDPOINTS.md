# Scoracle API Endpoints

> Last updated: 2026-07-05 (Leaderboard is the DB-first ranking surface; profile is entity drill-down; roster moved to leaderboard team scope).

Single public API base URL:

- Production: `https://api.scoracle.com` (Cloudflare Tunnel → self-hosted Go API)
- Local: `http://localhost:8000`

## Authoritative route inventory

The only source of truth is `go/internal/api/server.go`. Every route wired there, as of S17:

| Route | Notes |
|---|---|
| `GET /api/v1/entities` · `/autofill` | universal, cross-sport, text-only player/team directory for home search |
| `GET /api/v1/{sport}/{entityType}/{id}/stats` | season Composite rating + per-event series + `available_seasons` |
| `GET /api/v1/{sport}/{entityType}/{id}/rating` | model-divined stat read + PEAK trajectory metadata (was `/special`) |
| `GET /api/v1/{sport}/{entityType}/{id}/momentum` | Rating × Vibe trajectory (was `/trends`) |
| `GET /api/v1/{sport}/{entityType}/{id}/sigil` | Sigil crown synthesis (was per-entity `/vibes`) |
| `GET /api/v1/{sport}/{entityType}/{id}/news` | scoped model narratives with source freshness and trajectory markers |
| `GET /api/v1/{sport}/{entityType}/{id}/transfers` | scoped vetted transfer/trade rumor heat with source freshness and trajectory markers |
| `GET /api/v1/{sport}/{entityType}/{id}/meta` | per-entity identity (page header); 404 if unknown |
| `GET /api/v1/{sport}/team/{id}/results` · `/roster` | finalized scorelines · legacy roster compatibility |
| `GET /api/v1/{sport}/meta` · `/autofill` · `/health` | legacy sport-wide metadata/search payload · legacy sport autofill · freshness |
| `GET /api/v1/{sport}/leaderboard` | comprehensive ranked research database; `?board=rating\|vibes\|sigil\|news\|transfers\|momentum` (`trending` legacy alias) |
| `GET /api/v1/{sport}/leaderboard/{vibes,sigil,news,transfers,momentum}` | dedicated boards (`trending` legacy alias remains wired) |
| `GET /api/v1/{sport}/leagues/{leagueId}/{momentum,results,meta,health}` | league-scoped variants |
| `GET /` · `/health` · `/health/db` · `/health/cache` · `/docs/` · `/docs/go.json` | operational |
| `POST /api/v1/auth/{device,refresh,device/push,logout}` | mobile device-identity JWT |

**Removed (no longer wired — kept here so this file matches reality):** the bundled profile
route `/{sport}/{entityType}/{id}` (O16); the per-entity aliases `/special`, `/trends`,
`/vibes` (O14 convergence rename); the standalone `/headlines` and `/leaderboard/headlines`
routes (2026-07-03; folded into `/news`); the old live integration routes; the legacy
`/sparkline`/`/starline`.

## Core Data Endpoints (Canonical)

All canonical endpoints are sport-scoped under `/api/v1/{sport}`.

Supported sport path values:
- `nba`
- `nfl`
- `football`

Supported entity type values:
- `player`
- `team`

### `GET /api/v1/{sport}/{entityType}/{id}` — REMOVED (O16)

The bundled all-in-one profile route was removed 2026-06-19. Its page-shell payload is
now delivered by the per-product endpoints in the eager model: `available_seasons` +
`stat_definitions` ride **`/stats`**, entity identity is **`/meta`**, and the season
composite score/ranks come from **`/stats`** and **`/rating`**.

### `GET /api/v1/{sport}/{entityType}/{id}/momentum`

Combines **stats trend + narrative trend** in one read-only payload so a profile page can render "is this entity hot right now" without juggling multiple endpoint calls.

What you get, per request:
- **Stats trend** — the entity's average per stat over its **last 3 fixtures** alongside the **peer cohort's season averages**. The asymmetry is intentional: the entity carries the recent signal, the cohort carries the stable baseline.
- **Narrative trend** — the entity's **last 7 days** of model sentiment scores (1–100) from `vibe_scores`, newest first.

**Raw values only.** No "trending up" verdict, no deltas pre-computed. The frontend decides what the gap (or the slope) means visually.

Cache: 5 min TTL (`X-Cache: HIT/MISS`), ETag-enabled — send `If-None-Match` for a 304.

#### Path parameters

| Name | Type | Notes |
|---|---|---|
| `sport` | string | `nba`, `nfl`, or `football` |
| `entityType` | string | `player` or `team` |
| `id` | integer | Entity ID |

#### Query parameters

| Name | Type | Notes |
|---|---|---|
| `season` | integer (optional) | Defaults to the sport's `current_season` |
| `league_id` | integer (optional) | Filter to a specific league. **For football**, when omitted the entity's natural league is used so the cohort doesn't span multiple leagues. NBA/NFL ignore it. |

The league-scoped route `/api/v1/{sport}/leagues/{leagueId}/{entityType}/{id}/momentum` is an alias that takes `leagueId` from the path instead of the query string — identical payload.

#### Response fields

| Field | Type | Description |
|---|---|---|
| `page` | string | Literal `"momentum"` |
| `sport`, `entity_type`, `entity_id` | echo of inputs | |
| `window.games_used` | integer | How many events fed `entity_recent_avgs` (0–3) |
| `window.fixture_ids` | int[] | Fixture IDs of those events, newest first |
| `window.spans_prior_season` | bool | `true` when the 3-fixture window bridged into the prior season because the current season had fewer than 3 |
| `entity_recent_avgs` | `{stat: number}` | Per-stat average over those events. Keys come from `event_box_scores.stats` / `event_team_stats.stats`, filtered to `stat_definitions.comparable = true` so units match the season-rolled sides. `{}` if no events. |
| `entity_season_avgs` | `{stat: number}` | The entity's **own** season-rolled per-stat values, sourced from `player_stats.stats` / `team_stats.stats`. Same comparability filter and same per-game normalization as `peer_season_avgs`, applied to the single entity row instead of a cohort. Frontend uses this alongside `peer_season_avgs` to render a self-delta next to the peer-delta — important for dominant entities where every peer comparison reads as a huge positive and the user can't otherwise tell which way the entity is actually trending relative to its own baseline. `{}` for players (player cumulatives are non-comparable; their per-game / per-90 siblings already appear on the peer side and self-comparison adds no signal there). `{}` if the entity has no `*_stats` row in scope. |
| `peer_season_avgs` | `{stat: number}` | Per-stat average across the peer cohort, filtered to `stat_definitions.comparable = true`. For `unit = 'cumulative_total'` keys (e.g. football team `tackles`, NFL team `passing_yards`), the raw season values are normalized to per-game by dividing each peer's value by their `matches_played` (football) / `games_played` (NBA/NFL) before averaging, so both sides land in the same per-game unit. Compare by intersecting keys with `entity_recent_avgs`. |
| `peer_cohort_size` | integer | Peers contributing to `peer_season_avgs`. Use this to hide the comparison when the cohort is too thin to be meaningful (e.g., `< 5`). |
| `entity_event_scores` | object[] | **Every** played event in the current season, newest first (renamed from `entity_recent_scores` which only carried the last 3). Each entry: `{fixture_id, composite_score, minutes_played, start_time}`. `composite_score` is in `[0, 100]` — see the Interpretation section below. NULL composite for events with no eligible non-zero stats (e.g. a DNP-CD). `minutes_played` is provided for hover-tooltip context; `null` for team entities (teams play the full match). `start_time` is the fixture's UTC ISO-8601 timestamp — lets the frontend label hover-tooltips and bucket by week/month without a second fetch. Counts: ~82 for NBA, ~17 for NFL, ~38 for football (single league). |
| `entity_season_score_avg` | number \| null | The entity's own `season_composite_score` — AVG of season per-stat percentile ranks (migration 020). Cross-season comparable (relative-to-cohort). `null` if the entity has no eligible season stats. |
| `entity_season_score_rank` | number \| null | The entity's `season_composite_rank` — percentile rank of `season_composite_score` within the **current-season** cohort (`(sport, season, position)` for players, `(sport, season)` for teams). Uniform `[0, 100]`, top entity in cohort = 100. The **in-season leaderboard/headline number** — "96 = top 4% of Centers this season." `null` if no eligible season stats. |
| `entity_alltime_score_rank` | number \| null | The entity's `season_composite_rank_alltime` — percentile rank of `season_composite_score` against **every season in the DB** for the cohort. "Is this one of the best seasons we've ever recorded?" An entity hits 100 only if its season is the most dominant-relative-to-peers season in the data (e.g. Milwaukee's 2019 tops NBA teams; OKC's 2025 is ~98 — #1 this year but not all-time). Era-fair because the composite is already a percentile (controls for pace/rule changes). Refreshed nightly; previous seasons are a frozen reference. `null` if no eligible season stats. |
| `entity_season_score_rank_absolute` | number \| null | **Players only** (NULL for teams). The entity's `season_composite_rank_absolute` — percentile rank of `season_composite_score` within the current season **without position partition**. Answers "best player overall this season, regardless of position." Uniform `[0, 100]` within `(sport, season)`. Built by ranking the existing season composite cross-position (NOT by re-percentiling raw stats), so it stays position-fair in its inputs while producing an overall leaderboard. `null` for teams (already sport-wide; no position partition exists to escape) or for players with no eligible season stats. |
| `entity_alltime_score_rank_absolute` | number \| null | **Players only.** The entity's `season_composite_rank_alltime_absolute` — same cross-position logic as `entity_season_score_rank_absolute`, ranked across **every season in the DB** for the sport. "Best player-season overall, regardless of position, ever recorded." Refreshed nightly. `null` for teams or for players with no eligible season stats. |
| `peer_season_score_avg` | number | AVG of `season_composite_score` across the peer cohort. Hovers near 50; tier-rendering anchor. |
| `vibes.window_days` | integer | Currently fixed at `7`. May become a query param when data.scoracle ships. |
| `vibes.snapshots` | object[] | Last-7-days raw sentiment snapshots — `{sentiment, generated_at, trigger_type}` rows ordered newest first. `[]` when the entity has no scores in the window. Still the freshest single number for "right now" news; `entity_season_vibe_series` is the season trajectory companion. |
| `entity_season_vibe_series` | object[] | Daily-bucketed sentiment trajectory across the season, oldest-first. Each row: `{date, sentiment_avg, snapshot_count}` — `date` is the UTC day (`YYYY-MM-DD`); `sentiment_avg` is the integer mean of that day's snapshots on the 0–100 scale (matches the rating scale); `snapshot_count` is how many snapshots aggregated into that day (frontend hover-tooltip: "4 snapshots that day"). **Days with zero snapshots are omitted** so the sparkline renders quiet stretches as honest gaps rather than zero-sentiment dots. **Range: first kickoff of the most-recently-started season in this sport+league scope, through `NOW()`.** During the offseason the anchor stays pinned to the previous season's start, so vibes carry through gap periods (trade rumors, draft news, off-day reactions); once the next season's first fixture kicks off, the anchor moves forward. Per-sport (not per-entity) so two entities in the same scope share a date axis — frontend can compare them on aligned sparklines. `[]` when no season has started yet in scope. The frontend should filter out the long stretch of nulls before the vibe pipeline started accumulating data (~May 2026 in production); going forward the series will populate naturally from the season anchor. |
| `meta.season` | integer | Resolved season used for the peer cohort |
| `meta.league_id` | integer \| null | The league actually used (after the football fallback); `null` for NBA/NFL |
| `meta.position` | string \| null | The position used to partition the peer cohort. `null` for `entity_type=team` (teams have no position dimension). |

#### Peer cohort definition

Mirrors the percentile pipeline:
- **Player** entity → peers are same `sport` + same `position` (+ same `league` for football), excluding the target.
- **Team** entity → peers are same `sport` (+ same `league` for football), excluding the target.

#### Status codes & empty cases

| Case | Status | Body |
|---|---|---|
| Entity exists and has ≥ 1 fixture | 200 | Populated `entity_recent_avgs`, possibly `[]` vibes |
| Entity exists but no fixtures this season (and no prior-season bridge data) | 200 | `games_used: 0`, `entity_recent_avgs: {}`, `peer_season_avgs` still populated |
| Entity doesn't exist | 200 | `games_used: 0`, both avgs `{}`, `peer_cohort_size: 0` |
| Vibe panel empty | 200 | `vibes.snapshots: []` — usually a `starter`/`bench`-tier entity not yet covered by the milestone listener or nightly batch. Frontend should hide the vibes panel rather than render an empty chart. |

Note that entity existence is the **profile endpoint's** job — trends always returns 200. Legacy blurb-only `vibe_scores` rows (`sentiment IS NULL`) are excluded for consistency with the latest-vibe handler.

#### Comparability filter

Both `entity_recent_avgs` and `peer_season_avgs` are filtered to stat keys flagged `comparable = true` in `stat_definitions` (migration 016). A key is comparable when its unit on both sides resolves to the same scale:

- `rate_pct` (percentages, accuracies, efficiencies — directly comparable)
- `per_game_avg` (per-game / per-36 / per-90 derived stats)
- `cumulative_total` **for teams only** — normalized to per-game by dividing each peer's value by `matches_played` (football) / `games_played` (NBA/NFL) before averaging. Coverage of the divisor is 100% for the team tables.

Keys flagged `special` (standings columns like `wins`, `losses`, `goal_difference`, `points`) are never emitted — they're not per-event production stats.

#### Self-delta vs peer-delta — why both

The trends payload returns three dictionaries: `entity_recent_avgs` (last-3-fixture average), `entity_season_avgs` (the entity's own season baseline), and `peer_season_avgs` (the peer cohort's season baseline). The frontend computes two deltas per stat row — `(recent − self) / self` and `(recent − peer) / peer` — and renders both alongside the recent value.

This matters most for **dominant outliers**. A team that leads its league sees `(recent − peer)` skewed positive on virtually every stat regardless of recent form; the peer-delta column becomes a noise floor. The self-delta column cuts through that: `+0% vs self` next to `+155% vs peer` reads as "this is their normal level, not a recent surge", while `+50% vs self` next to `+200% vs peer` reads as "they're stepping up further on top of their usual dominance." For non-outlier entities the two deltas are usually directionally aligned and reinforce the signal.

`entity_season_avgs` is `{}` for player entities — player cumulative_total keys are non-comparable (their per-game / per-90 siblings carry the comparison on the peer side), and per-game / per-90 derived keys for a player are by definition their season baseline, so the self-delta column would be vacuously zero. The frontend should gate the self-delta rendering on the field being non-empty.

#### Composite score — a single number per game

`entity_event_scores`, `entity_season_score_avg`, and `peer_season_score_avg` (migrations 017 + 018) ship a data-driven single-number rating per event in `[0, 100]`, derived per the per-event percentile pipeline described in `progress_docs/2026-05-23_event-percentiles-and-composite-score-proposal.md`.

**Interpretation:** event-level `composite_score` is the **percentile rank of the event's raw composite within its position cohort** (migrations 017 → 018 → 019). Two passes: (1) unweighted mean of per-stat percentiles for stats the player had non-zero values in, producing a raw composite; (2) percent-rank that raw composite against every other same-season same-position event. **Mean per partition is 50 by construction; distribution is uniform in `[0, 100]`.** A 70 reads as "this event ranks in the top 30% of events at this position this season."

**Season `season_composite_score` and `entity_season_score_avg` (migration 020):** different source. It's the **AVG of season-level per-stat percentile ranks** for the entity, filtered to keys flagged `stat_definitions.is_percentile_eligible = true`. Each eligible stat contributes one vote — pure data-driven, no manual weighting.

What's eligible for the season composite:
- **Raw box-score counts** (`goals`, `tackles`, `passing_yards`, NBA `pts`/`reb`/`ast`, etc.)
- **Rate stats** (`fg_pct`, `pass_accuracy`, `true_shooting_pct`) — orthogonal dimensions, not unit re-expressions
- **Outcome stats** for teams (`wins`, `losses`, `draws`, `points`, `goal_difference`, `points_for/against`, `point_differential`, `win_pct`) — objective box-score data; included after migration 020

What's deliberately NOT in the season composite:
- **Per-X derived stats** (`*_per_game`, `*_per_36`, `*_per_90`) — they're the same underlying production as their raw counterparts, just normalized to a different unit. Including both would silently weight that performance 2×. The derived versions still exist in `player_stats.percentiles` JSONB and remain available to other consumers; they just don't enter the composite AVG.
- **Playing-time denominators** (`games_played`, `matches_played`, `minutes_played`, `lineups`) — opportunity, not production.
- **Provider composites** (`rating` from SportMonks) — would double-count itself.

**Composite fields — four layers + an orthogonal absolute axis for players** (migrations 017→026):

| Layer | Field (profile / trends) | Question | Distribution |
|---|---|---|---|
| Per-event | `entity_event_scores[].composite_score` | "How good was this *game*?" | uniform per cohort |
| Season (absolute number) | `season_composite_score` / `entity_season_score_avg` | "How did this *season* compare, cross-season?" | spread, relative-to-cohort |
| Season (in-season rank) | `season_composite_rank` / `entity_season_score_rank` | "Where does this entity *rank* among peers this season?" | uniform `[0, 100]`, top = 100 |
| Season (all-time rank) | `season_composite_rank_alltime` / `entity_alltime_score_rank` | "Is this one of the best seasons we've *ever* recorded?" | uniform `[0, 100]` across all seasons |
| **Players only** — cross-position in-season | `season_composite_rank_absolute` / `entity_season_score_rank_absolute` | "Best player overall this season, regardless of position?" | uniform `[0, 100]` within `(sport, season)` |
| **Players only** — cross-position all-time | `season_composite_rank_alltime_absolute` / `entity_alltime_score_rank_absolute` | "Best player-season overall ever recorded, regardless of position?" | uniform `[0, 100]` across all seasons |

Pick per surface:

- **In-season headline chip / single-season leaderboards** → `season_composite_rank` (position-partitioned, top of each position cohort sits at ~100).
- **"Best player overall" leaderboards** → `season_composite_rank_absolute` (players only). Position cohort still informs the underlying composite, but the leaderboard isn't sliced by position.
- **"All-time greats" / historical surfaces** → `season_composite_rank_alltime` (or `_absolute` if cross-position).
- **Year-over-year trajectory** → `season_composite_score` (cross-season comparable). NEVER use the ranks for cross-season comparison — every cohort's top entity reads ~100 each year by design.
- **Per-game sparkline** → `entity_event_scores[].composite_score`.

Teams have no `_absolute` fields (their position-partitioned ranks are already sport-wide; there's no position partition to escape). The four absolute fields are always `null` for teams.

**Refresh cadence:** the per-event, season composite, and in-season rank all recompute on `finalize_fixture` (current-season work, cheap, immediate). The all-time rank recomputes nightly via the maintenance worker — current season is re-ranked against the full frozen history each night, previous seasons are read-only, and a full re-baseline runs on process startup and at season rollover. The all-time number doesn't need per-game freshness; nightly keeps the finalize path doing only dynamic current-season work.

**Cold-start guard (migration 025):** for the first ~10% of season games, `season_composite_score` is linearly blended with a prior-season anchor (entity's own prev-season composite → prev-season cohort average → 50.0 fallback chain). Window: NBA 8 games, NFL 2 games, football 4 games. Phase-out is continuous and proportional: `blended = α·prior + (1−α)·current_composite` where `α = max(0, (window − games)/window)`. The early-season leaderboard opens as last season's standings and morphs into this season's by the window boundary, instead of being noise dominated by one-game wonders. Once an entity passes the window, the composite is the unmodified current-season value.

**Absolute (cross-position) ranks for players (migration 026):** orthogonal to the within-position ranks. `season_composite_rank` partitions by position (top Center, top Striker); `season_composite_rank_absolute` ranks across all positions for "best player overall." The absolute variant ranks the existing position-relative composite cross-position rather than re-percentiling raw stats — keeps the inputs position-fair while producing an overall leaderboard. Teams have no equivalent (their existing ranks are already sport-wide).

Cross-season comparison: each entity's `season_composite_score` is partitioned by `(sport, season, position)` for players / `(sport, season)` for teams. Previous seasons stay frozen (`finalize_fixture` only operates on the current season). Comparing the same entity year-over-year is "their average percentile vs same-position peers in that year." Caveat: it's relative-to-cohort rather than truly absolute, so cohort strength shifts (e.g. a stacked Center class in 2024) can move a player's number even with identical absolute production. `season_composite_rank` is explicitly within-season — do NOT use it for cross-season comparison (every cohort's top entity reads ~100 each year).

**Sample-size disclaimers:** `minutes_played` is surfaced alongside each event score so the frontend can flag short appearances (e.g. football subs under ~30 min where per-90 normalization can produce extreme scores from a single stat). The decision to NOT filter low-minute appearances was explicit — per-90 normalization is partly meant to illuminate underused players, and dropping those events server-side would defeat that. At full-season sparkline density (82 dots for NBA) the per-dot disclaimers naturally move to hover-tooltips rather than inline labels.

**Specialists vs well-rounded:** because the formula is an unweighted mean, a specialist who's elite at one stat but average elsewhere scores around 55, while a well-rounded entity scores higher. The per-stat percentiles in the existing trends payload (`entity_recent_avgs` etc.) remain the breakdown view; the composite score is the summary view.

**Form-streak rendering:** the `entity_event_scores` array is sufficient input for any client-side "N games above 70" display or full-season sparkline — no additional endpoint needed. The frontend's natural reading direction (oldest→newest, left→right) is the reverse of the server's newest-first ordering; reverse on the client.

Known limitation — **NFL/football player trends** have a small intersection between sides because the seeder writes raw per-event counts (`passing_yards`, `tackles`) to `event_box_scores` but writes derived per-game / per-90 keys (`passing_yards_per_game`, `tackles_per_90`) to `player_stats`. The trends card on player pages for these sports shows fewer rows until the seeder schema is unified.

#### Response example (player)

```json
{
  "page": "momentum",
  "sport": "nba",
  "entity_type": "player",
  "entity_id": 123,
  "window": {
    "games_used": 3,
    "fixture_ids": [9912, 9905, 9897],
    "spans_prior_season": false
  },
  "entity_recent_avgs": { "pts": 28.3, "reb": 8.1, "ast": 6.4 },
  "entity_season_avgs": {},
  "peer_season_avgs":   { "pts": 19.1, "reb": 5.4, "ast": 4.0 },
  "entity_event_scores": [
    { "fixture_id": 9912, "composite_score": 78.4, "minutes_played": 38, "start_time": "2026-04-18T02:30:00Z" },
    { "fixture_id": 9905, "composite_score": 82.1, "minutes_played": 35, "start_time": "2026-04-15T00:00:00Z" },
    { "fixture_id": 9897, "composite_score": 71.2, "minutes_played": 40, "start_time": "2026-04-12T02:30:00Z" },
    "... // every played event in the current season, newest first"
  ],
  "entity_season_score_avg": 75.3,
  "entity_season_score_rank": 96.0,
  "entity_alltime_score_rank": 98.4,
  "entity_season_score_rank_absolute": 94.2,
  "entity_alltime_score_rank_absolute": 97.1,
  "peer_season_score_avg":   50.0,
  "peer_cohort_size": 87,
  "vibes": {
    "window_days": 7,
    "snapshots": [
      { "sentiment": 78, "generated_at": "2026-05-22T11:00:14Z", "trigger_type": "milestone" },
      { "sentiment": 74, "generated_at": "2026-05-21T03:01:02Z", "trigger_type": "periodic" },
      { "sentiment": 71, "generated_at": "2026-05-20T03:00:55Z", "trigger_type": "periodic" }
    ]
  },
  "entity_season_vibe_series": [
    { "date": "2025-10-23", "sentiment_avg": 68, "snapshot_count": 3 },
    { "date": "2025-10-24", "sentiment_avg": 72, "snapshot_count": 5 },
    { "date": "2025-10-26", "sentiment_avg": 65, "snapshot_count": 2 },
    "... // one row per UTC day with >=1 snapshot, oldest first, through NOW()"
  ],
  "meta": { "season": 2025, "league_id": null, "position": "PG" }
}
```

#### Response example (team — note `meta.position: null`)

```json
{
  "page": "momentum",
  "sport": "football",
  "entity_type": "team",
  "entity_id": 18,
  "window": {
    "games_used": 3,
    "fixture_ids": [22781, 22769, 22754],
    "spans_prior_season": false
  },
  "entity_recent_avgs": { "shots_total": 14.7, "possession_pct": 58.2, "accurate_passes": 521.3 },
  "entity_season_avgs": { "shots_total": 12.4, "possession_pct": 55.1, "accurate_passes": 478.6 },
  "peer_season_avgs":   { "shots_total": 11.9, "possession_pct": 49.5, "accurate_passes": 442.1 },
  "entity_event_scores": [
    { "fixture_id": 22781, "composite_score": 20.1, "minutes_played": null, "start_time": "2026-05-19T18:30:00Z" },
    { "fixture_id": 22769, "composite_score": 26.6, "minutes_played": null, "start_time": "2026-05-09T15:00:00Z" },
    { "fixture_id": 22754, "composite_score": 77.9, "minutes_played": null, "start_time": "2026-05-04T15:30:00Z" },
    "... // every played fixture in the current season, newest first"
  ],
  "entity_season_score_avg": 65.0,
  "entity_season_score_rank": 80.0,
  "entity_alltime_score_rank": 79.7,
  "entity_season_score_rank_absolute": null,
  "entity_alltime_score_rank_absolute": null,
  "peer_season_score_avg":   48.0,
  "peer_cohort_size": 19,
  "vibes": {
    "window_days": 7,
    "snapshots": []
  },
  "entity_season_vibe_series": [
    { "date": "2025-08-17", "sentiment_avg": 55, "snapshot_count": 1 },
    { "date": "2025-08-30", "sentiment_avg": 70, "snapshot_count": 2 },
    "... // anchored at oldest scored fixture, through NOW()"
  ],
  "meta": { "season": 2025, "league_id": 8, "position": null }
}
```

> **Forward-compatibility — data.scoracle.** The endpoint is pure read-only SQL with no derived state stored anywhere. The CTE chain in each prepared statement (one per sport, in `go/internal/db/db.go`) lifts directly into a SQL function — `get_entity_trends(sport, entity_type, entity_id, season, league_id, window_size)` — and can be exposed on **data.scoracle** (the planned PostgREST surface) as an RPC the frontend calls directly with user-selected scope. Same query, different transport, no new derivation logic.

### `GET /api/v1/{sport}/team/{id}/results`

Returns a team's **finalized scorelines** for a season — every fixture with `status` in `('completed', 'seeded')`, framed from the team's perspective. Each row carries the opponent's identity (id, name, short_code, logo_url), home/away framing, the team's own score, the opponent's score, and a `W`/`L`/`D` result derived from the two scores.

Designed as a reusable scoreline source: a results strip, a form guide, head-to-head context, and similar UI all consume the same payload.

Cache: 5 min TTL (`X-Cache: HIT/MISS`), ETag-enabled — send `If-None-Match` for a 304.

#### Path parameters

| Name | Type | Notes |
|---|---|---|
| `sport` | string | `nba`, `nfl`, or `football` |
| `id` | integer | Team ID |

#### Query parameters

| Name | Type | Notes |
|---|---|---|
| `season` | integer (optional) | Defaults to the sport's `current_season` |
| `league_id` | integer (optional) | Filter to a specific league. **For football**, when omitted the team's natural league (from `team_stats`) is used so the response doesn't blend league + cup fixtures from different competitions. NBA/NFL ignore it. |

The league-scoped route `/api/v1/{sport}/leagues/{leagueId}/team/{id}/results` is an alias that takes `leagueId` from the path instead of the query string — identical payload.

#### Response fields

| Field | Type | Description |
|---|---|---|
| `page` | string | Literal `"results"` |
| `sport`, `team_id` | echo of inputs | |
| `results` | object[] | One entry per finalized fixture, ordered **newest first** by `start_time`. Empty `[]` when the team has no finalized fixtures in scope. |
| `results[].fixture_id` | integer | `fixtures.id` |
| `results[].start_time` | timestamp | UTC ISO-8601 |
| `results[].status` | string | Either `"completed"` or `"seeded"` (other statuses are excluded) |
| `results[].round` | string \| null | Sport-specific round/week label when set |
| `results[].home_away` | string | `"home"` or `"away"` from the requested team's perspective |
| `results[].team_score` | integer | The requested team's score |
| `results[].opponent_score` | integer | The opponent's score |
| `results[].result` | string \| null | `"W"`, `"L"`, `"D"`, or `null` if either score is missing |
| `results[].composite_score` | number \| null | Team-level composite score for this fixture in `[0, 100]` (migration 017). Same scale as the trends endpoint's composite fields. `null` if the team has no `event_team_stats` row for the fixture (e.g. a finalize that hasn't fully run yet). |
| `results[].opponent` | object | `{id, name, short_code, logo_url}` of the other team. Fields may be `null` if the opponent row is missing from `teams`. |
| `meta.season` | integer | Resolved season used |
| `meta.league_id` | integer \| null | The league actually used (after the football fallback); `null` for NBA/NFL or unscoped football |
| `meta.games_played` | integer | Count of entries in `results` — a convenience so the frontend doesn't need to `.length` to gate UI |

#### Response example

```json
{
  "page": "results",
  "sport": "nba",
  "team_id": 14,
  "results": [
    {
      "fixture_id": 9912,
      "start_time": "2026-04-12T02:30:00Z",
      "status": "seeded",
      "round": null,
      "home_away": "home",
      "team_score": 112,
      "opponent_score": 108,
      "result": "W",
      "composite_score": 62.4,
      "opponent": { "id": 7, "name": "Boston Celtics", "short_code": "BOS", "logo_url": "https://…/celtics.png" }
    },
    {
      "fixture_id": 9905,
      "start_time": "2026-04-10T00:00:00Z",
      "status": "completed",
      "round": null,
      "home_away": "away",
      "team_score": 99,
      "opponent_score": 104,
      "result": "L",
      "composite_score": 41.7,
      "opponent": { "id": 3, "name": "Denver Nuggets", "short_code": "DEN", "logo_url": "https://…/nuggets.png" }
    }
  ],
  "meta": { "season": 2025, "league_id": null, "games_played": 2 }
}
```

### `GET /api/v1/{sport}/meta`

Legacy sport-wide metadata/search payload. New frontend surfaces should hydrate page islands from dedicated backend endpoints like `/{sport}/{entityType}/{id}/meta`, `/stats`, `/rating`, `/news`, `/transfers`, `/momentum`, and `/sigil`. Do not use this as a new frontend local metadata DB; home search should use `GET /api/v1/entities`.

Query parameters:
- `league_id` (optional integer) - Scope to specific league

Response includes:
- `meta_version` - Unix timestamp of last data update (for cache invalidation)
- `current_season` - The sport's current active season year
- `total_entities` - Count of players and teams in the response
- `items` - All entities (players + teams) with search tokens and metadata
- `stat_definitions` - All stat keys with display names and categories
- `leagues` - League information (populated for multi-league sports like football)

### `GET /api/v1/{sport}/autofill`

Legacy sport-scoped autofill/search payload for downstream and profile-specific consumers. It currently returns the same payload as `/{sport}/meta` today (same `{sport}_meta_page` statement, same `league_id` query param). Keep it unchanged for existing consumers; the universal home-page search DB is `GET /api/v1/entities` (alias: `/api/v1/autofill`).

Query parameters:
- `league_id` (optional integer) - Scope to specific league

### `GET /api/v1/{sport}/health`

Returns sport-level data freshness and counts.

Query parameters:
- `league_id` (optional integer) - Scope to specific league

Response includes:
- Last update timestamp
- Fixture counts
- Box score coverage stats
- Data freshness indicators

## Rating Engine Endpoints

The **Scoracle Rating Engine** (migrations 027–028) is a **separate dataset** from the
profile/meta payloads above — it does not touch the counting-stats/pizza payload, and
the rating numbers are **not** in legacy sport-level `/meta`. The rating engine has
dedicated endpoints: a **leaderboard** (the sport-wide
board), a **roster** (that same board narrowed to one team), and a **sparkline**
(per-entity). The leaderboard and roster share one row shape — see the roster
note below.

### What the rating engine computes

Every entity is rated **positionlessly** by the z-score of each de-duped box-score
datapoint against the whole population. Two complementary scores, **never merged**:

- **`rating_composite`** — Σ z (breadth → all-rounders/grinders). Raw z-sum (e.g. ~12.5
  for the best player). For **NFL players** the Composite is **category-balanced**
  (offense / defense / special-teams facets, mean-of-z per facet, summed) so recording
  granularity doesn't silently weight defense 2×. NBA + football players and **all teams
  are flat** (Σ z) — a team is multi-phase, so no facet-balancing.
- **`rating_peak`** — peak z over the positive counting set (irreplaceability →
  difference-makers).
- **`rating_peak_label`** — the argmax datapoint label (e.g. `"Rim Protection"`,
  `"Sacks"`, `"Goalscoring"`; teams e.g. `"Steals"`, `"Goals For"`).
- **`rating_composite_rank` / `rating_peak_rank`** — positionless `percent_rank` ×
  100 (0–100), a friendly headline number; the raw z drives ordering.

No weighting, no hand-picked baselines — the z-score *is* the scarcity weighting (a rare
skill sits further from the mean → larger z). Floors: NBA ≥30 GP & ≥20 MPG; football ≥15
apps; NFL ≥8 GP (players). Teams: all rated.

### `GET /api/v1/{sport}/leaderboard`

The DB-first ranking surface. `/leaderboard` is the comprehensive ranked research
database; `/profile` is the deep drill-down for one selected entity. Roster discovery
is now a leaderboard scope (`entity_type=player&team_id=...`), not a team profile tab.

The default board is **Rating**. Rating rows carry both Composite and Specialist
(+ specialty); product boards use the same cohort controls and rank their own DB
signal.

**Board selector:** pass `?board=` to get any board from this one endpoint —
`rating` (default), `vibes`, `sigil`, `news`, `transfers`, or `momentum`.
`trending` remains a legacy alias for Momentum. The dedicated `/leaderboard/{board}`
routes remain for now where registered. The **`news` board ranks the hottest model
narratives by per-narrative impact** (each row = an entity's top narrative in the
selected news scope), superseding the old raw mention-count and standalone headlines
boards.

#### Query parameters

| Param | Type | Default | Notes |
|---|---|---|---|
| `entity_type` | string | `player` | `player` or `team`. **The type differentiator** — team calls return the team board (`team_stats`), player calls the player board (`player_stats`). |
| `scope` | string | `composite` | `composite`, `specialist`, or a **specialty label** (e.g. `Sacks`, `3PT Shooting`) for a per-skill board (rows whose top skill matches, ordered by peak z). Case-insensitive. |
| `season` | integer | latest rated | Season year. |
| `position` | string | — | Player boards only — the position scope ("best Center / QB"). |
| `position_group` | string | — | Player boards only — normalized group such as `Guard`, `Forward`, `Goalkeeper`. |
| `league_id` | integer | — | Filter to a league (football). |
| `conference` | string | — | Team/conference cohort filter where present in team metadata. |
| `division` | string | — | Team/division cohort filter where present in team metadata. |
| `team_id` | integer | — | Narrows to one team. With `entity_type=player`, includes the team's active/current roster from `team_rosters`; scored rows rank first and null product rows append with nullable metric/rank fields. |
| `limit` | integer | `50` | Max rows. |

#### Response shape

```jsonc
{
  "page": "leaderboard",
  "sport": "nba",
  "entity_type": "player",          // echoes the request
  "season": 2025,
  "scope": "composite",
  "count": 3,
  "leaders": [
    {
      "entity_type": "player",       // "player" | "team"
      "id": 56677822,
      "name": "Victor Wembanyama",
      "image": "https://…",          // player photo_url | team logo_url
      "position": "F-C",             // null for teams
      "team_id": 27,
      "team_name": "San Antonio Spurs",
      "team_code": "SAS",
      "team_logo": "https://…",
      "league_id": null,
      "rating_composite": 12.5226,
      "rating_peak": 6.1058,
      "rating_peak_label": "Rim Protection",
      "rating_composite_rank": 100.0,
      "rating_peak_rank": 100.0,
      "rank": 1
    }
  ]
}
```

Examples:
- `GET /api/v1/nba/leaderboard` → top 50 NBA players by Composite, latest season.
- `GET /api/v1/nba/leaderboard?entity_type=team` → the NBA **team** board.
- `GET /api/v1/nfl/leaderboard?scope=specialist&limit=10` → the irreplaceables board.
- `GET /api/v1/nba/leaderboard?scope=3PT%20Shooting` → the 3-point-specialist board.

### `GET /api/v1/{sport}/leaderboard/vibes`

The sport-wide **vibe** board — entities ranked by their latest model sentiment score
(1-100) in the last 48h: the sport-wide hottest-by-sentiment board (same window + filters,
but each row is joined to `players`/`teams` so it carries `name` / `image` / `team_*` —
one row shape shared with the news board below.

| Param | Type | Default | Notes |
|---|---|---|---|
| `entity_type` | string | both | `player` or `team`; omit for a mixed board. |
| `limit` | integer | `50` | Max rows. |

```jsonc
{
  "page": "vibes_leaderboard",
  "sport": "nba",
  "entity_type": "player",            // echoes the request, "all" when unfiltered
  "count": 3,
  "leaders": [
    {
      "entity_type": "player",         // "player" | "team"
      "id": 1057262088,
      "name": "Cooper Flagg",
      "image": "https://…",            // player photo_url | team logo_url (may be null)
      "team_id": 7,
      "team_name": "Dallas Mavericks",
      "team_code": "DAL",
      "team_logo": "https://…",
      "score": 92,                     // latest sentiment (1-100)
      "generated_at": "2026-06-04T12:04:16-04:00",
      "rank": 1
    }
  ]
}
```

### `GET /api/v1/{sport}/leaderboard/sigil`

The sport-wide **Sigil** board — entities ranked by their latest **Sigil crown score** (1-100),
the holistic Rating+Vibe synthesis the Product Narrative stack-ranks at the front door. Same
enriched row shape as the vibe board (`name` / `image` / `team_*`), plus `previous_score` so the
front door can render the crown's delta. Sourced from `sigil_synthesis` (latest scored row per
entity). Also reachable as `GET /api/v1/{sport}/leaderboard?board=sigil`.

| Param | Type | Default | Notes |
|---|---|---|---|
| `entity_type` | string | both | `player` or `team`; omit for a mixed board. |
| `limit` | integer | `50` | Max rows. |

```jsonc
{
  "page": "sigil_leaderboard",
  "sport": "nba",
  "entity_type": "player",            // echoes the request, "all" when unfiltered
  "count": 3,
  "leaders": [
    {
      "entity_type": "player",         // "player" | "team"
      "id": 15,
      "name": "Giannis Antetokounmpo",
      "image": "https://…",            // player photo_url | team logo_url (may be null)
      "team_id": 17,
      "team_name": "Milwaukee Bucks",
      "team_code": "MIL",
      "team_logo": "https://…",
      "score": 95,                     // latest Sigil crown score (1-100)
      "previous_score": 91,            // prior crown score (may be null) — for the delta
      "blurb": "…",                    // the Sigil felt-read
      "generated_at": "2026-06-18T14:33:22-04:00",
      "rank": 1
    }
  ]
}
```

### `GET /api/v1/{sport}/leaderboard/news`

The sport-wide **news** board — the **hottest model narratives**, ranked by per-narrative
`impact`. Each row is an entity's top narrative in the selected scope, enriched like the vibe board
(player/team name, image, current club) plus source freshness and trajectory fields. Supersedes the
old mention-count and standalone headlines boards. Also reachable as `/leaderboard?board=news`.

| Param | Type | Default | Notes |
|---|---|---|---|
| `entity_type` | string | both | `player` or `team`; omit for a mixed board. |
| `scope` | string | `current_week` | `current_week`, `last_week`, `two_weeks_ago`, `three_weeks_ago`, `last_month`. |
| `limit` | integer | `50` | Max rows. |

```jsonc
{
  "page": "news_leaderboard",
  "sport": "nba",
  "entity_type": "team",
  "scope": {"key": "current_week", "label": "Current week", "starts_at": "...", "ends_at": "..."},
  "count": 3,
  "leaders": [
    {
      "entity_type": "team",
      "id": 27,
      "name": "San Antonio Spurs",
      "image": "https://…",
      "team_id": 27,
      "team_name": "San Antonio Spurs",
      "team_code": "SAS",
      "team_logo": "https://…",
      "narrative_title": "...",
      "body": "...",
      "score": 85,                     // narrative impact
      "updated_at": "2026-07-03T13:47:21-04:00",
      "source_count": 4,
      "source_names": ["BBC Sport", "ESPN"],
      "trajectory": "heating_up",
      "trajectory_label": "Heating up",
      "generated_at": "2026-07-03T14:01:10-04:00",
      "rank": 1
    }
  ]
}
```

### `GET /api/v1/{sport}/leaderboard/transfers`

The sport-wide **transfers** board — the hottest model-vetted `(team, player)` rumors,
ranked by deterministic `heat` (0-100): latest row per pair (`DISTINCT ON`),
`is_rumor IS TRUE`, with **both** sides of the pair on each row. Transfers share the
same historical scopes and current-week cooling-off retirement rule as Narratives. The
per-entity transfer scope is the `/{entityType}/{id}/transfers` product (the old
team-only `/team/{id}/transfers` + player-only `/player/{id}/suitors` routes were
unified into it 2026-06-15).

| Param | Type | Default | Notes |
|---|---|---|---|
| `limit` | integer | `50` | Max rows. |
| `scope` | string | `current_week` | `current_week`, `last_week`, `two_weeks_ago`, `three_weeks_ago`, `last_month`. |

```jsonc
{
  "page": "transfers_leaderboard",
  "sport": "football",
  "scope": {"key": "current_week", "label": "Current week", "starts_at": "...", "ends_at": "..."},
  "count": 432,
  "rumors": [
    {
      "player_id": 154421,
      "player_name": "Erling Haaland",
      "player_image": "https://…",
      "team_id": 3468,
      "team_name": "Real Madrid",
      "team_code": "RMA",
      "team_logo": "https://…",
      "heat": 95,                      // 0-100 deterministic heat index
      "heat_components": { "volume": 1.0, "recency": 0.994, "tier_weight": 1.0, "distinct_sources": 9, "…": "…" },
      "direction": "incoming",          // "incoming" | "outgoing" | "unclear" | null
      "stage": "speculation",           // speculation | concrete_interest | advanced_talks | here_we_go | null
      "summary": "…",
      "source_attribution": "…",
      "updated_at": "2026-06-04T…",
      "source_count": 4,
      "source_names": ["BBC Sport", "ESPN"],
      "source_latest_at": "2026-06-04T…",
      "source_oldest_at": "2026-06-02T…",
      "trajectory": "heating_up",
      "trajectory_label": "Heating up",
      "trajectory_components": {"reason": "heat_up", "heat_delta": 14},
      "generated_at": "2026-06-04T…",
      "rank": 1
    }
  ]
}
```

### `GET /api/v1/{sport}/leaderboard/momentum`

The sport-wide **Momentum** board — entities whose Vibe or Rating is climbing fastest.
Pass `?metric=vibe` (default) or `?metric=rating`; the response echoes `"metric"`.
Same enriched row shape as the other boards (`name` / `image` / `team_*`). Reached via
the dedicated path or `/leaderboard?board=momentum`. `entity_type`, shared cohort
filters, and `limit` query params apply. `/leaderboard/trending` and
`/leaderboard?board=trending` remain legacy aliases.

```jsonc
{ "page": "trending_leaderboard", "sport": "nba", "metric": "vibe", "count": 50, "leaders": [ /* … */ ] }
```

### `GET /api/v1/{sport}/team/{id}/roster`

**Legacy compatibility.** New clients should use
`GET /api/v1/{sport}/leaderboard?entity_type=player&team_id={id}` for roster
discovery. That leaderboard scope includes the full active/current roster from
`team_rosters`, with scored product rows first and unscored roster rows appended
with nullable metric/rank fields.

This route remains temporarily for older clients. It returns rated players whose
`player_stats.team_id` is this team for the season and orders them by the sum of
Composite + Specialist.

#### Path / query parameters

| Param | Type | Default | Notes |
|---|---|---|---|
| `id` (path) | integer | — | Team ID. |
| `season` | integer | latest rated | Season year (latest season the team has rated players for). |
| `league_id` | integer | — | League filter (football). |

#### Response shape

```jsonc
{
  "page": "roster",
  "sport": "nba",
  "team_id": 21,
  "season": 2025,
  "count": 9,
  "players": [
    {
      "id": 175,
      "name": "Shai Gilgeous-Alexander",
      "image": null,                   // player photo_url (may be null)
      "position": "G",
      "rating_composite": 8.6853,
      "rating_peak": 3.0034,
      "rating_peak_label": "Scoring",
      "rating_composite_rank": 98.4,
      "rating_peak_rank": 94.4,
      "rank": 1                          // 1-based, by (rating_composite + rating_peak) DESC
    }
  ]
}
```

`players` is `[]` if the team has no rated players in scope. Player floors apply
(NBA ≥30 GP & ≥20 MPG, etc.), so a partially-seeded team returns a short list.

> **Shared shape — board ⇄ roster.** `leaderboard.leaders[]` and `roster.players[]`
> are the **same rating-row shape** (`id`, `name`, `image`, `position`,
> `rating_composite` / `rating_peak` `(+ _rank)`, `rating_peak_label`,
> `rank`). Roster is just that player board narrowed to one team and re-sorted by
> the Composite+Specialist sum — which is exactly why one frontend list component
> (`RatingList`) renders both. Any future "board over a different slice" (a
> conference board, a draft-class board, …) is the same recipe: identical row,
> different `WHERE` filter + `ORDER BY`.

### `GET /api/v1/{sport}/{entityType}/{id}/stats` &nbsp;·&nbsp; `/rating` &nbsp;(stats source)

> **Convergence rename (O14):** `/special` is gone — its lean specialist projection + the
> Model stat `commentary` folded into **`/rating`** (the "divined" statistical read). The
> retired `/sparkline` + `/starline` (2026-06-15) were split into the per-product stats
> source: `/stats` carries the rating + `available_seasons` + the per-event `events` series;
> `/momentum` carries the season sparkline (rating series × vibe series). The query
> parameters + rating/event field shapes below are unchanged.

The dedicated rating dataset for **one entity**: the season Composite/Specialist (the
numbers a meta card shows), **that season's team**, **and** the per-event dual-sparkline
series. `entityType` (`player` or `team`) comes from the path — team stats read
`event_team_stats`, player stats `event_box_scores`.

#### Query parameters

| Param | Type | Default | Notes |
|---|---|---|---|
| `season` | integer | latest rated | Season year. |
| `league_id` | integer | — | League filter (football). |

#### Response shape

```jsonc
{
  "page": "stats",
  "sport": "nba",
  "entity_type": "player",
  "entity_id": 56677822,
  "season": 2025,
  "rating": {                        // the season summary (meta-card numbers)
    "season": 2025,
    "league_id": null,
    "position": "F-C",               // null for teams
    "team": {                        // that SEASON's team (season-aware) — for players, the
      "id": 27, "name": "San Antonio Spurs", "short_code": "SAS", "logo_url": "https://…"
    },                               //   team they played for that year; teams: themselves. null if unknown.
    "rating_composite": 12.5226,
    "rating_composite_rank": 100.0,
    "rating_peak": 6.1058,
    "rating_peak_rank": 100.0,
    "rating_peak_label": "Rim Protection",
    "rating_categories": null,       // TEAMS ONLY: {facet → {z, pct}} ready-made, e.g.
                                     // {"offense":{"z":0.60,"pct":86.2},"defense":{"z":0.93,"pct":93.1}}
    "rating_breakdown": [            // per-datapoint transparency (migrations 030/037/038)
      { "label": "Rim Protection", "value": 3.8, "z": 6.1058, "pct": 100.0,
        "in_comp": true, "in_spec": true, "sign": 1, "facet": "all",
        "is_specialty": true },
      { "label": "Ball Security", "value": 1.9, "z": 0.9999, "pct": 15.1,
        "in_comp": true, "in_spec": false, "sign": -1, "facet": "all",
        "is_specialty": false }
      // … one object per datapoint. `value` = raw volume. Teams: facet is
      // 'offense'/'defense' (+ display-only 'discipline'/'squad' in football).
    ]
  },
  "events": [                        // the dual-sparkline series, chronological
    { "fixture_id": 12345, "start_time": "2025-10-26T…",
      "rating_composite": 16.015, "rating_peak": 6.85, "rating_peak_label": "Rim Protection",
      "rating_composite_pct": 99.4, "rating_peak_pct": 96.2 }
  ]
}
```

`rating` is `null` and `events` is `[]` if the entity has no rated season.

**`rating.rating_breakdown`** (migration 030) is the per-datapoint transparency
behind the Composite/Specialist scores — one object per datapoint the engine
z-scores. Each carries the raw `z`, its 0–100 `pct` (`percent_rank` of `sign*z`
within the `(sport, season, label)` population, so negative datapoints like
turnovers read correctly: low raw value → high pct), the `in_comp` / `in_spec` /
`sign` / `facet` config, and `is_specialty` (the single peak skill — exactly one
per entity, matching `rating_peak_label`). **Stored as raw z, served as a
percentile** — `pct` is what the UI draws (the Composite tab pizzas the `in_comp`
rows by `pct`; the Specialist tab heros the `is_specialty` row). Each datapoint also
carries **`value`** (migration 038) — the raw volume behind the z (e.g. `27.3` ppg),
so the UI shows the underlying counting stat next to its percentile.

**`rating.rating_categories`** (migration 037, **teams only** — `null` for players) is
the per-category summary served ready-made: `{facet → {z, pct}}` where the category `z`
is the mean of `sign*z` over that facet's `in_comp` datapoints and `pct` its
`percent_rank` within the `(sport, season, facet)` population. Teams are tagged
`offense` / `defense` (the rated categories) plus display-only `discipline` / `squad`
(football cards/injuries) which carry no category score. Margins (point/goal
differential) are intentionally **not** rated — teams are scored on the HOW, not outcomes.

Each event also carries **`rating_composite_pct` / `rating_peak_pct`**
(migration 029): the **0–100 positionless percentile** of that event's z within the
`(sport, season)` event population — a `percent_rank × 100` over the per-event z's,
the same normalization the season ranks use. These let the per-event lines be drawn
on the same **0–100 axis** as the vibe series (the frontend Trends sparkline plots
Composite + Specialist + Vibes together). The raw `rating_composite` /
`rating_peak` z's are unchanged; the pct columns sit beside them.

### `GET /api/v1/{sport}/{entityType}/{id}/rating`

The stats-rail **end product** (convergence rename O14 — absorbed the old `/special`): the
lean specialist projection plus the model's "divined" stat commentary. The commentary now carries
deterministic PEAK trajectory metadata from recent event Composite and PEAK z-score values, so
consumers can surface direction from the same metrics used by the ranking engine without asking the
model to infer form. Same path params and `season`/`league_id` query params as `/stats`.

```jsonc
{
  "page": "rating",
  "sport": "nba",
  "entity_type": "player",
  "entity_id": 56677822,
  "season": 2025,
  "rating": {                        // the lean specialist projection (no fantasy/template/datapoints)
    "rating_composite": 12.5226, "rating_composite_score": 100.0, "rating_composite_rank": 100.0,
    "rating_peak": 6.1058, "rating_peak_rank": 100.0, "rating_peak_score": 100.0,
    "rating_peak_label": "Rim Protection", "rating_breakdown": [ /* … */ ], "rating_modes": { /* … */ }
  },
  "commentary": {                    // latest stat_summaries generation that carries a body; null otherwise
    "body": "…", "notability": 88, "notability_components": {}, "season": 2025,
    "prompt_version": "…", "generated_at": "2026-06-20T…", "divined_peak": "Rim Protection",
    "peak_trajectory": "falling",
    "peak_trajectory_label": "Composite and PEAK z-scores trending down over recent games",
    "peak_trajectory_components": {
      "source": "event_rating_z_scores",
      "metrics": ["rating_composite", "rating_peak"],
      "combined_z_slope": -0.5,
      "composite_z_slope": -0.4,
      "peak_z_slope": -0.6,
      "recent_composite_z": [2.1, 1.8, 1.1],
      "recent_peak_z": [1.7, 1.0, 0.4]
    }
  }
}
```

`commentary` is `null` when the latest `stat_summaries` generation for the entity-season is a
no-stats marker (body `NULL`) — the canonical latest-generation rule (Session 11) clears stale
prose rather than serving it. `divined_peak` is the commentary's headline skill (renamed from
`divined_sigil` by migration 094). `peak_trajectory` is one of `rising`, `falling`, or `steady`;
the label is nullable when the recent event z-score sample is too sparse to make a useful claim.

## League-Scoped Endpoints

League-scoped routes are required for football (which has multiple leagues) and preferred when league context is explicit.

### `GET /api/v1/{sport}/leagues/{leagueId}/{entityType}/{id}` — REMOVED (O16)

The bundled league profile route was removed with its non-league sibling. Use the
per-product league routes (e.g. `/leagues/{leagueId}/{entityType}/{id}/momentum`,
`/leagues/{leagueId}/meta`).

### `GET /api/v1/{sport}/leagues/{leagueId}/{entityType}/{id}/momentum`

League-scoped alias for the momentum endpoint above. `leagueId` is taken from the URL path instead of the `league_id` query parameter; **the response body is identical** to the canonical route. Useful when the calling page already has league context (e.g., a Premier League team profile) and you want to keep that context in the URL.

Example: `GET /api/v1/football/leagues/8/team/18/momentum` is equivalent to `GET /api/v1/football/team/18/momentum?league_id=8`.

### `GET /api/v1/{sport}/leagues/{leagueId}/meta`

Returns metadata payload scoped to a specific league.

### `GET /api/v1/{sport}/leagues/{leagueId}/health`

Returns health/freshness payload scoped to a specific league.

## News-Source Products (per-entity card endpoints)

The old bundled **news rail** (`/news` = narratives + transfers + vibe in one payload)
was split into self-contained products on 2026-06-15. `news` is the data
**source**; each card fetches exactly its own product. Both take the same path
params (`sport` ∈ `nba|nfl|football`, `entityType` ∈ `player|team`, `id`). The
per-entity **vibe** product is no longer served here — it is folded into the Sigil
crown synthesis at `GET /api/v1/{sport}/{entityType}/{id}/sigil`.

**`GET /api/v1/{sport}/{entityType}/{id}/news`** — the entity's scoped model
**narratives** (hottest first by `impact`). News is a post-transfers pipeline layer, so
the narratives already carry transfer context, source freshness, and trajectory markers.

Query `scope` defaults to `current_week`; allowed values are `current_week`, `last_week`,
`two_weeks_ago`, `three_weeks_ago`, and `last_month`.
```json
{ "page": "news", "sport": "football", "entity_type": "team", "entity_id": 18,
  "scope": {"key": "current_week", "label": "Current week", "starts_at": "...", "ends_at": "..."},
  "narratives": [
    {"narrative_title": "...", "body": "...", "impact": 85, "impact_components": {},
     "source_attribution": null, "input_news_ids": [], "updated_at": "...",
     "source_count": 4, "source_names": ["BBC Sport", "ESPN"],
     "source_latest_at": "...", "source_oldest_at": "...",
     "trajectory": "heating_up", "trajectory_label": "Heating up",
     "trajectory_components": {}, "generated_at": "..."}
  ] }
```
`narratives`: latest generation within the selected scope only, ordered by `impact` DESC; `[]` when none.

**`GET /api/v1/{sport}/{entityType}/{id}/transfers`** — the scoped vetted transfer/trade
rumor heat list (the pre-narrative data). Transfers use the same historical scope and
staleness protocol as Narratives; in the current week, cooling-off rows retire after
three days unless they heat back up. The counterparty is the OTHER entity type: for a
`team` the linked **players**, for a `player` the **clubs**.

Query `scope` defaults to `current_week`; allowed values are `current_week`, `last_week`,
`two_weeks_ago`, `three_weeks_ago`, and `last_month`.
```json
{ "page": "transfers", "sport": "football", "entity_type": "team", "entity_id": 18,
  "scope": {"key": "current_week", "label": "Current week", "starts_at": "...", "ends_at": "..."},
  "transfers": [
    {"id": 448448, "name": "Marc Cucurella", "image": "...", "heat": 53, "heat_components": {}, "direction": "outgoing", "stage": "speculation", "summary": "...", "source_attribution": "...",
     "updated_at": "...", "source_count": 3, "source_names": ["Sky Sports"], "source_latest_at": "...", "source_oldest_at": "...",
     "trajectory": "developing_story", "trajectory_label": "Developing story...", "trajectory_components": {}, "rank": 1}
  ] }
```
`transfers`: vetted (`is_rumor`, `heat > 0`), latest per pair in the selected scope, ranked by heat (top 25); `[]` when none.

### `GET /api/v1/{sport}/{entityType}/{id}/meta`

The entity's **identity metadata** — the payload that drives the page-header island and is eager-loaded first. This makes a frontend local metadata DB unnecessary. Returns **404** when the entity doesn't exist.

Path parameters:
- `sport` - `nba`, `nfl`, or `football`
- `entityType` - `player` or `team`
- `id` - Entity ID (integer)

Player response:
```json
{
  "entity_type": "player", "id": 1592, "sport": "football", "name": "Jarrod Bowen",
  "first_name": "Jarrod", "last_name": "Bowen", "image": "https://.../1592.png",
  "nationality": "England", "date_of_birth": "1996-12-20", "height": "176", "weight": "70",
  "position": "Midfielder", "tier": "headliner",
  "team": {"id": 1, "name": "West Ham United", "short_code": "WHU", "image": "https://.../1.png"}
}
```

Team response:
```json
{
  "entity_type": "team", "id": 18, "sport": "football", "name": "Chelsea",
  "image": "https://.../18.png", "short_code": "CHE", "country": "England", "city": "London",
  "venue": "Stamford Bridge", "conference": null, "division": null, "tier": "headliner"
}
```

- `team` (players) is the **current** club from `player_current_team`; `null` if unknown.
- `position` is the latest season's; `conference`/`division` populate for NBA/NFL teams.

### ~~`GET /api/v1/{sport}/vibe/{entityType}/{id}` · `/history` · `/vibe/hottest`~~ (retired 2026-06-15)

The per-entity vibe endpoints were retired and superseded:
- per-entity vibe → folded into the **`GET /api/v1/{sport}/{entityType}/{id}/sigil`** crown synthesis (the per-entity `/vibes` alias was dropped).
- `/vibe/hottest` → **`GET /api/v1/{sport}/leaderboard/vibes`** (the enriched sport-wide hottest-by-sentiment board).

The vibe *data* (vibe_scores) and its writers (CLI / listener / cron) are unchanged — only these read routes moved.

## Operational Endpoints

- `GET /` - Root endpoint (API info)
- `GET /health` - General health check
- `GET /health/db` - Database connectivity check
- `GET /health/cache` - Cache health check
- `GET /docs/` - Swagger UI documentation
- `GET /docs/go.json` - OpenAPI/Swagger JSON spec

### Meta Response Example

```json
{
  "page": "meta",
  "sport": "nba",
  "scope": {
    "league_id": null
  },
  "meta_version": "1743772800",
  "generated_at": "2026-04-04T16:00:00Z",
  "current_season": 2025,
  "total_entities": 524,
  "items": [
    {
      "id": 666609,
      "type": "player",
      "name": "Rui Hachimura",
      "first_name": "Rui",
      "last_name": "Hachimura",
      "position": "F",
      "nationality": "Japan",
      "date_of_birth": "1998-02-08",
      "height": "6-8",
      "weight": "230",
      "photo_url": "https://...",
      "team_id": 14,
      "team_abbr": "LAL",
      "team_name": "Lakers",
      "search_tokens": ["rui", "hachimura", "ruihachimura", "lal", "lakers"],
      "meta": {
        "display_name": "Rui Hachimura",
        "jersey_number": "28",
        "draft_year": 2019,
        "draft_pick": 9,
        "years_pro": 6,
        "college": "Gonzaga"
      }
    }
  ],
  "stat_definitions": [
    {
      "id": 1,
      "key_name": "pts",
      "display_name": "Points Per Game",
      "entity_type": "player",
      "category": "scoring",
      "is_inverse": false,
      "is_derived": false,
      "is_percentile_eligible": true,
      "sort_order": 3
    }
  ],
  "leagues": []
}
```

**Frontend Caching Strategy:**

Store `meta_version` locally and send it on subsequent requests:

```javascript
const response = await fetch('/api/v1/nba/meta', {
  headers: {
    'If-None-Match': localStorage.getItem('nba_meta_version')
  }
});

if (response.status === 304) {
  // Use cached data
} else {
  const data = await response.json();
  localStorage.setItem('nba_meta_version', data.meta_version);
  // Store data.items, data.stat_definitions for local search
}
```

## Response & Cache Conventions

- JSON responses include ETags where applicable
- Send `If-None-Match` header to receive `304 Not Modified`
- `X-Cache` header indicates `HIT` or `MISS` for cache-backed endpoints
- `X-Process-Time` header shows request processing time

Cache TTL:
- Default data endpoints: 5 minutes
- News: 10 minutes (in addition to permanent write-through to `news_articles`)

## Data Retention Summary

Different consumers have different persistence rules:

| Source | Storage | Retention |
|---|---|---|
| `news_articles` (Google RSS) | Permanent, for training + RAG corpus | No TTL — grows indefinitely |
| `news_article_entities` | Same | Cascade on article delete |
| `vibe_scores` | Permanent, with `model_version` + `prompt_version` | No TTL |

## Entity Tiering

`players.tier` and `teams.tier` enum values drive vibe-generation scheduling:

| Tier | Description | Real-time vibe? | Daily batch vibe? |
|---|---|---|---|
| `headliner` | Top 150 starters per sport + all teams | ✅ on milestone | ✅ covered |
| `starter` | Regular contributors below top-150 | ❌ | ✅ (if played in last 24h) |
| `bench` | Played at some point but below starter bar | ❌ | ❌ |
| `inactive` | No box scores this season | ❌ | ❌ |

Recompute weekly via `SELECT * FROM recompute_entity_tiers('NBA', 2025);` (and equivalents for NFL / FOOTBALL). Real-time path also requires `new_percentile >= 90` + 30-min per-entity debounce.

## Football League IDs

When using league-scoped endpoints for football:

| League | ID |
|--------|-----|
| Premier League (England) | 8 |
| Bundesliga (Germany) | 82 |
| Ligue 1 (France) | 301 |
| Serie A (Italy) | 384 |
| La Liga (Spain) | 564 |

## Error Shape

```json
{
  "error": {
    "code": "INVALID_QUERY_PARAM",
    "message": "season must be an integer",
    "detail": "optional"
  }
}
```

## Auth (mobile) — device-identity JWT

Native apps can't share the web's `.scoracle.com` cookie, so they authenticate
with a bearer token. The user is **anonymous** (a UUID — no email/password);
identity persists via the refresh token in the device Keychain. Full design:
`~/scoracleWiki/wiki/Architecture/Mobile Auth.md`.

| Method · path | Auth | Body | Returns |
|---|---|---|---|
| `POST /api/v1/auth/device` | public | `{}` | `{ access_token, refresh_token, user_id, expires_in }` — new anonymous user. First launch. |
| `POST /api/v1/auth/refresh` | public | `{ refresh_token }` | `{ access_token, refresh_token, expires_in }` — rotates the refresh token; `401` if expired/revoked. |
| `POST /api/v1/auth/device/push` | bearer | `{ token, platform }` | `204` — upsert APNs/FCM token for the user. |
| `POST /api/v1/auth/logout` | bearer | `{ refresh_token }` | `204` — revoke the refresh token. |

- Access token: HS256 JWT (`sub`=user_id, `iss`=scoracle), ~30 min.
- Refresh token: opaque, SHA-256-hashed server-side (`auth_refresh_tokens`), ~90 days, rotated on every refresh.
- **Requires `JWT_SECRET`** (`.env.local`). If unset, `/auth/*` returns `503 AUTH_UNCONFIGURED`; the rest of the API is unaffected.

## Backend Implementation Map

- Router: `go/internal/api/server.go`
- Data handlers: `go/internal/api/handler/data.go`
- Auth handlers (mobile JWT): `go/internal/api/handler/auth.go`
- Auth token issue/verify: `go/internal/auth/auth.go`; bearer middleware: `go/internal/api/middleware.go` (`RequireAuth`)
- News corpus methods (background pipeline only — no serving route): `go/internal/thirdparty/news.go`
- Durable queue contract + operator CLI: `go/internal/work/work.go`, `go/cmd/work`
- LISTEN/NOTIFY listener (enqueues durable sigil work on percentile events): `go/internal/listener/listener.go`
- Maintenance tickers (news-scrub auto-vet + enqueue, pipeline-stats snapshot): `go/internal/maintenance/maintenance.go`
- Cron job wrappers + shared run-recording: `go/cmd/{pipeline,vibesynth}`, `go/internal/jobrun/jobrun.go`
- Rust cognition handlers (all model inference stages + rating batch): `rust/src/{scrub,transfer,narratives,vibe,sigil,rating}.rs`, `rust/src/main.rs`, `rust/src/bin/statcommentary.rs`
- Prepared statements: `go/internal/db/db.go`
- Cache/ETag implementation: `go/internal/cache/cache.go`
- Swagger docs (generated from handler annotations): `go/docs/`

---

**Note:** Legacy endpoints (`/players/`, `/teams/`, `/standings/`, `/leaders/`, `/search/`, `/autofill/`, `/similarity/`) were removed long ago, and the bundled `/api/v1/{sport}/{entityType}/{id}` profile route was removed (O16). Use the per-product endpoints in the route inventory at the top of this file. Comparison-style features should live on the frontend using data from the per-product + meta endpoints.
