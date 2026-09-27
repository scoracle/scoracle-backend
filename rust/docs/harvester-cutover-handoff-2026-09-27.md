# Harvester production cutover — debug-session handoff

**As of:** September 27, 2026, about 08:00 EDT  
**Status:** Production shadow sweep still draining scheduled retries. The live cutover and legacy retirement are **not complete**.  
**Deployed code:** `codex/harvester-cutover`, commit `41ea3f4c`, draft [backend PR #15](https://github.com/scoracle/scoracle-backend/pull/15). This documentation handoff advances the branch without changing the production worker.

This is the current-state handoff for a fresh debug session. The longer [implementation status](harvester-cutover-status-2026-09-26.md) and [original production plan](PLAN-harvester-production-cutover.md) contain the history. Some dated sections in those documents describe superseded pre-deployment state; use the live checks below as authority.

## Target state and decisions

Google supplies ranked article/query-entity candidates. Harvester deduplicates them, acquires the **publisher's extracted text**, retains its exact headline and body, and stores byte-verified openings and a bounded verbatim Laya input. Laya answers entity relevance and four character-routing questions as **advisory** distributions. Every acquired candidate remains eligible for each character's own final decision; an Laya `irrelevant` answer is not an admission gate. Journalist, Influencer, Insider, and Scout consume source text directly, record a terminal disposition for every assignment, and attach source IDs and exact quotes to products. Graph and identity work use source-bound receipts. Analyst and Oracle continue to consume finished character products.

Editor, generated packets, storylines, generated stories, and the stories API/UI are to be retired **only after** Harvester's live path, all character product paths, identity/Graph side effects, and Editor's non-editorial duties pass production smoke. The user approved the broad news intake (including women's-team coverage) and deferred relevance tuning. The 84-candidate annotation queue contains 71 **provisional AI** suggestions, 13 pending cases, and **zero human-adjudicated gold** labels. Do not report these as gold accuracy.

### Model change: Granite to Smol

The production character Ollama model is now **Smol**, specifically `OLLAMA_MODEL=alibayram/smollm3`; the Rust runtime default is also Smol (`src/runtime/config.rs`). Earlier experiments and parts of the historical plan used `granite4.2:3b`, and some adapter fallback literals and documentation still say Granite. Treat those as legacy references to review in the cleanup, not the current production model. **Laya is separate from Smol**: its on-host English classifier checkpoint is `55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851` (model identity `laya/english`). The current shadow sweep measures Laya classification, not Smol's live character decisions; Smol must be verified in the bounded character canary.

## Work completed

- Go ingest has an opt-in Harvester path covering every canonical candidate without Editor's ten-read cap. It retains query/team provenance and enqueues Harvester. In shadow mode, Editor still receives its existing live work; once shadow is off, fresh ingest stops adding Editor work.
- The Rust Harvester worker is a normal claimed plugin with four overlapping fetch/claim slots and on-host serialized Laya inference. It records acquisition outcome and attempts, exact source/hash/byte offsets, classification contract and model provenance, four advisory routes, and—only outside shadow—four auditable character assignments. Failures and low-content pages never become negative relevance decisions. The latest source also fails closed if a retained headline changes before delivery.
- Packet-free source adapters exist for Journalist, Influencer, Insider, and Scout. They verify retained source bytes, make their own final use/abstain decisions, and carry source/quote/product provenance. Insider has durable pair, identity-review, and scored-wrap obligations; Scout requires measured or corroborated structured data before ratings. Bounded progress and stale-claim fencing have isolated database coverage.
- Harvester and Graph now own source-bound canonical identity links, unresolved-name receipts, unknown-person Investigator nomination, completed-fixture review, week sealing, dedup scheduling, and Graph handoff. Laya tags alone cannot create authoritative identity links.
- Tested opt-in alternatives exist for ordinary Scout report reads, nightly player-rating selection, Sigil selection, watchdog coverage, narrative-link maintenance, and the restore drill. They have **not** yet been enabled in production.
- Additive migrations through **285** are applied on `archbox`. Migration 285 persists the live canary cohort because completed queue claims disappear. The canary enqueue script refuses an incomplete or byte-inconsistent nightly cohort. Its check script requires durable replay/acquisition receipts; the release/hold scripts bound character delivery and were exercised in a disposable PostgreSQL fixture.
- The production backup was made before migration. `/mnt/data/backup/scoracle/scoracle-20260927T050903Z.dump` (about 3 GB) was present at this handoff; an off-data-disk copy was previously byte-compared. A full Harvester-mode restore drill remains outstanding until after the timed sweep.
- Production `scoracle-api`, `scoracle-cognition`, and `scoracle-laya` **user** services are active. `HARVESTER_INGEST_ENABLED=1` and `HARVESTER_SHADOW_MODE=1`. `HARVESTER_DELIVERY_CHARACTERS` is not yet set. The installed `rust/bin/scoracle-cognition` is intentionally the older shadow binary; do **not** replace it or restart that service before pending/running claims and scheduled retries drain. The newer CI-passed source/build includes the closed delivery gate and headline guard.
- Local Rust library validation last reported **573 passed, 75 ignored**, with Go integration/CI passing at the latest code checkpoint. The production branch is `41ea3f4c`. A fresh `gh pr checks 15` call failed because `api.github.com` was unreachable, so current remote CI status was not reverified in this handoff.

## Production nightly run 333: measured snapshot

The September 27 ingest ran **02:00:00–02:03:13 EDT**: 193.1 seconds, 206/206 searches successful, 4,929 canonical articles, and 7,564 article/query-team edges. At approximately 07:58:34 EDT, the aggregate-only `rust/examples/harvester_nightly_report.sql` returned:

| Measure | Snapshot |
|---|---:|
| Acquired articles | 3,167 |
| Explicit acquisition errors | 1,292: 985 `low_content`, 273 `blocked`, 34 `retryable_error` |
| Duplicates | 470 |
| Missing acquisition state | 0 |
| Classified edges | 4,921 / 7,564 (65.1%) |
| Laya entity relevant / irrelevant | 3,891 / 1,030 |
| Laya classification errors | 0 |
| Unclassified edges | 2,643: 1,894 explicit terminal-error edges, **749 still unaccounted** |
| Harvester queue | 0 pending, 1 running, **693 scheduled retries**, 623 five-attempt dead letters |
| Shadow character assignments | 0, as intended |
| Laya inference | 4.238 s mean, 6.313 s p95 per classified edge; 2.65 mean recommended characters (3.19 on entity-relevant edges) |
| All-four Laya routes | 2,813 / 4,921 edges |
| Source integrity | 4,921 / 4,921 match retained body hash, headline, context bytes, and Laya-input bytes |
| Model/contract consistency | One group: `harvest-context-v1`, pinned Laya checkpoint above, `harvest-relevance-v3`, `harvest-character-routing-v3`; 0 reused pre-ingest classifications |
| Exact corpus end-to-end wall time | **Not available yet**; the report intentionally leaves it null while actionable work or unaccounted edges remain |

The 4,921 classifications took 21,513.4 seconds from ingest start to the last classification in this snapshot (about 5 h 58 m, roughly 823 edges/hour over that elapsed interval). This is **interim throughput**, not the final whole-corpus rate. Acquisition errors are distinct from Laya errors and from Laya `irrelevant` answers. The increasing low-content/blocked/retry counts warrant source/domain-level diagnosis **on the production host** after the timed sweep; the optional Chrome fallback is off so this run keeps one retrieval configuration. No publisher text or URLs were exported to the local machine.

Laya recommended Journalist on 3,489 edges, Scout on 3,181, Insider on 3,470, and Influencer on 2,908. Those are routing volumes, not effectiveness against human labels. A historical body-hash-matched replay against provisional AI suggestions showed useful recall proxies but broad Influencer/Insider fan-out; see the implementation status for exact numerators. Human adjudication and post-cutover calibration are still needed.

## Issues and unfinished gates

1. **Shadow sweep incomplete.** The 693 scheduled retries and 749 unaccounted edges prevent a final wall-time measurement and block the gated live canary. Do not infer completion from zero pending rows; include running work, attempts below five, missing acquisition state, and unexplained edges. Check the aggregate report until `corpus_end_to_end_seconds` is populated, then capture exact wall time, throughput, acquisition/Laya errors, source-byte integrity, model/contract mix, and Laya routes. Inspect persistent errors without treating access blocks or low content as irrelevance.
2. **Live gate not crossed.** Set `HARVESTER_DELIVERY_CHARACTERS=''` explicitly while still in shadow. Only after the queue and scheduled retries drain, atomically install the CI-passed release binary (`target/release/scoracle-cognition` into `bin/scoracle-cognition`), let the path unit restart into shadow, verify health, then set `HARVESTER_SHADOW_MODE=0` and restart. Do not reverse that sequence: an older binary with a live flag could deliver all characters.
3. **Bounded live canary pending.** Use `rust/examples/harvester_enqueue_live_canary.sql` for at most five source articles from run 333; verify migration-285 durable receipts with `rust/examples/harvester_live_canary_check.sql` after queue rows disappear, including a newer acquisition attempt, exact source bytes, four held assignments per classified edge, resolved identity and Graph receipts, and zero premature character products. Then release each character in bounded batches with `rust/examples/harvester_release_character.sql` and verify terminal dispositions and actual Journalist/Influencer/Insider/Scout product paths. Smol character behavior must be judged from those live receipts, not from the Laya shadow sweep.
4. **Non-editorial owner proof pending.** Enable and smoke the opt-in Scout, `statcommentary`, `vibesynth`, watchdog, and narrative-link Harvester readers; verify Graph/Investigator/fixture outcomes and Insider downstream obligations. Run `RESTORE_SOURCE_MODE=harvester scripts/hosting/restore-drill.sh` against the backup after timing is complete. Ratify watchdog's inherited 80% threshold against this full sweep.
5. **Legacy retirement pending.** Editor, packet, storyline, and stories consumers still exist. The frontend `scoracle-frontend` Stories board/detail route depends on backend story endpoints and must be removed in a coordinated release with backend API retirement; decide old deep-link behavior. Drain or explicitly migrate residual Editor work (the last audit found one historical five-attempt dead letter). Then remove legacy adapters, packet renderer, story routes/Swagger, worker stages, cron/SQL dependencies, memory/eval dependencies, and only later perform reviewed destructive table/trigger cleanup. The prior catalog audit found one packet trigger, `enqueue_voices_on_packet`, plus legacy functions listed in the implementation status.
6. **Quality/debug follow-up.** Some verbatim extracted openings contain navigation furniture or HTML entities, even though their bytes match the retained body. The 84-row AI annotations are provisional. Character-specific disagreement samples, especially all-four Laya recommendations and women's-team cases, need human review after the live path is stable. Review remaining `granite4.2:3b` fallback literals and historical docs during cleanup so Smol is consistently represented.

## Safe entry points for the fresh session

Run aggregate queries **on `archbox`**; keep publisher text and URLs on that host:

```sh
cd /home/sheneveld/scoracle/scoracle-backend
set -a; . ./.env.local; set +a
psql "$DATABASE_PRIVATE_URL" -X -f rust/examples/harvester_nightly_report.sql
psql "$DATABASE_PRIVATE_URL" -X -f rust/examples/harvester_cutover_readiness.sql
systemctl --user is-active scoracle-api scoracle-cognition scoracle-laya
```

Before any live canary, read the current [cutover status](harvester-cutover-status-2026-09-26.md), [canary enqueue](../examples/harvester_enqueue_live_canary.sql), [canary check](../examples/harvester_live_canary_check.sql), [release](../examples/harvester_release_character.sql), and [hold](../examples/harvester_hold_character.sql) scripts. The active `harvester-nightly-production-check` heartbeat monitors run 333 every 30 minutes and should stay active until the full cutover is verified. It should stay quiet while a healthy shadow sweep is incomplete and should not replace or restart the installed worker before the retry drain.
