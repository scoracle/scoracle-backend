package duckdb

import (
	"context"
	"testing"
)

func TestStatisticWindowsAndMissing(t *testing.T) {
	for _, missing := range []bool{false, true} {
		ctx := context.Background()
		a, err := Open(ctx, Options{})
		if err != nil {
			t.Fatal(err)
		}
		one, three := 1.0, 3.0
		value := &three
		if missing {
			value = nil
		}
		got, err := a.StudyStatistic(ctx, StatisticRequest{Version: "match-statistic-v1", Metric: "expected_goals_for", Unit: "expected goals", From: 100, Split: 200, Before: 300, Observations: []MatchObservation{{1, 100, &one}, {2, 200, value}, {3, 300, &three}}})
		a.Close(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if got.Previous.Fixtures != 1 || got.Current.Fixtures != 1 || len(got.FixtureIDs) != 2 {
			t.Fatalf("bad windows %+v", got)
		}
		if missing {
			if got.PerMatchChange != nil || got.Current.Total != nil {
				t.Fatalf("missing became measurement %+v", got)
			}
		} else if got.PerMatchChange == nil || *got.PerMatchChange != 2 || *got.PercentChange != 200 {
			t.Fatalf("bad comparison %+v", got)
		}
	}
}
