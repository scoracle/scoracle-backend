// Package sportmonks provides the HTTP client for the SportMonks Football API.
//
// SportMonks uses token-based auth (query parameter), page-based pagination,
// and nested include-based relationships.
package sportmonks

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"strconv"
	"time"

	"golang.org/x/time/rate"
)

const baseURL = "https://api.sportmonks.com/v3/football"

// Client is the HTTP client for SportMonks Football endpoints.
type Client struct {
	httpClient *http.Client
	apiToken   string
	limiter    *rate.Limiter
	logger     *slog.Logger
}

// NewClient creates a SportMonks HTTP client with rate limiting.
func NewClient(apiToken string, requestsPerMinute int, logger *slog.Logger) *Client {
	if logger == nil {
		logger = slog.Default()
	}
	rps := float64(requestsPerMinute) / 60.0
	return &Client{
		httpClient: &http.Client{Timeout: 30 * time.Second},
		apiToken:   apiToken,
		limiter:    rate.NewLimiter(rate.Limit(rps), 1),
		logger:     logger,
	}
}

// paginatedResponse is the common SportMonks response wrapper.
type paginatedResponse struct {
	Data       json.RawMessage `json:"data"`
	Pagination *struct {
		HasMore bool `json:"has_more"`
	} `json:"pagination"`
}

// get performs a rate-limited GET request to a SportMonks endpoint.
func (c *Client) get(ctx context.Context, path string, params url.Values) (*paginatedResponse, error) {
	if err := c.limiter.Wait(ctx); err != nil {
		return nil, fmt.Errorf("rate limit wait: %w", err)
	}

	if params == nil {
		params = url.Values{}
	}
	params.Set("api_token", c.apiToken)

	u := baseURL + path + "?" + params.Encode()

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
	if err != nil {
		return nil, fmt.Errorf("create request: %w", err)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("http request %s: %w", path, err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("read response body: %w", err)
	}

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("SportMonks %s returned %d: %s", path, resp.StatusCode, truncate(body, 200))
	}

	var result paginatedResponse
	if err := json.Unmarshal(body, &result); err != nil {
		return nil, fmt.Errorf("decode response: %w", err)
	}

	return &result, nil
}

// getPaginated fetches all pages from a paginated endpoint.
func (c *Client) getPaginated(ctx context.Context, path string, params url.Values, perPage int) ([]json.RawMessage, error) {
	if params == nil {
		params = url.Values{}
	}
	params.Set("per_page", strconv.Itoa(perPage))

	var allData []json.RawMessage
	page := 1

	for {
		params.Set("page", strconv.Itoa(page))
		resp, err := c.get(ctx, path, params)
		if err != nil {
			return nil, err
		}

		// Data can be array or object
		var items []json.RawMessage
		if err := json.Unmarshal(resp.Data, &items); err != nil {
			// Single item response
			allData = append(allData, resp.Data)
			break
		}

		allData = append(allData, items...)

		if resp.Pagination == nil || !resp.Pagination.HasMore {
			break
		}
		page++
	}

	return allData, nil
}

// truncate returns a truncated string for error messages.
func truncate(b []byte, maxLen int) string {
	if len(b) <= maxLen {
		return string(b)
	}
	return string(b[:maxLen]) + "..."
}
