# Local context harvesting trial — September 24, 2026

Historical experiment record. Its retired training, comparison, packet-archive and
alternative-adapter tools were removed during the September 27 cleanup. Commands
below document that experiment; they are not current operating instructions. See
[the cleanup handoff](harvester-cleanup-2026-09-27.md) for retained tools and owners.


Follow-up: [Laya domain training](harvester-laya-training.md) now provides a local
fine-tuning, calibration, and evaluation path. The zero-shot results below remain the
historical baseline, not a conclusion about domain-trained model quality.

The first working path is implemented and exercised on real sweep content:

`stored article → Harvester (opening + classification + policy) → verbatim context → trial database`

The classifier returns distributions over entity involvement, content type, four character interests, and emotional tone. Harvester copies publisher text and attaches the scores and source identity. There is no generated summary and no semantic regex filter. This is an offline trial; production Editor reads, queues, and publication behavior are unchanged.

## What exists

- `src/studio/decision.rs`: typed, provider-independent classification capability.
- `src/runtime/providers/system_one.rs`: HTTP adapter retaining the raw response.
- `src/plugins/harvester.rs`: independent plugin identity, manifest, provider injection, and the `Harvester::harvest` invocation.
- `src/plugins/harvester/cognition.rs`: Harvester-owned preparation, score validation, provisional policy, exact source extraction, and proposed context packets.
- `examples/context_harvest.rs`: corpus replay through the Harvester plugin, with per-article errors and immutable output files.
- `examples/harvest_laya_server.py` and `harvest_decider_server.py`: local checkpoint adapters, bound to loopback. Model downloads are setup; article inference runs locally.
- `examples/harvest_export.sql`: read-only export using the sweep's original query team and sport, rather than inferred entity links.
- `examples/harvest_store.py`: isolated SQLite persistence of original text, accepted excerpts, provenance, and proposed character contexts. It verifies source offsets before insertion.

Inputs include the headline and up to three opening paragraphs, bounded at 160 words and 1,800 UTF-8 bytes. Existing fetched text flattens paragraphs, so this corpus uses an explicitly marked opening-prefix fallback. All retained excerpt characters come directly from the original stored body. Full bodies stay in the experiment archive for inspection; downstream context contains the excerpt. Source text remains attributed evidence rather than instructions or verified truth.

All seven question distributions are checked for exact option sets, finite probabilities, normalization, and a consistent winning choice. Missing provenance or reported truncation fails the record. Uncertainty produces `review`; transport and protocol failures produce `error`, not rejection. Query identity and existing Editor results are retained separately; the old Editor's result is never supplied to the classifier.

## Corpus and measurements

At export time, today's sweep had 5,631 fetched articles, of which 1,082 had stored bodies longer than 200 characters. The frozen 120-article sample spans FOOTBALL, NBA, and NFL and is ordered by an article-ID hash. It contains articles that already have Editor reads and usable stored bodies, so it is biased toward the current pipeline's readable material. It is not a random sample of all new arrivals.

Trials ran on the local M4 Mac mini, not Archbox. Laya used two CPU threads; Decider used MPS. Measurements include the Rust client's per-article request and response time after server startup, with seven classification questions. They do not include article fetching or checkpoint loading.

| Candidate | Cases | Accept / review / reject | Median | p95 |
|---|---:|---:|---:|---:|
| Laya multilingual, CPU, question v1 | 120 | 20 / 86 / 14 | 733 ms | 1,025 ms |
| Decider 0.8B, MPS, question v1 | 40 | 19 / 21 / 0 | 3,745 ms | 4,722 ms |
| Decider 0.8B, MPS, question v2 | Same 40 | 19 / 19 / 2 | 3,655 ms | 4,635 ms |
| Decider 2B, MPS, question v2 | 12 selected diagnostics | 9 / 3 / 0 | 5,416 ms | 7,531 ms |

These runs had no inference/protocol errors. Accepted counts are policy outcomes, **not accuracy measurements**. The question-v2 change explicitly repeats the query entity in the entity question and clarifies the content categories. It was explored on the same cases, not evaluated on a held-out set.

The trial database stores 66, 24, 25, and 16 proposed character contexts respectively. These are alternatives from separate experiments, not cumulative production deliveries. The 2B run uses twelve deliberately selected successes and failures, not a comparable random sample. It recovered the Chiefs and Norwich injury stories but still admitted the Como/Kane mismatch and a Yahoo scoreboard opening under the original policy.

## What the examples establish

- Article 755102, “Andy Reid provides positive injury updates on 3 Chiefs starters,” is clearly relevant to the Chiefs. Laya assigned `absent` 0.975 and would reject it. Decider accepted it for Journalist and Scout. The existing generative Editor also labeled it irrelevant, so that baseline cannot serve as ground truth.
- Article 751801, Nets training-camp invitations, passed Decider for Journalist and Insider. Laya's uncertain content judgment sent it to review.
- Article 757864, a Norwich player sidelined with a shoulder injury, was another clear relevant opening Laya would reject.
- Article 758857, Harry Kane considering NFL kicking, was queried for Como. Decider 0.8B still accepted it despite the opening not supporting that connection. Repeating the query entity in question v2 did not fix this example.
- Some stored openings contain publisher navigation before the story. Preserving article-body paragraph boundaries and excluding DOM navigation during acquisition matters: the classifier cannot recover a story opening it was never given.

