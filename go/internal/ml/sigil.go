package ml

// Sigil synthesis — the holistic three-pillar score that replaces the
// single-source sentiment as what the product shows on the Sigil card.
//
// Three pillars:
//   P1 — News narrative (transfers-informed): the entity's latest narratives
//        from news_summaries (same source sentiment uses).
//   P2 — Sigil identity: the divined statistical identity from stat_summaries
//        (what kind of player/team this is, its defining strength).
//   P3 — Momentum: recent sentiment trend (slope over last 14 rows in
//        vibe_scores) + composite trend (slope over last 10 events).
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

// s1: original three-pillar synthesis (narrative / rating identity / momentum).
// s2: consumes the Vibe end product's felt-read PROMPT (vibe_scores.prompt) as
//
//	the emotional signal's headline — Sigil = f(Vibe, Rating, Momentum).
//
// s4: the stat-identity pillar's divined-strength label de-sigiled — read from
//
//	stat_summaries.divined_peak (was divined_sigil), the input-component key is
//	"divined_peak", and the P2 prompt section is relabeled PEAK. Pre-s4 rows are
//	re-stamped to the new input_hash by `vibesynth --restamp-vocab` (no Gemma
//	re-synthesis; existing scores/blurbs preserved).
//
// s5: re-aimed for Mistral (L8) as the beat reporter's "final word" — the blurb now
//
//	synthesizes ALL THREE pillars (identity + news + momentum), not just the loudest
//	news thread, in plain grounded prose (no purple, no headline, ~2 sentences). Only
//	the prompt text changed; input_components/hash are unchanged, so no restamp is
//	needed (existing rows keep their score/blurb; new generations stamp s5).
const sigilPromptVersion = "s5"

// SigilRequest describes the entity and the trigger that initiated
// this synthesis run.
type SigilRequest struct {
	EntityType    string // 'player' | 'team'
	EntityID      int
	EntityName    string
	Sport         string // 'NBA' | 'NFL' | 'FOOTBALL'
	TriggerType   string // 'narrative_change' | 'sentiment_break' | 'composite_shift' | 'lazy_view' | 'periodic' | 'manual'
	Trigger       map[string]any
	Season        *int // nil ⇒ the sport's current_season (resolved + stamped in Generate)
	SkipUnchanged bool // skip Gemma call when input hash matches last row
	DryRun        bool // load + score but never persist (matches RatingRequest.DryRun)
}

