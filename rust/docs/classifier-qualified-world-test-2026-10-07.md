# Qualified Classifier world — October 7, 2026

**Implemented and tested the next requirement: attach source-backed speaker, subject, target, time and qualification to each claim before expression.** The useful packet retains complete supporting source paragraphs, selects the character's claims, and keeps numeric classifier scores and review bookkeeping in audit receipts. This improved specific denial, correction and attribution controls. No articulator is promoted: real-report and control errors remain.

This tests the compiler and expression boundary using **provisional AI source annotations**. It does not establish automatic relationship extraction, classifier calibration or independently adjudicated accuracy. Additional heads, real gold labels and production Classifier cutover remain pending.

## What changed

The existing [review tool](../examples/classifier_review.py) now validates literal claim and qualifier spans against the original UTF-8 source, with a body hash, candidate target, named reviewer and complete-source/extraction decisions. It rejects altered quotes, duplicate claim spans, unsupported target links, missing denial/time/correction evidence and invalid dimensions. Literal matching cannot prove that the annotated relationship is semantically correct.

The existing [articulation replay](../examples/classifier_articulation.py) compiles a qualified variant. Each selected emotion or emotional-denial claim carries its speaker, subject, target relationship, time scope and exact qualifications. Only selected dimensions from overlapping original document windows enter the audit packet; all 28 raw scores and the complete original text remain retained and hash-bound. These selected scores still measure document text, **not the speaker's emotional intensity**.

The expression view contains source wording, named relationships, relevant qualifications and complete supporting source paragraphs. Model/checkpoint hashes, numeric scores, review status, candidate-dimension labels and null bookkeeping fields remain outside prose input. Missing attributes stay unknown in the audit record; removing null fields from the reader does not supply new values.

The first compiler selected a corrected quotation but dropped its antecedent. Models consequently treated the correction as withdrawing the current worry. The fix retains paragraphs containing the claim and its supporting qualification/identity spans. A regression check requires the complete original correction paragraph, including the withdrawn relief claim. This also restored the missing football-role context in the real report.

Complete reviewed sources with no eligible emotional claim produce a native abstention without a model call. Unresolved emotional target links instead fail for review. This is selection for the **emotion-focused character test**, not a binary Classifier gate that deletes reporting. The Melton availability source and its measurements remain useful to other characters even though this reviewed packet supplies no emotional claim.

## Test scope

- Fourteen unchanged sources: twelve fictional controls and two previously inspected retained reports. The new controls cover conditional future feeling, denial of a statement, correction, and current hope about a future event.
- Fourteen complete Horizon emotion receipts, spanning seventeen source windows. The four new controls were measured with pinned checkpoint `1bc9627f189e5aa08b00cf525c3c86052067b81e`; the ten earlier inputs retain their exact text and Rust window ranges.
- Nineteen provisional claims in fourteen source reviews. Nine sources supply selected emotional evidence; five supply none for this expression task.
- The same installed SmolLM3, SmolLM2 and Qwen3 candidates, model digests, temperature 0, seed 42, context 4,096, output budget 400 and disabled thinking. No repair model, publication, new dependency or Harvester/Editor call.

The experiment retained four stages rather than overwriting failures:

| Stage | Comparison slots | Actual model calls | Native abstentions | Structural errors |
|---|---:|---:|---:|---:|
| Source-only, raw spectrum, first qualified packet | 126 | 111 | 15 | 3 |
| Same qualified worlds/instructions, cleaned reader input | 42 | 27 | 15 | 0 |
| Same reader input, shorter instruction | 42 | 27 | 15 | 1 |
| Same shorter instruction, repaired supporting context | 42 | 27 | 15 | 0 |
| **Total** | **252** | **192** | **60** | **4** |

The three initial structural errors were SmolLM2 raw-spectrum replies at the output limit; the short-instruction stage also produced one malformed SmolLM2 reply. All failed requests/responses remain retained. The final run has **27 structurally valid model replies plus 15 native abstentions**. Those abstentions are code behavior using provisional reviews, not model accuracy successes.