The mechanism works locally today. The tested small models do not yet justify irreversible filtering on their scores. In particular, the downstream model's final say can catch a false positive it receives, but cannot recover an article discarded upstream. A useful first live mode would retain uncertain and sampled rejected material for evaluation while proposing routing, after production contracts are aligned.

The measured runs above use policy v1: reject when `absent` or an individual unwanted-content score reaches 0.90; accept when non-absence reaches 0.55, reporting reaches 0.50, and at least one character's usefulness reaches 0.50; otherwise review. Inspection exposed an overly permissive assumption: non-absence includes background mentions. Current code uses policy v2, requiring combined `subject` + `opponent` score of at least 0.55 instead. The original packets remain unchanged. A final end-to-end 2B run on the same twelve diagnostics produced six accepts, six reviews, zero rejects/errors, and eleven proposed contexts in 71.27 seconds. The Como/Kane case moved to review, while the Chiefs and Norwich injury stories still passed. This is a diagnostic correction, not evidence of general accuracy improvement. These numbers remain uncalibrated scores, not measured confidence guarantees. There is no evidence yet that they achieve a particular recall or clickbait-removal rate.

## Reproduction

The current run's private artifacts are ignored by Git under `logs/context-harvest-20260924/`: corpus JSONL, per-candidate packets, runtime dependency versions, service logs, and `harvest.sqlite`. The SQL export should run through the existing secure database connection; credentials do not belong in commands or artifacts.

Pinned checkpoints used:

| Repository | Revision |
|---|---|
| `convaiinnovations/laya` (multilingual) | `55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851` |
| `Mapika/decider-0.8b` | `a0a01d6f8135298f400a8c856b355793012ae971` |
| `Mapika/decider-2b` | `9839cc9d908be16c5988c0d041034b5fdf82c7a2` |

The local Python environment is `/private/tmp/scoracle-intake-runtime`, with Python 3.12, `laya==0.3.20`, `decider-ai==1.2.2`, `torch==2.14.0`, and `transformers==5.17.0`. Complete installed versions are in `runtime-requirements.txt`. Checkpoints and environments in `/private/tmp` are development caches and must be moved to managed storage before deployment. Each adapter records checkpoint file hashes, device, dtype, library versions, configuration, and input coverage in its responses.

Example commands, after installing dependencies and obtaining a pinned local checkpoint:

```sh
HF_HUB_OFFLINE=1 USE_TF=0 /private/tmp/scoracle-intake-runtime/bin/python examples/harvest_decider_server.py \
  --model-dir /private/tmp/scoracle-intake-models/decider-0.8b \
  --revision a0a01d6f8135298f400a8c856b355793012ae971 --device mps

cargo run --example context_harvest -- \
  logs/context-harvest-20260924/articles.jsonl \
  logs/context-harvest-20260924/new-run.jsonl http://127.0.0.1:8020/v1/systemone

python3 examples/harvest_store.py \
  --corpus logs/context-harvest-20260924/articles.jsonl \
  --database logs/context-harvest-20260924/harvest.sqlite \
  logs/context-harvest-20260924/new-run.jsonl
```

Omit the endpoint to prepare requests without running a model. Use a new output filename for each run. The initial Laya run predates raw-response retention; it retains normalized scores and provenance. Earlier packets also have the previous shorter body hash; the current implementation uses full SHA-256 for body identity.

## Integration boundary and remaining work

Harvester now owns this capability separately from Editor. Its stable identity is `scoracle.internal.harvester`; its reserved task key is `harvester`. The manifest declares `Classification`, with no generative inference routes or web-fetch grants. `application::plugins::build_harvester(endpoint)` binds the local provider, while `Harvester::new(Arc<dyn DecisionModel>)` accepts another classifier through the same typed contract. The replay executable calls this entry point rather than implementing its own classify/compile pipeline. New packets include the plugin identity without rewriting earlier trial artifacts.

The application fleet exports the manifest for explicit invocation, but excludes it from `ALL`, the deployable worker roster. Default worker startup therefore does not enable Harvester, and `COGNITION_STAGES=harvester` is rejected until a real queue/publication adapter exists. No placeholder `StudioPlugin::execute` claims success without persistence. Accepted contexts are proposals for future Editor/character consumption; there is no live subscriber yet.

The production Editor contract feeds evidence blurbs, extracted facts, people, discovery, and packet consumers. Replacing it with a text-only record without migrating those consumers would break existing behavior. This trial introduces a separate typed capability and context packet; it does not overload the existing generative read schema or bypass transactional claim/revision fencing.

Before a production cutover: label a representative held-out sample of fresh content; measure entity recall, false rejection, character usefulness, and per-source failures; calibrate or remove thresholds accordingly; benchmark the selected model on Archbox under normal load; preserve article-body structure during acquisition; and implement the production context storage and consumer migration together. Sentiment is advisory metadata and has not been validated independently. Sentence-level relevance selection is also not implemented: today's extraction is the bounded original opening.

Validation: seven cognition tests cover exact extraction, uncertainty, background-only mentions, invalid scores, missing coverage, and baseline isolation. Additional plugin tests exercise one-call classification, source attribution, failure propagation, and isolation from default worker activation. After extracting the plugin, the full library suite passed 551 tests with zero failures and 60 ignored integration tests. All 120 saved sweep articles also prepared successfully through the relocated cognition path, with every excerpt verified against the original source bytes. Example compilation, Python syntax checks, and SQLite excerpt verification also passed. No production model route, database migration, queue, or publication was changed.
