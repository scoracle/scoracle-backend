package thirdparty

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net/http"
	"net/url"
	"sort"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"golang.org/x/sync/singleflight"
)

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const (
	twitterBaseURL    = "https://api.twitter.com/2"
	twitterTimeout    = 15 * time.Second
	twitterMaxResults = 100
	twitterFeedLimit  = 100
)

// ErrSportNotConfigured is returned when a sport has no X list registered.
var ErrSportNotConfigured = errors.New("twitter list not configured for sport")

// ---------------------------------------------------------------------------
// Tweet types — normalized output
// ---------------------------------------------------------------------------

type TweetAuthor struct {
	Username        string  `json:"username"`
	Name            string  `json:"name"`
	Verified        bool    `json:"verified"`
	ProfileImageURL *string `json:"profile_image_url"`
}

type TweetMetrics struct {
	Likes    int `json:"likes"`
	Retweets int `json:"retweets"`
	Replies  int `json:"replies"`
}

type Tweet struct {
	ID        string       `json:"id"`
	Text      string       `json:"text"`
	Author    TweetAuthor  `json:"author"`
	CreatedAt string       `json:"created_at"`
	Metrics   TweetMetrics `json:"metrics"`
	URL       string       `json:"url"`
}

// ---------------------------------------------------------------------------
// TwitterService — per-sport lazy cache backed by Postgres
// ---------------------------------------------------------------------------

// TwitterService fetches tweets from curated X Lists on demand and caches them
// in Postgres. Feeds are refreshed only when stale (older than the list's TTL),
// so idle traffic produces zero upstream API calls. Concurrent requests against
// the same stale cache are coalesced by singleflight.
type TwitterService struct {
	pool        *pgxpool.Pool
	bearerToken string
	lists       map[string]string // sport -> list_id
	ttl         time.Duration
	httpClient  *http.Client
	flight      singleflight.Group
}

// NewTwitterService builds a service. pool may be nil (no caching — endpoint
// will report unavailable). bearerToken may be empty until X credentials land.
// lists maps lowercase sport identifiers to X List IDs.
func NewTwitterService(pool *pgxpool.Pool, bearerToken string, lists map[string]string, ttl time.Duration) *TwitterService {
	if ttl <= 0 {
		ttl = 20 * time.Minute
	}
	// Defensive copy so upstream mutations don't race with singleflight reads.
	listsCopy := make(map[string]string, len(lists))
	for k, v := range lists {
		listsCopy[strings.ToLower(k)] = v
	}
	return &TwitterService{
		pool:        pool,
		bearerToken: bearerToken,
		lists:       listsCopy,
		ttl:         ttl,
		httpClient:  &http.Client{Timeout: twitterTimeout},
	}
}

// SyncLists upserts each configured list into twitter_lists so status rows
// exist even before the first fetch. Safe to call repeatedly on startup.
func (s *TwitterService) SyncLists(ctx context.Context) error {
	if s.pool == nil {
		return nil
	}
	ttlSec := int(s.ttl.Seconds())
	for sport, listID := range s.lists {
		if _, err := s.pool.Exec(ctx, "twitter_list_upsert", sport, listID, ttlSec); err != nil {
			return fmt.Errorf("sync twitter_lists %s: %w", sport, err)
		}
	}
	return nil
}

func (s *TwitterService) HasBearerToken() bool { return s.bearerToken != "" }

func (s *TwitterService) IsConfigured(sport string) bool {
	_, ok := s.lists[strings.ToLower(sport)]
	return ok && s.bearerToken != ""
}

func (s *TwitterService) CacheTTLSeconds() int { return int(s.ttl.Seconds()) }

