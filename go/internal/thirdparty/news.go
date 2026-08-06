// Package thirdparty provides clients for third-party APIs.
package thirdparty

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"encoding/xml"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"regexp"
	"sort"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/work"
)

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const (
	newsMinArticles = 3
	newsRSSTimeout  = 15 * time.Second
	// Allow a small overlap around the cron boundary so a slightly late run does
	// not miss items published just before the lookback boundary.
	newsLookbackSlack = 15 * time.Minute
)

// RSS query lookback windows (hours). This MUST cover the ingest cron's period or
// the sweep is blind between runs: the window is both the `when:` token handed to
// Google News and the client-side `filterArticlesByLookback` cutoff, so anything
// older than it is dropped twice over and never reaches news_articles at all.
//
// 24h to match the daily 02:00 ingest (2026-07-26). The pairing has moved together
// every time: 6h window with the six-hour cron, 12h when that went twelve-hourly.
// A day's news now arrives in one sweep, and per-entity volume is bounded by
// `-rss-limit` (one Google page) rather than by how often the cron happens to fire.
var timeWindows = []int{24}

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

// footballTeamRSSEditions is ENGLISH-ONLY, deliberately, until the cognition layer can read other
// languages first-hand.
//
// The localized editions (es-es, fr-fr, de-de, it-it, pt-pt, nl-nl) were live from 2026-07-23 and
// measurably worked — they made the corpus only 24.1% English, with 45.3% definitively non-English.
// The problem is that nothing downstream was ready for that:
//
//   - the candle's embedder is `bge-small-en-v1.5`, an ENGLISH-ONLY model, so it scored three
//     quarters of the corpus on text its weights never saw;
//   - the Article Reader was left translating AND summarizing in one 7B call on 76% of articles;
//   - every prompt, guard list, and stopword downstream is English.
//
// Restore the localized editions when a model that reads them natively is in place (Scott's call:
// at the 30B tier). Until then, ingesting text we cannot read is buying GPU load and noise, not
// coverage. Re-adding them is this one slice — the edition machinery is untouched and still tested.
// en-GB alone. We cover five leagues — Premier League (England), La Liga (Spain), Serie A (Italy),
// Ligue 1 (France), Bundesliga (Germany) — so by the "only fetch countries whose leagues we cover"
// rule the correct edition set is en-GB + es/it/fr/de. The four localized ones are PARKED, not
// rejected, until a model that reads them exists; en-GB is kept because British outlets cover all
// five leagues in English, and en-US was dropped because it duplicates that coverage more thinly.
//
// The named consequence, so nobody rediscovers it as a surprise: four of our five leagues are now
// covered only by English-language reporting of them. That is a deliberate accuracy-over-breadth
// trade while the pipeline is GPU-bound.
var footballTeamRSSEditions = []rssEdition{
	{name: "en-gb", hl: "en-GB", gl: "GB", ceid: "GB:en"},
}

const broadTeamPrimaryConfidence = 0.95

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

	// FeedRank is this article's 0-based position in the RSS payload Google returned — its own
	// ranking of what matters most for the query. Captured at parse time, before any reordering
	// downstream can destroy it.
	//
	// This is the primary quality signal in the ingest path, and it is free. It decides which
	// articles survive `-rss-limit` here, and which are worth a model call once the reading
	// budget binds later. Both cuts rank by it, so "top N" means the same thing at every stage.
	FeedRank int `json:"feed_rank"`

	// Query provenance — which (query, lane, edition, window) request surfaced this
	// article. The query IS the hypothesis: every article is here because we asked
	// Google a specific question, and until now that question was discarded at fetch
	// time. persistArticles writes these into news_articles.raw on first insert.
	//
	// Unexported on purpose: provenance is for the corpus, not the API response, and
	// unexported fields stay out of the JSON marshal. Dedup keeps the first occurrence,
	// so an article seen by several lanes carries the lane that found it first (the
	// sweep runs primary before aliases — first-writer wins is also best-writer wins).
	queryTerm    string // literal search term as sent, minus the when: token
	queryLane    string // "primary" or "alias<N>" per buildRSSSearchQueries order
	queryEdition string // ceid of the edition that returned it
	queryWindow  string // when: token, e.g. "24h"
}

