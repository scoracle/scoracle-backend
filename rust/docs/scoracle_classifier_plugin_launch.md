# Scoracle Classifier Plugin

## Launch concept: measure a spectrum, then let characters express it

**Status:** Qualified claim-packet compiler tested across 14 sources and three articulators; 192 model calls retained, no model promoted. Automatic qualification, gold labels, additional heads and production cutover remain — October 7, 2026

**Placement:** Immediately after RSS fetch  
**First goal:** Replace generation-based gatekeeping with evidence-linked, graded signals  
**Model strategy:** Start with compact task specialists; evaluate a roughly 149M encoder before considering larger models

---

## Execution ledger

The tracked plan is this file; the original was supplied from Downloads. The user's launch instruction authorizes replacing Harvester and Editor directly on `main`; protecting the broken production flow with a prolonged parallel rollout is not a requirement. Model quality, source integrity, uncertainty, coverage, and reproducibility still need evidence.

- [x] Preserve current development and consolidate it onto `main`, pushed through `7f49d5a2`. Correct five stale regression checks; 384 active Rust tests pass (65 database/model checks remain ignored without their dependencies).
- [x] Trace current intake: Go RSS provenance → Harvester headline gate → Editor acquisition/windowed Laya scores → source-bound classifications and character assignments. Existing source receipts, canonical IDs, worker claims, and transactional publication are reusable. Current routing is uncalibrated and the source/model experiments did not establish factual fidelity.
- [x] Phase 1: source-bound 40-item replay of three compact emotion encoders; matched four-model comparison on 20 opening windows plus eight synthetic controls. Full vectors, exact input windows, checkpoint identities, timings and limitations retained.
- [x] Exclude Laya from the launch bank at the user's instruction; preserve its historical comparison receipts. The live Harvester/Editor runtime is unchanged until Classifier cutover.
- [x] Map all six character plugins' actual material contracts; define 51 independent presence dimensions, three ordinal annotations and exact-span claim qualifiers. Prepare and validate a source-only 40-item review pilot with all annotations unknown.
- [x] Export and prepare 400 retained sources (314 publishers, 3,699 complete source windows) for Phase 2 review.
- [ ] Phase 2: independently reviewed 300–500 item gold set, with source-group/time splits and ambiguous/unknown labels.
- [ ] Train and compare the additional heads on Horizon/SamLowe backbones; retain the existing emotion checkpoints as baselines. Evaluate each signal family before choosing a deployment checkpoint.
- [ ] Phase 3: held-out quality/calibration, real-corpus CPU/GPU throughput, and evidence/attribution checks.
- [ ] Replace the two intake stages with one Classifier stage; retain versioned measurements separately from routing and source evidence. Run source/publication checks and a direct production cutover when the selected slice works.
- [x] Complete an independent 60-call Classifier-to-expression diagnostic with source-bound emotion vectors; compare source-only and spectrum inputs on eight controls plus two retained reports across three articulators. Preserve failed replies and AI provisional semantic review; no model promoted.
- [x] Bind claim relationships and qualifications to exact source spans; select emotional evidence independently of Harvester and keep full score/source receipts.
- [x] Test source-only, raw-spectrum and qualified packets on 12 fictional controls plus two retained reports; inspect fidelity and distinguish model replies from native abstentions. Preserve 192 calls, 60 native abstentions and four failed replies across four stages; fix lost correction/identity context.
- [ ] Complete fidelity review and real-source evaluation before promoting an articulation model or replacing production intake.

First implementation decision: reuse the existing retained-source export and Python/PyTorch model tooling for the Phase 1 bench. No classifier framework, generated summaries, invented calibration mapping, or automatic emotional interpretation from score maxima.

October 7 progress: exported 40 canonical retained articles (35 publishers; FOOTBALL, NFL and NBA) through a read-only production transaction. The source-only sample includes injuries, transactions, recaps, opinions, non-English reporting, galleries, statistics and quiet/low-relevance items; it is not a gold or representative accuracy set. Added eight synthetic attribution, negation, mixed-emotion, time, unrelated-entity and sarcasm controls. The three emotion encoders receive all 374 real-source windows plus the controls. The matched Laya comparison uses the exact first production window of 20 articles plus the same controls; excerpt scope, original body hashes and byte ranges are explicit, and excerpt coverage is not called full-article coverage.

The user requested a Laya comparison. The installed English Laya checkpoint (`55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851`) is included with 28 independent emotion predicates and continuous scores. Current gatekeeper cutoffs are excluded from this bench. It is compared with Horizon small, ModernBERT-GoEmotions and the SamLowe RoBERTa reference; the latter is MIT and is an evaluation reference, not a deployment selection.

The initial token-overflow replay explicitly failed complete-source checks on 32–33 items per encoder. Its failed receipts are retained in the isolated run directory. The comparison initially reused `harvester::cognition::prepare_text`. Windowing now lives in `tools::source::windows`; Classifier imports the source tool directly, validates its exact UTF-8 spans and tokenizes each shared window without truncation. Coverage failures remain errors, never zero scores. Archbox's GTX 1070 Ti exists, but its installed PyTorch CUDA build excludes `sm_61`; the first comparison uses CPU with two threads and records that GPU limitation.

Code and raw run directory: `examples/classifier_{export.sql,windows.rs,replay.py}` and `/mnt/data/backup/scoracle/classifier-launch/2026-10-07/` on Archbox. `examples/test_classifier_replay.py` verifies original Unicode byte ranges, refuses omission of a late qualification and checks trained label identities, target scope and review evidence. The archived probe retains the corrected neutral question descriptions. Completed results and the evaluation decision follow below.

