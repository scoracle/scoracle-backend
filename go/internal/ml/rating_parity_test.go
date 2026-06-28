package ml

// rating_parity_test.go — the Go side of the Rust Cognition Harness L12 rating parity proof.
//
// Gated on RATING_PARITY_DB so it never runs in the normal `go test ./...` sweep (it needs a live
// DB). For each entity in RATING_PARITY_ENTITIES it loads the SAME rating profile through the
// package's own unexported loadRatingProfile (season nil → latest), computes the SAME deterministic
// notability + input-components JSON + input_hash via computeNotability / inputComponents /
// hashComponents, builds the SAME user prompt via buildStatPrompt, and writes a source='go' row to
// stat_summaries_shadow with the DETERMINISTIC parity axes — built_prompt + the canonical
// /api/generate body + model_version + prompt_version (the s6, carried verbatim) + input_hash. The
// Rust harness (rust/src/bin/rating_parity.rs) writes source='rust' rows the same way; the proof is
// the SQL diff between them.
//
// NO MODEL CALL (the L2 finding): the prose (body/divined_peak) is not a temp-0 parity axis, so the
// gate is fully deterministic and needs no Ollama. UNLIKE transfers (t4≠t3 by design), rating is a
// FAITHFUL port — the whole ollama_request is expected jsonb-equal, `system` INCLUDED (s6 verbatim).
// This test writes ONLY the shadow table — never stat_summaries, never pipeline_work.
//
// It reuses the shared parity helpers from vibe_parity_test.go (same package): parityCanonicalRequest,
// parityLookupName, parityFirstNonEmpty, paritySplitSpecs.

import (
	"context"
	"encoding/json"
	"os"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"
)

