# Trends endpoint — last-3 event averages vs peer-cohort season averages

Date: 2026-05-22

## Goal

Surface a lightweight "how is this entity trending recently relative to peers"
signal on the public API, with two constraints:

1. **Raw values only.** No "trending up", no narrative — frontend reads the
   numbers and decides what direction means visually.
2. **Stay Postgres-as-serializer.** No derived state stored anywhere; aggregate
   on read; cheap enough that cache TTL absorbs the cost.

## Decisions

- **On-read only.** No new tables, no snapshot pipeline, no `finalize_fixture`
  changes. The earlier iteration of this plan built a `player_trend_snapshots`
  history table with a fixture-finalize hook and per-peer rolling-window
  helpers — overbuilt for the signal we actually need. The signal compresses
  to two JSONB objects per request, which one prepared statement returns from
  existing tables (`event_box_scores`, `event_team_stats`, `fixtures`,
  `player_stats`, `team_stats`).
- **Window = last 3 fixtures the entity actually played.** Ordered by
  `fixtures.start_time DESC` with `LIMIT 3`. When the current season has
  fewer than 3, the window naturally bridges into the prior season; the
  response carries a `spans_prior_season` flag so the frontend can choose
  whether to render the comparison.
- **Comparison baseline = peer cohort's season averages, not their rolling
  window.** Using peers' season averages keeps the cohort scan O(cohort_size)
  rows of `player_stats` JSONB — fast, cache-friendly. A peer rolling-window
  baseline would have required `O(cohort × 3 event reads)` per request. The
  user-confirmed framing was "target entity's last 3 event stats compared to
  the peer averages for the season."
- **Peer cohort mirrors the existing percentile partitioning.** Players:
  same sport + same position (+ same league for football). Teams: same sport
  (+ same league for football). Football resolves the entity's natural
  league_id when no `league_id` query param is provided so the cohort doesn't
  span multiple competitions.
- **No peer-percentile-delta comparison.** Percentiles are already rank-relative
  within the position cohort, so the average peer percentile delta is
  mathematically ~0 across the cohort — no useful signal. Raw last-3 averages
  preserve the direction information we actually want.
- **Three prepared statements (`nba_trends_page`, `nfl_trends_page`,
  `football_trends_page`)** built from one Go helper (`trendsStatement`) that
  toggles the sport literal and a `leagueScoped` flag for football's league
  fallback. Mirrors the existing `*_profile_page` per-sport split for
  consistency, while keeping the SQL in one place.
- **Empty / new-entity case returns 200 with `games_used: 0`**, not 404.
  Entity existence is the profile endpoint's job; trends just returns what
  it can compute.

## Accomplishments

- New `GET /api/v1/{sport}/{entityType}/{id}/trends` and league-scoped
  variant `GET /api/v1/{sport}/leagues/{leagueId}/{entityType}/{id}/trends`
  on the Go API.
- `trendsStatement(sportTag, sportID, leagueScoped)` helper in
  `go/internal/db/db.go` that produces a 130-line CTE chain per sport.
- `GetTrendsPage` and `GetLeagueTrendsPage` handlers in
  `go/internal/api/handler/data.go`, both thin pass-throughs around
  `serveStatementJSON` with `notFoundOnNoRows=false`.
- Routes wired under the existing `/{sport:nba|nfl|football}` block in
  `go/internal/api/server.go`.
- `TestRouteOwnershipSplit` extended with three new cases covering the
  player, team, and league trends routes.
- `ENDPOINTS.md` and `README.md` updated; wiki updated at
  `~/scoracleWiki/wiki/Architecture/API Contracts.md` and `Changelog.md`.

## Forward path — data.scoracle

Because the endpoint is pure read-only SQL with no derived state stored
anywhere, the CTE chain lifts directly onto **data.scoracle**, the planned
PostgREST-powered surface of the platform. The trends statement can be
recast as a SQL function — `get_entity_trends(sport, entity_type, entity_id,
season, league_id, window_size)` — and exposed via PostgREST RPC so the
data.scoracle frontend can call it directly with user-selected scope
(window size, cohort filters) and skip the Go layer entirely. Worth keeping
in mind any time we touch the trends SQL: prefer named CTE boundaries that
survive the lift.

## Quick reference

| Item | Path |
|---|---|
| Prepared statement registration | `go/internal/db/db.go` (search `nba_trends_page`) |
| `trendsStatement` helper | `go/internal/db/db.go` (after `registerPreparedStatements`) |
| Handlers | `go/internal/api/handler/data.go` (`GetTrendsPage`, `GetLeagueTrendsPage`) |
| Route wiring | `go/internal/api/server.go` (`/{entityType:player|team}/{id}/trends`) |
| Tests | `go/internal/api/server_test.go` (`TestRouteOwnershipSplit`) |
| Public docs | `ENDPOINTS.md` (Trends section), `README.md` (API Surface) |
| Wiki | `~/scoracleWiki/wiki/Architecture/API Contracts.md`, `~/scoracleWiki/wiki/Changelog.md` |

## Verification status

- `gofmt -w .`, `go vet ./...`, `go build ./...`, `go test ./...` all clean.
- Route registration verified via `TestRouteOwnershipSplit` (player, team,
  league trends routes all 503 with nil pool — registered correctly).
- **End-to-end smoke test against a live database is pending** — no local
  Postgres / `.env.local` credentials available in this session. Recommended
  next steps when DB is reachable:
  1. `curl http://localhost:8000/api/v1/nba/player/<id>/trends | jq` for a
     player with ≥3 seeded fixtures — verify `games_used=3`,
     `entity_recent_avgs` and `peer_season_avgs` are populated, `meta.position`
     resolves.
  2. Manual cross-check: `SELECT e.fixture_id, e.stats FROM event_box_scores e
     JOIN fixtures f ON f.id=e.fixture_id WHERE e.player_id=<id> AND
     e.sport='NBA' ORDER BY f.start_time DESC LIMIT 3;` — confirm averaging
     matches the endpoint's `entity_recent_avgs`.
  3. Repeat for `entity_type=team` and for football's league-scoped variant.
  4. Confirm empty-entity case returns 200 with `games_used:0`, not 404.
