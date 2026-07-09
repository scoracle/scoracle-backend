// Package corpus holds the RSS sweep primitives used by the ingest pipeline.
package corpus

import (
	"context"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/thirdparty"
)

// sweepTimeout caps one team's RSS call. The RSS HTTP client already times out
// at 15s; this is the outer ctx budget per team.
const sweepTimeout = 30 * time.Second

// Team is a team we sweep RSS for.
type Team struct {
	ID      int
	Sport   string
	Name    string
	Aliases []string
}

// Sweep RSS-fetches every team in scope, writing through to news_article_entities
// (cross-entity linking pulls in co-mentioned players for free). It returns two
// handoffs:
//
//   - runStart: the watermark captured before the sweep.
//   - affected: article_id → sport for every article that gained a FRESH link
//     this run. This is the explicit batch the queueing layer derives from.
//
// ok/fail count the RSS calls. Honors ctx cancellation between teams.
func Sweep(ctx context.Context, pool *pgxpool.Pool, sports []string, rssLimit, rssPauseMs int, logger *slog.Logger) (runStart time.Time, affected map[int64]string, ok, fail int) {
	news := thirdparty.NewNewsService(pool, logger)
	runStart = time.Now().UTC()
	affected = make(map[int64]string)

	for _, sport := range sports {
		teams, err := LoadTeams(ctx, pool, sport)
		if err != nil {
			logger.Error("corpus: load teams failed", "sport", sport, "error", err)
			continue
		}
		logger.Info("corpus: rss sweep starting", "sport", sport, "teams", len(teams))

		for _, t := range teams {
			if ctx.Err() != nil {
				return runStart, affected, ok, fail
			}
			tctx, cancel := context.WithTimeout(ctx, sweepTimeout)
			_, ids, err := news.GetEntityNews(tctx, "team", t.ID, t.Name, t.Sport, "", rssLimit, "", "", t.Aliases)
			cancel()
			if err != nil {
				fail++
				logger.Warn("corpus: rss fetch failed", "sport", sport, "team", t.Name, "id", t.ID, "error", err)
			} else {
				ok++
				for _, id := range ids {
					affected[id] = t.Sport
				}
			}
			if rssPauseMs > 0 {
				time.Sleep(time.Duration(rssPauseMs) * time.Millisecond)
			}
		}
	}
	logger.Info("corpus: rss sweep complete",
		"ok", ok, "fail", fail, "fresh_articles", len(affected), "elapsed", time.Since(runStart).Round(time.Second))
	return runStart, affected, ok, fail
}

// LoadTeams returns every team in the sport (no tier filter — coverage shouldn't
// collapse in the offseason or for eliminated teams; the count is small).
func LoadTeams(ctx context.Context, pool *pgxpool.Pool, sport string) ([]Team, error) {
	qctx, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT id, sport, name, COALESCE(search_aliases, ARRAY[]::text[])
		FROM teams
		WHERE sport = $1
		ORDER BY id
	`, sport)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []Team
	for rows.Next() {
		var t Team
		if err := rows.Scan(&t.ID, &t.Sport, &t.Name, &t.Aliases); err != nil {
			return nil, err
		}
		out = append(out, t)
	}
	return out, rows.Err()
}
