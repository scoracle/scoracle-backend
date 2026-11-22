# Harvester production cutover plan

**Date:** September 26, 2026
**Status:** Packet-free cutover implementation underway locally; not deployed
**Objective:** Replace Editor intake with a regular Harvester plugin, inject verified publisher text directly into character context, and retire Editor, packets, and stories while preserving required non-editorial obligations.

## Start here

Local progress, preserved evidence indexes, annotation groundwork, and the initial
Editor dependency inventory are recorded in
[cutover groundwork](harvester-cutover-groundwork-2026-09-26.md).
The earlier 84-row annotation queue contains provisional AI suggestions, not human gold labels. Calibration is follow-up tuning; it does not gate the broad, advisory first cutover.

The approved product shape is:

```text
Google ranked candidate
  -> canonical deduplication
  -> publisher text acquisition
  -> Laya entity-relevance check
  -> Laya four-character routing distributions
  -> exact headline + first three source sentences stored
  -> advisory character assignments
  -> each character makes its own authoritative final decision
```

Google is the first relevance layer. Laya is the cheap, permissive second layer and routing comb. Granite character plugins do the expensive cognition and final product-level relevance judgment. Harvester does not summarize, rewrite, editorialize, or use regexes as semantic gates.

Do not delete Editor until its consumers and durable side effects have migrated or been retired. Laya calibration and routing precision are post-cutover tuning work; all acquired candidates remain eligible for character review during cutover.

The current implementation and experimental record are described in [harvester-cascade-2026-09-26.md](harvester-cascade-2026-09-26.md). The decisive replay artifact is `logs/harvest-focus-20260926/cohort-v2-publisher-first.json`; it is ignored by Git and must be deliberately preserved or converted into checked-in fixtures before relying on it long term.

## Decisions already made

- Harvester is a normal harness plugin. The older term *junction* does not identify a separate architectural kind.
- Google result membership, rank, and query entity are first-layer relevance provenance.
- Publisher content must be acquired before Laya classification. A Google RSS description is too thin to substitute for the publisher opening.
- Every canonical Google candidate is eligible for acquisition. The old Editor ceiling of ten results per entity does not apply.
- Laya records entity relevance and all four character perspectives as advisory signals. An `irrelevant` value does not discard a Google result.
- Harvester stores the unchanged headline and mechanically selected first three source sentences with exact source-byte provenance.
- Character tags are advisory. Each character plugin is the final relevance guard for its own product.
- Analyst and Oracle continue to consume finished character products, not raw Harvester contexts.
- Fetch failures, provider errors, invalid distributions, and truncation are errors or retry states, never negative relevance decisions.
- Editor has no future role in semantic admission, routing, source rewriting, or story voice. Stories, storylines, and generated packets are being sunset, not migrated.

## Evidence that constrains the plan

These are observations, not proposed production service levels.

| Observation | Result |
|---|---:|
| Google rows for Chelsea, Cowboys, and Pistons | 94 |
| Exact duplicates removed | 10 |
| Canonical candidates | 84 |
| Publisher texts acquired | 77 |
| Explicit acquisition failures | 7 |
| Laya entity accepts / rejects | 67 / 10 |
| Accepted contexts verified against source bytes | 67 of 67 |
| Accepted contexts with three detected sentence endings | 61 |
| Accepted contexts containing all available opening sentences | 6 |
| Exact reverse-enrichment context retained | 42,878 bytes |
| Accepted articles tagged for all four characters | 53 of 67 |
| Complete publisher-first replay time on Mac | 284.57 seconds |
| Median successful acquisition | 0.896 seconds |
| Median complete Laya cascade per materialized article | 1.011 seconds |
| Median grouped Granite call | 26.95 seconds |

The experiment establishes the pipeline shape and favorable cost split, but not production quality:

- The RSS-description-first run forwarded 83 of 84 articles and made obvious mistakes. It is an invalid production architecture.
- The corrected publisher-first run exposed visible false positives and false negatives in the entity gate.
- Zero-shot character routing is much too broad; 53 of 67 accepted articles received all four tags.
- Laya's base checkpoint clamps invalid temperature values. Its output distributions are not calibrated probabilities of correctness.
- Journalist received all 62 narrative-tagged contexts, produced six narratives, cited 18 unique articles, and left 44 inputs unused. This supports a final Granite guard, but it is not a labeled per-article evaluation.
- Influencer currently lacks per-source citations, so its final per-article decisions cannot be audited.
- Insider and Scout require identity, roster, transfer, or measured-stat context from their real adapters. A Harvester tag alone cannot safely invoke their product paths.
- Seven source acquisitions failed through HTTP restrictions or timeouts. They must remain visible and retryable.

