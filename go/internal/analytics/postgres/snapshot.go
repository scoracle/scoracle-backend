package postgres

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/albapepper/scoracle-data/internal/analytics/model"
	"github.com/jackc/pgx/v5"
)

// SnapshotContext is the cohort reference over exactly the exported inputs.
// No canonical table is read and no temporary or public table is written.
func SnapshotContext(ctx context.Context, conn *pgx.Conn, s model.CohortSnapshot) ([]model.EntityContextRow, error) {
	raw, err := json.Marshal(s.Inputs)
	if err != nil {
		return nil, err
	}
	rows, err := conn.Query(ctx, `WITH inputs AS (
 SELECT * FROM jsonb_to_recordset($1::jsonb) AS x(entity_id integer, league_id integer, season integer, rating double precision)
 ), deltas AS (
 SELECT c.league_id,c.entity_id,c.season,c.rating,p.season AS prior_season,p.rating AS prior_rating,c.rating-p.rating AS delta
 FROM inputs c LEFT JOIN inputs p ON p.entity_id=c.entity_id AND p.league_id=c.league_id AND p.season=c.season-1 AND p.rating IS NOT NULL
 WHERE c.season=$2 AND c.rating IS NOT NULL
 ), peer AS (
 SELECT league_id,count(*) AS peer_count,
 percentile_cont(0.5) WITHIN GROUP (ORDER BY delta) AS med,
 percentile_cont(0.25) WITHIN GROUP (ORDER BY delta) AS p25,
 percentile_cont(0.75) WITHIN GROUP (ORDER BY delta) AS p75
 FROM deltas WHERE delta IS NOT NULL GROUP BY league_id
 ), pct AS (
 SELECT league_id,entity_id,percent_rank() OVER (PARTITION BY league_id ORDER BY delta)*100 AS pct
 FROM deltas WHERE delta IS NOT NULL
 ) SELECT d.league_id,d.entity_id,d.season,d.rating,d.prior_season,d.prior_rating,d.delta,pct.pct,
 COALESCE(peer.peer_count,0),peer.med,peer.p25,peer.p75
 FROM deltas d LEFT JOIN peer USING (league_id) LEFT JOIN pct USING (league_id,entity_id)
 ORDER BY d.league_id,d.entity_id`, string(raw), s.Scope.Season)
	if err != nil {
		return nil, fmt.Errorf("postgres frozen cohort: %w", err)
	}
	defer rows.Close()
	out := []model.EntityContextRow{}
	for rows.Next() {
		r := model.EntityContextRow{Sport: s.Scope.Sport, EntityType: s.Scope.EntityType}
		if err := rows.Scan(&r.LeagueID, &r.EntityID, &r.Season, &r.Rating, &r.PriorSeason, &r.PriorRating, &r.Delta, &r.DeltaPctile, &r.PeerCount, &r.PeerDeltaMedian, &r.PeerDeltaP25, &r.PeerDeltaP75); err != nil {
			return nil, err
		}
		out = append(out, r)
	}
	return out, rows.Err()
}
