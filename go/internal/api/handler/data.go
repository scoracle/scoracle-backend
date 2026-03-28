package handler

import (
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/jackc/pgx/v5"

	"github.com/albapepper/scoracle-data/internal/api/respond"
	"github.com/albapepper/scoracle-data/internal/cache"
)

var validSports = map[string]struct{}{
	"nba":      {},
	"nfl":      {},
	"football": {},
}

var validEntityTypes = map[string]struct{}{
	"player": {},
	"team":   {},
}

// GetProfilePage returns a canonical profile payload for a sport entity.
// @Summary Get profile page
// @Description Returns the sport profile payload for an entity type and ID.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param season query int false "Season year"
// @Param league_id query int false "League ID filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/{entityType}/{id} [get]
func (h *Handler) GetProfilePage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}

	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}

	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}

	stmt := sport + "_profile_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, true, entityType, id, season, leagueID)
}

// GetLeagueProfilePage returns a league-scoped profile payload for a sport entity.
// @Summary Get league profile page
// @Description Returns the sport profile payload scoped to a specific league.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param leagueId path int true "League ID"
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param season query int false "Season year"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leagues/{leagueId}/{entityType}/{id} [get]
func (h *Handler) GetLeagueProfilePage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}

	leagueID, ok := parsePathID(w, r, "leagueId", "league id")
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}

	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}

	stmt := sport + "_profile_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, true, entityType, id, season, leagueID)
}

// GetMetaPage returns the canonical metadata payload used for frontend local DB hydration.
// @Summary Get meta page
// @Description Returns complete metadata and search payload for a sport.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param league_id query int false "League ID filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/meta [get]
func (h *Handler) GetMetaPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}

	stmt := sport + "_meta_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false, leagueID)
}

// GetLeagueMetaPage returns league-scoped metadata payload for a sport.
// @Summary Get league meta page
// @Description Returns metadata and search payload for a specific league.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param leagueId path int true "League ID"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leagues/{leagueId}/meta [get]
func (h *Handler) GetLeagueMetaPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	leagueID, ok := parsePathID(w, r, "leagueId", "league id")
	if !ok {
		return
	}

	stmt := sport + "_meta_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false, leagueID)
}

// GetSportHealthPage returns sport-level data health from Postgres.
// @Summary Get sport health page
// @Description Returns sport data freshness and counts.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param league_id query int false "League ID filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/health [get]
func (h *Handler) GetSportHealthPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}

	stmt := sport + "_health_page"
	h.serveStatementJSONNoCache(w, r, stmt, leagueID)
}

// GetLeagueHealthPage returns league-scoped sport health from Postgres.
// @Summary Get league health page
// @Description Returns sport data freshness and counts for a specific league.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param leagueId path int true "League ID"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leagues/{leagueId}/health [get]
func (h *Handler) GetLeagueHealthPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	leagueID, ok := parsePathID(w, r, "leagueId", "league id")
	if !ok {
		return
	}

	stmt := sport + "_health_page"
	h.serveStatementJSONNoCache(w, r, stmt, leagueID)
}

func (h *Handler) serveStatementJSON(
	w http.ResponseWriter,
	r *http.Request,
	stmt string,
	cacheKey string,
	ttl time.Duration,
	notFoundOnNoRows bool,
	args ...any,
) {
	if h.pool == nil {
		respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE", "database pool unavailable")
		return
	}

	if data, etag, ok := h.cache.Get(cacheKey); ok {
		if cache.CheckETagMatch(r.Header.Get("If-None-Match"), etag) {
			respond.WriteNotModified(w, etag)
			return
		}
		respond.WriteJSON(w, data, etag, ttl, true)
		return
	}

	var data []byte
	err := h.pool.QueryRow(r.Context(), stmt, args...).Scan(&data)
	if err != nil {
		if notFoundOnNoRows && errors.Is(err, pgx.ErrNoRows) {
			respond.WriteError(w, http.StatusNotFound, "NOT_FOUND", "resource not found")
			return
		}
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "database query failed")
		return
	}

	etag := h.cache.Set(cacheKey, data, ttl)
	respond.WriteJSON(w, data, etag, ttl, false)
}

func (h *Handler) serveStatementJSONNoCache(
	w http.ResponseWriter,
	r *http.Request,
	stmt string,
	args ...any,
) {
	if h.pool == nil {
		respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE", "database pool unavailable")
		return
	}

	var data []byte
	err := h.pool.QueryRow(r.Context(), stmt, args...).Scan(&data)
	if err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "database query failed")
		return
	}

	respond.WriteJSON(w, data, "", 0, false)
}

func parseSport(w http.ResponseWriter, r *http.Request) (string, bool) {
	sport := strings.ToLower(strings.TrimSpace(chi.URLParam(r, "sport")))
	if _, ok := validSports[sport]; !ok {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_SPORT", "sport must be one of: nba, nfl, football")
		return "", false
	}
	return sport, true
}

func parseEntityType(w http.ResponseWriter, r *http.Request) (string, bool) {
	entityType := strings.ToLower(strings.TrimSpace(chi.URLParam(r, "entityType")))
	if _, ok := validEntityTypes[entityType]; !ok {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_ENTITY_TYPE", "entityType must be one of: player, team")
		return "", false
	}
	return entityType, true
}

func parsePathID(w http.ResponseWriter, r *http.Request, param string, label string) (int, bool) {
	v := chi.URLParam(r, param)
	id, err := strconv.Atoi(v)
	if err != nil || id <= 0 {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_ID", fmt.Sprintf("%s must be a positive integer", label))
		return 0, false
	}
	return id, true
}

func requiredIntQuery(w http.ResponseWriter, r *http.Request, key string) (int, bool) {
	v := strings.TrimSpace(r.URL.Query().Get(key))
	if v == "" {
		respond.WriteError(w, http.StatusBadRequest, "MISSING_QUERY_PARAM", fmt.Sprintf("%s query parameter is required", key))
		return 0, false
	}
	n, err := strconv.Atoi(v)
	if err != nil {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_QUERY_PARAM", fmt.Sprintf("%s must be an integer", key))
		return 0, false
	}
	return n, true
}

func optionalIntQuery(w http.ResponseWriter, r *http.Request, key string) (any, bool) {
	v := strings.TrimSpace(r.URL.Query().Get(key))
	if v == "" {
		return nil, true
	}
	n, err := strconv.Atoi(v)
	if err != nil {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_QUERY_PARAM", fmt.Sprintf("%s must be an integer", key))
		return nil, false
	}
	return n, true
}

func optionalBoolQuery(w http.ResponseWriter, r *http.Request, key string) (any, bool) {
	v := strings.TrimSpace(r.URL.Query().Get(key))
	if v == "" {
		return nil, true
	}
	b, err := strconv.ParseBool(v)
	if err != nil {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_QUERY_PARAM", fmt.Sprintf("%s must be a boolean", key))
		return nil, false
	}
	return b, true
}

func optionalTextQuery(r *http.Request, key string) any {
	v := strings.TrimSpace(r.URL.Query().Get(key))
	if v == "" {
		return nil
	}
	return v
}

func dataCacheKey(r *http.Request) string {
	if r.URL.RawQuery == "" {
		return "data:" + r.URL.Path
	}
	return "data:" + r.URL.Path + "?" + r.URL.RawQuery
}