func TestRatingParityDump(t *testing.T) {
	if os.Getenv("RATING_PARITY_DB") == "" {
		t.Skip("set RATING_PARITY_DB=1 (with DATABASE_* + RATING_PARITY_ENTITIES) to run the rating parity dump")
	}
	dbURL := parityFirstNonEmpty(os.Getenv("DATABASE_PRIVATE_URL"), os.Getenv("DATABASE_URL"))
	if dbURL == "" {
		t.Fatal("DATABASE_PRIVATE_URL or DATABASE_URL must be set")
	}
	model := parityFirstNonEmpty(os.Getenv("OLLAMA_MODEL"), "mistral:7b")
	specs := paritySplitSpecs(os.Getenv("RATING_PARITY_ENTITIES"))
	if len(specs) == 0 {
		t.Fatal("RATING_PARITY_ENTITIES must be set (comma- or space-separated entity_type:id:sport)")
	}

	ctx := context.Background()
	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatalf("db connect: %v", err)
	}
	defer pool.Close()

	written := 0
	for _, s := range specs {
		sportUp := strings.ToUpper(s.sport)
		if s.entityType == "team" {
			// team_stats lacks the rating_modes column, so the (frozen) Go loadRatingProfile errors
			// on every team — team rating commentary is DORMANT in Go (0 team rows in stat_summaries).
			// The Rust L12 port FIXES this (loads teams with empty rate_modes), so team rating is
			// Rust-only NEW capability validated by quality-eval, not Go byte-parity (no baseline to
			// match). Skip teams here rather than failing on the loader error.
			t.Logf("%s/%d (%s): skipped — team rating is dormant in Go (team_stats has no rating_modes); Rust-only capability post-L12", s.entityType, s.entityID, s.sport)
			continue
		}
		name, err := parityLookupName(ctx, pool, s.entityType, s.entityID, sportUp)
		if err != nil {
			t.Errorf("%s/%d (%s): name lookup: %v", s.entityType, s.entityID, s.sport, err)
			continue
		}

		// season nil → latest, exactly like the production single/backfill path. profile == nil or a
		// row with no composite + empty breakdown ⇒ the no-usable-rating marker → skip (no diffable
		// axes), mirroring the Rust bin's RatingBuild::NoStats short-circuit.
		profile, err := loadRatingProfile(ctx, pool, s.entityType, s.entityID, sportUp, nil)
		if err != nil {
			t.Errorf("%s/%d: load rating profile: %v", s.entityType, s.entityID, err)
			continue
		}
		if profile == nil || (profile.compositeScore == nil && len(profile.breakdown) == 0) {
			t.Logf("%s/%d (%s): skipped (no usable rating profile)", s.entityType, s.entityID, s.sport)
			continue
		}

		// Canonical input components + hash (the bytes hashComponents hashes, so the persisted
		// input_components and input_hash agree — the strict 5th parity axis).
		ic := profile.inputComponents()
		hash := hashComponents(ic)
		icJSON, err := json.Marshal(orEmptyMap(ic))
		if err != nil {
			t.Errorf("%s/%d: marshal input_components: %v", s.entityType, s.entityID, err)
			continue
		}

		notability, ncomp := computeNotability(profile)
		ncompJSON, err := json.Marshal(orEmptyMap(ncomp))
		if err != nil {
			t.Errorf("%s/%d: marshal notability_components: %v", s.entityType, s.entityID, err)
			continue
		}

		req := RatingRequest{
			EntityType: s.entityType, EntityID: s.entityID, EntityName: name,
			Sport: sportUp, TriggerType: "periodic",
		}
		prompt := buildStatPrompt(req, profile, notability)
		// Rating is NOT JSON mode (free-text prose), so parityCanonicalRequest (no format field) is
		// the right body builder — num_predict 2000, temperature 0, the s6 system prompt verbatim.
		reqBody := parityCanonicalRequest(model, prompt, ratingSystemPrompt, 2000, 0)

		if err := parityInsertRatingShadow(ctx, pool, s.entityType, s.entityID, sportUp, profile.season,
			notability, ncompJSON, icJSON, hash, model, ratingPromptVersion, prompt, reqBody); err != nil {
			t.Errorf("%s/%d: insert: %v", s.entityType, s.entityID, err)
			continue
		}
		t.Logf("%s/%d (%s) season %d: notability %d | hash %s | %d bytes prompt",
			s.entityType, s.entityID, s.sport, profile.season, notability, hash, len(prompt))
		written++
	}
	t.Logf("done — %d entit(ies) written to stat_summaries_shadow (source=go)", written)
}

// parityInsertRatingShadow writes one source='go' row with the deterministic axes (body/divined_peak
// left NULL — the prose is not a temp-0 parity axis). notability is deterministic (it is rendered into
// built_prompt) so it is recorded. Mirrors the Rust bin's deterministic dump.
func parityInsertRatingShadow(
	ctx context.Context, pool *pgxpool.Pool,
	entityType string, entityID int, sport string, season int,
	notability int, notabilityComponents, inputComponents []byte, inputHash string,
	model, promptVersion, builtPrompt string, reqBody []byte,
) error {
	if len(notabilityComponents) == 0 {
		notabilityComponents = []byte("{}")
	}
	if len(inputComponents) == 0 {
		inputComponents = []byte("{}")
	}
	notabilityArg := int16(notability)
	_, err := pool.Exec(ctx, `
		INSERT INTO stat_summaries_shadow (
		    source, entity_type, entity_id, sport, season, trigger_type, trigger_payload,
		    notability, notability_components, input_components, input_hash,
		    model_version, prompt_version, temperature, built_prompt, ollama_request
		) VALUES ('go',$1,$2,$3,$4,'periodic','{}'::jsonb,$5,$6::jsonb,$7::jsonb,$8,$9,$10,$11,$12,$13::jsonb)
	`,
		entityType, entityID, sport, season,
		notabilityArg, string(notabilityComponents), string(inputComponents), inputHash,
		model, promptVersion, float32(0), builtPrompt, string(reqBody),
	)
	return err
}
