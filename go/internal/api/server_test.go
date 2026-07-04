package api

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/albapepper/scoracle-data/internal/auth"
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
		{name: "bundled profile route removed (O16)", path: "/api/v1/nba/player/1", wantStatus: http.StatusNotFound},
		{name: "canonical meta route registered", path: "/api/v1/nba/meta", wantStatus: http.StatusServiceUnavailable},
		{name: "canonical sport health route registered", path: "/api/v1/nba/health", wantStatus: http.StatusServiceUnavailable},
		{name: "universal entities route registered", path: "/api/v1/entities", wantStatus: http.StatusServiceUnavailable},
		{name: "universal autofill alias registered", path: "/api/v1/autofill", wantStatus: http.StatusServiceUnavailable},
		{name: "sport autofill route unchanged", path: "/api/v1/nba/autofill", wantStatus: http.StatusServiceUnavailable},
		{name: "bundled league profile route removed (O16)", path: "/api/v1/football/leagues/8/player/1", wantStatus: http.StatusNotFound},
		{name: "league meta route registered", path: "/api/v1/football/leagues/8/meta", wantStatus: http.StatusServiceUnavailable},
		{name: "league health route registered", path: "/api/v1/football/leagues/8/health", wantStatus: http.StatusServiceUnavailable},
		{name: "canonical momentum route registered", path: "/api/v1/nba/player/1/momentum", wantStatus: http.StatusServiceUnavailable},
		{name: "canonical team momentum route registered", path: "/api/v1/nba/team/1/momentum", wantStatus: http.StatusServiceUnavailable},
		{name: "league momentum route registered", path: "/api/v1/football/leagues/8/player/1/momentum", wantStatus: http.StatusServiceUnavailable},
		{name: "canonical team results route registered", path: "/api/v1/nba/team/1/results", wantStatus: http.StatusServiceUnavailable},
		{name: "league team results route registered", path: "/api/v1/football/leagues/8/team/1/results", wantStatus: http.StatusServiceUnavailable},
		{name: "player headlines route retired", path: "/api/v1/nba/player/1/headlines", wantStatus: http.StatusNotFound},
		{name: "team headlines route retired", path: "/api/v1/nba/team/1/headlines", wantStatus: http.StatusNotFound},
		{name: "football headlines route retired", path: "/api/v1/football/player/1/headlines", wantStatus: http.StatusNotFound},
		{name: "headlines leaderboard route retired", path: "/api/v1/nba/leaderboard/headlines", wantStatus: http.StatusNotFound},
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

// TestHealthReadinessRequiresDB asserts that both health endpoints reflect DB
// readiness. With no pool (degraded startup / DB unreachable) they must report
// 503 — a database-less API is not serviceable, so it must not look healthy to
// a readiness probe. (Launch-hardening audit, Session 2.)
func TestHealthReadinessRequiresDB(t *testing.T) {
	cfg := &config.Config{CORSAllowOrigins: []string{"http://localhost:3000"}}
	router := NewRouter(nil, cache.New(false), cfg)

	for _, path := range []string{"/health", "/health/db"} {
		t.Run(path, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodGet, path, nil)
			rec := httptest.NewRecorder()
			router.ServeHTTP(rec, req)
			if rec.Code != http.StatusServiceUnavailable {
				t.Fatalf("status for %s = %d, want %d", path, rec.Code, http.StatusServiceUnavailable)
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

func TestOpenCodeProxyRouteRequiresAuth(t *testing.T) {
	cfg := &config.Config{
		CORSAllowOrigins: []string{"http://localhost:3000"},
		JWTSecret:        "test-secret",
		JWTAccessTTL:     time.Hour,
	}
	router := NewRouter(nil, cache.New(false), cfg)

	unauthReq := httptest.NewRequest(http.MethodGet, "/api/opencode/session", nil)
	unauthRec := httptest.NewRecorder()
	router.ServeHTTP(unauthRec, unauthReq)
	if unauthRec.Code != http.StatusUnauthorized {
		t.Fatalf("unauth status = %d, want %d", unauthRec.Code, http.StatusUnauthorized)
	}

	token, err := auth.New(cfg).IssueAccess("user-1")
	if err != nil {
		t.Fatalf("IssueAccess() error = %v", err)
	}
	authReq := httptest.NewRequest(http.MethodGet, "/api/opencode/session", nil)
	authReq.Header.Set("Authorization", "Bearer "+token)
	authRec := httptest.NewRecorder()
	router.ServeHTTP(authRec, authReq)
	if authRec.Code != http.StatusBadGateway {
		t.Fatalf("auth status = %d, want %d when local OpenCode is unavailable", authRec.Code, http.StatusBadGateway)
	}
}

func TestOpenCodeHostRouteUsesCloudflareAccessGate(t *testing.T) {
	cfg := &config.Config{
		CORSAllowOrigins: []string{"http://localhost:3000"},
		JWTSecret:        "test-secret",
		JWTAccessTTL:     time.Hour,
	}
	router := NewRouter(nil, cache.New(false), cfg)

	req := httptest.NewRequest(http.MethodGet, "https://opencode.scoracle.com/session", nil)
	rec := httptest.NewRecorder()
	router.ServeHTTP(rec, req)

	if rec.Code != http.StatusBadGateway {
		t.Fatalf("status = %d, want %d when local OpenCode is unavailable", rec.Code, http.StatusBadGateway)
	}
}

func TestCORSAllowsOpenCodeProxyMethods(t *testing.T) {
	cfg := &config.Config{
		CORSAllowOrigins: []string{"http://localhost:3000"},
	}
	router := NewRouter(nil, cache.New(false), cfg)

	for _, method := range []string{http.MethodDelete, http.MethodPut, http.MethodPatch} {
		t.Run(method, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodOptions, "/api/opencode/session", nil)
			req.Header.Set("Origin", "http://localhost:3000")
			req.Header.Set("Access-Control-Request-Method", method)
			rec := httptest.NewRecorder()
			router.ServeHTTP(rec, req)

			if rec.Code != http.StatusNoContent {
				t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
			}
			if methods := rec.Header().Get("Access-Control-Allow-Methods"); !strings.Contains(methods, method) {
				t.Fatalf("Access-Control-Allow-Methods = %q, want %s", methods, method)
			}
		})
	}
}
