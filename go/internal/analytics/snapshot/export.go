// Package snapshot owns bounded cohort export and publication, not computation.
package snapshot

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math"
	"time"

	"github.com/albapepper/scoracle-data/internal/analytics/model"
	"github.com/jackc/pgx/v5"
)

const Formula = "cohort-context-v1"
const MaxRows = 100000

func source(scope model.CohortScope) (string, string, error) {
	if scope.Sport != "NBA" && scope.Sport != "NFL" && scope.Sport != "FOOTBALL" {
		return "", "", fmt.Errorf("unsupported sport %q", scope.Sport)
	}
	if scope.Season < 1900 || scope.Season > 2200 {
		return "", "", fmt.Errorf("explicit season required")
	}
	switch scope.EntityType {
	case "player":
		return "player_stats", "player_id", nil
	case "team":
		return "team_stats", "team_id", nil
	default:
		return "", "", fmt.Errorf("entity type must be player or team")
	}
}

func hash(v any) (string, error) {
	data, err := json.Marshal(v)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:]), nil
}

func readInputs(ctx context.Context, tx pgx.Tx, scope model.CohortScope) ([]model.CohortInput, error) {
	table, id, err := source(scope)
	if err != nil {
		return nil, err
	}
	rows, err := tx.Query(ctx, fmt.Sprintf(`SELECT %s, COALESCE(league_id,0), season, rating::float8
 FROM public.%s WHERE sport=$1 AND season IN ($2, $2-1)
 ORDER BY league_id NULLS FIRST, %s, season LIMIT $3`, id, table, id), scope.Sport, scope.Season, MaxRows+1)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	inputs := []model.CohortInput{}
	seen := map[[3]int32]bool{}
	for rows.Next() {
		var r model.CohortInput
		if err := rows.Scan(&r.EntityID, &r.LeagueID, &r.Season, &r.Rating); err != nil {
			return nil, err
		}
		key := [3]int32{r.LeagueID, r.EntityID, r.Season}
		if seen[key] {
			return nil, fmt.Errorf("duplicate normalized cohort key %v", key)
		}
		seen[key] = true
		if r.Rating != nil && (math.IsNaN(*r.Rating) || math.IsInf(*r.Rating, 0)) {
			return nil, fmt.Errorf("nonfinite rating")
		}
		inputs = append(inputs, r)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	if len(inputs) > MaxRows {
		return nil, fmt.Errorf("cohort exceeds %d input rows", MaxRows)
	}
	return inputs, nil
}

// Export closes its bounded repeatable-read, read-only transaction before compute.
// Hashes include corrections, deletions and NULLs, not a max(id) watermark.
func Export(ctx context.Context, conn *pgx.Conn, scope model.CohortScope, asOf time.Time) (model.CohortSnapshot, error) {
	s := model.CohortSnapshot{Scope: scope, AsOf: asOf.UTC().Truncate(time.Microsecond), Formula: Formula}
	if _, _, err := source(scope); err != nil {
		return s, err
	}
	if asOf.IsZero() {
		return s, fmt.Errorf("fixed as_of required")
	}
	tx, err := conn.BeginTx(ctx, pgx.TxOptions{IsoLevel: pgx.RepeatableRead, AccessMode: pgx.ReadOnly})
	if err != nil {
		return s, err
	}
	defer tx.Rollback(ctx)
	if _, err = tx.Exec(ctx, "SET LOCAL statement_timeout='15s'"); err != nil {
		return s, err
	}
	if err = tx.QueryRow(ctx, "SELECT pg_current_snapshot()::text, transaction_timestamp()").Scan(&s.MVCCSnapshot, &s.CapturedAt); err != nil {
		return s, err
	}
	s.Inputs, err = readInputs(ctx, tx, scope)
	if err != nil {
		return s, err
	}
	s.InputHash, err = hash(s.Inputs)
	if err != nil {
		return s, err
	}
	return s, tx.Commit(ctx)
}

// Validate rejects malformed or altered replay inputs before either engine runs.
func Validate(s model.CohortSnapshot) error {
	if _, _, err := source(s.Scope); err != nil {
		return err
	}
	if s.Formula != Formula || s.AsOf.IsZero() || s.CapturedAt.IsZero() || s.MVCCSnapshot == "" || !s.AsOf.Equal(s.AsOf.Truncate(time.Microsecond)) || len(s.Inputs) > MaxRows {
		return fmt.Errorf("invalid snapshot manifest")
	}
	h, err := hash(s.Inputs)
	if err != nil {
		return err
	}
	if h != s.InputHash {
		return fmt.Errorf("snapshot checksum mismatch")
	}
	seen := map[[3]int32]bool{}
	for _, r := range s.Inputs {
		k := [3]int32{r.LeagueID, r.EntityID, r.Season}
		if seen[k] || (r.Season != s.Scope.Season && r.Season != s.Scope.Season-1) {
			return fmt.Errorf("invalid snapshot membership")
		}
		seen[k] = true
	}
	return nil
}
