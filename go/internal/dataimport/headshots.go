package dataimport

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"net/http"
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

type nbaWikidataPlayer struct {
	Name  string
	NBAID string
}

const wikidataSPARQLURL = "https://query.wikidata.org/sparql"

// wikidataNBAPlayers obtains the NBA's own P3647 player identifier in one
// batch. The query never searches by a house name: matching happens locally
// under the unique-name gate below, so namesakes cannot inherit one another's
// image.
func wikidataNBAPlayers(ctx context.Context) ([]nbaWikidataPlayer, error) {
	query := `SELECT ?name ?nba_id WHERE {
  ?player wdt:P3647 ?nba_id ; rdfs:label ?name .
  FILTER(LANG(?name) = "en")
}`
	u, err := url.Parse(wikidataSPARQLURL)
	if err != nil {
		return nil, err
	}
	q := u.Query()
	q.Set("query", query)
	q.Set("format", "json")
	u.RawQuery = q.Encode()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u.String(), nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Accept", "application/sparql-results+json")
	req.Header.Set("User-Agent", userAgent)
	resp, err := httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("GET Wikidata NBA ids: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("GET Wikidata NBA ids: status %d", resp.StatusCode)
	}
	var body struct {
		Results struct {
			Bindings []struct {
				Name struct {
					Value string `json:"value"`
				} `json:"name"`
				NBAID struct {
					Value string `json:"value"`
				} `json:"nba_id"`
			} `json:"bindings"`
		} `json:"results"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		return nil, fmt.Errorf("decode Wikidata NBA ids: %w", err)
	}
	players := make([]nbaWikidataPlayer, 0, len(body.Results.Bindings))
	for _, row := range body.Results.Bindings {
		id := strings.TrimSpace(row.NBAID.Value)
		name := strings.TrimSpace(row.Name.Value)
		if name == "" || id == "" || strings.Trim(id, "0123456789") != "" {
			continue
		}
		players = append(players, nbaWikidataPlayer{Name: name, NBAID: id})
	}
	return players, nil
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

// BackfillNBAHeadshots first refreshes rows with an existing NBA id, then fills
// the remaining gap from Wikidata's NBA.com-id property. A name is accepted
// only when it is unique on both sides; all uncertainty stays blank for the
// evidence-gated investigator rather than risking the wrong face.
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

	source, err := wikidataNBAPlayers(ctx)
	if err != nil {
		return f, err
	}
	f.SourceRows = len(source)

	type housePlayer struct {
		id int
	}
	house := map[string][]housePlayer{}
	rows, err := pool.Query(ctx, `SELECT id, name FROM players WHERE sport = 'NBA'`)
	if err != nil {
		return f, fmt.Errorf("load NBA players: %w", err)
	}
	for rows.Next() {
		var p housePlayer
		var name string
		if err := rows.Scan(&p.id, &name); err != nil {
			rows.Close()
			return f, err
		}
		house[normName(name)] = append(house[normName(name)], p)
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return f, err
	}

	byName := map[string][]nbaWikidataPlayer{}
	for _, p := range source {
		byName[normName(p.Name)] = append(byName[normName(p.Name)], p)
	}
	owners := map[string]int{}
	rows, err = pool.Query(ctx, `
		SELECT external_id, entity_id FROM entity_external_ids
		WHERE entity_type = 'player' AND sport = 'NBA' AND namespace = 'nba'`)
	if err != nil {
		return f, fmt.Errorf("load NBA id bindings: %w", err)
	}
	for rows.Next() {
		var id string
		var owner int
		if err := rows.Scan(&id, &owner); err != nil {
			rows.Close()
			return f, err
		}
		owners[id] = owner
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return f, err
	}

	for name, candidates := range byName {
		players := house[name]
		if len(candidates) != 1 || len(players) != 1 {
			f.Unbound++
			continue
		}
		playerID, nbaID := players[0].id, candidates[0].NBAID
		if owner, exists := owners[nbaID]; exists && owner != playerID {
			f.Unbound++
			continue
		}
		if _, err := pool.Exec(ctx, upsertIdentitySQL, "player", playerID, "NBA", "nba", nbaID); err != nil {
			return f, fmt.Errorf("bind NBA id %s to player %d: %w", nbaID, playerID, err)
		}
		changed, err := setPlayerHeadshot(ctx, pool, playerID,
			"https://cdn.nba.com/headshots/nba/latest/1040x760/"+nbaID+".png")
		if err != nil {
			return f, fmt.Errorf("update NBA player %d: %w", playerID, err)
		}
		if changed {
			f.Updated++
		}
	}
	return f, nil
}
