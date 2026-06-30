# Rust Cognition Phase 1 Durable Spine Execution

Date: 2026-06-30
Commit: `ff258a1` (`Stabilize Rust cognition durable spine`)

## Context

This execution implemented Phase 1 of `rust/COGNITION_CONTEXT_ENRICHMENT_PLAN.md`: stabilize the durable spine of the Rust cognition layer without broad refactors. The goal was not long-term byte parity as a product strategy. The goal was to make orchestration, retries, config, and queue identity durable enough that richer context work can build on reliable execution.

## What Changed

- Ran `cargo fmt` and committed the resulting Rust formatting cleanup.
- Made numeric environment config parsing strict in `rust/src/config.rs`.
  - Invalid integer values now fail boot clearly instead of silently using defaults.
  - Negative values for unsigned settings now fail boot.
  - Non-finite floats such as `NaN` now fail boot.
  - Added unit coverage for these strict parsing paths.
- Widened `work::Item.entity_id` from `i32` to `i64` in `rust/src/work.rs`.
  - This preserves full article-keyed scrub work IDs.
  - Entity-keyed stages now use checked `i32` conversion at player/team table boundaries.
- Made the `vibe -> sigil` enqueue handoff retryable in `rust/src/vibe.rs`.
  - Vibe persists first, then enqueues sigil before completion.
  - If sigil enqueue fails, the vibe item now returns an error and backs off instead of completing.
- Made transfer pair infrastructure and persist errors fail the team work item in `rust/src/transfer.rs`.
  - DB/build/persist errors are counted and cause the team item to retry.
  - Model non-commitment still persists UNKNOWN fail-closed rows and retries through the existing unknown count.

## Why It Matters

Phase 1 is about preventing model value from being lost after it is produced or while orchestration prepares the next derivation. These changes make the Rust cognition layer more durable around the model:

- Required downstream work is not silently dropped.
- Bad deploy configuration fails before corrupting runtime behavior.
- Article-keyed scrub work is no longer constrained by player/team ID width.
- Transfer model uncertainty remains fail-closed, while infrastructure failures are no longer hidden as successful team completion.

## Verification

From `rust/`:

```text
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

All three passed. The test run reported 87 passed, 1 ignored.

## Remaining Risks

- Repeated model outages in transfers still produce UNKNOWN rows until queue attempt limits are reached. That is intentional fail-closed behavior, but operational alerting should watch for repeated UNKNOWN retries.
- The repo still has an untracked `rust/COGNITION_CONTEXT_ENRICHMENT_PLAN.md` file. It was read as the source plan for this execution but not included in the code commit.
- This did not introduce a generic context framework or broader stage refactors; those remain deferred to later phases.
