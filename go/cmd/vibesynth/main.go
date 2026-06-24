// vibesynth — CLI for the holistic three-pillar Sigil synthesis (Phase B).
//
// Modes:
//
//	single (default) — one entity. Dry-run by default (NO write); -persist to write.
//	  go run ./cmd/vibesynth -entity-type player -entity-id 237 -sport NBA
//	  go run ./cmd/vibesynth -entity-type team -entity-id 10 -sport NFL -persist
//
//	backfill — directly synthesize every (entity, season) that has a rating but no
//	  Sigil for that season yet (per-season; populates current + historical gaps).
//	  Needs Ollama. go run ./cmd/vibesynth -mode backfill [-sport NBA -limit 50]
//
//	nightly | reconcile — bounded reconciliation (FIRST-GPT-AUDIT Session 12): enumerate
//	  CURRENT-SEASON rated entities whose current-season Sigil is missing or stale (an
//	  input generation is newer than the Sigil) and ENQUEUE durable sigil pipeline_work;
//	  the always-on derive worker drains it (current-season, hash-gated). It NEVER
//	  synthesizes inline and never regenerates an unchanged Sigil because a schedule
//	  fired. Needs only the DB (no Ollama). go run ./cmd/vibesynth -mode nightly [-limit N]
//
//	restamp — one-time vocabulary migration (no Gemma): rename the crown's
//	  "divined_sigil" input-component key → "divined_peak" + recompute input_hash
//	  on existing rows, so the s4 key rename does not re-synthesize the corpus.
//	  go run ./cmd/vibesynth -mode restamp
//
// Env: DATABASE_PRIVATE_URL + OLLAMA_* (see config.go; nightly/reconcile + restamp
// need only the DB).
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
	"github.com/albapepper/scoracle-data/internal/work"
)

func main() {
	mode := flag.String("mode", "single", "single | backfill | nightly | restamp")
	entityType := flag.String("entity-type", "player", "[single] player | team")
	entityID := flag.Int("entity-id", 0, "[single] canonical entity id")
	season := flag.Int("season", 0, "[single] season (0 = latest)")
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL | all")
	trigger := flag.String("trigger", "manual", "[single] trigger_type")
	persist := flag.Bool("persist", false, "[single] persist (default: dry-run via SkipUnchanged=false, non-persist)")
	skipUnchanged := flag.Bool("skip-unchanged", false, "[single] short-circuit (no Gemma) when inputs are unchanged — verifies the debounce gate")
	throttleMs := flag.Int("throttle-ms", 0, "[backfill/nightly] ms pause between entities")
	limit := flag.Int("limit", 0, "[backfill/nightly] cap Gemma generations per run; 0 = unbounded")
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

	ml.SetGemmaConcurrency(cfg.OllamaMaxConcurrent) // shared GPU governor (Session 14)
	ollama := ml.NewOllamaClient(cfg.OllamaBaseURL, cfg.OllamaModel, cfg.OllamaTimeout)
	ollama.SetKeepAlive(cfg.OllamaKeepAlive) // keep the model warm across entities
	// Only the modes that actually call Gemma need Ollama. nightly/reconcile only
	// enqueues durable work and restamp is a pure DB vocab migration — both run DB-only.
	needsOllama := *mode == "single" || *mode == "backfill"
	if needsOllama {
		if err := ollama.Ping(context.Background()); err != nil {
			logger.Error("ollama unreachable", "error", err, "base_url", cfg.OllamaBaseURL)
			os.Exit(1)
		}
	}
	gen := ml.NewSigilGenerator(pool, ollama)

	switch *mode {
	case "single":
		runSingle(pool, gen, *entityType, *entityID, *season, *sport, *trigger, *persist, *skipUnchanged, cfg.OllamaTimeout, logger)
	case "backfill":
		os.Exit(runBackfill(pool, gen, *sport, *throttleMs, *limit, cfg.OllamaTimeout, cfg.DatabaseURL, logger))
	case "nightly", "reconcile":
		os.Exit(runReconcile(pool, *sport, *limit, cfg.DatabaseURL, logger))
	case "restamp":
		runReStamp(pool, gen, *sport, logger)
	default:
		fmt.Fprintf(os.Stderr, "unknown -mode %q; valid: single | backfill | nightly | reconcile | restamp\n", *mode)
		os.Exit(2)
	}
}

