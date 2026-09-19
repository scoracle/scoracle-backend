package analytics

import (
	"testing"

	"github.com/albapepper/scoracle-data/internal/analytics/model"
	"github.com/albapepper/scoracle-data/internal/config"
)

func TestWindowSizeMatchesScoutConstants(t *testing.T) {
	cases := []struct {
		events int64
		want   int64
	}{
		{0, 3}, {3, 3}, {29, 3}, {30, 3}, {35, 4}, {120, 12}, {150, 15},
		{160, 16}, {1000, 16},
	}
	for _, c := range cases {
		if got := model.WindowSize(c.events); got != c.want {
			t.Errorf("WindowSize(%d) = %d, want %d", c.events, got, c.want)
		}
	}
}

func TestLinearSlopeMatchesScoutSemantics(t *testing.T) {
	cases := []struct {
		name string
		vals []float64
		want float64
		tol  float64
	}{
		{"rising arithmetic", []float64{1, 2, 3}, 1.0, 1e-12},
		{"falling arithmetic", []float64{3, 2, 1}, -1.0, 1e-12},
		{"constant is steady", []float64{5, 5, 5, 5}, 0.0, 1e-12},
		{"single value is steady", []float64{9}, 0.0, 1e-12},
		{"empty is steady", nil, 0.0, 1e-12},
		{"pair", []float64{1, 3}, 2.0, 1e-12},
		{"noninteger", []float64{10, 11.5, 9, 12, 13.5, 11}, 0.4, 1e-12},
	}
	for _, c := range cases {
		if got := model.LinearSlope(c.vals); !model.Approx(got, c.want, c.tol) {
			t.Errorf("%s: LinearSlope = %v, want %v", c.name, got, c.want)
		}
	}
}

func TestNewTrajectorySparseFallbacks(t *testing.T) {
	sparse := model.NewTrajectory(2, nil, 0)
	if sparse.Key != "steady" || sparse.Reason != "sparse_recent_events" || sparse.SampleSize != 0 {
		t.Errorf("sparse_recent_events fallback wrong: %+v", sparse)
	}
	thin := model.NewTrajectory(10, []float64{5, 4}, 0)
	if thin.Key != "steady" || thin.Reason != "sparse_z_score_events" {
		t.Errorf("sparse_z_score_events fallback wrong: %+v", thin)
	}
}

func TestNewTrajectoryClassifiesAndRounds(t *testing.T) {
	t1 := model.NewTrajectory(100, []float64{3.14159, 2.0, 1.5, 1.0}, 0.3)
	if t1.Key != "rising" || t1.Label != "overall scores trending up over recent games" {
		t.Errorf("rising wrong: %+v", t1)
	}
	if t1.Latest != 3.1 || len(t1.Series) != 4 || t1.Series[0] != 3.1 {
		t.Errorf("rounding wrong: %+v", t1)
	}
	t2 := model.NewTrajectory(100, []float64{1.0, 2.0, 3.0}, -0.26)
	if t2.Key != "falling" {
		t.Errorf("falling wrong: %+v", t2)
	}
	t3 := model.NewTrajectory(100, []float64{1.0, 2.0, 3.0}, 0.25)
	if t3.Key != "steady" {
		t.Errorf("boundary 0.25 must be steady (strict >): %+v", t3)
	}
	t4 := model.NewTrajectory(100, []float64{1.0, 2.0, 3.0}, -0.25)
	if t4.Key != "steady" {
		t.Errorf("boundary -0.25 must be steady (strict <): %+v", t4)
	}
}

func TestOpenRejectsUnknownEngine(t *testing.T) {
	t.Setenv("ANALYTICS_ENGINE", "clickhouse")
	if _, err := config.Load(); err == nil {
		t.Fatal("expected error for unsupported ANALYTICS_ENGINE")
	}
}

func TestOpenDispatchesPostgresEngine(t *testing.T) {
	impl, err := Open(t.Context(), &config.Config{AnalyticsEngine: EnginePostgres}, nil)
	if err != nil {
		t.Fatalf("postgres engine open: %v", err)
	}
	if _, ok := impl.(Analytics); !ok {
		t.Fatalf("postgres implementation does not satisfy the Analytics interface")
	}
	if err := impl.Close(t.Context()); err != nil {
		t.Fatalf("close: %v", err)
	}
}

func TestOpenDispatchesDuckDBEngineWithoutAttachment(t *testing.T) {
	impl, err := Open(t.Context(), &config.Config{AnalyticsEngine: EngineDuckDB}, nil)
	if err != nil {
		t.Fatalf("duckdb engine open: %v", err)
	}
	if _, ok := impl.(Analytics); !ok {
		t.Fatalf("duckdb implementation does not satisfy the Analytics interface")
	}
	if err := impl.Close(t.Context()); err != nil {
		t.Fatalf("close: %v", err)
	}
}
