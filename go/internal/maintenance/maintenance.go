// Package maintenance runs periodic background tasks as Go tickers.
// Replaces pg_cron — all scheduled work is driven from Go since it is
// already a persistent, long-running service (required for LISTEN/NOTIFY).
package maintenance

import (
	"context"
	"log/slog"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/ml"
)

// Config controls maintenance task intervals. Zero duration disables a task.
type Config struct {
	CleanupInterval     time.Duration // Expired notifications + stale cache rows
	DigestInterval      time.Duration // Batch digest generation
	CatchUpInterval     time.Duration // Sweep for missed NOTIFY events
	AlltimeRankInterval time.Duration // season_composite_rank_alltime recompute cadence
	NewsScrubInterval   time.Duration // Gemma ID-gate sweep over unscrubbed news links
	NewsScrubBatch      int           // max candidate-rich articles Gemma-scrubbed per tick
	StatsInterval       time.Duration // pipeline_stats daily corpus snapshot cadence
	PeerCohortInterval  time.Duration // peer_cohort_aggregate (/momentum O1) refresh cadence
}

// DefaultConfig returns sensible production defaults.
func DefaultConfig() Config {
	return Config{
		CleanupInterval:     30 * time.Minute,
		DigestInterval:      1 * time.Hour,
		CatchUpInterval:     15 * time.Minute,
		AlltimeRankInterval: 24 * time.Hour,
		NewsScrubInterval:   30 * time.Minute,
		NewsScrubBatch:      15,
		StatsInterval:       24 * time.Hour,
		PeerCohortInterval:  24 * time.Hour,
	}
}

// newsScrubPrimaryBatch bounds the cheap per-tick auto-vet of primary links
// (match_confidence >= 1.0 — deterministically relevant, no Gemma). Generous
// because it's a single-column SQL UPDATE on uncontended rows; drains the
// backlog over a few ticks without ever holding a large lock.
const newsScrubPrimaryBatch = 20000

// alltimeRankSports are the sports whose season_composite_rank_alltime is
// recomputed on the AlltimeRankInterval cadence.
var alltimeRankSports = []string{"NBA", "NFL", "FOOTBALL"}

