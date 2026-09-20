// analytics-snapshot compares the existing cohort formula on one frozen input
// set. Shadow is the default. Publication is an explicit, separately approved
// producer action; no path invokes models or acknowledges dirty work.
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/albapepper/scoracle-data/internal/analytics/duckdb"
	"github.com/albapepper/scoracle-data/internal/analytics/model"
	"github.com/albapepper/scoracle-data/internal/analytics/postgres"
	"github.com/albapepper/scoracle-data/internal/analytics/snapshot"
	"github.com/jackc/pgx/v5"
	"github.com/joho/godotenv"
)

type report struct {
	Snapshot    model.CohortSnapshot     `json:"snapshot"`
	Reference   []model.EntityContextRow `json:"postgres"`
	Candidate   []model.EntityContextRow `json:"duckdb"`
	Durations   map[string]time.Duration `json:"duration_ns"`
	InputBytes  int                      `json:"input_bytes"`
	ResultBytes int                      `json:"result_bytes"`
	Tolerance   float64                  `json:"absolute_tolerance"`
	Published   bool                     `json:"published"`
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func run() error {
	sport := flag.String("sport", "", "required NBA, NFL or FOOTBALL")
	entity := flag.String("entity-type", "", "required player or team")
	season := flag.Int("season", 0, "required cohort season; exports this and prior season")
	asOf := flag.String("as-of", "", "required RFC3339 observation label (not historical time travel)")
	output := flag.String("output", "", "required new report file, includes replayable inputs")
	replay := flag.String("replay", "", "recompute a retained report after loss of DuckDB state")
	refresh := flag.Bool("refresh-all", false, "run the production DuckDB maintainer once; requires -publish; writes a summary, not a replay capture")
	publish := flag.Bool("publish", false, "replace this public cohort (requires migration 262 and production approval)")
	flag.Parse()
	if *refresh && (!*publish || *replay != "") {
		return fmt.Errorf("-refresh-all requires -publish and cannot replay")
	}
	if *output == "" {
		return fmt.Errorf("-output is required")
	}
	// Reserve the report before any publication; never overwrite acceptance evidence.
	file, err := os.OpenFile(*output, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0600)
	if err != nil {
		return err
	}
	defer file.Close()
	_ = godotenv.Load(".env.local")
	url := os.Getenv("DATABASE_PRIVATE_URL")
	if url == "" {
		url = os.Getenv("DATABASE_URL")
	}
	if url == "" {
		return fmt.Errorf("DATABASE_PRIVATE_URL or DATABASE_URL required")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()
	cfg, err := pgx.ParseConfig(url)
	if err != nil {
		return err
	}
	cfg.RuntimeParams["application_name"] = "scoracle-cohort-acceptance"
	if !*publish {
		cfg.RuntimeParams["default_transaction_read_only"] = "on"
	}
	conn, err := pgx.ConnectConfig(ctx, cfg)
	if err != nil {
		return err
	}
	defer conn.Close(ctx)
	if *refresh {
		result, err := snapshot.Refresh(ctx, conn)
		if writeErr := json.NewEncoder(file).Encode(result); writeErr != nil {
			return writeErr
		}
		if syncErr := file.Sync(); syncErr != nil {
			return syncErr
		}
		if err != nil {
			return err
		}
		return json.NewEncoder(os.Stdout).Encode(result)
	}
	r := report{Durations: map[string]time.Duration{}, Tolerance: snapshot.Tolerance}
	start := time.Now()
	if *replay != "" {
		f, err := os.Open(*replay)
		if err != nil {
			return err
		}
		err = json.NewDecoder(f).Decode(&r)
		f.Close()
		if err != nil {
			return err
		}
		r.Durations = map[string]time.Duration{}
		r.Tolerance = snapshot.Tolerance
		r.Published = false
	} else {
		fixed, err := time.Parse(time.RFC3339, *asOf)
		if err != nil {
			return fmt.Errorf("-as-of: %w", err)
		}
		r.Snapshot, err = snapshot.Export(ctx, conn, model.CohortScope{Sport: *sport, EntityType: *entity, Season: int32(*season)}, fixed)
		if err != nil {
			return err
		}
	}
	if err = snapshot.Validate(r.Snapshot); err != nil {
		return err
	}
	r.Durations["export_or_replay"] = time.Since(start)
	raw, _ := json.Marshal(r.Snapshot.Inputs)
	r.InputBytes = len(raw)
	start = time.Now()
	r.Reference, err = postgres.SnapshotContext(ctx, conn, r.Snapshot)
	if err != nil {
		return err
	}
	r.Durations["postgres"] = time.Since(start)
	start = time.Now()
	engine, err := duckdb.Open(ctx, duckdb.Options{MemoryLimit: "256MB"})
	if err != nil {
		return err
	}
	defer engine.Close(ctx)
	r.Candidate, err = engine.SnapshotContext(ctx, r.Snapshot)
	if err != nil {
		return err
	}
	r.Durations["duckdb_load_compute"] = time.Since(start)
	if err = snapshot.Compare(r.Reference, r.Candidate); err != nil {
		return err
	}
	raw, _ = json.Marshal(r.Candidate)
	r.ResultBytes = len(raw)
	// Persist replay evidence before a possible public transaction.
	if err = json.NewEncoder(file).Encode(r); err != nil {
		return err
	}
	if err = file.Sync(); err != nil {
		return err
	}
	if *publish {
		start = time.Now()
		r.Published, err = snapshot.Publish(ctx, conn, r.Snapshot, r.Candidate)
		if err != nil {
			return err
		}
		r.Durations["publication"] = time.Since(start)
	}
	// stdout records the final outcome; the retained file remains the pre-publication
	// replay artifact if the process dies immediately after commit.
	return json.NewEncoder(os.Stdout).Encode(struct {
		Scope       model.CohortScope
		Rows        int
		InputBytes  int
		ResultBytes int
		Published   bool
		DurationNS  map[string]time.Duration
	}{r.Snapshot.Scope, len(r.Candidate), r.InputBytes, r.ResultBytes, r.Published, r.Durations})
}
