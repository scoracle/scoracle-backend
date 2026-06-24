package ml

import (
	"context"
	"errors"
	"fmt"
	"net"
	"syscall"
	"testing"
)

// timeoutErr is a net.Error whose Timeout() is true — a slow op against a
// reachable server, which IsUnavailable must NOT classify as an outage.
type timeoutErr struct{}

func (timeoutErr) Error() string   { return "i/o timeout" }
func (timeoutErr) Timeout() bool   { return true }
func (timeoutErr) Temporary() bool { return true }

func TestIsUnavailable(t *testing.T) {
	cases := []struct {
		name string
		err  error
		want bool
	}{
		{"nil", nil, false},
		{"econnrefused", syscall.ECONNREFUSED, true},
		{"dial opError refused", &net.OpError{Op: "dial", Net: "tcp", Err: syscall.ECONNREFUSED}, true},
		{"wrapped refused string", fmt.Errorf("ollama request: %w", errors.New("dial tcp 127.0.0.1:11434: connect: connection refused")), true},
		{"ping failure", fmt.Errorf("ollama ping: %w", errors.New("boom")), true},
		{"no such host", errors.New("dial tcp: lookup ollama: no such host"), true},
		// Timeouts / deadlines are NOT outages — a too-slow op must fail+backoff,
		// not requeue forever without burning an attempt.
		{"context deadline", context.DeadlineExceeded, false},
		{"context canceled", context.Canceled, false},
		{"client timeout string", errors.New(`Post "http://x/api/generate": context deadline exceeded (Client.Timeout exceeded while awaiting headers)`), false},
		{"net timeout opError", &net.OpError{Op: "read", Net: "tcp", Err: timeoutErr{}}, false},
		// A genuine per-item generation/parse error is not an outage.
		{"parse error", errors.New(`parse narratives failed (raw="...")`), false},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := IsUnavailable(c.err); got != c.want {
				t.Fatalf("IsUnavailable(%v) = %v, want %v", c.err, got, c.want)
			}
		})
	}
}

// TestGemmaGate verifies the process-wide governor bounds concurrency and respects
// context cancellation while waiting for a slot.
func TestGemmaGate(t *testing.T) {
	SetGemmaConcurrency(1)

	release1, err := gemmaAcquire(context.Background())
	if err != nil {
		t.Fatalf("first acquire: %v", err)
	}

	// Gate is full (size 1): an acquire with an already-cancelled ctx must give up
	// with the context error rather than block forever.
	cancelled, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := gemmaAcquire(cancelled); !errors.Is(err, context.Canceled) {
		t.Fatalf("acquire on full gate with cancelled ctx = %v, want context.Canceled", err)
	}

	// Releasing the slot makes the next acquire succeed immediately.
	release1()
	release2, err := gemmaAcquire(context.Background())
	if err != nil {
		t.Fatalf("acquire after release: %v", err)
	}
	release2()
}

// TestGemmaConcurrencyTwo confirms a size-2 gate admits two holders at once but
// blocks the third (via a cancelled-ctx probe).
func TestGemmaConcurrencyTwo(t *testing.T) {
	SetGemmaConcurrency(2)
	defer SetGemmaConcurrency(1) // restore the production default for other tests

	r1, err := gemmaAcquire(context.Background())
	if err != nil {
		t.Fatalf("acquire 1: %v", err)
	}
	r2, err := gemmaAcquire(context.Background())
	if err != nil {
		t.Fatalf("acquire 2: %v", err)
	}
	cancelled, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := gemmaAcquire(cancelled); !errors.Is(err, context.Canceled) {
		t.Fatalf("third acquire on full size-2 gate = %v, want context.Canceled", err)
	}
	r1()
	r2()
}
