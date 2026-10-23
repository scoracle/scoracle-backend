// Package thirdparty provides clients for third-party APIs.
package thirdparty

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"encoding/xml"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"regexp"
	"sort"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/work"
)

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const newsRSSTimeout = 15 * time.Second

type newsWindow struct {
	since, through time.Time
}

func (w newsWindow) hours() int {
	hours := int((w.through.Sub(w.since) + time.Hour - 1) / time.Hour)
	if hours < 1 {
		return 1
	}
	return hours
}

var errCollectionUnavailable = errors.New("coverage collection unavailable")

// Sport-specific search term suffixes for RSS.
var sportTerms = map[string]string{
	"NBA":      "NBA basketball",
	"NFL":      "NFL football",
	"FOOTBALL": "soccer football",
}

type rssEdition struct {
	name string
	hl   string
	gl   string
	ceid string
}

var defaultRSSEditions = []rssEdition{
	{name: "en-us", hl: "en-US", gl: "US", ceid: "US:en"},
}

// Keep football RSS in English until downstream models support localized reporting.
// British outlets provide English coverage of all five leagues.
var footballTeamRSSEditions = []rssEdition{
	{name: "en-gb", hl: "en-GB", gl: "GB", ceid: "GB:en"},
}

// ---------------------------------------------------------------------------
// Article — normalized article shape
// ---------------------------------------------------------------------------

// Article is a normalized news article.
type Article struct {
	Title       string `json:"title"`
	Description string `json:"description"`
	URL         string `json:"url"`
	Source      string `json:"source"`
	PublishedAt string `json:"published_at"`

	// FeedRank preserves Google's original ordering for RSS limits and queue priority.
	FeedRank int `json:"feed_rank"`

	// Private query provenance is excluded from API JSON. Dedup keeps the first lane;
	// persistence retains that raw stamp plus every entity-query edge separately.
	queryTerm    string // literal search term as sent, minus the when: token
	queryLane    string // "primary" or "alias<N>" per buildRSSSearchQueries order
	queryEdition string // ceid of the edition that returned it
	queryWindow  string // when: token, e.g. "24h"
}

// ---------------------------------------------------------------------------
// NewsService — Google News RSS client
// ---------------------------------------------------------------------------

// The endpoint is replaceable with a local RSS test server.
const defaultRSSBaseURL = "https://news.google.com/rss/search"

// NewsService fetches entity news from Google News RSS.
//
// When constructed with a non-nil pool, matched articles are persisted to
// news_articles (with headline scoring enqueued in the same transaction).
// That populates the long-term corpus consumed by the cognition layer.
type NewsService struct {
	httpClient *http.Client
	rssBaseURL string
	pool       *pgxpool.Pool
	logger     *slog.Logger
}

// NewNewsService optionally persists articles and uses slog.Default when logger is nil.
func NewNewsService(pool *pgxpool.Pool, logger *slog.Logger) *NewsService {
	if logger == nil {
		logger = slog.Default()
	}
	return &NewsService{
		httpClient: &http.Client{
			Timeout: newsRSSTimeout,
		},
		rssBaseURL: defaultRSSBaseURL,
		pool:       pool,
		logger:     logger,
	}
}

func isTeamEntity(entityType string) bool {
	return strings.EqualFold(strings.TrimSpace(entityType), "team")
}

