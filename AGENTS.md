# AGENTS.md

Coding agent guide for the Scoracle Data repository. Read CLAUDE.md for full architecture context.

## Architecture Summary

Two services, one Neon PostgreSQL database:
- **PostgREST** (port 3000) — auto-generated REST API for all core data (stats, profiles, standings). Add endpoints by adding views/functions to `sql/<sport>.sql`.
- **Go API** (port 8000) — third-party integrations only (news, tweets), plus health checks, Swagger UI, and background workers.

All Go code lives under `go/`. Module: `github.com/albapepper/scoracle-data`, Go 1.25.

## Build & Run

All commands run from the `go/` directory:

```bash
go build -o bin/scoracle-api ./cmd/api
go build -o bin/scoracle-ingest ./cmd/ingest
./bin/scoracle-api                                          # start API server
./bin/scoracle-ingest seed nba --season 2025                # seed NBA data
./bin/scoracle-ingest percentiles --sport NBA --season 2025  # recalculate percentiles
./bin/scoracle-ingest fixtures process --sport NBA --max 10 --workers 2
```

Docker (from repo root):
```bash
docker compose up --build   # PostgREST :3000, Go API :8000
```

## Testing

```bash
go test ./...                                              # all tests
go test ./internal/api/ -run TestRouteOwnershipSplit       # single test
go test -v ./internal/api/ -run TestRouteOwnershipSplit    # verbose
go test -v ./internal/api/ -run TestRouteOwnershipSplit/news_remains_on_go  # single subtest
```

Conventions:
- Standard library `testing` + `net/http/httptest` only. No testify or other frameworks.
- Table-driven tests: slice named `tests`, loop var `tt`, subtests via `t.Run(tt.name, ...)`.
- Assertions use `t.Fatalf` with `got, want` format: `"status for %s = %d, want %d"`.
- Use `nil` for dependencies not needed by the test (e.g., `NewRouter(nil, cache.New(false), cfg)`).
- Test files live alongside source, same package (not `_test` external package).

## Linting & Formatting

No custom linter config exists. Use standard `gofmt` / `goimports`. No pre-commit hooks.

## Code Style

### Imports — Three Groups

```go
import (
    "context"        // 1. stdlib
    "fmt"

    "github.com/go-chi/chi/v5"  // 2. external

    "github.com/albapepper/scoracle-data/internal/api"  // 3. internal
)
```

Blank-line separated. Alphabetical within each group. Aliases only when needed: `corslib "github.com/rs/cors"`. Side-effect imports get a trailing comment: `_ "...docs" // swagger docs`.

### Naming

| What | Convention | Examples |
|------|-----------|----------|
| Packages | lowercase, short | `handler`, `cache`, `db`, `bdl`, `respond` |
| Exported types | PascalCase, noun | `Handler`, `Cache`, `SeedResult`, `Client` |
| Unexported types | camelCase | `entry`, `ipLimiter`, `bdlTeamRaw` |
| Exported functions | PascalCase | `UpsertTeam`, `WriteJSON`, `RecalculatePercentiles` |
| Unexported functions | camelCase | `normalizeNBATeam`, `envOr`, `nilEmpty` |
| Exported constants | PascalCase | `PlayersTable`, `TTLNews` |
| Unexported constants | camelCase | `sportNBA`, `newsDefaultLimit`, `reconnectBackoff` |
| Method receivers | 1-letter | `h` (Handler), `c` (Cache/Client), `p` (Pool), `r` (Result) |
| JSON tags | snake_case | `json:"short_code,omitempty"` |
| API error codes | SCREAMING_SNAKE | `NOT_FOUND`, `DB_ERROR`, `RATE_LIMITED` |

### Error Handling

