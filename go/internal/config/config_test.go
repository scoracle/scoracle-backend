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