The paired audit worlds, model digests and options match across the cleaned-reader and instruction tests. Cleaning holds the instruction fixed; shortening holds the model's reader input fixed. The final context repair also omits unknown bookkeeping fields, so its result does not isolate paragraph context from that omission. No sports accuracy percentage is claimed.

## Findings that matter

- **Raw probability vectors are poor prose input.** The first qualified baseline described a tiny excitement probability as a player's low excitement, and historical optimism as an emotion percentage. SmolLM2/Qwen turned provisional review or unknown-time metadata into story uncertainty. The final reader contains neither raw scores nor review status.
- **Literal quotations need their antecedents.** A correct excerpt with “that claim was corrected” becomes ambiguous when its earlier claim disappears. The repaired compiler preserves the source context; final SmolLM2 correctly retained worry and withdrew relief. Qwen still incorrectly turned the initial reporter claim into Alex expressing relief today.
- **Small models benefit from the supplied world.** Final SmolLM2 preserved one named person's relief, explicit denial, old interview timing, sarcastic attribution, correction and current hope about a future match. It still produced an odd two-speaker headline and an unsupported return-to-practice claim in the real report. These are diagnostic observations, not a deployment win.
- **The 3B baseline is not a quality ceiling.** Earlier compact inputs lost identity context and its football output invented an opponent/roles. Supporting paragraphs restored the correct quarterback/opponent relationships. A stronger negative emotional headline and an unselected other-team reaction still appeared in control outputs.
- **Source identity and relationships remain necessary.** Final Qwen's real-report response preserved the team hope and player's denial without inventing roles, but other controls gained accomplishment, misplaced the object of worry, or assigned the withdrawn feeling to its subject as a current utterance. No candidate is consistently faithful yet.

Native abstention covered the routine schedule, other entity, conditional future feeling, denied statement and availability-only report. The full corpus and vectors stay available; this selection does not determine relevance/topic routing for the other character plugins.

## Verification and operational state

All **five Python contract checks pass on Archbox**; local Python skips only the torch-dependent check. They cover source bounds, review/head integrity, unknown-label masking, quote/target/time/denial binding, selection and context preservation. The four focused Rust source-tool tests pass, and all 400 existing review packets still validate. Their training/evaluation eligibility is unchanged; no new real-source head was fitted.

Every recorded prompt plus its reserved 400 output tokens stayed below 4,096: maximum **3,736** across all stages. Final qualified maxima were 576/612/558 input tokens for SmolLM3/SmolLM2/Qwen, respectively. Complete paragraph context can grow on other articles; native token-budget validation is required before production integration. This run validates these fourteen inputs, not arbitrary long sources.

The cognition worker was briefly paused during model comparisons with an exit handler restoring it. Cognition, API and Laya services reported active after every stage. Production routes/configuration were unchanged; this is not the intake cutover.

The [source-free results and provisional review](../fixtures/classifier/qualified-world-test-2026-10-07.json) retain counts, model digests, file/prompt hashes, timings and final fictional outputs. Full publisher source, claim spans, packets, requests and responses remain private under Archbox `/mnt/data/backup/scoracle/classifier-launch/2026-10-07/qualification-v1/` and the local counterpart `/private/tmp/scoracle-classifier-launch-20261007/qualification-v1/`. Subdirectories `reader-v2`, `reader-v3` and `context-v4` preserve the follow-ups. Private sources are not added to Git.

## Next requirement

The compiler is ready for further offline evaluation. The unproven part is **producing accurate qualified claims automatically**. Independently review source extraction and relationships in the 400-source packet, fit/compare the missing presence heads, and validate span/relationship extraction separately. Presence vectors alone cannot identify who said what. Repeat the expression tests on a held-out real-source cohort before choosing an articulator or completing the independent Classifier cutover. Native token-budget and publication-contract checks belong in that integration; the missing target ordinals must remain unknown until measured.
