# Cleanup #1 — remove the dead leaderboard scope value 'sigil'

**Date:** 2026-06-18  ·  Backend, deployed (restart).

## Goal
The leaderboard scope filter accepted `'sigil'` (the old "peak board" selector, sorted by
`rating_specialist`). Post-Sigil-convergence the rail is Rating·News·Vibe·Trending·Transfers and the
frontend only ever sends `scope="composite"` or `"fantasy"` — `'sigil'` is unreachable.

## What Was Done
`db.go`: `req.scope IN ('composite','sigil','fantasy')` → `('composite','fantasy')` (player board) and
`IN ('composite','sigil')` → `IN ('composite')` (team board). Comments corrected
(`leaderboard.server.ts`, `data-sources.ts`).

## Note
If a "peak board" is ever wanted, re-add it as `scope='peak'` (consistent with `rating_peak*`), not
`'sigil'` (now exclusively the crown).

## Verification
`go build` clean; deployed; `/leaderboard` (composite) 200. No behavior change (removed an
unreachable branch).
