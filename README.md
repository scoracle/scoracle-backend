# Scoracle Data

Backend data pipeline and unified API for Scoracle sports data.

## Start Here

Before working in this repo, read these in order:

1. This README
2. [../scoracle-wiki/PRODUCT_NARRATIVE.md](../scoracle-wiki/PRODUCT_NARRATIVE.md)
3. [../scoracle-wiki/DATA_FLOW.md](../scoracle-wiki/DATA_FLOW.md)

The wiki owns product direction and cross-repo data-flow doctrine. This repo owns ingestion, storage, derivation, and serving implementation.

## Shared Organization Docs

Shared process, vocabulary, and history live in `scoracle-wiki`, not this repo:

- [../scoracle-wiki/wiki/CONVENTIONS.md](../scoracle-wiki/wiki/CONVENTIONS.md) - how shared docs, progress, glossary entries, and changelog entries are organized.
- [../scoracle-wiki/wiki/Glossary.md](../scoracle-wiki/wiki/Glossary.md) - cross-repo product and architecture vocabulary.
- [../scoracle-wiki/wiki/Changelog.md](../scoracle-wiki/wiki/Changelog.md) - landmark architecture and product shifts.

Use those docs when adding shared language, recording landmarks, or checking historical context. Keep backend-only implementation detail in this README, [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md), `RUNBOOK.md`, or `ENDPOINTS.md`.

**Planning and progress docs never live in this repo** (convention set
2026-07-11): every plan and work log goes in
`../scoracle-wiki/progress_docs/scoracle-backend/` as
`YYYY-MM-DD_short-description.md` — post a plan there when authored, mark it
executed when it lands. This repo holds only durable docs: this README,
`docs/DEVELOPMENT.md`, `RUNBOOK.md`, and `ENDPOINTS.md`.

## Pillars

Scoracle is lean, nimble, and durable.

Elegance comes through simplicity. Simple and durable beats clever and fragile. The flow of information must be clear and clean.

Our role is to eliminate noise around entities and divine the facts. Backend code should do the same: preserve clean source data, make derivation durable and observable, empower the model layer with clear context, and serve precomputed products through simple contracts.

## North Star: The Reading

The product framing every backend decision serves (canonical version in
[../scoracle-wiki/PRODUCT_NARRATIVE.md](../scoracle-wiki/PRODUCT_NARRATIVE.md)):

A user comes to the oracle for a reading on a sports entity. The oracle's name is Scoracle.
Scoracle reveals several distinct cards — PEAK, narratives, transfers, vibe, momentum — every
one a reading of a distinct aspect, each shaped by its own lens and voice. The final reveal,
the all-encompassing one shaped and informed by the rich readings before it, is the **Sigil**:
the one card that carries the Oracle lens — slightly mystic, without getting too far gone.
When a user wants the overview of an entity without reading through all the layers, this is
where they come. The final divination. The fog of noise peeled back.

In this repo, the cards are the cognition stages (`peak`, `narratives`, `transfers`, `vibe`,
`momentum` — each a `Role` with its own persona lens; see `rust/src/eval_tasks.rs::lens_parameters`),
and the Sigil is the crown synthesis whose user-facing voice is the Oracle lens (the `oracle`
stage, served inside the sigil card as its `oracle` key). The mysticism lives in the telling,
never the facts: every claim in a reading traces to a card, and the dataflow below exists so
that by the time the Sigil is read aloud, the fog is already peeled back.

## Model Hierarchy

The dataflow exists to refine work so each model tier does what it is best at.

**Candle (CPU) instances** handle low-reasoning classification: "is this transfer-related?", "is this about the right entity?", "which articles are about the same topic?", "how relevant is this narrative to this entity?" This is the sieve work — cheap, fast, runs on the CPU, never contends with the generation GPU. Every classification candle makes is one fewer classification the GPU has to make.

**The GPU (local) instance** handles the surfaceable product: the scouting report, the narrative, the sentiment, the sigil synthesis. This is the prose work — the FEELING that makes the product uniquely ours. It is expensive and it is the moat.

**The dataflow's goal is to provide such rich, clean context that by the time it reaches the GPU — especially at sigil — the instance is mostly focused on quality prose and tone, not on figuring out what the data means.** The candle layers filter, bucket, cluster, heat-rank, and weight. The deterministic layers compute heat, percentiles, trajectories, slopes. The GPU receives evidence that has already been refined into signal. Its job is to convey the feeling, not to decode the noise.

This is why the two rails exist: each rail refines independently so the convergence (momentum) and the final synthesis (sigil) receive the richest possible context. Scrub buckets articles so transfers and narratives each see the right corpus. Vibe summarizes both so sigil sees the felt state. PEAK distills the stats so sigil sees the scouting context. Momentum tracks the trajectories so sigil sees where the entity is heading. By the time the GPU writes the sigil blurb, the only question left is: what does this feel like?

