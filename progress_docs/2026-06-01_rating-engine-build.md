# Build — Scoracle Rating Engine (z-score, players + teams)

Date: 2026-06-01
Status: **SHIPPED.** Migrations 027 + 028 applied to prod; API rebuilt + restarted;
both new endpoints serving live and verified. The keystone design
(`planning_docs/SCORACLE_RATING_ENGINE.md`) is now implemented end to end.

## Goal

Replace the audited scoring-volume "composite" with the principled, bias-free
**z-score rating engine** — positionless Composite (Σz breadth) + Specialist
(peak z + skill label) from public-domain box scores only — and serve it cleanly
for the leaderboard, starline, and meta card. Build phase of the locked spec (§9).

## Decisions (this session)

- **Engine = `rating_datapoints(sport, stats)` as the single source of truth.** Each
  de-duped datapoint is a SQL expression over a box-score `stats` blob, applied at
  **both grains**: season (`player_stats`/`team_stats` → Composite/Specialist) and
  per-event (`event_box_scores`/`event_team_stats` → the dual sparkline). This is the
  bottom-up/event-as-base payoff — one definition, two grains. The data forced it:
  NFL `total_yards`/`total_touchdowns` are multi-key sums (no season key exists), and
  `(stats->>key)::numeric` is NULL when a key is absent, so `AVG/STDDEV_POP` measure
  role-exclusive stats (GK `saves`) among participants automatically while dense stats
  (explicit zeros) stay population-wide. One mechanism, both behaviors.
- **Bottom-up clarification:** the season aggregate is already the faithful event→season
  rollup that solves the provider omit-zeros sparsity; reading it is the correct
  projection, not "top-down." The genuinely new event-as-base win is the starline.
- **Player engine left byte-identical to the validated boards.** NFL players keep
  category-balancing (offense/defense/special facets); NBA/football flat-z. Reproduced
  every user-confirmed board read-only before committing (NBA Wemby rim-z 6.11; NFL
  Stafford→Garrett→M.Jones→McCaffrey→C.Williams→Love→Nacua→Maye→Burns→Anderson).
- **Teams: same engine, ALWAYS FLAT** (a team is multi-phase → no facets, even NFL).
  Built as SEPARATE functions (`rating_datapoints_team`, `compute_team_rating`,
  `compute_team_event_starline`) so the player path is never perturbed. Results enter
  via the team plus_minus analog: NBA `point_differential` (COALESCE→`plus_minus` at
  event grain), NFL `point_differential`, football `goal_difference`. Boards validated:
  NBA Thunder/Spurs/Pistons, NFL Rams/Seahawks, football Bayern/Barça/Madrid.
- **Serving: the rating engine is its OWN dataset — NOT in the profile or `/meta`
  payloads** (`/meta` is only the frontend local-DB refresh feed; reverted an initial
  attempt to fold ratings into the profile meta block). Two dedicated endpoints, each
  carrying Composite + Specialist + specialty in one payload.
- **`entity_type` is the player/team differentiator** on the leaderboard (query param);
  the starline takes it from the path. Both prepared statements are sport-parameterized
  and read the shared tables (join caveat: players & teams keyed by `(id, sport)` →
  every join needs `AND .sport=`).
- **Additive rollout:** legacy `season_composite_score` lineage untouched, so the live
  frontend keeps working; it migrates to `rating_*` via the new endpoints, then legacy
  can be retired.

## Accomplishments

- **Migration 027** — `rating_*` columns on `player_stats` + `team_stats` (+ indexes);
  `rating_datapoints` (player z-sets), `compute_rating`, `rating_datapoints_team`,
  `compute_team_rating`; `finalize_fixture` recomputes season ratings; full backfill.
- **Migration 028** — `rating_*` columns on `event_box_scores` + `event_team_stats`;
  `compute_event_starline`, `compute_team_event_starline`; `finalize_fixture` extended;
  full event backfill.
- **Go** — `leaderboard` + `starline` prepared statements (entity-type-aware, player ⊕
  team branches); thin `GetLeaderboard` / `GetStarline` handlers; routes in server.go;
  Swagger annotations. Profile/meta payloads left clean.
- **Docs** — ENDPOINTS.md (new Rating Engine section), README, regenerated OpenAPI spec.
- **Verified live (prod, 2025):** player_stats 20,413 rated · team_stats 1,078 ·
  event_box_scores 812,210 · event_team_stats 46,312. HTTP smoke: player + team
  leaderboards, player + team starlines, specialist + per-skill scopes all correct.

## Quick reference

- Engine SQL: `sql/migrations/027_rating_engine_z.sql`, `028_rating_engine_starline.sql`
- Canonical spec: `planning_docs/SCORACLE_RATING_ENGINE.md`
- Prepared statements: `go/internal/db/db.go` (`leaderboard`, `starline`)
- Handlers: `go/internal/api/handler/data.go` (`GetLeaderboard`, `GetStarline`)
- Routes: `go/internal/api/server.go`
- Endpoints:
  - `GET /api/v1/{sport}/leaderboard?entity_type=player|team&scope=composite|specialist|<skill>&season=&position=&league_id=&limit=`
  - `GET /api/v1/{sport}/{entityType}/{id}/starline?season=&league_id=`
- Lifecycle: ratings recompute in `finalize_fixture` (same per-season cadence as the
  existing percentile recompute). O(M²)-per-fixture is the documented future
  optimization for the whole event-recompute path.

## New columns

`player_stats` / `team_stats`: `rating_composite`, `rating_specialist`,
`rating_specialty`, `rating_composite_rank`, `rating_specialist_rank`.
`event_box_scores` / `event_team_stats`: `rating_composite`, `rating_specialist`,
`rating_specialty`.

## Known follow-ups (not blockers)

- Per-skill specialist boards currently surface entities whose *argmax* specialty is the
  skill (ordered by peak z); a fuller "everyone ranked by skill X's z" board would need
  per-datapoint z persisted.
- Team event-grain margin: NBA uses `plus_minus`; NFL/football lack a clean per-event
  margin key → that datapoint is z=0 at event grain (graceful degradation).
- Lockdown-corner blind spot & data wishlist carried from spec §9 (unchanged).
