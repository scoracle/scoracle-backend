// Command api is the Scoracle Data API server.
//
// Usage:
//
//	scoracle-api
//	API_PORT=8080 scoracle-api

// @title Scoracle Data API
// @version 2.0.0
// @description Unified Scoracle API serving sport data pages, derived Gemma products (narratives, transfer heat, Vibe, Sigil), health checks, and operational endpoints.
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

	// deriveDone is closed when the real-time derive worker has fully returned,
	// including settling (handing back) its leased pipeline_work rows on a graceful
	// shutdown (F-018). nil when the worker never started; the shutdown path waits on
	// it only when non-nil.
	var deriveDone chan struct{}

	if dbPool != nil {
		// Start notification dispatch worker (if FCM is configured)
		fcmSender := notifications.NewFCMSender(cfg.FCMCredentialsFile, logger)
		if fcmSender != nil {
			go notifications.StartWorker(ctx, dbPool, fcmSender, logger)
			logger.Info("Notification dispatch worker started")
		} else {
			logger.Info("Notification dispatch worker disabled (no FIREBASE_CREDENTIALS_FILE)")
		}

		// Gemma generators (FIRST-GPT-AUDIT Session 14): built UNCONDITIONALLY — no
		// longer gated on a one-time boot ping. If Ollama is unreachable now, the derive
		// worker's per-drain reachability gate DEFERS work (claims nothing, burns no
		// retries) and the maintenance scrub skips its Gemma phase; both resume the moment
		// Ollama returns, with NO API restart (F-014). SetGemmaConcurrency installs the
		// shared GPU governor that bounds the worker + scrub + any in-process Gemma together.
		ml.SetGemmaConcurrency(cfg.OllamaMaxConcurrent)
		ollamaCli := ml.NewOllamaClient(cfg.OllamaBaseURL, cfg.OllamaModel, cfg.OllamaTimeout)
		ollamaCli.SetKeepAlive(cfg.OllamaKeepAlive)
		narrator := ml.NewNewsNarrator(dbPool, ollamaCli)
		vibeGen := ml.NewVibeGenerator(dbPool, ollamaCli)
		transferGen := ml.NewTransferGenerator(dbPool, ollamaCli)
		synthGen := ml.NewSigilGenerator(dbPool, ollamaCli)
		newsScrubber := ml.NewNewsScrubber(dbPool, ollamaCli)
		// Non-gating boot probe — operator visibility only; it changes no behavior.
		pingCtx, pingCancel := context.WithTimeout(ctx, 3*time.Second)
		if err := ollamaCli.Ping(pingCtx); err != nil {
			logger.Warn("Ollama unreachable at boot — derive worker will defer work until it returns (no restart needed)",
				"base_url", cfg.OllamaBaseURL, "error", err)
		} else {
			logger.Info("Ollama reachable",
				"model", cfg.OllamaModel, "max_concurrent", cfg.OllamaMaxConcurrent,
				"keep_alive", cfg.OllamaKeepAlive, "short_timeout", cfg.OllamaShortTimeout, "long_timeout", cfg.OllamaTimeout)
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
		// Started whenever it is enabled — Ollama availability is NO LONGER a gate
		// (Session 14): the drainer's reachability pre-gate defers work while Ollama is
		// down (no claims, no burned retries) and resumes automatically when it returns,
		// so the worker no longer needs an API restart to come alive after an outage.
		if !cfg.DeriveWorkerEnabled {
			logger.Info("Real-time derive worker disabled (DERIVE_WORKER_ENABLED=false)")
		} else {
			drainer := &derive.Drainer{
				Pool:              dbPool,
				Ollama:            ollamaCli,
				TransferGen:       transferGen,
				Narrator:          narrator,
				VibeGen:           vibeGen,
				SynthGen:          synthGen,
				GemmaTimeout:      cfg.OllamaTimeout,
				GemmaShortTimeout: cfg.OllamaShortTimeout,
				MinArticles:       cfg.TransferMinArticles,
				Logger:            logger,
			}
			deriveDone = make(chan struct{})
			go func() {
				defer close(deriveDone)
				derive.StartWorker(ctx, cfg.DatabaseURL, drainer, cfg.DeriveDrainInterval, logger)
			}()
			logger.Info("Real-time derive worker started", "safety_net", cfg.DeriveDrainInterval)
		}

		// Start maintenance tickers (cleanup, digest, catch-up, ranks, news scrub)
		mc := maintenance.DefaultConfig()
		mc.NewsScrubInterval = cfg.NewsScrubInterval
		mc.NewsScrubBatch = cfg.NewsScrubBatch
		mc.NewsScrubViaQueue = cfg.NewsScrubViaQueue // L6: enqueue scrub to the Rust handler vs inline
		mc.NewsScrubTimeout = cfg.OllamaShortTimeout // per-article Gemma bound (Session 14)
		mc.Ollama = ollamaCli                        // reachability pre-gate for the scrub sweep
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

	// Give the derive worker a moment to settle (hand back) its leased pipeline_work
	// rows on a fresh context before the deferred pool.Close() drops the pool (F-018):
	// otherwise an in-flight batch would strand 'running' until the 30m stale lease.
	// The worker requeues its batch the instant the drain ctx is cancelled, so this
	// resolves fast; the bound just prevents a hung generation from blocking exit.
	if deriveDone != nil {
		select {
		case <-deriveDone:
			logger.Info("Derive worker settled its leased work")
		case <-time.After(8 * time.Second):
			logger.Warn("Derive worker did not settle within 8s; any leased rows wait for stale-lease recovery")
		}
	}
	logger.Info("Server stopped")
}