The GPU burns on entities where something meaningful moved, not on every daily tick. The candle burns on every article, cheaply. This is the hierarchy: cheap work happens often, expensive work happens when it matters.

## Repo Role

- Type: `backend/data-serving`
- Owns: provider ingestion, PostgreSQL schema and derivation, durable work queues, Rust cognition, Go public API, mobile auth, backend operations, and product endpoint contracts.
- Does not own: client presentation, shared visual doctrine, or product narrative changes without updating `scoracle-wiki`.
- Primary consumers: `scoracle-frontend`, `scoracle-ios`, future Scoracle clients, and backend operators.

## Session Workflow

1. Read this README.
2. Sync safely:

```bash
git fetch
git status --short --branch
```

Pull only when the working tree is clean and the branch has not diverged.

3. Read [../scoracle-wiki/PRODUCT_NARRATIVE.md](../scoracle-wiki/PRODUCT_NARRATIVE.md).
4. Read [../scoracle-wiki/DATA_FLOW.md](../scoracle-wiki/DATA_FLOW.md).
5. Perform the task in the smallest useful chunk.
6. Add a progress doc in `../scoracle-wiki/progress_docs/scoracle-backend/YYYY-MM-DD_short-description.md`.
7. If the change introduces shared vocabulary or a landmark shift, update `../scoracle-wiki/wiki/Glossary.md` or `../scoracle-wiki/wiki/Changelog.md`.
8. Run verification.
9. Commit and push.
10. For unfinished multi-step work, leave a copyable handoff.

## Working Context

Most backend tasks need only:

```text
scoracle-backend/
../scoracle-wiki/
```

Add client repos only for contract-consumer verification. Add `../scoracle-tokens/` only if a task explicitly touches client-facing visual output.

## Architecture

Scoracle runs as a Go API + Rust cognition layer backed by PostgreSQL.

- **Go API (`:8000`)** serves curated sport data pages and health/docs endpoints from precomputed Postgres tables. It runs SQL-only maintenance/notification workers and the ingest funnel wiring, but does not execute model inference.
- **Rust Cognition Harness (`rust/`)** owns all model inference stages (scrub, transfers, narratives, vibe, sigil) via `pipeline_work`, plus the `statcommentary` rating batch.
- **PostgreSQL (`sql/`)** is the source of truth for schema, derived stats, percentiles, views, and API-shaping SQL.

> Operating the backend (release/rollback, backup/restore, jobs, durable work queue + repair commands): see **[`RUNBOOK.md`](RUNBOOK.md)**.

The frontend calls one API origin and receives page-shaped JSON payloads designed for direct rendering.

Canonical cross-repo flow: [../scoracle-wiki/DATA_FLOW.md](../scoracle-wiki/DATA_FLOW.md).

## Service Responsibilities

| Component | Responsibility | Location |
|---|---|---|
| Go API | Public HTTP API, caching, ETags, CORS, rate limiting, mobile auth, SQL-only maintenance + notifications | `go/` |
| Rust Cognition Harness | Queue-stage model inference + rating batch (`statcommentary`) | `rust/` |
| PostgreSQL | Data model, stat normalization, derived metrics, percentile logic, shaping views/functions | `sql/` |

## API Surface

Canonical data routes are sport-scoped (`{sport}` ∈ `nba|nfl|football`). The page is
assembled from **per-product card endpoints** — the bundled all-in-one profile route
was removed (O16). The two data **sources** (stats, news) refine into end products that
converge into the **Sigil**. Route shape is authoritative in `go/internal/api/server.go`.

Surface ownership is deliberate:

- `/leaderboard` exposes the ranked hierarchy: sport -> league/conference -> division -> team -> player. It is the discovery and cohort-navigation surface.
- `/profile` surfaces cards for one selected entity. It should compose `meta`, `stats`, `rating`, `news`, `momentum`, and `sigil`, not become a roster or local discovery database.
- Team roster discovery is the player leaderboard narrowed by `team_id`; the legacy `/team/{id}/roster` route remains only for compatibility.

Per-entity products (`{entityType}` ∈ `player|team`):

- **stats source:**
  - `GET /api/v1/{sport}/{entityType}/{id}/stats` — season Composite rating (breakdown, modes, fantasy, scoped ranks) + `available_seasons` + the per-event series
  - `GET /api/v1/{sport}/{entityType}/{id}/rating` — model-divined statistical read + Composite/PEAK z-score trajectory metadata from recent event form (`stat_summaries`)
