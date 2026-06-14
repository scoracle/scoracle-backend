package ml

// News narratives — Stage 2 of the staged Gemma pipeline
// (raw → scrub → NARRATIVES → transfer heat → vibe). Reads an entity's VETTED
// news corpus and asks Gemma to group it into the DISTINCT storylines forming
// around the entity, writing each up as its own narrative. Each narrative carries
// a DETERMINISTIC per-narrative impact (volume × sources × recency over ITS
// articles, computeNewsImpact) that ranks the news leaderboard.
//
// Append model: one news_summaries row per narrative, all sharing a generated_at
// (a "generation"; migration 085). Supersedes the single-summary one-call
// ml/news_analysis.go — sentiment is no longer produced here; it moves to the
// vibe stage, which runs LAST over these narratives + transfer heat.
//
// See vault wiki/Plan - News pipeline integration.md (Gemma stage progression).

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
const newsNarrativesPromptVersion = "n1"

// maxNarrativeCorpus bounds the articles fed to the grouping prompt. Wider than
// the vibe/analysis window (maxNewsItems) so Gemma can see enough breadth to find
// the distinct storylines, but still bounded to keep the prompt + latency sane.
const maxNarrativeCorpus = 25

// Narrative is one storyline Gemma found in the corpus, grounded in the articles
// it cited (InputNewsIDs) with a deterministic per-narrative impact.
type Narrative struct {
	Title            string
	Body             string
	Impact           int
	ImpactComponents map[string]any
	InputNewsIDs     []int64
}

// NarrativesRequest describes the entity whose recent news to narrate.
type NarrativesRequest struct {
	EntityType  string // 'player' | 'team'
	EntityID    int
	EntityName  string
	Sport       string         // 'NBA' | 'NFL' | 'FOOTBALL'
	TriggerType string         // 'news_spike' | 'periodic' | 'manual'
	Trigger     map[string]any // optional context
	// DryRun skips persistence (used to verify Gemma output). The corpus is
	// still read + grouped + scored.
	DryRun bool
}

// NarrativesResult is what Generate produces. GeneratedAt is shared across all
// narratives of this run (the generation key). SkippedNoCorpus is true when there
// was no relevant news this cycle (a NULL-narrative marker row is persisted).
type NarrativesResult struct {
	Narratives      []Narrative
	SkippedNoCorpus bool
	Model           string
	PromptVersion   string
	GeneratedAt     time.Time
	Duration        time.Duration
}

// NewsNarrator wires Ollama to the Postgres news corpus.
type NewsNarrator struct {
	pool   *pgxpool.Pool
	ollama *OllamaClient
}

func NewNewsNarrator(pool *pgxpool.Pool, ollama *OllamaClient) *NewsNarrator {
	return &NewsNarrator{pool: pool, ollama: ollama}
}

// Generate loads the entity's vetted recent news, asks Gemma to group it into the
// distinct narratives (each with a write-up + the articles it used), computes a
// deterministic impact per narrative, and persists one news_summaries row each.
func (a *NewsNarrator) Generate(ctx context.Context, req NarrativesRequest) (*NarrativesResult, error) {
	if a.pool == nil {
		return nil, fmt.Errorf("news narrator: no db pool")
	}
	if req.EntityID <= 0 || req.EntityName == "" || req.Sport == "" || req.EntityType == "" {
		return nil, fmt.Errorf("news narrator: entity context incomplete")
	}
	sport := strings.ToUpper(req.Sport)
	now := time.Now()

	news, err := a.loadVettedCorpus(ctx, req.EntityType, req.EntityID, sport)
	if err != nil {
		return nil, fmt.Errorf("load news: %w", err)
	}

	// No corpus → persist a NULL-narrative marker so the read path returns an
	// explicit "no news" and the debounce skips until the corpus changes.
	if len(news) == 0 {
		res := &NarrativesResult{
			SkippedNoCorpus: true,
			Model:           a.ollama.Model(),
			PromptVersion:   newsNarrativesPromptVersion,
			GeneratedAt:     now,
		}
		if !req.DryRun {
			if err := a.persist(ctx, req, sport, res); err != nil {
				return nil, fmt.Errorf("persist no-corpus marker: %w", err)
			}
		}
		return res, nil
	}

	prompt := buildNarrativesPrompt(req, news)

	start := time.Now()
	gen, err := a.ollama.Generate(ctx, prompt, GenerateOptions{
		System:      newsNarrativesSystemPrompt,
		Temperature: 0.6,
		// Several narratives, each a multi-sentence write-up, on top of Gemma 4's
		// internal reasoning budget — give it real room. The prompt caps the count
		// and body length; the tolerant parser salvages a truncated tail regardless.
		NumPredict: 4000,
	})
	if err != nil {
		return nil, fmt.Errorf("gemma generate: %w", err)
	}
	duration := time.Since(start)

	parsed, ok := parseNarratives(gen.Response)
	if !ok {
		return nil, fmt.Errorf("parse narratives failed (raw=%q prompt_len=%d)",
			truncate(gen.Response, 200), len(prompt))
	}

	narratives := groundNarratives(parsed, news)
	if len(narratives) == 0 {
		// Gemma returned no usable, article-grounded narrative — treat as no-corpus
		// rather than persisting empty rows.
		res := &NarrativesResult{
			SkippedNoCorpus: true,
			Model:           gen.Model,
			PromptVersion:   newsNarrativesPromptVersion,
			GeneratedAt:     now,
			Duration:        duration,
		}
		if !req.DryRun {
			if err := a.persist(ctx, req, sport, res); err != nil {
				return nil, fmt.Errorf("persist no-corpus marker: %w", err)
			}
		}
		return res, nil
	}

	res := &NarrativesResult{
		Narratives:    narratives,
		Model:         gen.Model,
		PromptVersion: newsNarrativesPromptVersion,
		GeneratedAt:   now,
		Duration:      duration,
	}
	if !req.DryRun {
		if err := a.persist(ctx, req, sport, res); err != nil {
			return nil, fmt.Errorf("persist narratives: %w", err)
		}
	}
	return res, nil
}

