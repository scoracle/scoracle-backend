package config

import (
	"testing"
)

func TestLoadAddsProductionOrigins(t *testing.T) {
	t.Setenv("DATABASE_URL", "postgres://example")
	t.Setenv("ENVIRONMENT", "production")
	t.Setenv("CORS_ALLOW_ORIGINS", "http://localhost:3000")
	t.Setenv("CORS_PRODUCTION_ORIGINS", "https://scoracle.com,https://api.scoracle.com")

	cfg, err := Load()
	if err != nil {
		t.Fatalf("Load() error = %v, want nil", err)
	}

	wantOrigins := []string{
		"http://localhost:3000",
		"https://scoracle.com",
		"https://api.scoracle.com",
	}
	if len(cfg.CORSAllowOrigins) != len(wantOrigins) {
		t.Fatalf("len(CORSAllowOrigins) = %d, want %d", len(cfg.CORSAllowOrigins), len(wantOrigins))
	}
	for i, want := range wantOrigins {
		if cfg.CORSAllowOrigins[i] != want {
			t.Fatalf("CORSAllowOrigins[%d] = %q, want %q", i, cfg.CORSAllowOrigins[i], want)
		}
	}
}

func TestLoadPrefersPORTOverAPIPort(t *testing.T) {
	t.Setenv("DATABASE_URL", "postgres://example")
	t.Setenv("API_PORT", "8000")
	t.Setenv("PORT", "49231")

	cfg, err := Load()
	if err != nil {
		t.Fatalf("Load() error = %v, want nil", err)
	}

	if cfg.APIPort != 49231 {
		t.Fatalf("APIPort = %d, want %d", cfg.APIPort, 49231)
	}
}

func TestLoadNormalizesMalformedEnvironment(t *testing.T) {
	t.Setenv("DATABASE_URL", "postgres://example")
	t.Setenv("ENVIRONMENT", "RAILWAY_GO_BIN=api")
	t.Setenv("CORS_PRODUCTION_ORIGINS", "https://scoracle.com")

	cfg, err := Load()
	if err != nil {
		t.Fatalf("Load() error = %v, want nil", err)
	}

	if cfg.Environment != "development" {
		t.Fatalf("Environment = %q, want %q", cfg.Environment, "development")
	}

	for _, origin := range cfg.CORSAllowOrigins {
		if origin == "https://scoracle.com" {
			t.Fatalf("CORSAllowOrigins unexpectedly contains production origin in development mode")
		}
	}
}