// Start launches all configured maintenance tickers. Blocks until ctx is
// cancelled. Intended to be called with `go`.
func Start(ctx context.Context, pool *pgxpool.Pool, cfg Config, scrubber *ml.NewsScrubber, logger *slog.Logger) {
	logger.Info("Maintenance tickers started",
		"cleanup", cfg.CleanupInterval,
		"digest", cfg.DigestInterval,
		"catchup", cfg.CatchUpInterval,
		"news_scrub", cfg.NewsScrubInterval,
		"pipeline_stats", cfg.StatsInterval,
		"peer_cohort", cfg.PeerCohortInterval)

	tickers := make([]*time.Ticker, 0, 8)
	defer func() {
		for _, t := range tickers {
			t.Stop()
		}
	}()

	// Cleanup: remove old sent/failed notifications and expired cache rows
	if cfg.CleanupInterval > 0 {
		t := time.NewTicker(cfg.CleanupInterval)
		tickers = append(tickers, t)
		go runLoop(ctx, t.C, "cleanup", func() { cleanup(ctx, pool, logger) })
	}

	// Digest: generate batch notification records for digest delivery
	if cfg.DigestInterval > 0 {
		t := time.NewTicker(cfg.DigestInterval)
		tickers = append(tickers, t)
		go runLoop(ctx, t.C, "digest", func() { generateDigests(ctx, pool, logger) })
	}

	// Catch-up: sweep for NOTIFY events missed during downtime
	if cfg.CatchUpInterval > 0 {
		t := time.NewTicker(cfg.CatchUpInterval)
		tickers = append(tickers, t)
		go runLoop(ctx, t.C, "catchup", func() { catchUpSweep(ctx, pool, logger) })
	}

	// All-time composite rank: recompute season_composite_rank_alltime
	// across all seasons per sport. Decoupled from finalize_fixture (which
	// only does within-season work) so the expensive all-seasons pass runs
	// on a deliberate cadence instead of every game. Runs once on startup
	// for post-deploy freshness, then on the interval.
	if cfg.AlltimeRankInterval > 0 {
		recalcAlltimeRanks(ctx, pool, logger)
		t := time.NewTicker(cfg.AlltimeRankInterval)
		tickers = append(tickers, t)
		go runLoop(ctx, t.C, "alltime_rank", func() { recalcAlltimeRanks(ctx, pool, logger) })
	}

	// News scrub: BACKLOG + repair only (FIRST-GPT-AUDIT Session 8). The daily
	// cmd/pipeline now scrubs its own freshly-ingested batch in-run, so this ticker
	// no longer drives fresh content into derivation — it mops up the older
	// unscrubbed tail, real-time inserts between pipeline runs, and links a failed
	// in-run scrub left behind. Skipped when Ollama is unreachable (scrubber == nil)
	// so a model outage never stalls the rest of maintenance.
	if cfg.NewsScrubInterval > 0 && scrubber != nil {
		t := time.NewTicker(cfg.NewsScrubInterval)
		tickers = append(tickers, t)
		go runLoop(ctx, t.C, "news_scrub", func() {
			scrubNewsLinks(ctx, pool, scrubber, cfg.NewsScrubBatch, logger)
		})
	}

	// Pipeline stats: the daily corpus snapshot (news + vibes + transfers growth +
	// coverage) into pipeline_stats. Pure SQL — no Gemma, so it runs regardless of
	// model availability. Once on startup for an immediate row, then on the interval.
	if cfg.StatsInterval > 0 {
		writePipelineStats(ctx, pool, logger)
		t := time.NewTicker(cfg.StatsInterval)
		tickers = append(tickers, t)
		go runLoop(ctx, t.C, "pipeline_stats", func() { writePipelineStats(ctx, pool, logger) })
	}

	// Peer-cohort aggregates (O1): refresh the precomputed per-cohort season
	// aggregates that /momentum reads instead of scanning the whole peer cohort
	// live. Pure SQL (refresh_peer_cohort_aggregates). The season-rolled stats it
	// summarizes only move on seeding/finalize, so a daily rebuild keeps the
	// trajectory peer-deltas current. Once on startup for post-deploy freshness,
	// then on the interval.
	if cfg.PeerCohortInterval > 0 {
		refreshPeerCohortAggregates(ctx, pool, logger)
		t := time.NewTicker(cfg.PeerCohortInterval)
		tickers = append(tickers, t)
		go runLoop(ctx, t.C, "peer_cohort", func() { refreshPeerCohortAggregates(ctx, pool, logger) })
	}

	<-ctx.Done()
	logger.Info("Maintenance tickers stopped")
}

func runLoop(ctx context.Context, ch <-chan time.Time, name string, fn func()) {
	for {
		select {
		case <-ch:
			fn()
		case <-ctx.Done():
			return
		}
	}
}

// --------------------------------------------------------------------------
// Task implementations
// --------------------------------------------------------------------------

// cleanup removes notifications older than 30 days that have been sent or failed.
func cleanup(ctx context.Context, pool *pgxpool.Pool, logger *slog.Logger) {
	tag, err := pool.Exec(ctx, `
		DELETE FROM notifications
		WHERE status IN ('sent', 'failed')
		  AND updated_at < NOW() - INTERVAL '30 days'`)
	if err != nil {
		logger.Warn("Cleanup: failed to purge old notifications", "error", err)
	} else if tag.RowsAffected() > 0 {
		logger.Info("Cleanup: purged old notifications", "count", tag.RowsAffected())
	}
}

// generateDigests creates batch notification summaries for users who prefer
// digest-style delivery instead of real-time pushes.
// Currently a placeholder — will be implemented when user preference tables
// include a delivery_mode column.
func generateDigests(ctx context.Context, pool *pgxpool.Pool, logger *slog.Logger) {
	// TODO: When user preferences support digest mode:
	// 1. Query users with delivery_mode = 'digest'
	// 2. Aggregate pending changes since last digest
	// 3. Build summary notification and insert into notifications table
	_ = ctx
	_ = pool
	_ = logger
}

// lastSeenSeason tracks the current_season observed on the previous
// recalcAlltimeRanks run, per sport, to detect season rollover. Accessed
// only by the single alltime_rank ticker goroutine (+ the synchronous
// startup call before that goroutine launches), so no locking needed.
var lastSeenSeason = map[string]int{}

