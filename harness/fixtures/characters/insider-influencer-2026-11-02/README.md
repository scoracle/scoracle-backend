# Insider, Influencer and ten-article packets — 2026-11-02

The Journalist checkpoint was committed and pushed as `9d30f467`. This follow-up remains local for payload review. It implements the user's later ten-article cap and evidence-led paragraphs for Insider and Influencer.

## Current behavior

All article-consuming characters select at most **ten** articles using the decision model's corresponding score: Journalist `subject`, Insider `movement`, Influencer `emotion`, Scout `availability`. Zero is a score; missing/invalid scores rank below it. Scout's statistical evidence, Analyst's measurements and Oracle's finished cards retain their existing shapes. The decision model still evaluates one article at a time. Generation trials cover only Journalist, Insider and Influencer.

Insider writes as many concise paragraphs as the supported movement stories need. Influencer writes as many concise paragraphs as the supported reactions need, then assigns one overall vibe score, then writes one hook of at most 140 characters. Neither has a total sentence limit. Their existing `body` strings preserve blank lines, so no product-table migration or extra model pass is needed. Both allow up to 12,000 body characters and reserve up to 3,500 output tokens; these are ceilings, not length targets. The body-length allowance was increased at the user's request. All factual/name/quote/identity guards remain. Insider's separate exact-quote finding contract remains bounded to two findings. Its production temperature is now zero, matching the controlled runs.

The payload remains `instructions → target → new_content`. Voice stays in the task. Canonical identity and optional background come from the payload. Field meanings remain with their context providers and only appear when relevant. No general-purpose templating layer was added.

## Exact examples for review

- [Insider / Roma JSON](insider-roma.payload.json), [system instructions](insider-roma.system.txt), [complete native input](insider-roma.complete-input.txt): **nine articles, 1,954 input tokens**.
- [Influencer / Jerry Jones JSON](influencer-jerry-jones.payload.json), [system instructions](influencer-jerry-jones.system.txt), [complete native input](influencer-jerry-jones.complete-input.txt): **two articles, 1,321 input tokens**.
- [Both complete requests in one readable preview](prompt-preview.md).
- [Final raw outputs and human review](outputs.md), [structured review](review.json).

The system message accompanies the JSON; the token counts include both plus native chat framing. The separately transported decoding schema is retained in each request receipt. The examples contain the same target and selected evidence as the preceding ten-article batch.

## Controlled comparisons

All batches used local `Qwen3.5-4B-Q4_K_M` at `http://127.0.0.1:8081`, temperature 0, thinking disabled, concurrency 1 and no retries. This follow-up made **75 calls in four explicit batches**; none was a hidden retry. All responses and failures remain saved.

| Batch | Calls | Comparison | Native input tokens | Mechanical acceptance |
| --- | ---: | --- | ---: | --- |
| Baseline at pushed checkpoint, five articles | 17 | Frozen baseline | 17,211 | 10/17 |
| Short Insider/Influencer instruction revision, five articles | 17 | Identical target and selected evidence | 17,364 (+0.89%) | 13/17 |
| Ten-article cap, before paragraph request | 22 | Same underlying parts; selection intentionally changes; includes five Journalist cases | 23,179 → 24,404 (+5.28%) across matched cases | 16/22 |
| Evidence-led paragraphs | 19 | 17 identical-evidence pairs plus two authored multi-story controls | 18,293 → 19,164 (+4.76%) for the 17 pairs | 11/17 paired; 2/2 additional controls |

Current final requests range **859–1,954 tokens** for Insider/Influencer; the five current Journalist requests are **1,033–1,467**. They are below 4,096 native input tokens. This is not a hard total context setting: the trials retain 10,240 context tokens and reserve output separately. Ten unusually long source passages may still exceed admission; evidence is held rather than clipped.

For historical context, the seven matching development targets previously measured **32,070** native input tokens in total and now measure **10,768**, a **66.42% reduction** across those seven requests. This spans several refactors and selection changes, not an isolated instruction experiment. Individual values: Chelsea 9,929 → 1,467; Demichelis 4,247 → 1,283; Beal 4,956 → 1,033; Neal 3,507 → 1,925; Roma 4,372 → 1,954; De Zerbi 3,434 → 1,785; Jones 1,625 → 1,321. The earlier 29,705 figure was a total across twelve requests, not one request. Historical measurements are retained in the [five-article experiment](../top-five-2026-11-02/README.md).

