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
// (latest per counterparty, heat > 0, Gemma-vetted), naming the counterparty.
// Branches on entity type: a team's player rumors vs a player's suitor clubs.
// Sport-agnostic (the same rows back NBA/NFL trades and Football transfers).
//
// The is_rumor IS TRUE gate is applied AFTER picking the latest row per
// counterparty (FIRST-GPT-AUDIT Session 10) so a newer cleared/unknown verdict
// supersedes an older TRUE: a model failure (NULL) or a Gemma-cleared (FALSE) row
// stops grounding narratives/Vibe, mirroring the /transfers read contract.
//
// Freshness gate (L9): only rows regenerated within the last 14 days are
// considered, so a rumor the re-vet has stopped re-confirming ages out instead
// of grounding prompts forever. The re-vet (cmd/transfer -mode corpus) only
// refreshes ACTIVE candidates (>=2 co-mentions/14d), so without this gate a very
// old false positive — e.g. a pre-rivalry-clause heat-v1 row — serves
// indefinitely (Wembanyama still showed a stale Milwaukee Bucks heat 6 dated
// 06-02). Applied in the inner subquery before DISTINCT ON, matching the 14-day
// window the /transfers card read path uses (db.go entity_transfers /
// transfers_leaderboard) so the prompt grounds on the same set the card shows.
func loadTransferHeat(
	ctx context.Context, pool *pgxpool.Pool, entityType string, entityID int, sport string,
) ([]heatItem, error) {
	q := `
		SELECT counterparty, heat, stage, direction FROM (
		    SELECT DISTINCT ON (tr.team_id)
		           t.name AS counterparty, tr.heat, tr.is_rumor,
		           COALESCE(tr.stage,'') AS stage, COALESCE(tr.direction,'') AS direction,
		           tr.generated_at
		    FROM transfer_rumors tr
		    JOIN teams t ON t.id = tr.team_id AND t.sport = tr.sport
		    WHERE tr.player_id = $1 AND tr.sport = $2 AND tr.heat IS NOT NULL
		      AND tr.generated_at > NOW() - INTERVAL '14 days'
		    ORDER BY tr.team_id, tr.generated_at DESC
		) latest
		WHERE heat > 0 AND is_rumor IS TRUE ORDER BY heat DESC LIMIT $3`
	if entityType == "team" {
		q = `
		SELECT counterparty, heat, stage, direction FROM (
		    SELECT DISTINCT ON (tr.player_id)
		           p.name AS counterparty, tr.heat, tr.is_rumor,
		           COALESCE(tr.stage,'') AS stage, COALESCE(tr.direction,'') AS direction,
		           tr.generated_at
		    FROM transfer_rumors tr
		    JOIN players p ON p.id = tr.player_id AND p.sport = tr.sport
		    WHERE tr.team_id = $1 AND tr.sport = $2 AND tr.heat IS NOT NULL
		      AND tr.generated_at > NOW() - INTERVAL '14 days'
		    ORDER BY tr.player_id, tr.generated_at DESC
		) latest
		WHERE heat > 0 AND is_rumor IS TRUE ORDER BY heat DESC LIMIT $3`
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