// loadVettedCorpus reads the entity's recent VETTED news links (the scrub gate:
// keep what Gemma confirmed genuine + any not yet scrubbed). Mirrors the corpus
// query in news_analysis.go / vibe.go, wider (maxNarrativeCorpus) so Gemma sees
// enough breadth to find the distinct storylines.
func (a *NewsNarrator) loadVettedCorpus(
	ctx context.Context, entityType string, entityID int, sport string,
) ([]newsItem, error) {
	rows, err := a.pool.Query(ctx, `
		SELECT a.id, a.title, COALESCE(a.description, ''), COALESCE(a.source, ''), a.published_at
		FROM news_article_entities nae
		JOIN news_articles a ON a.id = nae.article_id
		WHERE nae.entity_type = $1 AND nae.entity_id = $2 AND nae.sport = $3
		  AND (nae.vetted IS TRUE OR nae.scrubbed_at IS NULL)
		  AND (a.published_at IS NULL OR a.published_at > NOW() - $4::interval)
		ORDER BY COALESCE(a.published_at, a.fetched_at) DESC
		LIMIT $5
	`, entityType, entityID, sport, fmt.Sprintf("%d seconds", int(newsLookback.Seconds())), maxNarrativeCorpus)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []newsItem
	for rows.Next() {
		var n newsItem
		if err := rows.Scan(&n.id, &n.title, &n.description, &n.source, &n.publishedAt); err != nil {
			return nil, err
		}
		out = append(out, n)
	}
	return out, rows.Err()
}

// ---------------------------------------------------------------------------
// Grounding: map Gemma's article numbers back to the corpus, compute per-narrative
// impact, keep only narratives with a title, a body, and >= 1 valid article.
// ---------------------------------------------------------------------------

func groundNarratives(parsed gemmaNarratives, news []newsItem) []Narrative {
	out := make([]Narrative, 0, len(parsed.Narratives))
	for _, p := range parsed.Narratives {
		title := strings.TrimSpace(p.Title)
		body := strings.TrimSpace(p.Body)
		if title == "" || body == "" {
			continue
		}
		// Article numbers are 1-indexed in the prompt; dedupe + bound to range.
		seen := make(map[int]struct{}, len(p.Articles))
		var subset []newsItem
		var ids []int64
		for _, num := range p.Articles {
			idx := num - 1
			if idx < 0 || idx >= len(news) {
				continue
			}
			if _, dup := seen[idx]; dup {
				continue
			}
			seen[idx] = struct{}{}
			subset = append(subset, news[idx])
			ids = append(ids, news[idx].id)
		}
		if len(subset) == 0 {
			continue // ungrounded — can't score or trace it
		}
		impact, components := computeNewsImpact(subset)
		out = append(out, Narrative{
			Title:            title,
			Body:             body,
			Impact:           impact,
			ImpactComponents: components,
			InputNewsIDs:     ids,
		})
	}
	return out
}

// ---------------------------------------------------------------------------
// Deterministic per-narrative impact (0-100) — the news analog of transfer heat,
// computed from a narrative's articles (NOT from Gemma) and stored with
// components for transparency. volume (count) + corroboration (distinct sources)
// + recency. Ranks the news leaderboard.
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

