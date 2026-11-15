# Harness → plugins → skills

Status: implemented on `codex/character-skills`; local validation complete, not deployed. Updated 2026-11-15.

## Objective and scope

Establish three plugin owners: Classifier, Character, and Caretaker. Prioritize a working Classifier and Character skill runtime, fewer registration paths, and preserved durability. Caretaker is an organizational placeholder for existing machinery only: no new maintenance workflows, discovery gates, repair policies, background schedules, or enablement.

This plan supersedes the six-character-plugin structure for this migration. README and prompt-ownership documentation now describe the hierarchy; older architecture records remain historical.

## Ownership

| Layer | Owns |
| --- | --- |
| Harness | Claims, retries, scheduling, shared resource enforcement, model routing, scoped capabilities, transaction fencing, outbox lifecycle, diagnostics |
| Plugin | Capability family, shared dependencies, skill roster, plugin-local shared policy |
| Skill | Task policy, input preparation, instructions, permitted tools/routes, output contract, validation, execution and domain publication |

One Character plugin owns six skills; it does not serialize them or union their permissions. Skills remain independently enabled, claimed, retried and scheduled. Code selects the skill from the existing durable task. No model-directed skill selection or dynamic skill loader is needed.

| Plugin | Initial skill/module | Durable task |
| --- | --- | --- |
| Classifier | classify | classifier |
| Character | journalist | narratives |
| Character | influencer | vibe |
| Character | scout | rating |
| Character | insider | transfers |
| Character | analyst | momentum |
| Character | oracle | sigil |
| Caretaker | investigator (retained, inactive) | investigate_entity |
| Caretaker | fixture_boxscore (retained, inactive) | fixture_boxscore |

Character persona and skill identity are separate concepts, although initially one-to-one. Keep current character names for module names to minimize churn. Note that Insider's existing task is `transfers`, not `transfer`.

## Caretaker inventory and disposition

The source paths below record the pre-migration inventory. Investigator and fixture_boxscore now live under `src/plugins/caretaker/skills/`. Inventory is based on the Rust sources and related SQL schema; it is not a full audit of external services or historical migrations.

| Existing machinery | Disposition | Reason and constraints |
| --- | --- | --- |
| `src/plugins/investigator/adapter/{mod,discover,publish}.rs` | Move under Caretaker's investigator skill | Candidate investigation, structured Wikimedia acquisition, player/team enrichment, canonical entities, aliases, external IDs and relationships belong to entity maintenance. Preserve current gates, provenance, refusal behavior and claim-fenced writes. |
| `src/plugins/investigator/cognition/` | Move with Investigator | Existing role classification and historical prose evaluation remain attached to their owner. Moving code must not enable prose extraction. |
| `src/plugins/investigator/adapter/factsweep.rs` and `src/bin/factsweep.rs` | Move implementation; update CLI import | Compatibility entry point currently refuses before retrieval or writes. It must continue to refuse. Do not implement factsweep. |
| `src/plugins/fixture_boxscore/` | Move under Caretaker's fixture_boxscore skill | Existing deterministic acquisition and persistence of underlying fixture data fits database upkeep. Preserve parser, source selection, fetch policy and publication. Remain outside the enabled fleet. |
| Classifier `identity.rs`, `delivery.rs`, source fencing and candidate reads | Keep with Classifier | They verify identity consistency and source provenance at classification/delivery boundaries; they are not entity creation or repair jobs. Do not move merely because names mention identity/discovery. |
| Classifier headline replay in `adapter.rs` | Keep with Classifier | Maintenance of classification obligations belongs with its workflow. Preserve cadence and registration; do not run once per skill accidentally. |
| Scout `RatingIdentityReaction` | Keep with Character/Scout | Consumes identity-change events to enqueue rating work; it does not apply identity changes. |
| Insider validation and transfer-product publication | Keep with Character/Insider | Character product logic and counterparty validation remain with the output that requires them. |
| SQL `apply_transfer_identity_candidate` and transfer identity outbox constraints | Document as existing maintenance-related database contracts; leave in place | Schema contains these functions, but the inspected Rust source has no call to the apply function. Do not invent a Caretaker producer or revive retired behavior. Preserve `transfer_identity_applied` event compatibility. |
| `src/tools/meta.rs`, shared web/source tools, SQL schema/migrations | Keep as shared infrastructure/data contracts | Readers and infrastructure do not become Caretaker-owned simply because they access canonical data. Move only helpers proven exclusive to a moved skill. |
| Harness queue recovery, coverage, publication and outbox | Keep in Harness | Execution durability is host responsibility, not database-domain maintenance. |
| `src/plugins/text_generation.rs` | Account for as provider wiring, not a fourth domain plugin | Move binding/transport wiring to the existing harness provider boundary if caller review confirms no domain policy. No new provider abstraction. |

