// Package thirdparty provides clients for third-party APIs (news, twitter).
package thirdparty

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/xml"
	"fmt"
	"io"
	"log"
	"net/http"
	"net/url"
	"regexp"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const (
	newsDefaultLimit = 10
	newsMaxLimit     = 50
	newsMinArticles  = 3
	newsRSSTimeout   = 15 * time.Second
)

// Time windows for escalation (hours).
var timeWindows = []int{24, 48, 168}

// Sport-specific search term suffixes for RSS.
var sportTerms = map[string]string{
	"NBA":      "NBA basketball",
	"NFL":      "NFL football",
	"FOOTBALL": "soccer football",
}

// ---------------------------------------------------------------------------
// Article — normalized article shape
// ---------------------------------------------------------------------------

// Article is a normalized news article.
type Article struct {
	Title       string  `json:"title"`
	Description string  `json:"description"`
	URL         string  `json:"url"`
	Source      string  `json:"source"`
	PublishedAt string  `json:"published_at"`
	ImageURL    *string `json:"image_url"`
	Author      *string `json:"author,omitempty"`
}

// ---------------------------------------------------------------------------
// NewsService — Google News RSS client
// ---------------------------------------------------------------------------

// entityPoolTTL controls how often the cross-entity match pool is refreshed
// from the DB. The pool is populated by player/team upserts via the seeder,
// so staleness of up to an hour is harmless.
const entityPoolTTL = 1 * time.Hour

// cachedEntity is one row in the in-memory entity pool used for cross-entity
// matching at write-through time. Built from teams + players rows.
type cachedEntity struct {
	entityType string // 'player' | 'team'
	entityID   int
	sport      string
	match      EntityMatchInput
}

type entityPool struct {
	entities  []cachedEntity
	refreshed time.Time
}

// NewsService fetches entity news from Google News RSS.
//
// When constructed with a non-nil pool, every matched article is also
// persisted to news_articles + news_article_entities as a write-through.
// That populates the long-term corpus Gemma reads from.
type NewsService struct {
	httpClient *http.Client
	pool       *pgxpool.Pool

	entityMu    sync.RWMutex
	entityPools map[string]*entityPool // keyed by sport, lowercased
}

// NewNewsService creates a news service. If pool is non-nil, fetched
// articles are written to news_articles / news_article_entities on each
// successful entity match.
func NewNewsService(pool *pgxpool.Pool) *NewsService {
	return &NewsService{
		httpClient: &http.Client{
			Timeout: newsRSSTimeout,
		},
		pool:        pool,
		entityPools: make(map[string]*entityPool),
	}
}

// Status returns service configuration status.
func (s *NewsService) Status() map[string]interface{} {
	return map[string]interface{}{
		"rss_available":  true,
		"primary_source": "google_news_rss",
	}
}

// GetEntityNews fetches news for an entity via Google News RSS.
// entityType ("player"|"team") and entityID drive the write-through —
// matched articles are linked back to the requested entity in
// news_article_entities. Pass entityType="" / entityID=0 to skip write-through.
func (s *NewsService) GetEntityNews(
	ctx context.Context,
	entityType string,
	entityID int,
	entityName, sport, team string,
	limit int,
	firstName, lastName string,
	aliases []string,
) (map[string]interface{}, error) {
	if limit < 1 {
		limit = newsDefaultLimit
	}
	if limit > newsMaxLimit {
		limit = newsMaxLimit
	}

	result, matched, err := s.fetchFromRSS(entityName, sport, team, limit, firstName, lastName, aliases)
	if err != nil {
		return nil, err
	}

	// Write-through: persist the matched articles and link them to this entity.
	// Non-fatal — a failed persist shouldn't break the response.
	if s.pool != nil && entityType != "" && entityID > 0 && len(matched) > 0 {
		if perr := s.persistArticles(ctx, sport, entityType, entityID, matched); perr != nil {
			log.Printf("[news] persist failed sport=%s entity=%s/%d: %v", sport, entityType, entityID, perr)
		}
	}

	return result, nil
}

