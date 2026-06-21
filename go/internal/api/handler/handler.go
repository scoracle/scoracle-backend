// Package handler provides HTTP handlers for all API endpoints.
// Handlers query Postgres directly via pgxpool — no service layer.
// Postgres functions return complete JSON; handlers pass raw bytes through.
package handler

import (
	"context"
	"errors"
	"net/http"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"golang.org/x/sync/singleflight"

	"github.com/albapepper/scoracle-data/internal/api/respond"
	"github.com/albapepper/scoracle-data/internal/auth"
	"github.com/albapepper/scoracle-data/internal/buildinfo"
	"github.com/albapepper/scoracle-data/internal/cache"
	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/thirdparty"
)

// errNoPool reports that the handler has no database pool (degraded startup).
var errNoPool = errors.New("database pool unavailable")

// Handler holds shared dependencies for all endpoint handlers.
type Handler struct {
	pool   *pgxpool.Pool
	cache  *cache.Cache
	cfg    *config.Config
	news   *thirdparty.NewsService
	auth   *auth.Tokens
	flight singleflight.Group // coalesces concurrent cache-miss loads by cache key
}

// New creates a Handler with shared dependencies.
func New(pool *pgxpool.Pool, c *cache.Cache, cfg *config.Config, tokens *auth.Tokens) *Handler {
	return &Handler{
		pool:  pool,
		cache: c,
		cfg:   cfg,
		news:  thirdparty.NewNewsService(pool, nil),
		auth:  tokens,
	}
}

// Root serves API info at /.
// @Summary API root info
// @Description Returns API name, version, status, and available optimizations.
// @Tags meta
// @Produce json
// @Success 200 {object} map[string]interface{}
// @Router / [get]
func (h *Handler) Root(w http.ResponseWriter, r *http.Request) {
	respond.WriteJSONObject(w, http.StatusOK, map[string]interface{}{
		"name":    "Scoracle Data API",
		"version": "2.0.0",
		"status":  "running",
		"commit":  buildinfo.Commit,
		"built":   buildinfo.BuildTime,
		"docs":    "/docs",
		"optimizations": []string{
			"pgxpool_connection_pooling",
			"prepared_statements",
			"postgres_json_passthrough",
			"gzip_compression",
			"in_memory_cache",
			"etag_support",
		},
	})
}

// dbReady returns nil when the database pool exists and answers a trivial
// query, and an error otherwise. It is the single source of truth shared by the
// liveness/readiness endpoints so they cannot disagree about DB reachability.
func (h *Handler) dbReady(ctx context.Context) error {
	if h.pool == nil {
		return errNoPool
	}
	var n int
	return h.pool.QueryRow(ctx, "health_check").Scan(&n)
}

// HealthCheck reports serving readiness, including database reachability.
//
// Every data endpoint is a precomputed read from Postgres, so an API that can't
// reach its database is not actually serviceable. This endpoint therefore
// returns 503 when the database is unreachable rather than reporting a process
// that is up-but-useless as healthy (the launch-hardening audit calls this out:
// a readiness probe pointed at /health must reflect DB readiness).
// @Summary Health check
// @Description Returns serving readiness; 503 when the database is unreachable.
// @Tags health
// @Produce json
// @Success 200 {object} map[string]interface{}
// @Failure 503 {object} map[string]interface{}
// @Router /health [get]
func (h *Handler) HealthCheck(w http.ResponseWriter, r *http.Request) {
	if err := h.dbReady(r.Context()); err != nil {
		respond.WriteJSONObject(w, http.StatusServiceUnavailable, map[string]interface{}{
			"status":    "unhealthy",
			"database":  "disconnected",
			"timestamp": time.Now().UTC().Format(time.RFC3339),
		})
		return
	}
	respond.WriteJSONObject(w, http.StatusOK, map[string]interface{}{
		"status":    "healthy",
		"database":  "connected",
		"timestamp": time.Now().UTC().Format(time.RFC3339),
	})
}

// HealthCheckDB verifies database connectivity.
// @Summary Database health check
// @Description Verifies Postgres connectivity.
// @Tags health
// @Produce json
// @Success 200 {object} map[string]interface{}
// @Failure 503 {object} map[string]interface{}
// @Router /health/db [get]
func (h *Handler) HealthCheckDB(w http.ResponseWriter, r *http.Request) {
	if err := h.dbReady(r.Context()); err != nil {
		respond.WriteJSONObject(w, http.StatusServiceUnavailable, map[string]interface{}{
			"status":    "unhealthy",
			"database":  "disconnected",
			"error":     "database connectivity check failed",
			"timestamp": time.Now().UTC().Format(time.RFC3339),
		})
		return
	}
	respond.WriteJSONObject(w, http.StatusOK, map[string]interface{}{
		"status":    "healthy",
		"database":  "connected",
		"timestamp": time.Now().UTC().Format(time.RFC3339),
	})
}

// HealthCheckCache returns cache statistics.
// @Summary Cache health check
// @Description Returns in-memory cache statistics (active keys, expired keys).
// @Tags health
// @Produce json
// @Success 200 {object} map[string]interface{}
// @Router /health/cache [get]
func (h *Handler) HealthCheckCache(w http.ResponseWriter, r *http.Request) {
	respond.WriteJSONObject(w, http.StatusOK, map[string]interface{}{
		"status":    "healthy",
		"cache":     h.cache.Stats(),
		"timestamp": time.Now().UTC().Format(time.RFC3339),
	})
}
