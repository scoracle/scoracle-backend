package handler

import (
	"fmt"
	"net/http"

	"github.com/albapepper/scoracle-data/internal/api/respond"
	"github.com/albapepper/scoracle-data/internal/cache"
)

// GetAutofillDatabase returns the entity bootstrap database for a sport.
// Uses the mv_autofill_entities materialized view for instant reads.
// @Summary Get autofill database
// @Description Returns the complete entity list (players + teams) for a sport, used for frontend search/autofill. Served from mv_autofill_entities materialized view.
// @Tags bootstrap
// @Produce json
// @Param sport query string true "Sport identifier" Enums(NBA, NFL, FOOTBALL)
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Router /autofill_databases [get]
func (h *Handler) GetAutofillDatabase(w http.ResponseWriter, r *http.Request) {
	sport := r.URL.Query().Get("sport")
	if sport == "" {
		respond.WriteError(w, http.StatusBadRequest, "MISSING_SPORT", "sport query parameter is required")
		return
	}

	cacheKey := fmt.Sprintf("autofill:%s", sport)
	ttl := cache.TTLEntityInfo

	if data, etag, ok := h.cache.Get(cacheKey); ok {
		if cache.CheckETagMatch(r.Header.Get("If-None-Match"), etag) {
			respond.WriteNotModified(w, etag)
			return
		}
		respond.WriteJSON(w, data, etag, ttl, true)
		return
	}

	var raw []byte
	err := h.pool.QueryRow(r.Context(), "autofill_entities", sport).Scan(&raw)
	if err != nil || raw == nil {
		respond.WriteError(w, http.StatusNotFound, "NOT_FOUND", "No entities found for "+sport)
		return
	}

	etag := h.cache.Set(cacheKey, raw, ttl)
	respond.WriteJSON(w, raw, etag, ttl, false)
}
