// work — operator CLI for the durable pipeline_work queue. The database is the
// source of truth for backend derivation work; this is the human window onto it.
//
//	go run ./cmd/work status                 # pending/running/failed by stage
//	go run ./cmd/work requeue-stale [lease]  # recover rows abandoned mid-lease
//	                                         # (lease default 15m, e.g. "30m")
//	go run ./cmd/work dead-letters [cap]     # work parked past the retry cap +
//	                                         # fixtures at the seed-retry cap (default 3)
//
// Env: DATABASE_PRIVATE_URL (or fallbacks) — see internal/config.
package main

import (
	"context"
	"fmt"
	"os"
	"strconv"
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
	case "dead-letters", "deadletters":
		retryCap := 3 // matches get_pending_fixtures' default p_max_retries
		if len(os.Args) > 2 {
			n, perr := strconv.Atoi(os.Args[2])
			if perr != nil || n < 1 {
				fmt.Fprintf(os.Stderr, "invalid retry cap %q\n", os.Args[2])
				os.Exit(2)
			}
			retryCap = n
		}
		runDeadLetters(ctx, pool, retryCap)
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

// runDeadLetters reports the two dead-letter classes an operator needs to see:
// pipeline_work rows parked past the retry cap, and fixtures stuck at the
// seed-retry cap so get_pending_fixtures no longer selects them.
func runDeadLetters(ctx context.Context, pool *pgxpool.Pool, retryCap int) {
	// 1) pipeline_work dead-letters.
	dls, err := work.DeadLetters(ctx, pool)
	if err != nil {
		fmt.Fprintf(os.Stderr, "dead-letters (pipeline_work) failed: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("== pipeline_work dead-letters (%d) ==\n", len(dls))
	if len(dls) == 0 {
		fmt.Println("(none — no derive work is permanently stuck)")
	} else {
		tw := tabwriter.NewWriter(os.Stdout, 0, 2, 2, ' ', 0)
		fmt.Fprintln(tw, "STAGE\tENTITY\tSPORT\tATTEMPTS\tUPDATED\tLAST_ERROR")
		for _, d := range dls {
			fmt.Fprintf(tw, "%s\t%s/%d\t%s\t%d\t%s\t%s\n",
				d.Stage, d.EntityType, d.EntityID, d.Sport, d.Attempts,
				d.UpdatedAt.UTC().Format(time.RFC3339), oneLine(d.LastError, 80))
		}
		_ = tw.Flush()
	}

	// 2) Fixtures at the seed-retry cap: past their seed window, still unseeded, but
	// excluded from get_pending_fixtures because seed_attempts >= the cap.
	fmt.Printf("\n== fixtures at the seed-retry cap (seed_attempts >= %d) ==\n", retryCap)
	rows, err := pool.Query(ctx, `
		SELECT sport, season, id, seed_attempts, status,
		       COALESCE(NULLIF(last_incomplete_reason, ''), NULLIF(last_seed_error, ''), '') AS reason,
		       start_time
		FROM fixtures
		WHERE seed_attempts >= $1
		  AND status IN ('scheduled', 'completed')
		  AND NOW() >= start_time + (seed_delay_hours || ' hours')::INTERVAL
		ORDER BY start_time DESC`, retryCap)
	if err != nil {
		fmt.Fprintf(os.Stderr, "dead-letters (fixtures) failed: %v\n", err)
		os.Exit(1)
	}
	defer rows.Close()
	tw := tabwriter.NewWriter(os.Stdout, 0, 2, 2, ' ', 0)
	fmt.Fprintln(tw, "SPORT\tSEASON\tFIXTURE\tATTEMPTS\tSTATUS\tSTART\tREASON")
	n := 0
	for rows.Next() {
		var sport, status, reason string
		var season, id, attempts int
		var start time.Time
		if err := rows.Scan(&sport, &season, &id, &attempts, &status, &reason, &start); err != nil {
			fmt.Fprintf(os.Stderr, "scan fixture: %v\n", err)
			os.Exit(1)
		}
		fmt.Fprintf(tw, "%s\t%d\t%d\t%d\t%s\t%s\t%s\n",
			sport, season, id, attempts, status, start.UTC().Format(time.RFC3339), oneLine(reason, 80))
		n++
	}
	if err := rows.Err(); err != nil {
		fmt.Fprintf(os.Stderr, "iterate fixtures: %v\n", err)
		os.Exit(1)
	}
	if n == 0 {
		fmt.Println("(none — no fixture is stuck at the retry cap)")
	} else {
		_ = tw.Flush()
	}
}

// oneLine flattens an error/reason to a single bounded line for the table.
func oneLine(s string, max int) string {
	out := make([]rune, 0, len(s))
	for _, r := range s {
		if r == '\n' || r == '\t' || r == '\r' {
			r = ' '
		}
		out = append(out, r)
	}
	s = string(out)
	if len(s) > max {
		return s[:max] + "…"
	}
	return s
}

func usage() {
	fmt.Fprintln(os.Stderr, "usage: work <status | requeue-stale [lease] | dead-letters [retry-cap]>")
	os.Exit(2)
}
