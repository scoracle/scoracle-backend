# Scoracle Backend

**Postgres stores the evolving world. DuckDB studies the world. Studio is the in-house harness where LLMs create its evolving picture.**

Studio and harness name the same system. Its code home is [`rust/src/studio/`](rust/src/studio/). The models create within the world using prepared evidence, character instructions, shared form, and validation. The application calls on storage, analytics, and Studio as work requires; they do not form a mandatory sequence for every task.

## Start here

1. Read this README and the shared [product narrative](../scoracle-wiki/PRODUCT_NARRATIVE.md).
2. Read the [data-flow map](../scoracle-wiki/DATA_FLOW.md) for system boundaries and current task dependencies.
3. Use [development guidance](run_docs/DEVELOPMENT.md), [Studio and Rust guidance](rust/README.md), [endpoint contracts](run_docs/ENDPOINTS.md), and the [runbook](run_docs/RUNBOOK.md) for implementation and operations.

The wiki owns shared vocabulary and architecture direction: [Glossary](../scoracle-wiki/wiki/Glossary.md), [Changelog](../scoracle-wiki/wiki/Changelog.md), and [Conventions](../scoracle-wiki/wiki/CONVENTIONS.md). Plans and progress belong in `../scoracle-wiki/progress_docs/scoracle-backend/`; this repository holds durable implementation documentation.

## Three systems, explicit responsibilities

| System | Responsibility | Boundary |
|---|---|---|
| Postgres | Durable facts, relationships, evidence provenance, product history, work state, and serving projections. | Store and retrieve versioned state through application adapters. |
| DuckDB | Analytical studies: trajectories, comparisons, cohorts, and richer derived evidence. | Read a bounded snapshot; return versioned results, coverage, and provenance for publication. |
| Studio | Give characters the materials and capabilities to create; call models, validate outputs, and return products with provenance. | Accept prepared assignments and injected capabilities; no concrete database or queue access in the core. |
| Application wiring | Acquire evidence, prepare assignments, select models, schedule work, publish results, and serve products. | Own cross-system coordination, credentials, transactions, retries, and resource budgets. |

These are responsibilities, not a requirement for three services or repositories. Keep deployment simple. Analytics must earn its resource cost through useful evidence or reduced database work. Richer studies are a product capability: they give the characters observations they cannot make from today's material alone.

## Implementation status

The architecture above is the migration direction. The checked-in implementation currently has these boundaries:

- **Studio core, Analyst, and Influencer:** `rust/src/studio/` owns the model contract, generation/provenance types, shared character form, and both characters' creation sessions. An assignment can run through validation and an injected publisher without Postgres, DuckDB, or the queue.
- **Application adapters:** `rust/src/application/analyst.rs` and `influencer.rs` own their characters' retrieval, memory preparation, work policy, and claim-aware publication. `rust/src/application/outbox.rs` durably reconciles Vibe's Momentum offer and the two seats' Oracle barriers. `runtime/route.rs` selects/governs model transports. `runtime/worker.rs` and `runtime/work.rs` retain the current queue lifecycle.
- **Other characters:** still use `junctions/` and the database-bearing `runtime/harness.rs` compatibility context. Shared extraction delegates to Studio. This context is migration scaffolding; it is not a second permanent harness.
- **Analytics:** the incorporated `cleanup/character-prompts` work (`ea20950`, including `6ded51e`) supplies `go/internal/analytics/`, Postgres/DuckDB implementations for rating trajectories and bundles, the snapshot CLI, and cohort context consumed by memories. DuckDB currently attaches Postgres read-only; Postgres remains the default engine. Bounded snapshot/publication and production cutover gates remain. Reuse this engine and its parity tests rather than building a competing implementation.
- **Durability:** migration 256 and `runtime/work.rs` give each running lease a UUID claim token plus its captured input revision. Migrations 257–258 make Influencer and Analyst claim-aware publishers: after inference, each short transaction locks the exact lease, writes any product with required provenance, records only its durable follow-up obligation, and deletes the claim. A stale or superseded execution publishes nothing. Vibe completion offers Momentum before checking Oracle; Momentum completion needs only the Oracle barrier. `NoMaterial` writes no Momentum product but still commits that barrier obligation and completion. Diagnostic ledger writes remain explicitly best-effort after commit. Other handlers still write before generic acknowledgement, so their stale-result publication and atomic follow-up migration remain open gates in the [approved plan](../scoracle-wiki/progress_docs/scoracle-backend/2026-09-19_backend-modernization-plan.md).

Update this status as each boundary moves. Old pipeline descriptions in Git history explain past implementations; they do not constrain the destination.

## Creation and serving

The application prepares the material a character needs. The Analyst currently receives form, mood, numeric movement, and prepared sourced memories. The Studio session builds its prompt, calls the selected model with the existing bounded correction policy, and validates the read for application publication. An empty Analyst assignment finishes without a model call or product row; the application still durably completes its claim and Oracle-barrier obligation. Invalid prose fails before publication. Direction and conviction remain deterministic measurements.

