# HANDOFF — The Voices Session (opened 2026-08-09, ep6/Investigator session closing)

Scott's direction, verbatim: *"Let's move into the voices in a fresh window… start the handoff
with shutting down production on the Mac and switching from Ollama to oMLX for that machine.
oMLX gives us huge gains in concurrent speed, and we may be able to crank up the concurrent
requests now that we have the Editor down to 4096 and because everything downstream will have a
4096 window max."*

Read `PLAN-one-rail.md` STATE + `PLAN-character-tuning.md` §D-T41 (the oMLX research),
§D-T44–46 (the method that worked) before anything else.

---

## 0 · TO-DOS CARRIED FROM THE LAST SESSION (do not lose these)

1. ⛔ **archbox Ollama is still at `OLLAMA_NUM_PARALLEL=4`; code and client already sit at 6.**
   Scott runs this in a real terminal (the `!` prefix could not service sudo; the classifier
   rightly refused Claude the password path):
   ```
   ssh sheneveld@archbox
   sudo sed -i 's/OLLAMA_NUM_PARALLEL=4/OLLAMA_NUM_PARALLEL=6/' /etc/systemd/system/ollama.service.d/concurrency.conf
   sudo systemctl daemon-reload && sudo systemctl restart ollama
   ```
   Then verify: `ollama ps` must say **100% GPU** after a generate (≈7.2 GiB expected; 8 slots
   would spill to CPU — do not go past 6). The drop-in's comment block still describes the
   gemma era; rewrite it while in there.
2. ⛔ **Rotate the archbox password** (`passwd` in that same SSH session) — it was pasted into
   the previous session's transcript three times.
3. Optional, recommended: a scoped sudoers rule so service ops never need the password again:
   `sheneveld ALL=(root) NOPASSWD: /usr/bin/systemctl daemon-reload, /usr/bin/systemctl restart ollama, /usr/bin/systemctl stop ollama, /usr/bin/systemctl start ollama`
   → `/etc/sudoers.d/50-scoracle-ollama` (validate with `visudo -cf` before installing).

---

## 1 · PHASE ZERO OF THIS SESSION: THE MAC MOVES FROM OLLAMA TO oMLX

**Why (D-T41, researched + Scott's call):** oMLX is a macOS inference server (menu-bar app +
headless, Apache-2.0, `github.com/jundot/omlx`) with **`--max-concurrent-requests` (default 8)
and CONTINUOUS BATCHING** — the D-T34 finding (MLX 2.13× at 4 concurrent, "still scaling")
made native — plus **paged SSD KV caching** (KV blocks persist and restore on recurring
prefixes; every voice sends a FIXED system prompt, which is exactly that case — unmeasured,
measure before booking). And it kills a whole failure class: Ollama reloads the model when a
request asks for a different `num_ctx`; oMLX has no per-request context reload at all, so the
D-T35 silent-eviction class may simply not exist on it.

**Why now:** every consumer of the Mac is at a 4096 ceiling — the Editor went 8192 → 4096 last
session, the voices' `VOICE_NUM_CTX` is pinned 4096 (boot log: "every voice on this host
requests num_ctx 4096"), and the Investigator's `ip1` prose reads are 4096. Small uniform
windows are what let concurrency climb.

**The order of operations:**
1. `systemctl --user stop scoracle-cognition` on archbox (production pause for the swap — the
   3b/archbox side is untouched throughout).
2. Mac: stop/disable the Ollama the voices use today. Install oMLX **with the grammar option —
   this is load-bearing**: `brew tap jundot/omlx https://github.com/jundot/omlx && brew install
   omlx --with-grammar` (xgrammar, ~2GB). ⛔ Without it FOUR seats lose their contract:
   narratives (`format_schema`), sigil (`format_schema`), transfers (`json_mode`), and now the
   Investigator's `ip1` (`format_schema_raw`, added last session). macOS 15+, Python 3.11–3.13.
   `omlx serve` on `:8000`; `brew services` runs it headless.