// recalcAlltimeRanks refreshes season_composite_rank_alltime, honoring the
// "previous seasons read-only, current season dynamic" invariant:
//
//   - First run after startup, or when current_season changed since the
//     last run (rollover): full re-baseline — recalculate_alltime_ranks(sport,
//     NULL) re-ranks every season against the complete history. Folds a
//     just-completed season into the permanent record and keeps prior
//     seasons consistent post-deploy.
//   - Steady state: current-season-only — recalculate_alltime_ranks(sport,
//     current_season) reads the full history as the comparison pool but
//     writes only the current season's rows. Previous seasons are never
//     touched in-season.
//
// Decoupled from finalize_fixture (within-season work only) so the
// all-seasons read runs on a deliberate nightly cadence — the all-time
// percentile barely moves game-to-game.
func recalcAlltimeRanks(ctx context.Context, pool *pgxpool.Pool, logger *slog.Logger) {
	for _, sport := range alltimeRankSports {
		var current int
		if err := pool.QueryRow(ctx,
			`SELECT current_season FROM public.sports WHERE id = $1`, sport,
		).Scan(&current); err != nil {
			logger.Warn("All-time rank: current_season lookup failed", "sport", sport, "error", err)
			continue
		}

		prev, seen := lastSeenSeason[sport]
		fullRebaseline := !seen || prev != current
		var scope *int // nil → full re-baseline; else current-season-only
		if !fullRebaseline {
			scope = &current
		} else if seen {
			logger.Info("All-time rank: season rollover, full re-baseline",
				"sport", sport, "from", prev, "to", current)
		}

		// Retry on deadlock — this writes the same player_stats / team_stats
		// rows the seeder's finalize_fixture touches, so transient
		// deadlocks under active seeding are expected. Postgres kills one
		// side; we retry. Three attempts is ample for a multi-second task.
		var players, teams int
		var err error
		for attempt := 1; attempt <= 3; attempt++ {
			err = pool.QueryRow(ctx,
				`SELECT players_updated, teams_updated FROM recalculate_alltime_ranks($1, $2)`,
				sport, scope,
			).Scan(&players, &teams)
			if err == nil || ctx.Err() != nil {
				break
			}
			if !strings.Contains(err.Error(), "deadlock") {
				break // non-transient error; don't retry
			}
			logger.Info("All-time rank: deadlock, retrying", "sport", sport, "attempt", attempt)
			select {
			case <-time.After(2 * time.Second):
			case <-ctx.Done():
			}
		}
		if err != nil {
			logger.Warn("All-time rank: recompute failed", "sport", sport, "error", err)
			continue
		}
		lastSeenSeason[sport] = current
		logger.Info("All-time rank: recomputed",
			"sport", sport, "scope", map[bool]string{true: "full", false: "current"}[fullRebaseline],
			"players", players, "teams", teams)

		// Append to the rating_history time-series after the all-time ranks are
		// fresh (the snapshot reads season_composite_rank_alltime). Current season
		// → in-season trajectory; on rollover, also stamp the just-concluded
		// season's final frozen row. Debounced insert-if-changed, so unchanged
		// entities add nothing.
		snapshotRatingHistory(ctx, pool, sport, current, "in_season", logger)
		if fullRebaseline && seen && prev != current {
			snapshotRatingHistory(ctx, pool, sport, prev, "season_close", logger)
		}
	}
}

// snapshotRatingHistory appends per-entity rating snapshots for (sport, season)
// into rating_history (debounced insert-if-changed). Best-effort — a snapshot
// failure must never stall the all-time-rank ticker.
//
// O3 (Optimization Ledger): rating_history is intentionally WRITE-ONLY today — an
// ML archive + the FUTURE trajectory source for /momentum. It only began accruing
// 2026-06-17 (migration 092), so it currently holds ~1-2 points per entity, far too
// shallow to drive the momentum sparkline (which reads per-event composite_score off
// event_box_scores/event_team_stats instead). Once it has multi-point depth per
// entity, wire it as the /momentum trajectory source (pairs with O1's cohort
// precompute). Until then, leave it as the archive — do NOT add a reader.
func snapshotRatingHistory(ctx context.Context, pool *pgxpool.Pool, sport string, season int, trigger string, logger *slog.Logger) {
	var inserted int
	if err := pool.QueryRow(ctx,
		`SELECT snapshot_rating_history($1, $2, $3)`, sport, season, trigger,
	).Scan(&inserted); err != nil {
		logger.Warn("Rating history: snapshot failed",
			"sport", sport, "season", season, "trigger", trigger, "error", err)
		return
	}
	if inserted > 0 {
		logger.Info("Rating history: snapshot",
			"sport", sport, "season", season, "trigger", trigger, "rows", inserted)
	}
}

