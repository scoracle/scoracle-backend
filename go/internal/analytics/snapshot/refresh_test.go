package snapshot

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
)

func TestRefreshPublishesChangesWithoutChurnAndRecovers(t *testing.T) {
	url := os.Getenv("TEST_DATABASE_URL")
	if url == "" {
		t.Skip("isolated migrated TEST_DATABASE_URL required")
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	defer cancel()
	conn, err := pgx.Connect(ctx, url)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close(ctx)
	exec := func(q string) {
		t.Helper()
		if _, err := conn.Exec(ctx, q); err != nil {
			t.Fatal(err)
		}
	}
	clean := func() {
		exec(`DROP TRIGGER IF EXISTS test_refresh_failure ON analytics_cohort_publication;
 DROP FUNCTION IF EXISTS test_refresh_failure();
 DELETE FROM analytics_entity_context WHERE sport='NBA' AND season IN (2195,2196);
 DELETE FROM analytics_cohort_publication WHERE sport='NBA' AND season IN (2195,2196);
 DELETE FROM team_stats WHERE sport='NBA' AND season IN (2195,2196)`)
	}
	clean()
	defer clean()
	exec(`INSERT INTO sports(id,display_name,current_season) VALUES('NBA','NBA',2025) ON CONFLICT DO NOTHING;
 INSERT INTO team_stats(team_id,sport,season,league_id,rating) VALUES
 (9600011,'NBA',2195,1,7),(9600011,'NBA',2196,1,9)`)
	run := func() RefreshResult {
		t.Helper()
		r, e := Refresh(ctx, conn)
		if e != nil {
			t.Fatal(e)
		}
		return r
	}
	if r := run(); r.Published < 2 {
		t.Fatalf("initial publication: %+v", r)
	}
	receipt := func() time.Time {
		t.Helper()
		var at time.Time
		if e := conn.QueryRow(ctx, "SELECT published_at FROM analytics_cohort_publication WHERE sport='NBA' AND entity_type='team' AND season=2196").Scan(&at); e != nil {
			t.Fatal(e)
		}
		return at
	}
	original := receipt()
	if r := run(); r.Published != 0 {
		t.Fatalf("unchanged data republished: %+v", r)
	}
	if !receipt().Equal(original) {
		t.Fatal("unchanged input changed receipt")
	}
	other, e := pgx.Connect(ctx, url)
	if e != nil {
		t.Fatal(e)
	}
	if _, e = other.Exec(ctx, "SELECT pg_advisory_lock(hashtextextended('duckdb-cohort-producer',0))"); e != nil {
		t.Fatal(e)
	}
	r := run()
	other.Close(ctx)
	if !r.Busy || r.Checked != 0 {
		t.Fatalf("competing producer ran: %+v", r)
	}
	exec(`UPDATE team_stats SET rating=10 WHERE sport='NBA' AND season=2196;
 CREATE FUNCTION test_refresh_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.season=2196 THEN RAISE EXCEPTION 'injected refresh failure'; END IF; RETURN NEW; END $$;
 CREATE TRIGGER test_refresh_failure BEFORE UPDATE ON analytics_cohort_publication FOR EACH ROW EXECUTE FUNCTION test_refresh_failure()`)
	if _, e = Refresh(ctx, conn); e == nil {
		t.Fatal("publication failure was swallowed")
	}
	if !receipt().Equal(original) {
		t.Fatal("failed publication changed receipt")
	}
	var rating, delta float64
	read := func() {
		t.Helper()
		if e := conn.QueryRow(ctx, "SELECT rating,delta FROM analytics_entity_context WHERE sport='NBA' AND entity_type='team' AND season=2196").Scan(&rating, &delta); e != nil {
			t.Fatal(e)
		}
	}
	read()
	if rating != 9 || delta != 2 {
		t.Fatal("failed publication leaked results")
	}
	exec("DROP TRIGGER test_refresh_failure ON analytics_cohort_publication; DROP FUNCTION test_refresh_failure()")
	run()
	read()
	if rating != 10 || delta != 3 {
		t.Fatal("correction was not retried")
	}
	exec("UPDATE team_stats SET rating=NULL WHERE sport='NBA' AND season=2196")
	run()
	var count int
	if e := conn.QueryRow(ctx, "SELECT count(*) FROM analytics_entity_context WHERE sport='NBA' AND entity_type='team' AND season=2196").Scan(&count); e != nil || count != 0 {
		t.Fatal("NULL rating retained a stale member", e)
	}
	exec("DELETE FROM team_stats WHERE sport='NBA' AND season IN (2195,2196)")
	run()
	if e := conn.QueryRow(ctx, "SELECT sum(row_count) FROM analytics_cohort_publication WHERE sport='NBA' AND entity_type='team' AND season IN (2195,2196)").Scan(&count); e != nil || count != 0 {
		t.Fatal("deleted source scope retained members", e)
	}
}
