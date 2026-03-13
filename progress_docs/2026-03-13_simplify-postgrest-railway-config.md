# Session: Simplify PostgREST Railway Config
**Date:** 2026-03-13

## Goals
- Reduce PostgREST deploy config complexity while preserving the current Railway setup
- Remove shell indirection from the container startup path
- Make the healthcheck target easier to reason about during deploy debugging

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| Keep the custom Alpine-based image | The official PostgREST image is scratch-based, while this deploy depends on a non-root user and runtime libs on Railway |
| Set `PGRST_SERVER_PORT=3000` as an image env var | This avoids shell-based startup logic and keeps PostgREST on a fixed, explicit port |
| Use `/` as the Railway healthcheck path | The root OpenAPI endpoint is the simplest built-in 200 response to probe |
| Drop the explicit `dockerfilePath` from `postgrest/railway.toml` | `Dockerfile` is already the default, so the extra setting was redundant |

## Accomplishments
### Updated
- `postgrest/Dockerfile` — simplified startup by removing the shell wrapper, baking `PGRST_SERVER_PORT=3000` into the image, and using `ENTRYPOINT ["/bin/postgrest"]`
- `postgrest/railway.toml` — kept the healthcheck on `/` and removed the redundant `dockerfilePath` setting

## Quick Reference
- PostgREST healthcheck path: `/`
- PostgREST listen port in image: `3000`

## File Layout After This Session
- `postgrest/Dockerfile`
- `postgrest/railway.toml`
- `progress_docs/2026-03-13_simplify-postgrest-railway-config.md`
