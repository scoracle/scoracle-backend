package snapshot

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"time"

	"github.com/albapepper/scoracle-data/internal/analytics/model"
	"github.com/jackc/pgx/v5"
)

var ErrSuperseded = errors.New("cohort inputs or publication superseded; export again")

// Publish atomically replaces the complete cohort and its receipt. The existing
// context table IS the serving projection: failure leaves the prior projection
// and receipt intact. It never acknowledges dirty work or invokes cognition.
// A short SHARE lock closes the source-validation/commit race, including deletes.
// This deliberately conservative first canary blocks source writers briefly;
// lock/statement deadlines bound impact, and no lock spans DuckDB computation.
func Publish(ctx context.Context, conn *pgx.Conn, s model.CohortSnapshot, results []model.EntityContextRow) (bool, error) {
	table, _, err := source(s.Scope)
	if err != nil {
		return false, err
	}
	if err := Validate(s); err != nil {
		return false, err
	}
	expected := map[[2]int32]float64{}
	for _, r := range s.Inputs {
		if r.Season == s.Scope.Season && r.Rating != nil {
			expected[[2]int32{r.LeagueID, r.EntityID}] = *r.Rating
		}
	}
	if len(results) != len(expected) {
		return false, fmt.Errorf("incomplete cohort result")
	}
	seen := map[[2]int32]bool{}
	for _, r := range results {
		key := [2]int32{r.LeagueID, r.EntityID}
		rating, exists := expected[key]
		if !exists || rating != r.Rating || r.Sport != s.Scope.Sport || r.EntityType != s.Scope.EntityType || r.Season != s.Scope.Season || seen[key] {
			return false, fmt.Errorf("result scope/duplicate mismatch")
		}
		seen[key] = true
	}
	resultHash, err := hash(results)
	if err != nil {
		return false, err
	}
	tx, err := conn.Begin(ctx)
	if err != nil {
		return false, err
	}
	defer tx.Rollback(ctx)
	if _, err = tx.Exec(ctx, "SET LOCAL lock_timeout='2s'; SET LOCAL statement_timeout='15s'"); err != nil {
		return false, err
	}
	scopeKey := fmt.Sprintf("cohort/%s/%s/%d", s.Scope.Sport, s.Scope.EntityType, s.Scope.Season)
	if _, err = tx.Exec(ctx, "SELECT pg_advisory_xact_lock(hashtextextended($1,0))", scopeKey); err != nil {
		return false, err
	}
	if _, err = tx.Exec(ctx, "LOCK TABLE public."+table+" IN SHARE MODE"); err != nil {
		return false, err
	}
	inputs, err := readInputs(ctx, tx, s.Scope)
	if err != nil {
		return false, err
	}
	currentHash, err := hash(inputs)
	if err != nil {
		return false, err
	}
	if currentHash != s.InputHash {
		return false, ErrSuperseded
	}
	var priorAsOf time.Time
	var priorInput, priorResult, priorFormula string
	err = tx.QueryRow(ctx, `SELECT as_of,input_hash,result_hash,formula FROM public.analytics_cohort_publication
 WHERE sport=$1 AND entity_type=$2 AND season=$3 FOR UPDATE`, s.Scope.Sport, s.Scope.EntityType, s.Scope.Season).Scan(&priorAsOf, &priorInput, &priorResult, &priorFormula)
	if err != nil && !errors.Is(err, pgx.ErrNoRows) {
		return false, err
	}
	if err == nil {
		if priorAsOf.After(s.AsOf) {
			return false, ErrSuperseded
		}
		if priorAsOf.Equal(s.AsOf) {
			if priorInput == s.InputHash && priorResult == resultHash && priorFormula == s.Formula {
				return false, tx.Commit(ctx)
			}
			return false, ErrSuperseded
		}
	}
	if _, err = tx.Exec(ctx, `DELETE FROM public.analytics_entity_context WHERE sport=$1 AND entity_type=$2 AND season=$3`, s.Scope.Sport, s.Scope.EntityType, s.Scope.Season); err != nil {
		return false, err
	}
	raw, err := json.Marshal(results)
	if err != nil {
		return false, err
	}
	if _, err = tx.Exec(ctx, `INSERT INTO public.analytics_entity_context
 (sport,entity_type,entity_id,season,league_id,rating,prior_season,prior_rating,delta,delta_pctile,peer_count,peer_delta_median,peer_delta_p25,peer_delta_p75,computed_at)
 SELECT sport,entity_type,entity_id,season,league_id,rating,prior_season,prior_rating,delta,delta_pctile,peer_count,peer_delta_median,peer_delta_p25,peer_delta_p75,$2
 FROM jsonb_populate_recordset(NULL::public.analytics_entity_context,$1::jsonb)`, string(raw), s.CapturedAt); err != nil {
		return false, err
	}
	_, err = tx.Exec(ctx, `INSERT INTO public.analytics_cohort_publication
 (sport,entity_type,season,as_of,captured_at,input_hash,result_hash,formula,mvcc_snapshot,row_count)
 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
 ON CONFLICT (sport,entity_type,season) DO UPDATE SET as_of=excluded.as_of,captured_at=excluded.captured_at,input_hash=excluded.input_hash,result_hash=excluded.result_hash,formula=excluded.formula,mvcc_snapshot=excluded.mvcc_snapshot,row_count=excluded.row_count,published_at=now()`, s.Scope.Sport, s.Scope.EntityType, s.Scope.Season, s.AsOf, s.CapturedAt, s.InputHash, resultHash, s.Formula, s.MVCCSnapshot, len(results))
	if err != nil {
		return false, err
	}
	return true, tx.Commit(ctx)
}
