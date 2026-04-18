// Package thirdparty provides clients for third-party APIs (news, twitter).
package thirdparty

import (
	"encoding/xml"
	"fmt"
	"io"
	"log"
	"net/http"
	"net/url"
	"regexp"
	"sort"
	"strings"
	"time"
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

// NewsService fetches entity news from Google News RSS.
type NewsService struct {
	httpClient *http.Client
}

// NewNewsService creates a news service.
func NewNewsService() *NewsService {
	return &NewsService{
		httpClient: &http.Client{
			Timeout: newsRSSTimeout,
		},
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
func (s *NewsService) GetEntityNews(
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

	return s.fetchFromRSS(entityName, sport, team, limit, firstName, lastName, aliases)
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
) (map[string]interface{}, error) {
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
	}, nil
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
