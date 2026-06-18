# Sigil convergence rename — Phase 1 foundation (table renames + product routes)

**Date:** 2026-06-17
**Scope:** Backend foundation for the Rating/Vibe/Momentum/Sigil vocabulary rotation.
**Commits:** `44c9c34` (table renames), `d56c33e` (routes) — on origin/main.

## Goal

Bring the new product vocabulary to the backend (see wiki "Sigil" / "Product Narrative"): the two
rails refine into end products that converge into the Sigil. Phase 1 lays the safe, zero-observable
foundation; the Vibe prompt, `rating_peak` wire aliasing, cosmetic file/symbol renames, the frontend
cutover, and the `/sigil` repoint follow.

## What Was Done

**Migration `093_sigil_convergence_rename.sql`** (verified: no PL/pgSQL/view deps on either table):
- `vibe_synthesis → sigil_synthesis` (the crown synthesis = Sigil) + indexes.
- `sentiment_scores → vibe_scores` (emotional end product = Vibe; reverts 088) + indexes + constraint.
- `ALTER TABLE vibe_scores ADD COLUMN prompt TEXT` (D4 column; emission is a follow-up).
- Table comments updated. Engine rating columns untouched (late-bound — read-layer aliasing only).
- NOTIFY wire string stays `'vibe_trigger'` — now consistent with the Vibe product (L2).

**Go (lockstep, build can't see SQL strings):** every reference to the two tables updated across
9 files — `db.go`, `ml/sentiment.go`, `ml/vibe_synthesis.go`, `corpus.go`, `listener/news_volume_worker.go`,
`maintenance.go`, `cmd/{pipeline,sentiment,vibesynth}`. Verified zero stale refs via grep.

**Routes:** additive `/rating` (→ GetEntitySigil) and `/momentum` (→ GetTrendsPage) under the new
vocabulary. `/sigil` and `/trends` keep serving (transitional); repoint/retire in Phase 3.

## Files Changed

- `sql/migrations/093_sigil_convergence_rename.sql` (new)
- `go/internal/db/db.go`, `go/internal/api/server.go`
- `go/internal/ml/sentiment.go`, `go/internal/ml/vibe_synthesis.go`
- `go/internal/corpus/corpus.go`, `go/internal/listener/news_volume_worker.go`, `go/internal/maintenance/maintenance.go`
- `go/cmd/pipeline/main.go`, `go/cmd/sentiment/main.go`, `go/cmd/vibesynth/main.go`

## Verification

- `go build ./...` / `go vet` / `gofmt` clean; zero stale table refs after rename.
- Deployed to archbox: all 7 binaries rebuilt (api → `bin/scoracle-api.bak092`); migration 093 applied
  back-to-back with `systemctl --user restart scoracle-api`.
- Smoke (LeBron 237): `/vibes` → score 78 + blurb (reads `sigil_synthesis` ✅); `/trends` → sentiment
  series + 9 snapshots (reads `vibe_scores` ✅); `/rating`, `/momentum` 200 (new routes); `/sigil`,
  `/stats` 200 (no regression). `/health` healthy.

## Result

The new tables (`sigil_synthesis`, `vibe_scores` + `prompt`) and product routes (`/rating`,
`/momentum`) are live on `api.scoracle.com` with the JSON wire contract unchanged (zero-observable).
Foundation for the rest of the rename. **Deferred to the next increment:** Vibe prompt emission +
synthesis-loader re-pointing (D4 — ML change, needs Ollama verification); `rating_peak` wire aliasing
(D1, additive); cosmetic file/symbol renames; then Phase 2 (frontend) and Phase 3 (`/sigil` repoint).
Note: `/rating` and `/momentum` currently echo `page:"sigil"/"trends"` (alias artifact) — cleaned when
dedicated statements land in Phase 3.
