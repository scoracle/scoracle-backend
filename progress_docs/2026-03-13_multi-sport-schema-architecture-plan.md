# Session: Multi-Sport Schema Architecture Plan
**Date:** 2026-03-13

## Goals
- Define a scalable database and API structure for supporting more sports over time
- Reduce the maintenance burden of the current monolithic `schema.sql`
- Preserve the strengths of the current provider-agnostic, Postgres-centered design
- Clarify when to keep one shared database versus when to split by sport
- Propose a practical rollout path with low disruption to ingestion, PostgREST, and frontend consumers

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| Keep a single Neon database as the default next step | The current platform has strong shared concerns across sports: auth, follows, notifications, search/autofill, and a common PostgREST surface. Splitting databases now would increase operational complexity before there is a clear payoff. |
| Stop treating one giant `schema.sql` as the human authoring format | The main scaling problem is maintainability of the schema source, not the existence of one database. A 1500+ line file with shared tables, sport-specific logic, API views, RLS, and notifications is too broad a blast radius for ongoing growth. |
| Organize SQL by domain and by sport, while preserving one public `api` schema | This keeps PostgREST stable for consumers while letting internals evolve cleanly. |
| Keep shared canonical entity tables for now | `players`, `teams`, `player_stats`, and `team_stats` still fit NBA, NFL, and football well enough. They support the provider-agnostic ingestion model and keep cross-sport platform features simple. |
| Use sport-specific Postgres functions and helper views instead of expanding global branching | Adding new sports should not require threading more `CASE WHEN sport = ...` logic throughout shared functions and materialized views. |
| Introduce sport-scoped endpoint organization at the API layer | Even with one DB, sport-specific URL structure will make frontend use clearer and reduce reliance on `sport` query params for everything. |
| Defer separate databases per sport until a concrete trigger appears | Separate DBs only make sense once there is a real need for operational isolation, independent scaling, or radically different data models. |

## Accomplishments
### Created
- `progress_docs/2026-03-13_multi-sport-schema-architecture-plan.md` with a detailed planning document covering database, schema, and endpoint evolution
- A target architecture for modular SQL authoring while preserving one public PostgREST contract
- A phased rollout plan for reorganizing schema ownership without immediately splitting databases

### Updated
- No application code or schema objects were modified in this session
- The plan is designed to inform future changes to `schema.sql`, DB deployment workflow, PostgREST objects, and sport onboarding flow

### Cleaned Up
- Clarified that the immediate problem is schema authoring and growth management, not the existence of one shared database
- Clarified that endpoint separation and database separation are independent decisions and do not need to happen together

## Recommended Target Architecture

### 1. Database Strategy
Keep one Neon PostgreSQL database as the default platform database.

Use that database for:
- shared sports catalog and league metadata
- shared canonical entities and seasonal stats
- shared PostgREST API schema
- shared user and platform features such as follows, devices, and notifications
- shared operational metadata like fixtures, sync state, and provider season mappings

Do not move to one database per sport yet.

### 2. Postgres Schema Layout
Move from one broad `public`-heavy layout to a clearer internal separation.

Recommended logical layout:
- `core` - shared sports data model
- `platform` - user and notification features
- `nba` - NBA-specific functions and helpers
- `nfl` - NFL-specific functions and helpers
- `football` - football-specific functions and helpers
- `api` - public PostgREST-facing views and RPCs only

#### `core` schema
Owns shared tables:
- `core.sports`
- `core.leagues`
- `core.players`
- `core.teams`
- `core.player_stats`
- `core.team_stats`
- `core.fixtures`
- `core.provider_seasons`
- `core.stat_definitions`
- `core.percentile_archive`
- `core.meta`

Purpose:
- central storage for all cross-sport canonical records
- shared foreign key relationships
- shared indexes
- low-friction support for provider-agnostic ingestion

#### `platform` schema
Owns platform product features:
- `platform.users`
- `platform.user_follows`
- `platform.user_devices`
- `platform.notifications`

