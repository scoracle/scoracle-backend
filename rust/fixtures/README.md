# Active evaluation data

- `contracts/`: identity resolution cases and representative memory packages used by deterministic Rust tests.
- `scout/`: retained S5 synthetic request/reply evidence, including failures; see [closure status](../docs/scout-closure-2026-09-29.md). These are replay records, not passing quality benchmarks.
- `harvester/`: source-bound, provisional intake training annotations; separate from human-reviewed quality benchmarks.
- `quality/<task>/`: selected evidence cases for `eval --task <task> --fixtures`. They use the current Studio system and schema; a version mismatch stops the run. Rebuild or recapture the evidence when the prompt contract changes.

Quality cases contain input, factual expectations where useful, and manual `review` criteria. Review criteria stay outside the model prompt. Avoid preferred phrases, copied system prompts and paragraph recipes. A passing parser is not a quality verdict.

The eight active Scout rating cases are the prepared synthetic worlds retained in `scout/s5-production-inputs.jsonl`: comparable, sparse, composite-only, zero/unknown, incompatible comparison, measured memory, withdrawn reporting and source-triggered work. They exercise known failures, not passing benchmarks. The missing-profile no-call path has a deterministic test and a retained assignment capture.

Seven damaged converted Scout fixtures have been retired from the active suite. Their original versions, attempted repairs and failed full responses remain in the S5 evidence. Do not infer model failures from missing fixture data or change factual boundaries to match a reply.

`eval --capture --task <task> <entity>` emits a current case skeleton. Add expectations and review criteria before keeping it. `--capture-ledger` emits a historical request skeleton; use `--replay-fixtures DIR` for explicit frozen-prompt replay with current provider options. Complete Scout assignment capture/replay uses `--capture-assignment --task rating` (optionally `--season YEAR`) and `--replay-assignment FILE`. Current Scout captures are version 2 and include prepared parts; version-1 captures require recapture. Aligned current-contract fixtures cannot fall back to a bare prompt.

Read-only inspection now lives under `eval --inspect memory`, `reports` and `identity`; run `eval` for arguments.
