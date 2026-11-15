package db

import (
	"context"
	"encoding/json"
	"os"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
)

// Both plans must remain usable after pgx promotes repeated reads to a generic plan.
func TestNarrativesLeaderboardPreparedPlans(t *testing.T) {
	url := os.Getenv("TEST_DATABASE_URL")
	if url == "" {
		t.Skip("requires migrated TEST_DATABASE_URL")
	}
	ctx := context.Background()
	conn, err := pgx.Connect(ctx, url)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close(ctx)
	if err := registerPreparedStatements(ctx, conn); err != nil {
		t.Fatal(err)
	}
	for _, mode := range []string{"force_custom_plan", "force_generic_plan"} {
		t.Run(mode, func(t *testing.T) {
			ctx, cancel := context.WithTimeout(ctx, 8*time.Second)
			defer cancel()
			tx, err := conn.BeginTx(ctx, pgx.TxOptions{AccessMode: pgx.ReadOnly})
			if err != nil {
				t.Fatal(err)
			}
			defer tx.Rollback(context.Background())
			if _, err = tx.Exec(ctx, "SELECT set_config('plan_cache_mode',$1,true)", mode); err != nil {
				t.Fatal(err)
			}
			var raw []byte
			if err = tx.QueryRow(ctx, "narratives_leaderboard", "NBA", 50, "player", nil, nil, nil, nil, nil, nil, nil).Scan(&raw); err != nil {
				t.Fatal(err)
			}
			var payload struct {
				Count   int `json:"count"`
				Leaders []struct {
					Rank       int    `json:"rank"`
					EntityType string `json:"entity_type"`
				} `json:"leaders"`
			}
			if err = json.Unmarshal(raw, &payload); err != nil {
				t.Fatal(err)
			}
			if payload.Count != len(payload.Leaders) || payload.Count > 50 {
				t.Fatalf("invalid count: %s", raw)
			}
			for i, row := range payload.Leaders {
				if row.Rank != i+1 || row.EntityType != "player" {
					t.Fatalf("invalid ranking/filter: %s", raw)
				}
			}
		})
	}
}
