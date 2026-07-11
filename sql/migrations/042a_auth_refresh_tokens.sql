-- ============================================================================
-- 042a_auth_refresh_tokens.sql  (renamed from 042_ — was a duplicate 042 number
-- alongside 042_rating_modes.sql; both already applied. See migration 051.)
-- Mobile auth (device-identity JWT). The Go API issues a short-lived access JWT
-- + a long-lived, ROTATING refresh token against the existing anonymous `users`
-- model (no password). Only the SHA-256 hash of the refresh token is stored, so
-- a DB leak can't mint tokens; rows are revocable (rotation on refresh + logout).
-- See scoracle-wiki/wiki/Architecture/Mobile Auth.md.
--
-- DEPLOY ORDER: apply this migration BEFORE restarting the API. (The auth
-- handlers use inline SQL, so a missing table fails only /auth/* — not the whole
-- API — but the table must exist for auth to work.)
-- ============================================================================

BEGIN;

CREATE TABLE IF NOT EXISTS auth_refresh_tokens (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash   TEXT NOT NULL UNIQUE,            -- SHA-256 hex of the opaque token
    expires_at   TIMESTAMPTZ NOT NULL,
    revoked_at   TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_auth_refresh_user ON auth_refresh_tokens(user_id);

COMMIT;
