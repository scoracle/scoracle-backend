# Model qualification and card availability — October 21, 2026

**Model approval: RED. Identity containment and the availability repairs are deployed; a fully qualified replacement model has not been established.** The complete six-character fixture assessment found factual failures that parser acceptance alone does not detect. No candidate model or experimental prompt was promoted.

This follows the [Gordon incident and wider repair](2026-10-20-gordon-transfer-incident.md). That repair withdrew 269 legacy readings, 255 rumors and 392 dependent Sigils, and repaired 686 metadata cells. The canonical-name comparison across 1,126 retained source-backed Insider generations found no reassigned names. The confirmed identity bug was writes scoped by numeric ID without sport, alongside inadequate legacy subject validation.

## Availability repairs and live recovery

The daily controller was stuck in draining. Thirty-four orphaned controller queries included a query running for over three hours. The freeze repeatedly scanned selected spectra for each assignment; controllers held the transition lock and blocked publishers. Recent editions missed their windows and the latest generated cards dated to October 16.

[Migration 318](../../sql/migrations/318_coverage_freeze_set_based.sql) groups pending spectra once and joins that result to assignments. The selected spectra and no-evidence assignment semantics are unchanged. Its rollback test covers 12,006 assignments, shared numeric IDs across sports and types, pending versus settled deliveries, and parity with the original selector. Server-side statement and lock timeouts now precede the controller's outer timeout, preventing another abandoned controller query. Only the identified orphaned controller queries were canceled.

A separate packet failure allowed one superseded source receipt to reject an entity's complete current packet. The Classifier delivery reader now excludes superseded receipts before deserialization and selection. They remain pending for reconciliation; they are not silently settled. Current evidence still undergoes identity checks, and publication rechecks source currency. The database regression proves that a revised source cannot publish while another current source remains usable.

A bounded ID/hash audit found 322 stale pending source versions among 17,420 unique versions: 122 football, 104 NFL and 96 NBA. Normal enqueue and Classifier reconciliation were used. At apply time 148 still needed reconciliation, five queue rows were newly inserted, and a targeted normal-path worker committed three remaining NBA jobs. Older sources retained the existing history-only policy.

Runtime `0b3eb01a43875bac6a3cc33ccaebf2b79d639ea4` is running on Mac and Archbox. Both passed database, memory-helper and plugin startup checks. The original binaries and launcher are archived. Model routes remain unchanged.

On October 21 at **9:54:26 a.m. Detroit time**, the missed October 21 edition was reopened with its prior schedule and edition archived. Recovery required the expected partial/missed status, coverage phase and no existing assignments. The controller naturally froze at **9:58:52 a.m.** and opened its normal six-hour character window, ending **3:58:52 p.m.** No artificial clock was supplied. Under the existing product convention, this edition publishes the **October 20 reporting day**, not an October 21 reporting-day card. The cutoff is explicitly the actual recovery time.

A read-only full preparation pass covered 2,510 Scout subjects in 24.202 seconds: 1,489 usable profiles and 1,021 without an eligible measured profile. The freeze therefore fits the controller budget after the query repair. These are evidence-eligibility counts, not model-quality or published-card counts.

The recovery receipt records new publications in five stages: 37 ratings, six transfers, four momentum readings, two narrative products and one Sigil. That is an intermediate snapshot, not completion of the edition. Most assignments were still pending. No complete-edition or six-hour throughput claim is made.

## Measurement error found in Gordon's live packet

The source packet labeled NBA `fta` as **field goals attempted**. Both SQL measurement producers read free-throw attempts but emitted the wrong measurement name. This was a source-contract defect, independent of the model and the provider-ID hypothesis.

[Migration 319](../../sql/migrations/319_nba_free_throw_measure.sql) corrects the two function definitions and stored NBA measurement labels. Production changed 4,470 player rows and 240 team rows. Transactional parity checked that every other field, including statistics, scores, ranks and sample counts, remained identical. Complete affected rows were archived privately. Missing measurements and canonical IDs were preserved.

