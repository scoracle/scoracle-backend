package ml

// Transfer/Trade rumor analyzer — clones the VibeGenerator pattern at the PAIR
// grain. For a team, it walks co-mention candidate players, computes the
// DETERMINISTIC heat index (compute_transfer_heat, migration 032), and uses
// Gemma ONLY to VET each pair — is it a real rumor, direction, stage, a grounded
// one-liner. Gemma never invents the number.
//
// Gemma's is_rumor=false is what removes the roster/match-report noise the
// heat-only seed surfaces (e.g. a team's own players). On a Gemma failure /
// unparseable output we fall back to a provisional heat-only row (is_rumor TRUE,
// no classification) so the card never breaks.

import (
	"context"
	"encoding/json"
	"fmt"
	"regexp"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// Bump when the transfer prompt materially changes.
// t2: identity card + same-person test (require THIS exact player, name-collision guard).
const transferPromptVersion = "t2"

const (
	transferMaxCorpusNews      = 12
	transferMaxCorpusTweets    = 8
	transferDefaultMinArticles = 2
	transferMaxCandidates      = 40 // load governor: cap Gemma calls per team
	// comentionProximityChars bounds how far apart (in title characters) a team
	// and a player may be mentioned and still count as a genuine co-mention.
	// Beyond it a shared article is almost always a multi-subject roundup, not a
	// link between the two. NULL-tolerant in SQL. Mirrors migration 033's gate.
	comentionProximityChars = 50
)

// TransferRequest describes one team's analysis pass.
type TransferRequest struct {
	TeamID      int
	TeamName    string
	Sport       string // 'NBA' | 'NFL' | 'FOOTBALL'
	TriggerType string // 'periodic' | 'news_spike' | 'manual'
	MinArticles int    // candidate pre-filter; 0 → default
}

// TransferResult is a per-team summary.
type TransferResult struct {
	Candidates int
	Rumors     int // is_rumor TRUE rows written (incl. provisional fallback)
	Cleared    int // Gemma said is_rumor=false (roster/match-report noise)
	Skipped    int // no corpus
	Errored    int
	Duration   time.Duration
}

// gemmaTransferVerdict is Gemma's JSON output (defensively parsed).
type gemmaTransferVerdict struct {
	IsRumor    *bool   `json:"is_rumor"`
	Subject    string  `json:"subject"` // who the sources are really about (audit trail for discarded impostors)
	Direction  string  `json:"direction"`
	Stage      string  `json:"stage"`
	Summary    string  `json:"summary"`
	Confidence float64 `json:"confidence"`
}

type TransferGenerator struct {
	pool   *pgxpool.Pool
	ollama *OllamaClient
}

func NewTransferGenerator(pool *pgxpool.Pool, ollama *OllamaClient) *TransferGenerator {
	return &TransferGenerator{pool: pool, ollama: ollama}
}

type transferCandidate struct {
	playerID    int
	playerName  string
	nationality string // identity-card disambiguators; empty when unknown
	currentClub string // latest-season club name (player_current_team), NOT the stale players.team_id
	position    string
}

// GenerateForTeam vets every candidate rumor for one team.
func (g *TransferGenerator) GenerateForTeam(ctx context.Context, req TransferRequest) (*TransferResult, error) {
	if g.pool == nil {
		return nil, fmt.Errorf("transfer generator: no db pool")
	}
	if req.TeamID <= 0 || req.Sport == "" {
		return nil, fmt.Errorf("transfer generator: incomplete request")
	}
	sport := strings.ToUpper(req.Sport)
	minArticles := req.MinArticles
	if minArticles <= 0 {
		minArticles = transferDefaultMinArticles
	}
	triggerType := req.TriggerType
	if triggerType == "" {
		triggerType = "manual"
	}

	tiers, err := g.loadTierMap(ctx)
	if err != nil {
		return nil, fmt.Errorf("load tiers: %w", err)
	}
	candidates, err := g.loadCandidates(ctx, req.TeamID, sport, minArticles)
	if err != nil {
		return nil, fmt.Errorf("load candidates: %w", err)
	}

	res := &TransferResult{Candidates: len(candidates)}
	start := time.Now()
	for _, c := range candidates {
		if err := g.analyzePair(ctx, req.TeamID, req.TeamName, c, sport, triggerType, tiers, res); err != nil {
			res.Errored++ // one bad pair shouldn't kill the run
		}
	}
	res.Duration = time.Since(start)
	return res, nil
}

// analyzePair: deterministic heat → corpus → Gemma vet → persist.
func (g *TransferGenerator) analyzePair(
	ctx context.Context, teamID int, teamName string, c transferCandidate,
	sport, triggerType string, tiers map[string]float64, res *TransferResult,
) error {
	var heat *int
	var components string
	var newsIDs []int64
	var tweetIDs []string
	err := g.pool.QueryRow(ctx,
		`SELECT heat, components, news_ids, tweet_ids FROM compute_transfer_heat($1,$2,$3)`,
		teamID, c.playerID, sport,
	).Scan(&heat, &components, &newsIDs, &tweetIDs)
	if err != nil {
		return err
	}
	if heat == nil {
		res.Skipped++ // no corpus
		return nil
	}

	news, err := g.loadPairNews(ctx, newsIDs)
	if err != nil {
		return err
	}
	tweets, err := g.loadPairTweets(ctx, tweetIDs)
	if err != nil {
		return err
	}

	// Grounding: the credibility attribution comes from the CORPUS, not Gemma.
	attribution, bestWeight := bestSource(news, tweets, tiers)

	// Direction AND the noise filter key off the player's RELATIONSHIP to the team
	// (computed deterministically from player_stats history), not Gemma's text
	// guess: "current" → outgoing; "former"/"none" → incoming. The "former" case
	// matters most — ex-players get co-mentioned in historical background
	// ("former Chelsea man …") that isn't a rumor; the prompt tells Gemma to clear
	// those unless there's a genuine return.
	rel, err := g.teamRelationship(ctx, teamID, c.playerID, sport)
	if err != nil {
		return err
	}

	prompt := buildTransferPrompt(teamName, c, sport, rel, news, tweets)
	gen, gerr := g.ollama.Generate(ctx, prompt, GenerateOptions{
		System:      transferSystemPrompt(sport),
		Temperature: 0.3,
		NumPredict:  1200,
		JSONMode:    true,
	})
	if gerr != nil {
		// Gemma down/slow → provisional heat-only row (the card still renders).
		return g.persist(ctx, teamID, c.playerID, sport, triggerType, heat, components, nil, rel, attribution, newsIDs, tweetIDs, res)
	}
	verdict, ok := parseTransferVerdict(gen.Response)
	if !ok {
		// Unparseable output → same provisional fallback.
		return g.persist(ctx, teamID, c.playerID, sport, triggerType, heat, components, nil, rel, attribution, newsIDs, tweetIDs, res)
	}
	// Former-player gate (deterministic — gemma4:e4b doesn't reliably honor the
	// prompt's "clear unless returning" instruction). A FORMER player is a live
	// rumor only if the corpus actually signals a RETURN; otherwise the co-mention
	// is historical background or a multi-entity-article artifact ("Everton want
	// [Chelsea's Delap] and [Spurs' Gallagher]" links Gallagher↔Chelsea spuriously).
	if rel == "former" && verdict.IsRumor != nil && *verdict.IsRumor && !hasReturnSignal(news, tweets) {
		cleared := false
		verdict.IsRumor = &cleared
	}
	// Grounding guard: a claimed rumor with no credible (tier-1/2) source is suspect.
	if verdict.IsRumor != nil && *verdict.IsRumor && bestWeight < 0.5 {
		verdict.Confidence *= 0.5
	}
	return g.persist(ctx, teamID, c.playerID, sport, triggerType, heat, components, &verdict, rel, attribution, newsIDs, tweetIDs, res)
}

// teamRelationship classifies the player's deterministic relationship to the team
// from player_stats history: "current" (latest season on the team), "former" (on
// the team in some past season but not now), or "none". Drives direction and the
// former-player noise filter. Bounded by seeded history — a stint that predates
// our data reads as "none".
func (g *TransferGenerator) teamRelationship(ctx context.Context, teamID, playerID int, sport string) (string, error) {
	var isCurrent, isEver bool
	err := g.pool.QueryRow(ctx, `
		SELECT
		    COALESCE(bool_or(ps.team_id = $3 AND ps.season = (SELECT MAX(season) FROM player_stats WHERE player_id = $1 AND sport = $2)), false),
		    COALESCE(bool_or(ps.team_id = $3), false)
		FROM player_stats ps
		WHERE ps.player_id = $1 AND ps.sport = $2
	`, playerID, sport, teamID).Scan(&isCurrent, &isEver)
	if err != nil {
		return "none", err
	}
	switch {
	case isCurrent:
		return "current", nil
	case isEver:
		return "former", nil
	default:
		return "none", nil
	}
}

// directionFor maps the team relationship to the rumor direction: a current
// player can only be leaving (outgoing); everyone else would be arriving.
func directionFor(rel string) string {
	if rel == "current" {
		return "outgoing"
	}
	return "incoming"
}

// ---------------------------------------------------------------------------
// Loaders
// ---------------------------------------------------------------------------

func (g *TransferGenerator) loadTierMap(ctx context.Context) (map[string]float64, error) {
	rows, err := g.pool.Query(ctx, `SELECT kind, lower(source), weight FROM source_tiers`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	m := make(map[string]float64)
	for rows.Next() {
		var kind, source string
		var weight float64
		if err := rows.Scan(&kind, &source, &weight); err != nil {
			return nil, err
		}
		m[kind+":"+source] = weight
	}
	return m, rows.Err()
}

func (g *TransferGenerator) loadCandidates(ctx context.Context, teamID int, sport string, minArticles int) ([]transferCandidate, error) {
	rows, err := g.pool.Query(ctx, `
		SELECT pe.entity_id, p.name,
		       COALESCE(p.nationality, '')                    AS nationality,
		       COALESCE(ct.name, '')                          AS current_club,
		       COALESCE(NULLIF(pos.position, 'Unknown'), '')  AS position
		FROM news_article_entities te
		JOIN news_article_entities pe
		  ON pe.article_id = te.article_id AND pe.sport = te.sport AND pe.entity_type = 'player'
		JOIN players p ON p.id = pe.entity_id AND p.sport = pe.sport
		-- Identity card: current club from the canonical latest-season source (NOT the
		-- stale players.team_id), position from the latest stats row.
		LEFT JOIN public.player_current_team pct ON pct.player_id = p.id AND pct.sport = p.sport
		LEFT JOIN teams ct ON ct.id = pct.team_id AND ct.sport = p.sport
		LEFT JOIN LATERAL (
		    SELECT ps.position FROM player_stats ps
		    WHERE ps.player_id = p.id AND ps.sport = p.sport
		    ORDER BY ps.season DESC NULLS LAST LIMIT 1
		) pos ON true
		WHERE te.entity_type = 'team' AND te.entity_id = $1 AND te.sport = $2
		  AND te.created_at > NOW() - INTERVAL '14 days'
		  -- Scrub gate: both the team and player link must be Gemma-vetted as genuine.
		  AND te.vetted IS TRUE
		  AND pe.vetted IS TRUE
		  AND (te.title_pos IS NULL OR pe.title_pos IS NULL
		       OR abs(te.title_pos - pe.title_pos) <= $5)
		GROUP BY pe.entity_id, p.name, p.nationality, ct.name, pos.position
		HAVING count(DISTINCT te.article_id) >= $3
		ORDER BY count(DISTINCT te.article_id) DESC
		LIMIT $4
	`, teamID, sport, minArticles, transferMaxCandidates, comentionProximityChars)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []transferCandidate
	for rows.Next() {
		var c transferCandidate
		if err := rows.Scan(&c.playerID, &c.playerName, &c.nationality, &c.currentClub, &c.position); err != nil {
			return nil, err
		}
		out = append(out, c)
	}
	return out, rows.Err()
}

func (g *TransferGenerator) loadPairNews(ctx context.Context, ids []int64) ([]newsItem, error) {
	if len(ids) == 0 {
		return nil, nil
	}
	rows, err := g.pool.Query(ctx, `
		SELECT id, title, COALESCE(description, ''), COALESCE(source, ''), published_at
		FROM news_articles WHERE id = ANY($1)
		ORDER BY published_at DESC NULLS LAST LIMIT $2
	`, ids, transferMaxCorpusNews)
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

func (g *TransferGenerator) loadPairTweets(ctx context.Context, ids []string) ([]tweetItem, error) {
	if len(ids) == 0 {
		return nil, nil
	}
	rows, err := g.pool.Query(ctx, `
		SELECT id, author_username, text, posted_at
		FROM tweets WHERE id = ANY($1)
		ORDER BY posted_at DESC LIMIT $2
	`, ids, transferMaxCorpusTweets)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []tweetItem
	for rows.Next() {
		var t tweetItem
		if err := rows.Scan(&t.id, &t.author, &t.text, &t.postedAt); err != nil {
			return nil, err
		}
		out = append(out, t)
	}
	return out, rows.Err()
}

// bestSource returns the highest-credibility source present in the corpus and
// its weight (unknown sources default to 0.3). Used for grounded attribution.
func bestSource(news []newsItem, tweets []tweetItem, tiers map[string]float64) (string, float64) {
	best, bestW := "", 0.0
	weightOf := func(kind, src string) float64 {
		if w, ok := tiers[kind+":"+strings.ToLower(src)]; ok {
			return w
		}
		return 0.3
	}
	for _, n := range news {
		if n.source == "" {
			continue
		}
		if w := weightOf("news", n.source); w > bestW {
			bestW, best = w, n.source
		}
	}
	for _, t := range tweets {
		if w := weightOf("twitter", t.author); w > bestW {
			bestW, best = w, "@"+t.author
		}
	}
	return best, bestW
}

// returnSignals are phrases that indicate a genuine RETURN move (vs. a former
// player merely mentioned in passing). Tunable — phrasing varies.
var returnSignals = []string{
	"return to", "returning to", "rejoin", "re-sign", "resign for",
	"back to", "back at", "comeback", "second spell", "reunite", "bring back", "brings back",
}

// hasReturnSignal reports whether the pair corpus contains return-move language.
func hasReturnSignal(news []newsItem, tweets []tweetItem) bool {
	contains := func(s string) bool {
		l := strings.ToLower(s)
		for _, kw := range returnSignals {
			if strings.Contains(l, kw) {
				return true
			}
		}
		return false
	}
	for _, n := range news {
		if contains(n.title) || contains(n.description) {
			return true
		}
	}
	for _, t := range tweets {
		if contains(t.text) {
			return true
		}
	}
	return false
}

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

func transferSystemPrompt(sport string) string {
	noun := "transfer"
	if sport == "NBA" || sport == "NFL" {
		noun = "trade"
	}
	return fmt.Sprintf(`You analyze sports %s rumors STRICTLY from the provided news headlines and tweets. Never invent facts; use only what the sources say. Cite the source.

You are given an IDENTITY line describing ONE specific player (name, nationality, current club, position). Decide whether the sources genuinely report a %s involving BOTH the named team AND THIS EXACT player — the same human as the identity line — not merely someone who shares the name.

Set is_rumor=false when ANY of these holds:
- The sources are really about a DIFFERENT person who happens to share the name — a club president/owner, a manager/coach, an unrelated public figure, or a different player at another club. (Example: a midfielder named "Florentino" is NOT Florentino Pérez, the Real Madrid president — clear it.)
- The current club or position in the sources contradicts the identity line — that means it is a different person.
- It is a match report, a "who is better" comparison, an injury note, or routine coverage of a player already on the team.

Use the identity line — ESPECIALLY the current club — as your tie-breaker for same-name people. When unsure it is the same person, prefer is_rumor=false.

Reply with ONLY a JSON object, no prose:
{"is_rumor": true|false, "subject": "who the sources are actually about (real name/person, even if NOT this player)", "direction": "incoming"|"outgoing"|"unclear", "stage": "speculation"|"concrete_interest"|"advanced_talks"|"here_we_go", "summary": "one short sentence grounded in and attributed to the sources", "confidence": 0.0-1.0}

direction is relative to the named team: "incoming" = the team is signing the player; "outgoing" = the player is leaving the team. If it is not a %s about THIS exact player, set is_rumor=false; still fill "subject" with who the sources are really about.`, noun, noun, noun)
}

func buildTransferPrompt(teamName string, c transferCandidate, sport, rel string, news []newsItem, tweets []tweetItem) string {
	playerName := c.playerName
	var b strings.Builder
	b.WriteString(fmt.Sprintf("Sport: %s\nTeam: %s\nPlayer: %s\n", sport, teamName, playerName))
	// Identity card — the disambiguators that separate same-name people. Led by
	// current club (nationality is sparse, ~39% coverage). The system prompt keys
	// its same-person test off this line.
	ident := []string{playerName}
	if c.nationality != "" {
		ident = append(ident, c.nationality)
	}
	if c.currentClub != "" {
		ident = append(ident, "currently at "+c.currentClub)
	} else {
		ident = append(ident, "current club unknown")
	}
	if c.position != "" {
		ident = append(ident, c.position)
	}
	b.WriteString("Identity (the ONE specific player to judge): " + strings.Join(ident, " · ") + "\n")
	switch rel {
	case "current":
		b.WriteString(fmt.Sprintf("Roster status: %s is CURRENTLY on %s — so any move is a DEPARTURE (outgoing). Frame the summary as other clubs' interest in signing them.\n", playerName, teamName))
	case "former":
		b.WriteString(fmt.Sprintf("Roster status: %s is a FORMER %s player who has SINCE LEFT. A 'former/ex-%s' mention is just background, NOT a transfer rumor — set is_rumor=false UNLESS the sources genuinely report %s RETURNING to %s (then it is incoming).\n", playerName, teamName, teamName, playerName, teamName))
	default:
		b.WriteString(fmt.Sprintf("Roster status: %s is NOT on %s — so any move is an ARRIVAL (incoming). Frame the summary as %s pursuing them.\n", playerName, teamName, teamName))
	}
	b.WriteString("\nNews headlines:\n")
	if len(news) == 0 {
		b.WriteString("- (none)\n")
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
	b.WriteString("\nTweets:\n")
	if len(tweets) == 0 {
		b.WriteString("- (none)\n")
	} else {
		for _, t := range tweets {
			b.WriteString(fmt.Sprintf("- @%s: %s\n", t.author, truncate(strings.ReplaceAll(t.text, "\n", " "), 200)))
		}
	}
	b.WriteString("\nReturn the JSON verdict now.")
	return b.String()
}

// ---------------------------------------------------------------------------
// Parse + persist
// ---------------------------------------------------------------------------

var jsonObjectRE = regexp.MustCompile(`(?s)\{.*\}`)

// parseTransferVerdict extracts the first {...} object and unmarshals it. With
// JSONMode the response is already JSON; the regex defends against any wrapping.
func parseTransferVerdict(raw string) (gemmaTransferVerdict, bool) {
	m := jsonObjectRE.FindString(raw)
	if m == "" {
		return gemmaTransferVerdict{}, false
	}
	var v gemmaTransferVerdict
	if err := json.Unmarshal([]byte(m), &v); err != nil {
		return gemmaTransferVerdict{}, false
	}
	return v, true
}

var validStages = map[string]bool{"speculation": true, "concrete_interest": true, "advanced_talks": true, "here_we_go": true}

func normStage(s string) string {
	s = strings.ToLower(strings.ReplaceAll(strings.TrimSpace(s), " ", "_"))
	if validStages[s] {
		return s
	}
	return "speculation"
}

func clampConf(c float64) float64 {
	if c < 0 {
		return 0
	}
	if c > 1 {
		return 1
	}
	return c
}

func strptr(s string) *string { return &s }

// persist writes one transfer_rumors row. verdict == nil → provisional heat-only
// (is_rumor TRUE, no classification). verdict.is_rumor=false → a "cleared" row
// (hidden by the read filter — removes roster/match-report noise).
func (g *TransferGenerator) persist(
	ctx context.Context, teamID, playerID int, sport, triggerType string,
	heat *int, components string, verdict *gemmaTransferVerdict, relationship string, attribution string,
	newsIDs []int64, tweetIDs []string, res *TransferResult,
) error {
	var (
		isRumor    *bool
		direction  *string
		stage      *string
		summary    *string
		confidence *float64
		model      *string
		promptVer  = strptr(transferPromptVersion)
	)

	switch {
	case verdict == nil:
		t := true
		isRumor = &t // provisional
		direction = strptr(directionFor(relationship))
		res.Rumors++
	default:
		ir := verdict.IsRumor != nil && *verdict.IsRumor
		isRumor = &ir
		model = strptr(g.ollama.Model())
		if ir {
			direction = strptr(directionFor(relationship)) // deterministic from roster, not Gemma's guess
			stage = strptr(normStage(verdict.Stage))
			if s := strings.TrimSpace(verdict.Summary); s != "" {
				summary = strptr(truncate(s, 240))
			}
			c := clampConf(verdict.Confidence)
			confidence = &c
			res.Rumors++
		} else {
			res.Cleared++
		}
	}

	var attr *string
	if attribution != "" {
		attr = &attribution
	}
	if newsIDs == nil {
		newsIDs = []int64{}
	}
	if tweetIDs == nil {
		tweetIDs = []string{}
	}

	// Audit trail: stash who Gemma judged the sources to be about, so a discarded
	// same-name impostor (is_rumor=false) leaves a record of what was filtered and
	// why. trigger_payload is NOT NULL DEFAULT '{}'.
	triggerPayload := "{}"
	if verdict != nil {
		if subj := strings.TrimSpace(verdict.Subject); subj != "" {
			if b, mErr := json.Marshal(map[string]string{"subject": subj}); mErr == nil {
				triggerPayload = string(b)
			}
		}
	}

	_, err := g.pool.Exec(ctx, `
		INSERT INTO transfer_rumors (
		    team_id, player_id, sport, trigger_type, heat, heat_components,
		    is_rumor, direction, stage, gemma_summary, source_attribution, confidence,
		    input_news_ids, input_tweet_ids, model_version, prompt_version, trigger_payload
		) VALUES ($1,$2,$3,$4,$5,$6::jsonb,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17::jsonb)
	`,
		teamID, playerID, sport, triggerType, heat, components,
		isRumor, direction, stage, summary, attr, confidence,
		newsIDs, tweetIDs, model, promptVer, triggerPayload,
	)
	return err
}
