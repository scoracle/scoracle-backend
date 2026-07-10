# 2026-07-10 - PEAK Context Drilldown Recap

## Why This Note Exists

The first stats/analytical lens bakeoff did not produce a clean model winner for Rating / PEAK.
That is itself the useful finding: Mistral, Qwen3, and Gemma3 all showed meaningful failures around
the structured PEAK contract.

The likely next improvement is not "try another model first." The sharper hypothesis is: the PEAK
context asks the model to infer too much from a stat list. The context should hand the model a more
explicit scouting card.

## What Was Measured

Fixture set:

- `fixed-budget-rich-profile`
- `rim-protector-specificity`
- `no-standout-restraint`
- `rate-adjusted-limited-minutes`

Models:

- incumbent `mistral:7b`
- candidate `qwen3:8b`
- candidate `gemma3:4b`

Routes:

- all comparisons used `Role::StatsLogic`
- challengers were configured only through `COGNITION_ROUTE_STATS_LOGIC_CANDIDATE`
- production routing was not changed

Results:

| Candidate run | Incumbent score | Candidate score | Decision |
|---|---:|---:|---|
| `qwen3:8b` Rating / PEAK | 16/22 | 16/22 | no stats route change |
| `gemma3:4b` Rating / PEAK | 17/22 | 17/22 | no stats route change |

The scores moved slightly between runs because local generation is not perfectly stable even at
temperature 0, but the qualitative failure pattern was stable: every model had trouble treating the
PEAK line as a constrained structured output.

## Model-Specific Findings

### Mistral

Mistral usually wrote grounded, useful scouting prose. Its main failure was structural:

- often omitted the required `PEAK:` marker entirely
- put the whole scouting report into prose without a separate structured PEAK label
- handled weaknesses and caveats reasonably well when the datapoints were clear
- sometimes stayed too verbose

Interpretation: Mistral can narrate the context, but the prompt/context is not making the structured
first-line decision obvious enough.

### Qwen3

Qwen3 was often concise and analytically neat, but it made two route-blocking PEAK errors:

- sometimes used the entity name as the PEAK label instead of the actual skill
- promoted a 64th percentile "above average" skill into the PEAK, violating the no-standout rule

Interpretation: Qwen3 may be good at trajectory-style reasoning, but the current PEAK context still
lets it reinterpret the rules. That is exactly what the stats lens must not allow.

### Gemma3

Gemma3 had the best isolated PEAK moment on the per-x fixture:

- correctly emitted `PEAK: Finishing efficiency`
- mentioned per-36 support cleanly

But it also failed important cases:

- omitted the `PEAK:` marker on no-standout
- used the entity name as PEAK on rim protection
- wrote `No standout skill` in the body on a rich profile with elite datapoints
- ran long on at least one fixture

Interpretation: Gemma3 is promising in narrow cases, but not reliable enough for Rating / PEAK
routing.

## The Important Cross-Model Pattern

All three models struggled with the same class of problem:

> The current PEAK prompt gives a sorted datapoint list and asks the model to infer the structured
> PEAK decision from "the first datapoint's skill only if it is strong or elite."

That is too indirect for the product contract. Scoracle already has the structured data needed to
decide the PEAK candidate deterministically. The model should probably not be asked to discover the
PEAK label from the list. It should be asked to explain a prepared scouting card.

## Current Context Shape

The current Rating prompt gives:

- entity name and sport/entity type
- profile distinctiveness
- composite score
- sorted datapoints with value, percentile, tier, z-score, and optional position percentile
- optional rate-adjusted/per-x corroboration
- instruction to write `PEAK: <label>`

What it does not give explicitly:

- "Required PEAK line: PEAK: Rim protection"
- "Stop this strength: Rim protection, because..."
- "Exploit this weakness: Turnovers, because..."
- "No standout is required because highest available tier is above average"
- "Do not choose the entity name; PEAK must be one skill label or exact No standout skill"

## Context Hypothesis

The stats lens should become an opposing-scout card:

```text
Entity: Nia Torres (NBA player, C)

SCOUTING DECISION
Required PEAK line: PEAK: Rim protection
Primary strength to stop: Rim protection
Why it matters: 3.2, 96th percentile, elite, z +2.4; position 94th percentile
Secondary strengths: Defensive rebounds, Screen assists
Primary weakness to exploit: Turnovers
Why it matters: 2.8, 28th percentile, poor, z -1.2

SUPPORTING DATAPOINTS
...

Write the opposing-scout read now.
```

For no-standout:

