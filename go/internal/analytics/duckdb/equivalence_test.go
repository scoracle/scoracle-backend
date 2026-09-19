// External test package: this is the Phase-1 correctness gate comparing the
// DuckDB engine against the faithful Postgres reproduction on the same
// entities and the same underlying data. Gated on SCORACLE_TEST_DATABASE_URL.
package duckdb_test

import (
	"context"
	"fmt"
	"os"
	"testing"
	"time"

	"github.com/albapepper/scoracle-data/internal/analytics"
	"github.com/albapepper/scoracle-data/internal/analytics/duckdb"
	"github.com/albapepper/scoracle-data/internal/analytics/model"
	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/db"
)

func TestRatingTrajectoryEquivalence(t *testing.T) {
	databaseURL := os.Getenv("TEST_DATABASE_URL")
	if databaseURL == "" {
		t.Skip("TEST_DATABASE_URL not set; equivalence needs the canonical database")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
	defer cancel()

	database, err := duckdb.Open(ctx, duckdb.Options{DatabaseURL: databaseURL, MemoryLimit: "512MB"})
	if err != nil {
		t.Fatalf("open duckdb engine: %v", err)
	}
	defer database.Close(ctx)

	pool, err := db.New(ctx, &config.Config{
		DatabaseURL:    databaseURL,
		DBPoolMinConns: 1,
		DBPoolMaxConns: 2,
	})
	if err != nil {
		t.Fatalf("open postgres pool: %v", err)
	}
	defer pool.Close()

	pg := analytics.NewPostgres(pool)
	defer pg.Close(ctx)

	entities := pickEntities(ctx, t, pool)
	if len(entities) == 0 {
		t.Skip("no rated event entities in the database yet")
	}

	t.Logf("equivalence over %d entities", len(entities))
	for _, e := range entities {
		t.Run(fmt.Sprintf("%s/%s/%d/%d", e.entityType, e.sport, e.season, e.entityID), func(t *testing.T) {
			want, err := pg.RatingTrajectory(ctx, e.entityType, e.entityID, e.sport, e.season)
			if err != nil {
				t.Fatalf("postgres path: %v", err)
			}
			got, err := database.RatingTrajectory(ctx, e.entityType, e.entityID, e.sport, e.season)
			if err != nil {
				t.Fatalf("duckdb path: %v", err)
			}
			assertEquivalent(t, want, got)
		})
	}
}

type entityKey struct {
	entityType string
	sport      string
	season     int32
	entityID   int32
}

// pickEntities takes the most-scored entities plus a random tail so the set
// covers both heavy and sparse samples.
func pickEntities(ctx context.Context, t *testing.T, pool *db.Pool) []entityKey {
	t.Helper()

	entities := []entityKey{}
	appendFrom := func(query string) {
		rows, err := pool.Query(ctx, query)
		if err != nil {
			t.Fatalf("pick entities: %v", err)
		}
		defer rows.Close()
		for rows.Next() {
			var e entityKey
			var scratch int64
			if err := rows.Scan(&e.entityType, &e.sport, &e.season, &e.entityID, &scratch); err != nil {
				t.Fatalf("scan entity: %v", err)
			}
			entities = append(entities, entityKey{e.entityType, e.sport, e.season, e.entityID})
		}
		if err := rows.Err(); err != nil {
			t.Fatalf("iterate entities: %v", err)
		}
	}

	appendFrom(`
		(SELECT 'player' AS entity_type, e.sport, e.season, e.player_id AS entity_id, COUNT(*) AS n
		   FROM public.event_box_scores e
		  WHERE e.rating IS NOT NULL
		  GROUP BY 1, 2, 3, 4
		 HAVING COUNT(*) >= 3
		  ORDER BY n DESC
		  LIMIT 10)
		UNION ALL
		(SELECT 'team' AS entity_type, e.sport, e.season, e.team_id, COUNT(*)
		   FROM public.event_team_stats e
		  WHERE e.rating IS NOT NULL
		  GROUP BY 1, 2, 3, 4
		 HAVING COUNT(*) >= 3
		  ORDER BY COUNT(*) DESC
		  LIMIT 5)
	`)
	appendFrom(`
		SELECT 'player', e.sport, e.season, e.player_id, COUNT(*)
		  FROM public.event_box_scores e
		 WHERE e.rating IS NOT NULL
		 GROUP BY 1, 2, 3, 4
		HAVING COUNT(*) >= 3
		 ORDER BY random()
		 LIMIT 15
	`)
	return entities
}

const (
	slopeTolerance = 1e-6
	valueTolerance = 1e-9
)

// BenchmarkRatingTrajectory is the Phase-2 latency read on one heavy entity
// (the most-scored player) through both engines. Gated on TEST_DATABASE_URL.
func BenchmarkRatingTrajectory(b *testing.B) {
	databaseURL := os.Getenv("TEST_DATABASE_URL")
	if databaseURL == "" {
		b.Skip("TEST_DATABASE_URL not set; benchmark needs the canonical database")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
	defer cancel()

	database, err := duckdb.Open(ctx, duckdb.Options{DatabaseURL: databaseURL, MemoryLimit: "512MB"})
	if err != nil {
		b.Fatalf("open duckdb engine: %v", err)
	}
	defer database.Close(ctx)

	pool, err := db.New(ctx, &config.Config{
		DatabaseURL:    databaseURL,
		DBPoolMinConns: 1,
		DBPoolMaxConns: 2,
	})
	if err != nil {
		b.Fatalf("open postgres pool: %v", err)
	}
	defer pool.Close()

	pg := analytics.NewPostgres(pool)
	defer pg.Close(ctx)

	var (
		entityID int32
		sport    string
		season   int32
		scratch  int64
	)
	err = pool.QueryRow(ctx, `
		SELECT e.player_id, e.sport, e.season, COUNT(*)
		  FROM public.event_box_scores e
		 WHERE e.rating IS NOT NULL
		 GROUP BY 1, 2, 3
		 ORDER BY COUNT(*) DESC
		 LIMIT 1
	`).Scan(&entityID, &sport, &season, &scratch)
	if err != nil {
		b.Fatalf("pick benchmark entity: %v", err)
	}
	b.Logf("benchmark entity player/%s/%d/%d", sport, season, entityID)

	b.Run("postgres", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			if _, err := pg.RatingTrajectory(ctx, "player", entityID, sport, season); err != nil {
				b.Fatalf("postgres path: %v", err)
			}
		}
	})
	b.Run("duckdb", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			if _, err := database.RatingTrajectory(ctx, "player", entityID, sport, season); err != nil {
				b.Fatalf("duckdb path: %v", err)
			}
		}
	})
}