// persistArticles upserts articles by URL hash and links them to the primary
// requested entity plus any other teams/players mentioned in the title (cross-
// entity linking). The secondary pass catches relational patterns Gemma can
// learn from — e.g. "Warriors trade talks for Durant" links to Warriors AND
// Durant even if only Durant was the queried entity.
//
// Runs in a single transaction. Errors are returned to the caller but don't
// break the response path — the caller logs and moves on.
func (s *NewsService) persistArticles(
	ctx context.Context,
	sport, primaryEntityType string,
	primaryEntityID int,
	articles []Article,
) error {
	tx, err := s.pool.Begin(ctx)
	if err != nil {
		return err
	}
	defer tx.Rollback(ctx)

	sportUpper := strings.ToUpper(sport)
	pool, err := s.getEntityPool(ctx, sportUpper)
	if err != nil {
		log.Printf("[news] entity pool load failed sport=%s: %v", sportUpper, err)
		pool = nil
	}

	for _, a := range articles {
		if a.URL == "" || a.Title == "" {
			continue
		}
		hash := sha256Hex(a.URL)

		var publishedAt *time.Time
		if ts := parseArticleDate(a.PublishedAt); !ts.IsZero() {
			publishedAt = &ts
		}

		var articleID int64
		err := tx.QueryRow(ctx, `
			INSERT INTO news_articles (url_hash, url, source, title, description, published_at)
			VALUES ($1, $2, $3, $4, $5, $6)
			ON CONFLICT (url_hash) DO UPDATE SET
			    title       = EXCLUDED.title,
			    description = EXCLUDED.description,
			    source      = COALESCE(EXCLUDED.source, news_articles.source),
			    published_at = COALESCE(EXCLUDED.published_at, news_articles.published_at)
			RETURNING id
		`, hash, a.URL, nullIfEmpty(a.Source), a.Title, nullIfEmpty(a.Description), publishedAt).Scan(&articleID)
		if err != nil {
			return fmt.Errorf("upsert article: %w", err)
		}

		// Primary link — the entity that was queried.
		if _, err := tx.Exec(ctx, `
			INSERT INTO news_article_entities (article_id, entity_type, entity_id, sport, match_confidence)
			VALUES ($1, $2, $3, $4, $5)
			ON CONFLICT (article_id, entity_type, entity_id, sport) DO NOTHING
		`, articleID, primaryEntityType, primaryEntityID, sportUpper, 1.0); err != nil {
			return fmt.Errorf("link primary entity: %w", err)
		}

		// Secondary links — scan the title against the cached entity pool
		// to pick up co-mentioned teams/players. ON CONFLICT DO NOTHING
		// means re-matching the primary entity is a cheap no-op.
		for i := range pool {
			e := &pool[i]
			// Skip the primary — we already linked it at confidence 1.0.
			if e.entityType == primaryEntityType && e.entityID == primaryEntityID {
				continue
			}
			if !MatchesEntity(a.Title, e.match) {
				continue
			}
			if _, err := tx.Exec(ctx, `
				INSERT INTO news_article_entities (article_id, entity_type, entity_id, sport, match_confidence)
				VALUES ($1, $2, $3, $4, $5)
				ON CONFLICT (article_id, entity_type, entity_id, sport) DO NOTHING
			`, articleID, e.entityType, e.entityID, sportUpper, 0.8); err != nil {
				return fmt.Errorf("link secondary entity: %w", err)
			}
		}
	}

	return tx.Commit(ctx)
}

// getEntityPool returns (and refreshes on staleness) the cached list of
// players + teams for a sport. The pool is small enough (<3.5k per sport
// post-purge) that scanning the whole list per article is fine.
func (s *NewsService) getEntityPool(ctx context.Context, sport string) ([]cachedEntity, error) {
	s.entityMu.RLock()
	p, ok := s.entityPools[sport]
	s.entityMu.RUnlock()

	if ok && time.Since(p.refreshed) < entityPoolTTL {
		return p.entities, nil
	}

	entities, err := s.loadEntityPool(ctx, sport)
	if err != nil {
		// If we have a stale cache, keep using it rather than failing.
		if ok {
			return p.entities, nil
		}
		return nil, err
	}

	s.entityMu.Lock()
	s.entityPools[sport] = &entityPool{entities: entities, refreshed: time.Now()}
	s.entityMu.Unlock()
	return entities, nil
}

