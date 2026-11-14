# Influencer: 15-case prompt tuning

The user requested evaluation and tuning before resuming production, then a fresh 48-hour production observation window. The `com.scoracle.influencer` service was disabled and unloaded for this experiment; Journalist was left running. Previously published products and receipts were preserved.

## Fixed evaluation set

Eight teams and seven players across NBA, NFL and football. `manifest.json` identifies retained attempt IDs. `cases/` contains frozen v54 fixtures; `candidate-cases/` uses identical evidence with revised instructions. All cases were selected before inspecting baseline trial outputs. `rubric.json` records evidence-specific review expectations before scoring the runs. This deliberately challenging development set includes known failures and is not a random sample or independent holdout.

The team packets cover dense mixed reporting, strongly negative reporting, opponent reactions and a sparse statistics-only abstention. Player packets cover positive praise with an opposing emotional thread, negative reactions, neutral statistics, sparse explicit disappointment and publisher framing.

Model: Qwen3.5-4B-Q4_K_M on the existing localhost server. Temperature 0, thinking disabled, context 10000, output allowance 3500, unchanged native JSON schema and parser. Each arm uses the production preparation path in `character_outputs`; no output is published by this runner. The shared server also runs Journalist, so timing is not a controlled performance benchmark.

The baseline is `vibe-frame-v54-target-task-evidence`. The first revision is `vibe-frame-v55-valence-attribution`: make polarity explicit in the main assignment, separate emotional ownership and source attribution, avoid unspecified facts/roles, and aim for complete hooks under 100 characters while retaining the 140-character maximum. This changes instructions only, not ranking, evidence fields, model settings or publication logic.

Directories retain complete requests, raw responses, decoded outputs and mechanical verdicts. Mechanical acceptance does not establish factual quality. Manual reviews compare each output against its own supplied evidence without external fact checking.

## Context/target revision awaiting visual review

After user review, `vibe-frame-v56-target-context` removes behavioral guidance from Influencer's `fresh.rs`. The brief context now describes collected coverage, target-name search ambiguity, item contents and reporting bounds. Evidence mirrors deployed Journalist's numbered publisher headline/excerpt presentation; classifier annotations remain only in stored inputs/receipts. The existing Journalist target formatter was moved unchanged into shared metadata code and reused for Influencer. This preserves identity and dated recorded attributes while removing internal provenance IDs from the writing payload.

The v55 writing instructions are intentionally unchanged in this step. `context-preview/` contains complete real team/player payloads for visual review. No v56 model calls or deployment have been performed. The preceding v55 run completed 14 decoded outputs; Seahawks hit the 3500-token output limit, so it is an incomplete-generation failure, not a parser or factual pass.

## Approved metadata trial (v57)

The approved metadata wording was applied as `vibe-frame-v57-recorded-context`, removing the obsolete classifier-flag reference and permitting dated recorded attributes to inform identity/relationships without overriding reporting or supplying feelings. All 15 frozen cases ran with unchanged model settings and stored input parts. All completed mechanically; manual review found both improvements and regressions. See `RECORDED-CONTEXT-REVIEW.md` and `recorded-context-review.json`. Production remains paused pending visual review; no subsequent instruction edit has been made.

## Approved emotional-claim admission trial (v58)

Applied the user-approved claim qualification and scoring wording only. All 15 cases completed with unchanged evidence and model settings; manual review shows corrected Buccaneers/Tatum abstentions and Dak polarity, alongside persistent actor/event/outcome errors. See `EMOTIONAL-CLAIMS-REVIEW.md` and `emotional-claims-review.json`. Production remains paused and no additional instruction edits were made.

## Approved target-description trial (v59)

Added only the approved sports-entity context paragraph before target. Rust compilation stalled, so 15 offline calls used the same native transport with verified baseline token reproduction. All completed; the Rust build/parser remain unvalidated. See `TARGET-CONTEXT-REVIEW.md` for execution limits and the full comparison. Production remains paused; no further prompt edits were made.

## User relevance edits and production release (v60)

Preserved the user’s instruction and evidence-provenance context edits. A clean temporary Cargo target directory resolved the build blockage sufficiently to produce and test an isolated release. The complete 15-case retest used compiled production assembly and the Rust parser: all 15 passed mechanically. Library tests: 407 passed, 74 ignored. See `RELEVANCE-REVIEW.md`. Deployment and the fresh observation deadline are recorded in `docs/releases/2026-11-14-influencer-observation/README.md`.
