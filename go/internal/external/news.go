// Package external provides clients for third-party APIs (news, twitter).
package external

import (
	"encoding/json"
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
)

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const (
	newsDefaultLimit = 10
	newsMaxLimit     = 50
	newsMinArticles  = 3
	newsRSSTimeout   = 15 * time.Second
	newsAPITimeout   = 15 * time.Second
)

// Time windows for escalation (hours).
var timeWindows = []int{24, 48, 168}

// Sport-specific domains for NewsAPI filtering.
var sportDomains = map[string]string{
	"NBA":      "espn.com,bleacherreport.com,nba.com,theathletic.com,cbssports.com",
	"NFL":      "espn.com,bleacherreport.com,nfl.com,theathletic.com,cbssports.com",
	"FOOTBALL": "espn.com,skysports.com,bbc.com,goal.com,theathletic.com,theguardian.com",
}

// Sport-specific search term suffixes for RSS.
var sportTerms = map[string]string{
	"NBA":      "NBA basketball",
	"NFL":      "NFL football",
	"FOOTBALL": "soccer football",
}

// ---------------------------------------------------------------------------
// Article — normalized article shape shared by both sources
// ---------------------------------------------------------------------------

// Article is a normalized news article from any source.
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
// NewsService — unified Google News RSS + NewsAPI facade
// ---------------------------------------------------------------------------

// NewsService combines Google News RSS (primary) and NewsAPI (fallback).
type NewsService struct {
	apiKey     string // NewsAPI key (empty = not configured)
	httpClient *http.Client
}

// NewNewsService creates a news service. apiKey may be empty.
func NewNewsService(apiKey string) *NewsService {
	return &NewsService{
		apiKey: apiKey,
		httpClient: &http.Client{
			Timeout: newsRSSTimeout,
		},
	}
}

// HasNewsAPI reports whether a NewsAPI key is configured.
func (s *NewsService) HasNewsAPI() bool { return s.apiKey != "" }

// Status returns service configuration status.
func (s *NewsService) Status() map[string]interface{} {
	var fallback interface{}
	if s.HasNewsAPI() {
		fallback = "newsapi"
	}
	return map[string]interface{}{
		"rss_available":      true,
		"newsapi_configured": s.HasNewsAPI(),
		"primary_source":     "google_news_rss",
		"fallback_source":    fallback,
	}
}

