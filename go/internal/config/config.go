// Package config provides centralized configuration loaded from environment
// variables for the Go API server.
package config

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"
)

// --------------------------------------------------------------------------
// Config struct — populated from environment variables
// --------------------------------------------------------------------------

type Config struct {
	// Database
	DatabaseURL    string
	DBPoolMinConns int
	DBPoolMaxConns int
	DBPoolMaxLife  time.Duration

	// API server
	APIHost     string
	APIPort     int
	Environment string // development, staging, production

	// CORS
	CORSAllowOrigins []string

	// Rate limiting
	RateLimitEnabled  bool
	RateLimitRequests int
	RateLimitWindow   time.Duration

	// Ollama (local inference — Mistral 7B since the L7 cutover off gemma4:e4b)
	OllamaBaseURL string // default http://localhost:11434
	OllamaModel   string // default mistral:7b
	// OllamaTimeout is the LONG-operation budget — narrative generation
	// (NumPredict 4000) genuinely needs it, and it doubles as the HTTP-client
	// hard backstop, so it must be >= every per-operation deadline. Short ops are
	// bounded tighter by OllamaShortTimeout via a per-call context (FIRST-GPT-AUDIT
	// Session 14). mistral:7b fits fully on the 8GB GPU (the L7 cutover off the
	// partial-CPU-offloaded gemma4:e4b), but a cold load or a queue wait behind another
	// call can still take a while — keep it roomy.
	OllamaTimeout time.Duration
	// OllamaShortTimeout bounds the quick model calls (scrub, vibe, sigil,
	// transfer-pair) so they fail fast and retry instead of waiting out the long
	// narrative budget. Per-call, applied by the drainer / maintenance scrub via context.
	OllamaShortTimeout time.Duration
	// OllamaMaxConcurrent caps in-process concurrent model calls (the shared GPU
	// governor, FIRST-GPT-AUDIT Session 14). 1 fully serializes — correct for a single
	// 8GB GPU running one mistral:7b instance. Applied as a package-level semaphore in
	// internal/ml, so it bounds the derive worker + maintenance scrub together.
	OllamaMaxConcurrent int
	// OllamaKeepAlive is how long Ollama keeps the model resident after a call
	// ("30m", "60m", "-1" = forever). Longer keeps the model warm across the gaps
	// between real-time work so a cold reload (the F-014 timeout trigger) is rare.
	OllamaKeepAlive string

	// Transfer-rumor news-spike worker (LISTEN transfer_trigger)
	TransferEnabled       bool          // master switch (default true)
	TransferDebounce      time.Duration // min gap between runs for one team
	TransferMinArticles   int           // candidate pre-filter
	TransferMaxConcurrent int           // global cap on concurrent team analyses (shared GPU)

	// News scrub sweep master switch. When disabled the maintenance ticker's
	// cadence is zeroed. The SQL auto-vet of primaries + enqueue to pipeline_work
	// runs at maintenance.DefaultConfig's cadence (30 min, batch 15).
	NewsScrubEnabled bool // master switch (default true)

	// Pipeline stats: the daily pipeline_stats corpus snapshot (pure SQL). 0 disables.
	PipelineStatsInterval time.Duration

	// Cache
	CacheEnabled bool

	// Notifications (FCM push)
	FCMCredentialsFile string

	// Mobile auth (device-identity JWT). JWTSecret empty => auth endpoints 503
	// (the rest of the API is unaffected). Set in .env.local.
	JWTSecret     string
	JWTAccessTTL  time.Duration
	JWTRefreshTTL time.Duration
}

