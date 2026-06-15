package ml

// Stat commentary — the STATS-rail narrative, the twin of news_narratives.go.
// Reads an entity's ALREADY-SCRUBBED rating-engine output (player_stats /
// team_stats: composite, specialist, the rating_breakdown datapoints + their
// cohort-scoped percentiles) and asks Gemma for the entity's ON-FIELD IDENTITY —
// a few sentences of actual analysis, NOT a strengths/weaknesses list.
//
// Framing (Scott): our COMPOSITE shows how WELL an entity performs, our SPECIAL
// shows HOW it performs; the commentary narrates the "how" (the identity) with the
// composite setting the quality register. Gemma works only from the engine's
// curated datapoints (the deterministic scrub), never raw box scores, and the
// percentiles are passed as FACTS so it never invents a number.
//
// Length scales with a DETERMINISTIC notability score (the stats-rail analog of
// news impact / transfer heat). One stat_summaries row per generation (an entity
// has a single identity, unlike news's N narratives). See vault
// Plan - Gemma stat-profile summaries.md + Plan - Two-rail API endpoint model.md.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"sort"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// Bump when the prompt below materially changes (traced in stat_summaries.prompt_version).
const statCommentaryPromptVersion = "s1"

// maxStatFacts bounds the breakdown datapoints fed to the prompt (the specialty is
// always kept; the rest are the highest-percentile skills).
const maxStatFacts = 14

// StatCommentaryRequest describes the entity whose rating profile to narrate.
type StatCommentaryRequest struct {
	EntityType  string // 'player' | 'team'
	EntityID    int
	EntityName  string
	Sport       string         // 'NBA' | 'NFL' | 'FOOTBALL'
	TriggerType string         // 'stat_change' | 'periodic' | 'manual'
	Trigger     map[string]any // optional context
	DryRun      bool           // skip persistence (the profile is still loaded + scored)
}

// StatCommentaryResult is what Generate produces. SkippedNoStats is true when the
// entity has no usable rating row (a NULL-body marker is persisted).
type StatCommentaryResult struct {
	Body                 string
	Notability           int
	NotabilityComponents map[string]any
	InputComponents      map[string]any
	SkippedNoStats       bool
	Model                string
	PromptVersion        string
	GeneratedAt          time.Time
	Duration             time.Duration
}

// StatCommentator wires Ollama to the Postgres rating tables.
type StatCommentator struct {
	pool   *pgxpool.Pool
	ollama *OllamaClient
}

func NewStatCommentator(pool *pgxpool.Pool, ollama *OllamaClient) *StatCommentator {
	return &StatCommentator{pool: pool, ollama: ollama}
}

// Generate loads the entity's latest rating profile, computes a deterministic
// notability, asks Gemma for the on-field identity analysis (length scaled to
// notability), and persists one stat_summaries row.
func (a *StatCommentator) Generate(ctx context.Context, req StatCommentaryRequest) (*StatCommentaryResult, error) {
	if a.pool == nil {
		return nil, fmt.Errorf("stat commentator: no db pool")
	}
	if req.EntityID <= 0 || req.EntityName == "" || req.Sport == "" || req.EntityType == "" {
		return nil, fmt.Errorf("stat commentator: entity context incomplete")
	}
	sport := strings.ToUpper(req.Sport)
	now := time.Now()

	profile, err := loadRatingProfile(ctx, a.pool, req.EntityType, req.EntityID, sport)
	if err != nil {
		return nil, fmt.Errorf("load rating profile: %w", err)
	}

	// No usable rating (no row, or a row with no composite + empty breakdown) →
	// persist a NULL-body marker so the read path returns "no profile" and the
	// debounce skips until the rating changes.
	if profile == nil || (profile.compositeScore == nil && len(profile.breakdown) == 0) {
		res := &StatCommentaryResult{
			SkippedNoStats: true,
			Model:          a.ollama.Model(),
			PromptVersion:  statCommentaryPromptVersion,
			GeneratedAt:    now,
		}
		if !req.DryRun {
			if err := a.persist(ctx, req, sport, res); err != nil {
				return nil, fmt.Errorf("persist no-stats marker: %w", err)
			}
		}
		return res, nil
	}

	notability, ncomp := computeNotability(profile)
	prompt := buildStatPrompt(req, profile, notability)

	start := time.Now()
	gen, err := a.ollama.Generate(ctx, prompt, GenerateOptions{
		System:      statCommentarySystemPrompt,
		Temperature: 0.6,
		// A few sentences of prose on top of Gemma 4's internal reasoning budget.
		NumPredict: 2000,
	})
	if err != nil {
		return nil, fmt.Errorf("gemma generate: %w", err)
	}
	duration := time.Since(start)

	body := cleanCommentary(gen.Response)
	if body == "" {
		return nil, fmt.Errorf("empty commentary (raw=%q prompt_len=%d)", truncate(gen.Response, 160), len(prompt))
	}

	res := &StatCommentaryResult{
		Body:                 body,
		Notability:           notability,
		NotabilityComponents: ncomp,
		InputComponents:      profile.inputComponents(),
		Model:                gen.Model,
		PromptVersion:        statCommentaryPromptVersion,
		GeneratedAt:          now,
		Duration:             duration,
	}
	if !req.DryRun {
		if err := a.persist(ctx, req, sport, res); err != nil {
			return nil, fmt.Errorf("persist commentary: %w", err)
		}
	}
	return res, nil
}

