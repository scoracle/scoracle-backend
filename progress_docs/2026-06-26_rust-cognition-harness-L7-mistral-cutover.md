# L7 — One model, not two: the Mistral 7B cutover (off the oversized local model)

**Date:** 2026-06-26 · **Plan:** `scoracleWiki/wiki/Plan - Rust Cognition Harness build.md` §7 (L7)
**Supersedes:** the L7 two-model "batch-by-model scheduler" premise — see "The pivot" below.

## Goal & outcome

L7 set out to run two role-specialized models (Mistral news + local model stats) on the 1070 Ti with a
batch-by-model scheduler. Reconnaissance + a measured bake-off **collapsed that premise into something
simpler and bigger**: replace the oversized multimodal `local-model:tag` with a single fitting text model,
**`mistral:7b`**, for *everything*. Cutover is **live on archbox** (~19 tok/s, ~2× the old local model).

## The reconnaissance that changed the plan

- The "2025 local model stats backlog" the handoff targeted is **already computed** (rating/sigil/stat_summaries
  populated for 2025; 0–4 pending). The live `pipeline_work` backlog is **~99% news-role**
  (vibe 169 / narratives 173 / transfers 135 ≈ 477), not stats.
- `local-model:tag` is an **8B multimodal** model (Q4_K_M, carries vision+audio weights we never use). `ollama ps`
  shows it loaded at **10 GB running 67% on the CPU** — two-thirds off the GPU. *That* is the throughput problem.

## The bake-off (5 models, same 15 vibe entities; fit + prose, contention-proof axes)

| Model | Fit (8 GB) | Prose |
|---|---|---|
| local-model:tag (old) | 10 GB → **67% CPU** | verbose (500–800 tok, ignores "two lines"), drama-skewed, sometimes wrong ("Mayfield = trade chip") |
| small local model | 2.6 GB → 100% GPU | formulaic, narrow scores, **over-commits** ("Giannis joins Miami" as fact) |
| **mistral:7b** | **5.2 GB → 92% GPU** | concise, specific, instruction-compliant — **the pick** |
| larger local model | 8.1 GB → slight spill | most measured/nuanced; borderline fit |
| nemo:12b | 8.2 GB → 42% CPU | richest prose; but spills (reintroduces the problem) |

Tooling: extended `bin/eval` to capture **prose + per-call throughput + optional labels** (label-free
quality A/B) and to skip failed calls under contention. (Throughput numbers in the bake-off were poisoned
by GPU contention; fit + the live post-cutover rate are the clean signals.)

## The pivot — one model, not two (the real insight, user-led)

The two-model scheduler was solving a split the architecture already erased: **all deterministic math lives
in Postgres** (composite/T-score/percentiles/`compute_transfer_heat`), so the model *never computes* — its
only job is to **verbalize/judge over precomputed signals**. "What is this entity strong at" (rating) and
"how does it feel right now" (vibe) are the *same kind of task*: prose over signals. So **one strong-prose
model handles every role** — no swaps, no scheduler, no batch-by-model. The L7 "unlock" (scheduling two
co-resident-impossible models) is **moot**.

Bonus properties that sealed it: Mistral is **multilingual** (the §1.5 Multilang capability, for free) and
**follows instructions** (local model ignored format constraints; the defensive prompt "clamps" were local model-tax).

## The cutover

- `OLLAMA_MODEL=mistral:7b` set in **archbox `.env.local`** (the Go worker derives *everything* on Mistral).
  Worker restarted; confirmed `model=mistral:7b` in the logs + HTTP 200.
- **`local-model:tag` removed** (`ollama rm`) — this both streamlines the stack AND clears the wedged-in-VRAM
  model that had been forcing Mistral onto the CPU. Bake-off scratch models also removed.
- **Result: ~19 tok/s** (vs local model's 9.9 clean baseline; ~2×), Mistral sole-resident at 92% GPU.

## Loose ends (carry)

- **2 worker instances** are running — a relaunch vector *outside* systemd (not the `.path` watcher, not the
  crontab fixture jobs) that wasn't pinned this session. Benign (`pipeline_work` `SKIP LOCKED` prevents
  double-derivation) but likely caps throughput via Ollama's single-slot queue; pinning it should buy a
  further speed bump.
- **Cross-machine durability:** the cutover is an archbox `.env.local` override (gitignored, machine-local).
  To make Mistral the committed default, pull `mistral:7b` on **archx220** first, then flip the committed
  `.env` / Go config default. Not done here to avoid breaking archx220 on its next pull.
- **Streamlined stack (user-confirmed):** the only LLM is `mistral:7b` (Ollama / GPU diviner); the only
  other model is **BGE-small** (candle / CPU) — the front-of-funnel scrub *sieve*. `model:latest`, the last
  leftover, was removed. *The sieve surfaces, the diviner judges* — now drawn on silicon: CPU embeddings
  filter the funnel; Mistral adjudicates. No second LLM anywhere.
- The Rust **scrub cutover** stays HELD (unchanged). The **two-model scheduler is shelved** (superseded).

## Files

- **changed (committed):** `rust/src/bin/eval.rs` — prose + per-call throughput + optional-label capture;
  resilient (skip-on-error) generate for contention runs.
- **config (local, not committed):** archbox `.env.local` `OLLAMA_MODEL=mistral:7b`.

## Gate

`cargo clippy --all-targets -- -D warnings` clean · `cargo test --lib` 35/35 (1 ignored) · cutover verified
live (worker on `mistral:7b`, HTTP 200, ~19 tok/s, Mistral sole-resident 92% GPU).

## Next session — prompt tuning on a compliant model

Mistral follows instructions, so the prompts can shed their local model-era **defensive clamps** (rigid format
demands, repetition, "don't invent drama" guardrails — all written to wrangle a model that wouldn't listen).
Next session: tune the prompt + desired output per product (vibe / narratives / rating / sigil / transfers)
for a model that obeys format and handles richer, more nuanced instructions. Handoff prompt in the plan's §7
ledger.
