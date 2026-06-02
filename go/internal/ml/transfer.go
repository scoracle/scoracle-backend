package ml

// Transfer/Trade rumor analyzer — clones the Vibe Generator pattern at the PAIR
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
const transferPromptVersion = "t1"

const (
	transferMaxCorpusNews      = 12
	transferMaxCorpusTweets    = 8
	transferDefaultMinArticles = 2
	transferMaxCandidates      = 40 // load governor: cap Gemma calls per team
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
	playerID   int
	playerName string
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

	// Direction is DETERMINISTIC from roster membership, not Gemma's text guess:
	// a player currently on the team can only be an OUTGOING rumor; a player not
	// on the roster can only be INCOMING. (Gemma's direction field is ignored.)
	onRoster, err := g.isOnRoster(ctx, teamID, c.playerID, sport)
	if err != nil {
		return err
	}

	prompt := buildTransferPrompt(teamName, c.playerName, sport, onRoster, news, tweets)
	gen, gerr := g.ollama.Generate(ctx, prompt, GenerateOptions{
		System:      transferSystemPrompt(sport),
		Temperature: 0.3,
		NumPredict:  1200,
		JSONMode:    true,
	})
	if gerr != nil {
		// Gemma down/slow → provisional heat-only row (the card still renders).
		return g.persist(ctx, teamID, c.playerID, sport, triggerType, heat, components, nil, onRoster, attribution, newsIDs, tweetIDs, res)
	}
	verdict, ok := parseTransferVerdict(gen.Response)
	if !ok {
		// Unparseable output → same provisional fallback.
		return g.persist(ctx, teamID, c.playerID, sport, triggerType, heat, components, nil, onRoster, attribution, newsIDs, tweetIDs, res)
	}
	// Grounding guard: a claimed rumor with no credible (tier-1/2) source is suspect.
	if verdict.IsRumor != nil && *verdict.IsRumor && bestWeight < 0.5 {
		verdict.Confidence *= 0.5
	}
	return g.persist(ctx, teamID, c.playerID, sport, triggerType, heat, components, &verdict, onRoster, attribution, newsIDs, tweetIDs, res)
}

// isOnRoster reports whether the player's most recent season was spent on this
// team — the deterministic signal for transfer direction (on → outgoing, off →
// incoming).
func (g *TransferGenerator) isOnRoster(ctx context.Context, teamID, playerID int, sport string) (bool, error) {
	var on bool
	err := g.pool.QueryRow(ctx, `
		SELECT EXISTS (
			SELECT 1 FROM player_stats ps
			WHERE ps.player_id = $1 AND ps.sport = $2 AND ps.team_id = $3
			  AND ps.season = (SELECT MAX(season) FROM player_stats WHERE player_id = $1 AND sport = $2)
		)
	`, playerID, sport, teamID).Scan(&on)
	return on, err
}

// directionFor maps roster membership to the rumor direction.
func directionFor(onRoster bool) string {
	if onRoster {
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
		SELECT pe.entity_id, p.name
		FROM news_article_entities te
		JOIN news_article_entities pe
		  ON pe.article_id = te.article_id AND pe.sport = te.sport AND pe.entity_type = 'player'
		JOIN players p ON p.id = pe.entity_id AND p.sport = pe.sport
		WHERE te.entity_type = 'team' AND te.entity_id = $1 AND te.sport = $2
		  AND te.created_at > NOW() - INTERVAL '14 days'
		GROUP BY pe.entity_id, p.name
		HAVING count(DISTINCT te.article_id) >= $3
		ORDER BY count(DISTINCT te.article_id) DESC
		LIMIT $4
	`, teamID, sport, minArticles, transferMaxCandidates)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []transferCandidate
	for rows.Next() {
		var c transferCandidate
		if err := rows.Scan(&c.playerID, &c.playerName); err != nil {
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

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

func transferSystemPrompt(sport string) string {
	noun := "transfer"
	if sport == "NBA" || sport == "NFL" {
		noun = "trade"
	}
	return fmt.Sprintf(`You analyze sports %s rumors STRICTLY from the provided news headlines and tweets. Never invent facts; use only what the sources say. Cite the source.

Decide whether the source material is genuinely about a %s involving BOTH the named team and the named player — NOT a match report, a "who is better" comparison, an injury note, or routine coverage of a player already on the team.

Reply with ONLY a JSON object, no prose:
{"is_rumor": true|false, "direction": "incoming"|"outgoing"|"unclear", "stage": "speculation"|"concrete_interest"|"advanced_talks"|"here_we_go", "summary": "one short sentence grounded in and attributed to the sources", "confidence": 0.0-1.0}

direction is relative to the named team: "incoming" = the team is signing the player; "outgoing" = the player is leaving the team. If not a %s, set is_rumor=false and the other fields to your best guess or empty.`, noun, noun, noun)
}

func buildTransferPrompt(teamName, playerName, sport string, onRoster bool, news []newsItem, tweets []tweetItem) string {
	var b strings.Builder
	b.WriteString(fmt.Sprintf("Sport: %s\nTeam: %s\nPlayer: %s\n", sport, teamName, playerName))
	if onRoster {
		b.WriteString(fmt.Sprintf("Roster status: %s is CURRENTLY on %s — so any move is a DEPARTURE (outgoing). Frame the summary as other clubs' interest in signing them.\n", playerName, teamName))
	} else {
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
	heat *int, components string, verdict *gemmaTransferVerdict, onRoster bool, attribution string,
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
		direction = strptr(directionFor(onRoster))
		res.Rumors++
	default:
		ir := verdict.IsRumor != nil && *verdict.IsRumor
		isRumor = &ir
		model = strptr(g.ollama.Model())
		if ir {
			direction = strptr(directionFor(onRoster)) // deterministic from roster, not Gemma's guess
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

	_, err := g.pool.Exec(ctx, `
		INSERT INTO transfer_rumors (
		    team_id, player_id, sport, trigger_type, heat, heat_components,
		    is_rumor, direction, stage, gemma_summary, source_attribution, confidence,
		    input_news_ids, input_tweet_ids, model_version, prompt_version
		) VALUES ($1,$2,$3,$4,$5,$6::jsonb,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
	`,
		teamID, playerID, sport, triggerType, heat, components,
		isRumor, direction, stage, summary, attr, confidence,
		newsIDs, tweetIDs, model, promptVer,
	)
	return err
}