func runSingle(pool *pgxpool.Pool, gen *ml.SigilGenerator, entityType string, entityID, season int, sport, trigger string, persist, skipUnchanged bool, timeout time.Duration, logger *slog.Logger) {
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

	req := ml.SigilRequest{
		EntityType:    entityType,
		EntityID:      entityID,
		EntityName:    name,
		Sport:         sportUpper,
		Season:        seasonPtr(season),
		TriggerType:   trigger,
		SkipUnchanged: skipUnchanged,
		DryRun:        !persist, // dry-run loads + scores but never writes (real DryRun)
	}
	if !persist {
		fmt.Println("(dry-run: result shown but NOT written to sigil_synthesis)")
	}

	res, err := gen.Generate(ctx, req)
	if err != nil {
		logger.Error("vibesynth generation failed", "error", err)
		os.Exit(1)
	}

	fmt.Printf("\n=== Vibe synthesis: %s (%s %s %d) ===\n", name, sportUpper, entityType, entityID)
	switch {
	case res.SkippedNoPillars:
		fmt.Println("(no pillar data — null marker)")
	case res.SkippedUnchanged:
		fmt.Println("(unchanged since last synthesis — skipped)")
	default:
		fmt.Printf("\nScore: %d  (prev: %d)\n%s\n", res.Score, res.PreviousScore, res.Blurb)
	}
	fmt.Printf("\n(model=%s prompt=%s hash=%s duration=%s)\n",
		res.Model, res.PromptVersion, res.InputHash, res.Duration.Round(10*time.Millisecond))
}

// ---------------------------------------------------------------------------
// Corpus modes
// ---------------------------------------------------------------------------

type target struct {
	entityType string
	entityID   int
	season     int
	sportName  string
}

// runBackfill directly synthesizes the (entity, season) pairs that have a rating but
// no Sigil for that season yet — filling current-season AND historical gaps. Needs
// Ollama. Generate stamps the season (passed explicitly), so every backfilled row is
// season-correct.
func runBackfill(pool *pgxpool.Pool, gen *ml.SigilGenerator, sportArg string, throttleMs, limit int, gemmaTimeout time.Duration, dbURL string, logger *slog.Logger) int {
	sports := resolveSports(sportArg)
	ctx := context.Background()
	start := time.Now()

	// Overlap guard + durable run record (shared "vibesynth" lock with reconcile, so
	// a manual backfill and the nightly reconcile cannot run at once).
	run, acquired, err := jobrun.Guard(ctx, pool, dbURL, "vibesynth")
	if err != nil {
		logger.Error("vibesynth backfill: run-guard failed", "error", err)
		return 1
	}
	if !acquired {
		logger.Warn("vibesynth backfill: another vibesynth run holds the lock — exiting cleanly")
		return 0
	}
	defer run.Close()

	var targets []target
	enumErrs := 0
	for _, sp := range sports {
		ts, eerr := enumRatedMissing(ctx, pool, sp)
		if eerr != nil {
			enumErrs++
			logger.Error("vibesynth backfill: enumerate failed", "sport", sp, "error", eerr)
			continue
		}
		targets = append(targets, ts...)
	}
	if enumErrs == len(sports) {
		runErr := fmt.Errorf("enumeration failed for all %d sport(s)", len(sports))
		if ferr := run.Finish(ctx, jobrun.StatusFailed, jobrun.Counts{}, runErr); ferr != nil {
			logger.Warn("vibesynth backfill: record run failed", "error", ferr)
		}
		logger.Error("vibesynth backfill: enumeration failed for all sports")
		return 1
	}
	logger.Info("vibesynth backfill: starting", "sports", sports, "targets", len(targets), "gen_limit", limit)

	ok, noPillars, fail := 0, 0, 0
	for i, t := range targets {
		if limit > 0 && ok >= limit {
			logger.Info("vibesynth backfill: generation limit reached", "limit", limit, "scanned", i)
			break
		}
		lctx, lcancel := context.WithTimeout(ctx, 5*time.Second)
		name, err := corpus.LookupEntityName(lctx, pool, t.entityType, t.entityID, t.sportName)
		lcancel()
		if err != nil || name == "" {
			fail++
			continue
		}

		gctx, gcancel := context.WithTimeout(ctx, gemmaTimeout+10*time.Second)
		res, err := gen.Generate(gctx, ml.SigilRequest{
			EntityType:  t.entityType,
			EntityID:    t.entityID,
			EntityName:  name,
			Sport:       t.sportName,
			Season:      seasonPtr(t.season),
			TriggerType: "periodic",
		})
		gcancel()

		switch {
		case err != nil:
			fail++
			logger.Warn("vibesynth backfill: generate failed", "sport", t.sportName, "entity", name, "id", t.entityID, "season", t.season, "error", err)
		case res.SkippedNoPillars:
			noPillars++
		default:
			ok++
		}
		if (i+1)%25 == 0 {
			logger.Info("vibesynth backfill: progress", "done", i+1, "total", len(targets),
				"ok", ok, "no_pillars", noPillars, "fail", fail)
		}
		if throttleMs > 0 {
			time.Sleep(time.Duration(throttleMs) * time.Millisecond)
		}
	}
	status := jobrun.StatusSuccess
	exit := 0
	var runErr error
	switch {
	case fail > 0 && ok == 0 && noPillars == 0:
		status, exit = jobrun.StatusFailed, 1
		runErr = fmt.Errorf("all %d attempted synthesis(es) failed", fail)
	case fail > 0:
		status, exit = jobrun.StatusPartial, 3
		runErr = fmt.Errorf("%d synthesis(es) failed", fail)
	}
	counts := jobrun.Counts{
		Attempted: ok + noPillars + fail,
		Succeeded: ok,
		Skipped:   noPillars,
		Failed:    fail,
	}
	if ferr := run.Finish(ctx, status, counts, runErr); ferr != nil {
		logger.Warn("vibesynth backfill: record run failed", "error", ferr)
	}
	logger.Info("vibesynth backfill: complete", "status", status, "exit", exit,
		"ok", ok, "no_pillars", noPillars, "fail", fail,
		"elapsed", time.Since(start).Round(time.Second))
	return exit
}

