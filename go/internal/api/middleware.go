package api

import (
	"crypto/subtle"
	"fmt"
	"net"
	"net/http"
	"strings"
	"sync"
	"time"

	"golang.org/x/time/rate"

	"github.com/albapepper/scoracle-data/internal/api/respond"
	"github.com/albapepper/scoracle-data/internal/auth"
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

// clientIP resolves the end-user IP for rate-limit bucketing. Behind Cloudflare
// every request arrives from a shared edge IP, so RemoteAddr would pool all
// users (and the frontend Worker's SSR fetches) into a handful of buckets;
// CF-Connecting-IP carries the real client address.
func clientIP(r *http.Request) string {
	if ip := strings.TrimSpace(r.Header.Get("CF-Connecting-IP")); ip != "" {
		return ip
	}
	ip, _, _ := net.SplitHostPort(r.RemoteAddr)
	if ip == "" {
		ip = r.RemoteAddr
	}
	return ip
}

// RateLimitMiddleware returns middleware that rate-limits by client IP.
// Requests bearing X-Scoracle-Internal-Key matching internalKey bypass the
// limit entirely (trusted server-side callers); an empty internalKey disables
// the bypass.
func RateLimitMiddleware(requestsPerWindow int, window time.Duration, internalKey string) func(http.Handler) http.Handler {
	limiter := newIPLimiter(requestsPerWindow, window)

	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if internalKey != "" &&
				subtle.ConstantTimeCompare([]byte(r.Header.Get("X-Scoracle-Internal-Key")), []byte(internalKey)) == 1 {
				next.ServeHTTP(w, r)
				return
			}

			if !limiter.getLimiter(clientIP(r)).Allow() {
				w.Header().Set("Retry-After", "60")
				respond.WriteError(w, http.StatusTooManyRequests, "RATE_LIMITED", "Too many requests")
				return
			}

			next.ServeHTTP(w, r)
		})
	}
}

// --------------------------------------------------------------------------
// Mobile auth middleware (JWT bearer)
// --------------------------------------------------------------------------

// RequireAuth returns middleware that requires a valid access token. It extracts
// the bearer token, verifies it, and puts the user id in the request context
// (read with auth.UserIDFromContext). 401 on a missing or invalid token.
func RequireAuth(tokens *auth.Tokens) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			const prefix = "Bearer "
			header := r.Header.Get("Authorization")
			if !strings.HasPrefix(header, prefix) {
				respond.WriteError(w, http.StatusUnauthorized, "UNAUTHORIZED", "Missing bearer token")
				return
			}
			userID, err := tokens.ParseAccess(strings.TrimPrefix(header, prefix))
			if err != nil {
				respond.WriteError(w, http.StatusUnauthorized, "UNAUTHORIZED", "Invalid or expired token")
				return
			}
			next.ServeHTTP(w, r.WithContext(auth.WithUserID(r.Context(), userID)))
		})
	}
}
