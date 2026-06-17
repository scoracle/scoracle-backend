package ml

// Vibe synthesis — the holistic three-pillar vibe score that replaces the
// single-source sentiment as what the product shows on the Vibe card.
//
// Three pillars:
//   P1 — News narrative (transfers-informed): the entity's latest narratives
//        from news_summaries (same source sentiment uses).
//   P2 — Sigil identity: the divined statistical identity from stat_summaries
//        (what kind of player/team this is, its defining strength).
//   P3 — Momentum: recent sentiment trend (slope over last 14 rows in
//        sentiment_scores) + composite trend (slope over last 10 events).
//
// Generated event-driven + debounced (24h window per trigger type), not on a
// schedule. The SkipUnchanged gate hashes the three pillar inputs and skips
// a Gemma call when nothing material changed since the last synthesis.
//
// A no-pillar path (all loaders return empty) persists a marker row
// (score=NULL, blurb=NULL) without a Gemma call.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

const vibeSynthesisPromptVersion = "s1"

// VibeSynthesisRequest describes the entity and the trigger that initiated
// this synthesis run.
type VibeSynthesisRequest struct {
	EntityType    string // 'player' | 'team'
	EntityID      int
	EntityName    string
	Sport         string // 'NBA' | 'NFL' | 'FOOTBALL'
	TriggerType   string // 'narrative_change' | 'sentiment_break' | 'composite_shift' | 'lazy_view' | 'periodic' | 'manual'
	Trigger       map[string]any
	Season        *int
	SkipUnchanged bool // skip Gemma call when input hash matches last row
}

// VibeSynthesisResult is what Generate produces and persists to vibe_synthesis.
type VibeSynthesisResult struct {
	Score            int
	PreviousScore    int // last non-null score before this run; 0 if none
	Blurb            string
	InputComponents  map[string]any
	InputHash        string
	Model            string
	PromptVersion    string
	GeneratedAt      time.Time
	Duration         time.Duration
	SkippedNoPillars bool // all three loaders returned empty
	SkippedUnchanged bool // input hash matched last row
}

// SynthesisGenerator wires Ollama to the Postgres tables for vibe synthesis.
// It is a separate type from Generator (the sentiment generator) to keep
// the two pipelines independent.
type SynthesisGenerator struct {
	pool   *pgxpool.Pool
	ollama *OllamaClient
}

