# PLAN — The lean Rust layer

*(Scott, 2026-09-10: "form.rs provides the canvas, the surface, and the character prompts
provide the color. The model does the painting, staying on the canvas and using the colors."
The question is not "what can we trim?" but "what needs to stay?")*

Branch: `cleanup/character-prompts`. Companion doctrine: `DOCTRINE-directing.md`.

## The finding

The load-bearing skeleton is already small. The canvas (`junctions/form.rs`), the six colors
(`junctions/*/prompt.rs`) and the six evidence renderers (`junctions/*/inputs.rs`) are about
1,000 lines of a 45,700-line tree. The goal is not to shrink the skeleton. It is to stop the
other 44,000 lines from burying it.

| Layer | What it is | Lines now | Verdict |
|---|---|---|---|
| Canvas | `form.rs`: shared form, hook rule, six wire contracts, schemas | 157 | Keep as is |
| Color | six `prompt.rs` briefs, 100–200 words each | ~85 | Keep as is |
| Evidence | six `inputs.rs` renderers plus the SQL loaders behind them | ~700 + ~2,500 | Keep; the model needs evidence to paint |
| Fail-closed parse | one `Parser<T>` per seat | ~350 | Keep; the form promises a wire shape, the parser enforces it |
| Integrity floor | `clean_served_prose`, `settle_title`, product-name, foreign-script, bookkeeping-citation, entity-named-in-title | ~300 of 1,070 | Keep; these protect evidence and identity, not wording |
| Machinery | worker, work queue, stage, harness, route, ollama, config, ledger | ~4,300 | Keep; trim the narration |
| Provenance | input_hash debounce, ledger row, built_prompt archive | spread across seats | Keep the contract; collapse the implementation |
| Acquisition | Editor, Investigator, graph, fetch | ~13,000 | Keep; separate cleanup, not the painting problem |
| Junction-owned numbers | Analyst direction/conviction, Oracle omen/convergence, Scout notability/trajectory | ~600 | Keep; code decides numbers, the model interprets them |

## What does not need to stay

None of this is eval.

1. **Dated history narration.** ~7,500 comment lines in non-test files. `guards.rs` is 33%
   comments with 33 date-stamped entries. The README already rules that retired instructions
   belong in git history; retired *reasoning* belongs there too. A comment earns its place
   by saying what the code does or which invariant it holds, not when and why it was
   measured. ~5,000 lines.
2. **Go byte-parity scaffolding.** `GoJson`, `go_json_string`, `trim_float` "like Go", and
   the tests asserting Go marshal bytes. Go no longer produces any of these hashes; parity
   only avoids a one-time regen wave. ~800 lines across 116 call sites.
3. **Legacy-transition tolerance in parsers.** Stripping `PEAK:`/`SIGIL:` markers retired at
   s18, `Analysis:` prefixes, the hookless v12 vibe shape, tests pinning the legacy prompt
   byte-identical to the no-packet prompt. These tolerate models and prompts that no longer
   exist. ~400 lines plus tests.
4. **Ghost inputs in the Influencer.** `influencer/inputs.rs` takes `narratives` and `heat`
   and discards both. The handler still runs two SQL loads for them, sorts them, and folds
   them into the input_hash — she is woken by evidence she never sees. Decision: render them
   or stop loading and hashing them.
5. **Dead guards.** `ORACLE_READING_BANS` and `VIBE_BODY_BANS` are empty lists kept as
   "seams" and scanned on every Oracle and vibe reply; `RATING_BODY_BANS` is a single
   middle-dot; `has_ascii_digit` is test-only. (`truncate_self_review` looked dead but runs
   inside `clean_served_prose`; it stays.)
6. **Six copies of one pipeline.** Every character `mod.rs` repeats: consts, an Output struct
   carrying 10–12 provenance fields, a 35–55 line `CognitionLedgerEntry` literal, and
   `included_evidence` / `excluded_evidence` / `parser_outcome` helpers. The Scout builds its
   20-field Output literal three times. One `Generation<T>` envelope plus one ledger writer
   collapses this. ~1,500–2,000 lines.
7. **The Insider is two things in one file.** 2,848 lines, three model calls, plus a
   transfer-application state machine (`maybe_apply_transfer_identity`, autofill refresh,
   junction-event banking). Split the application out and the Insider's card code looks like
   the other five.
8. **Finished one-shots and dead backend branches.** `bucketlabel` and `storylinefill` were
   one-time jobs and are done. The OpenAI-compatible backend itself stays: route config can still
   select it, and deployment history records that seam as intentional. Its opt-in
   `response_format` branch had no production caller and can go. ~850 lines.
