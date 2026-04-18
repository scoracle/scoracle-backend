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

	"github.com/joho/godotenv"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/api"
	"github.com/albapepper/scoracle-data/internal/cache"
	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/db"
	"github.com/albapepper/scoracle-data/internal/listener"
	"github.com/albapepper/scoracle-data/internal/maintenance"
	"github.com/albapepper/scoracle-data/internal/notifications"
	"github.com/albapepper/scoracle-data/internal/thirdparty"

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

	if dbPool != nil {
		// Start notification dispatch worker (if FCM is configured)
		fcmSender := notifications.NewFCMSender(cfg.FCMCredentialsFile, logger)
		if fcmSender != nil {
			go notifications.StartWorker(ctx, dbPool, fcmSender, logger)
			logger.Info("Notification dispatch worker started")
		} else {
			logger.Info("Notification dispatch worker disabled (no FIREBASE_CREDENTIALS_FILE)")
		}

		// Start LISTEN/NOTIFY consumer for real-time milestone events
		go listener.Start(ctx, cfg.DatabaseURL, dbPool, fcmSender, logger)

		// Start maintenance tickers (cleanup, digest, catch-up sweep)
		go maintenance.Start(ctx, dbPool, maintenance.DefaultConfig(), logger)

		// Seed twitter_lists rows for configured sports so status endpoints can
		// report cold-cache state before the first refresh.
		if len(cfg.TwitterLists) > 0 {
			tw := thirdparty.NewTwitterService(dbPool, cfg.TwitterBearerToken, cfg.TwitterLists, cfg.TwitterCacheTTL)
			if err := tw.SyncLists(ctx); err != nil {
				logger.Warn("Failed to sync twitter_lists", "error", err)
			}
		}
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
