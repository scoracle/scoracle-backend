# Vibe prompt — the emotional end product gains its felt read (D4)

**Date:** 2026-06-18
**Scope:** Sigil convergence — make the Vibe a score **+ prompt** and feed it into the Sigil.
**Commit:** `666b9b4` (origin/main).

## Goal

In the Rating/Vibe/Momentum/Sigil model the Vibe (the emotional rail's end product) is a sentiment
score **plus a one-sentence "prompt" (felt read)** that feeds the Sigil. The `prompt` column shipped
in migration 093; this lands the local model emission + the synthesis wiring. No migration.

## What Was Done

**`ml/sentiment.go` (prompt v5 → v6):** local model now emits two lines — `SCORE: <1-100>` and
`VIBE: <one sentence>` — the felt read a fan would nod at. Persisted to `vibe_scores.prompt`
(NULL when empty). `parseSentimentAndPrompt` extracts both and **falls back to the first integer
anywhere for the score**, so model/format drift can never break sentiment generation.

**`ml/vibe_synthesis.go` (synthesis prompt s1 → s2):** the momentum loader now also captures the
latest `vibe_scores.prompt`; `buildSynthesisPrompt` feeds it to the Sigil as the emotional signal's
"felt read," and it's added to `input_components` (so the debounce hash reflects prompt changes).
Additive — the richer narrative pillar is retained, so crown-product quality is preserved while the
code now matches `f(Vibe, Rating, Momentum)`.

## Files Changed

- `go/internal/ml/sentiment.go`, `go/internal/ml/vibe_synthesis.go`

## Verification

- `go build` / `go vet` / `gofmt` clean.
- **End-to-end vs local Ollama (`local-model:tag`):**
  - `sentiment -mode single` (LeBron 237) → score **88** + `vibe_scores.prompt` = *"The Lakers are the
    favorite, but the drama surrounding old teammates keeps the speculation burning hot."* (v6).
  - `vibesynth` dry-run (LeBron) → score 78, blurb reflects the felt read (*"...intense speculation
    surrounding his contract and potential reunions..."*), `prompt=s2`.
- Deployed: rebuilt all binaries (api → `bak093`); `systemctl --user restart scoracle-api` (healthy;
  statements re-prepared cleanly; listeners connected). `/vibes`, `/rating`, `/momentum` 200.

## Result

The Vibe is now a complete end product (sentiment + felt-read prompt), and the Sigil consumes it.
New `s2` syntheses populate as the `vibesynth`/`pipeline` crons and lazy-view/triggers regenerate;
existing `s1` rows serve until then. **Remaining for the Sigil rename:** D1 `rating_sigil*→rating_peak*`
read-layer alias; cosmetic file/symbol renames; Phase 2 (frontend card rotation); Phase 3 (repoint
`/sigil`→synthesis, retire `/trends`+`/vibes`).
