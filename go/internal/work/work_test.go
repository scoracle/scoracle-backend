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

func TestRequeueHandsBackLeasedRow(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	// Enqueue + claim → 'running' (a held lease).
	if err := Enqueue(ctx, pool, item(StageVibe, 40, "v1")); err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	claimed, err := Claim(ctx, pool, StageVibe, 10)
	if err != nil || len(claimed) != 1 {
		t.Fatalf("claim: %v len=%d", err, len(claimed))
	}
	if status, _ := countRows(t, pool, StageVibe, 40); status != "running" {
		t.Fatalf("want running after claim, got %q", status)
	}

	// Requeue (graceful-shutdown hand-back) → 'pending' and immediately re-claimable.
	if err := Requeue(ctx, pool, claimed[0]); err != nil {
		t.Fatalf("requeue: %v", err)
	}
	if status, _ := countRows(t, pool, StageVibe, 40); status != "pending" {
		t.Fatalf("want pending after requeue, got %q", status)
	}
	again, err := Claim(ctx, pool, StageVibe, 10)
	if err != nil || len(again) != 1 {
		t.Fatalf("want 1 re-claimable after requeue, got %d (%v)", len(again), err)
	}
	// Requeue does NOT burn an attempt — a shutdown is not the item's fault.
	if again[0].Attempts != 0 {
		t.Fatalf("requeue should not bump attempts, got %d", again[0].Attempts)
	}
}

func TestRequeueIsStatusGuarded(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	// A 'pending' (not leased) row is left alone — Requeue only acts on 'running'.
	if err := Enqueue(ctx, pool, item(StageSigil, 41, "v1")); err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	if err := Requeue(ctx, pool, item(StageSigil, 41, "v1")); err != nil {
		t.Fatalf("requeue: %v", err)
	}
	if status, n := countRows(t, pool, StageSigil, 41); n != 1 || status != "pending" {
		t.Fatalf("want untouched pending row, got status=%q n=%d", status, n)
	}
}

func TestDeadLettersReportsParkedRows(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	// A row failed at the cap (maxAttempts=1) is dead-lettered (parked far future).
	if err := Enqueue(ctx, pool, item(StageNarratives, 42, "")); err != nil {
		t.Fatalf("enqueue dead: %v", err)
	}
	dead, err := Claim(ctx, pool, StageNarratives, 10)
	if err != nil || len(dead) != 1 {
		t.Fatalf("claim dead: %v len=%d", err, len(dead))
	}
	if err := Fail(ctx, pool, dead[0], "boom", time.Minute, 1); err != nil {
		t.Fatalf("fail dead: %v", err)
	}

	// A second row failed but still retryable (cap=5) must NOT be reported.
	if err := Enqueue(ctx, pool, item(StageNarratives, 43, "")); err != nil {
		t.Fatalf("enqueue retryable: %v", err)
	}
	retry, err := Claim(ctx, pool, StageNarratives, 10)
	if err != nil || len(retry) != 1 {
		t.Fatalf("claim retryable: %v len=%d", err, len(retry))
	}
	if err := Fail(ctx, pool, retry[0], "transient", time.Minute, 5); err != nil {
		t.Fatalf("fail retryable: %v", err)
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

// TestEnqueueSameVersionWhileRunningIsNoop locks the no-duplicate-scheduled-
// generation guarantee the current-season Sigil reconciler depends on (FIRST-GPT-
// AUDIT Session 12). A reconciler re-enqueues every current-season entity each
// pass; if an entity is already mid-generation (a claimed 'running' row), a
// re-enqueue carrying the SAME input_version must be a no-op — it must NOT reset
// the row to 'pending', which would schedule a second, redundant generation after
// the in-flight one completes. The ON CONFLICT WHERE clause (input_version IS
// DISTINCT FROM EXCLUDED OR status='failed') is what enforces this.
func TestEnqueueSameVersionWhileRunningIsNoop(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	if err := Enqueue(ctx, pool, item(StageSigil, 50, "v1")); err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	claimed, err := Claim(ctx, pool, StageSigil, 10)
	if err != nil || len(claimed) != 1 {
		t.Fatalf("claim: %v len=%d", err, len(claimed))
	}
	if status, _ := countRows(t, pool, StageSigil, 50); status != "running" {
		t.Fatalf("want running after claim, got %q", status)
	}

	// Reconciler re-enqueues the same entity, same input_version, while it runs.
	if err := Enqueue(ctx, pool, item(StageSigil, 50, "v1")); err != nil {
		t.Fatalf("re-enqueue same version: %v", err)
	}
	if status, n := countRows(t, pool, StageSigil, 50); n != 1 || status != "running" {
		t.Fatalf("want the in-flight row left untouched (running, 1 row), got status=%q n=%d", status, n)
	}
	// Nothing new is claimable — no duplicate generation was scheduled.
	leftover, err := Claim(ctx, pool, StageSigil, 10)
	if err != nil {
		t.Fatalf("claim after re-enqueue: %v", err)
	}
	if len(leftover) != 0 {
		t.Fatalf("a same-version re-enqueue during generation must schedule nothing, got %d claimable", len(leftover))
	}
}

// TestEnqueueNewVersionWhileRunningReopens is the deliberate contrast to the
// no-op above: when the entity's INPUTS changed (a new input_version) while a row
// is running, the re-enqueue DOES reopen it to 'pending' so the newer inputs get
// synthesized. Convergence on real change, debounce on no change.
func TestEnqueueNewVersionWhileRunningReopens(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	if err := Enqueue(ctx, pool, item(StageSigil, 51, "v1")); err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	if _, err := Claim(ctx, pool, StageSigil, 10); err != nil {
		t.Fatalf("claim: %v", err)
	}
	if status, _ := countRows(t, pool, StageSigil, 51); status != "running" {
		t.Fatalf("want running after claim, got %q", status)
	}

	// New inputs arrive mid-flight.
	if err := Enqueue(ctx, pool, item(StageSigil, 51, "v2")); err != nil {
		t.Fatalf("re-enqueue new version: %v", err)
	}
	if status, n := countRows(t, pool, StageSigil, 51); n != 1 || status != "pending" {
		t.Fatalf("want a single reopened pending row, got status=%q n=%d", status, n)
	}
	again, err := Claim(ctx, pool, StageSigil, 10)
	if err != nil || len(again) != 1 || again[0].InputVersion != "v2" {
		t.Fatalf("want the reopened v2 work re-claimable, got %+v (%v)", again, err)
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
