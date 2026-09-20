# 2026-08-20 — The single-box consolidation: everything lands on the 1070

*(Decision: Scott, 2026-08-20 morning. Supersedes `PLAN-mistral-return.md`'s DECIDED
block after one day in service. Companion code commit: this one.)*

## The decision

All LLM junctions run on **archbox's 1070 Ti — ministral-3:3b on Ollama,
`localhost:11434`**. Editor, Investigator, Scout, Journalist, Influencer, Analyst,
Oracle, Graph, Insider: one pinned Apache 2.0 model on one machine Scott built in
2017, one self-sustaining, self-cleaning pipeline. The quality step down from the
Mac's 8b is **accepted, not gated** — plumbing over benchmarks has been the doctrine
since 08-09, and this is its final application: the discovery arc (seat gates, guards
over evals, ctx-buys-slots, the directing doctrine) was the point, and it all funnels
into the simplest possible production shape.

The Mac mini leaves production entirely. It keeps Ollama and its models and becomes
the standalone LLM/agent machine — local agent work and 1B training live there — but
no `*.scoracle.com` production path touches it.

In Scott's words: *"There isn't a more 'Scotty' setup than this. Self served, on a
self built machine, on an Apache 2.0 model. It's time to let this go. To let it fly."*

## What actually changed (it was smaller than it looked)

The 08-19 MLX cutover had **already been rolled back** by the evening (the ~4k-prompt
crash boundary; `com.scoracle.mlxlm` disabled 20:28, Mac Ollama re-activated) — so at
decision time the four voices were routing to the Mac's *Ollama* with grammar already
back on the wire. The consolidation was therefore an env flip plus code cleanup, not a
wire-format migration:

- **Routes** (`.env.local` on archbox): the four voice roles drop their
  `_BASE_URL`/`_BACKEND` keys and pin `ministral-3:3b`; every role now rides the
  default `localhost:11434`. `COGNITION_BACKEND_CONCURRENCY` loses the Mac entry.
- **Slot groups** (`rust/src/stage.rs`): `MAC_MLX_SLOTS` deleted;
  `ARCHBOX_GEMMA_SLOTS` renamed to `ARCHBOX_SLOTS` (the gemma3 era is long over) and
  resized **6 → 4**. Editor/graph expand to the full group; the four voices cap at 2
  within it so one long decode can't take the whole card from the Editor.
- **`Backend::OpenAi` seam** (`openai.rs`): kept, marked currently-unused — the
  measured oMLX/MLX notes stay as the plug point for any future OpenAI-compatible
  host.
- **n21 + the 900-token packet reservation: kept.** They were compact-wire scar
  tissue from the grammarless MLX path, but an n-bump forces a full regen per
  news-active entity, so reverting is deliberately NOT coupled to this cutover.
  Optional later (bundle with the output-simplification pass).
- **No THINK or reservation arithmetic changes**: `_THINK` defaults to omitted;
  all output reservations key on the 4096 window, not the host.

## The card-preservation stack (why 4 slots, why no duty cycle)

Scott's constraint: this is a 2017 card carrying every seat — don't burn it out.
The protection is three layers, none of them a clock:

1. **135W power cap** (75% of the 180W default) — already in place and persistent
   (`gpu-power-cap.service`, enabled). Bounds heat no matter how many slots decode.
2. **4 parallel slots**, down from the 6 the VRAM allows (~7.2 GiB ceiling stands;
   at 4-way expect ~6 GiB). Three knobs moved together: `ARCHBOX_SLOTS`,
   `COGNITION_BACKEND_CONCURRENCY`'s localhost entry, `OLLAMA_NUM_PARALLEL`.
3. **Work-driven operation — the duty cycle is RETIRED.** The 1-on/1-off (and the
   2-on/1-off it was about to become) maximized the thing that actually ages a GPU:
   thermal cycling, eight heat/cool transitions a day against Pascal's known
   solder-fatigue failure mode. Steady capped load at ~60-70°C is the benign
   mining-rig regime. Run-until-done gives ONE warm-up per work burst, then true
   cold idle (9.5W measured). The `scoracle-cognition-{pause,resume}` timers are
   deleted, not rescheduled; the watchdog un-pins from on-hours (its `drain_alive`
   check only fires when claimable work exists but nothing was produced — a running
   daemon with an empty queue never trips it).

**Verify on the first full drain:** sustained GPU temp <75°C across a multi-hour
block (`nvidia-smi --query-gpu=temperature.gpu`), `ollama ps` says 100% GPU
(placement — the D-T35 spill check), VRAM steady ≈6 GiB.

## What consolidation buys beyond simplicity

- **Grammar for every seat.** Narratives and sigil declare `format_schema` that the
  MLX wire silently dropped; on Ollama it is enforced. Every structured contract in
  the pipeline now rides real grammar or its guards — the directing doctrine's tier
  order, uniformly applied.
- **The throughput measurement gets honest.** With run-until-done, "hours from
  ingest to empty queues" IS the daily-clear number — the 4-5h goal is now read
  directly off the queue, not extrapolated across duty-cycle windows.
- **One box to watch.** The freshness watchdog, the wedge watchdog, and
  `pipeline_runs` all describe one machine. mlxhealth and its LaunchAgent are gone.

## Rollback (env-only, no code revert)

The Mac's Ollama stays installed with the 8b pulled. If the 3b proves unable to
carry a voice: re-add that role's
`_BASE_URL="http://192.168.1.77:11434"` + model `ministral-3:8b`, re-add
`,http://192.168.1.77:11434=4` to `COGNITION_BACKEND_CONCURRENCY`, restart the
daemon. The voices would share `ARCHBOX_SLOTS` while routing remotely —
throughput-conservative but functional.

## The follow-up chapter (deliberately not in this pass)

1. **ctx ↓ toward 2048 per junction** — outputs fit a tarot-sized card; smaller
   ctx buys KV headroom (audit real prompt sizes first: on Ollama an oversized
   prompt silently truncates, the D-T35 class).
2. **Output simplification** — trim the multi-output extras; clean, simple story
   updates only.
3. Optional n22: revert compact-wire + 900→700 (full regen; bundle with #2).
4. **The 1B model → the mobile app.** The reason we're landing this plane.
