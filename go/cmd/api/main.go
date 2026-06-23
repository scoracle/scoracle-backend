// Command api is the Scoracle Data API server.
//
// Usage:
//
//	scoracle-api
//	API_PORT=8080 scoracle-api

// @title Scoracle Data API
// @version 2.0.0
// @description Unified Scoracle API serving sport data pages, news, journalist tweets, health checks, and operational endpoints.
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
	"github.com/albapepper/scoracle-data/internal/derive"
	"github.com/albapepper/scoracle-data/internal/listener"
	"github.com/albapepper/scoracle-data/internal/maintenance"
	"github.com/albapepper/scoracle-data/internal/ml"
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

	// synthGen is set inside the dbPool block when Ollama is reachable; stays nil
	// otherwise. It feeds the percentile listener's composite_shift Sigil trigger.
	var synthGen *ml.SigilGenerator

	if dbPool != nil {
		// Start notification dispatch worker (if FCM is configured)
		fcmSender := notifications.NewFCMSender(cfg.FCMCredentialsFile, logger)
		if fcmSender != nil {
			go notifications.StartWorker(ctx, dbPool, fcmSender, logger)
			logger.Info("Notification dispatch worker started")
		} else {
			logger.Info("Notification dispatch worker disabled (no FIREBASE_CREDENTIALS_FILE)")
		}

		// Gemma generators, gated on Ollama being reachable at boot. They stay nil
		// when Ollama is unreachable — which disables the derive worker + the
		// maintenance scrub (degraded mode) while the rest of the API keeps serving.
		var narrator *ml.NewsNarrator
		var vibeGen *ml.VibeGenerator
		var transferGen *ml.TransferGenerator
		var newsScrubber *ml.NewsScrubber
		ollamaCli := ml.NewOllamaClient(cfg.OllamaBaseURL, cfg.OllamaModel, cfg.OllamaTimeout)
		pingCtx, pingCancel := context.WithTimeout(ctx, 3*time.Second)
		if err := ollamaCli.Ping(pingCtx); err != nil {
			logger.Warn("Gemma workers disabled (Ollama unreachable)",
				"base_url", cfg.OllamaBaseURL, "error", err)
		} else {
			logger.Info("Gemma workers enabled", "model", cfg.OllamaModel)
			narrator = ml.NewNewsNarrator(dbPool, ollamaCli)
			vibeGen = ml.NewVibeGenerator(dbPool, ollamaCli)
			transferGen = ml.NewTransferGenerator(dbPool, ollamaCli)
			synthGen = ml.NewSigilGenerator(dbPool, ollamaCli)
			newsScrubber = ml.NewNewsScrubber(dbPool, ollamaCli)
		}
		pingCancel()

		// Percentile listener: FCM push on significant stat-line percentile crossings.
		// A large composite shift also ENQUEUES durable Sigil convergence work
		// (FIRST-GPT-AUDIT Session 12, F-017) — drained by the derive worker below, not
		// generated inline — so it no longer needs the SigilGenerator.
		go listener.Start(ctx, cfg.DatabaseURL, dbPool, fcmSender, logger)

		// Real-time derive worker (FIRST-GPT-AUDIT Session 9): drains the durable
		// pipeline_work queue, woken by NOTIFY pipeline_work_ready (the migration-103
		// vetted-transition trigger), and on startup + a safety-net interval so a
		// missed NOTIFY never costs correctness. Replaces the old news-volume +
		// transfer LISTEN workers that ran Gemma directly off a transient NOTIFY.
		// Started only when the generators exist — otherwise every claimed item would
		// fail and burn its retries; durable work simply waits for the next start.
		switch {
		case !cfg.DeriveWorkerEnabled:
			logger.Info("Real-time derive worker disabled (DERIVE_WORKER_ENABLED=false)")
		case vibeGen == nil:
			logger.Warn("Real-time derive worker disabled (Ollama unreachable) — durable work drains on next start with Ollama up")
		default:
			drainer := &derive.Drainer{
				Pool:         dbPool,
				TransferGen:  transferGen,
				Narrator:     narrator,
				VibeGen:      vibeGen,
				SynthGen:     synthGen,
				GemmaTimeout: cfg.OllamaTimeout,
				MinArticles:  cfg.TransferMinArticles,
				Logger:       logger,
			}
			go derive.StartWorker(ctx, cfg.DatabaseURL, drainer, cfg.DeriveDrainInterval, logger)
			logger.Info("Real-time derive worker started", "safety_net", cfg.DeriveDrainInterval)
		}

		// Start maintenance tickers (cleanup, digest, catch-up, ranks, news scrub)
		mc := maintenance.DefaultConfig()
		mc.NewsScrubInterval = cfg.NewsScrubInterval
		mc.NewsScrubBatch = cfg.NewsScrubBatch
		if !cfg.NewsScrubEnabled {
			mc.NewsScrubInterval = 0 // disable the scrub ticker
		}
		mc.StatsInterval = cfg.PipelineStatsInterval
		go maintenance.Start(ctx, dbPool, mc, newsScrubber, logger)
	} else {
		logger.Warn("Database-backed background workers disabled in degraded mode")
	}

	// Create router
	router := api.NewRouter(dbPool, appCache, cfg)

	// Create HTTP server
	addr := fmt.Sprintf("%s:%d", cfg.APIHost, cfg.APIPort)
	srv := &http.Server{
		Addr:         addr,
		Handler:      router,
		ReadTimeout:  10 * time.Second,
		WriteTimeout: 30 * time.Second,
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