Purpose:
- keep user concerns distinct from sports-data concerns
- isolate RLS-heavy application tables from sports ingestion tables

#### Sport schemas
Own sport-specific logic:
- `nba.compute_derived_player_stats()`
- `nba.compute_derived_team_stats()`
- `nfl.compute_derived_player_stats()`
- `nfl.compute_derived_team_stats()`
- `football.compute_derived_player_stats()`
- `football.compute_derived_team_stats()` if needed
- sport-specific standings helpers
- sport-specific stat leader helpers if needed
- sport-specific materialized-view sources if needed

Purpose:
- reduce branching in shared SQL
- let each sport own its own derived metrics and ranking semantics
- make adding a new sport a bounded change

#### `api` schema
Remain the public contract exposed by PostgREST:
- `api.players`
- `api.teams`
- `api.player_stats`
- `api.team_stats`
- `api.standings`
- `api.stat_definitions`
- `api.leagues`
- `api.sports`
- `api.autofill_entities`
- `api.stat_leaders(...)`
- `api.health()`
- authenticated user-facing views like `api.my_follows`

Purpose:
- stable external API
- internal freedom to refactor underlying tables and functions
- clean separation between public and private DB objects

## SQL Source Organization Plan

### Problem
The current `schema.sql` mixes:
- tables
- seed data
- views
- derived-stat functions
- triggers
- API response functions
- materialized views
- auth roles
- grants
- RLS
- notifications

That makes review and safe evolution hard.

### Recommendation
Split SQL source files by responsibility, while preserving a generated or assembled deployment artifact if desired.

Suggested layout:

```text
sql/
  00_extensions.sql
  01_core_tables.sql
  02_core_seed_data.sql
  03_platform_tables.sql
  10_nba_stats.sql
  11_nfl_stats.sql
  12_football_stats.sql
  20_nba_logic.sql
  21_nfl_logic.sql
  22_football_logic.sql
  30_shared_views.sql
  31_materialized_views.sql
  40_api_views.sql
  41_api_functions.sql
  50_roles_and_grants.sql
  51_rls.sql
  60_notifications.sql
  99_bootstrap.sql
```

### File ownership model
- Core and shared data definitions live in `01` and `02`
- Sport-specific stat definitions and derived logic live in `10` through `22`
- Public API surface lives in `40` and `41`
- Security concerns live in `50` and `51`
- Notification and listener concerns live in `60`

### Deployment model
Use a deterministic ordered apply step:
- local bootstrap script
- CI validation script
- deploy script that assembles or applies files in order

If desired, still produce a root `schema.sql` artifact for:
- fresh environment bootstrap
- disaster recovery
- portability

That artifact should be generated, not hand-maintained.

## Data Model Direction

### Keep the current shared canonical tables
Retain the current coarse model because it still matches your active sports:
- players
- teams
- player seasonal stats
- team seasonal stats
- fixtures
- leagues
- provider season mappings

This fits NBA, NFL, and football well enough.

### Keep JSONB for sport-specific stats
Continue storing `stats`, `percentiles`, and `meta` as JSONB.

Why:
- preserves provider-agnostic ingestion
- avoids schema churn when adding stat keys
- keeps new sport onboarding cheaper

But tighten governance:
- every stat key must exist in `core.stat_definitions`
- hot query paths should get explicit expression indexes
- sport-specific derived keys should be declared and documented alongside stat definitions

### Reduce sentinel semantics
The current `league_id = 0` pattern for NBA and NFL works, but it is a long-term smell.

Recommended direction:
- use nullable `league_id` for single-league sports, or
- create canonical league rows for NBA and NFL as first-class records

Preferred option:
- create canonical league rows for all sports, even single-league sports

Why:
- removes the `0` sentinel special case
- makes API and filtering behavior more consistent
- simplifies future multi-league expansion for any sport
- reduces code paths that treat football differently for league semantics

