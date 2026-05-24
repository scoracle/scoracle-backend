# Scoracle API Endpoints

> Last updated: 2026-05-24 (entity_season_vibe_series — daily-bucketed sentiment trajectory across the season, mirrors entity_event_scores' date range)

Single public API base URL:

- Production: `https://api.scoracle.com` (Cloudflare Tunnel → self-hosted Go API)
- Local: `http://localhost:8000`

## Core Data Endpoints (Canonical)

All canonical endpoints are sport-scoped under `/api/v1/{sport}`.

Supported sport path values:
- `nba`
- `nfl`
- `football`

Supported entity type values:
- `player`
- `team`

### `GET /api/v1/{sport}/{entityType}/{id}`

Returns the canonical profile payload for a sport entity (player or team).

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
| `season` | integer (optional) | Filter to a specific season year (e.g. `2024`). When omitted, the most recent season the entity has data for is returned. |
| `league_id` | integer (optional) | Filter by league. For football, scopes the response (and `available_seasons`) to that league. NBA/NFL are single-league and ignore it. |

#### Response shape

Top-level keys:

| Field | Type | Description |
|---|---|---|
| `page` | string | Literal `"profile"` |
| `sport` | string | Echo of the path param |
| `entity_type` | string | Echo (`"player"` or `"team"`) |
| `entity` | object | Entity profile: identifiers, names, position, team/league context, and a `stats` JSONB with all aggregated season stats keyed by canonical stat name. |
| `percentiles` | `{stat: number}` | Sport-wide percentiles, partitioned by position (player) or none (team). Keys mirror `entity.stats`. |
| `percentile_metadata` | object | `{position_group, sample_size}` for players; `{sample_size}` for teams. |
| `scoped_percentiles` | `{stat: number}` | Narrower-cohort percentiles — partitioned by `(position, conference)` for NBA/NFL, `(position, league)` for Football. Same shape as `percentiles`. |
| `scoped_percentile_metadata` | object | Adds `scope_type` (`"conference"` / `"league"`), `scope_id`, `scope_name` on top of the base metadata. |
| `stat_definitions` | object[] | Sport's full stat catalog (key_name, display_name, category, is_inverse, is_derived, is_percentile_eligible, sort_order, unit, comparable). `unit` is one of `rate_pct` / `per_game_avg` / `cumulative_total` / `special`; `comparable` is the flag the trends endpoint filters on. |
| `meta` | object | Resolved scope + season selector data — see below. |
| `league_context` | object \| null | Football only: `{id, name, country, logo_url, is_benchmark, is_active}` of the league in scope. `null` for NBA/NFL or unscoped football. |

`meta` object:

| Field | Type | Description |
|---|---|---|
| `meta.season` | integer | The season this response is for. Echoes `?season=` if provided, else the entity's most recent season. The frontend should treat this as the "selected" value of its season picker. |
| `meta.league_id` | integer \| null | League actually used (after football's natural-league fallback). `null` for NBA/NFL and for unscoped football responses. |
| `meta.available_seasons` | int[] | **All seasons this entity has data for**, within the current league scope, sorted newest first. See dedicated section below. |
| `meta.season_composite_score` | number \| null | Headline single-number rating for the entity in the resolved season, in `[0, 100]`. AVG of per-event composite scores (migrations 017 + 018). Same scale as `entity_event_scores[].composite_score` and `entity_season_score_avg` on the trends endpoint — per-event distribution is uniform with mean 50, so a player whose events consistently rank in the top quartile will land in the 75+ range here. Top NBA centers ship around 90; bench players land in the 20s; bottom-of-rotation in the 10s. `null` if the entity has no scored events in the season. |

#### Season selection (`meta.available_seasons`)

The profile payload tells the frontend exactly which seasons it can ask for, so a season selector renders with **no dead options**.

**Semantics:**

- It's the list of distinct `season` values in `player_stats` (for `entity_type=player`) or `team_stats` (for `entity_type=team`) where the row matches the requested `sport` and `entity_id`.
- When the request is **league-scoped** (`/leagues/{leagueId}/...` or `?league_id=N`), the list is filtered to seasons the entity appeared in **that league**. A footballer who played in the Bundesliga in 2023 and the Premier League in 2024 returns `[2024]` for `?league_id=8` (Premier League) and `[2023]` for `?league_id=82` (Bundesliga).
- When the request is **unscoped**, the list spans every league. For NBA/NFL (single league) this is just every season the entity has stats for.
- Sorted **newest first**. Always an array — never `null`. Empty `[]` when the entity exists in `players`/`teams` but has no `*_stats` rows in scope (rare; profile would typically 404 first).
- Each integer in the list is a valid value for the `?season=` query parameter — passing it back is guaranteed to return populated stats for this entity, within the same league scope.

**Frontend usage pattern:**

```ts
// On profile load
const res = await fetch(`/api/v1/${sport}/${entityType}/${id}?season=${selected ?? ''}`);
const { meta, entity, stats, percentiles } = await res.json();

// Render dropdown from meta.available_seasons; mark meta.season as current.
// On change:
const next = await fetch(`/api/v1/${sport}/${entityType}/${id}?season=${newSeason}`);
// → returns the same shape, with meta.season === newSeason.
```

**Cross-entity navigation:** if the user picked 2023 on entity A and navigates to entity B which doesn't have 2023 (`available_seasons` lacks it), fall back to entity B's newest. Keep the user-preferred season in URL/state and reconcile per-page.

### `GET /api/v1/{sport}/{entityType}/{id}/trends`

Combines **stats trend + narrative trend** in one read-only payload so a profile page can render "is this entity hot right now" without juggling multiple endpoint calls.

What you get, per request:
- **Stats trend** — the entity's average per stat over its **last 3 fixtures** alongside the **peer cohort's season averages**. The asymmetry is intentional: the entity carries the recent signal, the cohort carries the stable baseline.
- **Narrative trend** — the entity's **last 7 days** of Gemma sentiment scores (1–100) from `vibe_scores`, newest first.

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

The league-scoped route `/api/v1/{sport}/leagues/{leagueId}/{entityType}/{id}/trends` is an alias that takes `leagueId` from the path instead of the query string — identical payload.

#### Response fields

| Field | Type | Description |
|---|---|---|
| `page` | string | Literal `"trends"` |
| `sport`, `entity_type`, `entity_id` | echo of inputs | |
| `window.games_used` | integer | How many events fed `entity_recent_avgs` (0–3) |
| `window.fixture_ids` | int[] | Fixture IDs of those events, newest first |
| `window.spans_prior_season` | bool | `true` when the 3-fixture window bridged into the prior season because the current season had fewer than 3 |
| `entity_recent_avgs` | `{stat: number}` | Per-stat average over those events. Keys come from `event_box_scores.stats` / `event_team_stats.stats`, filtered to `stat_definitions.comparable = true` so units match the season-rolled sides. `{}` if no events. |
| `entity_season_avgs` | `{stat: number}` | The entity's **own** season-rolled per-stat values, sourced from `player_stats.stats` / `team_stats.stats`. Same comparability filter and same per-game normalization as `peer_season_avgs`, applied to the single entity row instead of a cohort. Frontend uses this alongside `peer_season_avgs` to render a self-delta next to the peer-delta — important for dominant entities where every peer comparison reads as a huge positive and the user can't otherwise tell which way the entity is actually trending relative to its own baseline. `{}` for players (player cumulatives are non-comparable; their per-game / per-90 siblings already appear on the peer side and self-comparison adds no signal there). `{}` if the entity has no `*_stats` row in scope. |
| `peer_season_avgs` | `{stat: number}` | Per-stat average across the peer cohort, filtered to `stat_definitions.comparable = true`. For `unit = 'cumulative_total'` keys (e.g. football team `tackles`, NFL team `passing_yards`), the raw season values are normalized to per-game by dividing each peer's value by their `matches_played` (football) / `games_played` (NBA/NFL) before averaging, so both sides land in the same per-game unit. Compare by intersecting keys with `entity_recent_avgs`. |
| `peer_cohort_size` | integer | Peers contributing to `peer_season_avgs`. Use this to hide the comparison when the cohort is too thin to be meaningful (e.g., `< 5`). |
| `entity_event_scores` | object[] | **Every** played event in the current season, newest first (renamed from `entity_recent_scores` which only carried the last 3). Each entry: `{fixture_id, composite_score, minutes_played, start_time}`. `composite_score` is in `[0, 100]` — see the Interpretation section below. NULL composite for events with no eligible non-zero stats (e.g. a DNP-CD). `minutes_played` is provided for hover-tooltip context; `null` for team entities (teams play the full match). `start_time` is the fixture's UTC ISO-8601 timestamp — lets the frontend label hover-tooltips and bucket by week/month without a second fetch. Counts: ~82 for NBA, ~17 for NFL, ~38 for football (single league). |
| `entity_season_score_avg` | number \| null | The entity's own `season_composite_score` — AVG of their event composite scores across the resolved season. `null` if the entity has no scored events. |
| `peer_season_score_avg` | number | AVG of `season_composite_score` across the peer cohort. By construction this hovers near 50 (it's an average of percentile averages within a position cohort); useful as a tier-rendering anchor. |
| `vibes.window_days` | integer | Currently fixed at `7`. May become a query param when data.scoracle ships. |
| `vibes.snapshots` | object[] | Last-7-days raw sentiment snapshots — `{sentiment, generated_at, trigger_type}` rows ordered newest first. `[]` when the entity has no scores in the window. Still the freshest single number for "right now" headlines; `entity_season_vibe_series` is the season trajectory companion. |
| `entity_season_vibe_series` | object[] | Daily-bucketed sentiment trajectory across the season, oldest-first. Each row: `{date, sentiment_avg, snapshot_count}` — `date` is the UTC day (`YYYY-MM-DD`); `sentiment_avg` is the integer mean of that day's snapshots on the 0–100 scale (matches the rating scale); `snapshot_count` is how many snapshots aggregated into that day (frontend hover-tooltip: "4 snapshots that day"). **Days with zero snapshots are omitted** so the sparkline renders quiet stretches as honest gaps rather than zero-sentiment dots. Range: anchored at the entity's oldest scored event in the season (`MIN(start_time)` over `entity_event_scores`) through `NOW()` — vibes after the most recent game still matter (off-day news, trade rumors, post-game reactions). `[]` when the entity has no scored events in scope; frontend should hide the season vibe sparkline in that case. |
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

**Interpretation:** the score is the **percentile rank of the event's raw composite within its position cohort** (migration 018 — replaces the unweighted-mean exposed by 017). Concretely, two passes:
1. Compute the unweighted mean of per-stat percentiles for stats the player had non-zero values in (the raw composite).
2. Percent-rank that raw composite against every other same-season same-position event.

The stored value is the result of pass 2. **Mean per partition is 50 by construction; distribution is uniform in `[0, 100]`.** A 70 reads as "this event ranks in the top 30% of events at this position this season." 80+ is a top-decile game. Below 30 is a bottom-third outing. The two-pass design has the side effect of bounding sparse-stat outliers (a goalkeeper event whose raw composite was 100 because only one stat ranked simply takes its place among other sparse-stat events rather than pinning to the top).

**Sample-size disclaimers:** `minutes_played` is surfaced alongside each event score so the frontend can flag short appearances (e.g. football subs under ~30 min where per-90 normalization can produce extreme scores from a single stat). The decision to NOT filter low-minute appearances was explicit — per-90 normalization is partly meant to illuminate underused players, and dropping those events server-side would defeat that. At full-season sparkline density (82 dots for NBA) the per-dot disclaimers naturally move to hover-tooltips rather than inline labels.

**Specialists vs well-rounded:** because the formula is an unweighted mean, a specialist who's elite at one stat but average elsewhere scores around 55, while a well-rounded entity scores higher. The per-stat percentiles in the existing trends payload (`entity_recent_avgs` etc.) remain the breakdown view; the composite score is the summary view.

**Form-streak rendering:** the `entity_event_scores` array is sufficient input for any client-side "N games above 70" display or full-season sparkline — no additional endpoint needed. The frontend's natural reading direction (oldest→newest, left→right) is the reverse of the server's newest-first ordering; reverse on the client.

Known limitation — **NFL/football player trends** have a small intersection between sides because the seeder writes raw per-event counts (`passing_yards`, `tackles`) to `event_box_scores` but writes derived per-game / per-90 keys (`passing_yards_per_game`, `tackles_per_90`) to `player_stats`. The trends card on player pages for these sports shows fewer rows until the seeder schema is unified.

#### Response example (player)

```json
{
  "page": "trends",
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
  "page": "trends",
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

Returns complete metadata payload for frontend local DB hydration. Designed for caching on the frontend to enable instant autocomplete without repeated API calls.

Query parameters:
- `league_id` (optional integer) - Scope to specific league

Response includes:
- `meta_version` - Unix timestamp of last data update (for cache invalidation)
- `current_season` - The sport's current active season year
- `total_entities` - Count of players and teams in the response
- `items` - All entities (players + teams) with search tokens and metadata
- `stat_definitions` - All stat keys with display names and categories
- `leagues` - League information (populated for multi-league sports like football)

### `GET /api/v1/{sport}/health`

Returns sport-level data freshness and counts.

Query parameters:
- `league_id` (optional integer) - Scope to specific league

Response includes:
- Last update timestamp
- Fixture counts
- Box score coverage stats
- Data freshness indicators

## League-Scoped Endpoints

League-scoped routes are required for football (which has multiple leagues) and preferred when league context is explicit.

### `GET /api/v1/{sport}/leagues/{leagueId}/{entityType}/{id}`

Returns profile payload scoped to a specific league. Body shape is identical to the canonical route — `meta.league_id` reflects the path param, and `meta.available_seasons` is filtered to seasons the entity appeared in **this league** (so a multi-league football player gets a different list per league-scoped URL).

Path parameters:
- `leagueId` - League identifier (e.g., 8 for Premier League)
- `entityType` - `player` or `team`
- `id` - Entity ID

Query parameters:
- `season` (optional integer)

### `GET /api/v1/{sport}/leagues/{leagueId}/{entityType}/{id}/trends`

League-scoped alias for the trends endpoint above. `leagueId` is taken from the URL path instead of the `league_id` query parameter; **the response body is identical** to the canonical route. Useful when the calling page already has league context (e.g., a Premier League team profile) and you want to keep that context in the URL.

Example: `GET /api/v1/football/leagues/8/team/18/trends` is equivalent to `GET /api/v1/football/team/18/trends?league_id=8`.

### `GET /api/v1/{sport}/leagues/{leagueId}/meta`

Returns metadata payload scoped to a specific league.

### `GET /api/v1/{sport}/leagues/{leagueId}/health`

Returns health/freshness payload scoped to a specific league.

## News Endpoints

### `GET /api/v1/news/status`

News provider configuration status. Reports the Google News RSS source only — NewsAPI was removed; RSS is the sole provider.

### `GET /api/v1/news/{entityType}/{entityID}`

Returns news articles for a player or team, pulled from Google News RSS. Matched articles are also **write-through persisted** to the `news_articles` and `news_article_entities` tables, with cross-entity linking against a cached pool of all teams + players for the sport (confidence 1.0 for the queried entity, 0.8 for co-mentioned entities).

Path parameters:
- `entityType` - `player` or `team`
- `entityID` - Entity ID (integer)

Query parameters:
- `sport` (required) - `NBA`, `NFL`, or `FOOTBALL`
- `team` (optional) - Team name for player context
- `limit` (optional, 1-50) - Number of articles to return

## Twitter / X Endpoints

X is used for a user-facing journalist feed and as a real-time context signal for the vibe generator. Tweet data is subject to a hard 24h TTL (ToS compliance — no long-term storage for ML training).

### `GET /api/v1/twitter/status`

Per-sport Twitter list configuration + cache state + daily cost telemetry.

Response includes, per sport:
- `sport`, `list_id`, `configured`, `ttl_seconds`
- `last_fetched_at`, `since_id`, `last_error`, `last_error_at`
- `calls_today` - X API calls made today (resets 00:00 UTC)
- `tweets_today` - tweets returned from those calls today

Plus service-level fields:
- `bearer_token_configured`, `cache_ttl_seconds`
- `rate_limit` - human-readable limit string
- `architecture: "lazy_cache"`

### `GET /api/v1/{sport}/twitter/feed`

Cached journalist tweets for a sport. Refreshes on demand from X when the cache is older than the list TTL (default 20 min). Concurrent refreshes are coalesced via singleflight; stale cache is served if the upstream call fails.

Path parameters:
- `sport` - `nba`, `nfl`, or `football`

Query parameters:
- `limit` (optional, 1-100, default 25)

### `GET /api/v1/{sport}/twitter/{entityType}/{id}`

Cached tweets linked to a specific player or team via the shared `search_aliases` matcher.

Path parameters:
- `sport` - `nba`, `nfl`, or `football`
- `entityType` - `player` or `team`
- `id` - Entity ID

Query parameters:
- `limit` (optional, 1-100, default 25)

## Vibe (Gemma Blurb) Endpoints

Vibe blurbs are ~140-character narrative summaries produced by a local Ollama-hosted Gemma 4 e4b model, informed by recent news (last 72h) and recent tweets (last 24h) for the entity. Blurbs are NOT generated on request — they land via the vibe CLI (`go/cmd/vibe`) or the milestone listener worker (fires on `percentile_changed` events for `tier=headliner` entities crossing the 90th percentile, with a 30-min per-entity debounce). These endpoints serve what has already been generated.

### `GET /api/v1/{sport}/vibe/{entityType}/{id}`

Returns the most recent vibe blurb for the entity. Returns **404** (not a 200 with empty data) when no blurb has been generated yet — frontends should handle that as "no blurb to show" rather than as an error state.

Path parameters:
- `sport` - `nba`, `nfl`, or `football`
- `entityType` - `player` or `team`
- `id` - Entity ID (integer)

Response example:
```json
{
  "id": 6,
  "entity_type": "player",
  "entity_id": 115,
  "sport": "NBA",
  "trigger_type": "manual",
  "trigger_payload": {"stat_key": "pts", "new_percentile": 97.2},
  "blurb": "From play-in losses and knee drama, Curry keeps the vibes going with jokes and the anticipation of a massive comeback.",
  "model_version": "gemma4:e4b",
  "prompt_version": "v1",
  "generated_at": "2026-04-19T11:51:45.006058-04:00"
}
```

`trigger_type` values: `milestone` (listener-driven), `manual` (CLI ad-hoc), `periodic` (nightly batch).

`trigger_payload` is JSONB — populated for milestone triggers (stat_key + percentile movement), often `null` for manual / periodic runs.

### `GET /api/v1/{sport}/vibe/{entityType}/{id}/history`

Returns the N most recent vibe blurbs for the entity, newest first.

Path parameters: same as above.

Query parameters:
- `limit` (optional, 1-50, default 10)

Response:
```json
{
  "entity_type": "player",
  "entity_id": 115,
  "sport": "NBA",
  "count": 3,
  "vibes": [ /* array of full vibe objects as above */ ]
}
```

## Operational Endpoints

- `GET /` - Root endpoint (API info)
- `GET /health` - General health check
- `GET /health/db` - Database connectivity check
- `GET /health/cache` - Cache health check
- `GET /docs/` - Swagger UI documentation
- `GET /docs/go.json` - OpenAPI/Swagger JSON spec

## Response Structure

### Profile Response Example (Player)

```json
{
  "page": "profile",
  "sport": "nba",
  "entity_type": "player",
  "entity": {
    "id": 666609,
    "name": "Rui Hachimura",
    "first_name": "Rui",
    "last_name": "Hachimura",
    "position": "F",
    "nationality": "Japan",
    "height": "6-8",
    "weight": "230",
    "team": {
      "id": 14,
      "name": "Lakers",
      "abbreviation": "LAL",
      "city": "Los Angeles",
      "conference": "West",
      "division": "Pacific"
    },
    "season": 2025
  },
  "stats": {
    "pts": 18.0,
    "reb": 5.0,
    "ast": 1.0,
    "games_played": 1
  },
  "percentiles": {
    "pts": 100.0,
    "reb": 50.0,
    "ast": 0.0
  },
  "percentile_metadata": {
    "position_group": "F",
    "sample_size": 13
  },
  "scoped_percentiles": {
    "pts": 92.3,
    "reb": 61.5,
    "ast": 7.7
  },
  "scoped_percentile_metadata": {
    "scope_type": "conference",
    "scope_id": "West",
    "scope_name": "West",
    "position_group": "F",
    "sample_size": 13
  },
  "meta": {
    "season": 2025,
    "league_id": null,
    "available_seasons": [2025, 2024, 2023]
  }
}
```

`percentiles` is sport-wide (partitioned by player position). `scoped_percentiles` is partitioned by **(position, conference)** for NBA/NFL and **(position, league)** for Football — same percentile semantics, narrower comparison set. Both are emitted; clients can show either or both.

### Profile Response Example (Team)

```json
{
  "page": "profile",
  "sport": "nba",
  "entity_type": "team",
  "entity": {
    "id": 18,
    "name": "Timberwolves",
    "short_code": "MIN",
    "city": "Minnesota",
    "conference": "West",
    "division": "Northwest",
    "season": 2025
  },
  "stats": {
    "wins": 0,
    "losses": 1,
    "pts": 103.0,
    "games_played": 1
  },
  "percentiles": {
    "wins": 0.0,
    "pts": 0.0
  },
  "percentile_metadata": {
    "sample_size": 30
  },
  "scoped_percentiles": {
    "wins": 0.0,
    "pts": 6.7
  },
  "scoped_percentile_metadata": {
    "scope_type": "conference",
    "scope_id": "West",
    "scope_name": "West",
    "sample_size": 15
  },
  "stat_definitions": [...],
  "meta": {
    "season": 2025,
    "league_id": null,
    "available_seasons": [2025, 2024, 2023]
  }
}
```

For teams, `scoped_percentiles` is partitioned by **conference** (NBA/NFL) or **league** (Football) — no position dimension since teams don't have positions.

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
- Twitter cache: per-sport, default 20 minutes (configurable via `TWITTER_CACHE_TTL_SECONDS`)
- Tweet retention: hard 24h TTL enforced by the maintenance ticker (ToS)

## Data Retention Summary

Different consumers have different persistence rules:

| Source | Storage | Retention |
|---|---|---|
| `news_articles` (Google RSS) | Permanent, for training + RAG corpus | No TTL — grows indefinitely |
| `news_article_entities` | Same | Cascade on article delete |
| `tweets` + `tweet_entities` | Inference cache only | 24h hard TTL |
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

## Backend Implementation Map

- Router: `go/internal/api/server.go`
- Data handlers: `go/internal/api/handler/data.go`
- News handler: `go/internal/api/handler/news.go`
- Twitter handler: `go/internal/api/handler/twitter.go`
- Vibe handlers: `go/internal/api/handler/vibe.go`
- News write-through + entity pool: `go/internal/thirdparty/news.go`
- Twitter service + telemetry: `go/internal/thirdparty/twitter.go`
- Ollama client + vibe generator: `go/internal/ml/ollama.go`, `go/internal/ml/vibe.go`
- Listener (vibe dispatch): `go/internal/listener/listener.go`, `go/internal/listener/vibe_worker.go`
- Maintenance tickers (tweet TTL purge, etc.): `go/internal/maintenance/maintenance.go`
- Prepared statements: `go/internal/db/db.go`
- Cache/ETag implementation: `go/internal/cache/cache.go`
- Swagger docs: `go/docs/swagger.json`, `go/docs/swagger.yaml`

---

**Note:** Legacy endpoints (`/players/`, `/teams/`, `/standings/`, `/leaders/`, `/search/`, `/autofill/`, `/similarity/`) have been removed. Use the canonical `/api/v1/{sport}/{entityType}/{id}` routes. Comparison-style features should live on the frontend using data from the profile + meta endpoints.
