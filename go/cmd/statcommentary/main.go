// statcommentary — CLI for the Gemma stats-rail commentary (the on-field IDENTITY
// analysis derived from an entity's rating-engine datapoints: composite = how well,
// peak = how, + the per-x lens).
//
// Four modes:
//
//	single (default) — one entity (dry-run unless -persist).
//	  go run ./cmd/statcommentary -entity-type player -entity-id 15 -sport NBA
//	  go run ./cmd/statcommentary -entity-type team -entity-id 18 -sport FOOTBALL -season 2025 -persist
//
//	backfill — every (entity, season) with a rating but no commentary yet. Previous
//	           seasons are immutable, so this is a one-time pass; the current season
//	           is then maintained by nightly. Heavy (one Gemma call per entity-season).
//	  go run ./cmd/statcommentary -mode backfill            # all sports
//	  go run ./cmd/statcommentary -mode backfill -sport NBA -limit 50
//
//	nightly — the current season only, regenerating an entity ONLY when its rating
//	          snapshot changed (input_hash differs) — the "work only on new data" gate.
//	  go run ./cmd/statcommentary -mode nightly
//
//	restamp — one-time vocabulary migration (no Gemma): rename the input-component
//	          keys "sigil_label"/"sigil_score" → "peak_label"/"peak_score" + recompute
//	          input_hash on existing rows, so the key rename does not re-generate the corpus.
//	  go run ./cmd/statcommentary -mode restamp
//
// Env: DATABASE_PRIVATE_URL (or fallbacks) + OLLAMA_* (restamp needs only the DB).
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
	"github.com/albapepper/scoracle-data/internal/corpus"
	"github.com/albapepper/scoracle-data/internal/jobrun"
	"github.com/albapepper/scoracle-data/internal/ml"
)

func main() {
	mode := flag.String("mode", "single", "single | backfill | nightly")
	entityType := flag.String("entity-type", "player", "[single] player | team")
	entityID := flag.Int("entity-id", 0, "[single] canonical entity id")
	season := flag.Int("season", 0, "[single] season (0 = latest)")
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL | all (single requires it; corpus defaults to all)")
	trigger := flag.String("trigger", "manual", "[single] manual | periodic | stat_change")
	persist := flag.Bool("persist", false, "[single] persist (default: dry-run)")
	skipUnchanged := flag.Bool("skip-unchanged", false, "[single] short-circuit (no Gemma) when the rating snapshot is unchanged — verifies the debounce gate")
	throttleMs := flag.Int("throttle-ms", 0, "[backfill/nightly] pause N ms between entities")
	limit := flag.Int("limit", 0, "[backfill/nightly] cap GENERATIONS (Gemma calls) per run; 0 = unbounded (hash-skips don't count)")
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
	// restamp is a pure DB vocabulary migration (no Gemma), so skip the Ollama ping.
	if *mode != "restamp" {
		if err := ollama.Ping(context.Background()); err != nil {
			logger.Error("ollama unreachable", "error", err, "base_url", cfg.OllamaBaseURL)
			os.Exit(1)
		}
	}
	gen := ml.NewRatingGenerator(pool, ollama)

	switch *mode {
	case "single":
		runSingle(pool, gen, *entityType, *entityID, *season, *sport, *trigger, *persist, *skipUnchanged, cfg.OllamaTimeout, logger)
	case "backfill":
		os.Exit(runCorpus(pool, gen, false, *sport, *throttleMs, *limit, cfg.OllamaTimeout, cfg.DatabaseURL, logger))
	case "nightly":
		os.Exit(runCorpus(pool, gen, true, *sport, *throttleMs, *limit, cfg.OllamaTimeout, cfg.DatabaseURL, logger))
	case "restamp":
		runReStamp(pool, gen, *sport, logger)
	default:
		fmt.Fprintf(os.Stderr, "unknown -mode %q; valid: single | backfill | nightly | restamp\n", *mode)
		os.Exit(2)
	}
}

// ---------------------------------------------------------------------------
// Single mode
// ---------------------------------------------------------------------------

