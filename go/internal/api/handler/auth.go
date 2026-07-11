package handler

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"

	"github.com/albapepper/scoracle-data/internal/api/respond"
	"github.com/albapepper/scoracle-data/internal/auth"
)

// Mobile auth — device-identity JWT. The native apps can't share the web's
// `.scoracle.com` cookie, so they authenticate with a bearer token issued here.
// See scoracle-wiki/wiki/Architecture/Mobile Auth.md.
//
// These handlers use inline SQL (not prepared statements in db.go) deliberately:
// the auth path is cold, and inline SQL means a missing table or query error
// fails only /auth/* — never the whole API (no degraded-mode coupling). Mirrors
// the inline-SQL writes in internal/notifications/store.go.

// execer is satisfied by both *pgxpool.Pool and pgx.Tx, so issuePair can run on
// a plain connection or inside a transaction.
type execer interface {
	Exec(ctx context.Context, sql string, args ...any) (pgconn.CommandTag, error)
}

// tokenResponse is the issued token pair returned to a mobile client.
type tokenResponse struct {
	AccessToken  string `json:"access_token"`
	RefreshToken string `json:"refresh_token"`
	UserID       string `json:"user_id,omitempty"`
	ExpiresIn    int    `json:"expires_in"` // access-token lifetime, seconds
}

// authReady guards the auth endpoints when no signing secret is configured.
// The rest of the API is fine when mobile auth is unavailable.
func (h *Handler) authReady(w http.ResponseWriter) bool {
	if h.auth == nil || !h.auth.Configured() {
		respond.WriteError(w, http.StatusServiceUnavailable, "AUTH_UNCONFIGURED",
			"Auth is not configured. Set JWT_SECRET.")
		return false
	}
	return true
}

// issuePair generates a refresh token, stores its hash via q (a pool or a
// transaction), and issues an access token.
func (h *Handler) issuePair(ctx context.Context, q execer, userID string) (tokenResponse, error) {
	raw, hash, err := h.auth.NewRefreshToken()
	if err != nil {
		return tokenResponse{}, err
	}
	expiresAt := time.Now().Add(h.auth.RefreshTTL())
	if _, err := q.Exec(ctx,
		`INSERT INTO auth_refresh_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3)`,
		userID, hash, expiresAt); err != nil {
		return tokenResponse{}, err
	}
	access, err := h.auth.IssueAccess(userID)
	if err != nil {
		return tokenResponse{}, err
	}
	return tokenResponse{
		AccessToken:  access,
		RefreshToken: raw,
		ExpiresIn:    int(h.auth.AccessTTL().Seconds()),
	}, nil
}

// AuthDevice creates a new anonymous user and returns the first token pair.
// @Summary Register device (anonymous)
// @Description Creates a new anonymous user and returns an access + refresh token pair. First launch only; later launches use /auth/refresh.
// @Tags auth
// @Accept json
// @Produce json
// @Success 200 {object} map[string]interface{}
// @Failure 503 {object} respond.ErrorResponse
// @Router /api/v1/auth/device [post]
func (h *Handler) AuthDevice(w http.ResponseWriter, r *http.Request) {
	if !h.authReady(w) {
		return
	}
	ctx := r.Context()

	var userID string
	if err := h.pool.QueryRow(ctx, `INSERT INTO users DEFAULT VALUES RETURNING id`).Scan(&userID); err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "Could not create user")
		return
	}

	pair, err := h.issuePair(ctx, h.pool, userID)
	if err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "TOKEN_ERROR", "Could not issue tokens")
		return
	}
	pair.UserID = userID
	respond.WriteJSONObject(w, http.StatusOK, pair)
}