func NewSynthesisGenerator(pool *pgxpool.Pool, ollama *OllamaClient) *SynthesisGenerator {
	return &SynthesisGenerator{pool: pool, ollama: ollama}
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

// Generate loads the three pillar inputs, optionally skips when unchanged,
// asks Gemma for the synthesis, and persists one vibe_synthesis row.
func (s *SynthesisGenerator) Generate(ctx context.Context, req VibeSynthesisRequest) (*VibeSynthesisResult, error) {
	if s.pool == nil {
		return nil, fmt.Errorf("synthesis generator: no db pool")
	}
	if req.EntityID <= 0 || req.EntityName == "" || req.Sport == "" || req.EntityType == "" {
		return nil, fmt.Errorf("synthesis generator: entity context incomplete")
	}
	sport := strings.ToUpper(req.Sport)
	now := time.Now()

	// --- Load the three pillars ---
	narratives, err := s.loadNarrativePillar(ctx, req.EntityType, req.EntityID, sport)
	if err != nil {
		return nil, fmt.Errorf("narrative pillar: %w", err)
	}
	sigilData, err := s.loadSigilPillar(ctx, req.EntityType, req.EntityID, sport, req.Season)
	if err != nil {
		return nil, fmt.Errorf("sigil pillar: %w", err)
	}
	momentum, err := s.loadMomentumPillar(ctx, req.EntityType, req.EntityID, sport, req.Season)
	if err != nil {
		return nil, fmt.Errorf("momentum pillar: %w", err)
	}

	// No-pillar path: persist a marker row without a Gemma call so the read
	// path returns "no synthesis yet" and callers skip until data arrives.
	if len(narratives) == 0 && sigilData == nil && momentum.empty() {
		res := &VibeSynthesisResult{
			SkippedNoPillars: true,
			Model:            s.ollama.Model(),
			PromptVersion:    vibeSynthesisPromptVersion,
			GeneratedAt:      now,
		}
		if err := s.persist(ctx, req, sport, res); err != nil {
			return nil, fmt.Errorf("persist no-pillars marker: %w", err)
		}
		return res, nil
	}

	// Build input components + hash for the debounce gate.
	ic := buildSynthesisInputComponents(narratives, sigilData, momentum)
	hash := hashComponents(ic) // reuse from stat_commentary.go (same package)
	if req.SkipUnchanged {
		last, err := s.lastSynthesisHash(ctx, req.EntityType, req.EntityID, sport)
		if err == nil && last != "" && last == hash {
			return &VibeSynthesisResult{
				SkippedUnchanged: true,
				InputHash:        hash,
				Model:            s.ollama.Model(),
				PromptVersion:    vibeSynthesisPromptVersion,
				GeneratedAt:      now,
			}, nil
		}
	}

	// Load previous score for the delta display on the Vibe card.
	prev, _ := s.lastScore(ctx, req.EntityType, req.EntityID, sport)

	prompt := buildSynthesisPrompt(req, narratives, sigilData, momentum)

	start := time.Now()
	gen, err := s.ollama.Generate(ctx, prompt, GenerateOptions{
		System:      vibeSynthesisSystemPrompt,
		Temperature: 0.6,
		NumPredict:  1000,
	})
	if err != nil {
		return nil, fmt.Errorf("gemma generate: %w", err)
	}
	duration := time.Since(start)

	score, blurb := parseSynthesisResponse(gen.Response)
	if score == 0 {
		return nil, fmt.Errorf("synthesis: could not parse score from response (raw=%q)", truncate(gen.Response, 200))
	}

	res := &VibeSynthesisResult{
		Score:           score,
		PreviousScore:   prev,
		Blurb:           blurb,
		InputComponents: ic,
		InputHash:       hash,
		Model:           gen.Model,
		PromptVersion:   vibeSynthesisPromptVersion,
		GeneratedAt:     now,
		Duration:        duration,
	}
	if err := s.persist(ctx, req, sport, res); err != nil {
		return nil, fmt.Errorf("persist synthesis: %w", err)
	}
	return res, nil
}

// RecentlySynthesized returns true when a synthesis row exists for this entity
// within the given window. Used for debouncing across callers.
func RecentlySynthesized(ctx context.Context, pool *pgxpool.Pool, entityType string, entityID int, sport string, window time.Duration) bool {
	var exists bool
	err := pool.QueryRow(ctx, `
		SELECT EXISTS (
			SELECT 1 FROM vibe_synthesis
			WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
			  AND generated_at > NOW() - $4::interval
		)`, entityType, entityID, sport, window).Scan(&exists)
	return err == nil && exists
}

// ---------------------------------------------------------------------------
// Pillar loaders
// ---------------------------------------------------------------------------

type synthNarrative struct {
	title  string
	body   string
	impact float64
}

type synthSigil struct {
	divinedSigil string
	body         string
	notability   int
}

type synthMomentum struct {
	sentimentSlope  float64 // positive = trending up; derived from last 14 sentiment_scores rows
	compositeSlope  float64 // from last 10 event composite scores
	latestSentiment *int
	latestComposite *float64
}

func (m synthMomentum) empty() bool {
	return m.latestSentiment == nil && m.latestComposite == nil
}

func (s *SynthesisGenerator) loadNarrativePillar(ctx context.Context, entityType string, entityID int, sport string) ([]synthNarrative, error) {
	rows, err := s.pool.Query(ctx, `
		SELECT narrative_title, body, COALESCE(impact, 0)
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
		return nil, err
	}
	defer rows.Close()
	var out []synthNarrative
	for rows.Next() {
		var n synthNarrative
		if err := rows.Scan(&n.title, &n.body, &n.impact); err != nil {
			return nil, err
		}
		out = append(out, n)
	}
	return out, rows.Err()
}

func (s *SynthesisGenerator) loadSigilPillar(ctx context.Context, entityType string, entityID int, sport string, season *int) (*synthSigil, error) {
	var sig synthSigil
	err := s.pool.QueryRow(ctx, `
		SELECT COALESCE(divined_sigil, ''), body, COALESCE(notability, 0)
		FROM stat_summaries
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
		  AND body IS NOT NULL
		  AND ($4::int IS NULL OR season = $4)
		ORDER BY generated_at DESC
		LIMIT 1
	`, entityType, entityID, sport, season).Scan(&sig.divinedSigil, &sig.body, &sig.notability)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	return &sig, nil
}

func (s *SynthesisGenerator) loadMomentumPillar(ctx context.Context, entityType string, entityID int, sport string, season *int) (synthMomentum, error) {
	var m synthMomentum

	// Sentiment trend: last 14 rows from sentiment_scores
	sentRows, err := s.pool.Query(ctx, `
		SELECT sentiment FROM sentiment_scores
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
		  AND sentiment IS NOT NULL
		ORDER BY generated_at DESC
		LIMIT 14
	`, entityType, entityID, sport)
	if err != nil {
		return m, err
	}
	defer sentRows.Close()
	var sentScores []float64
	for sentRows.Next() {
		var v int
		if err := sentRows.Scan(&v); err != nil {
			return m, err
		}
		sentScores = append(sentScores, float64(v))
	}
	if err := sentRows.Err(); err != nil {
		return m, err
	}
	if len(sentScores) > 0 {
		latest := int(sentScores[0])
		m.latestSentiment = &latest
		// Reverse to chronological order for slope calculation
		for i, j := 0, len(sentScores)-1; i < j; i, j = i+1, j-1 {
			sentScores[i], sentScores[j] = sentScores[j], sentScores[i]
		}
		m.sentimentSlope = linearSlope(sentScores)
	}

	// Composite trend: last 10 events from the appropriate event table
	var compTable, idCol string
	switch entityType {
	case "player":
		compTable, idCol = "event_box_scores", "player_id"
	default:
		compTable, idCol = "event_team_stats", "team_id"
	}
	q := fmt.Sprintf(`
		SELECT e.rating_composite_pct FROM public.%s e
			JOIN public.fixtures f ON f.id = e.fixture_id
			WHERE e.%s = $1 AND e.sport = $2
			  AND e.rating_composite_pct IS NOT NULL
			  AND ($3::int IS NULL OR e.season = $3)
			ORDER BY f.start_time DESC
			LIMIT 10
		`, compTable, idCol)
	compRows, err := s.pool.Query(ctx, q, entityID, sport, season)
	if err != nil {
		return m, err
	}
	defer compRows.Close()
	var compScores []float64
	for compRows.Next() {
		var v float64
		if err := compRows.Scan(&v); err != nil {
			return m, err
		}
		compScores = append(compScores, v)
	}
	if err := compRows.Err(); err != nil {
		return m, err
	}
	if len(compScores) > 0 {
		latest := compScores[0]
		m.latestComposite = &latest
		for i, j := 0, len(compScores)-1; i < j; i, j = i+1, j-1 {
			compScores[i], compScores[j] = compScores[j], compScores[i]
		}
		m.compositeSlope = linearSlope(compScores)
	}

	return m, nil
}

// linearSlope computes the slope of a simple OLS regression on the series
// [0..N-1] → values. Positive = trending up.
func linearSlope(vals []float64) float64 {
	n := float64(len(vals))
	if n < 2 {
		return 0
	}
	var sumX, sumY, sumXY, sumXX float64
	for i, v := range vals {
		x := float64(i)
		sumX += x
		sumY += v
		sumXY += x * v
		sumXX += x * x
	}
	denom := n*sumXX - sumX*sumX
	if math.Abs(denom) < 1e-9 {
		return 0
	}
	return (n*sumXY - sumX*sumY) / denom
}

// ---------------------------------------------------------------------------
// Input components + hash
// ---------------------------------------------------------------------------

func buildSynthesisInputComponents(narratives []synthNarrative, sigil *synthSigil, mom synthMomentum) map[string]any {
	out := map[string]any{}

	titles := make([]string, len(narratives))
	for i, n := range narratives {
		titles[i] = n.title
	}
	sort.Strings(titles)
	out["narrative_titles"] = titles

	if sigil != nil {
		out["divined_sigil"] = sigil.divinedSigil
		out["notability"] = sigil.notability
	}
	if mom.latestSentiment != nil {
		out["latest_sentiment"] = *mom.latestSentiment
	}
	if mom.latestComposite != nil {
		out["latest_composite"] = round1(*mom.latestComposite)
	}
	return out
}

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

const vibeSynthesisSystemPrompt = `You are a holistic sports analyst synthesizing three signals — news narrative, statistical identity, and momentum — into a single VIBE score and a short blurb.

The vibe is SLOW-MOVING and SEASON-AWARE: it reflects the entity's whole-season arc, not a single game.

Rules:
- Weigh all three signals. One weak signal does not override the others.
- The score is 1-100: 1 = deeply troubled/in freefall, 50 = neutral/steady, 100 = dominant/surging.
- The blurb is 1-2 sentences of plain prose: what STORY this entity is telling right now. No headlines, no bullet points.
- Respond on EXACTLY two lines:
    SCORE: <integer>
    BLURB: <1-2 sentences>
- No other text, no preamble, no explanation.`

func buildSynthesisPrompt(req VibeSynthesisRequest, narratives []synthNarrative, sigil *synthSigil, mom synthMomentum) string {
	var b strings.Builder

	header := fmt.Sprintf("%s %s", req.Sport, req.EntityType)
	b.WriteString(fmt.Sprintf("Entity: %s (%s)\n", req.EntityName, header))

	// P1 — News narrative
	if len(narratives) > 0 {
		b.WriteString("\n=== NEWS NARRATIVE ===\n")
		for _, n := range narratives {
			b.WriteString(fmt.Sprintf("[impact %.0f] %s\n%s\n\n", n.impact, n.title, n.body))
		}
	} else {
		b.WriteString("\n=== NEWS NARRATIVE ===\n(no recent narratives)\n")
	}

	// P2 — Sigil identity
	b.WriteString("\n=== SIGIL IDENTITY (how the entity performs, not how well) ===\n")
	if sigil != nil {
		if sigil.divinedSigil != "" {
			b.WriteString(fmt.Sprintf("Sigil: %s (notability %d/100)\n", sigil.divinedSigil, sigil.notability))
		}
		if sigil.body != "" {
			b.WriteString(sigil.body + "\n")
		}
	} else {
		b.WriteString("(no stat commentary available)\n")
	}

	// P3 — Momentum
	b.WriteString("\n=== MOMENTUM ===\n")
	if mom.latestSentiment != nil {
		dir := trendDir(mom.sentimentSlope)
		b.WriteString(fmt.Sprintf("News sentiment: %d/100 (%s)\n", *mom.latestSentiment, dir))
	}
	if mom.latestComposite != nil {
		dir := trendDir(mom.compositeSlope)
		b.WriteString(fmt.Sprintf("Composite rating: %.1f/100 (%s)\n", *mom.latestComposite, dir))
	}
	if mom.latestSentiment == nil && mom.latestComposite == nil {
		b.WriteString("(no momentum data)\n")
	}

	b.WriteString("\nRespond now.")
	return b.String()
}

func trendDir(slope float64) string {
	switch {
	case slope > 1.5:
		return "trending up strongly"
	case slope > 0.3:
		return "trending up"
	case slope < -1.5:
		return "trending down strongly"
	case slope < -0.3:
		return "trending down"
	default:
		return "steady"
	}
}

// parseSynthesisResponse extracts SCORE and BLURB from Gemma's two-line reply.
func parseSynthesisResponse(raw string) (score int, blurb string) {
	lines := strings.Split(strings.TrimSpace(raw), "\n")
	for i, line := range lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "SCORE: ") {
			val := strings.TrimSpace(strings.TrimPrefix(trimmed, "SCORE: "))
			if n, err := strconv.Atoi(val); err == nil {
				if n < 1 {
					n = 1
				} else if n > 100 {
					n = 100
				}
				score = n
			}
		} else if strings.HasPrefix(trimmed, "BLURB: ") {
			blurb = strings.TrimSpace(strings.TrimPrefix(trimmed, "BLURB: "))
			// Absorb continuation lines
			for _, extra := range lines[i+1:] {
				extra = strings.TrimSpace(extra)
				if extra != "" {
					blurb += " " + extra
				}
			}
			break
		}
	}
	return
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

func (s *SynthesisGenerator) persist(ctx context.Context, req VibeSynthesisRequest, sport string, res *VibeSynthesisResult) error {
	triggerJSON, err := json.Marshal(orEmptyMap(req.Trigger))
	if err != nil {
		return err
	}
	icomp, err := json.Marshal(orEmptyMap(res.InputComponents))
	if err != nil {
		return err
	}

	var score, prevScore, blurb, inputHash any
	if !res.SkippedNoPillars {
		score = res.Score
		if res.PreviousScore > 0 {
			prevScore = res.PreviousScore
		}
		blurb = res.Blurb
	}
	if res.InputHash != "" {
		inputHash = res.InputHash
	}

	_, err = s.pool.Exec(ctx, `
		INSERT INTO vibe_synthesis (
		    entity_type, entity_id, sport, season, trigger_type, trigger_payload,
		    score, previous_score, blurb, input_components, input_hash,
		    model_version, prompt_version, generated_at
		) VALUES ($1,$2,$3,$4,$5,$6, $7,$8,$9,$10,$11, $12,$13,$14)`,
		req.EntityType, req.EntityID, sport, req.Season, req.TriggerType, triggerJSON,
		score, prevScore, blurb, icomp, inputHash,
		res.Model, vibeSynthesisPromptVersion, res.GeneratedAt)
	return err
}

func (s *SynthesisGenerator) lastSynthesisHash(ctx context.Context, entityType string, entityID int, sport string) (string, error) {
	var hash *string
	err := s.pool.QueryRow(ctx, `
		SELECT input_hash FROM vibe_synthesis
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
		  AND score IS NOT NULL
		ORDER BY generated_at DESC
		LIMIT 1`, entityType, entityID, sport).Scan(&hash)
	if errors.Is(err, pgx.ErrNoRows) {
		return "", nil
	}
	if err != nil {
		return "", err
	}
	if hash == nil {
		return "", nil
	}
	return *hash, nil
}

func (s *SynthesisGenerator) lastScore(ctx context.Context, entityType string, entityID int, sport string) (int, error) {
	var score *int
	err := s.pool.QueryRow(ctx, `
		SELECT score FROM vibe_synthesis
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
		  AND score IS NOT NULL
		ORDER BY generated_at DESC
		LIMIT 1`, entityType, entityID, sport).Scan(&score)
	if errors.Is(err, pgx.ErrNoRows) || score == nil {
		return 0, nil
	}
	return *score, err
}
