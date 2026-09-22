# Scoracle Backend

**The Studio provides the room. Plugins bring the cognition. Postgres stores the evolving world. DuckDB studies the world.**

Studio and harness name the same system. Its code home is [`rust/src/studio/`](rust/src/studio/). Every cognitive seat is a plugin with a manifest declaring its identity, contract version, tasks, model roles, products, resources, and grants; Studio is the contained room that resolves, budgets, validates, and commits their work. The models create within the world using prepared evidence, character instructions, shared form, and validation. The application calls on storage, analytics, and Studio as work requires; they do not form a mandatory sequence for every task.

> **A plugin declares what it may reach; the room decides how the reach happens, records that it happened, and can refuse it.**

**Harness direction lives in the [Studio north star](rust/README.md): shared form, one character voice, compact identity, and only that character's evidence.** Use it when changing prompts, assignments, memories or evaluation. Implementation and rollout details below describe current behavior, not extra permanent harness requirements.

## Start here

1. Read this README and the shared [product narrative](../scoracle-wiki/PRODUCT_NARRATIVE.md).
2. Read the [data-flow map](../scoracle-wiki/DATA_FLOW.md) for system boundaries and current task dependencies.
3. Use [development guidance](run_docs/DEVELOPMENT.md), [Studio and Rust guidance](rust/README.md), [endpoint contracts](run_docs/ENDPOINTS.md), and the [runbook](run_docs/RUNBOOK.md) for implementation and operations.

The wiki owns shared vocabulary and architecture direction: [Glossary](../scoracle-wiki/wiki/Glossary.md), [Changelog](../scoracle-wiki/wiki/Changelog.md), and [Conventions](../scoracle-wiki/wiki/CONVENTIONS.md). Repository plans and progress records live in [`docs/planning_docs/`](docs/planning_docs/) and [`docs/progress_docs/`](docs/progress_docs/). Shared cross-repository records remain in `../scoracle-wiki/progress_docs/scoracle-backend/`.

## Three systems, explicit responsibilities

| System | Responsibility | Boundary |
|---|---|---|
| Postgres | Durable facts, relationships, evidence provenance, product history, work state, and serving projections. | Store and retrieve versioned state through application adapters. |
| DuckDB | Analytical studies: trajectories, comparisons, cohorts, and richer derived evidence. | Read a bounded snapshot; return versioned results, coverage, and provenance for publication. |
| Studio | The contained room for cognition: the plugin registry, lifecycle, budgets, validation, and commit. Plugins define the cognition that happens inside it. | Accept prepared assignments and injected capabilities; no concrete database or queue access in the core; no character behavior in the core. |
| Application wiring | Acquire evidence, prepare assignments, select models, schedule work, publish results, and serve products. | Own cross-system coordination, credentials, transactions, retries, and resource budgets. |

These are responsibilities, not a requirement for three services or repositories. Keep deployment simple. Analytics must earn its resource cost through useful evidence or reduced database work. Richer studies are a product capability: they give the characters observations they cannot make from today's material alone.

## Implementation status

The architecture above is the migration direction. The checked-in implementation currently has these boundaries:

- **Studio core and migrated seats:** `rust/src/studio/` owns the model contract, generation/provenance types, shared character form, and Analyst, Influencer, Scout, Journalist, Insider, Oracle, Editor, Investigator, and Graph creation. Their assignments run without Postgres, DuckDB, queues, or concrete model-host knowledge. Insider owns pair verdict, identity-adjudication, and wrap contracts; Oracle receives five typed cards and interprets complete, partial, or empty spreads inside Studio.
- **Plugin contract and fleet registry:** `rust/src/studio/plugin.rs` defines the plugin seam — `PluginManifest` (identity, contract version, tasks, model roles, context requirements, consumed/produced products, resource profile, tool grants), the `StudioPlugin` trait implemented directly by every adapter, and `PluginRegistry` with boot validation. `rust/src/studio/fleet.rs` holds the ten first-party manifests; their resource profiles are the single source the worker's scheduler reads, and their product relationships record the reading graph. `rust/src/studio/tools.rs` is the tool broker — the room's shared web workspace (`WebBroker`/`ScopedWeb`) enforces deny-by-default domain-class grants, per-run call budgets, and a per-run call ledger; the Investigator's Wikidata/Wikipedia discovery and the box-score plugin's registered sources already route through it. `PluginOutcome` makes disposition a plugin *report*: the plugin states what it committed or deferred (with its progress guarantee), and the worker performs the queue operation — a defer without progress falls to the retry ladder. Boot logs the resolved fleet. Scoped context plans (executed context providers) and manifest-derived dependency invalidation remain the next plugin slices; see the [plugin architecture plan](../scoracle-wiki/progress_docs/scoracle-backend/2026-09-21_studio-plugin-architecture.md).
- **Application adapters:** `rust/src/application/` owns concrete retrieval, memory preparation, model selection, work policy, and claim-aware publication for all nine migrated seats. Insider's adapter owns transfer retrieval, partial pair progress, identity effects, routing, and fenced publication; Oracle's owns pillar readiness/enqueue policy and terminal crown publication. `rust/src/application/queue/outbox.rs` durably reconciles concrete Momentum/Oracle obligations, including transfer-pair fanout.
- **Application runtime:** `application/queue/worker.rs` owns queue dispatch, maintenance, and recovery. Every registered plugin implements `StudioPlugin`: it accepts only an exact claim and reports a durable `PluginOutcome` (committed, deferred with its progress guarantee, or superseded); the worker performs the queue operation. Scheduling caps and web-fetch grants come from each plugin's manifest. Storage-only helpers take a pool, while `application/models.rs` holds only model routes and limits. The legacy database-bearing harness, junctions, composition compatibility exports, and generic worker completion are removed.
- **Analytics:** the API maintains Rating cohort and season-change context with DuckDB on startup and every five minutes. Bounded PostgreSQL snapshots feed private computation; source-checked transactions publish changed results and retain prior results on failure. The snapshot CLI provides frozen-input parity checks and explicit publication. SQL rating, event-score and Momentum formulas remain active until separately migrated with parity evidence. See the [SQL contract and source tree](sql/README.md) and [acceptance operations](run_docs/RECOVERY_ANALYTICS_ACCEPTANCE.md). Reuse the existing engines and tests rather than building a competing implementation.
- **Momentum maintenance:** Go now commits SQL scores, their concurrent serving-projection refresh, and exact dirty-marker acknowledgement together. Failure preserves retryable work and the prior projection; newer markers survive. This recovery fix is deployed on Archbox in `ebac794`, retaining the existing SQL producer; full-load transaction/lock costs remain under observation.
- **Durability:** migration 256 and `application/queue/work.rs` give each running lease a UUID claim token plus its captured input revision. Migrations 257–261 make Influencer, Analyst, Scout, Journalist, and Insider claim-aware publishers with narrow durable follow-up obligations. Insider preserves partial progress while each pair and identity effect is fenced by the exact team lease; transfer products atomically record per-target Oracle barriers and final completion records the team barrier. Oracle uses the same exact-claim boundary without a new outbox obligation because it is terminal. A stale or superseded execution publishes nothing. Diagnostic ledger writes remain explicitly best-effort after commit. All nine seats now run on the fenced production runtime. A bounded live Rating → Momentum → Sigil canary completed; overnight behavior and live fault-injection acceptance remain; see the [approved plan](../scoracle-wiki/progress_docs/scoracle-backend/2026-09-19_backend-modernization-plan.md).

