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

	// Ollama (local Gemma inference)
	OllamaBaseURL string // default http://localhost:11434
	OllamaModel   string // default gemma4:e4b
	OllamaTimeout time.Duration

	// Transfer-rumor news-spike worker (LISTEN transfer_trigger)
	TransferEnabled       bool          // master switch (default true)
	TransferDebounce      time.Duration // min gap between runs for one team
	TransferMinArticles   int           // candidate pre-filter
	TransferMaxConcurrent int           // global cap on concurrent team analyses (shared GPU)

	// News scrub sweep (Gemma ID-gate over unscrubbed news_article_entities
	// links). Runs as a maintenance ticker: auto-vets primaries (cheap SQL) +
	// disambiguates candidate-rich secondaries via Gemma. See ml/news_scrub.go.
	NewsScrubEnabled  bool          // master switch (default true)
	NewsScrubInterval time.Duration // sweep cadence
	NewsScrubBatch    int           // max candidate-rich articles Gemma-scrubbed per tick

	// Pipeline stats: the daily pipeline_stats corpus snapshot (pure SQL). 0 disables.
	PipelineStatsInterval time.Duration

	// Real-time derive worker (FIRST-GPT-AUDIT Session 9): the in-API drain of the
	// durable pipeline_work queue. Woken by NOTIFY pipeline_work_ready (fired by the
	// migration-103 vetted-transition trigger), it also drains on startup and on the
	// safety-net interval, so a missed NOTIFY never costs correctness. Replaces the
	// old news-volume + transfer LISTEN workers, which ran Gemma directly off a
	// transient NOTIFY.
	DeriveWorkerEnabled bool          // master switch (default true)
	DeriveDrainInterval time.Duration // safety-net drain cadence between wake-up NOTIFYs

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

		OllamaBaseURL: envOr("OLLAMA_BASE_URL", "http://localhost:11434"),
		OllamaModel:   envOr("OLLAMA_MODEL", "gemma4:e4b"),
		OllamaTimeout: time.Duration(envInt("OLLAMA_TIMEOUT_SECONDS", 60)) * time.Second,

		TransferEnabled:       envBool("TRANSFER_ENABLED", true),
		TransferDebounce:      time.Duration(envInt("TRANSFER_DEBOUNCE_MINUTES", 60)) * time.Minute,
		TransferMinArticles:   envInt("TRANSFER_MIN_ARTICLES", 2),
		TransferMaxConcurrent: envInt("TRANSFER_MAX_CONCURRENT", 2),

		NewsScrubEnabled:  envBool("NEWS_SCRUB_ENABLED", true),
		NewsScrubInterval: time.Duration(envInt("NEWS_SCRUB_INTERVAL_MINUTES", 30)) * time.Minute,
		NewsScrubBatch:    envInt("NEWS_SCRUB_BATCH", 15),

		PipelineStatsInterval: time.Duration(envInt("PIPELINE_STATS_INTERVAL_MINUTES", 1440)) * time.Minute,

		DeriveWorkerEnabled: envBool("DERIVE_WORKER_ENABLED", true),
		DeriveDrainInterval: time.Duration(envInt("DERIVE_DRAIN_INTERVAL_SECONDS", 30)) * time.Second,

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
