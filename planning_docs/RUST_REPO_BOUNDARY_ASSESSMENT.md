# Rust Repo Boundary Assessment

Date: 2026-06-30

## Recommendation

Keep the Rust Cognition Harness in this repo for now.

The Rust layer is important enough to be treated as its own product boundary, but splitting it into
a separate repository today would add coordination overhead before the contract is stable enough to
pay for that overhead. The durable boundary is already Postgres: `pipeline_work` for work leasing and
the product tables for outputs. As long as schema migrations, Go API reads, ops scripts, and Rust
writes are changing together, a mono repo keeps releases atomic and lowers failure risk.

The right move now is a "repo-ready module" inside the mono repo:

- keep `rust/` buildable and testable on its own;
- keep Rust deployment artifacts under `rust/bin/` and `scripts/systemd/scoracle-cognition.*`;
- keep the DB contract documented in migrations/runbooks;
- prevent Go from regrowing model inference;
- trim generated docs and stale route probes so operational checks reflect the current split.

## Why Not Split Yet

- **Schema and code still co-evolve.** Rust stages write `news_summaries`, `transfer_rumors`,
  `vibe_scores`, `sigil_synthesis`, `headlines`, and `stat_summaries`. Go serves those tables, and
  SQL migrations define them. Splitting now means cross-repo migration choreography for almost every
  cognition change.
- **Release atomicity matters.** `scripts/hosting/release.sh` builds three Go binaries and two Rust
  binaries from one commit, then places and restarts them together. That is exactly what you want
  while the AI layer is still moving quickly.
- **The local-instance story is still operationally joined.** The same `.env.local`, systemd user
  units, PostgreSQL, Ollama, and backup/release scripts operate the stack. A split repo would still
  need this repo for orchestration.
- **Parity history still matters.** The Rust ports were proven against Go behavior. Keeping the old
  references, migrations, parity bins, and API consumers close reduces drift while the cutover hardens.

## When To Split

Split Rust into its own repo when at least three of these are true:

- Rust releases on a materially different cadence than Go/API/SQL.
- The DB contract is versioned explicitly, with migration compatibility rules and a rollback matrix.
- Local instances need to install or update cognition independently from the public API.
- The Rust layer grows multiple backends or model runtimes that do not belong in the backend release.
- CI time or dependency weight from `candle`/model tooling becomes a frequent blocker for ordinary Go
  and SQL work.
- Another repo or machine consumes the Rust cognition layer as an independently deployed component.

## Split Shape, When Ready

If the split becomes justified, do it as an extraction rather than a rewrite:

1. Create `scoracle-cognition` with the current `rust/` crate at repo root.
2. Move only Rust source, Cargo files, Rust-specific docs, and Rust tests.
3. Keep SQL migrations in `scoracle-backend`; publish a small DB-contract document consumed by both.
4. Keep release orchestration in `scoracle-backend` until the Rust repo has its own packaged release.
5. Pin the backend to a cognition release artifact instead of building from a sibling directory.

## Current Risk Register

- The Rust queue item still stores `entity_id` as `i32`; article-keyed scrub currently fits, but a
  future widening should happen before article ids can exceed 2,147,483,647.
- Full-crate `cargo fmt --check` still reports pre-existing formatting drift outside the entrypoint;
  run a dedicated formatting pass when a noisy Rust-wide diff is acceptable.
- `go/docs/` is generated from handler annotations, so route pruning should update annotations first
  and regenerate Swagger in the same change.
