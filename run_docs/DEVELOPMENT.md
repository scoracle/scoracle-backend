# Backend Development Rules

Repo-local implementation guidance for `scoracle-backend`. Start with `README.md`, then use this file when adding endpoints, migrations, SQL contracts, or backend code.

## Design Rules

1. Postgres owns durable world state, provenance, products, work state, and serving projections.
2. DuckDB owns the analytical direction: bounded studies of versioned evidence, published through application adapters. Existing SQL analytics stay authoritative until their replacement passes parity and cutover gates. Do not add new computations to Postgres solely because legacy rules put all analytics there.
3. Studio (`rust/src/studio/`) is the in-house harness. Its core accepts prepared domain material and injected capabilities; it must not import concrete database, queue, or model-host adapters.
4. Application wiring owns retrieval, snapshot export, model routing, scheduling, transactions, publication, retries, and resource limits. No system reaches into another system's internals.
5. Go handlers stay thin: validate, cache/ETag, prepared statement, passthrough JSON. No acquisition, study, or model call runs inline with serving.
6. Acquisition preserves source identity and provenance before publication. Retired provider/seed tooling is not an architectural requirement.
7. Sport boundaries stay explicit: `nba`, `nfl`, and `football` logic should not blur accidentally. Public route shape is defined by `go/internal/api/server.go`.

## Dependency and migration boundaries

A Studio seat consumes a prepared assignment, not rows or another seat's storage representation. All nine seats create in `studio/`; application/evidence adapters own retrieval, routing, queue policy, and publication. Studio production code has no concrete application, evidence-provider, database, or queue imports. There is one harness and no compatibility junction or storage-bearing execution context.

Inject narrow capabilities only when a real assignment requires them. Keep prompt construction, parsing, and product assembly testable with a fake model and fake publisher. `Generation<T>` carries provenance through the boundary. Errors must reach the application; adapters own the durability guarantees behind `Publisher<T>`.

A migration slice preserves the existing material, hash, prompt, and output meaning before adding richer evidence. Then version any intentional analytical or character-contract change and evaluate its value. Preserve missingness, coverage, measurement origin, and snapshot provenance; missing data must not silently become zero.

One producer owns each live output during cutover. Migration 256 establishes queue acknowledgement ownership: a running item has a unique claim token and captured input revision, and every complete/fail/defer/release must match both. Migrations 257–261 apply the publication pattern to Influencer, Analyst, Scout, Journalist, and Insider: infer without a transaction, then lock the exact claim and commit any product plus required provenance and the seat's narrow durable follow-up intent. A superseded execution publishes nothing. Insider preserves bounded partial progress: each pair or identity effect rechecks the exact team lease, served pairs atomically record distinct player Oracle barriers, and final completion records the team barrier while deleting the claim. Analyst `NoMaterial`, Scout debounce, Journalist debounce, and cleared transfer pairs commit no product but preserve their required lifecycle semantics. Scout's no-stats marker follows the normal Momentum path; Journalist's no-corpus or called-empty edition writes one marker. Journalist also commits all chapter rows and storyline progression atomically. Optional diagnostic ledger writes remain best-effort after commit and are not proof of durability. Bind dependencies when constructing a `WorkHandler`. Its sole `handle(Item)` entry point must return a durable completed, deferred, or superseded disposition; the worker never completes successful work or dispatches a best-effort follow-up. Editor, Investigator, and Graph atomically publish their retained effects and exact completion; terminal Oracle needs no outbox. Use the approved wiki modernization plan for live acceptance and analytical gates.

Update the README and wiki data-flow implementation status with each migrated boundary. The destination is three distinct systems coordinated by application code, not a universal pipeline every task must traverse.

## Shared worker scheduling

Workers on multiple hosts compete for individual `pipeline_work` rows in the same PostgreSQL database. No host owns an entire stage pool. `pipeline_work_ready` NOTIFY broadcasts after enqueue commits; each worker maintains LISTEN plus startup and periodic recovery. Notifications are hints, while the durable row is the obligation. Atomic `FOR UPDATE SKIP LOCKED` claims give workers disjoint current jobs; exact claim tokens and captured revisions fence stale publication and acknowledgements. Retries and newer revisions can repeat computation, so this is not an exactly-once inference guarantee.

Editor publishes its reading and downstream Graph/Investigator obligations atomically; packet publication fans out the news-dependent character work. The Scout's statistics rail remains independent. Claim-time entity checks keep Analyst behind pending/running Scout and Influencer work, and Oracle behind all five pending/running pillars. Undispatched completion outbox events also block these consumers, closing the gap between publication and follow-up enqueue. Existing failed-pillar partial-read policy remains unchanged. These checks cover known work at claim time; new evidence can arrive during inference and trigger a subsequent revision. They do not impose a global barrier on unrelated articles or entities.

Every live consumer must run compatible fenced code and the same readiness policy. Rotate admission across ready stages so continuous upstream inflow cannot monopolize a worker's slots. Poll bounded outbox dispatch independently of drain completion. Packet compilation takes a transaction-scoped PostgreSQL advisory lease before scanning dirty storylines, so multiple Editor-capable hosts cannot compile the same sweep concurrently; any host can take the next sweep after release or connection loss. Model concurrency budgets remain per process; point each worker at its intended model host and account for aggregate load if several workers share one model server.

## Public Endpoint Flow

Any new public data endpoint should follow this path:

1. Add or update a prepared statement in `go/internal/db/db.go` that returns final JSON.
2. Add a thin handler in `go/internal/api/handler/data.go`.
3. Wire the route in `go/internal/api/server.go`.
4. Add Swagger annotations.
5. Update `run_docs/ENDPOINTS.md`.
6. Update `README.md` if the route changes the public surface.
7. Record progress in `../scoracle-wiki/progress_docs/scoracle-backend/`; use the wiki Changelog for landmarks.

## Route Conventions

Verify routes against `go/internal/api/server.go`; it is the source of truth.

Canonical per-entity profile products:

```text
/{sport}/{entityType}/{id}/meta
/{sport}/{entityType}/{id}/stats
/{sport}/{entityType}/{id}/rating
/{sport}/{entityType}/{id}/news
/{sport}/{entityType}/{id}/momentum
/{sport}/{entityType}/{id}/sigil
/{sport}/team/{id}/results
```

Canonical discovery products:

```text
/{sport}/leaderboard
/{sport}/leaderboard/{vibes|sigil|news|transfers|momentum}
```

`/{sport}/leaderboard` is the hierarchy surface: sport -> league/conference
-> division -> team -> player. The default Rating board with
`entity_type=player&team_id=...` is the full current roster surface and includes
active `team_rosters` members even when product metrics are null. Do not add
roster-style discovery back to profile cards.

`/{sport}/team/{id}/roster` remains wired as legacy compatibility only; new
clients should use `/leaderboard?entity_type=player&team_id={id}`.

The bundled all-in-one profile route is retired. `/special`, `/trends`, and per-entity `/vibes` are retired names, not current products.

## Migrations

Migrations live in `sql/migrations/` and are tracked in `public.schema_migrations`.

Apply pending migrations with:

```bash
DATABASE_PRIVATE_URL=... ./sql/migrate.sh
```

Fresh environments should be created from the current schema snapshot, not by replaying all migrations against an empty database:

```bash
./sql/build.sh "$NEW_ENV_URL"
```

Before restarting the Go API, make sure the live schema and prepared statements agree. `db.New` prepares statements at boot and should fail fast against drifted schema.

Full migration operations live in `../sql/README.md` and `RUNBOOK.md`.

## Cognition memory taxonomy (continuity vs measurement)

Postgres preserves evidence and prior interpretations. Application adapters select the material a Studio assignment receives. Every fact it reads carries a
**provenance class**, and the two classes must never cross — this is the *echo-chamber rule*:
the model's own conclusions may inform continuity but can never become evidence that inflates
the numeric signal it later reads.

- **Measurement** — anchored to raw inputs only: `news_articles` (provider articles) and
  `transfer_ground_truth` (confirmed moves). This is what heat, likelihood, confirm/fizzle, and
  typed-link scoring are computed from. Authoritative; feeds the numeric loop.
- **Continuity** — the harness's own past outputs, re-surfaced to a stage as provenance-labeled
  `"Our prior read:"` lines so a junction sees its own paper trail (mig 168, card-level). It
  frames the arc a read sits in; it is *never itself evidence* for a new claim.

The partition is enforced structurally, not just by prompt labels:

- `narrative_events.origin` is `'extraction'` (a model read of one raw article — measurement) or
  `'junction'` (a stage's own served verdict banked into the unified event log — continuity/audit),
  added by **mig 170**. The dedupe key includes `origin`, so an extraction event and a junction
  verdict for the same `(article, pair, predicate)` coexist without clobbering.
- Every **measurement consumer** of `narrative_events` filters `origin = 'extraction'` — today
  `refresh_typed_links` (events → typed links) and `score_transfer_likelihood` (events → likelihood
  language input). Junction-authored events are invisible to both. Episodes derive from links
  (already filtered) or backfill from the raw news rail, so they read no junction events directly.
- **`assert_provenance_firewall()`** (mig 172) is the guard: it RAISEs if any named measurement
  consumer's current body reads `narrative_events` without the `origin = 'extraction'` filter.
  **Any future migration that rebuilds a measurement consumer must end with
  `PERFORM public.assert_provenance_firewall();`** (register new consumers in the function's
  `v_consumers` list), so a re-introduced leak is caught at apply time.

## Go Style

- `gofmt` is authoritative.
- Use PascalCase for exported names and camelCase for unexported names.
- Group imports as standard library, third-party, then internal.
- Wrap errors with context using `%w`.
- Return early on errors.
- Add doc comments for exported symbols.
- Keep handlers thin.

Useful commands:

```bash
cd go
gofmt -w .
go vet ./...
go test ./...
```

## SQL Style

- Use sport schemas for sport-specific data.
- Use `public` for shared sport-agnostic tables.
- Use `json_build_object`, `jsonb_build_object`, and row JSON helpers for API-shaped responses.
- Keep current analytical producers authoritative until a verified replacement takes ownership. New DuckDB studies use bounded snapshots and versioned results; serving projections remain in Postgres.
- Keep prepared statement output presentation-free and product-aligned.

## Key Files

- `go/internal/api/server.go` - route wiring.
- `go/internal/api/handler/data.go` - data endpoint handlers.
- `go/internal/api/respond/` - response helpers.
- `go/internal/cache/cache.go` - cache policy defaults.
- `go/internal/config/config.go` - environment resolution.
- `go/internal/db/db.go` - prepared statements.
- `rust/src/studio/` - Studio, the in-house harness and migrated character creation.
- `rust/src/application/` - explicit evidence, persistence, and work-coordination adapters.
- `rust/src/evidence/memories` - sourced context selection, fingerprints, and rendering.
- `rust/src/application/queue/work.rs`, `rust/src/application/queue/worker.rs` - current durable queue runtime.
- `sql/` - schema, migrations, functions, views, and snapshots.
