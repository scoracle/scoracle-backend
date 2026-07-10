# 2026-07-10 - Multi-Lens Stats and Momentum Bakeoff

## Goal

Resume the Multi-Lens Cognition Panel after the transfer bakeoff by defining the lens operating
parameters and measuring early stats/PEAK and Momentum model fit without changing production routes.

## What Changed

- Rust now has a code-level lens catalog:
  - Narratives: beat writer.
  - Transfers: fast but credible transfer expert.
  - Vibe: content creator reading the interactable mood.
  - Rating / PEAK: opposing team scout naming the strength to stop and weakness to exploit.
  - Momentum: form scout judging rising/falling/steady from PEAK/rating plus Vibe trajectories.
  - Sigil: reasoned expert network panelist synthesizing all pillars.
- `target/debug/eval` now prints lens rail/operator/mandate/credibility guard.
- Added fixture-first `momentum` eval task on `Role::StatsLogic`; production Momentum remains
  deterministic/read-model based.
- Expanded Rating / PEAK fixtures with no-standout restraint and per-x limited-minutes cases.
- Fixed fixture summaries so candidates are summarized and unparseable replies count against authored
  expectations.

## Results

| Task | Candidate | Score | Decision |
|---|---:|---|---|
| Rating / PEAK | `qwen3:8b` vs `mistral:7b` | 16/22 vs 16/22 | No stats route change |
| Rating / PEAK | `gemma3:4b` vs `mistral:7b` | 17/22 vs 17/22 | No stats route change |
| Momentum | `qwen3:8b` vs `mistral:7b` | 19/19 vs 16/19 | Candidate signal only |
| Momentum | `gemma3:4b` vs `mistral:7b` | 19/19 vs 16/19 | Candidate signal only |

Qwen3 and Gemma3 both look promising for Momentum trajectory reasoning, but the fixture set is too
small to justify adding `MomentumLogic` yet. Neither challenger currently earns the Rating / PEAK
route because the structured `PEAK:` line remains unreliable.

## Decision

- Keep production routing unchanged.
- Keep Mistral as the emotional/news rail default.
- Do not add `TransferLogic`, `MomentumLogic`, or `SynthesisLogic` yet.
- Broaden Momentum fixtures next; only split routing after the broader eval stays green.