// GetEntityNews sweeps Google News RSS and atomically persists discovery plus
// Classifier headline work. With no entity identity it only fetches.
// The first return contains article IDs whose scoring was newly queued or revised.
//
// The second return is the fetch funnel for this entity: how much of the query
// grid ran and what each filter stage discarded. It is returned even on error,
// because a sweep that times out mid-fetch still did (and dropped) real work.
func (s *NewsService) GetEntityNews(
	ctx context.Context,
	entityType string,
	entityID int,
	entityName, sport string,
	limit int,
	aliases []string,
) ([]int64, Funnel, error) {
	through := time.Now()
	window := newsWindow{since: through.Add(-24 * time.Hour), through: through}
	if s.pool != nil && entityType != "" && entityID > 0 {
		err := s.pool.QueryRow(ctx, `WITH clock AS (SELECT clock_timestamp() AS at)
			SELECT COALESCE((SELECT fetched_through FROM headline_fetch_watermarks
				WHERE entity_type=$1 AND entity_id=$2 AND sport=$3),
				(SELECT max(started_at) FROM pipeline_runs WHERE job='pipeline' AND status='success'),
				at-interval '24 hours'),at FROM clock`, entityType, entityID, strings.ToUpper(sport)).Scan(&window.since, &window.through)
		if err != nil {
			return nil, Funnel{}, fmt.Errorf("read RSS fetch progress: %w", err)
		}
	}
	if window.since.After(window.through) {
		return nil, Funnel{}, fmt.Errorf("RSS fetch progress is ahead of the database clock")
	}
	matched, funnel, err := s.fetchFromRSS(ctx, entityType, entityName, sport, limit, aliases, window)
	if err != nil {
		return nil, funnel, err
	}

	// Persist discovery and enqueue scoring in the same transaction.
	var affected []int64
	if s.pool != nil && entityType != "" && entityID > 0 {
		affected, err = s.persistArticles(ctx, sport, entityType, entityID, matched, window, funnel.RSSErrors == 0 && funnel.LimitTruncated == 0)
		if err != nil {
			return nil, funnel, fmt.Errorf("persist RSS discovery: %w", err)
		}
	}

	return affected, funnel, nil
}

