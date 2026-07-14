# Rust Stage Layer - next steps

**Date:** 2026-06-30
**Scope:** `rust/` cognition harness stage runtime, post-refactor.
**Status:** Structural stage layer implemented and verified locally; live DB smoke still needs a
filled `DATABASE_PRIVATE_URL` or `DATABASE_URL`.

## Why this matters

The Rust layer is becoming the product's AI/cognition runtime, not just a port of the old Go
derive code. The stage layer gives that runtime an explicit home:

- stage contracts and metadata live in one place;
- stage registration is centralized;
- per-item execution policy is no longer embedded in the worker loop;
- model availability checks happen before work is claimed;
- future reliability work has a natural insertion point.

This is the right direction if Rust is going to own more of Scoracle's model reasoning,
grounding, evaluation, and provenance.

## What landed in the current working tree

### New stage layout

The requested shape now exists:

```text
rust/src/stage/
  mod.rs
  registry.rs
  runner.rs
  prompt.rs
  scrub.rs
  headlines.rs
  transfers.rs
  narratives.rs
  vibe.rs
  sigil.rs
```

The old public module paths remain compatible through re-exports in `rust/src/lib.rs`:

- `scoracle_cognition::vibe`
- `scoracle_cognition::transfer`
- `scoracle_cognition::narratives`
- `scoracle_cognition::headline`
- `scoracle_cognition::scrub`
- `scoracle_cognition::sigil`

This keeps the parity binaries compiling while allowing new code to use
`scoracle_cognition::stage::*`.

### Stage metadata

`rust/src/stage/mod.rs` now owns:

- `StageHandler`
- `StageSpec`
- per-stage `required_roles`
- per-stage `needs_embedder`
- per-stage `downstream`

Each queue stage now advertises its runtime requirements:

- `scrub`: `EmotionalNews`, embedder, downstream trigger stages
- `headlines`: `EmotionalNews`
- `transfers`: `EmotionalNews`
- `narratives`: `EmotionalNews`, embedder, downstream `vibe`
- `vibe`: `EmotionalNews`, downstream `sigil`
- `sigil`: `StatsLogic`

### Registry

`rust/src/stage/registry.rs` now owns:

- `DEFAULT_STAGE_LIST`
- `parse_enabled_stages`
- ordered handler construction
- embedder requirement detection

`rust/src/main.rs` is now smaller and delegates stage setup to the registry.

### Runner

`rust/src/stage/runner.rs` now owns per-item execution policy:

- elapsed-time logging;
- complete bookkeeping;
- retry/fail bookkeeping;
- defer-without-attempt bookkeeping;
- model preflight before stage claims.

This is intentionally still thin. It is now the place to add cancellation, lease heartbeat,
more structured metrics, and model-unavailable classification.

### Queue defer primitive

`rust/src/work.rs` now has `work::defer`.

This releases a `running` item back to `pending` with a future `available_at`, records
`last_error`, and does **not** increment `attempts`.

This is the primitive needed for outages and shutdown paths where the input is still valid and
should not move toward dead-letter.

### Model preflight

`rust/src/route.rs` now exposes:

- `Inference::ping`
- `Router::ping_role`

`rust/src/worker.rs` calls `stage::runner::preflight` before claiming a stage. If the required
model role cannot ping, the worker skips claims for that stage instead of claiming rows that will
immediately fail.

This directly addresses the audit finding that Ollama outages could burn attempts.

## Verification completed

All of these pass locally:

```bash
cd rust
cargo check --all-targets
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

Current test count:

- 94 passed
- 1 ignored (`embed::tests::paraphrase_beats_unrelated`, intentionally ignored because it
  downloads BGE-small and runs CPU inference)

Additional focused tests were added for `Router::ping_role`:

- reachable `/api/tags` backend succeeds;
- unreachable backend errors with the role context.

Local Ollama was also reachable during the smoke pass:

- `mistral-32k:latest`
- `mistral:latest`

## Not verified yet

The live DB-backed worker smoke could not be run from this shell because:

- `../.env` is the committed template and has blank `DATABASE_PRIVATE_URL` / `DATABASE_URL`;
- no `.env.local` is present in this checkout;
- `psql`, Docker, and local Postgres binaries are not available in the environment.

That means the exact `pipeline_work` claim/no-claim behavior has not been observed against a
real database from this session. The code path is covered by compile/test and the route preflight
tests, but the full worker smoke should still be run before release.

## Next execution sequence

### 1. Stage the refactor as renames

Before review/commit, stage with rename detection in mind:

```bash
git status --short
git diff --find-renames --stat
```

The current working tree will show old root stage files as deleted and `rust/src/stage/` as
untracked until staged. That is expected.

Suggested commit boundary:

```text
Introduce Rust stage runtime layer
```

Keep this commit focused on:

- module move;
- registry;
- runner;
- stage metadata;
- defer primitive;
- model preflight.

Do **not** mix shutdown cancellation or schema snapshot updates into the same commit.

### 2. Run DB-backed outage smoke

Once a real DB URL is available:

```bash
cd rust
DATABASE_PRIVATE_URL='<dev-db-url>' \
OLLAMA_BASE_URL='http://127.0.0.1:9' \
OLLAMA_MODEL='mistral:latest' \
COGNITION_STAGES='headlines' \
COGNITION_SAFETY_NET_SECONDS=3600 \
COGNITION_STALE_LEASE_SECONDS=315360000 \
RUST_LOG=debug \
cargo run --bin scoracle-cognition
```

Expected behavior:

- worker starts;
- stage preflight fails;
- worker logs `stage preflight failed; skipping claims`;
- no `headlines` rows are claimed;
- no `pipeline_work.attempts` values increment.

Use a huge stale lease for this smoke so the worker does not requeue unrelated stale rows before
the preflight check.

### 3. Run DB-backed live-Ollama smoke

With the real Ollama URL:

```bash
cd rust
DATABASE_PRIVATE_URL='<dev-db-url>' \
OLLAMA_BASE_URL='http://localhost:11434' \
OLLAMA_MODEL='mistral:latest' \
COGNITION_STAGES='headlines' \
COGNITION_SAFETY_NET_SECONDS=3600 \
RUST_LOG=debug \
cargo run --bin scoracle-cognition
```

Expected behavior:

- stage preflight passes;
- `headlines` claims normally;
- successful items complete normally;
- ordinary stage failures still go through retry/backoff;
- there is no unexpected attempt burn from backend unavailability.

### 4. Add shutdown/cancellation handling

Next reliability commit:

- add a cancellation token to `Worker`;
- stop claiming new work after shutdown starts;
- let `runner` requeue/defer claimed-but-unprocessed items;
- eventually wrap model calls in cancellation-aware timeouts.

This should be separate from the structural stage-layer commit.

### 5. Add lease heartbeat or bounded stage rounds

Next scheduling commit:

- consider per-item `touch_lease`;
- or bound each stage to one/few claim batches before moving to the next stage;
- or both, if transfer/team drains are still long.

This addresses duplicate/stale lease risk and stage fairness.

### 6. Finish observability

`stage::runner` should become the single place for structured stage telemetry:

- stage;
- entity type/id;
- sport;
- required role;
- model;
- prompt version, where known;
- prompt bytes, where available;
- eval count;
- wall time;
- outcome: complete, no-op, defer, retry, fail bookkeeping error.

Do this before deeper prompt/model optimization so the impact of prompt changes is measurable.

### 7. Revisit prompt contracts

`stage/prompt.rs` currently contains only the first metadata shape. Do not force every stage into
a generic prompt abstraction yet.

Use `PromptContract` first for:

- tracing;
- eval harness labeling;
- review dashboards;
- prompt-version inventory.

Keep per-stage prompt builders typed and local.

## Proposed Rust-local AI docs folder

The project likely needs a Rust-local documentation area for the AI/cognition system, separate
from broad backend docs and separate from date-stamped progress notes.

Recommended folder:

```text
rust/docs/cognition/
```

Recommended initial files:

```text
rust/docs/cognition/README.md
rust/docs/cognition/stage-runtime.md
rust/docs/cognition/prompt-contracts.md
rust/docs/cognition/model-routing.md
rust/docs/cognition/eval-and-parity.md
rust/docs/cognition/operations.md
```

Purpose of each:

- `README.md`: what the Rust cognition layer owns and does not own.
- `stage-runtime.md`: `StageHandler`, `StageSpec`, registry, runner, queue lifecycle.
- `prompt-contracts.md`: prompt versions, output schemas, parser/fail-closed rules.
- `model-routing.md`: roles, model swaps, Ollama/vLLM future seam, GPU governor.
- `eval-and-parity.md`: parity binaries, eval harnesses, model promotion checklist.
- `operations.md`: systemd, outage behavior, DB smoke, dead letters, shutdown playbook.

Why `rust/docs/cognition/` instead of only `progress_docs/`:

- `progress_docs/` is a ledger.
- `rust/docs/cognition/` should be the durable operator/developer manual.
- The AI layer is now important enough that future contributors should not have to reconstruct
  live behavior from dozens of dated notes.

## Carry list

- Run real DB-backed outage/live-Ollama smoke.
- Stage the file moves as renames and commit the stage-layer refactor.
- Add shutdown cancellation/defer handling.
- Add lease heartbeat or bounded stage rounds.
- Expand runner telemetry.
- Create `rust/docs/cognition/` after the stage-layer commit lands.
- Update `rust/README.md` layout references from root stage files to `src/stage/*.rs`.
- Consider moving future AI design docs out of ad hoc progress docs and into the new
  Rust-local cognition docs folder.