Bounded-comparison decision: the initial complete-article Laya run took 57,409 ms and 89,464 ms for its first two articles with 28 independent emotion questions. It was stopped with completed receipts preserved, rather than extrapolating a production throughput claim or requiring an hours-long experiment before inspecting any quality. This checkpoint contains 421,293,830 stored weight elements, so it is not the proposed 149M specialist. CPU, two threads, precision, question wording and model revisions are retained; GPU execution fails a measured kernel probe with the installed CUDA build. The user specifically requested comparison rather than a predetermined encoder or LLM choice.

Phase 1 completed: [measured comparison and recommendation](classifier-spectrum-comparison-2026-10-07.md). Verified 144 full-source encoder receipts and 112 matched receipts: complete declared input, 28 finite scores, no truncation or errors. Matched median CPU latency per real opening window was 159 ms Horizon, 254 ms SamLowe, 321 ms ModernBERT-GoEmotions and 24,827 ms for the English Laya 28-question bank. These are measurements of this configuration, not daily-corpus throughput or model accuracy. The three specialists took 60.22, 97.82 and 136.37 seconds respectively for the 40 complete retained sources.

Laya can measure continuous predicates and ordinal intensity without text generation. Its baseline emotion questions nevertheless scored explicitly denied joy/relief highly. A separate five-question probe improved relief negation when scoped to Alex, but current-time and target relevance remained unreliable. The initial neutral question had conflicting answer descriptions; those receipts are preserved, its scores excluded from quality conclusions, and a corrected neutral probe was run separately. The documented two-option `choice` workaround was also tested: it still attributed current optimism to an old quote and failed to separate unrelated-team reporting reliably. SDK confidence and calibration claims do not establish Scoracle calibration.

Initial comparison decision, subsequently superseded for Laya: retain Laya as a measured candidate, not the sole gatekeeper by default. The user has now excluded Laya from the launch bank. Carry Horizon small and SamLowe forward as fast emotion baselines; compare target relevance/topic separately on reviewed sources. None of these article/window emotion vectors identifies the speaker, time or supporting phrase by itself. In particular, past hope and another team's joy are legitimate text-level signals but cannot establish current target mood. Source byte ranges bind the input; they are not model explanations.

Next work is the independently reviewed gold set and held-out relevance/attribution checks, followed by the smallest useful Classifier slice and direct cutover on `main`. No production Classifier stage has been deployed in Phase 1. The 8,000–9,000-item replay, GPU execution, sports calibration, downstream factual/emotional support and removal of Harvester/Editor remain open. The retained source sample also contains publisher navigation/paywall/gallery furniture, so source acquisition quality must be reviewed alongside model quality.

## Downstream vector contract — October 7 update

The user wants the gate to distill raw reporting into scored, attributed evidence before vetting articulation models. One Classifier plugin can supply a bank of measurements. The benchmark selected fast **emotion baselines**, not an accuracy winner for all downstream tasks. New labels require supervised training and held-out validation; changing a checkpoint's label names or adding randomly initialized output weights does not create a capable injury, transfer or relevance model. Standard [Transformers classification fine-tuning](https://huggingface.co/docs/transformers/main/en/tasks/sequence_classification) supplies the existing model-loading/training machinery; no new classifier framework is needed.

The executable contract is [vector-schema-v1.json](../fixtures/classifier/vector-schema-v1.json):

| Family | Dimensions and scope | Preparation / model role |
|---|---|---|
| Emotion | 28 independent GoEmotions dimensions per exact document window | Existing Horizon/SamLowe heads; expression in a text remains distinct from current target emotion. |
| Relevance | Direct subject, opponent context, incidental mention, per supplied target/window | New target-conditioned head. Query provenance supplies a candidate identity, never a positive label. |
| Topic | Match event, performance, availability, player move, contract, staffing, discipline, off-field, routine, league context, per target/window | New multi-label head; an injury topic or transfer topic does not establish an actual injury or move. |
| Discourse | Reported assertion, attributed quote, opinion, speculation, prediction, explicit denial, correction, per document window | New multi-label head; preserve competing forms in the same report. |
| Temporal framing | Current, historical and future references, per document window | New head plus explicit event-time qualifiers; report dates never substitute for event dates. |
| Ordinal measurements | Target valence 0–100, intensity 0–3, negotiation stage 0–3 | Reviewed annotations and later task-specific heads; no invented conversion from emotion maxima. Unknown remains null. A stage score describes reporting, not a completed move. |
| Claim qualifiers | Speaker, subject, counterparty, reported event time, negation, uncertainty, source disagreement | Exact supporting source spans and reviewed identity/claim relationships; a window classifier alone does not extract these. Span selection/extraction still requires its own validation. |

Each downstream world must preserve the source excerpts, evidence scope, model/head/version, raw score, calibration status and explicit unknowns alongside any selected signal. It must not replace source claims with a generated summary or collapse everything into one confidence number. Document-level signals, attributed claims and trusted structured records remain distinguishable in the package.

| Character / real implementation | Required world | Where it comes from |
|---|---|---|
| Journalist — `plugins/journalist/prompt.rs`, `memories.rs` | Fresh events/results, targets, attributed reporting, denial/correction/uncertainty, report dates and specifically attached history | Classifier relevance/topic/discourse/time measurements plus exact source claims; existing source-backed continuity. |
| Insider — `plugins/insider/prompt.rs`, `mod.rs` | Moves/contracts/staffing, counterparty, reported versus denied claim, negotiation stage, source disagreement, dated history and publisher outcome record | Classified evidence and exact quotes plus existing resolved identity links and tracked/confirmed publisher samples. A move score never mutates a roster. |
| Influencer — `plugins/influencer/prompt.rs` | Named speaker/target emotion, supported mixtures, valence, intensity, timing and change during a reporting period | Emotion vectors plus attributed spans and explicit time, followed by validated period aggregation. Unknown is neither neutral nor valence 50; one speaker is not the fanbase. |
| Scout — `plugins/scout/prompt.rs`, `sources.rs`, `performance.rs` | Performance measurements and their percentile/cohort/sample limits; availability/personnel changes; relevant attributed news | Keep existing deterministic/statistical measurements and adjudicated records; add scored reporting slices. Do not classify a percentile, fitness outcome or completed transfer into existence. |
| Analyst — `plugins/analyst/prompt.rs`, `tools/memories.rs` | Finished Scout/Influencer readings, dated score histories, window/sample sizes and computed trajectories | Existing products and deterministic history calculations. No separate RSS momentum label is required. |
| Oracle — `plugins/oracle/prompt.rs` | The five finished character cards, agreement/conflict, missing products and shared provenance | Existing downstream synthesis inputs. Do not treat repeated cards sharing one article as independent confirmation. |

