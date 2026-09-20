package snapshot

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"time"

	"github.com/albapepper/scoracle-data/internal/analytics/duckdb"
	"github.com/albapepper/scoracle-data/internal/analytics/model"
	"github.com/jackc/pgx/v5"
)

const RefreshInterval = 5 * time.Minute

// Maintain gives the API one bounded analytical producer. PostgreSQL stores its
// inputs and durable receipts; DuckDB is private, disposable computation. A failed
// pass leaves prior publications intact and is retried without new source writes.
func Maintain(ctx context.Context, url string, logger *slog.Logger) {
	ticker := time.NewTicker(RefreshInterval)
	defer ticker.Stop()
	for {
		pass, cancel := context.WithTimeout(ctx, 2*time.Minute)
		cfg, err := pgx.ParseConfig(url)
		if err == nil {
			cfg.RuntimeParams["application_name"] = "scoracle-duckdb-cohorts"
			var conn *pgx.Conn
			conn, err = pgx.ConnectConfig(pass, cfg)
			if err == nil {
				var result RefreshResult
				result, err = Refresh(pass, conn)
				closeCtx, closeCancel := context.WithTimeout(context.Background(), 5*time.Second)
				_ = conn.Close(closeCtx)
				closeCancel()
				logger.Info("DuckDB cohort refresh", "checked", result.Checked, "published", result.Published, "unchanged", result.Unchanged, "busy", result.Busy)
			}
		}
		cancel()
		if err != nil && ctx.Err() == nil {
			logger.Error("DuckDB cohort refresh failed; retained prior publications", "error", err)
		}
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
	}
}

type RefreshResult struct {
	Checked, Published, Unchanged int
	Busy                          bool
}

// Refresh checks current and historical scopes so corrections, NULL transitions,
// deleted members and season rollovers all participate in invalidation. The caller
// owns the connection. A session lock prevents competing API instances doing the
// same study; Publish separately fences every source snapshot at commit.
func Refresh(ctx context.Context, conn *pgx.Conn) (result RefreshResult, err error) {
	var locked bool
	if err = conn.QueryRow(ctx, "SELECT pg_try_advisory_lock(hashtextextended('duckdb-cohort-producer',0))").Scan(&locked); err != nil {
		return
	}
	if !locked {
		result.Busy = true
		return
	}
	defer func() {
		release, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		var unlocked bool
		unlockErr := conn.QueryRow(release, "SELECT pg_advisory_unlock(hashtextextended('duckdb-cohort-producer',0))").Scan(&unlocked)
		err = errors.Join(err, unlockErr)
	}()
	rows, err := conn.Query(ctx, `SELECT sport,entity_type,season FROM (
 SELECT sport,'player'::text AS entity_type,season FROM public.player_stats
 UNION SELECT sport,'team',season FROM public.team_stats
 UNION SELECT sport,entity_type,season FROM public.analytics_cohort_publication
 ) scopes WHERE sport IN ('NBA','NFL','FOOTBALL') ORDER BY season DESC,sport,entity_type LIMIT 121`)
	if err != nil {
		return result, err
	}
	var scopes []model.CohortScope
	for rows.Next() {
		var scope model.CohortScope
		if err = rows.Scan(&scope.Sport, &scope.EntityType, &scope.Season); err != nil {
			rows.Close()
			return result, err
		}
		scopes = append(scopes, scope)
	}
	rows.Close()
	if err = rows.Err(); err != nil {
		return result, err
	}
	if len(scopes) > 120 {
		return result, fmt.Errorf("cohort scope limit exceeded: review source seasons")
	}
	var engine *duckdb.Analytics
	defer func() {
		if engine != nil {
			err = errors.Join(err, engine.Close(context.Background()))
		}
	}()
	var failures []error
	for _, scope := range scopes {
		if ctx.Err() != nil {
			failures = append(failures, ctx.Err())
			break
		}
		result.Checked++
		snapshot, exportErr := Export(ctx, conn, scope, time.Now())
		if exportErr != nil {
			failures = append(failures, fmt.Errorf("%+v export: %w", scope, exportErr))
			continue
		}
		var unchanged bool
		checkErr := conn.QueryRow(ctx, `SELECT EXISTS(SELECT 1 FROM public.analytics_cohort_publication
   WHERE sport=$1 AND entity_type=$2 AND season=$3 AND input_hash=$4 AND formula=$5)`,
			scope.Sport, scope.EntityType, scope.Season, snapshot.InputHash, snapshot.Formula).Scan(&unchanged)
		if checkErr != nil {
			failures = append(failures, checkErr)
			continue
		}
		if unchanged {
			result.Unchanged++
			continue
		}
		if engine == nil {
			engine, err = duckdb.Open(ctx, duckdb.Options{MemoryLimit: "256MB"})
			if err != nil {
				return result, err
			}
		}
		output, computeErr := engine.SnapshotContext(ctx, snapshot)
		if computeErr != nil {
			failures = append(failures, fmt.Errorf("%+v compute: %w", scope, computeErr))
			continue
		}
		published, publishErr := Publish(ctx, conn, snapshot, output)
		if publishErr != nil {
			failures = append(failures, fmt.Errorf("%+v publish: %w", scope, publishErr))
			continue
		}
		if published {
			result.Published++
		}
	}
	return result, errors.Join(failures...)
}
