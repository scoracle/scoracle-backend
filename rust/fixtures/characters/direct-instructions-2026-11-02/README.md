# Plain briefs and one Journalist hook — 2026-11-02

Journalist now writes a recap with sourced paragraphs first, then one hook of at most 140 characters for the whole recap. The task explicitly requests the tone of a seasoned, reliable journalist, concise paragraphs, source attribution, useful direct quotes, and no invented details or gap filling. Insider and Influencer use the same plain assignment style for their existing movement and mood products. Scout, Analyst and Oracle remain unchanged in this pass.

The input remains `instructions → target → new_content`. Article handles are now local integers: `"id": 1`. Journalist output is:

```json
{
  "paragraphs": [
    {"body": "A concise paragraph naming its source.", "report_ids": [1]}
  ],
  "headline": "One hook written after the recap"
}
```

The native JSON schema preserves paragraphs-before-headline generation order. All five final Journalist outputs follow it. Each paragraph is validated against its own cited articles; the hook is validated against the union of articles actually cited. IDs must be integers in the selected range, unique within each citation list and nonempty. Old string keys and per-story headlines are not accepted by the new contract. There is no extra generation pass.

The existing product adapter produces one recap row: one title/hook, paragraphs joined with blank lines, and deduplicated internal source IDs with their existing metadata. Paragraph citations remain in the raw response/receipt. This uses the existing table and publication path; no database migration or production write was performed. The number of paragraphs is bounded by the selected article count, replacing the previous three-story limit. Each paragraph retains the 1200-character bound; the hook retains its 140-character bound. The generation allowance scales with article count (up to 3500 output tokens for five reports). Empty reporting returns `{"paragraphs":[],"headline":null}`.

## Reviewable payloads

- [Chelsea JSON](chelsea-live.payload.json), [system rules](chelsea-live.system.txt), [complete native input](chelsea-live.complete-input.txt).
- [Dak Prescott JSON](dak-prescott.payload.json), [system rules](dak-prescott.system.txt), [complete native input](dak-prescott.complete-input.txt).
- [All nine exact requests](prompt-preview.md), [before/after checks](parity.json), [raw outputs and manual review](outputs.md).

The human review standard is a short press packet: a reader with passing sports knowledge should understand the job, the subject, and what the supplied headlines establish. Readability is an input-design goal, not proof of factual output quality. The temporary five-highest-decision-score article limit remains. Fifty-headline capacity and quality have not been tested. Dates are not added to the writing payload, and the wording does not claim all queued articles were published today.

## Controlled trials

Qwen3.5-4B-Q4_K_M at `http://127.0.0.1:8081`, temperature 0, thinking disabled, concurrency 1, no retries. The final nine cases use exactly the source parts from the clear-payload trial. The final batch covers Journalist, Insider and Influencer only. It compares complete native token-ID counts, including system rules and chat framing; the decoding schema is transported separately.

Two earlier five-case Journalist batches explored the clearer brief under the old multiple-hook output structure. They remain under `initial-*` and `refined-*`, including failed outputs, prompts and receipts. The approved single-hook structure is the final `outputs/` batch. This pass made **19 model calls** (5 + 5 + 9) and reused nine saved baseline generations. These are three explicit instruction/contract batches, not retries. The old invocation paths in receipts describe paths as they existed at generation time; each receipt directory also retains the exact fixture that ran.

| Case | Baseline input | Final input | Validation |
| --- | ---: | ---: | --- |
| chelsea-live-live-journalist-preview | 1,224 | 1,182 | accepted → accepted |
| dak-prescott-live-journalist-preview | 1,188 | 1,146 | rejected → accepted |
| chelsea-target-preview | 1,213 | 1,171 | accepted → rejected |
| narratives-person-86-FOOTBALL | 1,322 | 1,283 | accepted → accepted |
| narratives-player-37-NBA | 1,069 | 1,033 | rejected → rejected |
| transfer-player-83946-NFL | 1,556 | 1,595 | rejected → rejected |
| transfer-team-37-FOOTBALL | 1,451 | 1,490 | rejected → rejected |
| vibe-person-23-FOOTBALL | 1,435 | 1,474 | accepted → accepted |
| vibe-person-959-NFL | 1,210 | 1,249 | accepted → accepted |

Complete native inputs total **11,668 → 11,623 tokens**, saving **45 (0.39%)**. The final range is **1,033–1,595 tokens**. Journalist alone falls from 6,016 to 5,815 tokens; the more explicit Insider/Influencer tasks add some tokens. This pass prioritizes clear assignments and the correct recap shape, not further large token reductions.

Mechanical acceptance is **5/9 both before and after**, with different cases passing. The new contract and task wording do not establish a general factual improvement. Chelsea still blends unrelated developments; the older Chelsea case changes the meaning of the charges; Demichelis adds an unsupported nationality and position; Beal invents a jersey retirement; Roma confuses a player with a coach and transfers a duration between publishers; Jones still calls two negative reactions mixed. Some errors are rejected, while others pass. Dak avoids the incompatible ranking windows by choosing different stories, but its coverage is narrower. See [structured manual review](review.json) and [per-case token receipts](input-tokens.json). No held-out accuracy claim or isolated causal claim is made: instructions, IDs and the Journalist contract changed together.

## Verification and replay

**422 tests passed, 74 ignored** with `cargo test --offline --lib --examples --bin eval`. New checks cover paragraph-local evidence, hook evidence restricted to cited articles, single-hook/abstention consistency, numeric citation validity, more than three paragraphs, native schema order, and the assembled recap's internal source mapping. Formatting and diff checks pass.

Production and fixture replay use the same assembly, preparation and parser path. All 61 requests were inspected: 38 requests for the three changed characters were regenerated, and 23 Scout/Analyst/Oracle requests stayed byte-identical. Source parts, source wording, selection and target data/guides are unchanged. Journalist schema and output allowance changed deliberately; Insider and Influencer changed only task text. The unrelated production-recovery planning document retains its initial hash.

Migration failures are retained: a legacy string-key test was accidentally converted to an integer, an old absence check conflicted with the new local `id`, and a malformed-body test still addressed `narratives`. These expectations were corrected; evidence guards were not weakened. The initially stalled check/build logs are also retained. Older multi-story experimental runners now fail explicitly against the new recap contract; use the current replay tool below.

```sh
cargo build --offline --example character_package --example character_outputs
python3 fixtures/characters/direct-instructions-2026-11-02/recapture.py
VOICE_NUM_CTX=10240 target/debug/examples/character_outputs /tmp/scoracle-recap-replay http://127.0.0.1:8081 Qwen3.5-4B-Q4_K_M fixtures/characters/direct-instructions-2026-11-02/cases/*.json
```

Use a new output directory and preserve the frozen baseline. See [verification](verification.json). Nothing was deployed, published or written to production.