// Status returns per-sport configuration + cache state.
func (s *TwitterService) Status(ctx context.Context) map[string]interface{} {
	sports := make([]map[string]interface{}, 0, len(s.lists))

	dbRows := map[string]map[string]interface{}{}
	if s.pool != nil {
		rows, err := s.pool.Query(ctx, "twitter_list_status_all")
		if err == nil {
			defer rows.Close()
			for rows.Next() {
				var sport, listID string
				var ttlSec int
				var sinceID, lastError *string
				var lastFetchedAt, lastErrorAt *time.Time
				if err := rows.Scan(&sport, &listID, &ttlSec, &sinceID, &lastFetchedAt, &lastError, &lastErrorAt); err == nil {
					dbRows[sport] = map[string]interface{}{
						"list_id":         listID,
						"ttl_seconds":     ttlSec,
						"since_id":        ptrToString(sinceID),
						"last_fetched_at": timePtrToISO(lastFetchedAt),
						"last_error":      ptrToString(lastError),
						"last_error_at":   timePtrToISO(lastErrorAt),
					}
				}
			}
		}
	}

	for sport, listID := range s.lists {
		entry := map[string]interface{}{
			"sport":      sport,
			"list_id":    listID,
			"configured": s.bearerToken != "",
		}
		if state, ok := dbRows[sport]; ok {
			for k, v := range state {
				entry[k] = v
			}
		}
		sports = append(sports, entry)
	}
	sort.Slice(sports, func(i, j int) bool {
		return sports[i]["sport"].(string) < sports[j]["sport"].(string)
	})

	return map[string]interface{}{
		"service":                 "twitter",
		"bearer_token_configured": s.bearerToken != "",
		"cache_ttl_seconds":       s.CacheTTLSeconds(),
		"rate_limit":              "900 requests / 15 min (List endpoint)",
		"architecture":            "lazy_cache",
		"sports":                  sports,
	}
}

// ---------------------------------------------------------------------------
// Sport feed — lazy cache entrypoint
// ---------------------------------------------------------------------------

// GetSportFeed returns the cached tweet feed for a sport. If the cache is stale
// (past the list's TTL) it synchronously refreshes from X using since_id before
// returning. Concurrent callers for the same sport share a single upstream call.
func (s *TwitterService) GetSportFeed(ctx context.Context, sport string, limit int) (json.RawMessage, error) {
	sport = strings.ToLower(strings.TrimSpace(sport))
	if s.pool == nil {
		return nil, fmt.Errorf("database pool unavailable")
	}
	if _, ok := s.lists[sport]; !ok {
		return nil, ErrSportNotConfigured
	}
	if limit < 1 {
		limit = 25
	}
	if limit > twitterFeedLimit {
		limit = twitterFeedLimit
	}

	state, err := s.getListState(ctx, sport)
	if err != nil {
		return nil, err
	}
	if state.listID == "" {
		// Row not yet synced — upsert and proceed as a cold cache.
		if _, err := s.pool.Exec(ctx, "twitter_list_upsert", sport, s.lists[sport], s.CacheTTLSeconds()); err != nil {
			return nil, fmt.Errorf("upsert twitter_lists: %w", err)
		}
		state.listID = s.lists[sport]
		state.ttlSeconds = s.CacheTTLSeconds()
	}

	if s.isStale(state) && s.bearerToken != "" {
		_, err, _ := s.flight.Do("fetch:"+sport, func() (interface{}, error) {
			return nil, s.refresh(ctx, sport, state)
		})
		if err != nil {
			log.Printf("[twitter] refresh sport=%s failed: %v (serving stale cache)", sport, err)
			_, _ = s.pool.Exec(ctx, "twitter_list_mark_error", sport, err.Error())
		}
	}

	return s.readFeed(ctx, sport, limit)
}

// GetEntityTweets returns cached tweets linked to a specific player/team.
// Does not trigger a refresh — relies on periodic GetSportFeed calls to keep
// the linked pool fresh.
func (s *TwitterService) GetEntityTweets(ctx context.Context, sport, entityType string, entityID, limit int) (json.RawMessage, error) {
	if s.pool == nil {
		return nil, fmt.Errorf("database pool unavailable")
	}
	if limit < 1 {
		limit = 25
	}
	if limit > twitterFeedLimit {
		limit = twitterFeedLimit
	}

	var raw []byte
	err := s.pool.QueryRow(ctx, "twitter_feed_by_entity", sport, entityType, entityID, limit).Scan(&raw)
	if err != nil {
		return nil, fmt.Errorf("entity feed query: %w", err)
	}
	return raw, nil
}

// ---------------------------------------------------------------------------
// Feed read / staleness
// ---------------------------------------------------------------------------

type listState struct {
	listID        string
	ttlSeconds    int
	sinceID       *string
	lastFetchedAt *time.Time
}

func (s *TwitterService) getListState(ctx context.Context, sport string) (*listState, error) {
	var st listState
	err := s.pool.QueryRow(ctx, "twitter_list_get", sport).
		Scan(&st.listID, &st.ttlSeconds, &st.sinceID, &st.lastFetchedAt)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			return &st, nil
		}
		return nil, fmt.Errorf("read twitter_lists: %w", err)
	}
	return &st, nil
}