func assertEquivalent(t *testing.T, want, got model.Trajectory) {
	t.Helper()
	if want.Key != got.Key {
		t.Errorf("key mismatch: postgres %q vs duckdb %q", want.Key, got.Key)
	}
	if want.Label != got.Label {
		t.Errorf("label mismatch: postgres %q vs duckdb %q", want.Label, got.Label)
	}
	if want.Reason != got.Reason {
		t.Errorf("reason mismatch: postgres %q vs duckdb %q", want.Reason, got.Reason)
	}
	if want.EventsPlayed != got.EventsPlayed {
		t.Errorf("events played mismatch: postgres %d vs duckdb %d", want.EventsPlayed, got.EventsPlayed)
	}
	if want.WindowSize != got.WindowSize {
		t.Errorf("window size mismatch: postgres %d vs duckdb %d", want.WindowSize, got.WindowSize)
	}
	if want.SampleSize != got.SampleSize {
		t.Errorf("sample size mismatch: postgres %d vs duckdb %d", want.SampleSize, got.SampleSize)
	}
	if !model.Approx(want.Slope, got.Slope, slopeTolerance) {
		t.Errorf("slope mismatch beyond %v: postgres %v vs duckdb %v", slopeTolerance, want.Slope, got.Slope)
	}
	if len(want.Series) != len(got.Series) {
		t.Fatalf("series length mismatch: postgres %d vs duckdb %d", len(want.Series), len(got.Series))
	}
	for i := range want.Series {
		if !model.Approx(want.Series[i], got.Series[i], valueTolerance) {
			t.Errorf("series[%d] mismatch beyond %v: postgres %v vs duckdb %v", i, valueTolerance, want.Series[i], got.Series[i])
		}
	}
}

