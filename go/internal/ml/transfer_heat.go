package ml

// Shared transfer-heat primitive: the entity's hottest active transfer/trade
// rumors (latest per counterparty, heat > 0), plus the prompt-line formatter.
// Used by BOTH the vibe stage (weighs heat alongside narratives) and the
// narratives stage (grounds transfer storylines in these vetted facts — Task 12),
// so the two stages read + render the transfer signal identically.

import (
	"context"
	"fmt"
	"strings"

	"github.com/jackc/pgx/v5/pgxpool"
)

// maxHeatItems bounds the transfer rumors shown to a Gemma stage as the entity's
// "transfer temperature".
const maxHeatItems = 6

type heatItem struct {
	counterparty string
	heat         int
	stage        string
	direction    string
}

// loadTransferHeat returns the entity's hottest active transfer/trade rumors
// (latest per counterparty, heat > 0), naming the counterparty. Branches on
// entity type: a team's player rumors vs a player's suitor clubs. Sport-agnostic
// (the same rows back NBA/NFL trades and Football transfers).
func loadTransferHeat(
	ctx context.Context, pool *pgxpool.Pool, entityType string, entityID int, sport string,
) ([]heatItem, error) {
	q := `
		SELECT counterparty, heat, stage, direction FROM (
		    SELECT DISTINCT ON (tr.team_id)
		           t.name AS counterparty, tr.heat,
		           COALESCE(tr.stage,'') AS stage, COALESCE(tr.direction,'') AS direction,
		           tr.generated_at
		    FROM transfer_rumors tr
		    JOIN teams t ON t.id = tr.team_id AND t.sport = tr.sport
		    WHERE tr.player_id = $1 AND tr.sport = $2 AND tr.heat IS NOT NULL
		    ORDER BY tr.team_id, tr.generated_at DESC
		) latest
		WHERE heat > 0 ORDER BY heat DESC LIMIT $3`
	if entityType == "team" {
		q = `
		SELECT counterparty, heat, stage, direction FROM (
		    SELECT DISTINCT ON (tr.player_id)
		           p.name AS counterparty, tr.heat,
		           COALESCE(tr.stage,'') AS stage, COALESCE(tr.direction,'') AS direction,
		           tr.generated_at
		    FROM transfer_rumors tr
		    JOIN players p ON p.id = tr.player_id AND p.sport = tr.sport
		    WHERE tr.team_id = $1 AND tr.sport = $2 AND tr.heat IS NOT NULL
		    ORDER BY tr.player_id, tr.generated_at DESC
		) latest
		WHERE heat > 0 ORDER BY heat DESC LIMIT $3`
	}
	rows, err := pool.Query(ctx, q, entityID, sport, maxHeatItems)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []heatItem
	for rows.Next() {
		var h heatItem
		if err := rows.Scan(&h.counterparty, &h.heat, &h.stage, &h.direction); err != nil {
			return nil, err
		}
		out = append(out, h)
	}
	return out, rows.Err()
}

// writeHeatLines renders the heat items as prompt bullets — one shared format so
// the vibe and narratives stages present transfer facts identically:
//
//   - <counterparty> — heat <heat>, <direction>, <stage>
func writeHeatLines(b *strings.Builder, heat []heatItem) {
	for _, h := range heat {
		line := fmt.Sprintf("- %s — heat %d", h.counterparty, h.heat)
		if h.direction != "" {
			line += ", " + h.direction
		}
		if h.stage != "" {
			line += ", " + h.stage
		}
		b.WriteString(line + "\n")
	}
}
