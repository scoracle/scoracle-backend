package snapshot

import (
	"context"
	"errors"
	"os"
	"reflect"
	"testing"
	"time"

	"github.com/albapepper/scoracle-data/internal/analytics/duckdb"
	"github.com/albapepper/scoracle-data/internal/analytics/model"
	"github.com/albapepper/scoracle-data/internal/analytics/postgres"
	"github.com/jackc/pgx/v5"
)

func TestCohortAcceptance(t *testing.T) {
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
	exec := func(q string, args ...any) {
		t.Helper()
		if _, err := conn.Exec(ctx, q, args...); err != nil {
			t.Fatal(err)
		}
	}
	// Deliberately empty synthetic seasons; no production data is copied.
	cleanup := func() {
		exec("DELETE FROM public.analytics_entity_context WHERE sport='NBA' AND season=2198")
		exec("DELETE FROM public.analytics_cohort_publication WHERE sport='NBA' AND season=2198")
		exec("DELETE FROM public.team_stats WHERE sport='NBA' AND season IN (2197,2198)")
	}
	cleanup()
	defer cleanup()
	exec("INSERT INTO public.sports(id,display_name,current_season) VALUES ('NBA','NBA',2025) ON CONFLICT DO NOTHING")
	exec(`INSERT INTO public.team_stats(team_id,sport,season,league_id,rating) VALUES
 (9600001,'NBA',2197,1,10),(9600001,'NBA',2198,1,12),
 (9600002,'NBA',2197,1,20),(9600002,'NBA',2198,1,22),
 (9600003,'NBA',2197,1,30),(9600003,'NBA',2198,1,29),
 (9600004,'NBA',2197,1,NULL),(9600004,'NBA',2198,1,0),
 (9600005,'NBA',2197,1,10),(9600005,'NBA',2198,1,NULL),
 (9600006,'NBA',2197,1,20),(9600006,'NBA',2198,2,22),
 (9600007,'NBA',2197,3,10),(9600007,'NBA',2198,3,11)`)
	scope := model.CohortScope{Sport: "NBA", EntityType: "team", Season: 2198}
	fixed := time.Date(2026, 9, 20, 0, 0, 0, 123456789, time.UTC)
	export := func(n int) model.CohortSnapshot {
		t.Helper()
		s, err := Export(ctx, conn, scope, fixed.Add(time.Duration(n)*time.Second))
		if err != nil {
			t.Fatal(err)
		}
		return s
	}
	compute := func(s model.CohortSnapshot) []model.EntityContextRow {
		t.Helper()
		start := time.Now()
		p, err := postgres.SnapshotContext(ctx, conn, s)
		if err != nil {
			t.Fatal(err)
		}
		pgTime := time.Since(start)
		start = time.Now()
		d, err := duckdb.Open(ctx, duckdb.Options{MemoryLimit: "128MB"})
		if err != nil {
			t.Fatal(err)
		}
		got, err := d.SnapshotContext(ctx, s)
		d.Close(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if err = Compare(p, got); err != nil {
			t.Fatal(err)
		}
		t.Logf("inputs=%d outputs=%d postgres=%s duckdb_load_compute=%s", len(s.Inputs), len(got), pgTime, time.Since(start))
		return got
	}
	exec("INSERT INTO momentum_refresh_needed(sport,reason) VALUES ('NBA','cohort acceptance unrelated obligation') ON CONFLICT DO NOTHING")
	defer exec("DELETE FROM momentum_refresh_needed WHERE sport='NBA' AND reason='cohort acceptance unrelated obligation'")
	var dirtyBefore time.Time
	if err = conn.QueryRow(ctx, "SELECT last_marked_at FROM momentum_refresh_needed WHERE sport='NBA'").Scan(&dirtyBefore); err != nil {
		t.Fatal(err)
	}
	// Establish an MVCC snapshot, mutate from another connection, then prove the
	// exporter read still sees the captured version across separate statements.
	tx, err := conn.BeginTx(ctx, pgx.TxOptions{IsoLevel: pgx.RepeatableRead, AccessMode: pgx.ReadOnly})
	if err != nil {
		t.Fatal(err)
	}
	if _, err = tx.Exec(ctx, "SELECT pg_current_snapshot()"); err != nil {
		t.Fatal(err)
	}
	other, err := pgx.Connect(ctx, url)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = other.Exec(ctx, "UPDATE team_stats SET rating=14 WHERE sport='NBA' AND season=2198 AND team_id=9600001"); err != nil {
		t.Fatal(err)
	}
	frozen, err := readInputs(ctx, tx, scope)
	if err != nil {
		t.Fatal(err)
	}
	tx.Rollback(ctx)
	for _, r := range frozen {
		if r.EntityID == 9600001 && r.Season == 2198 && (r.Rating == nil || *r.Rating != 12) {
			t.Fatal("mixed MVCC export")
		}
	}
	if _, err = other.Exec(ctx, "UPDATE team_stats SET rating=12 WHERE sport='NBA' AND season=2198 AND team_id=9600001"); err != nil {
		t.Fatal(err)
	}
	other.Close(ctx)
	s := export(0)
	rows := compute(s)
	if len(rows) != 6 {
		t.Fatalf("missingness: %+v", rows)
	}
	if rows[0].DeltaPctile == nil || *rows[0].DeltaPctile != 50 || rows[0].PeerCount != 3 {
		t.Fatalf("tie rank/self membership: %+v", rows[0])
	}
	if rows[3].PriorRating != nil || rows[3].Rating != 0 {
		t.Fatalf("NULL vs zero: %+v", rows[3])
	}
	if rows[4].PriorRating != nil || rows[4].PeerCount != 0 {
		t.Fatalf("league move must not bridge: %+v", rows[4])
	}
	if rows[5].DeltaPctile == nil || *rows[5].DeltaPctile != 0 {
		t.Fatalf("singleton: %+v", rows[5])
	}
	// Loss of the entire private DuckDB instance is routine, not a repair operation.
	if rebuilt := compute(s); !reflect.DeepEqual(rows, rebuilt) {
		t.Fatal("rebuild differs")
	}
	if _, err = Publish(ctx, conn, s, rows[:1]); err == nil {
		t.Fatal("incomplete publication accepted")
	}
	start := time.Now()
	changed, err := Publish(ctx, conn, s, rows)
	if err != nil || !changed {
		t.Fatalf("publish %v %v", changed, err)
	}
	t.Logf("publication=%s", time.Since(start))
	var receipt time.Time
	if err = conn.QueryRow(ctx, "SELECT published_at FROM analytics_cohort_publication WHERE sport='NBA' AND season=2198").Scan(&receipt); err != nil {
		t.Fatal(err)
	}
	changed, err = Publish(ctx, conn, s, rows)
	if err != nil || changed {
		t.Fatalf("replay %v %v", changed, err)
	}
	var replayReceipt time.Time
	if err = conn.QueryRow(ctx, "SELECT published_at FROM analytics_cohort_publication WHERE sport='NBA' AND season=2198").Scan(&replayReceipt); err != nil || !receipt.Equal(replayReceipt) {
		t.Fatal("replay rewrote receipt")
	}
	// Correction during compute invalidates the old export. Frozen replay is unchanged.
	inflight := export(1)
	exec("UPDATE team_stats SET rating=13 WHERE sport='NBA' AND season=2198 AND team_id=9600001")
	if _, err = Publish(ctx, conn, inflight, compute(inflight)); !errors.Is(err, ErrSuperseded) {
		t.Fatalf("newer source lost: %v", err)
	}
	corrected := export(2)
	correctRows := compute(corrected)
	// Fail late in projection publication, after DELETE/INSERT: all products and
	// receipt must roll back. The same retained batch can then retry.
	exec(`CREATE FUNCTION public.test_cohort_receipt_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.season=2198 THEN RAISE EXCEPTION 'injected receipt failure'; END IF; RETURN NEW; END $$`)
	exec(`CREATE TRIGGER test_cohort_receipt_failure BEFORE UPDATE ON analytics_cohort_publication FOR EACH ROW EXECUTE FUNCTION test_cohort_receipt_failure()`)
	_, failed := Publish(ctx, conn, corrected, correctRows)
	exec("DROP TRIGGER test_cohort_receipt_failure ON analytics_cohort_publication")
	exec("DROP FUNCTION public.test_cohort_receipt_failure()")
	if failed == nil {
		t.Fatal("failure not injected")
	}
	var rating float64
	if err = conn.QueryRow(ctx, "SELECT rating FROM analytics_entity_context WHERE sport='NBA' AND season=2198 AND entity_id=9600001").Scan(&rating); err != nil || rating != 12 {
		t.Fatalf("partial projection escaped %v %v", rating, err)
	}
	if changed, err = Publish(ctx, conn, corrected, correctRows); err != nil || !changed {
		t.Fatalf("retry %v %v", changed, err)
	}
	if _, err = Publish(ctx, conn, s, rows); !errors.Is(err, ErrSuperseded) {
		t.Fatalf("old batch accepted %v", err)
	}
	newer := export(3)
	if _, err = Publish(ctx, conn, newer, compute(newer)); err != nil {
		t.Fatal(err)
	}
	if _, err = Publish(ctx, conn, corrected, correctRows); !errors.Is(err, ErrSuperseded) {
		t.Fatalf("same-input older batch accepted %v", err)
	}
	exec("DELETE FROM team_stats WHERE sport='NBA' AND season=2198")
	empty := export(4)
	emptyRows := compute(empty)
	if len(emptyRows) != 0 {
		t.Fatal("deleted population survived")
	}
	if _, err = Publish(ctx, conn, empty, emptyRows); err != nil {
		t.Fatal(err)
	}
	var count int
	if err = conn.QueryRow(ctx, "SELECT count(*) FROM analytics_entity_context WHERE sport='NBA' AND season=2198").Scan(&count); err != nil || count != 0 {
		t.Fatalf("stale projection rows %d %v", count, err)
	}
	var dirtyAfter time.Time
	if err = conn.QueryRow(ctx, "SELECT last_marked_at FROM momentum_refresh_needed WHERE sport='NBA'").Scan(&dirtyAfter); err != nil || !dirtyBefore.Equal(dirtyAfter) {
		t.Fatalf("dirty obligation changed: %v", err)
	}
}

func TestCompareRejectsSemanticDifferences(t *testing.T) {
	z := 0.0
	one := 1.0
	a := []model.EntityContextRow{{EntityID: 1, Rating: 1, DeltaPctile: &z}}
	b := []model.EntityContextRow{{EntityID: 1, Rating: 1, DeltaPctile: &one}}
	if Compare(a, b) == nil {
		t.Fatal("numeric mismatch accepted")
	}
	b[0].DeltaPctile = nil
	if Compare(a, b) == nil {
		t.Fatal("missingness accepted")
	}
	b[0] = a[0]
	b[0].EntityID = 2
	if Compare(a, b) == nil {
		t.Fatal("membership accepted")
	}
}
