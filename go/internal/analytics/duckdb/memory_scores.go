package duckdb

import (
	"context"
	"encoding/json"
	"fmt"
	"math"
)

type ScoreObservation struct {
	ID         int64    `json:"id"`
	Rail       string   `json:"rail"`
	ObservedAt int64    `json:"observed_at"`
	WeekStart  int64    `json:"week_start"`
	Value      *float64 `json:"value"`
}
type ScoreHistoryRequest struct {
	Kind         string             `json:"kind"`
	Version      string             `json:"version"`
	Before       int64              `json:"before"`
	RatingWeeks  int                `json:"rating_weeks"`
	Observations []ScoreObservation `json:"observations"`
}

// StudyScoreHistory studies unchanged Scout notability and Influencer sentiment
// records. Weekly means give each reporting week equal weight. Null is unknown.
func (a *Analytics) StudyScoreHistory(ctx context.Context, req ScoreHistoryRequest) (json.RawMessage, error) {
	if req.Version != "score-history-v1" || req.RatingWeeks < 2 || req.RatingWeeks > 20 || len(req.Observations) > 20000 {
		return nil, fmt.Errorf("invalid score history request")
	}
	seen := map[string]bool{}
	for _, r := range req.Observations {
		key := fmt.Sprintf("%s/%d", r.Rail, r.ID)
		if r.ID <= 0 || (r.Rail != "rating" && r.Rail != "vibe") || seen[key] || (r.Value != nil && (math.IsNaN(*r.Value) || math.IsInf(*r.Value, 0) || *r.Value < 0 || *r.Value > 100)) {
			return nil, fmt.Errorf("invalid score observation")
		}
		seen[key] = true
	}
	raw, err := json.Marshal(req.Observations)
	if err != nil {
		return nil, err
	}
	var result string
	err = a.database.QueryRowContext(ctx, `WITH observations AS (
        SELECT value->>'rail' rail,(value->>'observed_at')::BIGINT observed_at,
        (value->>'week_start')::BIGINT week_start,(value->>'value')::DOUBLE score FROM json_each(?)
    ), weekly AS (
        SELECT rail,week_start,avg(score) mean,count(score) samples,
          min(observed_at) first_at,max(observed_at) last_at FROM observations
        WHERE observed_at<=? AND score IS NOT NULL AND week_start<=?
          AND (rail='rating' OR week_start+604800> ?-1814400)
        GROUP BY rail,week_start
    ), ranked AS (
        SELECT *,row_number() OVER(PARTITION BY rail ORDER BY week_start DESC) rank FROM weekly
    ), studied AS (
        SELECT rail,(first(mean ORDER BY week_start DESC)-first(mean ORDER BY week_start)) delta,
          sum(samples) samples,min(first_at) first_at,max(last_at) last_at
        FROM ranked WHERE rail='vibe' OR rank<=? GROUP BY rail
        HAVING count(*)>=2 AND (rail='rating' OR sum(samples)>=3)
    ) SELECT json_object(
        'rating_slope',max(delta) FILTER(WHERE rail='rating'),
        'rating_samples',coalesce(max(samples) FILTER(WHERE rail='rating'),0),
        'rating_window_start',strftime(to_timestamp(max(first_at) FILTER(WHERE rail='rating')),'%Y-%m-%dT%H:%M:%SZ'),
        'rating_window_end',strftime(to_timestamp(max(last_at) FILTER(WHERE rail='rating')),'%Y-%m-%dT%H:%M:%SZ'),
        'vibe_slope',max(delta) FILTER(WHERE rail='vibe'),
        'vibe_samples',coalesce(max(samples) FILTER(WHERE rail='vibe'),0),
        'vibe_window_start',strftime(to_timestamp(max(first_at) FILTER(WHERE rail='vibe')),'%Y-%m-%dT%H:%M:%SZ'),
        'vibe_window_end',strftime(to_timestamp(max(last_at) FILTER(WHERE rail='vibe')),'%Y-%m-%dT%H:%M:%SZ'),
        'momentum_score',avg(delta),
        'generated_at',strftime(to_timestamp(?),'%Y-%m-%dT%H:%M:%SZ'))::VARCHAR
        FROM studied`, string(raw), req.Before, req.Before, req.Before, req.RatingWeeks, req.Before).Scan(&result)
	return json.RawMessage(result), err
}
