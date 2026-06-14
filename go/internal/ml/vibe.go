package ml

import (
	"context"
	"encoding/json"
	"fmt"
	"regexp"
	"strconv"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// Prompt version — bump when the prompt text below materially changes so
// we can trace which prompt produced which score in vibe_scores.
//
// v2: 1-10 internal scale, x10 multiplier on persist. Always landed on
//
//	multiples of 10 (e.g. 90/100), which read as low-resolution.
//
// v3: 1-100 direct scale. Prompt explicitly discourages round answers so
//
//	Gemma uses the full range (e.g. 73, 87) instead of multiples of 10.
//
// v4: the vibe is now the LAST stage — it DETERMINES sentiment from the DERIVED
//
//	layer (the entity's latest narratives from ml/news_narratives.go + its
//	transfer heat), not raw news/tweets. Richest context, per the staged pipeline.
const vibePromptVersion = "v4"

// Corpus windows — how far back we look when assembling Gemma's context.
//
// NewsLookback is exported so callers that *select candidates* (e.g.
// cmd/vibe corpus mode) can apply the same window when picking entities
// whose links are "fresh enough to be worth scoring." Without that
// alignment, an entity with a brand-new link to an old article gets
// queued and then skipped inside Generate, producing a null marker.
const (
	NewsLookback  = 72 * time.Hour // 3 days
	tweetLookback = 24 * time.Hour // matches the tweet TTL
	maxNewsItems  = 12
	maxTweetItems = 8
)

// newsLookback is the unexported alias the rest of this package uses.
// Kept so existing call sites read naturally; switch to NewsLookback at
// any external boundary.
const newsLookback = NewsLookback

// VibeRequest describes the entity to score and the triggering fact (if any).
type VibeRequest struct {
	EntityType  string // 'player' or 'team'
	EntityID    int
	EntityName  string         // used in the prompt for Gemma's reference
	Sport       string         // 'NBA' | 'NFL' | 'FOOTBALL'
	TriggerType string         // 'milestone' | 'manual' | 'periodic'
	Trigger     map[string]any // milestone fact (stat key, value, percentile)
}

// VibeResult is what the generator persists to vibe_scores and returns
// to callers (CLI / HTTP endpoint). Sentiment is on a 1-100 scale,
// produced directly by Gemma (v3 prompt).
//
// When the corpus is empty (no news AND no tweets) the generator skips
// the Gemma call and writes a row with NULL sentiment as an explicit
// "no data" signal for the frontend. In that case Sentiment == 0 and
// SkippedNoCorpus == true.
type VibeResult struct {
	Sentiment       int
	SkippedNoCorpus bool
	Model           string
	PromptVersion   string
	InputNewsIDs    []int64
	InputTweetIDs   []string
	Duration        time.Duration
}

// Generator wires Ollama to the Postgres corpus. Reused by CLI,
// HTTP endpoint, and the milestone listener — single primitive.
type Generator struct {
	pool   *pgxpool.Pool
	ollama *OllamaClient
}

func NewGenerator(pool *pgxpool.Pool, ollama *OllamaClient) *Generator {
	return &Generator{pool: pool, ollama: ollama}
}

// Generate determines an overall 1-100 sentiment for the entity from the DERIVED
// layer — its latest narratives (ml/news_narratives.go) + current transfer heat —
// persists it to vibe_scores, and returns the result. This is the LAST stage of
// the pipeline (raw → scrub → narratives → transfer heat → VIBE): by now Gemma
// reasons over the curated storylines, not raw headlines (the richest context).
func (g *Generator) Generate(ctx context.Context, req VibeRequest) (*VibeResult, error) {
	if g.pool == nil {
		return nil, fmt.Errorf("vibe generator: no db pool")
	}
	if req.EntityID <= 0 || req.EntityName == "" || req.Sport == "" || req.EntityType == "" {
		return nil, fmt.Errorf("vibe generator: entity context incomplete")
	}
	sport := strings.ToUpper(req.Sport)

	narratives, newsIDs, err := g.loadLatestNarratives(ctx, req.EntityType, req.EntityID, sport)
	if err != nil {
		return nil, fmt.Errorf("load narratives: %w", err)
	}
	heat, err := g.loadTransferHeat(ctx, req.EntityType, req.EntityID, sport)
	if err != nil {
		return nil, fmt.Errorf("load transfer heat: %w", err)
	}

	// No derived signal (no narratives AND no transfer heat) → no rating. Persist
	// a NULL-sentiment row so the read path returns "no data" and the debounce
	// skips this entity until its narratives/heat change. The vibe runs LAST, so
	// "no narratives" means the narratives stage found no corpus this cycle; a
	// neutral-anchor 50 would only pollute /vibe/hottest.
	if len(narratives) == 0 && len(heat) == 0 {
		if err := g.persistNoCorpus(ctx, req, sport); err != nil {
			return nil, fmt.Errorf("persist no-corpus marker: %w", err)
		}
		return &VibeResult{
			SkippedNoCorpus: true,
			Model:           g.ollama.Model(),
			PromptVersion:   vibePromptVersion,
			InputNewsIDs:    []int64{},
			InputTweetIDs:   []string{},
		}, nil
	}

	prompt := buildVibePrompt(req, narratives, heat)

	start := time.Now()
	// Gemma 4 burns predict tokens on internal reasoning before any visible
	// output — too low a cap returns empty. 1200 covers a number-only answer
	// over the narratives + heat context; the cost is dominated by eval_count
	// (a single digit), not the cap.
	gen, err := g.ollama.Generate(ctx, prompt, GenerateOptions{
		System:      vibeSystemPrompt,
		Temperature: 0.7,
		NumPredict:  1200,
	})
	if err != nil {
		return nil, fmt.Errorf("gemma generate: %w", err)
	}
	duration := time.Since(start)

	sentiment, err := parseSentiment(gen.Response)
	if err != nil {
		return nil, fmt.Errorf("parse sentiment (raw=%q prompt_len=%d): %w",
			truncate(gen.Response, 120), len(prompt), err)
	}

	// input_news_ids = the narratives' source articles (provenance); tweets are
	// no longer a vibe input (the derived layer supersedes raw feeds).
	if err := g.persistVibe(ctx, req, sport, sentiment, gen.Model, newsIDs, nil); err != nil {
		return nil, fmt.Errorf("persist vibe: %w", err)
	}

	return &VibeResult{
		Sentiment:     sentiment,
		Model:         gen.Model,
		PromptVersion: vibePromptVersion,
		InputNewsIDs:  newsIDs,
		InputTweetIDs: []string{},
		Duration:      duration,
	}, nil
}

// persistNoCorpus writes a row with NULL sentiment so the API returns
// {"sentiment": null} and the batch debounce stops re-running this entity
// until something in the corpus changes.
func (g *Generator) persistNoCorpus(ctx context.Context, req VibeRequest, sport string) error {
	triggerJSON, err := json.Marshal(req.Trigger)
	if err != nil {
		return err
	}
	_, err = g.pool.Exec(ctx, `
		INSERT INTO vibe_scores (
		    entity_type, entity_id, sport,
		    trigger_type, trigger_payload,
		    sentiment, blurb, input_news_ids, input_tweet_ids,
		    model_version, prompt_version
		) VALUES ($1,$2,$3,$4,$5,NULL,NULL,'{}','{}',$6,$7)
	`,
		req.EntityType, req.EntityID, sport,
		req.TriggerType, triggerJSON,
		g.ollama.Model(), vibePromptVersion,
	)
	return err
}

// ---------------------------------------------------------------------------
// Corpus loaders
// ---------------------------------------------------------------------------

type newsItem struct {
	id          int64
	title       string
	description string
	source      string
	publishedAt *time.Time
}

type tweetItem struct {
	id       string
	author   string
	text     string
	postedAt time.Time
}

// maxHeatItems bounds the transfer rumors shown to the vibe as the "transfer
// temperature" signal alongside the narratives.
const maxHeatItems = 6

type narrativeItem struct {
	title  string
	body   string
	impact int
}

type heatItem struct {
	counterparty string
	heat         int
	stage        string
	direction    string
}

// loadLatestNarratives returns the narratives from the entity's most recent
// generation (news_summaries; ml/news_narratives.go), hottest first, plus the
// deduped union of their source article ids for provenance. Empty when the latest
// generation was a no-narratives marker or the entity has none yet.
func (g *Generator) loadLatestNarratives(
	ctx context.Context, entityType string, entityID int, sport string,
) ([]narrativeItem, []int64, error) {
	rows, err := g.pool.Query(ctx, `
		SELECT narrative_title, body, COALESCE(impact, 0), input_news_ids
		FROM news_summaries
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
		  AND body IS NOT NULL
		  AND generated_at = (
		      SELECT max(generated_at) FROM news_summaries
		      WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
		  )
		ORDER BY impact DESC NULLS LAST
	`, entityType, entityID, sport)
	if err != nil {
		return nil, nil, err
	}
	defer rows.Close()

	var out []narrativeItem
	var ids []int64
	for rows.Next() {
		var n narrativeItem
		var nids []int64
		if err := rows.Scan(&n.title, &n.body, &n.impact, &nids); err != nil {
			return nil, nil, err
		}
		out = append(out, n)
		ids = append(ids, nids...)
	}
	return out, dedupeInt64(ids), rows.Err()
}

// loadTransferHeat returns the entity's hottest active transfer/trade rumors
// (latest per counterparty, heat > 0), naming the counterparty — the transfer
// "temperature" the vibe weighs alongside the narratives. Branches on entity type
// (a team's player rumors vs a player's suitor clubs).
func (g *Generator) loadTransferHeat(
	ctx context.Context, entityType string, entityID int, sport string,
) ([]heatItem, error) {
	q := `
		SELECT counterparty, heat, stage, direction FROM (
		    SELECT DISTINCT ON (tr.team_id)
		           t.name AS counterparty, tr.heat,
		           COALESCE(tr.stage,'') AS stage, COALESCE(tr.direction,'') AS direction,
		           tr.generated_at
		    FROM transfer_rumors tr
		    JOIN teams t ON t.id = tr.team_id AND t.sport = tr.sport
		    WHERE tr.player_id = $1 AND tr.sport = $2 AND tr.heat IS NOT NULL
		    ORDER BY tr.team_id, tr.generated_at DESC
		) latest
		WHERE heat > 0 ORDER BY heat DESC LIMIT $3`
	if entityType == "team" {
		q = `
		SELECT counterparty, heat, stage, direction FROM (
		    SELECT DISTINCT ON (tr.player_id)
		           p.name AS counterparty, tr.heat,
		           COALESCE(tr.stage,'') AS stage, COALESCE(tr.direction,'') AS direction,
		           tr.generated_at
		    FROM transfer_rumors tr
		    JOIN players p ON p.id = tr.player_id AND p.sport = tr.sport
		    WHERE tr.team_id = $1 AND tr.sport = $2 AND tr.heat IS NOT NULL
		    ORDER BY tr.player_id, tr.generated_at DESC
		) latest
		WHERE heat > 0 ORDER BY heat DESC LIMIT $3`
	}
	rows, err := g.pool.Query(ctx, q, entityID, sport, maxHeatItems)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []heatItem
	for rows.Next() {
		var h heatItem
		if err := rows.Scan(&h.counterparty, &h.heat, &h.stage, &h.direction); err != nil {
			return nil, err
		}
		out = append(out, h)
	}
	return out, rows.Err()
}

func dedupeInt64(in []int64) []int64 {
	out := make([]int64, 0, len(in))
	seen := make(map[int64]struct{}, len(in))
	for _, v := range in {
		if _, ok := seen[v]; ok {
			continue
		}
		seen[v] = struct{}{}
		out = append(out, v)
	}
	return out
}

// ---------------------------------------------------------------------------
// Prompt assembly
// ---------------------------------------------------------------------------

const vibeSystemPrompt = `You determine overall sentiment toward a sports entity from the NARRATIVES forming around it and its current TRANSFER/TRADE activity — the derived signals, not raw headlines.
Reply with ONLY a single integer from 1 to 100.
1 = overwhelmingly negative, 50 = neutral or mixed, 100 = overwhelmingly positive.
Use the FULL range with precision. Pick exact numbers like 47, 63, 78, 92.
Avoid round multiples of 10 (50, 60, 70, 80, 90) unless the evidence genuinely lands there.
Weigh each narrative by its impact (the bracketed number) — bigger storylines move the score more. Transfer heat signals activity and drama, which is NOT inherently positive or negative; judge the tone of what is actually happening.
If the material is thin or neutral, answer somewhere in 45-55 — don't invent drama.
No words, no punctuation, no explanation. Just the number.`

func buildVibePrompt(req VibeRequest, narratives []narrativeItem, heat []heatItem) string {
	var b strings.Builder

	b.WriteString(fmt.Sprintf("Entity: %s %s (%s)\n", strings.Title(req.EntityType), req.EntityName, req.Sport))

	b.WriteString("\nNarratives forming around them (impact in brackets):\n")
	if len(narratives) == 0 {
		b.WriteString("- (none this cycle)\n")
	} else {
		for _, n := range narratives {
			b.WriteString(fmt.Sprintf("- [%d] %s: %s\n", n.impact, n.title, truncate(n.body, 280)))
		}
	}

	b.WriteString("\nCurrent transfer/trade activity (heat 0-100):\n")
	if len(heat) == 0 {
		b.WriteString("- (none)\n")
	} else {
		for _, h := range heat {
			line := fmt.Sprintf("- %s — heat %d", h.counterparty, h.heat)
			if h.direction != "" {
				line += ", " + h.direction
			}
			if h.stage != "" {
				line += ", " + h.stage
			}
			b.WriteString(line + "\n")
		}
	}

	b.WriteString("\nReply with the sentiment score (1-100, exact value, not rounded) now.")
	return b.String()
}

// ---------------------------------------------------------------------------
// Output sanitation + persistence
// ---------------------------------------------------------------------------

var sentimentDigitRE = regexp.MustCompile(`\d+`)

// parseSentiment pulls the first integer out of Gemma's response and clamps
// it into 1-100. Returns an error only when the response contains no digits
// at all. The v3 prompt asks for the score directly on the 1-100 scale, so
// no scaling is applied — Gemma is expected to produce e.g. 73, not 7.
func parseSentiment(raw string) (int, error) {
	match := sentimentDigitRE.FindString(raw)
	if match == "" {
		return 0, fmt.Errorf("no digit in response")
	}
	n, err := strconv.Atoi(match)
	if err != nil {
		return 0, fmt.Errorf("parse digit %q: %w", match, err)
	}
	if n < 1 {
		n = 1
	}
	if n > 100 {
		n = 100
	}
	return n, nil
}

func (g *Generator) persistVibe(
	ctx context.Context,
	req VibeRequest,
	sport string, sentiment int, model string,
	newsIDs []int64,
	tweetIDs []string,
) error {
	triggerJSON, err := json.Marshal(req.Trigger)
	if err != nil {
		return err
	}
	// pgx encodes nil Go slices as NULL, but these columns are NOT NULL
	// with default '{}'. Coerce nils to empty slices so the insert works
	// even when there's no corpus context yet.
	if newsIDs == nil {
		newsIDs = []int64{}
	}
	if tweetIDs == nil {
		tweetIDs = []string{}
	}
	_, err = g.pool.Exec(ctx, `
		INSERT INTO vibe_scores (
		    entity_type, entity_id, sport,
		    trigger_type, trigger_payload,
		    sentiment, blurb, input_news_ids, input_tweet_ids,
		    model_version, prompt_version
		) VALUES ($1,$2,$3,$4,$5,$6,NULL,$7,$8,$9,$10)
	`,
		req.EntityType, req.EntityID, sport,
		req.TriggerType, triggerJSON,
		sentiment, newsIDs, tweetIDs,
		model, vibePromptVersion,
	)
	return err
}
