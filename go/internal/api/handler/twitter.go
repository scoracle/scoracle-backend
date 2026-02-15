package handler

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"strings"

	"github.com/albapepper/scoracle-data/go/internal/api/respond"
	"github.com/albapepper/scoracle-data/go/internal/cache"
)

// GetJournalistFeed returns tweets from the curated journalist list.
// @Summary Search journalist feed
// @Description Searches the cached journalist X List feed for entity mentions. Full feed is fetched once and cached for 1 hour; searches filter the cache client-side.
// @Tags twitter
// @Produce json
// @Param q query string true "Search query (1-200 chars)"
// @Param sport query string false "Sport context (metadata only)" Enums(NBA, NFL, FOOTBALL)
// @Param limit query int false "Max tweets (1-50, default 10)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 502 {object} respond.ErrorResponse
// @Failure 503 {object} respond.ErrorResponse
// @Router /twitter/journalist-feed [get]
func (h *Handler) GetJournalistFeed(w http.ResponseWriter, r *http.Request) {
	if !h.twitter.IsConfigured() {
		if h.cfg.TwitterListID == "" {
			respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE",
				"Twitter journalist list not configured. Set TWITTER_JOURNALIST_LIST_ID.")
			return
		}
		respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE",
			"Twitter API not configured. Set TWITTER_BEARER_TOKEN.")
		return
	}

	query := r.URL.Query().Get("q")
	if query == "" || len(query) > 200 {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_QUERY",
			"q parameter is required and must be 1-200 characters")
		return
	}

	sport := strings.ToUpper(r.URL.Query().Get("sport"))

	limit := 10
	if l := r.URL.Query().Get("limit"); l != "" {
		if n, err := strconv.Atoi(l); err == nil && n >= 1 && n <= 50 {
			limit = n
		}
	}

	result, err := h.twitter.GetJournalistFeed(query, sport, limit)
	if err != nil {
		respond.WriteError(w, http.StatusBadGateway, "EXTERNAL_SERVICE_ERROR",
			fmt.Sprintf("Twitter fetch failed: %v", err))
		return
	}

	// Marshal and write with cache headers.
	data, err := json.Marshal(result)
	if err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "MARSHAL_ERROR", "Failed to encode response")
		return
	}

	// Use feed_cached metadata to set cache headers.
	meta, _ := result["meta"].(map[string]interface{})
	feedCached, _ := meta["feed_cached"].(bool)

	etag := cache.ComputeETag(data)
	if cache.CheckETagMatch(r.Header.Get("If-None-Match"), etag) {
		respond.WriteNotModified(w, etag)
		return
	}

	// Set headers manually since this isn't from the generic cache.
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("ETag", etag)
	w.Header().Set("Vary", "Accept-Encoding")
	ttlSec := h.twitter.CacheTTLSeconds()
	w.Header().Set("Cache-Control",
		fmt.Sprintf("public, max-age=%d, stale-while-revalidate=%d", ttlSec, ttlSec/2))
	if feedCached {
		w.Header().Set("X-Cache", "HIT")
	} else {
		w.Header().Set("X-Cache", "MISS")
	}
	w.WriteHeader(http.StatusOK)
	w.Write(data)
}

// GetTwitterStatus returns Twitter API configuration status.
// @Summary Twitter service status
// @Description Returns Twitter API configuration state, list ID, cache TTL, and rate limit info.
// @Tags twitter
// @Produce json
// @Success 200 {object} map[string]interface{}
// @Router /twitter/status [get]
func (h *Handler) GetTwitterStatus(w http.ResponseWriter, r *http.Request) {
	respond.WriteJSONObject(w, http.StatusOK, h.twitter.Status())
}