// bundleCohorts: (sport, season, rate_mode) pairs for the bundle gate — all
// three sports across old/new seasons (era + early-season gate variety) and
// every rate mode on one season.
var bundleCohorts = []struct {
	sport    string
	season   int32
	rateMode string
}{
	{"NBA", 2024, "total"},
	{"NBA", 2018, "total"},
	{"NBA", 2024, "per_36"},
	{"NBA", 2024, "per_season"},
	{"FOOTBALL", 2023, "total"},
	{"FOOTBALL", 2026, "total"},
	{"FOOTBALL", 2023, "per_90"},
	{"NFL", 2023, "total"},
	{"NFL", 2018, "total"},
	{"NFL", 2023, "per_game"},
}

// TestRatingBundleEquivalence is the Phase-1b correctness gate: DuckDB's
// re-computation of the migration-253 bundle must match the live production
// function on the same data, per cohort.
func TestRatingBundleEquivalence(t *testing.T) {
	databaseURL := os.Getenv("TEST_DATABASE_URL")
	if databaseURL == "" {
		t.Skip("TEST_DATABASE_URL not set; equivalence needs the canonical database")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Minute)
	defer cancel()

	database, err := duckdb.Open(ctx, duckdb.Options{DatabaseURL: databaseURL, MemoryLimit: "1GB"})
	if err != nil {
		t.Fatalf("open duckdb engine: %v", err)
	}
	defer database.Close(ctx)

	pool, err := db.New(ctx, &config.Config{
		DatabaseURL:    databaseURL,
		DBPoolMinConns: 1,
		DBPoolMaxConns: 2,
	})
	if err != nil {
		t.Fatalf("open postgres pool: %v", err)
	}
	defer pool.Close()

	pg := analytics.NewPostgres(pool)
	defer pg.Close(ctx)

	for _, c := range bundleCohorts {
		t.Run(fmt.Sprintf("%s/%d/%s", c.sport, c.season, c.rateMode), func(t *testing.T) {
			want, err := pg.RatingBundle(ctx, c.sport, c.season, c.rateMode)
			if err != nil {
				t.Fatalf("postgres path: %v", err)
			}
			got, err := database.RatingBundle(ctx, c.sport, c.season, c.rateMode)
			if err != nil {
				t.Fatalf("duckdb path: %v", err)
			}
			assertBundlesEquivalent(t, want, got)
		})
	}
}

