package api

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strings"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/jackc/pgx/v5/pgxpool"
	corslib "github.com/rs/cors"
	httpSwagger "github.com/swaggo/http-swagger/v2"

	apidocs "github.com/albapepper/scoracle-data/docs"
	"github.com/albapepper/scoracle-data/internal/api/handler"
	"github.com/albapepper/scoracle-data/internal/api/opencodeproxy"
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
	compress := middleware.Compress(5) // gzip
	r.Use(func(next http.Handler) http.Handler {
		compressed := compress(next)
		return http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
			// The OpenCode proxy must preserve upstream headers and stream bytes as
			// OpenCode writes them, so bypass response compression for that mount.
			if isOpenCodeProxyRequest(req) {
				next.ServeHTTP(w, req)
				return
			}
			compressed.ServeHTTP(w, req)
		})
	})

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
		AllowedMethods:   []string{"GET", "HEAD", "OPTIONS", "POST", "DELETE", "PUT", "PATCH"},
		AllowedHeaders:   []string{"Accept", "Accept-Encoding", "Content-Type", "If-None-Match", "Cache-Control", "Authorization"},
		ExposedHeaders:   []string{"X-Process-Time", "X-Cache", "Link", "ETag"},
		AllowCredentials: false,
	})
	r.Use(c.Handler)

	// Rate limiting
	if cfg.RateLimitEnabled {
		r.Use(RateLimitMiddleware(cfg.RateLimitRequests, cfg.RateLimitWindow))
	}

	// Browser OpenCode UI route. Cloudflare Tunnel sends opencode.scoracle.com
	// to this same Go API, and Cloudflare Access should protect that hostname at
	// the edge. Keeping this host-mounted at / avoids breaking OpenCode's UI
	// assets and absolute API paths.
	opencodeHostHandler := opencodeproxy.NewDefaultHostHandler()
	r.Use(func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
			if opencodeproxy.IsHost(req.Host) {
				opencodeHostHandler.ServeHTTP(w, req)
				return
			}
			next.ServeHTTP(w, req)
		})
	})

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

	// Authenticated OpenCode proxy. OpenCode stays bound to 127.0.0.1:4096;
	// the public Cloudflare path terminates here and reuses the API bearer auth.
	r.Group(func(r chi.Router) {
		r.Use(RequireAuth(tokens))
		r.Mount(opencodeproxy.MountPath, opencodeproxy.NewDefaultHandler())
	})

	// API v1 routes
	r.Route("/api/v1", func(r chi.Router) {
		// Universal home-page entity directory. This is intentionally separate from
		// the sport-scoped /{sport}/autofill payloads, which remain profile/sport
		// hydration indexes with heavier metadata.
		r.Get("/entities", h.GetEntities)
		r.Get("/autofill", h.GetEntities)

		r.Route("/{sport:nba|nfl|football}", func(r chi.Router) {
			// Per-product endpoints keep each card independently cacheable and avoid
			// rebuilding a bundled profile payload on every page load. /stats carries
			// structured season data, /rating the stat read, /news and /transfers the
			// news products, /momentum the trajectory data, and /sigil the final crown
			// synthesis.
			r.Get("/{entityType:player|team}/{id}/stats", h.GetEntityStats)
			r.Get("/{entityType:player|team}/{id}/rating", h.GetEntityRating)
			r.Get("/{entityType:player|team}/{id}/sigil", h.GetEntitySigil)
			r.Get("/{entityType:player|team}/{id}/momentum", h.GetTrendsPage)
			r.Get("/team/{id}/results", h.GetTeamResults)
			r.Get("/team/{id}/roster", h.GetRoster)
			r.Get("/{entityType:player|team}/{id}/news", h.GetEntityNarratives)
			r.Get("/{entityType:player|team}/{id}/transfers", h.GetEntityTransfers)
			// Per-entity identity metadata for the page-header island. Frontend
			// surfaces should hydrate islands from these dedicated endpoints; the
			// universal /entities directory is the only small local search DB.
			r.Get("/{entityType:player|team}/{id}/meta", h.GetEntityMeta)
			// Legacy sport-wide metadata/search payload. Kept for existing consumers;
			// new home search should use /api/v1/entities or /api/v1/autofill.
			r.Get("/meta", h.GetMetaPage)
			// Legacy sport-scoped autofill payload. Kept unchanged for downstream
			// sport/profile-specific consumers; not the universal home search DB.
			r.Get("/autofill", h.GetAutofill)
			r.Get("/health", h.GetSportHealthPage)
			r.Get("/leaderboard", h.GetLeaderboard)
			r.Get("/leaderboard/vibes", h.GetVibesLeaderboard)
			r.Get("/leaderboard/sigil", h.GetSigilLeaderboard)
			r.Get("/leaderboard/news", h.GetNewsLeaderboard)
			r.Get("/leaderboard/transfers", h.GetTransfersLeaderboard)
			r.Get("/leaderboard/trending", h.GetTrendingLeaderboard)
			r.Get("/leaderboard/momentum", h.GetTrendingLeaderboard)
			r.Get("/leagues/{leagueId}/{entityType:player|team}/{id}/momentum", h.GetLeagueTrendsPage)
			r.Get("/leagues/{leagueId}/team/{id}/results", h.GetLeagueTeamResults)
			r.Get("/leagues/{leagueId}/meta", h.GetLeagueMetaPage)
			r.Get("/leagues/{leagueId}/health", h.GetLeagueHealthPage)
		})
		// The eager News card reads the precomputed /{sport}/{type}/{id}/news narratives.

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

func isOpenCodeProxyRequest(r *http.Request) bool {
	if opencodeproxy.IsHost(r.Host) {
		return true
	}
	return r.URL != nil && (r.URL.Path == opencodeproxy.MountPath || strings.HasPrefix(r.URL.Path, opencodeproxy.MountPath+"/"))
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