Investigator and fixture_boxscore are statically present but excluded from `fleet::ALL` and rejected by current enabled-stage configuration. Preserve this inactive status. A Caretaker plugin can have zero enabled skills; do not manufacture a no-op queue task or require model/network initialization for the placeholder.

## Target organization

```text
src/harness/                       # shared runtime and provider machinery
src/plugins/
  classifier/
    skills/classify/               # existing classifier workflow
  character/
    skills/
      journalist/
      influencer/
      scout/
      insider/
      analyst/
      oracle/
  caretaker/
    skills/
      investigator/                # existing code, inactive
      fixture_boxscore/             # existing code, inactive
```

Keep typed inputs, parsers and publication functions. Shared code belongs at the nearest actual shared owner. Do not create a universal context, generic workflow engine, tool registry or forwarding layer merely to make every skill look identical.

## Implementation checkpoints

### 0. Isolate and establish baseline

- Use branch `codex/character-skills` in a separate worktree from local HEAD `9be32e645648ec814a9cba4981a6031d8d597c2c`.
- Preserve the source checkout's seven local commits and uncommitted tuning work; do not copy unrelated dirty files or fixtures.
- Before code changes, record the available offline test baseline and database-test prerequisites.

### 1. Separate plugin ownership from executable skill policy

- Split current `PluginManifest` responsibilities: plugin identity/roster versus skill identity/task/claim policy/resources/routes/grants.
- Adapt the existing object-safe execution seam for skills rather than adding a second competing executor. Preserve outcome and publication receipt semantics.
- Registry validates unique plugins, unique skills within a plugin, unique durable task ownership, coherent grants/routes and resource-group budgets.
- Worker consumes enabled skill registrations with their owner identity. Keep scheduling order and shared backend limits intact.
- Empty plugin/empty fleet is valid. Scheduled operations and reactions register once under the appropriate owner.
- Tests: duplicate ownership, idle Caretaker, scoped capabilities, per-skill caps and sibling progress after failure.

### 2. Assemble Classifier and Character

- Convert `registration.rs`, `fleet.rs`, model capability resolution and worker wiring to plugin-owned rosters.
- Classifier provides its current classify skill and existing replay operation. Character provides all six existing handlers as skills.
- Preserve `COGNITION_STAGES`, including Classifier opt-in, explicit idle configuration and inactive maintenance stages.
- Preserve model route names/environment suffixes, generation settings, claim ordering, fairness and shared GPU budgets.
- First prove runtime parity with existing module locations; then move modules and update all callers in a mechanical checkpoint.
- Reuse shared prompt assembly already in `tools/prompt.rs`; avoid building another layer that duplicates it.

### 3. Organize Caretaker without expanding it

- Add the Caretaker owner and move the two retained maintenance modules, tests and private helpers.
- Update factsweep CLI, investigator evaluation imports, manifest references and compile-time fixtures/include paths.
- Preserve inactive registration, unavailable factsweep behavior and historical evaluation semantics.
- No additional database migrations or new handoff workflows for Caretaker.

### 4. Simplify, preserve provenance and validate

- Remove obsolete six-plugin registration glue, duplicated rosters and temporary re-exports after migrating callers.
- Review `ledger.rs` and every ledger producer/reader. Record owner plus skill in new diagnostics while keeping historical rows interpretable. Prefer existing task/stage identity for skills if sufficient; add a column only if a demonstrated query requirement needs it. Document the legacy-ID mapping before changing emitted IDs.
- Preserve prompt/input versions and hashing unless content actually changes. Module relocation alone must not invalidate products.
- Keep all claim fences, source revalidation, atomic product/completion/follow-up writes, partial progress and stale-claim handling.
- Update README, character prompt ownership docs, examples, evaluation entry points and relevant tests. Historical release records need no wholesale rewrite.

### 5. Verify and prepare rollout

- Run formatting, compile all affected targets, and focused registry, config, routing, worker, parser and evaluation tests.
- Run existing disposable-database publication/outbox/coverage tests when prerequisites are available; report unavailable coverage explicitly.
- Compare deterministic prepared prompts/options/hashes and recorded-response parsing using existing fixtures. Do not require stochastic live output equality.
- Verify rating/vibe → momentum and completion barriers → sigil, independent reactions across partial fleets, source fencing and stale-claim rejection.
- Verify unchanged stage/config vocabulary and that Caretaker performs no newly enabled work.
- Prepare deployment/rollback instructions using existing stage controls and a known prior binary. Production rollout is a later action; the setup task does not deploy.

## Completion criteria