// ---------------------------------------------------------------------------
// Rating profile — the scrubbed datapoints
// ---------------------------------------------------------------------------

// ratingDatapoint mirrors one element of the rating_breakdown JSONB (migration
// 030/043). pct is the percentile of sign*z, so HIGHER IS ALWAYS BETTER —
// including for "negative" stats (a high pct in turnovers = commits few).
type ratingDatapoint struct {
	Label       string             `json:"label"`
	Value       float64            `json:"value"`
	Z           float64            `json:"z"`
	Pct         float64            `json:"pct"`
	InComp      bool               `json:"in_comp"`
	InSpec      bool               `json:"in_spec"`
	Sign        int                `json:"sign"`
	Facet       string             `json:"facet"`
	IsSpecialty bool               `json:"is_specialty"`
	ScopedPct   map[string]float64 `json:"scoped_pct"`
}

type ratingProfile struct {
	entityType      string
	season          int
	position        string // players only
	compositeScore  *float64
	specialistScore *float64
	specialty       string
	breakdown       []ratingDatapoint
	scopedRanks     map[string]float64 // rating_scoped_ranks (entity-level cohort percentiles)
}

// loadRatingProfile reads the entity's latest unscoped (league_id 0/NULL) rating
// row. Returns nil when there is no rating row at all.
func loadRatingProfile(ctx context.Context, pool *pgxpool.Pool, entityType string, entityID int, sport string) (*ratingProfile, error) {
	var idCol string
	switch entityType {
	case "player":
		idCol = "player_id"
	case "team":
		idCol = "team_id"
	default:
		return nil, fmt.Errorf("unknown entity type %q", entityType)
	}
	table := entityType + "_stats" // player_stats | team_stats

	posSelect := "''::text"
	if entityType == "player" {
		posSelect = "COALESCE(position, '')"
	}
	// Latest season; prefer the unscoped row (NBA/NFL carry league_id 0/NULL), else
	// the richest league row (FOOTBALL is league-scoped with no aggregate — the
	// most-datapoints row is the main competition, e.g. the domestic league over a cup).
	q := fmt.Sprintf(`
		SELECT season, %s,
		       rating_composite_score, rating_specialist_score, COALESCE(rating_specialty, ''),
		       COALESCE(rating_breakdown, '[]'::jsonb), COALESCE(rating_scoped_ranks, '{}'::jsonb)
		FROM public.%s
		WHERE sport = $1 AND %s = $2
		ORDER BY season DESC,
		         (COALESCE(league_id, 0) = 0) DESC,
		         jsonb_array_length(COALESCE(rating_breakdown, '[]'::jsonb)) DESC,
		         COALESCE(league_id, 0) ASC
		LIMIT 1`, posSelect, table, idCol)

	var (
		p            ratingProfile
		breakdownRaw []byte
		scopedRaw    []byte
	)
	err := pool.QueryRow(ctx, q, sport, entityID).Scan(
		&p.season, &p.position,
		&p.compositeScore, &p.specialistScore, &p.specialty,
		&breakdownRaw, &scopedRaw,
	)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	if len(breakdownRaw) > 0 {
		if err := json.Unmarshal(breakdownRaw, &p.breakdown); err != nil {
			return nil, fmt.Errorf("unmarshal rating_breakdown: %w", err)
		}
	}
	if len(scopedRaw) > 0 {
		_ = json.Unmarshal(scopedRaw, &p.scopedRanks) // tolerant: cohort framing is optional
	}
	p.entityType = entityType
	return &p, nil
}