func (s *NewsService) loadEntityPool(ctx context.Context, sport string) ([]cachedEntity, error) {
	// Teams
	teamRows, err := s.pool.Query(ctx, `
		SELECT id, name, COALESCE(short_code, ''), COALESCE(search_aliases, ARRAY[]::text[])
		FROM teams WHERE sport = $1
	`, sport)
	if err != nil {
		return nil, fmt.Errorf("load teams: %w", err)
	}
	var out []cachedEntity
	for teamRows.Next() {
		var id int
		var name, shortCode string
		var aliases []string
		if err := teamRows.Scan(&id, &name, &shortCode, &aliases); err != nil {
			teamRows.Close()
			return nil, err
		}
		al := aliases
		if shortCode != "" {
			al = append(al, shortCode)
		}
		out = append(out, cachedEntity{
			entityType: "team",
			entityID:   id,
			sport:      sport,
			match: EntityMatchInput{
				Name:    name,
				Aliases: al,
				Sport:   sport,
			},
		})
	}
	teamRows.Close()

	// Players
	playerRows, err := s.pool.Query(ctx, `
		SELECT id, name, COALESCE(first_name, ''), COALESCE(last_name, ''),
		       COALESCE(search_aliases, ARRAY[]::text[])
		FROM players WHERE sport = $1
	`, sport)
	if err != nil {
		return nil, fmt.Errorf("load players: %w", err)
	}
	for playerRows.Next() {
		var id int
		var name, first, last string
		var aliases []string
		if err := playerRows.Scan(&id, &name, &first, &last, &aliases); err != nil {
			playerRows.Close()
			return nil, err
		}
		out = append(out, cachedEntity{
			entityType: "player",
			entityID:   id,
			sport:      sport,
			match: EntityMatchInput{
				Name:      name,
				FirstName: first,
				LastName:  last,
				Aliases:   aliases,
				Sport:     sport,
			},
		})
	}
	playerRows.Close()

	log.Printf("[news] loaded entity pool sport=%s count=%d", sport, len(out))
	return out, nil
}

func sha256Hex(s string) string {
	h := sha256.Sum256([]byte(s))
	return hex.EncodeToString(h[:])
}

func nullIfEmpty(s string) interface{} {
	if s == "" {
		return nil
	}
	return s
}

func parseArticleDate(s string) time.Time {
	if s == "" {
		return time.Time{}
	}
	for _, f := range []string{
		time.RFC1123Z,
		time.RFC1123,
		time.RFC3339,
		"2006-01-02T15:04:05Z",
		"2006-01-02T15:04:05-07:00",
	} {
		if t, err := time.Parse(f, strings.TrimSpace(s)); err == nil {
			return t
		}
	}
	return time.Time{}
}

// ---------------------------------------------------------------------------
// RSS implementation
// ---------------------------------------------------------------------------

// rssResponse is the minimal XML structure for Google News RSS.
type rssResponse struct {
	XMLName xml.Name  `xml:"rss"`
	Items   []rssItem `xml:"channel>item"`
}

type rssItem struct {
	Title       string `xml:"title"`
	Link        string `xml:"link"`
	PubDate     string `xml:"pubDate"`
	Description string `xml:"description"`
}

func (s *NewsService) fetchRSS(query string, hoursBack int) ([]Article, error) {
	when := "1d"
	if hoursBack > 24 && hoursBack <= 168 {
		when = "7d"
	} else if hoursBack > 168 {
		when = "30d"
	}

	u := fmt.Sprintf(
		"https://news.google.com/rss/search?q=%s+when:%s&hl=en-US&gl=US&ceid=US:en",
		url.QueryEscape(query), when,
	)

	req, err := http.NewRequest("GET", u, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", "Mozilla/5.0 (compatible; ScoracleBot/1.0)")
	req.Header.Set("Accept", "application/rss+xml, application/xml, text/xml")

	resp, err := s.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("RSS fetch error: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("RSS HTTP %d", resp.StatusCode)
	}

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("RSS read error: %w", err)
	}

	var rss rssResponse
	if err := xml.Unmarshal(body, &rss); err != nil {
		return nil, fmt.Errorf("RSS parse error: %w", err)
	}

	articles := make([]Article, 0, len(rss.Items))
	htmlTagRe := regexp.MustCompile(`<[^>]+>`)

	for _, item := range rss.Items {
		title := item.Title
		source := "Google News"

		// Extract source from "Title - Source" format.
		if idx := strings.LastIndex(title, " - "); idx != -1 {
			source = strings.TrimSpace(title[idx+3:])
			title = strings.TrimSpace(title[:idx])
		}

		desc := htmlTagRe.ReplaceAllString(item.Description, "")
		if len(desc) > 300 {
			desc = desc[:300] + "..."
		}

		articles = append(articles, Article{
			Title:       title,
			Description: desc,
			URL:         item.Link,
			Source:      source,
			PublishedAt: item.PubDate,
			ImageURL:    nil,
		})
	}

	return articles, nil
}

