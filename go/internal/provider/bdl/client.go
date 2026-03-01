// Package bdl provides HTTP client infrastructure shared by the NBA and NFL
// BallDontLie handlers.
//
// BDL uses cursor-based pagination and Authorization header auth.
// Rate limiting is handled via a token bucket limiter.
package bdl

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"time"

	"golang.org/x/time/rate"
)

// Client is the shared HTTP client for all BDL endpoints.
type Client struct {
	httpClient *http.Client
	baseURL    string
	apiKey     string
	limiter    *rate.Limiter
	logger     *slog.Logger
}

// NewClient creates a BDL HTTP client with rate limiting.
func NewClient(baseURL, apiKey string, requestsPerMinute int, logger *slog.Logger) *Client {
	if logger == nil {
		logger = slog.Default()
	}
	rps := float64(requestsPerMinute) / 60.0
	return &Client{
		httpClient: &http.Client{Timeout: 30 * time.Second},
		baseURL:    baseURL,
		apiKey:     apiKey,
		limiter:    rate.NewLimiter(rate.Limit(rps), 1),
		logger:     logger,
	}
}

// paginatedResponse is the common BDL response wrapper.
type paginatedResponse struct {
	Data json.RawMessage `json:"data"`
	Meta struct {
		NextCursor *int `json:"next_cursor"`
	} `json:"meta"`
}

// get performs a rate-limited GET request to a BDL endpoint.
func (c *Client) get(ctx context.Context, path string, params url.Values) (*paginatedResponse, error) {
	if err := c.limiter.Wait(ctx); err != nil {
		return nil, fmt.Errorf("rate limit wait: %w", err)
	}

	u := c.baseURL + path
	if len(params) > 0 {
		u += "?" + params.Encode()
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
	if err != nil {
		return nil, fmt.Errorf("create request: %w", err)
	}
	req.Header.Set("Authorization", c.apiKey)

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
		return nil, fmt.Errorf("BDL %s returned %d: %s", path, resp.StatusCode, truncate(body, 200))
	}

	var result paginatedResponse
	if err := json.Unmarshal(body, &result); err != nil {
		return nil, fmt.Errorf("decode response: %w", err)
	}

	return &result, nil
}

// truncate returns a truncated string representation for error messages.
func truncate(b []byte, maxLen int) string {
	if len(b) <= maxLen {
		return string(b)
	}
	return string(b[:maxLen]) + "..."
}