- **news source:**
  - `GET /api/v1/{sport}/{entityType}/{id}/news` — scoped model narratives, hottest first by impact, with source timestamps and trajectory markers (`news_summaries`; `scope=current_week|last_week|two_weeks_ago|three_weeks_ago|last_month`)
  - `GET /api/v1/{sport}/{entityType}/{id}/transfers` — scoped vetted transfer/trade rumors by heat, with the same timestamp/source/trajectory protocol as narratives — team→players, player→clubs (`transfer_rumors`)
- **convergence:**
  - `GET /api/v1/{sport}/{entityType}/{id}/momentum` — Rating-trajectory × Vibe-trajectory (stats trend + narrative trend)
  - `GET /api/v1/{sport}/{entityType}/{id}/sigil` — the Sigil crown synthesis (Rating + Vibe + Momentum → `sigil_synthesis`)
- `GET /api/v1/{sport}/{entityType}/{id}/meta` — per-entity identity (page header); 404 when the entity is unknown
- `GET /api/v1/{sport}/team/{id}/results` — a team's finalized scorelines for a season
- `GET /api/v1/{sport}/team/{id}/roster` — legacy compatibility; new clients use `/leaderboard?entity_type=player&team_id={id}`

> **Convergence rename (O14):** the earlier per-product names `/special`, `/trends`, and
> per-entity `/vibes` are **gone** — `/special` folded into `/rating`, `/trends` became
> `/momentum`, and per-entity `/vibes` became `/sigil` (the crown). The bundled `/news`
> rail and `/sparkline`/`/starline` were retired earlier (2026-06-15).

Sport-level + leaderboard routes:

- `GET /api/v1/entities` (alias: `/api/v1/autofill`) — universal text-only player/team directory for home search
- `GET /api/v1/{sport}/meta`, `GET /api/v1/{sport}/autofill`, `GET /api/v1/{sport}/health` — legacy sport-wide metadata/search payload, legacy sport autofill, freshness
- `GET /api/v1/{sport}/leaderboard` (DB-first ranking/cohort surface — top-down filters from sport → league/conference → division → team → player; `entity_type=player&team_id=...` is the full current roster surface; also `?board=rating|vibes|sigil|news|transfers|momentum`)
- `GET /api/v1/{sport}/leaderboard/vibes` — sport-wide Vibe board (latest sentiment 1-100)
- `GET /api/v1/{sport}/leaderboard/sigil` — sport-wide Sigil crown board (+ `previous_score` delta)
- `GET /api/v1/{sport}/leaderboard/news` — hottest model narratives by per-narrative impact (`scope=current_week|last_week|two_weeks_ago|three_weeks_ago|last_month`)
- `GET /api/v1/{sport}/leaderboard/transfers` — model-vetted rumors by heat 0-100 with the same historical scopes as News
- `GET /api/v1/{sport}/leaderboard/momentum` — stored Momentum snapshots from `momentum_scores`, refreshed when upstream Vibe/rating data changes; rating lookback = the entity's last `season_bridge_window(sport)` games (~10% of season), season-spanning (`/trending` legacy alias)

League-scoped variants (preferred for multi-league precision):

- `GET /api/v1/{sport}/leagues/{leagueId}/{entityType}/{id}/momentum`
- `GET /api/v1/{sport}/leagues/{leagueId}/team/{id}/results`
- `GET /api/v1/{sport}/leagues/{leagueId}/meta`
- `GET /api/v1/{sport}/leagues/{leagueId}/health`

Operational + mobile-auth routes:

- `GET /`, `GET /health`, `GET /health/db`, `GET /health/cache`
- `GET /docs/`, `GET /docs/go.json` (Swagger UI + spec)
- `POST /api/v1/auth/device`, `POST /api/v1/auth/refresh` (public); `POST /api/v1/auth/device/push`, `POST /api/v1/auth/logout` (bearer)

See `ENDPOINTS.md` for full contract details.

## Implementation Notes

- Core data handlers live in `go/internal/api/handler/data.go` and follow a strict thin pattern (validate -> cache -> prepared statement -> passthrough JSON).
- Prepared statements for canonical payloads are registered in `go/internal/db/db.go` and return final JSON documents for frontend widgets.
- Sport routes are constrained to `nba`, `nfl`, and `football` at the router level.
- Data endpoints use in-memory caching with ETag support (`TTLData=5m`).
- Background workers in the API process are SQL-only (maintenance/news-scrub enqueue + notifications/listener) and are not on the serving path. Model inference runs in Rust (`scoracle-cognition` + `statcommentary`). See `RUNBOOK.md`.

Detailed repo-local development rules live in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).
AI-layer work should start in [rust/README.md](rust/README.md).

## Repository Layout