func (s *NewsService) fetchFromRSS(
	entityName, sport, team string,
	limit int,
	firstName, lastName string,
	aliases []string,
) (map[string]interface{}, []Article, error) {
	sportSuffix := ""
	if sport != "" {
		if term, ok := sportTerms[strings.ToUpper(sport)]; ok {
			sportSuffix = " " + term
		}
	}

	// Build search queries: primary name first, then best alias as fallback.
	searchName := buildSearchName(entityName, firstName, lastName)
	searchQueries := []string{searchName + sportSuffix}

	// Pick the best alias for a fallback query — prefer the longest one that
	// differs meaningfully from the primary search name (likely the anglicized form).
	if best := bestAliasQuery(searchName, aliases); best != "" {
		searchQueries = append(searchQueries, best+sportSuffix)
	}

	var allArticles []Article

	for _, query := range searchQueries {
		for _, hours := range timeWindows {
			articles, err := s.fetchRSS(query, hours)
			if err != nil {
				log.Printf("[news] RSS fetch error (window=%dh): %v", hours, err)
				continue
			}

			// Filter to articles that mention the entity (by name or alias).
			matchInput := EntityMatchInput{
				Name:      entityName,
				FirstName: firstName,
				LastName:  lastName,
				Team:      team,
				Aliases:   aliases,
				Sport:     sport,
			}
			for _, a := range articles {
				if MatchesEntity(a.Title, matchInput) {
					allArticles = append(allArticles, a)
				}
			}
			allArticles = deduplicateArticles(allArticles)

			if len(allArticles) >= newsMinArticles {
				break
			}
			time.Sleep(100 * time.Millisecond)
		}
	}

	sortArticlesByDate(allArticles)
	if len(allArticles) > limit {
		allArticles = allArticles[:limit]
	}

	return map[string]interface{}{
		"query":    entityName,
		"sport":    sport,
		"articles": allArticles,
		"provider": "google_news_rss",
		"meta": map[string]interface{}{
			"total_results": len(allArticles),
			"returned":      len(allArticles),
			"source":        "google_news_rss",
		},
	}, allArticles, nil
}

// bestAliasQuery returns the best alias to use as a fallback search query.
// Prefers the longest alias that differs from the primary search name.
func bestAliasQuery(primarySearch string, aliases []string) string {
	primaryLower := strings.ToLower(primarySearch)
	best := ""
	for _, a := range aliases {
		// Skip short aliases (abbreviations) — they're too broad for search queries.
		if len(a) < 4 {
			continue
		}
		if strings.ToLower(a) == primaryLower {
			continue
		}
		if len(a) > len(best) {
			best = a
		}
	}
	return best
}

// ---------------------------------------------------------------------------
// Helpers — name matching, dedup, sort (ported from Python)
// ---------------------------------------------------------------------------

// buildSearchName shortens very long names (e.g. Brazilian players).
func buildSearchName(fullName, firstName, lastName string) string {
	parts := strings.Fields(fullName)

	// Long names (4+ parts): use first + last.
	if len(parts) >= 4 && firstName != "" && lastName != "" {
		return firstName + " " + lastName
	}

	// Names ending in Jr/Junior/II/III: use first + suffix.
	if len(parts) >= 3 {
		suffix := strings.ToLower(parts[len(parts)-1])
		if suffix == "jr" || suffix == "jr." || suffix == "junior" || suffix == "ii" || suffix == "iii" {
			return parts[0] + " " + parts[len(parts)-1]
		}
	}

	return fullName
}

// deduplicateArticles removes duplicate articles by URL.
func deduplicateArticles(articles []Article) []Article {
	seen := make(map[string]bool)
	out := make([]Article, 0, len(articles))
	for _, a := range articles {
		if a.URL != "" && !seen[a.URL] {
			seen[a.URL] = true
			out = append(out, a)
		}
	}
	return out
}

// sortArticlesByDate sorts articles by published date, newest first.
func sortArticlesByDate(articles []Article) {
	parseFmts := []string{
		time.RFC1123Z,
		time.RFC1123,
		time.RFC3339,
		"2006-01-02T15:04:05Z",
		"2006-01-02T15:04:05-07:00",
	}

	parseDate := func(s string) time.Time {
		s = strings.TrimSpace(s)
		for _, f := range parseFmts {
			if t, err := time.Parse(f, s); err == nil {
				return t
			}
		}
		return time.Time{}
	}

	sort.Slice(articles, func(i, j int) bool {
		ti := parseDate(articles[i].PublishedAt)
		tj := parseDate(articles[j].PublishedAt)
		return ti.After(tj)
	})
}
