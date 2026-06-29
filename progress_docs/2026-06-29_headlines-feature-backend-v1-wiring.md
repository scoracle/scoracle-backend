# Headlines Feature — Backend v1 Wiring & Provenance

**Date:** 2026-06-29
**Plan:** `planning_docs/HEADLINES_MASTER_PLAN.md` + `planning_docs/HEADLINES_FEATURE.md`
**Status:** Per-entity route wired, cache TTL corrected, provenance columns added, Rust
persist updated. Backend v1 complete.

## What was built

### Route wiring (previously blocked)

- `go/internal/api/server.go`
  - Wired `GET /api/v1/{sport}/{entityType:player|team}/{id}/headlines` → `h.GetEntityHeadlines`
    alongside `/news` and `/transfers`.
  - Wired `GET /api/v1/{sport}/leaderboard/headlines` → `h.GetHeadlinesLeaderboard`
    alongside the other dedicated leaderboard boards.
  - Done carefully so the unrelated OpenCode-proxy WIP in the same file remains untouched.

### Cache policy fix

- `go/internal/api/handler/data.go`
  - `GetEntityHeadlines` now uses `cache.TTLNews` (10 min) instead of `cache.TTLData`
    (5 min), matching the other news-rail products and the backend plan.

### Provenance columns

- `sql/migrations/114_headlines_provenance.sql` (new)
  - Adds `input_news_ids`, `model_version`, `prompt_version`, `trigger_type`, and
    `generated_at` to `public.headlines`.
  - Drops the redundant `created_at` column added in migration 113.
  - `trigger_type` defaults to `'news_spike'` and is constrained to the planned enum.

- `rust/src/headline.rs`
  - `HeadlineRow` now carries `input_news_ids`.
  - `generate_headlines` maps the model's `article_numbers` back to cited corpus article IDs.
  - `persist_headlines` writes the full provenance envelope:
    `input_news_ids`, `model_version`, `prompt_version` (constant), `trigger_type`
    (`'news_spike'`), and `generated_at = NOW()`.

### Tests

- `go/internal/api/server_test.go` — added route-registration cases for player/team
  `/headlines` and `/leaderboard/headlines`.
- `rust/src/headline.rs` — added parser test for `article_numbers` arrays.

## Verification

```bash
# Rust gate
cd rust
cargo build --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --lib   # 83 passed, 0 failed, 1 ignored

# Go build
cd go
go build ./...
gofmt -w internal/api/server.go internal/api/handler/data.go
go vet ./...

# Go tests (failures are pre-existing OpenCode-proxy WIP)
go test ./...
```

The two failing `internal/api` tests (`TestOpenCodeProxyRouteRequiresAuth`,
`TestOpenCodeHostRouteUsesCloudflareAccessGate`) are part of the untouched
OpenCode-proxy WIP and pre-date the Headlines work.

## File layout delta

```
sql/migrations/114_headlines_provenance.sql              NEW
rust/src/headline.rs                                     + input_news_ids + provenance persist
go/internal/api/server.go                                + /headlines + /leaderboard/headlines routes
go/internal/api/handler/data.go                          ~ cache TTL: TTLData → TTLNews
go/internal/api/server_test.go                           + route registration tests
progress_docs/2026-06-29_headlines-feature-backend-v1-wiring.md  NEW (this doc)
```

## Carry

- Apply migration 114 before the next API restart so `entity_headlines` prepared-statement
  registration sees the final column set.
- The OpenCode-proxy WIP in `server.go` / `opencodeproxy/` remains untouched.
- `099_team_rosters.sql` remains untracked and untouched (not ours).
