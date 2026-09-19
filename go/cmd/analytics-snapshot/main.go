// analytics-snapshot is the Phase-3 batch job of the DuckDB analytics POC:
// it runs the DuckDB cohort-context computation and writes the derived,
// season-grain rows back into Postgres (public.analytics_entity_context,
// migration 255). DuckDB itself stays strictly read-only — this job owns the
// only write. Postgres remains canonical: the rows are recomputable from
// player_stats/team_stats ratings and nothing operational depends on them
// except memories.rs, which reads them as sourced records.
//
// Usage (from go/):
//
//	DATABASE_PRIVATE_URL=… go run ./cmd/analytics-snapshot
package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/albapepper/scoracle-data/internal/analytics/duckdb"
	"github.com/albapepper/scoracle-data/internal/analytics/model"
	"github.com/jackc/pgx/v5"
	"github.com/joho/godotenv"
)

func main() {
	only := flag.String("sport", "", "restrict to one sport (optional)")
	seasonFlag := flag.Int("season", 0, "restrict to one season (optional)")
	flag.Parse()

	_ = godotenv.Load(".env.local")
	databaseURL := os.Getenv("DATABASE_PRIVATE_URL")
	if databaseURL == "" {
		databaseURL = os.Getenv("DATABASE_URL")
	}
	if databaseURL == "" {
		fmt.Fprintln(os.Stderr, "DATABASE_PRIVATE_URL or DATABASE_URL must be set")
		os.Exit(1)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Minute)
	defer cancel()

	engine, err := duckdb.Open(ctx, duckdb.Options{
		DatabaseURL: databaseURL,
		Path:        os.Getenv("ANALYTICS_DUCKDB_PATH"),
		MemoryLimit: os.Getenv("ANALYTICS_DUCKDB_MEMORY_LIMIT"),
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "open duckdb engine: %v\n", err)
		os.Exit(1)
	}
	defer engine.Close(ctx)

	poolCfg, err := pgx.ParseConfig(databaseURL)
	if err != nil {
		fmt.Fprintf(os.Stderr, "parse database URL: %v\n", err)
		os.Exit(1)
	}
	conn, err := pgx.ConnectConfig(ctx, poolCfg)
	if err != nil {
		fmt.Fprintf(os.Stderr, "connect postgres: %v\n", err)
		os.Exit(1)
	}
	defer conn.Close(ctx)

	if err := run(ctx, engine, conn, *only, *seasonFlag); err != nil {
		fmt.Fprintf(os.Stderr, "snapshot: %v\n", err)
		os.Exit(1)
	}
}

func run(ctx context.Context, engine *duckdb.Analytics, conn *pgx.Conn, onlySport string, onlySeason int) error {
	type cohort struct {
		sport  string
		season int32
	}
	var cohorts []cohort
	for entityType, source := range map[string]string{"player": "player_stats", "team": "team_stats"} {
		rows, err := conn.Query(ctx, fmt.Sprintf(`
			SELECT DISTINCT sport, season FROM public.%s
			WHERE rating IS NOT NULL
			ORDER BY sport, season
		`, source))
		if err != nil {
			return fmt.Errorf("list %s cohorts: %w", entityType, err)
		}
		for rows.Next() {
			var c cohort
			if err := rows.Scan(&c.sport, &c.season); err != nil {
				rows.Close()
				return fmt.Errorf("scan %s cohort: %w", entityType, err)
			}
			cohorts = append(cohorts, c)
		}
		rows.Close()
		if err := rows.Err(); err != nil {
			return fmt.Errorf("iterate %s cohorts: %w", entityType, err)
		}
	}

	start := time.Now()
	var total int64
	for _, c := range cohorts {
		if onlySport != "" && c.sport != onlySport {
			continue
		}
		if onlySeason != 0 && c.season != int32(onlySeason) {
			continue
		}
		for _, entityType := range []string{"player", "team"} {
			rows, err := engine.EntityContext(ctx, c.sport, c.season, entityType)
			if err != nil {
				return fmt.Errorf("context %s/%d/%s: %w", c.sport, c.season, entityType, err)
			}
			written, err := writeRows(ctx, conn, rows)
			if err != nil {
				return fmt.Errorf("write %s/%d/%s: %w", c.sport, c.season, entityType, err)
			}
			total += written
			fmt.Printf("%-9s %d %-6s %4d rows\n", c.sport, c.season, entityType, written)
		}
	}
	fmt.Printf("done: %d rows in %s\n", total, time.Since(start).Round(time.Millisecond))
	return nil
}

func writeRows(ctx context.Context, conn *pgx.Conn, rows []duckdbRow) (int64, error) {
	if len(rows) == 0 {
		return 0, nil
	}
	batch := &pgx.Batch{}
	for _, r := range rows {
		batch.Queue(`
			INSERT INTO public.analytics_entity_context (
				sport, entity_type, entity_id, season, league_id,
				rating, prior_season, prior_rating, delta, delta_pctile,
				peer_count, peer_delta_median, peer_delta_p25, peer_delta_p75, computed_at
			) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14, now())
			ON CONFLICT (sport, entity_type, entity_id, season, league_id) DO UPDATE SET
				rating = EXCLUDED.rating,
				prior_season = EXCLUDED.prior_season,
				prior_rating = EXCLUDED.prior_rating,
				delta = EXCLUDED.delta,
				delta_pctile = EXCLUDED.delta_pctile,
				peer_count = EXCLUDED.peer_count,
				peer_delta_median = EXCLUDED.peer_delta_median,
				peer_delta_p25 = EXCLUDED.peer_delta_p25,
				peer_delta_p75 = EXCLUDED.peer_delta_p75,
				computed_at = now()
		`,
			r.Sport, r.EntityType, r.EntityID, r.Season, r.LeagueID,
			r.Rating, r.PriorSeason, r.PriorRating, r.Delta, r.DeltaPctile,
			r.PeerCount, r.PeerDeltaMedian, r.PeerDeltaP25, r.PeerDeltaP75)
	}
	results := conn.SendBatch(ctx, batch)
	defer results.Close()
	var written int64
	for range rows {
		ct, err := results.Exec()
		if err != nil {
			return written, err
		}
		written += ct.RowsAffected()
	}
	return written, nil
}

// duckdbRow aliases the model row so the job depends only on the engine and
// the model shape.
type duckdbRow = model.EntityContextRow
