// Package config provides centralized configuration loaded from environment
// variables. Shared by both cmd/api and cmd/ingest.
package config

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"
)

// --------------------------------------------------------------------------
// Sport registry — mirrors Python core/types.py SPORT_REGISTRY
// --------------------------------------------------------------------------

type SportConfig struct {
	ID            string
	Name          string
	CurrentSeason int
}

var SportRegistry = map[string]SportConfig{
	"NBA":      {ID: "NBA", Name: "National Basketball Association", CurrentSeason: 2025},
	"NFL":      {ID: "NFL", Name: "National Football League", CurrentSeason: 2025},
	"FOOTBALL": {ID: "FOOTBALL", Name: "Football (Soccer)", CurrentSeason: 2025},
}

// --------------------------------------------------------------------------
// Table names — single source of truth, matches schema.sql
// --------------------------------------------------------------------------

const (
	PlayersTable     = "players"
	PlayerStatsTable = "player_stats"
	TeamsTable       = "teams"
	TeamStatsTable   = "team_stats"
	LeaguesTable     = "leagues"
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
	Debug       bool

	// CORS
	CORSAllowOrigins []string

	// Rate limiting
	RateLimitEnabled  bool
	RateLimitRequests int
	RateLimitWindow   time.Duration

	// External API keys
	BDLAPIKey          string
	SportMonksAPIToken string
	TwitterBearerToken string
	TwitterListID      string
	NewsAPIKey         string

	// Cache
	CacheEnabled bool
}

// Load reads configuration from environment variables with sensible defaults.
func Load() (*Config, error) {
	dbURL := envOr("NEON_DATABASE_URL_V2", envOr("DATABASE_URL", envOr("NEON_DATABASE_URL", "")))
	if dbURL == "" {
		return nil, fmt.Errorf("NEON_DATABASE_URL_V2, DATABASE_URL, or NEON_DATABASE_URL must be set")
	}

	return &Config{
		DatabaseURL:    dbURL,
		DBPoolMinConns: envInt("DB_POOL_MIN_CONNS", 2),
		DBPoolMaxConns: envInt("DB_POOL_MAX_CONNS", 10),
		DBPoolMaxLife:  time.Duration(envInt("DB_POOL_MAX_LIFE_MINUTES", 30)) * time.Minute,

		APIHost:     envOr("API_HOST", "0.0.0.0"),
		APIPort:     envInt("API_PORT", envInt("PORT", 8000)),
		Environment: envOr("ENVIRONMENT", "development"),
		Debug:       envBool("DEBUG", false),

		CORSAllowOrigins: envList("CORS_ALLOW_ORIGINS", []string{
			"http://localhost:3000",
			"http://localhost:4321",
			"http://localhost:5173",
		}),

		RateLimitEnabled:  envBool("RATE_LIMIT_ENABLED", true),
		RateLimitRequests: envInt("RATE_LIMIT_REQUESTS", 100),
		RateLimitWindow:   time.Duration(envInt("RATE_LIMIT_WINDOW", 60)) * time.Second,

		BDLAPIKey:          envOr("BALLDONTLIE_API_KEY", ""),
		SportMonksAPIToken: envOr("SPORTMONKS_API_TOKEN", ""),
		TwitterBearerToken: envOr("TWITTER_BEARER_TOKEN", ""),
		TwitterListID:      envOr("TWITTER_JOURNALIST_LIST_ID", ""),
		NewsAPIKey:         envOr("NEWS_API_KEY", ""),

		CacheEnabled: envBool("CACHE_ENABLED", true),
	}, nil
}

// IsProduction returns true if running in production environment.
func (c *Config) IsProduction() bool {
	return c.Environment == "production"
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
