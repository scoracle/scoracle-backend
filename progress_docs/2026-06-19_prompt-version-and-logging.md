# 2026-06-19: Prompt Version Surfacing + Logging Standardization

## Goal

Improve debuggability and observability across the backend:
- **Phase 4:** Surface `prompt_version` and `model_version` in API responses so clients can track which prompt/model generated each derived output
- **Phase 5:** Standardize error logging from `log.Printf` to `slog` for structured, consistent logging

## What Was Done

### Phase 4: Prompt Version in API Responses

Updated the `entity_news` prepared statement to include `model_version` and `prompt_version` from the `news_summaries` table:

**File:** `go/internal/db/db.go` (entity_news statement)
- Added `ns.model_version, ns.prompt_version` to the SELECT clause
- Added corresponding fields to the JSON output via `row_to_json`

**Verification:** The `entity_vibes` statement already included `vs.model_version, vs.prompt_version` from `sigil_synthesis`, so both news and sigil endpoints now surface these fields.

### Phase 5: Logging Standardization

Migrated all `log.Printf` calls in the news package to structured `slog` logging:

**File:** `go/internal/thirdparty/news.go`
- Added `log/slog` import (replaced `log`)
- Added `logger *slog.Logger` field to `NewsService` struct
- Updated `NewNewsService` to accept logger parameter (defaults to `slog.Default()` if nil)
- Replaced 4 `log.Printf` calls with structured `slogger` methods:
  - Line 150: `s.logger.Warn("persist failed", ...)`
  - Line 180: `s.logger.Warn("entity pool load failed", ...)`
  - Line 357: `s.logger.Info("loaded entity pool", ...)`
  - Line 606: `s.logger.Warn("RSS fetch error", ...)`

**File:** `go/internal/corpus/corpus.go`
- Updated `Sweep` function to pass logger to `NewNewsService(pool, logger)`

**File:** `go/cmd/comention-backfill/main.go`
- Updated to pass logger to `NewNewsService(pool, logger)`

**File:** `go/internal/api/handler/handler.go`
- Updated to pass `nil` to `NewNewsService(pool, nil)` (handler doesn't have logger; news service unused in handler currently)

## Files Changed

| File | Change | Lines |
|------|--------|-------|
| `go/internal/db/db.go` | Added model_version, prompt_version to entity_news query | +2 -2 |
| `go/internal/thirdparty/news.go` | Migrated to slog, added logger field | +11 -9 |
| `go/internal/corpus/corpus.go` | Pass logger to NewNewsService | +1 -1 |
| `go/cmd/comention-backfill/main.go` | Pass logger to NewNewsService | +1 -1 |
| `go/internal/api/handler/handler.go` | Pass nil to NewNewsService | +1 -1 |

## Verification

- [x] Branch synced with `origin/main` before editing
- [x] Code compiles: `cd go && go build ./...` — PASS
- [x] All tests pass: `cd go && go test ./...` — PASS
- [x] No remaining `log.Printf` in ml/ or thirdparty/ packages
- [x] Prompt version fields verified in prepared statements

## Result

**Phase 4 Complete:** API responses for `/news` and `/sigil` endpoints now include `model_version` and `prompt_version` fields, enabling clients to:
- Track which prompt version generated each narrative or sigil
- Debug prompt regression issues
- Correlate API responses with backend prompt versions

**Phase 5 Complete:** All news-related logging now uses structured `slog` with consistent field names, improving:
- Log parsing and filtering
- Error context (structured key-value pairs vs formatted strings)
- Consistency with the rest of the codebase (which uses slog)

**No breaking changes** — new fields are additive to API responses, and logging changes are internal only.

## Related

- [[Product Narrative]] — the curated derivation engine
- [[Backend Architecture]] — the Go API design
- [[AI Architecture]] — the Gemma pipeline
- [progress_docs/2026-06-19_scrub-wiring-complete.md](2026-06-19_scrub-wiring-complete.md) — the scrub precision pass this complements
