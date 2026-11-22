package api

import (
	"bytes"
	"github.com/go-chi/chi/v5/middleware"
	"log"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestLimiterEvictsIdleVisitorsWithoutResettingActiveLimits(t *testing.T) {
	l := newIPLimiter(2, time.Minute)
	active := l.getLimiter("active")
	if !active.Allow() || active.Allow() {
		t.Fatal("expected exhausted active bucket")
	}
	l.getLimiter("idle")
	old := l.limiters["idle"]
	old.lastSeen = time.Now().Add(-l.idleTTL - time.Second)
	l.limiters["idle"] = old
	l.nextSweep = time.Time{}
	if l.getLimiter("active") != active || active.Allow() {
		t.Fatal("active bucket reset")
	}
	if _, ok := l.limiters["idle"]; ok {
		t.Fatal("idle visitor retained")
	}
	if !l.getLimiter("idle").Allow() {
		t.Fatal("returning visitor should get a full bucket")
	}
}

func TestInternalAuthAcceptanceTelemetry(t *testing.T) {
	var output bytes.Buffer
	prior := log.Writer()
	log.SetOutput(&output)
	defer log.SetOutput(prior)
	for _, test := range []struct {
		name, configured, supplied string
		accepted                   bool
	}{
		{"matching", "configured-secret", "configured-secret", true},
		{"wrong", "configured-secret", "wrong-secret", false},
		{"missing", "configured-secret", "", false},
		{"unconfigured", "", "", false},
	} {
		t.Run(test.name, func(t *testing.T) {
			output.Reset()
			handler := middleware.RequestID(RateLimitMiddleware(2, time.Hour, test.configured)(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(http.StatusNoContent) })))
			for i := 0; i < 2; i++ {
				r := httptest.NewRequest("GET", "/api/v1/nba/player/177/news?secret=query-secret", nil)
				r.Header.Set("X-Request-ID", "gate-request")
				r.Header.Set("X-Scoracle-Internal-Key", test.supplied)
				r.Header.Set("Authorization", "Bearer bearer-secret")
				r.Header.Set("Cookie", "session=cookie-secret")
				w := httptest.NewRecorder()
				handler.ServeHTTP(w, r)
				want := http.StatusNoContent
				if i == 1 && !test.accepted {
					want = http.StatusTooManyRequests
				}
				if w.Code != want {
					t.Fatalf("status %d, want %d", w.Code, want)
				}
			}
			text := output.String()
			if test.accepted {
				if strings.Count(text, "internal_auth_accepted") != 2 || !strings.Contains(text, `request_id="gate-request" method="GET" path="/api/v1/nba/player/177/news"`) {
					t.Fatalf("missing acceptance correlation: %s", text)
				}
			} else if text != "" {
				t.Fatalf("unaccepted authentication logged: %s", text)
			}
			for _, secret := range []string{"configured-secret", "wrong-secret", "query-secret", "bearer-secret", "cookie-secret"} {
				if strings.Contains(text, secret) {
					t.Fatal("credential or query leaked")
				}
			}
		})
	}
}
