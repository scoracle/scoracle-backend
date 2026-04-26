package handler

import (
	"encoding/json"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/jackc/pgx/v5"

	"github.com/albapepper/scoracle-data/internal/api/respond"
)

type vibeRow struct {
	ID             int64           `json:"id"`
	EntityType     string          `json:"entity_type"`
	EntityID       int             `json:"entity_id"`
	Sport          string          `json:"sport"`
	TriggerType    string          `json:"trigger_type"`
	TriggerPayload json.RawMessage `json:"trigger_payload"`
	Sentiment      *int            `json:"sentiment"`
	ModelVersion   string          `json:"model_version"`
	PromptVersion  string          `json:"prompt_version"`
	GeneratedAt    time.Time       `json:"generated_at"`
}

type hottestRow struct {
	EntityType  string    `json:"entity_type"`
	EntityID    int       `json:"entity_id"`
	Sport       string    `json:"sport"`
	Sentiment   int       `json:"sentiment"`
	GeneratedAt time.Time `json:"generated_at"`
}

func (h *Handler) parseVibeParams(r *http.Request) (sport, entityType string, entityID int, err error) {
	sport = strings.ToUpper(chi.URLParam(r, "sport"))
	entityType = chi.URLParam(r, "entityType")
	entityIDStr := chi.URLParam(r, "id")
	entityID, err = strconv.Atoi(entityIDStr)
	return
}

// GetLatestVibe returns the most recent vibe sentiment score for an entity.
// @Summary Latest vibe score
// @Description Returns the most recent Gemma-generated sentiment score (1-100) for a player or team. 404 if no score has been generated yet.
// @Tags vibe
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Success 200 {object} map[string]interface{}
// @Failure 404 {object} respond.ErrorResponse
// @Router /{sport}/vibe/{entityType}/{id} [get]
func (h *Handler) GetLatestVibe(w http.ResponseWriter, r *http.Request) {
	if h.pool == nil {
		respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE", "database pool unavailable")
		return
	}

	sport, entityType, entityID, err := h.parseVibeParams(r)
	if err != nil {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_ENTITY_ID", "id must be an integer")
		return
	}

	// Skip legacy blurb-only rows (sentiment IS NULL) from the pre-v2 era —
	// otherwise an older null-scored row gets surfaced and the frontend
	// shows "Not enough news" when the real story is "no real score yet".
	// Falling back to 404 lets the frontend render the honest "Training"
	// state.
	row := h.pool.QueryRow(r.Context(), `
		SELECT id, entity_type, entity_id, sport, trigger_type, trigger_payload,
		       sentiment, model_version, prompt_version, generated_at
		FROM vibe_scores
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
		  AND sentiment IS NOT NULL
		ORDER BY generated_at DESC
		LIMIT 1
	`, entityType, entityID, sport)

	var v vibeRow
	if err := row.Scan(&v.ID, &v.EntityType, &v.EntityID, &v.Sport,
		&v.TriggerType, &v.TriggerPayload, &v.Sentiment,
		&v.ModelVersion, &v.PromptVersion, &v.GeneratedAt); err != nil {
		if err == pgx.ErrNoRows {
			respond.WriteError(w, http.StatusNotFound, "NOT_FOUND",
				"no vibe score generated for this entity yet")
			return
		}
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "vibe lookup failed")
		return
	}

	respond.WriteJSONObject(w, http.StatusOK, v)
}

