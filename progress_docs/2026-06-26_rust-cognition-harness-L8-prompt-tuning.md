# L8 — Prompt tuning for Mistral: the "beat reporter" voice

**Date:** 2026-06-26 · **Plan:** `scoracleWiki/wiki/Plan - Rust Cognition Harness build.md` §7 (L8)
**Follows:** L7 (the one-model Mistral cutover). Now that the model *obeys format*, re-aim every
derived product's prompt for quality + voice instead of wrangling a model that wouldn't listen.

## Goal & outcome

Tune the prompt + desired output for all five derived products on `mistral:7b`, strip the Gemma-era
defensive clamps, and unify the voice. **Outcome:** all five re-aimed and locked, validated, with a
new all-stage A/B harness — plus a real upstream data bug (false transfer signals) traced and fixed.

## The governing concept (user-led)

Mistral speaks as **the respected beat reporter a national broadcast calls in**; each product is a
different question the anchor asks:

- **vibe** → "what's the feel — the locker room and the fans?"
- **rating/peak** → "break down what this player/team *is*, statistically."
- **transfers** → "what's the latest — names and money?"
- **sigil** → "the big picture, including momentum" (the culmination of the three pillars).

Cross-cutting principles the user set: **invested but honest** (feelings shown, never invented);
**organic length by substance** (a busy entity earns more, a quiet one reads short — no rigid `<2`
clamps, but a real ceiling); **value-dense** ("our job is signal from noise" — no filler, no purple
prose); **specifics must be clear** (name the players/clubs/numbers). Deterministic math stays in
Postgres — the model only verbalizes.

## What changed, per product

