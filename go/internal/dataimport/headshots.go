package dataimport

import (
	"context"
	"fmt"
	"log/slog"
	"net/url"
	"strings"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// HeadshotFunnel makes the one-off repair observable. A source row without a
// bound house player is counted instead of silently treated as coverage.
type HeadshotFunnel struct {
	SourceRows int
	Updated    int
	Cleared    int
	Unbound    int
}

// validHeadshotURL refuses malformed and non-web values from an upstream CSV.
// The client owns fetching the image, so a bad URL should become a skipped row,
// never a broken image persisted to every card.
func validHeadshotURL(raw string) string {
	s := strings.TrimSpace(raw)
	u, err := url.Parse(s)
	if err != nil || (u.Scheme != "https" && u.Scheme != "http") || u.Host == "" {
		return ""
	}
	return s
}

// setPlayerHeadshot overwrites a provider-managed player image. This is
// deliberately unlike investigator enrichment's COALESCE: a previous generic
// search result is not evidence once a league data source identifies the player.
func setPlayerHeadshot(ctx context.Context, q querier, playerID int, photo string) (bool, error) {
	tag, err := q.Exec(ctx, `
		UPDATE players
		SET photo_url = $2, updated_at = NOW()
		WHERE id = $1 AND photo_url IS DISTINCT FROM $2`, playerID, photo)
	if err != nil {
		return false, err
	}
	return tag.RowsAffected() > 0, nil
}

// BackfillNFLHeadshots copies the image URLs emitted by nflverse player stats.
// The player_id is the same GSIS key bound by the normal NFL importer. Seasons
// are read from the house, so a full repair covers exactly its imported history.
func BackfillNFLHeadshots(ctx context.Context, pool *pgxpool.Pool, seasonOverride int, logger *slog.Logger) (HeadshotFunnel, error) {
	var f HeadshotFunnel
	// An NBA CDN path cannot belong to an NFL player. Clear these legacy
	// generic-search mistakes even when the player is too old to have a bound
	// nflverse GSIS id; the UI's fallback is preferable to the wrong person.
	tag, err := pool.Exec(ctx, `
		UPDATE players SET photo_url = NULL, updated_at = NOW()
		WHERE sport = 'NFL'
		  AND photo_url LIKE 'https://cdn.nba.com/headshots/nba/%'`)
	if err != nil {
		return f, fmt.Errorf("clear cross-sport NFL headshots: %w", err)
	}
	f.Cleared = int(tag.RowsAffected())
	seasons := []int{}
	if seasonOverride != 0 {
		seasons = append(seasons, seasonOverride)
	} else {
		rows, err := pool.Query(ctx, `SELECT DISTINCT season FROM player_stats WHERE sport = 'NFL' ORDER BY season`)
		if err != nil {
			return f, fmt.Errorf("list NFL player-stat seasons: %w", err)
		}
		defer rows.Close()
		for rows.Next() {
			var season int
			if err := rows.Scan(&season); err != nil {
				return f, err
			}
			seasons = append(seasons, season)
		}
		if err := rows.Err(); err != nil {
			return f, err
		}
	}

	for _, season := range seasons {
		t, err := fetchCSV(ctx, fmt.Sprintf(nflPlayerWeekURL, season))
		if err != nil {
			if err == errNotPublished {
				continue
			}
			return f, fmt.Errorf("fetch NFL player stats %d: %w", season, err)
		}
		if !t.Has("headshot_url") {
			return f, fmt.Errorf("NFL player stats %d has no headshot_url column", season)
		}
		// One weekly CSV contains the same player on every appearance. Retain
		// their latest supplied URL and issue one database write per provider ID.
		photos := make(map[string]string)
		for i := 0; i < t.Len(); i++ {
			photo := validHeadshotURL(t.Get(i, "headshot_url"))
			gsis := strings.TrimSpace(t.Get(i, "player_id"))
			if gsis == "" || photo == "" {
				continue
			}
			photos[gsis] = photo
		}
		for gsis, photo := range photos {
			f.SourceRows++
			var playerID int
			err := pool.QueryRow(ctx, `
				SELECT entity_id FROM entity_external_ids
				WHERE namespace = 'nflverse' AND entity_type = 'player'
				  AND sport = 'NFL' AND external_id = $1`, gsis).Scan(&playerID)
			if err == pgx.ErrNoRows {
				f.Unbound++
				continue
			}
			if err != nil {
				return f, fmt.Errorf("look up NFL player %s: %w", gsis, err)
			}
			changed, err := setPlayerHeadshot(ctx, pool, playerID, photo)
			if err != nil {
				return f, fmt.Errorf("update NFL player %d: %w", playerID, err)
			}
			if changed {
				f.Updated++
			}
		}
		logger.Info("dataimport: NFL headshot season complete", "season", season, "updated", f.Updated, "unbound", f.Unbound)
	}
	return f, nil
}

// BackfillNBAHeadshots derives each image from the verified NBA player ID.
// No name lookup occurs, so an NBA image can never be assigned to an NFL row.
func BackfillNBAHeadshots(ctx context.Context, pool *pgxpool.Pool) (HeadshotFunnel, error) {
	var f HeadshotFunnel
	tag, err := pool.Exec(ctx, `
		UPDATE players p
		SET photo_url = 'https://cdn.nba.com/headshots/nba/latest/1040x760/' || x.external_id || '.png',
		    updated_at = NOW()
		FROM entity_external_ids x
		WHERE x.entity_type = 'player' AND x.sport = 'NBA' AND x.namespace = 'nba'
		  AND x.entity_id = p.id AND p.sport = 'NBA'
		  AND x.external_id ~ '^[0-9]+$'
		  AND p.photo_url IS DISTINCT FROM
		      'https://cdn.nba.com/headshots/nba/latest/1040x760/' || x.external_id || '.png'`)
	if err != nil {
		return f, fmt.Errorf("backfill NBA headshots: %w", err)
	}
	f.Updated = int(tag.RowsAffected())
	return f, nil
}