// SigilResult is what Generate produces and persists to sigil_synthesis.
type SigilResult struct {
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

// SigilGenerator wires Ollama to the Postgres tables for vibe synthesis.
// It is a separate type from VibeGenerator (the sentiment generator) to keep
// the two pipelines independent.
type SigilGenerator struct {
	pool   *pgxpool.Pool
	ollama *OllamaClient
}

func NewSigilGenerator(pool *pgxpool.Pool, ollama *OllamaClient) *SigilGenerator {
	return &SigilGenerator{pool: pool, ollama: ollama}
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

// Generate loads the three pillar inputs, optionally skips when unchanged,
// asks Gemma for the synthesis, and persists one sigil_synthesis row.
func (s *SigilGenerator) Generate(ctx context.Context, req SigilRequest) (*SigilResult, error) {
	if s.pool == nil {
		return nil, fmt.Errorf("synthesis generator: no db pool")
	}
	if req.EntityID <= 0 || req.EntityName == "" || req.Sport == "" || req.EntityType == "" {
		return nil, fmt.Errorf("synthesis generator: entity context incomplete")
	}
	sport := strings.ToUpper(req.Sport)
	now := time.Now()

	// Resolve the season this convergence is for and STAMP it on req so every
	// persisted row carries a concrete season (never NULL) — the basis for the
	// season-scoped /sigil + crown board reads (FIRST-GPT-AUDIT Session 12). A
	// real-time/manual trigger (Season nil) targets current_season; an explicit
	// season (backfill) is honored as-is. The three pillars load season-exact.
	season, err := s.resolveSeason(ctx, sport, req.Season)
	if err != nil {
		return nil, err
	}
	req.Season = &season

	// --- Load the three pillars (season-exact) ---
	narratives, err := s.loadNarrativePillar(ctx, req.EntityType, req.EntityID, sport)
	if err != nil {
		return nil, fmt.Errorf("narrative pillar: %w", err)
	}
	ratingData, err := s.loadRatingPillar(ctx, req.EntityType, req.EntityID, sport, &season)
	if err != nil {
		return nil, fmt.Errorf("rating pillar: %w", err)
	}
	momentum, err := s.loadMomentumPillar(ctx, req.EntityType, req.EntityID, sport, &season)
	if err != nil {
		return nil, fmt.Errorf("momentum pillar: %w", err)
	}

	// No-pillar path: persist a marker row without a Gemma call so the read
	// path returns "no synthesis yet" and callers skip until data arrives.
	if len(narratives) == 0 && ratingData == nil && momentum.empty() {
		res := &SigilResult{
			SkippedNoPillars: true,
			Model:            s.ollama.Model(),
			PromptVersion:    sigilPromptVersion,
			GeneratedAt:      now,
		}
		if !req.DryRun {
			if err := s.persist(ctx, req, sport, res); err != nil {
				return nil, fmt.Errorf("persist no-pillars marker: %w", err)
			}
		}
		return res, nil
	}

	// Build input components + hash for the debounce gate.
	ic := buildSynthesisInputComponents(narratives, ratingData, momentum)
	hash := hashComponents(ic) // reuse from stat_commentary.go (same package)
	if req.SkipUnchanged {
		last, err := s.lastSynthesisHash(ctx, req.EntityType, req.EntityID, sport, season)
		if err == nil && last != "" && last == hash {
			return &SigilResult{
				SkippedUnchanged: true,
				InputHash:        hash,
				Model:            s.ollama.Model(),
				PromptVersion:    sigilPromptVersion,
				GeneratedAt:      now,
			}, nil
		}
	}

	// Load previous score for the delta display on the Sigil card.
	prev, _ := s.lastScore(ctx, req.EntityType, req.EntityID, sport, season)

	prompt := buildSynthesisPrompt(req, narratives, ratingData, momentum)

	start := time.Now()
	gen, err := s.ollama.Generate(ctx, prompt, GenerateOptions{
		System:      sigilSystemPrompt,
		Temperature: 0.6,
		NumPredict:  1000,
		Op:          "sigil",
	})
	if err != nil {
		return nil, fmt.Errorf("gemma generate: %w", err)
	}
	duration := time.Since(start)

	score, blurb := parseSynthesisResponse(gen.Response)
	if score == 0 {
		return nil, fmt.Errorf("synthesis: could not parse score from response (raw=%q)", truncate(gen.Response, 200))
	}

	res := &SigilResult{
		Score:           score,
		PreviousScore:   prev,
		Blurb:           blurb,
		InputComponents: ic,
		InputHash:       hash,
		Model:           gen.Model,
		PromptVersion:   sigilPromptVersion,
		GeneratedAt:     now,
		Duration:        duration,
	}
	if !req.DryRun {
		if err := s.persist(ctx, req, sport, res); err != nil {
			return nil, fmt.Errorf("persist synthesis: %w", err)
		}
	}
	return res, nil
}

// resolveSeason returns the concrete season this synthesis is for: the caller's
// explicit season when given, else the sport's current_season. Real-time and
// manual convergence (Season nil) always target the current season; historical
// seasons are only (re)synthesized by an explicit-season backfill. Stamping a
// concrete season on every row — never NULL — is what lets /sigil and the crown
// board scope by season (FIRST-GPT-AUDIT Session 12).
func (s *SigilGenerator) resolveSeason(ctx context.Context, sport string, want *int) (int, error) {
	if want != nil {
		return *want, nil
	}
	var cur int
	if err := s.pool.QueryRow(ctx,
		`SELECT current_season FROM public.sports WHERE id = $1`, sport).Scan(&cur); err != nil {
		return 0, fmt.Errorf("resolve current_season for %s: %w", sport, err)
	}
	return cur, nil
}

// ---------------------------------------------------------------------------
// Pillar loaders
// ---------------------------------------------------------------------------

type synthNarrative struct {
	title  string
	body   string
	impact float64
}

type synthRating struct {
	divinedPeak string
	body        string
	notability  int
}

type synthMomentum struct {
	sentimentSlope   float64 // positive = trending up; derived from last 14 vibe_scores rows
	compositeSlope   float64 // from last 10 event composite scores
	latestSentiment  *int
	latestComposite  *float64
	latestVibePrompt string // the Vibe end product's felt read (latest vibe_scores.prompt)
}

func (m synthMomentum) empty() bool {
	return m.latestSentiment == nil && m.latestComposite == nil
}

func (s *SigilGenerator) loadNarrativePillar(ctx context.Context, entityType string, entityID int, sport string) ([]synthNarrative, error) {
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

func (s *SigilGenerator) loadRatingPillar(ctx context.Context, entityType string, entityID int, sport string, season *int) (*synthRating, error) {
	var divinedPeak string
	var body *string
	var notability int
	// Canonical latest-generation rule (FIRST-GPT-AUDIT Session 11 / F-023): take the
	// entity-season's LATEST commentary regardless of nullability (no `body IS NOT NULL`
	// pre-filter), then suppress the pillar if that latest generation is a no-stats
	// marker (body NULL) — never fall back to an older real commentary that a marker has
	// superseded.
	err := s.pool.QueryRow(ctx, `
		SELECT COALESCE(divined_peak, ''), body, COALESCE(notability, 0)
		FROM stat_summaries
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
		  AND ($4::int IS NULL OR season = $4)
		ORDER BY generated_at DESC
		LIMIT 1
	`, entityType, entityID, sport, season).Scan(&divinedPeak, &body, &notability)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	if body == nil {
		return nil, nil // latest generation is a marker → pillar suppressed
	}
	return &synthRating{divinedPeak: divinedPeak, body: *body, notability: notability}, nil
}

func (s *SigilGenerator) loadMomentumPillar(ctx context.Context, entityType string, entityID int, sport string, season *int) (synthMomentum, error) {
	var m synthMomentum

	// Sentiment trend: last 14 rows from vibe_scores. Also capture the latest
	// row's felt-read prompt — the Vibe end product's prose, fed into the Sigil.
	sentRows, err := s.pool.Query(ctx, `
		SELECT sentiment, COALESCE(prompt, '') FROM vibe_scores
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
		var p string
		if err := sentRows.Scan(&v, &p); err != nil {
			return m, err
		}
		if len(sentScores) == 0 {
			m.latestVibePrompt = p // first row is the most recent (DESC)
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

func buildSynthesisInputComponents(narratives []synthNarrative, rating *synthRating, mom synthMomentum) map[string]any {
	out := map[string]any{}

	titles := make([]string, len(narratives))
	for i, n := range narratives {
		titles[i] = n.title
	}
	sort.Strings(titles)
	out["narrative_titles"] = titles

	if rating != nil {
		// Input-component key (de-sigiled s4). This string is hashed into input_hash
		// (the synthesis debounce gate), so renaming it changes every entity's hash.
		// Pre-s4 rows were re-stamped to this key by `vibesynth -mode restamp` so the
		// next run still skips (no spurious re-synthesis).
		out["divined_peak"] = rating.divinedPeak
		out["notability"] = rating.notability
	}
	if mom.latestSentiment != nil {
		out["latest_sentiment"] = *mom.latestSentiment
	}
	if mom.latestComposite != nil {
		out["latest_composite"] = round1(*mom.latestComposite)
	}
	if mom.latestVibePrompt != "" {
		out["latest_vibe_prompt"] = mom.latestVibePrompt
	}
	return out
}

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

const sigilSystemPrompt = `You are the seasoned beat reporter delivering the final word on this entity — invested and plugged-in, you know all its storylines and you do not hide your well-informed read, but you stay measured and honest. You are given three signals to synthesize into a single SIGIL score and a blurb: the NEWS NARRATIVE forming around it, its STATISTICAL IDENTITY (what it is best at), and its MOMENTUM (where sentiment and form are trending). This is the culmination — the whole picture.

SCORE (1-100): the entity's overall standing right now — 1 deeply troubled or in freefall, 50 steady or genuinely mixed, 100 dominant or surging. It is SLOW-MOVING and SEASON-AWARE: the whole-season arc, not one game. Weigh all three signals; one weak signal does not override the others.

BLURB: the story this entity is telling right now, in the reporter's voice — plain, grounded prose, never a headline and never flowery (no "precipice", "talisman", "pastures new", "shadow over his tenure"). In about TWO sentences (a third only when a lot genuinely converges), synthesize all three signals: capture who they ARE in a quick phrase ("an elite creative engine", "a dominant rim protector" — do NOT recite percentiles or per-36 detail, that is the stats card's job), the news storyline that defines the moment, and which way they are trending. Be specific and name the real storyline, but do not catalogue every rumor or list every draft pick. Every word earns its place — no filler, no purple prose, no headline.

Reply with exactly these two lines:
SCORE: <integer 1-100>
BLURB: <the story>`

func buildSynthesisPrompt(req SigilRequest, narratives []synthNarrative, rating *synthRating, mom synthMomentum) string {
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

	// P2 — Rating identity (the stat end product)
	b.WriteString("\n=== PEAK IDENTITY (how the entity performs, not how well) ===\n")
	if rating != nil {
		if rating.divinedPeak != "" {
			b.WriteString(fmt.Sprintf("Peak: %s (notability %d/100)\n", rating.divinedPeak, rating.notability))
		}
		if rating.body != "" {
			b.WriteString(rating.body + "\n")
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
	if mom.latestVibePrompt != "" {
		b.WriteString(fmt.Sprintf("Vibe (the felt read): %s\n", mom.latestVibePrompt))
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

func (s *SigilGenerator) persist(ctx context.Context, req SigilRequest, sport string, res *SigilResult) error {
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
		INSERT INTO sigil_synthesis (
		    entity_type, entity_id, sport, season, trigger_type, trigger_payload,
		    score, previous_score, blurb, input_components, input_hash,
		    model_version, prompt_version, generated_at
		) VALUES ($1,$2,$3,$4,$5,$6, $7,$8,$9,$10,$11, $12,$13,$14)`,
		req.EntityType, req.EntityID, sport, req.Season, req.TriggerType, triggerJSON,
		score, prevScore, blurb, icomp, inputHash,
		res.Model, sigilPromptVersion, res.GeneratedAt)
	return err
}

// lastSynthesisHash returns the input_hash of the entity-season's LATEST synthesis
// generation (the regen-debounce key). Canonical latest-generation rule (Session 11
// / F-023): take the latest row regardless of nullability and season-scoped; a marker
// (no-pillar) row has a NULL input_hash → "" → the next run never wrongly skips, so a
// marker propagates into convergence instead of comparing against a stale scored gen.
func (s *SigilGenerator) lastSynthesisHash(ctx context.Context, entityType string, entityID int, sport string, season int) (string, error) {
	var hash *string
	err := s.pool.QueryRow(ctx, `
		SELECT input_hash FROM sigil_synthesis
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4
		ORDER BY generated_at DESC
		LIMIT 1`, entityType, entityID, sport, season).Scan(&hash)
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

// lastScore returns the previous_score baseline for the delta display: the
// entity-season's LATEST generation's score. Canonical rule (Session 11 / F-023):
// if the latest generation is a marker (score NULL) the current crown is cleared,
// so the baseline is 0 — do not surface the delta against a pre-marker score.
func (s *SigilGenerator) lastScore(ctx context.Context, entityType string, entityID int, sport string, season int) (int, error) {
	var score *int
	err := s.pool.QueryRow(ctx, `
		SELECT score FROM sigil_synthesis
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4
		ORDER BY generated_at DESC
		LIMIT 1`, entityType, entityID, sport, season).Scan(&score)
	if errors.Is(err, pgx.ErrNoRows) || score == nil {
		return 0, nil
	}
	return *score, err
}

// ReStampDivinedKey migrates a single entity's latest scored synthesis row from
// the legacy "divined_sigil" input-component key to "divined_peak", recomputing
// input_hash — NO Gemma call; score / blurb / prompt_version are left untouched.
//
// It operates on the row's STORED input_components, renaming only the key, so this
// is a pure vocabulary migration of the debounce gate: if the entity's inputs are
// otherwise unchanged, the next synthesis run recomputes the identical hash and
// SKIPS (no spurious re-synthesis from the rename); if the data genuinely changed
// since this row, the run still regenerates as normal. Returns true when a row was
// rewritten, false when there is nothing to migrate (no scored row, or the key is
// already migrated / was absent because the stat pillar was empty at generation).
func (s *SigilGenerator) ReStampDivinedKey(ctx context.Context, entityType string, entityID int, sport string) (bool, error) {
	sport = strings.ToUpper(sport)
	var id int64
	var icRaw []byte
	err := s.pool.QueryRow(ctx, `
		SELECT id, input_components FROM sigil_synthesis
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
		  AND score IS NOT NULL
		ORDER BY generated_at DESC
		LIMIT 1`, entityType, entityID, sport).Scan(&id, &icRaw)
	if errors.Is(err, pgx.ErrNoRows) {
		return false, nil
	}
	if err != nil {
		return false, err
	}

	var ic map[string]any
	if err := json.Unmarshal(icRaw, &ic); err != nil {
		return false, fmt.Errorf("unmarshal input_components (id=%d): %w", id, err)
	}
	v, ok := ic["divined_sigil"]
	if !ok {
		return false, nil // already migrated, or the stat pillar was empty at generation
	}
	delete(ic, "divined_sigil")
	ic["divined_peak"] = v

	newHash := hashComponents(ic)
	newIC, err := json.Marshal(ic)
	if err != nil {
		return false, err
	}
	if _, err := s.pool.Exec(ctx, `
		UPDATE sigil_synthesis SET input_components = $1, input_hash = $2 WHERE id = $3`,
		newIC, newHash, id); err != nil {
		return false, err
	}
	return true, nil
}