### Keep shared stat registry, but modularize its ownership
Preserve one canonical `stat_definitions` table, but source it from sport-specific SQL files.

Recommended shape:
- one shared table in `core`
- separate seed blocks or inserts per sport
- optional metadata additions over time:
  - `comparison_group`
  - `display_group`
  - `unit`
  - `decimal_precision`
  - `supports_percentiles`
  - `default_sort_direction`

This table should remain the authoritative registry for:
- stat naming
- display labels
- category grouping
- percentile eligibility
- inverse-score behavior

## API Structure Plan

### Current strength to preserve
The split between:
- PostgREST for core data
- Go for third-party integrations

is correct and should remain.

### Recommended endpoint organization
Introduce sport-scoped API organization without requiring separate deployments.

Preferred shape:
- `/nba/players`
- `/nba/teams`
- `/nba/player-stats`
- `/nba/team-stats`
- `/nba/standings`
- `/nfl/...`
- `/football/...`

There are two ways to support this:

#### Option A: Sport-scoped views in `api`
Examples:
- `api.nba_players`
- `api.nba_team_stats`
- `api.nfl_players`
- `api.football_standings`

Pros:
- very explicit
- frontend-friendly
- less repetitive query filtering by `sport`

Cons:
- more objects in `api`

#### Option B: Generic views plus frontend or gateway routing convention
Keep:
- `api.players`
- `api.teams`
- `api.player_stats`

And expose sport-specific routes via a gateway or convention that pre-applies `sport` filters.

Pros:
- fewer DB objects
- less duplication

Cons:
- sport filtering remains implicit in many requests

Recommendation:
- start with generic DB views
- add sport-scoped API wrappers only for the highest-traffic or most sport-specific endpoints

### Standings and leaders
Move sport-specific ranking semantics out of one shared branching function where practical.

Recommended direction:
- keep a shared entrypoint in `api`
- delegate internally to sport-specific helper functions
- let each sport define:
  - ordering rules
  - tie-breaks
  - required stat keys
  - comparison groups

This avoids a future giant `fn_standings()` with many sport branches.

## New Sport Onboarding Model

### Goal
Adding a new sport should become a bounded checklist, not a repo-wide hunt.

### Target onboarding workflow
1. Register the sport in `core.sports`
2. Add canonical league rows if applicable
3. Add the sport-specific stat definitions file
4. Add the derived-stat function file only if needed
5. Add standings and leader helper functions if needed
6. Add a provider package under `go/internal/provider/`
7. Add a seed runner under `go/internal/seed/`
8. Add CLI command wiring in `go/cmd/ingest/main.go`
9. Add public API exposure in `api`
10. Add smoke tests for views, functions, and seed flow

### Result
The change surface becomes localized and predictable.

## When To Revisit Separate Databases

### Stay with one DB while:
- sports still fit the shared canonical entity model
- cross-sport product features remain important
- operational scale is manageable on one Neon instance
- engineering velocity benefits from one shared dataset

### Reconsider separate DBs if one or more of these become true:
- one sport has drastically different data primitives than team, player, and season stats
- one sport needs materially different scaling, retention, or maintenance windows
- one sport becomes operationally independent with its own deploy cadence
- noisy workloads in one sport materially degrade others
- compliance, tenancy, or business ownership requires hard isolation

### If separate DBs are ever introduced
Use a split platform model:
- one platform DB for users, follows, devices, notifications, and maybe search metadata
- one sports DB per major sport or sport family

That would be a second-phase architecture, not the recommended next step.

## Phased Rollout Plan

### Phase 0 - Planning and Safety
- Inventory all objects in current `schema.sql`
- Classify each object into core, platform, sport-specific, api, or security
- Identify all prepared statements and API dependencies in Go
- Document every place where sport branching exists in SQL and Go
- Decide whether to normalize `league_id` by creating canonical league rows for NBA and NFL

Exit criteria:
- full dependency map
- file split plan approved
- public API compatibility rules defined