// Load reads configuration from environment variables with sensible defaults.
func Load() (*Config, error) {
	dbURL := envOr("DATABASE_PRIVATE_URL", envOr("DATABASE_URL", ""))
	if dbURL == "" {
		return nil, fmt.Errorf("DATABASE_PRIVATE_URL or DATABASE_URL must be set")
	}

	environment := normalizeEnvironment(envOr("ENVIRONMENT", "development"))
	corsOrigins := envList("CORS_ALLOW_ORIGINS", []string{
		"http://localhost:3000",
		"http://localhost:4321", // Astro flagship dev (legacy, retires at DNS cutover)
		"http://localhost:5173", // Vite default (SolidStart flagship dev)
		"http://localhost:5185", // Vite fallback when 5173 is occupied (observed in scoracle-frontend dev)
	})
	if environment == "production" {
		corsOrigins = appendUnique(corsOrigins, envList("CORS_PRODUCTION_ORIGINS", []string{
			"https://scoracle.com",
			"https://www.scoracle.com",
		})...)
	}

	return &Config{
		DatabaseURL:    dbURL,
		DBPoolMinConns: envInt("DB_POOL_MIN_CONNS", 2),
		DBPoolMaxConns: envInt("DB_POOL_MAX_CONNS", 25),
		DBPoolMaxLife:  time.Duration(envInt("DB_POOL_MAX_LIFE_MINUTES", 30)) * time.Minute,

		APIHost:     envOr("API_HOST", "0.0.0.0"),
		APIPort:     envInt("PORT", envInt("API_PORT", 8000)),
		Environment: environment,

		CORSAllowOrigins: corsOrigins,

		RateLimitEnabled:  envBool("RATE_LIMIT_ENABLED", true),
		RateLimitRequests: envInt("RATE_LIMIT_REQUESTS", 100),
		RateLimitWindow:   time.Duration(envInt("RATE_LIMIT_WINDOW", 60)) * time.Second,

		OllamaBaseURL:       envOr("OLLAMA_BASE_URL", "http://localhost:11434"),
		OllamaModel:         envOr("OLLAMA_MODEL", "mistral:7b"),
		OllamaTimeout:       time.Duration(envInt("OLLAMA_TIMEOUT_SECONDS", 300)) * time.Second,
		OllamaShortTimeout:  time.Duration(envInt("OLLAMA_SHORT_TIMEOUT_SECONDS", 120)) * time.Second,
		OllamaMaxConcurrent: envInt("OLLAMA_MAX_CONCURRENT", 1),
		OllamaKeepAlive:     envOr("OLLAMA_KEEP_ALIVE", "30m"),

		TransferEnabled:       envBool("TRANSFER_ENABLED", true),
		TransferDebounce:      time.Duration(envInt("TRANSFER_DEBOUNCE_MINUTES", 60)) * time.Minute,
		TransferMinArticles:   envInt("TRANSFER_MIN_ARTICLES", 2),
		TransferMaxConcurrent: envInt("TRANSFER_MAX_CONCURRENT", 2),

		NewsScrubEnabled: envBool("NEWS_SCRUB_ENABLED", true),

		PipelineStatsInterval: time.Duration(envInt("PIPELINE_STATS_INTERVAL_MINUTES", 1440)) * time.Minute,

		CacheEnabled: envBool("CACHE_ENABLED", true),

		FCMCredentialsFile: envOr("FIREBASE_CREDENTIALS_FILE", ""),

		JWTSecret:     envOr("JWT_SECRET", ""),
		JWTAccessTTL:  time.Duration(envInt("JWT_ACCESS_TTL_MINUTES", 30)) * time.Minute,
		JWTRefreshTTL: time.Duration(envInt("JWT_REFRESH_TTL_DAYS", 90)) * 24 * time.Hour,
	}, nil
}

// --------------------------------------------------------------------------
// Env helpers
// --------------------------------------------------------------------------

func envOr(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}

func envInt(key string, fallback int) int {
	if v := os.Getenv(key); v != "" {
		if n, err := strconv.Atoi(v); err == nil {
			return n
		}
	}
	return fallback
}

func envBool(key string, fallback bool) bool {
	if v := os.Getenv(key); v != "" {
		b, err := strconv.ParseBool(v)
		if err == nil {
			return b
		}
	}
	return fallback
}

func envList(key string, fallback []string) []string {
	if v := os.Getenv(key); v != "" {
		parts := strings.Split(v, ",")
		result := make([]string, 0, len(parts))
		for _, p := range parts {
			if trimmed := strings.TrimSpace(p); trimmed != "" {
				result = append(result, trimmed)
			}
		}
		if len(result) > 0 {
			return result
		}
	}
	return fallback
}

func appendUnique(base []string, extras ...string) []string {
	seen := make(map[string]struct{}, len(base))
	result := append([]string{}, base...)
	for _, value := range base {
		seen[value] = struct{}{}
	}
	for _, value := range extras {
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		result = append(result, value)
	}
	return result
}

func normalizeEnvironment(value string) string {
	v := strings.TrimSpace(strings.ToLower(value))
	if v == "production" || v == "staging" || v == "development" {
		return v
	}
	return "development"
}
