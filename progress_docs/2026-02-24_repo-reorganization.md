# Session: Repo Reorganization & README Overhaul
**Date:** 2026-02-24

## Goals

- Reorganize the repo root to clearly separate concerns
- Move legacy Python files to `legacy_fastapi/` for reference during Go migration
- Move `ARCHITECTURE-API.md` to `planning_docs/` where it belongs
- Clean up stale local artifacts
- Update `README.md` to reflect Docker integration, service roles, and current repo structure

## Decisions Made

| Decision | Rationale |
|----------|-----------|
| Keep `go.mod`, `cmd/`, `internal/` at repo root | Official Go documentation recommends this layout for server projects. Moving them into `go/` would break 52 import paths across 20 files and fight Go tooling conventions. |
| Keep `go/` directory for Dockerfile only | Peer-service directory pattern (`go/Dockerfile`, `postgrest/Dockerfile`) is the standard approach for co-equal services in a multi-service repo. No conflict with Go module layout. |
| Create `legacy_fastapi/` instead of deleting Python | Python code serves as reference material during the ongoing Go migration. Preserves context for feature parity work. |
| Clean stale artifacts locally, not via git | `seed_2025.log` and `.pytest_cache/` were already gitignored. Deleted locally, added `.pytest_cache/` to `.gitignore` for completeness. |

## Accomplishments

### Repo Reorganization
- Created `legacy_fastapi/` and moved `python/`, `pyproject.toml`, `requirements.txt`, `uv.lock` into it
- Moved `ARCHITECTURE-API.md` to `planning_docs/ARCHITECTURE-API.md`
- Deleted local stale artifacts (`seed_2025.log`, `.pytest_cache/`)
- Added `.pytest_cache/` to `.gitignore`

### README Overhaul
- Rewrote architecture diagram to show the dual-service split (Go + PostgREST)
- Documented Docker Compose as the primary local development workflow
- Clarified service roles: PostgREST as data provider, Go as third-party integrations provider
- Added multi-spec Swagger UI documentation
- Updated codebase structure to reflect new directory layout
- Replaced Railpack deployment section with Dockerfile-based deployment
- Removed outdated Python CLI section

## Repo Root After This Session

```
cmd/                       # Go entry points
internal/                  # Go packages
docs/                      # Swagger specs
go/                        # Go service Dockerfile
postgrest/                 # PostgREST service Dockerfile + passwd
legacy_fastapi/            # Python reference code
planning_docs/             # Design and architecture docs
progress_docs/             # Session progress tracking
docker-compose.yml
.dockerignore
.env.example
.gitignore
go.mod
go.sum
railway.toml
schema.sql
API_KEYS.md
README.md
```
