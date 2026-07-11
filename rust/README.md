# scoracle-cognition

Rust Cognition Harness for Scoracle: the AI derivation layer that empowers local models, drains durable model work, and writes precomputed products for the Go API to serve.

This folder is not a side experiment. It is the production cognition layer for Scoracle.

Post the **Step-3 cutover (2026-06-28)** and the **Narratives news-hub fold-in (2026-07-03)**, the Go LLM derive stages are retired into Rust:

- **6 live queue stages** — `scrub` → `peak` → `transfers` → `narratives` → `vibe` → `sigil` —
  drained by the long-running **`scoracle-cognition`** daemon. Transfer heat and breaking-story
  urgency enrich both `narratives` and the final `sigil` panel read.
- **rating / PEAK** is produced by durable `pipeline_work(peak)` in nightly mode; the
  **`statcommentary`** bin remains the current-season producer and explicit historical backfill tool.

## Start Here

Before working in this folder, read these in order:

1. `../README.md`
2. `../../scoracle-wiki/PRODUCT_NARRATIVE.md`
3. `../../scoracle-wiki/DATA_FLOW.md`
4. This README
5. `../docs/DEVELOPMENT.md`
6. `../RUNBOOK.md` for release, rollback, and production operations

Shared process, vocabulary, and landmark history live in:

- `../../scoracle-wiki/wiki/CONVENTIONS.md`
- `../../scoracle-wiki/wiki/Glossary.md`
- `../../scoracle-wiki/wiki/Changelog.md`

## Layer Role

- Type: `backend/ai-cognition`
- Owns: model routing, model calls, extraction, fail-closed parsing, model-derived products, queue-stage draining, rating commentary batch, offline parity/eval harnesses, and CPU embedding helpers.
- Does not own: provider ingestion, public API serving, client presentation, product doctrine, or visual doctrine.
- Primary consumers: Postgres product tables, Go API prepared reads, and client cards through those API reads.

The system shape:

```text
Python ingest + Go RSS sweep
  -> Postgres source tables and pipeline_work
  -> Rust cognition stages and rating batch
  -> Postgres product tables
  -> Go API endpoints
  -> web/iOS cards
```

Serving requests must never call this layer directly. The Go API serves precomputed rows written by this layer.

## Product Pillars

Scoracle is lean, nimble, and durable.

Elegance comes through simplicity. Simple and durable beats clever and fragile. The flow of information must be clear and clean.

For this folder, that means:

- Make model inputs explicit.
- Make stage handoffs durable.
- Parse model outputs fail-closed.
- Persist provenance with every derived output.
- Prefer clear typed gates over clever recovery behavior.
- Keep GPU usage bounded and intentional.

## Current Production Shape

Post Step-3 cutover, Rust owns every live LLM queue stage:

```text
scrub -> transfers -> narratives -> vibe -> sigil
```

The long-running daemon is:

```text
scoracle-cognition
```

Rating commentary is not a queue stage. It runs as the Rust batch binary:

```text
statcommentary
```

Go no longer performs model inference on the serving path. The Go API handles serving, SQL-only maintenance, queue notification, and ingest funnel wiring.

## Mental Model

This layer has one durable boundary: Postgres.

Stages claim work from `pipeline_work`, read their source context from Postgres, call the configured local model through the router, parse and validate the response, write a product row, and enqueue downstream work when needed.

```text
claim work
  -> load context
  -> build deterministic request
  -> route role to model
  -> extract typed output
  -> persist product + provenance
  -> enqueue downstream work
  -> complete work row
```

Failures should be visible and recoverable:

```text
pending -> running -> complete/delete
pending -> running -> failed/backoff -> pending retry
running stale -> pending recovery
failed past retry cap -> dead-letter for human repair
```

## Stage Map

| Stage | File | Input | Output | Notes |
|---|---|---|---|---|
| `scrub` | `src/scrub.rs` | `news_articles`, entity context | `news_article_entities.vetted` | Article-keyed ID gate; uses embedding-assisted resolve. |
| `transfers` | `src/transfer.rs` | vetted news/entity pairs | `transfer_rumors` | Transfer/trade truth and heat, with shared source freshness and trajectory markers; fail closed on uncertain validity. |
| `narratives` | `src/narratives.rs` | vetted/link clusters + transfer heat | `news_summaries` | Storyline grouping, source freshness, and trajectory markers. |
| `vibe` | `src/vibe.rs` | narrative/corpus context | `vibe_scores` | Emotional rail end product. |
| `momentum` | `src/momentum.rs` | PEAK, Vibe, deterministic momentum snapshot | `momentum_summaries` | Generated trajectory direction/score/blurb; keeps `momentum_scores` as numeric backbone. |
| `sigil` | `src/sigil.rs` | Stats, narrative, transfer, momentum, previous Sigil | `sigil_synthesis` | Panel convergence; event-driven and debounced by `input_hash`. |
| `peak` | `src/rating.rs`, `src/bin/statcommentary.rs` | stats/rating context | `stat_summaries` | Statistical rail model read plus deterministic Composite/PEAK z-score trajectory; need-based `pipeline_work` stage for current season. |