// catchUpSweep checks for entities with high percentiles that may not have
// had their NOTIFY events processed (e.g., during listener downtime).
// Compares current percentiles against the last archived snapshot and
// re-triggers notification processing for any gaps.
func catchUpSweep(ctx context.Context, pool *pgxpool.Pool, logger *slog.Logger) {
	// Find player_stats with percentiles >= 90 that were updated recently
	// but don't have a corresponding notification scheduled
	tag, err := pool.Exec(ctx, `
		INSERT INTO notifications (user_id, entity_type, entity_id, sport, stat_key, percentile, message, status, scheduled_for)
		SELECT
			uf.user_id,
			'player',
			ps.player_id,
			ps.sport,
			kv.key,
			(kv.value::text)::numeric,
			p.name || ' reached ' || round((kv.value::text)::numeric) || 'th percentile in ' || COALESCE(sd.display_name, kv.key),
			'scheduled',
			NOW()
		FROM player_stats ps
		CROSS JOIN LATERAL jsonb_each(ps.percentiles) AS kv(key, value)
		JOIN players p ON p.id = ps.player_id AND p.sport = ps.sport
		JOIN user_follows uf ON uf.entity_type = 'player' AND uf.entity_id = ps.player_id AND uf.sport = ps.sport
		LEFT JOIN stat_definitions sd ON sd.sport = ps.sport AND sd.key_name = kv.key AND sd.entity_type = 'player'
		WHERE kv.key NOT LIKE '\_%'
		  -- skip per-rate siblings; the suffix family lives in rate_modes
		  AND NOT EXISTS (SELECT 1 FROM rate_modes rm WHERE kv.key ~ (rm.suffix || '$'))
		  AND jsonb_typeof(kv.value) = 'number'
		  AND (kv.value::text)::numeric >= 90
		  AND ps.updated_at > NOW() - INTERVAL '1 hour'
		  AND NOT EXISTS (
			SELECT 1 FROM notifications n
			WHERE n.entity_type = 'player'
			  AND n.entity_id = ps.player_id
			  AND n.sport = ps.sport
			  AND n.stat_key = kv.key
			  AND n.created_at > NOW() - INTERVAL '2 hours'
		  )
		ON CONFLICT DO NOTHING`)
	if err != nil {
		logger.Warn("Catch-up sweep: failed", "error", err)
	} else if tag.RowsAffected() > 0 {
		logger.Info("Catch-up sweep: created missed notifications", "count", tag.RowsAffected())
	}
}

// refreshPeerCohortAggregates rebuilds peer_cohort_aggregate (Optimization Ledger
// O1) — the precomputed per-cohort season aggregates that /momentum reconstructs
// exact leave-one-out peer deltas from, instead of scanning the whole peer cohort
// live on every read. Pure SQL; refresh_peer_cohort_aggregates() does a
// transactional DELETE+INSERT so readers see the prior snapshot until commit. On
// error the existing snapshot is left in place and the read path keeps serving it.
func refreshPeerCohortAggregates(ctx context.Context, pool *pgxpool.Pool, logger *slog.Logger) {
	var n int
	if err := pool.QueryRow(ctx, `SELECT public.refresh_peer_cohort_aggregates()`).Scan(&n); err != nil {
		logger.Warn("Peer-cohort refresh failed", "error", err)
		return
	}
	logger.Info("Peer-cohort aggregates refreshed", "cohorts", n)
}

