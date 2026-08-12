// Package corpus holds the RSS sweep primitives used by the ingest pipeline.
package corpus

import (
	"context"
	"log/slog"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/thirdparty"
)

// sweepTimeout caps one team's full RSS sweep. Each individual RSS HTTP call
// already times out at 15s; the wider alias/language net needs a larger outer
// budget so slow editions do not cancel the whole team too aggressively.
const sweepTimeout = 90 * time.Second

// Team is a team we sweep RSS for.
type Team struct {
	ID        int
	Sport     string
	Name      string
	ShortCode string
	Aliases   []string
}

// Sweep RSS-fetches every team in scope. Each team's matched articles are
// persisted to news_articles with the Editor's read enqueued in the same
// transaction (thirdparty.NewsService write-through). ok/fail count the RSS
// calls. Honors ctx cancellation between teams.
//
// Observability: every team's fetch funnel is folded into a per-sport and a
// per-run total (thirdparty.Funnel). The rolled-up lines land at Info as each
// sport finishes and again at the end of the run, with per-team detail at
// Debug. The per-sport line is deliberately emitted before the run total: a
// sweep that is killed mid-flight — which has happened — still leaves evidence
// of where its articles went.
func Sweep(ctx context.Context, pool *pgxpool.Pool, sports []string, rssLimit, rssPauseMs int, logger *slog.Logger) (ok, fail int) {
	news := thirdparty.NewNewsService(pool, logger)
	runStart := time.Now().UTC()
	fresh := 0 // articles handed to the Editor this run

	var runFunnel thirdparty.Funnel

	for _, sport := range sports {
		teams, err := LoadTeams(ctx, pool, sport)
		if err != nil {
			logger.Error("corpus: load teams failed", "sport", sport, "error", err)
			continue
		}
		logger.Info("corpus: rss sweep starting", "sport", sport, "teams", len(teams))

		var sportFunnel thirdparty.Funnel
		var zeroAdmitted []string // teams RSS returned items for and the sweep kept none of

		for i, t := range teams {
			if ctx.Err() != nil {
				logSportFunnel(logger, sport, len(teams), i, sportFunnel, zeroAdmitted)
				runFunnel.Add(sportFunnel)
				logRunFunnel(logger, runFunnel)
				return ok, fail
			}
			tctx, cancel := context.WithTimeout(ctx, sweepTimeout)
			ids, funnel, err := news.GetEntityNews(tctx, "team", t.ID, t.Name, t.Sport, rssLimit, t.Aliases)
			cancel()

			sportFunnel.Add(funnel)
			logger.Debug("corpus: team funnel",
				append([]any{"sport", sport, "team", t.Name, "id", t.ID}, funnel.LogAttrs()...)...)
			if funnel.RSSItems > 0 && funnel.Matched == 0 {
				zeroAdmitted = append(zeroAdmitted, t.Name)
			}
			if err != nil {
				fail++
				logger.Warn("corpus: rss fetch failed", "sport", sport, "team", t.Name, "id", t.ID, "error", err)
			} else {
				ok++
				fresh += len(ids)
			}
			if (i+1)%sweepProgressEvery == 0 {
				logger.Info("corpus: rss sweep progress",
					append([]any{"sport", sport, "teams_done", i + 1, "teams", len(teams)}, sportFunnel.LogAttrs()...)...)
			}
			if rssPauseMs > 0 {
				time.Sleep(time.Duration(rssPauseMs) * time.Millisecond)
			}
		}

		logSportFunnel(logger, sport, len(teams), len(teams), sportFunnel, zeroAdmitted)
		runFunnel.Add(sportFunnel)
	}

	// `fresh_articles` is what will be READ, not what was WRITTEN — D-T21's cap can withhold a
	// read from an article it just stored, so the two are printed side by side. A sweep that
	// says `fresh_articles=0 reads_withheld=788` did real work; one that says `0 0` did not.
	logger.Info("corpus: rss sweep complete",
		"ok", ok, "fail", fail, "fresh_articles", fresh,
		"reads_withheld", runFunnel.ReadsWithheld,
		"elapsed", time.Since(runStart).Round(time.Second))
	logRunFunnel(logger, runFunnel)
	return ok, fail
}

// sweepProgressEvery is how often the running per-sport funnel is echoed at Info
// during a long sport. FOOTBALL is ~142 teams over ten minutes; without this the
// only signal between "starting" and "complete" is silence.
const sweepProgressEvery = 50

// zeroAdmittedSample bounds the team names carried on the per-sport line. The
// count is the metric; the names are just enough to start debugging.
const zeroAdmittedSample = 10

// logSportFunnel emits one sport's rolled-up funnel. teamsDone is separate from
// teamsTotal so a cancelled sweep reports honestly on what it actually covered.
func logSportFunnel(logger *slog.Logger, sport string, teamsTotal, teamsDone int, f thirdparty.Funnel, zeroAdmitted []string) {
	sample := zeroAdmitted
	if len(sample) > zeroAdmittedSample {
		sample = sample[:zeroAdmittedSample]
	}
	logger.Info("corpus: rss sweep funnel",
		append([]any{
			"sport", sport,
			"teams", teamsTotal,
			"teams_done", teamsDone,
			// A team that fetched items and admitted none is what a broken query
			// plan looks like from here. One or two is normal; a whole sport going
			// quiet is the regression this counter exists to catch.
			"teams_zero_admitted", len(zeroAdmitted),
			"zero_admitted_sample", strings.Join(sample, "; "),
		}, f.LogAttrs()...)...)
}

// logRunFunnel emits the whole-run funnel and flags a non-zero residual, which
// means the fetch path grew a drop stage that nothing counts.
func logRunFunnel(logger *slog.Logger, f thirdparty.Funnel) {
	logger.Info("corpus: rss sweep funnel total", f.LogAttrs()...)
	if r := f.Residual(); r != 0 {
		logger.Warn("corpus: funnel does not balance — an uncounted drop stage exists in the RSS fetch path",
			"residual", r)
	}
}

// LoadTeams returns every team in the sport (no tier filter — coverage shouldn't
// collapse in the offseason or for eliminated teams; the count is small).
func LoadTeams(ctx context.Context, pool *pgxpool.Pool, sport string) ([]Team, error) {
	qctx, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT id, sport, name, COALESCE(short_code, ''), COALESCE(search_aliases, ARRAY[]::text[])
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
		if err := rows.Scan(&t.ID, &t.Sport, &t.Name, &t.ShortCode, &t.Aliases); err != nil {
			return nil, err
		}
		if t.ShortCode != "" {
			t.Aliases = append(t.Aliases, t.ShortCode)
		}
		out = append(out, t)
	}
	return out, rows.Err()
}
