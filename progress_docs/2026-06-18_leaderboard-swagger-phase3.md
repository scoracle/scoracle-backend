# Leaderboard Vibe board + swagger cleanup + Phase 3 /sigil repoint

**Date:** 2026-06-18
**Scope:** Finish the Sigil convergence on the backend — the discovery-board model, the swagger gap, and the crown repoint.
**Commits:** `fb55f27` (vibes board), `ffcf659` (Phase 3 + swagger) — origin/main.

## What Was Done

- **`/leaderboard/vibes` → the Vibe board.** Repointed `vibes_leaderboard`'s `latest` CTE from
  `sigil_synthesis` → `vibe_scores`: `sentiment` as the score, `prompt` as the blurb. The board now
  ranks the **Vibe end product** (its only public surface) — not the crown synthesis. The four boards
  are Rating · News · Vibe · Transfers; the Sigil is a profile crown, not a leaderboard rank.
- **Phase 3 — `/sigil` serves the crown.** Route repoint (`server.go`): `/sigil` → `GetEntityVibes`
  (the synthesis); `/rating` → `GetEntitySigil` (the divined stat read); `/momentum` → `GetTrendsPage`.
  `/vibes` + `/trends` kept as deprecated aliases. The Sigil now lands at its canonical path.
- **Swagger (gap #8).** Added `@Router` for `/rating`, `/momentum`, `/sigil` (the new product paths had
  none); refreshed the GetEntitySigil (Rating), GetEntityVibes (Sigil crown), GetTrendsPage (Momentum)
  summaries/descriptions; de-staled `/special` + "Composite card" references; `swag init` regenerated docs.

## Verification

- `go build ./...` clean; deployed (rebuild + `systemctl --user restart scoracle-api`, healthy).
- Live: `/sigil` → synthesis (LeBron score 78 + blurb); `/rating` → composite_score 64.7 + strength
  "Playmaking" + commentary; `/leaderboard/vibes` ranks sentiment (Jaylen Brown 92…). Blurbs backfill as
  the v6 sentiment cron runs.

## Result

The backend Sigil convergence is functionally complete: the crown is at `/sigil`, the rails' end products
at `/rating` + the Vibe leaderboard board, the trajectory at `/momentum`, all documented. **Remaining tail
(cosmetic/non-blocking):** page-field purity (statement variants so `/sigil` emits `page:"sigil"` etc.),
the D1 `rating_sigil*`→`rating_peak*` wire alias, the R2 `vibe_trigger` NOTIFY rename, `ml/sigil.go`
internal vocab, and a rating-prompt strengths-emphasis tweak.
