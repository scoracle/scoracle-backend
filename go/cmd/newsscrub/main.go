// newsscrub — CLI to dry-run (or apply) the Gemma news scrub: for a team's
// recent candidate-rich articles, show which linked entities Gemma keeps vs
// drops (same-name disambiguation + relevance).
//
//	go run ./cmd/newsscrub -team-id 18 -sport FOOTBALL -limit 8
//	go run ./cmd/newsscrub -article-id 12345 -sport FOOTBALL
//	# apply (record vetted verdicts; non-destructive): add -persist
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
	teamID := flag.Int("team-id", 0, "scrub this team's recent candidate-rich articles")
	articleID := flag.Int64("article-id", 0, "scrub a single article by id")
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL")
	limit := flag.Int("limit", 8, "[team mode] number of articles to scrub")
	minCands := flag.Int("min-candidates", 3, "[team mode] only articles with >= this many linked entities")
	persist := flag.Bool("persist", false, "apply verdicts (set vetted flag, non-destructive); default dry-run")
	flag.Parse()

	logger := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelWarn}))
	_ = godotenv.Load(".env.local", ".env")
	cfg, err := config.Load()
	if err != nil {
		logger.Error("config load failed", "error", err)
		os.Exit(1)
	}
	if *sport == "" || (*teamID == 0 && *articleID == 0) {
		fmt.Fprintln(os.Stderr, "-sport and one of -team-id / -article-id are required")
		os.Exit(2)
	}
	sportUpper := strings.ToUpper(*sport)

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
	scrubber := ml.NewNewsScrubber(pool, ollama)

	var articleIDs []int64
	if *articleID != 0 {
		articleIDs = []int64{*articleID}
	} else {
		articleIDs, err = candidateRichArticles(pool, *teamID, sportUpper, *minCands, *limit)
		if err != nil {
			logger.Error("find articles failed", "error", err)
			os.Exit(1)
		}
	}
	if len(articleIDs) == 0 {
		fmt.Println("no matching articles")
		return
	}

	mode := "DRY-RUN"
	if *persist {
		mode = "PERSIST (recording vetted verdicts)"
	}
	fmt.Printf("=== News scrub (%s) — %d article(s) ===\n", mode, len(articleIDs))
	kept, dropped := 0, 0
	for _, id := range articleIDs {
		ctx, cancel := context.WithTimeout(context.Background(), cfg.OllamaTimeout+10*time.Second)
		res, err := scrubber.ScrubArticle(ctx, id, sportUpper, !*persist)
		cancel()
		if err != nil {
			fmt.Printf("\n[article %d] ERROR: %v\n", id, err)
			continue
		}
		fmt.Printf("\n[article %d] %s\n", id, res.Title)
		for _, v := range res.Verdicts {
			mark := "DROP"
			if v.Relevant {
				mark = "keep"
				kept++
			} else {
				dropped++
			}
			fmt.Printf("   %-4s %s %d  %s\n", mark, v.EntityType, v.EntityID, v.Name)
		}
	}
	fmt.Printf("\n--- totals: %d kept, %d dropped ---\n", kept, dropped)
}

// candidateRichArticles returns recent articles linked to the team that also
// carry several other entity links — the interesting disambiguation cases.
func candidateRichArticles(pool *pgxpool.Pool, teamID int, sport string, minCands, limit int) ([]int64, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	rows, err := pool.Query(ctx, `
		SELECT a.id
		FROM news_articles a
		JOIN news_article_entities team ON team.article_id = a.id
		     AND team.entity_type = 'team' AND team.entity_id = $1 AND team.sport = $2
		WHERE a.published_at > NOW() - INTERVAL '7 days'
		  AND (SELECT count(*) FROM news_article_entities nae WHERE nae.article_id = a.id) >= $3
		ORDER BY a.published_at DESC NULLS LAST
		LIMIT $4
	`, teamID, sport, minCands, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []int64
	for rows.Next() {
		var id int64
		if err := rows.Scan(&id); err != nil {
			return nil, err
		}
		out = append(out, id)
	}
	return out, rows.Err()
}
