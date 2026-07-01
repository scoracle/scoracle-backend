# L6 — Scrub as a `pipeline_work` stage (built + validated) → the GPU-throughput pivot

**Date:** 2026-06-25 · **Plan:** `scoracleWiki/wiki/Plan - Rust Cognition Harness build.md` §8 + §7 (L6)
**Builds on:** L5 (the at-scale shadow) + the §8 conception (the asymmetric gate + option (i) wiring).

## Goal & outcome

L6 = **option (i)** of the live-scrub flip: scrub becomes a first-class `pipeline_work` stage with a Rust
handler and the **asymmetric gate**. The foundation is **built (Phase A, committed `ae346cd`)** and
**validated live** (canary + ramp). But the ramp surfaced the decisive fact: **the pipeline is GPU-bound
and the derive backlog is growing.** So the steady-state flip is **HELD**, and — per the user's steer
(2026-06-25) — the build **pivots to throughput/efficiency on the 1070 Ti**: get the 2025 local model-derived
products computed, then keep the news side current until the 2026 seasons start.

## Accomplishments (Phase A — all committed `ae346cd`, inert until flipped)

- **Asymmetric gate** (`resolve.rs`): `classify` drops its auto-DROP arm — the cheap cosine fast-tracks
  keeps but **never excludes**; only the model (or its fail-closed non-commitment) drops. Settled by the
  L5 redundancy finding (0/9 auto-dropped stories were captured elsewhere). *The sieve surfaces; the
  diviner judges; only the diviner excludes.*
- **mig 109**: `pipeline_work.entity_type` admits `'article'` (scrub is article-keyed; entity_id stays
  INTEGER — max article id ~90k).
- **`Stage::Scrub` + `ScrubHandler`** (`scrub.rs`): ports `news_scrub.go` — force-keep the primary, run
  the asymmetric `resolve_set` on secondaries, write `vetted` → fires the mig-103 trigger. `EntityType`
  gained `as_str`/`from_db_str` + `Hash`.
- **`main.rs`**: env-driven handler registration (`COGNITION_STAGES`); the embedder loads **only** for
  the scrub path. Run scrub-only so the service never double-claims vibe/sigil (the Go Drainer has no
  scrub handler → no collision).
- **Go**: `StageScrub` + a **flag-gated** (`NEWS_SCRUB_VIA_QUEUE`, default off) enqueue path in the
  maintenance scrub ticker.
- **CI fix** (`f9bed88`): recorded migrations 107–109 in `sql/schema/schema_migrations.txt` (the snapshot
  lineage was stale at 106).

## Validation (live, this session)

- **Canary (5 articles):** the full path — claim → asymmetric gate → write `vetted` → **mig-103 cascade
  fired (narratives + vibe enqueued)** → complete. The whole (i) integration works end-to-end.
- **Ramp (30 articles, bounded 10 min, under real GPU contention):** **24 drained cleanly, 0 failures**;
  the live gate kept **49 / dropped 7** secondaries (87.5% keep — matches the offline-validated ~85%
  genuine rate); ~2.4 articles/min.

## The pivotal finding — the bottleneck is the GPU, and it's behind

The **derive backlog GREW during the ramp: 146 → 192** (narratives 98 / vibe 61 / transfers 33). That
backlog **pre-existed** the scrub work (the canary added only 6) — the news/derive pipeline is **already
GPU-bound and falling behind.** The GTX 1070 Ti (8 GB, Pascal) forces `OLLAMA_MAX_CONCURRENT=1` (no
request parallelism), and scrubbing *adds* derive work faster than the single GPU clears it. So the
asymmetric gate's ~50% scrub saving is real but **derive (esp. narratives) is the heavier local model
consumer**, and **blitzing the 31k unscrubbed backlog would flood it.**

(An RTX A5000 would ~5–10× this — ~3× bandwidth, tensor-core prefill, and 24 GB for concurrency or a
27–31B model via one router config line — but the build targets the 1070 *now*.)

## Decisions / the steer

- **Scrub cutover is HELD** (flag off). Flipping `NEWS_SCRUB_VIA_QUEUE` would make the pipeline depend on
  a **reliable always-up Rust daemon** (pipeline stalls if it dies) — needs systemd-class supervision,
  not a session — *and* the GPU shouldn't take added steady-state load while the derive backlog is
  underwater. Go's in-process inline scrub handles steady-state news fine. Use the Rust `scrub` stage
  only for **bounded** backlog drains when the GPU is idle.
- **NEW PRIORITY (user steer, 2026-06-25): efficient 2025-derive throughput on the 1070.** Use Rust to
  make the available models (**local modelb + Mistral 7b**, the latter not yet implemented) handle their
  roles **more efficiently** — fewer/shorter local model calls, the right model per role (eval-gated), and
  **batch-by-model scheduling** to amortize the single-GPU model-swap cost. Then keep up with the news
  side until the 2026 seasons kick off. See the L7 handoff (plan §7).

## Quick reference (the cutover, when resumed)

```bash
# steady-state flip (needs a supervised daemon): NEWS_SCRUB_VIA_QUEUE=true + restart Go API,
# then run the drainer supervised:  COGNITION_STAGES=scrub ./bin/scoracle-cognition
# bounded backlog drain (no daemon): enqueue scrub items, then a bounded run:
#   timeout 600 env COGNITION_STAGES=scrub ./rust/target/release/scoracle-cognition
# rollback: NEWS_SCRUB_VIA_QUEUE off + restart + stop the service.
```

## Files

- **new:** `sql/migrations/109_pipeline_work_article_stage.sql`, `rust/src/scrub.rs`.
- **changed:** `rust/src/{resolve,harness,work,main,lib}.rs`; `go/internal/{work,config,maintenance}`,
  `go/cmd/api/main.go`; `sql/schema/schema_migrations.txt` (CI).

## Gate

`cargo clippy --all-targets -D warnings` clean · `cargo test --lib` 35+1 · `go build/vet/gofmt` clean ·
live canary + 30-article ramp validated (0 failures; gate 49 keep / 7 drop = 87.5%, matching offline).

## Not done — L7 = two specialized models on one 1070 (see the L7 handoff)

The headline unlock: run **Mistral 7B (news/emotion) + local modelB (stats/math)** on the single 1070 via
**sequential residence + batch-by-model scheduling** — they can't co-reside in 8 GB, so the harness keeps
one model hot and drains all its work before swapping once (Ollama would thrash per-request otherwise).
The cheap-first sequence:
1. **Eval Mistral-as-news offline** (`bin/eval` + the built `vibe` loaders) — measure the quality win
   justifies the swap before committing; adopt on the result (models by role, eval-gated).
2. **Model-affinity scheduler** (`worker.rs`) — drain grouped by routed model; two-pass per entity-batch
   (news-pass [Mistral] → one swap → stats-pass [local model], since sigil/momentum read vibe). The actual unlock.
3. **Port `narratives`** as `embed+cluster + route + extract + persist` — the biggest Mistral-news consumer
   + `cluster()` dedup → shorter prompts on the heaviest GPU stage → the 2025 narrative backlog drains cheaper.

Drain the 2025 backlog in bounded Rust sessions (not an always-up daemon). The scrub steady-state flip
stays held (needs systemd-class supervision + GPU headroom).
