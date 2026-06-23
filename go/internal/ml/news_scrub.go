package ml

// News scrub — the Gemma ID-gate (Stage 1). The fuzzy matcher casts a wide,
// high-recall net (false positives welcome); this is the PRECISION pass that
// confirms each linked entity is GENUINELY the subject and disambiguates
// same-name people via their identity card (the "Murillo" problem). Reuses the
// transfer subject-resolver's disambiguation principle (current club is the
// tie-breaker). The clean verdicts hydrate both news AND transfers, retiring
// the fuzzy-only matching + the 033 proximity gate.
//
// This file is additive + verification-first: ScrubArticle reads the article +
// its candidate links, returns per-candidate verdicts, and (only when !DryRun)
// applies them. Wiring into ingestion + flipping consumers onto the vetted set
// is a deliberate follow-on once the verdicts are validated.
//
// See vault wiki/Plan - News to Gemma Summaries.md (Stage 1).

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

const newsScrubPromptVersion = "v1"

// scrubCandidate is one entity currently linked to an article (the fuzzy
// matcher's guess), with the identity card used to disambiguate same-name people.
type scrubCandidate struct {
	entityType  string
	entityID    int
	name        string
	nationality string // player only
	currentClub string // player only
	position    string // player only
	confidence  float64
}

// ScrubVerdict is Gemma's call on one candidate.
type ScrubVerdict struct {
	EntityType string
	EntityID   int
	Name       string
	Relevant   bool
}

// ScrubArticleResult bundles the per-candidate verdicts for one article.
type ScrubArticleResult struct {
	ArticleID int64
	Title     string
	Verdicts  []ScrubVerdict
	Model     string
	Duration  time.Duration
}

// NewsScrubber wires Ollama to the link table.
type NewsScrubber struct {
	pool   *pgxpool.Pool
	ollama *OllamaClient
}

func NewNewsScrubber(pool *pgxpool.Pool, ollama *OllamaClient) *NewsScrubber {
	return &NewsScrubber{pool: pool, ollama: ollama}
}

// ScrubArticle vets every entity currently linked to the article: Gemma decides
// which are GENUINELY the subject (disambiguating same-name people), returning a
// verdict per candidate. dryRun=true reads + judges only (no writes).
func (s *NewsScrubber) ScrubArticle(ctx context.Context, articleID int64, sport string, dryRun bool) (*ScrubArticleResult, error) {
	if s.pool == nil {
		return nil, fmt.Errorf("news scrubber: no db pool")
	}
	sport = strings.ToUpper(sport)

	var title, description string
	if err := s.pool.QueryRow(ctx, `
		SELECT title, COALESCE(description, '') FROM news_articles WHERE id = $1
	`, articleID).Scan(&title, &description); err != nil {
		return nil, fmt.Errorf("load article: %w", err)
	}

	cands, err := s.loadCandidates(ctx, articleID, sport)
	if err != nil {
		return nil, fmt.Errorf("load candidates: %w", err)
	}
	if len(cands) == 0 {
		return &ScrubArticleResult{ArticleID: articleID, Title: title, Model: s.ollama.Model()}, nil
	}

	prompt := buildScrubPrompt(title, description, cands)

	start := time.Now()
	gen, err := s.ollama.Generate(ctx, prompt, GenerateOptions{
		System:      newsScrubSystemPrompt,
		Temperature: 0.2, // a judgment call; keep it tight + repeatable
		NumPredict:  1200,
	})
	if err != nil {
		return nil, fmt.Errorf("gemma generate: %w", err)
	}
	duration := time.Since(start)

	relevant, ok := parseScrubRelevant(gen.Response, len(cands))
	if !ok {
		return nil, fmt.Errorf("parse scrub verdict (raw=%q)", truncate(gen.Response, 160))
	}

	verdicts := make([]ScrubVerdict, len(cands))
	for i, c := range cands {
		// The PRIMARY link (confidence 1.0 — the entity this article was fetched
		// for, returned by Google RSS for that query) is deterministically
		// relevant; never let Gemma drop it. It still appears in the prompt as
		// context. Only the secondary fuzzy guesses are subject to the verdict.
		relevantHere := c.confidence >= 1.0 || relevant[i+1] // candidates are 1-indexed in the prompt
		verdicts[i] = ScrubVerdict{
			EntityType: c.entityType,
			EntityID:   c.entityID,
			Name:       c.name,
			Relevant:   relevantHere,
		}
	}

	if !dryRun {
		if err := s.applyVerdicts(ctx, articleID, sport, verdicts); err != nil {
			return nil, fmt.Errorf("apply verdicts: %w", err)
		}
	}

	return &ScrubArticleResult{
		ArticleID: articleID,
		Title:     title,
		Verdicts:  verdicts,
		Model:     gen.Model,
		Duration:  duration,
	}, nil
}