func (s *TwitterService) isStale(st *listState) bool {
	if st == nil || st.lastFetchedAt == nil {
		return true
	}
	ttl := time.Duration(st.ttlSeconds) * time.Second
	if ttl <= 0 {
		ttl = s.ttl
	}
	return time.Since(*st.lastFetchedAt) >= ttl
}

func (s *TwitterService) readFeed(ctx context.Context, sport string, limit int) (json.RawMessage, error) {
	var raw []byte
	err := s.pool.QueryRow(ctx, "twitter_feed_by_sport", sport, limit).Scan(&raw)
	if err != nil {
		return nil, fmt.Errorf("feed query: %w", err)
	}
	return raw, nil
}

// ---------------------------------------------------------------------------
// Refresh path — X API call, upsert, entity linking
// ---------------------------------------------------------------------------

func (s *TwitterService) refresh(ctx context.Context, sport string, st *listState) error {
	since := ""
	if st.sinceID != nil {
		since = *st.sinceID
	}

	tweets, newestID, err := s.fetchListTweets(ctx, st.listID, since)
	if err != nil {
		return err
	}

	if err := s.persistTweets(ctx, sport, tweets); err != nil {
		return fmt.Errorf("persist tweets: %w", err)
	}

	if len(tweets) > 0 {
		if err := s.linkEntities(ctx, sport, tweets); err != nil {
			// Non-fatal: feed still serves; log and continue.
			log.Printf("[twitter] entity link sport=%s failed: %v", sport, err)
		}
	}

	var sinceArg *string
	if newestID != "" {
		sinceArg = &newestID
	}
	if _, err := s.pool.Exec(ctx, "twitter_list_mark_fetched", sport, sinceArg); err != nil {
		return fmt.Errorf("mark fetched: %w", err)
	}
	return nil
}

func (s *TwitterService) persistTweets(ctx context.Context, sport string, tweets []Tweet) error {
	if len(tweets) == 0 {
		return nil
	}
	tx, err := s.pool.Begin(ctx)
	if err != nil {
		return err
	}
	defer tx.Rollback(ctx)

	for _, t := range tweets {
		postedAt, _ := time.Parse(time.RFC3339, t.CreatedAt)
		if postedAt.IsZero() {
			postedAt = time.Now().UTC()
		}
		_, err := tx.Exec(ctx, "twitter_tweet_upsert",
			t.ID, sport,
			"", // author_id — not populated to normalized Tweet struct; harmless empty.
			t.Author.Username, t.Author.Name, t.Author.Verified, t.Author.ProfileImageURL,
			t.Text, postedAt,
			t.Metrics.Likes, t.Metrics.Retweets, t.Metrics.Replies,
		)
		if err != nil {
			return err
		}
	}
	return tx.Commit(ctx)
}

// linkEntities runs the shared MatchesEntity matcher over each new tweet against
// all players and teams for the sport, writing matches to tweet_entities.
func (s *TwitterService) linkEntities(ctx context.Context, sport string, tweets []Tweet) error {
	rows, err := s.pool.Query(ctx, "twitter_entities_for_sport", strings.ToUpper(sport))
	if err != nil {
		return err
	}
	defer rows.Close()

	type entity struct {
		kind            string
		id              int
		name, fn, ln    string
		aliases         []string
	}
	var entities []entity
	for rows.Next() {
		var e entity
		if err := rows.Scan(&e.kind, &e.id, &e.name, &e.fn, &e.ln, &e.aliases); err != nil {
			return err
		}
		entities = append(entities, e)
	}

	tx, err := s.pool.Begin(ctx)
	if err != nil {
		return err
	}
	defer tx.Rollback(ctx)

	for _, t := range tweets {
		for _, e := range entities {
			if !MatchesEntity(t.Text, EntityMatchInput{
				Name:      e.name,
				FirstName: e.fn,
				LastName:  e.ln,
				Aliases:   e.aliases,
				Sport:     sport,
			}) {
				continue
			}
			if _, err := tx.Exec(ctx, "twitter_entity_link", t.ID, sport, e.kind, e.id); err != nil {
				return err
			}
		}
	}
	return tx.Commit(ctx)
}

// ---------------------------------------------------------------------------
// X API client
// ---------------------------------------------------------------------------

