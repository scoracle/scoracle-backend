# Chelsea context budget: 32k capacity versus actual input

The old model servers allocated 32,768 tokens **per slot**. That was capacity, not a measurement of the evidence. Three character option builders also forced a minimum of 32,768 regardless of the configured window. Those floors are removed in this change. The proposed request cap is 7,900 tokens, including the reserved output, below the requested 8,096-token window. Production has not been restarted or reconfigured.

## Matched old evidence, same Qwen tokenizer

All rows below hold source wording and the old instructions constant. The comparison removes only classifier routing URLs, gates and scores from writing inputs; the complete metadata stays in the source receipts. Evidence scope, unresolved identity flags and semantic qualifications remain in the fresh model input. Memory prompts keep the full historical headline, publisher and date; their IDs, URLs and scores remain in receipts. Token differences include serialization boundaries.

| Character | Before | After metadata trim | Removed | Memories |
|---|---:|---:|---:|---:|
| Journalist | 10,568 | 2,033 | 8,535 | 0 |
| Influencer | 817 | 363 | 454 | 0 |
| Scout | 360 | 360 | 0 | 0 |
| Insider | 4,057 | 1,060 | 2,997 | 0 |
| Analyst | 230 | 230 | 0 | 0 |

Journalist’s matched comparison is the 24-headline diagnostic subset from the earlier run. URL removal alone saves 5,232 tokens; routing scores save 2,535 and gates 768. Together they consume 8,535 of 10,568 tokens (80.8%). The remaining 2,033 tokens contain about 300 tokens of identity, instructions and template, plus about 1,733 tokens of fresh evidence and serialization. Memories contribute zero in these Chelsea packets. These are Qwen counts; earlier Smol generation receipts used a different tokenizer.

## Full current Chelsea replay with new instructions

Continuous fetching grew the packet to 61 retained headlines. Production preparation retained **all 58 unique eligible headlines**, excluding only three exact-text duplicates. No headline was dropped for the byte budget, and the complete prompt plus output reservation fit the native tokenizer check. This uses the current working-tree prompt changes and Qwen for every stage. It is a new live-corpus observation, not a matched quality comparison with the older 24-headline run.

| Character | Input | Reserved output | Total budget |
|---|---:|---:|---:|
| Journalist | 4,636 | 2,100 | **6,736** |
| Influencer | 586 | 600 | **1,186** |
| Scout | 589 | 700 | **1,289** |
| Insider | 1,356 | 1,400 | **2,756** |
| Analyst | 323 | 700 | **1,023** |

Scout’s row is its largest of three correction attempts. Every call stayed below 7,900 tokens. Zero memories were selected in the live replay. A separate synthetic stress check added 20 dated memories within the existing 6,000-byte retained-history cap: input was 5,556 tokens and the total with 2,100 reserved output tokens was **7,656**, still under 7,900. The current quality fixture containing one historical headline also drops from 566 to 522 input tokens after the memory metadata trim. These memory checks use the live tokenizer and no generation; they establish budget fit, not model fidelity. Oversized future packets still fail closed rather than clipping source qualifications or denials. No acquisition, database schema or publication code changed.

## Remaining output defects

Token cost is resolved for this packet; semantic quality is not qualified. Journalist still grouped unrelated developments and treated Berekum Chelsea as Chelsea. Insider emitted a source-linked but unsupported Xabi Alonso transfer finding that passed its current mechanical parser. Scout exhausted three corrections and rejected the final prose. Analyst then described divergence despite a missing performance reading. These outputs and all rejected attempts are retained, so changing the model must not be presented as a factual guarantee.

## Checks and rollout state

350 Rust library tests pass, including complete Chelsea headline preservation, fresh and historical semantic qualification preservation and the existing no-clipping tokenizer boundary. All CLI unit checks and all-target compilation pass. Current quality fixtures were regenerated from their original evidence and new prompt versions; historical benchmark/capture files are unchanged.

The one-Qwen/four-slot configuration and evidence-only prompts are prepared locally. The production launcher, environment and model servers are unchanged while the output defects above are investigated. Concurrent inference and a natural nightly edition have not been qualified. A four-slot server would have a shared token pool near 32k in total, with each request bounded below 8k; that is distinct from the old 32k per-slot configuration.

See [memory-budget.json](memory-budget.json), [matched-token-profile.json](matched-token-profile.json), [input-budget.json](input-budget.json), the five `*-input.json` and `*-output.json` files, and [replay.rs](replay.rs).
