package handler

import (
	"context"
	"fmt"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/auth"
	"github.com/albapepper/scoracle-data/internal/config"
)

func TestRefreshConsumesTokenOnceUnderConcurrency(t *testing.T) {
	url := os.Getenv("TEST_DATABASE_URL")
	if url == "" {
		t.Skip("requires disposable TEST_DATABASE_URL")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	admin, err := pgxpool.New(ctx, url)
	if err != nil {
		t.Fatal(err)
	}
	defer admin.Close()
	schema := fmt.Sprintf("auth_rotation_%d", time.Now().UnixNano())
	if _, err = admin.Exec(ctx, "CREATE SCHEMA "+schema); err != nil {
		t.Fatal(err)
	}
	defer admin.Exec(context.Background(), "DROP SCHEMA "+schema+" CASCADE")
	cfg, err := pgxpool.ParseConfig(url)
	if err != nil {
		t.Fatal(err)
	}
	cfg.ConnConfig.RuntimeParams["search_path"] = schema
	cfg.ConnConfig.RuntimeParams["application_name"] = schema
	cfg.MaxConns = 4
	pool, err := pgxpool.NewWithConfig(ctx, cfg)
	if err != nil {
		t.Fatal(err)
	}
	defer pool.Close()
	_, err = pool.Exec(ctx, `CREATE TABLE auth_refresh_tokens (
 user_id text NOT NULL, token_hash text PRIMARY KEY, expires_at timestamptz NOT NULL,
 revoked_at timestamptz, last_used_at timestamptz)`)
	if err != nil {
		t.Fatal(err)
	}
	tokens := auth.New(&config.Config{JWTSecret: "test-secret", JWTAccessTTL: time.Hour, JWTRefreshTTL: 24 * time.Hour})
	raw, hash, err := tokens.NewRefreshToken()
	if err != nil {
		t.Fatal(err)
	}
	_, err = pool.Exec(ctx, `INSERT INTO auth_refresh_tokens(user_id,token_hash,expires_at) VALUES('test-user',$1,NOW()+INTERVAL '1 hour')`, hash)
	if err != nil {
		t.Fatal(err)
	}
	lock, err := pool.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer lock.Rollback(context.Background())
	if _, err = lock.Exec(ctx, `SELECT 1 FROM auth_refresh_tokens WHERE token_hash=$1 FOR UPDATE`, hash); err != nil {
		t.Fatal(err)
	}
	h := &Handler{pool: pool, auth: tokens}
	results := make(chan int, 2)
	for i := 0; i < 2; i++ {
		go func() {
			req := httptest.NewRequest("POST", "/auth/refresh", strings.NewReader(`{"refresh_token":"`+raw+`"}`)).WithContext(ctx)
			rec := httptest.NewRecorder()
			h.AuthRefresh(rec, req)
			results <- rec.Code
		}()
	}
	// Hold the original row until both requests have reached the consuming update.
	for {
		var waiting int
		err = admin.QueryRow(ctx, `SELECT count(*) FROM pg_stat_activity WHERE application_name=$1 AND wait_event_type='Lock' AND query LIKE '%UPDATE auth_refresh_tokens%'`, schema).Scan(&waiting)
		if err != nil {
			t.Fatal(err)
		}
		if waiting == 2 {
			break
		}
		select {
		case <-ctx.Done():
			t.Fatal("refresh requests did not reach row lock")
		case <-time.After(10 * time.Millisecond):
		}
	}
	if err = lock.Commit(ctx); err != nil {
		t.Fatal(err)
	}
	codes := map[int]int{}
	for i := 0; i < 2; i++ {
		select {
		case c := <-results:
			codes[c]++
		case <-ctx.Done():
			t.Fatal(ctx.Err())
		}
	}
	if codes[200] != 1 || codes[401] != 1 {
		t.Fatalf("expected one success and one rejection: %v", codes)
	}
	var active int
	if err = pool.QueryRow(ctx, `SELECT count(*) FROM auth_refresh_tokens WHERE revoked_at IS NULL`).Scan(&active); err != nil {
		t.Fatal(err)
	}
	if active != 1 {
		t.Fatalf("active replacements = %d, want 1", active)
	}
}
