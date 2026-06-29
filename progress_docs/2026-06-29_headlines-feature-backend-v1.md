# Headlines Feature — Backend v1

**Date:** 2026-06-29
**Plan:** `planning_docs/HEADLINES_MASTER_PLAN.md` + `planning_docs/HEADLINES_FEATURE.md`
**Status:** Backend Phase 1–3 done; Phase 4/5 docs done. Per-entity route wiring in
`go/internal/api/server.go` is intentionally NOT applied because that file carries
pre-existing OpenCode-proxy WIP that is not mine.

## What was built

### Phase 1: Database

- `sql/migrations/113_create_headlines_table.sql`
  - New `public.headlines` table (id, sport, entity_type, entity_id, title, category,
    source_url, source_name, published_at, created_at).
  - Indexes for entity reads, category filtering, and the 2-day expiration cutoff.
  - Updates `enqueue_derive_on_vetted()` to enqueue the new `headlines` stage alongside
    `narratives`/`vibe` (and `transfers` for teams).

### Phase 2: Rust Cognition Harness pipeline integration

- `rust/src/work.rs` — added `Stage::Headlines` to the enum.
- `rust/src/headline.rs` — new stage handler:
  - Loads the entity's recent vetted corpus (72h, bounded to 15 articles).
  - Prompts the `EmotionalNews` role to identify breaking headline news and emit JSON
    `{"headlines": [{"title": "...", "category": "...", "article_numbers": [...]}]}`.
  - Validates/normalizes categories to `{transfer, injury, coaching, contract, other}`.
  - Persists each headline with resolved source URL/name and original `published_at`.
  - Unit tests for parser, category normalization, and prompt assembly.
- `rust/src/lib.rs` — exported `pub mod headline`.
- `rust/src/main.rs` — registered `HeadlinesHandler`, updated `COGNITION_STAGES` default
  to `scrub,headlines,transfers,narratives,vibe,sigil`.
- `scripts/systemd/scoracle-cognition.service` — updated the hardcoded
  `COGNITION_STAGES` env to include `headlines`.

### Phase 3: Go API handler

- `go/internal/db/db.go`
  - Added prepared statement `entity_headlines` — per-entity card read (2-day window,
    `published_at DESC`, default limit 20).
  - Added prepared statement `headlines_leaderboard` — sport-wide board ranked by
    headline count + most recent headline.
- `go/internal/api/handler/data.go`
  - Added `GetEntityHeadlines` handler (cached 5 min TTL).
  - Added `GetHeadlinesLeaderboard` handler.
  - Extended `GetLeaderboard` `?board=` switch to accept `headlines`.

### Phase 4: Leaderboard integration

Covered by `headlines_leaderboard` + the `?board=headlines` path above.

### Phase 5: Documentation

- `ENDPOINTS.md` — added `/{entityType}/{id}/headlines`, `/leaderboard/headlines`, and
  `?board=headlines`; updated the authoritative route inventory + last-updated date.
- `rust/README.md` — updated stage count and layout to include `headline.rs`.
- `RUNBOOK.md` — updated the cognition daemon stage list to `scrub → headlines → ...`.
- This progress doc.

## Not done / blocked

- **Route wiring in `go/internal/api/server.go`**: the per-entity `/{sport}/{entityType}/{id}/headlines`
  route is NOT wired because `server.go` currently carries pre-existing OpenCode-proxy WIP
  (`go/internal/api/opencodeproxy/`) that is not mine. The handler and prepared statement
  exist; the route only needs:
  ```go
  r.Get("/{entityType}/{id}/headlines", h.GetEntityHeadlines)
  ```
  added in the same block as `/news`, `/transfers`, `/sigil`, etc. The dedicated
  `/leaderboard/headlines` route is already reachable via `/leaderboard?board=headlines`,
  so no extra wiring is required for that path.

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

# Go tests (the only failures are the pre-existing OpenCode-proxy WIP tests)
go test ./...
```

The two failing `internal/api` tests (`TestOpenCodeProxyRouteRequiresAuth`,
`TestOpenCodeHostRouteUsesCloudflareAccessGate`) are part of the untouched OpenCode-proxy
WIP and pre-date the Headlines work.

## File layout delta

```
sql/migrations/113_create_headlines_table.sql      NEW (Phase 1 + trigger update)
rust/src/headline.rs                               NEW (Phase 2)
rust/src/work.rs                                   + Stage::Headlines
rust/src/lib.rs                                    + pub mod headline
rust/src/main.rs                                   + handler registration + default stages
scripts/systemd/scoracle-cognition.service        + headlines in COGNITION_STAGES
go/internal/db/db.go                              + entity_headlines + headlines_leaderboard statements
go/internal/api/handler/data.go                  + GetEntityHeadlines + GetHeadlinesLeaderboard + board switch
ENDPOINTS.md                                       + headlines routes/sections
rust/README.md                                     + stage count / layout
RUNBOOK.md                                         + daemon stage list
progress_docs/2026-06-29_headlines-feature-backend-v1.md   NEW (this doc)
```

## Carry

- Wire the per-entity `/headlines` route once the OpenCode-proxy WIP in `server.go` is
  resolved.
- Frontend: consume the new `GET /api/v1/{sport}/{entityType}/{id}/headlines` endpoint
  and add Headlines to the news-rail scope selector.
- `099_team_rosters.sql` remains untracked and untouched (not ours).
- F-046 remains open (DB password in git history; coordinate before any force-push).
- B3 — widen `work::Item.entity_id` i32 → i64 — still deferred.