Momentum's generated card is a queue stage. Its deterministic `/momentum` numeric backbone remains
`momentum_scores` / `latest_momentum_scores_per_entity`.

## Rail / Lens / Stage / Role Map

The Multi-Lens Cognition Panel uses three related words deliberately:

- **Rail** is the broad model-family lane: stats/analytical, emotional/news, or synthesis.
- **Lens** is the product perspective Scoracle wants to own: PEAK identity, Momentum trajectory,
  narrative grouping, transfer/trade truth, Vibe temperature, and final Sigil synthesis.
- **Stage** is the durable execution unit: a queue handler or batch that loads context, calls a
  model when needed, persists a product row, and writes `cognition_ledger` provenance.
- **Role** is the model-routing job sent to `Route`; it decides which concrete model/backend serves
  the call.

Current mapping:

| Rail | Lens | Stage or batch | Route role | Product / ledger surface |
|---|---|---|---|---|
| Stats/analytical | Rating / PEAK | rating batch | `StatsLogic` | `stat_summaries`, rating ledger rows, rating fixtures |
| Stats/analytical | Momentum | `momentum` | `StatsLogic` | `momentum_summaries`, `latest_momentum_scores_per_entity`, `/momentum`, momentum fixtures |
| Emotional/news | Narratives | `narratives` | `EmotionalNews` | `news_summaries`, narrative ledger rows, narrative fixtures |
| Emotional/news | Transfers | `transfers` | `EmotionalNews` | `transfer_rumors`, transfer ledger rows, transfer fixtures |
| Emotional/news | Vibe | `vibe` | `EmotionalNews` | `vibe_scores`, vibe ledger rows, vibe fixtures |
| Synthesis | Sigil synthesis | `sigil` | `StatsLogic` today | `sigil_synthesis`, sigil ledger rows, sigil fixtures |

`scrub` is upstream evidence gating, not a lens. Transfer still routes through
`Role::EmotionalNews`; the measured local bakeoff kept `mistral:7b` as the transfer baseline, so a
separate `TransferLogic` role remains deferred until fixtures and live pair captures justify it.
Momentum has a generated production card on the incumbent `StatsLogic` route. The `momentum` eval
task remains the route gate for Qwen/Gemma-style analytical candidates; broaden fixtures and live
captures before introducing a dedicated `MomentumLogic` split. Sigil stays on `StatsLogic` until
synthesis fixtures justify a `SynthesisLogic` split.

Current lens operating parameters:

| Lens | Operator frame | Mandate | Credibility guard |
|---|---|---|---|
| Narratives | beat writer | Compile the stories swirling around the entity. | Group what sources actually say; do not inflate vague hype or off-entity noise. |
| Transfers | transfer expert | Get movement predictions out quickly while preserving long-term credibility. | Fail closed on name-drops, stale links, weak sourcing, and misleading heat. |
| Vibe | content creator | Read the current vibe so a creator can piggyback on the conversation. | Separate interactable mood from durable truth; do not invent a narrative hook. |
| Rating / PEAK | opposing team scout | Name the greatest strength to stop and greatest weakness to exploit. | Use supplied tiers and datapoints only; never turn average marks into strengths. |
| Momentum | nimble trader | Read PEAK/rating trajectory as price action and Vibe/news as investor sentiment, then decide whether momentum is rising, falling, or a hold. | Stay detached and results-only; do not chase sentiment hype or cling to stale PEAK strength. |
| Sigil synthesis | reasoned expert network panelist | Summarize all pillars into the final Scoracle read. | Preserve real disagreement between pillars instead of flattening it. |

## Repository Layout