## What the outputs show

The authored Insider control correctly separates an agreement and a denial into two attributed paragraphs and exact findings. The Influencer control separates positive stadium-opening and negative ticket-price reactions and assigns a supported mixed score of 50. Jones retains the improved negative score of 25, but the final reading omits the former-player criticism. Correct mood direction does not prove complete coverage.

Longer prose exposes remaining weaknesses. Roma adds injury-only paragraphs and misattributes an injury duration to Bulinews. De Zerbi now keeps Kulusevski and Kudus separate, but still turns hope into an asserted state and invents an explanation for an unspecified irony. Positive-only coverage still omits publisher names. Insider sometimes ignores the required target opening, expands source names or emits quotes that fail its full-name contract. One denial's prose chronology is supported by its supplied excerpt but rejected by the existing chronology guard; rejection is not automatically proof of fabrication.

The ten-article Chelsea case returned an accepted empty recap despite usable news. Beal produced different text from an identical native input at temperature zero; both responses are retained and rejected. A shared local server and deterministic sampling settings do not establish bit-for-bit reproducibility.

These are development observations, not held-out accuracy results. No general factual-quality gain is established. Parsers were not relaxed to turn these failures into passes. The new control's stricter agreement/source expectations were added to the canonical fixture after its one generation; [saved-output assertions](control-expectation-review.json) confirm them without another call.

## Database integration and rollout status

The writer had a five-article cap while the database froze up to forty pending deliveries. That mismatch could leave undelivered obligations after generation. Forward migration `325_ten_article_character_packets.sql` aligns the existing SQL selection policy with the shared Rust cap of ten for every article-consuming role. Ranked exclusions retain `headline-selection-v3`, `model_called:false`, rank, criterion, scores and cap receipts. They are never recorded as model-used. Existing duplicate, spam, identity and source fences remain; selecting again is a no-op. The baseline schema/checksum, recovery regression and read-only deployment verifier are updated. Historical migration 323 is unchanged.

The migration does not reset existing live assignments or prepared requests. Production was inspected read-only: the active edition was October 23, collection was closed, and the Mac character LaunchAgent was not loaded. Journalist had queued/failed work; Insider and Influencer had failed assignments without queued work. Deploying a prompt alone would not restart that backlog. No production write, deployment, restart, publication or model download occurred. A scoped recovery/release remains separate from this payload review.

The unrelated production-recovery plan and the separately appearing `fixtures/journalist/lfm3-comparison-2026-11-02/` work are untouched.

## Verification and replay

`recapture.py` regenerates the current 24 cases and 42 quality fixtures through the production assembler, verifies unchanged target/evidence for paired requests and checks the intended body-bound schema change. `recapture-ten.py` records the preceding ten-article export against its archived prompt versions. Old cases, native requests, token IDs and raw responses are not overwritten.

Build `character_package` and `character_outputs` offline. Run `recapture.py`; invoke `character_outputs` with a new output directory and the transfer/vibe files in `paragraph-cases/`, with `VOICE_NUM_CTX=10240`. Journalist's unchanged final cases are in `ten-outputs/`. Do not reuse an existing output directory. No database is needed for replay.

**Final checks pass:** `scripts/check.sh` on a fresh disposable PostgreSQL 18 database (including 73 explicitly enabled ignored tests, SQL/recovery checks, Go vet/build/statement validation/race tests), plus **424 passed / 74 ignored** with `cargo test --offline --lib --examples --bin eval`. Formatting and source diff checks pass. The final schema verifier passes, and the temporary database server is stopped. Verification receipts are in `verification.json`. Failed intermediate checks remain: initial local PostgreSQL shared-memory allocation, stale five-article/history test expectations, a non-isolated gender fixture, the archived-memory capacity assertion, and the new control's missing stage expectation. The tests were corrected to the actual contracts and isolated test database; production parsers were unchanged except the requested body length bound. The complete suite uses fresh disposable PostgreSQL 18 databases only.
