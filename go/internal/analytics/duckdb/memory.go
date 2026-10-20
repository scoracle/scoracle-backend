package duckdb

import (
	"context"
	"encoding/json"
	"fmt"
)

// HeadlineMemoryRequest ranks a frozen, character-filtered source population.
// Full source records stay with the caller; the study returns only their IDs.
type HeadlineMemoryRequest struct {
	Version      string                      `json:"version"`
	Character    string                      `json:"character"`
	From         int64                       `json:"from"`
	Before       int64                       `json:"before"`
	Limit        int                         `json:"limit"`
	Observations []HeadlineMemoryObservation `json:"observations"`
}

type HeadlineMemoryObservation struct {
	ArticleID  int64              `json:"article_id"`
	ReportedAt int64              `json:"reported_at"`
	Spectrum   map[string]float64 `json:"spectrum"`
}

func (a *Analytics) StudyHeadlineMemory(ctx context.Context, req HeadlineMemoryRequest) ([]int64, error) {
	criterion := map[string]string{"journalist": "subject", "influencer": "emotion", "insider": "movement"}[req.Character]
	if criterion == "" || req.Version != "headline-spectrum-memory-v1" || req.From >= req.Before || req.Limit < 1 || req.Limit > 20 || len(req.Observations) > 20000 {
		return nil, fmt.Errorf("invalid headline memory request")
	}
	seen := make(map[int64]bool, len(req.Observations))
	for _, observation := range req.Observations {
		if observation.ArticleID <= 0 || seen[observation.ArticleID] {
			return nil, fmt.Errorf("invalid or duplicate headline memory observation")
		}
		seen[observation.ArticleID] = true
		for _, key := range []string{"subject", criterion} {
			score, ok := observation.Spectrum[key]
			if !ok || !(score >= 0 && score <= 100) {
				return nil, fmt.Errorf("invalid headline memory spectrum")
			}
		}
	}
	raw, err := json.Marshal(req.Observations)
	if err != nil {
		return nil, err
	}
	rows, err := a.database.QueryContext(ctx, `WITH observations AS (
 SELECT (value->>'article_id')::BIGINT article_id,(value->>'reported_at')::BIGINT reported_at,
 json_extract_string(value,?)::DOUBLE lens,json_extract_string(value,'$.spectrum.subject')::DOUBLE subject
 FROM json_each(?)
 ) SELECT article_id FROM observations WHERE reported_at>=? AND reported_at<?
 ORDER BY lens DESC,subject DESC,reported_at DESC,article_id DESC LIMIT ?`,
		"$.spectrum."+criterion, string(raw), req.From, req.Before, req.Limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	result := []int64{}
	for rows.Next() {
		var id int64
		if err := rows.Scan(&id); err != nil {
			return nil, err
		}
		result = append(result, id)
	}
	return result, rows.Err()
}
