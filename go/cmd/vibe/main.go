// vibe — CLI for generating vibe sentiment scores (1-100).
//
// Two modes:
//
//	single (default) — generate one score for a given entity.
//	  go run ./cmd/vibe -entity-type player -entity-id 237 -sport NBA
//	  go run ./cmd/vibe -entity-type team -entity-id 14 -sport NBA
//
//	batch — iterate starter-tier entities that played in the last N hours
//	        and generate one score each. Intended to run overnight from
//	        cron so the long-tail of active players gets daily coverage
//	        that doesn't compete with the real-time headliner path.
//	  go run ./cmd/vibe -mode batch -sport NBA -since-hours 24
//	  go run ./cmd/vibe -mode batch -sport all -since-hours 30
//
// Env: DATABASE_PRIVATE_URL (or fallbacks) + OLLAMA_* (see config.go).
package main

import (
	"context"
	"flag"
	"fmt"
	"log/slog"
	"os"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/joho/godotenv"

	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/ml"
)

func main() {
	mode := flag.String("mode", "single", "single | batch")

	// single-mode flags
	entityType := flag.String("entity-type", "player", "[single] player | team")
	entityID := flag.Int("entity-id", 0, "[single] canonical entity id")
	trigger := flag.String("trigger", "manual", "[single] milestone | manual | periodic")

	// shared + batch-mode flags
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL | all (required)")
	sinceHours := flag.Int("since-hours", 24, "[batch] look back this many hours for entities with recent activity")
	throttleMs := flag.Int("throttle-ms", 0, "[batch] pause N ms between generations; 0 = back-to-back")
	skipRecentHours := flag.Int("skip-recent-hours", 20, "[batch] skip entities that already have a vibe within the last N hours")
	maxEntities := flag.Int("max", 0, "[batch] cap entities per sport; 0 = no cap")

	flag.Parse()

	logger := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))

	_ = godotenv.Load(".env.local", ".env")
	cfg, err := config.Load()
	if err != nil {
		logger.Error("config load failed", "error", err)
		os.Exit(1)
	}

	pool, err := pgxpool.New(context.Background(), cfg.DatabaseURL)
	if err != nil {
		logger.Error("db connect failed", "error", err)
		os.Exit(1)
	}
	defer pool.Close()

	ollama := ml.NewOllamaClient(cfg.OllamaBaseURL, cfg.OllamaModel, cfg.OllamaTimeout)
	if err := ollama.Ping(context.Background()); err != nil {
		logger.Error("ollama unreachable", "error", err, "base_url", cfg.OllamaBaseURL)
		os.Exit(1)
	}
	gen := ml.NewGenerator(pool, ollama)

	switch *mode {
	case "single":
		runSingle(pool, gen, *entityType, *entityID, *sport, *trigger, cfg.OllamaTimeout, logger)
	case "batch":
		runBatch(pool, gen, *sport, *sinceHours, *skipRecentHours, *throttleMs, *maxEntities, cfg.OllamaTimeout, logger)
	default:
		fmt.Fprintf(os.Stderr, "unknown -mode %q; valid: single | batch\n", *mode)
		os.Exit(2)
	}
}

// ---------------------------------------------------------------------------
// Single mode
// ---------------------------------------------------------------------------

func runSingle(
	pool *pgxpool.Pool, gen *ml.Generator,
	entityType string, entityID int,
	sport string, trigger string,
	timeout time.Duration, logger *slog.Logger,
) {
	if entityID <= 0 || sport == "" {
		fmt.Fprintln(os.Stderr, "-entity-id and -sport are required in single mode")
		os.Exit(2)
	}

	ctx, cancel := context.WithTimeout(context.Background(), timeout+10*time.Second)
	defer cancel()

	sportUpper := strings.ToUpper(sport)
	entityName, err := lookupEntityName(ctx, pool, entityType, entityID, sportUpper)
	if err != nil {
		logger.Error("entity lookup failed", "error", err)
		os.Exit(1)
	}

	result, err := gen.Generate(ctx, ml.VibeRequest{
		EntityType:  entityType,
		EntityID:    entityID,
		EntityName:  entityName,
		Sport:       sportUpper,
		TriggerType: trigger,
	})
	if err != nil {
		logger.Error("vibe generate failed", "error", err)
		os.Exit(1)
	}

	fmt.Printf("\n--- Vibe for %s (%s %d, %s) ---\n", entityName, entityType, entityID, sportUpper)
	fmt.Printf("Sentiment: %d/100\n", result.Sentiment)
	fmt.Printf("\n(model=%s prompt=%s duration=%s news=%d tweets=%d)\n",
		result.Model, result.PromptVersion, result.Duration.Round(10*time.Millisecond),
		len(result.InputNewsIDs), len(result.InputTweetIDs))
}

// ---------------------------------------------------------------------------
// Batch mode
// ---------------------------------------------------------------------------

type batchCandidate struct {
	entityType string
	entityID   int
	sport      string
	name       string
}

