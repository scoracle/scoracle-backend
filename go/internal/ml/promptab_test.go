package ml

// promptab_test.go — A/B prompt harness (gated on PROMPT_AB).
//
// The all-stages Go analog of rust/src/bin/eval.rs: it builds the EXACT production
// prompt for a stage+entity through the package's own unexported loaders + builders,
// then runs the INCUMBENT system prompt and an optional CANDIDATE system prompt (from
// a file) against live Ollama, printing both outputs + token counts side by side. The
// evidence engine for prompt tuning — a prompt change is adopted on a read of real
// output, never on faith.
//
// Read-only on the pipeline: it NEVER persists a product row, NEVER touches
// pipeline_work. It only reads the corpus/rating tables and POSTs to Ollama.
//
// Usage (env from the repo .env.local — export DATABASE_PRIVATE_URL + OLLAMA_*):
//
//	PROMPT_AB=1 AB_STAGE=vibe AB_ENTITY=team:8:FOOTBALL \
//	  AB_CANDIDATE=/path/to/candidate_system.txt \
//	  go test ./internal/ml -run TestPromptAB -v -timeout 900s
//
// AB_STAGE ∈ vibe | narratives | rating | sigil | transfer. AB_ENTITY is one or more
// (comma/space separated) type:id:sport tokens (transfer wants team:id:sport). Omit
// AB_CANDIDATE to run incumbent-only (recon). AB_TRANSFER_N caps transfer pairs (3).
import (
	"context"
	"fmt"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// Production NumPredict caps per stage (mirrors each Generate call site) so the A/B
// runs under the same token budget the worker uses.
const (
	abVibeNumPredict     = 1200
	abNarrNumPredict     = 4000
	abRatingNumPredict   = 2000
	abSigilNumPredict    = 1000
	abTransferNumPredict = 1200
)

// abCase is one prompt to run (most stages emit a single case per entity; transfer
// emits one per candidate pair, all sharing the stage's system prompt + options).
type abCase struct {
	label string
	user  string
}

func TestPromptAB(t *testing.T) {
	if os.Getenv("PROMPT_AB") == "" {
		t.Skip("set PROMPT_AB=1 AB_STAGE=<vibe|narratives|rating|sigil|transfer> AB_ENTITY=type:id:sport [AB_CANDIDATE=file] to run")
	}
	dbURL := parityFirstNonEmpty(os.Getenv("DATABASE_PRIVATE_URL"), os.Getenv("DATABASE_URL"))
	if dbURL == "" {
		t.Fatal("DATABASE_PRIVATE_URL or DATABASE_URL must be set")
	}
	baseURL := parityFirstNonEmpty(os.Getenv("OLLAMA_BASE_URL"), "http://localhost:11434")
	model := parityFirstNonEmpty(os.Getenv("OLLAMA_MODEL"), "mistral:7b")
	stage := strings.ToLower(strings.TrimSpace(os.Getenv("AB_STAGE")))
	specs := paritySplitSpecs(os.Getenv("AB_ENTITY"))
	if stage == "" || len(specs) == 0 {
		t.Fatal("AB_STAGE and AB_ENTITY (type:id:sport) are required")
	}

	// Optional candidate system prompt (the draft under test). Absent ⇒ incumbent-only.
	var candSys string
	if p := strings.TrimSpace(os.Getenv("AB_CANDIDATE")); p != "" {
		b, err := os.ReadFile(p)
		if err != nil {
			t.Fatalf("read AB_CANDIDATE %q: %v", p, err)
		}
		candSys = string(b)
	}
	nTransfer := 3
	if v := os.Getenv("AB_TRANSFER_N"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			nTransfer = n
		}
	}

	ctx := context.Background()
	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatalf("db connect: %v", err)
	}
	defer pool.Close()
	oc := NewOllamaClient(baseURL, model, 900*time.Second)

	for _, s := range specs {
		sp := strings.ToUpper(s.sport)
		name, err := parityLookupName(ctx, pool, s.entityType, s.entityID, sp)
		if err != nil {
			t.Errorf("name lookup %s/%d (%s): %v", s.entityType, s.entityID, sp, err)
			continue
		}

		var (
			sys        string
			temp       float64
			numPredict int
			jsonMode   bool
			cases      []abCase
		)

		switch stage {
		case "vibe":
			g := &VibeGenerator{pool: pool, ollama: oc}
			narr, _, err := g.loadLatestNarratives(ctx, s.entityType, s.entityID, sp)
			if err != nil {
				t.Errorf("%s: load narratives: %v", name, err)
				continue
			}
			heat, err := loadTransferHeat(ctx, pool, s.entityType, s.entityID, sp)
			if err != nil {
				t.Errorf("%s: load heat: %v", name, err)
				continue
			}
			if len(narr) == 0 && len(heat) == 0 {
				t.Logf("%s: no corpus (vibe would write a marker)", name)
				continue
			}
			req := VibeRequest{EntityType: s.entityType, EntityID: s.entityID, EntityName: name, Sport: sp, TriggerType: "periodic"}
			cases = append(cases, abCase{label: name, user: buildSentimentPrompt(req, narr, heat)})
			sys, temp, numPredict = vibeSystemPrompt, 0.7, abVibeNumPredict

		case "narratives":
			a := &NewsNarrator{pool: pool, ollama: oc}
			news, err := a.loadVettedCorpus(ctx, s.entityType, s.entityID, sp)
			if err != nil {
				t.Errorf("%s: load corpus: %v", name, err)
				continue
			}
			if len(news) == 0 {
				t.Logf("%s: no corpus", name)
				continue
			}
			heat, _ := loadTransferHeat(ctx, pool, s.entityType, s.entityID, sp)
			req := NarrativesRequest{EntityType: s.entityType, EntityID: s.entityID, EntityName: name, Sport: sp, TriggerType: "periodic"}
			cases = append(cases, abCase{label: name, user: buildNarrativesPrompt(req, news, heat)})
			sys, temp, numPredict = newsNarrativesSystemPrompt, 0.6, abNarrNumPredict

		case "rating":
			profile, err := loadRatingProfile(ctx, pool, s.entityType, s.entityID, sp, nil)
			if err != nil {
				t.Errorf("%s: load profile: %v", name, err)
				continue
			}
			if profile == nil || (profile.compositeScore == nil && len(profile.breakdown) == 0) {
				t.Logf("%s: no usable rating profile", name)
				continue
			}
			notability, _ := computeNotability(profile)
			req := RatingRequest{EntityType: s.entityType, EntityID: s.entityID, EntityName: name, Sport: sp, TriggerType: "periodic"}
			cases = append(cases, abCase{label: fmt.Sprintf("%s (notability %d)", name, notability), user: buildStatPrompt(req, profile, notability)})
			sys, temp, numPredict = ratingSystemPrompt, 0.6, abRatingNumPredict

		case "sigil":
			sg := &SigilGenerator{pool: pool, ollama: oc}
			season, err := sg.resolveSeason(ctx, sp, nil)
			if err != nil {
				t.Errorf("%s: resolve season: %v", name, err)
				continue
			}
			narr, err := sg.loadNarrativePillar(ctx, s.entityType, s.entityID, sp)
			if err != nil {
				t.Errorf("%s: narrative pillar: %v", name, err)
				continue
			}
			rating, err := sg.loadRatingPillar(ctx, s.entityType, s.entityID, sp, &season)
			if err != nil {
				t.Errorf("%s: rating pillar: %v", name, err)
				continue
			}
			mom, err := sg.loadMomentumPillar(ctx, s.entityType, s.entityID, sp, &season)
			if err != nil {
				t.Errorf("%s: momentum pillar: %v", name, err)
				continue
			}
			if len(narr) == 0 && rating == nil && mom.empty() {
				t.Logf("%s: no pillars", name)
				continue
			}
			req := SigilRequest{EntityType: s.entityType, EntityID: s.entityID, EntityName: name, Sport: sp, TriggerType: "periodic", Season: &season}
			cases = append(cases, abCase{label: name, user: buildSynthesisPrompt(req, narr, rating, mom)})
			sys, temp, numPredict = sigilSystemPrompt, 0.6, abSigilNumPredict

		case "transfer":
			if s.entityType != "team" {
				t.Errorf("transfer stage needs a team entity, got %s", s.entityType)
				continue
			}
			g := &TransferGenerator{pool: pool, ollama: oc}
			tiers, err := g.loadTierMap(ctx)
			if err != nil {
				t.Errorf("%s: load tiers: %v", name, err)
				continue
			}
			cands, err := g.loadCandidates(ctx, s.entityID, sp, transferDefaultMinArticles)
			if err != nil {
				t.Errorf("%s: load candidates: %v", name, err)
				continue
			}
			for i, c := range cands {
				if i >= nTransfer {
					break
				}
				var heat *int
				var comp string
				var nids []int64
				if err := pool.QueryRow(ctx,
					`SELECT heat, components, news_ids FROM compute_transfer_heat($1,$2,$3)`,
					s.entityID, c.playerID, sp).Scan(&heat, &comp, &nids); err != nil {
					t.Errorf("%s ⟵ %s: heat: %v", name, c.playerName, err)
					continue
				}
				if heat == nil {
					continue // no corpus for this pair
				}
				news, err := g.loadPairNews(ctx, nids)
				if err != nil {
					t.Errorf("%s ⟵ %s: pair news: %v", name, c.playerName, err)
					continue
				}
				rel, err := g.teamRelationship(ctx, s.entityID, c.playerID, sp)
				if err != nil {
					t.Errorf("%s ⟵ %s: relationship: %v", name, c.playerName, err)
					continue
				}
				_ = tiers // bestSource attribution is deterministic + not part of the prompt; loaded to mirror analyzePair
				cases = append(cases, abCase{
					label: fmt.Sprintf("%s ⟵ %s [%s, heat %d]", name, c.playerName, rel, *heat),
					user:  buildTransferPrompt(name, c, sp, rel, news),
				})
			}
			sys, temp, numPredict, jsonMode = transferSystemPrompt(sp), 0.3, abTransferNumPredict, true

		default:
			t.Fatalf("unknown AB_STAGE %q", stage)
		}

		for _, c := range cases {
			fmt.Printf("\n==================== %s :: %s ====================\n", stage, c.label)
			fmt.Printf("\n----------------- USER PROMPT -----------------\n%s\n", c.user)
			abRun(ctx, oc, "INCUMBENT", sys, c.user, temp, numPredict, jsonMode)
			if candSys != "" {
				abRun(ctx, oc, "CANDIDATE", candSys, c.user, temp, numPredict, jsonMode)
			}
		}
	}
}

// abRun executes one (system, user) prompt and prints the trimmed response with the
// token count + wall time — the side-by-side fodder for judging incumbent vs candidate.
func abRun(ctx context.Context, oc *OllamaClient, label, sys, user string, temp float64, numPredict int, jsonMode bool) {
	start := time.Now()
	gen, err := oc.Generate(ctx, user, GenerateOptions{
		System:      sys,
		Temperature: temp,
		NumPredict:  numPredict,
		JSONMode:    jsonMode,
		Op:          "ab",
	})
	if err != nil {
		fmt.Printf("\n----- [%s] ERROR: %v -----\n", label, err)
		return
	}
	fmt.Printf("\n----- [%s] %d tok / %s -----\n%s\n",
		label, gen.EvalCount, time.Since(start).Round(10*time.Millisecond), strings.TrimSpace(gen.Response))
}
