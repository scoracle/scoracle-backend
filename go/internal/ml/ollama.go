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
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net"
	"net/http"
	"strings"
	"sync"
	"syscall"
	"time"
)

// ---------------------------------------------------------------------------
// Shared GPU concurrency governor (FIRST-GPT-AUDIT Session 14)
//
// One 8GB GPU runs a single gemma4:e4b instance, so every Gemma caller in this
// process — the in-API derive worker, the maintenance scrub ticker, the nightly
// cron's drainer — must share one concurrency bound or they pile onto the card and
// each call's wall time balloons with queue wait (the F-014 timeout trigger). The
// gate is process-wide (package-level), not per-client, so it bounds all of them
// together regardless of how many OllamaClient values exist. Default 1 = full
// serialization; raise via OLLAMA_MAX_CONCURRENT only if the hardware changes.
// ---------------------------------------------------------------------------

var (
	gemmaGateMu sync.Mutex
	gemmaGate   chan struct{}
	gemmaGateN  = 1
)

// SetGemmaConcurrency sets the process-wide cap on concurrent Gemma calls. Call
// once at startup, before any Generate (long-lived binaries: the API + the cron
// jobs). n <= 0 is treated as 1. Binaries that never call it serialize at 1.
func SetGemmaConcurrency(n int) {
	if n <= 0 {
		n = 1
	}
	gemmaGateMu.Lock()
	defer gemmaGateMu.Unlock()
	gemmaGateN = n
	gemmaGate = make(chan struct{}, n)
}

// gemmaAcquire blocks until a GPU slot is free or ctx is cancelled. It returns a
// release func bound to the exact gate it acquired, so a startup reconfigure can
// never cause a release on the wrong channel.
func gemmaAcquire(ctx context.Context) (func(), error) {
	gemmaGateMu.Lock()
	if gemmaGate == nil {
		gemmaGate = make(chan struct{}, gemmaGateN)
	}
	g := gemmaGate
	gemmaGateMu.Unlock()
	select {
	case g <- struct{}{}:
		return func() { <-g }, nil
	case <-ctx.Done():
		return func() {}, ctx.Err()
	}
}

// IsUnavailable reports whether err means Ollama itself is unreachable (process
// down, refusing connections, DNS gone) — an OUTAGE — as opposed to a slow call
// that timed out or a per-item generation/parse error. Callers use it to defer work
// WITHOUT burning a retry attempt during an outage (FIRST-GPT-AUDIT Session 14): a
// down model must never dead-letter good work. A timeout / context-deadline is
// deliberately NOT "unavailable" — that is a genuinely-too-slow op that should fail
// and back off normally.
func IsUnavailable(err error) bool {
	if err == nil {
		return false
	}
	if errors.Is(err, context.DeadlineExceeded) || errors.Is(err, context.Canceled) {
		return false
	}
	if errors.Is(err, syscall.ECONNREFUSED) {
		return true
	}
	var netErr *net.OpError
	if errors.As(err, &netErr) {
		if netErr.Timeout() {
			return false // a slow op against a reachable server, not an outage
		}
		return true // dial/connect/reset against the ollama socket
	}
	s := err.Error()
	if strings.Contains(s, "Client.Timeout") || strings.Contains(s, "context deadline") {
		return false
	}
	return strings.Contains(s, "connection refused") ||
		strings.Contains(s, "no such host") ||
		strings.Contains(s, "ollama ping") ||
		strings.Contains(s, "dial tcp") ||
		strings.Contains(s, "connect: connection")
}

// OllamaClient talks to a local Ollama instance. Safe for concurrent use
// (the underlying http.Client is).
type OllamaClient struct {
	baseURL    string
	model      string
	httpClient *http.Client
	keepAlive  string // model residency after a call ("30m"); "" = Ollama default
}

// NewOllamaClient builds a client. Pass baseURL like "http://localhost:11434"
// and model like "gemma4:e4b". timeout is the HTTP hard BACKSTOP — it must be at
// least the longest per-operation deadline (narrative generation), because shorter
// operations are bounded tighter by their own per-call context (FIRST-GPT-AUDIT
// Session 14). It is not the per-op budget; it only stops a wedged socket from
// hanging forever past every context. Default residency is 30m (override with
// SetKeepAlive) so the model stays warm between calls and rarely cold-reloads.
func NewOllamaClient(baseURL, model string, timeout time.Duration) *OllamaClient {
	if timeout <= 0 {
		timeout = 300 * time.Second
	}
	return &OllamaClient{
		baseURL:    baseURL,
		model:      model,
		httpClient: &http.Client{Timeout: timeout},
		keepAlive:  "30m",
	}
}

// SetKeepAlive overrides how long Ollama keeps the model resident after a call
// ("30m", "60m", "-1" = forever, "0" = unload immediately). Call at startup.
func (c *OllamaClient) SetKeepAlive(v string) {
	if v != "" {
		c.keepAlive = v
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
	Op          string  // operation label for per-call timing logs (e.g. "scrub", "narratives")
}

type ollamaGenerateRequest struct {
	Model     string                 `json:"model"`
	Prompt    string                 `json:"prompt"`
	System    string                 `json:"system,omitempty"`
	Stream    bool                   `json:"stream"`
	Format    string                 `json:"format,omitempty"`
	KeepAlive string                 `json:"keep_alive,omitempty"`
	Options   map[string]interface{} `json:"options,omitempty"`
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
func (c *OllamaClient) Generate(ctx context.Context, prompt string, opts GenerateOptions) (res *GenerateResult, err error) {
	// Shared GPU governor: only OllamaMaxConcurrent calls run at once across this
	// whole process. Acquired before the request so callers queue HERE (cheap, cancellable)
	// rather than all piling onto Ollama's socket. Respects ctx — a per-op deadline or a
	// shutdown unblocks the wait instead of stranding the goroutine.
	release, gerr := gemmaAcquire(ctx)
	if gerr != nil {
		return nil, fmt.Errorf("gemma gate: %w", gerr)
	}
	defer release()

	op := opts.Op
	if op == "" {
		op = "gemma"
	}
	wallStart := time.Now()
	// Per-call timing + outcome metric (FIRST-GPT-AUDIT Session 14): one line per
	// Gemma call so an operator sees model latency, not just job outcome. wall_ms
	// includes any in-process gate wait; a timeout/outage is flagged distinctly.
	defer func() {
		wallMs := time.Since(wallStart).Milliseconds()
		if err != nil {
			reason := "error"
			switch {
			case IsUnavailable(err):
				reason = "unavailable"
			case errors.Is(err, context.DeadlineExceeded) || strings.Contains(err.Error(), "Client.Timeout"):
				reason = "timeout"
			case errors.Is(err, context.Canceled):
				reason = "canceled"
			}
			slog.Warn("gemma call failed", "op", op, "model", c.model, "wall_ms", wallMs, "reason", reason, "error", err)
			return
		}
		evalCount := 0
		if res != nil {
			evalCount = res.EvalCount
		}
		slog.Info("gemma call", "op", op, "model", c.model, "wall_ms", wallMs, "eval_count", evalCount)
	}()

	req := ollamaGenerateRequest{
		Model:     c.model,
		Prompt:    prompt,
		System:    opts.System,
		Stream:    false,
		KeepAlive: c.keepAlive,
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
