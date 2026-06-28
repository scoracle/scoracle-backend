package ml

// narratives_parity_test.go — Go side of the Rust Cognition Harness L13 narratives parity proof.
//
// Gated on NARRATIVES_PARITY_DB. For each entity in NARRATIVES_PARITY_ENTITIES it loads the same
// vetted recent corpus and transfer heat through the package's unexported loaders, builds the same
// n3 prompt, and writes source='go' rows to news_summaries_shadow with deterministic axes only:
// built_prompt + ollama_request + model_version + prompt_version. It never writes news_summaries or
// claims pipeline_work.

import (
	"context"
	"os"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"
)

func TestNarrativesParityDump(t *testing.T) {
	if os.Getenv("NARRATIVES_PARITY_DB") == "" {
		t.Skip("set NARRATIVES_PARITY_DB=1 (with DATABASE_* + NARRATIVES_PARITY_ENTITIES) to run the narratives parity dump")
	}
	dbURL := parityFirstNonEmpty(os.Getenv("DATABASE_PRIVATE_URL"), os.Getenv("DATABASE_URL"))
	if dbURL == "" {
		t.Fatal("DATABASE_PRIVATE_URL or DATABASE_URL must be set")
	}
	model := parityFirstNonEmpty(os.Getenv("OLLAMA_MODEL"), "mistral:7b")
	specs := paritySplitSpecs(os.Getenv("NARRATIVES_PARITY_ENTITIES"))
	if len(specs) == 0 {
		t.Fatal("NARRATIVES_PARITY_ENTITIES must be set (comma- or space-separated entity_type:id:sport)")
	}

	ctx := context.Background()
	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatalf("db connect: %v", err)
	}
	defer pool.Close()
	g := &NewsNarrator{pool: pool}

	written := 0
	for _, s := range specs {
		sportUp := strings.ToUpper(s.sport)
		name, err := parityLookupName(ctx, pool, s.entityType, s.entityID, sportUp)
		if err != nil {
			t.Errorf("%s/%d (%s): name lookup: %v", s.entityType, s.entityID, s.sport, err)
			continue
		}
		news, err := g.loadVettedCorpus(ctx, s.entityType, s.entityID, sportUp)
		if err != nil {
			t.Errorf("%s/%d: load news: %v", s.entityType, s.entityID, err)
			continue
		}
		if len(news) == 0 {
			t.Logf("%s/%d (%s): skipped (no vetted recent corpus)", s.entityType, s.entityID, s.sport)
			continue
		}
		heat, err := loadTransferHeat(ctx, pool, s.entityType, s.entityID, sportUp)
		if err != nil {
			t.Logf("%s/%d: load heat failed, continuing ungrounded: %v", s.entityType, s.entityID, err)
			heat = nil
		}
		req := NarrativesRequest{
			EntityType: s.entityType, EntityID: s.entityID, EntityName: name,
			Sport: sportUp, TriggerType: "periodic",
		}
		prompt := buildNarrativesPrompt(req, news, heat)
		reqBody := parityCanonicalRequest(model, prompt, newsNarrativesSystemPrompt, 4000, 0)
		newsIDs := make([]int64, 0, len(news))
		for _, n := range news {
			newsIDs = append(newsIDs, n.id)
		}

		if err := parityInsertNarrativesShadow(ctx, pool, s.entityType, s.entityID, sportUp,
			newsIDs, len(news), model, newsNarrativesPromptVersion, prompt, reqBody); err != nil {
			t.Errorf("%s/%d: insert: %v", s.entityType, s.entityID, err)
			continue
		}
		t.Logf("%s/%d (%s): corpus %d | %d bytes prompt", s.entityType, s.entityID, s.sport, len(news), len(prompt))
		written++
	}
	t.Logf("done — %d entit(ies) written to news_summaries_shadow (source=go)", written)
}

func parityInsertNarrativesShadow(
	ctx context.Context, pool *pgxpool.Pool,
	entityType string, entityID int, sport string,
	newsIDs []int64, corpusSize int,
	model, promptVersion, builtPrompt string, reqBody []byte,
) error {
	if newsIDs == nil {
		newsIDs = []int64{}
	}
	_, err := pool.Exec(ctx, `
		INSERT INTO news_summaries_shadow (
		    source, entity_type, entity_id, sport, trigger_type, trigger_payload,
		    impact_components, input_news_ids, original_corpus_size, deduped_corpus_size,
		    model_version, prompt_version, temperature, built_prompt, ollama_request
		) VALUES ('go',$1,$2,$3,'periodic','null'::jsonb,'{}'::jsonb,$4,$5,$6,$7,$8,$9,$10,$11::jsonb)
	`,
		entityType, entityID, sport, newsIDs, corpusSize, corpusSize,
		model, promptVersion, float32(0), builtPrompt, string(reqBody),
	)
	return err
}
