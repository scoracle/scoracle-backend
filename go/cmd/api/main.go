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
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/joho/godotenv"

	"github.com/albapepper/scoracle-data/internal/api"
	"github.com/albapepper/scoracle-data/internal/cache"
	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/db"
	"github.com/albapepper/scoracle-data/internal/listener"
	"github.com/albapepper/scoracle-data/internal/maintenance"
	"github.com/albapepper/scoracle-data/internal/ml"
	"github.com/albapepper/scoracle-data/internal/notifications"

	_ "github.com/albapepper/scoracle-data/docs" // swagger docs
)

func main() {
	logger := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))
	slog.SetDefault(logger)

	// Load .env.local (real values, gitignored) then .env (committed template).
	// godotenv does not overwrite already-set vars, so .env.local wins.
	_ = godotenv.Load(".env.local", ".env")

	// Load configuration
	cfg, err := config.Load()
	if err != nil {
		logger.Error("Failed to load configuration", "error", err)
		os.Exit(1)
	}

	// Context with signal handling
	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt)
	defer cancel()

	// Connect to database
	logger.Info("Connecting to database...")
	pool, err := db.New(ctx, cfg)
	var dbPool *pgxpool.Pool
	if err != nil {
		logger.Error("Failed to connect to database", "error", err)
		logger.Warn("Starting in degraded mode without database connectivity")
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
	// otherwise. Declared here so NewRouter can receive it at the outer scope.
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

		// News-volume vibe worker: listens on the vibe_trigger channel and
		// runs Gemma when an entity's news article count spikes. Disabled
		// gracefully if Ollama isn't reachable — the LISTEN goroutine still
		// starts (events are logged) so we observe spikes even when the
		// model is offline.
		var newsVolumeGen *ml.VibeGenerator
		var newsVolumeNarrator *ml.NewsNarrator
		ollamaCli := ml.NewOllamaClient(cfg.OllamaBaseURL, cfg.OllamaModel, cfg.OllamaTimeout)
		pingCtx, pingCancel := context.WithTimeout(ctx, 3*time.Second)
		if err := ollamaCli.Ping(pingCtx); err != nil {
			logger.Warn("News-volume vibe disabled (Ollama unreachable)",
				"base_url", cfg.OllamaBaseURL, "error", err)
		} else {
			logger.Info("News-volume vibe enabled", "model", cfg.OllamaModel)
			newsVolumeGen = ml.NewVibeGenerator(dbPool, ollamaCli)
			newsVolumeNarrator = ml.NewNewsNarrator(dbPool, ollamaCli)
			synthGen = ml.NewSigilGenerator(dbPool, ollamaCli)
		}
		pingCancel()

		// Start LISTEN/NOTIFY consumers (one goroutine per channel). On a news
		// spike the news-volume worker refreshes narratives (stage 2), sentiment
		// (stage 3), then vibe synthesis (stage 4, 24h debounced).
		go listener.Start(ctx, cfg.DatabaseURL, dbPool, fcmSender, synthGen, logger)
		go listener.StartNewsVolume(ctx, cfg.DatabaseURL, dbPool, newsVolumeNarrator, newsVolumeGen, synthGen, logger)

		// Transfer-rumor news-spike worker. Reuses the SAME ollamaCli (no second
		// ping) — newsVolumeGen != nil is exactly "Ollama reachable". nil gen → the
		// listener still runs, logging spikes without generating.
		if cfg.TransferEnabled {
			var transferGen *ml.TransferGenerator
			if newsVolumeGen != nil {
				transferGen = ml.NewTransferGenerator(dbPool, ollamaCli)
				logger.Info("Transfer rumor worker enabled", "model", cfg.OllamaModel,
					"max_concurrent", cfg.TransferMaxConcurrent)
			} else {
				logger.Warn("Transfer rumor worker degraded (Ollama unreachable) — spikes logged only")
			}
			go listener.StartTransfer(ctx, cfg.DatabaseURL, dbPool, transferGen, listener.TransferConfig{
				MaxConcurrent: cfg.TransferMaxConcurrent,
				Debounce:      cfg.TransferDebounce,
				MinArticles:   cfg.TransferMinArticles,
			}, logger)
		}

		// News scrub sweep (Gemma ID-gate). Reuses the SAME ollamaCli; nil when
		// Ollama is unreachable (newsVolumeGen == nil) → maintenance skips the sweep.
		var newsScrubber *ml.NewsScrubber
		if newsVolumeGen != nil {
			newsScrubber = ml.NewNewsScrubber(dbPool, ollamaCli)
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
