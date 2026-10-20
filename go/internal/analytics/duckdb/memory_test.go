package duckdb

import (
	"context"
	"fmt"
	"testing"
)

func TestMemoryFrequency(t *testing.T) {
	ctx := context.Background()
	a, err := Open(ctx, Options{MemoryLimit: "128MB"})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close(ctx)
	req := MemoryRequest{Version: MemoryVersion, From: 100, Before: 200, Limit: 2, PerTopic: 1, Observations: []MemoryObservation{
		{1, 1, "older", "Outlet", 100, "Earlier report"}, {2, 2, "older", "Other", 120, "Follow-up"},
		{3, 3, "newer", "Outlet", 190, "New story"}, {4, 3, "newer", "Copy", 195, "Repost"},
		{5, 5, "outside", "X", 99, "Old"}, {6, 6, "outside", "X", 200, "Fresh"},
	}}
	got, err := a.StudyMemory(ctx, req)
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != 2 || got[0].Topic != "older" || got[0].ArticleCount != 2 || got[0].PublisherCount != 2 || got[0].Reports[0].ArticleID != 2 || len(got[0].SourceIDs) != 2 || got[1].ArticleCount != 1 || got[1].Reports[0].ArticleID != 3 {
		t.Fatalf("unexpected study: %+v", got)
	}
}
func TestMemoryRejectsDuplicateObservation(t *testing.T) {
	ctx := context.Background()
	a, err := Open(ctx, Options{})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close(ctx)
	r := MemoryObservation{1, 1, "x", "p", 1, "headline"}
	_, err = a.StudyMemory(ctx, MemoryRequest{Version: MemoryVersion, From: 0, Before: 2, Limit: 1, PerTopic: 1, Observations: []MemoryObservation{r, r}})
	if err == nil {
		t.Fatal("duplicate accepted")
	}
}

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