Influencer receives current packets and rendered sourced memory. Never-scored empty material publishes an uncalled NULL marker; empty material after a prior real score gets one closing quiet read. Unchanged material debounces before memory rendering and model generation, while still offering Momentum work. Latest-row/prior-memory disagreement preserves the existing buried-read exception.

Other tasks have their own prerequisites. The Editor extracts evidence from articles; the Investigator verifies identities; the Scout interprets performance; the Journalist reports stories; the Influencer reads emotional charge; the Insider reads transfer developments; the Oracle creates a final reading from the supplied cards. These responsibilities do not imply that every task runs through every character.

Serving stays precomputed:

```text
published Postgres products -> Go prepared reads/cache -> product API -> web/iOS cards
```

No serving request starts a study, discovers evidence, or calls an LLM. Public route wiring lives in [`go/internal/api/server.go`](go/internal/api/server.go); prepared queries live in [`go/internal/db/db.go`](go/internal/db/db.go). [ENDPOINTS.md](run_docs/ENDPOINTS.md) documents the full contracts.

The leaderboard owns discovery and hierarchy, including roster scope through `entity_type=player&team_id=...`. Profiles compose per-entity products. Clients control presentation; they do not reconstruct ratings, momentum, or readings from raw ingredients.

## Repository map

| Path | Purpose |
|---|---|
| `go/` | API, acquisition wiring, maintenance, notifications, authentication, and prepared reads. |
| `sql/` | Durable schema, migrations, current analytics, triggers, and serving projections. |
| `rust/src/studio/` | In-house harness, character creation, shared form, and generation contracts. |
| `rust/src/application/` | Explicit application adapters; Analyst and Influencer evidence, publication, and follow-up coordination. |
| `rust/src/junctions/` | Seven remaining seats and their transitional application adapters. |
| `rust/src/runtime/` | Model transports/routing, queue runtime, and transitional application context. |
| `rust/src/composition/`, `rust/src/evidence/`, `rust/src/evaluation/` | Memory preparation, evidence adapters, character briefs awaiting migration, and evaluation. |
| `go/internal/analytics/` | Existing analytical interface and Postgres/DuckDB engines. |
| `scripts/hosting/` | Release, scheduling, backup, and recovery tooling. |
| `run_docs/` | Development, endpoint, and operational contracts. |

Current acquisition includes the Go RSS funnel and Rust evidence/identity/fixture work. Historical imports remain data; retired paid providers and the removed seed/Candle code are not current dependencies.

## Build and verify

```bash
(cd go && go build -o bin/scoracle-api ./cmd/api)
(cd rust && cargo test --lib)
(cd rust && cargo check --all-targets)
(cd rust && cargo build --bin scoracle-cognition --bin statcommentary)
```

Studio's Analyst and Influencer tests use fake model, publication, and application follow-up adapters and require no service credentials. Model-quality evaluations and live database integration checks are separate gates. See [rust/README.md](rust/README.md).

Local API startup:

```bash
cd go
go build -o bin/scoracle-api ./cmd/api
./bin/scoracle-api
```

`.env.local` is gitignored; there is no committed `.env` template. Supply `DATABASE_PRIVATE_URL` or `DATABASE_URL` (private URL takes precedence). See [`go/internal/config/config.go`](go/internal/config/config.go) and [`rust/src/runtime/config.rs`](rust/src/runtime/config.rs) for current environment defaults. Model routing is configured through `COGNITION_ROUTE_<ROLE>`; routing chooses the backend outside Studio. Optional mobile auth uses `JWT_SECRET`; unset auth is unavailable.

Use the [runbook](run_docs/RUNBOOK.md) and [release tooling](scripts/hosting/README.md) for deployment and rollback. A source commit or local test run does not establish the deployed version.

## Working on a change

Check branch and working-tree state before syncing. Preserve unrelated changes. Implement a bounded slice, verify the affected contracts, and record its evidence and remaining work in the wiki. Architecture and implementation status must change together; update public endpoint docs when contracts change. Keep shared jargon in the Glossary and meaningful architectural milestones in the Changelog.

## Trademarks & Nominative Fair Use

Team names, logos, and other identifying marks displayed by Scoracle are the property of their respective owners (leagues, teams, and affiliated entities). These marks are used solely to identify the teams and players whose statistical data is presented — not to imply any official sponsorship, endorsement, or affiliation between Scoracle and any league, team, or player.

This usage satisfies the three-part test for nominative fair use:

1. The teams and leagues cannot reasonably be identified without reference to their marks.
2. Only as much of each mark is used as necessary for identification.
3. Nothing in the presentation suggests official sponsorship or endorsement by the mark holder.

Scoracle is not affiliated with, endorsed by, or in any way officially connected to the NBA, NFL, the Premier League, La Liga, Bundesliga, Serie A, Ligue 1, or any of their member teams and clubs.

## License & Copyright

Copyright (c) 2026 Scoracle. All rights reserved.

This repository and its contents — including but not limited to source code, database schemas, API designs, data pipeline architecture, and documentation — are proprietary and confidential. No part of this repository may be reproduced, distributed, transmitted, or otherwise used in any form without the prior written permission of the copyright holder.

Unauthorized use, copying, modification, or distribution of any materials in this repository is strictly prohibited and may result in legal action.
