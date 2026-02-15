package external

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"sync"
	"time"
)

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const (
	twitterBaseURL    = "https://api.twitter.com/2"
	twitterTimeout    = 15 * time.Second
	twitterCacheTTL   = 1 * time.Hour // default feed cache TTL
	twitterMaxResults = 100
)

// ---------------------------------------------------------------------------
// Tweet types — normalized output
// ---------------------------------------------------------------------------

// TweetAuthor is the author of a tweet.
type TweetAuthor struct {
	Username        string  `json:"username"`
	Name            string  `json:"name"`
	Verified        bool    `json:"verified"`
	ProfileImageURL *string `json:"profile_image_url"`
}

// TweetMetrics holds engagement metrics.
type TweetMetrics struct {
	Likes    int `json:"likes"`
	Retweets int `json:"retweets"`
	Replies  int `json:"replies"`
}

// Tweet is a normalized tweet.
type Tweet struct {
	ID        string       `json:"id"`
	Text      string       `json:"text"`
	Author    TweetAuthor  `json:"author"`
	CreatedAt string       `json:"created_at"`
	Metrics   TweetMetrics `json:"metrics"`
	URL       string       `json:"url"`
}

// ---------------------------------------------------------------------------
// TwitterService — list tweets client with in-memory feed caching
// ---------------------------------------------------------------------------

// TwitterService fetches and caches tweets from a curated X List.
type TwitterService struct {
	bearerToken string
	listID      string
	cacheTTL    time.Duration
	httpClient  *http.Client

	mu             sync.RWMutex
	cachedTweets   []Tweet
	cacheTimestamp time.Time
}

// NewTwitterService creates a twitter service. bearerToken and listID may be empty.
func NewTwitterService(bearerToken, listID string) *TwitterService {
	return &TwitterService{
		bearerToken: bearerToken,
		listID:      listID,
		cacheTTL:    twitterCacheTTL,
		httpClient: &http.Client{
			Timeout: twitterTimeout,
		},
	}
}

// IsConfigured reports whether bearer token and list ID are both set.
func (s *TwitterService) IsConfigured() bool {
	return s.bearerToken != "" && s.listID != ""
}

// CacheTTLSeconds returns the cache TTL in seconds.
func (s *TwitterService) CacheTTLSeconds() int {
	return int(s.cacheTTL.Seconds())
}

// Status returns service configuration status.
func (s *TwitterService) Status() map[string]interface{} {
	return map[string]interface{}{
		"service":                    "twitter",
		"configured":                 s.bearerToken != "",
		"journalist_list_configured": s.listID != "",
		"journalist_list_id":         s.listID,
		"feed_cache_ttl_seconds":     s.CacheTTLSeconds(),
		"rate_limit":                 "900 requests / 15 min (List endpoint)",
		"note":                       "Only journalist-feed endpoint available. Generic search removed to ensure content quality.",
	}
}

// GetJournalistFeed searches the cached journalist feed for query mentions.
func (s *TwitterService) GetJournalistFeed(query, sport string, limit int) (map[string]interface{}, error) {
	if s.bearerToken == "" {
		return nil, fmt.Errorf("Twitter API not configured. Set TWITTER_BEARER_TOKEN.")
	}
	if s.listID == "" {
		return nil, fmt.Errorf("Twitter journalist list not configured. Set TWITTER_JOURNALIST_LIST_ID.")
	}
	if limit < 1 {
		limit = 10
	}
	if limit > 50 {
		limit = 50
	}

	allTweets, feedFromCache, err := s.getOrRefreshFeed()
	if err != nil {
		return nil, err
	}

	// Filter for query matches (case-insensitive substring).
	queryLower := strings.ToLower(query)
	var filtered []Tweet
	for _, t := range allTweets {
		if strings.Contains(strings.ToLower(t.Text), queryLower) {
			filtered = append(filtered, t)
		}
	}

	if len(filtered) > limit {
		filtered = filtered[:limit]
	}

	var sportVal interface{} = sport
	if sport == "" {
		sportVal = nil
	}

	return map[string]interface{}{
		"query":  query,
		"sport":  sportVal,
		"tweets": filtered,
		"meta": map[string]interface{}{
			"result_count":      len(filtered),
			"feed_cached":       feedFromCache,
			"feed_size":         len(allTweets),
			"cache_ttl_seconds": s.CacheTTLSeconds(),
		},
	}, nil
}

// ---------------------------------------------------------------------------
// Internal — feed fetch + cache
// ---------------------------------------------------------------------------

// getOrRefreshFeed returns the cached feed or fetches a fresh one.
// Returns (tweets, fromCache, error).
func (s *TwitterService) getOrRefreshFeed() ([]Tweet, bool, error) {
	s.mu.RLock()
	if s.cachedTweets != nil && time.Since(s.cacheTimestamp) < s.cacheTTL {
		tweets := s.cachedTweets
		s.mu.RUnlock()
		return tweets, true, nil
	}
	s.mu.RUnlock()

	// Fetch fresh feed.
	tweets, err := s.fetchListTweets()
	if err != nil {
		return nil, false, err
	}

	s.mu.Lock()
	s.cachedTweets = tweets
	s.cacheTimestamp = time.Now()
	s.mu.Unlock()

	return tweets, false, nil
}

// fetchListTweets calls GET /2/lists/{id}/tweets.
func (s *TwitterService) fetchListTweets() ([]Tweet, error) {
	params := url.Values{}
	params.Set("max_results", fmt.Sprintf("%d", twitterMaxResults))
	params.Set("tweet.fields", "created_at,public_metrics,author_id")
	params.Set("user.fields", "username,name,verified,profile_image_url")
	params.Set("expansions", "author_id")

	u := fmt.Sprintf("%s/lists/%s/tweets?%s", twitterBaseURL, s.listID, params.Encode())

	req, err := http.NewRequest("GET", u, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Authorization", "Bearer "+s.bearerToken)

	resp, err := s.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("twitter API error: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("twitter API HTTP %d", resp.StatusCode)
	}

	var apiResp twitterAPIResponse
	if err := json.NewDecoder(resp.Body).Decode(&apiResp); err != nil {
		return nil, fmt.Errorf("twitter decode error: %w", err)
	}

	return formatTweets(&apiResp), nil
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

// formatTweets converts the raw API response into normalized Tweet objects.
func formatTweets(resp *twitterAPIResponse) []Tweet {
	usersMap := make(map[string]*twitterUserData, len(resp.Includes.Users))
	for i := range resp.Includes.Users {
		usersMap[resp.Includes.Users[i].ID] = &resp.Includes.Users[i]
	}

	tweets := make([]Tweet, 0, len(resp.Data))
	for _, td := range resp.Data {
		user := usersMap[td.AuthorID]
		author := TweetAuthor{
			Username: "unknown",
			Name:     "Unknown",
		}
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