// normalizeScoped drops null entries and empty maps so DuckDB's
// always-four-key json_object matches Postgres's jsonb_strip_nulls +
// NULLIF('{}').
func normalizeScoped(scoped map[string]*float64) map[string]*float64 {
	out := map[string]*float64{}
	for k, v := range scoped {
		if v != nil {
			out[k] = v
		}
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

func isEmptyScoped(scoped map[string]*float64) bool {
	return normalizeScoped(scoped) == nil
}

func assertBundlesEquivalent(t *testing.T, want, got []model.BundleRow) {
	t.Helper()
	if len(want) != len(got) {
		t.Fatalf("row count mismatch: postgres %d vs duckdb %d", len(want), len(got))
	}
	index := map[[2]int32]model.BundleRow{}
	for _, row := range got {
		key := [2]int32{row.PlayerID, row.LeagueID}
		if _, dup := index[key]; dup {
			t.Fatalf("duckdb returned duplicate (player,league) %v", key)
		}
		index[key] = row
	}
	for _, w := range want {
		g, ok := index[[2]int32{w.PlayerID, w.LeagueID}]
		if !ok {
			t.Fatalf("duckdb missing player %d league %d", w.PlayerID, w.LeagueID)
		}
		assertNilFloat(t, w.Composite, g.Composite, "composite", w.PlayerID, 4e-4)
		assertNilFloat(t, w.CompositeRank, g.CompositeRank, "composite_rank", w.PlayerID, 0.1001)
		assertNilFloat(t, w.CompositeScore, g.CompositeScore, "composite_score", w.PlayerID, 0.1001)
		assertScoped(t, normalizeScoped(w.ScopedRanks), normalizeScoped(g.ScopedRanks), "scoped_ranks", w.PlayerID)
		assertScoped(t, normalizeScoped(w.ScopedScores), normalizeScoped(g.ScopedScores), "scoped_scores", w.PlayerID)
		assertBreakdown(t, w.Breakdown, g.Breakdown, w.PlayerID)
	}
}

func assertNilFloat(t *testing.T, want, got *float64, field string, playerID int32, tol float64) {
	t.Helper()
	switch {
	case want == nil && got == nil:
		return
	case want == nil || got == nil:
		t.Fatalf("player %d %s: nil mismatch postgres %v vs duckdb %v", playerID, field, want, got)
	case !model.Approx(*want, *got, tol):
		t.Errorf("player %d %s beyond %v: postgres %v vs duckdb %v", playerID, field, tol, *want, *got)
	}
}

func assertScoped(t *testing.T, want, got map[string]*float64, field string, playerID int32) {
	t.Helper()
	if (want == nil) != (got == nil) {
		t.Fatalf("player %d %s: presence mismatch postgres %v vs duckdb %v", playerID, field, want, got)
	}
	for k, w := range want {
		g := got[k]
		switch {
		case g == nil:
			t.Errorf("player %d %s[%s]: duckdb nil", playerID, field, k)
		case !model.Approx(*w, *g, 0.1001):
			t.Errorf("player %d %s[%s] beyond 0.1: postgres %v vs duckdb %v", playerID, field, k, *w, *g)
		}
	}
}

func assertBreakdown(t *testing.T, want, got []model.BreakdownEntry, playerID int32) {
	t.Helper()
	if len(want) != len(got) {
		t.Fatalf("player %d breakdown length: postgres %d vs duckdb %d", playerID, len(want), len(got))
	}
	byKey := map[string]model.BreakdownEntry{}
	for _, e := range got {
		byKey[e.Label] = e
	}
	for _, w := range want {
		g, ok := byKey[w.Label]
		if !ok {
			t.Fatalf("player %d breakdown missing label %q", playerID, w.Label)
		}
		if w.Measure != g.Measure {
			t.Errorf("player %d %s: measure mismatch postgres %q vs duckdb %q", playerID, w.Label, w.Measure, g.Measure)
		}
		if w.Eligible != g.Eligible {
			t.Errorf("player %d %s: eligible mismatch postgres %v vs duckdb %v", playerID, w.Label, w.Eligible, g.Eligible)
		}
		if w.InComp != g.InComp || w.InSpec != g.InSpec || w.Sign != g.Sign || w.Facet != g.Facet {
			t.Errorf("player %d %s: identity mismatch postgres %+v vs duckdb %+v", playerID, w.Label, w, g)
		}
		assertNilFloat(t, w.Value, g.Value, w.Label+"/value", playerID, 1e-9)
		assertNilFloat(t, w.Z, g.Z, w.Label+"/z", playerID, 1e-9)
		assertNilFloat(t, w.Pct, g.Pct, w.Label+"/pct", playerID, 0.1001)
		if (w.ScopedPct == nil) != (g.ScopedPct == nil) && !isEmptyScoped(w.ScopedPct) && !isEmptyScoped(g.ScopedPct) {
			t.Errorf("player %d %s: scoped_pct presence mismatch", playerID, w.Label)
		}
		for k, wv := range normalizeScoped(w.ScopedPct) {
			gv := g.ScopedPct[k]
			if gv == nil {
				t.Errorf("player %d %s scoped_pct[%s]: duckdb missing", playerID, w.Label, k)
				continue
			}
			if !model.Approx(*wv, *gv, 0.1001) {
				t.Errorf("player %d %s scoped_pct[%s] beyond 0.1: postgres %v vs duckdb %v", playerID, w.Label, k, *wv, *gv)
			}
		}
	}
}

// BenchmarkRatingBundle is the Phase-2 latency read on the bundle workload:
// the live Postgres function versus the DuckDB re-computation. Runs the two
// heaviest cohorts. Gated on TEST_DATABASE_URL.
func BenchmarkRatingBundle(b *testing.B) {
	databaseURL := os.Getenv("TEST_DATABASE_URL")
	if databaseURL == "" {
		b.Skip("TEST_DATABASE_URL not set; benchmark needs the canonical database")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Minute)
	defer cancel()

	database, err := duckdb.Open(ctx, duckdb.Options{DatabaseURL: databaseURL, MemoryLimit: "1GB"})
	if err != nil {
		b.Fatalf("open duckdb engine: %v", err)
	}
	defer database.Close(ctx)

	pool, err := db.New(ctx, &config.Config{
		DatabaseURL:    databaseURL,
		DBPoolMinConns: 1,
		DBPoolMaxConns: 2,
	})
	if err != nil {
		b.Fatalf("open postgres pool: %v", err)
	}
	defer pool.Close()

	pg := analytics.NewPostgres(pool)
	defer pg.Close(ctx)

	for _, cohort := range []struct {
		name   string
		sport  string
		season int32
		mode   string
	}{
		{"NBA2024_total", "NBA", 2024, "total"},
		{"FOOTBALL2023_total", "FOOTBALL", 2023, "total"},
	} {
		b.Run(cohort.name, func(b *testing.B) {
			b.Run("postgres", func(b *testing.B) {
				for i := 0; i < b.N; i++ {
					if _, err := pg.RatingBundle(ctx, cohort.sport, cohort.season, cohort.mode); err != nil {
						b.Fatalf("postgres path: %v", err)
					}
				}
			})
			b.Run("duckdb", func(b *testing.B) {
				for i := 0; i < b.N; i++ {
					if _, err := database.RatingBundle(ctx, cohort.sport, cohort.season, cohort.mode); err != nil {
						b.Fatalf("duckdb path: %v", err)
					}
				}
			})
		})
	}
}

// TestBenchmarkSweep times both engines once per cohort (total mode) and logs
// the full comparison table for the Phase-2 write-up. Gated on
// TEST_DATABASE_URL.
func TestBenchmarkSweep(t *testing.T) {
	if os.Getenv("DUCKDB_BENCHMARK_SWEEP") == "" {
		t.Skip("DUCKDB_BENCHMARK_SWEEP not set; run explicitly to produce the sweep table")
	}
	databaseURL := os.Getenv("TEST_DATABASE_URL")
	if databaseURL == "" {
		t.Skip("TEST_DATABASE_URL not set")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Minute)
	defer cancel()

	database, err := duckdb.Open(ctx, duckdb.Options{DatabaseURL: databaseURL, MemoryLimit: "1GB"})
	if err != nil {
		t.Fatalf("open duckdb engine: %v", err)
	}
	defer database.Close(ctx)

	pool, err := db.New(ctx, &config.Config{DatabaseURL: databaseURL, DBPoolMinConns: 1, DBPoolMaxConns: 2})
	if err != nil {
		t.Fatalf("open postgres pool: %v", err)
	}
	defer pool.Close()

	pg := analytics.NewPostgres(pool)
	defer pg.Close(ctx)

	rows, err := pool.Query(ctx, `
		SELECT DISTINCT sport, season
		FROM public.player_stats
		WHERE stats::text <> '{}'
		ORDER BY sport, season
	`)
	if err != nil {
		t.Fatalf("list cohorts: %v", err)
	}
	type cohort struct {
		sport  string
		season int32
	}
	var cohorts []cohort
	for rows.Next() {
		var c cohort
		if err := rows.Scan(&c.sport, &c.season); err != nil {
			t.Fatalf("scan cohort: %v", err)
		}
		cohorts = append(cohorts, c)
	}
	rows.Close()

	for _, c := range cohorts {
		start := time.Now()
		if _, err := pg.RatingBundle(ctx, c.sport, c.season, "total"); err != nil {
			t.Fatalf("postgres %s/%d: %v", c.sport, c.season, err)
		}
		pgMs := float64(time.Since(start).Microseconds()) / 1000.0

		start = time.Now()
		if _, err := database.RatingBundle(ctx, c.sport, c.season, "total"); err != nil {
			t.Fatalf("duckdb %s/%d: %v", c.sport, c.season, err)
		}
		duckMs := float64(time.Since(start).Microseconds()) / 1000.0
		t.Logf("bundle %s/%d: postgres %.0f ms | duckdb %.0f ms", c.sport, c.season, pgMs, duckMs)
	}
}
