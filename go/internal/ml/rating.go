package ml

// Stat commentary — the STATS-rail narrative, the twin of news_narratives.go.
// Reads an entity's ALREADY-SCRUBBED rating-engine output (player_stats /
// team_stats: composite, sigil, the rating_breakdown datapoints + their
// cohort-scoped percentiles) and asks Gemma for the entity's ON-FIELD IDENTITY —
// a few sentences of actual analysis, NOT a strengths/weaknesses list.
//
// Framing (Scott): our COMPOSITE shows how WELL an entity performs, our PEAK
// shows HOW it performs; the commentary narrates the "how" (the identity) with the
// composite setting the quality register. Gemma divines the entity's PEAK — its
// single defining statistical strength — on the first output line ("PEAK: <label>"),
// then writes the identity analysis. Gemma works only from the engine's curated
// datapoints (the deterministic scrub), never raw box scores, and the percentiles
// are passed as FACTS so it never invents a number.
//
// Length scales with a DETERMINISTIC notability score (the stats-rail analog of
// news impact / transfer heat). One stat_summaries row per generation (an entity
// has a single identity, unlike news's N narratives). See vault
// Plan - Gemma stat-profile summaries.md + Plan - Two-rail API endpoint model.md.

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
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
//
// s2: feeds the rate-adjusted (per-x: per_36/per_90) standouts so Gemma can reveal
//
//	elite rate production the raw totals hide; per-x peak also counts toward notability.
//
// s3: Gemma divines the sigil on line 1 ("SIGIL: <label>"); specialty-first sort
//
//	removed; the pre-supplied "Special" hint line dropped from the prompt.
//
// s5: the divined-label marker renamed SIGIL: → PEAK: (de-sigil — post-convergence
//
//	"sigil" means only the crown; this peak-strength label is "peak"). The parser
//	still accepts the legacy SIGIL: prefix for any in-flight responses.
//
// s6: re-aimed for Mistral (L8) as the on-air beat analyst. The prompt now also
//
//	carries the raw VALUE and the sign-adjusted Z (scarcity/scale) per datapoint, and
//	a deterministic TIER label (pctBand: elite…poor) so the model verbalizes the
//	spectrum instead of (mis)mapping percentile→quality itself — judge by the absolute
//	tier, never vs the entity's own other marks. Strength-led + breadth-in-one-stroke,
//	no forced negatives, honest mediocrity ("No standout skill"), one organic paragraph.
const ratingPromptVersion = "s6"

// maxStatFacts bounds the breakdown datapoints fed to the prompt.
const maxStatFacts = 14

// RatingRequest describes the entity whose rating profile to narrate.
type RatingRequest struct {
	EntityType  string // 'player' | 'team'
	EntityID    int
	EntityName  string
	Sport       string         // 'NBA' | 'NFL' | 'FOOTBALL'
	Season      *int           // nil = the entity's latest season
	TriggerType string         // 'stat_change' | 'periodic' | 'manual'
	Trigger     map[string]any // optional context
	DryRun      bool           // skip persistence (the profile is still loaded + scored)
	// SkipUnchanged returns early (SkippedUnchanged) without a Gemma call when the
	// entity-season's last commentary was built from the same rating snapshot
	// (matching input_hash). The nightly cadence's "only work on new data" gate.
	SkipUnchanged bool
}

// RatingResult is what Generate produces. SkippedNoStats is true when the
// entity has no usable rating row; SkippedUnchanged when SkipUnchanged short-circuited.
type RatingResult struct {
	Body                 string
	DivinedPeak          string // extracted from "PEAK: <label>" first line; empty if absent
	Notability           int
	NotabilityComponents map[string]any
	InputComponents      map[string]any
	Season               int
	InputHash            string
	SkippedNoStats       bool
	SkippedUnchanged     bool
	Model                string
	PromptVersion        string
	GeneratedAt          time.Time
	Duration             time.Duration
}

// RatingGenerator wires Ollama to the Postgres rating tables.
type RatingGenerator struct {
	pool   *pgxpool.Pool
	ollama *OllamaClient
}

func NewRatingGenerator(pool *pgxpool.Pool, ollama *OllamaClient) *RatingGenerator {
	return &RatingGenerator{pool: pool, ollama: ollama}
}

