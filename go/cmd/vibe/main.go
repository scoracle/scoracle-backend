// vibe — CLI for generating vibe sentiment scores (1-100).
//
// Two modes:
//
//	single (default) — generate one score for a given entity.
//	  go run ./cmd/vibe -entity-type player -entity-id 237 -sport NBA
//	  go run ./cmd/vibe -entity-type team -entity-id 14 -sport NBA
//
//	corpus — RSS-sweep every team across NBA/NFL/FOOTBALL, then run Gemma
//	         only on entities that picked up fresh news in this run. The
//	         corpus presence is the candidate signal — every Gemma call
//	         is guaranteed real input. Cross-entity linking inside the
//	         news write-through pulls in co-mentioned players for free,
//	         so the player layer is included without per-player RSS calls.
//	         Intended for a noon + midnight cron pair.
//	  go run ./cmd/vibe -mode corpus
//	  go run ./cmd/vibe -mode corpus -sport NBA  # one-sport smoke run
//
// Real-time coverage between corpus runs is handled inside the API by the
// news-volume LISTEN/NOTIFY worker (internal/listener/news_volume_worker.go),
// not by this CLI.
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
	"github.com/albapepper/scoracle-data/internal/thirdparty"
)

func main() {
	mode := flag.String("mode", "single", "single | corpus")

	// single-mode flags
	entityType := flag.String("entity-type", "player", "[single] player | team")
	entityID := flag.Int("entity-id", 0, "[single] canonical entity id")
	trigger := flag.String("trigger", "manual", "[single] manual | periodic | news_spike")

	// shared + corpus-mode flags
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL | all (single requires it; corpus defaults to all)")
	throttleMs := flag.Int("throttle-ms", 0, "[corpus] pause N ms between generations; 0 = back-to-back")
	corpusSkipHours := flag.Int("corpus-skip-recent-hours", 10, "[corpus] skip entities with a vibe newer than this; <= half the cron cadence")
	corpusRSSPause := flag.Int("corpus-rss-pause-ms", 100, "[corpus] pause between team RSS calls to be polite to Google News")
	corpusRSSLimit := flag.Int("corpus-rss-limit", 10, "[corpus] articles per team RSS call")

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
	case "corpus":
		runCorpus(pool, gen, *sport, *corpusSkipHours, *throttleMs, *corpusRSSPause, *corpusRSSLimit, cfg.OllamaTimeout, logger)
	default:
		fmt.Fprintf(os.Stderr, "unknown -mode %q; valid: single | corpus\n", *mode)
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
	if result.SkippedNoCorpus {
		fmt.Println("Sentiment: (no data — corpus empty)")
	} else {
		fmt.Printf("Sentiment: %d/100\n", result.Sentiment)
	}
	fmt.Printf("\n(model=%s prompt=%s duration=%s news=%d tweets=%d)\n",
		result.Model, result.PromptVersion, result.Duration.Round(10*time.Millisecond),
		len(result.InputNewsIDs), len(result.InputTweetIDs))
}

// ---------------------------------------------------------------------------
// Corpus mode — RSS sweep + corpus-driven Gemma queue
// ---------------------------------------------------------------------------

// corpusSweepTimeout caps the per-team RSS call. The default RSS HTTP client
// already times out at 15s; this is the outer ctx budget per team.
const corpusSweepTimeout = 30 * time.Second

type corpusTeam struct {
	id      int
	sport   string
	name    string
	aliases []string
}

type corpusEntity struct {
	entityType string
	entityID   int
	sport      string
}

