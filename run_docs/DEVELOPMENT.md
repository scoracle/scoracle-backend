# Backend Development Rules

Repo-local implementation guidance for `scoracle-backend`. Start with `README.md`, then use this file when adding endpoints, migrations, SQL contracts, or backend code.

## Design Rules

1. Postgres is the domain engine and response shaper for data endpoints.
2. Go handlers stay thin: validate, cache/ETag, prepared statement, passthrough JSON.
3. Python is ingestion-only: call providers, normalize enough to upsert, write source rows, and call domain functions such as `finalize_fixture()`.
4. Rust cognition owns model inference stages and rating commentary. Go serving must not invoke the model inline.
5. Derived stats, percentiles, rankings, and API-shaped JSON belong in SQL.
6. Sport boundaries stay explicit: `nba`, `nfl`, and `football` logic should not blur accidentally.
7. Public route shape is defined by `go/internal/api/server.go`.

## Public Endpoint Flow

Any new public data endpoint should follow this path:

1. Add or update a prepared statement in `go/internal/db/db.go` that returns final JSON.
2. Add a thin handler in `go/internal/api/handler/data.go`.
3. Wire the route in `go/internal/api/server.go`.
4. Add Swagger annotations.
5. Update `ENDPOINTS.md`.
6. Update `README.md` if the route changes the public surface.
7. Add progress docs locally and in `../scoracle-wiki/progress_docs/` for landmarks.

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
./sql/build.sh "$PROD_URL" "$NEW_ENV_URL"
```

Before restarting the Go API, make sure the live schema and prepared statements agree. `db.New` prepares statements at boot and should fail fast against drifted schema.

Full migration operations live in `../sql/README-migrations.md` and `RUNBOOK.md`.

## Cognition memory taxonomy (continuity vs measurement)

The relational DB is the cognition harness's memory. Every fact a stage reads carries a
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
- Keep percentile, rating, and derived-stat logic in database functions/triggers.
- Keep prepared statement output presentation-free and product-aligned.

## Key Files

- `go/internal/api/server.go` - route wiring.
- `go/internal/api/handler/data.go` - data endpoint handlers.
- `go/internal/api/respond/` - response helpers.
- `go/internal/cache/cache.go` - cache policy defaults.
- `go/internal/config/config.go` - environment resolution.
- `go/internal/db/db.go` - prepared statements.
- `rust/` - Rust cognition harness and rating batch.
- `sql/` - schema, migrations, functions, views, and snapshots.
