package api

import (
	"fmt"
	"io"
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/jackc/pgx/v5/pgxpool"
	corslib "github.com/rs/cors"
	httpSwagger "github.com/swaggo/http-swagger/v2"

	"github.com/albapepper/scoracle-data/internal/api/handler"
	"github.com/albapepper/scoracle-data/internal/api/respond"
	"github.com/albapepper/scoracle-data/internal/cache"
	"github.com/albapepper/scoracle-data/internal/config"
)

// NewRouter creates and configures the Chi router with all middleware and routes.
func NewRouter(pool *pgxpool.Pool, appCache *cache.Cache, cfg *config.Config) *chi.Mux {
	r := chi.NewRouter()

	// --- Middleware stack ---
	r.Use(middleware.RequestID)
	r.Use(middleware.RealIP)
	r.Use(TimingMiddleware)
	r.Use(middleware.Compress(5)) // gzip

	// CORS
	c := corslib.New(corslib.Options{
		AllowedOrigins:   cfg.CORSAllowOrigins,
		AllowedMethods:   []string{"GET", "HEAD", "OPTIONS"},
		AllowedHeaders:   []string{"Accept", "Accept-Encoding", "Content-Type", "If-None-Match", "Cache-Control"},
		ExposedHeaders:   []string{"X-Process-Time", "X-Cache", "Link", "ETag"},
		AllowCredentials: false,
	})
	r.Use(c.Handler)

	// Rate limiting
	if cfg.RateLimitEnabled {
		r.Use(RateLimitMiddleware(cfg.RateLimitRequests, cfg.RateLimitWindow))
	}

	// --- Handler dependencies ---
	h := handler.New(pool, appCache, cfg)

	// --- Routes ---

	// Root
	r.Get("/", h.Root)

	// Health checks
	r.Route("/health", func(r chi.Router) {
		r.Get("/", h.HealthCheck)
		r.Get("/db", h.HealthCheckDB)
		r.Get("/cache", h.HealthCheckCache)
	})

	// Swagger UI — multi-spec dropdown showing Go API and PostgREST specs.
	// PostgREST auto-generates its OpenAPI spec at its root endpoint.
	// We proxy it through /docs/postgrest.json to avoid cross-origin issues.
	postgrestURL := cfg.PostgRESTURL
	if postgrestURL == "" {
		postgrestURL = "http://localhost:3000" // default for local dev
	}

	// Proxy PostgREST OpenAPI spec to avoid cross-origin fetch in Swagger UI.
	r.Get("/docs/postgrest.json", func(w http.ResponseWriter, r *http.Request) {
		const cacheKey = "postgrest-openapi-spec"
		const ttl = 30 * time.Minute

		if data, etag, ok := appCache.Get(cacheKey); ok {
			if cache.CheckETagMatch(r.Header.Get("If-None-Match"), etag) {
				respond.WriteNotModified(w, etag)
				return
			}
			respond.WriteJSON(w, data, etag, ttl, true)
			return
		}

		req, err := http.NewRequestWithContext(r.Context(), http.MethodGet, postgrestURL+"/", nil)
		if err != nil {
			respond.WriteError(w, http.StatusBadGateway, "proxy_error", "failed to build request")
			return
		}

		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			respond.WriteError(w, http.StatusBadGateway, "proxy_error", "failed to fetch PostgREST spec")
			return
		}
		defer resp.Body.Close()

		if resp.StatusCode != http.StatusOK {
			respond.WriteError(w, http.StatusBadGateway, "proxy_error",
				fmt.Sprintf("PostgREST returned status %d", resp.StatusCode))
			return
		}

		data, err := io.ReadAll(resp.Body)
		if err != nil {
			respond.WriteError(w, http.StatusBadGateway, "proxy_error", "failed to read response body")
			return
		}

		etag := appCache.Set(cacheKey, data, ttl)
		respond.WriteJSON(w, data, etag, ttl, false)
	})

	r.Get("/docs/*", httpSwagger.Handler(
		httpSwagger.URL("/docs/doc.json"),
		httpSwagger.UIConfig(map[string]string{
			"urls": `[
				{"url": "/docs/doc.json", "name": "Ingestion & Notifications API (Go)"},
				{"url": "/docs/postgrest.json", "name": "Stats API (PostgREST)"}
			]`,
		}),
	))

	// API v1 routes
	r.Route("/api/v1", func(r chi.Router) {
		// News
		r.Get("/news/status", h.GetNewsStatus)
		r.Get("/news/{entityType}/{entityID}", h.GetEntityNews)

		// Twitter
		r.Get("/twitter/journalist-feed", h.GetJournalistFeed)
		r.Get("/twitter/status", h.GetTwitterStatus)
	})

	return r
}