func (s *TwitterService) fetchListTweets(ctx context.Context, listID, sinceID string) ([]Tweet, string, error) {
	params := url.Values{}
	params.Set("max_results", fmt.Sprintf("%d", twitterMaxResults))
	params.Set("tweet.fields", "created_at,public_metrics,author_id")
	params.Set("user.fields", "username,name,verified,profile_image_url")
	params.Set("expansions", "author_id")
	if sinceID != "" {
		params.Set("since_id", sinceID)
	}

	u := fmt.Sprintf("%s/lists/%s/tweets?%s", twitterBaseURL, listID, params.Encode())

	req, err := http.NewRequestWithContext(ctx, "GET", u, nil)
	if err != nil {
		return nil, "", err
	}
	req.Header.Set("Authorization", "Bearer "+s.bearerToken)

	resp, err := s.httpClient.Do(req)
	if err != nil {
		return nil, "", fmt.Errorf("twitter API error: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, "", fmt.Errorf("twitter API HTTP %d", resp.StatusCode)
	}

	var apiResp twitterAPIResponse
	if err := json.NewDecoder(resp.Body).Decode(&apiResp); err != nil {
		return nil, "", fmt.Errorf("twitter decode error: %w", err)
	}

	// X v2 only populates meta.newest_id when the request includes since_id,
	// so cold calls return an empty cursor. Tweets come back newest-first,
	// so derive the cursor from data[0] when meta is empty — without this,
	// every refresh re-pulls 100 tweets instead of just the delta.
	newestID := apiResp.Meta.NewestID
	if newestID == "" && len(apiResp.Data) > 0 {
		newestID = apiResp.Data[0].ID
	}

	return formatTweets(&apiResp), newestID, nil
}

// ---------------------------------------------------------------------------
// Twitter API response types
// ---------------------------------------------------------------------------

type twitterAPIResponse struct {
	Data     []twitterTweetData `json:"data"`
	Includes struct {
		Users []twitterUserData `json:"users"`
	} `json:"includes"`
	Meta struct {
		ResultCount int    `json:"result_count"`
		NewestID    string `json:"newest_id"`
		OldestID    string `json:"oldest_id"`
	} `json:"meta"`
}

type twitterTweetData struct {
	ID            string `json:"id"`
	Text          string `json:"text"`
	AuthorID      string `json:"author_id"`
	CreatedAt     string `json:"created_at"`
	PublicMetrics struct {
		LikeCount    int `json:"like_count"`
		RetweetCount int `json:"retweet_count"`
		ReplyCount   int `json:"reply_count"`
		QuoteCount   int `json:"quote_count"`
	} `json:"public_metrics"`
}

type twitterUserData struct {
	ID              string  `json:"id"`
	Username        string  `json:"username"`
	Name            string  `json:"name"`
	Verified        bool    `json:"verified"`
	ProfileImageURL *string `json:"profile_image_url"`
}

func formatTweets(resp *twitterAPIResponse) []Tweet {
	usersMap := make(map[string]*twitterUserData, len(resp.Includes.Users))
	for i := range resp.Includes.Users {
		usersMap[resp.Includes.Users[i].ID] = &resp.Includes.Users[i]
	}

	tweets := make([]Tweet, 0, len(resp.Data))
	for _, td := range resp.Data {
		user := usersMap[td.AuthorID]
		author := TweetAuthor{Username: "unknown", Name: "Unknown"}
		if user != nil {
			author = TweetAuthor{
				Username:        user.Username,
				Name:            user.Name,
				Verified:        user.Verified,
				ProfileImageURL: user.ProfileImageURL,
			}
		}

		tweets = append(tweets, Tweet{
			ID:        td.ID,
			Text:      td.Text,
			Author:    author,
			CreatedAt: td.CreatedAt,
			Metrics: TweetMetrics{
				Likes:    td.PublicMetrics.LikeCount,
				Retweets: td.PublicMetrics.RetweetCount,
				Replies:  td.PublicMetrics.ReplyCount,
			},
			URL: fmt.Sprintf("https://twitter.com/%s/status/%s", author.Username, td.ID),
		})
	}

	return tweets
}

// ---------------------------------------------------------------------------
// small helpers
// ---------------------------------------------------------------------------

func ptrToString(p *string) interface{} {
	if p == nil {
		return nil
	}
	return *p
}

func timePtrToISO(t *time.Time) interface{} {
	if t == nil {
		return nil
	}
	return t.UTC().Format(time.RFC3339)
}
