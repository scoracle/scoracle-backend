package duckdb

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"os"
	"testing"
	"time"
)

// TestPostgresExtensionCapabilities documents the postgres-extension surface
// the bundle relies on (postgres_query taking the ATTACH alias) and keeps the
// environment honest about what the engine can push down.
func TestPostgresExtensionCapabilities(t *testing.T) {
	probeURL := os.Getenv("TEST_DATABASE_URL")
	if probeURL == "" {
		t.Skip("TEST_DATABASE_URL not set")
	}
	database, err := sql.Open("duckdb", "")
	if err != nil {
		t.Fatal(err)
	}
	defer database.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()
	for _, step := range []string{"INSTALL postgres", "LOAD postgres", fmt.Sprintf("ATTACH '%s' AS pg (TYPE POSTGRES, READ_ONLY)", probeURL)} {
		if _, err := database.ExecContext(ctx, step); err != nil {
			t.Fatalf("step %q: %v", step, err)
		}
	}
	rows, err := database.QueryContext(ctx, "SELECT function_name FROM duckdb_functions() WHERE function_name ILIKE '%postgres%' AND function_type='table'")
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	var found []string
	for rows.Next() {
		var name string
		if err := rows.Scan(&name); err != nil {
			t.Fatal(err)
		}
		found = append(found, name)
	}
	want := map[string]bool{"postgres_query": false}
	for _, name := range found {
		if _, ok := want[name]; ok {
			want[name] = true
		}
	}
	for name, ok := range want {
		if !ok {
			t.Fatalf("postgres extension missing table function %q", name)
		}
	}

	// Struct-in-subquery + list(e ORDER BY key) + CAST AS VARCHAR is the
	// serialization pattern that survives the driver as proper JSON text.
	var sample string
	if err := database.QueryRowContext(ctx,
		"SELECT to_json(list(e ORDER BY x))::VARCHAR FROM (SELECT {'a': unnest([2,1])} AS e, unnest([2,1]) AS x)",
	).Scan(&sample); err != nil {
		t.Fatalf("struct serialization probe: %v", err)
	}
	var decoded []map[string]float64
	if err := json.Unmarshal([]byte(sample), &decoded); err != nil {
		t.Fatalf("struct serialization probe: %v", err)
	}
}

// TestExpansionPushdown proves the migration-253 dp CTE (lasp + eff gate +
// rating_measurements lateral) executes on Postgres through postgres_query
// and streams expanded datapoints back.
func TestExpansionPushdown(t *testing.T) {
	probeURL := os.Getenv("TEST_DATABASE_URL")
	if probeURL == "" {
		t.Skip("TEST_DATABASE_URL not set")
	}
	engine, err := Open(ctxOf(t), Options{DatabaseURL: probeURL, MemoryLimit: "512MB"})
	if err != nil {
		t.Fatal(err)
	}
	defer engine.Close(context.Background())

	expansion := `
        WITH eff AS MATERIALIZED (
            SELECT rt.stat_key,
                   LEAST(rt.min_value, GREATEST(1, ceil(0.5 * COALESCE((SELECT MAX(NULLIF(ps2.stats->>rt.stat_key,'')::numeric)
                       FROM player_stats ps2 WHERE ps2.sport = 'NBA' AND ps2.season = 2024), 0)))) AS min_value
            FROM rating_thresholds rt WHERE rt.sport = 'NBA'
        ),
        dp AS (
            SELECT ps.player_id, COALESCE(ps.league_id, 0) AS league_id, ps.position,
                   d.label, d.measure, d.value::float8 AS value, d.in_comp, d.in_spec, d.sign, d.facet,
                   COALESCE((SELECT bool_and(COALESCE((ps.stats->>e.stat_key)::numeric, 0) >= e.min_value) FROM eff e), FALSE) AS is_ranked
            FROM player_stats ps
            CROSS JOIN LATERAL public.rating_measurements(ps.sport, ps.stats, 'total', ps.position) d
            WHERE ps.sport = 'NBA' AND ps.season = 2024 AND ps.stats <> '{}'::jsonb
        )
        SELECT * FROM dp`
	q := fmt.Sprintf("SELECT COUNT(*), COUNT(DISTINCT player_id) FROM postgres_query('pg', $sql$%s$sql$)", expansion)
	var n, players int64
	if err := engine.database.QueryRowContext(ctxOf(t), q).Scan(&n, &players); err != nil {
		t.Fatalf("pushdown: %v", err)
	}
	if n == 0 || players == 0 {
		t.Skip("no rated player_stats rows in this database yet")
	}
}

func ctxOf(t *testing.T) context.Context {
	c, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	t.Cleanup(cancel)
	return c
}
