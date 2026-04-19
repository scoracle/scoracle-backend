// Package ml hosts clients for local ML inference. Currently Ollama-only.
//
// We target the Ollama HTTP API running on the dev box (default
// http://localhost:11434). No external model providers — all inference
// stays local for privacy + cost predictability (free, just GPU time).
package ml

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"
)

// OllamaClient talks to a local Ollama instance. Safe for concurrent use
// (the underlying http.Client is).
type OllamaClient struct {
	baseURL    string
	model      string
	httpClient *http.Client
}

// NewOllamaClient builds a client. Pass baseURL like "http://localhost:11434"
// and model like "gemma4:e4b". Timeout bounds a single generate call —
// Gemma e4b on a consumer GPU is typically 2-8s but can spike.
func NewOllamaClient(baseURL, model string, timeout time.Duration) *OllamaClient {
	if timeout <= 0 {
		timeout = 60 * time.Second
	}
	return &OllamaClient{
		baseURL:    baseURL,
		model:      model,
		httpClient: &http.Client{Timeout: timeout},
	}
}

// Model returns the model name the client is configured with.
func (c *OllamaClient) Model() string { return c.model }

// GenerateOptions tunes a single call. Zero values mean "let Ollama default."
// JSONMode forces the model to produce valid JSON — useful for structured
// outputs like vibe blurbs where we want predictable parsing.
type GenerateOptions struct {
	System      string  // system prompt prepended to the conversation
	Temperature float64 // 0.0-2.0, lower = more deterministic
	NumPredict  int     // max tokens to generate; 0 = no limit
	JSONMode    bool    // request JSON-formatted output
}

type ollamaGenerateRequest struct {
	Model   string                 `json:"model"`
	Prompt  string                 `json:"prompt"`
	System  string                 `json:"system,omitempty"`
	Stream  bool                   `json:"stream"`
	Format  string                 `json:"format,omitempty"`
	Options map[string]interface{} `json:"options,omitempty"`
}

type ollamaGenerateResponse struct {
	Model         string `json:"model"`
	Response      string `json:"response"`
	Done          bool   `json:"done"`
	TotalDuration int64  `json:"total_duration"` // nanoseconds
	LoadDuration  int64  `json:"load_duration"`
	EvalCount     int    `json:"eval_count"`
	EvalDuration  int64  `json:"eval_duration"`
	Error         string `json:"error,omitempty"`
}

// GenerateResult holds both the text output and performance metrics.
// Callers doing debouncing or perf tuning read the metrics; callers that
// just want the answer read Response.
type GenerateResult struct {
	Response      string
	Model         string
	TotalDuration time.Duration
	EvalCount     int
}

// Generate performs a single non-streaming completion. Errors include:
//   - network failures (connection refused, timeout)
//   - Ollama returning an error payload (e.g. model not pulled)
//   - HTTP 4xx/5xx
//
// We do NOT auto-retry. Callers decide based on context (a vibe worker
// for a firing milestone might skip retry to keep the queue moving;
// a CLI test probably wants to report the error and let the human retry).
func (c *OllamaClient) Generate(ctx context.Context, prompt string, opts GenerateOptions) (*GenerateResult, error) {
	req := ollamaGenerateRequest{
		Model:  c.model,
		Prompt: prompt,
		System: opts.System,
		Stream: false,
	}
	if opts.JSONMode {
		req.Format = "json"
	}
	if opts.Temperature > 0 || opts.NumPredict > 0 {
		req.Options = map[string]interface{}{}
		if opts.Temperature > 0 {
			req.Options["temperature"] = opts.Temperature
		}
		if opts.NumPredict > 0 {
			req.Options["num_predict"] = opts.NumPredict
		}
	}

	body, err := json.Marshal(req)
	if err != nil {
		return nil, fmt.Errorf("marshal request: %w", err)
	}

	url := c.baseURL + "/api/generate"
	httpReq, err := http.NewRequestWithContext(ctx, "POST", url, bytes.NewReader(body))
	if err != nil {
		return nil, fmt.Errorf("build request: %w", err)
	}
	httpReq.Header.Set("Content-Type", "application/json")

	resp, err := c.httpClient.Do(httpReq)
	if err != nil {
		return nil, fmt.Errorf("ollama request: %w", err)
	}
	defer resp.Body.Close()

	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("read response: %w", err)
	}

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("ollama HTTP %d: %s", resp.StatusCode, truncate(string(raw), 300))
	}

	var parsed ollamaGenerateResponse
	if err := json.Unmarshal(raw, &parsed); err != nil {
		return nil, fmt.Errorf("decode response: %w (body=%s)", err, truncate(string(raw), 200))
	}
	if parsed.Error != "" {
		return nil, fmt.Errorf("ollama error: %s", parsed.Error)
	}

	return &GenerateResult{
		Response:      parsed.Response,
		Model:         parsed.Model,
		TotalDuration: time.Duration(parsed.TotalDuration),
		EvalCount:     parsed.EvalCount,
	}, nil
}

// Ping hits /api/tags to verify Ollama is reachable. Used by CLI
// preflight + health endpoints. Cheap: no model inference.
func (c *OllamaClient) Ping(ctx context.Context) error {
	req, err := http.NewRequestWithContext(ctx, "GET", c.baseURL+"/api/tags", nil)
	if err != nil {
		return err
	}
	resp, err := c.httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("ollama ping: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("ollama ping HTTP %d", resp.StatusCode)
	}
	return nil
}

func truncate(s string, max int) string {
	if len(s) <= max {
		return s
	}
	return s[:max] + "..."
}
