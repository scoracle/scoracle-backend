package derive

import (
	"context"
	"io"
	"log/slog"
	"testing"
	"time"

	"github.com/albapepper/scoracle-data/internal/ml"
)

// TestDrainAllDefersWhenOllamaUnreachable is the core F-014 guarantee: when Ollama is
// down, DrainAll DEFERS — it returns Deferred and claims nothing — so no retries are
// burned and the durable queue simply waits for recovery. It needs no database because
// the reachability pre-gate returns before any work is claimed (Pool is never touched).
func TestDrainAllDefersWhenOllamaUnreachable(t *testing.T) {
	// 127.0.0.1:1 refuses connections — stands in for a down Ollama.
	cli := ml.NewOllamaClient("http://127.0.0.1:1", "gemma4:e4b", 2*time.Second)
	d := &Drainer{
		Pool:   nil, // must NOT be reached; the pre-gate returns first
		Ollama: cli,
		Logger: slog.New(slog.NewTextHandler(io.Discard, nil)),
	}

	res := d.DrainAll(context.Background())
	if !res.Deferred {
		t.Fatalf("DrainAll with unreachable Ollama: Deferred=false, want true")
	}
	if len(res.Stages) != 0 {
		t.Fatalf("deferred drain should run no stages, got %d", len(res.Stages))
	}
	if ok, failed, requeued := res.Totals(); ok != 0 || failed != 0 || requeued != 0 {
		t.Fatalf("deferred drain should touch nothing, got ok=%d failed=%d requeued=%d", ok, failed, requeued)
	}
}

// TestShortTimeoutFallback verifies the quick-stage budget falls back to the long
// budget when GemmaShortTimeout is left unset.
func TestShortTimeoutFallback(t *testing.T) {
	d := &Drainer{GemmaTimeout: 300 * time.Second}
	if got := d.shortTimeout(); got != 300*time.Second {
		t.Fatalf("shortTimeout fallback = %v, want 300s", got)
	}
	d.GemmaShortTimeout = 120 * time.Second
	if got := d.shortTimeout(); got != 120*time.Second {
		t.Fatalf("shortTimeout = %v, want 120s", got)
	}
}