func runSingle(pool *pgxpool.Pool, gen *ml.RatingGenerator, entityType string, entityID, season int, sport, trigger string, persist, skipUnchanged bool, timeout time.Duration, logger *slog.Logger) {
	if entityID <= 0 || sport == "" {
		fmt.Fprintln(os.Stderr, "-entity-id and -sport are required in single mode")
		os.Exit(2)
	}
	ctx, cancel := context.WithTimeout(context.Background(), timeout+10*time.Second)
	defer cancel()

	sportUpper := strings.ToUpper(sport)
	name, err := corpus.LookupEntityName(ctx, pool, entityType, entityID, sportUpper)
	if err != nil || name == "" {
		logger.Error("entity lookup failed", "error", err)
		os.Exit(1)
	}

	res, err := gen.Generate(ctx, ml.RatingRequest{
		EntityType:    entityType,
		EntityID:      entityID,
		EntityName:    name,
		Sport:         sportUpper,
		Season:        seasonPtr(season),
		TriggerType:   trigger,
		DryRun:        !persist,
		SkipUnchanged: skipUnchanged,
	})
	if err != nil {
		logger.Error("stat commentary generation failed", "error", err)
		os.Exit(1)
	}

	mode := "DRY-RUN (not persisted)"
	if persist {
		mode = "PERSISTED to stat_summaries"
	}
	fmt.Printf("\n=== Stat commentary: %s (%s %d, %s s%d) — %s ===\n", name, entityType, entityID, sportUpper, res.Season, mode)
	switch {
	case res.SkippedNoStats:
		fmt.Println("(no usable rating profile — null marker)")
	case res.SkippedUnchanged:
		fmt.Println("(unchanged since last commentary — skipped)")
	default:
		fmt.Printf("\n[notability %d/100] %v\n\n%s\n", res.Notability, res.NotabilityComponents, res.Body)
	}
	fmt.Printf("\n(model=%s prompt=%s hash=%s duration=%s)\n", res.Model, res.PromptVersion, res.InputHash, res.Duration.Round(10*time.Millisecond))
}

// ---------------------------------------------------------------------------
// Corpus modes — backfill (all missing entity-seasons) + nightly (current season,
// regenerate only on a changed rating snapshot).
// ---------------------------------------------------------------------------

type target struct {
	entityType string
	entityID   int
	season     int
	sportName  string
}

