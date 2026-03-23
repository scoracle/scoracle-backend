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

// GetPlayerPage returns a curated player page payload.
// @Summary Get player page
// @Description Returns a player-focused payload with profile, season stats, and stat definitions.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param id path int true "Player ID"
// @Param season query int false "Season year"
// @Param league_id query int false "Football league filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/players/{id} [get]
func (h *Handler) GetPlayerPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "player id")
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

	stmt := sport + "_player_page"
	args := []any{id, season}
	if sport == "football" {
		args = append(args, leagueID)
	}

	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, true, args...)
}

// GetTeamPage returns a curated team page payload.
// @Summary Get team page
// @Description Returns a team-focused payload with profile, season stats, and stat definitions.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param id path int true "Team ID"
// @Param season query int false "Season year"
// @Param league_id query int false "Football league filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/teams/{id} [get]
func (h *Handler) GetTeamPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "team id")
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

	stmt := sport + "_team_page"
	args := []any{id, season}
	if sport == "football" {
		args = append(args, leagueID)
	}

	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, true, args...)
}

// GetStandingsPage returns standings payload for a sport and season.
// @Summary Get standings page
// @Description Returns standings with sport-specific sort order and filters.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param season query int true "Season year"
// @Param conference query string false "Conference (NBA/NFL)"
// @Param division query string false "Division (NBA/NFL)"
// @Param league_id query int false "League ID (football)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/standings [get]
func (h *Handler) GetStandingsPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	season, ok := requiredIntQuery(w, r, "season")
	if !ok {
		return
	}

	conference := optionalTextQuery(r, "conference")
	division := optionalTextQuery(r, "division")
	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}

	stmt := sport + "_standings_page"
	args := []any{season}
	switch sport {
	case "football":
		if conference != nil || division != nil {
			respond.WriteError(w, http.StatusBadRequest, "INVALID_QUERY", "conference and division are not supported for football standings")
			return
		}
		args = append(args, leagueID)
	default:
		args = append(args, conference, division)
	}

	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false, args...)
}

// GetLeadersPage returns stat leaders payload for a sport.
// @Summary Get stat leaders page
// @Description Returns ranked leaders for a given stat and season.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param season query int true "Season year"
// @Param stat query string true "Stat key"
// @Param limit query int false "Result limit (1-100, default 25)"
// @Param position query string false "Position filter"
// @Param league_id query int false "League ID (required for football)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leaders [get]
func (h *Handler) GetLeadersPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	season, ok := requiredIntQuery(w, r, "season")
	if !ok {
		return
	}

	stat := strings.TrimSpace(r.URL.Query().Get("stat"))
	if stat == "" {
		respond.WriteError(w, http.StatusBadRequest, "MISSING_STAT", "stat query parameter is required")
		return
	}

	limit := 25
	if v := r.URL.Query().Get("limit"); v != "" {
		n, err := strconv.Atoi(v)
		if err != nil || n < 1 || n > 100 {
			respond.WriteError(w, http.StatusBadRequest, "INVALID_LIMIT", "limit must be an integer between 1 and 100")
			return
		}
		limit = n
	}

	position := optionalTextQuery(r, "position")
	leagueID := 0
	leagueValue, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}
	if sport == "football" {
		if leagueValue == nil {
			respond.WriteError(w, http.StatusBadRequest, "MISSING_LEAGUE_ID", "league_id query parameter is required for football leaders")
			return
		}
		leagueID = leagueValue.(int)
	} else if leagueValue != nil {
		leagueID = leagueValue.(int)
	}

	stmt := sport + "_leaders_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false, season, stat, limit, position, leagueID)
}

// GetSearchPage returns search/autofill payload for entities.
// @Summary Search entities
// @Description Returns matching players and teams for the sport.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param q query string true "Search query"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/search [get]
func (h *Handler) GetSearchPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	q := strings.TrimSpace(r.URL.Query().Get("q"))
	if q == "" || len(q) > 100 {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_QUERY", "q is required and must be 1-100 characters")
		return
	}

	stmt := sport + "_search_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false, q)
}

// GetStatDefinitionsPage returns stat definition payload for a sport.
// @Summary Get stat definitions
// @Description Returns stat metadata for players/teams in the selected sport.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entity_type query string false "Entity type" Enums(player, team)
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/stat-definitions [get]
func (h *Handler) GetStatDefinitionsPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	entityType := optionalTextQuery(r, "entity_type")
	if entityType != nil && entityType != "player" && entityType != "team" {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_ENTITY_TYPE", "entity_type must be one of: player, team")
		return
	}

	stmt := sport + "_stat_definitions_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false, entityType)
}

// GetLeaguesPage returns football leagues payload.
// @Summary Get football leagues
// @Description Returns football leagues and optional active/benchmark filtering.
// @Tags data
// @Produce json
// @Param active query bool false "Filter active leagues"
// @Param benchmark query bool false "Filter benchmark leagues"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /football/leagues [get]
func (h *Handler) GetLeaguesPage(w http.ResponseWriter, r *http.Request) {
	active, ok := optionalBoolQuery(w, r, "active")
	if !ok {
		return
	}
	benchmark, ok := optionalBoolQuery(w, r, "benchmark")
	if !ok {
		return
	}

	h.serveStatementJSON(w, r, "football_leagues_page", dataCacheKey(r), cache.TTLData, false, active, benchmark)
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

func parseSport(w http.ResponseWriter, r *http.Request) (string, bool) {
	sport := strings.ToLower(strings.TrimSpace(chi.URLParam(r, "sport")))
	if _, ok := validSports[sport]; !ok {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_SPORT", "sport must be one of: nba, nfl, football")
		return "", false
	}
	return sport, true
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
