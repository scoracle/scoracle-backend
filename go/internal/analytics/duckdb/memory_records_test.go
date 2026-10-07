package duckdb

import (
	"context"
	"encoding/json"
	"testing"
)

func TestSourceRecordsDeduplicatePairsAndPreserveSamples(t *testing.T) {
	ctx := context.Background()
	engine, err := Open(ctx, Options{})
	if err != nil {
		t.Fatal(err)
	}
	defer engine.Close(ctx)
	req := SourceRecordsRequest{Version: "publisher-outcomes-v1", Observations: []SourceOutcome{
		{"Wire", 1, 2, true}, {"Wire", 1, 2, true}, {"Wire", 3, 2, false}, {"Other", 1, 2, false},
	}}
	got, err := engine.StudySourceRecords(ctx, req)
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != 2 || got[0] != (SourceRecord{"Wire", 1, 2, 17}) || got[1] != (SourceRecord{"Other", 0, 1, 0}) {
		t.Fatalf("unexpected publisher study: %+v", got)
	}
	req.Observations[0].PlayerID = 0
	if _, err = engine.StudySourceRecords(ctx, req); err == nil {
		t.Fatal("invalid player accepted")
	}
}

func TestScoutMemoryPreservesCorrectionsAndSourceClaims(t *testing.T) {
	ctx := context.Background()
	engine, err := Open(ctx, Options{})
	if err != nil {
		t.Fatal(err)
	}
	defer engine.Close(ctx)
	record := json.RawMessage(`{"kind":"reverted","date_label":"2026-10-05T10:00:00Z","event_kind":"injury","player_name":"Synthetic Player","event_date_label":"2026-10-01"}`)
	req := ScoutRecordsRequest{Version: "scout-records-v1", EntityType: "player", MaxReports: 4,
		Availability: []json.RawMessage{record, record},
		Personnel:    []json.RawMessage{json.RawMessage(`{"kind":"applied","date_label":"2026-10-04T10:00:00Z","player_name":"Synthetic Player","old_team":"Old Club","new_team":"New Club"}`)},
		Claims:       []json.RawMessage{json.RawMessage(`{"source":"Wire","fact":"Synthetic Player is not injured.","published_at":1791194400}`)},
	}
	got, err := engine.StudyScoutRecords(ctx, req)
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != 3 || got[0].Withdrawn == nil || !*got[0].Withdrawn || got[0].PublishedAt != "2026-10-05T10:00:00Z" ||
		got[0].ReportedHeadline != "An earlier injury-report for The player dated 2026-10-01 was withdrawn as incorrect; it is not evidence of a return" ||
		got[1].ReportedHeadline != "Recorded move for The player from Old Club to New Club" ||
		got[2].ReportedHeadline != "Synthetic Player is not injured." || got[2].Publisher != "Wire" || got[2].Disputed != nil {
		t.Fatalf("unexpected Scout study: %+v", got)
	}
}

func TestScoutMemoryRejectsUnknownRecordKinds(t *testing.T) {
	ctx := context.Background()
	engine, err := Open(ctx, Options{})
	if err != nil {
		t.Fatal(err)
	}
	defer engine.Close(ctx)
	req := ScoutRecordsRequest{Version: "scout-records-v1", EntityType: "player", MaxReports: 4,
		Availability: []json.RawMessage{json.RawMessage(`{"kind":"maybe","date_label":"2026-10-05","event_kind":"injury","player_name":"Synthetic Player","event_date_label":"2026-10-01"}`)},
	}
	if _, err = engine.StudyScoutRecords(ctx, req); err == nil {
		t.Fatal("unknown kind accepted")
	}
}
