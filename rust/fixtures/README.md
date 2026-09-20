# Active evaluation data

- `contracts/`: identity resolution cases and representative memory packages used by deterministic Rust tests.
- `quality/<task>/`: selected evidence cases for `eval --task <task> --fixtures`. They use the current Studio system and schema; a version mismatch stops the run. Rebuild or recapture the evidence when the prompt contract changes.

Quality cases contain input, factual expectations where useful, and manual `review` criteria. Review criteria stay outside the model prompt. Avoid preferred phrases, copied system prompts and paragraph recipes. A passing parser is not a quality verdict.

`eval --capture --task <task> <entity>` emits a current case skeleton. Add expectations and review criteria before keeping it. `--capture-ledger` emits a historical request skeleton; use `--replay-fixtures DIR` for explicit frozen-prompt replay with current provider options. Complete Scout assignment capture/replay uses `--capture-assignment --task rating` (optionally `--season YEAR`) and `--replay-assignment FILE`.

Read-only inspection now lives under `eval --inspect memory`, `reports` and `identity`; run `eval` for arguments. The former `examples/` tools and full pre-cleanup fixture tree are preserved in the [wiki archive](../../../scoracle-wiki/raw/studio-history/2026-09-20/README.md).
