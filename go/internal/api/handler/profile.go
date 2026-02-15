package handler

import (
	"fmt"
	"net/http"
	"strconv"

	"github.com/go-chi/chi/v5"

	"github.com/albapepper/scoracle-data/go/internal/api/respond"
	"github.com/albapepper/scoracle-data/go/internal/cache"
)

// GetProfile returns a player or team profile.
// Postgres returns complete JSON — no struct scanning, no marshaling.
// @Summary Get entity profile
// @Description Returns a complete player or team profile with bio, current team, and summary stats. Response is raw JSON from Postgres api_player_profile/api_team_profile functions.
// @Tags profiles
// @Produce json
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param entityID path int true "Entity ID"
// @Param sport query string true "Sport identifier" Enums(NBA, NFL, FOOTBALL)
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Router /profile/{entityType}/{entityID} [get]
func (h *Handler) GetProfile(w http.ResponseWriter, r *http.Request) {
	entityType := chi.URLParam(r, "entityType")
	idStr := chi.URLParam(r, "entityID")
	sport := r.URL.Query().Get("sport")

	if entityType != "player" && entityType != "team" {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_TYPE", "Entity type must be 'player' or 'team'")
		return
	}

	id, err := strconv.Atoi(idStr)
	if err != nil {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_ID", "ID must be an integer")
		return
	}

	if sport == "" {
		respond.WriteError(w, http.StatusBadRequest, "MISSING_SPORT", "sport query parameter is required")
		return
	}

	cacheKey := fmt.Sprintf("profile:%s:%d:%s", entityType, id, sport)
	ttl := cache.TTLEntityInfo

	// Check cache
	if data, etag, ok := h.cache.Get(cacheKey); ok {
		if cache.CheckETagMatch(r.Header.Get("If-None-Match"), etag) {
			respond.WriteNotModified(w, etag)
			return
		}
		respond.WriteJSON(w, data, etag, ttl, true)
		return
	}

	// Query Postgres — function returns complete JSON
	stmtName := "api_player_profile"
	if entityType == "team" {
		stmtName = "api_team_profile"
	}

	var raw []byte
	err = h.pool.QueryRow(r.Context(), stmtName, id, sport).Scan(&raw)
	if err != nil || raw == nil {
		respond.WriteError(w, http.StatusNotFound, "NOT_FOUND",
			fmt.Sprintf("%s not found", entityType))
		return
	}

	etag := h.cache.Set(cacheKey, raw, ttl)
	respond.WriteJSON(w, raw, etag, ttl, false)
}
