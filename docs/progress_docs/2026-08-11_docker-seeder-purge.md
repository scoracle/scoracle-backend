# Docker + Python Seeder Purge

**Date:** 2026-08-11

## Overview

Purged all Docker dependencies and the Python seeder layer from the scoracle-backend repository, transitioning to a native self-hosted setup.

## Changes

### Docker Removed
- `docker-compose.yml` — local dev stack; prod runs natively per `RUNBOOK.md`
- `go/Dockerfile` — multi-stage Go build for Docker; replaced with native `go build`
- `seed/Dockerfile` — Python seeder Docker image
- `.dockerignore` — root-level Docker ignore file

### Python Seeder Removed — Entire `seed/` directory
- Core: `pyproject.toml`, `Dockerfile`, `.dockerignore`, `SEEDING_INSTRUCTIONS.md`
- CLI: `scoracle_seed/cli.py`
- Services: `services/event/`, `services/meta/`, `services/roster/`
- Shared: 15 utility modules (bdl_client, sportmonks_client, db, models, config, etc.)
- Tests: 12 pytest test files

### CI Updated (`.github/workflows/ci.yml`)
- Removed `python` job (pytest suite for seeder)
- Removed `docker` job (`docker build go/`)
- Remaining jobs: `go`, `shell`, `schema`

### Documentation Updated
- `README.md`: Removed Docker Compose Quick Start, Python seeder references from Architecture overview, Service Responsibilities table, Repo Layout
- `docs/DEVELOPMENT.md`: Removed Python Style section (lines 130-143), `seed/` from Key Files
- `RUNBOOK.md`: Updated F-043 note about seed/Dockerfile
- `go/internal/buildinfo/buildinfo.go`: Removed `go/Dockerfile` build arg reference
- `sql/platform.sql`: Cleaned up Dockerfile config comment

## Result

Production runs natively on archbox with zero Docker dependency:
- **Go API** (`scoracle-api`) — `go build ./cmd/api` + `systemd --user`
- **Rust cognition** (`scoracle-cognition`, `statcommentary`) — native binaries + systemd
- **Postgres 18** — system service
- **Ollama** — native, no container
- **Cron jobs** — system crontab
- **Cloudflare Tunnel** — `cloudflared` native binary

All 5 release binaries built/stamped by `scripts/hosting/release.sh`.

## Files Count
- Deleted: 17 files directly + entire `seed/` directory (42+ files)
- Modified: 7 documentation/code files