Implementation completed in this update:

- The replay accepts one or more trained vector families sharing a document or target input scope; verifies exact checkpoint label identities; retains every independent score; and binds target identity framing plus unchanged source windows to the measured input. Existing emotion inference remains unchanged. An emotion checkpoint fails clearly if asked for topic/discourse labels. Local trained safetensors checkpoints can be replayed with declared revision and weight hashes. Ordinal-head inference/training is not implemented yet.
- Laya inference and SDK question helpers were removed from the active bench. The exact original and corrected-neutral scripts remain in the private Archbox experiment directory as `classifier_replay.original.py` and `classifier_replay.neutral-v2.py`, along with the original requests/responses.
- [classifier_review.py](../examples/classifier_review.py) prepares blind review packets and validates edits against a separate retained-source input. Labels start as null, positives and ordinal annotations require exact model-visible evidence, and every evidence span retains the claim qualifier fields. Reviewers must mark source extraction usability separately. A named reviewer is recorded, but the validator cannot prove reviewer independence or annotation correctness.
- The 40 existing sources produced 374 document windows and 976 review units (374 document, 602 target). All 22,038 presence annotations and 1,806 ordinal annotations remain unknown. These are prepared review materials, **not gold labels**. All 40 sources were already measured in Phase 1 and are barred from a fresh held-out test split. Syndication groups must be assigned before splitting, and the validator rejects groups shared across splits; time-based separation still requires review.
- Pilot materials are local and private: `/private/tmp/scoracle-classifier-launch-20261007/review-v1/windowed-source.jsonl`, `review-with-ordinal.jsonl`, and `manifest-with-ordinal.json`. The directory is mode 0700. The raw publisher text was not added to Git.
- The initial 400-source export approval block was resolved by the user’s explicit authorization to export and continue. The existing read-only `examples/classifier_export.sql` completed with `sample_size=400`; full text and metadata remain in the private Archbox review directory and its local counterpart. No publisher text was added to Git.

Verification at this phase: two runnable contract checks covered complete Unicode source bounds, no omitted late qualification, exact supporting evidence, named review, incompatible head labels and document/target scope separation. Sixteen real-model receipts on the eight fictional controls reproduced the prior Horizon/SamLowe token IDs and emotion vectors with **maximum score delta 0**. This verifies the replay refactor, not new-head accuracy. The larger gold set, additional trained heads, evidence extraction, calibration, full-load replay, production replacement and held-out articulation-model evaluation remain open.

Validate the current review packet:

```sh
python3 examples/classifier_review.py \
  /private/tmp/scoracle-classifier-launch-20261007/review-v1/source-400.jsonl \
  /private/tmp/scoracle-classifier-launch-20261007/review-v1/review-400-provisional-v2.jsonl --validate
python3 -m unittest discover -s examples -p test_classifier_replay.py
```

The next execution step is to review and split independent source groups, train the smallest additional heads on the candidate backbones, compare per-family quality/calibration and verify exact claim support. Select the usable gate after that evaluation. A bounded expression pilot can proceed now on existing emotion measurements, with every untrained signal explicitly unknown; it is not gate acceptance. Keep the original emotion checkpoints intact while training/comparing the new tasks; share encoder execution or consolidate heads only when the measured implementation warrants it.

## Pruning and Phase 2 preparation — October 7 continuation

The user's governing instruction is to retain only complexity that serves the durable Classifier product. Current use alone is not a retention reason. The direct `main` cutover remains the goal; these offline preparation tools are not a deployed Classifier.

- Removed seven obsolete Harvester/Fastino experiment files, including two historical test files: 745 lines of binary annotation, trace-manifest, headline/routing scoring and unused alternative-server tooling. No runtime launcher or Rust caller used them. Their exact source and dated evidence remain recoverable from commit `5c956402`; the old reports now identify the commands as retired. Classifier review code imports no legacy review format.
- Moved the existing complete source-window algorithm into `tools::source::windows`. Classifier no longer imports Harvester cognition, its first-three-paragraph delivery shape or its 64-window policy. The offline Classifier budget defaults to 256 windows and remains explicit; exceeding it fails complete coverage. Existing intake consumes the same source tool until replacement, without a duplicate algorithm or a new compatibility layer.
- The 400-source export contains 314 publishers and 773 resolved query-target identities across FOOTBALL/NFL/NBA. It covers 2,082,928 source bytes in 3,699 windows; the longest source requires 116 windows. The original 64-window attempt failed and its partial output is retained separately. The successful neutral-tool export exactly reproduces the earlier complete export's text and UTF-8 ranges, while dropping the old selection metadata.
- Prepared 11,662 review units: 3,699 document windows and 7,963 target windows. Forty sources were already measured in Phase 1; the additional 360 have not had their model scores inspected or used for selection. This is a stratified retained-source sample, not a representative gold set. Syndication groups and time splits remain unassigned.
- Added a bounded AI-assisted seed: 25 presence suggestions across 12 units, with exact supporting spans and explicit claim qualifiers. These remain `ai_provisional`, unassigned and barred from training/evaluation. All 23,889 ordinal annotations and 244,056 remaining presence annotations are unknown. Initial inspection flags obfuscated publisher text, ad-block interstitials, galleries and recommended-story feeds separately from relevance.
- Added [classifier_train.py](../examples/classifier_train.py): one native Transformers/PyTorch multi-label head on a frozen candidate encoder, exact requested label identities, unchanged target framing and source text, unknown-label masking, positive/negative coverage reporting and hash-bound checkpoint provenance. It resets the new task's linear head weights, preserves the original emotion checkpoints and adds no dependencies. It refuses empty training data, unreviewed extraction and missing positive/negative training coverage. `--check` confirms **zero eligible training/dev/test units** in the current real-source packets. Independent review remains necessary before real-source fitting; no new task accuracy or calibration is claimed.

