// Package corpus holds shared RSS sweep + touched-entity selection primitives.
// The sweep refreshes news_article_entities and returns the fresh article set
// for downstream durable queue work.
package corpus

import (
	"context"
	"fmt"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/thirdparty"
)

// sweepTimeout caps one team's RSS call. The RSS HTTP client already times out
// at 15s; this is the outer ctx budget per team.
const sweepTimeout = 30 * time.Second

// NewsLookback is the corpus window — how far back we look when assembling an
// entity's context (the articles whose links are "fresh enough to be worth
// scoring"). Shared by the candidate-selection queries here and by the model
// generators so an entity with a brand-new link to an old article is not queued
// only to be skipped inside generation (which would write a null marker).
// Canonical home since the Go AI prune; the Rust cognition layer mirrors it.
const NewsLookback = 72 * time.Hour // 3 days

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
// (cross-entity linking pulls in co-mentioned players for free). It returns two
// handoffs:
//
//   - runStart: the watermark captured BEFORE the sweep — legacy freshness
//     boundary kept for helper queries that still take a `since` window.
//   - affected: article_id → sport for every article that gained a FRESH link
//     this run. This is the explicit batch the FIRST-GPT-AUDIT Session 8 pipeline
//     scrubs in-run and then enqueues derive work from — replacing the runStart
//     watermark as the correctness boundary (no more starvation when a re-seen
//     URL lands no new link rows).
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

// LoadTouchedEntities returns the deduped set of entities to process this run.
// Two sources are unioned:
//
//  1. from_run — entities with a news_article_entities row created at-or-after
//     `since` whose linked article was published within NewsLookback. The two
//     filters together guarantee a non-empty corpus inside the generators, so the
//     run never writes null markers from this branch.
//
//  2. stale_teams — popular teams whose NewsLookback corpus has any fresh
//     article AND who haven't been scored in 18h. Rescues teams starved by
//     from_run because users continuously hit /news/team/{id} between runs (by the
//     time the sweep runs, every Google News URL it ingests is already in
//     news_articles from a user fetch, so no new link rows land in the run-start
//     window). Teams-only keeps the rescue scope small; headliner players ride
//     along via cross-entity linking in from_run, and real-time players are caught
//     by the news-volume LISTEN/NOTIFY worker.
//
// An entity with only fresh links pointing to stale articles is dropped here.
func LoadTouchedEntities(ctx context.Context, pool *pgxpool.Pool, since time.Time, sports []string) ([]Entity, error) {
	qctx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()
	lookbackSecs := int(NewsLookback.Seconds())
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

// AffectedVettedEntities returns the distinct (entity_type, entity_id, sport)
// that the scrub stage marked vetted=TRUE on the given freshly-ingested articles
// — the entities the Session 8 pipeline enqueues derive work for. Only vetted
// links count: a model-dropped same-name candidate never reaches the queue.
func AffectedVettedEntities(ctx context.Context, pool *pgxpool.Pool, articleIDs []int64) ([]Entity, error) {
	if len(articleIDs) == 0 {
		return nil, nil
	}
	qctx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT DISTINCT entity_type, entity_id, sport
		FROM news_article_entities
		WHERE article_id = ANY($1::bigint[])
		  AND vetted IS TRUE
		ORDER BY sport, entity_type, entity_id
	`, articleIDs)
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

// CorpusVersion fingerprints an entity's current vetted, in-lookback corpus so a
// changed corpus reopens its queued derivation work (pipeline_work.input_version)
// and an unchanged one dedupes — the audit's "prefer an input hash over elapsed
// time." The fingerprint is the link count plus the latest scrub time over the
// articles the generators actually read (published within NewsLookback); a new
// vetted link advances both. Best-effort: returns "" on error (the queue then
// falls back to plain idempotent enqueue).
func CorpusVersion(ctx context.Context, pool *pgxpool.Pool, e Entity) (string, error) {
	qctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	lookbackSecs := int(NewsLookback.Seconds())
	var count int
	var maxEpoch int64
	err := pool.QueryRow(qctx, `
		SELECT count(*),
		       COALESCE(EXTRACT(EPOCH FROM max(nae.scrubbed_at))::bigint, 0)
		FROM news_article_entities nae
		JOIN news_articles a ON a.id = nae.article_id
		WHERE nae.entity_type = $1 AND nae.entity_id = $2 AND nae.sport = $3
		  AND nae.vetted IS TRUE
		  AND (a.published_at IS NULL OR a.published_at > NOW() - ($4 || ' seconds')::interval)
	`, e.EntityType, e.EntityID, e.Sport, fmt.Sprintf("%d", lookbackSecs)).Scan(&count, &maxEpoch)
	if err != nil {
		return "", err
	}
	return fmt.Sprintf("%d:%d", count, maxEpoch), nil
}

// LookupEntityName resolves the display name for model prompts.
func LookupEntityName(ctx context.Context, pool *pgxpool.Pool, entityType string, id int, sport string) (string, error) {
	q := `SELECT name FROM teams WHERE id = $1 AND sport = $2`
	if entityType == "player" {
		q = `SELECT name FROM players WHERE id = $1 AND sport = $2`
	}
	var name string
	err := pool.QueryRow(ctx, q, id, sport).Scan(&name)
	return name, err
}