// loadCandidates returns every entity linked to the article with its identity
// card (player current club from the canonical latest-season source, NOT the
// stale players.team_id; position from the latest stats row) — same disambiguators
// transfers use.
func (s *NewsScrubber) loadCandidates(ctx context.Context, articleID int64, sport string) ([]scrubCandidate, error) {
	rows, err := s.pool.Query(ctx, `
		SELECT nae.entity_type, nae.entity_id,
		       COALESCE(p.name, t.name, '')                  AS name,
		       COALESCE(p.nationality, '')                   AS nationality,
		       COALESCE(ct.name, '')                         AS current_club,
		       COALESCE(NULLIF(pos.position, 'Unknown'), '') AS position,
		       nae.match_confidence
		FROM news_article_entities nae
		LEFT JOIN players p ON nae.entity_type = 'player' AND p.id = nae.entity_id AND p.sport = nae.sport
		LEFT JOIN teams   t ON nae.entity_type = 'team'   AND t.id = nae.entity_id AND t.sport = nae.sport
		LEFT JOIN public.player_current_team pct ON nae.entity_type = 'player' AND pct.player_id = nae.entity_id AND pct.sport = nae.sport
		LEFT JOIN teams ct ON ct.id = pct.team_id AND ct.sport = nae.sport
		LEFT JOIN LATERAL (
		    SELECT ps.position FROM player_stats ps
		    WHERE ps.player_id = nae.entity_id AND ps.sport = nae.sport
		    ORDER BY ps.season DESC NULLS LAST LIMIT 1
		) pos ON nae.entity_type = 'player'
		WHERE nae.article_id = $1 AND nae.sport = $2
		ORDER BY nae.match_confidence DESC, nae.entity_type, nae.entity_id
	`, articleID, sport)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []scrubCandidate
	for rows.Next() {
		var c scrubCandidate
		if err := rows.Scan(&c.entityType, &c.entityID, &c.name, &c.nationality, &c.currentClub, &c.position, &c.confidence); err != nil {
			return nil, err
		}
		out = append(out, c)
	}
	return out, rows.Err()
}

// applyVerdicts records the Gemma verdict on every link of the article: vetted =
// the relevance call, scrubbed_at = NOW(). Non-destructive (migration 083) — the
// dropped links stay, flagged vetted=false, so we keep the recall to re-judge and
// can compare fuzzy-vs-vetted before consumers trust the flag. Every candidate
// (kept and dropped) is stamped so the async worker knows the article is scrubbed;
// the primary link is always vetted=true (ScrubArticle marks it relevant).
//
// One UPDATE per article (over unnest'd parallel arrays), not one per link: the
// whole article's scrub lands in a SINGLE transaction. That matters for the
// migration-103 enqueue trigger (AFTER UPDATE OF vetted) — its per-row firings all
// see the article's final vetted state and its constant-payload pg_notify de-dups
// to one wake-up per article — and it makes the persist atomic (no partial write
// on a mid-batch error).
func (s *NewsScrubber) applyVerdicts(ctx context.Context, articleID int64, sport string, verdicts []ScrubVerdict) error {
	if len(verdicts) == 0 {
		return nil
	}
	entityTypes := make([]string, len(verdicts))
	entityIDs := make([]int, len(verdicts))
	relevants := make([]bool, len(verdicts))
	for i, v := range verdicts {
		entityTypes[i] = v.EntityType
		entityIDs[i] = v.EntityID
		relevants[i] = v.Relevant
	}
	_, err := s.pool.Exec(ctx, `
		UPDATE news_article_entities n
		   SET vetted = v.relevant, scrubbed_at = NOW()
		  FROM unnest($2::text[], $3::int[], $4::bool[]) AS v(entity_type, entity_id, relevant)
		 WHERE n.article_id = $1 AND n.sport = $5
		   AND n.entity_type = v.entity_type AND n.entity_id = v.entity_id
	`, articleID, entityTypes, entityIDs, relevants, sport)
	return err
}

// ---------------------------------------------------------------------------
// Prompt + parse
// ---------------------------------------------------------------------------

const newsScrubSystemPrompt = `You decide which of the listed players/teams a news article is GENUINELY ABOUT, so we tag it correctly. Same-name people are common — use each player's identity (nationality, current club, position) to tell them apart; CURRENT CLUB is the strongest tie-breaker.

A candidate is RELEVANT if the article genuinely concerns that EXACT person/team — a real subject or a meaningful mention. A candidate is NOT relevant when:
- it is a DIFFERENT same-name person — the article's club/position/role contradicts the identity (e.g. a club president or a manager, or a different player at another club). When the identity's current club is contradicted by the article, it is a different person.
- the name appears only as incidental noise (a long roundup where they are not actually discussed).

Be inclusive for genuine mentions, strict on same-name confusion. Reply with ONLY a JSON object, no prose:
{"relevant": [<the candidate numbers that are genuinely about this article>]}`

func buildScrubPrompt(title, description string, cands []scrubCandidate) string {
	var b strings.Builder
	b.WriteString("Article:\n")
	b.WriteString(title)
	if description != "" {
		b.WriteString(" — ")
		b.WriteString(truncate(description, 300))
	}
	b.WriteString("\n\nCandidates (same-name people may appear — disambiguate by identity):\n")
	for i, c := range cands {
		b.WriteString(fmt.Sprintf("%d. ", i+1))
		if c.entityType == "team" {
			b.WriteString(c.name + " (team)")
		} else {
			ident := []string{c.name}
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
			b.WriteString(strings.Join(ident, " · "))
		}
		b.WriteString("\n")
	}
	b.WriteString("\nReturn the JSON now.")
	return b.String()
}

type gemmaScrubVerdict struct {
	Relevant []int `json:"relevant"`
}

// parseScrubRelevant extracts {"relevant":[...]} and returns a 1-indexed set of
// candidate numbers judged relevant. Numbers out of range are ignored.
func parseScrubRelevant(raw string, n int) (map[int]bool, bool) {
	start := strings.IndexByte(raw, '{')
	end := strings.LastIndexByte(raw, '}')
	if start < 0 || end <= start {
		return nil, false
	}
	var v gemmaScrubVerdict
	if err := json.Unmarshal([]byte(raw[start:end+1]), &v); err != nil {
		return nil, false
	}
	set := make(map[int]bool, len(v.Relevant))
	for _, idx := range v.Relevant {
		if idx >= 1 && idx <= n {
			set[idx] = true
		}
	}
	return set, true
}
