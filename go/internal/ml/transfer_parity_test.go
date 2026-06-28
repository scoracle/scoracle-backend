package ml

// transfer_parity_test.go — the Go side of the Rust Cognition Harness L11 transfers parity proof.
//
// Gated on TRANSFER_PARITY_DB so it never runs in the normal `go test ./...` sweep (it needs a live
// DB). For each team in TRANSFER_PARITY_TEAMS it loads the SAME co-mention candidates through the
// package's own unexported loaders, takes the first TRANSFER_PARITY_MAX_PAIRS by player_id (matching
// the Rust bin's deterministic cap), computes the deterministic heat/relationship, builds the SAME
// user prompt via buildTransferPrompt, and writes a source='go' row to transfer_rumors_shadow with
// the DETERMINISTIC parity axes — built_prompt + the canonical /api/generate body + model_version +
// prompt_version (the frozen t3). The Rust harness (rust/src/bin/transfer_parity.rs) writes
// source='rust' (t4) rows the same way; the proof is the SQL diff between them.
//
// NO MODEL CALL (the L2 finding): the verdict is not a temp-0 parity axis, so the gate is fully
// deterministic and needs no Ollama. The diff expects built_prompt byte-equal and the request equal
// EXCEPT the `system` field — Go ships t3, Rust ships t4 (the roundup clause + never-invent-a-fee,
// the L9 false-heat root fix single-homed in Rust). This test writes ONLY the shadow table — never
// transfer_rumors, never pipeline_work.

import (
	"context"
	"encoding/json"
	"os"
	"sort"
	"strconv"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"
)

func TestTransferParityDump(t *testing.T) {
	if os.Getenv("TRANSFER_PARITY_DB") == "" {
		t.Skip("set TRANSFER_PARITY_DB=1 (with DATABASE_* + TRANSFER_PARITY_TEAMS) to run the transfers parity dump")
	}
	dbURL := parityFirstNonEmpty(os.Getenv("DATABASE_PRIVATE_URL"), os.Getenv("DATABASE_URL"))
	if dbURL == "" {
		t.Fatal("DATABASE_PRIVATE_URL or DATABASE_URL must be set")
	}
	model := parityFirstNonEmpty(os.Getenv("OLLAMA_MODEL"), "mistral:7b")
	specs := paritySplitSpecs(os.Getenv("TRANSFER_PARITY_TEAMS"))
	if len(specs) == 0 {
		t.Fatal("TRANSFER_PARITY_TEAMS must be set (comma- or space-separated team:id:sport)")
	}
	maxPairs := 4
	if v, err := strconv.Atoi(os.Getenv("TRANSFER_PARITY_MAX_PAIRS")); err == nil && v > 0 {
		maxPairs = v
	}

	ctx := context.Background()
	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatalf("db connect: %v", err)
	}
	defer pool.Close()
	g := &TransferGenerator{pool: pool}

	written := 0
	for _, s := range specs {
		if s.entityType != "team" {
			t.Errorf("spec %s/%d: transfers is team-keyed", s.entityType, s.entityID)
			continue
		}
		sportUp := strings.ToUpper(s.sport)
		teamName, err := parityLookupName(ctx, pool, "team", s.entityID, sportUp)
		if err != nil {
			t.Errorf("team/%d (%s): name lookup: %v", s.entityID, s.sport, err)
			continue
		}
		candidates, err := g.loadCandidates(ctx, s.entityID, sportUp, transferDefaultMinArticles)
		if err != nil {
			t.Errorf("team/%d: load candidates: %v", s.entityID, err)
			continue
		}
		// Deterministic, Rust-matchable cap: sort by playerID, take the first N.
		sort.Slice(candidates, func(i, j int) bool { return candidates[i].playerID < candidates[j].playerID })
		if len(candidates) > maxPairs {
			candidates = candidates[:maxPairs]
		}
		t.Logf("team/%d %s (%s): %d candidate pair(s)", s.entityID, teamName, s.sport, len(candidates))

		for _, c := range candidates {
			// Deterministic heat (the number stays in Postgres). heat == nil ⇒ no corpus ⇒ skip
			// (no row), mirroring analyzePair.
			var heat *int
			var components string
			var newsIDs []int64
			if err := pool.QueryRow(ctx,
				`SELECT heat, components, news_ids FROM compute_transfer_heat($1,$2,$3)`,
				s.entityID, c.playerID, sportUp,
			).Scan(&heat, &components, &newsIDs); err != nil {
				t.Errorf("team/%d player/%d: heat: %v", s.entityID, c.playerID, err)
				continue
			}
			if heat == nil {
				t.Logf("team/%d player/%d: skipped (no pair corpus)", s.entityID, c.playerID)
				continue
			}
			news, err := g.loadPairNews(ctx, newsIDs)
			if err != nil {
				t.Errorf("team/%d player/%d: load news: %v", s.entityID, c.playerID, err)
				continue
			}
			rel, err := g.teamRelationship(ctx, s.entityID, c.playerID, sportUp)
			if err != nil {
				t.Errorf("team/%d player/%d: relationship: %v", s.entityID, c.playerID, err)
				continue
			}

			prompt := buildTransferPrompt(teamName, c, sportUp, rel, news)
			reqBody := parityTransferRequest(model, prompt, transferSystemPrompt(sportUp))
			if err := parityInsertTransferShadow(ctx, pool, s.entityID, c.playerID, sportUp,
				heat, components, newsIDs, directionFor(rel), model, transferPromptVersion, prompt, reqBody); err != nil {
				t.Errorf("team/%d player/%d: insert: %v", s.entityID, c.playerID, err)
				continue
			}
			written++
		}
	}
	t.Logf("done — %d pair(s) written to transfer_rumors_shadow (source=go)", written)
}

