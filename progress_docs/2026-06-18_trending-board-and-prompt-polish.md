# Trending leaderboard board + Momentum→Trends + prompt/vocab polish

**Date:** 2026-06-18
**Scope:** New Trending board (risers), the Momentum→Trends surface naming, and the naming/polish tail.
**Commits:** `00d2275` (trending), `aff6fc5` (prompt/vocab) — origin/main.

## What Was Done

- **Trending leaderboard board** (`/leaderboard/trending`, `?board=trending`): entities ranked by the
  recent RISE (delta = latest − earliest) of their trajectory. `?metric=vibe` (default, `vibe_scores`
  sentiment over 21d) or `?metric=rating` (event `rating_composite_pct` over 60d). Risers only (delta>0),
  ≥3 points. Two prepared statements (`trending_vibe_leaderboard`, `trending_rating_leaderboard`) +
  `GetTrendingLeaderboard` + the `?board=` switch + swagger. Validated vs the live DB before restart.
- **Naming:** "momentum" stays the under-the-hood id/endpoint/component; it **surfaces as "Trends"**
  (frontend label only). The leaderboard's risers board is "Trending" — symmetric with the profile
  Trends card (one entity's trajectory ↔ the leaderboard's risers).
- **Rating prompt → strengths-first (s3→s4):** the stat-identity read now LEADS WITH and emphasizes the
  entity's greatest strength(s). Verified vs Ollama.
- **Sigil vocab (s2→s3):** the crown synthesis prompt/comments now say "SIGIL score"/"Sigil synthesis"/
  "Sigil card" (was "VIBE"/"Vibe card"). The synthesis IS the Sigil.

## Verification

- `go build` clean; trending SQL validated with literal params (vibe risers=61 top +60; rating risers
  computed) before restart; deployed (rebuilt api/statcommentary/vibesynth/pipeline, restarted, healthy).
- Live: `/leaderboard/trending?metric=vibe` (Jalen Green +60, Cason Wallace +56) + `?metric=rating`
  (Jaden McDaniels +92, Rudy Gobert +81.3); `/rating` + `/sigil` 200. s4 read verified strengths-first.

## Deferred (cosmetic, non-blocking — tracked)

- **D1** `rating_sigil*` → `rating_peak*` wire alias: still consumed by RosterCard + og-bodies, so it's a
  backend+frontend cutover for purely cosmetic wire-naming. Lowest value / highest coordination.
- **Page-field purity**: `/sigil` emits `page:"vibes"`, `/rating:"sigil"`, `/momentum:"trends"` — needs
  per-route statement variants; nothing keys on `page` for logic.
- **R2 `vibe_trigger`**: now consistent (`vibe_scores` IS the Vibe product; the channel fires the vibe
  cascade) — effectively a non-issue, no rename needed.
