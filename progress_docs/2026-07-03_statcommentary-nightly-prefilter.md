# 2026-07-03 - Statcommentary nightly prefilter

## Goal

Keep the Rust statcommentary nightly job from scanning every current-season rated entity when no stats rows have changed.

## What Changed

- `rust/src/bin/statcommentary.rs` now DB-prefilters current-season rated entities before generation.
- The enum picks one stats row per entity with `row_number()`:
  - prefer the unscoped row (`league_id` 0 or NULL);
  - otherwise prefer the row with the richest `rating_breakdown`;
  - then use lowest `league_id` as the stable fallback.
- The enum only returns entities whose selected stats row is newer than the latest `stat_summaries` row, or whose latest summary has no usable `input_hash`.
- Empty `input_hash` now counts as missing, matching `last_commentary_hash`.
- The nightly crontab comment now documents the DB prefilter plus the existing input-hash debounce.

## Verification

- `cargo test` passed.
- Added a focused offline guard test for the enum SQL shape and hash/timestamp predicates.
- Ran a read-only DB validation query against current seasons:
  - all current-season rated entities currently skip by prefilter;
  - all latest summaries have hashes;
  - 22 FOOTBALL player duplicate league rows collapse to one selected row;
  - NBA and NFL showed no duplicate league rows.

## Decision

Keep the optimization. The `updated_at > generated_at` gate is conservative because the runtime `input_hash` debounce still protects material-change correctness after enumeration. Current live data supports the intended offseason behavior: no stats-rail GPU work when selected stats rows are not newer than latest summaries.

## Commit

- `49de7c5 Optimize nightly statcommentary enumeration`
