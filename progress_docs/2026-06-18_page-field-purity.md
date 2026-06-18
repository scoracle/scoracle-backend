# Page-field purity — each endpoint self-describes its canonical route

**Date:** 2026-06-18
**Commit:** backend — deployed (rebuild + restart).

## Goal
The per-entity product endpoints emitted a stale `page` value left over from the Sigil rotation
(`/sigil`→"vibes", `/rating`→"sigil", `/momentum`→"trends"). Make each emit its own canonical name.

## What Was Done
Since the Phase-3 repoint, each statement now serves ONE canonical route, so the fix was just the
hardcoded `page` literal in `db.go` (no new statement variants needed):
- entity_sigil (serves `/rating`): `'page','sigil'` → `'page','rating'`
- entity_vibes (serves `/sigil` crown, `/vibes` alias): `'page','vibes'` → `'page','sigil'`
- trends (serves `/momentum`, `/trends` alias): `'page','trends'` → `'page','momentum'`

## Verification
`go build` clean; deployed (restart, healthy). Live: `/rating`→page "rating", `/sigil`→"sigil",
`/momentum`→"momentum". Deprecated aliases (`/vibes`,`/trends`) return the canonical page — correct.
Nothing keys on `page` for logic (frontend `.page` grep = type decls only), so zero behavior change.

## Result
The wire now self-describes the route — closes the page-field item from the Sigil-convergence tail.