```text
SCOUTING DECISION
Required PEAK line: PEAK: No standout skill
Reason: highest datapoint is Spot-up shooting, 64th percentile, above average; no strong/elite
datapoint exists.
Primary usable skill: Spot-up shooting, but it is not a PEAK.
Primary weakness to exploit: Turnovers
```

This flips PEAK from an inference task into a grounded explanation task.

## Why Momentum Looked Better

Momentum used more direct context:

- PEAK/rating slope
- Vibe slope
- signed Momentum score
- explicit instruction to choose `rising`, `falling`, or `steady`
- explicit split-signal rule

Qwen3 and Gemma3 both went 19/19 on the first three Momentum fixtures. That does not prove they
deserve a route yet, but it supports the context hypothesis: when the decision frame is explicit,
the analytical models do better.

## Recommended Follow-Up

Do not start with a model swap. Start with context and fixture work.

1. Add a deterministic `ScoutingDecision` or similar struct in `rating.rs`.
2. Compute:
   - required PEAK label
   - primary strength to stop
   - secondary strengths
   - primary weakness to exploit
   - no-standout reason when applicable
3. Render that decision above the raw datapoint list.
4. Keep the raw datapoints for grounding, but stop making the model discover the label.
5. Add fixtures specifically for:
   - entity-name-as-PEAK guard
   - no-standout guard
   - elite strength plus obvious exploit weakness
   - per-x support that should corroborate but not replace the PEAK
   - team profile with multiple elite strengths
6. Re-run `mistral:7b`, `qwen3:8b`, and `gemma3:4b`.
7. Only then decide whether this is a prompt/context fix, a parser contract fix, or a model-fit issue.

## Follow-Up Session Handoff

```text
Resume in /home/sheneveld/scoracle/scoracle-backend/rust.

Goal:
Drill down into the Rating / PEAK context problem discovered in the Multi-Lens stats/momentum
bakeoff. Do not start by changing production routing. The working hypothesis is that every tested
model struggled because the current prompt asks the model to infer the structured PEAK label from a
datapoint list, instead of giving it a deterministic opposing-scout decision card.

Current evidence:
- Rating / PEAK fixtures:
  - qwen3:8b tied mistral:7b at 16/22.
  - gemma3:4b tied mistral:7b at 17/22.
  - All models had PEAK-line failures.
- Common failures:
  - omitted `PEAK:` marker
  - entity name emitted as PEAK
  - 64th percentile above-average skill promoted as PEAK
  - no-standout stated in body but not as structured PEAK line
- Momentum fixtures:
  - qwen3:8b and gemma3:4b both scored 19/19 vs mistral:7b at 16/19.
  - This supports the idea that explicit decision context helps.

Files to inspect first:
- src/rating.rs
- src/eval_tasks.rs
- fixtures/rating/*.json
- progress_docs/2026-07-10_multi-lens-stats-momentum-bakeoff.md
- progress_docs/2026-07-10_peak-context-drilldown-recap.md
- ../planning_docs/MULTI_LENS_COGNITION_PANEL_PLAN.md

First implementation target:
Add a deterministic scouting-decision layer in rating.rs before the model prompt:
- required_peak_line
- primary_strength_to_stop
- secondary_strengths
- primary_weakness_to_exploit
- no_standout_reason

Likely prompt shape:
Render a `SCOUTING DECISION` block before the existing datapoints:
- Required PEAK line: PEAK: <label or No standout skill>
- Primary strength to stop: <skill + tier/evidence>
- Primary weakness to exploit: <skill + tier/evidence>
- Why no standout: <only when highest tier is not strong/elite>

Important constraints:
- Do not change production routing.
- If changing RATING_SYSTEM_PROMPT or build_stat_prompt behavior, bump RATING_PROMPT_VERSION.
- Update rating fixtures to the new prompt version.
- Keep parser behavior that salvages body text, but make fixtures keep failing when `divined_peak`
  is wrong or empty.
- Run:
  cargo fmt
  cargo test --lib
  cargo test --bin eval
  cargo build --bins
  git diff --check
- Then run:
  COGNITION_ROUTE_STATS_LOGIC_CANDIDATE=qwen3:8b OLLAMA_TIMEOUT_SECONDS=180 target/debug/eval --task rating --fixtures
  COGNITION_ROUTE_STATS_LOGIC_CANDIDATE=gemma3:4b OLLAMA_TIMEOUT_SECONDS=180 target/debug/eval --task rating --fixtures

Decision rule:
If the scouting-decision context makes Mistral pass the PEAK fixtures, keep routing unchanged and
ship the context fix. If Qwen3/Gemma3 still materially outperform after the context fix, only then
consider a StatsLogic candidate adoption. Do not add MomentumLogic or SynthesisLogic in this
follow-up; that is separate.
```
