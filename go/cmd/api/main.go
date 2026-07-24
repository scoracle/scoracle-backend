// Command api is the Scoracle Data API server.
//
// Usage:
//
//	scoracle-api
//	API_PORT=8080 scoracle-api

// @title Scoracle Data API
// @version 2.0.0
// @description Unified Scoracle API serving sport data pages, derived products (narratives, transfer heat, Vibe, Sigil — precomputed by the Rust cognition layer), health checks, and operational endpoints.
// @host localhost:8000
// @BasePath /api/v1
// @schemes http https
// @contact.name Scoracle
// @license.name MIT
package main

import (
	"context"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/joho/godotenv"

	"github.com/albapepper/scoracle-data/internal/api"
	"github.com/albapepper/scoracle-data/internal/buildinfo"
	"github.com/albapepper/scoracle-data/internal/cache"
	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/db"
	"github.com/albapepper/scoracle-data/internal/listener"
	"github.com/albapepper/scoracle-data/internal/maintenance"
	"github.com/albapepper/scoracle-data/internal/notifications"

	_ "github.com/albapepper/scoracle-data/docs" // swagger docs
)

func main() {
	logger := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))
	slog.SetDefault(logger)
	logger.Info("Scoracle Data API build", "commit", buildinfo.Commit, "built", buildinfo.BuildTime)

	// Load .env.local (real values, gitignored) then .env (committed template).
	// godotenv does not overwrite already-set vars, so .env.local wins.
	_ = godotenv.Load(".env.local", ".env")

	// Load configuration
	cfg, err := config.Load()
	if err != nil {
		logger.Error("Failed to load configuration", "error", err)
		os.Exit(1)
	}

	// Context with signal handling. SIGINT (os.Interrupt) covers a Ctrl-C in an
	// interactive shell; SIGTERM is what systemd `stop`/`restart` and container
	// runtimes send — without it the process would be SIGKILLed after the stop
	// timeout instead of shutting down gracefully.
	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer cancel()

	// Connect to database
	logger.Info("Connecting to database...")
	pool, err := db.New(ctx, cfg)
	var dbPool *pgxpool.Pool
	if err != nil {
		// Every serving endpoint is a precomputed read from Postgres, so a
		// database-less API serves nothing useful. In production we fail fast and
		// let systemd's Restart=always bring us straight back when Postgres
		// returns, rather than parking a healthy-looking but useless process.
		// Non-production keeps degraded startup so local dev / CI can boot the
		// HTTP surface without a database.
		if cfg.Environment == "production" {
			logger.Error("Failed to connect to database; refusing to start in production", "error", err)
			os.Exit(1)
		}
		logger.Error("Failed to connect to database", "error", err)
		logger.Warn("Starting in degraded mode without database connectivity", "environment", cfg.Environment)
	} else {
		dbPool = pool.Pool
		defer pool.Close()
		logger.Info("Database connected",
			"min_conns", cfg.DBPoolMinConns,
			"max_conns", cfg.DBPoolMaxConns)
	}

	// Initialize cache
	appCache := cache.New(cfg.CacheEnabled)
	logger.Info("Cache initialized", "enabled", cfg.CacheEnabled)

	// The API serves precomputed reads from Postgres — it performs no model
	// calls on serving requests. Every LLM derivation stage is owned by the
	// Rust Cognition Harness (scoracle-cognition daemon + the rust/bin
	// statcommentary rating batch), which drains the durable pipeline_work
	// queue. Go's background workers below are model-free: FCM notification
	// dispatch, the percentile LISTEN worker (FCM push + durable sigil-convergence
	// enqueue), and the SQL-only maintenance tickers.
	if dbPool != nil {
		// Start notification dispatch worker (if FCM is configured)
		fcmSender := notifications.NewFCMSender(cfg.FCMCredentialsFile, logger)
		if fcmSender != nil {
			go notifications.StartWorker(ctx, dbPool, fcmSender, logger)
			logger.Info("Notification dispatch worker started")
		} else {
			logger.Info("Notification dispatch worker disabled (no FIREBASE_CREDENTIALS_FILE)")
		}

		// Percentile listener: FCM push on significant stat-line percentile
		// crossings. A large composite shift also enqueues durable Sigil
		// convergence work for the Rust cognition daemon.
		go listener.Start(ctx, cfg.DatabaseURL, dbPool, fcmSender, logger)

		// Start maintenance workers (cleanup, catch-up, ranks, news
		// scrub auto-vet + enqueue, pipeline stats, peer cohorts, momentum dirty-queue drain). The scrub
		// ticker is SQL-only: auto-vets primaries + enqueues candidate-rich
		// secondaries to pipeline_work for the Rust ScrubHandler.
		mc := maintenance.DefaultConfig()
		if !cfg.NewsScrubEnabled {
			mc.NewsScrubInterval = 0 // disable the scrub ticker
		}
		mc.StatsInterval = cfg.PipelineStatsInterval
		if !cfg.BoxscoreBackfillEnabled {
			mc.BoxscoreBackfillInterval = 0
		} else {
			mc.BoxscoreBackfillInterval = cfg.BoxscoreBackfillInterval
			mc.BoxscoreBackfillBatch = cfg.BoxscoreBackfillBatch
		}
		go maintenance.Start(ctx, dbPool, mc, logger)
	} else {
		logger.Warn("Database-backed background workers disabled in degraded mode")
	}

	// Create router
	router := api.NewRouter(dbPool, appCache, cfg)

	// Create HTTP server
	addr := fmt.Sprintf("%s:%d", cfg.APIHost, cfg.APIPort)
	srv := &http.Server{
		Addr:        addr,
		Handler:     router,
		ReadTimeout: 10 * time.Second,
		// Keep WriteTimeout disabled so long-lived streaming endpoints can stay
		// open. This is required for OpenCode's browser/SSE sessions through the
		// reverse proxy; net/http does not support path-specific write timeouts.
		WriteTimeout: 0,
		IdleTimeout:  60 * time.Second,
	}

	// Start server in background
	go func() {
		logger.Info("Starting Scoracle Data API",
			"addr", addr,
			"environment", cfg.Environment,
			"docs", fmt.Sprintf("http://localhost:%d/docs/", cfg.APIPort))
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			logger.Error("Server failed", "error", err)
			os.Exit(1)
		}
	}()

	// Wait for interrupt
	<-ctx.Done()
	logger.Info("Shutting down...")

	// Graceful shutdown with timeout
	shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer shutdownCancel()

	if err := srv.Shutdown(shutdownCtx); err != nil {
		logger.Error("Shutdown error", "error", err)
	}
	logger.Info("Server stopped")
}