The live edition already had 272 frozen packets containing the incorrect label. A private one-off repair deserialized the existing Scout types, verified the original rendered prompt and input hash, corrected only that measurement and its provenance, rendered the packet again and recomputed its hash with the existing function. Exact before/after comparison permitted only those label changes and the resulting hash. Old leases were revoked, assignment revisions were updated, and all 272 were re-enqueued normally. The selected packets had no committed daily publication. Zero incorrect frozen labels remained afterward.

Player, team and per-36 tests distinguish `fta` from deliberately different `fga` values. The populated [source-identity test](../../sql/tests/264_rating_source_identity.sql) also verifies the corrected identity in stored player and team breakdowns. It is now included in the release checks.

Gordon's old statistical body reversed the meaning of improved turnover standing. His corrected frozen packet was processed through an exact player/177/NBA/rating lease and the ordinary production handler, model, ledger and publication fences. New summary **69580**, generated at **10:10:36 a.m. Detroit time**, is served by the public rating endpoint. It names Aaron Gordon, reports ball security as above average, and uses the corrected free-throw evidence. The new body remains verbose and shortens “free throws attempted” to “free throws”; it is **not** evidence of full editorial approval.

Gordon's transfer endpoint still correctly has no eligible replacement move and no DeRozan reading. The current-corpus preparation found no eligible Gordon transfer, narrative or mood packet. Absence of such evidence cannot be repaired by inventing reporting. His statistical card is available; universal nonempty transfer coverage has not been achieved.

## Six-character qualification

The incumbent was `Qwen3.5-4B-Q4_K_M` on native llama.cpp, reasoning off, four physical 10,240-token slots and a 10,000-token request envelope. Its weights SHA-256 is `00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4`. The serving build was 0.6.0 / 11429 / `d81235049`.

The expanded suite has **44 character cases**, excluding three separate Investigator fixtures. It includes the Gordon/DeRozan negative case in Insider, Journalist and Influencer; a separate shared-decline holdout; fully named transfer positives and a denial; unnamed-signing identity ambiguity; NFL measured zero; and football player totals. Original evidence packets were retained. Fixtures whose quotes omit full names cannot establish a linked transfer under the current evidence policy; their expected findings were corrected accordingly.

The completed first-pass run used the production temperatures: Analyst and Insider 0.3, Oracle and Scout 0.6, Journalist and Influencer 0.0. It retained complete prepared input, exact native tokenizer evidence, request, raw response, completion reason and parser results. It did not publish, use a semantic judge or retry failed cases. Subsequent production-parser repairs were replayed offline against the same 44 saved replies. The table below is that latest-parser assessment; it is not a new model run or a simulation of each plugin's bounded rewrite lifecycle.

| Character | Cases | Accepted mechanically | Rejected | No model call |
|---|---:|---:|---:|---:|
| Scout | 11 | 8 | 3 | 0 |
| Journalist | 11 | 9 | 1 | 1 |
| Insider | 7 | 3 | 4 | 0 |
| Influencer | 5 | 5 | 0 | 0 |
| Analyst | 6 | 5 | 0 | 1 |
| Oracle | 4 | 1 | 3 | 0 |
| **Total** | **44** | **31** | **11** | **2** |

Four accepted cases correctly produce no content: the Journalist Gordon negative and three Influencer evidence/quiet cases. Missing Analyst peers and an empty Journalist corpus correctly avoid inference. Neither warranted abstention nor a no-call is an invented replacement card. Conversely, “accepted” does not mean factually correct or useful.

Direct comparison with supplied evidence found release-blocking false accepts:

- Analyst describes the shared-decline holdout as both reinforcing and diverging.
- Journalist adds a scoring-leader claim when the report supplies only a player's point total.
- Scout calls an ordinary 25-game sample a sample limit, states incompatible measurements support comparison across seasons, and turns an unknown sample's policy minimum of 10 into 10 observed records.
- Insider adds a “following” chronology between independent community-event and unnamed-signing reports. The player identity is not joined, but the chronology is still unsupported.
- Oracle's outputs include unsupported embellishment, metadata presented as substantive reporting, invented source-count framing and excessive prose. Three cases fail the explicit style constraint.

These failures disqualify the incumbent from full model-quality approval. Five passing Influencer fixtures do not establish population-wide reliability. The suite is targeted incident and evidence-contract coverage, not a representative statistical estimate of production error rate.

