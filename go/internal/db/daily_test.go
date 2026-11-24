package db

import (
	"context"
	"encoding/json"
	"github.com/jackc/pgx/v5"
	"os"
	"strings"
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
		{"entity_vibe", []any{"NFL", "person", 7, nil}},
		{"entity_news", []any{"NFL", "person", 7, nil}},
		{"entity_transfers", []any{"NFL", "person", 7, nil}},
		{"entity_headlines", []any{"NFL", "person", 7, nil}},
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

func TestPersonReportingReadsKeepEntityTypesSeparate(t *testing.T) {
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
	tx, err := conn.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer tx.Rollback(ctx)
	// All writes belong to this rollback-only fixture.
	if _, err := tx.Exec(ctx, "UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL"); err != nil {
		t.Fatal(err)
	}

	for _, kind := range []string{"team", "player", "person"} {
		for _, statement := range []string{
			`INSERT INTO vibe_scores(entity_type,entity_id,sport,trigger_type,sentiment,hook,prompt,model_version,prompt_version)
             VALUES($1,9991051,'NFL','manual',75,$1||' hope',$1||' emotional reporting','fixture','fixture')`,
			`INSERT INTO news_summaries(entity_type,entity_id,sport,trigger_type,narrative_title,body,card_score,headline)
             VALUES($1,9991051,'NFL','classifier',$1||' story',$1||' news reporting',40,$1||' update')`,
			`INSERT INTO insider_scores(entity_type,entity_id,sport,score,read,headline,prompt_version)
             VALUES($1,9991051,'NFL',40,$1||' movement reporting',$1||' movement','fixture')`,
		} {
			if _, err := tx.Exec(ctx, statement, kind); err != nil {
				t.Fatal(err)
			}
		}
	}

	for _, statement := range []string{"entity_vibe", "entity_news", "entity_transfers", "entity_headlines"} {
		var payload []byte
		if err := tx.QueryRow(ctx, statement, "NFL", "person", 9991051, nil).Scan(&payload); err != nil {
			t.Fatal(err)
		}
		var result map[string]any
		if err := json.Unmarshal(payload, &result); err != nil {
			t.Fatal(err)
		}
		if result["entity_type"] != "person" || !strings.Contains(string(payload), "person ") || strings.Contains(string(payload), "player ") || strings.Contains(string(payload), "team ") {
			t.Fatalf("%s mixed person/player/team IDs: %s", statement, payload)
		}
		// Selecting an empty historical day cannot leak today's saved movement card.
		if statement == "entity_transfers" {
			if err := tx.QueryRow(ctx, statement, "NFL", "person", 9991051, "2001-01-01").Scan(&payload); err != nil {
				t.Fatal(err)
			}
			if strings.Contains(string(payload), "person movement") {
				t.Fatalf("historical day leaked a current card: %s", payload)
			}
		}
	}
}

func TestAnalystEstimateIsServedWithoutMeasuredChange(t *testing.T) {
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
	tx, err := conn.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer tx.Rollback(ctx)
	// Fresh schema restores leave this projection unpopulated.
	if _, err := tx.Exec(ctx, "REFRESH MATERIALIZED VIEW public.latest_momentum_scores_per_entity"); err != nil {
		t.Fatal(err)
	}
	if _, err := tx.Exec(ctx, `INSERT INTO momentum_summaries
        (entity_type,entity_id,sport,season,trigger_type,blurb,headline,input_components,estimated_score,model_version,prompt_version)
        SELECT 'team',-345,'NBA',current_season,'periodic','Current reading','An estimate','{"card_score":null}',0,'fixture','fixture' FROM sports WHERE id='NBA'`); err != nil {
		t.Fatal(err)
	}
	for _, route := range []string{"entity_momentum", "entity_headlines"} {
		var raw []byte
		if err := tx.QueryRow(ctx, route, "NBA", "team", -345, nil).Scan(&raw); err != nil {
			t.Fatal(err)
		}
		var payload struct {
			Summary struct {
				CardScore *float64 `json:"card_score"`
				Source    string   `json:"card_score_source"`
				Heat      *int     `json:"heat"`
			}
			Scores  json.RawMessage
			Entries []struct {
				Card  string
				Score *float64
			}
		}
		if err := json.Unmarshal(raw, &payload); err != nil {
			t.Fatal(err)
		}
		if route == "entity_momentum" {
			if payload.Summary.CardScore == nil || *payload.Summary.CardScore != 0 || payload.Summary.Source != "estimated" || payload.Summary.Heat != nil || string(payload.Scores) != "null" {
				t.Fatalf("unexpected estimate: %s", raw)
			}
		} else if len(payload.Entries) != 1 || payload.Entries[0].Card != "momentum" || payload.Entries[0].Score == nil || *payload.Entries[0].Score != 0 {
			t.Fatalf("wrong headline score: %s", raw)
		}
	}
}
