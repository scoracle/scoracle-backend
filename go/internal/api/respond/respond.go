// Package respond provides shared JSON response utilities for API handlers.
package respond

import (
	"encoding/json"
	"fmt"
	"net/http"
	"time"
)

// ErrorResponse is the standard error shape for all API errors.
type ErrorResponse struct {
	Error struct {
		Code    string `json:"code"`
		Message string `json:"message"`
		Detail  string `json:"detail,omitempty"`
	} `json:"error"`
}

// WriteJSON writes raw JSON bytes to the response with cache and ETag headers.
func WriteJSON(w http.ResponseWriter, data []byte, etag string, ttl time.Duration, cacheHit bool) {
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("ETag", etag)
	w.Header().Set("Vary", "Accept-Encoding")
	setCacheHeaders(w, ttl, cacheHit)
	w.WriteHeader(http.StatusOK)
	w.Write(data)
}

// WriteNotModified sends a 304 with the matching ETag.
func WriteNotModified(w http.ResponseWriter, etag string) {
	w.Header().Set("ETag", etag)
	w.WriteHeader(http.StatusNotModified)
}

// WriteError sends a structured JSON error response.
func WriteError(w http.ResponseWriter, status int, code, message string) {
	resp := ErrorResponse{}
	resp.Error.Code = code
	resp.Error.Message = message
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "no-cache, no-store, must-revalidate")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(resp)
}

// WriteErrorDetail sends a structured error with additional detail.
func WriteErrorDetail(w http.ResponseWriter, status int, code, message, detail string) {
	resp := ErrorResponse{}
	resp.Error.Code = code
	resp.Error.Message = message
	resp.Error.Detail = detail
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "no-cache, no-store, must-revalidate")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(resp)
}

// WriteJSONObject marshals a Go value to JSON and writes it.
// Used for non-Postgres responses (health checks, news, twitter).
func WriteJSONObject(w http.ResponseWriter, status int, v interface{}) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(v)
}

func setCacheHeaders(w http.ResponseWriter, ttl time.Duration, cacheHit bool) {
	maxAge := int(ttl.Seconds())
	swr := maxAge / 2
	if cacheHit {
		w.Header().Set("X-Cache", "HIT")
	} else {
		w.Header().Set("X-Cache", "MISS")
	}
	w.Header().Set("Cache-Control",
		fmt.Sprintf("public, max-age=%d, stale-while-revalidate=%d", maxAge, swr))
}
