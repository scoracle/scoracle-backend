# Scoracle Classifier Plugin

## Launch concept: measure a spectrum, then let characters express it

**Status:** Production cognition remains paused; daily RSS discovery continues. This session now focuses on plumbing and replaceable model plugins. Correctness calibration, independent accuracy evaluation and model selection belong to a separate dedicated session. Native source acquisition, Classifier registration, model routing and durable receipts are implemented and checked in isolation — October 8, 2026.

**Placement:** After Google News RSS discovery and native full-source acquisition

**First goal:** Replace separate intake relevance gates with a reusable spectrum of attributed evidence

**Model strategy:** Public model weights and open inference code; hosted/proprietary Jev is excluded at the user's instruction. The Classifier owns a fixed source/output contract; its model is replaceable. Model-specific route defaults, prompts, generation settings and input admission live in `src/plugins/classifier/prompt.rs`. Calibration will be handled separately.

**Governing target:** [Target design — October 7, 2026](#target-design--october-7-2026). Relevance is derived from the complete spectrum of supported downstream evidence. No useful signal means no new character work; unknown or failed measurement is never absence. This target supersedes earlier model-size and separate-gate assumptions.

---

## Remaining plumbing — priority 1, October 8

Production cognition remains paused. Build and verify the replacement flow before enabling it; daily RSS discovery must keep accumulating data. Model accuracy and calibration are a separate session.

1. [x] Connect RSS discovery to `classifier_acquire`; replay retained discovery idempotently. Duplicate sweeps/replays preserve leases, retry backoff and parked failures, and skip unchanged acquired sources. Acquisition runs independently of inference. Implemented and verified locally; production activation remains pending.
2. [x] Complete the fixed measurement envelope: all 51 presence dimensions, three ordinals, qualified claims, explicit unknowns and source/model/schema provenance. This establishes the plumbing contract; model support and accuracy remain separate.
3. [ ] Finish replaceable model adapters. Exact complete-input admission is implemented for native llama.cpp generative models and checked with two installed models. Keep tuning in `classifier/prompt.rs`; specialized Laya/Kev/GLiNER head protocols still need their production adapters and an explicit disposition for missing claim extraction.
4. [ ] Finish character delivery. Journalist, Influencer and Insider now read Classifier worlds and publish through native delivery obligations, source checks and the existing claim/outbox transaction; their local flows are verified. Scout still needs its selection/identity adapter, followed by Analyst/Oracle dependency checks.
5. [ ] Exercise the complete flow through duplicate discovery, backlog replay, worker restarts, retries and model swaps. Deploy acquisition independently first; resume production cognition only after the replacement is ready and the pause is lifted.

Current implementation: independent workers, immutable snapshots, attempt receipts, claim fencing, RSS producer wiring, backlog replay and the complete measurement envelope are checked locally. Exact tokenizer admission now works through the native llama.cpp adapter. Next: Scout's direct character boundary and their identity/selection rules, specialized model protocols, then deployment/recovery verification. No production replacement has been enabled.

## Execution ledger

The tracked plan is this file; the original was supplied from Downloads. The user's launch instruction authorizes replacing Harvester and Editor directly on `main`; protecting the broken production flow with a prolonged parallel rollout is not a requirement. Model quality, source integrity, uncertainty, coverage, and reproducibility still need evidence.

- [x] Recalibrate this session to plumbing only at the user's instruction. Keep structural/source checks and durable errors; defer correctness calibration and independent accuracy evaluation to a dedicated session.
- [x] Reuse the existing text-generation model plugin and routed inference capability. Move Classifier model selection defaults, prompt construction, constrained schema, temperature, context/output reservation and input admission to `classifier/prompt.rs`.
- [x] Register independent opt-in `classifier_acquire` and `classifier` workers. Acquisition preserves complete native source snapshots and atomically queues classification; it can run without an inference handler. Classifier stores versioned requests, source-bound provisional claims, world selections, raw responses and failures in its own tables.
- [x] Verify a model swap across Ollama and compatible-chat transport serializers without changing source or downstream claim contracts. Check artifact-aware reuse, retries, source changes and stale claims against disposable Postgres. No human gold or semantic calibration is required for these plumbing checks.
- [x] Replace the Go RSS Harvester enqueue with native Classifier acquisition. Share discovery revisions and enqueue policy through migration 292; replay retained query edges at worker startup and periodically. Duplicate sweeps/replays preserve active claims and unchanged failures. Changed source metadata, candidate names/ranks and bodies invalidate snapshots. Propagate persistence errors into the existing RSS sweep error counters.
- [x] Connect Journalist to independent Classifier delivery records (migration 293). Keep complete source text and source-bound proposed claims, qualifications and unresolved relationships together in the writing payload. Dispatch an eligible pending obligation atomically through the existing queue; unchanged ready updates preserve leases and failed backoff. Supersede older pending/held measurements without deleting completed history. Publication rechecks sources, query identities, immutable receipts and delivery state; busy rows fail fast instead of deadlocking a dispatch writer. Unknown dates and input failures stay held rather than becoming absence.
- [x] Verify native acquisition → Classifier → Journalist preparation/inference → product/delivery/outbox/completion in disposable Postgres with fictional controls. Three-report batches survive a worker restart; source drift and stale leases cannot publish; product/outbox failures roll back all associated changes. Replays do not reopen used evidence. No real measurement is promoted: unassessed receipts create held obligations, and only the test fixture explicitly releases its fictional controls.
- [x] Connect Influencer to Classifier receipts and atomic delivery dispatch (migration 294). Preserve complete publisher text and literal proposed relationships in fresh reporting and accepted reporting-period history. Use the existing DuckDB study and period-card publisher; worlds participate in copy detection and attempt fingerprints. Unknown dates remain held. An isolated native acquisition → Classifier → Influencer check drains two periods through a worker restart, distinguishes an exact copy from a changed report, rejects source drift, verifies outbox rollback/retry and reuses an unchanged finished card. Only fictional controls are released; other characters' obligations remain held.
- [x] Connect Insider to independently measured canonical targets and Classifier obligations (migration 295). Native exact canonical-name resolution preserves ambiguous/alias-only names as unresolved reporting; model names never create database identities. Player and coach targets receive their own measurements, without sharing the RSS team's target interpretation. Identity/name changes invalidate discovery and publication. Insider retains full-source proposed worlds, existing publisher/history studies and source-grounded denied/reported findings. Products, dispositions and outbox remain atomic; coach queue support and the transfer trigger schema are included. The isolated native flow checks team/player/coach targets, ambiguous identity changes, outbox rollback, restart/retry and explicit denials. No real evidence is released.
- [x] Add a native llama.cpp model adapter using the serving model's chat template and tokenizer. Submit those exact token IDs, reserve the actual transport output budget against the smallest serving slot, verify consumed tokens and reject template/model drift, clipping and incomplete output. Existing inference concurrency limits also govern prepared completions. Native Qwen/Granite swaps preserve the source/output contract; legacy routes without exact tokenizer evidence fail before generation. No model is promoted.
- [x] Store a fixed 51-presence/three-ordinal envelope, filling omitted dimensions with explicit unknowns. Accept sparse raw model measurements only for schema dimensions/ranges; bind evidence to exact UTF-8 source spans. Check the same typed consumer envelope across model transports. No calibration or absence threshold is introduced.
- [x] Verify the real Go producer and Rust acquisition/storage path in separate disposable Postgres databases. Duplicate discovery, active claims, parked failures, changed metadata, completed acquisition reuse, source changes, model swaps and typed measurement reads pass. Publication retries busy source locks immediately so it cannot deadlock RSS's article-before-queue transaction. Rust: 370 active library checks pass, plus the explicitly run acquisition integration check; Go thirdparty/work checks pass. Production deployment remains pending.

- [x] Pause production cognition and its automatic binary-change restart watcher. On Archbox, `scoracle-cognition.service` and `scoracle-cognition.path` are inactive and disabled; the API remains active. Daily RSS ingestion remains scheduled at 02:00 America/New_York; the October 7 sweep retained 8,364 new articles with no RSS errors. Existing queue and source records are preserved.
- [x] Retire Harvester/Editor as the production approach at the user's instruction. Allow the RSS discovery backlog to grow while development focuses on the independent Classifier. RSS discovery retains URLs, metadata and query provenance; new full publisher-body acquisition depended on the paused cognition worker and is not currently running. The replacement must acquire those bodies independently of model admission before replaying the backlog.
- [x] Preserve current development and consolidate it onto `main`, pushed through `7f49d5a2`. Correct five stale regression checks; 384 active Rust tests pass (65 database/model checks remain ignored without their dependencies).
- [x] Trace current intake: Go RSS provenance → Harvester headline gate → Editor acquisition/windowed Laya scores → source-bound classifications and character assignments. Existing source receipts, canonical IDs, worker claims, and transactional publication are reusable. Current routing is uncalibrated and the source/model experiments did not establish factual fidelity.
- [x] Phase 1: source-bound 40-item replay of three compact emotion encoders; matched four-model comparison on 20 opening windows plus eight synthetic controls. Full vectors, exact input windows, checkpoint identities, timings and limitations retained.
- [x] Restore Laya to the comparison at the user's subsequent instruction, superseding its earlier exclusion. Harvester/Editor cognition remains paused; do not restore it as an interim production path.
- [x] Complete the four-model GPU baseline in an isolated compatible CUDA environment: 112 matched-excerpt, 192 complete-source and 28 Laya binary-choice receipts. Record float32 and per-article batching limits separately from model quality.
- [x] Pause further qualification inference and inspect public Jev/Laya classification repositories at the user's instruction. Record source-candidate selection, shared-state batching, precision and output-consistency experiments in the [comparison report](classifier-spectrum-comparison-2026-10-07.md#gpu-baseline-and-github-investigation--october-7-continuation).
- [x] Search Hugging Face for open alternatives and verify public weight artifacts, declared licenses and checkpoint revisions. Record GLiNER2.5 extraction/decision models, Kev, Intern-Decision and alternative Laya checkpoints in the [candidate shortlist](classifier-spectrum-comparison-2026-10-07.md#hugging-face-open-model-shortlist). This is research, not completed runtime or accuracy qualification.
- [x] Update both Ollama installations to [official stable 0.40.1](https://github.com/ollama/ollama/releases/tag/v0.40.1), verified October 8. Both CLI/server versions verified; bounded-context inference passed on Mac and Archbox, with Archbox GPU offload confirmed. All six model identities on each host are preserved. Mac official binaries are installed at `/Users/scotty/.local/lib/ollama/0.40.1`, with prior CLI link and launch-agent settings retained under `rollback-20261008`. Archbox rollback files and verification receipt are retained at `/mnt/data/backup/scoracle/ollama-update-v0.40.1/`. Cognition remains inactive and disabled. Updating the runtime does not establish candidate-model support or accuracy.
- [x] Run the first full-source open decision/extraction bank: Laya and Kev on Mac Metal, GLiNER2.5 base/multi-Decide and Intern-Decision on Archbox CUDA. Retain 160 complete spectrum receipts, explicit token/source coverage, native claim-validation failures and frozen source-only AI provisional checks. [October 8 results](classifier-spectrum-comparison-2026-10-07.md#full-source-runtime-and-qualification-experiment--october-8): no candidate is qualified; Intern's 41/48 diagnostic result misses every positive and only matches an absent-only baseline.
- [x] Correct the qualification runner's invalid emotion enum and verify exact preflight token IDs through native schema-constrained completion. Five Qwen3 smoke generations complete, but all fail native source/qualification checks. Earlier Ollama runner/tokenizer failures remain retained; serialization success is not semantic success.
- [ ] Recheck the GPU bank with supported mixed precision and cross-article length-grouped batching, retaining score differences and complete-source coverage; measure latency and throughput separately.
- [ ] Qualify automatic extraction, native source-candidate selection and speaker/claim/time links against the fixed contract. Expand the provisional diagnostic bank into independently adjudicated accuracy evaluation; generated completion and document predicates cannot substitute for qualified evidence.
- [x] Map all six character plugins' actual material contracts; define 51 independent presence dimensions, three ordinal annotations and exact-span claim qualifiers. Prepare and validate a source-only 40-item review pilot with all annotations unknown.
- [x] Document the governing deconstruct → retain → build → discover/express → serve architecture, spectrum-derived relevance, per-character no-action decisions and explicit unknown/failure states. Remove superseded fixed-size and separate-admission design assumptions.
- [x] Export and prepare 400 retained sources (314 publishers, 3,699 complete source windows) for Phase 2 review.
- [ ] Phase 2: independently reviewed 300–500 item gold set, with source-group/time splits and ambiguous/unknown labels.
- [ ] Train and compare the additional heads on Horizon/SamLowe backbones; retain the existing emotion checkpoints as baselines. Evaluate each signal family before choosing a deployment checkpoint.
- [ ] Phase 3: held-out quality/calibration, real-corpus CPU/GPU throughput, and evidence/attribution checks.
- [ ] Validate the spectrum-derived gate against reviewed downstream usefulness; distinguish per-character abstention from item-wide no action and preserve failures/unknowns and retained evidence.
- [ ] Replace the two intake stages with one Classifier stage; retain versioned measurements separately from routing and source evidence. Run source/publication checks and a direct production cutover when the selected slice works.
- [x] Complete an independent 60-call Classifier-to-expression diagnostic with source-bound emotion vectors; compare source-only and spectrum inputs on eight controls plus two retained reports across three articulators. Preserve failed replies and AI provisional semantic review; no model promoted.
- [x] Bind claim relationships and qualifications to exact source spans; select emotional evidence independently of Harvester and keep full score/source receipts.
- [x] Test source-only, raw-spectrum and qualified packets on 12 fictional controls plus two retained reports; inspect fidelity and distinguish model replies from native abstentions. Preserve 192 calls, 60 native abstentions and four failed replies across four stages; fix lost correction/identity context.
- [x] Build the independent native qualification boundary and emotional world compiler; verify exact source/target/request binding, retain failed replies and prepare full-source requests for all 400 retained articles. This is offline contract verification, not automatic extraction accuracy or a registered worker.
- [ ] Complete fidelity review and real-source evaluation before promoting an articulation model or replacing production intake.

First implementation decision: reuse the existing retained-source export and Python/PyTorch model tooling for the Phase 1 bench. No classifier framework, generated summaries, invented calibration mapping, or automatic emotional interpretation from score maxima.

October 7 progress: exported 40 canonical retained articles (35 publishers; FOOTBALL, NFL and NBA) through a read-only production transaction. The source-only sample includes injuries, transactions, recaps, opinions, non-English reporting, galleries, statistics and quiet/low-relevance items; it is not a gold or representative accuracy set. Added eight synthetic attribution, negation, mixed-emotion, time, unrelated-entity and sarcasm controls. The three emotion encoders receive all 374 real-source windows plus the controls. The matched Laya comparison uses the exact first production window of 20 articles plus the same controls; excerpt scope, original body hashes and byte ranges are explicit, and excerpt coverage is not called full-article coverage.

The user requested a Laya comparison. The installed English Laya checkpoint (`55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851`) is included with 28 independent emotion predicates and continuous scores. Current gatekeeper cutoffs are excluded from this bench. It is compared with Horizon small, ModernBERT-GoEmotions and the SamLowe RoBERTa reference; the latter is MIT and is an evaluation reference, not a deployment selection.

The initial token-overflow replay explicitly failed complete-source checks on 32–33 items per encoder. Its failed receipts are retained in the isolated run directory. The comparison initially reused `harvester::cognition::prepare_text`. Windowing now lives in `tools::source::windows`; Classifier imports the source tool directly, validates its exact UTF-8 spans and tokenizes each shared window without truncation. Coverage failures remain errors, never zero scores. Archbox's GTX 1070 Ti exists, but its installed PyTorch CUDA build excludes `sm_61`; the first comparison uses CPU with two threads and records that GPU limitation.

Code and raw run directory: `examples/classifier_{export.sql,windows.rs,replay.py}` and `/mnt/data/backup/scoracle/classifier-launch/2026-10-07/` on Archbox. `examples/test_classifier_replay.py` verifies original Unicode byte ranges, refuses omission of a late qualification and checks trained label identities, target scope and review evidence. The archived probe retains the corrected neutral question descriptions. Completed results and the evaluation decision follow below.

Bounded-comparison decision: the initial complete-article Laya run took 57,409 ms and 89,464 ms for its first two articles with 28 independent emotion questions. It was stopped with completed receipts preserved, rather than extrapolating a production throughput claim or requiring an hours-long experiment before inspecting any quality. This checkpoint contains 421,293,830 stored weight elements, so it is not the proposed 149M specialist. CPU, two threads, precision, question wording and model revisions are retained; GPU execution fails a measured kernel probe with the installed CUDA build. The user specifically requested comparison rather than a predetermined encoder or LLM choice.

Phase 1 completed: [measured comparison and recommendation](classifier-spectrum-comparison-2026-10-07.md). Verified 144 full-source encoder receipts and 112 matched receipts: complete declared input, 28 finite scores, no truncation or errors. Matched median CPU latency per real opening window was 159 ms Horizon, 254 ms SamLowe, 321 ms ModernBERT-GoEmotions and 24,827 ms for the English Laya 28-question bank. These are measurements of this configuration, not daily-corpus throughput or model accuracy. The three specialists took 60.22, 97.82 and 136.37 seconds respectively for the 40 complete retained sources.

Laya can measure continuous predicates and ordinal intensity without text generation. Its baseline emotion questions nevertheless scored explicitly denied joy/relief highly. A separate five-question probe improved relief negation when scoped to Alex, but current-time and target relevance remained unreliable. The initial neutral question had conflicting answer descriptions; those receipts are preserved, its scores excluded from quality conclusions, and a corrected neutral probe was run separately. The documented two-option `choice` workaround was also tested: it still attributed current optimism to an old quote and failed to separate unrelated-team reporting reliably. SDK confidence and calibration claims do not establish Scoracle calibration.

Initial comparison decision: retain Laya as a measured candidate, not the sole gatekeeper by default. The user subsequently excluded it, then restored it for GPU comparison and requested research into Jev-style classification implementations. Carry Horizon small and SamLowe forward as fast emotion baselines; compare target relevance/topic separately on reviewed sources. None of these article/window emotion vectors identifies the speaker, time or supporting phrase by itself. In particular, past hope and another team's joy are legitimate text-level signals but cannot establish current target mood. Source byte ranges bind the input; they are not model explanations.

The current session implements the plumbing and model swap boundary. The revised GPU experiment, real-model correctness qualification and independently reviewed gold set move to the separate calibration session. The independent workers are implemented but have not been deployed or enabled in production. The 8,000–9,000-item replay, optimized GPU throughput, sports calibration, downstream factual/emotional support and code replacement of Harvester/Editor remain open. The retained source sample also contains publisher navigation/paywall/gallery furniture, so source acquisition quality must be reviewed alongside model quality.

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
- Laya inference and SDK question helpers were removed at this phase, then restored for the later user-requested GPU comparison. The exact original and corrected-neutral scripts remain archived privately; current helpers retain corrected neutral wording and reject source/question truncation.
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

## Native Classifier data flow — October 7 continuation

Implemented [the native Classifier core](../src/plugins/classifier.rs) and [offline qualification replay](../examples/classifier_qualify.rs). Neither imports or calls Harvester, Editor or Laya. The core prepares a complete-source qualification request, converts model-copied text and its declared occurrence into exact UTF-8 byte spans, validates the existing `classifier-qualified-claims-v1` contract, and compiles a provisional emotional expression world with complete supporting paragraphs. The existing Python validator accepts its output without a compatibility format.

The reply contract asks for literal claims and qualifier text rather than generated summaries, byte arithmetic or invented probabilities. Model-proposed source usability and completeness remain provisional assertions. Native checks reject missing source quotes, invalid occurrences, source/hash or candidate-target drift, duplicate spans, unsupported emotion labels, missing speaker/subject, and unanchored denial, correction or known-time fields. Exact matching establishes literal integrity; it does not prove that the model selected the right relationship, time or occurrence.

Receipts keep original source/provenance, the complete preparation request and its full SHA-256, model/checkpoint identity, supplied generation provenance, raw reply, provisional qualifications, native selection and explicit unmeasured families. Malformed replies and request drift remain retained failures. Expression input excludes numeric scores, hashes, review metadata and candidate label names. Unknown emotional target relationships or required event time produce `unresolved` with no expression world. Empty eligible emotion produces a character-specific selection record; it does not delete reporting, establish item-wide irrelevance or schedule production work. Every replay result has `production_eligible=false`.

Checks: 12 retained fictional annotation records round-trip through model-shaped proposals into equivalent qualification records with unchanged source spans; selections are five candidate worlds, four empty emotional selections and three unresolved worlds. These replies were mechanically derived from the fixtures, **not generated by an extraction model**. Two injected failures retain their raw replies and source/request context and make the replay exit unsuccessfully. All 400 retained sources produced 773 candidate-target requests; original bodies and full request SHA-256s were verified unchanged. The Rust library passes 369 active tests (64 environment-dependent tests ignored); four Python checks pass and the torch-dependent check is skipped locally.

Prepare requests and decode model replies separately:

```sh
cargo run --example classifier_qualify -- SOURCE.jsonl REQUESTS.jsonl
cargo run --example classifier_qualify -- SOURCE.jsonl REPLIES.jsonl RECEIPTS.jsonl
cargo test --lib plugins::classifier
```

Each reply envelope supplies `article_id`, `target`, `model`, `revision`, `request_sha256` and `raw_response`; extra generation provenance is retained. Outputs are created exclusively rather than overwriting prior receipts. Private checks and prepared source requests are under `/private/tmp/scoracle-classifier-native-20261007/`; publisher text remains outside Git.

**Remaining boundary:** preparation makes no inference call and marks the token budget `unverified_do_not_submit`. Bind the actual model/tokenizer, verify the full chat-template input plus reserved output budget before inference, and retain completion/coverage and generation receipts before running automatic qualification. Oversized sources need an explicit coverage-preserving disposition. Evaluate extraction and relationships independently against reviewed sources before accepting a model. Production registration, SQL persistence, all-consumer routing and character delivery cutover are still pending; no production model or intake path changed in this continuation.

## Target design — October 7, 2026

This is the governing target for the Classifier launch. It incorporates the user's clarified design and supersedes the earlier assumption that classification should be a cheap preliminary gate, a fixed model-size target, and the separate Harvester/Editor admission path. The execution ledger above records what is actually implemented; this section records what the finished product must do.

### Deconstruct, retain, build, discover and express

Classification is valuable intelligence work. The Classifier deconstructs publisher reporting into a spectrum of source-backed signals and relationships. SQL retains that measured world. Rust builds the evidence payload for each character. Character AI discovers what the supplied evidence supports and expresses a useful reading. SQL and Go retain and serve the finished products.

The same reporting can support several different jobs. An availability update can matter to Scout without expressing emotion. A denied transfer can matter to Insider and Journalist even though no transfer happened. A report can contain both hope and worry, expressed by different people at different times. Accurate deconstruction lets every character work from those distinctions instead of reconstructing them independently from a pile of articles.

The central design decision is:

> **Relevance is derived from the spectrum of evidence needed downstream. A complete, usable measurement with no supported signal for any character produces no new character work.**

The binary decision to do work is a projection of that richer record. There is no separate AI yes/no relevance test before the Classifier and no delegation to Harvester or Editor. Target relevance is still measured inside the spectrum: direct subject, opponent context and incidental mention distinguish whose reporting a signal belongs to.

### End-to-end ownership

```mermaid
flowchart TD
    G[Google News RSS: discovery and ranking] --> F[Native acquisition: full publisher source]
    F --> C[Independent Classifier: spectrum and qualified claims]
    C --> S[(SQL: source, measurements and provenance)]
    S --> R[Rust: select evidence and build each character world]
    T[Structured stats, fixtures and dated history] --> R
    R --> E{Supported evidence for this character?}
    E -->|Yes| A[AI: discover supported meaning and express it]
    E -->|Complete measurement, none useful| N[No new character call; source and scores retained]
    E -->|Unknown or incomplete| U[Visible unresolved state; existing review or retry work]
    A --> V[Native validation and atomic publication]
    V --> P[(SQL: finished products and history)]
    P --> API[Go: queries, cache and API]
    API --> Client[Client]
```

| Stage | Responsibility | Intelligence and cost boundary |
|---|---|---|
| Google News / Go RSS intake | Discover and rank candidate reporting; retain query, rank, URL, publisher and candidate identity. | Reuse Google's discovery signal without a model call in intake. A search match is a candidate, not proof of target relevance. Native network and ingest work still exists. |
| Native acquisition | Resolve publisher URLs, fetch or reuse full text, normalize and deduplicate, retain canonical IDs and exact source bytes. | Ordinary code owns collection. Failed extraction and publisher furniture remain quality issues, not negative relevance. |
| Classifier AI | Measure independent signals and identify source-supported claims and their relationships, qualifications and time. | Spend intelligence where semantic understanding is necessary. An encoder, LLM or measured combination may serve this job. |
| SQL and native analytics | Store source and derived records separately; query history, count, aggregate and calculate bounded factual measurements. | Reuse evidence and arithmetic. A stored AI interpretation remains an interpretation; SQL does not turn it into a verified fact. |
| Rust character plugins | Select each character's scope, useful signals, exact supporting source, compatible history, statistics, limits and instructions. | Deterministic preparation builds a coherent world without prewriting the model's conclusion. |
| Character AI | Reason across that world, discover supported relationships or patterns, and express the character's reading. | This is the second semantic stage. The model reads prepared evidence; it does not retrieve more material or invent missing facts. |
| SQL + Go serving | Retain accepted products, query them, cache and serve them to clients. | Client reads reuse saved work. Normal product requests do not repeat classification or character inference. |

“Cheap” and “expensive” describe where the architecture concentrates work, not measured prices or fixed resource promises. Acquisition, storage and analytics have costs too. Classification quality comes first; runtime cost and throughput are measured on the real workload.

### Spectrum-derived relevance

“Nothing registers” means **no supported, actionable evidence under the evaluated policy for the processed target and downstream character scope**. It does not mean every raw score equals zero. Independent sigmoid heads generally produce nonzero numbers, and different dimensions need different evaluated interpretations. A high generic emotion score can describe someone other than the target or an old quotation. A low emotion score can accompany important factual reporting.

Rust applies explicit, versioned selection rules over the measured spectrum and qualified source evidence. These rules check the target relationship, signal meaning, source support, time, qualification and the character's actual job. They produce scheduling decisions without asking another model to repeat a binary relevance judgment. Keep the dimensions and decisions inspectable; do not collapse them into one global confidence number.

Two scopes matter:

- **Character scope:** no eligible emotional evidence can mean no Influencer call while the same source still serves Scout or Journalist. Each character chooses its own slice.
- **Item/target scope:** no new character work is warranted only when every applicable consumer has a complete, usable measurement for its required signals and none has eligible evidence. This conclusion applies to the processed scope; it does not declare the article irrelevant to every other entity or future use.

| Measured situation | Required disposition |
|---|---|
| Supported signal with the necessary target, time and qualification | Make it available to the relevant character. Coalesce with its other evidence and avoid duplicate calls when the prepared world has not changed. |
| Complete usable measurement; no eligible signal for this character | Record the disposition and omit that character call. Preserve the source, vectors and other characters' eligibility. |
| Complete usable measurement; no eligible signal for any applicable consumer in scope | No new character work. Keep the retained record available for history, aggregation, audit and future remeasurement. |
| Untrained family, uncertain relationship, insufficient support or missing time required by the task | Keep the affected value or disposition unknown/unresolved. Do not turn it into zero, neutral or a completed “nothing useful” decision. |
| Fetch failure, unusable extraction, unsupported labels, changed source, missing windows, truncation or inference failure | Record the failure and use existing durable work/retry mechanics where appropriate. Never publish an all-zero replacement result. |

There is no need to manufacture a character card to say nothing happened. There is also no need to call an articulator merely to confirm the compiler's completed no-evidence decision. An unresolved input can prevent a call without becoming evidence of absence.

Weak signals can still contribute to a later reporting-period study even when they do not trigger an immediate character call. No action means no new generation now, not deletion from the measured world. Mixed evidence, denials and corrections are meaningful signals; their polarity must survive selection.

### Examples that define the gate

These are design controls, not claims about measured model accuracy or numeric scores.

| Source situation | Spectrum and relationships | Downstream consequence |
|---|---|---|
| A target player is unavailable; no emotion is expressed. | Target availability/reporting evidence is present; attributed emotion may be absent. | Scout and possibly Journalist can use it. An emotion-only absence cannot discard the article. |
| A different club's player celebrates while the queried club is an incidental mention. | Emotion is present in the document; subject/target relationships bind it to the other club. | Do not assign that joy to the queried club. Evaluate other resolved targets within the actual processing scope. |
| “Alex denies agreeing to a move.” | Move topic, explicit denial and attributed subject are present. | Insider/Journalist can express the denial; no completed transfer or roster change is inferred. |
| Alex expressed hope in an old interview. | Optimism, attributed speaker and historical event-time evidence are present. | Keep it as dated history where appropriate; it does not establish current target mood. |
| A relief claim is withdrawn and replaced by Alex's current worry. | Correction, withdrawn claim, current worry and their source relationships are distinct. | Preserve the full supporting context and select the current claim; never revive the withdrawn feeling. |
| The complete source contains no evidence useful to any applicable character for the processed target. | Required signal families and relationships were measured successfully; all character selections are empty. | Retain the receipt and skip new character work. |
| Only the emotion head ran, or the publisher returned an interstitial. | Required relevance/topic/relationship evidence or usable source coverage is missing. | Unknown/incomplete, never a successful “nothing registered” result. |

### The reusable measured world

The [vector schema](../fixtures/classifier/vector-schema-v1.json) remains the concrete vocabulary: **51 independent presence dimensions, three target ordinals and seven claim qualifiers**. The dimensions include relevance, topic, emotion, discourse and temporal framing. The qualifiers bind speaker, subject, counterparty, reported event time, negation, uncertainty and source disagreement to exact source spans. A presence vector alone does not extract these relationships.

Every measured record needs enough information to distinguish support, absence, ambiguity and failure:

- Canonical article/source IDs, original retained text and hash, publisher, URL, report date, query provenance and known duplicate references.
- Exact complete input windows and byte ranges; model-visible coverage, token counts and explicit truncation or acquisition failures.
- Model/head/tokenizer and schema versions, input scope, supplied target identity where relevant, original scores and calibration status. Missing or untrained dimensions stay unknown.
- Exact source claims and supporting qualifier/identity spans, including competing or withdrawn statements. Literal matching proves source binding; relationship correctness requires separate evaluation.
- Target- and character-specific selection decisions, their policy versions and reasons. A decision never overwrites the underlying measurement.
- Native statistical inputs, dates, samples, cohorts and calculation provenance kept distinguishable from semantic model outputs.

Document/window measurements may be reused across characters. Target-conditioned measurements remain bound to their particular target. Reuse unchanged source/model/scope results instead of independently reclassifying the article for each character; recompute when the actual source, scope, head or model version changes. Do not pretend one document-wide vector represents every player in it.

Classification records describe reporting and its evidence. Topic presence does not confirm an injury or transfer, model confidence does not measure a person's emotional intensity, and report time does not supply missing event time. Confirmed identity changes and trusted structured records retain their own native acceptance rules.

### Rust prepares; AI discovers and expresses

The character plugin chooses its world from measured evidence, history and trusted structured records. It supplies the selected exact claims, complete supporting source paragraphs, named relationships, relevant dates and qualifications, and meaningful gaps or disagreement. The world carries evidence and constraints without prescribing a headline, sentiment or conclusion.

Keep two views of this work:

- **Audit/selection view:** original scores, scope, hashes, model versions, review status, source bindings, explicit unknowns and policy decisions. SQL and Rust use this record for selection, studies and reproducibility.
- **Expression view:** supported source wording and relationships, relevant history, task instructions, and native statistical measurements needed by that character. Raw text-label probabilities, checkpoint hashes and provisional review bookkeeping are not prose clues. Trusted performance values, percentiles and computed trends can still be legitimate numerical evidence.

Do not replace the original evidence with an intermediate generated summary. Preserve the supporting paragraph when a quotation depends on an antecedent, denial, correction or identity elsewhere in that paragraph. Validate the actual token budget before inference; oversized evidence needs a visible disposition rather than silent clipping or an invented summary.

Character AI has room to discover meaning across the supplied evidence, including supported mixtures, tensions and changes over time. Its job extends beyond paraphrasing, but its claims must remain traceable to that world. Native acceptance checks structure, source integrity, qualifications and product-specific constraints before atomic publication. Structural validity alone does not prove semantic fidelity.

The downstream dependency order remains:

```mermaid
flowchart TD
    W[Measured evidence + selected history and structured data] --> J[Journalist]
    W --> I[Insider]
    W --> V[Influencer]
    W --> S[Scout]
    S --> A[Analyst: Scout + Influencer + dated computed trends]
    V --> A
    J --> O[Oracle: five finished character cards]
    I --> O
    V --> O
    S --> O
    A --> O
```

Each character owns its inference and accepted product. Analyst studies finished Scout/Influencer readings and native trajectories. Oracle synthesizes five finished cards and their explicit missingness and provenance. Repeated cards based on one article are not independent confirmation.

### Quality first; choose compute from evidence

The target does not require an LLM, an encoder-only bank, a particular parameter count or a particular inference cost. Classification and relationship extraction need serious semantic accuracy. Choose the smallest measured implementation that meets the job, and use a stronger resource when the evidence shows the smaller one fails. A roughly 149M encoder is a benchmark candidate, not a design ceiling. Smaller articulators are also candidates rather than a promised consequence.

Evaluate signal detection, target/time/qualification accuracy, evidence extraction, calibration, native no-action decisions and downstream expression separately. Fast scoring and successful JSON cannot substitute for those checks. Spend compute on reusable deconstruction and evidence-based discovery; keep collection, arithmetic, persistence, payload assembly and serving in native tools.

Prune anything that does not serve this contract: the standalone headline relevance gate, the Harvester/Editor classification/routing dependency, generated editorial intermediates, obsolete review formats, unsupported score conversions and unused experiment machinery. Retain reusable acquisition/source tools, canonical identities, history, source validation, durable work receipts, claim fencing and atomic publication. Do not keep a legacy fallback through the old plugins in the finished Classifier design.

### What is built and what remains

| Capability | October 7 state |
|---|---|
| Complete source windows and source/checkpoint-bound emotion measurements | Implemented and tested offline. Emotion presence is not independently validated current-target attribution or sports calibration. |
| Qualified claim validation, character evidence selection and supporting-context compiler | Implemented in Python and native Rust, checked offline with provisional annotations. Native selection preserves unresolved target/time separately from empty eligible emotional evidence. |
| Automatic source claim/relationship extraction | Native request/decode/source-binding and tokenizer-bound llama.cpp inference implemented; semantic evaluation remains pending. Literal span checks do not establish extraction accuracy. |
| Additional presence heads and target ordinals | Schema/review/training preparation exists; real-source fitting, independent gold and held-out validation remain pending. Missing values stay unknown. |
| All-consumer spectrum-derived no-action policy | Target design, not a demonstrated production gate. The current emotion-focused abstention test does not establish item-wide irrelevance. |
| Articulation model selection | Three candidates tested; fidelity errors remain and no model is promoted. |
| Independent Classifier storage and registration | Workers, migrations 291–295, RSS producer wiring, periodic backlog replay and Journalist/Influencer/Insider delivery are implemented and checked locally. Production activation and the other direct character readers remain; cognition stays paused. |

### Current plumbing boundary — October 8

`classifier/prompt.rs` is the model update surface. `DEFAULT_MODEL` selects the local default; deployment can override `COGNITION_ROUTE_CLASSIFIER`, `_BACKEND`, `_BASE_URL` and `_THINK` through the existing router. `VERSION`, the system/input prompt, schema translation, temperature, context window, output reservation and input-admission policy are together in that file. Shared provider code owns transport and exact tokenizer preparation, not model tuning. A different transport protocol still needs an actual model adapter; prompt edits cannot make an unsupported runtime protocol work.

The model returns the fixed literal-quote proposal with optional sparse measurements. Native Classifier owns UTF-8 span binding, validation and the typed `Source`/`Record`/`Measurements` consumer contract. `load_measurement` binds receipts to the acquired source table and reconstructs both claims and the stored envelope from the retained reply; even source-valid altered claims are rejected. Every receipt has all 51 presence dimensions and three ordinals, source hash, full byte extent, target and schema version. Unsupported values remain null. Raw presence values are [0,1] without a calibrated absence/presence interpretation; ordinals use integer schema anchors. Positive presence and every known ordinal require exact evidence. Calibration is `unassessed`; receipts remain provisional. Journalist, Influencer and Insider now read Classifier delivery records in the local implementation. Scout still reads legacy delivery; Analyst/Oracle still consume the existing finished products. None of this has been deployed.

Migration [291_classifier_plumbing.sql](../../sql/migrations/291_classifier_plumbing.sql) adds source snapshots and attempt receipts. [292_classifier_acquisition_intake.sql](../../sql/migrations/292_classifier_acquisition_intake.sql) adds shared discovery revision/enqueue/replay functions. The updated Go RSS collector atomically retains metadata, candidate-query edges and acquisition intent; it no longer requires Harvester flags or queues Harvester. Acquisition replays retained discovery at startup and periodically in 1,000-item batches, preserving unchanged active/failed work and skipping acquired revisions. `COGNITION_STAGES=classifier_acquire` retains sources and queues classification without inference. These changes are not deployed.

Deployment order: apply migrations 291/292/293/294/295, then install the matching Go producer and Rust worker. Preserve the production cognition pause. Separately enabled acquisition is the first activation target; character cognition stays paused until the remaining flow is ready. Do not use the existing full-release script unchanged: it restarts cognition.

Successful measurements are reused only when complete request settings, source/target identity and an exposed immutable model artifact revision match. Model changes retain separate receipts. Providers that cannot expose a revision rerun rather than reuse mutable tags. Errors retain raw provider replies when available, commit as diagnostic progress, and return to the existing worker retry/dead-letter policy. Acquisition survives later model failures. Source changes and superseded claims cannot publish current results. A stale acquired snapshot atomically queues native reacquisition and completes the outdated Classifier claim. A model attempt overtaken by source changes is retained as an error against its historical snapshot.

Input admission now requires exact tokenizer evidence. The native llama.cpp adapter renders the full chat template, tokenizes it and submits the resulting token IDs unchanged. The full input plus the actual wire output reservation must fit both the requested 16,384-token window and the smallest serving slot. Successful completions must report the exact preflight input count, EOS and no truncation. Oversized sources fail durably without clipping; coverage-preserving handling of longer sources remains open. Ollama and compatible-chat providers currently lack this proof and are refused before generation. Their serializer fixtures do not establish live token coverage. Native server metadata is fenced across preparation/inference, but does not expose a verified immutable weights digest; its receipts therefore remain ineligible for artifact-based reuse.

Run the model boundary without database writes:

```sh
COGNITION_ROUTE_CLASSIFIER_BACKEND=llamacpp \
COGNITION_ROUTE_CLASSIFIER=qwen3:4b \
COGNITION_ROUTE_CLASSIFIER_BASE_URL=http://127.0.0.1:11438 \
cargo run --example classifier_model -- SOURCES.jsonl RECEIPTS.jsonl
```

Live Mac native-server smoke: the same fictional source was submitted through llama.cpp b11496 with the already-installed `qwen3:4b` and `granite4.2:3b` GGUF files. Preflight/consumed input counts matched exactly: **1,194/1,194** and **1,233/1,233** tokens, respectively. Both reached EOS without truncation; both replies were structurally rejected because they declared unusable/incomplete extraction. Exact requests, preflight and failed replies are retained under `/private/tmp/classifier-native-{qwen,granite}-proof-20261008.jsonl`. The live Ollama route was refused before generation with missing-tokenizer evidence. These are runtime/coverage/error checks, not accuracy results or model promotion. The isolated native servers were stopped afterward. Serve one configured model alias with `/props`, `/slots`, `/apply-template`, `/tokenize` and `/completion` available; disable context shifting. See the [native server reference](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md).

Changing the route selects another model; tuning stays in `prompt.rs`. The runner preserves replies and failures in a new receipt file and does not score semantic accuracy. Durable integration check: `TEST_DATABASE_URL=<disposable classifier_test database> cargo test --lib durable_acquisition_swap_reuse_retry_and_claim_fence -- --ignored --nocapture` against an empty disposable database whose name begins `classifier_test`. The check creates its minimal discovery tables and applies real migrations 102/109/256, the existing publication-outbox migrations and Classifier migrations 291–295. It verifies acquisition → queue → inference → storage → typed consumer reads, artifact-aware reuse, failure/retry survival, unknown-revision behavior, changed source rejection and lease fencing.

### Journalist delivery boundary — October 8

Migration [293_classifier_character_delivery.sql](../../sql/migrations/293_classifier_character_delivery.sql) retains per-measurement/plugin obligations and versioned policy reasons. Classifier records a held Journalist obligation atomically with each successful measurement, including cached reuse. New measurements supersede older held/pending obligations for the same canonical target and article; used receipts remain history. Calibration and target-selection uncertainty remain explicit. This session adds no score threshold or automatic eligibility promotion.

When a separately evaluated obligation becomes eligible and pending, the database dispatches its character work in the same transaction. Duplicate ready updates do not reset an active lease or parked failure. Migration 294 extends the dispatcher to Influencer; it rejects unwired destinations. The production acquisition process can still run independently while character cognition remains paused.

Journalist builds its writing payload from complete retained publisher text and native proposed claims. Each claim preserves target relationship, kind, time and literal qualifiers, including explicit unknowns. Scores and byte/receipt identifiers remain outside the writing surface. Payload changes invalidate its fingerprint; changed relationships are new material even when the source text is unchanged. Exact duplicate worlds reuse prior coverage. Historical finished Harvester-backed reports remain self-memory; there is no active legacy delivery fallback.

Publication locks and rechecks the current discovery revision, candidate names, retained receipt and ready obligation inside the existing queue-claim transaction. Products, delivery dispositions, completion and the Oracle outbox remain atomic. Batches checkpoint independently and recover through the existing queue. Missing/future dates, unusable input and oversized complete reports return to a held disposition; they do not overwrite the last useful product or declare the reporting irrelevant.

The local check uses five fictional sources: four dated reports publish in two batches, while the undated report remains held. It explicitly releases fictional controls to test plumbing; real unassessed receipts remain held. It also injects source drift, lease replacement, a busy delivery, product failure and outbox failure. The actual publisher and prompt assembler run in the test; no semantic accuracy is scored.

```sh
TEST_DATABASE_URL=<disposable classifier_test database> \
cargo test --lib classifier_journalist_delivery_publication_and_recovery -- --ignored --nocapture
```

Remaining direct consumer: Scout's independent identity/structured-record acceptance. Their old delivery readers and routing thresholds are not a fallback for the new Classifier. Verify their replacements before removing the remaining Harvester/Editor registration and delivery code, then check Analyst/Oracle reactions and serving compatibility.

### Influencer delivery boundary — October 8

Migration [294_classifier_influencer_delivery.sql](../../sql/migrations/294_classifier_influencer_delivery.sql) adds atomic `vibe` dispatch to the existing delivery trigger. Each successful Classifier measurement now retains separate held obligations for Journalist and Influencer; neither is released by raw scores. Per-plugin publication never consumes the other character's evidence.

Influencer reads its own eligible receipts, anchors backlog work to the original reporting calendar and retains accepted reporting from the preceding 30 days through the existing DuckDB study. Fresh sources and attributed history carry complete source text and proposed literal claim relationships; unknowns remain explicit. Pending sources must match current discovery. Historical consumed receipts retain their immutable source snapshots. Copy detection includes the complete Classifier world, so changed relationships cannot disappear behind a duplicate flag. The capture cutoff bounds measurement availability to the captured UTC second; a newly released delivery supersedes the active queue claim.

Publication rechecks and locks all supplied fresh/history receipts, then atomically writes the period product, delivery dispositions, completed attempt, outbox event and claim completion. Unknown publication dates return to held obligations instead of acquiring today's period. Input/selection/budget failures remain retryable work. The prompt contract is `vibe-frame-v20-classifier-world`; old generation receipts remain immutable, and old attempt hashes are not reused. The four source-only quality fixtures retain their existing structural expectations with current prompt metadata; no new semantic evaluation is claimed.

```sh
TEST_DATABASE_URL=<disposable classifier_test database> \
SCORACLE_MEMORY_STUDY_BIN=<built go/cmd/memory-study> \
cargo test --lib plugins::influencer::tests::period_card_flow -- --ignored --nocapture
```

Verification: 372 active library checks and `cargo check --all-targets` pass. Explicitly run acquisition, Journalist delivery and Influencer period-card database checks pass against disposable Postgres, using the existing Go/DuckDB helper. No production deployment or real-evidence release occurred. Next: Scout and Analyst/Oracle dependency checks.

### Insider delivery boundary — October 8

Migration [295_classifier_insider_delivery.sql](../../sql/migrations/295_classifier_insider_delivery.sql) extends dispatch to `transfers`, permits the publisher's existing coach/person queue targets and adds `classifier` to the transfer product trigger vocabulary. Native acquisition resolves unique canonical name surfaces over complete retained source text using the existing database normalizer. Aliases, ambiguous surfaces, stale surface names and unsupported persons cannot create canonical targets. Matched player/coach identities become separate Classifier targets and therefore separate measurement/delivery obligations. Resolving an identity never establishes a move or current affiliation.

Snapshots retain immutable identity candidates and an identity hash. Source/name changes invalidate discovery revisions and yield a new snapshot. Classification and publication recheck those candidates under short source/name locks; a concurrent refresh or busy source causes retry. Insider's current writing packet preserves full source bodies, literal proposed Classifier relationships and canonical counterparties. Unknown/future dates and input failures preserve pending work. Source-grounded findings continue through the existing activity calculation, rumor publisher and scores. Denied reports retain null stages and never become active moves or roster mutations.

```sh
TEST_DATABASE_URL=<disposable classifier_test database> \
SCORACLE_MEMORY_STUDY_BIN=<built go/cmd/memory-study> \
cargo test --lib classifier_insider_identity_and_publication -- --ignored --nocapture
```

The fictional native flow checks team, player and coach measurements and independently held obligations, source-bound counterparties, ambiguity rejection, outbox rollback and restart/retry. The prompt is `insider-source-v7-classifier-world`; source-only quality expectations retain their original structural assertions with current metadata. Calibration, real-model promotion and production activation remain separate.

### Deferred accuracy qualification and production acceptance

These accuracy tasks belong to the dedicated calibration session. They do not block implementation or testing of the plumbing. The current Rust suite has 372 passing active tests; all three explicitly run disposable-database flow checks also pass. Production cognition remains paused until deployment is explicitly resumed.

1. **Review the real evidence.** Independently review extraction and claim/target/time/qualification relationships in the prepared 400-source packet. Resolve source groups and time cohorts before fitting or evaluation; provisional annotations remain distinct from gold.
2. **Validate the full required spectrum.** Fit/compare missing families and evaluate automatic claim qualification. Measure per-family false positives/negatives, ambiguity, calibration and complete-source coverage. Unvalidated families cannot supply a successful no-signal judgment.
3. **Test the derived gate.** Measure missed useful reporting and unnecessary character calls against independently reviewed character selections. Include availability without emotion, other-entity emotion, denials, corrections, historical claims, weak accumulated evidence, duplicates and every unknown/failure condition in the table above. Verify that a no-action disposition preserves source and measurements.
4. **Evaluate the actual runtime.** Measure complete-article and target-scoped throughput, peak bursts, CPU/GPU behavior, budgets, reuse and changed-version remeasurement on the real corpus. Do not extrapolate the opening-window benchmark into daily production capacity.
5. **Replace intake directly on main.** Register the independent Classifier, persist its versioned records and change character queries to its own delivery contract. Reuse native mechanics without calling or importing Harvester/Editor. Remove their scheduling, model bindings and obsolete delivery path together when the replacement works. The user does not require a prolonged shadow rollout; failures remain visible through ordinary durable work mechanics.
6. **Accept expression on held-out worlds.** Compare candidate articulators on the same supported inputs, examining factual and emotional support, attribution, time, denial/correction handling, missingness, usefulness, format and token limits. Preserve failed attempts. Keep model choice open until fidelity is demonstrated.

Success is a reusable measured world that makes character work more accurate and avoids inference where there is no supported job to do. It preserves useful gradations and uncertainty while giving each character a clear, evidence-backed scope in which to discover and express meaning.

## Sources

Historical benchmark references checked October 7, 2026. These do not commit the target design to a model or parameter count:

- [Horizon-Labs/multilingual-emotions-small](https://huggingface.co/Horizon-Labs/multilingual-emotions-small) — 141M, Apache 2.0, 28 GoEmotions labels, benchmark table, data and limitations.
- [HR26kk/modernbert-emotion-classifier](https://huggingface.co/HR26kk/modernbert-emotion-classifier) — Apache 2.0, 149M ModernBERT-base, six-label task and reported benchmark.
- [Pradeep-mahato/ModernBERT-GoEmotions](https://huggingface.co/Pradeep-mahato/ModernBERT-GoEmotions) — Apache 2.0, multi-label GoEmotions, independent sigmoid scores and limitations.
- [Hidden-States/roberta-base-goemotions](https://huggingface.co/Hidden-States/roberta-base-goemotions) — Apache 2.0 and model-card metrics for a GoEmotions classifier.
- [SamLowe/roberta-base-go_emotions](https://huggingface.co/SamLowe/roberta-base-go_emotions) — widely used GoEmotions reference checkpoint; license and comparison figures noted in the Horizon card.
- [Horizon-Labs/multilingual-emotions-base](https://huggingface.co/Horizon-Labs/multilingual-emotions-base) — 308M Apache 2.0 comparison model.
- [answerdotai/ModernBERT-base](https://huggingface.co/answerdotai/ModernBERT-base) — 149M base encoder, Apache 2.0, 8,192-token maximum sequence support.