// GetEntityNews fetches news for an entity. preferSource is "rss" | "api" | "both".
func (s *NewsService) GetEntityNews(
	entityName, sport, team, preferSource string,
	limit int,
	firstName, lastName string,
) (map[string]interface{}, error) {
	if limit < 1 {
		limit = newsDefaultLimit
	}
	if limit > newsMaxLimit {
		limit = newsMaxLimit
	}

	switch preferSource {
	case "api":
		if !s.HasNewsAPI() {
			return nil, fmt.Errorf("NewsAPI not configured")
		}
		return s.fetchFromNewsAPI(entityName, sport, limit)
	case "both":
		return s.fetchFromBoth(entityName, sport, team, limit, firstName, lastName)
	default: // "rss"
		return s.fetchFromRSS(entityName, sport, team, limit, firstName, lastName)
	}
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
) (map[string]interface{}, error) {
	searchName := buildSearchName(entityName, firstName, lastName)
	searchQuery := searchName
	if sport != "" {
		if term, ok := sportTerms[strings.ToUpper(sport)]; ok {
			searchQuery = searchName + " " + term
		}
	}

	var allArticles []Article

	for _, hours := range timeWindows {
		articles, err := s.fetchRSS(searchQuery, hours)
		if err != nil {
			log.Printf("[news] RSS fetch error (window=%dh): %v", hours, err)
			continue
		}

		// Filter to articles that mention the entity.
		for _, a := range articles {
			if nameInText(entityName, a.Title, firstName, lastName, team) {
				allArticles = append(allArticles, a)
			}
		}
		allArticles = deduplicateArticles(allArticles)

		if len(allArticles) >= newsMinArticles {
			break
		}
		time.Sleep(100 * time.Millisecond)
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

// ---------------------------------------------------------------------------
// NewsAPI implementation
// ---------------------------------------------------------------------------

type newsAPIResponse struct {
	Status       string `json:"status"`
	TotalResults int    `json:"totalResults"`
	Articles     []struct {
		Source struct {
			Name string `json:"name"`
		} `json:"source"`
		Author      *string `json:"author"`
		Title       string  `json:"title"`
		Description string  `json:"description"`
		URL         string  `json:"url"`
		URLToImage  *string `json:"urlToImage"`
		PublishedAt string  `json:"publishedAt"`
	} `json:"articles"`
	Message string `json:"message"` // on error
}

func (s *NewsService) fetchFromNewsAPI(entityName, sport string, limit int) (map[string]interface{}, error) {
	if !s.HasNewsAPI() {
		return nil, fmt.Errorf("NewsAPI not configured")
	}

	fromDate := time.Now().UTC().AddDate(0, 0, -7).Format("2006-01-02")

	params := url.Values{}
	params.Set("q", entityName)
	params.Set("from", fromDate)
	params.Set("sortBy", "relevancy")
	params.Set("pageSize", fmt.Sprintf("%d", limit))
	params.Set("language", "en")

	if domain, ok := sportDomains[strings.ToUpper(sport)]; ok {
		params.Set("domains", domain)
	}

	u := "https://newsapi.org/v2/everything?" + params.Encode()

	req, err := http.NewRequest("GET", u, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("X-Api-Key", s.apiKey)

	client := &http.Client{Timeout: newsAPITimeout}
	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("NewsAPI request error: %w", err)
	}
	defer resp.Body.Close()

	var apiResp newsAPIResponse
	if err := json.NewDecoder(resp.Body).Decode(&apiResp); err != nil {
		return nil, fmt.Errorf("NewsAPI decode error: %w", err)
	}

	if apiResp.Status != "ok" {
		return nil, fmt.Errorf("NewsAPI error: %s", apiResp.Message)
	}

	articles := make([]Article, 0, len(apiResp.Articles))
	for _, a := range apiResp.Articles {
		articles = append(articles, Article{
			Title:       a.Title,
			Description: a.Description,
			URL:         a.URL,
			Source:      a.Source.Name,
			Author:      a.Author,
			PublishedAt: a.PublishedAt,
			ImageURL:    a.URLToImage,
		})
	}

	return map[string]interface{}{
		"query":    entityName,
		"sport":    sport,
		"articles": articles,
		"provider": "newsapi",
		"meta": map[string]interface{}{
			"total_results": apiResp.TotalResults,
			"returned":      len(articles),
		},
	}, nil
}

// ---------------------------------------------------------------------------
// Combined fetch
// ---------------------------------------------------------------------------

func (s *NewsService) fetchFromBoth(
	entityName, sport, team string,
	limit int,
	firstName, lastName string,
) (map[string]interface{}, error) {
	type result struct {
		data map[string]interface{}
		err  error
	}

	var wg sync.WaitGroup
	rssCh := make(chan result, 1)
	apiCh := make(chan result, 1)

	wg.Add(1)
	go func() {
		defer wg.Done()
		d, e := s.fetchFromRSS(entityName, sport, team, limit*2, firstName, lastName)
		rssCh <- result{d, e}
	}()

	if s.HasNewsAPI() {
		wg.Add(1)
		go func() {
			defer wg.Done()
			d, e := s.fetchFromNewsAPI(entityName, sport, limit*2)
			apiCh <- result{d, e}
		}()
	}

	go func() {
		wg.Wait()
		close(rssCh)
		close(apiCh)
	}()

	rssResult := <-rssCh
	if rssResult.err != nil && !s.HasNewsAPI() {
		return nil, rssResult.err
	}

	// Merge articles, dedup by URL.
	seenURLs := make(map[string]bool)
	var merged []Article

	if rssResult.data != nil {
		if arts, ok := rssResult.data["articles"].([]Article); ok {
			for _, a := range arts {
				if a.URL != "" && !seenURLs[a.URL] {
					seenURLs[a.URL] = true
					merged = append(merged, a)
				}
			}
		}
	}

	rssCount := len(merged)
	apiCount := 0

	if s.HasNewsAPI() {
		apiResult := <-apiCh
		if apiResult.data != nil {
			if arts, ok := apiResult.data["articles"].([]Article); ok {
				apiCount = len(arts)
				for _, a := range arts {
					if a.URL != "" && !seenURLs[a.URL] {
						seenURLs[a.URL] = true
						merged = append(merged, a)
					}
				}
			}
		}
	}

	if len(merged) > limit {
		merged = merged[:limit]
	}

	return map[string]interface{}{
		"articles": merged,
		"query":    entityName,
		"sport":    sport,
		"provider": "combined",
		"meta": map[string]interface{}{
			"rss_count":    rssCount,
			"api_count":    apiCount,
			"merged_count": len(merged),
		},
	}, nil
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

// nameInText checks if an entity name appears in text with stricter matching.
func nameInText(name, text, firstName, lastName, team string) bool {
	if name == "" || text == "" {
		return false
	}
	nameLower := strings.ToLower(strings.TrimSpace(name))
	textLower := strings.ToLower(strings.TrimSpace(text))

	// Exact full name match.
	if strings.Contains(textLower, nameLower) {
		return true
	}

	// Multi-part name matching.
	nameParts := strings.Fields(nameLower)
	if len(nameParts) >= 2 {
		fn := strings.ToLower(strings.TrimSpace(firstName))
		if fn == "" {
			fn = nameParts[0]
		}
		ln := strings.ToLower(strings.TrimSpace(lastName))
		if ln == "" {
			ln = nameParts[len(nameParts)-1]
		}

		fnMatch := len(fn) > 1 && wordBoundaryMatch(fn, textLower)
		lnMatch := len(ln) > 1 && wordBoundaryMatch(ln, textLower)

		// Both first AND last name present.
		if fnMatch && lnMatch {
			return true
		}

		// Name part + team context.
		if team != "" && (fnMatch || lnMatch) {
			if strings.Contains(textLower, strings.ToLower(strings.TrimSpace(team))) {
				return true
			}
		}
	}

	return false
}

// wordBoundaryMatch checks for a whole-word match using \b.
func wordBoundaryMatch(word, text string) bool {
	re, err := regexp.Compile(`\b` + regexp.QuoteMeta(word) + `\b`)
	if err != nil {
		return strings.Contains(text, word)
	}
	return re.MatchString(text)
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