const newsNarrativesSystemPrompt = `You read recent NEWS about one sports entity and identify the DISTINCT STORYLINES (narratives) forming around it. Return STRICT JSON only — one object, no markdown fences, no text before or after:
{"narratives": [{"title": "<short headline>", "body": "<write-up>", "articles": [<article numbers>]}, ...]}

Group the numbered articles into the distinct narratives they form. A narrative is one coherent storyline — a specific transfer/trade saga, a managerial/coaching search, an injury situation, a results run, a contract dispute, etc. A busy week has several; a quiet week may have just one. Do NOT split one story into multiple narratives, and do NOT merge unrelated stories.

For each narrative:
- title: a short, specific headline that NAMES the key people/clubs (e.g. "Cucurella to Real Madrid saga", "Managerial search after Maresca exit"). Never generic like "Transfer news".
- body: an original-prose write-up of that storyline — what is happening, who is involved (use the REAL names of players, managers, executives, and other clubs from the articles; never genericize to "a Real Madrid star"), and where it stands. One to three sentences; longer only when the story is genuinely big.
- articles: the numbers of the articles that belong to this narrative.

Return AT MOST 6 narratives, ordered MOST IMPACTFUL first. Keep each body to 1-3 sentences (a genuinely big story may use up to 4).

Rules: write original prose (do NOT quote headlines verbatim, no URLs, no source-name dumps); never invent facts not present in the articles; ignore any article clearly not about this entity.`

func buildNarrativesPrompt(req NarrativesRequest, news []newsItem) string {
	var b strings.Builder
	b.WriteString(fmt.Sprintf("Entity: %s (%s %s)\n", req.EntityName, req.Sport, req.EntityType))
	b.WriteString("\nRecent news (numbered):\n")
	for i, n := range news {
		b.WriteString(fmt.Sprintf("%d. ", i+1))
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

type gemmaNarrative struct {
	Title    string `json:"title"`
	Body     string `json:"body"`
	Articles []int  `json:"articles"`
}

type gemmaNarratives struct {
	Narratives []gemmaNarrative `json:"narratives"`
}

// parseNarratives salvages each complete narrative object from Gemma's response
// independently, rather than requiring the whole document to be well-formed. LLM
// output length is non-deterministic, so a multi-narrative reply can truncate
// mid-array or carry one malformed object; this scanner extracts every balanced
// top-level {...} inside the "narratives" array (respecting strings/escapes),
// unmarshals each on its own, and keeps the ones that parse. A truncated tail
// just drops its last incomplete object. Succeeds when >= 1 narrative survives.
func parseNarratives(raw string) (gemmaNarratives, bool) {
	var out gemmaNarratives
	key := strings.Index(raw, `"narratives"`)
	if key < 0 {
		return out, false
	}
	lb := strings.IndexByte(raw[key:], '[')
	if lb < 0 {
		return out, false
	}
	s := raw[key+lb+1:] // just after the array's '['

	depth, start := 0, -1
	inStr, esc := false, false
	for i := 0; i < len(s); i++ {
		c := s[i]
		if inStr {
			switch {
			case esc:
				esc = false
			case c == '\\':
				esc = true
			case c == '"':
				inStr = false
			}
			continue
		}
		switch c {
		case '"':
			inStr = true
		case '{':
			if depth == 0 {
				start = i
			}
			depth++
		case '}':
			if depth > 0 {
				depth--
				if depth == 0 && start >= 0 {
					var n gemmaNarrative
					if err := json.Unmarshal([]byte(s[start:i+1]), &n); err == nil {
						out.Narratives = append(out.Narratives, n)
					}
					start = -1
				}
			}
		case ']':
			if depth == 0 {
				return out, len(out.Narratives) > 0 // end of the array
			}
		}
	}
	return out, len(out.Narratives) > 0
}

// ---------------------------------------------------------------------------
// Persist — one row per narrative, all sharing res.GeneratedAt (a generation).
// ---------------------------------------------------------------------------

func (a *NewsNarrator) persist(ctx context.Context, req NarrativesRequest, sport string, res *NarrativesResult) error {
	triggerJSON, err := json.Marshal(req.Trigger)
	if err != nil {
		return err
	}

	tx, err := a.pool.Begin(ctx)
	if err != nil {
		return err
	}
	defer tx.Rollback(ctx)

	const insert = `
		INSERT INTO news_summaries (
		    entity_type, entity_id, sport, trigger_type, trigger_payload,
		    narrative_title, body, impact, impact_components,
		    source_attribution, input_news_ids, model_version, prompt_version, generated_at
		) VALUES ($1,$2,$3,$4,$5, $6,$7,$8,$9, NULL,$10, $11,$12,$13)`

	if len(res.Narratives) == 0 {
		// No-narratives marker row.
		if _, err := tx.Exec(ctx, insert,
			req.EntityType, req.EntityID, sport, req.TriggerType, triggerJSON,
			nil, nil, nil, []byte("{}"),
			[]int64{}, res.Model, newsNarrativesPromptVersion, res.GeneratedAt); err != nil {
			return err
		}
	} else {
		for _, n := range res.Narratives {
			componentsJSON, err := json.Marshal(n.ImpactComponents)
			if err != nil {
				return err
			}
			ids := n.InputNewsIDs
			if ids == nil {
				ids = []int64{}
			}
			if _, err := tx.Exec(ctx, insert,
				req.EntityType, req.EntityID, sport, req.TriggerType, triggerJSON,
				n.Title, n.Body, n.Impact, componentsJSON,
				ids, res.Model, newsNarrativesPromptVersion, res.GeneratedAt); err != nil {
				return err
			}
		}
	}
	return tx.Commit(ctx)
}