### Phase 1 - Source Reorganization Only
- Split SQL into modular files with deterministic apply order
- Keep runtime object names the same initially
- Keep one DB and one exposed `api` schema
- Add a schema validation or bootstrap script for local and CI use

Exit criteria:
- same database objects as today
- no API behavior change
- easier review and safer iteration

### Phase 2 - Internal Schema Separation
- Move private objects into `core`, `platform`, and sport-specific schemas
- Keep compatibility wrappers or views where needed during transition
- Update grants and RLS carefully
- Update any prepared statements or function references in Go

Exit criteria:
- public API unchanged
- internal object ownership clarified
- sport-specific logic isolated

### Phase 3 - API Surface Cleanup
- Decide where sport-scoped endpoints materially improve frontend ergonomics
- Add sport-specific views or wrappers for high-value endpoints
- Reduce over-reliance on generic `sport` query params where clearer URL structure helps

Exit criteria:
- cleaner consumer API
- no ambiguity around sport-specific data access
- backward compatibility plan communicated if routes change

### Phase 4 - Data Model Hardening
- Normalize league handling across all sports
- Add targeted indexes for hot JSONB stat keys
- Refine standings and percentile helpers into clearer sport-owned logic
- Optimize materialized views and refresh strategy as sports expand

Exit criteria:
- fewer sentinel and special-case branches
- better query performance predictability
- clearer new-sport onboarding path

### Phase 5 - Reevaluate Isolation Strategy
- Assess whether one or more sports warrant separate databases or isolated services
- Only proceed if scale, ownership, or reliability pressure justifies it

Exit criteria:
- decision driven by observed bottlenecks, not premature separation

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Breaking PostgREST consumers during refactor | Keep `api` as the stable contract and introduce compatibility views or functions where needed. |
| Security regressions in roles, grants, or RLS | Isolate security SQL into dedicated files, validate grants in staging, and test authenticated flows explicitly. |
| Hidden dependencies in Go prepared statements | Inventory every statement in `go/internal/db/db.go` and map each to its underlying SQL object before moving anything. |
| Schema split creates deployment ordering issues | Use strict file ordering and a single bootstrap or apply command in CI and local dev. |
| More sports create more branching again | Push sport-specific rules into sport-owned functions and files instead of shared global functions. |
| JSONB queries slow down as data grows | Add targeted expression indexes and only index stats that back important filters and sorts. |
| Transition takes too long and stalls product work | Start with source reorganization only, which has the highest maintainability return for the lowest migration risk. |

## Proposed Success Metrics
- A new sport can be added by touching a bounded set of sport-specific files
- SQL review surface is smaller and more understandable
- PostgREST public objects remain stable during internal refactors
- League handling no longer relies on the `0` sentinel pattern
- Shared platform features remain cross-sport and easy to reason about
- Query performance remains stable or improves on top endpoints
- Internal SQL branching by sport decreases over time rather than grows

## Quick Reference
- Keep one Neon DB
- Keep PostgREST for core sports data
- Keep Go for third-party integrations and ingestion orchestration
- Split SQL source into modular files
- Move private DB objects into `core`, `platform`, and per-sport schemas
- Preserve `api` as the only public PostgREST surface
- Normalize league handling across sports
- Reevaluate separate DBs only after concrete scale or isolation pressure appears

## File Layout After This Session
```text
recommended target:
sql/
  00_extensions.sql
  01_core_tables.sql
  02_core_seed_data.sql
  03_platform_tables.sql
  10_nba_stats.sql
  11_nfl_stats.sql
  12_football_stats.sql
  20_nba_logic.sql
  21_nfl_logic.sql
  22_football_logic.sql
  30_shared_views.sql
  31_materialized_views.sql
  40_api_views.sql
  41_api_functions.sql
  50_roles_and_grants.sql
  51_rls.sql
  60_notifications.sql
  99_bootstrap.sql

postgres schemas:
  core/
  platform/
  nba/
  nfl/
  football/
  api/
```
