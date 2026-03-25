package api

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/albapepper/scoracle-data/internal/cache"
	"github.com/albapepper/scoracle-data/internal/config"
)

func TestRouteOwnershipSplit(t *testing.T) {
	cfg := &config.Config{CORSAllowOrigins: []string{"http://localhost:3000"}}
	router := NewRouter(nil, cache.New(false), cfg)

	tests := []struct {
		name       string
		path       string
		wantStatus int
	}{
		{name: "news route remains available", path: "/api/v1/news/status", wantStatus: http.StatusOK},
		{name: "twitter route remains available", path: "/api/v1/twitter/status", wantStatus: http.StatusOK},
		{name: "data player route registered", path: "/api/v1/nba/players/1", wantStatus: http.StatusServiceUnavailable},
		{name: "data standings route validates input", path: "/api/v1/nfl/standings", wantStatus: http.StatusBadRequest},
		{name: "legacy profile route removed", path: "/api/v1/profile/player/1?sport=NBA", wantStatus: http.StatusNotFound},
		{name: "autofill route registered", path: "/api/v1/nba/autofill", wantStatus: http.StatusServiceUnavailable},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodGet, tt.path, nil)
			rec := httptest.NewRecorder()
			router.ServeHTTP(rec, req)
			if rec.Code != tt.wantStatus {
				t.Fatalf("status for %s = %d, want %d", tt.path, rec.Code, tt.wantStatus)
			}
		})
	}
}

func TestGoSpecProxyUsesRequestHost(t *testing.T) {
	cfg := &config.Config{
		CORSAllowOrigins: []string{"http://localhost:3000"},
	}
	router := NewRouter(nil, cache.New(false), cfg)

	req := httptest.NewRequest(http.MethodGet, "/docs/go.json", nil)
	req.Host = "api.scoracle.com"
	req.Header.Set("X-Forwarded-Proto", "https")
	rec := httptest.NewRecorder()

	router.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status for %s = %d, want %d", req.URL.Path, rec.Code, http.StatusOK)
	}

	var spec map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &spec); err != nil {
		t.Fatalf("json.Unmarshal() error = %v, want nil", err)
	}
	if spec["host"] != "api.scoracle.com" {
		t.Fatalf("host = %v, want %q", spec["host"], "api.scoracle.com")
	}
	schemes, ok := spec["schemes"].([]any)
	if !ok || len(schemes) != 1 || schemes[0] != "https" {
		t.Fatalf("schemes = %v, want [https]", spec["schemes"])
	}
	if spec["basePath"] != "/api/v1" {
		t.Fatalf("basePath = %v, want %q", spec["basePath"], "/api/v1")
	}
}

func TestRewriteSwaggerServerUsesPublicURL(t *testing.T) {
	data := []byte(`{"host":"0.0.0.0:3000","basePath":"/","schemes":["http"]}`)

	rewritten, err := rewriteSwaggerServer(data, "https://api.scoracle.com")
	if err != nil {
		t.Fatalf("rewriteSwaggerServer() error = %v, want nil", err)
	}

	var spec map[string]any
	if err := json.Unmarshal(rewritten, &spec); err != nil {
		t.Fatalf("json.Unmarshal() error = %v, want nil", err)
	}
	if spec["host"] != "api.scoracle.com" {
		t.Fatalf("host = %v, want %q", spec["host"], "api.scoracle.com")
	}
	if spec["basePath"] != "/" {
		t.Fatalf("basePath = %v, want %q", spec["basePath"], "/")
	}
	schemes, ok := spec["schemes"].([]any)
	if !ok || len(schemes) != 1 || schemes[0] != "https" {
		t.Fatalf("schemes = %v, want [https]", spec["schemes"])
	}
}
