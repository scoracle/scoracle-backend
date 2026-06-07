# 2026-06-07 — Mobile auth: device-identity JWT

## Goals
Build the JWT issuance/refresh endpoints the native apps need — the gating
dependency for mobile auth ([[Mobile Auth]]). Anonymous device identity (the
existing `users` UUID model — no password), short access JWT + rotating refresh
token. Code-complete + tested locally; deploy is a separate, careful step.

## Decisions
- **Anonymous device identity, not passwords.** Reuses the existing minimal
  `users` table (UUID, no PII). A user = a UUID; identity persists via the
  refresh token in the device Keychain. Future email/OAuth attaches to the same
  row without changing the JWT subject.
- **Hand-rolled HS256, no new dependency.** The JWT envelope is ~40 lines over
  stdlib `crypto/hmac` (`internal/auth`). Keeps the lean dep set; no network
  `go get`. Refresh tokens are opaque + SHA-256-hashed server-side, rotated on
  every refresh (revoke old, issue new).
- **Inline SQL, not prepared statements (deliberate).** The auth path is cold,
  and inline SQL means a missing `auth_refresh_tokens` table or a query error
  fails only `/auth/*` — never the whole API (no `AfterConnect` degraded-mode
  coupling, the trap in [[backend-api-restart-mechanics]]). Mirrors the
  inline-SQL writes in `internal/notifications/store.go`.
- **`Configured()` guard.** Without `JWT_SECRET`, `/auth/*` returns
  `503 AUTH_UNCONFIGURED`; everything else is unaffected. So the code is safe to
  ship before the secret is set.

## Accomplishments
- **Migration `042_auth_refresh_tokens.sql`** — revocable, hashed refresh-token
  store (FK to `users`, `ON DELETE CASCADE`).
- **`internal/auth/`** — `Tokens` (IssueAccess/ParseAccess HS256, NewRefreshToken/
  HashRefresh, context user-id helpers) + `auth_test.go` (roundtrip, expiry,
  tamper, wrong-secret, refresh uniqueness, unconfigured). **All pass.**
- **`internal/api/middleware.go`** — `RequireAuth(tokens)` bearer middleware →
  user id in request context.
- **`internal/api/handler/auth.go`** — `AuthDevice`, `AuthRefresh` (atomic
  rotation in a tx), `AuthRegisterPush`, `AuthLogout`, with swagger annotations.
- **`config.go`** — `JWT_SECRET`, `JWT_ACCESS_TTL_MINUTES` (30),
  `JWT_REFRESH_TTL_DAYS` (90); `.env` template updated.
- **`server.go`** — `/api/v1/auth/*` routes (public device+refresh; bearer-gated
  push+logout); CORS widened to allow `POST` + `Authorization`.
- **Docs** — `ENDPOINTS.md` auth section + impl map. Swagger regen (`swag init`)
  is a follow-up (CLI not on this host).

## Verification
`gofmt` clean · `go build ./...` OK · `go vet` OK · `go test ./...` **all pass**
(incl. `internal/api` server test; new `internal/auth` suite green).

## Deployment — done 2026-06-07 (archbox)
Ran in order, all ✅:
1. Migration applied to prod (`042_auth_refresh_tokens.sql`; `BEGIN/CREATE TABLE/CREATE INDEX/COMMIT`; table confirmed present).
2. `JWT_SECRET` generated (`openssl rand -base64 48`) → `.env.local`.
3. `cd go && go build -o bin/scoracle-api ./cmd/api`.
4. `systemctl --user restart scoracle-api.service` → `active`.
5. Smoke end-to-end against `http://localhost:8000`.

**Smoke result:** `/health/db` healthy (clean boot, not degraded) · `POST /auth/device`
→ 200 (JWT + refresh + `user_id`, `expires_in` 1800) · `POST /auth/refresh` →
rotated · bearer `POST /auth/logout` → 204 · reuse of the rotated refresh → 401 ·
logout with no bearer → 401 · `GET /api/v1/nba/meta` → 200 (**data API
unaffected** by the restart; inline SQL meant zero degraded-mode risk).

## Follow-ups
- Wire `scoracle-ios` `TokenStore` (Keychain) + refresh-on-401 / on-launch.
- `swag init` to regenerate served swagger.
- Apply `RequireAuth` to `user_follows` / `notifications` routes when they land.
