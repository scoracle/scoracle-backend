# HANDOFF — The Insider session (written 2026-08-10, the Influencer/Scout/Analyst session closing)

Read `PLAN-one-rail.md` STATE (2026-08-10) + `PLAN-character-tuning.md` §D-T50/51/52 (the three
register passes — the method now five-times proven, and TWICE burned by its own shortcuts this
session) before anything else.

**STANDING STATE:** Production LIVE @ `71dbfdb`: archbox 1070 Ti / ollama / pinned
`ministral-3:3b` (Editor + utility + Investigator, NUM_PARALLEL=6); Mac / oMLX /
`ministral-3-14b` (six characters, grammar suppressed per D-T47). ⛔ **Mac client concurrency is
2** (was 6 — the 08-10 thrash incident, STATE ⛔ block; 6 and 4 both fail the 16GB memory
arithmetic at real prompt sizes). Duty-cycle timers: drain pauses at 00/03/06/09/12/15/18/21:00,
resumes one hour later — **the rest hours are the eval windows.** All 1,985 failed Mac-stage rows
requeued post-deploy and draining. Voices tuned so far: Journalist n18, Influencer v17, Scout
s17, Analyst s14 (registers per Scott, quoted verbatim in the D-Ts).

## 1 · THE INSIDER (transfers) — next voice

* The last un-refreshed character besides the Oracle. json_mode seat (D-T47 note: transfers ride
  `json_mode` with grammar OFF — the prompt and parser carry the whole contract; audit the
  prompt for enum/shape guidance the D-T43 era deleted, same as the other passes).
* The method, with this session's two additions: (a) fixture checks can themselves be wrong —
  probe BEFORE trusting a new axis (the "surge"/"not falling" honest-negation false-positives,
  D-T50/52); (b) readings on a busy GPU are the GPU's numbers — daemon stopped, rest window,
  and take TWO consecutive runs (the 79-vs-74 lesson, D-T52).
* Ask Scott for the register brief first — his verbatim answers drove all four passes.

## 2 · THE ORACLE after that, then Scott's parked directive

1. **Oracle (sigil)** — the crown voice, last in the sweep.
2. **The concurrency/throughput deep-dive (Scott, verbatim):** *"we're doing something wrong
   with concurrency. Ollama was getting through 3 concurrent requests (may have been silently
   failing though) and oMLX tested out much more efficient at concurrency. The 8.5 tokens/second
   is way off what we tested as well. We need to dive into that."* Inputs to that dive: D-T34
   measured 2.13× at 4 concurrent on a FRESH server with no drain; today's failures came with
   ~3GB pool/prefix-cache bloat + other-app RAM pressure shrinking the dynamic ceiling + 6-7k
   token prompts. The diet (D-T52) has since cut momentum's worst case under ~5k. Instrument:
   `grep "Chat completion" ~/.omlx/logs/server.log` (tok/s, prompt size), `cached_tokens` in
   responses (the D-T41 KV question), `vm_stat` for wired pages. Any concurrency re-raise past
   2 needs a measured saturated-window reading.

## 3 · CARRIED / OWED (unchanged from the last handoff where not struck)

1. **Frontend entity sync (design done, build owed):** persons reach no frontend surface;
   Fix A (person_rows CTE arm in `universalEntitiesStatement`, `go/internal/db/db.go:21-146`) +
   Fix B (computed max version for the ETag); guard test + ENDPOINTS.md. Fix C was rejected.
2. **ep6 production reading owed** (register rate, `unknown` rate, story_type mix) — the drain
   since requeue is accumulating the sample.
3. **Upstream bug report to `jundot/omlx`** — the tekken grammar corruption, 3B repro in D-T47.
   Scott's call on filing.
4. **Requeue-verification reading:** the 320-row markdown-prose momentum class should NOT recur
   at s14 (two worked examples pin the READ shape). Check
   `SELECT count(*) FROM pipeline_work WHERE stage='momentum' AND status='failed' AND last_error
   LIKE 'momentum: invalid response%'` after a drain day; a recurrence is a finding.
5. **oMLX ops notes:** `brew services restart omlx` can leave the job dead — `launchctl bootout
   gui/501/homebrew.mxcl.omlx` + `bootstrap` + `kickstart` is the recovery; server RSS grows
   ~3GB over a long run (pool/prefix-cache bloat) and a restart in a rest window clears it.

## 4 · THE METHOD (now five voices deep)

Trace prompt vs consumers → author gate checks for every tuned field BEFORE tuning (and PROBE the
new checks — a check can be wrong) → baseline on a QUIET server, two runs → register edit per
Scott's brief, schema first, prose second, worked example third (one example per REGIME the seat
must voice — the Analyst needed rising AND steady) → one contract bump per voice, both plan
files, same commit → deploy BEFORE requeue. A field the gate cannot see is a field a prompt edit
can quietly break — and a reading the drain can touch is a reading of the drain, not the model.
