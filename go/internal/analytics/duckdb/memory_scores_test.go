package duckdb

import (
	"context"
	"encoding/json"
	"testing"
)

func TestScoreHistoryUsesWeeklyMeansAndKeepsMissingRailsUnknown(t *testing.T) {
	ctx := context.Background()
	engine, err := Open(ctx, Options{})
	if err != nil {
		t.Fatal(err)
	}
	defer engine.Close(ctx)
	low, high := 10.0, 30.0
	req := ScoreHistoryRequest{Version: "score-history-v1", Before: 2000000, RatingWeeks: 3,
		Observations: []ScoreObservation{
			{1, "rating", 100, 0, &low}, {2, "rating", 200, 0, &high},
			{3, "rating", 700000, 604800, &high},
			{4, "rating", 800000, 604800, nil},
			{1, "vibe", 100, 0, nil},
			{5, "rating", 2000001, 1814400, &low},
		},
	}
	raw, err := engine.StudyScoreHistory(ctx, req)
	if err != nil {
		t.Fatal(err)
	}
	var got map[string]any
	if err = json.Unmarshal(raw, &got); err != nil {
		t.Fatal(err)
	}
	if got["rating_slope"] != 10.0 || got["rating_samples"] != 3.0 || got["vibe_slope"] != nil || got["vibe_samples"] != 0.0 || got["momentum_score"] != 10.0 {
		t.Fatalf("unexpected score study: %s", raw)
	}
	req.Observations = nil
	raw, err = engine.StudyScoreHistory(ctx, req)
	if err != nil {
		t.Fatal(err)
	}
	if err = json.Unmarshal(raw, &got); err != nil {
		t.Fatal(err)
	}
	if got["rating_slope"] != nil || got["vibe_slope"] != nil || got["momentum_score"] != nil {
		t.Fatalf("empty history became a zero: %s", raw)
	}
}