// runReconcile is the bounded current-season reconciliation (FIRST-GPT-AUDIT Session 12):
// it enumerates current-season rated entities whose current-season Sigil is missing or
// stale and ENQUEUES a durable sigil pipeline_work item for each — it never synthesizes
// inline. The always-on derive worker drains the queue (resolving current_season and
// hash-gating the Gemma call), so an unchanged Sigil is a cheap skip, never a duplicate
// scheduled generation. DB-only (no Ollama).
func runReconcile(pool *pgxpool.Pool, sportArg string, limit int, dbURL string, logger *slog.Logger) int {
	sports := resolveSports(sportArg)
	ctx := context.Background()
	start := time.Now()

	// Overlap guard + durable run record (shared "vibesynth" lock with backfill).
	run, acquired, err := jobrun.Guard(ctx, pool, dbURL, "vibesynth")
	if err != nil {
		logger.Error("vibesynth reconcile: run-guard failed", "error", err)
		return 1
	}
	if !acquired {
		logger.Warn("vibesynth reconcile: another vibesynth run holds the lock — exiting cleanly")
		return 0
	}
	defer run.Close()

	totalEnq, candidates, enqFail, enumErrs := 0, 0, 0, 0
	for _, sp := range sports {
		cur, cerr := currentSeason(ctx, pool, sp)
		if cerr != nil {
			enumErrs++
			logger.Error("vibesynth reconcile: current_season lookup failed", "sport", sp, "error", cerr)
			continue
		}
		targets, terr := enumStaleSigil(ctx, pool, sp, cur)
		if terr != nil {
			enumErrs++
			logger.Error("vibesynth reconcile: enumerate failed", "sport", sp, "season", cur, "error", terr)
			continue
		}
		candidates += len(targets)
		enq := 0
		for _, t := range targets {
			if limit > 0 && totalEnq >= limit {
				logger.Info("vibesynth reconcile: enqueue limit reached", "limit", limit)
				break
			}
			if eerr := work.Enqueue(ctx, pool, work.Item{
				Stage: work.StageSigil, EntityType: t.entityType, EntityID: t.entityID, Sport: sp,
			}); eerr != nil {
				enqFail++
				logger.Warn("vibesynth reconcile: enqueue failed", "sport", sp, "type", t.entityType, "id", t.entityID, "error", eerr)
				continue
			}
			enq++
			totalEnq++
		}
		logger.Info("vibesynth reconcile: sport done", "sport", sp, "season", cur, "candidates", len(targets), "enqueued", enq)
	}

	status := jobrun.StatusSuccess
	exit := 0
	var runErr error
	switch {
	case enumErrs == len(sports):
		status, exit = jobrun.StatusFailed, 1
		runErr = fmt.Errorf("enumeration failed for all %d sport(s)", len(sports))
	case enqFail > 0:
		status, exit = jobrun.StatusPartial, 3
		runErr = fmt.Errorf("%d sigil enqueue(s) failed", enqFail)
	}
	counts := jobrun.Counts{
		Attempted: candidates,
		Succeeded: totalEnq,
		Failed:    enqFail,
	}
	if ferr := run.Finish(ctx, status, counts, runErr); ferr != nil {
		logger.Warn("vibesynth reconcile: record run failed", "error", ferr)
	}
	logger.Info("vibesynth reconcile: complete", "status", status, "exit", exit,
		"enqueued", totalEnq, "candidates", candidates, "enqueue_fail", enqFail,
		"elapsed", time.Since(start).Round(time.Second))
	return exit
}

