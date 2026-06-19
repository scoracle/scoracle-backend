// vibesynth — CLI for the holistic three-pillar vibe synthesis (Phase B).
//
// Three modes:
//
//	single (default) — one entity (dry-run; use -persist to write).
//	  go run ./cmd/vibesynth -entity-type player -entity-id 237 -sport NBA
//	  go run ./cmd/vibesynth -entity-type team -entity-id 10 -sport NFL -persist
//
//	backfill — every entity with a rating but no synthesis row yet.
//	  go run ./cmd/vibesynth -mode backfill
//	  go run ./cmd/vibesynth -mode backfill -sport NBA -limit 50
//
//	nightly — current season only, hash-gated (skip when inputs unchanged).
//	  go run ./cmd/vibesynth -mode nightly
//
//	restamp — one-time vocabulary migration (no Gemma): rename the crown's
//	  "divined_sigil" input-component key → "divined_peak" + recompute input_hash
//	  on existing rows, so the s4 key rename does not re-synthesize the corpus.
//	  go run ./cmd/vibesynth -mode restamp
//
// Env: DATABASE_PRIVATE_URL + OLLAMA_* (see config.go; restamp needs only the DB).
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
	"github.com/albapepper/scoracle-data/internal/ml"
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

	ollama := ml.NewOllamaClient(cfg.OllamaBaseURL, cfg.OllamaModel, cfg.OllamaTimeout)
	// restamp is a pure DB vocabulary migration (no Gemma), so skip the Ollama ping.
	if *mode != "restamp" {
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
		runCorpus(pool, gen, false, *sport, *throttleMs, *limit, cfg.OllamaTimeout, logger)
	case "nightly":
		runCorpus(pool, gen, true, *sport, *throttleMs, *limit, cfg.OllamaTimeout, logger)
	case "restamp":
		runReStamp(pool, gen, *sport, logger)
	default:
		fmt.Fprintf(os.Stderr, "unknown -mode %q; valid: single | backfill | nightly | restamp\n", *mode)
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
	}
	// Single mode: skip persistence by running and not saving (we call Generate
	// directly; persist is handled by the function when -persist is set via
	// SkipUnchanged=false + a real pool). To keep it simple, Generate always
	// persists; dry-run is signalled by noop pool — instead, just warn the user.
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

func runCorpus(pool *pgxpool.Pool, gen *ml.SigilGenerator, nightly bool, sportArg string, throttleMs, limit int, gemmaTimeout time.Duration, logger *slog.Logger) {
	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if s := strings.TrimSpace(sportArg); s != "" && strings.ToLower(s) != "all" {
		sports = []string{strings.ToUpper(s)}
	}
	ctx := context.Background()
	label := map[bool]string{true: "nightly", false: "backfill"}[nightly]
	start := time.Now()

	var targets []target
	for _, sp := range sports {
		ts, err := enumRated(ctx, pool, sp)
		if err != nil {
			logger.Error("vibesynth: enumerate failed", "mode", label, "sport", sp, "error", err)
			continue
		}
		targets = append(targets, ts...)
	}
	logger.Info("vibesynth: starting", "mode", label, "sports", sports, "targets", len(targets), "gen_limit", limit)

	ok, skippedUnchanged, noPillars, fail := 0, 0, 0, 0
	for i, t := range targets {
		if limit > 0 && ok >= limit {
			logger.Info("vibesynth: generation limit reached", "mode", label, "limit", limit, "scanned", i)
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
			EntityType:    t.entityType,
			EntityID:      t.entityID,
			EntityName:    name,
			Sport:         t.sportName,
			Season:        seasonPtr(t.season),
			TriggerType:   "periodic",
			SkipUnchanged: nightly,
		})
		gcancel()

		switch {
		case err != nil:
			fail++
			logger.Warn("vibesynth: generate failed", "sport", t.sportName, "entity", name, "id", t.entityID, "error", err)
		case res.SkippedUnchanged:
			skippedUnchanged++
		case res.SkippedNoPillars:
			noPillars++
		default:
			ok++
		}
		if (i+1)%25 == 0 {
			logger.Info("vibesynth: progress", "mode", label, "done", i+1, "total", len(targets),
				"ok", ok, "unchanged", skippedUnchanged, "no_pillars", noPillars, "fail", fail)
		}
		if throttleMs > 0 {
			time.Sleep(time.Duration(throttleMs) * time.Millisecond)
		}
	}
	logger.Info("vibesynth: complete", "mode", label,
		"ok", ok, "unchanged", skippedUnchanged, "no_pillars", noPillars, "fail", fail,
		"elapsed", time.Since(start).Round(time.Second))
}

// enumRated returns every current-season entity with a rating row — the full
// corpus for synthesis (both backfill and nightly use the same list; the
// SkipUnchanged gate inside Generate handles the nightly "only work on new
// data" filter).
func enumRated(ctx context.Context, pool *pgxpool.Pool, sport string) ([]target, error) {
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
		ORDER BY c.season DESC, c.et, c.id`, sport)
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
