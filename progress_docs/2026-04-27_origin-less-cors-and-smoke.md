# Origin-less CORS allowance + no-Origin smoke probe

## Goal

Document and explicitly permit origin-less requests at the Go CORS layer, and add a smoke test that catches the case where they fail. Driven by the SolidStart frontend adopting `"use server"` / `query()` for SSR-side fetches: the frontend worker calls the API without a browser Origin header, and that needs to keep working through any future tightening of the rs/cors library default or the api.scoracle.com Cloudflare zone config.

## Decisions

- The rs/cors library already permits Origin-less requests by default — but stating it explicitly via `AllowOriginFunc` documents the intent and survives future library behavior changes. Cheap insurance.
- When `Origin == ""` (server-to-server), allow. When `Origin` is present, use the existing whitelist (`cfg.CORSAllowOrigins`).
- The 403 the frontend hit during SSR-fetch attempts is **not** the Go server — grep shows zero `403`/`StatusForbidden` emissions in the codebase. The block lives at the api.scoracle.com Cloudflare zone (WAF or Bot Fight). That's a dashboard-side fix; this commit just makes sure the Go server itself isn't accidentally introducing a new 403 mode in the future.

## Accomplishments

- `go/internal/api/server.go:36-58` — `AllowOriginFunc` added to the `corslib.Options`. Origin-less requests pass; origin-bearing requests must match the configured allowlist.
- `scripts/hosting/tunnel-smoke.sh:153+` — new `check_origin_less` helper. Curls the endpoint with no `Origin` header and a generic User-Agent, asserts 200. A 403 is flagged as "likely WAF/Bot-Fight on the api zone."
- Smoke run section adds two probes: `GET /api/v1/nba/meta` and `GET /health/db`, both no-Origin. Catches a regression on either the Go side (this commit's safeguard) or the Cloudflare zone (the actual current pain point).

## Verification

- `go build ./...` — green.
- `go vet ./...` — clean.
- `bash -n scripts/hosting/tunnel-smoke.sh` — script syntax OK.
- Manual smoke once the Cloudflare WAF rule is identified and exempted: `bash scripts/hosting/tunnel-smoke.sh https://api.scoracle.com` should now exercise the origin-less path and report pass.

## Why now

The SolidStart frontend's Tier 1 migration (`query()` / `createAsync()` / `"use server"` adoption per `~/scoracle-frontend/docs/audits/2026-04-26_pre-cutover-audit-findings.md` follow-up) needs server-side fetches to succeed. This commit doesn't fix the live 403 (that's a Cloudflare dashboard step), but it locks in that the Go server explicitly accepts the origin-less call shape and that future regressions on either side are caught by the smoke run.
