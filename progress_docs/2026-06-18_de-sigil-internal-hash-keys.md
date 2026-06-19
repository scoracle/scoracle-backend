# De-sigil the two parallel internal "sigil = strength" leftovers

**Date:** 2026-06-18  ·  Backend, **deployed** (rebuild + restart + stat_summaries re-stamp).

## Goal
Follow-up to the `divined_peak` rename ([[2026-06-18_divined-peak-rename]]). Two internal
"sigil = strength" names the divined_peak work deliberately left, both because each is load-bearing on
an `input_hash` gate — so the same **re-stamp** pattern is needed, not a drive-by rename.

## What Was Done
**(a) `rating.go` — the engine peak datapoint's input-component keys (HASHED → re-stamp).**
- Go fields `ratingProfile.sigilScore`/`sigilLabel` → `peakScore`/`peakLabel` (+ scan).
- `inputComponents()` map keys **`sigil_label`/`sigil_score` → `peak_label`/`peak_score`** — these are
  hashed into `stat_summaries.input_hash` (the rating debounce gate), so renaming them would
  re-generate the whole rating corpus (Gemma).
- **`RatingGenerator.ReStampPeakKeys`** + **`statcommentary -mode restamp`**: rewrite each entity-season's
  latest stored `input_components` (rename only the keys) + recompute `input_hash` via the canonical
  `hashComponents` — **no Gemma**, body/divined_peak/prompt_version preserved. Plus
  `statcommentary -mode single -skip-unchanged` to verify the gate.

**(b) `sigil.go` — the crown's P2-pillar internal naming (NOT hashed → pure rename).**
- `synthSigil` → `synthRating`, `loadSigilPillar` → `loadRatingPillar`, `sigilData`/param `sigil` →
  `ratingData`/`rating`, comments + the "sigil pillar" error → "rating pillar". The P2 pillar IS the
  Rating end product (`f(Vibe, Rating, Momentum)`). No map-key or prompt-text change → crown hash and
  output are byte-identical (no re-stamp, no regen). Also scrubbed `statcommentary` header "special = how"
  → "peak = how".

## Files Changed
`go/internal/ml/rating.go` · `go/internal/ml/sigil.go` · `go/cmd/statcommentary/main.go`.

## Verification
- `go build ./...` · `go vet` · `go test -count=1 ./...` (all pass).
- Deploy: rebuilt `scoracle-api` + `statcommentary`; `systemctl --user restart scoracle-api` (active in 1s;
  the API embeds the crown synthesis only — `NewRatingGenerator` is constructed solely in
  `cmd/statcommentary`, so part (a) only needed the rebuilt cron binary + the re-stamp before the 03:00 tick).
- **Re-stamp:** 1606 entity-seasons → **401 rewritten, 1205 noop, 0 fail** (noops = rows whose stored
  components predate the label key — mostly immutable prior seasons the nightly never re-evaluates).
- **Skip proof:** `statcommentary -mode single -skip-unchanged` on 3 current-season NBA entities
  (Bam Adebayo, LeBron, Grayson Allen) → all "unchanged — skipped", `duration=0s`, `prompt=s5`. Fresh
  hash (now `peak_label`/`peak_score`-keyed) matches the re-stamped stored hash → no spurious regen.
- API healthy post-restart: `/rating` + `/sigil` → 200.

## Result
The "sigil = strength" leftovers are gone from the internal hash surfaces too: the engine peak datapoint
is `peak_*` and the crown's stat pillar is the `Rating` pillar. "Sigil" now means only the crown
everywhere — wire, prompt, hash keys, and Go internals. Closes the de-sigil follow-up Scott queued.
