# CLAUDE.md — owned-data / no-passthrough doc lock

**Date:** 2026-06-19 · Backend (docs only).

## Goal
Reflect the fully-owned-data model in the backend conventions: every serving endpoint is a precomputed
read; the third-party (Google RSS) compile lives in the background pipeline, never on a request.

## What Was Done
- Architecture / endpoint list: reframed "Third-party integrations (news + journalist tweets)" → a
  precomputed derived-product surface (`/news`, `/transfers`, `/rating`, `/momentum`, `/sigil`), with a
  "no third-party call on a serving request" statement; the local model pipeline (which *does* call RSS, off
  the request path) noted under background workers.
- Route conventions: the `/api/v1/news/...` (live-RSS) and `/api/v1/twitter/...` routes are now marked
  **legacy/being-retired** and **PARKED** respectively.
- Env: dropped the unused `NEWS_API_KEY`; added `DB_POOL_MAX_CONNS` (default 25, eager fan-out); moved
  the `TWITTER_*` vars under a "Parked" note.

## Files Changed
`CLAUDE.md`.

## Verification
Docs only — no build/migration. (Phase A behavior changes were committed separately in `195c8f1`.)

## Result
Backend conventions state the owned-data, no-passthrough model; the live-RSS + Twitter serving surface
is documented as legacy/parked ahead of its removal (ledger O12–O15).