// inputComponents records the scrubbed datapoints we fed Gemma — the grounding
// trace (provenance) stored on the row.
func (p *ratingProfile) inputComponents() map[string]any {
	facts := make([]map[string]any, 0, len(p.breakdown))
	for _, d := range p.breakdown {
		facts = append(facts, map[string]any{"label": d.Label, "pct": round1(d.Pct), "is_specialty": d.IsSpecialty})
	}
	out := map[string]any{
		"season":     p.season,
		"specialty":  p.specialty,
		"datapoints": facts,
	}
	if p.compositeScore != nil {
		out["composite_score"] = round1(*p.compositeScore)
	}
	if p.specialistScore != nil {
		out["specialist_score"] = round1(*p.specialistScore)
	}
	if p.position != "" {
		out["position"] = p.position
	}
	return out
}

// ---------------------------------------------------------------------------
// Deterministic notability (0-100) — distinctiveness of the profile. Drives the
// dynamic analysis length + a future "most distinctive identities" board. Gemma
// never sees the formula; it only gets the resulting length guidance.
// ---------------------------------------------------------------------------

func computeNotability(p *ratingProfile) (int, map[string]any) {
	peak := 0.0
	eliteCount := 0
	for _, d := range p.breakdown {
		if d.Pct > peak {
			peak = d.Pct
		}
		if d.Pct >= 85 {
			eliteCount++
		}
	}
	comp := 50.0 // average T-score anchor when no composite
	if p.compositeScore != nil {
		comp = *p.compositeScore
	}
	// A standout skill drives it (peak); elite breadth adds; overall quality nudges.
	score := 0.6*peak + math.Min(30, float64(eliteCount)*10) + clampF(-10, 10, (comp-50)*0.4)
	n := int(math.Round(clampF(0, 100, score)))
	comps := map[string]any{
		"peak_pct":    round1(peak),
		"elite_count": eliteCount,
		"composite":   round1(comp),
	}
	return n, comps
}

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

const statCommentarySystemPrompt = `You are a sharp sports analyst. From an entity's RATING-ENGINE datapoints — already computed, our COMPOSITE shows how WELL it performs and our SPECIAL shows HOW — write a short, original-prose read of its ON-FIELD IDENTITY: what kind of player or team this is and what defines it. This is an ACTUAL ANALYSIS, not a strengths/weaknesses list and not a stat recap.

Rules:
- Lead with the DEFINING trait (the specialty, framed by its cohort percentile when given — e.g. "especially among forwards").
- Every percentile is SIGN-ADJUSTED so HIGHER IS ALWAYS BETTER — a high percentile in a "negative" stat (turnovers, goals conceded) means the entity EXCELS there (commits/concedes few). Read every number as goodness.
- Ground every claim in the given datapoints; never invent a stat, number, or fact not provided. Do NOT recite the raw numbers as a list — weave them into the read.
- Synthesize a coherent identity (e.g. "a low-usage floor-spacer who defends above his profile"), don't just enumerate skills.
- Return ONLY the analysis prose — no title, no headers, no bullet points, no preamble like "Analysis:".`

