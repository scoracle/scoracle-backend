// Package auth issues and verifies the native mobile tokens. Access tokens are
// stateless HS256 JWTs; refresh tokens are opaque random strings whose SHA-256
// hash is stored server-side. No external dependency — the JWT envelope is a few
// lines over stdlib crypto/hmac, which keeps the backend's dependency set lean.
package auth

import (
	"context"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"strings"
	"time"

	"github.com/albapepper/scoracle-data/internal/config"
)

// ErrInvalidToken is returned when an access token fails verification.
var ErrInvalidToken = errors.New("invalid token")

const issuer = "scoracle"

var b64 = base64.RawURLEncoding

// Tokens issues and verifies mobile auth tokens.
type Tokens struct {
	secret     []byte
	accessTTL  time.Duration
	refreshTTL time.Duration
}

// New builds a Tokens from config. If the JWT secret is empty the issuer is
// "unconfigured" and Configured reports false (the auth endpoints then 503,
// leaving the rest of the API untouched).
func New(cfg *config.Config) *Tokens {
	return &Tokens{
		secret:     []byte(cfg.JWTSecret),
		accessTTL:  cfg.JWTAccessTTL,
		refreshTTL: cfg.JWTRefreshTTL,
	}
}

// Configured reports whether a signing secret is set.
func (t *Tokens) Configured() bool { return len(t.secret) > 0 }

// AccessTTL is the lifetime of an access token.
func (t *Tokens) AccessTTL() time.Duration { return t.accessTTL }

// RefreshTTL is the lifetime of a refresh token.
func (t *Tokens) RefreshTTL() time.Duration { return t.refreshTTL }

// --- Access token (HS256 JWT) ---------------------------------------------

type jwtHeader struct {
	Alg string `json:"alg"`
	Typ string `json:"typ"`
}

type jwtClaims struct {
	Sub string `json:"sub"`
	Iss string `json:"iss"`
	Iat int64  `json:"iat"`
	Exp int64  `json:"exp"`
}

// IssueAccess returns a signed HS256 JWT for the user id.
func (t *Tokens) IssueAccess(userID string) (string, error) {
	now := time.Now()
	header, err := json.Marshal(jwtHeader{Alg: "HS256", Typ: "JWT"})
	if err != nil {
		return "", err
	}
	claims, err := json.Marshal(jwtClaims{
		Sub: userID,
		Iss: issuer,
		Iat: now.Unix(),
		Exp: now.Add(t.accessTTL).Unix(),
	})
	if err != nil {
		return "", err
	}
	signing := b64.EncodeToString(header) + "." + b64.EncodeToString(claims)
	return signing + "." + t.sign(signing), nil
}

// ParseAccess verifies a JWT's signature, algorithm, issuer, and expiry, and
// returns the subject (user id).
func (t *Tokens) ParseAccess(token string) (string, error) {
	parts := strings.Split(token, ".")
	if len(parts) != 3 {
		return "", ErrInvalidToken
	}
	expectedSig := t.sign(parts[0] + "." + parts[1])
	if subtle.ConstantTimeCompare([]byte(expectedSig), []byte(parts[2])) != 1 {
		return "", ErrInvalidToken
	}
	headerBytes, err := b64.DecodeString(parts[0])
	if err != nil {
		return "", ErrInvalidToken
	}
	var h jwtHeader
	if err := json.Unmarshal(headerBytes, &h); err != nil || h.Alg != "HS256" {
		return "", ErrInvalidToken
	}
	claimsBytes, err := b64.DecodeString(parts[1])
	if err != nil {
		return "", ErrInvalidToken
	}
	var c jwtClaims
	if err := json.Unmarshal(claimsBytes, &c); err != nil {
		return "", ErrInvalidToken
	}
	if c.Iss != issuer || c.Sub == "" || time.Now().Unix() >= c.Exp {
		return "", ErrInvalidToken
	}
	return c.Sub, nil
}

func (t *Tokens) sign(signing string) string {
	mac := hmac.New(sha256.New, t.secret)
	mac.Write([]byte(signing))
	return b64.EncodeToString(mac.Sum(nil))
}

// --- Refresh token (opaque + hashed) --------------------------------------

// NewRefreshToken returns a fresh opaque refresh token and its SHA-256 hash.
// Hand the raw token to the client; persist only the hash.
func (t *Tokens) NewRefreshToken() (raw, hash string, err error) {
	buf := make([]byte, 32)
	if _, err := rand.Read(buf); err != nil {
		return "", "", err
	}
	raw = b64.EncodeToString(buf)
	return raw, HashRefresh(raw), nil
}

// HashRefresh returns the SHA-256 hex hash of a refresh token.
func HashRefresh(raw string) string {
	sum := sha256.Sum256([]byte(raw))
	return hex.EncodeToString(sum[:])
}

// --- Request-context user id ----------------------------------------------

type ctxKey struct{}

var userIDKey ctxKey

// WithUserID stores the authenticated user id in the context.
func WithUserID(ctx context.Context, id string) context.Context {
	return context.WithValue(ctx, userIDKey, id)
}

// UserIDFromContext returns the authenticated user id, if present.
func UserIDFromContext(ctx context.Context) (string, bool) {
	id, ok := ctx.Value(userIDKey).(string)
	return id, ok
}
