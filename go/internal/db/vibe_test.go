package db

import (
	"context"
	"encoding/json"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
)

// Executes the production projection against transaction-local fixtures.
func TestEntityVibeCurrent(t *testing.T) {
	url := os.Getenv("TEST_DATABASE_URL")
	if url == "" {
		t.Skip("set TEST_DATABASE_URL to run the Vibe serving check")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	conn, err := pgx.Connect(ctx, url)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close(ctx)
	tx, err := conn.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer tx.Rollback(ctx)
	_, err = tx.Exec(ctx, `
        CREATE TEMP TABLE vibe_scores (
            id bigint, entity_type text, entity_id integer, sport text,
            sentiment smallint, hook text, prompt text, input_news_ids bigint[],
            trigger_type text DEFAULT 'periodic', generated_at timestamptz,
            model_version text DEFAULT 'fixture', prompt_version text DEFAULT 'fixture',
            input_hash text, scoring_version text, source_references jsonb, week_season int, week_no int,
            reporting_start timestamptz, reporting_end timestamptz, evidence_cutoff timestamptz
        );
        CREATE TEMP TABLE news_articles (id bigint, source text, url text, published_at timestamptz);
        INSERT INTO news_articles VALUES (42, 'Wire', 'https://example.com/42', NOW() - INTERVAL '11 days');
        INSERT INTO vibe_scores (id,entity_type,entity_id,sport,sentiment,hook,prompt,input_news_ids,generated_at) VALUES
            (1,'team',7,'NFL',80,'Older scored card','Old reading.',ARRAY[41],NOW() - INTERVAL '11 days'),
            (2,'team',7,'NFL',NULL,'A supported contrast','The speakers disagree.',ARRAY[42],NOW() - INTERVAL '10 days'),
            (3,'team',7,'NFL',NULL,'Latest supported contrast','They still disagree.',ARRAY[42],NOW() - INTERVAL '10 days'),
            (4,'team',7,'NFL',90,'Incomplete replacement',NULL,ARRAY[99],NOW()),
            (5,'team',7,'NFL',90,repeat('x',141),'Overlong replacement.',ARRAY[99],NOW()),
            (6,'team',8,'NFL',90,'Other entity','Other reading.',ARRAY[99],NOW());
    `)
	if err != nil {
		t.Fatal(err)
	}
	statement := strings.NewReplacer("public.vibe_scores", "pg_temp.vibe_scores", "public.news_articles", "pg_temp.news_articles").Replace(entityVibeStatement)
	var payload []byte
	if err := tx.QueryRow(ctx, statement, "nfl", "team", 7, nil, nil).Scan(&payload); err != nil {
		t.Fatal(err)
	}
	var result struct {
		Current *struct {
			ID          int       `json:"id"`
			Heat        *int      `json:"heat"`
			Headline    string    `json:"headline"`
			Body        string    `json:"body"`
			GeneratedAt time.Time `json:"generated_at"`
			Sources     []struct {
				ID  int    `json:"id"`
				URL string `json:"url"`
			} `json:"sources"`
		} `json:"current"`
	}
	if err := json.Unmarshal(payload, &result); err != nil {
		t.Fatal(err)
	}
	c := result.Current
	if c == nil || c.ID != 3 || c.Heat != nil || c.Headline != "Latest supported contrast" || c.Body != "They still disagree." {
		t.Fatalf("wrong current product: %s", payload)
	}
	if c.GeneratedAt.After(time.Now().Add(-9*24*time.Hour)) || len(c.Sources) != 1 || c.Sources[0].ID != 42 || c.Sources[0].URL != "https://example.com/42" {
		t.Fatalf("lost original date or source binding: %s", payload)
	}
	_, err = tx.Exec(ctx, `INSERT INTO vibe_scores(id,entity_type,entity_id,sport,sentiment,hook,prompt,generated_at,week_season,week_no,reporting_start,reporting_end) VALUES
        (7,'team',7,'NFL',0,'A difficult week','A coherent zero score.',NOW(),2026,1,'2026-09-01','2026-09-08'),
        (8,'team',7,'NFL',75,'A hopeful week','A different period.',NOW(),2026,2,'2026-09-08','2026-09-15')`)
	if err != nil {
		t.Fatal(err)
	}
	for _, period := range []struct{ week, want int }{{1, 7}, {2, 8}, {3, 0}} {
		if err := tx.QueryRow(ctx, statement, "nfl", "team", 7, 2026, period.week).Scan(&payload); err != nil {
			t.Fatal(err)
		}
		if err := json.Unmarshal(payload, &result); err != nil {
			t.Fatal(err)
		}
		if period.want == 0 {
			if result.Current != nil {
				t.Fatalf("empty period borrowed a card: %s", payload)
			}
		} else {
			if result.Current == nil || result.Current.ID != period.want {
				t.Fatalf("wrong period: %s", payload)
			}
			if period.week == 1 && (result.Current.Heat == nil || *result.Current.Heat != 0) {
				t.Fatalf("zero lost: %s", payload)
			}
		}
	}

	if err := tx.QueryRow(ctx, statement, "nfl", "team", 999, nil, nil).Scan(&payload); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(payload, &result); err != nil {
		t.Fatal(err)
	}
	if result.Current != nil {
		t.Fatalf("empty entity borrowed a card: %s", payload)
	}
}
