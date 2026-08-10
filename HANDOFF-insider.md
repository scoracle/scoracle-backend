# HANDOFF — The Insider session (written 2026-08-10, the Influencer/Scout/Analyst session closing)

Read `PLAN-one-rail.md` STATE (2026-08-10) + `PLAN-character-tuning.md` §D-T50/51/52 (the three
register passes — the method now five-times proven, and TWICE burned by its own shortcuts this
session) before anything else.

**STANDING STATE (updated again after D-T56, 2026-08-10 ~17:00 — SUPERSEDES the block below):**
⭐⭐ **Production is `ministral-3-8b` on oMLX at client concurrency 4, guard ceiling 11.2GB**
(D-T56 + its sustained correction: burst cells read 255/h @6 with 30/30, but SUSTAINED drain is
~180 req/h at 3, 4, or 6 alike with ~1/min fat-tail guard retries — the census tail, not
concurrency, is the binding constraint, so 4 ships and THE DIETS ARE THE UNLOCK; quality
cleared at D-T55: 490/508, the 8B WINS the crown and the vibe). Mac: LaunchAgent
`com.scoracle.omlx` (the brew record is flaky — bootstrap error 5 — and stopped;
`com.scoracle.ollama` plist kept as fallback at NUM_PARALLEL=4, model `ministral-3:8b` already
pulled); oMLX settings: max_concurrent 6, guard custom 10.5GB, chunked prefill, **cache off**
(re-testing the prefix cache is now worthwhile — memory finally breathes). Archbox routes:
six character seats → `ministral-3-8b@http://192.168.1.77:8000` backend omlx, map `:8000=6`
(backup `.env.local.bak-20260810-pre8b`). **THE VOICE QUEUE (all on the 8B now):** (1) 8B
mini-tunes: rating plain-text prominence (it bolds labels), narratives card budget (one
52-sentence edition); (2) Scott's CARD-SURFACE brief into every voice (output = one tarot
card; Journalist/Insider budgets are per-CARD across multiple entries); (3) the Insider pass +
the HOT/COLD verdict (contract addition — gate first); (4) the Oracle pass (single-peer rule —
the 14B broke it 4×, the 8B once). Ask Scott for Insider/Oracle register briefs. Also owed:
`contains_ci` diacritic folding (Sørensen), and the D-T54 diets as optimization.

*(The 13:00 block below is superseded by the above.)*
**STANDING STATE (updated after the D-T53 flip, 2026-08-10 ~13:00):** Production LIVE @
`71dbfdb`: archbox 1070 Ti / ollama / pinned `ministral-3:3b` (Editor + utility + Investigator,
NUM_PARALLEL=6); ⭐ **Mac is BACK ON OLLAMA** (D-T53's A/B: ollama 30/30 at 165 req/h vs oMLX
failing a third of the workload) — LaunchAgent `com.scoracle.ollama` (`OLLAMA_HOST=0.0.0.0:11434`,
`NUM_PARALLEL=2`, flash-attn, `q8_0` KV, keep_alive 60m), `ministral-3:14b`, client concurrency
**3** (the measured best: `-np 4`/client-4 re-measured as a REGRESSION, 134 req/h — D-T30
repeats). Routes flipped in archbox `.env.local` (backup `.env.local.bak-20260810-preflipback`);
`_BACKEND` lines removed (ollama is the default). oMLX is stopped but installed
(`brew services`), with stale `launchctl setenv` OLLAMA_* leftovers now cleared. Grammar stays
OFF everywhere — the voices are validated grammarless. Duty-cycle timers: drain pauses at
00/03/06/09/…, resumes one hour later — **the rest hours are the eval windows.** All 1,985
failed Mac-stage rows requeued and draining. Voices tuned: Journalist n18, Influencer v17,
Scout s17, Analyst s14 (registers per Scott, verbatim in the D-Ts; v17/s17/s14 gates re-read
GREEN on ollama, narratives checked at flip time).

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
