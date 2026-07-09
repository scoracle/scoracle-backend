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

Audited against the Rust tree at `2fc1b55`. The cognition layer already has the right foundation:

- `rust/src/route.rs` routes by `Role`, not model name. Roles today: `StatsLogic`,
  `EmotionalNews`, `Multilang`, `Sql`.
- `COGNITION_ROUTE_<ROLE>` makes model identity a config concern.
- `COGNITION_ROUTE_<ROLE>_CANDIDATE` supports challenger evaluation, and `bin/eval` already runs
  incumbent-vs-candidate A/Bs with quality, throughput, and MAE axes — but only for the vibe
  task (`EVAL_ROLE` is hardcoded to `EmotionalNews`).
- `OLLAMA_MAX_CONCURRENT` bounds local inference through `GovernedInference`: one shared
  semaphore at the model-call seam, which stage code cannot bypass.
- Every derived row carries the `Provenance` envelope: `model_version`, `prompt_version`,
  `input_ids`, optional `input_hash` debounce, optional `trigger_payload`.
- `Inference::generate` already returns the exact wire body it POSTed (system prompt included).
  Today only the parity harnesses read it; the lens ledger (Phase 2) can persist it with no new
  plumbing.
- Queue stages are `scrub -> transfers -> narratives -> vibe -> sigil`, all Rust-owned since the
  Step-3 cutover (2026-06-28). Rating is the stats batch (`bin/statcommentary`). Momentum is not
  a stage — it is a deterministic projection over the rating/vibe series (mig 140).

This plan should build on that seam, not replace it.

## Lens / Stage / Role Map

The panel has four lenses; the pipeline has five queue stages plus a batch. The mapping is the
real Phase 1 deliverable, and today it is:

| Lens | Runtime | Role today | Writes |
|---|---|---|---|
| Stats lens | `rating` batch (`bin/statcommentary`) | `StatsLogic` | `stat_summaries` (PEAK) |
| Narrative lens | `narratives` + `vibe` stages | `EmotionalNews` | `news_summaries`, `vibe_scores` |
| Transfer lens | `transfers` stage | `EmotionalNews` | `transfer_rumors` |
| Panel synthesis | `sigil` stage | `StatsLogic` | `sigil_synthesis` |

Not lenses: `scrub` is the evidence gate upstream of every lens (BGE sieve + resolve; its
`vetted` write fires the mig-103 fan-out), and Momentum is deterministic trajectory math — a
synthesis *input*, not an accountable perspective with a model behind it.

Vibe maps INTO the narrative lens deliberately: emotional temperature is one of the narrative
lens's outputs, not a fourth perspective. If evals ever justify separating felt-state from
storyline grounding, that is a role split (the Phase 4 pattern), not a new lens.

## Trigger Topology

The panel's re-synthesis trigger is narrower than the lens model implies:

```text
article vetted (scrub)
  -> mig-103 trigger enqueues narratives + vibe (+ transfers when movement-flagged)
  -> vibe is the ONLY enqueuer of sigil
```

The vibe→sigil gate (`should_enqueue_sigil`) is already lens-aware — it fires on a new
narrative, a new transfer rumor, or a vibe delta past threshold. Three gaps remain:

1. **Sigil's prompt has no transfer pillar.** The gate can fire BECAUSE a rumor landed, and the
   synthesis prompt (P1 narratives, P2 PEAK, P3 vibe, P4 momentum) cannot see it.
2. **The rating batch never triggers synthesis.** A fresh PEAK report reaches sigil only when
   the next news event happens to run vibe for that entity.
3. **Same-fan-out race.** Transfers and vibe drain independently; a transfer that adjudicates
   after the entity's vibe run is invisible until the next news event re-runs vibe.

Phase 5 owns closing these; they are listed here because they bound what "panel synthesis" can
honestly claim today.

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

Primary role today: `Role::StatsLogic` through `sigil.rs`. Since the Wave 5 rebaseline (prompt
`s8`), sigil already composes FOUR pillars — narratives, PEAK scouting report, vibe, momentum —
so the current contract is closer to panel synthesis than "combine Rating/Vibe/Momentum"
suggests. What is genuinely missing: a transfer pillar, the previous Sigil as a prompt input
(persisted as `previous_score` today but never read back into the prompt), and
disagreement/convergence as first-class outputs.

## Phase Completion Protocol

When a phase (or a numbered sub-step large enough to hand off) lands, before moving on:

1. **Update this plan to reflect it.** Mark the phase/step `DONE (<date>, commit <sha>)`, fold
   any findings back into the relevant sections, and correct anything the implementation proved
   wrong. The plan is the running memory; a stale plan is worse than none.
2. **Commit the plan update surgically** — only the touched planning/doc files, same convention
   as the rest of this repo.
3. **Generate a click-to-copy text handoff for the next session** — a single fenced code block
   the next session can paste to resume cold: what landed, the gate/verification result,
   decisions carried, landmines, and the exact next step.

This mirrors the Rust cognition build-ledger discipline: state moves forward through the plan and
the handoff, so the next session resumes from the handoff rather than re-deriving context.

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

Most of the ledger already exists. The `Provenance` envelope persists `model_version`,
`prompt_version`, `input_ids`, and `input_hash`; `Inference::generate` returns the exact
`/api/generate` wire body — system prompt included — on every call, and only the parity
harnesses read it today. The delta to persist per lens call:

- the wire body itself (already in hand at every call site — persistence, not plumbing)
- excluded evidence: rows considered and dropped, with the reason (budget, staleness, dedup)
- context budget used vs available
- output contract version, distinct from prompt version

Decide once where the ledger lives: a dedicated `cognition_ledger` table keyed by
(stage, entity, generated_at) vs widening `trigger_payload` on product rows. A separate table
is the safer default — ledger rows are diagnostic and prunable, product rows are served;
different readers, different retention.

Start with narratives and transfers because the evidence set changes fastest and false positives
hurt trust.

Success:

- When a model output is weak, we can tell whether the failure was model, context, or prompt —
  without re-running anything.

### Phase 3 - Create Stage-Specific Evals

`bin/eval` already runs incumbent-vs-candidate A/Bs (quality prose, throughput, optional MAE),
but it is hardwired to the vibe task and reads the LIVE corpus — an eval run today is not
reproducible after the corpus moves on. Two moves:

1. Generalize `bin/eval` from the hardcoded `EVAL_ROLE` to a per-lens task registry: prompt
   builder + parser + scorer per lens, reusing each stage's public loaders the way the vibe
   eval already does.
2. Add frozen fixtures: a curated context snapshot (the Phase 2 ledger's wire bodies are
   exactly this shape) plus expected properties, checked in under `rust/fixtures/`. Live-DB
   evals remain for freshness; fixture evals become the regression gate.

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
- An eval run against fixtures is reproducible after the live corpus moves on.

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

The mechanics are cheap by design: a new `Role` variant is four match arms
(`all`/`as_str`/`env_suffix` plus the config read), and with no `COGNITION_ROUTE_TRANSFER_LOGIC`
set the role falls back to the default model — the split ships as a no-op and becomes real only
when config routes it differently. The cost is never the code; it is the Phase 3 eval evidence
that justifies routing it differently. Note the transfer stage already carries two distinct
prompt contracts (`t6` adjudication + `identity-adjudication-v1`), so "transfer lens" may
eventually mean two routes, not one.

Success:

- Transfer adjudication can improve independently from narrative prose.

### Phase 5 - Evolve Sigil Into Panel Synthesis

Sigil is already a four-pillar synthesis (narratives, PEAK, vibe, momentum — prompt `s8`).
This phase closes the gap between that and an honest panel:

```text
Stats lens (PEAK)          [in prompt today]
Narrative lens (narratives + vibe)  [in prompt today]
Transfer lens              [gate sees it; prompt does NOT]
Momentum / trajectory      [in prompt today]
previous Sigil             [persisted; prompt does NOT see it]
  -> panel synthesis
```

1. **Add the transfer pillar.** The trigger gate already watches `transfer_rumors`; the prompt
   must be able to see what the gate saw. `load_transfer_heat` in `corpus.rs` is the loader.
2. **Feed the previous Sigil into the prompt.** `previous_score` is persisted today but never
   read back into synthesis. Continuity is what makes the read feel like memory rather than a
   fresh take.
3. **Expose disagreement as output.** Convergence score, disagreement summary, and a "why now"
   freshness note are NEW columns on `sigil_synthesis` — an additive, nullable migration.
   Phase 4's route split changes no product tables; this phase does, and should say so.
4. **Fix the trigger topology.** Let every lens movement reach synthesis, debounced by the
   existing `input_hash`: the rating batch enqueues sigil for entities whose PEAK changed, and
   the transfers handler enqueues sigil on adjudication instead of waiting for the next vibe
   run. Spurious enqueues are cheap — an unchanged pillar hash skips the model call.

The two-pass breaking-news principle (design principle 6) is mostly already paid for: `enqueue`
reopens rows on a changed `input_version`, the mig-103 trigger re-fires as more sources land,
and the debounce skips unchanged re-runs. The missing piece is only the "why now" output in (3).

Success:

- The final product feels like Scoracle's read of the sports timeline, not a summary of the latest
  article.
- A PEAK change or a transfer adjudication re-synthesizes Sigil without waiting for the next
  news event.

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

1. Done — wiki architecture doc added (`wiki/Architecture/Multi-Lens Cognition Panel.md`).
2. Done — linked from AI Architecture and Hardware Roadmap.
3. Add `lens` language and the Lens / Stage / Role map to `rust/README.md` and `route.rs` docs.
4. Add an eval fixture shape for panel/lens comparisons (the Phase 3 frozen-context format).
5. Sequence the Phase 5.4 trigger-topology fix — it is independent of any role split, needs no
   new role or table, and is what makes the panel claim honest.
6. Decide whether `TransferLogic` deserves its own role after the first transfer eval set.

## Risks

- Over-splitting roles before evals exist.
- Treating model brand as product identity.
- Letting richer voice weaken grounding.
- Creating a slow "panel" that misses breaking-news freshness.
- Publishing blended output that hides meaningful disagreement between rails.
- Prompt bloat: a fifth and sixth pillar on an 8 GB serial box eats context budget; the Phase 2
  ledger's budget field is what makes this measurable rather than felt.
- Queue pressure: re-synthesis on every lens movement multiplies model calls on busy news days;
  the `input_hash` debounce is the control — watch backoff and dead-letter rates after Phase 5.4.
- Fixture rot: frozen eval fixtures drift from live prompt shapes unless regenerated on every
  `prompt_version` bump.

## Decision

Proceed. This is the right priority direction for the cognition layer.

The durable product is a lens-owning sports interpretation engine. Models should compete to speak
through Scoracle's context, not define Scoracle's context.
