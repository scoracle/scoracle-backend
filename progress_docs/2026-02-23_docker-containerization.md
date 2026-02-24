# Session: Docker Containerization
**Date:** 2026-02-23

## Goals

- Containerize both the Go API and PostgREST services with lean, production-ready Dockerfiles
- Create a Docker Compose file to deploy both services simultaneously for local development
- Organize Dockerfiles using the service-directory pattern (`go/`, `postgrest/`) reflecting both services as co-equal peers
- Migrate Railway deployment from Railpack to the new Dockerfile
- Clean up artifacts made redundant by Docker integration

## Decisions Made

| Decision | Rationale |
|----------|-----------|
| Service-directory layout (`go/Dockerfile`, `postgrest/Dockerfile`) | Both services carry equal weight — PostgREST serves the core stats endpoints, Go handles news/twitter. Neither is ancillary. |
| Multi-stage Go build (build + runtime) | Final image ~15MB vs ~800MB with full toolchain. Static binary, stripped symbols, non-root user. |
| No local Postgres in Compose | Database is Neon (managed cloud). Adding a local container would add complexity without matching the real environment. |
| API-only Dockerfile (no ingestion CLI) | The ingestion tool is ad-hoc, not a long-running service. Including it would bloat the image. |
| Switch Railway from Railpack to Dockerfile builder | Single build definition used by both local development and production. No divergence. |

## Accomplishments

### Created
- `go/Dockerfile` — two-stage build: `golang:1.25-alpine` compiles static binary, `alpine:3.21` runs as non-root (UID 1000)
- `docker-compose.yml` — orchestrates both services, health-check dependency, shared `.env`, inter-container networking via service names
- `.dockerignore` — excludes Python legacy, docs, logs, secrets, `.git/` from build context

### Updated
- `postgrest/Dockerfile` — added HEALTHCHECK (TCP probe on :3000), EXPOSE declaration, comment header
- `railway.toml` — `builder = "DOCKERFILE"`, `dockerfilePath = "go/Dockerfile"`, removed `startCommand` (entrypoint handles it)
- `.env.example` — documented Compose usage, PostgREST variable mapping, `POSTGREST_URL` override for container networking

### Cleaned Up
- Deleted pre-built binaries `go/bin/scoracle-api` (31MB) and `go/bin/scoracle-ingest` (15MB) — Docker builds these now

## Quick Reference

```bash
# Start both services
docker compose up --build

# Stop
docker compose down

# Rebuild a single service
docker compose up --build api
docker compose up --build postgrest
```

## File Layout After This Session

```
go/Dockerfile              # Go API service
postgrest/Dockerfile       # PostgREST stats service
docker-compose.yml         # Orchestrates both
.dockerignore              # Build context filter
progress_docs/             # Session progress tracking
```
