package work

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// These exercise the real claim/enqueue SQL against Postgres (FOR UPDATE SKIP
// LOCKED, ON CONFLICT reopen, lease recovery) and are skipped unless
// TEST_DATABASE_URL points at a migrated database (Session 16 wires this into
// CI). A sentinel sport keeps them isolated from real rows.
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

func countRows(t *testing.T, pool *pgxpool.Pool, stage Stage, id int) (status string, n int) {
	t.Helper()
	rows, err := pool.Query(context.Background(),
		`SELECT status FROM pipeline_work WHERE stage=$1 AND entity_type='team' AND entity_id=$2 AND sport=$3`,
		string(stage), id, testSport)
	if err != nil {
		t.Fatalf("count: %v", err)
	}
	defer rows.Close()
	for rows.Next() {
		if err := rows.Scan(&status); err != nil {
			t.Fatalf("count scan: %v", err)
		}
		n++
	}
	return status, n
}

func TestEnqueueDedups(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	for i := 0; i < 3; i++ {
		if err := Enqueue(ctx, pool, item(StageNarratives, 1, "v1")); err != nil {
			t.Fatalf("enqueue: %v", err)
		}
	}
	status, n := countRows(t, pool, StageNarratives, 1)
	if n != 1 {
		t.Fatalf("want 1 row after duplicate enqueues, got %d", n)
	}
	if status != "pending" {
		t.Fatalf("want pending, got %q", status)
	}
}

func TestChangedInputReopens(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	// Enqueue v1, claim it, complete it (row deleted).
	if err := Enqueue(ctx, pool, item(StageVibe, 2, "v1")); err != nil {
		t.Fatalf("enqueue v1: %v", err)
	}
	claimed, err := Claim(ctx, pool, StageVibe, 10)
	if err != nil {
		t.Fatalf("claim: %v", err)
	}
	if len(claimed) != 1 {
		t.Fatalf("want 1 claimed, got %d", len(claimed))
	}
	if err := Complete(ctx, pool, claimed[0]); err != nil {
		t.Fatalf("complete: %v", err)
	}
	if _, n := countRows(t, pool, StageVibe, 2); n != 0 {
		t.Fatalf("want 0 rows after complete, got %d", n)
	}

	// A changed input re-creates claimable work.
	if err := Enqueue(ctx, pool, item(StageVibe, 2, "v2")); err != nil {
		t.Fatalf("enqueue v2: %v", err)
	}
	again, err := Claim(ctx, pool, StageVibe, 10)
	if err != nil {
		t.Fatalf("claim again: %v", err)
	}
	if len(again) != 1 || again[0].InputVersion != "v2" {
		t.Fatalf("want reopened v2 work, got %+v", again)
	}
}

func TestEnqueueReopensInTableOnNewVersion(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	// Same-version re-enqueue while pending is a no-op; a new version reopens.
	if err := Enqueue(ctx, pool, item(StageMomentum, 3, "v1")); err != nil {
		t.Fatalf("enqueue v1: %v", err)
	}
	if err := Enqueue(ctx, pool, item(StageMomentum, 3, "v2")); err != nil {
		t.Fatalf("enqueue v2: %v", err)
	}
	claimed, err := Claim(ctx, pool, StageMomentum, 10)
	if err != nil {
		t.Fatalf("claim: %v", err)
	}
	if len(claimed) != 1 || claimed[0].InputVersion != "v2" {
		t.Fatalf("want single v2 row, got %+v", claimed)
	}
}

func TestClaimIsExclusiveAndDrains(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	for i := 10; i < 14; i++ {
		if err := Enqueue(ctx, pool, item(StageTransfers, i, "")); err != nil {
			t.Fatalf("enqueue %d: %v", i, err)
		}
	}

	first, err := Claim(ctx, pool, StageTransfers, 2)
	if err != nil {
		t.Fatalf("claim 1: %v", err)
	}
	second, err := Claim(ctx, pool, StageTransfers, 2)
	if err != nil {
		t.Fatalf("claim 2: %v", err)
	}
	if len(first) != 2 || len(second) != 2 {
		t.Fatalf("want 2+2 claimed, got %d+%d", len(first), len(second))
	}
	seen := map[int]bool{}
	for _, it := range append(first, second...) {
		if seen[it.EntityID] {
			t.Fatalf("entity %d claimed twice — SKIP LOCKED failed", it.EntityID)
		}
		seen[it.EntityID] = true
	}
	// All four are now 'running' → nothing left to claim.
	third, err := Claim(ctx, pool, StageTransfers, 2)
	if err != nil {
		t.Fatalf("claim 3: %v", err)
	}
	if len(third) != 0 {
		t.Fatalf("want 0 left, got %d", len(third))
	}
}

func TestRequeueStaleRecoversClaim(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	if err := Enqueue(ctx, pool, item(StageSigil, 20, "")); err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	if _, err := Claim(ctx, pool, StageSigil, 10); err != nil {
		t.Fatalf("claim: %v", err)
	}
	if status, _ := countRows(t, pool, StageSigil, 20); status != "running" {
		t.Fatalf("want running after claim, got %q", status)
	}

	// Backdate the lease so the row looks abandoned, then recover it.
	if _, err := pool.Exec(ctx,
		`UPDATE pipeline_work SET updated_at = NOW() - INTERVAL '1 hour'
		 WHERE stage=$1 AND entity_id=$2 AND sport=$3`,
		string(StageSigil), 20, testSport); err != nil {
		t.Fatalf("backdate: %v", err)
	}
	recovered, err := RequeueStale(ctx, pool, 5*time.Minute)
	if err != nil {
		t.Fatalf("requeue stale: %v", err)
	}
	if recovered != 1 {
		t.Fatalf("want 1 recovered, got %d", recovered)
	}
	reclaim, err := Claim(ctx, pool, StageSigil, 10)
	if err != nil {
		t.Fatalf("reclaim: %v", err)
	}
	if len(reclaim) != 1 {
		t.Fatalf("want 1 reclaimable after recovery, got %d", len(reclaim))
	}
}

func TestFailBacksOffThenDeadLetters(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	if err := Enqueue(ctx, pool, item(StageNarratives, 30, "")); err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	claimed, err := Claim(ctx, pool, StageNarratives, 10)
	if err != nil || len(claimed) != 1 {
		t.Fatalf("claim: %v len=%d", err, len(claimed))
	}
	// maxAttempts=1 → first failure parks it as a dead-letter (far future), so
	// it is not immediately re-claimable.
	if err := Fail(ctx, pool, claimed[0], "boom", time.Minute, 1); err != nil {
		t.Fatalf("fail: %v", err)
	}
	if status, _ := countRows(t, pool, StageNarratives, 30); status != "failed" {
		t.Fatalf("want failed, got %q", status)
	}
	next, err := Claim(ctx, pool, StageNarratives, 10)
	if err != nil {
		t.Fatalf("claim after fail: %v", err)
	}
	if len(next) != 0 {
		t.Fatalf("dead-lettered row should not be claimable, got %d", len(next))
	}
}
