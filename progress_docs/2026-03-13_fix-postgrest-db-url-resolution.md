# Session: Fix PostgREST DB URL Resolution
**Date:** 2026-03-13

## Goals
- Diagnose why PostgREST health checks fail after changing the database connection string
- Implement a clean, long-term fix so future connection string changes work for both services

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| Add entrypoint.sh with same fallback chain as Go API | Single source of truth for DB URL resolution — both services now follow the same priority: `NEON_DATABASE_URL_V2` > `DATABASE_URL` > `NEON_DATABASE_URL` |
| Forward all three env vars in docker-compose instead of mapping one | Lets entrypoint.sh handle resolution consistently across local dev and Railway |
| Keep `PGRST_DB_URI` as an explicit override | If someone sets it directly (e.g., in Railway dashboard), it takes top priority — no surprise behavior |

## Root Cause
The Go API (`config.go`) resolves the DB URL through a 3-level fallback chain (`NEON_DATABASE_URL_V2` > `DATABASE_URL` > `NEON_DATABASE_URL`), but `docker-compose.yml` hardcoded PostgREST's `PGRST_DB_URI` to only `${DATABASE_URL}`. When the connection string lived in a higher-priority variable (e.g., `NEON_DATABASE_URL_V2`), the Go API connected fine while PostgREST received an empty/stale URL and failed permanently.

## Accomplishments
### Created
- `postgrest/entrypoint.sh` — shell wrapper that resolves the DB URL using the same priority chain as the Go API before exec-ing PostgREST

### Updated
- `postgrest/Dockerfile` — switched `ENTRYPOINT` from `/bin/postgrest` to `/bin/entrypoint.sh`
- `docker-compose.yml` — forwards all three DB URL env vars instead of mapping only `DATABASE_URL`
- `.env.example` — updated PostgREST and Docker Compose sections to document the shared resolution chain