// persistArticles retains every RSS query edge and queues headline scoring.
// Query provenance supplies candidate identities, never a relevance decision.
func (s *NewsService) persistArticles(
	ctx context.Context,
	sport, primaryEntityType string,
	primaryEntityID int,
	articles []Article,
	window newsWindow,
	complete bool,
) ([]int64, error) {
	tx, err := s.pool.Begin(ctx)
	if err != nil {
		return nil, err
	}
	defer tx.Rollback(ctx)

	// Fence discovery against the same transition used by queue claims/publication.
	if _, err := tx.Exec(ctx, "SELECT public.coverage_lock()"); err != nil {
		return nil, err
	}
	var open bool
	if err := tx.QueryRow(ctx, "SELECT public.coverage_collection_open()").Scan(&open); err != nil {
		return nil, err
	}
	if !open {
		return nil, fmt.Errorf("coverage closed before discovery persistence; retry on reopen")
	}

	sportUpper := strings.ToUpper(sport)

	var discovered []int64
	seen := make(map[int64]bool)

	for _, a := range articles {
		if a.URL == "" || a.Title == "" {
			continue
		}
		hash := sha256Hex(a.URL)

		var publishedAt *time.Time
		if ts := parseArticleDate(a.PublishedAt); !ts.IsZero() {
			publishedAt = &ts
		}

		// Keep first-discovery metadata on insert; every query edge is stored separately.
		rawProv := []byte(`{}`)
		if a.queryTerm != "" {
			prov := map[string]any{
				"q":       a.queryTerm,
				"lane":    a.queryLane,
				"edition": a.queryEdition,
				"window":  a.queryWindow,
			}
			if isTeamEntity(primaryEntityType) {
				prov["query_team_id"] = primaryEntityID
				// Team IDs are sport-scoped.
				prov["query_sport"] = sportUpper
			}
			if b, jerr := json.Marshal(prov); jerr == nil {
				rawProv = b
			}
		}

		var articleID, queueID int64
		err := tx.QueryRow(ctx, `
			INSERT INTO news_articles (url_hash, url, source, title, description, published_at, feed_rank, raw)
			VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
			ON CONFLICT (url_hash) DO UPDATE SET
			    title       = CASE WHEN public.headline_content_key(news_articles.title)=public.headline_content_key(EXCLUDED.title)
			        THEN news_articles.title ELSE EXCLUDED.title END,
			    duplicate_of = CASE WHEN public.headline_content_key(news_articles.title)=public.headline_content_key(EXCLUDED.title)
			        THEN news_articles.duplicate_of ELSE NULL END,
			    description = EXCLUDED.description,
			    source      = COALESCE(EXCLUDED.source, news_articles.source),
			    published_at = CASE WHEN public.headline_content_key(news_articles.title)=public.headline_content_key(EXCLUDED.title)
			        AND news_articles.source IS NOT DISTINCT FROM EXCLUDED.source
			        THEN COALESCE(news_articles.published_at, EXCLUDED.published_at)
			        ELSE COALESCE(EXCLUDED.published_at, news_articles.published_at) END,
			    feed_rank   = LEAST(COALESCE(news_articles.feed_rank, EXCLUDED.feed_rank), EXCLUDED.feed_rank)
			RETURNING id,COALESCE(duplicate_of,id)
		`, hash, a.URL, work.NullIfEmpty(a.Source), a.Title, work.NullIfEmpty(a.Description), publishedAt, a.FeedRank, rawProv).Scan(&articleID, &queueID)
		if err != nil {
			return nil, fmt.Errorf("upsert article: %w", err)
		}
		if publishedAt != nil {
			// shortcut: exact normalized reposts within 72 hours; changed wording remains new evidence.
			var canonical int64
			err := tx.QueryRow(ctx, `SELECT c.id FROM news_articles c
				WHERE c.id<$1 AND c.duplicate_of IS NULL
				AND public.headline_content_key(c.title)=public.headline_content_key($2)
				AND length(public.headline_content_key(c.title))>=30
				AND c.published_at BETWEEN $3::timestamptz-interval '72 hours' AND $3::timestamptz+interval '72 hours'
				AND EXISTS(SELECT 1 FROM harvester_query_provenance p WHERE p.article_id=c.id
					AND p.entity_type=$4 AND p.entity_id=$5 AND p.sport=$6)
				ORDER BY c.id LIMIT 1 FOR UPDATE OF c NOWAIT`, articleID, a.Title, publishedAt,
				primaryEntityType, primaryEntityID, sportUpper).Scan(&canonical)
			if err != nil && !errors.Is(err, pgx.ErrNoRows) {
				return nil, fmt.Errorf("find repost: %w", err)
			}
			if err == nil {
				queueID = canonical
				if _, err := tx.Exec(ctx, "UPDATE news_articles SET duplicate_of=$2 WHERE id=$1", articleID, canonical); err != nil {
					return nil, err
				}
			}
		}
		// Preserve every entity-query edge, including URLs another sweep first inserted.
		// This is retrieval provenance, never an authoritative article/entity link.
		hit, err := json.Marshal(map[string]any{
			"q": a.queryTerm, "lane": a.queryLane, "edition": a.queryEdition,
			"window": a.queryWindow, "feed_rank": a.FeedRank,
		})
		if err != nil {
			return nil, fmt.Errorf("encode harvester query: %w", err)
		}
		provenanceIDs := []int64{articleID}
		if queueID != articleID {
			provenanceIDs = append(provenanceIDs, queueID)
		}
		for _, provenanceID := range provenanceIDs {
			_, err = tx.Exec(ctx, `
			INSERT INTO public.harvester_query_provenance
			    (article_id, entity_type, entity_id, sport, feed_rank, query_terms)
			VALUES ($1, $2, $3, $4, $5, jsonb_build_array($6::jsonb))
			ON CONFLICT (article_id, entity_type, entity_id, sport) DO UPDATE SET
			    feed_rank = LEAST(COALESCE(harvester_query_provenance.feed_rank, EXCLUDED.feed_rank), EXCLUDED.feed_rank),
			    query_terms = CASE WHEN harvester_query_provenance.query_terms @> EXCLUDED.query_terms
			                       THEN harvester_query_provenance.query_terms
			                       ELSE harvester_query_provenance.query_terms || EXCLUDED.query_terms END,
			    last_seen_at = NOW()
		`, provenanceID, primaryEntityType, primaryEntityID, sportUpper, a.FeedRank, hit)
			if err != nil {
				return nil, fmt.Errorf("upsert harvester query provenance: %w", err)
			}
		}
		if !seen[queueID] {
			seen[queueID] = true
			discovered = append(discovered, queueID)
		}
	}

	var queued []int64
	for _, id := range discovered {
		var changed bool
		if err := tx.QueryRow(ctx, "SELECT public.classifier_enqueue_admission($1,$2)", id, sportUpper).Scan(&changed); err != nil {
			return nil, fmt.Errorf("queue Classifier admission: %w", err)
		}
		if changed {
			queued = append(queued, id)
		}
	}
	if !window.through.IsZero() {
		var recheckAfter *time.Time
		err := tx.QueryRow(ctx, `SELECT recheck_after FROM headline_fetch_watermarks
			WHERE entity_type=$1 AND entity_id=$2 AND sport=$3 FOR UPDATE`,
			primaryEntityType, primaryEntityID, sportUpper).Scan(&recheckAfter)
		if err != nil && !errors.Is(err, pgx.ErrNoRows) {
			return nil, fmt.Errorf("read catch-up recheck: %w", err)
		}
		through := window.since
		if complete {
			switch {
			case recheckAfter != nil && window.through.Before(*recheckAfter):
				// An immediate manual retry cannot substitute for the next hourly recheck.
			case recheckAfter != nil:
				through, recheckAfter = window.through, nil
			case window.through.Sub(window.since) > 2*time.Hour:
				// Recheck long catch-ups after an hour for delayed Google indexing.
				next := window.through.Add(time.Hour)
				recheckAfter = &next
			default:
				through = window.through
			}
		}
		if _, err := tx.Exec(ctx, `INSERT INTO headline_fetch_watermarks(entity_type,entity_id,sport,fetched_through,recheck_after)
			VALUES($1,$2,$3,$4,$5) ON CONFLICT(entity_type,entity_id,sport) DO UPDATE
			SET fetched_through=GREATEST(headline_fetch_watermarks.fetched_through,EXCLUDED.fetched_through),
				recheck_after=EXCLUDED.recheck_after`,
			primaryEntityType, primaryEntityID, sportUpper, through, recheckAfter); err != nil {
			return nil, fmt.Errorf("save RSS fetch progress: %w", err)
		}
	}
	if err := tx.Commit(ctx); err != nil {
		return nil, err
	}
	return queued, nil

}

