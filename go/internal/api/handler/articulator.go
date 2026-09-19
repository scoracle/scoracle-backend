package handler

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/jackc/pgx/v5"

	"github.com/albapepper/scoracle-data/internal/api/respond"
	"github.com/albapepper/scoracle-data/internal/articulator"
	"github.com/albapepper/scoracle-data/internal/cache"
)

// GetArticulatorSlice serves the exact compact DATA string used to train the
// on-device Articulator. Teams and players share the product boundary.
//
// The request reads only precomputed product statements. Composition is
// deterministic Go code mirroring scoracle-articulator's slim_teams.py and
// build_prompts.py; no model, provider, or client-side derivation enters the
// serving path.
// @Summary Get an on-device Articulator DATA slice
// @Description Returns a byte-stable compact JSON string composed from precomputed entity products. kind is p1 through p8; p6 also returns followup_data.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param kind path string true "Training slice" Enums(p1, p2, p3, p4, p5, p6, p7, p8)
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/{entityType}/{id}/articulator/{kind} [get]
func (h *Handler) GetArticulatorSlice(w http.ResponseWriter, r *http.Request) {
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
	kind := chi.URLParam(r, "kind")
	if !validArticulatorKind(kind) {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_ARTICULATOR_KIND", "kind must be p1 through p8")
		return
	}
	if h.pool == nil {
		respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE", "database pool unavailable")
		return
	}

	cacheKey := dataCacheKey(r)
	if data, etag, found := h.cache.Get(cacheKey); found {
		if cache.CheckETagMatch(r.Header.Get("If-None-Match"), etag) {
			respond.WriteNotModified(w, etag)
			return
		}
		respond.WriteJSON(w, data, etag, cache.TTLData, true)
		return
	}

	value, err, _ := h.flight.Do(cacheKey, func() (any, error) {
		if data, etag, found := h.cache.Get(cacheKey); found {
			return cachedPayload{data: data, etag: etag}, nil
		}
		ctx, cancel := context.WithTimeout(context.Background(), dbQueryTimeout)
		defer cancel()

		inputs, err := h.loadArticulatorInputs(ctx, sport, entityType, id, kind)
		if err != nil {
			return nil, err
		}
		slice, err := articulator.Compose(kind, inputs)
		if err != nil {
			return nil, err
		}
		payload := struct {
			Kind   string `json:"kind"`
			Entity struct {
				Sport      string `json:"sport"`
				EntityType string `json:"entity_type"`
				EntityID   int    `json:"entity_id"`
				Name       string `json:"name"`
			} `json:"entity"`
			Data         string  `json:"data"`
			FollowupData *string `json:"followup_data,omitempty"`
		}{Kind: kind, Data: slice.Data, FollowupData: slice.FollowupData}
		payload.Entity.Sport = sport
		payload.Entity.EntityType = entityType
		payload.Entity.EntityID = id
		payload.Entity.Name = slice.EntityName

		data, err := json.Marshal(payload)
		if err != nil {
			return nil, err
		}
		etag := h.cache.Set(cacheKey, data, cache.TTLData)
		return cachedPayload{data: data, etag: etag}, nil
	})
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			respond.WriteError(w, http.StatusNotFound, "NOT_FOUND", "entity not found")
			return
		}
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "failed to compose articulator slice")
		return
	}
	result := value.(cachedPayload)
	respond.WriteJSON(w, result.data, result.etag, cache.TTLData, false)
}

func validArticulatorKind(kind string) bool {
	return len(kind) == 2 && kind[0] == 'p' && kind[1] >= '1' && kind[1] <= '8'
}

func (h *Handler) loadArticulatorInputs(
	ctx context.Context, sport, entityType string, entityID int, kind string,
) (articulator.Inputs, error) {
	var inputs articulator.Inputs
	query := func(target *[]byte, statement string, args ...any) error {
		return h.pool.QueryRow(ctx, statement, args...).Scan(target)
	}

	// Identity is required for every envelope and every DATA shape.
	if err := query(&inputs.Meta, "entity_meta", sport, entityType, entityID); err != nil {
		return inputs, err
	}

	if kind == "p1" || kind == "p2" || kind == "p3" || kind == "p6" {
		if err := query(&inputs.Rating, "entity_rating", sport, entityType, entityID, nil, nil); err != nil {
			return inputs, err
		}
	}
	if kind == "p1" || kind == "p3" || kind == "p6" || (kind == "p4" && entityType == "player") {
		if err := query(&inputs.Momentum, sport+"_trends_page", entityType, entityID, nil, nil); err != nil {
			return inputs, err
		}
	}
	if kind == "p3" {
		if err := query(&inputs.MomentumSummary, "entity_momentum", sport, entityType, entityID, nil); err != nil {
			return inputs, err
		}
	}
	if kind == "p4" && entityType == "team" {
		if err := query(&inputs.Results, sport+"_team_results", entityID, nil, nil); err != nil {
			return inputs, err
		}
	}
	if kind == "p1" || kind == "p5" || kind == "p6" {
		if err := query(&inputs.News, "entity_news", sport, entityType, entityID, nil, nil, nil); err != nil {
			return inputs, err
		}
	}
	if kind == "p1" || kind == "p6" || kind == "p7" {
		if err := query(&inputs.Vibe, "entity_vibe", sport, entityType, entityID); err != nil {
			return inputs, err
		}
	}
	if kind == "p8" {
		if err := query(&inputs.Transfers, "entity_transfers", sport, entityType, entityID, nil, nil, nil); err != nil {
			return inputs, err
		}
	}
	return inputs, nil
}