The source-free [preparation manifest](../fixtures/classifier/review-preparation-2026-10-07.json) records counts, limits and exact file hashes. Private Archbox files are under `/mnt/data/backup/scoracle/classifier-launch/2026-10-07/review-v1/`; the local copies are under `/private/tmp/scoracle-classifier-launch-20261007/review-v1/`. `review-400-v2.jsonl` is the blind packet and `review-400-provisional-v2.jsonl` contains the clearly marked AI suggestions. Old pilot packets remain historical receipts, not an active compatibility format.

Verification: the Rust library passes **368 tests, 64 ignored**, including four source-tool checks and six existing intake cognition checks. Complete byte-for-byte span equivalence across all 400 sources and validation of all 400 review packets also pass. Three Python contract checks pass in Archbox's installed model runtime, including zero gradient for an unknown label; local Python skips that one check because torch is absent. The [native training/replay check](../fixtures/classifier/training-contract-check-2026-10-07.json) fits and reloads both candidate heads on eight entirely fictional controls with explicitly artificial labels: 16 successful receipts, all 13 requested presence dimensions finite, no truncation, and unchanged encoder tensors (134 Horizon; 197 SamLowe). That mechanical check does not establish sports accuracy. Actual fitting of the provisional real-source packet fails before loading a model or creating a checkpoint. The two artificial test checkpoints were then pruned (1.10 GB); inputs, settings, hashes and replay receipts remain. Ordinal heads, calibrated routing, evidence extraction and production replacement remain open.

Check readiness before fitting the target-conditioned head:

```sh
python3 examples/classifier_train.py SOURCE.jsonl REVIEW.jsonl NEW_CHECKPOINT \
  --model Horizon-Labs/multilingual-emotions-small@1bc9627f189e5aa08b00cf525c3c86052067b81e \
  --vector-family relevance --vector-family topic --check
```

Next: independently verify the AI seed and extraction flags, label the missing positive/negative cases, assign source groups and a later held-out time cohort, fit/compare the two candidate backbones, then replace the old intake path with the smallest validated Classifier slice. The plan's original prolonged shadow phase is superseded by the user's direct-cutover authorization.

## Classifier-to-expression pilot — October 7

The user confirmed that Classifier must replace Harvester as a separate plugin. The offline Classifier and articulation replay import no Harvester or Editor code, invoke no Laya protocol and use no old routing thresholds. The retained-source SQL export reads historical RSS query provenance to construct the review sample; those query identities are candidate targets, not positive relevance labels. Live stage registration, acquisition/publication ownership and delivery queries still need the actual cutover; no deployed replacement is claimed.

Moved `SourceContext` from Harvester delivery into the existing shared `tools::source` module and updated all character consumers. There is no compatibility re-export under Harvester. The current delivery reader consumes the neutral type until replacement. This move preserves serialization and existing receipts; it does not make Classifier depend on Harvester's reader.

The new [offline articulation replay](../examples/classifier_articulation.py) binds complete source text to the existing Horizon emotion receipts, validates all 28 dimensions, preserves the exact raw vectors and model revision, and leaves relevance/topic/discourse/qualifiers/ordinals unknown. The world contains original reporting, not a generated summary. The expression task can reason across the supplied clues but must preserve speaker, target, time, denial and uncertainty. It does not invent a target valence score while that head is absent.

The pilot holds ten cases constant: eight fictional controls plus two previously exported publisher reports (Melton availability and Daniels absence). Each model receives both source-only input and the same input with the full spectrum. Expected review notes are retained outside generation input. The trials use temperature 0, seed 42, context 4,096, output budget 400, thinking disabled, the same prose schema and a single model call; no repair stage or database publication occurs.

