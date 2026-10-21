# Aaron Gordon transfer subject incident — October 20, 2026

The production transfer endpoint for NBA player 177 (Aaron Gordon) served DeMar DeRozan’s reading and a purported outgoing Nuggets move. The ID and generated headline belonged to Gordon; the stored evidence and prose concerned DeRozan. This was a publication validation failure in an October 7 legacy output, not an API entity lookup failure. Exposing `wire_read` made the existing bad stored report visible.

## Containment

Preserved complete product rows and generation ledger 838119 before transactionally deleting exactly:

- `insider_scores` 40766 and `transfer_rumors` 239856, both Gordon / NBA / input hash `b7992882ecef711e1e071c0065597196`.
- `sigil_synthesis` 130925 (October 7) and 131069 (October 14), whose readings repeated the same DeRozan claim under Gordon’s ID.

Each transaction checked the exact entity and expected row count. The score and rumor had no retained Harvester wrap or identity-review dependents. The original cognition ledger remains for diagnosis. The Go API was restarted after invalidations to clear all cached copies. No replacement move, score, or generated reading was invented.

Public verification: Gordon’s default and `?date=2026-10-07` transfers return null score/headline/read and an empty transfer list. The Nuggets’ October 7 transfer page no longer lists Gordon. Gordon’s Sigil defaults to its previous October 6 reading, which does not contain DeRozan. The date parameter on Sigil currently does not select historical editions; its response was checked directly rather than assuming date isolation.

## Validation

The invalid rumor’s evidence quote was source article 859922’s headline: “DeMar DeRozan’s Career Takes Stunning Turn After 17 Years in the NBA As Nuggets Reveal New Plan.” It named neither Aaron Gordon nor the full counterparty. Legacy `insider-source-v3` accepted it. The current parser already requires the full target and counterparty names in the same exact source quote, and an explicit move term. The regression verifies that this historical quote is rejected even with a correctly named neutral Gordon body.

The remaining reading-name guard accepted any matching name word anywhere in the body. Gordon’s incidental teammate mention therefore passed. The fix requires the full canonical subject name at the beginning of the reading, with case, accent and typographic-apostrophe folding and a name boundary. The prompt explicitly requests that form; prompt and parser contracts are versioned as `insider-source-v19-subject` and `insider-reading-findings-v4-subject`. Tests cover wrong-person co-mentions, surname-only matches, name substrings, possessives, accents and initials. Rejected replies follow the existing held-delivery path and cannot publish a product.

This is a structural subject and evidence check, not proof that every generated sentence is factually correct. A model can still make an unsupported assertion after a correct subject prefix. The subsequent [model qualification and availability recovery](2026-10-21-model-qualification-and-availability.md) tested all six characters and found additional release-blocking factual failures.

## Wider repair and identity findings

The user first requested an audit only, then explicitly authorized resolving the wider contamination. The [initial audit](2026-10-20-transfer-subject-audit.md) records all 141 candidates and its limitations. All 141 were covered by the subsequent withdrawal.

A database-local comparison of original generation `meta.name` against current canonical names covered 1,126 retained source-backed Insider generations: 438 football, 401 NBA and 287 NFL. All had retained receipts; none had changed names. Gordon’s separately preserved original receipt also used Aaron Gordon / 177 and DeMar DeRozan / 125. No canonical IDs were reassigned during remediation. This evidence does not support provider ID rebinding as the cause of these transfer reports.

A separate SQL identity-boundary bug was confirmed: Investigator player/team convenience updates and the Go headshot writer filtered by `id` alone, although players and teams have composite `(id,sport)` primary keys. The NFL Browns logo fact for ID 8 was correct in `entity_facts`, but its convenience update copied the logo onto NBA Denver and football Liverpool, also ID 8. The fixes scope each write by sport and update every headshot caller. PostgreSQL regression tests use two sports sharing one numeric ID and verify that the other sport stays unchanged.

The repair withdrew the entire two-version cohort that lacked adequate subject validation, rather than attempting to guess which unqualified outputs were true. Additional to Gordon’s initial containment, it archived and removed:

- 268 `insider_scores` readings from `insider-source-v3` / `insider-source-v6-concise-body`.
- 254 `transfer_rumors` from those versions.
- 390 Sigils whose saved `input_components.insider.body` exactly matched a withdrawn reading for the same entity, type and sport.

Totals including Gordon: 269 readings, 255 rumors and 392 Sigils. These totals include unqualified legacy outputs that may have been factually correct; they are not a count of independently confirmed wrong-person claims. No replacement claims were invented.

The repair also examined convenience values matching another sport’s active fact for the same numeric ID, excluding values supported by an owned fact. It repaired 686 cells across 41 teams and 222 players: eight restored from the entity’s own sourced fact; 678 cleared because no owned replacement was available. Facts and canonical identities were retained. A repeated preview after commit found zero selected legacy products, zero dependent Sigils and zero remaining selected metadata copies.

The [repair script](../../scripts/ops/repair_subject_contamination.sql) previews and rolls back by default. Apply requires an explicit private backup path and archives complete affected rows before any update or delete. It refuses unexpected retained Harvester dependents. The [disposable-database check](../../scripts/ops/test_subject_contamination.py) verifies preview immutability, recovery rows, cohort isolation, exact downstream dependency matching, sport boundaries and preservation of owned metadata.

## Runtime and verification

Runtime commit `722a3d882c28` is deployed on both Mac and Archbox workers; Archbox’s importer binary is also updated. The clean Archbox source checkout is pinned to that commit. The API binary remains the prior version because no API code changed; it was restarted to clear cached products. The existing long-running RSS monitor is in `monitor` mode and does not call the changed headshot writer. Configuration, model routing and schema were preserved.

Final validation:

- 358 Rust library tests passed; 65 service-dependent tests were skipped in the ordinary run. The new PostgreSQL metadata regression was executed separately and passed.
- Rust all-target compilation passed.
- Go importer and pipeline tests and importer vet passed. The PostgreSQL headshot regression was executed and passed.
- The repair preview/archive/withdrawal/metadata self-check passed on a disposable PostgreSQL 18 database.
- Twelve public checks passed: Gordon default and October 7 transfers, Articulator p8, October 7 headlines and Sigil; the Siakam and Gibson affected daily transfer pages; Nuggets daily transfers and transfer leaderboard; and same-ID Nuggets, Liverpool and Browns metadata. Gordon no longer serves DeRozan. The Browns retain their owned logo; the other two do not show it.
- Worker logs confirm commit `722a3d882c28`, database connectivity, model reachability and registered stages. The initial Gordon launcher lacked its adjacent memory helper and failed its startup check; the existing verified helper was added, and both subsequent worker activations passed that check.

Read-only Qwen replay accepted the explicit-denial fixture and rejected two other transfer fixtures because their quotes omitted a full party name. All three began with the correct full subject. This is not a passing model-quality gate: incomplete quotes remain rejected through the existing held-delivery path. The incident repair deliberately invalidated unsafe products instead of forcing regeneration or weakening the evidence guard. Those checks were not part of the initial containment. The subsequent [qualification report](2026-10-21-model-qualification-and-availability.md) records the completed database suite, six-character assessment, runtime follow-up and remaining RED model gate.

## Recovery receipts

Private production recovery directory:
`/mnt/data/backup/scoracle/incidents/subject-contamination-20261020/`.

- `rows-before.json`: initial Gordon products and generation receipt.
- `withdrawal-before.jsonl`: 268 additional readings, 254 rumors, 390 Sigils, full prior rows for 41 teams / 222 players, and all 686 repair decisions.
- `repair-apply.log` and `repair-verify.log`: committed counts and zero-candidate follow-up.
- `runtime-722a3d88/`: exact source, builds, installed binary hashes and previous Archbox binaries.

Mac recovery and exact worker/helper hashes:
`/Users/scotty/scoracle-worker/releases/subject-boundary-20261020-722a3d88/`.
The earlier Gordon release directory retains its original row backup and prior launcher. Local test, replay and public verification receipts are also retained under `/tmp/scoracle-gordon-incident/`; required receipts are copied into the durable release directories before delivery. Recovery must restore selected backed-up rows deliberately; restoring the full withdrawn cohort would reintroduce the unsafe products.