```text
rust/
├── Cargo.toml
├── README.md
├── build.rs
└── src/
    ├── main.rs              # the scoracle-cognition daemon: boots Harness, registers handlers, runs Worker
    ├── lib.rs               # library exports
    ├── buildinfo.rs         # exposes BUILD_COMMIT / BUILD_TIME (set by build.rs via env!)
    ├── config.rs            # env config; mirrors Go var names (.env.local)
    ├── db.rs                # sqlx Postgres pool (bounded — the GPU is the real ceiling)
    ├── work.rs              # pipeline_work client: claim/complete/fail/requeue_stale/enqueue + the Stage enum
    ├── ollama.rs            # Ollama HTTP client (mirrors go/internal/ml/ollama.go)
    ├── stage.rs             # StageHandler trait — the per-stage plug-in point
    ├── worker.rs            # LISTEN(pipeline_work_ready) + safety-net drain loop
    ├── route.rs             # the model-call seam (Role → concrete model); the GPU governor lives here
    ├── harness.rs           # Harness context + the capability primitives: extract, persist, debounce, resolve, embed, cluster (Plan §1)
    ├── util.rs              # shared helpers: truncate, go_json_* (Go-encoding/json byte parity), hash_components
    ├── embed.rs             # candle CPU embedder (BGE-small default) + cosine_similarity
    ├── resolve.rs           # the asymmetric embedding-hybrid relevance gate (resolve_set + resolve_one)
    ├── scrub.rs             # news-scrub stage handler (asymmetric gate, writes news_article_entities.vetted)
    ├── transfer.rs          # transfers stage: per-(team,player) rumor vetting with the t6 prompt
    ├── narratives.rs        # narratives stage: news storyline clustering + summarization
    ├── rating.rs            # rating stage per-entity core (the cmd/statcommentary batch body)
    ├── vibe.rs              # vibe stage: the sentiment + felt-read
    ├── sigil.rs             # sigil stage: panel convergence over stats, narrative, transfer, momentum, memory
    └── bin/
        ├── statcommentary.rs
        ├── eval.rs
        ├── parity.rs
        ├── sigil_parity.rs
        ├── rating_parity.rs
        ├── transfer_parity.rs
        └── narratives_parity.rs
```

## Core Primitives

`Harness` is the context passed to every stage. It owns:

- Postgres pool.
- Model router.
- Optional CPU embedder.
- Resolve policy.
- Shared extraction/debounce helpers.

`Route` is the model seam:

- Stage code names a `Role`, not a concrete model.
- `Router` maps each role to a configured backend/model.
- `GovernedInference` enforces the shared GPU concurrency budget.
- Ollama is the only backend today; vLLM or another backend should land as a new `Inference` implementation when real.

`Extract` is the typed model-call pattern:

```text
role -> request -> model reply -> Parser<T> -> Option<T> or failure
```

`Persist` is the moat envelope:

- `model_version`
- `prompt_version`
- `input_ids`
- optional `input_hash`
- `generated_at`

`Resolve` is asymmetric:

- CPU cosine can fast-track an obvious keep.
- Ambiguous cases go to the model.
- Do not let a cheap heuristic exclude real truth unless the measured policy explicitly supports it.

## Change Workflow

Use this workflow for most cognition changes:

1. Confirm sync:

```bash
git fetch
git status --short --branch
```

2. Read the relevant product/data docs:

```text
../../scoracle-wiki/PRODUCT_NARRATIVE.md
../../scoracle-wiki/DATA_FLOW.md
```

3. Identify the owned stage or primitive.
4. Preserve the product contract. If the contract changes, update `../ENDPOINTS.md`, `../README.md`, and the wiki if it is a landmark.
5. Add or update focused tests/parity harnesses.
6. Run verification.
7. Add a progress doc in `../../scoracle-wiki/progress_docs/scoracle-backend/`.
8. Commit and push.

## Adding Or Changing A Stage

Each queue stage should follow the same composition shape:

1. Constants: prompt version, temperature, token budget, and system prompt.
2. Loaders: SQL that reads the exact context needed.
3. Request builder: deterministic prompt and model options.
4. Extract: call `Harness::extract` through a `Role`.
5. Parser: typed parse of model reply.
6. Gates: fail-closed validation and debounce.
7. Persist: insert into the product table with provenance.
8. Handoff: enqueue downstream work before completing the current item when correctness depends on it.

Rules:

- Never fabricate a valid row from an uncertain model reply.
- Use `Ok(None)` or marker semantics when a stage should fail closed without retrying forever.
- Keep prompt versions explicit and bump them when output meaning changes.
- Keep model-specific IDs in routing config, not stage code.
- Do not write presentation concerns into product rows.
- Do not bypass `pipeline_work` for correctness-critical handoffs.

## Build And Verify

From repo root:

```bash
cd rust
cargo build
cargo test --lib
cargo clippy --all-targets -- -D warnings
```

Build the two live production binaries:

```bash
cargo build --bin scoracle-cognition --bin statcommentary
```

Run all Rust tests:

```bash
cargo test
```

Use release script for production builds:

```bash
../scripts/hosting/release.sh --build-only
```

The release script builds the live Go binaries plus the live Rust binaries from one commit, then places them atomically during full release.

## Offline Harnesses

Offline bins are for parity, shadow, and eval work. They must not claim live queue work unless explicitly designed to do so.

| Binary | Purpose |
|---|---|
| `parity` | Vibe temp-0 parity/shadow harness. |
| `sigil_parity` | Sigil temp-0 parity/shadow harness. |
| `rating_parity` | Rating/stat commentary parity harness. |
| `transfer_parity` | Transfer/trade parity harness. |
| `narratives_parity` | Narratives parity harness. |
| `eval` | Role/model A/B eval harness. |
| `statcommentary` | Live rating batch binary. |

Before changing a prompt, loader, parser, or shared JSON/hash utility, consider whether the relevant parity harness should be run or updated.

## Environment

The Rust layer reads environment variables directly. In production systemd loads `../.env` first and `../.env.local` second, so local secrets override committed defaults.

Required:

```text
DATABASE_PRIVATE_URL or DATABASE_URL
```

Common model/runtime config:

```text
OLLAMA_BASE_URL
OLLAMA_MODEL
OLLAMA_TIMEOUT_SECONDS
OLLAMA_MAX_CONCURRENT
COGNITION_STAGES
COGNITION_DB_MAX_CONNS
COGNITION_SAFETY_NET_SECONDS
COGNITION_STALE_LEASE_SECONDS
```

Routing:

```text
COGNITION_ROUTE_<ROLE>
COGNITION_ROUTE_<ROLE>_CANDIDATE
```

Embeddings and resolve:

```text
COGNITION_EMBED_MODEL
COGNITION_EMBED_REVISION
COGNITION_EMBED_POOLING
COGNITION_EMBED_MAX_TOKENS
COGNITION_RESOLVE_KEEP_THRESHOLD
COGNITION_RESOLVE_DROP_THRESHOLD
```

Defaults are configured for one local Ollama model and one GPU. Raise concurrency only when the hardware and live workload justify it.

## Operations

Production daemon:

```text
scoracle-cognition.service
```

Systemd unit:

```text
../scripts/systemd/scoracle-cognition.service
```

Logs:

```bash
journalctl --user -u scoracle-cognition -f
```

Standard release:

```bash
../scripts/hosting/release.sh
```

One-off debug rebuild on the production box:

```bash
cargo build --bin scoracle-cognition
cp target/debug/scoracle-cognition bin/scoracle-cognition
```

The path watcher may restart the daemon after the copy. Use the release script for normal production changes.

Emergency rollback shape:

```text
stop scoracle-cognition.service
set DERIVE_WORKER_ENABLED=true for Go fallback where still supported
restart scoracle-api.service
```

See `../RUNBOOK.md` before doing this in production. The rating batch is Rust-only after Step 3.

## Progress Docs

Every meaningful Rust cognition session adds a progress doc:

```text
../../scoracle-wiki/progress_docs/scoracle-backend/YYYY-MM-DD_short-description.md
```

Landmark AI-layer changes that affect other repos or the wiki instead go flat at
`../../scoracle-wiki/progress_docs/YYYY-MM-DD_short-description.md`. Landmarks include:

- new or removed cognition stage
- prompt semantics change
- model routing change
- product table/provenance change
- queue semantics change
- release/rollback behavior change
- GPU/concurrency policy change

## Handoff Format

For unfinished multi-step cognition work, leave:

```text
Continue work in scoracle-backend/rust on branch <branch>.

Read first:
1. ../README.md
2. ../../scoracle-wiki/PRODUCT_NARRATIVE.md
3. ../../scoracle-wiki/DATA_FLOW.md
4. rust/README.md

Last completed:
- <summary>

Changed files:
- <files>

Verification run:
- <commands/results>

Next task:
- <specific next step>

Known risks:
- <risks or none>
```

## Known Limits

- Team-roster Phase 2 and top-down roster coverage remain backend carry.
- The live single-GPU box is the throughput ceiling; Rust improves control, semantics, routing, and CPU-side capability, not raw model latency.
- `work::Item.entity_id` is guarded when narrowing to `i32`, but article IDs should be widened deliberately if corpus scale demands it.
