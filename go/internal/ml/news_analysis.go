package ml

// News analysis — the unified Gemma "buzz" pass. ONE call over an entity's
// recent news corpus produces a prose summary + trending topics + a 1-100
// sentiment (the news analog of the vibe score, derived from the SAME articles
// in the SAME call — the efficiency win). A DETERMINISTIC impact score (0-100,
// the news analog of transfer heat) is computed from the corpus in Go (Gemma
// never invents the number) and stored with transparent components; it ranks
// the news leaderboard and orders the summary impactful-first.
//
// Mirrors ml/vibe.go (corpus loader → prompt → Gemma → parse → persist). This
// file is additive: it does NOT modify vibe.go and (for now) persists ONLY to
// news_summaries — the sentiment it produces is returned for verification, and
// wiring it into the live trigger (to supersede the separate vibe call + write
// both tables from one pass) is a deliberate follow-on once verified.
//
// See vault wiki/Plan - News to Gemma Summaries.md (Stage 2a).

import (
	"context"
	"encoding/json"
	"fmt"
	"math"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// Bump when the prompt below materially changes (traced in news_summaries.prompt_version).
//
// v1: first cut — concise summaries.
// v2: comprehensive recap (a briefing, not a teaser) + name-drop specific
//     people/clubs instead of genericizing ("a Real Madrid star" → the name).
const newsAnalysisPromptVersion = "v2"

// NewsAnalysisRequest describes the entity whose recent news to analyze.
type NewsAnalysisRequest struct {
	EntityType  string // 'player' | 'team'
	EntityID    int
	EntityName  string
	Sport       string         // 'NBA' | 'NFL' | 'FOOTBALL'
	TriggerType string         // 'news_spike' | 'periodic' | 'manual'
	Trigger     map[string]any // optional context
	// DryRun skips persistence (used to verify Gemma output before the
	// news_summaries table is applied). The corpus is still read + scored.
	DryRun bool
}

// NewsAnalysisResult is what Analyze produces. Sentiment is 1-100 (Gemma),
// Impact is 0-100 (deterministic). SkippedNoCorpus is true when there was no
// relevant news this cycle (a NULL-summary row is persisted as the marker).
type NewsAnalysisResult struct {
	Summary          string
	TrendingTopics   []string
	Sentiment        int
	Impact           int
	ImpactComponents map[string]any
	SkippedNoCorpus  bool
	Model            string
	PromptVersion    string
	InputNewsIDs     []int64
	Duration         time.Duration
}

// NewsAnalyzer wires Ollama to the Postgres news corpus. Reusable by the
// listener, the cron sweep, and a CLI — single primitive (like ml.Generator).
type NewsAnalyzer struct {
	pool   *pgxpool.Pool
	ollama *OllamaClient
}

func NewNewsAnalyzer(pool *pgxpool.Pool, ollama *OllamaClient) *NewsAnalyzer {
	return &NewsAnalyzer{pool: pool, ollama: ollama}
}

// Analyze loads the entity's recent news, asks Gemma for summary+topics+sentiment
// in one call, computes a deterministic impact, and persists to news_summaries.
func (a *NewsAnalyzer) Analyze(ctx context.Context, req NewsAnalysisRequest) (*NewsAnalysisResult, error) {
	if a.pool == nil {
		return nil, fmt.Errorf("news analyzer: no db pool")
	}
	if req.EntityID <= 0 || req.EntityName == "" || req.Sport == "" || req.EntityType == "" {
		return nil, fmt.Errorf("news analyzer: entity context incomplete")
	}
	sport := strings.ToUpper(req.Sport)

	news, newsIDs, err := a.loadRecentNews(ctx, req.EntityType, req.EntityID, sport)
	if err != nil {
		return nil, fmt.Errorf("load news: %w", err)
	}

	// No corpus → persist a NULL-summary marker so the read path returns an
	// explicit "no news" signal and the debounce skips this entity until the
	// corpus changes (the vibe persistNoCorpus analog).
	if len(news) == 0 {
		if !req.DryRun {
			if err := a.persistNoCorpus(ctx, req, sport); err != nil {
				return nil, fmt.Errorf("persist no-corpus marker: %w", err)
			}
		}
		return &NewsAnalysisResult{
			SkippedNoCorpus: true,
			Model:           a.ollama.Model(),
			PromptVersion:   newsAnalysisPromptVersion,
			InputNewsIDs:    []int64{},
		}, nil
	}

	prompt := buildNewsAnalysisPrompt(req, news)

	start := time.Now()
	// Gemma 4 spends predict tokens on internal reasoning before visible
	// output; a comprehensive (multi-sentence, name-dropping) summary needs
	// real room on top of that, so budget higher than vibe's number-only 1200.
	gen, err := a.ollama.Generate(ctx, prompt, GenerateOptions{
		System:      newsAnalysisSystemPrompt,
		Temperature: 0.6,
		NumPredict:  2200,
	})
	if err != nil {
		return nil, fmt.Errorf("gemma generate: %w", err)
	}
	duration := time.Since(start)

	parsed, ok := parseNewsAnalysis(gen.Response)
	if !ok {
		return nil, fmt.Errorf("parse analysis failed (raw=%q prompt_len=%d)",
			truncate(gen.Response, 160), len(prompt))
	}

	impact, components := computeNewsImpact(news)

	res := &NewsAnalysisResult{
		Summary:          parsed.Summary,
		TrendingTopics:   parsed.TrendingTopics,
		Sentiment:        parsed.Sentiment,
		Impact:           impact,
		ImpactComponents: components,
		Model:            gen.Model,
		PromptVersion:    newsAnalysisPromptVersion,
		InputNewsIDs:     newsIDs,
		Duration:         duration,
	}

	if !req.DryRun {
		if err := a.persistSummary(ctx, req, sport, res); err != nil {
			return nil, fmt.Errorf("persist news summary: %w", err)
		}
	}
	return res, nil
}

// ---------------------------------------------------------------------------
// Corpus loader (reads the existing news_article_entities links — the same
// corpus vibe uses; will be Gemma-gated in the Stage-1 follow-on).
// ---------------------------------------------------------------------------

func (a *NewsAnalyzer) loadRecentNews(
	ctx context.Context, entityType string, entityID int, sport string,
) ([]newsItem, []int64, error) {
	rows, err := a.pool.Query(ctx, `
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

// ---------------------------------------------------------------------------
// Deterministic impact (0-100) — the news analog of transfer heat. Computed
// from the corpus, NOT from Gemma, and stored with components for transparency.
// v1: volume (count) + corroboration (distinct sources) + recency. Source-tier
// weighting is a refinement (see source_tiers, migration 031).
// ---------------------------------------------------------------------------

func computeNewsImpact(news []newsItem) (int, map[string]any) {
	n := len(news)
	// Volume: saturating curve — a handful of articles is already hot, returns
	// diminish. ~0 at 0 articles, ~60 by ~10 articles.
	volume := 60.0 * (1.0 - math.Exp(-float64(n)/5.0))

	// Corroboration: distinct sources (independent coverage), capped at 25.
	sources := map[string]struct{}{}
	for _, a := range news {
		if a.source != "" {
			sources[strings.ToLower(a.source)] = struct{}{}
		}
	}
	distinct := len(sources)
	corroboration := math.Min(25.0, float64(distinct)*6.0)

	// Recency: how fresh is the freshest article.
	recency := 0.0
	var newest *time.Time
	for i := range news {
		if news[i].publishedAt != nil && (newest == nil || news[i].publishedAt.After(*newest)) {
			newest = news[i].publishedAt
		}
	}
	if newest != nil {
		age := time.Since(*newest)
		switch {
		case age <= 12*time.Hour:
			recency = 15.0
		case age <= 24*time.Hour:
			recency = 10.0
		case age <= 48*time.Hour:
			recency = 5.0
		}
	}

	score := int(math.Round(volume + corroboration + recency))
	if score < 0 {
		score = 0
	}
	if score > 100 {
		score = 100
	}
	components := map[string]any{
		"article_count":    n,
		"distinct_sources": distinct,
		"volume":           math.Round(volume*10) / 10,
		"corroboration":    math.Round(corroboration*10) / 10,
		"recency":          recency,
	}
	return score, components
}

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

const newsAnalysisSystemPrompt = `You analyze recent NEWS about one sports entity and return STRICT JSON only.
Output exactly one JSON object, no markdown fences, no text before or after:
{"sentiment": <integer 1-100>, "summary": "<recap>", "trending_topics": ["<topic>", ...]}

summary: a COMPREHENSIVE recap of the recent news in your own words — a full briefing, NOT a teaser. Cover the notable storylines, not just one. NAME the specific people and clubs: players, managers, executives, and the OTHER clubs involved in any move — always use the real names from the articles; never genericize to "a Real Madrid star" or "a former player" when the article gives a name. Lead with the most impactful storyline, then work through the rest. Length follows the news — a busy week earns several sentences; a quiet one stays short. Write original prose: do NOT quote headlines verbatim, do NOT include URLs or dump source names, do NOT invent facts not present in the articles.
sentiment: 1 = overwhelmingly negative, 50 = neutral/mixed, 100 = overwhelmingly positive. Use the full range with precision (e.g. 47, 63, 78); avoid round multiples of 10 unless genuinely earned.
trending_topics: 2-5 short noun phrases naming the key threads (e.g. "Cucurella future", "Osimhen pursuit", "managerial search").
If an article clearly is NOT about this entity, ignore it. If the material is genuinely thin/neutral, sentiment ~50 and keep it brief.`

func buildNewsAnalysisPrompt(req NewsAnalysisRequest, news []newsItem) string {
	var b strings.Builder
	b.WriteString(fmt.Sprintf("Entity: %s (%s %s)\n", req.EntityName, req.Sport, req.EntityType))
	b.WriteString("\nRecent news (last 3 days):\n")
	for _, n := range news {
		b.WriteString("- ")
		if n.source != "" {
			b.WriteString(fmt.Sprintf("[%s] ", n.source))
		}
		b.WriteString(n.title)
		if n.description != "" {
			b.WriteString(" — ")
			b.WriteString(truncate(n.description, 200))
		}
		b.WriteString("\n")
	}
	b.WriteString("\nReturn the JSON object now.")
	return b.String()
}

// ---------------------------------------------------------------------------
// Parse
// ---------------------------------------------------------------------------

type gemmaNewsAnalysis struct {
	Sentiment      int      `json:"sentiment"`
	Summary        string   `json:"summary"`
	TrendingTopics []string `json:"trending_topics"`
}

// parseNewsAnalysis extracts the first JSON object from Gemma's response
// (tolerating any stray text/markdown fences around it), unmarshals it, and
// clamps the sentiment to 1-100.
func parseNewsAnalysis(raw string) (gemmaNewsAnalysis, bool) {
	var out gemmaNewsAnalysis
	start := strings.IndexByte(raw, '{')
	end := strings.LastIndexByte(raw, '}')
	if start < 0 || end <= start {
		return out, false
	}
	if err := json.Unmarshal([]byte(raw[start:end+1]), &out); err != nil {
		return out, false
	}
	out.Summary = strings.TrimSpace(out.Summary)
	if out.Summary == "" {
		return out, false
	}
	if out.Sentiment < 1 {
		out.Sentiment = 1
	}
	if out.Sentiment > 100 {
		out.Sentiment = 100
	}
	return out, true
}

// ---------------------------------------------------------------------------
// Persist
// ---------------------------------------------------------------------------

func (a *NewsAnalyzer) persistNoCorpus(ctx context.Context, req NewsAnalysisRequest, sport string) error {
	triggerJSON, err := json.Marshal(req.Trigger)
	if err != nil {
		return err
	}
	_, err = a.pool.Exec(ctx, `
		INSERT INTO news_summaries (
		    entity_type, entity_id, sport, trigger_type, trigger_payload,
		    summary, trending_topics, impact, impact_components,
		    source_attribution, input_news_ids, model_version, prompt_version
		) VALUES ($1,$2,$3,$4,$5, NULL,'[]'::jsonb, NULL,'{}'::jsonb, NULL,'{}', $6,$7)
	`, req.EntityType, req.EntityID, sport, req.TriggerType, triggerJSON,
		a.ollama.Model(), newsAnalysisPromptVersion)
	return err
}

func (a *NewsAnalyzer) persistSummary(ctx context.Context, req NewsAnalysisRequest, sport string, res *NewsAnalysisResult) error {
	triggerJSON, err := json.Marshal(req.Trigger)
	if err != nil {
		return err
	}
	topicsJSON, err := json.Marshal(res.TrendingTopics)
	if err != nil {
		return err
	}
	componentsJSON, err := json.Marshal(res.ImpactComponents)
	if err != nil {
		return err
	}
	newsIDs := res.InputNewsIDs
	if newsIDs == nil {
		newsIDs = []int64{}
	}
	_, err = a.pool.Exec(ctx, `
		INSERT INTO news_summaries (
		    entity_type, entity_id, sport, trigger_type, trigger_payload,
		    summary, trending_topics, impact, impact_components,
		    source_attribution, input_news_ids, model_version, prompt_version
		) VALUES ($1,$2,$3,$4,$5, $6,$7,$8,$9, NULL,$10, $11,$12)
	`, req.EntityType, req.EntityID, sport, req.TriggerType, triggerJSON,
		res.Summary, topicsJSON, res.Impact, componentsJSON,
		newsIDs, res.Model, newsAnalysisPromptVersion)
	return err
}
