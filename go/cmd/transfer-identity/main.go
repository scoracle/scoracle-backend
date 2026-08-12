// transfer-identity is the operator window for applied-transfer current identity.
//
// Usage:
//
//	go run ./cmd/transfer-identity list [-sport NBA] [-player 123] [-status applied] [-source-rumor-id 456] [-limit 50] [-json]
//	go run ./cmd/transfer-identity inspect <application-id>
//	go run ./cmd/transfer-identity revert <application-id> -by operator -reason "bad source"
//
// Env: DATABASE_PRIVATE_URL or DATABASE_URL, optionally from .env.local.
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"strconv"
	"text/tabwriter"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/joho/godotenv"

	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/transferidentity"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}

	_ = godotenv.Load(".env.local")
	cfg, err := config.Load()
	if err != nil {
		fatalf("config load failed: %v", err)
	}

	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DatabaseURL)
	if err != nil {
		fatalf("db connect failed: %v", err)
	}
	defer pool.Close()

	switch os.Args[1] {
	case "list":
		runList(ctx, pool, os.Args[2:])
	case "inspect":
		runInspect(ctx, pool, os.Args[2:])
	case "revert":
		runRevert(ctx, pool, os.Args[2:])
	default:
		usage()
	}
}

func runList(ctx context.Context, pool *pgxpool.Pool, args []string) {
	fs := flag.NewFlagSet("list", flag.ExitOnError)
	sport := fs.String("sport", "", "sport filter: NBA, NFL, FOOTBALL")
	player := fs.Int("player", 0, "player_id filter")
	status := fs.String("status", "", "status filter: applied, reverted, rejected, manual_review, failed_closed")
	sourceRumorID := fs.Int64("source-rumor-id", 0, "source_rumor_id filter")
	limit := fs.Int("limit", 50, "max rows, capped at 500")
	asJSON := fs.Bool("json", false, "emit full JSON records")
	_ = fs.Parse(args)

	f := transferidentity.Filters{
		Sport:  *sport,
		Status: *status,
		Limit:  *limit,
	}
	if *player > 0 {
		f.PlayerID = player
	}
	if *sourceRumorID > 0 {
		f.SourceRumorID = sourceRumorID
	}

	apps, err := transferidentity.List(ctx, pool, f)
	if err != nil {
		fatalf("list failed: %v", err)
	}
	if *asJSON {
		writeJSON(apps)
		return
	}
	if len(apps) == 0 {
		fmt.Println("no transfer identity applications found")
		return
	}

	tw := tabwriter.NewWriter(os.Stdout, 0, 2, 2, ' ', 0)
	fmt.Fprintln(tw, "ID\tCREATED\tSPORT\tPLAYER\tOLD_TEAM\tNEW_TEAM\tSTATUS\tDECISION\tOVERRIDE\tSOURCE_RUMOR\tREASON")
	for _, app := range apps {
		fmt.Fprintf(tw, "%d\t%s\t%s\t%d %s\t%s\t%d %s\t%s\t%s\t%s\t%s\t%s\n",
			app.ID,
			app.CreatedAt.UTC().Format(time.RFC3339),
			app.Sport,
			app.PlayerID,
			oneLine(app.PlayerName, 28),
			teamLabel(app.OldTeamID, app.OldTeamName),
			app.NewTeamID,
			oneLine(app.NewTeamName, 24),
			app.Status,
			app.Decision,
			int64Label(app.OverrideID),
			int64Label(app.SourceRumorID),
			oneLine(app.Reason, 80),
		)
	}
	_ = tw.Flush()
}

func runInspect(ctx context.Context, pool *pgxpool.Pool, args []string) {
	if len(args) != 1 {
		fmt.Fprintln(os.Stderr, "usage: transfer-identity inspect <application-id>")
		os.Exit(2)
	}
	id, err := strconv.ParseInt(args[0], 10, 64)
	if err != nil || id <= 0 {
		fatalf("invalid application id %q", args[0])
	}
	app, err := transferidentity.Get(ctx, pool, id)
	if err != nil {
		if err == pgx.ErrNoRows {
			fatalf("application %d not found", id)
		}
		fatalf("inspect failed: %v", err)
	}
	writeJSON(app)
}

func runRevert(ctx context.Context, pool *pgxpool.Pool, args []string) {
	fs := flag.NewFlagSet("revert", flag.ExitOnError)
	by := fs.String("by", "", "operator identity recorded in reverted_by")
	reason := fs.String("reason", "", "revert reason")
	_ = fs.Parse(args)
	if fs.NArg() != 1 {
		fmt.Fprintln(os.Stderr, "usage: transfer-identity revert <application-id> -by operator -reason reason")
		os.Exit(2)
	}
	if *by == "" || *reason == "" {
		fmt.Fprintln(os.Stderr, "revert requires -by and -reason")
		os.Exit(2)
	}
	id, err := strconv.ParseInt(fs.Arg(0), 10, 64)
	if err != nil || id <= 0 {
		fatalf("invalid application id %q", fs.Arg(0))
	}

	result, err := transferidentity.Revert(ctx, pool, id, *by, *reason)
	if err != nil {
		fatalf("revert failed: %v", err)
	}
	writeJSON(result)
}

func teamLabel(id *int, name string) string {
	if id == nil {
		return "-"
	}
	if name == "" {
		return strconv.Itoa(*id)
	}
	return fmt.Sprintf("%d %s", *id, oneLine(name, 24))
}

func int64Label(v *int64) string {
	if v == nil {
		return "-"
	}
	return strconv.FormatInt(*v, 10)
}

func oneLine(s string, max int) string {
	out := make([]rune, 0, len(s))
	for _, r := range s {
		if r == '\n' || r == '\t' || r == '\r' {
			r = ' '
		}
		out = append(out, r)
	}
	s = string(out)
	if max > 0 && len(s) > max {
		return s[:max] + "..."
	}
	return s
}

func writeJSON(v any) {
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	if err := enc.Encode(v); err != nil {
		fatalf("json encode failed: %v", err)
	}
}

func fatalf(format string, args ...any) {
	fmt.Fprintf(os.Stderr, format+"\n", args...)
	os.Exit(1)
}

func usage() {
	fmt.Fprintln(os.Stderr, "usage: transfer-identity <list | inspect | revert>")
	os.Exit(2)
}
