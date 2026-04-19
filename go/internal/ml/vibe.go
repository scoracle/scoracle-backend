package ml

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// Prompt version — bump when the prompt text below materially changes so
// we can trace which prompt produced which blurb in vibe_scores.
const vibePromptVersion = "v1"

// Maximum blurb length in characters. Enforced by truncation after the
// model responds — Gemma isn't always obedient about length requests.
const vibeMaxChars = 140

// Corpus windows — how far back we look when assembling Gemma's context.
const (
	newsLookback  = 72 * time.Hour // 3 days
	tweetLookback = 24 * time.Hour // matches the tweet TTL
	maxNewsItems  = 12
	maxTweetItems = 8
)

// VibeRequest describes the entity to score and the triggering fact (if any).
type VibeRequest struct {
	EntityType  string         // 'player' or 'team'
	EntityID    int
	EntityName  string         // used in the prompt for Gemma's reference
	Sport       string         // 'NBA' | 'NFL' | 'FOOTBALL'
	TriggerType string         // 'milestone' | 'manual' | 'periodic'
	Trigger     map[string]any // milestone fact (stat key, value, percentile)
}

// VibeResult is what the generator persists to vibe_scores and returns
// to callers (CLI / HTTP endpoint).
type VibeResult struct {
	Blurb         string
	Model         string
	PromptVersion string
	InputNewsIDs  []int64
	InputTweetIDs []string
	Duration      time.Duration
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

// Generate builds a prompt from recent news + tweets + milestone context,
// calls Gemma, persists the blurb to vibe_scores, and returns the result.
//
// Tweet reads are cache-only. If the tweets table has nothing recent for
// the sport, the blurb is assembled from news + milestone alone — we
// NEVER trigger a fresh X API fetch from this path. Fresh pulls are
// strictly user-traffic-driven (see thirdparty/twitter.go).
func (g *Generator) Generate(ctx context.Context, req VibeRequest) (*VibeResult, error) {
	if g.pool == nil {
		return nil, fmt.Errorf("vibe generator: no db pool")
	}
	if req.EntityID <= 0 || req.EntityName == "" || req.Sport == "" || req.EntityType == "" {
		return nil, fmt.Errorf("vibe generator: entity context incomplete")
	}
	sport := strings.ToUpper(req.Sport)

	news, newsIDs, err := g.loadRecentNews(ctx, req.EntityType, req.EntityID, sport)
	if err != nil {
		return nil, fmt.Errorf("load news: %w", err)
	}
	tweets, tweetIDs, err := g.loadRecentTweets(ctx, req.EntityName, sport)
	if err != nil {
		return nil, fmt.Errorf("load tweets: %w", err)
	}

	prompt := buildVibePrompt(req, news, tweets)

	start := time.Now()
	// num_predict=800 gives Gemma 4 enough internal reasoning headroom
	// before it emits the final blurb. Lower values caused empty responses
	// (done_reason=length with eval_count hitting the cap before any
	// visible output) during early testing.
	gen, err := g.ollama.Generate(ctx, prompt, GenerateOptions{
		System:      vibeSystemPrompt,
		Temperature: 0.7,
		NumPredict:  800,
	})
	if err != nil {
		return nil, fmt.Errorf("gemma generate: %w", err)
	}
	duration := time.Since(start)

	blurb := sanitizeBlurb(gen.Response)
	if blurb == "" {
		return nil, fmt.Errorf("gemma returned empty blurb (raw=%q prompt_len=%d)",
			truncate(gen.Response, 300), len(prompt))
	}

	if err := g.persistVibe(ctx, req, sport, blurb, gen.Model, newsIDs, tweetIDs); err != nil {
		return nil, fmt.Errorf("persist vibe: %w", err)
	}

	return &VibeResult{
		Blurb:         blurb,
		Model:         gen.Model,
		PromptVersion: vibePromptVersion,
		InputNewsIDs:  newsIDs,
		InputTweetIDs: tweetIDs,
		Duration:      duration,
	}, nil
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

func (g *Generator) loadRecentNews(
	ctx context.Context, entityType string, entityID int, sport string,
) ([]newsItem, []int64, error) {
	rows, err := g.pool.Query(ctx, `
		SELECT a.id, a.title, COALESCE(a.description, ''), COALESCE(a.source, ''), a.published_at
		FROM news_article_entities nae
		JOIN news_articles a ON a.id = nae.article_id
		WHERE nae.entity_type = $1 AND nae.entity_id = $2 AND nae.sport = $3
		  AND (a.published_at IS NULL OR a.published_at > NOW() - $4::interval)
		ORDER BY COALESCE(a.published_at, a.fetched_at) DESC
		LIMIT $5
	`, entityType, entityID, sport, fmt.Sprintf("%d seconds", int(newsLookback.Seconds())), maxNewsItems)
	if err != nil {
		return nil, nil, err
	}
	defer rows.Close()

	var out []newsItem
	var ids []int64
	for rows.Next() {
		var n newsItem
		if err := rows.Scan(&n.id, &n.title, &n.description, &n.source, &n.publishedAt); err != nil {
			return nil, nil, err
		}
		out = append(out, n)
		ids = append(ids, n.id)
	}
	return out, ids, nil
}

// loadRecentTweets pulls up to maxTweetItems tweets from the sport's
// cached feed whose author_username OR text mentions the entity name.
// This is lazy-only — no fresh X API calls originate here.
func (g *Generator) loadRecentTweets(
	ctx context.Context, entityName, sport string,
) ([]tweetItem, []string, error) {
	// Simple pattern match: entity full name OR last-word surname.
	// Tweet text matching in SQL is crude but cheap; the vibe is
	// qualitative anyway.
	last := entityName
	if parts := strings.Fields(entityName); len(parts) > 1 {
		last = parts[len(parts)-1]
	}

	rows, err := g.pool.Query(ctx, `
		SELECT id, author_username, text, posted_at
		FROM tweets
		WHERE sport = $1
		  AND fetched_at > NOW() - $2::interval
		  AND (text ILIKE '%' || $3 || '%' OR text ILIKE '%' || $4 || '%')
		ORDER BY posted_at DESC
		LIMIT $5
	`, strings.ToLower(sport),
		fmt.Sprintf("%d seconds", int(tweetLookback.Seconds())),
		entityName, last, maxTweetItems,
	)
	if err != nil {
		return nil, nil, err
	}
	defer rows.Close()

	var out []tweetItem
	var ids []string
	for rows.Next() {
		var t tweetItem
		if err := rows.Scan(&t.id, &t.author, &t.text, &t.postedAt); err != nil {
			return nil, nil, err
		}
		out = append(out, t)
		ids = append(ids, t.id)
	}
	return out, ids, nil
}

// ---------------------------------------------------------------------------
// Prompt assembly
// ---------------------------------------------------------------------------

const vibeSystemPrompt = `You write ultra-short sports vibe blurbs (<= 140 characters).
Conversational fan voice, not a press-release tone. One sentence.
No emojis, no hashtags, no quotation marks around the whole blurb.
Lead with sentiment from the news and tweets — stats are context, not the point.
If the source material is thin or neutral, reflect that honestly — don't fabricate drama.
Respond immediately with just the blurb. No thinking, no preamble, no explanation.`

func buildVibePrompt(req VibeRequest, news []newsItem, tweets []tweetItem) string {
	var b strings.Builder

	b.WriteString(fmt.Sprintf("Entity: %s %s (%s)\n", strings.Title(req.EntityType), req.EntityName, req.Sport))

	if len(req.Trigger) > 0 && req.TriggerType == "milestone" {
		raw, _ := json.Marshal(req.Trigger)
		b.WriteString(fmt.Sprintf("Milestone context (light sprinkle only): %s\n", string(raw)))
	}

	b.WriteString("\nRecent news headlines:\n")
	if len(news) == 0 {
		b.WriteString("- (none in the last 3 days)\n")
	} else {
		for _, n := range news {
			b.WriteString("- ")
			if n.source != "" {
				b.WriteString(fmt.Sprintf("[%s] ", n.source))
			}
			b.WriteString(n.title)
			if n.description != "" {
				b.WriteString(" — ")
				b.WriteString(truncate(n.description, 160))
			}
			b.WriteString("\n")
		}
	}

	b.WriteString("\nRecent tweets:\n")
	if len(tweets) == 0 {
		b.WriteString("- (none in the last 24 hours)\n")
	} else {
		for _, t := range tweets {
			b.WriteString(fmt.Sprintf("- @%s: %s\n", t.author, truncate(strings.ReplaceAll(t.text, "\n", " "), 200)))
		}
	}

	b.WriteString("\nWrite the vibe blurb now.")
	return b.String()
}

// ---------------------------------------------------------------------------
// Output sanitation + persistence
// ---------------------------------------------------------------------------

func sanitizeBlurb(raw string) string {
	s := strings.TrimSpace(raw)
	// Strip leading/trailing quotes if Gemma wrapped the whole thing.
	s = strings.Trim(s, `"'` + "`")
	// Collapse whitespace.
	s = strings.Join(strings.Fields(s), " ")
	// Hard truncate to cap.
	if len(s) > vibeMaxChars {
		s = s[:vibeMaxChars]
		// Trim any partial word at the edge.
		if idx := strings.LastIndexAny(s, " .,;:!?"); idx > vibeMaxChars-40 {
			s = s[:idx]
		}
		s = strings.TrimRight(s, " .,;:")
		s += "…"
	}
	return s
}

func (g *Generator) persistVibe(
	ctx context.Context,
	req VibeRequest,
	sport, blurb, model string,
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
		    blurb, input_news_ids, input_tweet_ids,
		    model_version, prompt_version
		) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
	`,
		req.EntityType, req.EntityID, sport,
		req.TriggerType, triggerJSON,
		blurb, newsIDs, tweetIDs,
		model, vibePromptVersion,
	)
	return err
}
