# Active evaluation data

Evaluate model discovery and articulation under the [governing contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md). Tools supply traceable clues; the model reasons across them. Review supported relationships, attribution, uncertainty and unsupported additions rather than a preferred paraphrase.

- `contracts/`: identity resolution cases and representative memory packages used by deterministic Rust tests.
- `scout/`: retained S5 synthetic request/reply evidence, including failures; see [closure status](../docs/scout-closure-2026-09-29.md). These are replay records, not passing quality benchmarks.
- `harvester/`: source-bound, provisional intake training annotations; separate from human-reviewed quality benchmarks.
- `quality/<task>/`: selected evidence cases for `eval --task <task> --fixtures`. They use the current Studio system and schema; a version mismatch stops the run. Rebuild or recapture the evidence when the prompt contract changes.

Quality cases contain input, factual expectations where useful, and manual `review` criteria. Review criteria stay outside the model prompt. Avoid preferred phrases, copied system prompts and paragraph recipes. A passing parser is not a quality verdict.

`influencer/clue-controls.jsonl` contains four fully fictional pairs that change one source field: a coach/club connection, a speaker role, a factual headline, or an explicit Sunday event. All other parts stay fixed, including history. These are investigation controls, not passing benchmarks. Replay them through the current production assembler/parser with `cargo run --example influencer_replay -- --recorded fixtures/influencer/clue-controls.jsonl OUTPUT.jsonl OLLAMA_URL alibayram/smollm3:latest`. Keep exact requests and raw replies; compare each pair against its separate review criteria. Do not replace unknown baseline facts with facts supplied only in the paired variant.

The eight active Scout rating cases are the prepared synthetic worlds retained in `scout/s5-production-inputs.jsonl`: comparable, sparse, composite-only, zero/unknown, incompatible comparison, measured memory, withdrawn reporting and source-triggered work. They exercise known failures, not passing benchmarks. The missing-profile no-call path has a deterministic test and a retained assignment capture.

Seven damaged converted Scout fixtures have been retired from the active suite. Their original versions, attempted repairs and failed full responses remain in the S5 evidence. Do not infer model failures from missing fixture data or change factual boundaries to match a reply.

`eval --capture --task <task> <entity>` emits a current case skeleton. Add expectations and review criteria before keeping it. `--capture-ledger` emits a historical request skeleton; use `--replay-fixtures DIR` for explicit frozen-prompt replay with current provider options. Complete Scout assignment capture/replay uses `--capture-assignment --task rating` (optionally `--season YEAR`) and `--replay-assignment FILE`. Current Scout captures are version 2 and include prepared parts; version-1 captures require recapture. Aligned current-contract fixtures cannot fall back to a bare prompt.

Read-only inspection now lives under `eval --inspect reports` and `identity`; run `eval` for arguments.

Journalist's active `quality/narratives` cases now use `n102-fresh-only-json`: only
`meta`, `voice` and `fresh` reach the model, with one fixed instruction. Historical
n101 quality cases are preserved under `journalist/history-quality-n101/` for later
work; their expectations require history and do not apply to this pilot. Current
training cases explicitly forbid inventing a prior timetable or a reason the club
has not given. Production and fixture replay use the same assembler and JSON parser.

[Journalist synthesis comparison](journalist/synthesis-comparison-2026-10-08.md)
retains two fictional three-article payloads and 18 Ollama replies from Ministral,
Granite4.2 and SmolLM3. Its combined report is experimental; production report slots
are unchanged. The adjacent GPU concurrency receipt records four simultaneous
Granite requests. These are calibration evidence, not passing launch benchmarks.
