package maintenance

import (
	"context"
	"fmt"
	"io"
	"log/slog"
	"os"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// Creates a separate synthetic database; never replaces functions in the supplied
// database. The disposable TEST_DATABASE_URL role needs CREATEDB.
func TestMomentumPublicationRecovery(t *testing.T) {
	url := os.Getenv("TEST_DATABASE_URL")
	if url == "" {
		t.Skip("disposable TEST_DATABASE_URL with CREATEDB required")
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	defer cancel()
	admin, err := pgx.Connect(ctx, url)
	if err != nil {
		t.Fatal(err)
	}
	defer admin.Close(context.Background())
	name := fmt.Sprintf("momentum_acceptance_%d", time.Now().UnixNano())
	ident := pgx.Identifier{name}.Sanitize()
	if _, err = admin.Exec(ctx, "CREATE DATABASE "+ident+" TEMPLATE template0"); err != nil {
		t.Fatal(err)
	}
	defer func() {
		if _, err := admin.Exec(context.Background(), "DROP DATABASE "+ident+" WITH (FORCE)"); err != nil {
			t.Error(err)
		}
	}()
	cfg, err := pgxpool.ParseConfig(url)
	if err != nil {
		t.Fatal(err)
	}
	cfg.ConnConfig.Database = name
	pool, err := pgxpool.NewWithConfig(ctx, cfg)
	if err != nil {
		t.Fatal(err)
	}
	defer pool.Close()
	exec := func(q string, args ...any) {
		t.Helper()
		if _, err := pool.Exec(ctx, q, args...); err != nil {
			t.Fatal(err)
		}
	}
	exec(`CREATE TABLE momentum_refresh_needed(sport text PRIMARY KEY, last_marked_at timestamptz NOT NULL);
 CREATE TABLE source_scores(sport text PRIMARY KEY, value int);
 INSERT INTO source_scores VALUES ('NBA', 10);
 CREATE TABLE momentum_scores(id bigint GENERATED ALWAYS AS IDENTITY, sport text, value int);
 CREATE TABLE failure_switch(projection boolean, acknowledgement boolean);
 INSERT INTO failure_switch VALUES(false,false);
 CREATE FUNCTION projection_value(v int) RETURNS int LANGUAGE plpgsql AS $$ BEGIN
   PERFORM pg_advisory_xact_lock(987654321);
   IF (SELECT projection FROM public.failure_switch) THEN RAISE EXCEPTION 'projection failure'; END IF;
   RETURN v;
 END $$;
 CREATE MATERIALIZED VIEW latest_momentum_scores_per_entity AS
 SELECT DISTINCT ON (sport) sport, public.projection_value(value) AS value FROM momentum_scores ORDER BY sport,id DESC;
 CREATE UNIQUE INDEX latest_momentum_key ON latest_momentum_scores_per_entity(sport);
 CREATE FUNCTION refresh_momentum_scores(s text) RETURNS int LANGUAGE plpgsql AS $$ BEGIN
   IF NOT pg_try_advisory_xact_lock(hashtext('refresh_momentum_scores')) THEN RETURN NULL; END IF;
   INSERT INTO public.momentum_scores(sport,value) SELECT sport,value FROM public.source_scores WHERE sport=s;
   RETURN 1;
 END $$;
 CREATE FUNCTION fail_ack() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
   IF (SELECT acknowledgement FROM public.failure_switch) THEN RAISE EXCEPTION 'ack failure'; END IF;
   RETURN OLD;
 END $$;
 CREATE TRIGGER fail_ack BEFORE DELETE ON momentum_refresh_needed FOR EACH ROW EXECUTE FUNCTION fail_ack();`)
	logger := slog.New(slog.NewTextHandler(io.Discard, nil))
	reset := func() {
		t.Helper()
		exec("UPDATE failure_switch SET projection=false, acknowledgement=false; TRUNCATE momentum_scores, momentum_refresh_needed; UPDATE source_scores SET value=10; REFRESH MATERIALIZED VIEW latest_momentum_scores_per_entity; INSERT INTO momentum_refresh_needed VALUES('NBA',now())")
		momentumRefreshMu.Lock()
		momentumLastRefresh = map[string]time.Time{}
		momentumRefreshMu.Unlock()
	}
	defer func() {
		momentumRefreshMu.Lock()
		momentumLastRefresh = map[string]time.Time{}
		momentumRefreshMu.Unlock()
	}()
	assertCounts := func(t *testing.T, scores, projection, dirty int) {
		t.Helper()
		var s, p, d int
		if err := pool.QueryRow(ctx, `SELECT (SELECT count(*) FROM momentum_scores),(SELECT count(*) FROM latest_momentum_scores_per_entity),(SELECT count(*) FROM momentum_refresh_needed)`).Scan(&s, &p, &d); err != nil {
			t.Fatal(err)
		}
		if s != scores || p != projection || d != dirty {
			t.Fatalf("scores/projection/dirty = %d/%d/%d, want %d/%d/%d", s, p, d, scores, projection, dirty)
		}
	}
	for _, failure := range []string{"projection", "acknowledgement"} {
		t.Run(failure+" failure rolls back and retries", func(t *testing.T) {
			reset()
			drainMomentumRefreshNeeded(ctx, pool, logger)
			assertCounts(t, 1, 1, 0)
			exec("UPDATE source_scores SET value=20; INSERT INTO momentum_refresh_needed VALUES('NBA',now())")
			momentumRefreshMu.Lock()
			momentumLastRefresh = map[string]time.Time{}
			momentumRefreshMu.Unlock()
			exec("UPDATE failure_switch SET " + failure + "=true")
			drainMomentumRefreshNeeded(ctx, pool, logger)
			assertCounts(t, 1, 1, 1)
			var value int
			if err := pool.QueryRow(ctx, "SELECT value FROM latest_momentum_scores_per_entity").Scan(&value); err != nil || value != 10 {
				t.Fatalf("previous projection lost: %d %v", value, err)
			}
			exec("UPDATE failure_switch SET " + failure + "=false")
			drainMomentumRefreshNeeded(ctx, pool, logger)
			assertCounts(t, 2, 1, 0)
			if err := pool.QueryRow(ctx, "SELECT value FROM latest_momentum_scores_per_entity").Scan(&value); err != nil || value != 20 {
				t.Fatalf("retry projection stale: %d %v", value, err)
			}
		})
	}
	// Hold publication while newer work arrives, a second drain competes, or
	// the first connection is cancelled. The database lock establishes race ordering.
	for _, abort := range []bool{false, true} {
		t.Run(fmt.Sprintf("concurrent publication abort=%v", abort), func(t *testing.T) {
			reset()
			blocker, err := pool.Acquire(ctx)
			if err != nil {
				t.Fatal(err)
			}
			defer blocker.Release()
			if _, err = blocker.Exec(ctx, "SELECT pg_advisory_lock(987654321)"); err != nil {
				t.Fatal(err)
			}
			defer blocker.Exec(context.Background(), "SELECT pg_advisory_unlock(987654321)")
			runCtx, stop := context.WithCancel(ctx)
			defer stop()
			done := make(chan struct{})
			go func() { defer close(done); drainMomentumRefreshNeeded(runCtx, pool, logger) }()
			for {
				var waiting bool
				if err := pool.QueryRow(ctx, `SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND objid=987654321 AND NOT granted)`).Scan(&waiting); err != nil {
					t.Fatal(err)
				}
				if waiting {
					break
				}
				select {
				case <-done:
					t.Fatal("drain finished before publication gate")
				case <-ctx.Done():
					t.Fatal(ctx.Err())
				case <-time.After(5 * time.Millisecond):
				}
			}
			// A competing process must leave the first process's obligation intact.
			drainMomentumRefreshNeeded(ctx, pool, logger)
			assertCounts(t, 0, 0, 1)
			if abort {
				stop()
			} else {
				exec("UPDATE source_scores SET value=20; UPDATE momentum_refresh_needed SET last_marked_at=last_marked_at+interval '1 second'")
			}
			if _, err = blocker.Exec(ctx, "SELECT pg_advisory_unlock(987654321)"); err != nil {
				t.Fatal(err)
			}
			select {
			case <-done:
			case <-ctx.Done():
				t.Fatal(ctx.Err())
			}
			if abort {
				assertCounts(t, 0, 0, 1)
				// pgx may close a cancelled connection before PostgreSQL has
				// finished aborting it. Wait for server-side lease release.
				for {
					var acquired bool
					if err := blocker.QueryRow(ctx, "SELECT pg_try_advisory_lock(hashtext('refresh_momentum_scores'))").Scan(&acquired); err != nil {
						t.Fatal(err)
					}
					if acquired {
						if _, err := blocker.Exec(ctx, "SELECT pg_advisory_unlock(hashtext('refresh_momentum_scores'))"); err != nil {
							t.Fatal(err)
						}
						break
					}
					select {
					case <-ctx.Done():
						t.Fatal(ctx.Err())
					case <-time.After(5 * time.Millisecond):
					}
				}
			} else {
				assertCounts(t, 1, 1, 1)
				var value int
				if err := pool.QueryRow(ctx, "SELECT value FROM latest_momentum_scores_per_entity").Scan(&value); err != nil || value != 10 {
					t.Fatalf("projection: %d %v", value, err)
				}
				// The normal throttle retains the newer obligation.
				drainMomentumRefreshNeeded(ctx, pool, logger)
				assertCounts(t, 1, 1, 1)
				momentumRefreshMu.Lock()
				momentumLastRefresh = map[string]time.Time{}
				momentumRefreshMu.Unlock()
			}
			drainMomentumRefreshNeeded(ctx, pool, logger)
			if abort {
				assertCounts(t, 1, 1, 0)
			} else {
				assertCounts(t, 2, 1, 0)
			}
		})
	}
}
