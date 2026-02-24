package api

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/albapepper/scoracle-data/internal/cache"
	"github.com/albapepper/scoracle-data/internal/config"
)

func TestRouteOwnershipSplit(t *testing.T) {
	cfg := &config.Config{
		CORSAllowOrigins: []string{"http://localhost:3000"},
		PostgRESTURL:     "http://localhost:3000",
	}
	router := NewRouter(nil, cache.New(false), cfg)

	tests := []struct {
		name       string
		path       string
		wantStatus int
	}{
		{name: "news remains on go", path: "/api/v1/news/status", wantStatus: http.StatusOK},
		{name: "twitter remains on go", path: "/api/v1/twitter/status", wantStatus: http.StatusOK},
		{name: "profile moved to postgrest", path: "/api/v1/profile/player/1?sport=NBA", wantStatus: http.StatusNotFound},
		{name: "stats moved to postgrest", path: "/api/v1/stats/definitions?sport=NBA", wantStatus: http.StatusNotFound},
		{name: "autofill moved to postgrest", path: "/api/v1/autofill_databases?sport=NBA", wantStatus: http.StatusNotFound},
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