## Execution order

Build and verify Phases 3–8 for the cutover first. Phase 0 provides regression fixtures;
Phases 1–2 are post-cutover calibration work and do not block broad delivery.
No Laya score suppresses a candidate in the first production cutover.

### Phase 0 — Freeze the contract and evidence

Make the successful experiment reproducible before changing behavior.

- Version the packet-free source-context contract and retain the exact meaning of every field.
- Preserve the corrected cohort trace outside ignored scratch storage, or reduce it to durable, license-safe fixtures and aggregate reports.
- Add golden cases covering known entity false positives, known false negatives, blocked acquisitions, short openings, duplicate URLs, and multi-entity stories.
- Keep the failed RSS-first trace as a regression example: RSS text must never silently replace publisher text for Laya classification.
- Verify headline and context offsets byte-for-byte after serialization and storage.
- Record the exact Laya checkpoint, tokenizer, prompt contract, runtime, and character manifest IDs used for every evaluation.

**Exit gate:** A clean checkout can reproduce contract tests without depending on ignored logs, and tests prove that errors cannot become relevance rejects.

### Phase 1 — Build the labeled calibration set after cutover

Create the evidence needed to decide whether and how to tune Laya.

- Label the current 84-candidate cohort and several fresh nightly cohorts.
- Assign five independent labels per materialized article:
  - useful news for the query entity;
  - useful Journalist evidence;
  - useful Influencer evidence;
  - useful Insider evidence;
  - useful Scout evidence.
- Label story groups together so duplicates and syndicated variants do not leak across training and evaluation splits.
- Keep acquisition failures in a separate state; do not guess their relevance.
- Record concise label reasons and the evidence span that justified each positive label.
- Reserve a later nightly cohort as an untouched temporal evaluation set.
- Have a human adjudicate ambiguous examples and a sample of apparent model errors before treating labels as gold.

Measure entity recall, entity precision, per-character recall and precision, average character fan-out, all-four assignment rate, final character acceptance, and compute cost. Overall accuracy alone is not sufficient for this permissive top-of-funnel job.

**Tuning gate:** There is a versioned labeled set, a written split policy, and an agreed metric/fan-out budget before thresholds are introduced. This is not a cutover prerequisite.

### Phase 2 — Train or calibrate the Laya comb after cutover

Treat today's zero-shot prompts as a baseline, not the production router.

- Evaluate a trained/calibrated Laya model for the entity gate and four independent character decisions.
- Compare it with a small shared-encoder multi-label classifier if that is cheaper or easier to calibrate.
- Calibrate decision values on held-out data before describing them as probabilities or applying numeric thresholds.
- Optimize the entity gate for high useful-news recall while measuring how much irrelevant work it forwards.
- Optimize each character route independently; do not force a single shared threshold or exclusive class.
- Preserve complete distributions and model/version metadata for replay.
- Test truncation, malformed output, unavailable model service, and checkpoint mismatch as explicit errors.

**Tuning gate:** Introduce selective routing only after a calibrated model beats the broad advisory baseline on held-out nightly data and satisfies agreed recall and fan-out budgets.

### Phase 3 — Make acquisition and reverse enrichment durable

Build the non-generative intake layer around the classifier.

- Canonically deduplicate every Google candidate while retaining all source/query provenance.
- Attempt publisher acquisition for every canonical result; do not restore a ten-result cap.
- Add bounded retry/backoff and a documented fallback policy for 402, 403, 429, malformed HTML, timeouts, and low-content/video pages.
- Never substitute a headline-only RSS description without marking a different evidence contract.
- Retain enough source provenance to replay extraction: original URL, final URL, domain, source/body hash, headline, retained body or authorized source reference, and exact byte offsets.
- Store the exact first three source sentences, or all available opening sentences when fewer than three are detectable.
- Retain source-context classifications and explicit acquisition/classification errors for audit under an explicit retention policy.
- Decide after a schema audit whether to extend `news_articles` or add dedicated Harvester run, context, and assignment records. Avoid duplicating existing `full_text` ownership accidentally.

