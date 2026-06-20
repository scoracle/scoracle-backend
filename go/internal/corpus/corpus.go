// Package corpus holds the shared "RSS sweep + touched-entity selection"
// primitives for the staged Gemma pipeline. The sweep refreshes
// news_article_entities; LoadTouchedEntities then returns exactly the entities
// whose corpus changed in this run (the in-process runStart watermark is the
// cross-stage handoff). Extracted from cmd/vibe so the cmd/pipeline orchestrator
// and the ad-hoc cmd/vibe corpus mode share one implementation.
package corpus

import (
	"context"
	"fmt"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/ml"
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

// Entity is an (entity_type, entity_id, sport) the pipeline operates on.
type Entity struct {
	EntityType string
	EntityID   int
	Sport      string
}

// Sweep RSS-fetches every team in scope, writing through to news_article_entities
// (cross-entity linking pulls in co-mentioned players for free), and returns the
// watermark captured BEFORE the sweep — the "fresh corpus from this run" boundary
// LoadTouchedEntities filters on. ok/fail count the RSS calls. Honors ctx
// cancellation between teams.
func Sweep(ctx context.Context, pool *pgxpool.Pool, sports []string, rssLimit, rssPauseMs int, logger *slog.Logger) (time.Time, int, int) {
	news := thirdparty.NewNewsService(pool, logger)
	runStart := time.Now().UTC()

	ok, fail := 0, 0
	for _, sport := range sports {
		teams, err := LoadTeams(ctx, pool, sport)
		if err != nil {
			logger.Error("corpus: load teams failed", "sport", sport, "error", err)
			continue
		}
		logger.Info("corpus: rss sweep starting", "sport", sport, "teams", len(teams))

		for _, t := range teams {
			if ctx.Err() != nil {
				return runStart, ok, fail
			}
			tctx, cancel := context.WithTimeout(ctx, sweepTimeout)
			_, err := news.GetEntityNews(tctx, "team", t.ID, t.Name, t.Sport, "", rssLimit, "", "", t.Aliases)
			cancel()
			if err != nil {
				fail++
				logger.Warn("corpus: rss fetch failed", "sport", sport, "team", t.Name, "id", t.ID, "error", err)
			} else {
				ok++
			}
			if rssPauseMs > 0 {
				time.Sleep(time.Duration(rssPauseMs) * time.Millisecond)
			}
		}
	}
	logger.Info("corpus: rss sweep complete",
		"ok", ok, "fail", fail, "elapsed", time.Since(runStart).Round(time.Second))
	return runStart, ok, fail
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

// LoadTouchedEntities returns the deduped set of entities to process this run.
// Two sources are unioned:
//
//  1. from_run — entities with a news_article_entities row created at-or-after
//     `since` whose linked article was published within ml.NewsLookback. The two
//     filters together guarantee a non-empty corpus inside the generators, so the
//     run never writes null markers from this branch.
//
//  2. stale_teams — popular teams whose ml.NewsLookback corpus has any fresh
//     article AND who haven't been scored in 18h. Rescues teams starved by
//     from_run because users continuously hit /news/team/{id} between runs (by the
//     time the sweep runs, every Google News URL it ingests is already in
//     news_articles from a user fetch, so no new link rows land in the run-start
//     window). Teams-only keeps the rescue scope small; headliner players ride
//     along via cross-entity linking in from_run, and real-time players are caught
//     by the news-volume LISTEN/NOTIFY worker.
//
// An entity with only fresh links pointing to stale articles is dropped here —
// stale evidence isn't worth Gemma's time.
func LoadTouchedEntities(ctx context.Context, pool *pgxpool.Pool, since time.Time, sports []string) ([]Entity, error) {
	qctx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()
	lookbackSecs := int(ml.NewsLookback.Seconds())
	rows, err := pool.Query(qctx, `
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

	var out []Entity
	for rows.Next() {
		var e Entity
		if err := rows.Scan(&e.EntityType, &e.EntityID, &e.Sport); err != nil {
			return nil, err
		}
		out = append(out, e)
	}
	return out, rows.Err()
}

// RecentlyGenerated reports whether `table` already holds a row for this entity
// newer than `within` ago — the shared per-stage debounce so a batch run doesn't
// re-do what a real-time spike worker just produced. Works for the entity-keyed
// generation tables (vibe_scores, news_summaries). `table` MUST be a trusted
// constant — it is interpolated, not parameterized. Fails open (returns false) on
// a transient error: better to over-generate than silently drop an entity.
func RecentlyGenerated(ctx context.Context, pool *pgxpool.Pool, table string, e Entity, within time.Duration) bool {
	if within <= 0 {
		return false
	}
	q := fmt.Sprintf(`
		SELECT EXISTS (
			SELECT 1 FROM %s
			WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
			  AND generated_at > NOW() - $4::interval
		)`, table)
	var exists bool
	if err := pool.QueryRow(ctx, q, e.EntityType, e.EntityID, e.Sport, within.String()).Scan(&exists); err != nil {
		return false
	}
	return exists
}

// LookupEntityName resolves the display name for a Gemma prompt.
func LookupEntityName(ctx context.Context, pool *pgxpool.Pool, entityType string, id int, sport string) (string, error) {
	q := `SELECT name FROM teams WHERE id = $1 AND sport = $2`
	if entityType == "player" {
		q = `SELECT name FROM players WHERE id = $1 AND sport = $2`
	}
	var name string
	err := pool.QueryRow(ctx, q, id, sport).Scan(&name)
	return name, err
}
