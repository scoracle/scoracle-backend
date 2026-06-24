package ml

// vibe_parity_test.go — the Go side of the Rust scrubber Phase 1 vibe parity proof.
//
// Gated on VIBE_PARITY_DB so it never runs in the normal `go test ./...` sweep (it needs
// a live DB + Ollama). For each entity in VIBE_PARITY_ENTITIES it loads the SAME
// narratives + heat through the package's own unexported loaders, builds the SAME prompt
// via buildSentimentPrompt, calls Ollama at an EXPLICIT temperature 0 (the normal client
// drops temp<=0, which would un-pin determinism), parses with parseSentimentAndPrompt,
// and writes a source='go' row to vibe_scores_shadow. The Rust harness (rust/src/bin/
// parity.rs) writes source='rust' rows the same way; the proof is the SQL diff between
// them. This test writes ONLY the shadow table — never vibe_scores, never pipeline_work.

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

type paritySpec struct {
	entityType string
	entityID   int
	sport      string
}

func TestVibeParityDump(t *testing.T) {
	if os.Getenv("VIBE_PARITY_DB") == "" {
		t.Skip("set VIBE_PARITY_DB=1 (with DATABASE_* + OLLAMA_* env + VIBE_PARITY_ENTITIES) to run the parity dump")
	}
	dbURL := parityFirstNonEmpty(os.Getenv("DATABASE_PRIVATE_URL"), os.Getenv("DATABASE_URL"))
	if dbURL == "" {
		t.Fatal("DATABASE_PRIVATE_URL or DATABASE_URL must be set")
	}
	baseURL := parityFirstNonEmpty(os.Getenv("OLLAMA_BASE_URL"), "http://localhost:11434")
	model := parityFirstNonEmpty(os.Getenv("OLLAMA_MODEL"), "gemma4:e4b")
	specs := paritySplitSpecs(os.Getenv("VIBE_PARITY_ENTITIES"))
	if len(specs) == 0 {
		t.Fatal("VIBE_PARITY_ENTITIES must be set (comma- or space-separated entity_type:id:sport)")
	}

	ctx := context.Background()
	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatalf("db connect: %v", err)
	}
	defer pool.Close()

	oc := NewOllamaClient(baseURL, model, 300*time.Second)
	g := &VibeGenerator{pool: pool, ollama: oc}

	for _, s := range specs {
		sportUp := strings.ToUpper(s.sport)
		name, err := parityLookupName(ctx, pool, s.entityType, s.entityID, sportUp)
		if err != nil {
			t.Errorf("%s/%d (%s): name lookup: %v", s.entityType, s.entityID, s.sport, err)
			continue
		}

		// Reads use the upper-cased sport, exactly like VibeGenerator.Generate.
		narratives, newsIDs, err := g.loadLatestNarratives(ctx, s.entityType, s.entityID, sportUp)
		if err != nil {
			t.Errorf("%s/%d: load narratives: %v", s.entityType, s.entityID, err)
			continue
		}
		heat, err := loadTransferHeat(ctx, pool, s.entityType, s.entityID, sportUp)
		if err != nil {
			t.Errorf("%s/%d: load heat: %v", s.entityType, s.entityID, err)
			continue
		}

		// No-corpus → NULL marker (no model call), mirroring persistNoCorpus.
		if len(narratives) == 0 && len(heat) == 0 {
			if err := parityInsertShadow(ctx, pool, s.entityType, s.entityID, sportUp,
				nil, "", nil, model, vibePromptVersion, "", nil); err != nil {
				t.Errorf("%s/%d: insert marker: %v", s.entityType, s.entityID, err)
				continue
			}
			t.Logf("%s/%d (%s): no-corpus NULL marker", s.entityType, s.entityID, s.sport)
			continue
		}

		req := VibeRequest{
			EntityType: s.entityType, EntityID: s.entityID, EntityName: name,
			Sport: sportUp, TriggerType: "periodic",
		}
		prompt := buildSentimentPrompt(req, narratives, heat)
		reqBody := parityCanonicalRequest(model, prompt, vibeSystemPrompt, 1200, 0)

		respModel, response, err := parityOllamaTemp0(ctx, baseURL, reqBody)
		if err != nil {
			t.Errorf("%s/%d: generate: %v", s.entityType, s.entityID, err)
			continue
		}
		score, vibePrompt, err := parseSentimentAndPrompt(response)
		if err != nil {
			t.Errorf("%s/%d: parse (raw=%q): %v", s.entityType, s.entityID, truncate(response, 120), err)
			continue
		}
		if respModel == "" {
			respModel = model
		}
		if err := parityInsertShadow(ctx, pool, s.entityType, s.entityID, sportUp,
			&score, vibePrompt, newsIDs, respModel, vibePromptVersion, prompt, reqBody); err != nil {
			t.Errorf("%s/%d: insert: %v", s.entityType, s.entityID, err)
			continue
		}
		t.Logf("%s/%d (%s): SCORE %d | VIBE %q", s.entityType, s.entityID, s.sport, score, vibePrompt)
	}
}

