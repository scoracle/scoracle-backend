// validate-stmts — boot the EXACT prepared-statement registration path against a
// target database and exit 0 only if every statement registers cleanly.
//
// db.New runs AfterConnect → registerPreparedStatements → Ping, validating
// every prepared statement's columns/functions against the live schema.
// `go build`/`go vet` do NOT catch a bad column reference inside a prepared
// statement string — this does, without starting any worker or listener.
//
// Uses:
//   - restore-drill.sh — prove a RESTORED database can register the API's
//     statements (i.e. the restored backend would actually boot).
//   - pre-deploy — validate edited reads against the live (or a prod-cloned)
//     schema before release.sh restarts prod.
//   - CI — prepare every statement against a migrated test DB.
//
// Usage:
//
//	go run ./cmd/validate-stmts                       # uses DATABASE_PRIVATE_URL / DATABASE_URL
//	go run ./cmd/validate-stmts -db "postgres://…"    # explicit target (wins over env)
//
// Exit codes: 0 = every statement registered; 1 = config/connect/registration error.
package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/joho/godotenv"

	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/db"
)

func main() {
	dbURL := flag.String("db", "", "target connection string (overrides DATABASE_PRIVATE_URL/DATABASE_URL)")
	flag.Parse()

	// An explicit -db wins; export it BEFORE config.Load so the drill's restored
	// DB is validated, never the ambient prod URL from .env.local.
	if *dbURL != "" {
		os.Setenv("DATABASE_PRIVATE_URL", *dbURL)
	} else {
		// Only fall back to .env files when no explicit target was passed.
		// godotenv.Load never overrides an env var that is already set.
		_ = godotenv.Load(".env.local", ".env")
	}

	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintf(os.Stderr, "validate-stmts: config load failed: %v\n", err)
		os.Exit(1)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	pool, err := db.New(ctx, cfg)
	if err != nil {
		// db.New's AfterConnect runs registerPreparedStatements; a bad column or
		// missing function surfaces here as the registration error.
		fmt.Fprintf(os.Stderr, "validate-stmts: prepared-statement registration FAILED: %v\n", err)
		os.Exit(1)
	}
	defer pool.Close()

	// Belt-and-suspenders: force a fresh connection so AfterConnect runs at least
	// once even if Ping reused an eagerly-opened pool conn.
	conn, err := pool.Acquire(ctx)
	if err != nil {
		fmt.Fprintf(os.Stderr, "validate-stmts: acquire after boot failed: %v\n", err)
		os.Exit(1)
	}
	conn.Release()

	fmt.Println("OK — every prepared statement registered against the target database")
}