// runCorpus runs backfill/nightly and returns a process exit code (FIRST-GPT-AUDIT
// Session 13): 0 = success or clean overlap-skip; 1 = every sport's enumeration
// failed, or every generation we attempted failed (systemic); 3 = partial (some
// per-entity generations failed but the run made progress). The run is recorded in
// pipeline_runs.
func runCorpus(pool *pgxpool.Pool, gen *ml.RatingGenerator, nightly bool, sportArg string, throttleMs, limit int, gemmaTimeout time.Duration, dbURL string, logger *slog.Logger) int {
	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if s := strings.ToLower(strings.TrimSpace(sportArg)); s != "" && s != "all" {
		sports = []string{strings.ToUpper(sportArg)}
	}
	ctx := context.Background()
	label := map[bool]string{true: "nightly", false: "backfill"}[nightly]
	start := time.Now()

	// Overlap guard + durable run record. A second statcommentary run (e.g. a manual
	// backfill racing the nightly cron) finds the lock held and exits cleanly.
	run, acquired, err := jobrun.Guard(ctx, pool, dbURL, "statcommentary")
	if err != nil {
		logger.Error("statcommentary: run-guard failed", "error", err)
		return 1
	}
	if !acquired {
		logger.Warn("statcommentary: another statcommentary run holds the lock — exiting cleanly")
		return 0
	}
	defer run.Close()

	var targets []target
	enumErrs := 0
	for _, sp := range sports {
		var ts []target
		var eerr error
		if nightly {
			season, serr := currentSeason(ctx, pool, sp)
			if serr != nil || season == 0 {
				enumErrs++
				logger.Warn("statcommentary: current_season lookup failed", "sport", sp, "error", serr)
				continue
			}
			ts, eerr = enumCurrentSeason(ctx, pool, sp, season)
		} else {
			ts, eerr = enumMissing(ctx, pool, sp)
		}
		if eerr != nil {
			enumErrs++
			logger.Error("statcommentary: enumerate failed", "mode", label, "sport", sp, "error", eerr)
			continue
		}
		targets = append(targets, ts...)
	}
	// If every sport failed to enumerate, the run could not even see its work-list.
	if enumErrs == len(sports) {
		runErr := fmt.Errorf("enumeration failed for all %d sport(s)", len(sports))
		if ferr := run.Finish(ctx, jobrun.StatusFailed, jobrun.Counts{}, runErr); ferr != nil {
			logger.Warn("statcommentary: record run failed", "error", ferr)
		}
		logger.Error("statcommentary: enumeration failed for all sports", "mode", label)
		return 1
	}
	logger.Info("statcommentary: starting", "mode", label, "sports", sports, "targets", len(targets), "gen_limit", limit)

	// limit caps GENERATIONS (Gemma calls), not entities scanned — cheap hash-skips
	// don't consume the budget, so a filled nightly run still hash-checks the whole
	// current season (DB-only) while a fill night is bounded to `limit` real writes.
	ok, skippedUnchanged, noStats, fail := 0, 0, 0, 0
	for i, t := range targets {
		if limit > 0 && ok >= limit {
			logger.Info("statcommentary: generation limit reached", "mode", label, "limit", limit, "scanned", i)
			break
		}
		lctx, lcancel := context.WithTimeout(ctx, 5*time.Second)
		name, err := corpus.LookupEntityName(lctx, pool, t.entityType, t.entityID, t.sportName)
		lcancel()
		if err != nil || name == "" {
			fail++
			continue
		}
		gctx, cancel := context.WithTimeout(ctx, gemmaTimeout+10*time.Second)
		res, err := gen.Generate(gctx, ml.RatingRequest{
			EntityType:    t.entityType,
			EntityID:      t.entityID,
			EntityName:    name,
			Sport:         t.sportName,
			Season:        seasonPtr(t.season),
			TriggerType:   "periodic",
			SkipUnchanged: nightly,
		})
		cancel()
		switch {
		case err != nil:
			fail++
			logger.Warn("statcommentary: generate failed", "sport", t.sportName, "entity", name, "id", t.entityID, "season", t.season, "error", err)
		case res.SkippedUnchanged:
			skippedUnchanged++
		case res.SkippedNoStats:
			noStats++
		default:
			ok++
		}
		if (i+1)%25 == 0 {
			logger.Info("statcommentary: progress", "mode", label, "done", i+1, "total", len(targets),
				"ok", ok, "unchanged", skippedUnchanged, "no_stats", noStats, "fail", fail)
		}
		if throttleMs > 0 {
			time.Sleep(time.Duration(throttleMs) * time.Millisecond)
		}
	}
	// Decide the outcome. Everything-failed (we attempted generations but none
	// succeeded) is treated as systemic ⇒ failure; some failures ⇒ partial.
	status := jobrun.StatusSuccess
	exit := 0
	var runErr error
	switch {
	case fail > 0 && ok == 0 && skippedUnchanged == 0 && noStats == 0:
		status, exit = jobrun.StatusFailed, 1
		runErr = fmt.Errorf("all %d attempted generation(s) failed", fail)
	case fail > 0:
		status, exit = jobrun.StatusPartial, 3
		runErr = fmt.Errorf("%d generation(s) failed (retried next run)", fail)
	}

	counts := jobrun.Counts{
		Attempted: ok + skippedUnchanged + noStats + fail,
		Succeeded: ok,
		Skipped:   skippedUnchanged + noStats,
		Failed:    fail,
	}
	if ferr := run.Finish(ctx, status, counts, runErr); ferr != nil {
		logger.Warn("statcommentary: record run failed", "error", ferr)
	}
	logger.Info("statcommentary: complete", "mode", label, "status", status, "exit", exit,
		"ok", ok, "unchanged", skippedUnchanged, "no_stats", noStats, "fail", fail,
		"elapsed", time.Since(start).Round(time.Second))
	return exit
}

// ---------------------------------------------------------------------------
// Enumeration
// ---------------------------------------------------------------------------

func currentSeason(ctx context.Context, pool *pgxpool.Pool, sport string) (int, error) {
	qctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	var season int
	err := pool.QueryRow(qctx, `SELECT current_season FROM public.sports WHERE id = $1`, sport).Scan(&season)
	return season, err
}

// enumMissing returns every (entity, season) with a rating but no stat_summaries
// row yet (real or marker) — the backfill work-list, newest season first.
func enumMissing(ctx context.Context, pool *pgxpool.Pool, sport string) ([]target, error) {
	qctx, cancel := context.WithTimeout(ctx, 60*time.Second)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT c.et, c.id, c.season FROM (
		    SELECT 'player'::text AS et, player_id AS id, season FROM player_stats
		     WHERE sport = $1 AND rating_composite_score IS NOT NULL
		     GROUP BY player_id, season
		    UNION ALL
		    SELECT 'team'::text, team_id, season FROM team_stats
		     WHERE sport = $1 AND rating_composite_score IS NOT NULL
		     GROUP BY team_id, season
		) c
		WHERE NOT EXISTS (
		    SELECT 1 FROM stat_summaries s
		     WHERE s.entity_type = c.et AND s.entity_id = c.id AND s.sport = $1 AND s.season = c.season
		)
		ORDER BY c.season DESC, c.et, c.id`, sport)
	if err != nil {
		return nil, err
	}
	return scanTargets(rows, sport)
}

// enumCurrentSeason returns every entity with a rating in the given (current)
// season — the nightly work-list (each is then hash-gated inside Generate).
func enumCurrentSeason(ctx context.Context, pool *pgxpool.Pool, sport string, season int) ([]target, error) {
	qctx, cancel := context.WithTimeout(ctx, 60*time.Second)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT c.et, c.id, c.season FROM (
		    SELECT 'player'::text AS et, player_id AS id, $2::int AS season FROM player_stats
		     WHERE sport = $1 AND season = $2 AND rating_composite_score IS NOT NULL
		     GROUP BY player_id
		    UNION ALL
		    SELECT 'team'::text, team_id, $2::int FROM team_stats
		     WHERE sport = $1 AND season = $2 AND rating_composite_score IS NOT NULL
		     GROUP BY team_id
		) c
		ORDER BY EXISTS (
		    SELECT 1 FROM stat_summaries s
		     WHERE s.entity_type = c.et AND s.entity_id = c.id AND s.sport = $1 AND s.season = c.season
		) ASC, c.et, c.id`, sport, season)
	if err != nil {
		return nil, err
	}
	return scanTargets(rows, sport)
}