9. **Tests that freeze the slop.** Tests are ~13,000 lines, 28% of the tree. Many are
   regression anecdotes by name or pin the exact things above. Keep: parser fail-closed,
   form contract, hash order-insensitivity, guard behavior, evidence presence. Drop the ones
   whose subject is being deleted.

## The passes

### Pass 1 — free, no behavior change

- Strip narration comments to git history. Rule: keep the sentence that says what the item
  does or which invariant it holds; delete the measurement story, the date, the quote, the
  "used to say". Doc comments on `pub` items may stay at one to three lines.
- Delete dead guards and the empty ban seams.
- Delete `src/bin/bucketlabel.rs`, `src/bin/storylinefill.rs`, and the probe examples
  (`crown_probe`, `graph_probe`, `topology_probe`, `extract_probe`). Fixture generators stay
  (they are the fixtures' source of truth).
- Fix the two compiler warnings (`factsweep`).
- Target: ~7,000 lines. `cargo test` green before and after.

### Pass 2 — behavior-neutral refactor

- One `Generation<T>` envelope (Extracted + Provenance) in `harness.rs`.
- One ledger writer taking the envelope; the six `CognitionLedgerEntry` literals and the
  `*_included_evidence` / `*_excluded_evidence` / `*_parser_outcome` families go.
- Output structs shrink to product fields.
- Measured: 53 lines. The original ~2,000 estimate counted seat-specific product and evidence
  code that still has to exist; the structural gain is eight call sites sharing one envelope and
  one ledger writer.

### Pass 3 — decisions Scott owns

- Drop Go parity; hash with `serde_json` canonical form; accept one regen wave (bump the
  prompt versions in the same commit so the wave is one, not two).
- Drop legacy parser tolerance.
- Resolve the Influencer's ghost inputs.
- Fold the belt-and-braces version leg in the Scout's debounce (the version is already in
  the hash pre-image since s14).
- Measured: 963 lines. The runtime simplification is larger than the raw count: two SQL reads and
  their shaping path left the Influencer; six hand-built hash serializers became `serde_json`;
  current parsers no longer carry retired wire forms; and the Scout now has one debounce key.

### Pass 4 — scope calls

- Split the Insider's transfer application out of the character file. Complete:
  `insider/application.rs` now owns junction-event banking, identity adjudication, roster
  override, Scout re-enqueue, and autofill refresh; the character module owns evidence, model
  verdicts, scoring, and card persistence.
- Shrink `openai.rs`, but keep the backend. Complete: route config and startup can select the
  OpenAI-compatible client, so deleting it would remove a deployment capability. The unreachable
  `with_constraint` / `response_format` fork and its tests are gone; the active unconstrained
  request path remains.
- Keep the eval harness. Audit found 69 frozen cases across nine lenses. Its expectations cover
  semantic properties production guards cannot replace: grounding, narrative grouping, relation
  attachment, transfer direction, and evidence specificity. Removing the optional-field union in
  favor of guards would reduce lines by silently weakening the evaluation boundary.
- Measured: 142 lines. This pass is primarily a responsibility split, not a deletion pass.

### Pass 5 — current contracts only

- Make `COGNITION_STAGES` reject every unknown value. Retired stage names no longer warn and
  disappear, so stale deployment configuration fails visibly.
- Capture eval fixtures only from the current chat `messages` request body. Remove the old
  `/api/generate` prompt/system fallback and read temperature from either active backend shape.
- Enforce the current Analyst `READ … HEADLINE` and Influencer `SCORE … HOOK … VIBE` wire order.
  Retired labels, decorated labels, and out-of-order sections now fail into the normal retry path;
  a missing or unusable title still does not cost an otherwise valid card.
- Measured: 240 lines.

Landing point: ~30,000 lines with the same canvas, the same colors, and one pipeline under
them.

## Ledger

| Date | Pass | Commit | Lines |
|---|---|---|---|
| 2026-09-10 | Plan written | | 45,697 |
| 2026-09-11 | Pass 1 complete | uncommitted | 41,686 |
| 2026-09-11 | Pass 2 complete | uncommitted | 41,633 |
| 2026-09-11 | Pass 3 complete | uncommitted | 40,670 |
| 2026-09-11 | Pass 4 complete | uncommitted | 40,528 |
| 2026-09-11 | Pass 5 complete | uncommitted | 40,288 |
