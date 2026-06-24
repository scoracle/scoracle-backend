package ml

// sigil_parity_test.go — the Go side of the Rust Cognition Harness L3 sigil parity proof.
//
// Gated on SIGIL_PARITY_DB so it never runs in the normal `go test ./...` sweep (it needs a
// live DB + Ollama). For each entity in SIGIL_PARITY_ENTITIES it resolves the sport's
// current_season, loads the SAME three pillars through the package's own unexported loaders,
// builds the SAME prompt via buildSynthesisPrompt, computes the SAME input-components JSON +
// hash via buildSynthesisInputComponents/hashComponents, calls Ollama at an EXPLICIT
// temperature 0 (the normal client drops temp<=0, which would un-pin determinism), parses with
// parseSynthesisResponse, and writes a source='go' row to sigil_synthesis_shadow. The Rust
// harness (rust/src/bin/sigil_parity.rs) writes source='rust' rows the same way; the proof is
// the SQL diff between them. This test writes ONLY the shadow table — never sigil_synthesis,
// never pipeline_work.
//
// It reuses the shared parity helpers from vibe_parity_test.go (same package):
// parityCanonicalRequest, parityOllamaTemp0, parityLookupName, parityFirstNonEmpty,
// paritySplitSpecs.

import (
	"context"
	"encoding/json"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

func TestSigilParityDump(t *testing.T) {
	if os.Getenv("SIGIL_PARITY_DB") == "" {
		t.Skip("set SIGIL_PARITY_DB=1 (with DATABASE_* + OLLAMA_* env + SIGIL_PARITY_ENTITIES) to run the parity dump")
	}
	dbURL := parityFirstNonEmpty(os.Getenv("DATABASE_PRIVATE_URL"), os.Getenv("DATABASE_URL"))
	if dbURL == "" {
		t.Fatal("DATABASE_PRIVATE_URL or DATABASE_URL must be set")
	}
	baseURL := parityFirstNonEmpty(os.Getenv("OLLAMA_BASE_URL"), "http://localhost:11434")
	model := parityFirstNonEmpty(os.Getenv("OLLAMA_MODEL"), "gemma4:e4b")
	specs := paritySplitSpecs(os.Getenv("SIGIL_PARITY_ENTITIES"))
	if len(specs) == 0 {
		t.Fatal("SIGIL_PARITY_ENTITIES must be set (comma- or space-separated entity_type:id:sport)")
	}

	ctx := context.Background()
	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatalf("db connect: %v", err)
	}
	defer pool.Close()

	oc := NewOllamaClient(baseURL, model, 300*time.Second)
	g := &SigilGenerator{pool: pool, ollama: oc}

	for _, s := range specs {
		sportUp := strings.ToUpper(s.sport)
		name, err := parityLookupName(ctx, pool, s.entityType, s.entityID, sportUp)
		if err != nil {
			t.Errorf("%s/%d (%s): name lookup: %v", s.entityType, s.entityID, s.sport, err)
			continue
		}

		// Resolve + stamp the season exactly like Generate (Season nil → current_season), then
		// load the three pillars season-exact.
		season, err := g.resolveSeason(ctx, sportUp, nil)
		if err != nil {
			t.Errorf("%s/%d: resolve season: %v", s.entityType, s.entityID, err)
			continue
		}
		narratives, err := g.loadNarrativePillar(ctx, s.entityType, s.entityID, sportUp)
		if err != nil {
			t.Errorf("%s/%d: narrative pillar: %v", s.entityType, s.entityID, err)
			continue
		}
		rating, err := g.loadRatingPillar(ctx, s.entityType, s.entityID, sportUp, &season)
		if err != nil {
			t.Errorf("%s/%d: rating pillar: %v", s.entityType, s.entityID, err)
			continue
		}
		mom, err := g.loadMomentumPillar(ctx, s.entityType, s.entityID, sportUp, &season)
		if err != nil {
			t.Errorf("%s/%d: momentum pillar: %v", s.entityType, s.entityID, err)
			continue
		}

		// No-pillar path → NULL marker (no model call), mirroring the SkippedNoPillars persist.
		if len(narratives) == 0 && rating == nil && mom.empty() {
			if err := paritySigilInsertShadow(ctx, pool, s.entityType, s.entityID, sportUp, season,
				nil, nil, []byte("{}"), "", model, sigilPromptVersion, "", nil); err != nil {
				t.Errorf("%s/%d: insert marker: %v", s.entityType, s.entityID, err)
				continue
			}
			t.Logf("%s/%d (%s) season %d: no-pillar NULL marker", s.entityType, s.entityID, s.sport, season)
			continue
		}

		// Canonical input components + hash (the pre-image bytes are exactly what hashComponents
		// hashes, so the persisted input_components and input_hash agree, as in the live persist).
		ic := buildSynthesisInputComponents(narratives, rating, mom)
		hash := hashComponents(ic)
		icJSON, err := json.Marshal(orEmptyMap(ic))
		if err != nil {
			t.Errorf("%s/%d: marshal input_components: %v", s.entityType, s.entityID, err)
			continue
		}

		req := SigilRequest{
			EntityType: s.entityType, EntityID: s.entityID, EntityName: name,
			Sport: sportUp, TriggerType: "periodic",
		}
		prompt := buildSynthesisPrompt(req, narratives, rating, mom)
		reqBody := parityCanonicalRequest(model, prompt, sigilSystemPrompt, 1000, 0)

		respModel, response, err := parityOllamaTemp0(ctx, baseURL, reqBody)
		if err != nil {
			t.Errorf("%s/%d: generate: %v", s.entityType, s.entityID, err)
			continue
		}
		score, blurb := parseSynthesisResponse(response)
		if score == 0 {
			t.Errorf("%s/%d: parse: no score in response (raw=%q)", s.entityType, s.entityID, truncate(response, 120))
			continue
		}
		if respModel == "" {
			respModel = model
		}
		if err := paritySigilInsertShadow(ctx, pool, s.entityType, s.entityID, sportUp, season,
			&score, &blurb, icJSON, hash, respModel, sigilPromptVersion, prompt, reqBody); err != nil {
			t.Errorf("%s/%d: insert: %v", s.entityType, s.entityID, err)
			continue
		}
		t.Logf("%s/%d (%s) season %d: SCORE %d | hash %s | BLURB %q", s.entityType, s.entityID, s.sport, season, score, hash, blurb)
	}
}

