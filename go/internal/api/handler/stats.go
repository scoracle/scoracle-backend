package handler

import (
	"fmt"
	"net/http"
	"strconv"
	"time"

	"github.com/go-chi/chi/v5"

	"github.com/albapepper/scoracle-data/go/internal/api/respond"
	"github.com/albapepper/scoracle-data/go/internal/cache"
	"github.com/albapepper/scoracle-data/go/internal/config"
)

// GetEntityStats returns stats + percentiles for an entity.
// @Summary Get entity stats
// @Description Returns stats and percentiles for a player or team for a given season. Response is raw JSON from Postgres api_entity_stats function.
// @Tags stats
// @Produce json
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param entityID path int true "Entity ID"
// @Param sport query string true "Sport identifier" Enums(NBA, NFL, FOOTBALL)
// @Param season query int false "Season year (defaults to current)"
// @Param league_id query int false "League ID (for FOOTBALL)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Router /stats/{entityType}/{entityID} [get]
func (h *Handler) GetEntityStats(w http.ResponseWriter, r *http.Request) {
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

	// Season: default to current
	season := getCurrentSeason(sport)
	if s := r.URL.Query().Get("season"); s != "" {
		season, err = strconv.Atoi(s)
		if err != nil {
			respond.WriteError(w, http.StatusBadRequest, "INVALID_SEASON", "season must be an integer")
			return
		}
		if season < 2000 || season > time.Now().Year()+1 {
			respond.WriteError(w, http.StatusBadRequest, "INVALID_SEASON",
				fmt.Sprintf("Season must be between 2000 and %d", time.Now().Year()+1))
			return
		}
	}

	leagueID := 0
	if lid := r.URL.Query().Get("league_id"); lid != "" {
		leagueID, _ = strconv.Atoi(lid)
	}

	ttl := statsTTL(sport, season)
	cacheKey := fmt.Sprintf("stats:%s:%d:%s:%d:%d", entityType, id, sport, season, leagueID)

	// Check cache
	if data, etag, ok := h.cache.Get(cacheKey); ok {
		if cache.CheckETagMatch(r.Header.Get("If-None-Match"), etag) {
			respond.WriteNotModified(w, etag)
			return
		}
		respond.WriteJSON(w, data, etag, ttl, true)
		return
	}

	// Query Postgres
	var raw []byte
	err = h.pool.QueryRow(r.Context(), "api_entity_stats",
		entityType, id, sport, season, leagueID).Scan(&raw)
	if err != nil || raw == nil {
		respond.WriteError(w, http.StatusNotFound, "NOT_FOUND",
			fmt.Sprintf("%s stats not found for %s %d", entityType, sport, season))
		return
	}

	etag := h.cache.Set(cacheKey, raw, ttl)
	respond.WriteJSON(w, raw, etag, ttl, false)
}

// GetAvailableSeasons returns seasons with stats for an entity.
// @Summary Get available seasons
// @Description Returns a list of seasons that have stats for an entity.
// @Tags stats
// @Produce json
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param entityID path int true "Entity ID"
// @Param sport query string true "Sport identifier" Enums(NBA, NFL, FOOTBALL)
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Router /stats/{entityType}/{entityID}/seasons [get]
func (h *Handler) GetAvailableSeasons(w http.ResponseWriter, r *http.Request) {
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

	cacheKey := fmt.Sprintf("seasons:%s:%d:%s", entityType, id, sport)
	ttl := cache.TTLCurrentSeason

	if data, etag, ok := h.cache.Get(cacheKey); ok {
		if cache.CheckETagMatch(r.Header.Get("If-None-Match"), etag) {
			respond.WriteNotModified(w, etag)
			return
		}
		respond.WriteJSON(w, data, etag, ttl, true)
		return
	}

	var raw []byte
	err = h.pool.QueryRow(r.Context(), "api_available_seasons", entityType, id, sport).Scan(&raw)
	if err != nil || raw == nil {
		respond.WriteError(w, http.StatusNotFound, "NOT_FOUND", "No seasons found")
		return
	}

	etag := h.cache.Set(cacheKey, raw, ttl)
	respond.WriteJSON(w, raw, etag, ttl, false)
}

// GetStatDefinitions returns canonical stat definitions for a sport.
// @Summary Get stat definitions
// @Description Returns canonical stat definitions (names, display labels, categories) for a sport.
// @Tags stats
// @Produce json
// @Param sport query string true "Sport identifier" Enums(NBA, NFL, FOOTBALL)
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Router /stats/definitions [get]
func (h *Handler) GetStatDefinitions(w http.ResponseWriter, r *http.Request) {
	sport := r.URL.Query().Get("sport")
	if sport == "" {
		respond.WriteError(w, http.StatusBadRequest, "MISSING_SPORT", "sport query parameter is required")
		return
	}

	cacheKey := fmt.Sprintf("stat_defs:%s", sport)
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
	err := h.pool.QueryRow(r.Context(), "stat_definitions", sport).Scan(&raw)
	if err != nil || raw == nil {
		respond.WriteError(w, http.StatusNotFound, "NOT_FOUND", "No stat definitions found for "+sport)
		return
	}

	etag := h.cache.Set(cacheKey, raw, ttl)
	respond.WriteJSON(w, raw, etag, ttl, false)
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

func getCurrentSeason(sport string) int {
	if cfg, ok := config.SportRegistry[sport]; ok {
		return cfg.CurrentSeason
	}
	return time.Now().Year()
}

func statsTTL(sport string, season int) time.Duration {
	current := getCurrentSeason(sport)
	if season >= current {
		return cache.TTLCurrentSeason
	}
	return cache.TTLHistorical
}
