# 2026-06-19: Scrub Wiring Complete — Flip to Vetted-Only Corpus

## Goal

Activate the precision pass: flip all corpus-consuming queries to require `vetted IS TRUE` only, completing the scrub transition. The maintenance worker (`news_scrub`) already vets articles via local model; this change enforces that only vetted articles feed the pipeline (narratives → vibe → sigil).

## What Was Done

Updated three consumer query locations to require `vetted IS TRUE` (removed the transition clause `OR scrubbed_at IS NULL`):

1. **News narratives** (`go/internal/ml/news_narratives.go:201`) — `loadVettedCorpus` now only returns vetted articles for narrative generation
2. **Transfer heat** (`go/internal/ml/transfer.go:283-284`) — transfer query now requires both team and player links to be vetted; updated comment to reflect transition is complete
3. **Pipeline stats** (`go/internal/maintenance/maintenance.go:493`) — `in_scope` CTE now only counts vetted entities for coverage metrics

The maintenance scrub worker (`go/internal/maintenance/maintenance.go:372-451`) continues to find and vet unvetted articles in two phases:
- Phase 1: Auto-vets primary links (`match_confidence >= 1.0`) via SQL UPDATE (no local model, bounded to 20k rows/tick)
- Phase 2: local model scrubs candidate-rich articles (`match_confidence < 1.0`) in batches of 15

This worker runs every 30 minutes by default (`NewsScrubInterval`), ensuring the backlog drains continuously.

## Files Changed

| File | Change | Lines |
|------|--------|-------|
| `go/internal/ml/news_narratives.go` | Removed `OR scrubbed_at IS NULL` from corpus query | -1 |
| `go/internal/ml/transfer.go` | Removed transition clause, updated comment | -5 +3 |
| `go/internal/maintenance/maintenance.go` | Removed `OR scrubbed_at IS NULL` from in_scope CTE | -1 +1 |

## Verification

- [x] Branch synced with `origin/main` before editing (`git fetch && git status` confirmed clean)
- [x] All consumer queries now require `vetted IS TRUE` (grep confirmed no `OR scrubbed_at IS NULL` in consumer code paths)
- [x] Maintenance scrub worker queries unchanged (intentionally find unvetted work)
- [x] Code compiles: `cd go && go build ./...` — PASS
- [x] All tests pass: `cd go && go test ./...` — PASS (11 tests)

## Result

The scrub precision pass is now **active and enforced**. Every article feeding the news rail pipeline has been vetted by local model as genuinely about the linked entity. This eliminates noise from fuzzy name matching (e.g., same-name disambiguation failures) and ensures the derived products (narratives, vibe, sigil) are built on a verified corpus.

**No breaking changes for clients** — this only affects the backend pipeline's internal data selection. The API contracts remain unchanged.

## Related

- [[Product Narrative]] — the compile→scrub→reveal pipeline this completes
- [[Backend Architecture]] — the two-rail model
- [[AI Architecture]] — local model as the scrub gate
- [progress_docs/2026-06-13_news-scrub-id-gate.md](2026-06-13_news-scrub-id-gate.md) — the scrub gate design
- [progress_docs/2026-05-02_vibe-corpus-mode.md](2026-05-02_vibe-corpus-mode.md) — corpus-driven mode foundation