func buildStatPrompt(req StatCommentaryRequest, p *ratingProfile, notability int) string {
	var b strings.Builder

	header := fmt.Sprintf("%s %s", req.Sport, req.EntityType)
	if p.position != "" {
		header += ", " + p.position
	}
	b.WriteString(fmt.Sprintf("Entity: %s (%s)\n", req.EntityName, header))

	b.WriteString(fmt.Sprintf("\nLength: %s (notability %d/100 — scale the depth to how distinctive this profile is).\n",
		lengthGuidance(notability), notability))

	if p.compositeScore != nil {
		b.WriteString(fmt.Sprintf("\nComposite (how WELL — T-score, 50 = average): %.0f\n", *p.compositeScore))
	}
	if p.specialty != "" {
		line := fmt.Sprintf("Special (how — the standout skill): %s", p.specialty)
		if p.specialistScore != nil {
			line += fmt.Sprintf(" (%.0f)", *p.specialistScore)
		}
		b.WriteString(line + "\n")
	}

	b.WriteString("\nDatapoints (percentile vs peers; higher = better; [cohort] when available):\n")
	for _, d := range orderedFacts(p.breakdown) {
		b.WriteString(fmt.Sprintf("- %s: %.0fth", d.Label, d.Pct))
		if d.IsSpecialty {
			b.WriteString(" — THE specialty")
		}
		if pos, ok := d.ScopedPct["position"]; ok {
			b.WriteString(fmt.Sprintf(" [position: %.0fth]", pos))
		}
		b.WriteString("\n")
	}

	b.WriteString("\nWrite the identity analysis now.")
	return b.String()
}

// orderedFacts puts the specialty first, then the highest-percentile datapoints,
// bounded to maxStatFacts so the prompt stays tight.
func orderedFacts(in []ratingDatapoint) []ratingDatapoint {
	facts := make([]ratingDatapoint, len(in))
	copy(facts, in)
	sort.SliceStable(facts, func(i, j int) bool {
		if facts[i].IsSpecialty != facts[j].IsSpecialty {
			return facts[i].IsSpecialty // specialty first
		}
		return facts[i].Pct > facts[j].Pct
	})
	if len(facts) > maxStatFacts {
		facts = facts[:maxStatFacts]
	}
	return facts
}

func lengthGuidance(notability int) string {
	switch {
	case notability < 40:
		return "one crisp sentence"
	case notability < 70:
		return "two sentences"
	default:
		return "three to four sentences"
	}
}

// ---------------------------------------------------------------------------
// Output + persistence
// ---------------------------------------------------------------------------

// cleanCommentary trims Gemma's prose and strips a leading "Analysis:"-style
// label or wrapping quotes/fences if one slips through despite the prompt.
func cleanCommentary(raw string) string {
	s := strings.TrimSpace(raw)
	s = strings.Trim(s, "`")
	s = strings.TrimSpace(s)
	for _, p := range []string{"Analysis:", "Identity:", "On-field identity:"} {
		if strings.HasPrefix(s, p) {
			s = strings.TrimSpace(s[len(p):])
		}
	}
	return strings.TrimSpace(s)
}

func (a *StatCommentator) persist(ctx context.Context, req StatCommentaryRequest, sport string, res *StatCommentaryResult) error {
	triggerJSON, err := json.Marshal(req.Trigger)
	if err != nil {
		return err
	}
	ncomp, err := json.Marshal(orEmptyMap(res.NotabilityComponents))
	if err != nil {
		return err
	}
	icomp, err := json.Marshal(orEmptyMap(res.InputComponents))
	if err != nil {
		return err
	}

	var body any
	var notability any
	if !res.SkippedNoStats {
		body = res.Body
		notability = res.Notability
	}

	_, err = a.pool.Exec(ctx, `
		INSERT INTO stat_summaries (
		    entity_type, entity_id, sport, trigger_type, trigger_payload,
		    body, notability, notability_components, input_components,
		    model_version, prompt_version, generated_at
		) VALUES ($1,$2,$3,$4,$5, $6,$7,$8,$9, $10,$11,$12)`,
		req.EntityType, req.EntityID, sport, req.TriggerType, triggerJSON,
		body, notability, ncomp, icomp,
		res.Model, statCommentaryPromptVersion, res.GeneratedAt)
	return err
}

// ---------------------------------------------------------------------------
// small helpers
// ---------------------------------------------------------------------------

func clampF(lo, hi, v float64) float64 {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}

func round1(x float64) float64 { return math.Round(x*10) / 10 }

func orEmptyMap(m map[string]any) map[string]any {
	if m == nil {
		return map[string]any{}
	}
	return m
}
