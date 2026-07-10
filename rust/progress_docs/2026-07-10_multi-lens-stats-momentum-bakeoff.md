# 2026-07-10 - Multi-Lens Stats and Momentum Bakeoff

## Goal

Start the dedicated-lens pass for the Multi-Lens Cognition Panel without changing production
routing prematurely.

## What Changed

- Added code-level lens operating parameters in `eval_tasks.rs`:
  - Narratives: beat writer compiling stories swirling around the entity.
  - Transfers: transfer expert moving quickly while protecting credibility.
  - Vibe: content creator reading the interactable mood around an entity.
  - Rating / PEAK: opposing team scout naming the strength to stop and weakness to exploit.
  - Momentum: form scout judging rising/falling/steady from PEAK/rating and Vibe trajectories.
  - Sigil: reasoned expert network panelist summarizing all pillars.
- Registered an eval-only `momentum` task on `Role::StatsLogic`.
  - Production Momentum remains deterministic/read-model trajectory math.
  - No `MomentumLogic` route was added.
- Added frozen fixtures:
  - `fixtures/rating/no-standout-restraint.json`
  - `fixtures/rating/rate-adjusted-limited-minutes.json`
  - `fixtures/momentum/mixed-peak-up-vibe-down.json`
  - `fixtures/momentum/rating-surge-vibe-flat.json`
  - `fixtures/momentum/vibe-slide-steady-peak.json`
- Improved `eval --fixtures` output:
  - prints lens rail/operator/mandate/credibility guard.
  - summarizes candidate property checks.
  - counts unparseable replies as failed authored expectations instead of dropping them from the
    denominator.

## Model Results

| Task | Incumbent | Candidate | Result | Decision |
|---|---:|---:|---|---|
| Rating / PEAK | `mistral:7b` 16/22 | `qwen3:8b` 16/22 | Tie; Qwen3 still turns a 64th percentile above-average skill into a PEAK and often emits the entity name as PEAK. | Do not adopt |
| Rating / PEAK | `mistral:7b` 17/22 | `gemma3:4b` 17/22 | Tie; Gemma3 handles one per-x fixture well but misses rim-protector PEAK specificity and omits the PEAK marker on no-standout. | Do not adopt |
| Momentum | `mistral:7b` 16/19 | `qwen3:8b` 19/19 | Qwen3 correctly holds split PEAK-up/Vibe-down as steady and stays concise. | Candidate signal |
| Momentum | `mistral:7b` 16/19 | `gemma3:4b` 19/19 | Gemma3 also clears all three initial trajectory fixtures with concise reads. | Candidate signal |

## Decision

- Keep production `Role::StatsLogic` routing unchanged.
- Do not change Rating / PEAK model routing yet. The current challengers are not better on the
  structured PEAK contract.
- Treat Qwen3 and Gemma3 as live candidates for a future Momentum-specific route only after more
  fixtures or live captures confirm the 19/19 signal.
- Do not add `MomentumLogic` or `SynthesisLogic` yet. The current change is measurement surface plus
  lens taxonomy.

## Commands Run

```bash
cargo fmt
cargo test --lib
cargo test --bin eval
cargo build --bins
COGNITION_ROUTE_STATS_LOGIC_CANDIDATE=qwen3:8b OLLAMA_TIMEOUT_SECONDS=180 target/debug/eval --task rating --fixtures
COGNITION_ROUTE_STATS_LOGIC_CANDIDATE=gemma3:4b OLLAMA_TIMEOUT_SECONDS=180 target/debug/eval --task rating --fixtures
COGNITION_ROUTE_STATS_LOGIC_CANDIDATE=qwen3:8b OLLAMA_TIMEOUT_SECONDS=180 target/debug/eval --task momentum --fixtures
COGNITION_ROUTE_STATS_LOGIC_CANDIDATE=gemma3:4b OLLAMA_TIMEOUT_SECONDS=180 target/debug/eval --task momentum --fixtures
```

## Follow-Up

- Add more Momentum fixtures before a route split:
  - sparse signal / low sample counts.
  - stats down but Vibe up.
  - noisy transfer-driven Vibe spike that should not override stable PEAK.
- Consider a measured Rating prompt-version update for the opposing-scout frame; do not fold it into
  production silently.
- If Momentum remains green on a broader set, add a no-op `MomentumLogic` role with incumbent
  fallback and route it only by config.