// parityTransferRequest builds the exact /api/generate body the Rust transfer client POSTs at temp
// 0 with JSON mode: {model, prompt, system, stream, format:"json", options:{num_predict, temperature}}.
// Stored as jsonb, so key order and 0-vs-0.0 normalize away — a jsonb-equal body (minus the
// deliberately-diverged `system` field) proves prompt + options parity.
func parityTransferRequest(model, prompt, system string) []byte {
	body, _ := json.Marshal(map[string]any{
		"model":  model,
		"prompt": prompt,
		"system": system,
		"stream": false,
		"format": "json",
		"options": map[string]any{
			"num_predict": 1200,
			"temperature": 0,
		},
	})
	return body
}

// parityInsertTransferShadow writes one source='go' row with the deterministic axes (verdict columns
// left NULL — not a temp-0 parity axis). Mirrors the Rust bin's deterministic dump.
func parityInsertTransferShadow(
	ctx context.Context, pool *pgxpool.Pool,
	teamID, playerID int, sport string,
	heat *int, components string, newsIDs []int64,
	direction, model, promptVersion, builtPrompt string, reqBody []byte,
) error {
	if newsIDs == nil {
		newsIDs = []int64{}
	}
	if components == "" {
		components = "{}"
	}
	var heatArg *int16
	if heat != nil {
		v := int16(*heat)
		heatArg = &v
	}
	_, err := pool.Exec(ctx, `
		INSERT INTO transfer_rumors_shadow (
		    source, team_id, player_id, sport, trigger_type, trigger_payload,
		    heat, heat_components, direction, input_news_ids,
		    model_version, prompt_version, temperature, built_prompt, ollama_request
		) VALUES ('go',$1,$2,$3,'periodic','{}'::jsonb,$4,$5::jsonb,$6,$7,$8,$9,$10,$11,$12::jsonb)
	`,
		teamID, playerID, sport, heatArg, components, direction, newsIDs,
		model, promptVersion, float32(0), builtPrompt, string(reqBody),
	)
	return err
}