| Product | Files | Ver | Shift |
|---|---|---|---|
| **vibe** | `ml/vibe.go` + `rust/src/vibe.rs` | v6→**v7** | The on-air "feel" (locker room + fans), mood-first, anchored to named specifics; dynamic length, 3-sentence ceiling. Dropped the `45-55 don't-invent-drama` deadener + the rigid `EXACTLY two lines / one sentence` clamps; kept grounding. |
| **rating** | `ml/rating.go` | s5→**s6** | Prompt now carries **value · percentile + a deterministic TIER (pctBand) · scarcity-z · per-x corroboration**. Reads the *absolute* percentile spectrum (not vs the entity's own marks), strength-led with breadth-in-one-stroke, honest mediocrity ("No standout skill"), no forced negatives, one organic paragraph. |
| **sigil** | `ml/sigil.go` + `rust/src/sigil.rs` | s4→**s5** | The beat reporter's "final word" — synthesizes **all three pillars** (identity + news + momentum), not just the loudest news thread; plain grounded prose, ~2 sentences, no headline/purple. |
| **transfer** | `ml/transfer.go` | t2→**t3** | Beat-reporter verdict that **names counterparties + capital** (fees, pick/asset compensation), attributed to one source; **liveness gate** (no stale/completed moves) + a **rivalry clause** (a player as opponent/draft-counter is not a transfer). Same-person disambiguation + JSON contract kept. |
| **narratives** | `ml/news_narratives.go` | n2→**n3** | Leaner scaffolding, more selective (consolidate, drop minor threads); beat-writer bodies; fixed the heat-list leak (was citing "the heat level") + a who-is-it-about guard. |

## Key technical decisions

- **The percentile→quality mapping is deterministic, in Go (`pctBand`).** Mistral kept judging a
  skill against the entity's *own* other stats (calling a 90th-pct mark "concerning" for a star, a
  37th-pct "above average" for a role player). The fix (user's diagnosis) was to compute the tier
  (`elite/strong/above average/average/below average/poor`, boundary at the 50th) in code and have
  the model *verbalize the labeled truth* — not map percentiles itself. This is transient
  prompt-shaping (like sigil's `trendDir`), not a stored derived stat.
- **Few-shot beats instructions for length/format.** A single invented-player example anchored
  rating's "one paragraph, short PEAK label" far better than prose rules.
- **Parity preserved by construction.** Only the system-prompt consts changed (vibe + sigil) — the
  `buildSentimentPrompt`/`buildSynthesisPrompt` builders and all loaders are untouched, and the
  versions bumped in lockstep — so the deterministic parity axes (built_prompt bytes, ollama_request,
  model_version, input_hash) hold. Verified by a byte-diff of the Go vs Rust prompt strings
  (vibe 2002 B, sigil 1574 B, both identical).
- **New tool — `go/internal/ml/promptab_test.go`** (gated on `PROMPT_AB`): the all-five-stage Go
  analog of `rust/bin/eval`. Builds the real production prompt for a stage+entity through the
  package's own unexported loaders/builders, runs the incumbent system prompt and an optional
  candidate (from a file) against live Ollama, prints both side by side. Read-only on the pipeline.
  This was the evidence engine for every change above. (Use `-count=1` — Go caches test results and
  the candidate file is read at runtime, not part of the cache key.)

## Upstream data bug found + fixed: false transfer signals

**Symptom:** an established star (Wembanyama) showed "NBA debut / draft race" across sigil + vibe.
**Trace:** his `transfer_rumors` carried `is_rumor=true` rows like *"OKC pursuing Wembanyama"* —
but every OKC↔Wemby co-mention is rivalry/game/draft-counter ("Aday Mara ready to be his stopper",
"Thunder address the Wembanyama problem", "Game 7 win over Thunder"). `loadCandidates` makes any
co-mention a transfer candidate; `compute_transfer_heat` can't tell rivalry from interest; the
**LLM vet (t2) is the only semantic filter — and it hallucinated the interest.**
**Fix:** the t3 transfer prompt (liveness + explicit **rivalry clause**) now reliably clears these
(OKC⟵Wemby → `is_rumor:false`) while keeping real trades (Wiggins *for two second-round picks*,
Holmgren, Jalen Williams). Verified live on the OKC candidate set. *The prompt was the right upstream
lever — co-mention surfacing is intentionally noisy; the vet is the designed filter.*

**Stale data:** 740 currently-served heat rows are all old-prompt (528 t2 / 72 t1 / 140 heat-v1 / 0
t3). Cleanup = re-vet (append-only, latest-wins) via `cmd/transfer -mode corpus` over the 141 teams
with active candidates — kicked off in the background this session.

## Gate

`gofmt` clean · `go build ./...` · `go vet ./internal/ml` · `go test ./internal/ml` pass ·
`cargo build` · `cargo test --lib` 35/35 · `cargo clippy --all-targets -- -D warnings` clean ·
vibe + sigil prompts byte-identical Go↔Rust.

## Loose ends (carry)

- **Cross-machine durability** (from L7, still open): the Mistral cutover is an archbox `.env.local`
  override; pull `mistral:7b` on **archx220** + flip the committed default before its next pull.
- **Transfer re-vet** running in the background — confirm it drained and the served heat is clean
  (no `is_rumor=true` rivalry rows) next session.
- **Rating length** is the softest axis: a generational profile occasionally still runs ~3 short
  paragraphs at temp 0.6 despite the one-paragraph + few-shot guidance. Acceptable (the user wants
  stars richer), but the lever if it drifts is a tighter few-shot or a lower `NumPredict`.
- **L7's two-model scheduler stays shelved**; the Rust scrub cutover stays HELD.

## Quick reference — the A/B harness

```bash
# from repo root (loads .env.local creds)
export DATABASE_PRIVATE_URL=$(grep '^DATABASE_PRIVATE_URL=' .env.local | cut -d= -f2-)
export OLLAMA_BASE_URL=http://localhost:11434 OLLAMA_MODEL=mistral:7b
cd go
PROMPT_AB=1 AB_STAGE=rating AB_ENTITY="player:56677822:NBA" \
  AB_CANDIDATE=/path/to/candidate_system_prompt.txt \
  go test ./internal/ml -run TestPromptAB -v -count=1 -timeout 600s
# AB_STAGE ∈ vibe|narratives|rating|sigil|transfer · AB_ENTITY=type:id:sport (transfer wants a team)
# omit AB_CANDIDATE for incumbent-only recon · AB_TRANSFER_N caps transfer pairs
```
