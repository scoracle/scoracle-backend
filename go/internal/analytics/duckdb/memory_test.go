package duckdb

import (
	"context"
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
