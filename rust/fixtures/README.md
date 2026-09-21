# Active evaluation data

- `contracts/`: identity resolution cases and representative memory packages used by deterministic Rust tests.
- `quality/<task>/`: selected evidence cases for `eval --task <task> --fixtures`. They use the current Studio system and schema; a version mismatch stops the run. Rebuild or recapture the evidence when the prompt contract changes.

Quality cases contain input, factual expectations where useful, and manual `review` criteria. Review criteria stay outside the model prompt. Avoid preferred phrases, copied system prompts and paragraph recipes. A passing parser is not a quality verdict.

The shared publishing language permits a complete reading of a partial profile. Review observed absence, missing evidence and measured stability separately. Scout's `observed-zero-unknown-creation` case supplies zero goals but unknown creation and recent direction; the existing falling-form case checks that real direction still reaches the reading. The preparation path marks missing recent form explicitly without overwriting available trends.

Scout cases distinguish raw-value polarity from quality standing: SQL measurement `sign` is +1 for higher-is-better and -1 for lower-is-better, stored `z` is raw, and `pct` is already quality-oriented. Studio renders `sign * z` as quality z without inverting the percentile again. Missing polarity stays unknown. Review the resulting playing characteristics as well as the numbers; the negative-stat case checks favorable ball security, and the metadata/form case separates a strong profile from falling recent form. All publishing characters use identity as context and contribute their own perspective to the story.

`eval --capture --task <task> <entity>` emits a current case skeleton. Add expectations and review criteria before keeping it. `--capture-ledger` emits a historical request skeleton; use `--replay-fixtures DIR` for explicit frozen-prompt replay with current provider options. Complete Scout assignment capture/replay uses `--capture-assignment --task rating` (optionally `--season YEAR`) and `--replay-assignment FILE`.

Read-only inspection now lives under `eval --inspect memory`, `reports` and `identity`; run `eval` for arguments. The former `examples/` tools and full pre-cleanup fixture tree are preserved in the [wiki archive](../../../scoracle-wiki/raw/studio-history/2026-09-20/README.md).
