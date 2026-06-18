# Backend cleanup — ml files/types renamed to Sigil vocabulary

**Date:** 2026-06-18
**Scope:** Pure cosmetic rename so the ml code matches the shipped product nouns. No behavior/wire change.
**Commit:** `d07ece6` (origin/main). No redeploy (live binary functionally identical).

## What Was Done

| Before | After |
|---|---|
| `ml/vibe_synthesis.go` · `SynthesisGenerator`/`NewSynthesisGenerator`/`VibeSynthesisRequest`/`VibeSynthesisResult`/`vibeSynthesisPromptVersion`/`vibeSynthesisSystemPrompt` | `ml/sigil.go` · `SigilGenerator`/`NewSigilGenerator`/`SigilRequest`/`SigilResult`/`sigilPromptVersion`/`sigilSystemPrompt` |
| `ml/sentiment.go` · `Generator`/`NewGenerator`/`SentimentRequest`/`SentimentResult`/`sentimentPromptVersion`/`sentimentSystemPrompt` | `ml/vibe.go` · `VibeGenerator`/`NewVibeGenerator`/`VibeRequest`/`VibeResult`/`vibePromptVersion`/`vibeSystemPrompt` |
| `ml/stat_commentary.go` · `StatCommentator`/`NewStatCommentator`/`StatCommentaryRequest`/`StatCommentaryResult`/`statCommentaryPromptVersion`/`statCommentarySystemPrompt` | `ml/rating.go` · `RatingGenerator`/`NewRatingGenerator`/`RatingRequest`/`RatingResult`/`ratingPromptVersion`/`ratingSystemPrompt` |

Clean trio: **VibeGenerator / SigilGenerator / RatingGenerator**. Prompt-version *values* unchanged
(v6 / s2 / s3) — only the Go identifiers. Internal helpers, the `divined_sigil` wire field, and the
prompt formats are untouched. Callers updated in lockstep across cmd/api, cmd/{pipeline,sentiment,vibesynth,statcommentary}, listener, api/handler.

## Verification

`go build ./...` / `go vet` / `gofmt` clean; grep confirms zero stale identifiers. Behavior identical,
so the running binary (D4, `666b9b4`) needs no restart.

## Deferred (paired with Phase 2 frontend cutover)

- **D1** `rating_sigil* → rating_peak*` read-layer alias — add it exactly when the frontend starts
  reading `rating_peak` (and drop `rating_sigil` then), to avoid carrying duplicate wire fields.
- Swagger annotation vocab + `swag init` regen (regenerates on next deploy).

## Result

The backend ml layer reads in the Rating/Vibe/Sigil vocabulary. Backend rename work is complete
except the cutover-coupled D1; next is **Phase 2 (frontend)**.