// parityCanonicalRequest builds the exact /api/generate body POSTed at temp 0. It carries
// the SAME fields the Rust client serializes (model, prompt, system, stream, options);
// stored as jsonb, key order and 0-vs-0.0 are normalized away, so a jsonb-equal body
// across Go and Rust proves prompt + options parity.
func parityCanonicalRequest(model, prompt, system string, numPredict, temperature int) []byte {
	body, _ := json.Marshal(map[string]any{
		"model":  model,
		"prompt": prompt,
		"system": system,
		"stream": false,
		"options": map[string]any{
			"num_predict": numPredict,
			"temperature": temperature,
		},
	})
	return body
}

// parityOllamaTemp0 POSTs the prebuilt body (which pins temperature 0) and returns the
// response model + text. A direct call on purpose: ml.OllamaClient.Generate omits
// temperature<=0, which would fall back to Ollama's non-deterministic default.
func parityOllamaTemp0(ctx context.Context, baseURL string, body []byte) (model, response string, err error) {
	httpReq, err := http.NewRequestWithContext(ctx, "POST", baseURL+"/api/generate", bytes.NewReader(body))
	if err != nil {
		return "", "", err
	}
	httpReq.Header.Set("Content-Type", "application/json")
	resp, err := (&http.Client{Timeout: 300 * time.Second}).Do(httpReq)
	if err != nil {
		return "", "", err
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", "", err
	}
	if resp.StatusCode != http.StatusOK {
		return "", "", fmt.Errorf("ollama HTTP %d: %s", resp.StatusCode, truncate(string(raw), 300))
	}
	var parsed struct {
		Model    string `json:"model"`
		Response string `json:"response"`
		Error    string `json:"error"`
	}
	if err := json.Unmarshal(raw, &parsed); err != nil {
		return "", "", err
	}
	if parsed.Error != "" {
		return "", "", fmt.Errorf("ollama error: %s", parsed.Error)
	}
	return parsed.Model, parsed.Response, nil
}

// parityInsertShadow writes one source='go' row, mirroring the vibe_scores persist
// (trigger 'periodic', trigger_payload JSON null, empty felt-read → NULL, nil ids → '{}').
func parityInsertShadow(
	ctx context.Context, pool *pgxpool.Pool,
	entityType string, entityID int, sport string,
	score *int, vibePrompt string, newsIDs []int64,
	model, promptVersion, builtPrompt string, reqBody []byte,
) error {
	if newsIDs == nil {
		newsIDs = []int64{}
	}
	var scoreArg *int16
	if score != nil {
		v := int16(*score)
		scoreArg = &v
	}
	var promptArg any
	if vibePrompt != "" {
		promptArg = vibePrompt
	}
	var builtArg any
	if builtPrompt != "" {
		builtArg = builtPrompt
	}
	var reqArg any
	if reqBody != nil {
		reqArg = string(reqBody)
	}
	_, err := pool.Exec(ctx, `
		INSERT INTO vibe_scores_shadow (
		    source, entity_type, entity_id, sport,
		    trigger_type, trigger_payload,
		    sentiment, prompt, input_news_ids,
		    model_version, prompt_version,
		    temperature, built_prompt, ollama_request
		) VALUES ('go',$1,$2,$3,'periodic','null'::jsonb,$4,$5,$6,$7,$8,$9,$10,$11::jsonb)
	`,
		entityType, entityID, sport,
		scoreArg, promptArg, newsIDs,
		model, promptVersion,
		float32(0), builtArg, reqArg,
	)
	return err
}

func parityLookupName(ctx context.Context, pool *pgxpool.Pool, entityType string, id int, sport string) (string, error) {
	q := `SELECT name FROM teams WHERE id = $1 AND sport = $2`
	if entityType == "player" {
		q = `SELECT name FROM players WHERE id = $1 AND sport = $2`
	}
	var name string
	if err := pool.QueryRow(ctx, q, id, sport).Scan(&name); err != nil {
		return "", err
	}
	if name == "" {
		return "", fmt.Errorf("empty name for %s/%d (%s)", entityType, id, sport)
	}
	return name, nil
}

func parityFirstNonEmpty(vals ...string) string {
	for _, v := range vals {
		if v != "" {
			return v
		}
	}
	return ""
}

// paritySplitSpecs parses comma- or whitespace-separated entity_type:id:sport tokens.
func paritySplitSpecs(raw string) []paritySpec {
	fields := strings.FieldsFunc(raw, func(r rune) bool { return r == ',' || r == ' ' || r == '\n' || r == '\t' })
	var out []paritySpec
	for _, f := range fields {
		parts := strings.Split(f, ":")
		if len(parts) != 3 {
			continue
		}
		id, err := strconv.Atoi(parts[1])
		if err != nil {
			continue
		}
		out = append(out, paritySpec{
			entityType: strings.ToLower(parts[0]),
			entityID:   id,
			sport:      strings.ToUpper(parts[2]),
		})
	}
	return out
}