Candidates: current `alibayram/smollm3:latest` versus `smollm2:1.7b` and `qwen3:1.7b`. The latter two were downloaded into the existing local runtime for this pilot. SmolLM2 is actually 1,711,376,384 stored parameters, Q8_0. The Qwen 1.7B tag reports 2,031,739,904 stored parameters, Q4_K_M, so the runtime size is recorded explicitly rather than assuming its tag is a precise count. The current SmolLM3 is reported as 3.1B, Q4_K_M. Precision, load and active-host contention differ; latency is configuration-specific. The primary [SmolLM2 model card](https://huggingface.co/HuggingFaceTB/SmolLM2-1.7B-Instruct) describes rewriting/summarization support, and the [Qwen card](https://huggingface.co/Qwen/Qwen3-1.7B) describes its modes. Neither establishes Scoracle fidelity.

Completed: **60 calls**, with 20/20 structurally valid replies from SmolLM3, 18/20 from SmolLM2 and 20/20 from Qwen. SmolLM2's two failed spectrum replies reached the 400-token output limit and remain retained. AI source inspection flagged attribution, time, unsupported-emotion, abstention and prose-completeness failures across all three models. No articulator is promoted; full vectors alone did not establish usable expression. The [diagnostic report](classifier-articulation-pilot-2026-10-07.md) and [source-free receipts](../fixtures/classifier/articulation-pilot-2026-10-07.json) record the findings and limitations. Independent review and held-out real-source acceptance remain open.

The Rust library still passes 368 tests, with 64 environment-dependent checks ignored. All four Python contract checks pass in the existing Archbox runtime; local Python skips the torch-dependent one. The new check covers unchanged source binding, full-vector retention, explicit unknowns and consistent abstention. Exact requests, replies, models/digests and timing receipts are retained privately under Archbox `/mnt/data/backup/scoracle/classifier-launch/2026-10-07/articulation-v1/` and its local counterpart. Structural success is recorded separately from semantic fidelity. The cognition worker was briefly paused to resolve model-loading contention and restored afterward; cognition, API and Laya services all reported active, with production model routes unchanged.

Next expression test: select supported measured signals and exact attributed claims into a compact world after relevance/time/qualification validation, then compare the same candidates again. Smaller models remain plausible, but this pilot does not demonstrate that either can replace the baseline safely. Live Classifier registration and delivery cutover are still pending; Classifier will replace the old plugins, not delegate to them.

The user's research handoff also identifies Pleias-RAG-1B (described as 1.2B) and Pleias-RAG-350M as future source-synthesis candidates, with custom query/source tokens and 4,096-token training context. Their primary cards/paper and limitations are linked in the diagnostic report. Neither was installed, tested or selected here. The handoff reports current plugin contexts of 4,096; future evidence bundles must reserve room for instructions, reasoning and output within that total.

## Qualified claim-packet retest — October 7 continuation

The next requirement is an expression world with speaker, subject, target relationship, time and qualification attached to each exact source claim. Implemented source-bound claim validation in the existing review tool and a qualified variant in the existing articulation replay. It rejects changed source hashes/quotes, unanchored target links, missing denial/time/correction evidence, duplicate claim spans and unresolved emotional target relationships. Literal span validation cannot prove that an annotated relationship is semantically correct.

The qualified audit world contains selected exact claims and literal qualifier text, source identity/hash, review provenance and selected emotion dimensions from their original document windows. Full source, all 28 scores and full qualification spans remain in hash-bound receipts. These window scores are not target probabilities or intensity; additional trained heads and target ordinals remain absent. Information-only, unrelated, conditional-feeling, statement-denial and withdrawn claims remain in the review ledger but do not supply an observed emotional claim. When a complete reviewed source has no eligible emotional claim, the offline replay abstains without a model call; an unresolved target relation fails for review rather than becoming an all-zero vector. This is character-specific evidence selection, not deletion of reporting from the Classifier world.

This isolates packet usefulness with **provisional AI source annotations**, not automatic claim-extraction accuracy or independent gold. The 400-source training/evaluation eligibility is unchanged. Four new fictional controls cover conditional future feeling, denial of a statement, correction and a current feeling about a future event. The 14-case retest holds the three candidates and generation settings constant and compares source-only, raw spectrum and qualified claims under the same updated instructions. Failed replies are retained; no repair or publication stage is added.

Completed: **252 comparison slots = 192 model calls + 60 native abstentions** across the initial three-input comparison and three focused follow-ups. Four malformed replies remain retained. Model inputs now omit numeric scores, review bookkeeping and unknown bookkeeping fields while preserving those details in the audit world. The compiler retains complete supporting paragraphs so corrections and identity links keep their antecedents. The final run has 27 structurally valid model replies and 15 native abstentions, with fidelity errors still present; no model is promoted. The [qualified-world report](classifier-qualified-world-test-2026-10-07.md) and [source-free receipts](../fixtures/classifier/qualified-world-test-2026-10-07.json) record the findings and provisional source inspection.

All five Python checks pass in Archbox's installed runtime; local Python skips the torch-dependent check. Four focused Rust source-tool tests pass, and all 400 existing review packets validate. Every recorded input plus its reserved output budget stayed below 4,096 (maximum 3,736); full paragraph context needs native token-budget checks before production integration. The cognition worker's exit handler restored it after each run, and cognition/API/Laya services all reported active. No production model route changed.

Private run directory: `/mnt/data/backup/scoracle/classifier-launch/2026-10-07/qualification-v1/` on Archbox, with a local counterpart under `/private/tmp/scoracle-classifier-launch-20261007/qualification-v1/`. `reader-v2`, `reader-v3` and `context-v4` retain the follow-ups without overwriting earlier failures. Next: independently review extraction/relationships, fit/compare missing heads and validate automatic span/relationship extraction; then evaluate held-out real-source expression and integrate the independent Classifier stage.

## The idea

Scoracle’s news world is not binary. A story can be partly relevant to a team, strongly relevant to one player, mostly factual with a small speculative edge, emotionally hopeful and anxious at once, and more useful to one character than another. Reducing that to yes/no, positive/neutral/negative, or one assigned writer throws away the shape of the story.

The Classifier plugin should preserve that shape. It receives the fetched corpus and measures it across useful dimensions, returning structured signals with evidence and uncertainty. Plugins then select and frame the measured world for their jobs. Character models articulate what that world supports.

> **Intent selects the world.**  
> **Plugins define the world.**  
> **Tools provide its elements and rules.**  
> **AI builds and expresses within it.**

For this launch, the Classifier is a plugin that measures the incoming world after RSS. It is not a replacement writer and it does not author summaries. Its job is to create a richer, queryable layer between fetched source material and character cognition.

## Why change the gatekeeper

The Editor and Harvester experiments exposed a mismatch: generation was being asked to decide what the world contains. A generative model can summarize and assign, but it can also complete a plausible sports narrative when the evidence is thin. That makes its output difficult to treat as a reliable boundary for every downstream character.

A classifier has a narrower contract. It estimates labels or scores for supplied text. It does not independently invent a game result or quote. Its mistakes are still possible—misclassification, bias, weak domain transfer, poor calibration—but the failure surface is more inspectable: we can see the score, label, excerpt, model version, and threshold that drove a decision.

The goal is not to make every score authoritative. The goal is to represent uncertainty explicitly and keep the original evidence close to every derived signal.

## Proposed flow

```text
RSS fetch
  → normalize and deduplicate source items
  → Classifier plugin measures each item
  → persist scores, evidence spans, provenance, and model versions
  → character plugins select the slices they need
  → compact articulator expresses the supplied world
```

The Classifier runs once per canonical story (or per relevant excerpt), not once for each character. It should preserve the original article and produce reusable measurements. The harness chooses which classifier resources to invoke; the plugin declares its model, label schema, thresholds, and output contract. A character plugin can then request different score slices without rerunning the same broad inference.

This follows Scoracle’s current boundary: the harness selects plugins; plugins determine which tools, SQL tables, context, memories, voice, and resources enter an inference; the LLM articulates the supplied world rather than assembling or reconstructing it.

## A spectrum-shaped record

A first-pass record might look like this:

```json
{
  "item_id": "news_4821",
  "model_set": "classifier-bank-v0",
  "scores": {
    "entity_relevance": {
      "detroit_pistons": 0.98,
      "cade_cunningham": 0.73
    },
    "topics": {
      "performance": 0.89,
      "injury": 0.11,
      "transaction": 0.02
    },
    "discourse": {
      "reported_fact": 0.81,
      "opinion": 0.42,
      "speculation": 0.17
    },
    "emotion": {
      "optimism": 0.64,
      "excitement": 0.51,
      "concern": 0.38,
      "disappointment": 0.05
    }
  },
  "evidence": [
    {
      "dimension": "emotion.optimism",
      "text": "The team has won four of its last five games...",
      "start": 214,
      "end": 260
    }
  ],
  "coverage": {
    "input_tokens": 612,
    "truncated": false
  },
  "provenance": {
    "source_id": "rss_source_12",
    "published_at": "2026-10-07T14:00:00Z",
    "classifier_version": "classifier-bank-v0"
  }
}
```

The values above illustrate a shape, not a claim about an actual Pistons story. In production, store raw model scores as scores, not as calibrated probabilities unless calibration has been measured. Keep model outputs separate from policy decisions such as “send to Vibe” or “drop as irrelevant.” Those decisions should be explicit, versioned rules over the signal vector.

### Design rules for useful spectrums

- **Keep independent dimensions independent.** An article can score high for both optimism and concern. Multi-label outputs are more faithful than forcing one winning label.
- **Keep neutral/unknown distinct from low signal.** “No detected emotion” is not the same as “neutral article,” and missing coverage is not zero relevance.
- **Retain the score vector.** Thresholding can happen later. A score like 0.43 may matter for a trend, even if it does not trigger a single-item action.
- **Attach evidence.** Where feasible, store the sentence or span that contributed to a signal. Scores without source support are hard to inspect and unsafe to turn into prose.
- **Represent uncertainty honestly.** Scores from an uncalibrated model are ranking signals, not truth percentages. Track model, tokenizer, label schema, thresholds, and calibration set versions.
- **Support opposing and mixed signals.** A player story can contain confidence and doubt, praise and criticism, or a positive result with concern about the process.
- **Preserve time.** A single item describes one moment; the Vibe or Momentum character needs a time series over classified items, with decay and source weighting applied by the consuming plugin.
- **Keep source stance separate from emotion.** The article may report a quote expressing anger without the reporter endorsing that emotion. Discourse, quoted speaker, and target should be separate dimensions where possible.

## The classifier bank: specialized measurements, not one magic model

“Classifier” describes a role in the harness, not necessarily one neural network. A bank can begin with a small number of independent measurements and grow only when evaluation shows a gap.

| Measurement | Useful output | Likely consumer | Notes |
|---|---|---|---|
| Entity relevance | Per-entity score, entity mention spans | Every character | Combine deterministic entity matching with a compact ranker/classifier; alias and opponent handling are domain-specific. |
| Topic | Multi-label scores: injury, performance, transaction, coaching, roster, off-court, league context | Journalist, Scout, Insider | Taxonomy should reflect Scoracle’s actual character jobs. |
| Emotion / affect | Multi-label emotion scores, optional valence and intensity | Vibe, Momentum | General emotion labels are a starting vocabulary, not a finished sports-fan taxonomy. |
| Discourse / claim type | Reporting, quote, opinion, speculation, rumor, prediction | Journalist, Insider | Classify the article’s framing, not whether its claims are true. |
| Source reliability | Source identity and historical reliability features | Insider, harness policy | Primarily a data and history calculation; do not ask a generic emotion encoder to infer trustworthiness from prose alone. |
| Character routing | Per-character suitability scores | Harness queue selection | Prefer rules over measured dimensions initially; train a dedicated classifier from reviewed Scoracle decisions later. |
| Evidence selection | Sentence relevance or span scores | All characters | A smaller sentence ranker can surface support without asking the articulator to reread the whole corpus. |

A compact classifier bank can be a mix of learned and deterministic tools. Exact player/team aliases, RSS metadata, published timestamps, source history, database joins, and duplicate fingerprints are often better handled in Rust/SQL than by a model. The neural classifier should focus on semantic distinctions that rules handle poorly.

## Why a roughly 149M model is plausible

A 149M encoder is small beside a 3B generative model and is built for classification rather than open-ended completion. ModernBERT-base is 149M parameters, Apache 2.0, and supports sequences up to 8,192 tokens. A 141M multilingual emotion model built on mmBERT-small is also Apache 2.0. These are credible starting sizes for testing whether one or more encoders can measure Scoracle’s daily corpus quickly on the available CPU/GPU setup.

The number of articles alone does not establish throughput. Runtime depends on average token length, truncation/chunking, batch size, hardware, precision, inference engine, and how many classifier heads run per item. At 8,000–9,000 items/day, the average arrival is only about 0.09–0.10 items/second over a full day, but the actual RSS batch may arrive in bursts and articles can be much longer than social posts. Benchmark with actual Scoracle text and batch shapes before claiming a speed target.

A reasonable prototype should measure:

- wall-clock time for the full daily corpus and for peak fetch batches;
- items/second and tokens/second by article length bucket;
- CPU-only and available GPU inference, including memory use;
- latency and throughput with dynamic batching;
- cost of chunking longer articles and deduplicating near-identical stories;
- quality and calibration on a manually reviewed Scoracle sample.

The same architectural shift could allow smaller character articulators. If the plugin supplies structured measurements and source excerpts, the character model no longer needs to discover every topic, infer the emotional mix, route articles, and reconstruct the facts in one pass. That is a hypothesis to validate: compare the current character model with smaller models on identical, evidence-bounded inputs and judge factual support, voice, and usefulness separately.

## Models to put on the test bench

The list below is a starting bench, not a leaderboard. Hugging Face cards describe different datasets, label schemes, and evaluation setups, so the reported scores are not directly comparable. For Scoracle, domain fit and calibration on real sports reporting matter more than a high score on Reddit or a small six-class benchmark.

| Candidate | Size / license | What it offers | Watch-outs | Suggested role |
|---|---|---|---|---|
| [Horizon-Labs/multilingual-emotions-small](https://huggingface.co/Horizon-Labs/multilingual-emotions-small) | 141M; Apache 2.0 | 28 independent GoEmotions labels; multi-label sigmoid outputs; ONNX availability; compact and multilingual. | Trained on GoEmotions Reddit comments plus translated data. Its own card reports modest macro-F1 and weak rare labels; sports journalism is a domain shift. | **First emotion-vector baseline**, especially if multilingual flexibility or ONNX deployment matters. |
| [HR26kk/modernbert-emotion-classifier](https://huggingface.co/HR26kk/modernbert-emotion-classifier) | ModernBERT-base backbone, 149M; Apache 2.0 | Six labels (sadness, joy, love, anger, fear, surprise); card reports 92.25% accuracy and 87.02 macro-F1 on its benchmark. | Single-label/six-class framing is too narrow for the spectrum goal; benchmark is dair-ai/emotion, not sports news. Card shows limited adoption. | **149M speed/quality baseline**, but adapt output to multi-label before treating it as the desired design. |
| [Pradeep-mahato/ModernBERT-GoEmotions](https://huggingface.co/Pradeep-mahato/ModernBERT-GoEmotions) | ModernBERT-base backbone, 149M; Apache 2.0 | 27 emotions plus neutral; independent sigmoid scores; multi-label; easy Transformers use. | Short-text GoEmotions domain; limited adoption; card does not establish sports-news performance or calibrated scores. | **Direct 149M multi-label comparator** to Horizon’s 141M model. |
| [Hidden-States/roberta-base-goemotions](https://huggingface.co/Hidden-States/roberta-base-goemotions) | RoBERTa-base, about 125M; Apache 2.0 | GoEmotions multi-label model; reports macro-F1 0.56 and micro-F1 0.62 on its test setup; useful alternate training approach. | Metrics are model-card claims and may not share identical splits/evaluation; still Reddit-domain labels and no sports validation. | **Strong baseline comparator** for whether ModernBERT materially improves the task. |
| [SamLowe/roberta-base-go_emotions](https://huggingface.co/SamLowe/roberta-base-go_emotions) | RoBERTa-base, about 125M; MIT | Widely used GoEmotions multi-label baseline; Horizon’s comparison reports tuned GoEmotions macro-F1 around 0.519. | MIT rather than Apache 2.0; Reddit-domain mismatch; verify exact output and calibration conventions. | **Reference baseline** if the project accepts MIT for evaluation. |
| [Horizon-Labs/multilingual-emotions-base](https://huggingface.co/Horizon-Labs/multilingual-emotions-base) | 308M; Apache 2.0 | Same 28-label family at a larger size; its card reports improved GoEmotions and BRIGHTER results over its small sibling. | Larger than the 149M target; same source-domain limitations; only test if quality gains justify extra compute. | **Quality ceiling comparison**, not first deployment choice. |

### Read the benchmark claims carefully

- HR26kk’s reported 92.25% accuracy is on a six-class emotion benchmark. It should not be compared directly with 28-label multi-label macro-F1.
- Horizon’s card reports 0.494 GoEmotions macro-F1 at tuned thresholds for its released 141M checkpoint, and shows that its comparison models can score differently depending on dataset and threshold. The card also notes weaker performance on rare labels and modest annotation agreement.
- The ModernBERT-GoEmotions model card warns that emotion labels are subjective and scores should be treated as probabilistic signals, not ground truth.
- Model popularity is useful for ecosystem confidence, but not proof that a classifier will understand sports fan affect or long-form reporting.

## Don’t mistake a score for a spectrum

A probability vector is a useful raw material, but it is not automatically a reliable spectrum. A model may be overconfident, underconfident, or consistently miscalibrated for sports language. The plugin should preserve raw logits/probabilities and derive operational score bands only after evaluation.

A practical interpretation layer might use:

- **raw score:** output exactly as produced by the model;
- **calibrated score:** optional adjusted value learned on reviewed Scoracle examples;
- **evidence strength:** whether a sentence or span supports this dimension;
- **coverage:** how much of the source was inspected and whether it was truncated;
- **policy band:** a versioned routing rule such as low / review / high, derived from score plus evidence and source conditions.

The user-facing world can remain continuous even if a queue needs a threshold. Do not discard the vector after making the routing decision.

## A Scoracle-specific label system

General-purpose labels are useful for bootstrapping, but the platform likely needs a sports-specific affect vocabulary. Start by reviewing outputs from existing models, then decide which distinctions actually improve Vibe and Momentum. A first candidate set could include:

- **positive energy:** optimism, excitement, pride, relief, admiration;
- **negative energy:** disappointment, frustration, anger, concern, fear;
- **mixed or changing state:** uncertainty, tension, cautious optimism, resignation;
- **intensity:** subdued ↔ intense;
- **trajectory:** improving ↔ worsening ↔ stable, computed from timestamped signals rather than guessed from one article;
- **stance/source:** reporter assertion, attributed quote, fan reaction, pundit opinion, speculation.

Do not force labels such as “fanbase mood” from a single article unless the text actually represents fan reaction. A reporter’s tone, a quoted coach’s confidence, and the collective fan mood are different entities and should be represented separately.

## Evidence-bounded articulation

The character LLM should receive a compact, coherent world assembled by its plugin:

1. the relevant measured spectrum and its time window;
2. the source excerpts that support the important signals;
3. source identity, timestamp, and discourse/attribution information;
4. explicit gaps or disagreement, such as “emotion evidence is mixed” or “no reliable injury update found”;
5. the character’s voice and output form.

The articulator should not be asked to recompute the classification. It should express the supplied world. If the evidence is incomplete, the plugin should pass that incompleteness into context rather than asking the model to fill it. Keep factual claims traceable to source excerpts or trusted structured data.

## Evaluation plan

### Phase 1: corpus sample, no integration

Take 20–50 representative RSS items across teams and story types. Include straight reporting, opinion, quoted emotion, speculation, injury reports, transactions, game recaps, duplicates, and low-relevance league stories. Run the candidate emotion models and inspect their full score vectors and evidence windows.

### Phase 2: Scoracle gold set

Build a reviewed set of roughly 300–500 items. Annotate multiple labels per item, evidence spans, target (player/team/source), stance, and “not enough evidence.” Let annotators mark ambiguity rather than forcing a crisp label. Keep the test split separate from any fine-tuning data.

### Phase 3: score the properties that matter

- per-label precision, recall, and macro-F1;
- ranking quality (does a higher score generally mean stronger reviewed evidence?);
- calibration (Brier score or reliability plots where labels support it);
- evidence-span precision and coverage;
- disagreement and ambiguity rates;
- robustness to negation, sarcasm, quotes, headlines, and duplicated syndication;
- sports/team/player relevance and topic-routing quality;
- throughput over a real 8,000–9,000 item corpus;
- downstream character factual support and usefulness.

A model should not win solely by producing more labels. Evaluate false positives because fabricated emotional color is also a form of distortion. For the character stage, count unsupported factual claims and unsupported emotional claims separately.

### Phase 4: shadow mode

Run the plugin beside the current flow without changing downstream behavior. Compare its vectors and proposed routing against reviewed outcomes. Log model version, thresholds, article coverage, and processing time. Only promote a classifier dimension after it demonstrates stable value on the Scoracle set.

### Phase 5: narrow production rollout

Start with one proven signal family—likely relevance/topic or emotion—then expand. Keep a fallback path for model errors and preserve the raw article. Do not make a low-confidence prediction silently erase an item from the world.

## Operational contract

Each classifier result should carry:

- canonical item ID and source IDs;
- classifier/plugin version and model revision hash;
- label schema version;
- raw scores and calibrated scores, if any;
- evidence spans or sentence IDs where available;
- input token count, chunking strategy, and coverage/truncation status;
- timestamps and runtime metadata;
- explicit error/unknown state distinct from all-zero scores.

Store source articles and derived measurements separately. Derived scores can be recomputed when a model or schema changes. Keep model inference out of the system-of-record meaning of the source itself: the article remains the evidence; the classifier output is a versioned interpretation.

## What success looks like

The launch is successful when Scoracle can process its daily fetched corpus predictably and characters receive a better-shaped world than a pile of undifferentiated articles. The system should retain useful gradations, show why a signal exists, preserve mixed evidence, and allow each plugin to choose what matters for its own purpose.

The central shift is simple:

> **The gatekeeper stops writing reality. It measures dimensions of the supplied reality.**

Then the character is free to be expressive while remaining bounded by a world built from source material, structured data, and inspectable signals.

## Sources

Model cards and specifications checked October 7, 2026:

- [Horizon-Labs/multilingual-emotions-small](https://huggingface.co/Horizon-Labs/multilingual-emotions-small) — 141M, Apache 2.0, 28 GoEmotions labels, benchmark table, data and limitations.
- [HR26kk/modernbert-emotion-classifier](https://huggingface.co/HR26kk/modernbert-emotion-classifier) — Apache 2.0, 149M ModernBERT-base, six-label task and reported benchmark.
- [Pradeep-mahato/ModernBERT-GoEmotions](https://huggingface.co/Pradeep-mahato/ModernBERT-GoEmotions) — Apache 2.0, multi-label GoEmotions, independent sigmoid scores and limitations.
- [Hidden-States/roberta-base-goemotions](https://huggingface.co/Hidden-States/roberta-base-goemotions) — Apache 2.0 and model-card metrics for a GoEmotions classifier.
- [SamLowe/roberta-base-go_emotions](https://huggingface.co/SamLowe/roberta-base-go_emotions) — widely used GoEmotions reference checkpoint; license and comparison figures noted in the Horizon card.
- [Horizon-Labs/multilingual-emotions-base](https://huggingface.co/Horizon-Labs/multilingual-emotions-base) — 308M Apache 2.0 comparison model.
- [answerdotai/ModernBERT-base](https://huggingface.co/answerdotai/ModernBERT-base) — 149M base encoder, Apache 2.0, 8,192-token maximum sequence support.