// ---------------------------------------------------------------------------
// NewsService — Google News RSS client
// ---------------------------------------------------------------------------

// defaultRSSBaseURL is the Google News RSS search endpoint. Held in a field
// rather than inlined so the funnel accounting can be exercised against a local
// test server instead of the live endpoint.
const defaultRSSBaseURL = "https://news.google.com/rss/search"

// NewsService fetches entity news from Google News RSS.
//
// When constructed with a non-nil pool, every matched article is also
// persisted to news_articles + news_article_entities as a write-through.
// That populates the long-term corpus consumed by the cognition layer.
type NewsService struct {
	httpClient *http.Client
	rssBaseURL string
	pool       *pgxpool.Pool
	logger     *slog.Logger
}

// NewNewsService creates a news service. If pool is non-nil, fetched
// articles are written to news_articles / news_article_entities on each
// successful entity match. If logger is nil, a default no-op logger is used.
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

// Status returns service configuration status.
func (s *NewsService) Status() map[string]interface{} {
	return map[string]interface{}{
		"rss_available":  true,
		"primary_source": "google_news_rss",
	}
}

// isTeamEntity reports whether entityType is the team grain. Moved here from the deleted
// match.go (PLAN-one-rail 8.8) — it is not relevance machinery, it is a grain test, and every
// remaining caller is in this file: query building, provenance, and the edition/lane plan.
func isTeamEntity(entityType string) bool {
	return strings.EqualFold(strings.TrimSpace(entityType), "team")
}

// GetEntityNews fetches news for an entity via Google News RSS.
// entityType ("player"|"team") and entityID drive the write-through —
// matched articles are linked back to the requested entity in
// news_article_entities. Pass entityType="" / entityID=0 to skip write-through.
//
// The second return is the article IDs that gained a fresh link on this call.
// It is informational — persistArticles already enqueues the Editor's read in
// its own transaction. It is nil when write-through is skipped or the persist
// failed.
//
// The third return is the fetch funnel for this entity: how much of the query
// grid ran and what each filter stage discarded. It is returned even on error,
// because a sweep that times out mid-fetch still did (and dropped) real work.
func (s *NewsService) GetEntityNews(
	ctx context.Context,
	entityType string,
	entityID int,
	entityName, sport, team string,
	limit int,
	firstName, lastName string,
	aliases []string,
) (map[string]interface{}, []int64, Funnel, error) {
	result, matched, funnel, err := s.fetchFromRSS(ctx, entityType, entityName, sport, team, limit, firstName, lastName, aliases)
	if err != nil {
		return nil, nil, funnel, err
	}

	// Write-through: persist the matched articles and link them to this entity
	// (persistArticles also enqueues the Editor's read in-txn).
	// Non-fatal — a failed persist shouldn't break the response. affected is the
	// set of articles that gained a NEW link this call; it stays nil on the
	// serving path / on persist failure.
	var affected []int64
	if s.pool != nil && entityType != "" && entityID > 0 && len(matched) > 0 {
		ids, perr := s.persistArticles(ctx, sport, entityType, entityID, matched)
		if perr != nil {
			s.logger.Warn("persist failed", "sport", sport, "entity_type", entityType, "entity_id", entityID, "error", perr)
		} else {
			affected = ids
		}
	}

	return result, affected, funnel, nil
}

