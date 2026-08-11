# HANDOFF — The Analyst session (written 2026-08-10, the flip/Journalist session closing)

Read `PLAN-one-rail.md` STATE + `PLAN-character-tuning.md` §D-T47 (grammar OFF on oMLX — why),
§D-T48/§D-T49 (the Journalist's n17/n18 — the method, twice more proven) before anything else.

**STANDING STATE:** Production LIVE on the new topology @ `6205aca`: archbox 1070 Ti / ollama /
pinned `ministral-3:3b` = Editor + utility + Investigator (NUM_PARALLEL=6, verified 100% GPU);
Mac / oMLX / `ministral-3-14b` = the six characters, **grammar suppressed** (D-T47 — oMLX's
xgrammar corrupts tekken output; contracts ride the fail-closed parsers). Client concurrency 6
on both hosts. Journalist at n18 (beat-writer register, publications cited in prose, 110/110).
Backlogs draining: narratives ~2k, vibe ~2.8k, momentum ~2k at last look. article_read backfill
(30,224 regex-contaminated rows) pruned on Scott's order. Resume/pause timers re-armed.

## 1 · THE ANALYST (momentum) — next voice, and it carries real debt

* **399 failed rows, all one class:** `momentum: invalid response (raw="**Chelsea's...` — the
  model writes MARKDOWN PROSE instead of the contract. Pre-flip vintage (last seen 08-09), the
  seat never had grammar. The D-T45/48/49 method applies: trace consumers → author gate checks
  for every field FIRST (audit `fixtures/momentum/` coverage) → probe → schema-shape pinning in
  the prompt (worked example; the ep6 lesson) → bump → requeue the 399 + the 1 stale-net row.
* **3 NEW `omlx-prefill-abort` rows (08-10):** momentum prompts can exceed the Mac's prefill
  guard (~6,835 tok aggressive). The Analyst needs the prompt DIET as well as the shape pass —
  measure its built prompts from `cognition_ledger` before editing.
* Scott's frame for the seat (README lens table): nimble trader, PEAK trajectory as price
  action, Vibe/news as sentiment. Ask Scott for the register refresh like the Journalist got.

## 2 · CARRIED / OWED

1. **Frontend entity sync (Scott's ask, DESIGN DONE, build owed):** the Investigator writes
   `public.persons`; `/api/v1/entities` (the frontend local-DB source,
   `go/internal/db/db.go:21-146` `universalEntitiesStatement`) unions ONLY players+teams — new
   persons reach NO frontend surface. Fix A: add a `person_rows` CTE arm (`type='person'`,
   filter `sport IS NOT NULL`, key on (type,id) — persons/players sequences overlap). Fix B:
   replace `'generated_at', NOW()` with a computed max version so the ETag goes stable and
   frontends detect change cheaply (persons has only `created_at`; players/teams `updated_at`
   churn from nightly jobs — accept, or add persons.updated_at later). Update the guard test
   `go/internal/db/entities_test.go` + `ENDPOINTS.md`. Fix C (meta-table trigger) was examined
   and REJECTED: `sport_autofill_versions` binds the legacy matviews, refresh can't run in the
   accept txn, and persons.sport is nullable.
2. **Concurrency-6 throughput reading owed:** per-stream 8.2 tok/s @6 vs 9.6 @3 (expected dip);
   the AGGREGATE reading needs a saturated window — take it during a heavy drain hour, then
   decide 6→8. Instrument: `grep "Chat completion" ~/.omlx/logs/server.log` (tok/s + prompt
   size per line; `cached_tokens` in responses for the KV-cache question D-T41 left open).
3. **Upstream bug report to `jundot/omlx`:** the tekken grammar corruption — the 3B one-liner
   repro is in D-T47. Scott's call on filing (external action).
4. **ep6 production reading still owed** (register rate, `unknown` rate, story_type mix at the
   real article mix) — D-T45's list; the drain since unpause is accumulating the sample.
5. **Voice sweep order after the Analyst:** Insider (transfers, json_mode seat), Influencer
   (vibe, ~2.8k backlog), Scout, Oracle — each per the method; audit `fixtures/<task>/`
   coverage FIRST every time.
6. Mac sudo items are DONE (ollama LaunchDaemon out, archbox NUM_PARALLEL=6, password rotated).
   `_reference:` memory `pending-ops-archbox` — prune completed items when convenient.

## 3 · THE METHOD (unchanged, thrice-proven)

Trace prompt vs consumers → author gate checks for every tuned field BEFORE tuning → probe live
before coding, again after → schema first, prose second, worked example third → one contract
bump per voice, both plan files, same commit. A field the gate cannot see is a field a prompt
edit can quietly break.
