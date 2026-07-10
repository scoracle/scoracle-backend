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
- `COGNITION_ROUTE_<ROLE>_CANDIDATE` supports challenger evaluation, and `bin/eval` now runs
  incumbent-vs-candidate A/Bs through the per-lens task registry (`vibe`, `sigil`,
  `narratives`, `transfer`), including transfer team-player pair specs.
- `OLLAMA_MAX_CONCURRENT` bounds local inference through `GovernedInference`: one shared
  semaphore at the model-call seam, which stage code cannot bypass.
- Every derived row carries the `Provenance` envelope: `model_version`, `prompt_version`,
  `input_ids`, optional `input_hash` debounce, optional `trigger_payload`.
- `Inference::generate` already returns the exact wire body it POSTed (system prompt included).
  Parity harnesses still use it for byte-level diffs; Phase 2 now also persists it to
  `cognition_ledger` for narratives and transfers.
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
  -> sigil is enqueued by vibe (meaningful-change gate), the nightly rating batch
     (changed PEAK), AND the transfers handler (each served rumor) — Phases 5.4 + 5.1
```

The vibe→sigil gate (`should_enqueue_sigil`) is already lens-aware — it fires on a new
narrative, a new transfer rumor, or a vibe delta past threshold. The three original gaps are now
closed:

1. **Sigil's prompt has no transfer pillar.** ~~The gate can fire BECAUSE a rumor landed, and the
   synthesis prompt cannot see it.~~ **CLOSED (2026-07-09, commit `85753ce`).** Sigil now composes a
   fifth pillar (P5 `=== TRANSFER HEAT ===`) via `corpus::load_transfer_heat`, and a conditional
   `transfer_heat` key enters the `input_hash`, so the debounce no longer hides it.
2. **The rating batch never triggers synthesis.** ~~A fresh PEAK report reaches sigil only when the
   next news event runs vibe.~~ **CLOSED (2026-07-09, commit `89fdff3`).** The nightly rating batch
   enqueues sigil on a changed PEAK.
3. **Same-fan-out race.** ~~A transfer that adjudicates after the entity's vibe run is invisible
   until the next news event re-runs vibe.~~ **CLOSED (2026-07-09, commit `85753ce`).** The transfers
   handler now enqueues sigil directly for the player AND team on every served rumor, so a transfer
   reaches synthesis without waiting for a vibe re-run.

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

Primary role today: `Role::StatsLogic` through `sigil.rs`. Since Phase 5.1 (prompt `s9`), sigil
composes FIVE pillars — narratives, PEAK scouting report, vibe, momentum, and transfer heat — so
the current contract is genuine panel synthesis over every accountable lens. Phase 5.2 (prompt
`s10`) then fed the previous Sigil (score + blurb) back into the prompt as a continuity anchor —
prompt-only, outside the `input_hash`. Phase 5.3 (prompt `s11`) made panel DISAGREEMENT explicit:
the reply now carries three OPTIONAL outputs — convergence score, disagreement summary, and a
"why now" freshness note — persisted as additive nullable columns and served on the /sigil card.
The contract in this section is now fully realized; what remains for the phase is only trigger/eval
polish, not a missing output.

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

**MVP DONE (2026-07-10, commit `6b42383`; remaining lens wiring + live DB/schema validation
in commit `5b9d856`).** The ledger lives in a dedicated `public.cognition_ledger` table (mig 144),
not widened product rows. Product tables remain the served surface; ledger rows are diagnostic,
prunable, and keyed back to product row ids.

What landed:

- generic Rust helper `ledger::CognitionLedgerEntry` + best-effort insert, so diagnostic table
  issues do not fail served narratives/transfers.
- narratives ledger writes after `news_summaries` rows commit: exact request body/prompt when a
  model call happened, product row ids, model/prompt/output-contract versions, input ids, parser
  outcome (`no_call`, `parsed_empty`, `parsed`), context budget (`num_predict`, `eval_count`), and
  included/excluded evidence. Dedup exclusions currently record counts/reason, not exact dropped ids.
- transfers ledger writes after each `transfer_rumors` product row: team-player pair identity,
  request body/prompt, product row id, model/prompt/output-contract versions, input news ids, heat
  components, parser outcome (`rumor`, `cleared`, `unknown`), and model-cleared/unknown exclusion
  reasons.
- vibe, sigil, and rating/stat_summaries ledger writes are now wired after their product rows
  persist: product row ids, model/prompt/output-contract versions, input ids or input hash,
  exact prompt/request when a model call happened, parser/no-call outcome, context budget telemetry,
  and basic included/excluded evidence.
- migrations `143_sigil_disagreement_outputs` and `144_cognition_ledger` are applied in the target
  DB, `public.cognition_ledger` was read-back validated, and `sql/schema/schema.sql` was refreshed.

Remaining Phase 2 hardening:

- make excluded evidence richer where the pipeline has the detail (e.g. exact dedup-dropped article
  ids, budget-truncated rows, stale rows).
- add a fixture-capture helper that can source frozen fixtures directly from `cognition_ledger`.

Success:

- When a model output is weak, we can tell whether the failure was model, context, or prompt —
  without re-running anything.

### Phase 3 - Create Stage-Specific Evals

**SEAM + FOUR TASK SETS DONE (2026-07-09, commits below).** `bin/eval` was hardwired to the vibe task
(`EVAL_ROLE`) and read the LIVE corpus — not reproducible once the corpus moved. Both moves landed
for vibe, sigil, narratives, and transfer fixtures; the remaining eval sets are additive on the
proven seam.

1. **Per-lens task registry — DONE.** New lib module `rust/src/eval_tasks.rs`: a
   `#[async_trait] LensTask` (name/role/prompt_version/gen_options/build_prompt/evaluate) +
   `Box<dyn LensTask>` registry (`resolve_task`/`all_task_names`), composing the existing capability
   library (each stage's public loaders + prompt builder + `Parser`). `VibeTask` is a
   behavior-preserving port of the old path; `SigilTask` composes `load_pillars` (made `pub`) +
   `build_synthesis_prompt(prev=None)` + a disagreement rubric. A unified `CaseVerdict` carries BOTH
   axes — MAE (`abs_err`, vibe) and a property rubric (`checks`, sigil). `bin/eval` rewired to
   `--task`/`--fixtures`/`--capture` (default `vibe`, so the old CLI is unchanged); live mode
   generalized, throughput/side-by-side preserved.
2. **Frozen fixtures — DONE (panel-disagreement + a vibe band).** `rust/fixtures/<task>/*.json`
   (`Fixture` = frozen system + user_prompt + prompt_version + temperature + `Expect`). `--fixtures`
   runs them DB-free (Router-only), warns on `prompt_version` drift, prints a per-property ✓/✗ table.
   `--capture` emits a skeleton to stdout (bootstrap / Phase-2-ledger stand-in). The disagreement set
   was sourced **synthetic honesty-targets** (a real-model probe validated crisp ground truth), each
   `Expect` a **floor** (green today) or **target** (the honest bar the model fails, documenting the
   gap). Evidence the s11 gate surfaced on `mistral:7b` (temp 0, reproducible): (a) convergence runs
   too high on genuine conflict (70/80 where it should be low); (b) `DISAGREEMENT: N/A` (often quoted)
   instead of omitting — **FIXED at the source (2026-07-09):** `parse_synthesis_response` now
   normalizes DISAGREEMENT/WHY_NOW (`N/A`/`none`/`-` → None, fully quoted lines unwrapped) so the
   persisted column + served /sigil card stay clean; the eval reads the parser's normalized output
   directly (one source of truth — the fixtures now reflect + guard what's actually served); (c) it
   parrots the system-prompt's example disagreement verbatim for a conflict that isn't there — caught
   by `disagreement_excludes`.

3. **Narrative grounding set — DONE (2026-07-09, commit `6f811a3`).** `NarrativeTask` joins the
   registry as the narrative lens's storyline grouping + grounding half (vibe already covers its
   emotional temperature). It composes the capability library unchanged — `load_vetted_corpus` +
   `build_narratives_prompt` (live) and `NarrativesParser` (scoring) — with one minimal read
   accessor (`ParsedNarratives::returned()` → title/body/cited-article-numbers; the private
   `ModelNarrative` DTO stays encapsulated, the `load_pillars`/`ParsedSynthesis` precedent). New
   `Expect` rubric: `narratives_min/max` (count discipline), `title_/body_includes/excludes`
   (specificity + no-invention), `all_cite_articles` + `max_article_num` (grounding: every storyline
   cites a real corpus article, no invented reference). Five synthetic honesty-target fixtures,
   probe-validated on `mistral:7b` (temp 0): four are green regression floors (grouping, grounding,
   the "other team scheming around the entity" trap correctly NOT turned into an entity move, hype
   restraint, no over-split of one story). The fifth — `off-entity-and-hype-contamination` — surfaces
   a REAL gap: given a well-sourced trade + vague hype + an off-entity article, mistral keeps
   PRECISION (leaks no off-entity storyline — excludes green) but fails RECALL (returns zero,
   dropping the real trade too). `narratives_min 1` + `title_includes "Foss"` are checked-in RED
   targets a prompt/model fix flips green. Evidence over taste: the narrative lens's over-suppression
   under noise is now MEASURED, not hoped for.

4. **Transfer FP/TP adjudication set + live pair seam — DONE.** `TransferTask`
   joins the registry as the transfer lens's frozen-prompt adjudication half. Because the live
   production unit is a team-player NEWS PAIR (candidate + pair corpus through `build_pair_request`),
   not an `EntitySpec`, this shipped fixture-first: DB-free frozen `system` + `user_prompt` cases
   scored through the production `TransferParser` (2026-07-09, commit `ac527e9`). The follow-up
   live pair seam landed 2026-07-10: `eval --task transfer team:<team_id>:player:<player_id>:<sport>`
   and `eval --capture --task transfer ...` now build the prompt through the production
   `build_pair_request` prefix, use production candidate identity cards, and choose the sport-specific
   transfer/trade system prompt per case. New `Expect` rubric: `transfer_is_rumor`,
   `transfer_direction`, `transfer_stage`,
   `subject_includes/excludes`, `summary_includes/excludes`, and `confidence_min/max`. Four
   synthetic honesty-target fixtures are checked in: advanced-talks true positive, former-player
   return true positive, same-name owner false positive, and roundup/name-drop false positive.
   Probe on `mistral:7b` at temp 0: **26/26 green checks**. Important rubric finding: model
   `confidence` and `summary` on `is_rumor=false` are not served-row risk surfaces because
   `row_from_verdict` clears the persisted rumor fields for false verdicts; the false-positive
   fixtures therefore guard the real risk — clearing the pair and identifying the subject.

Remaining curated eval sets (DEFERRED — additive; each needs its own rubric vocabulary, not a seam
change):

- ~~transfer false positives and true positives~~ **DONE (2026-07-09, commit `ac527e9`)**
- ~~narrative grouping and grounding~~ **DONE (2026-07-09, commit `6f811a3`)**
- stats identity specificity
- ~~panel synthesis disagreement handling~~ **DONE (first set)**
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

Sigil is now a five-pillar synthesis (narratives, PEAK, vibe, momentum, transfer heat — prompt
`s11`), plus previous-Sigil continuity AND explicit panel disagreement outputs. This phase closed
the gap between that and an honest panel:

```text
Stats lens (PEAK)          [in prompt today]
Narrative lens (narratives + vibe)  [in prompt today]
Transfer lens              [in prompt — Phase 5.1]
Momentum / trajectory      [in prompt today]
previous Sigil             [in prompt — Phase 5.2]
  -> panel synthesis
  -> convergence + disagreement + why_now  [output — Phase 5.3]
```

1. **Add the transfer pillar. — DONE (2026-07-09, commit `85753ce`).** Sigil loads a fifth pillar
   via `corpus::load_transfer_heat` (the same served-rumor read the /transfers card and the
   vibe/narratives heat lines use), renders it as a P5 `=== TRANSFER HEAT ===` section through the
   shared `write_heat_lines`, and folds it into the no-pillar marker gate. A **conditional**
   `transfer_heat` key (one sorted `counterparty:heat:direction:stage` line per rumor) enters
   `build_synthesis_input_components` → the `input_hash`. Conditional on purpose: an entity with no
   rumors keeps its pre-5.1 hash (no deploy-time avalanche); an entity with served heat flips the
   hash and re-synthesizes once. Prompt bumped `s8`→`s9`. The transfer→sigil trigger shipped in the
   SAME commit (see 4) — now that transfers are in the hash, a transfer-only enqueue is real work
   rather than a debounced skip.
2. **Feed the previous Sigil into the prompt. — DONE (2026-07-09, commit `75ca616`).** The
   handler's existing single latest-row read (`latest_with_hash`) was widened `(score, hash)` →
   `(score, blurb, hash)`, so the prior score AND blurb come from ONE consistent (non-torn) row —
   no extra round-trip. When a real prior read exists (`previous_score` present), a `PrevSigil` is
   rendered as a `=== PREVIOUS SIGIL ===` lead-in BEFORE the fresh pillars, with a system-prompt
   rule to move from it deliberately ("memory, not a reset"). Deliberately kept OUT of
   `build_synthesis_input_components`/the `input_hash` — the score always moves, so hashing it
   would self-trigger every re-run (mirrors how `previous_score` is persisted-but-not-hashed).
   Prompt bumped `s9`→`s10` (provenance-only; no gate regenerates on prompt_version). Continuity
   is what makes the read feel like memory rather than a fresh take.
3. **Expose disagreement as output. — DONE (2026-07-09, commit `c854d06`).** Convergence score,
   disagreement summary, and a "why now" freshness note are three NEW additive, nullable columns on
   `sigil_synthesis` (mig 143: `convergence smallint` CHECK 1-100 / `disagreement text` /
   `why_now text`; folded into schema.sql + schema_migrations.txt). The synthesis reply gained three
   OPTIONAL labeled lines (`CONVERGENCE:` / `DISAGREEMENT:` / `WHY_NOW:`) after the required
   SCORE + BLURB; `parse_synthesis_response` now returns a `ParsedSynthesis` struct and degrades
   gracefully — only SCORE is required, so a missing panel field persists as NULL rather than
   failing the terminal stage, and blurb absorption stops at any known label so the fields parse
   regardless of emission order. These are model OUTPUTS, not pillar inputs, so the `input_hash`
   stays pillar-inputs-only: old rows remain valid and populate lazily on their next real
   re-synthesis (no deploy-time avalanche; no backfill). Prompt bumped `s10`→`s11` (provenance-only).
   The Go read path (`entity_sigil` in `db.go`) surfaces the three fields in the `current` object.
   This is the first Phase-5 step that changes product tables, as flagged. DECISION:
   `prompt_version s11` marks product rows generated under the new Sigil output shape; the separate
   output-contract version now lives in the Phase 2 `cognition_ledger` and should be wired for Sigil
   when its ledger writer lands.
4. **Fix the trigger topology.** Let every lens movement reach synthesis, debounced by the
   existing `input_hash`. Spurious enqueues are cheap — an unchanged pillar hash skips the model
   call. Two halves with different readiness:
   - **rating→sigil — DONE (2026-07-09, commit `89fdff3`).** The nightly `statcommentary` batch
     enqueues a sigil work item for each entity it regenerates (`enqueue_sigil`), keyed on the
     rating `input_hash` as the work-row `input_version`. Nightly-only (backfill would avalanche
     the queue); best-effort (a failed enqueue never fails a persisted rating). Effective
     immediately: PEAK (`divined_peak`/`notability`/`peak_trajectory`) is in sigil's
     `input_hash`, so a real PEAK change flips the hash and re-synthesizes. Verified: the batch
     runs the enqueue branch on a live generation (`ok=1`) via the unit-tested `work::enqueue` in
     the proven vibe→sigil pattern (sport-casing/gating/`input_version` confirmed). Direct
     observation of the transient work row was precluded by the live daemon draining+debouncing
     it (pausing production to watch is not allowed); confirm the visible end-to-end refresh
     during a real nightly (many PEAKs move) or on a test DB.
   - **transfer→sigil — DONE (2026-07-09, commit `85753ce`).** On a served rumor
     (`is_rumor == Some(true)`), the transfers handler enqueues sigil for the affected player AND
     team (both watched by the existing vibe→sigil gate), keyed on the persisted rumor id as the
     work-row `input_version` so a done sigil row reopens on each new served rumor and idempotently
     coalesces within a drain. Uppercase sport matches the news-rail conflict key (same convention
     as the rating→sigil `enqueue_sigil`, whose sport-casing was already confirmed). Best-effort: a
     failed enqueue never fails the persisted rumor or stalls the team item. Effective immediately —
     transfer heat is in the `input_hash` as of step 1, so a real change to the served-rumor set
     re-synthesizes; an unchanged re-vet costs only a cheap queue reopen the Sigil debounce skips.
     Live end-to-end deferred to the next real nightly (the prod daemon on archbox
     drains+debounces sigil rows under GPU contention; not driven from this session).

The two-pass breaking-news principle (design principle 6) is now fully paid for: `enqueue`
reopens rows on a changed `input_version`, the mig-103 trigger re-fires as more sources land,
the debounce skips unchanged re-runs, and the "why now" output (3, DONE) names what moved.

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
4. Add an eval fixture shape for panel/lens comparisons (the Phase 3 frozen-context format). — DONE:
   `Fixture`/`Expect` in `rust/src/eval_tasks.rs`, `rust/fixtures/<task>/*.json`, run via
   `eval --fixtures`.
5. Phase 5.4 rating→sigil trigger — DONE (commit `89fdff3`). The transfer→sigil half is deferred
   to ship with Phase 5.1 (the transfer pillar), because sigil's `input_hash` excludes transfers
   today and a transfer-only trigger would debounce to a skip.
6. Phase 5.1 (transfer pillar + the deferred transfer→sigil trigger) — DONE (commit `85753ce`). It
   closed the "gate fires on a rumor sigil can't see" gap AND the deferred trigger.
   Phase 5.2 (feed the previous Sigil — score + blurb — into the prompt as continuity) — DONE
   (commit `75ca616`, prompt `s10`). Phase 5.3 (convergence/disagreement/"why now" as additive
   nullable `sigil_synthesis` columns + the reply-format extension + Go serve surfacing) — DONE
   (commit `c854d06`, prompt `s11`, mig 143).
   Phase 3 (`bin/eval` per-lens registry + frozen fixtures) — SEAM + FOUR task sets DONE: the
   `eval_tasks::LensTask` registry now carries `vibe` + `sigil` + `narratives` + `transfer`,
   `--task`/`--fixtures`/`--capture`, a synthetic panel-disagreement fixture set scoring the s11
   CONVERGENCE/DISAGREEMENT/WHY_NOW outputs, and a narrative grounding set (commit `6f811a3`) whose
   contamination fixture MEASURED mistral's over-suppression under noise, plus a transfer FP/TP
   fixture set (commit `ac527e9`) scoring frozen transfer-pair prompts through `TransferParser`.
   Transfer live pair capture/A-B is now available through `team:<team_id>:player:<player_id>:<sport>`
   specs backed by `build_pair_request`.
   Next highest-leverage: add ledger-sourced fixture capture, finish the remaining Phase 3 eval
   sets (stats identity, prose-richness — additive on the seam), and run measured transfer candidate
   A/Bs for a real role-split call.
7. Decide whether `TransferLogic` deserves its own role after measured transfer live pair A/B runs;
   the fixture set gives green regression floors, and the live pair seam now supplies candidate
   comparison units.

## Risks

- Over-splitting roles before evals exist.
- Treating model brand as product identity.
- Letting richer voice weaken grounding.
- Creating a slow "panel" that misses breaking-news freshness.
- Publishing blended output that hides meaningful disagreement between rails. (Mitigated as of
  Phase 5.3: convergence/disagreement are first-class, model-emitted outputs. As of Phase 3 the gap
  is now MEASURED, not just hoped for: the panel-disagreement fixture set caught `mistral:7b`
  over-reporting convergence on real conflicts, emitting `DISAGREEMENT: N/A` instead of omitting (now
  FIXED at the parser — normalized to NULL so the card never shows "N/A"), and parroting the prompt's
  example disagreement verbatim. The remaining two are checked-in `target` assertions the current
  model fails — a prompt/model fix is now a fixture flipping green, not a subjective read.)
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
