# Optimization Ledger O19 — Sigil leaderboard board

**Date:** 2026-06-19 · Backend (Go API; new prepared statement + handler + route; **service restarted**, prepared-statement validation passed at boot). No migration.

## Goal
The Product Narrative wants the front door to stack-rank on **Rating · Vibe · Sigil**. The backend
had Rating, Vibes, News, Transfers and Trending boards — but no **Sigil** board, even though
`sigil_synthesis.score` and the purpose-built partial index `idx_sigil_synthesis_sport_score`
(`(sport, score DESC, generated_at DESC) WHERE score IS NOT NULL AND blurb IS NOT NULL`) already
existed *for* it. O19 closes that gap.

## What Was Done
- **New prepared statement `sigil_leaderboard`** (`db.go`). Mirrors `vibes_leaderboard` exactly —
  `DISTINCT ON (entity_type, entity_id)` latest scored row from the append-only `sigil_synthesis`,
  enriched with `name`/`image`/`team_*` (one row shape across every board), ranked by `score DESC`.
  Adds `previous_score` (native to the Sigil synthesis) so the front door can render the crown's delta.
  `$1 sport · $2 limit (NULL⇒50) · $3 entity_type (NULL⇒both)`.
- **`GetSigilLeaderboard` handler** (`data.go`) — mirrors `GetVibesLeaderboard`; full swaggo annotations.
- **Board dispatch** — added `case "sigil"` to `GetLeaderboard`'s `?board=` switch and to the
  `INVALID_BOARD` allow-list (`rating, vibes, sigil, news, transfers, trending`).
- **Route** `GET /api/v1/{sport}/leaderboard/sigil` (`server.go`), alongside the other `/leaderboard/{board}` routes.
- **`ENDPOINTS.md`** — documented the board + JSON shape.

## Files Changed
- `go/internal/db/db.go` — `sigil_leaderboard` statement.
- `go/internal/api/handler/data.go` — `GetSigilLeaderboard` + dispatch.
- `go/internal/api/server.go` — route.
- `ENDPOINTS.md` — board doc.

## Verification
- Statement PREPARE'd cleanly against prod before deploy (boot-equivalent validation).
- Population (latest-per-entity, scored): NBA 47p/1t, NFL 114p/32t, FOOTBALL 57p/84t.
- Post-restart (port 8000): `/leaderboard/sigil?entity_type=player` → Giannis 95, Franz Wagner 90,
  Karl-Anthony Towns 88. `?board=sigil` dispatch → football top = Paris. Sibling boards
  (vibes/news/rating) still HTTP 200 (no regression). Health 200.

## Result
O19 ✅ shipped + deployed. The front door can now stack-rank on the Sigil crown (web/iOS consume next).
Next: O1 (momentum cohort precompute, with equivalence harness), then the Twitter/RSS decommission pass.
