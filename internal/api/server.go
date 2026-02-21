package api

import (
	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/jackc/pgx/v5/pgxpool"
	corslib "github.com/rs/cors"
	httpSwagger "github.com/swaggo/http-swagger/v2"

	"github.com/albapepper/scoracle-data/internal/api/handler"
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
	postgrestURL := cfg.PostgRESTURL
	if postgrestURL == "" {
		postgrestURL = "http://localhost:3000" // default for local dev
	}
	r.Get("/docs/*", httpSwagger.Handler(
		httpSwagger.URL("/docs/doc.json"),
		httpSwagger.UIConfig(map[string]string{
			"urls": `[
				{"url": "/docs/doc.json", "name": "Ingestion & Notifications API (Go)"},
				{"url": "` + postgrestURL + `/", "name": "Stats API (PostgREST)"}
			]`,
		}),
	))

	// API v1 routes
	r.Route("/api/v1", func(r chi.Router) {
		// Profiles
		r.Get("/profile/{entityType}/{entityID}", h.GetProfile)

		// Stats
		r.Get("/stats/definitions", h.GetStatDefinitions)
		r.Get("/stats/{entityType}/{entityID}", h.GetEntityStats)
		r.Get("/stats/{entityType}/{entityID}/seasons", h.GetAvailableSeasons)

		// Bootstrap / autofill
		r.Get("/autofill_databases", h.GetAutofillDatabase)

		// News
		r.Get("/news/status", h.GetNewsStatus)
		r.Get("/news/{entityType}/{entityID}", h.GetEntityNews)

		// Twitter
		r.Get("/twitter/journalist-feed", h.GetJournalistFeed)
		r.Get("/twitter/status", h.GetTwitterStatus)
	})

	return r
}