// AuthRefresh validates a refresh token, rotates it, and returns a new pair.
// @Summary Refresh tokens
// @Description Validates a refresh token, rotates it (the old one is revoked), and returns a new access + refresh pair. 401 if expired/revoked/unknown.
// @Tags auth
// @Accept json
// @Produce json
// @Success 200 {object} map[string]interface{}
// @Failure 401 {object} respond.ErrorResponse
// @Router /api/v1/auth/refresh [post]
func (h *Handler) AuthRefresh(w http.ResponseWriter, r *http.Request) {
	if !h.authReady(w) {
		return
	}
	ctx := r.Context()

	var body struct {
		RefreshToken string `json:"refresh_token"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.RefreshToken == "" {
		respond.WriteError(w, http.StatusBadRequest, "BAD_REQUEST", "refresh_token required")
		return
	}
	hash := auth.HashRefresh(body.RefreshToken)

	var (
		userID    string
		expiresAt time.Time
		revokedAt *time.Time
	)
	err := h.pool.QueryRow(ctx,
		`SELECT user_id, expires_at, revoked_at FROM auth_refresh_tokens WHERE token_hash = $1`,
		hash).Scan(&userID, &expiresAt, &revokedAt)
	if errors.Is(err, pgx.ErrNoRows) {
		respond.WriteError(w, http.StatusUnauthorized, "UNAUTHORIZED", "Invalid refresh token")
		return
	}
	if err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "Refresh lookup failed")
		return
	}
	if revokedAt != nil || time.Now().After(expiresAt) {
		respond.WriteError(w, http.StatusUnauthorized, "UNAUTHORIZED", "Refresh token expired or revoked")
		return
	}

	// Rotate atomically: revoke the presented token, then issue a new pair.
	tx, err := h.pool.Begin(ctx)
	if err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "tx begin failed")
		return
	}
	defer tx.Rollback(ctx)

	if _, err := tx.Exec(ctx,
		`UPDATE auth_refresh_tokens SET revoked_at = NOW(), last_used_at = NOW() WHERE token_hash = $1`,
		hash); err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "Could not rotate token")
		return
	}
	pair, err := h.issuePair(ctx, tx, userID)
	if err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "TOKEN_ERROR", "Could not issue tokens")
		return
	}
	if err := tx.Commit(ctx); err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "commit failed")
		return
	}
	respond.WriteJSONObject(w, http.StatusOK, pair)
}

// AuthRegisterPush upserts an APNs/FCM device token for the authenticated user.
// @Summary Register push token
// @Description Registers (or reactivates) the device's push token for the authenticated user. Requires a bearer token.
// @Tags auth
// @Accept json
// @Produce json
// @Success 204
// @Failure 401 {object} respond.ErrorResponse
// @Security BearerAuth
// @Router /api/v1/auth/device/push [post]
func (h *Handler) AuthRegisterPush(w http.ResponseWriter, r *http.Request) {
	userID, ok := auth.UserIDFromContext(r.Context())
	if !ok {
		respond.WriteError(w, http.StatusUnauthorized, "UNAUTHORIZED", "auth required")
		return
	}
	var body struct {
		Token    string `json:"token"`
		Platform string `json:"platform"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.Token == "" {
		respond.WriteError(w, http.StatusBadRequest, "BAD_REQUEST", "token required")
		return
	}
	switch body.Platform {
	case "ios", "android", "web":
	default:
		respond.WriteError(w, http.StatusBadRequest, "BAD_REQUEST", "platform must be ios, android, or web")
		return
	}
	if _, err := h.pool.Exec(r.Context(),
		`INSERT INTO user_devices (user_id, platform, token) VALUES ($1, $2, $3)
		 ON CONFLICT (user_id, token) DO UPDATE SET is_active = true`,
		userID, body.Platform, body.Token); err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "device registration failed")
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// AuthLogout revokes the presented refresh token for the authenticated user.
// @Summary Log out
// @Description Revokes the given refresh token. Requires a bearer token.
// @Tags auth
// @Accept json
// @Produce json
// @Success 204
// @Failure 401 {object} respond.ErrorResponse
// @Security BearerAuth
// @Router /api/v1/auth/logout [post]
func (h *Handler) AuthLogout(w http.ResponseWriter, r *http.Request) {
	userID, ok := auth.UserIDFromContext(r.Context())
	if !ok {
		respond.WriteError(w, http.StatusUnauthorized, "UNAUTHORIZED", "auth required")
		return
	}
	var body struct {
		RefreshToken string `json:"refresh_token"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.RefreshToken == "" {
		respond.WriteError(w, http.StatusBadRequest, "BAD_REQUEST", "refresh_token required")
		return
	}
	if _, err := h.pool.Exec(r.Context(),
		`UPDATE auth_refresh_tokens SET revoked_at = NOW() WHERE token_hash = $1 AND user_id = $2`,
		auth.HashRefresh(body.RefreshToken), userID); err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "logout failed")
		return
	}
	w.WriteHeader(http.StatusNoContent)
}