// persistArticles upserts articles by URL hash and links them to the primary
// requested entity — the one we asked Google about. That link IS the query
// hypothesis (broadTeamPrimaryConfidence), and the Editor confirms or denies it
// downstream having read the body (PLAN-one-rail 8.5).
//
// Runs in a single transaction, and the transaction also opens the pipe end: the
// article is enqueued for the Editor on first sighting, so new material flows on
// commit rather than on a sweep's cadence.
//
// The cross-entity secondary pass that used to run here is gone (8.8). It scanned
// a cached pool of every team and player in the sport against the title with a
// regex matcher and wrote a 0.8 link on any hit — guessing relevance beside the
// thing that now decides it by reading. Google's ranking picks the articles; the
// Editor picks the links.
//
// Errors are returned to the caller but don't break the response path — the
// caller logs and moves on. The returned slice is the article IDs that gained a
// NEW entity link (RowsAffected>0); it is informational. An article re-seen with
// only pre-existing links is omitted.
func (s *NewsService) persistArticles(
	ctx context.Context,
	sport, primaryEntityType string,
	primaryEntityID int,
	articles []Article,
) ([]int64, error) {
	tx, err := s.pool.Begin(ctx)
	if err != nil {
		return nil, err
	}
	defer tx.Rollback(ctx)

	sportUpper := strings.ToUpper(sport)

	affected := make(map[int64]bool)   // fresh primary link — the informational return
	needEditor := make(map[int64]bool) // fresh ARTICLE ROW → the greenfield Editor reads it once

	for _, a := range articles {
		if a.URL == "" || a.Title == "" {
			continue
		}
		hash := sha256Hex(a.URL)

		var publishedAt *time.Time
		if ts := parseArticleDate(a.PublishedAt); !ts.IsZero() {
			publishedAt = &ts
		}

		// Query provenance (Phase 2, PLAN-one-rail): the question we asked Google,
		// written on INSERT ONLY — the conflict branch never touches raw, so the
		// first sweep that lands an article owns its provenance and re-seen
		// articles keep theirs. query_team_id records which team's sweep asked;
		// the sweep is teams-only by doctrine, so the guard is belt-and-braces
		// for any non-team caller of GetEntityNews.
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
			}
			if b, jerr := json.Marshal(prov); jerr == nil {
				rawProv = b
			}
		}

		// (xmax = 0) distinguishes a fresh INSERT from a conflict-update in the same
		// statement — the standard idiom, used here so the editor enqueue below fires
		// once per article's first sighting rather than once per sweep that re-sees it.
		var articleID int64
		var inserted bool
		err := tx.QueryRow(ctx, `
			INSERT INTO news_articles (url_hash, url, source, title, description, published_at, feed_rank, raw)
			VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
			ON CONFLICT (url_hash) DO UPDATE SET
			    title       = EXCLUDED.title,
			    description = EXCLUDED.description,
			    source      = COALESCE(EXCLUDED.source, news_articles.source),
			    published_at = COALESCE(EXCLUDED.published_at, news_articles.published_at),
			    feed_rank   = LEAST(COALESCE(news_articles.feed_rank, EXCLUDED.feed_rank), EXCLUDED.feed_rank)
			RETURNING id, (xmax = 0)
		`, hash, a.URL, nullIfEmpty(a.Source), a.Title, nullIfEmpty(a.Description), publishedAt, a.FeedRank, rawProv).Scan(&articleID, &inserted)
		if err != nil {
			return nil, fmt.Errorf("upsert article: %w", err)
		}
		if inserted {
			needEditor[articleID] = true
		}

		// Primary link — the entity that was queried, and the ONE link ingest writes.
		// It lands at broadTeamPrimaryConfidence rather than 1.0 because it is a
		// hypothesis, not a finding: we asked Google about this entity, so the article
		// is *probably* about it. Nothing here has read the body, so nothing here
		// vets. `title_pos` is not written any more — it was computed by a regex to
		// feed a proximity gate no consumer on this rail reads (8.8).
		primaryConfidence := broadTeamPrimaryConfidence
		ct, err := tx.Exec(ctx, `
			INSERT INTO news_article_entities (article_id, entity_type, entity_id, sport, match_confidence)
			VALUES ($1, $2, $3, $4, $5)
			ON CONFLICT (article_id, entity_type, entity_id, sport) DO NOTHING
		`, articleID, primaryEntityType, primaryEntityID, sportUpper, primaryConfidence)
		if err != nil {
			return nil, fmt.Errorf("link primary entity: %w", err)
		}
		if ct.RowsAffected() > 0 {
			affected[articleID] = true
		}
	}

	// The greenfield Editor reads EVERY new article once (PLAN-one-rail 3.5) — same tx, so
	// new material reaches cognition on commit (mig-150 NOTIFY). Keyed on fresh INSERTs
	// rather than fresh links: the Editor's unit is the article, and a re-seen URL keeps its
	// read. Duplicate enqueues collapse on the pipeline_work PK without yanking a live lease.
	// The stage drains only where COGNITION_STAGES includes 'editor' (archbox).
	for id := range needEditor {
		if err := work.Enqueue(ctx, tx, work.Item{
			Stage:      work.StageEditor,
			EntityType: "article",
			EntityID:   int(id),
			Sport:      sportUpper,
		}); err != nil {
			return nil, err
		}
	}

	if err := tx.Commit(ctx); err != nil {
		return nil, err
	}

	ids := make([]int64, 0, len(affected))
	for id := range affected {
		ids = append(ids, id)
	}
	return ids, nil
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