func runBatch(
	pool *pgxpool.Pool, gen *ml.Generator,
	sportArg string, sinceHours, skipRecentHours, throttleMs, maxPerSport int,
	timeout time.Duration, logger *slog.Logger,
) {
	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if strings.ToLower(sportArg) != "all" {
		sports = []string{strings.ToUpper(sportArg)}
	}

	overallOK, overallFail, overallSkip := 0, 0, 0
	startAll := time.Now()

	for _, sport := range sports {
		candidates, err := loadStarterCandidates(pool, sport, sinceHours, skipRecentHours, maxPerSport)
		if err != nil {
			logger.Error("batch: load candidates failed", "sport", sport, "error", err)
			continue
		}
		if len(candidates) == 0 {
			logger.Info("batch: no candidates", "sport", sport)
			continue
		}

		logger.Info("batch: starting",
			"sport", sport, "candidates", len(candidates),
			"since_hours", sinceHours, "skip_recent_hours", skipRecentHours)

		ok, fail := 0, 0
		sportStart := time.Now()

		for i, c := range candidates {
			ctx, cancel := context.WithTimeout(context.Background(), timeout+10*time.Second)
			_, err := gen.Generate(ctx, ml.VibeRequest{
				EntityType:  c.entityType,
				EntityID:    c.entityID,
				EntityName:  c.name,
				Sport:       c.sport,
				TriggerType: "periodic",
			})
			cancel()

			if err != nil {
				fail++
				logger.Warn("batch: generate failed",
					"sport", sport, "entity", c.name, "id", c.entityID, "error", err)
			} else {
				ok++
			}

			if (i+1)%25 == 0 {
				logger.Info("batch: progress",
					"sport", sport, "done", i+1, "total", len(candidates),
					"ok", ok, "fail", fail)
			}

			if throttleMs > 0 {
				time.Sleep(time.Duration(throttleMs) * time.Millisecond)
			}
		}

		overallOK += ok
		overallFail += fail
		logger.Info("batch: sport complete",
			"sport", sport, "ok", ok, "fail", fail,
			"elapsed", time.Since(sportStart).Round(time.Second))
	}

	logger.Info("batch: all sports complete",
		"ok", overallOK, "fail", overallFail, "skipped_pre_filter", overallSkip,
		"elapsed", time.Since(startAll).Round(time.Second))
}

// loadStarterCandidates returns starter+headliner-tier players AND teams
// whose fixtures actually kicked off in the last N hours. skipRecentHours
// excludes entities that already have a vibe within that window (idempotent
// reruns).
//
// Activity is measured against fixtures.start_time (when the game happened),
// not fixtures.seeded_at (when we ingested the row). A historical re-seed
// should not make every player "recently active".
func loadStarterCandidates(
	pool *pgxpool.Pool, sport string,
	sinceHours, skipRecentHours, maxPerSport int,
) ([]batchCandidate, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	limitClause := ""
	if maxPerSport > 0 {
		limitClause = fmt.Sprintf("LIMIT %d", maxPerSport)
	}

	// Players who appeared in a fixture that kicked off within the window
	// UNION teams that played in such a fixture, both filtered to the
	// relevant tiers and excluding entities with a recent vibe.
	query := fmt.Sprintf(`
		WITH recent_fixtures AS (
			SELECT id, home_team_id, away_team_id
			FROM fixtures
			WHERE sport = $1
			  AND start_time > NOW() - ($2 || ' hours')::interval
			  AND start_time <= NOW()
		),
		recent_play AS (
			SELECT DISTINCT ebs.player_id
			FROM event_box_scores ebs
			JOIN recent_fixtures rf ON rf.id = ebs.fixture_id
			WHERE ebs.sport = $1
		),
		recent_teams AS (
			SELECT DISTINCT team_id FROM (
				SELECT home_team_id AS team_id FROM recent_fixtures
				UNION ALL
				SELECT away_team_id FROM recent_fixtures
			) _
		)
		SELECT 'player'::text AS entity_type, p.id, p.sport, p.name
		FROM players p
		JOIN recent_play rp ON rp.player_id = p.id
		WHERE p.sport = $1
		  AND p.tier IN ('starter', 'headliner')
		  AND NOT EXISTS (
			  SELECT 1 FROM vibe_scores v
			  WHERE v.entity_type = 'player' AND v.entity_id = p.id
			    AND v.sport = $1
			    AND v.generated_at > NOW() - ($3 || ' hours')::interval
		  )
		UNION ALL
		SELECT 'team'::text AS entity_type, t.id, t.sport, t.name
		FROM teams t
		JOIN recent_teams rt ON rt.team_id = t.id
		WHERE t.sport = $1
		  AND t.tier IN ('starter', 'headliner')
		  AND NOT EXISTS (
			  SELECT 1 FROM vibe_scores v
			  WHERE v.entity_type = 'team' AND v.entity_id = t.id
			    AND v.sport = $1
			    AND v.generated_at > NOW() - ($3 || ' hours')::interval
		  )
		ORDER BY 1, 2
		%s
	`, limitClause)

	// Passed as strings because the query does `($n || ' hours')::interval`;
	// pgx's int text-encoding path trips on that concat.
	rows, err := pool.Query(ctx, query, sport,
		fmt.Sprintf("%d", sinceHours),
		fmt.Sprintf("%d", skipRecentHours),
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]batchCandidate, 0, 256)
	for rows.Next() {
		var c batchCandidate
		if err := rows.Scan(&c.entityType, &c.entityID, &c.sport, &c.name); err != nil {
			return nil, err
		}
		out = append(out, c)
	}
	return out, nil
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

func lookupEntityName(ctx context.Context, pool *pgxpool.Pool, entityType string, id int, sport string) (string, error) {
	var query string
	if entityType == "player" {
		query = `SELECT name FROM players WHERE id = $1 AND sport = $2`
	} else {
		query = `SELECT name FROM teams WHERE id = $1 AND sport = $2`
	}
	var name string
	err := pool.QueryRow(ctx, query, id, sport).Scan(&name)
	return name, err
}