// Generate loads the entity's latest rating profile, computes a deterministic
// notability, asks Gemma for the on-field identity analysis (length scaled to
// notability), and persists one stat_summaries row.
func (a *RatingGenerator) Generate(ctx context.Context, req RatingRequest) (*RatingResult, error) {
	if a.pool == nil {
		return nil, fmt.Errorf("stat commentator: no db pool")
	}
	if req.EntityID <= 0 || req.EntityName == "" || req.Sport == "" || req.EntityType == "" {
		return nil, fmt.Errorf("stat commentator: entity context incomplete")
	}
	sport := strings.ToUpper(req.Sport)
	now := time.Now()

	profile, err := loadRatingProfile(ctx, a.pool, req.EntityType, req.EntityID, sport, req.Season)
	if err != nil {
		return nil, fmt.Errorf("load rating profile: %w", err)
	}

	// No usable rating (no row, or a row with no composite + empty breakdown) →
	// persist a NULL-body marker so the read path returns "no profile" and the
	// cadence skips until the rating changes.
	if profile == nil || (profile.compositeScore == nil && len(profile.breakdown) == 0) {
		res := &RatingResult{
			SkippedNoStats: true,
			Season:         derefOrZero(req.Season),
			Model:          a.ollama.Model(),
			PromptVersion:  ratingPromptVersion,
			GeneratedAt:    now,
		}
		if !req.DryRun {
			if err := a.persist(ctx, req, sport, res); err != nil {
				return nil, fmt.Errorf("persist no-stats marker: %w", err)
			}
		}
		return res, nil
	}

	// Build the grounding facts + their hash up front so the nightly cadence can
	// skip the (expensive) Gemma call when the rating snapshot is unchanged.
	ic := profile.inputComponents()
	hash := hashComponents(ic)
	if req.SkipUnchanged {
		last, err := a.lastCommentaryHash(ctx, req.EntityType, req.EntityID, sport, profile.season)
		if err == nil && last != "" && last == hash {
			return &RatingResult{
				SkippedUnchanged: true,
				Season:           profile.season,
				InputHash:        hash,
				Model:            a.ollama.Model(),
				PromptVersion:    ratingPromptVersion,
				GeneratedAt:      now,
			}, nil
		}
	}

	notability, ncomp := computeNotability(profile)
	prompt := buildStatPrompt(req, profile, notability)

	start := time.Now()
	gen, err := a.ollama.Generate(ctx, prompt, GenerateOptions{
		System:      ratingSystemPrompt,
		Temperature: 0.6,
		// A few sentences of prose on top of Gemma 4's internal reasoning budget.
		NumPredict: 2000,
		Op:         "rating",
	})
	if err != nil {
		return nil, fmt.Errorf("gemma generate: %w", err)
	}
	duration := time.Since(start)

	divinedPeak, rawBody := parsePeakCommentary(gen.Response)
	body := cleanCommentary(rawBody)
	if body == "" {
		return nil, fmt.Errorf("empty commentary (raw=%q prompt_len=%d)", truncate(gen.Response, 160), len(prompt))
	}

	res := &RatingResult{
		Body:                 body,
		DivinedPeak:          divinedPeak,
		Notability:           notability,
		NotabilityComponents: ncomp,
		InputComponents:      ic,
		Season:               profile.season,
		InputHash:            hash,
		Model:                gen.Model,
		PromptVersion:        ratingPromptVersion,
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
	entityType     string
	season         int
	position       string // players only
	compositeScore *float64
	peakScore      *float64 // engine peak datapoint (rating_specialist*; "peak" post-convergence)
	peakLabel      string
	breakdown      []ratingDatapoint
	scopedRanks    map[string]float64           // rating_scoped_ranks (entity-level cohort percentiles)
	rateModes      map[string][]ratingDatapoint // rating_modes — per-x breakdowns (per_36, per_90, …)
}

// loadRatingProfile reads the entity's rating row for `season` (nil = latest).
// Prefers the unscoped row, falling back to the richest league row. Returns nil
// when there is no rating row at all.
func loadRatingProfile(ctx context.Context, pool *pgxpool.Pool, entityType string, entityID int, sport string, season *int) (*ratingProfile, error) {
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
		       COALESCE(rating_breakdown, '[]'::jsonb), COALESCE(rating_scoped_ranks, '{}'::jsonb),
		       COALESCE(rating_modes, '{}'::jsonb)
		FROM public.%s
		WHERE sport = $1 AND %s = $2 AND ($3::int IS NULL OR season = $3)
		ORDER BY season DESC,
		         (COALESCE(league_id, 0) = 0) DESC,
		         jsonb_array_length(COALESCE(rating_breakdown, '[]'::jsonb)) DESC,
		         COALESCE(league_id, 0) ASC
		LIMIT 1`, posSelect, table, idCol)

	var (
		p            ratingProfile
		breakdownRaw []byte
		scopedRaw    []byte
		modesRaw     []byte
	)
	err := pool.QueryRow(ctx, q, sport, entityID, season).Scan(
		&p.season, &p.position,
		&p.compositeScore, &p.peakScore, &p.peakLabel,
		&breakdownRaw, &scopedRaw, &modesRaw,
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
	// rating_modes: a per-x bundle per mode (per_36 / per_90 / …), each with its own
	// breakdown. The per-x lens is the reveal — elite rate production raw totals hide.
	if len(modesRaw) > 0 {
		var modes map[string]struct {
			Breakdown []ratingDatapoint `json:"breakdown"`
		}
		if err := json.Unmarshal(modesRaw, &modes); err == nil {
			p.rateModes = make(map[string][]ratingDatapoint, len(modes))
			for name, m := range modes {
				if len(m.Breakdown) > 0 {
					p.rateModes[name] = m.Breakdown
				}
			}
		}
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
		"peak_label": p.peakLabel,
		"datapoints": facts,
	}
	if rs := collectRateStandouts(p); len(rs) > 0 {
		rates := make([]map[string]any, 0, len(rs))
		for _, r := range rs {
			rates = append(rates, map[string]any{"mode": r.mode, "label": r.label, "pct": round1(r.pct)})
		}
		out["rate_standouts"] = rates
	}
	if p.compositeScore != nil {
		out["composite_score"] = round1(*p.compositeScore)
	}
	if p.peakScore != nil {
		out["peak_score"] = round1(*p.peakScore)
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
	// The per-x lens counts toward the PEAK (so a limited-minutes player who is
	// elite per-36/per-90 is judged notable and earns a fuller reveal), but not
	// toward eliteCount (avoid double-counting the same skill across modes).
	for _, dps := range p.rateModes {
		for _, d := range dps {
			if d.Pct > peak {
				peak = d.Pct
			}
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

const ratingSystemPrompt = `You are the respected analyst a national broadcast brings on to break down what this player or team is, statistically. You have the entity's RATING-ENGINE profile — already computed. COMPOSITE is how WELL it performs overall. Each skill is given as: VALUE (the raw stat) · PERCENTILE versus peers and its TIER (elite / strong / above average / average / below average / poor) · Z (standard deviations above the mean — the scarcity and scale of the edge; a high z is a rarer, more premium skill, the kind that decides games). Percentiles come at scopes — overall, versus position, and per-x (per-36 / per-90). Use them together: a modest raw output but an elite per-x mark means an efficient, lower-minutes contributor worth noting.

THE TIER IS THE TRUTH. Judge each skill by its labeled tier, never by how it stacks up against the entity's OWN other skills: a "strong" or "elite" mark is a strength even if it is this entity's lowest; a "poor" mark is a weakness even if it is their highest. The PEAK is simply the highest-percentile skill.

Write the read of the entity's ON-FIELD IDENTITY in the analyst's voice:
- First line exactly: PEAK: followed by EITHER the skill of the FIRST datapoint listed (the highest percentile), OR the exact words "No standout skill" if that first skill is not at least "strong" tier. Those are the ONLY two valid forms. The PEAK can NEVER be a skill labeled "average", "below average", or "poor" — if the top skill is not "strong" or "elite", you MUST write exactly "PEAK: No standout skill". (Do not pick the most eye-catching weakness — the peak is about the best skill, not the most notable one.)
- Lead with the genuine strengths (the elite and strong skills). When the entity is strong across many areas, name its single most dominant skill, then capture the breadth in one stroke ("a dominant all-around offensive producer") rather than listing every skill. Cite the value and percentile, and when an edge holds up in the per-x numbers say so (proof it is not a fluke). Reserve "elite", "rare", "premium", "game-wrecking" for elite, high-z marks.
- Do NOT force negatives, and never praise a mark below the 50th percentile — anything under 50 is below average by definition. Mention a weakness only when a skill is genuinely "below average" or "poor," and name it as the limitation it is. If NO skill rates "below average" or "poor," the entity has no statistical weakness — mention none at all, and never call a "strong" or "elite" mark "below par," "room for improvement," or a shortcoming just because the entity is even better elsewhere.
- If nothing rates strong or better, say so plainly and point to their greatest impact with its percentile ("no standout skill, but his biggest impact is tackling, around the 59th percentile") — the number lets the reader judge.
- Land it with a verdict on what this entity is — the line an analyst signs off with ("a legit two-way game-wrecker", "a low-usage floor-spacer who defends above his profile", "a replaceable rotation piece").

Deliver it as ONE flowing paragraph, the way an analyst speaks on air — no line breaks, no "strengths / weaknesses / summary" sections. A modest profile is 2-3 sentences; a multi-skill standout up to five; never more. Pack several stats into a sentence; never give one its own sentence, and never walk the list in order. Cite a value as the plain figure given — never fabricate a per-game or per-90 rate that is not in the data. Ground every claim; never invent a number or a skill that is not there.

Match this format and length exactly — it is a different, invented player, so take nothing factual from it, only the shape:
PEAK: Elite finishing
A ruthless penalty-box striker — 24 goals (97th percentile, a rare +3.1 z) with elite shot volume (92nd) and conversion that holds up per-90, the kind of scarce finishing that decides matches; beyond the box he offers little, with poor creation (28th) and pressing (22nd), but as a pure number nine he is among the league's best. A lethal poacher, plain and simple.

After the PEAK line, return only the paragraph — no headers, no bullets, no preamble.`

func buildStatPrompt(req RatingRequest, p *ratingProfile, notability int) string {
	var b strings.Builder

	header := fmt.Sprintf("%s %s", req.Sport, req.EntityType)
	if p.position != "" {
		header += ", " + p.position
	}
	b.WriteString(fmt.Sprintf("Entity: %s (%s)\n", req.EntityName, header))

	b.WriteString(fmt.Sprintf("\nProfile distinctiveness: %d/100 (higher = more standout skills — let a richer profile earn a fuller read).\n", notability))

	if p.compositeScore != nil {
		b.WriteString(fmt.Sprintf("\nComposite (how WELL overall — T-score, 50 = average): %.0f\n", *p.compositeScore))
	}

	b.WriteString("\nDatapoints — value · percentile + TIER (the percentile mapped to elite/strong/above average/average/below average/poor; THIS TIER IS THE TRUTH) · z (standard deviations above the mean: the scarcity/scale of the edge; a high z is a rarer, more premium skill); [position] percentile shown when present:\n")
	for _, d := range orderedFacts(p.breakdown) {
		dz := float64(d.Sign) * d.Z // sign-adjusted so + is always the good direction
		b.WriteString(fmt.Sprintf("- %s: %s · %.0fth pct (%s) · z %+.1f", d.Label, trimFloat(d.Value), d.Pct, pctBand(d.Pct), dz))
		if pos, ok := d.ScopedPct["position"]; ok {
			b.WriteString(fmt.Sprintf(" [position: %.0fth, %s]", pos, pctBand(pos)))
		}
		b.WriteString("\n")
	}

	if rs := collectRateStandouts(p); len(rs) > 0 {
		b.WriteString("\nRate-adjusted (per-x) corroboration — these also rate elite on a per-minute / per-90 basis (so the edge is not just a counting-stat artifact of heavy minutes):\n")
		for _, r := range rs {
			b.WriteString(fmt.Sprintf("- [%s] %s: %.0fth pct\n", strings.ReplaceAll(r.mode, "_", "-"), r.label, r.pct))
		}
	}

	b.WriteString("\nWrite the identity analysis now.")
	return b.String()
}

// pctBand maps a percentile to its quality tier — the deterministic part of
// "read the spectrum," done in code so the model never has to map percentile→quality
// itself (Mistral was inverting it, e.g. calling a 37th-percentile skill "above
// average"). The model just verbalizes the labeled tier. Transient prompt-shaping
// (like sigil's trendDir), not a stored derived stat.
func pctBand(pct float64) string {
	switch {
	case pct >= 90:
		return "elite"
	case pct >= 75:
		return "strong"
	case pct >= 60:
		return "above average"
	case pct >= 50:
		return "average"
	case pct >= 35:
		return "below average"
	default:
		return "poor"
	}
}

// trimFloat renders a datapoint value compactly: integers without a decimal,
// small fractions (percentages, rates < 1) with two places, everything else with
// one — so the prompt shows "3" / "0.38" / "10.7" rather than "3.0" / "0.4".
func trimFloat(f float64) string {
	switch {
	case f == math.Trunc(f):
		return fmt.Sprintf("%.0f", f)
	case math.Abs(f) < 1:
		return fmt.Sprintf("%.2f", f)
	default:
		return fmt.Sprintf("%.1f", f)
	}
}

type rateStandout struct {
	mode  string
	label string
	pct   float64
}

// collectRateStandouts surfaces, per rate mode, the elite (pct >= 80) per-x
// datapoints — the lens that reveals a limited-minutes player producing at an
// elite rate. Modes sorted for stable output; bounded per mode.
func collectRateStandouts(p *ratingProfile) []rateStandout {
	modes := make([]string, 0, len(p.rateModes))
	for m := range p.rateModes {
		modes = append(modes, m)
	}
	sort.Strings(modes)

	var out []rateStandout
	for _, m := range modes {
		dps := make([]ratingDatapoint, len(p.rateModes[m]))
		copy(dps, p.rateModes[m])
		sort.SliceStable(dps, func(i, j int) bool { return dps[i].Pct > dps[j].Pct })
		cnt := 0
		for _, d := range dps {
			if d.Pct < 80 {
				break
			}
			out = append(out, rateStandout{mode: m, label: d.Label, pct: d.Pct})
			if cnt++; cnt >= 5 {
				break
			}
		}
	}
	return out
}

// orderedFacts returns the highest-percentile datapoints, bounded to maxStatFacts.
func orderedFacts(in []ratingDatapoint) []ratingDatapoint {
	facts := make([]ratingDatapoint, len(in))
	copy(facts, in)
	sort.SliceStable(facts, func(i, j int) bool {
		return facts[i].Pct > facts[j].Pct
	})
	if len(facts) > maxStatFacts {
		facts = facts[:maxStatFacts]
	}
	return facts
}

// ---------------------------------------------------------------------------
// Output + persistence
// ---------------------------------------------------------------------------

// parsePeakCommentary extracts the divined peak-strength label from the first
// line ("PEAK: <label>") and returns (divinedPeak, body). The body is everything
// after that line. If the model omits the marker line the whole response becomes
// the body and divinedPeak is empty. The legacy "SIGIL: " prefix is still accepted
// (forward-only marker rename, s5) so any in-flight pre-s5 response still parses.
func parsePeakCommentary(raw string) (divinedPeak, body string) {
	trimmed := strings.TrimSpace(raw)
	idx := strings.Index(trimmed, "\n")
	if idx >= 0 {
		firstLine := strings.TrimSpace(trimmed[:idx])
		rest := strings.TrimSpace(trimmed[idx+1:])
		if label, ok := trimMarker(firstLine); ok {
			return label, rest
		}
		return "", trimmed
	}
	// single-line response — could be just a bare marker line with no body
	if label, ok := trimMarker(trimmed); ok {
		return label, ""
	}
	return "", trimmed
}

// trimMarker strips the divined-label marker ("PEAK: " or the legacy "SIGIL: ")
// from a line, returning the label and whether a marker was present.
func trimMarker(line string) (label string, ok bool) {
	for _, prefix := range [...]string{"PEAK: ", "SIGIL: "} {
		if strings.HasPrefix(line, prefix) {
			return strings.TrimSpace(strings.TrimPrefix(line, prefix)), true
		}
	}
	return "", false
}

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

func (a *RatingGenerator) persist(ctx context.Context, req RatingRequest, sport string, res *RatingResult) error {
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
	var divinedPeak any
	if !res.SkippedNoStats {
		body = res.Body
		notability = res.Notability
		if res.DivinedPeak != "" {
			divinedPeak = res.DivinedPeak
		}
	}
	var season any
	if res.Season > 0 {
		season = res.Season
	}
	var inputHash any
	if res.InputHash != "" {
		inputHash = res.InputHash
	}

	_, err = a.pool.Exec(ctx, `
		INSERT INTO stat_summaries (
		    entity_type, entity_id, sport, season, trigger_type, trigger_payload,
		    body, notability, notability_components, input_components, input_hash,
		    model_version, prompt_version, generated_at, divined_peak
		) VALUES ($1,$2,$3,$4,$5,$6, $7,$8,$9,$10,$11, $12,$13,$14,$15)`,
		req.EntityType, req.EntityID, sport, season, req.TriggerType, triggerJSON,
		body, notability, ncomp, icomp, inputHash,
		res.Model, ratingPromptVersion, res.GeneratedAt, divinedPeak)
	return err
}

// lastCommentaryHash returns the input_hash of the entity-season's LATEST commentary
// generation — the nightly skip signal. Canonical latest-generation rule (Session 11
// / F-023): take the latest row regardless of nullability (no `body IS NOT NULL`
// pre-filter); a no-stats marker has a NULL input_hash → "" → the next run never wrongly
// skips against an older real commentary the marker has superseded.
func (a *RatingGenerator) lastCommentaryHash(ctx context.Context, entityType string, entityID int, sport string, season int) (string, error) {
	var hash *string
	err := a.pool.QueryRow(ctx, `
		SELECT input_hash FROM stat_summaries
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

// ReStampPeakKeys migrates a single entity-season's latest commentary row from the
// legacy "sigil_label"/"sigil_score" input-component keys to "peak_label"/"peak_score"
// and recomputes input_hash — NO Gemma call; body / divined_peak / prompt_version are
// left untouched. Same pattern as the crown's ReStampDivinedKey: it rewrites only the
// keys in the STORED input_components, so for an unchanged entity the next nightly run
// recomputes the identical hash and SKIPS (the key rename causes no spurious regen);
// genuinely-changed entities still regenerate. Returns true when a row was rewritten.
func (a *RatingGenerator) ReStampPeakKeys(ctx context.Context, entityType string, entityID int, sport string, season int) (bool, error) {
	sport = strings.ToUpper(sport)
	var id int64
	var icRaw []byte
	// Canonical latest-generation rule (Session 11 / F-023): operate on the entity-
	// season's latest generation regardless of nullability. If that latest row is a
	// marker its input_components is '{}' (no legacy keys) → the restamp is a no-op,
	// so a marker is never bypassed to restamp an older real row.
	err := a.pool.QueryRow(ctx, `
		SELECT id, input_components FROM stat_summaries
		WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4
		ORDER BY generated_at DESC
		LIMIT 1`, entityType, entityID, sport, season).Scan(&id, &icRaw)
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
	changed := false
	if v, ok := ic["sigil_label"]; ok {
		delete(ic, "sigil_label")
		ic["peak_label"] = v
		changed = true
	}
	if v, ok := ic["sigil_score"]; ok {
		delete(ic, "sigil_score")
		ic["peak_score"] = v
		changed = true
	}
	if !changed {
		return false, nil // already migrated, or the keys were absent
	}

	newHash := hashComponents(ic)
	newIC, err := json.Marshal(ic)
	if err != nil {
		return false, err
	}
	if _, err := a.pool.Exec(ctx, `
		UPDATE stat_summaries SET input_components = $1, input_hash = $2 WHERE id = $3`,
		newIC, newHash, id); err != nil {
		return false, err
	}
	return true, nil
}

// hashComponents is a stable hash of the grounding facts. encoding/json marshals
// map keys in sorted order, so the same rating snapshot always hashes the same.
func hashComponents(ic map[string]any) string {
	b, err := json.Marshal(orEmptyMap(ic))
	if err != nil {
		return ""
	}
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:16]) // 128-bit prefix is ample for a change signal
}

func derefOrZero(p *int) int {
	if p == nil {
		return 0
	}
	return *p
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
