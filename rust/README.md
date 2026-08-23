# scoracle-cognition

Rust Cognition Harness for Scoracle: the AI derivation layer that empowers local models, drains durable model work, and writes precomputed products for the Go API to serve.

This folder is not a side experiment. It is the production cognition layer for Scoracle.

Post the **Step-3 cutover (2026-06-28)**, the **junctions refactor**, and the **Phase-9 demolition
(2026-08-08)**, Rust owns every LLM stage, organized as CHARACTER JUNCTIONS (`src/junctions/`):

- **10 live queue stages** — `graph` → `editor` → `investigate_entity` / `fixture_boxscore` →
  `peak` → `momentum` → `transfers` → `narratives` → `vibe` → `sigil` — drained by the
  long-running **`scoracle-cognition`** daemon on the packet rail (the legacy rail was demolished
  in Phase 9; `RAIL` is no longer a knob).
- **rating / PEAK** also runs as the **`statcommentary`** batch binary (current-season producer
  and explicit historical backfill tool).

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
- Owns: model routing, model calls, extraction, fail-closed parsing, model-derived products, queue-stage draining, rating commentary batch, eval/operator-support tools, and CPU embedding helpers.
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

Rust owns every live LLM queue stage:

```text
graph -> editor -> investigate_entity / fixture_boxscore
      -> peak -> momentum -> transfers -> narratives -> vibe -> sigil
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

Stage code lives in CHARACTER JUNCTIONS — `src/junctions/<character>/` with `mod.rs` (stage),
`prompt.rs` (contract + version), `tests.rs`. The junction roster table in
`src/junctions/mod.rs` is the authoritative seat map; prompt versions live in each junction's
`prompt.rs` and rot fast in any doc that copies them, so none are copied here.

| Stage | Junction (character) | Input | Output |
|---|---|---|---|
| `graph` | `graph` (typed extraction) | article full text | graph entities/claims |
| `editor` | `editor` (The Editor) | article full text | evidence cards, `story_type`, packets, routing |
| `investigate_entity` | `investigator` (The Investigator) | encyclopedia summaries | identity verdicts |
| `fixture_boxscore` | `investigator/boxscore` | fixture pages | box-score facts |
| `rating` | `scout` (The Scout) | rating profile + decision card | `stat_summaries` (body + headline) |
| `momentum` | `analyst` (The Analyst) | form/mood trends + snapshot | `momentum_summaries` |
| `transfers` | `insider` (The Insider) | vetted pair context | `transfer_rumors` |
| `narratives` | `journalist` (The Journalist) | packet corpus + evidence cards | `news_summaries` (+ `card_score`) |
| `vibe` | `influencer` (The Influencer) | packets, narratives, heat | `vibe_scores` (SCORE/HOOK/VIBE) |
| `sigil` | `oracle` (the Oracle) | the five pillar cards + computed omen — nothing else (blind to memories since or9) | `sigil_synthesis` |

Momentum's generated card is a queue stage. Its deterministic `/momentum` numeric backbone remains
`momentum_scores` / `latest_momentum_scores_per_entity`. Rating also runs as the `statcommentary`
batch. (`divined_peak` left the product at s16/or10 with the PEAK concept; the stage was named
`peak` until mig 221 and is `rating` now.)

## Seat Doctrine

Rewritten 2026-08-22 after a full-fleet role audit. Everything here is measured; the numbers are
the argument.

### One card, one job

Each seat owns exactly one question, and its value is the part no other seat can supply.

| Seat | Owns | Must not touch |
|---|---|---|
| The Journalist | each developing narrative, reported **with attribution** | — (may cover transfers: she reports, the Insider vets) |
| The Influencer | each developing **emotional** story, focused on now | the transfer ledger, stats, direction |
| The Scout | the entity's **z-score profile now**, plus developing statistical trends | news, transfers, emotion, overall direction |
| The Insider | each developing **transfer** story, vetted for stage and credibility | emotion, stats, trajectory |
| The Analyst | the **direction** of the rating and vibe trajectories, and their relationship | peer prose, news, transfers, stat specifics |
| The Oracle | the verdict over five cards | being a sixth reporter |

The Analyst's whole value is the interplay: *"the results are poor but the room is high"* is her
sentence and nobody else's. The Oracle's is that it comes last.

### The title is the hook — every seat, one contract

Scott's ruling (2026-08-23, the headline+body era): *"the hook should be the one sentence hook
to draw the reader in. That should be the same across characters. This is key on the leaderboard
because it's what leads the user to click on the entity for more."*

Every card title — the Influencer's HOOK, the Analyst's and Scout's HEADLINE, the Journalist's
narrative title, the Insider's wire line, the Oracle's crown title — is the same product object:
**one sentence, twelve words or fewer, written to make a fan tap the card.** The entity's name
inside a claim, never a `"Label: description"` taxonomy line — a label files the card; a hook
sells it. The shared contract is enforced once (`guards::hook_violation` + `settle_title`); the
per-seat prompt states it AT THE EMISSION SITE, because a bare "card title" ask begets a label
(measured 2026-08-23: 138 Analyst + 56 Scout colon-labels dropped in 3h before the asks carried
the doctrine).

**Measured before this pass** — rows are the seat writing, columns the domain it talked about,
eight well-covered teams, `[]` marks its own job:

```text
seat          stat profile  trajectory  emotion  transfers   news
rating             [100%]        12%      25%        0%      25%
momentum              42%      [57%]      42%       28%      42%
vibe                  12%        25%    [87%]       75%      25%
transfers              0%        25%       0%     [100%]     12%
news                  25%        12%       0%      100%     [62%]
```

Off-diagonal average 26%. **No seat was disobeying its contract.** The Analyst's already said
"narrates the decided direction... and what tension exists between the two" — she was narrating
her INPUTS, four fifths of which were other seats' output. The Influencer called
`write_heat_lines`, the identical function that builds the Insider's prompt, so she recited his
ledger. Fix the input, not the rule.

### The law: a ban loses to the phrase in the input

**A prohibition in a prompt cannot beat the same words sitting in that prompt's own material.**
Recorded reproductions on this rail:

- Analyst s13 (101/109) and s14 (98/109) — banned vocabulary that the PEAK trajectory label kept
  supplying.
- 2026-08-22, the momentum ban list re-added to close 7 production failures: **without it 86/86
  and zero occurrences; with it 84/86 and the READ writes "the tape calls this"** — the exact
  phrase the clause forbade. Withdrawn.
- 2026-08-22, the Oracle's opening line enumerated all five seats by name, then rule 3 forbade
  naming more than one. It roll-called four. Removing the names fixed what the rule could not.

The corollary is a division of labour: **guards enforce, prompts instruct.** Wherever
`src/guards.rs` already covers a rule, the prompt states it in a clause or not at all — never an
essay. The essay is what causes the violation.

### Claim order is a dependency order

`worker` tops up "in registration (DAG) order", so registration order IS priority.
`work::VOICE_ORDER` holds it, `main.rs` registers by iterating it, and unit tests assert the
dependency rules rather than the literal list:

```text
1 Journalist  2 Influencer  3 Scout  4 Insider  5 Analyst  6 Oracle
```

The Analyst consumes the Scout's card and the Influencer's; the Oracle consumes all five. Running
a consumer ahead of its producers does not fail — it quietly synthesises yesterday's cards, which
is worse, because nothing reports it. Per-stage caps in `worker::stage_room` keep this an order
and not a starvation ladder.

`Stage::claim_order` additionally drains **teams before players** on every card-writing stage.
The three stages missing from that arm (rating, momentum, transfers) had team cards up to six days
stale behind thousands of player rows, while the three in it were current.

### Fail open on titles

A junk card title must never discard the card. The Analyst degrades to NULL (s18, "a junk TITLE
never kills it"), the Influencer salvages (`guards::salvage_hook`), and the Scout joined them on
2026-08-22 after a complete graded profile was thrown away over a colon — then re-rolled at
temp=0 to produce the same colon, which is a permanent stall rather than a retry.

### One window for the whole fleet

`MAX_LOADED_MODELS=1` on archbox's single runner, and ollama reloads whenever `num_ctx` changes
(the mixed-window era cost ~a fifth of wall clock). `VOICE_NUM_CTX_PACKET` and
`LOCAL_STAGE_NUM_CTX` move together or not at all.

**The Editor sets the floor.** It is the only stage reading a full article and the gatekeeper for
everything downstream. At 3072 its article budget halves to ~3,700 chars against a 6,142-char
median body, so the fleet stays at 4096. Trim before shrinking (D-T35).

Token ratios, measured with the live tokenizer: **~7 chars/token** for instructional prose,
**4.68** for article text. Estimating at 4 inflates a budget table by ~40%.

### Gates test meaning, not keywords

`prose_includes:falling` failed on "a steady slide" and "in decline", which read fine. Lean
prompts and literal-keyword assertions are incompatible — the gate forces vocabulary stuffing into
the prose it is meant to protect. Use `prose_includes_any` synonym sets, and reserve exact matches
for contract tokens the parser actually needs.

`eval --task <seat> --fixtures --live-system` replays frozen fixture inputs against the CURRENT
source constant. That is the gate for a prompt rewrite; without `--live-system` you are scoring
the prompt the fixture froze. **Baseline the seat before calling a score a regression** — the
Oracle's lean prompt read as a failure at 62/76 until the original was measured at 33/60.

## Rail / Lens / Stage / Role Map

The Multi-Lens Cognition Panel uses three related words deliberately:

- **Rail** is the broad model-family lane: stats/analytical, emotional/news, or synthesis.
- **Lens** is the product perspective Scoracle wants to own: PEAK identity, Momentum trajectory,
  narrative grouping, transfer/trade truth, Vibe temperature, and final Sigil synthesis.
- **Stage** is the durable execution unit: a queue handler or batch that loads context, calls a
  model when needed, persists a product row, and writes `cognition_ledger` provenance.
- **Role** is the model-routing job sent to `Route`; it decides which concrete model/backend serves
  the call.

Current mapping — every character seat owns its role (the identity split), and roles resolve to
concrete models/hosts via `COGNITION_ROUTE_<ROLE>` (see `src/route.rs` for the authoritative
role list; `src/eval_tasks.rs::lens_parameters` for the operator frames):

| Rail | Lens | Stage or batch | Route role | Product / ledger surface |
|---|---|---|---|---|
| Stats/analytical | Rating / PEAK | `peak` + rating batch | `StatsLogic` | `stat_summaries`, rating fixtures |
| Stats/analytical | Momentum | `momentum` | `MomentumLogic` | `momentum_summaries`, momentum fixtures |
| Emotional/news | Narratives | `narratives` | `NarrativeLogic` | `news_summaries`, narrative fixtures |
| Emotional/news | Transfers | `transfers` | `TransferLogic` | `transfer_rumors`, transfer fixtures |
| Emotional/news | Vibe | `vibe` | `VibeLogic` | `vibe_scores`, vibe fixtures |
| Emotional/news | Evidence / routing | `editor` | `Editor` | evidence cards, packets, editor fixtures |
| Emotional/news | Identity | `investigate_entity` | `Investigator` | identity verdicts, investigate fixtures |
| Synthesis | The crown reading | `sigil` | `OracleLogic` | `sigil_synthesis`, oracle fixtures |

One doctrine note that shapes every seat: served prose never names the internal machinery. The
product names ("PEAK", "Vibe") and field words (notability, sentiment, z-score …) are desk
bookkeeping; the gate enforces this with the case-sensitive `no_product_names` invariant on the
Scout, the Analyst, and the Oracle (D-T57).

## Repository Layout

```text
rust/
├── Cargo.toml
├── README.md
├── build.rs
├── fixtures/                # frozen eval fixtures, one dir per eval task (regenerate via examples/)
├── examples/                # fixture GENERATORS (the regeneration path) + read-only probes
└── src/
    ├── main.rs              # the scoracle-cognition daemon: boots Harness, registers handlers, runs Worker
    ├── lib.rs               # library exports
    ├── buildinfo.rs         # exposes BUILD_COMMIT / BUILD_TIME (set by build.rs via env!)
    ├── config.rs            # env config; mirrors Go var names (.env.local)
    ├── db.rs                # sqlx Postgres pool (bounded — the GPU is the real ceiling)
    ├── work.rs              # pipeline_work client: claim/complete/fail/requeue_stale/enqueue
    ├── ollama.rs            # local Ollama HTTP client
    ├── openai.rs            # OpenAI-compatible client (oMLX/MLX backends; response_format withheld by default)
    ├── stage.rs             # StageHandler trait — the per-stage plug-in point
    ├── worker.rs            # LISTEN(pipeline_work_ready) + safety-net drain loop
    ├── route.rs             # the model-call seam (Role → concrete model/host); the GPU governor lives here
    ├── harness.rs           # Harness context + the capability primitives: extract, persist, debounce, embed
    ├── util.rs              # shared helpers: truncate, canonical JSON formatting, hash_components
    ├── embed.rs             # candle CPU embedder (BGE-small default) + cosine_similarity
    ├── corpus.rs            # shared corpus loaders + heat-line rendering
    ├── ledger.rs            # cognition_ledger provenance writes
    ├── eval_tasks.rs        # the per-lens eval TASK REGISTRY (fixtures, Expect axes, invariants)
    ├── judge.rs             # reading-sheet / voice-spec judge support
    ├── bucket.rs, threads.rs, trajectory.rs, fetch.rs   # supporting modules
    ├── junctions/           # THE CHARACTER LAYER — one junction per seat:
    │   ├── mod.rs           #   the junction roster (authoritative seat map)
    │   ├── editor/  investigator/  journalist/  insider/
    │   ├── influencer/  analyst/  scout/  oracle/  graph/
    │   └── <each>: mod.rs (stage) + prompt.rs (contract + version) + tests.rs
    └── bin/
        ├── eval.rs          # fixture gate + live A/B harness
        ├── statcommentary.rs
        └── remap.rs, storylinefill.rs, bucketlabel.rs   # spent one-shot backfills (prune candidates)
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

(The `Resolve` primitive and the `scrub` stage were demolished with the legacy rail in Phase 9 —
relevance belongs to The Editor now. The embedder survives for narratives near-duplicate dedup.)

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
5. Add or update focused tests, fixtures, or eval coverage.
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

## Offline Tools

Offline bins are for evaluation and operator-support work. They must not claim live queue work unless explicitly designed to do so.

| Binary | Purpose |
|---|---|
| `eval` | Role/model A/B eval harness + the frozen-fixture gate (`--task <T> --fixtures`). |
| `statcommentary` | Live rating batch binary. |
| `remap` / `storylinefill` / `bucketlabel` | Spent one-shot backfills — their runs are on the record; prune candidates (2026-08-10 audit). |

Before changing a prompt, loader, parser, or shared JSON/hash utility, add or refresh focused tests/fixtures and consider whether `eval` should cover the behavior.

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