// fetchRSS runs one (query, window, edition) request and returns the items that
// survive the lookback filter. It records the raw item count and the window drop
// into f — the window is the first place an article can vanish, and until now
// the only trace of it was a smaller number downstream.
func (s *NewsService) fetchRSS(ctx context.Context, query string, hoursBack int, edition rssEdition, f *Funnel) ([]Article, error) {
	u, err := url.Parse(s.rssBaseURL)
	if err != nil {
		return nil, err
	}
	q := u.Query()
	q.Set("q", fmt.Sprintf("%s when:%s", query, rssWhenToken(hoursBack)))
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
			ImageURL:    nil,
			// Google's own ordering, captured before anything re-sorts. Position 0 is the
			// result Google ranked first for this query — the cheapest quality signal in the
			// pipeline and the only one that costs nothing to produce.
			FeedRank: i,
		})
	}

	kept := filterArticlesByLookback(articles, hoursBack, time.Now())
	f.RSSItems += len(articles)
	f.WindowDropped += len(articles) - len(kept)
	return kept, nil
}

func (s *NewsService) fetchFromRSS(
	ctx context.Context,
	entityType string,
	entityName, sport, team string,
	limit int,
	firstName, lastName string,
	aliases []string,
) (map[string]interface{}, []Article, Funnel, error) {
	searchName := buildSearchName(entityName, firstName, lastName)
	searchQueries := buildRSSSearchQueries(searchName, sport, aliases)
	editions := rssEditionsForEntity(entityType, sport)

	var allArticles []Article
	f := Funnel{
		Entities:        1,
		EditionsPlanned: len(editions),
		QueriesPlanned:  len(editions) * len(searchQueries),
	}

	// EVERY lane runs, every time. The `-rss-limit` cap used to end the query plan early, and a
	// hand-rolled exemption (`runRSSQueryPastLimit`) existed on top of it to guarantee team lanes
	// survived the cap — an exemption that only made sense while alias lanes were RANKED and the
	// good ones needed protecting. With every name asked equally there is nothing to protect and
	// nothing to rank: the plan is small (1.8–2.0 aliases per team on average, 7 at the outlier)
	// and the cap belongs on the RESULTS, where `limitRSSArticles` applies it after Google's own
	// feed rank has decided which articles are the best ones to keep (PLAN-one-rail 8.9).
	for _, edition := range editions {
		f.EditionsQueried++
		for queryIdx, query := range searchQueries {
			f.QueriesRun++
			for _, hours := range timeWindows {
				if err := ctx.Err(); err != nil {
					return nil, nil, f, err
				}
				f.RSSCalls++
				articles, err := s.fetchRSS(ctx, query, hours, edition, &f)
				if err != nil {
					f.RSSErrors++
					s.logger.Warn("RSS fetch error", "edition", edition.name, "window_hours", hours, "error", err)
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
					articles[i].queryWindow = rssWhenToken(hours)
				}

				// No relevance filter here. This used to run an entity matcher over
				// title+description and drop whatever did not name the entity — 4,859 of
				// 9,694 articles on the 2026-07-26 sweep, half of everything Google
				// returned, and fifteen clubs (Nice, Spezia, Leganés, Huesca …) admitted
				// nothing at all because their names are short or ambiguous.
				//
				// It was re-deriving something the query already established: we asked
				// Google for this entity, so the link IS the query. Relevance is decided
				// once, downstream, by something that reads the body instead of
				// pattern-matching a headline. The matcher itself is gone as of 8.8, and
				// with it the funnel's MatchRejected counter — a filter returning here
				// would owe the Residual invariant a counter of its own.
				beforeDedup := len(allArticles) + len(articles)
				allArticles = deduplicateArticles(append(allArticles, articles...))
				f.DedupCollapsed += beforeDedup - len(allArticles)

				if limit > 0 && len(allArticles) >= limit {
					break
				}
				if len(allArticles) >= newsMinArticles {
					break
				}
				if err := sleepContext(ctx, 100*time.Millisecond); err != nil {
					return nil, nil, f, err
				}
			}
		}
	}

	// Cut by Google's ranking, not by recency. This sort decides WHICH articles survive
	// `-rss-limit`, and sorting by date here meant the cap kept the newest rather than the
	// best: on the 2026-07-26 sweep that discarded 2,235 of 9,694 articles on publish time
	// alone, while the read budget downstream was separately ranking by `feed_rank` — two
	// different notions of "top N" in one path.
	//
	// Ranking relevance is the one thing Google is unambiguously better at than anything
	// here, and FeedRank is that judgment for free. Recency is not this product's axis; the
	// entity's story is, and a well-ranked piece from this morning beats a thin one from
	// twenty minutes ago. Stable, so that at equal rank the primary query still outranks the
	// alias lanes that ran after it.
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

	return map[string]interface{}{
		"query":    entityName,
		"sport":    sport,
		"articles": allArticles,
		"provider": "google_news_rss",
		"meta": map[string]interface{}{
			"total_results": totalResults,
			"returned":      len(allArticles),
			"source":        "google_news_rss",
		},
	}, allArticles, f, nil
}

