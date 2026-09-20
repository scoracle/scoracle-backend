package duckdb

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	"github.com/albapepper/scoracle-data/internal/analytics/model"
)

// SnapshotContext uses the existing cohort formula over private, frozen inputs.
// Use one private engine per batch; no Postgres attachment or extension is needed.
func (a *Analytics) SnapshotContext(ctx context.Context, snapshot model.CohortSnapshot) ([]model.EntityContextRow, error) {
	raw, err := json.Marshal(snapshot.Inputs)
	if err != nil {
		return nil, err
	}
	_, err = a.database.ExecContext(ctx, `CREATE OR REPLACE TEMP TABLE cohort_input AS
 SELECT (value->>'entity_id')::INTEGER AS entity_id,
        (value->>'league_id')::INTEGER AS league_id,
        (value->>'season')::INTEGER AS season,
        (value->>'rating')::DOUBLE AS rating
 FROM json_each(?)`, string(raw))
	if err != nil {
		return nil, fmt.Errorf("load frozen cohort: %w", err)
	}
	query := fmt.Sprintf(contextQuery, sqlLiteral(snapshot.Scope.Sport), snapshot.Scope.Season, "cohort_input", "entity_id")
	query = strings.Replace(query, "pg.public.cohort_input", "cohort_input", 1)
	query = strings.Replace(query, "sport = "+sqlLiteral(snapshot.Scope.Sport)+" AND ", "", 1)
	return a.queryEntityContext(ctx, snapshot.Scope.Sport, snapshot.Scope.Season, snapshot.Scope.EntityType, query)
}
