# Multi-Lens Cognition Panel Plan

Date: 2026-07-09
Status: priority planning

## Thesis

Scoracle's moat is not a particular local model. The moat is the prepared sports worldview:
entity identity, scrubbed news, transfer truth, statistical identity, trajectory, provenance,
and the product vocabulary that turns those inputs into Rating, Vibe, Momentum, and Sigil.

The model is interchangeable. Its job is to produce the richest grounded prose and judgment over
the context the schema provides.

The next cognition step is to make that explicit: Scoracle should own a set of durable lenses
over the sports timeline, then synthesize those lenses into the final product read.

```text
breaking sports input
  -> scrub + entity resolution
  -> transfer lens
  -> narrative lens
  -> stats lens
  -> panel synthesis
  -> Sigil / product rows
```

## Product Shape

The "panel" is not three model brands. It is three accountable perspectives:

| Lens | Owns | Product value |
|---|---|---|
| Stats lens | Rating, statistical identity, role, distinctiveness, on-field proof | Grounds hype in what the entity actually is. |
| Narrative lens | story grouping, source freshness, emotional temperature, coverage stakes | Explains what the timeline is saying and why it matters. |
| Transfer lens | rumor/trade credibility, movement fit, source ladder, roster identity | Separates actionable movement signals from recycled noise. |
| Panel synthesis | disagreement, convergence, final Scoracle read | Produces the user-facing interpretation rather than a generic summary. |

The final output should preserve rail disagreement instead of flattening it. A player can have
strong stats and negative narrative pressure. A transfer can be hot but weakly sourced. A team's
Sigil should be able to say that clearly.

## Current Fit

The Rust cognition layer already has the right foundation:

- `rust/src/route.rs` routes by `Role`, not model name.
- `COGNITION_ROUTE_<ROLE>` makes model identity a config concern.
- `COGNITION_ROUTE_<ROLE>_CANDIDATE` supports challenger evaluation.
- `OLLAMA_MAX_CONCURRENT` already bounds local inference.
- `model_version`, `prompt_version`, input ids, and hashes are already persisted across derived rows.
- Current stages cover `scrub -> transfers -> narratives -> vibe -> sigil`, with rating as the stats batch.

This plan should build on that seam, not replace it.

## Hardware Read

Current box:

- GTX 1070 Ti, 8 GB VRAM.
- 32 GB system RAM.
- Good enough for serial 7B/8B-class quantized local cognition.
- Not enough for multiple strong local models hot at the same time.

Near-term unlock:

- A 24 GB card, practically a used RTX 3090, turns the concept from a serial pipeline into a real
  local panel system.
- It allows a stronger model to stay resident, multiple smaller role models to swap with less pain,
  or a local backend such as vLLM to batch more effectively.

Do not block the architecture on 24 GB. Build the lens abstraction now. Let hardware determine
whether the lenses run serially, warm-swapped, or concurrently.

## Design Principles

1. Lenses are product contracts; models are implementation details.
2. Rust prepares, grounds, compresses, and proves context. The model reasons over that context.
3. Every lens output carries provenance: source ids, input hash, model version, prompt version, role,
   and generated timestamp.
4. Model routing must be earned by evals, not taste.
5. Voice matters, but grounding wins. Add a skeptic/check step anywhere false positives harm trust.
6. Breaking news can have a quick first pass and a richer second pass. The product should be allowed
   to improve as more sources and stats settle.

## Target Contracts

### Stats Lens

Inputs:

- entity identity card
- current-season and recent-form rating context
- composite scopes
- event/box-score context where available
- prior stat summary / Rating
- relevant Vibe and transfer heat only when it helps frame the read

Outputs:

- statistical identity label
- concise Rating read
- proof points
- caveats
- confidence / sufficiency marker

Primary role today: `Role::StatsLogic`.

### Narrative Lens

Inputs:

- vetted article/entity links
- source freshness and source ladder
- storyline clusters
- prior narratives
- transfer heat markers
- entity identity card

Outputs:

- current storyline set
- emotional direction
- stakes
- source-grounded summary
- stale/recycled/noise marker

Primary role today: `Role::EmotionalNews`.

### Transfer Lens

Inputs:

- vetted news pairs
- transfer/trade keywords and buckets
- roster identity and current team
- prior rumors
- source ladder
- deterministic heat and confidence

Outputs:

- apply / reject / unknown adjudication
- source attribution
- movement direction
- fit/read note
- fail-closed reason where applicable

Primary role today: `Role::EmotionalNews`, but this should become separable once evals show a
different model or prompt shape is better at transfer adjudication than narrative prose.

### Panel Synthesis

Inputs:

- latest Stats lens output
- latest Narrative lens output
- latest Transfer lens output where relevant
- Momentum / trajectory inputs
- previous Sigil
- explicit disagreement markers

Outputs:

- final Scoracle read
- convergence score
- disagreement summary
- product-facing Sigil blurb
- short "why now" note for breaking-news freshness

Primary role today: `Role::StatsLogic` through `sigil.rs`, but the product contract should evolve
from "combine Rating/Vibe/Momentum" toward "synthesize accountable lenses."

## Implementation Phases

### Phase 1 - Name The Lenses Without Moving Behavior

- Add explicit lens terminology to Rust docs and prompt comments.
- Keep current roles and queue stages intact.
- Treat current `rating`, `narratives`, `transfers`, `vibe`, and `sigil` outputs as the first lens
  implementation.
- Add trace fields/log language that distinguish `role`, `stage`, and `lens`.

Success:

- A future contributor can tell the difference between a stage, a lens, and a model route.

### Phase 2 - Add Lens Ledgers

For each lens call, persist or reconstruct a ledger:

- source rows read
- included/excluded evidence
- context budget
- final prompt sections
- input hash
- model route
- output contract version

Start with narratives and transfers because the evidence set changes fastest and false positives
hurt trust.

Success:

- When a model output is weak, we can tell whether the failure was model, context, or prompt.

### Phase 3 - Create Stage-Specific Evals

Add curated eval sets for:

- transfer false positives and true positives
- narrative grouping and grounding
- stats identity specificity
- panel synthesis disagreement handling
- prose richness under a fixed context budget

Rubrics:

- groundedness
- specificity
- freshness
- false-positive risk
- useful synthesis
- voice quality

Success:

- `COGNITION_ROUTE_<ROLE>` changes are backed by evidence.
- Model swaps become routine and reversible.

### Phase 4 - Split Transfer From Narrative Routing If Earned

Today both transfer and narrative work use `Role::EmotionalNews`. If evals show different model
strengths, add a distinct role such as:

```text
Role::TransferLogic
COGNITION_ROUTE_TRANSFER_LOGIC
COGNITION_ROUTE_TRANSFER_LOGIC_CANDIDATE
```

Do this only when a measured model/prompt difference justifies the new route. The route split
should not change product tables by itself.

Success:

- Transfer adjudication can improve independently from narrative prose.

### Phase 5 - Evolve Sigil Into Panel Synthesis

Expand `sigil.rs` from three-signal convergence toward a panel synthesis contract:

```text
Rating / stats lens
Vibe / narrative lens
Transfer lens
Momentum / trajectory
previous Sigil
  -> panel synthesis
```

The output should expose convergence and disagreement instead of only a blended score.

Success:

- The final product feels like Scoracle's read of the sports timeline, not a summary of the latest
  article.

### Phase 6 - Hardware-Aware Runtime

Keep runtime topology behind config:

- 8 GB VRAM: serial local inference, one governed model call at a time.
- 24 GB VRAM: stronger resident model or multiple warm role models.
- 48 GB+ VRAM: larger model, higher concurrency, or vLLM-style batching.

Do not encode GPU assumptions in stage logic. Stage code asks for a role; the router decides where
that role lives.

Success:

- Hardware upgrades improve throughput and model quality without rewriting cognition stages.

## First Slice

1. Add the wiki architecture doc for the multi-lens panel.
2. Link the doc from AI Architecture and Hardware Roadmap.
3. Add `lens` language to Rust README / route docs in a small follow-up.
4. Add an eval fixture shape for panel/lens comparisons.
5. Decide whether `TransferLogic` deserves its own role after the first eval set.

## Risks

- Over-splitting roles before evals exist.
- Treating model brand as product identity.
- Letting richer voice weaken grounding.
- Creating a slow "panel" that misses breaking-news freshness.
- Publishing blended output that hides meaningful disagreement between rails.

## Decision

Proceed. This is the right priority direction for the cognition layer.

The durable product is a lens-owning sports interpretation engine. Models should compete to speak
through Scoracle's context, not define Scoracle's context.
