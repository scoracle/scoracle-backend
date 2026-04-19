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
	Blurb          string          `json:"blurb"`
	ModelVersion   string          `json:"model_version"`
	PromptVersion  string          `json:"prompt_version"`
	GeneratedAt    time.Time       `json:"generated_at"`
}

func (h *Handler) parseVibeParams(r *http.Request) (sport, entityType string, entityID int, err error) {
	sport = strings.ToUpper(chi.URLParam(r, "sport"))
	entityType = chi.URLParam(r, "entityType")
	entityIDStr := chi.URLParam(r, "id")
	entityID, err = strconv.Atoi(entityIDStr)
	return
}

// GetLatestVibe returns the most recent vibe blurb for an entity.
// @Summary Latest vibe blurb
// @Description Returns the most recent Gemma-generated vibe blurb for a player or team. 404 if no blurb has been generated yet.
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

	row := h.pool.QueryRow(r.Context(), `
		SELECT id, entity_type, entity_id, sport, trigger_type, trigger_payload,
		       blurb, model_version, prompt_version, generated_at
		FROM vibe_scores
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
		ORDER BY generated_at DESC
		LIMIT 1
	`, entityType, entityID, sport)

	var v vibeRow
	if err := row.Scan(&v.ID, &v.EntityType, &v.EntityID, &v.Sport,
		&v.TriggerType, &v.TriggerPayload, &v.Blurb,
		&v.ModelVersion, &v.PromptVersion, &v.GeneratedAt); err != nil {
		if err == pgx.ErrNoRows {
			respond.WriteError(w, http.StatusNotFound, "NOT_FOUND",
				"no vibe blurb generated for this entity yet")
			return
		}
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "vibe lookup failed")
		return
	}

	respond.WriteJSONObject(w, http.StatusOK, v)
}

// GetVibeHistory returns the N most recent vibe blurbs for an entity.
// @Summary Vibe blurb history
// @Description Returns the N most recent vibe blurbs for an entity, newest first.
// @Tags vibe
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param limit query int false "Max blurbs (1-50, default 10)"
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

	rows, err := h.pool.Query(r.Context(), `
		SELECT id, entity_type, entity_id, sport, trigger_type, trigger_payload,
		       blurb, model_version, prompt_version, generated_at
		FROM vibe_scores
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
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
			&v.TriggerType, &v.TriggerPayload, &v.Blurb,
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
