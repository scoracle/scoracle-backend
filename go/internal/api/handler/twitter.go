package handler

import (
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"strings"

	"github.com/go-chi/chi/v5"

	"github.com/albapepper/scoracle-data/internal/api/respond"
	"github.com/albapepper/scoracle-data/internal/cache"
	"github.com/albapepper/scoracle-data/internal/thirdparty"
)

// GetSportTweets returns the cached tweet feed for a sport.
// @Summary Sport tweet feed
// @Description Returns cached tweets from the sport's curated X List. Refreshes on demand when older than the TTL; concurrent refreshes are coalesced via singleflight.
// @Tags twitter
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param limit query int false "Max tweets (1-100, default 25)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 502 {object} respond.ErrorResponse
// @Failure 503 {object} respond.ErrorResponse
// @Router /{sport}/twitter/feed [get]
func (h *Handler) GetSportTweets(w http.ResponseWriter, r *http.Request) {
	sport := strings.ToLower(chi.URLParam(r, "sport"))
	if sport == "" {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_SPORT", "sport path parameter required")
		return
	}

	if !h.twitter.IsConfigured(sport) {
		if !h.twitter.HasBearerToken() {
			respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE",
				"Twitter API not configured. Set TWITTER_BEARER_TOKEN.")
			return
		}
		respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE",
			fmt.Sprintf("Twitter list not configured for %s. Set TWITTER_LIST_%s.", sport, strings.ToUpper(sport)))
		return
	}

	limit := 25
	if l := r.URL.Query().Get("limit"); l != "" {
		if n, err := strconv.Atoi(l); err == nil && n >= 1 && n <= 100 {
			limit = n
		}
	}

	raw, err := h.twitter.GetSportFeed(r.Context(), sport, limit)
	if err != nil {
		if errors.Is(err, thirdparty.ErrSportNotConfigured) {
			respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE",
				"Twitter list not configured for "+sport)
			return
		}
		respond.WriteError(w, http.StatusBadGateway, "EXTERNAL_SERVICE_ERROR",
			fmt.Sprintf("Twitter fetch failed: %v", err))
		return
	}

	writeTwitterResponse(w, r, h, raw)
}

// GetEntityTweets returns tweets linked to a specific player or team.
// @Summary Entity tweet feed
// @Description Returns cached tweets mentioning the given player or team. Matching uses the shared search_aliases logic.
// @Tags twitter
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param limit query int false "Max tweets (1-100, default 25)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 503 {object} respond.ErrorResponse
// @Router /{sport}/twitter/{entityType}/{id} [get]
func (h *Handler) GetEntityTweets(w http.ResponseWriter, r *http.Request) {
	sport := strings.ToLower(chi.URLParam(r, "sport"))
	entityType := chi.URLParam(r, "entityType")
	idStr := chi.URLParam(r, "id")

	if entityType != "player" && entityType != "team" {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_ENTITY_TYPE",
			"entityType must be 'player' or 'team'")
		return
	}
	id, err := strconv.Atoi(idStr)
	if err != nil {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_ENTITY_ID", "id must be an integer")
		return
	}

	limit := 25
	if l := r.URL.Query().Get("limit"); l != "" {
		if n, err := strconv.Atoi(l); err == nil && n >= 1 && n <= 100 {
			limit = n
		}
	}

	// Trigger a sport-level refresh if configured and stale, so entity feeds
	// benefit from a warm cache. Ignore errors — the entity query still works.
	if h.twitter.IsConfigured(sport) {
		if _, err := h.twitter.GetSportFeed(r.Context(), sport, 1); err != nil {
			// no-op: log in service
		}
	}

	raw, err := h.twitter.GetEntityTweets(r.Context(), sport, entityType, id, limit)
	if err != nil {
		respond.WriteError(w, http.StatusBadGateway, "EXTERNAL_SERVICE_ERROR",
			fmt.Sprintf("Twitter entity fetch failed: %v", err))
		return
	}
	writeTwitterResponse(w, r, h, raw)
}

// GetTwitterStatus returns per-sport Twitter list configuration + cache state.
// @Summary Twitter service status
// @Description Returns per-sport cache state, since_id, last_fetched_at, and bearer-token configuration.
// @Tags twitter
// @Produce json
// @Success 200 {object} map[string]interface{}
// @Router /twitter/status [get]
func (h *Handler) GetTwitterStatus(w http.ResponseWriter, r *http.Request) {
	respond.WriteJSONObject(w, http.StatusOK, h.twitter.Status(r.Context()))
}

// writeTwitterResponse emits a JSON blob with ETag + cache headers tuned for the
// lazy-cache TTL. Used by both sport and entity feeds.
func writeTwitterResponse(w http.ResponseWriter, r *http.Request, h *Handler, data []byte) {
	etag := cache.ComputeETag(data)
	if cache.CheckETagMatch(r.Header.Get("If-None-Match"), etag) {
		respond.WriteNotModified(w, etag)
		return
	}
	ttl := h.twitter.CacheTTLSeconds()
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("ETag", etag)
	w.Header().Set("Vary", "Accept-Encoding")
	w.Header().Set("Cache-Control",
		fmt.Sprintf("public, max-age=%d, stale-while-revalidate=%d", ttl, ttl/2))
	w.WriteHeader(http.StatusOK)
	w.Write(data)
}