- Wrap with `fmt.Errorf("short context: %w", err)` — lowercase, no trailing period.
- No custom error types anywhere. All errors are inline `fmt.Errorf` strings.
- Handlers convert errors to HTTP responses; they never propagate errors upward.
- String matching for 404 detection: `strings.Contains(err.Error(), "not found")`.
- Seed operations accumulate non-fatal errors via `result.AddErrorf(...)` instead of failing fast.
- Discard errors with `_` only for truly optional operations (e.g., `.env` loading).

### Logging

- Use `log/slog` exclusively. Pass `*slog.Logger` as a dependency, never use globals.
- Structured key-value pairs: `logger.Info("message", "key", value)`.
- Levels: `Info` for operations/progress, `Warn` for recoverable issues, `Error` for serious failures.
- Startup failure pattern: `logger.Error("...", "error", err); os.Exit(1)` — not `log.Fatal`.

### Types & Constructors

- Constructor: `New()` (one per package) or `NewXxx()` (multiple types). Returns `*T` or `(*T, error)`.
- `nil` return signals "disabled/not configured" (e.g., missing credentials).
- Unexported fields on behavior types; exported fields only on data carriers (with JSON tags).
- Pointers for nullable values: `*int`, `*string` with `omitempty`.
- No interfaces — provider-agnosticism uses canonical output structs in `provider/canonical.go`.

### Function Signatures

- `context.Context` is always the first parameter.
- `error` is always the last return value.
- HTTP handlers: `func (h *Handler) Name(w http.ResponseWriter, r *http.Request)` — no error return.
- Named returns only for multi-value functions: `(playersUpdated, teamsUpdated int, err error)`.
- Streaming callbacks: `func(provider.Player) error` passed as `fn` parameter.

### Comments

- Package docs: `// Package xxx provides ...` on every package.
- Function docs: Godoc-style, start with function name: `// New creates a Handler ...`.
- Section dividers within files: `// ----------- Section Name -----------`.
- Swagger annotations (`@Summary`, `@Tags`, `@Router`, etc.) on all HTTP handler methods.

### HTTP Handler Pattern

Every handler follows this lifecycle:
1. Parse/validate request params — return `WriteError` on failure.
2. Build cache key, check cache via `h.cache.Get(key)`.
3. If cache hit, check ETag with `cache.CheckETagMatch` — return `WriteNotModified` or `WriteJSON` with `cacheHit=true`.
4. Query DB via prepared statement or call external service.
5. On error, return `WriteError` with appropriate status code.
6. Store result in cache via `h.cache.Set(key, data, ttl)`.
7. Return `WriteJSON(w, data, etag, ttl, false)`.

Response helpers: `respond.WriteJSON` (raw Postgres bytes), `respond.WriteJSONObject` (Go structs), `respond.WriteError` (structured errors with SCREAMING_SNAKE code), `respond.WriteNotModified` (304).

## Architecture Rules

1. **Postgres-as-serializer** — SQL functions return JSON. Go passes raw `[]byte` to the response. No struct scanning/marshaling for data endpoints.
2. **No service layer** — Handlers call `pgxpool` directly. Do not add service/repository patterns.
3. **No shared Provider interface** — Canonical output structs are the contract, not input interfaces.
4. **Per-sport schemas** — Separate Postgres schemas (`nba`, `nfl`, `football`). Each sport has its own `sql/<sport>.sql`. Never cross sport boundaries.
5. **Derived stats in Postgres** — Triggers compute per-36, per-90, TS%, win_pct, etc. Go never calculates derived stats or percentiles.
6. **JSONB for sport-specific data** — `stats` and `meta` columns are JSONB. No schema changes for new stat keys.
7. **New data endpoints go in PostgREST** — Add views/functions in `sql/`, not Go handlers.
8. **New third-party integrations go in Go** — Add handlers in `go/internal/api/handler/`.
9. **Do not edit `sql/` as migrations** — Edit sport schema files directly, no migration files.
10. **Ignore `legacy_fastapi/`** — Dead code. Do not reference, modify, or port from it.