**Exit gate:** Every canonical candidate has one terminal, queryable state—duplicate, acquired and classified, acquisition error, or classification error—and every accepted context passes byte-range verification.

### Phase 4 — Add a claim-fenced Harvester adapter

Harvester must not join the deployed worker roster until it can finish durable work atomically.

- Claim work through the existing `pipeline_work` lease/fencing contract.
- In one fenced transaction, persist the classification record, exact context, per-character assignments, required handoffs, and work completion.
- Use stable idempotency keys derived from the canonical article, query entity, and contract/model version.
- Reject stale workers at every authoritative write.
- Make retries safe after partial network/model work and process restarts.
- Make replay an explicit operation that cannot silently duplicate downstream character work.
- Add database integration tests for stale leases, duplicate delivery, transaction failure, retry, and concurrent claims.

**Exit gate:** A crashed or stale worker cannot lose, double-publish, or falsely complete a Harvester item.

### Phase 5 — Make each character's final decision auditable

Every advisory assignment needs a terminal character-owned outcome.

| Character | Required cutover work |
|---|---|
| Journalist | Receive full Harvester context and source metadata; retain explicit cited source IDs; distinguish irrelevant from relevant-but-unused or redundant evidence; reject uncited factual claims. |
| Influencer | Add per-source accepted/rejected IDs or citations so batch output can be traced to individual Harvester contexts. |
| Insider | Resolve person/team identity and construct the real transfer evidence pair before cognition; never treat a character tag as transfer truth. |
| Scout | Resolve identity and roster availability, then merge appropriate reports with measured evidence through the existing adapter; never let news text alter statistics. |

Shared requirements:

- Keep the headline and exact source text unchanged in character context.
- Record one terminal status for each assignment: used, relevant-but-unused, redundant, irrelevant, abstained, or error.
- Preserve the source IDs that support every published claim.
- Keep Analyst and Oracle on their existing finished-product inputs.
- Bound batch size without silently dropping assignments.

**Exit gate:** Every Harvester tag has an auditable character outcome, and no source disappears between assignment and final product decision.

### Phase 6 — Move Editor's durable side effects

Prune Editor only after every current effect has a tested new owner.

| Current Editor effect | Recommended future owner | Required proof before cutover |
|---|---|---|
| Publisher fetch and `news_articles.full_text` retention | Harvester acquisition adapter | Parity on success, failure, retry, and provenance |
| Fetch/failure bookkeeping and exact claim completion | Harvester adapter | Fenced integration tests and replay safety |
| Name resolution and unknown-person nomination | Explicit Investigator/Graph handoff | Known/unknown identity cases and idempotent nominations |
| Authoritative entity links | Resolved identity writer, never a Laya tag | No advisory classification can create an authoritative link |
| Fixture-result nomination | Existing deterministic fixture/Boxscore owner, confirmed by dependency audit | Equivalent nominations on frozen cases |
| Storyline membership | Retire with stories | Character products preserve article IDs without storyline IDs |
| Deterministic packet compilation | Retire | Every character consumes verified publisher context directly |
| Graph enqueueing | Harvester/identity outbox handoff | Atomic enqueue and duplicate-delivery tests |
| Editor semantic decisions, generated story output, and voice | Retire | Shadow evidence shows no remaining consumer |

Before code deletion, audit database triggers, views, queue stages, scheduled jobs, APIs, analytics, and dashboards for hidden Editor dependencies.

**Exit gate:** An ownership matrix names a live, tested owner—or an explicit retirement decision—for every Editor effect. No effect is removed merely because it lived in Editor.

### Phase 7 — Shadow on the production nightly flow

Use the Mac model host without competing unsafely with live production.

- Run the full nightly candidate population through Harvester in shadow mode with no public output and no duplicate character publication.
- Serialize or safely batch Mac MPS inference; prior concurrent model calls were unstable.
- First compare acquisition, entity decisions, and routing against labeled samples.
- Then enable limited character shadow delivery with explicit cost and audit capture; all four character assignments remain visible.
- Track queue age, nightly completion time, source failure rate, Laya error rate, average fan-out, all-four rate, character accept/reject rate, Granite calls/tokens, and missed gold cases.
- Sample fresh disagreements for human review each cycle.

**Exit gate:** Harvester completes the full nightly population inside the operational window for repeated cycles, accounts for every source and assignment, and does not interfere with production inference. Routing quality is observed, not used as an admission gate.