// resolveSports expands the -sport flag to the sports to process ("" / "all" ⇒ all three).
func resolveSports(sportArg string) []string {
	if s := strings.TrimSpace(sportArg); s != "" && strings.ToLower(s) != "all" {
		return []string{strings.ToUpper(s)}
	}
	return []string{"NBA", "NFL", "FOOTBALL"}
}

// currentSeason resolves the sport's current_season from public.sports.
func currentSeason(ctx context.Context, pool *pgxpool.Pool, sport string) (int, error) {
	qctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	var s int
	err := pool.QueryRow(qctx, `SELECT current_season FROM public.sports WHERE id = $1`, strings.ToUpper(sport)).Scan(&s)
	return s, err
}

// enumRatedMissing returns every (entity, season) with a rating row but NO Sigil for
// that exact season — the backfill work list (current-season + historical gaps). Once
// a season has a Sigil row it drops off the list, so re-running backfill only fills
// gaps rather than regenerating the whole corpus.
func enumRatedMissing(ctx context.Context, pool *pgxpool.Pool, sport string) ([]target, error) {
	qctx, cancel := context.WithTimeout(ctx, 60*time.Second)
	defer cancel()
	rows, err := pool.Query(qctx, `
		WITH rated AS (
		    SELECT 'player'::text AS et, player_id AS id, season FROM player_stats
		     WHERE sport = $1 AND rating_composite_score IS NOT NULL
		     GROUP BY player_id, season
		    UNION ALL
		    SELECT 'team'::text, team_id, season FROM team_stats
		     WHERE sport = $1 AND rating_composite_score IS NOT NULL
		     GROUP BY team_id, season
		)
		SELECT r.et, r.id, r.season FROM rated r
		WHERE NOT EXISTS (
		    SELECT 1 FROM public.sigil_synthesis s
		    WHERE s.sport = $1 AND s.entity_type = r.et AND s.entity_id = r.id AND s.season = r.season
		)
		ORDER BY r.season DESC, r.et, r.id`, sport)
	if err != nil {
		return nil, err
	}
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

// enumStaleSigil returns the current-season rated entities whose current-season Sigil
// is MISSING (no season-stamped row — includes entities that only have legacy
// NULL-season rows) or STALE (an input generation — stat commentary for this season,
// or news/vibe — is newer than the Sigil). It is the reconciliation candidate list.
// GREATEST() ignores NULL inputs, so an entity with no inputs is never flagged stale.
func enumStaleSigil(ctx context.Context, pool *pgxpool.Pool, sport string, season int) ([]target, error) {
	qctx, cancel := context.WithTimeout(ctx, 60*time.Second)
	defer cancel()
	rows, err := pool.Query(qctx, `
		WITH rated AS (
		    SELECT 'player'::text AS et, player_id AS id FROM player_stats
		     WHERE sport = $1 AND season = $2 AND rating_composite_score IS NOT NULL GROUP BY player_id
		    UNION ALL
		    SELECT 'team'::text, team_id FROM team_stats
		     WHERE sport = $1 AND season = $2 AND rating_composite_score IS NOT NULL GROUP BY team_id
		),
		sig AS (
		    SELECT entity_type AS et, entity_id AS id, max(generated_at) AS g
		    FROM public.sigil_synthesis WHERE sport = $1 AND season = $2 GROUP BY entity_type, entity_id
		),
		st AS (
		    SELECT entity_type AS et, entity_id AS id, max(generated_at) AS g
		    FROM public.stat_summaries WHERE sport = $1 AND season = $2 GROUP BY entity_type, entity_id
		),
		vb AS (
		    SELECT entity_type AS et, entity_id AS id, max(generated_at) AS g
		    FROM public.vibe_scores WHERE sport = $1 GROUP BY entity_type, entity_id
		),
		nw AS (
		    SELECT entity_type AS et, entity_id AS id, max(generated_at) AS g
		    FROM public.news_summaries WHERE sport = $1 GROUP BY entity_type, entity_id
		)
		SELECT r.et, r.id FROM rated r
		LEFT JOIN sig ON sig.et = r.et AND sig.id = r.id
		LEFT JOIN st  ON st.et  = r.et AND st.id  = r.id
		LEFT JOIN vb  ON vb.et  = r.et AND vb.id  = r.id
		LEFT JOIN nw  ON nw.et  = r.et AND nw.id  = r.id
		WHERE sig.g IS NULL
		   OR sig.g < GREATEST(st.g, vb.g, nw.g)
		ORDER BY r.et, r.id`, sport, season)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []target
	for rows.Next() {
		var t target
		if err := rows.Scan(&t.entityType, &t.entityID); err != nil {
			return nil, err
		}
		t.season = season
		t.sportName = sport
		out = append(out, t)
	}
	return out, rows.Err()
}

// ---------------------------------------------------------------------------
// Re-stamp mode (one-time vocabulary migration — no Gemma)
// ---------------------------------------------------------------------------

// runReStamp migrates every scored synthesis row's input-component key from the
// legacy "divined_sigil" to "divined_peak" and recomputes input_hash, WITHOUT a
// Gemma call (see SigilGenerator.ReStampDivinedKey). Run it once, right after the
// 094 rename + API restart and BEFORE the next nightly tick, so the renamed crown
// hash key does not spuriously re-synthesize the existing corpus.
//
//	go run ./cmd/vibesynth -mode restamp            # all sports
//	go run ./cmd/vibesynth -mode restamp -sport NBA # one sport
func runReStamp(pool *pgxpool.Pool, gen *ml.SigilGenerator, sportArg string, logger *slog.Logger) {
	ctx := context.Background()
	start := time.Now()
	targets, err := enumSynthesized(ctx, pool, sportArg)
	if err != nil {
		logger.Error("vibesynth restamp: enumerate failed", "error", err)
		os.Exit(1)
	}
	logger.Info("vibesynth restamp: starting", "targets", len(targets))

	rewritten, noop, fail := 0, 0, 0
	for i, t := range targets {
		rctx, rcancel := context.WithTimeout(ctx, 10*time.Second)
		ok, err := gen.ReStampDivinedKey(rctx, t.entityType, t.entityID, t.sportName)
		rcancel()
		switch {
		case err != nil:
			fail++
			logger.Warn("vibesynth restamp: failed", "sport", t.sportName, "type", t.entityType, "id", t.entityID, "error", err)
		case ok:
			rewritten++
		default:
			noop++
		}
		if (i+1)%50 == 0 {
			logger.Info("vibesynth restamp: progress", "done", i+1, "total", len(targets),
				"rewritten", rewritten, "noop", noop, "fail", fail)
		}
	}
	logger.Info("vibesynth restamp: complete",
		"rewritten", rewritten, "noop", noop, "fail", fail,
		"elapsed", time.Since(start).Round(time.Second))
}

// enumSynthesized returns every entity with a scored synthesis row — the corpus
// the re-stamp walks (one latest row per entity is rewritten inside ReStamp).
func enumSynthesized(ctx context.Context, pool *pgxpool.Pool, sportArg string) ([]target, error) {
	qctx, cancel := context.WithTimeout(ctx, 60*time.Second)
	defer cancel()
	q := `SELECT DISTINCT entity_type, entity_id, sport FROM sigil_synthesis WHERE score IS NOT NULL`
	var args []any
	if s := strings.TrimSpace(sportArg); s != "" && strings.ToLower(s) != "all" {
		q += ` AND sport = $1`
		args = append(args, strings.ToUpper(s))
	}
	q += ` ORDER BY sport, entity_type, entity_id`
	rows, err := pool.Query(qctx, q, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []target
	for rows.Next() {
		var t target
		if err := rows.Scan(&t.entityType, &t.entityID, &t.sportName); err != nil {
			return nil, err
		}
		out = append(out, t)
	}
	return out, rows.Err()
}

func seasonPtr(s int) *int {
	if s <= 0 {
		return nil
	}
	return &s
}
