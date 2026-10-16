package db

import (
	"context"
	"github.com/jackc/pgx/v5"
	"os"
	"testing"
	"time"
)

func TestDailyReadStatements(t *testing.T) {
	url := os.Getenv("TEST_DATABASE_URL")
	if url == "" {
		t.Skip("requires migrated TEST_DATABASE_URL")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	conn, err := pgx.Connect(ctx, url)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close(ctx)
	if err := registerPreparedStatements(ctx, conn); err != nil {
		t.Fatal(err)
	}
	for _, test := range []struct {
		name string
		args []any
	}{
		{"entity_vibe", []any{"NFL", "team", 7, nil}},
		{"entity_news", []any{"NFL", "team", 7, nil}},
		{"entity_transfers", []any{"NFL", "team", 7, nil}},
		{"entity_headlines", []any{"NFL", "team", 7, "2026-03-08"}},
		{"vibes_leaderboard", []any{"NFL", 10, nil, nil, nil, nil, nil, nil, nil, nil}},
		{"sigil_leaderboard", []any{"NFL", 10, nil, nil, nil, nil, nil, nil, nil, nil, nil}},
		{"narratives_leaderboard", []any{"NFL", 10, nil, nil, nil, nil, nil, nil, nil, nil}},
		{"transfers_leaderboard", []any{"NFL", 10, nil, nil, nil, nil, nil, nil, nil, nil}},
	} {
		t.Run(test.name, func(t *testing.T) {
			var payload []byte
			if err := conn.QueryRow(ctx, test.name, test.args...).Scan(&payload); err != nil {
				t.Fatal(err)
			}
		})
	}
}