```text
scoracle-backend/
├ README.md
├ ENDPOINTS.md
├ sql/                    # Postgres schemas, views, functions, triggers
├ go/                     # Unified public API service
│   ├── cmd/api/
│   ├── internal/
│   ├── docs/
│   └── go.mod
├ rust/                   # Rust Cognition Harness: queue-stage model inference + rating batch
└── scripts/              # Hosting scripts, cron, release management
```

## Quick Start

### Run Components Manually

Go API:

```bash
cd go
go build -o bin/scoracle-api ./cmd/api
./bin/scoracle-api
```

Seeder boundary: `roster seed` owns season-scoped player discovery via
`team_rosters`; `meta seed` only enriches that roster. This avoids BDL's
historical player-list payloads becoming the metadata universe.

## Testing

```bash
(cd go && go test ./...)
(cd go && go build -o bin/scoracle-api ./cmd/api)
(cd rust && cargo test --lib)
(cd rust && cargo build --bin scoracle-cognition --bin statcommentary)
```

See [`../scoracle-wiki/progress_docs/scoracle-backend/RUST_REPO_BOUNDARY_ASSESSMENT.md`](../scoracle-wiki/progress_docs/scoracle-backend/RUST_REPO_BOUNDARY_ASSESSMENT.md)
for the current recommendation on whether the Rust cognition layer should become its own repo.

## Progress Docs

Every meaningful backend session adds a doc in
`../scoracle-wiki/progress_docs/scoracle-backend/` — see [Shared Organization Docs](#shared-organization-docs)
above. Suggested format:

```md
# YYYY-MM-DD - <Title>

## Goal

## What Changed

## Files Changed

## Verification

## Result

## Follow-Up
```

## Handoff Format

For unfinished multi-step work, end with:

```text
Continue work in scoracle-backend on branch <branch>.

Read first:
1. README.md
2. ../scoracle-wiki/PRODUCT_NARRATIVE.md
3. ../scoracle-wiki/DATA_FLOW.md

Last completed:
- <summary>

Changed files:
- <files>

Verification run:
- <commands/results>

Next task:
- <specific next step>

Known risks:
- <risks or none>
```

## Environment Variables

See `.env` (committed template) and copy to `.env.local` (gitignored) for real values.
DB URL priority (per `go/internal/config/config.go`): `DATABASE_PRIVATE_URL` > `DATABASE_URL`.

Required for local operation:

- `DATABASE_PRIVATE_URL` (or `DATABASE_URL`)
- `BALLDONTLIE_API_KEY` (seeder, NBA/NFL)
- `SPORTMONKS_API_TOKEN` (seeder, football)

Common optional (full list + defaults in `config.go`):

- `API_PORT`/`PORT`, `CACHE_ENABLED`, `RATE_LIMIT_ENABLED`
- `DB_POOL_MAX_CONNS` (default `25`), `DB_POOL_MIN_CONNS`, `DB_POOL_MAX_LIFE_MINUTES`
- `CORS_ALLOW_ORIGINS`, `CORS_PRODUCTION_ORIGINS`
- Rust cognition (read by Rust config, not Go): `OLLAMA_BASE_URL`, `OLLAMA_MODEL` (default `mistral:7b`), `OLLAMA_TIMEOUT_SECONDS`, `OLLAMA_MAX_CONCURRENT`
- Go workers: `PIPELINE_STATS_INTERVAL_MINUTES`
- Mobile auth: `JWT_SECRET` (unset ⇒ `/auth/*` returns 503), `JWT_ACCESS_TTL_MINUTES`, `JWT_REFRESH_TTL_DAYS`
- `FIREBASE_CREDENTIALS_FILE`; seeder third key `API_SPORTS_KEY`

## Trademarks & Nominative Fair Use

Team names, logos, and other identifying marks displayed by Scoracle are the property of their respective owners (leagues, teams, and affiliated entities). These marks are used solely to identify the teams and players whose statistical data is presented — not to imply any official sponsorship, endorsement, or affiliation between Scoracle and any league, team, or player.

This usage satisfies the three-part test for nominative fair use:

1. The teams and leagues cannot reasonably be identified without reference to their marks.
2. Only as much of each mark is used as necessary for identification.
3. Nothing in the presentation suggests official sponsorship or endorsement by the mark holder.

Scoracle is not affiliated with, endorsed by, or in any way officially connected to the NBA, NFL, the Premier League, La Liga, Bundesliga, Serie A, Ligue 1, or any of their member teams and clubs.

## License & Copyright

Copyright (c) 2026 Scoracle. All rights reserved.

This repository and its contents — including but not limited to source code, database schemas, API designs, data pipeline architecture, and documentation — are proprietary and confidential. No part of this repository may be reproduced, distributed, transmitted, or otherwise used in any form without the prior written permission of the copyright holder.

Unauthorized use, copying, modification, or distribution of any materials in this repository is strictly prohibited and may result in legal action.
