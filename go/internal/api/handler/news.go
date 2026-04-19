package handler

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"strings"

	"github.com/go-chi/chi/v5"

	"github.com/albapepper/scoracle-data/internal/api/respond"
	"github.com/albapepper/scoracle-data/internal/cache"
)

// GetEntityNews returns news articles for an entity (player or team).
// @Summary Get entity news
// @Description Returns news articles about a player or team from Google News RSS. Supports time-window escalation and strict name matching.
// @Tags news
// @Produce json
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param entityID path int true "Entity ID"
// @Param sport query string true "Sport identifier" Enums(NBA, NFL, FOOTBALL)
// @Param team query string false "Team name for player context"
// @Param limit query int false "Max articles (1-50, default 10)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Failure 502 {object} respond.ErrorResponse
// @Router /news/{entityType}/{entityID} [get]
func (h *Handler) GetEntityNews(w http.ResponseWriter, r *http.Request) {
	if h.pool == nil {
		respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE", "database pool unavailable")
		return
	}

	entityType := chi.URLParam(r, "entityType")
	entityIDStr := chi.URLParam(r, "entityID")

	if entityType != "player" && entityType != "team" {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_ENTITY_TYPE",
			"entityType must be 'player' or 'team'")
		return
	}

	entityID, err := strconv.Atoi(entityIDStr)
	if err != nil {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_ENTITY_ID",
			"entityID must be an integer")
		return
	}

	sport := strings.ToUpper(r.URL.Query().Get("sport"))
	if sport == "" {
		respond.WriteError(w, http.StatusBadRequest, "MISSING_SPORT",
			"sport query parameter is required")
		return
	}

	limit := 10
	if l := r.URL.Query().Get("limit"); l != "" {
		if n, err := strconv.Atoi(l); err == nil && n >= 1 && n <= 50 {
			limit = n
		}
	}

	team := r.URL.Query().Get("team")

	// Check cache first.
	cacheKey := fmt.Sprintf("news:%s:%d:%s:%d", entityType, entityID, sport, limit)
	if data, etag, ok := h.cache.Get(cacheKey); ok {
		if cache.CheckETagMatch(r.Header.Get("If-None-Match"), etag) {
			respond.WriteNotModified(w, etag)
			return
		}
		respond.WriteJSON(w, data, etag, cache.TTLNews, true)
		return
	}

	// Look up entity name from DB.
	var entityName, firstName, lastName string
	var aliases []string

	ctx := r.Context()

	if entityType == "player" {
		entityName, firstName, lastName, team, aliases, err = h.lookupPlayer(ctx, entityID, sport, team)
	} else {
		entityName, aliases, err = h.lookupTeam(ctx, entityID, sport)
	}
	if err != nil {
		if strings.Contains(err.Error(), "not found") {
			respond.WriteError(w, http.StatusNotFound, "NOT_FOUND", err.Error())
		} else {
			respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "Failed to look up entity")
		}
		return
	}

	// Fetch news. Handler also write-throughs matched articles into
	// news_articles + news_article_entities for the Gemma corpus.
	result, err := h.news.GetEntityNews(ctx, entityType, entityID, entityName, sport, team, limit, firstName, lastName, aliases)
	if err != nil {
		respond.WriteError(w, http.StatusBadGateway, "EXTERNAL_SERVICE_ERROR",
			fmt.Sprintf("News fetch failed: %v", err))
		return
	}

	// Add entity info.
	result["entity"] = map[string]interface{}{
		"type":  entityType,
		"id":    entityID,
		"name":  entityName,
		"sport": sport,
	}

	// Marshal and cache.
	data, err := json.Marshal(result)
	if err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "MARSHAL_ERROR", "Failed to encode response")
		return
	}

	etag := h.cache.Set(cacheKey, data, cache.TTLNews)
	respond.WriteJSON(w, data, etag, cache.TTLNews, false)
}

// GetNewsStatus returns news service configuration status.
// @Summary News service status
// @Description Returns configuration state for the Google News RSS source.
// @Tags news
// @Produce json
// @Success 200 {object} map[string]interface{}
// @Router /news/status [get]
func (h *Handler) GetNewsStatus(w http.ResponseWriter, r *http.Request) {
	respond.WriteJSONObject(w, http.StatusOK, h.news.Status())
}

// ---------------------------------------------------------------------------
// DB lookups
// ---------------------------------------------------------------------------

// lookupPlayer fetches player name, first/last names, aliases, and resolves team if missing.
func (h *Handler) lookupPlayer(ctx context.Context, id int, sport, team string) (name, first, last, teamName string, aliases []string, err error) {
	row := h.pool.QueryRow(ctx, "player_news_lookup", id, sport)
	var teamID *int
	var fnPtr, lnPtr *string
	if err = row.Scan(&name, &fnPtr, &lnPtr, &teamID, &aliases); err != nil {
		return "", "", "", "", nil, fmt.Errorf("player %d not found", id)
	}
	if fnPtr != nil {
		first = *fnPtr
	}
	if lnPtr != nil {
		last = *lnPtr
	}

	// Resolve team name if not provided.
	teamName = team
	if teamName == "" && teamID != nil {
		var tn string
		if err := h.pool.QueryRow(ctx,
			"team_name_lookup",
			*teamID, sport,
		).Scan(&tn); err == nil {
			teamName = tn
		}
	}
	return name, first, last, teamName, aliases, nil
}

// lookupTeam fetches team name and search aliases.
func (h *Handler) lookupTeam(ctx context.Context, id int, sport string) (string, []string, error) {
	var name string
	var aliases []string
	err := h.pool.QueryRow(ctx, "team_news_lookup", id, sport).Scan(&name, &aliases)
	if err != nil {
		return "", nil, fmt.Errorf("team %d not found", id)
	}
	return name, aliases, nil
}
