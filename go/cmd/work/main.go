// work — operator CLI for the durable pipeline_work queue (FIRST-GPT-AUDIT
// Session 7). The database is the source of truth for backend derivation work;
// this is the human window onto it.
//
//	go run ./cmd/work status                 # pending/running/failed by stage
//	go run ./cmd/work requeue-stale [lease]  # recover rows abandoned mid-lease
//	                                         # (lease default 15m, e.g. "30m")
//
// Env: DATABASE_PRIVATE_URL (or fallbacks) — see internal/config.
package main

import (
	"context"
	"fmt"
	"os"
	"text/tabwriter"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/joho/godotenv"

	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/work"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}

	_ = godotenv.Load(".env.local", ".env")
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintf(os.Stderr, "config load failed: %v\n", err)
		os.Exit(1)
	}

	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DatabaseURL)
	if err != nil {
		fmt.Fprintf(os.Stderr, "db connect failed: %v\n", err)
		os.Exit(1)
	}
	defer pool.Close()

	switch os.Args[1] {
	case "status":
		runStatus(ctx, pool)
	case "requeue-stale":
		lease := 15 * time.Minute
		if len(os.Args) > 2 {
			d, perr := time.ParseDuration(os.Args[2])
			if perr != nil {
				fmt.Fprintf(os.Stderr, "invalid lease %q: %v\n", os.Args[2], perr)
				os.Exit(2)
			}
			lease = d
		}
		runRequeueStale(ctx, pool, lease)
	default:
		usage()
	}
}

func runStatus(ctx context.Context, pool *pgxpool.Pool) {
	counts, err := work.Counts(ctx, pool)
	if err != nil {
		fmt.Fprintf(os.Stderr, "status failed: %v\n", err)
		os.Exit(1)
	}
	if len(counts) == 0 {
		fmt.Println("pipeline_work is empty — no pending, running, or failed work.")
		return
	}
	tw := tabwriter.NewWriter(os.Stdout, 0, 2, 2, ' ', 0)
	fmt.Fprintln(tw, "STAGE\tSTATUS\tN\tMAX_ATTEMPTS\tOLDEST_AVAILABLE")
	for _, c := range counts {
		oldest := "-"
		if c.Oldest != nil {
			oldest = c.Oldest.UTC().Format(time.RFC3339)
		}
		fmt.Fprintf(tw, "%s\t%s\t%d\t%d\t%s\n", c.Stage, c.Status, c.Count, c.MaxAttempts, oldest)
	}
	_ = tw.Flush()
}

func runRequeueStale(ctx context.Context, pool *pgxpool.Pool, lease time.Duration) {
	n, err := work.RequeueStale(ctx, pool, lease)
	if err != nil {
		fmt.Fprintf(os.Stderr, "requeue-stale failed: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("Requeued %d stale 'running' row(s) (lease %s).\n", n, lease)
}

func usage() {
	fmt.Fprintln(os.Stderr, "usage: work <status | requeue-stale [lease]>")
	os.Exit(2)
}