func runCorpus(
	pool *pgxpool.Pool, gen *ml.Generator,
	sportArg string, skipRecentHours, throttleMs, rssPauseMs, rssLimit int,
	gemmaTimeout time.Duration, logger *slog.Logger,
) {
	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if s := strings.ToLower(strings.TrimSpace(sportArg)); s != "" && s != "all" {
		sports = []string{strings.ToUpper(sportArg)}
	}

	news := thirdparty.NewNewsService(pool)

	// Phase 1 — RSS sweep over every team in scope.
	// `runStart` is the watermark that defines "fresh corpus from this run."
	// Capture before the sweep so any link write inside persistArticles counts.
	runStart := time.Now().UTC()

	rssOK, rssFail := 0, 0
	for _, sport := range sports {
		teams, err := loadTeams(pool, sport)
		if err != nil {
			logger.Error("corpus: load teams failed", "sport", sport, "error", err)
			continue
		}
		logger.Info("corpus: rss sweep starting", "sport", sport, "teams", len(teams))

		for _, t := range teams {
			ctx, cancel := context.WithTimeout(context.Background(), corpusSweepTimeout)
			_, err := news.GetEntityNews(
				ctx, "team", t.id, t.name, t.sport, "",
				rssLimit, "", "", t.aliases,
			)
			cancel()
			if err != nil {
				rssFail++
				logger.Warn("corpus: rss fetch failed",
					"sport", sport, "team", t.name, "id", t.id, "error", err)
			} else {
				rssOK++
			}
			if rssPauseMs > 0 {
				time.Sleep(time.Duration(rssPauseMs) * time.Millisecond)
			}
		}
	}
	logger.Info("corpus: rss sweep complete",
		"ok", rssOK, "fail", rssFail, "elapsed", time.Since(runStart).Round(time.Second))

	// Phase 2 — Gemma queue. Pick up every (entity_type, entity_id, sport)
	// whose news_article_entities row is newer than runStart. That set
	// includes the queried teams AND any players/teams co-mentioned in
	// the article titles via persistArticles' cross-entity linking.
	touched, err := loadTouchedEntities(pool, runStart, sports)
	if err != nil {
		logger.Error("corpus: load touch-set failed", "error", err)
		return
	}
	logger.Info("corpus: gemma queue starting", "candidates", len(touched))

	gemmaStart := time.Now()
	ok, fail, skipped, noCorpus := 0, 0, 0, 0
	for i, e := range touched {
		if recentlyVibed(pool, e, skipRecentHours) {
			skipped++
			continue
		}

		name, err := lookupEntityNameCtx(pool, e.entityType, e.entityID, e.sport)
		if err != nil || name == "" {
			fail++
			logger.Warn("corpus: entity lookup failed",
				"entity_type", e.entityType, "entity_id", e.entityID, "sport", e.sport, "error", err)
			continue
		}

		ctx, cancel := context.WithTimeout(context.Background(), gemmaTimeout+10*time.Second)
		result, err := gen.Generate(ctx, ml.VibeRequest{
			EntityType:  e.entityType,
			EntityID:    e.entityID,
			EntityName:  name,
			Sport:       e.sport,
			TriggerType: "periodic",
		})
		cancel()

		switch {
		case err != nil:
			fail++
			logger.Warn("corpus: generate failed",
				"sport", e.sport, "entity", name, "id", e.entityID, "error", err)
		case result.SkippedNoCorpus:
			// Should be rare in corpus mode (we filtered to entities
			// with fresh links), but the news lookback inside Generate
			// is still 72h — a link from an old article that just got
			// re-linked could still net zero "recent" rows.
			noCorpus++
		default:
			ok++
		}

		if (i+1)%25 == 0 {
			logger.Info("corpus: progress",
				"done", i+1, "total", len(touched),
				"ok", ok, "fail", fail, "skipped", skipped, "no_corpus", noCorpus)
		}

		if throttleMs > 0 {
			time.Sleep(time.Duration(throttleMs) * time.Millisecond)
		}
	}

	logger.Info("corpus: complete",
		"ok", ok, "fail", fail, "skipped_recent", skipped, "no_corpus", noCorpus,
		"gemma_elapsed", time.Since(gemmaStart).Round(time.Second),
		"total_elapsed", time.Since(runStart).Round(time.Second))
}

// loadTeams returns the team set we sweep RSS for. All teams in the sport,
// regardless of tier — teams default to 'headliner' anyway and the count is
// small (~30-100 per sport).
func loadTeams(pool *pgxpool.Pool, sport string) ([]corpusTeam, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	rows, err := pool.Query(ctx, `
		SELECT id, sport, name, COALESCE(search_aliases, ARRAY[]::text[])
		FROM teams
		WHERE sport = $1
		ORDER BY id
	`, sport)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []corpusTeam
	for rows.Next() {
		var t corpusTeam
		if err := rows.Scan(&t.id, &t.sport, &t.name, &t.aliases); err != nil {
			return nil, err
		}
		out = append(out, t)
	}
	return out, nil
}

