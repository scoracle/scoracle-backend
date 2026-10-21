package dataimport

import (
	"context"
	"os"
	"testing"

	"github.com/jackc/pgx/v5"
)

func TestHeadshotUpdateKeepsSportBoundary(t *testing.T) {
	databaseURL := os.Getenv("TEST_DATABASE_URL")
	if databaseURL == "" {
		t.Skip("requires TEST_DATABASE_URL")
	}
	ctx := context.Background()
	conn, err := pgx.Connect(ctx, databaseURL)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close(ctx)
	_, err = conn.Exec(ctx, `CREATE TEMP TABLE players(id int, sport text, photo_url text, updated_at timestamptz, PRIMARY KEY(id,sport));
		INSERT INTO players VALUES (177,'NBA','nba-original',now()),(177,'NFL','nfl-original',now())`)
	if err != nil {
		t.Fatal(err)
	}
	changed, err := setPlayerHeadshot(ctx, conn, 177, "NFL", "nfl-new")
	if err != nil || !changed {
		t.Fatalf("headshot update: changed=%v error=%v", changed, err)
	}
	var nbaPhoto, nflPhoto string
	err = conn.QueryRow(ctx, `SELECT max(photo_url) FILTER(WHERE sport='NBA'),max(photo_url) FILTER(WHERE sport='NFL') FROM players`).Scan(&nbaPhoto, &nflPhoto)
	if err != nil || nbaPhoto != "nba-original" || nflPhoto != "nfl-new" {
		t.Fatalf("NBA=%q NFL=%q error=%v", nbaPhoto, nflPhoto, err)
	}
}
