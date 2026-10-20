package duckdb

import (
	"context"
	"fmt"
	"testing"
)

func TestHeadlineMemoriesUseCharacterSpectraAndStableTies(t *testing.T) {
	ctx := context.Background()
	engine, err := Open(ctx, Options{MemoryLimit: "128MB"})
	if err != nil {
		t.Fatal(err)
	}
	defer engine.Close(ctx)
	req := HeadlineMemoryRequest{Version: "headline-spectrum-memory-v1", From: 100, Before: 200, Limit: 20,
		Observations: []HeadlineMemoryObservation{
			{1, 100, map[string]float64{"subject": 99, "emotion": 20, "movement": 80}},
			{2, 190, map[string]float64{"subject": 95, "emotion": 99, "movement": 83}},
			{3, 150, map[string]float64{"subject": 90, "emotion": 85, "movement": 99}},
			{4, 200, map[string]float64{"subject": 100, "emotion": 100, "movement": 100}},
			{5, 99, map[string]float64{"subject": 100, "emotion": 100, "movement": 100}},
			{6, 110, map[string]float64{"subject": 99, "emotion": 20, "movement": 80}},
			{7, 110, map[string]float64{"subject": 99, "emotion": 20, "movement": 80}},
			{8, 100, map[string]float64{"subject": 96, "emotion": 99, "movement": 99}},
		}}
	for character, want := range map[string]string{"journalist": "[7 6 1 8 2 3]", "influencer": "[8 2 3 7 6 1]", "insider": "[8 3 2 7 6 1]"} {
		req.Character = character
		got, err := engine.StudyHeadlineMemory(ctx, req)
		if err != nil || fmt.Sprint(got) != want {
			t.Fatalf("%s: %v %v, want %s", character, got, err, want)
		}
	}
	req.Character = "unknown"
	if _, err := engine.StudyHeadlineMemory(ctx, req); err == nil {
		t.Fatal("unknown character accepted")
	}
	req.Character = "influencer"
	delete(req.Observations[0].Spectrum, "emotion")
	if _, err := engine.StudyHeadlineMemory(ctx, req); err == nil {
		t.Fatal("missing spectrum accepted")
	}
}