// buildRSSSearchQueries returns ONE Google query per name we know this entity by: the canonical
// search name first, then every alias, each carrying the sport term.
//
// PLAN-one-rail 8.9. What used to live here was ~350 lines of pre-AI judgment deciding which
// alias was "safe" enough to ask about — hand-tuned lane scores (base 60, +20 for two tokens,
// +25 for a shared token, +10 for a club designator), an 18-word hardcoded list of risky solo
// club words (athletic, celtic, city, inter, nice, real, united …), four trusted literals
// (barca, barça, spurs, juve), and a hand-maintained short-alias allowlist. Every line of it
// existed to protect a downstream regex from whatever came back. Nothing downstream guesses any
// more: Google ranks, and the Editor reads the body and decides. So we ask about every name we
// have and let the two things that are genuinely good at this do the work.
//
// Aliases earn their lane on measurement, not on principle — sampled live 2026-08-06, an alias
// lane returns 16–29 links the canonical-name lane never saw (Spurs 18/47, Barça 29/102, PSG
// 16/47). Dropping to one query per entity would have thrown that away.
//
// The sport term is applied UNIFORMLY — every sport, every entity kind, every lane. That one
// rule replaced the per-term suffix branching, and it is not scar tissue: measured the same day,
// bare "Nice" returns the NHS institute (NICE), pharma press releases and Formula 1, while
// "Nice soccer football" returns OGC Nice. The suffix is the context Google needs to rank the
// right entity, which is precisely the job being handed to it.
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
	// under a day so the six-hour cron searches only the matching lookback.
	switch {
	case hoursBack <= 0:
		return "1d"
	case hoursBack < 24:
		return fmt.Sprintf("%dh", hoursBack)
	case hoursBack <= 168:
		return fmt.Sprintf("%dd", (hoursBack+23)/24)
	default:
		return "30d"
	}
}

func filterArticlesByLookback(articles []Article, hoursBack int, now time.Time) []Article {
	if hoursBack <= 0 || now.IsZero() {
		return articles
	}
	cutoff := now.Add(-time.Duration(hoursBack)*time.Hour - newsLookbackSlack)
	out := make([]Article, 0, len(articles))
	for _, a := range articles {
		published := parseArticleDate(a.PublishedAt)
		if published.IsZero() || !published.Before(cutoff) {
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

func sleepContext(ctx context.Context, d time.Duration) error {
	timer := time.NewTimer(d)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}

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

