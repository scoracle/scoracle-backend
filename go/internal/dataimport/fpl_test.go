package dataimport

import (
	"testing"
	"time"
)

func TestFPLSeasonUsesEuropeanSeasonStart(t *testing.T) {
	tests := []struct {
		kickoff string
		want    int
	}{
		{"2026-08-24T15:00:00Z", 2026},
		{"2027-01-02T15:00:00Z", 2026},
		{"2027-06-30T15:00:00Z", 2026},
		{"2027-07-01T15:00:00Z", 2027},
	}
	for _, tt := range tests {
		kickoff, err := time.Parse(time.RFC3339, tt.kickoff)
		if err != nil {
			t.Fatal(err)
		}
		if got := fplSeason(kickoff); got != tt.want {
			t.Errorf("fplSeason(%s) = %d, want %d", tt.kickoff, got, tt.want)
		}
	}
}

func TestFPLRoundKeepsGameweekMeaning(t *testing.T) {
	if got := fplRound(nil); got != nil {
		t.Fatalf("fplRound(nil) = %q, want nil", *got)
	}
	event := 4
	got := fplRound(&event)
	if got == nil || *got != "Matchweek 4" {
		t.Fatalf("fplRound(4) = %v, want Matchweek 4", got)
	}
}