// scrubNewsLinks is the Gemma ID-gate sweep — the async "scrub". Since
// FIRST-GPT-AUDIT Session 8 this is the BACKLOG/repair path, not the primary
// scrub: the daily pipeline scrubs the batch it just ingested in-run, so what
// reaches this newest-first sweep is the older tail, real-time inserts, and
// failed-in-run links. Two bounded, non-destructive phases per tick:
//
//  1. Auto-vet primaries (cheap SQL, no Gemma): links at match_confidence >= 1.0
//     are the entity the article was fetched for — deterministically relevant, no
//     disambiguation needed. Stamp them vetted=true in one bounded UPDATE.
//  2. Gemma pass: take the newest `batch` candidate-rich articles (those with an
//     unscrubbed SECONDARY link, conf < 1.0 — the ones that actually need
//     disambiguation) and scrub each serially, writing vetted verdicts.
//
// Newest-first so the recent window the consumers read stays scrubbed; the
// ancient tail is harmless (consumers filter by recency). Serial Gemma calls keep
// us a good GPU citizen — Ollama serializes anyway, so a reactive vibe/transfer
// spike just waits one call. A single article's error is logged and skipped; the
// sweep never aborts mid-batch.
func scrubNewsLinks(ctx context.Context, pool *pgxpool.Pool, scrubber *ml.NewsScrubber, batch int, logger *slog.Logger) {
	if scrubber == nil || batch <= 0 {
		return
	}

	// Phase 1 — auto-vet unscrubbed primaries (bounded, no Gemma).
	if tag, err := pool.Exec(ctx, `
		WITH b AS (
			SELECT ctid FROM news_article_entities
			 WHERE scrubbed_at IS NULL AND match_confidence >= 1.0
			 LIMIT $1
		)
		UPDATE news_article_entities n
		   SET vetted = TRUE, scrubbed_at = NOW()
		  FROM b WHERE n.ctid = b.ctid`, newsScrubPrimaryBatch); err != nil {
		logger.Warn("News scrub: auto-vet primaries failed", "error", err)
	} else if tag.RowsAffected() > 0 {
		logger.Info("News scrub: auto-vetted primary links", "count", tag.RowsAffected())
	}

	// Phase 2 — collect the newest candidate-rich articles (one row per article;
	// drained into a slice before the slow Gemma loop so we don't hold a cursor).
	type job struct {
		id    int64
		sport string
	}
	var jobs []job
	rows, err := pool.Query(ctx, `
		SELECT nae.article_id, nae.sport
		FROM news_article_entities nae
		JOIN news_articles a ON a.id = nae.article_id
		WHERE nae.scrubbed_at IS NULL AND nae.match_confidence < 1.0
		GROUP BY nae.article_id, nae.sport, a.published_at
		ORDER BY a.published_at DESC NULLS LAST
		LIMIT $1`, batch)
	if err != nil {
		logger.Warn("News scrub: candidate query failed", "error", err)
		return
	}
	for rows.Next() {
		var j job
		if err := rows.Scan(&j.id, &j.sport); err != nil {
			rows.Close()
			logger.Warn("News scrub: scan failed", "error", err)
			return
		}
		jobs = append(jobs, j)
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		logger.Warn("News scrub: candidate rows error", "error", err)
		return
	}
	if len(jobs) == 0 {
		return
	}

	var articles, kept, dropped, failed int
	for _, j := range jobs {
		if ctx.Err() != nil {
			break
		}
		res, err := scrubber.ScrubArticle(ctx, j.id, j.sport, false /* persist */)
		if err != nil {
			failed++
			logger.Warn("News scrub: article failed", "article_id", j.id, "sport", j.sport, "error", err)
			continue
		}
		articles++
		for _, v := range res.Verdicts {
			if v.Relevant {
				kept++
			} else {
				dropped++
			}
		}
	}
	logger.Info("News scrub: swept",
		"articles", articles, "kept", kept, "dropped", dropped, "failed", failed, "batch", batch)
}

// pipelineStatsSports are the sports a daily pipeline_stats snapshot is written for.
var pipelineStatsSports = []string{"NBA", "NFL", "FOOTBALL"}

