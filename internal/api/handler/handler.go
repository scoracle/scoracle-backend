// Package handler provides HTTP handlers for all API endpoints.
// Handlers query Postgres directly via pgxpool — no service layer.
// Postgres functions return complete JSON; handlers pass raw bytes through.
package handler

import (
	"net/http"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/api/respond"
	"github.com/albapepper/scoracle-data/internal/cache"
	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/external"
)

// Handler holds shared dependencies for all endpoint handlers.
type Handler struct {
	pool    *pgxpool.Pool
	cache   *cache.Cache
	cfg     *config.Config
	news    *external.NewsService
	twitter *external.TwitterService
}

// New creates a Handler with shared dependencies.
func New(pool *pgxpool.Pool, c *cache.Cache, cfg *config.Config) *Handler {
	return &Handler{
		pool:    pool,
		cache:   c,
		cfg:     cfg,
		news:    external.NewNewsService(cfg.NewsAPIKey),
		twitter: external.NewTwitterService(cfg.TwitterBearerToken, cfg.TwitterListID),
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

// HealthCheck returns basic health status.
// @Summary Health check
// @Description Returns basic health status and timestamp.
// @Tags health
// @Produce json
// @Success 200 {object} map[string]interface{}
// @Router /health [get]
func (h *Handler) HealthCheck(w http.ResponseWriter, r *http.Request) {
	respond.WriteJSONObject(w, http.StatusOK, map[string]interface{}{
		"status":    "healthy",
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
	var n int
	err := h.pool.QueryRow(r.Context(), "health_check").Scan(&n)
	if err != nil {
		respond.WriteJSONObject(w, http.StatusServiceUnavailable, map[string]interface{}{
			"status":    "unhealthy",
			"database":  "disconnected",
			"error":     "Database connection check failed",
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
