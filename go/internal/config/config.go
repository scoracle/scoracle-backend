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

	// External API keys (third-party integrations only — seeding keys are in Python)
	TwitterBearerToken string
	TwitterListID      string
	NewsAPIKey         string

	// Cache
	CacheEnabled bool

	// Notifications (FCM push)
	FCMCredentialsFile string
}

// Load reads configuration from environment variables with sensible defaults.
func Load() (*Config, error) {
	dbURL := envOr("NEON_DATABASE_URL_V2", envOr("DATABASE_URL", envOr("NEON_DATABASE_URL", "")))
	if dbURL == "" {
		return nil, fmt.Errorf("NEON_DATABASE_URL_V2, DATABASE_URL, or NEON_DATABASE_URL must be set")
	}

	environment := normalizeEnvironment(envOr("ENVIRONMENT", "development"))
	corsOrigins := envList("CORS_ALLOW_ORIGINS", []string{
		"http://localhost:3000",
		"http://localhost:4321",
		"http://localhost:5173",
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
		DBPoolMaxConns: envInt("DB_POOL_MAX_CONNS", 10),
		DBPoolMaxLife:  time.Duration(envInt("DB_POOL_MAX_LIFE_MINUTES", 30)) * time.Minute,

		APIHost:     envOr("API_HOST", "0.0.0.0"),
		APIPort:     envInt("PORT", envInt("API_PORT", 8000)),
		Environment: environment,

		CORSAllowOrigins: corsOrigins,

		RateLimitEnabled:  envBool("RATE_LIMIT_ENABLED", true),
		RateLimitRequests: envInt("RATE_LIMIT_REQUESTS", 100),
		RateLimitWindow:   time.Duration(envInt("RATE_LIMIT_WINDOW", 60)) * time.Second,

		TwitterBearerToken: envOr("TWITTER_BEARER_TOKEN", ""),
		TwitterListID:      envOr("TWITTER_JOURNALIST_LIST_ID", ""),
		NewsAPIKey:         envOr("NEWS_API_KEY", ""),

		CacheEnabled: envBool("CACHE_ENABLED", true),

		FCMCredentialsFile: envOr("FIREBASE_CREDENTIALS_FILE", ""),
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