// writePipelineStats upserts one pipeline_stats row per (sport, today) capturing
// the corpus asset's daily size + coverage: article counts, the count of entities
// with a CURRENT (<24h) narrative / vibe, the number of distinct ACTIVE transfer
// pairs (latest generation, heat > 0, within 14d — not raw append rows), and
// coverage/staleness of vibe analysis over the in-scope (fresh-news) entity set.
// Pure SQL, idempotent per day (ON CONFLICT upsert).
func writePipelineStats(ctx context.Context, pool *pgxpool.Pool, logger *slog.Logger) {
	const upsert = `
		INSERT INTO pipeline_stats (
		    sport, snapshot_date,
		    total_articles, new_articles,
		    entities_with_summary, entities_with_vibe, transfer_rumors_active,
		    coverage_pct, median_staleness_hours
		)
		SELECT
		    $1, CURRENT_DATE,
		    art.total_articles, art.new_articles,
		    cov.entities_with_summary, cov.entities_with_vibe, tr.active,
		    cov.coverage_pct, cov.median_staleness_hours
		FROM
		(
		    SELECT count(DISTINCT article_id) AS total_articles,
		           count(DISTINCT article_id) FILTER (WHERE created_at > NOW() - INTERVAL '24 hours') AS new_articles
		    FROM news_article_entities WHERE sport = $1
		) art,
		(
		    -- Active = latest row per pair, Gemma-VETTED (is_rumor IS TRUE), heat>0,
		    -- within 14d — matches the /transfers read contract (S10 fail-closed): a
		    -- newer cleared/unknown verdict drops the pair from the count.
		    SELECT count(*) AS active FROM (
		        SELECT DISTINCT ON (team_id, player_id) heat, is_rumor, generated_at
		        FROM transfer_rumors WHERE sport = $1
		        ORDER BY team_id, player_id, generated_at DESC
		    ) latest
		    WHERE heat > 0 AND is_rumor IS TRUE AND generated_at > NOW() - INTERVAL '14 days'
		) tr,
		(
		    WITH in_scope AS (
		        SELECT DISTINCT nae.entity_type, nae.entity_id
		        FROM news_article_entities nae JOIN news_articles a ON a.id = nae.article_id
		        WHERE nae.sport = $1 AND nae.vetted IS TRUE
		          AND (a.published_at IS NULL OR a.published_at > NOW() - INTERVAL '72 hours')
		    ),
		    lv AS (
		        SELECT DISTINCT ON (entity_type, entity_id) entity_type, entity_id, generated_at
		        FROM vibe_scores WHERE sport = $1 AND sentiment IS NOT NULL
		        ORDER BY entity_type, entity_id, generated_at DESC
		    ),
		    ls AS (
		        SELECT DISTINCT ON (entity_type, entity_id) entity_type, entity_id, generated_at
		        FROM news_summaries WHERE sport = $1 AND body IS NOT NULL
		        ORDER BY entity_type, entity_id, generated_at DESC
		    )
		    SELECT
		        (SELECT count(*) FROM ls WHERE generated_at > NOW() - INTERVAL '24 hours') AS entities_with_summary,
		        (SELECT count(*) FROM lv WHERE generated_at > NOW() - INTERVAL '24 hours') AS entities_with_vibe,
		        CASE WHEN (SELECT count(*) FROM in_scope) > 0
		            THEN round(100.0 * (SELECT count(*) FROM in_scope i JOIN lv ON lv.entity_type = i.entity_type AND lv.entity_id = i.entity_id
		                                 WHERE lv.generated_at > NOW() - INTERVAL '24 hours')
		                        / (SELECT count(*) FROM in_scope), 2)
		            ELSE NULL END AS coverage_pct,
		        (SELECT round(percentile_cont(0.5) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM (NOW() - lv.generated_at)) / 3600.0)::numeric, 2)
		            FROM in_scope i JOIN lv ON lv.entity_type = i.entity_type AND lv.entity_id = i.entity_id) AS median_staleness_hours
		) cov
		ON CONFLICT (sport, snapshot_date) DO UPDATE SET
		    total_articles         = EXCLUDED.total_articles,
		    new_articles           = EXCLUDED.new_articles,
		    entities_with_summary  = EXCLUDED.entities_with_summary,
		    entities_with_vibe     = EXCLUDED.entities_with_vibe,
		    transfer_rumors_active = EXCLUDED.transfer_rumors_active,
		    coverage_pct           = EXCLUDED.coverage_pct,
		    median_staleness_hours = EXCLUDED.median_staleness_hours,
		    generated_at           = NOW()`

	var written int
	for _, sport := range pipelineStatsSports {
		if _, err := pool.Exec(ctx, upsert, sport); err != nil {
			logger.Warn("Pipeline stats: upsert failed", "sport", sport, "error", err)
			continue
		}
		written++
	}
	if written > 0 {
		logger.Info("Pipeline stats: daily snapshot written", "sports", written)
	}
}
