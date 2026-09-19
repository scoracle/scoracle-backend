// Package postgres implements the analytics boundary over the existing
// canonical Postgres path. Queries and window semantics mirror the Rust
// Scout's load_rating_trajectory (rust/src/junctions/scout/mod.rs) so the
// DuckDB implementation can be compared against a faithful reproduction of
// production behavior.
package postgres

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"

	"github.com/albapepper/scoracle-data/internal/analytics/model"
	"github.com/albapepper/scoracle-data/internal/db"
)

// Analytics serves analytical context from the canonical Postgres pool.
type Analytics struct {
	pool *db.Pool
}

// New wraps the existing shared pool; it does not own it and Close is a
// no-op.
func New(pool *db.Pool) *Analytics {
	return &Analytics{pool: pool}
}

// Close is a no-op: the pool is shared with the rest of the application.
func (a *Analytics) Close(context.Context) error { return nil }

func eventTable(entityType string) (table string, idCol string, err error) {
	switch entityType {
	case "player":
		return "event_box_scores", "player_id", nil
	case "team":
		return "event_team_stats", "team_id", nil
	default:
		return "", "", fmt.Errorf("unsupported trajectory entity type %q", entityType)
	}
}

// RatingTrajectory reproduces the Scout's two-step path: a scored-event count
// (which sizes the adaptive window), then the most recent window ratings in
// descending fixture-time order, then the Go port of the Rust linear_slope.
// A deterministic (start_time, rating) tiebreaker is added on top of the
// production ORDER BY, which is nondeterministic on equal start times.
func (a *Analytics) RatingTrajectory(ctx context.Context, entityType string, entityID int32, sport string, season int32) (model.Trajectory, error) {
	table, idCol, err := eventTable(entityType)
	if err != nil {
		return model.Trajectory{}, err
	}

	var eventsPlayed int64
	err = a.pool.QueryRow(ctx, fmt.Sprintf(`
		SELECT COUNT(*)
		FROM public.%s e
		WHERE e.%s = $1 AND e.sport = $2 AND e.season = $3
		  AND e.rating IS NOT NULL
	`, table, idCol), entityID, sport, season).Scan(&eventsPlayed)
	if err != nil {
		return model.Trajectory{}, fmt.Errorf("count trajectory events %s/%d: %w", entityType, entityID, err)
	}
	if eventsPlayed < model.WindowMin {
		return model.NewTrajectory(eventsPlayed, nil, 0), nil
	}

	window := model.WindowSize(eventsPlayed)
	rows, err := a.pool.Query(ctx, fmt.Sprintf(`
		SELECT e.rating::float8
		FROM public.%s e
		JOIN public.fixtures f ON f.id = e.fixture_id
		WHERE e.%s = $1
		  AND e.sport = $2
		  AND e.season = $3
		  AND e.rating IS NOT NULL
		ORDER BY f.start_time DESC, e.rating DESC
		LIMIT $4
	`, table, idCol), entityID, sport, season, window)
	if err != nil {
		return model.Trajectory{}, fmt.Errorf("load rating trajectory %s/%d: %w", entityType, entityID, err)
	}
	defer rows.Close()

	var series []float64
	for rows.Next() {
		var rating float64
		if err := rows.Scan(&rating); err != nil {
			return model.Trajectory{}, fmt.Errorf("scan trajectory rating %s/%d: %w", entityType, entityID, err)
		}
		series = append(series, rating)
	}
	if err := rows.Err(); err != nil {
		return model.Trajectory{}, fmt.Errorf("iterate trajectory ratings %s/%d: %w", entityType, entityID, err)
	}

	// Chronological reversal, matching the Rust Scout's
	// composite_chrono.reverse() before the OLS slope.
	chrono := make([]float64, len(series))
	for i, v := range series {
		chrono[len(series)-1-i] = v
	}
	return model.NewTrajectory(eventsPlayed, series, model.LinearSlope(chrono)), nil
}

// RatingBundle runs the production rating engine as-is:
// _compute_rating_bundle (sql/migrations/253). This is the canonical path —
// measurement identity, eligibility and ranks all stay Postgres-owned.
func (a *Analytics) RatingBundle(ctx context.Context, sport string, season int32, rateMode string) ([]model.BundleRow, error) {
	rows, err := a.pool.Query(ctx, `
		SELECT player_id, league_id,
		       composite::float8, composite_rank::float8, composite_score::float8,
		       breakdown::text, scoped_ranks::text, scoped_scores::text
		FROM public._compute_rating_bundle($1, $2, $3)
	`, sport, season, rateMode)
	if err != nil {
		return nil, fmt.Errorf("postgres rating bundle %s/%d/%s: %w", sport, season, rateMode, err)
	}
	defer rows.Close()

	out := []model.BundleRow{}
	for rows.Next() {
		var r model.BundleRow
		var breakdown, scopedRanks, scopedScores sql.NullString
		if err := rows.Scan(&r.PlayerID, &r.LeagueID, &r.Composite, &r.CompositeRank, &r.CompositeScore, &breakdown, &scopedRanks, &scopedScores); err != nil {
			return nil, fmt.Errorf("scan rating bundle %s/%d/%s: %w", sport, season, rateMode, err)
		}
		if err := parseBundleJSON(&r, breakdown, scopedRanks, scopedScores); err != nil {
			return nil, fmt.Errorf("parse rating bundle %s/%d/%s: %w", sport, season, rateMode, err)
		}
		out = append(out, r)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("iterate rating bundle %s/%d/%s: %w", sport, season, rateMode, err)
	}
	return out, nil
}

func parseBundleJSON(r *model.BundleRow, breakdown, scopedRanks, scopedScores sql.NullString) error {
	if breakdown.Valid && breakdown.String != "" && breakdown.String != "[]" {
		entries := []model.BreakdownEntry{}
		if err := json.Unmarshal([]byte(breakdown.String), &entries); err != nil {
			return fmt.Errorf("breakdown: %w", err)
		}
		r.Breakdown = entries
	}
	r.ScopedRanks = parseScopedJSON(scopedRanks)
	r.ScopedScores = parseScopedJSON(scopedScores)
	return nil
}

func parseScopedJSON(raw sql.NullString) map[string]*float64 {
	if !raw.Valid || raw.String == "" || raw.String == "{}" {
		return nil
	}
	scoped := map[string]*float64{}
	if err := json.Unmarshal([]byte(raw.String), &scoped); err != nil {
		return nil
	}
	if len(scoped) == 0 {
		return nil
	}
	return scoped
}
