# `divined_sigil` → `divined_peak` — de-sigil the local model peak-strength label (Item A)

**Date:** 2026-06-18  ·  Backend, **deployed** (migration 094 + API restart + re-stamp).

## Goal
Close Item A of the [[Handoff - divined_sigil rename + OG meta fix]] — the last "sigil = strength"
leftover. `stat_summaries.divined_sigil` is the local model-divined peak-strength label (e.g. "Playmaking"),
the Rating card's hero label. Post-convergence "sigil" means ONLY the crown, so this column / wire /
prompt rename to **`divined_peak`** (parallel to the shipped `rating_peak*` D1 rename). A real stored
column → coordinated rename across SQL + Go + the crown synthesis hash + frontend.

## What Was Done
- **Migration 094** — `ALTER TABLE stat_summaries RENAME COLUMN divined_sigil TO divined_peak`
  (idempotent `DO`-block; reverse noted in-comment). No SQL function/trigger/view references it
  (local model-written, not engine-computed) → plain rename, no L1 late-binding risk.
- **`ml/rating.go`** (writer) — `DivinedSigil`→`DivinedPeak`, INSERT col `divined_peak`, parser
  `parseSigilCommentary`→`parsePeakCommentary`. Prompt marker `SIGIL:`→`PEAK:` (prompt s4→**s5**); the
  parser still accepts the legacy `SIGIL:` prefix for in-flight responses. (Regen gate keys only on the
  input-data hash, NOT the prompt version, so the bump forces no mass regen.)
- **`ml/sigil.go`** (crown) — reader `SELECT divined_peak`; struct field + prompt section
  "SIGIL IDENTITY"/"Sigil:" → "PEAK IDENTITY"/"Peak:"; **the input-component key
  `out["divined_sigil"]`→`out["divined_peak"]`** (Plan A — see below); `sigilPromptVersion` s3→**s4**.
- **`db/db.go`** — the `/rating` commentary `row_to_json` selects `s.divined_peak`, so the public wire
  key follows the column name automatically.
- **Plan A re-stamp** (`ml/sigil.go` `ReStampDivinedKey` + `cmd/vibesynth -mode restamp`) — the crown's
  input-component key is hashed into `input_hash` (the synthesis debounce gate), so renaming it would
  change every entity's hash and re-synthesize all 381 Sigils (local model recompute, score/blurb drift).
  Instead, a one-time pass rewrites each existing row's STORED `input_components` (rename only the key)
  and recomputes `input_hash` via the canonical `hashComponents` — **no local model**, scores/blurbs/`prompt_version`
  untouched. Operating on stored (not fresh) components keeps it a pure vocabulary migration: unchanged
  entities still skip; genuinely-changed ones still regenerate.
- Added `cmd/vibesynth -mode single -skip-unchanged` (verify the gate for one entity without a local model call).

## Files Changed
`sql/migrations/094_rename_divined_sigil_to_divined_peak.sql` (new) · `go/internal/ml/rating.go` ·
`go/internal/ml/sigil.go` · `go/internal/db/db.go` · `go/cmd/vibesynth/main.go`.

## Verification
- `go build ./...` · `go vet` · `go test ./...` (5 pkg, 0 fail).
- **Deploy (lockstep):** 094 applied → rebuilt → `systemctl --user restart scoracle-api` (active in 1s,
  proving every prepared statement validated against `divined_peak`).
- **Re-stamp:** 381 targets → **110 rewritten, 271 noop, 0 fail** (noops = syntheses generated when the
  entity had no stat label yet — no key to migrate).
- **Wire:** `GET /api/v1/nba/player/237/rating` → `commentary.divined_peak = "Playmaking"`,
  `divined_sigil` absent (data preserved through the rename).
- **Skip proof (Plan A):** `vibesynth -mode single -skip-unchanged` on 4 re-stamped entities across all
  sports (LeBron 237, Bam Adebayo 4, Bernardo 32666 FOOTBALL, Emmanwori 13880574 NFL) → all
  "unchanged — skipped", `duration=0s`. The live fresh hash matches the re-stamped stored hash → the
  key rename causes **zero** spurious re-synthesis.

## Result
The peak-strength label is `divined_peak` end-to-end (column, wire, prompt, crown hash); the 381 existing
syntheses are preserved verbatim and skip on the gate. "Sigil" now means only the crown everywhere users
or the wire can see. Closes the Sigil-convergence tail (with Item B, the metaBody OG fix, on the frontend).

## Follow-up (logged, out of scope here)
Two parallel "sigil = strength" internals remain, each load-bearing on a hash (same re-stamp pattern
needed if ever renamed): `rating.go` `inputComponents()` keys `sigil_score`/`sigil_label` (the engine
peak datapoint, feeds the *rating* `input_hash`), and the crown's internal `synthSigil`/`loadSigilPillar`
naming.