// loadTouchedEntities returns the deduped set of entities to score this run.
// Two sources are unioned:
//
//  1. from_run — entities with a news_article_entities row created at-or-after
//     `since` whose linked article was published within ml.NewsLookback. The
//     two filters together guarantee non-empty corpus inside Generate, so the
//     corpus run never writes null-sentiment markers from this branch.
//
//  2. stale_teams — popular teams whose ML.NewsLookback corpus has any fresh
//     article AND who haven't been scored in 18h. This rescues teams that get
//     starved by from_run because users continuously hit /news/team/{id}
//     between cron passes: by the time the noon RSS sweep runs, every Google
//     News URL it tries to ingest is already in news_articles from a user
//     fetch, so no new link rows appear in the run-start window and the team
//     drops out. Teams-only keeps the rescue scope small (~30-100/sport);
//     headliner players ride along via cross-entity linking in from_run, and
//     real-time players get caught by the news-volume LISTEN/NOTIFY worker.
//
// An entity with only fresh links pointing to stale articles (e.g. RSS
// just discovered a 3-week-old article that mentions player X) is dropped
// here — stale evidence isn't worth Gemma's time, and writing a null row
// would dilute the "no-corpus markers stop accumulating" property that
// the corpus mode design promises.
func loadTouchedEntities(pool *pgxpool.Pool, since time.Time, sports []string) ([]corpusEntity, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	lookbackSecs := int(ml.NewsLookback.Seconds())
	rows, err := pool.Query(ctx, `
		WITH from_run AS (
			SELECT nae.entity_type, nae.entity_id, nae.sport
			FROM news_article_entities nae
			JOIN news_articles a ON a.id = nae.article_id
			WHERE nae.created_at >= $1
			  AND nae.sport = ANY($2::text[])
			  AND (a.published_at IS NULL OR a.published_at > NOW() - ($3 || ' seconds')::interval)
			GROUP BY nae.entity_type, nae.entity_id, nae.sport
		),
		stale_teams AS (
			SELECT 'team'::text AS entity_type, t.id AS entity_id, t.sport
			FROM teams t
			WHERE t.sport = ANY($2::text[])
			  AND EXISTS (
				  SELECT 1
				  FROM news_article_entities nae
				  JOIN news_articles a ON a.id = nae.article_id
				  WHERE nae.entity_type = 'team' AND nae.entity_id = t.id AND nae.sport = t.sport
					AND (a.published_at IS NULL OR a.published_at > NOW() - ($3 || ' seconds')::interval)
				  LIMIT 1
			  )
			  AND NOT EXISTS (
				  SELECT 1 FROM vibe_scores v
				  WHERE v.entity_type = 'team' AND v.entity_id = t.id AND v.sport = t.sport
					AND v.generated_at > NOW() - INTERVAL '18 hours'
			  )
		)
		SELECT entity_type, entity_id, sport FROM from_run
		UNION
		SELECT entity_type, entity_id, sport FROM stale_teams
		ORDER BY 3, 1, 2
	`, since, sports, fmt.Sprintf("%d", lookbackSecs))
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []corpusEntity
	for rows.Next() {
		var e corpusEntity
		if err := rows.Scan(&e.entityType, &e.entityID, &e.sport); err != nil {
			return nil, err
		}
		out = append(out, e)
	}
	return out, nil
}

// recentlyVibed checks whether this entity already has a vibe row within the
// debounce window. Mirrors the milestone listener's check so noon and midnight
// runs don't duplicate work.
func recentlyVibed(pool *pgxpool.Pool, e corpusEntity, skipRecentHours int) bool {
	if skipRecentHours <= 0 {
		return false
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	var exists bool
	err := pool.QueryRow(ctx, `
		SELECT EXISTS (
			SELECT 1 FROM vibe_scores
			WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
			  AND generated_at > NOW() - ($4 || ' hours')::interval
		)
	`, e.entityType, e.entityID, e.sport, fmt.Sprintf("%d", skipRecentHours)).Scan(&exists)
	if err != nil {
		// Fail open — better to over-generate than drop on a transient error.
		return false
	}
	return exists
}

// lookupEntityNameCtx is the same as lookupEntityName but builds its own ctx
// with a short timeout so a slow lookup doesn't stall a full corpus run.
func lookupEntityNameCtx(pool *pgxpool.Pool, entityType string, id int, sport string) (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	return lookupEntityName(ctx, pool, entityType, id, sport)
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
