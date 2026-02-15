package api

import (
	"fmt"
	"net"
	"net/http"
	"sync"
	"time"

	"golang.org/x/time/rate"

	"github.com/albapepper/scoracle-data/go/internal/api/respond"
)

// --------------------------------------------------------------------------
// Request timing middleware
// --------------------------------------------------------------------------

// TimingMiddleware adds X-Process-Time header to all responses.
func TimingMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		start := time.Now()
		next.ServeHTTP(w, r)
		elapsed := time.Since(start)
		w.Header().Set("X-Process-Time", fmt.Sprintf("%.2fms", float64(elapsed.Microseconds())/1000.0))
	})
}

// --------------------------------------------------------------------------
// Rate limiting middleware (IP-based token bucket)
// --------------------------------------------------------------------------

type ipLimiter struct {
	mu       sync.Mutex
	limiters map[string]*rate.Limiter
	rate     rate.Limit
	burst    int
}

func newIPLimiter(requestsPerWindow int, window time.Duration) *ipLimiter {
	rps := float64(requestsPerWindow) / window.Seconds()
	return &ipLimiter{
		limiters: make(map[string]*rate.Limiter),
		rate:     rate.Limit(rps),
		burst:    requestsPerWindow / 2,
	}
}

func (l *ipLimiter) getLimiter(ip string) *rate.Limiter {
	l.mu.Lock()
	defer l.mu.Unlock()
	if limiter, exists := l.limiters[ip]; exists {
		return limiter
	}
	limiter := rate.NewLimiter(l.rate, l.burst)
	l.limiters[ip] = limiter
	return limiter
}

// RateLimitMiddleware returns middleware that rate-limits by client IP.
func RateLimitMiddleware(requestsPerWindow int, window time.Duration) func(http.Handler) http.Handler {
	limiter := newIPLimiter(requestsPerWindow, window)

	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			ip, _, _ := net.SplitHostPort(r.RemoteAddr)
			if ip == "" {
				ip = r.RemoteAddr
			}

			if !limiter.getLimiter(ip).Allow() {
				w.Header().Set("Retry-After", "60")
				respond.WriteError(w, http.StatusTooManyRequests, "RATE_LIMITED", "Too many requests")
				return
			}

			next.ServeHTTP(w, r)
		})
	}
}
