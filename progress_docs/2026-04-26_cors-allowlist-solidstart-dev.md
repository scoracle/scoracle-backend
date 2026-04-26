# CORS allowlist update — SolidStart flagship dev port

## Goal

Add the SolidStart flagship dev server's port to the default
`CORSAllowOrigins` list so future dev paths that bypass the new
Vite proxy still work without an env-var override.

## Decisions

The greenfield `scoracle-frontend` (SolidStart 2.0-alpha) runs its
Vite dev server on `:5173` by default, but Vite increments the port
when 5173 is occupied. Observed live: dev settled on `:5185` while
testing the profile page. The first browser test failed on
CORS because neither Vite shift was in the backend allowlist.

The frontend now ships a Vite dev proxy
(`scoracle-frontend@4fd7fa1`) that side-steps CORS entirely in dev,
so the backend allowlist is belt-and-suspenders — but worth keeping
accurate so any future dev path that talks to `:8000` directly
(e.g., a curl from a browser tab in DevTools, or a sandbox dev
server pointed at the same backend) doesn't trip CORS.

Comment-tagged each entry with what it serves. Astro's `:4321` stays
in the list until DNS cutover; once the legacy frontend is retired,
that line can be dropped.

## Accomplishments

- `go/internal/config/config.go` default `CORSAllowOrigins`:
  - `:3000` (kept)
  - `:4321` — Astro flagship dev (legacy, retires at DNS cutover)
  - `:5173` — Vite default (SolidStart flagship dev)
  - `:5185` — Vite fallback when 5173 is occupied (observed in scoracle-frontend dev)

`CORS_ALLOW_ORIGINS` env var still overrides the default if set, so
prod-like environments are unchanged.

## Verification

- `go build ./...` — green.
- `go test ./internal/config/...` — passes (test only exercises explicit
  env-var override, not defaults; new entries don't affect coverage).

## Quick reference

```bash
# Default allowlist (no env var):
http://localhost:3000
http://localhost:4321  # Astro flagship dev
http://localhost:5173  # SolidStart flagship dev (Vite default)
http://localhost:5185  # SolidStart flagship dev (Vite fallback)

# Override:
CORS_ALLOW_ORIGINS=https://staging.scoracle.com go run ./cmd/scoracle-api
```