// paritySigilInsertShadow writes one source='go' row, mirroring the sigil_synthesis persist
// (trigger 'periodic', trigger_payload '{}', empty blurb → "" for a scored row / NULL for a
// marker). score/blurb are nil for the no-pillar marker; built_prompt/reqBody/inputHash empty
// for the marker → NULL. input_components is always the JSON ("{}" for the marker).
func paritySigilInsertShadow(
	ctx context.Context, pool *pgxpool.Pool,
	entityType string, entityID int, sport string, season int,
	score *int, blurb *string, inputComponents []byte, inputHash string,
	model, promptVersion, builtPrompt string, reqBody []byte,
) error {
	var scoreArg *int16
	if score != nil {
		v := int16(*score)
		scoreArg = &v
	}
	var hashArg any
	if inputHash != "" {
		hashArg = inputHash
	}
	var builtArg any
	if builtPrompt != "" {
		builtArg = builtPrompt
	}
	var reqArg any
	if reqBody != nil {
		reqArg = string(reqBody)
	}
	if inputComponents == nil {
		inputComponents = []byte("{}")
	}
	_, err := pool.Exec(ctx, `
		INSERT INTO sigil_synthesis_shadow (
		    source, entity_type, entity_id, sport, season,
		    trigger_type, trigger_payload,
		    score, blurb, input_components, input_hash,
		    model_version, prompt_version,
		    temperature, built_prompt, ollama_request
		) VALUES ('go',$1,$2,$3,$4,'periodic','{}'::jsonb,$5,$6,$7::jsonb,$8,$9,$10,$11,$12,$13::jsonb)
	`,
		entityType, entityID, sport, season,
		scoreArg, blurb, string(inputComponents), hashArg,
		model, promptVersion,
		float32(0), builtArg, reqArg,
	)
	return err
}
