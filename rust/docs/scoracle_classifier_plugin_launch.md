# Scoracle Classifier Plugin

## Launch concept: measure a spectrum, then let characters express it

**Status:** Phase 1 comparison complete; reviewed gold set next — October 7, 2026

**Placement:** Immediately after RSS fetch  
**First goal:** Replace generation-based gatekeeping with evidence-linked, graded signals  
**Model strategy:** Start with compact task specialists; evaluate a roughly 149M encoder before considering larger models

---

## Execution ledger

The tracked plan is this file; the original was supplied from Downloads. The user's launch instruction authorizes replacing Harvester and Editor directly on `main`; protecting the broken production flow with a prolonged parallel rollout is not a requirement. Model quality, source integrity, uncertainty, coverage, and reproducibility still need evidence.

- [x] Preserve current development and consolidate it onto `main`, pushed through `7f49d5a2`. Correct five stale regression checks; 384 active Rust tests pass (65 database/model checks remain ignored without their dependencies).
- [x] Trace current intake: Go RSS provenance → Harvester headline gate → Editor acquisition/windowed Laya scores → source-bound classifications and character assignments. Existing source receipts, canonical IDs, worker claims, and transactional publication are reusable. Current routing is uncalibrated and the source/model experiments did not establish factual fidelity.
- [x] Phase 1: source-bound 40-item replay of three compact emotion encoders; matched four-model comparison on 20 opening windows plus eight synthetic controls. Full vectors, exact input windows, checkpoint identities, timings and limitations retained.
- [ ] Phase 2: independently reviewed 300–500 item gold set, with source-group/time splits and ambiguous/unknown labels.
- [ ] Phase 3: held-out quality/calibration, real-corpus CPU/GPU throughput, and evidence/attribution checks.
- [ ] Replace the two intake stages with one Classifier stage; retain versioned measurements separately from routing and source evidence. Run source/publication checks and a direct production cutover when the selected slice works.
- [ ] Supply measured evidence slices to characters and compare unsupported factual/emotional claims on identical inputs.

First implementation decision: reuse the existing retained-source export and Python/PyTorch model tooling for the Phase 1 bench. No classifier framework, generated summaries, invented calibration mapping, or automatic emotional interpretation from score maxima.

October 7 progress: exported 40 canonical retained articles (35 publishers; FOOTBALL, NFL and NBA) through a read-only production transaction. The source-only sample includes injuries, transactions, recaps, opinions, non-English reporting, galleries, statistics and quiet/low-relevance items; it is not a gold or representative accuracy set. Added eight synthetic attribution, negation, mixed-emotion, time, unrelated-entity and sarcasm controls. The three emotion encoders receive all 374 real-source windows plus the controls. The matched Laya comparison uses the exact first production window of 20 articles plus the same controls; excerpt scope, original body hashes and byte ranges are explicit, and excerpt coverage is not called full-article coverage.

The user requested a Laya comparison. The installed English Laya checkpoint (`55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851`) is included with 28 independent emotion predicates and continuous scores. Current gatekeeper cutoffs are excluded from this bench. It is compared with Horizon small, ModernBERT-GoEmotions and the SamLowe RoBERTa reference; the latter is MIT and is an evaluation reference, not a deployment selection.

The initial token-overflow replay explicitly failed complete-source checks on 32–33 items per encoder. Its failed receipts are retained in the isolated run directory. The comparison now reuses `harvester::cognition::prepare_text`, validates its exact UTF-8 spans, and tokenizes each shared window without truncation. Coverage failures remain errors, never zero scores. Archbox's GTX 1070 Ti exists, but its installed PyTorch CUDA build excludes `sm_61`; the first comparison uses CPU with two threads and records that GPU limitation.

Code and raw run directory: `examples/classifier_{export.sql,windows.rs,replay.py}` and `/mnt/data/backup/scoracle/classifier-launch/2026-10-07/` on Archbox. `examples/test_classifier_replay.py` verifies original Unicode byte ranges, refuses omission of a late qualification and checks the corrected neutral question descriptions. Completed results and the evaluation decision follow below.

Bounded-comparison decision: the initial complete-article Laya run took 57,409 ms and 89,464 ms for its first two articles with 28 independent emotion questions. It was stopped with completed receipts preserved, rather than extrapolating a production throughput claim or requiring an hours-long experiment before inspecting any quality. This checkpoint contains 421,293,830 stored weight elements, so it is not the proposed 149M specialist. CPU, two threads, precision, question wording and model revisions are retained; GPU execution fails a measured kernel probe with the installed CUDA build. The user specifically requested comparison rather than a predetermined encoder or LLM choice.

Phase 1 completed: [measured comparison and recommendation](classifier-spectrum-comparison-2026-10-07.md). Verified 144 full-source encoder receipts and 112 matched receipts: complete declared input, 28 finite scores, no truncation or errors. Matched median CPU latency per real opening window was 159 ms Horizon, 254 ms SamLowe, 321 ms ModernBERT-GoEmotions and 24,827 ms for the English Laya 28-question bank. These are measurements of this configuration, not daily-corpus throughput or model accuracy. The three specialists took 60.22, 97.82 and 136.37 seconds respectively for the 40 complete retained sources.

Laya can measure continuous predicates and ordinal intensity without text generation. Its baseline emotion questions nevertheless scored explicitly denied joy/relief highly. A separate five-question probe improved relief negation when scoped to Alex, but current-time and target relevance remained unreliable. The initial neutral question had conflicting answer descriptions; those receipts are preserved, its scores excluded from quality conclusions, and a corrected neutral probe was run separately. The documented two-option `choice` workaround was also tested: it still attributed current optimism to an old quote and failed to separate unrelated-team reporting reliably. SDK confidence and calibration claims do not establish Scoracle calibration.

Decision: retain Laya as a measured candidate, not the sole gatekeeper by default. Carry Horizon small and SamLowe forward as fast emotion baselines; compare target relevance/topic separately on reviewed sources. None of these article/window emotion vectors identifies the speaker, time or supporting phrase by itself. In particular, past hope and another team's joy are legitimate text-level signals but cannot establish current target mood. Source byte ranges bind the input; they are not model explanations.

Next work is the independently reviewed gold set and held-out relevance/attribution checks, followed by the smallest useful Classifier slice and direct cutover on `main`. No production Classifier stage has been deployed in Phase 1. The 8,000–9,000-item replay, GPU execution, sports calibration, downstream factual/emotional support and removal of Harvester/Editor remain open. The retained source sample also contains publisher navigation/paywall/gallery furniture, so source acquisition quality must be reviewed alongside model quality.

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
