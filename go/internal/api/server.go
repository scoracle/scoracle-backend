package api

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/jackc/pgx/v5/pgxpool"
	corslib "github.com/rs/cors"
	httpSwagger "github.com/swaggo/http-swagger/v2"

	apidocs "github.com/albapepper/scoracle-data/docs"
	"github.com/albapepper/scoracle-data/internal/api/handler"
	"github.com/albapepper/scoracle-data/internal/api/respond"
	"github.com/albapepper/scoracle-data/internal/cache"
	"github.com/albapepper/scoracle-data/internal/config"
)

// NewRouter creates and configures the Chi router with all middleware and routes.
func NewRouter(pool *pgxpool.Pool, appCache *cache.Cache, cfg *config.Config) *chi.Mux {
	r := chi.NewRouter()
	if appCache == nil {
		appCache = cache.New(false)
	}

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

	// Swagger UI
	r.Get("/docs/go.json", func(w http.ResponseWriter, r *http.Request) {
		data, err := rewriteSwaggerServer([]byte(apidocs.SwaggerInfo.ReadDoc()), requestBaseURL(r))
		if err != nil {
			respond.WriteError(w, http.StatusBadGateway, "proxy_error", "failed to rewrite Go spec")
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		w.Write(data)
	})

	r.Get("/docs/*", httpSwagger.Handler(
		httpSwagger.URL("/docs/go.json"),
	))

	// API v1 routes
	r.Route("/api/v1", func(r chi.Router) {
		r.Route("/{sport:nba|nfl|football}", func(r chi.Router) {
			r.Get("/players/{id}", h.GetPlayerPage)
			r.Get("/teams/{id}", h.GetTeamPage)
			r.Get("/standings", h.GetStandingsPage)
			r.Get("/leaders", h.GetLeadersPage)
			r.Get("/search", h.GetSearchPage)
			r.Get("/stat-definitions", h.GetStatDefinitionsPage)
		})
		r.Get("/football/leagues", h.GetLeaguesPage)

		// News
		r.Get("/news/status", h.GetNewsStatus)
		r.Get("/news/{entityType}/{entityID}", h.GetEntityNews)

		// Twitter
		r.Get("/twitter/journalist-feed", h.GetJournalistFeed)
		r.Get("/twitter/status", h.GetTwitterStatus)
	})

	return r
}

func requestBaseURL(r *http.Request) string {
	scheme := "http"
	if forwardedProto := r.Header.Get("X-Forwarded-Proto"); forwardedProto != "" {
		scheme = forwardedProto
	} else if r.TLS != nil {
		scheme = "https"
	}
	return scheme + "://" + r.Host
}

func rewriteSwaggerServer(data []byte, publicURL string) ([]byte, error) {
	if publicURL == "" {
		return data, nil
	}

	var spec map[string]any
	if err := json.Unmarshal(data, &spec); err != nil {
		return nil, err
	}

	parsed, err := url.Parse(publicURL)
	if err != nil {
		return nil, err
	}
	if parsed.Scheme == "" || parsed.Host == "" {
		return nil, fmt.Errorf("invalid public API URL: %s", publicURL)
	}

	spec["host"] = parsed.Host
	if _, ok := spec["basePath"]; !ok {
		spec["basePath"] = "/"
	}
	spec["schemes"] = []string{parsed.Scheme}

	return json.Marshal(spec)
}