func scanTargets(rows interface {
	Next() bool
	Scan(...any) error
	Close()
	Err() error
}, sport string) ([]target, error) {
	defer rows.Close()
	var out []target
	for rows.Next() {
		var t target
		if err := rows.Scan(&t.entityType, &t.entityID, &t.season); err != nil {
			return nil, err
		}
		t.sportName = sport
		out = append(out, t)
	}
	return out, rows.Err()
}

// ---------------------------------------------------------------------------
// Re-stamp mode (one-time vocabulary migration — no Gemma)
// ---------------------------------------------------------------------------

// runReStamp migrates every commentary row's input-component keys from the legacy
// "sigil_label"/"sigil_score" to "peak_label"/"peak_score" and recomputes input_hash,
// WITHOUT a Gemma call (see RatingGenerator.ReStampPeakKeys). Run it once, right after
// deploying the renamed binary and BEFORE the nightly statcommentary tick, so the
// de-sigiled keys don't spuriously re-generate the corpus.
//
//	go run ./cmd/statcommentary -mode restamp            # all sports
//	go run ./cmd/statcommentary -mode restamp -sport NBA # one sport
func runReStamp(pool *pgxpool.Pool, gen *ml.RatingGenerator, sportArg string, logger *slog.Logger) {
	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if s := strings.ToLower(strings.TrimSpace(sportArg)); s != "" && s != "all" {
		sports = []string{strings.ToUpper(sportArg)}
	}
	ctx := context.Background()
	start := time.Now()

	var targets []target
	for _, sp := range sports {
		ts, err := enumSummarized(ctx, pool, sp)
		if err != nil {
			logger.Error("statcommentary restamp: enumerate failed", "sport", sp, "error", err)
			continue
		}
		targets = append(targets, ts...)
	}
	logger.Info("statcommentary restamp: starting", "targets", len(targets))

	rewritten, noop, fail := 0, 0, 0
	for i, t := range targets {
		rctx, rcancel := context.WithTimeout(ctx, 10*time.Second)
		ok, err := gen.ReStampPeakKeys(rctx, t.entityType, t.entityID, t.sportName, t.season)
		rcancel()
		switch {
		case err != nil:
			fail++
			logger.Warn("statcommentary restamp: failed", "sport", t.sportName, "type", t.entityType, "id", t.entityID, "season", t.season, "error", err)
		case ok:
			rewritten++
		default:
			noop++
		}
		if (i+1)%100 == 0 {
			logger.Info("statcommentary restamp: progress", "done", i+1, "total", len(targets),
				"rewritten", rewritten, "noop", noop, "fail", fail)
		}
	}
	logger.Info("statcommentary restamp: complete",
		"rewritten", rewritten, "noop", noop, "fail", fail,
		"elapsed", time.Since(start).Round(time.Second))
}

// enumSummarized returns every (entity, season) with a real commentary row — the
// corpus the re-stamp walks (the latest row per entity-season is rewritten inside
// ReStampPeakKeys, matching what the nightly gate reads).
func enumSummarized(ctx context.Context, pool *pgxpool.Pool, sport string) ([]target, error) {
	qctx, cancel := context.WithTimeout(ctx, 60*time.Second)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT DISTINCT entity_type, entity_id, season FROM stat_summaries
		WHERE sport = $1 AND body IS NOT NULL
		ORDER BY entity_type, entity_id`, sport)
	if err != nil {
		return nil, err
	}
	return scanTargets(rows, sport)
}

func seasonPtr(s int) *int {
	if s <= 0 {
		return nil
	}
	return &s
}