- Three domain plugin owners exist; Character exposes six independently runnable skills and Classifier exposes its existing work cleanly.
- Caretaker houses existing maintenance machinery and remains inactive.
- One harness execution path enforces scheduling, capabilities and durability for every skill.
- No task/schema/prompt changes are introduced merely to rename ownership.
- Existing CLI/evaluation callers compile, relevant checks pass, and any unrun database/live-model checks are documented.
- Adding a character output means adding a skill to Character, without another top-level plugin or bespoke worker path.

## Deferred

New entity finding, identity adjudication, factsweep implementation, cleanup/repair jobs, automatic maintenance schedules, dynamic skill loading, model-selected tools, prompt/model tuning, and a generic cross-plugin messaging framework. None is required to establish this hierarchy.


## Implementation result — 2026-11-15

Checkpoints 0–4 are implemented. Checkpoint 5 has passed local validation; production rollout and live-model qualification remain separate work.

- `PluginManifest` declares a capability owner and its skill roster; `Plugin` binds its enabled handlers. `SkillManifest`, `Skill`, and `SkillOutcome` replace the former per-task plugin execution contract.
- Registry validates declared and enabled ownership, duplicate task/skill registration, exact manifest membership, inference grants and consistent shared-resource budgets. The worker flattens enabled skills once at boot and preserves its existing scheduling, retry and publication behavior.
- Classifier owns `skills/classify`; Character owns the six character skills; Caretaker retains Investigator and fixture box-score code but binds zero handlers. Configuration still rejects both maintenance stages. Factsweep still refuses execution.
- Text-generation binding moved into harness providers. Old module paths and redundant fleet manifest exports were removed; CLI, evaluation and example callers were migrated.
- Generation diagnostics now use the Character owner plus the unchanged stage. Source/delivery receipts retain existing skill IDs, preserving their compatibility and freshness behavior. No schema or prompt versions changed.

### Provenance compatibility

| Existing skill/legacy plugin ID | Plugin owner | Durable task |
| --- | --- | --- |
| `scoracle.internal.classifier` | `scoracle.classifier` | `classifier` |
| `scoracle.character.narrative` | `scoracle.character` | `narratives` |
| `scoracle.character.vibe` | `scoracle.character` | `vibe` |
| `scoracle.character.rating` | `scoracle.character` | `rating` |
| `scoracle.character.transfers` | `scoracle.character` | `transfers` |
| `scoracle.character.momentum` | `scoracle.character` | `momentum` |
| `scoracle.character.sigil` | `scoracle.character` | `sigil` |
| `scoracle.internal.investigator` | `scoracle.caretaker` | `investigate_entity` (inactive) |
| `scoracle.internal.fixture_boxscore` | `scoracle.caretaker` | `fixture_boxscore` (inactive) |

Historical diagnostic rows are not rewritten. Queries spanning the change should use the stable stage and account for the old/new owner mapping. Caretaker's retained ledger behavior is unchanged because its workflows remain inactive.

### Validation evidence

- `cargo test --offline --all-targets`: **446 passed**, 76 intentionally ignored integration/live checks.
- With a newly initialized, migrated, disposable **PostgreSQL 18 UTF8** database and the existing Go memory-study helper: `cargo test --offline --lib --bins -- --ignored --skip local_real_model --skip live_ --test-threads=1`: **74 passed**. Includes queue/recovery, claim fencing, outbox, all character publication contracts, classifier dispatch and retained maintenance tests.
- The unmodified base commit failed 10 of those database tests because its mock decision server and gender assertion expected the former text prompt. Updated only those test expectations to the already-existing JSON brief; all 10 now pass.
- Compiled `character_package` at base commit and working tree; compared complete `--request` output over quality fixtures: **57 byte-identical requests**, zero mismatches. Coverage: Investigator 3, Momentum 8, Journalist 16, Oracle 4, Scout 11, Insider 9, Influencer 6. One nonmatching input was rejected by the baseline and excluded.
- Formatting and whitespace checks passed. No new dependencies or migrations.

### Rollout and rollback

Build from this branch and retain the previously deployed binary before deploying. Keep current `COGNITION_STAGES`, route settings and resource budgets; Caretaker must remain inactive. Check startup plugin/skill inventory, queue progress, source outcomes, completion barriers and owner/stage diagnostics. Roll back by restoring the previous binary with the same configuration; durable task names and schemas are unchanged. No production deployment or live-model quality claim is included in this implementation.


### Pre-commit review

Reviewed registration, scheduling, scoped capabilities, claim fencing, source settlement, atomic outbox follow-ups and inactive Caretaker assembly. Added fail-fast validation for zero-capacity slot groups, which otherwise silently prevent claims, with a regression test. Removed the unused worker skill accessor and merged duplicate registration test modules. The final all-target suite passes 446 tests; the earlier disposable-database run passed 74 integration checks. No additional execution/publication changes were needed. Caretaker remains inactive; deployment and live-model qualification remain deferred.