Workers share per-item PostgreSQL claims across hosts, with broadcast wake-ups and fenced publication. The scheduling repair adds independent outbox dispatch, rotating admission, and claim-time Analyst/Oracle dependency checks; see [shared worker scheduling](run_docs/DEVELOPMENT.md#shared-worker-scheduling).

Update this status as each boundary moves. Old pipeline descriptions in Git history explain past implementations; they do not constrain the destination.

## Creation and serving

The application prepares the material a character needs. The Analyst currently receives form, mood, numeric movement, and prepared sourced memories. The Studio session builds its prompt, calls the selected model with the existing bounded correction policy, and validates the read for application publication. An empty Analyst assignment finishes without a model call or product row; the application still durably completes its claim and Oracle-barrier obligation. Invalid prose fails before publication. Direction and conviction remain deterministic measurements.

Influencer receives current packets and rendered sourced memory. Never-scored empty material publishes an uncalled NULL marker; empty material after a prior real score gets one closing quiet read. Unchanged material debounces before memory rendering and model generation, while still offering Momentum work. Latest-row/prior-memory disagreement preserves the existing buried-read exception.

Scout now creates in `studio/scout` from a prepared assignment containing only its subject, selected measurements, trajectory, sourced context, prompt, options, and deterministic product fields. `application/scout.rs` owns PostgreSQL retrieval, memory selection/fingerprints, model routing, debounce, triggers, and publication. A no-stats result atomically publishes its NULL-body marker and the normal Momentum/Oracle obligation. An unchanged assignment publishes no row and records only the Oracle check. Transfer, availability, and packet-triggered reruns still bypass that stats-only debounce. The former Scout junction and duplicate composition brief are removed.

Journalist now creates in `studio/journalist` from a prepared subject, bounded packet corpus, rendered memory/framing, input fingerprint, and options. Studio owns the brief, prompt, tolerant parser, citation grounding, deterministic impact and source metadata, and called/uncalled edition assembly. `application/journalist.rs` owns packet/Postgres retrieval, routing, debounce, queue claims, storyline progression, publication, and diagnostics. A claimed edition atomically writes every chapter or one marker, progresses the cited storyline parts, records its Oracle-barrier obligation, and completes the exact claim. The former Journalist junction and duplicate composition brief are removed.

Oracle now creates in `studio/oracle` from a subject, typed narratives/Rating/Vibe/Momentum/transfer cards, explicit spread readiness, prepared identity, deterministic input components, and application-selected options. Studio owns the brief, prompt, divergence/convergence and omen calculations, parser/guards, empty marker, and model session. `application/oracle.rs` owns PostgreSQL pillar retrieval, the existing readiness barrier (pending/running block; failed pillars permit a partial read), routing, debounce, exact-claim publication, and diagnostics. A current claim atomically writes one `sigil_synthesis` crown or marker and completes; unchanged work completes without another product; stale work publishes nothing. The former Oracle junction and duplicate brief are removed.

Insider now creates in `studio/insider` from prepared transfer candidates, article evidence, prior reads, verification material, and application-selected options. Studio owns the brief, pair and identity prompts, parsers/guards, deterministic direction/stage/confidence shaping, wrap scoring, and model sessions. `application/insider` owns PostgreSQL retrieval, routing, per-pair partial progress, identity effects, queue policy, and publication. Every current pair publication locks the exact team claim and atomically records its player Oracle barrier; final completion records the team barrier and deletes that lease. Revision supersession and same-revision reclaim fence old workers without discarding already committed pair progress. The former Insider junction and duplicate composition brief are removed.

Editor now reads in `studio/editor` from one prepared article assignment (source, title, description, body, hypothesis identities). Studio owns the unchanged `ep8` prompt, schema, parser, deterministic judgments, and model session. `application/editor` owns fetching, debounce, exact-name SQL resolution, nominations, storyline attachment, and claim-aware publication. The body/read or terminal marker, required fetch provenance, entity links, candidate evidence and Investigator work, storyline state, Graph work, and exact completion commit in one transaction after fetching and inference. Any required write failure rolls everything back for retry. Packet compilation and rendering now live in shared `evidence/news`; the Editor junction and best-effort publication wrappers are removed. Migration 233's retired boxscore enqueue path stays retired. No new migration or outbox is needed for this boundary.

Investigator now reads prepared Wikimedia prose and applies identity gates in `studio/investigator`. `application/investigator` gathers evidence and atomically publishes candidate decisions, identities, team mappings, facts, source references, attempt stamps, and exact queue completion. Source-document caching remains independent; model and network calls finish before publication begins. Stale or reclaimed work publishes nothing. Venue labels cite their own source document, and convenience fields respect fact policy. The retained fixture acquisition adapter also commits its result and required fetch ledger with exact completion; the retired enqueue trigger remains retired.

Graph now extracts from an article and its closed candidate list through `Studio::extract_graph`. Studio owns the unchanged `g5` prompt, parser, vocabulary, and model options. `application/graph` owns SQL evidence preparation, material debounce, routing, and publication. Relations, candidate-person mentions and evidence counters, extraction provenance/marker, and exact claim completion commit together. A failed required write rolls everything back; stale work writes nothing. Diagnostic logging stays best-effort after commit. All production, evaluation, and fixture callers use the new owners, and `junctions/graph` is deleted.

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
| `rust/src/studio/` | In-house harness, all nine migrated seat assignments, shared form, generation contracts, and the plugin contract (`plugin.rs`) plus fleet manifests (`fleet.rs`). |
| `rust/src/application/` | Explicit retrieval, routing, publication, and work adapters for migrated seats; each presents its manifest-backed plugin face. |
| `rust/src/runtime/` | Concrete model transports/routing, configuration, fetching, diagnostics, and exact queue operations. |
| `rust/src/evidence/`, `rust/src/evaluation/` | Sourced-memory preparation, evidence adapters, offline probes, and evaluation of the same Studio contracts. |
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

Studio's nine migrated seat creation tests, the plugin contract and fleet manifest tests, and application lifecycle tests require no service credentials. Model-quality evaluations and live database integration checks are separate gates. See [rust/README.md](rust/README.md).

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
