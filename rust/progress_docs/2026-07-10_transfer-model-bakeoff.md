# 2026-07-10 - Transfer Model Bakeoff

## Goal

Measure whether a local Qwen or small Gemma model should replace or split away from `mistral:7b`
for transfer adjudication inside the Multi-Lens Cognition Panel.

## What Changed

- Installed and tested `qwen2.5:7b`, `qwen3:8b`, and `gemma3:4b` as swappable
  `Role::EmotionalNews` candidates through `COGNITION_ROUTE_EMOTIONAL_NEWS_CANDIDATE`.
- Kept `mistral:7b` as the transfer brain because it remained the only model to clear the frozen
  transfer false-positive floor.
- Recorded the route decision in `planning_docs/MULTI_LENS_COGNITION_PANEL_PLAN.md`.
- Enriched cognition ledger provenance so excluded evidence now includes exact dropped ids/labels
  where the pipeline has them:
  - narratives: dedup-dropped, budget-truncated, and stale article ids
  - transfers: full heat corpus ids, prompt-rendered ids, prompt-budget truncation, and stale pair ids
  - rating: fixed-budget stat labels dropped from the top-14 prompt

## Model Results

| Model | Footprint | Live NBA pair A/B | Frozen transfer fixtures | Decision |
| --- | ---: | --- | --- | --- |
| `mistral:7b` | 4.4 GB | Correct on Kuminga/Lakers and Hachimura/Clippers | 26/26 property checks | Keep as incumbent |
| `qwen2.5:7b` | 4.7 GB | Missed Hachimura/Clippers by setting `is_rumor=false` while summarizing it as an agreed move | Effectively 25/26; failed the roundup/name-drop false-positive guard | Do not adopt |
| `qwen3:8b` | 5.2 GB | Repeated the Hachimura/Clippers false negative | Failed advanced-talks true positive and over-downgraded former-player return | Do not adopt |
| `gemma3:4b` | 3.3 GB | Correct on both live positives; comparable warm-call throughput | Failed fixture floor: over-staged one concrete-interest case and turned both noise guards into false positives | Do not adopt |

## Commands Run

```bash
cargo fmt
cargo test --lib
cargo build --bins
ollama pull qwen2.5:7b
ollama pull qwen3:8b
ollama pull gemma3:4b
COGNITION_ROUTE_EMOTIONAL_NEWS_CANDIDATE=qwen2.5:7b target/debug/eval --task transfer team:14:player:17553979:NBA team:13:player:666609:NBA
COGNITION_ROUTE_EMOTIONAL_NEWS_CANDIDATE=qwen2.5:7b target/debug/eval --task transfer --fixtures
COGNITION_ROUTE_EMOTIONAL_NEWS_CANDIDATE=qwen3:8b target/debug/eval --task transfer team:14:player:17553979:NBA team:13:player:666609:NBA
COGNITION_ROUTE_EMOTIONAL_NEWS_CANDIDATE=qwen3:8b target/debug/eval --task transfer --fixtures
COGNITION_ROUTE_EMOTIONAL_NEWS_CANDIDATE=gemma3:4b target/debug/eval --task transfer team:14:player:17553979:NBA team:13:player:666609:NBA
COGNITION_ROUTE_EMOTIONAL_NEWS_CANDIDATE=gemma3:4b target/debug/eval --task transfer --fixtures
git diff --check
```

## Verification

- `cargo test --lib`: 133 passed, 1 ignored.
- `cargo build --bins`: green; only the existing `sigil::linear_slope` warning.
- Transfer fixtures: `mistral:7b` stayed 26/26.
- `git diff --check`: clean.

## Result

Model swapping works, but the measured transfer route should stay on Mistral. `TransferLogic` should
not get its own configured route until a challenger beats Mistral on false-positive discipline, not
just live positive recall or disk footprint.

## Follow-Up

- Commit the enriched excluded-evidence ledger details and this model decision.
- Keep the Qwen/Gemma installs available for future route experiments, but do not route production
  transfer adjudication to them.
- Next Multi-Lens Panel work should continue from provenance/fixture hardening rather than model
  churn.
