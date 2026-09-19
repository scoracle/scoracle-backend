package duckdb

import (
	"context"
	"strings"
	"testing"
)

// TestTrajectoryQueryContract pins the DuckSQL contract in repo test style.
func TestTrajectoryQueryContract(t *testing.T) {
	needles := []string{
		"pg.public.%[1]s e",
		"JOIN pg.public.fixtures f ON f.id = e.fixture_id",
		"e.%[2]s = $1 AND e.sport = $2 AND e.season = $3",
		"e.rating IS NOT NULL",
		"COUNT(*) OVER () AS events_played",
		"ROW_NUMBER() OVER (ORDER BY start_time DESC, rating DESC) AS recent_idx",
		"ROW_NUMBER() OVER (ORDER BY start_time ASC, rating DESC) AS chrono_idx",
		"GREATEST(%[3]d, LEAST(%[4]d, CAST(ROUND(events_played * %[5]f) AS BIGINT)))",
		"regr_slope(rating, chrono_idx)",
		"ORDER BY chrono_idx DESC",
	}
	for _, needle := range needles {
		if !strings.Contains(trajectoryQuery, needle) {
			t.Fatalf("trajectory query missing %q", needle)
		}
	}
	if !strings.Contains(attachQuery, "TYPE POSTGRES, READ_ONLY") {
		t.Fatalf("attach query missing READ_ONLY TYPE POSTGRES: %q", attachQuery)
	}
}

// TestUnsupportedEntityTypeRejects pins the unsupported-entity-type boundary
// (the Rust Scout falls back to steady("unknown_entity_type"); the Go
// implementations return an error at the interface).
func TestUnsupportedEntityTypeRejects(t *testing.T) {
	impl, err := Open(context.Background(), Options{})
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer impl.Close(context.Background())
	if _, err := impl.RatingTrajectory(context.Background(), "person", 1, "NBA", 2026); err == nil {
		t.Fatal("expected error for unsupported entity type")
	}
}