### Phase 8 — Cut over and prune Editor

- Stop creating new Editor semantic work only after Harvester owns all required intake effects.
- Drain or explicitly migrate existing Editor claims.
- Enable Harvester delivery by character in reversible stages.
- Keep replay/audit access and a rollback switch through the stabilization period.
- Remove Editor prompts, semantic branches, packets, stories routes, generated story products, and dead tests after consumers are proven absent.
- Perform destructive database cleanup only in a later, reviewed migration after the application no longer reads the data.

**Exit gate:** The live path contains no Editor semantic dependency, all durable obligations remain healthy, and rollback has been tested.

### Phase 9 — Operate and tune

- Monitor retrieval volume, dedupe rate, acquisition failure by domain, entity-gate drift, per-character fan-out, final character rejection, latency, and cost.
- Refresh labels from new entities, sources, seasons, and story types.
- Version every classifier, calibration, prompt contract, and extraction contract.
- Re-evaluate thresholds on temporal holdouts; never tune directly against the live evaluation set.
- Keep false-negative review prominent because rejected stories cannot be recovered by the final character guard.

## Production acceptance checklist

- [ ] Every canonical Google candidate is accounted for as a duplicate, acquired/classified item, acquisition error, or classification error.
- [ ] No Editor-era ten-result ceiling remains.
- [ ] Every accepted context is exact source text with a retained headline and verified byte provenance.
- [ ] Fetch/model/transport failures cannot become irrelevant decisions.
- [ ] Laya character values are either calibrated or clearly represented as uncalibrated distributions.
- [ ] Every durable write and assignment is fenced and idempotent.
- [ ] Every character assignment reaches a recorded terminal character-owned outcome.
- [ ] Published claims retain source provenance.
- [ ] Insider and Scout receive their required resolved identity and structured evidence, not raw tags alone.
- [ ] Analyst and Oracle do not receive raw Harvester context.
- [ ] Every non-editorial Editor side effect has migrated or has an explicit retirement decision.
- [ ] The full nightly population completes within the agreed production window on the Mac without disrupting production.
- [ ] Reverse enrichment can retrieve and replay stored opening context by article, entity, source, contract version, and character assignment.
- [ ] Unit, integration, replay, and shadow acceptance suites pass before roster enrollment.

## Risks to keep visible

- Publisher blocks and low-content pages can bias the surviving corpus if treated as relevance failures.
- An uncalibrated permissive router can erase Laya's cost advantage by sending nearly everything to every character.
- Over-tightening the entity gate creates unrecoverable false negatives before Granite sees the story.
- Batch character outputs can merge stories, omit citations, or leave inputs without an explicit disposition.
- A hidden Editor trigger, queue handoff, or analytical dependency can make apparently safe deletion destructive.
- Mac MPS contention or concurrent calls can destabilize both Harvester and production cognition.
- Stored source text and replay retention must follow the project's existing licensing and retention policy.
- Ignored local traces are not durable evidence and may disappear between sessions.

## Decisions for the next session

Resolve these before production code is widened:

1. Who reviews the provisional AI annotations for later calibration?
2. How many later nightly dates and entities constitute the first calibration set and untouched temporal holdout?
3. What latency, completion, and Granite-cost budgets define operational cutover success? Set recall/fan-out thresholds during later calibration.
4. What is the retry/fallback policy for blocked or non-article publisher pages?
5. Is exact stored context modeled once per canonical article with many entity links, or once per article/entity classification?
6. How long are source-context classifications and errors retained for replay?
7. Which non-story Editor side effects still require a new owner? Stories, storylines, and packets are explicit retirements.

## Fresh-session resume block

```bash
cd /Users/scotty/scoracle/scoracle-backend/rust
git status --short
sed -n '1,320p' docs/PLAN-harvester-production-cutover.md
sed -n '1,360p' docs/harvester-cascade-2026-09-26.md
cargo test --lib
```

Recommended first work in that session:

1. Read this plan and the cascade record; inspect the dirty worktree before editing.
2. Preserve the corrected cohort evidence or convert representative cases into durable fixtures.
3. Define the annotation schema and label the current cohort independently for entity plus four character decisions.
4. Audit the database/queue effects currently owned by Editor and confirm the future-owner matrix.
5. Choose training, calibration, and production thresholds only after the labeled baseline exists.