// GetVibeHistory returns the N most recent vibe scores for an entity.
// @Summary Vibe score history
// @Description Returns the N most recent vibe sentiment scores for an entity, newest first.
// @Tags vibe
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param limit query int false "Max scores (1-50, default 10)"
// @Success 200 {object} map[string]interface{}
// @Router /{sport}/vibe/{entityType}/{id}/history [get]
func (h *Handler) GetVibeHistory(w http.ResponseWriter, r *http.Request) {
	if h.pool == nil {
		respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE", "database pool unavailable")
		return
	}

	sport, entityType, entityID, err := h.parseVibeParams(r)
	if err != nil {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_ENTITY_ID", "id must be an integer")
		return
	}

	limit := 10
	if l := r.URL.Query().Get("limit"); l != "" {
		if n, err := strconv.Atoi(l); err == nil && n >= 1 && n <= 50 {
			limit = n
		}
	}

	// Same legacy-row filter as GetLatestVibe — pre-v2 blurb-only rows
	// would otherwise pollute the history view.
	rows, err := h.pool.Query(r.Context(), `
		SELECT id, entity_type, entity_id, sport, trigger_type, trigger_payload,
		       sentiment, model_version, prompt_version, generated_at
		FROM vibe_scores
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
		  AND sentiment IS NOT NULL
		ORDER BY generated_at DESC
		LIMIT $4
	`, entityType, entityID, sport, limit)
	if err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "vibe history query failed")
		return
	}
	defer rows.Close()

	out := make([]vibeRow, 0, limit)
	for rows.Next() {
		var v vibeRow
		if err := rows.Scan(&v.ID, &v.EntityType, &v.EntityID, &v.Sport,
			&v.TriggerType, &v.TriggerPayload, &v.Sentiment,
			&v.ModelVersion, &v.PromptVersion, &v.GeneratedAt); err != nil {
			continue
		}
		out = append(out, v)
	}

	respond.WriteJSONObject(w, http.StatusOK, map[string]interface{}{
		"entity_type": entityType,
		"entity_id":   entityID,
		"sport":       sport,
		"count":       len(out),
		"vibes":       out,
	})
}

// GetHottestEntities returns the entities with the highest recent sentiment for a sport.
// @Summary Hottest entities by vibe score
// @Description Returns the entities with the highest recent vibe sentiment in the sport, using each entity's latest score from the last 48 hours.
// @Tags vibe
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType query string false "Filter to player or team" Enums(player, team)
// @Param limit query int false "Max entities (1-50, default 10)"
// @Success 200 {object} map[string]interface{}
// @Router /{sport}/vibe/hottest [get]
func (h *Handler) GetHottestEntities(w http.ResponseWriter, r *http.Request) {
	if h.pool == nil {
		respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE", "database pool unavailable")
		return
	}

	sport := strings.ToUpper(chi.URLParam(r, "sport"))

	var entityTypeFilter *string
	if et := r.URL.Query().Get("entityType"); et == "player" || et == "team" {
		entityTypeFilter = &et
	}

	limit := 10
	if l := r.URL.Query().Get("limit"); l != "" {
		if n, err := strconv.Atoi(l); err == nil && n >= 1 && n <= 50 {
			limit = n
		}
	}

	// DISTINCT ON collapses to each entity's latest scored row in the
	// window; the outer SELECT orders the survivors by score. The partial
	// index idx_vibe_scores_sport_sentiment covers the inner scan.
	rows, err := h.pool.Query(r.Context(), `
		SELECT entity_type, entity_id, sport, sentiment, generated_at
		FROM (
			SELECT DISTINCT ON (entity_type, entity_id)
			       entity_type, entity_id, sport, sentiment, generated_at
			FROM vibe_scores
			WHERE sport = $1
			  AND ($2::text IS NULL OR entity_type = $2)
			  AND sentiment IS NOT NULL
			  AND generated_at > NOW() - INTERVAL '48 hours'
			ORDER BY entity_type, entity_id, generated_at DESC
		) latest
		ORDER BY sentiment DESC, generated_at DESC
		LIMIT $3
	`, sport, entityTypeFilter, limit)
	if err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "hottest entities query failed")
		return
	}
	defer rows.Close()

	out := make([]hottestRow, 0, limit)
	for rows.Next() {
		var hr hottestRow
		if err := rows.Scan(&hr.EntityType, &hr.EntityID, &hr.Sport,
			&hr.Sentiment, &hr.GeneratedAt); err != nil {
			continue
		}
		out = append(out, hr)
	}

	respond.WriteJSONObject(w, http.StatusOK, map[string]interface{}{
		"sport":    sport,
		"count":    len(out),
		"entities": out,
	})
}
