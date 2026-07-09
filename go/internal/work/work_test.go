package work

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// These exercise the real enqueue/operator SQL against Postgres and are skipped
// unless TEST_DATABASE_URL points at a migrated database. A sentinel sport keeps
// them isolated from real rows.
const testSport = "ZZ_WORK_TEST"

func testPool(t *testing.T) *pgxpool.Pool {
	t.Helper()
	url := os.Getenv("TEST_DATABASE_URL")
	if url == "" {
		t.Skip("TEST_DATABASE_URL not set; skipping pipeline_work integration tests")
	}
	pool, err := pgxpool.New(context.Background(), url)
	if err != nil {
		t.Fatalf("connect test db: %v", err)
	}
	clean(t, pool)
	t.Cleanup(func() {
		clean(t, pool)
		pool.Close()
	})
	return pool
}

func clean(t *testing.T, pool *pgxpool.Pool) {
	t.Helper()
	if _, err := pool.Exec(context.Background(),
		`DELETE FROM pipeline_work WHERE sport = $1`, testSport); err != nil {
		t.Fatalf("clean: %v", err)
	}
}

func item(stage Stage, id int, version string) Item {
	return Item{Stage: stage, EntityType: "team", EntityID: id, Sport: testSport, InputVersion: version}
}

func rowState(t *testing.T, pool *pgxpool.Pool, stage Stage, id int) (status, inputVersion string, n int) {
	t.Helper()
	rows, err := pool.Query(context.Background(),
		`SELECT status, COALESCE(input_version, '')
		 FROM pipeline_work
		 WHERE stage=$1 AND entity_type='team' AND entity_id=$2 AND sport=$3`,
		string(stage), id, testSport)
	if err != nil {
		t.Fatalf("row state: %v", err)
	}
	defer rows.Close()
	for rows.Next() {
		if err := rows.Scan(&status, &inputVersion); err != nil {
			t.Fatalf("row state scan: %v", err)
		}
		n++
	}
	if err := rows.Err(); err != nil {
		t.Fatalf("row state rows: %v", err)
	}
	return status, inputVersion, n
}

func markRunning(t *testing.T, pool *pgxpool.Pool, stage Stage, id int, updatedAt string) {
	t.Helper()
	_, err := pool.Exec(context.Background(), `
		UPDATE pipeline_work
		   SET status='running', updated_at = NOW() - ($4 || ' seconds')::interval
		 WHERE stage=$1 AND entity_type='team' AND entity_id=$2 AND sport=$3`,
		string(stage), id, testSport, updatedAt)
	if err != nil {
		t.Fatalf("mark running: %v", err)
	}
}

func TestEnqueueDedups(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	for i := 0; i < 3; i++ {
		if err := Enqueue(ctx, pool, item(StageNarratives, 1, "v1")); err != nil {
			t.Fatalf("enqueue: %v", err)
		}
	}
	status, inputVersion, n := rowState(t, pool, StageNarratives, 1)
	if n != 1 {
		t.Fatalf("want 1 row after duplicate enqueues, got %d", n)
	}
	if status != "pending" || inputVersion != "v1" {
		t.Fatalf("want pending/v1, got %q/%q", status, inputVersion)
	}
}

func TestChangedInputReopensPendingRow(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	if err := Enqueue(ctx, pool, item(StageMomentum, 3, "v1")); err != nil {
		t.Fatalf("enqueue v1: %v", err)
	}
	if err := Enqueue(ctx, pool, item(StageMomentum, 3, "v2")); err != nil {
		t.Fatalf("enqueue v2: %v", err)
	}
	status, inputVersion, n := rowState(t, pool, StageMomentum, 3)
	if n != 1 || status != "pending" || inputVersion != "v2" {
		t.Fatalf("want one pending v2 row, got n=%d status=%q version=%q", n, status, inputVersion)
	}
}

func TestRequeueStaleRecoversRunningRow(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	if err := Enqueue(ctx, pool, item(StageSigil, 20, "")); err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	markRunning(t, pool, StageSigil, 20, "3600")

	recovered, err := RequeueStale(ctx, pool, 5*time.Minute)
	if err != nil {
		t.Fatalf("requeue stale: %v", err)
	}
	if recovered != 1 {
		t.Fatalf("want 1 recovered, got %d", recovered)
	}
	status, _, n := rowState(t, pool, StageSigil, 20)
	if n != 1 || status != "pending" {
		t.Fatalf("want recovered pending row, got n=%d status=%q", n, status)
	}
}

func TestDeadLettersReportsParkedRows(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	if err := Enqueue(ctx, pool, item(StageNarratives, 42, "")); err != nil {
		t.Fatalf("enqueue dead: %v", err)
	}
	if err := Enqueue(ctx, pool, item(StageNarratives, 43, "")); err != nil {
		t.Fatalf("enqueue retryable: %v", err)
	}
	if _, err := pool.Exec(ctx, `
		UPDATE pipeline_work
		   SET status='failed', attempts=1, last_error='boom',
		       available_at = NOW() + INTERVAL '100 years'
		 WHERE stage=$1 AND entity_id=$2 AND sport=$3`,
		string(StageNarratives), 42, testSport); err != nil {
		t.Fatalf("park dead row: %v", err)
	}
	if _, err := pool.Exec(ctx, `
		UPDATE pipeline_work
		   SET status='failed', attempts=1, last_error='transient',
		       available_at = NOW() + INTERVAL '1 minute'
		 WHERE stage=$1 AND entity_id=$2 AND sport=$3`,
		string(StageNarratives), 43, testSport); err != nil {
		t.Fatalf("mark retryable row: %v", err)
	}

	dls, err := DeadLetters(ctx, pool)
	if err != nil {
		t.Fatalf("dead letters: %v", err)
	}
	var seen42, seen43 bool
	for _, d := range dls {
		if d.Sport != testSport {
			continue
		}
		if d.EntityID == 42 {
			seen42 = true
		}
		if d.EntityID == 43 {
			seen43 = true
		}
	}
	if !seen42 {
		t.Fatalf("want the parked row (42) reported as a dead-letter")
	}
	if seen43 {
		t.Fatalf("a still-retryable row (43) must not be reported as a dead-letter")
	}
}

func TestEnqueueSameVersionWhileRunningIsNoop(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	if err := Enqueue(ctx, pool, item(StageSigil, 50, "v1")); err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	markRunning(t, pool, StageSigil, 50, "0")

	// Reconciler re-enqueues the same entity, same input_version, while it runs.
	if err := Enqueue(ctx, pool, item(StageSigil, 50, "v1")); err != nil {
		t.Fatalf("re-enqueue same version: %v", err)
	}
	status, inputVersion, n := rowState(t, pool, StageSigil, 50)
	if n != 1 || status != "running" || inputVersion != "v1" {
		t.Fatalf("want running v1 row left untouched, got n=%d status=%q version=%q", n, status, inputVersion)
	}
}

func TestEnqueueNewVersionWhileRunningReopens(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	if err := Enqueue(ctx, pool, item(StageSigil, 51, "v1")); err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	markRunning(t, pool, StageSigil, 51, "0")

	// New inputs arrive mid-flight.
	if err := Enqueue(ctx, pool, item(StageSigil, 51, "v2")); err != nil {
		t.Fatalf("re-enqueue new version: %v", err)
	}
	status, inputVersion, n := rowState(t, pool, StageSigil, 51)
	if n != 1 || status != "pending" || inputVersion != "v2" {
		t.Fatalf("want reopened pending v2 row, got n=%d status=%q version=%q", n, status, inputVersion)
	}
}
