package auth

import (
	"testing"
	"time"
)

func testTokens(accessTTL time.Duration) *Tokens {
	return &Tokens{
		secret:     []byte("test-secret-0123456789abcdef"),
		accessTTL:  accessTTL,
		refreshTTL: 24 * time.Hour,
	}
}

func TestAccessRoundTrip(t *testing.T) {
	tk := testTokens(time.Hour)
	token, err := tk.IssueAccess("user-123")
	if err != nil {
		t.Fatalf("issue: %v", err)
	}
	sub, err := tk.ParseAccess(token)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if sub != "user-123" {
		t.Fatalf("sub = %q, want user-123", sub)
	}
}

func TestExpiredRejected(t *testing.T) {
	tk := testTokens(-time.Minute) // already expired
	token, _ := tk.IssueAccess("u")
	if _, err := tk.ParseAccess(token); err == nil {
		t.Fatal("expected expired token to be rejected")
	}
}

func TestTamperedRejected(t *testing.T) {
	tk := testTokens(time.Hour)
	token, _ := tk.IssueAccess("u")
	if _, err := tk.ParseAccess(token + "x"); err == nil {
		t.Fatal("expected tampered signature to be rejected")
	}
	if _, err := tk.ParseAccess("not.a.jwt.at.all"); err == nil {
		t.Fatal("expected malformed token to be rejected")
	}
}

func TestWrongSecretRejected(t *testing.T) {
	a := testTokens(time.Hour)
	b := &Tokens{secret: []byte("a-different-secret"), accessTTL: time.Hour}
	token, _ := a.IssueAccess("u")
	if _, err := b.ParseAccess(token); err == nil {
		t.Fatal("expected verification to fail under a different secret")
	}
}

func TestRefreshHashAndUniqueness(t *testing.T) {
	tk := testTokens(time.Hour)
	raw1, hash1, err := tk.NewRefreshToken()
	if err != nil {
		t.Fatalf("new refresh: %v", err)
	}
	if HashRefresh(raw1) != hash1 {
		t.Fatal("HashRefresh did not match the issued hash")
	}
	raw2, _, _ := tk.NewRefreshToken()
	if raw1 == raw2 {
		t.Fatal("refresh tokens should be unique")
	}
}

func TestUnconfigured(t *testing.T) {
	if (&Tokens{}).Configured() {
		t.Fatal("empty secret should report unconfigured")
	}
	if !testTokens(time.Hour).Configured() {
		t.Fatal("non-empty secret should report configured")
	}
}
