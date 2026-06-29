// Package opencodeproxy exposes a local OpenCode Server through the Go API.
//
// The OpenCode process listens only on loopback. External callers reach it by
// authenticating to the Go API and using /api/opencode/*, which this package
// rewrites to the OpenCode root before proxying. Browser callers can also reach
// it through opencode.scoracle.com when that hostname is protected by
// Cloudflare Access and routed to the same Go API tunnel.
package opencodeproxy

import (
	"context"
	"errors"
	"net"
	"net/http"
	"net/http/httputil"
	"net/url"
	"strings"
	"time"

	"github.com/albapepper/scoracle-data/internal/api/respond"
)

const (
	// MountPath is the public API prefix. It is stripped before forwarding.
	MountPath = "/api/opencode"

	// Hostname is the browser-friendly hostname for OpenCode. It proxies from
	// the origin path so OpenCode can serve its UI as if it were mounted at /.
	Hostname = "opencode.scoracle.com"

	// DefaultTarget is intentionally loopback-only; Cloudflare should expose the
	// Go API, not OpenCode Server itself.
	DefaultTarget = "http://127.0.0.1:4096"

	dialTimeout           = 5 * time.Second
	dialKeepAlive         = 30 * time.Second
	maxIdleConns          = 100
	maxIdleConnsPerHost   = 16
	idleConnTimeout       = 90 * time.Second
	tlsHandshakeTimeout   = 5 * time.Second
	responseHeaderTimeout = 30 * time.Second
	expectContinueTimeout = 1 * time.Second
)

// NewDefaultHandler returns the production OpenCode reverse proxy handler.
func NewDefaultHandler() http.Handler {
	target := mustParseURL(DefaultTarget)
	return NewHandler(target, MountPath)
}

// NewDefaultHostHandler returns the production host-based OpenCode handler.
func NewDefaultHostHandler() http.Handler {
	target := mustParseURL(DefaultTarget)
	return NewHandler(target, "")
}

// NewHandler builds a reverse proxy to target.
//
// httputil.ReverseProxy preserves methods, headers, query strings, request
// bodies, response headers/status codes, streaming responses, and protocol
// upgrades. FlushInterval=-1 asks Go to flush each write immediately, which
// keeps SSE events moving through the proxy without application buffering.
func NewHandler(target *url.URL, stripPrefix string) http.Handler {
	proxy := &httputil.ReverseProxy{
		Director: func(req *http.Request) {
			originalHost := req.Host
			originalScheme := forwardedProto(req)

			req.URL.Scheme = target.Scheme
			req.URL.Host = target.Host
			req.URL.Path = stripPathPrefix(req.URL.Path, stripPrefix)
			req.URL.RawPath = ""

			if target.RawQuery == "" || req.URL.RawQuery == "" {
				req.URL.RawQuery = target.RawQuery + req.URL.RawQuery
			} else {
				req.URL.RawQuery = target.RawQuery + "&" + req.URL.RawQuery
			}

			// Keep the caller's headers intact, but add the usual forwarding
			// context so OpenCode can reconstruct the external request if needed.
			if originalHost != "" {
				req.Header.Set("X-Forwarded-Host", originalHost)
			}
			req.Header.Set("X-Forwarded-Proto", originalScheme)
			req.Host = target.Host
		},
		Transport:     newTransport(),
		FlushInterval: -1,
		ErrorHandler: func(w http.ResponseWriter, r *http.Request, err error) {
			status := gatewayStatus(err)
			code := "OPENCODE_BAD_GATEWAY"
			message := "OpenCode Server is unavailable"
			if status == http.StatusGatewayTimeout {
				code = "OPENCODE_GATEWAY_TIMEOUT"
				message = "OpenCode Server timed out"
			}
			respond.WriteError(w, status, code, message)
		},
	}

	return proxy
}

// IsHost reports whether host is the browser OpenCode hostname. The request
// Host can include a port in local tests, so compare after trimming it.
func IsHost(host string) bool {
	if h, _, err := net.SplitHostPort(host); err == nil {
		host = h
	}
	return strings.EqualFold(host, Hostname)
}

func newTransport() *http.Transport {
	return &http.Transport{
		Proxy:                 nil,
		DialContext:           (&net.Dialer{Timeout: dialTimeout, KeepAlive: dialKeepAlive}).DialContext,
		ForceAttemptHTTP2:     false,
		MaxIdleConns:          maxIdleConns,
		MaxIdleConnsPerHost:   maxIdleConnsPerHost,
		IdleConnTimeout:       idleConnTimeout,
		TLSHandshakeTimeout:   tlsHandshakeTimeout,
		ResponseHeaderTimeout: responseHeaderTimeout,
		ExpectContinueTimeout: expectContinueTimeout,
		DisableCompression:    true,
	}
}

func stripPathPrefix(path, prefix string) string {
	if prefix == "" {
		if path == "" {
			return "/"
		}
		return path
	}
	if path == prefix {
		return "/"
	}
	if strings.HasPrefix(path, prefix+"/") {
		stripped := strings.TrimPrefix(path, prefix)
		if stripped == "" {
			return "/"
		}
		return stripped
	}
	return path
}

func forwardedProto(r *http.Request) string {
	if proto := r.Header.Get("X-Forwarded-Proto"); proto != "" {
		return proto
	}
	if r.TLS != nil {
		return "https"
	}
	return "http"
}

func gatewayStatus(err error) int {
	if errors.Is(err, context.DeadlineExceeded) {
		return http.StatusGatewayTimeout
	}
	var netErr net.Error
	if errors.As(err, &netErr) && netErr.Timeout() {
		return http.StatusGatewayTimeout
	}
	return http.StatusBadGateway
}

func mustParseURL(raw string) *url.URL {
	target, err := url.Parse(raw)
	if err != nil {
		panic(err)
	}
	return target
}