The evaluation framework now uses the actual Oracle and Analyst production parsers. An unparsed reply cannot pass an empty rubric. Missing-peer fixture execution matches production's no-call path. This removes misleading green evaluations; it does not provide semantic entailment validation.

Safe runtime repairs found during qualification include Scout direction clause boundaries, an explicit source-count qualification, and treating “While” as prose rather than part of an invented capitalized name. Insider now rejects a body consisting only of the canonical subject and allows the existing bounded surface correction. Its output contract is `insider-reading-findings-v5-prose`; the subject/evidence prompt remains v19. A real production-factory denial replay required two calls and produced substantive prose. Invalid full-name quotes remained rejected. No generic evidence rejection was converted into an unrestricted rewrite.

## Candidate trials and remaining gates

Higher quantization, alternate prompt projections, zero temperature and 9B configurations were explored privately. Failed experimental prompts and schemas were reverted; none are active runtime instructions.

The 9B Q4 artifact was pinned to Unsloth revision `3885219b6810b007914f3a7950a8d1b469d598a5`, SHA-256 `03b74727a860a56338e042c4420bb3f04b2fec5734175f4cb9fa853daf52b7e8`. Unbounded reasoning took roughly three minutes per challenge; four of five exhausted the answer budget without a final answer. Bounded reasoning at 256 tokens still reversed the shared-decline meaning.

A compact system-task/body-only synthesis experiment with a 128-token reasoning budget produced five factually promising challenge outputs in roughly 11–13 seconds each. That changed the candidate input projection and removed the Analyst's abstract relationship verdict; it was not a validated production prompt.

A wider 39-case candidate diagnostic was stopped after 16 completed cases: ten mechanically accepted, four rejected and two no-calls. Rejections included malformed JSON and a capitalized headline falsely interpreted as a new name. Its OpenAI-compatible diagnostic transport lacks the exact tokenizer admission required by production. The isolated 9B server also competed for the production Mac's memory. It was stopped, and no replacement route was installed. This partial diagnostic is not a complete 9B qualification or a clean capacity benchmark.

A bounded current-corpus replay covered eleven available packets for Gordon and same-ID Denver/Liverpool/Cleveland. It reproduced usable statistical outputs and both warranted abstentions and publication-blocking errors. The NFL thin-sample wording, for example, was rejected for “reliable comparison” language. These failures and the stale-source issue show why a simple parser-pass percentage cannot explain empty cards.

Still required for model promotion: a candidate with zero observed critical subject/factual errors, complete output under its production transport and retry policy, usable content for eligible evidence, independent held-out cases, and measured capacity for the actual daily workload without disturbing production. The source/identity/full-name/quote checks and complete-input admission must remain intact. A suitable hosted inference endpoint or confirmation that the next search must remain local has been requested; no endpoint has been supplied in this chat.

## Software validation and recovery evidence

The complete release check passed on a fresh disposable PostgreSQL 18 database: baseline checksum/restore, migration no-op, SQL contracts, Go formatting/vet/build, 360 Rust library tests, 17 bin tests, 64 database contracts, prepared-statement registration and all Go race tests. Two live/private Rust replay tests remain intentionally excluded from the ordinary database suite. Relevant real model trials were executed separately and are not called passing software tests. The additional populated free-throw identity assertions passed after being added to the check script.

Draft [PR 16](https://github.com/scoracle/scoracle-backend/pull/16) contains the repairs and qualification changes. Model approval remains RED; the PR has not been merged.

The original incident and Archbox runtime archives remain under `/mnt/data/backup/scoracle/incidents/subject-contamination-20261020/`. Today's complete qualification and recovery receipts are retained locally, with private permissions, at `/Users/scotty/scoracle-worker/releases/identity-availability-20261021-0b3eb01a/qualification/`: edition and queue backups, stale-version manifests, migration/parity logs, frozen-packet backups, raw trials, one-off tool sources and public API receipts. Automatic approval review rejected transferring these sensitive production packets to Archbox; no such transfer was performed. Model weights are represented by pinned revisions and hashes rather than copied into the receipt archive. Raw production packets are not committed to the public repository. Restoring withdrawn products or old frozen packets would reintroduce known defects.
