package handler

import (
	"context"
	"encoding/json"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/db"
)

func TestArticulatorUsesDailyReportingStatements(t *testing.T) {
	url := os.Getenv("TEST_DATABASE_URL")
	if url == "" {
		t.Skip("requires migrated disposable TEST_DATABASE_URL")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	pool, err := db.New(ctx, &config.Config{DatabaseURL: url, DBPoolMaxConns: 1, DBPoolMaxLife: time.Minute})
	if err != nil {
		t.Fatal(err)
	}
	defer pool.Close()
	var database string
	if err := pool.QueryRow(ctx, "SELECT current_database()").Scan(&database); err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(database, "classifier_test") {
		t.Fatal("requires a disposable classifier_test database")
	}
	if _, err := pool.Exec(ctx, "INSERT INTO sports(id,display_name,current_season) VALUES('ZZ_ARTICULATOR','Articulator test',2026)"); err != nil {
		t.Fatal(err)
	}
	defer pool.Exec(ctx, "DELETE FROM sports WHERE id='ZZ_ARTICULATOR'")
	if _, err := pool.Exec(ctx, "INSERT INTO teams(id,sport,name) VALUES(9991052,'ZZ_ARTICULATOR','Memory Test Team')"); err != nil {
		t.Fatal(err)
	}
	defer pool.Exec(ctx, "DELETE FROM teams WHERE id=9991052 AND sport='ZZ_ARTICULATOR'")
	h := &Handler{pool: pool.Pool}
	for _, kind := range []string{"p5", "p7", "p8"} {
		t.Run(kind, func(t *testing.T) {
			inputs, err := h.loadArticulatorInputs(ctx, "ZZ_ARTICULATOR", "team", 9991052, kind)
			if err != nil {
				t.Fatal(err)
			}
			payload := inputs.News
			if kind == "p7" {
				payload = inputs.Vibe
			} else if kind == "p8" {
				payload = inputs.Transfers
			}
			var product map[string]any
			if err := json.Unmarshal(payload, &product); err != nil {
				t.Fatal(err)
			}
			if product["entity_type"] != "team" {
				t.Fatalf("unexpected daily product: %s", payload)
			}
		})
	}
}
