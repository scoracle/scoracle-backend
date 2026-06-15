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
	"github.com/albapepper/scoracle-data/internal/auth"
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
	//
	// AllowedOrigins covers browser callers (the configured frontend origins).
	// AllowOriginFunc is an additional gate that fires when an Origin header is
	// present; we accept any origin already in AllowedOrigins, and we explicitly
	// allow origin-less requests (Origin == "") which is how server-to-server
	// callers — e.g. the SolidStart frontend's worker doing SSR fetches via
	// "use server" / `query()` — reach the API. The rs/cors library default
	// already permits origin-less requests, but stating it here documents the
	// intent and survives any future tightening of the library default.
	allowed := make(map[string]struct{}, len(cfg.CORSAllowOrigins))
	for _, o := range cfg.CORSAllowOrigins {
		allowed[o] = struct{}{}
	}
	c := corslib.New(corslib.Options{
		AllowedOrigins: cfg.CORSAllowOrigins,
		AllowOriginFunc: func(origin string) bool {
			if origin == "" {
				return true // origin-less server-to-server calls
			}
			_, ok := allowed[origin]
			return ok
		},
		AllowedMethods:   []string{"GET", "HEAD", "OPTIONS", "POST"},
		AllowedHeaders:   []string{"Accept", "Accept-Encoding", "Content-Type", "If-None-Match", "Cache-Control", "Authorization"},
		ExposedHeaders:   []string{"X-Process-Time", "X-Cache", "Link", "ETag"},
		AllowCredentials: false,
	})
	r.Use(c.Handler)

	// Rate limiting
	if cfg.RateLimitEnabled {
		r.Use(RateLimitMiddleware(cfg.RateLimitRequests, cfg.RateLimitWindow))
	}

	// --- Handler dependencies ---
	tokens := auth.New(cfg)
	h := handler.New(pool, appCache, cfg, tokens)

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
			// Canonical sport routes (vNext)
			r.Get("/{entityType:player|team}/{id}", h.GetProfilePage)
			r.Get("/{entityType:player|team}/{id}/trends", h.GetTrendsPage)
			r.Get("/{entityType:player|team}/{id}/sparkline", h.GetSparkline)
			// Deprecated alias — remove once the frontend rollout to /sparkline settles.
			r.Get("/{entityType:player|team}/{id}/starline", h.GetSparkline)
			r.Get("/team/{id}/results", h.GetTeamResults)
			r.Get("/team/{id}/roster", h.GetRoster)
			r.Get("/team/{id}/transfers", h.GetTransfers)
			r.Get("/player/{id}/suitors", h.GetPlayerSuitors)
			// News rail (two-rail model): narratives + transfer scope + vibe in one payload.
			r.Get("/{entityType:player|team}/{id}/news", h.GetEntityNewsRail)
			r.Get("/meta", h.GetMetaPage)
			r.Get("/health", h.GetSportHealthPage)
			r.Get("/leaderboard", h.GetLeaderboard)
			r.Get("/leaderboard/vibes", h.GetVibesLeaderboard)
			r.Get("/leaderboard/news", h.GetNewsLeaderboard)
			r.Get("/leaderboard/transfers", h.GetTransfersLeaderboard)
			r.Get("/leagues/{leagueId}/{entityType:player|team}/{id}", h.GetLeagueProfilePage)
			r.Get("/leagues/{leagueId}/{entityType:player|team}/{id}/trends", h.GetLeagueTrendsPage)
			r.Get("/leagues/{leagueId}/team/{id}/results", h.GetLeagueTeamResults)
			r.Get("/leagues/{leagueId}/meta", h.GetLeagueMetaPage)
			r.Get("/leagues/{leagueId}/health", h.GetLeagueHealthPage)

			// Sport-scoped twitter lazy cache
			r.Get("/twitter/feed", h.GetSportTweets)
			r.Get("/twitter/{entityType:player|team}/{id}", h.GetEntityTweets)

			// Vibe sentiment scores (Gemma) — read-only. Generation happens
			// via the vibe CLI or the milestone listener worker.
			r.Get("/vibe/hottest", h.GetHottestEntities)
			r.Get("/vibe/{entityType:player|team}/{id}", h.GetLatestVibe)
			r.Get("/vibe/{entityType:player|team}/{id}/history", h.GetVibeHistory)
		})
		// News
		r.Get("/news/status", h.GetNewsStatus)
		r.Get("/news/{entityType}/{entityID}", h.GetEntityNews)

		// Twitter
		r.Get("/twitter/status", h.GetTwitterStatus)

		// Mobile auth (device-identity JWT). /device + /refresh are public;
		// /device/push + /logout require a valid access token.
		r.Route("/auth", func(r chi.Router) {
			r.Post("/device", h.AuthDevice)
			r.Post("/refresh", h.AuthRefresh)
			r.Group(func(r chi.Router) {
				r.Use(RequireAuth(tokens))
				r.Post("/device/push", h.AuthRegisterPush)
				r.Post("/logout", h.AuthLogout)
			})
		})
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
