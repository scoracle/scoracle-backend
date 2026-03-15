# Session: Add Accept-Profile to PostgREST CORS
**Date:** 2026-03-15

## Goals
- Fix browser preflight (OPTIONS) failures when the frontend sends `Accept-Profile` to select a sport schema via PostgREST

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| Set `PGRST_SERVER_CORS_ALLOWED_HEADERS` explicitly in Dockerfile | PostgREST's default CORS allowed headers don't include `Accept-Profile` or `Content-Profile`, which are required for multi-schema mode |
| Include `Content-Profile` alongside `Accept-Profile` | `Content-Profile` is used for write operations targeting a specific schema; including it now avoids a future breakage |
| Keep standard headers (Accept, Content-Type, Authorization, Accept-Language) | These are commonly needed and match PostgREST defaults |

## Accomplishments
### Updated
- `postgrest/Dockerfile` — added `PGRST_SERVER_CORS_ALLOWED_HEADERS` env var with `Accept-Profile` and `Content-Profile` included