func sha256Hex(s string) string {
	h := sha256.Sum256([]byte(s))
	return hex.EncodeToString(h[:])
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

// fetchRSS runs one (query, window, edition) request and returns the items that
// survive the lookback filter. It records the raw item count and the window drop
// into f — the window is the first place an article can vanish, and until now
// the only trace of it was a smaller number downstream.
func (s *NewsService) fetchRSS(ctx context.Context, query string, window newsWindow, edition rssEdition, f *Funnel) ([]Article, error) {
	u, err := url.Parse(s.rssBaseURL)
	if err != nil {
		return nil, err
	}
	q := u.Query()
	q.Set("q", fmt.Sprintf("%s when:%s", query, rssWhenToken(window.hours())))
	q.Set("hl", edition.hl)
	q.Set("gl", edition.gl)
	q.Set("ceid", edition.ceid)
	u.RawQuery = q.Encode()

	req, err := http.NewRequestWithContext(ctx, "GET", u.String(), nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", "Mozilla/5.0 (compatible; ScoracleBot/1.0)")
	req.Header.Set("Accept", "application/rss+xml, application/xml, text/xml")

	if s.pool != nil {
		var open bool
		var deadline *time.Time
		if err := s.pool.QueryRow(ctx, "WITH t AS MATERIALIZED (SELECT clock_timestamp() at) SELECT public.coverage_collection_open(at),public.coverage_collection_deadline(at) FROM t").Scan(&open, &deadline); err != nil {
			return nil, fmt.Errorf("%w: %v", errCollectionUnavailable, err)
		}
		if !open {
			return nil, fmt.Errorf("%w: collection is closed", errCollectionUnavailable)
		}
		if deadline != nil {
			requestContext, cancel := context.WithDeadline(ctx, *deadline)
			defer cancel()
			req = req.WithContext(requestContext)
		}
	}
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

	for i, item := range rss.Items {
		title := item.Title
		source := "Google News"

		// Extract source from "Title - Source" format.
		if idx := strings.LastIndex(title, " - "); idx != -1 {
			source = strings.TrimSpace(title[idx+3:])
			title = strings.TrimSpace(title[:idx])
		}

		desc := cleanRSSDescription(item.Description)
		if len(desc) > 300 {
			desc = desc[:300] + "..."
		}

		articles = append(articles, Article{
			Title:       title,
			Description: desc,
			URL:         item.Link,
			Source:      source,
			PublishedAt: item.PubDate,
			// Google's own ordering, captured before anything re-sorts. Position 0 is the
			// result Google ranked first for this query — the cheapest quality signal in the
			// pipeline and the only one that costs nothing to produce.
			FeedRank: i,
		})
	}

	kept := filterArticlesByWindow(articles, window)
	f.RSSItems += len(articles)
	f.WindowDropped += len(articles) - len(kept)
	return kept, nil
}

func (s *NewsService) fetchFromRSS(
	ctx context.Context,
	entityType string,
	entityName, sport string,
	limit int,
	aliases []string,
	window newsWindow,
) ([]Article, Funnel, error) {
	searchQueries := buildRSSSearchQueries(entityName, sport, aliases)
	editions := rssEditionsForEntity(entityType, sport)

	var allArticles []Article
	f := Funnel{
		Entities:        1,
		EditionsPlanned: len(editions),
		QueriesPlanned:  len(editions) * len(searchQueries),
	}

	// Run every alias lane before any explicitly requested acquisition limit.
	for _, edition := range editions {
		f.EditionsQueried++
		for queryIdx, query := range searchQueries {
			f.QueriesRun++
			if err := ctx.Err(); err != nil {
				return nil, f, err
			}
			f.RSSCalls++
			articles, err := s.fetchRSS(ctx, query, window, edition, &f)
			if err != nil {
				f.RSSErrors++
				if errors.Is(err, errCollectionUnavailable) {
					return nil, f, err
				}
				s.logger.Warn("RSS fetch error", "edition", edition.name, "window_hours", window.hours(), "error", err)
				continue
			}

			// Stamp query provenance here, the only place the lane index is
			// known. deduplicateArticles below keeps the first occurrence, so
			// the stamp survives cross-lane collisions with the earliest lane.
			lane := "primary"
			if queryIdx > 0 {
				lane = fmt.Sprintf("alias%d", queryIdx)
			}
			for i := range articles {
				articles[i].queryTerm = query
				articles[i].queryLane = lane
				articles[i].queryEdition = edition.ceid
				articles[i].queryWindow = rssWhenToken(window.hours())
			}

			// The decision model determines subject relevance downstream.
			beforeDedup := len(allArticles) + len(articles)
			allArticles = deduplicateArticles(append(allArticles, articles...))
			f.DedupCollapsed += beforeDedup - len(allArticles)
		}
	}

	// Stable rank ordering keeps primary-query results ahead of aliases on ties.
	sortArticlesByFeedRank(allArticles)
	totalResults, allArticles := limitRSSArticles(allArticles, limit)
	f.LimitTruncated = totalResults - len(allArticles)
	f.Matched = len(allArticles)
	for _, a := range allArticles {
		if a.Description != "" {
			f.DescriptionBearing++
		} else {
			f.DescriptionEmpty++
		}
	}

	return allArticles, f, nil
}

// Query the canonical name and every alias with the sport suffix for disambiguation.
func buildRSSSearchQueries(primarySearch, sport string, aliases []string) []string {
	suffix := rssSportSuffix(sport)
	seen := make(map[string]bool, len(aliases)+1)
	out := make([]string, 0, len(aliases)+1)

	// Deduping on a normalized key is plumbing, not judgment: asking Google the identical
	// question twice spends a call to be handed the same page.
	add := func(term string) {
		term = strings.TrimSpace(term)
		if term == "" {
			return
		}
		key := strings.ToLower(strings.Join(strings.Fields(term), " "))
		if seen[key] {
			return
		}
		seen[key] = true
		out = append(out, formatRSSQueryTerm(term)+suffix)
	}

	add(primarySearch)
	for _, alias := range aliases {
		add(alias)
	}
	return out
}

func rssSportSuffix(sport string) string {
	if sport == "" {
		return ""
	}
	if term, ok := sportTerms[strings.ToUpper(sport)]; ok {
		return " " + term
	}
	return ""
}

func rssEditionsForEntity(entityType, sport string) []rssEdition {
	if isTeamEntity(entityType) && strings.ToUpper(strings.TrimSpace(sport)) == "FOOTBALL" {
		return footballTeamRSSEditions
	}
	return defaultRSSEditions
}

func formatRSSQueryTerm(term string) string {
	term = strings.TrimSpace(strings.ReplaceAll(term, `"`, ""))
	if strings.ContainsAny(term, " \t-") {
		return `"` + term + `"`
	}
	return term
}

func rssWhenToken(hoursBack int) string {
	// Google News RSS accepts when:Nh / Nd / Ny. Use hours directly when
	// under a day and round longer gaps up; the exact cursor window filters results.
	switch {
	case hoursBack <= 0:
		return "1d"
	case hoursBack < 24:
		return fmt.Sprintf("%dh", hoursBack)
	default:
		return fmt.Sprintf("%dd", (hoursBack+23)/24)
	}
}

func filterArticlesByWindow(articles []Article, window newsWindow) []Article {
	out := make([]Article, 0, len(articles))
	for _, a := range articles {
		published := parseArticleDate(a.PublishedAt)
		if !published.IsZero() && !published.Before(window.since) && !published.After(window.through) {
			out = append(out, a)
		}
	}
	return out
}

var (
	rssHTMLTagRe    = regexp.MustCompile(`<[^>]+>`)
	rssEntityRe     = regexp.MustCompile(`&(nbsp|amp|quot|apos|lt|gt|#3[49]|#160);`)
	rssWhitespaceRe = regexp.MustCompile(`\s+`)
)

// cleanRSSDescription renders a Google News RSS <description> as plain text. It strips the anchor
// markup, decodes the handful of entities Google actually emits, and collapses whitespace.
//
// This is data hygiene, not interpretation. The raw field is
// `<a href="...">Headline</a>&nbsp;&nbsp;<font>Source</font>`, so tag-stripping alone left literal
// "&nbsp;&nbsp;" glued between the headline and the outlet name — which then reached the model's
// prompt verbatim and was embedded as if it were prose. 99.7% of descriptions are the title
// repeated plus the source, so what this mostly does is make that redundancy legible to the
// consumers that now have to decide whether the field adds anything.
func cleanRSSDescription(raw string) string {
	s := rssHTMLTagRe.ReplaceAllString(raw, " ")
	s = rssEntityRe.ReplaceAllStringFunc(s, func(e string) string {
		switch e {
		case "&amp;":
			return "&"
		case "&quot;", "&#34;":
			return `"`
		case "&apos;", "&#39;":
			return "'"
		case "&lt;":
			return "<"
		case "&gt;":
			return ">"
		default: // &nbsp; / &#160;
			return " "
		}
	})
	return strings.TrimSpace(rssWhitespaceRe.ReplaceAllString(s, " "))
}

func limitRSSArticles(articles []Article, limit int) (int, []Article) {
	total := len(articles)
	if limit > 0 && len(articles) > limit {
		return total, articles[:limit]
	}
	return total, articles
}

// deduplicateArticles removes duplicate articles by normalized source key.
func deduplicateArticles(articles []Article) []Article {
	seen := make(map[string]bool)
	out := make([]Article, 0, len(articles))
	for _, a := range articles {
		keys := articleDedupKeys(a)
		if len(keys) == 0 {
			out = append(out, a)
			continue
		}
		if hasSeenArticleKey(seen, keys) {
			continue
		}
		for _, key := range keys {
			seen[key] = true
		}
		out = append(out, a)
	}
	return out
}

func articleDedupKeys(a Article) []string {
	keys := make([]string, 0, 2)
	if raw := strings.TrimSpace(a.URL); raw != "" {
		u, err := url.Parse(raw)
		if err == nil && strings.EqualFold(u.Hostname(), "news.google.com") {
			u.RawQuery = ""
			u.Fragment = ""
			keys = append(keys, "url:"+strings.ToLower(u.String()))
		} else {
			keys = append(keys, "url:"+raw)
		}
	}
	title := strings.Join(strings.Fields(a.Title), " ")
	source := strings.Join(strings.Fields(a.Source), " ")
	key := strings.TrimSpace(title + " " + source)
	if key != "" {
		keys = append(keys, "title_source:"+strings.ToLower(key))
	}
	return keys
}

func hasSeenArticleKey(seen map[string]bool, keys []string) bool {
	for _, key := range keys {
		if seen[key] {
			return true
		}
	}
	return false
}

// sortArticlesByFeedRank orders articles by the position Google gave them, best first.
//
// Stable on purpose: FeedRank is per-query, so the primary query's rank-3 and an alias
// lane's rank-3 collide. Ties then resolve to the order the queries actually ran, which
// puts the entity's own name ahead of its aliases.
func sortArticlesByFeedRank(articles []Article) {
	sort.SliceStable(articles, func(i, j int) bool {
		return articles[i].FeedRank < articles[j].FeedRank
	})
}