3. **The client is real work, not a URL swap:** oMLX speaks OpenAI (`/v1/chat/completions`,
   `/v1/completions`) and Anthropic (`/v1/messages`) — NOT Ollama's `/api/generate`. The crate
   has `src/openai.rs`; the route layer (`route.rs`, `COGNITION_BACKEND_CONCURRENCY`,
   `Inference` trait) needs a per-host backend kind and a mapping for the schema contract
   (`format_schema_raw` → oMLX's structured-output param; verify ORDER-TRUE emission survives —
   the ar4/ep1 lesson — with a probe before trusting it). Model naming/pull on oMLX differs
   (HuggingFace models, not ollama tags): pick the MLX build of the 14B, pin it in
   `COGNITION_ROUTE_*`.
4. Probe before production: the D-T41 rule — **oMLX structured output either enforces the
   grammar or errors; there is no quiet middle** — makes a 3-fixture probe per schema-carrying
   seat cheap and decisive. `eval --task investigator --fixtures` (8 checks, frozen) and
   `--task editor --fixtures` still point wherever `COGNITION_ROUTE_*` says — run the
   investigator set against oMLX as the first grammar smoke.
5. Concurrency: client budget for the Mac is 3 today (`COGNITION_BACKEND_CONCURRENCY`,
   `.env.local`). oMLX defaults to 8. Raise the client stepwise (3 → 6 → 8) reading tok/s and
   TTFT — D-T34's curve says the win is real but measure where it flattens. Then restart the
   archbox daemon and watch the voice backlogs drain (narratives ~3.4k, vibe ~3k pending as of
   last look).

---

## 2 · THEN: THE VOICES THEMSELVES

The method that worked twice last session — written into D-T45/D-T46, apply it per voice:
1. **Trace the prompt against its consumers before editing** (the ep6 field-by-field table).
2. **Author gate checks for every field being tuned BEFORE tuning** — three prompt variants
   drifted `story_type` invisibly because no fixture asserted it. Fixture sets exist for
   editor (12), investigator (3); the voices have some (`fixtures/<task>/`) — audit coverage
   first.
3. **Probe live before coding; probe again after.** The gate caught 2 real code bugs and the
   claims-lag finding before any deploy.
4. **Schema first, prose second, worked example third** — enums/bounds are token-free; for
   small models a worked example carries what prose cannot (the ep6 58→59 lesson).
5. One contract version bump per voice, both plan files, same commit.

The cast and their contracts (registry: `eval_tasks.rs::lens_parameters`): The Journalist
(narratives), The Insider (transfer), The Influencer (vibe), The Scout (rating), The Analyst
(momentum), the Oracle (oracle/sigil). All ride the Mac's 14B → everything above about oMLX is
their runtime. `momentum` carries 358 failed work rows, `narratives` 22 — diagnose those while
draining.

**After the voices: Appendix S (the schema inbox), per the standing order.**

---

## 3 · STANDING STATE (as the last session closed)

* Production RUNS (unpaused 2026-08-09): archbox daemon on the greenfield rail, Editor at
  `ep6` (59/60 gate), first real ep6 reading owed off the next 02:00 drain (register rate,
  `unknown` rate, story_type mix — D-T45 closes with the list).
* Investigator: prose arm `ip1` live (verbatim-quote contract, containment-verified), owner
  class live-verified (Jerry Jones: candidate 548 rejected by the old code Aug 7, accepted
  20s after re-queue — persons 959, `owner_of` → Cowboys), enrichment corroborates stale
  Wikidata claims against page prose, `scripts/investigate.sh` queues names/metadata/status.
  Recovery-rate reading of the corroborated NBA re-run may still be owed — check
  `PLAN-character-tuning.md` §D-T46's owed list and `investigate.sh status`.
* Headshots: NBA CDN preferred, P18 Commons for everyone else (NFL's only source). Backfill
  runs via `investigate.sh enrich <SPORT> <n>` whenever wanted; 2s Wikimedia spacing governs.
