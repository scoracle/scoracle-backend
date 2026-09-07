// FPL import — the football half of the data sweep (the FPL remap,
// PLAN-weekly-fantasy-rail; built 2026-09-06 when the season's first PL
// weekend completed with zero stat rows).
//
// Source: the Fantasy Premier League API (fantasy.premierleague.com) — free
// JSON, Premier League only. Same shape as the nflverse arm: resolve
// identities through entity_external_ids (namespace 'fpl' — mig 233's
// conflict predicate already lists it), find completed fixtures missing event
// stats, fetch, promote through finalize_fixture(), recompute touched
// seasons, roll current_season forward.
//
// Differences from the NFL arm, each deliberate:
//   - Fixtures are MATCHED, never created: the fixture feed already carries
//     the PL schedule; FPL binds onto it by (home, away, kickoff window).
//   - Players are MATCHED, never created: every PL player exists from the
//     vendor-era seed, so a miss here is a name-normalization event for the
//     funnel, not a new row (creation would mint duplicates against the
//     existing ids).
//   - Stats are written under the EXISTING football vocabulary
//     (stat_definitions key_name), so ratings stay comparable across the
//     vendor era and the FPL era. FPL-only stats with no house key are
//     dropped, named in the map below rather than silently.
//   - Per-fixture player stats come from the live endpoint's `explain`
//     blocks, which split a gameweek's totals by fixture — correct in double
//     gameweeks where the flat stats are GW-wide.
package dataimport

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"strconv"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

const (
	fplBootstrapURL = "https://fantasy.premierleague.com/api/bootstrap-static/"
	fplFixturesURL  = "https://fantasy.premierleague.com/api/fixtures/"
	fplLiveURLFmt   = "https://fantasy.premierleague.com/api/event/%d/live/"
	fplLeagueID     = 8 // Premier League in the house teams/fixtures tables.
)

// FPL team names diverge from the house full names in a small, stable set;
// the alias map covers the known shapes and the resolver's search_aliases
// cover the rest. An unmatched team is a funnel event, never a guess.
var fplTeamAliases = map[string]string{
	"Man City":      "Manchester City",
	"Man Utd":       "Manchester United",
	"Spurs":         "Tottenham Hotspur",
	"Nott'm Forest": "Nottingham Forest",
	"Wolves":        "Wolverhampton Wanderers",
	"West Brom":     "West Bromwich Albion",
	"Sheffield Utd": "Sheffield United",
	"Bournemouth":   "AFC Bournemouth",
	"Brighton":      "Brighton & Hove Albion",
	"West Ham":      "West Ham United",
	"Newcastle":     "Newcastle United",
	"Leeds":         "Leeds United",
	"Norwich":       "Norwich City",
	"Leicester":     "Leicester City",
	"Ipswich":       "Ipswich Town",
	"Luton":         "Luton Town",
}

// explain identifiers → house stat_definitions key_name (FOOTBALL, player
// grain, non-derived). Identifiers absent here are DROPPED BY NAME:
// clean_sheets (no player-grain house key), bonus/bps (fantasy bookkeeping),
// defensive_contribution / clearances_blocks_interceptions (composites the
// house tracks as separate keys it cannot recover from the sum).
var fplStatKey = map[string]string{
	"minutes":          "minutes_played",
	"goals_scored":     "goals",
	"assists":          "assists",
	"goals_conceded":   "goals_conceded",
	"own_goals":        "own_goals",
	"penalties_saved":  "penalties_saved",
	"penalties_missed": "penalties_missed",
	"yellow_cards":     "yellow_cards",
	"red_cards":        "red_cards",
	"saves":            "saves",
	"tackles":          "tackles",
	"recoveries":       "ball_recovery",
	"starts":           "lineups",
}

var fplPositions = map[int]string{1: "Goalkeeper", 2: "Defender", 3: "Midfielder", 4: "Forward"}

type fplTeam struct {
	ID        int    `json:"id"`
	Name      string `json:"name"`
	ShortName string `json:"short_name"`
}

type fplElement struct {
	ID         int    `json:"id"`
	FirstName  string `json:"first_name"`
	SecondName string `json:"second_name"`
	WebName    string `json:"web_name"`
	Team       int    `json:"team"`
	Type       int    `json:"element_type"`
}

type fplBootstrap struct {
	Teams    []fplTeam    `json:"teams"`
	Elements []fplElement `json:"elements"`
}

type fplFixture struct {
	ID          int     `json:"id"`
	Event       *int    `json:"event"`
	TeamH       int     `json:"team_h"`
	TeamA       int     `json:"team_a"`
	TeamHScore  *int    `json:"team_h_score"`
	TeamAScore  *int    `json:"team_a_score"`
	KickoffTime *string `json:"kickoff_time"`
	Finished    bool    `json:"finished"`
}

type fplExplainStat struct {
	Identifier string `json:"identifier"`
	Value      int    `json:"value"`
}

type fplLiveElement struct {
	ID      int `json:"id"`
	Explain []struct {
		Fixture int              `json:"fixture"`
		Stats   []fplExplainStat `json:"stats"`
	} `json:"explain"`
}

type fplLive struct {
	Elements []fplLiveElement `json:"elements"`
}

func fetchFPLJSON(ctx context.Context, url string, out any) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return err
	}
	req.Header.Set("User-Agent", userAgent)
	resp, err := httpClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("GET %s: %s", url, resp.Status)
	}
	body, err := io.ReadAll(io.LimitReader(resp.Body, 32<<20))
	if err != nil {
		return err
	}
	return json.Unmarshal(body, out)
}

// RunFPL is the football data sweep. Mirrors RunNFL's funnel discipline: every
// drop is counted, a single bad fixture never blocks the batch, and the season
// recompute + current_season roll run once at the end.
func RunFPL(ctx context.Context, pool *pgxpool.Pool, logger *slog.Logger) (Funnel, error) {
	var f Funnel
	logger.Info("dataimport: fpl run starting")

	res, err := NewResolver(ctx, pool, "fpl", "FOOTBALL")
	if err != nil {
		return f, err
	}

	var boot fplBootstrap
	if err := fetchFPLJSON(ctx, fplBootstrapURL, &boot); err != nil {
		return f, fmt.Errorf("bootstrap: %w", err)
	}

	// Teams: resolve every FPL club once; the run cannot meaningfully continue
	// past a club that will not resolve (its fixtures would all mismatch), so
	// unmatched clubs are counted and their fixtures skipped.
	teamByFPL := map[int]int{}
	for _, t := range boot.Teams {
		id, err := resolveFPLTeam(ctx, pool, res, t)
		if err != nil {
			f.TeamsUnmatched++
			logger.Warn("dataimport: fpl team unmatched", "fpl_team", t.Name, "error", err)
			continue
		}
		teamByFPL[t.ID] = id
	}

	elementByID := map[int]fplElement{}
	for _, e := range boot.Elements {
		elementByID[e.ID] = e
	}

	var fixtures []fplFixture
	if err := fetchFPLJSON(ctx, fplFixturesURL, &fixtures); err != nil {
		return f, fmt.Errorf("fixtures: %w", err)
	}

	// The gap set: finished FPL fixtures whose HOUSE fixture is completed and
	// still has no event_team_stats. Grouped by gameweek so each live payload
	// is fetched once.
	type gap struct {
		fpl       fplFixture
		fixtureID int
		season    int
		homeID    int
		awayID    int
	}
	byEvent := map[int][]gap{}
	for _, fx := range fixtures {
		if !fx.Finished || fx.Event == nil || fx.TeamHScore == nil || fx.TeamAScore == nil {
			continue
		}
		homeID, okH := teamByFPL[fx.TeamH]
		awayID, okA := teamByFPL[fx.TeamA]
		if !okH || !okA {
			continue
		}
		fixtureID, season, err := resolveFPLFixture(ctx, pool, res, fx, homeID, awayID)
		if err != nil || fixtureID == 0 {
			continue // not in the house schedule (or not yet); next run retries.
		}
		var have int
		if err := pool.QueryRow(ctx,
			`SELECT COUNT(*) FROM event_team_stats WHERE fixture_id = $1`, fixtureID).Scan(&have); err != nil {
			return f, err
		}
		if have > 0 {
			continue
		}
		f.Gaps++
		byEvent[*fx.Event] = append(byEvent[*fx.Event], gap{fpl: fx, fixtureID: fixtureID, season: season, homeID: homeID, awayID: awayID})
	}
	if f.Gaps == 0 {
		logger.Info("dataimport: fpl run complete", f.LogAttrs()...)
		return f, nil
	}

	touched := map[int]bool{}
	for event, group := range byEvent {
		var live fplLive
		if err := fetchFPLJSON(ctx, fmt.Sprintf(fplLiveURLFmt, event), &live); err != nil {
			f.GapsWaiting += len(group)
			logger.Warn("dataimport: fpl live fetch failed", "event", event, "error", err)
			continue
		}
		// Per-fixture player stat lines out of the explain blocks.
		perFixture := map[int][]playerLine{}
		for _, el := range live.Elements {
			meta, ok := elementByID[el.ID]
			if !ok {
				continue
			}
			for _, ex := range el.Explain {
				stats := map[string]float64{}
				for _, s := range ex.Stats {
					if key, ok := fplStatKey[s.Identifier]; ok && s.Value != 0 {
						stats[key] += float64(s.Value)
					}
				}
				if stats["minutes_played"] <= 0 {
					continue
				}
				stats["appearances"] = 1
				perFixture[ex.Fixture] = append(perFixture[ex.Fixture], playerLine{el: meta, stats: stats})
			}
		}
		for _, g := range group {
			lines := perFixture[g.fpl.ID]
			if len(lines) == 0 {
				f.GapsWaiting++
				continue
			}
			if err := promoteFPLFixture(ctx, pool, res, g.fixtureID, g.season, g.homeID, g.awayID,
				*g.fpl.TeamHScore, *g.fpl.TeamAScore, teamByFPL, lines, &f, logger); err != nil {
				f.GapsFailed++
				logger.Warn("dataimport: fpl fixture promotion failed",
					"fpl_fixture", g.fpl.ID, "fixture", g.fixtureID, "error", err)
				continue
			}
			f.GapsFilled++
			touched[g.season] = true
		}
	}

	for season := range touched {
		logger.Info("dataimport: recompute_season", "sport", "FOOTBALL", "season", season)
		if _, err := pool.Exec(ctx, `SELECT recompute_season('FOOTBALL', $1)`, season); err != nil {
			return f, fmt.Errorf("recompute_season FOOTBALL %d: %w", season, err)
		}
		if _, err := pool.Exec(ctx, `
			UPDATE sports SET current_season = $1
			WHERE id = 'FOOTBALL' AND current_season < $1`, season); err != nil {
			return f, fmt.Errorf("roll current_season: %w", err)
		}
	}

	logger.Info("dataimport: fpl run complete", f.LogAttrs()...)
	return f, nil
}

type playerLine struct {
	el    fplElement
	stats map[string]float64
}

func resolveFPLTeam(ctx context.Context, pool *pgxpool.Pool, res *Resolver, t fplTeam) (int, error) {
	ext := strconv.Itoa(t.ID)
	if id, err := res.Team(ctx, pool, ext); err == nil {
		return id, nil
	}
	// First encounter: resolve by name shapes, then bind the numeric id so
	// every later run is a map hit. league_id guards against same-named clubs
	// in other leagues.
	for _, name := range []string{t.Name, fplTeamAliases[t.Name], t.ShortName} {
		if name == "" {
			continue
		}
		var id int
		err := pool.QueryRow(ctx, `
			SELECT id FROM teams
			WHERE sport = 'FOOTBALL' AND league_id = $2
			  AND (name = $1 OR $1 = ANY(COALESCE(search_aliases, '{}')))`,
			name, fplLeagueID).Scan(&id)
		if err == pgx.ErrNoRows {
			continue
		}
		if err != nil {
			return 0, err
		}
		if err := res.bind(ctx, pool, "team", ext, id); err != nil {
			return 0, err
		}
		res.teams[ext] = id
		return id, nil
	}
	return 0, fmt.Errorf("no house club for %q/%q", t.Name, t.ShortName)
}

// resolveFPLFixture binds an FPL fixture onto the house schedule by (home,
// away, kickoff ±10 days). League play has one pairing per venue per season,
// so the window is a sanity guard, not a discriminator.
func resolveFPLFixture(ctx context.Context, pool *pgxpool.Pool, res *Resolver, fx fplFixture, homeID, awayID int) (int, int, error) {
	ext := strconv.Itoa(fx.ID)
	if id := res.Fixture(ext); id != 0 {
		var season int
		if err := pool.QueryRow(ctx, `SELECT season FROM fixtures WHERE id = $1`, id).Scan(&season); err != nil {
			return 0, 0, err
		}
		return id, season, nil
	}
	if fx.KickoffTime == nil {
		return 0, 0, nil
	}
	kickoff, err := time.Parse(time.RFC3339, *fx.KickoffTime)
	if err != nil {
		return 0, 0, nil
	}
	var id, season int
	err = pool.QueryRow(ctx, `
		SELECT id, season FROM fixtures
		WHERE sport = 'FOOTBALL' AND league_id = $1
		  AND home_team_id = $2 AND away_team_id = $3
		  AND start_time BETWEEN $4::timestamptz - INTERVAL '10 days'
		                     AND $4::timestamptz + INTERVAL '10 days'`,
		fplLeagueID, homeID, awayID, kickoff).Scan(&id, &season)
	if err == pgx.ErrNoRows {
		return 0, 0, nil
	}
	if err != nil {
		return 0, 0, err
	}
	if err := res.BindFixture(ctx, pool, ext, id); err != nil {
		return 0, 0, err
	}
	return id, season, nil
}

// promoteFPLFixture is the promotion transaction, mirroring promoteNFLFixture:
// event_team_stats + event_box_scores + finalize_fixture, atomically. Team
// stats are derived — the per-fixture score plus sums of the mapped player
// lines plus the result bookkeeping the season aggregates key on.
func promoteFPLFixture(ctx context.Context, pool *pgxpool.Pool, res *Resolver,
	fixtureID, season, homeID, awayID, homeScore, awayScore int,
	teamByFPL map[int]int, lines []playerLine, f *Funnel, logger *slog.Logger) error {

	tx, err := pool.Begin(ctx)
	if err != nil {
		return err
	}
	defer tx.Rollback(ctx)

	teamSums := map[int]map[string]float64{
		homeID: {},
		awayID: {},
	}

	for _, ln := range lines {
		teamID, ok := teamByFPL[ln.el.Team]
		if !ok || (teamID != homeID && teamID != awayID) {
			// A mid-window transfer can leave an element pointing at a third
			// club; the fixture sides are the truth, so the line is dropped.
			f.PlayersUnmatched++
			continue
		}
		name := ln.el.FirstName + " " + ln.el.SecondName
		pid, err := matchFPLPlayer(ctx, tx, res, ln.el, name, teamID)
		if err != nil {
			f.PlayersUnmatched++
			logger.Warn("dataimport: fpl player unmatched", "player", name, "web", ln.el.WebName, "error", err)
			continue
		}
		for k, v := range ln.stats {
			teamSums[teamID][k] += v
		}
		stats, err := json.Marshal(ln.stats)
		if err != nil {
			return err
		}
		if _, err := tx.Exec(ctx, `
			INSERT INTO event_box_scores (fixture_id, player_id, team_id, sport, season, league_id, stats, position)
			VALUES ($1, $2, $3, 'FOOTBALL', $4, $5, $6::jsonb, NULLIF($7, ''))
			ON CONFLICT (fixture_id, player_id) DO UPDATE SET
				team_id = EXCLUDED.team_id, stats = EXCLUDED.stats, position = EXCLUDED.position`,
			fixtureID, pid, teamID, season, fplLeagueID, string(stats), fplPositions[ln.el.Type]); err != nil {
			return fmt.Errorf("event_box_scores player %d: %w", pid, err)
		}
		f.EventPlayers++
	}

	for teamID, sums := range teamSums {
		goalsFor, goalsAgainst := homeScore, awayScore
		if teamID == awayID {
			goalsFor, goalsAgainst = awayScore, homeScore
		}
		ts := map[string]float64{
			"goals_for":      float64(goalsFor),
			"goals_against":  float64(goalsAgainst),
			"matches_played": 1,
		}
		// The season table's result bookkeeping sums off per-event units.
		switch {
		case goalsFor > goalsAgainst:
			ts["wins"], ts["points"] = 1, 3
		case goalsFor < goalsAgainst:
			ts["losses"] = 1
		default:
			ts["draws"], ts["points"] = 1, 1
		}
		if teamID == homeID {
			ts["home_played"] = 1
			ts["home_scored"], ts["home_conceded"] = float64(goalsFor), float64(goalsAgainst)
			switch {
			case goalsFor > goalsAgainst:
				ts["home_won"], ts["home_points"] = 1, 3
			case goalsFor < goalsAgainst:
				ts["home_lost"] = 1
			default:
				ts["home_draw"], ts["home_points"] = 1, 1
			}
		} else {
			ts["away_played"] = 1
			ts["away_scored"], ts["away_conceded"] = float64(goalsFor), float64(goalsAgainst)
			switch {
			case goalsFor > goalsAgainst:
				ts["away_won"], ts["away_points"] = 1, 3
			case goalsFor < goalsAgainst:
				ts["away_lost"] = 1
			default:
				ts["away_draw"], ts["away_points"] = 1, 1
			}
		}
		ts["goal_difference"] = float64(goalsFor - goalsAgainst)
		for src, dst := range map[string]string{
			"yellow_cards": "yellow_cards_total", "red_cards": "red_cards_total",
			"saves": "saves", "tackles": "tackles", "ball_recovery": "ball_recovery",
			"assists": "assists",
		} {
			if v := sums[src]; v != 0 {
				ts[dst] = v
			}
		}
		stats, err := json.Marshal(ts)
		if err != nil {
			return err
		}
		if _, err := tx.Exec(ctx, `
			INSERT INTO event_team_stats (fixture_id, team_id, sport, season, league_id, score, stats)
			VALUES ($1, $2, 'FOOTBALL', $3, $4, $5, $6::jsonb)
			ON CONFLICT (fixture_id, team_id) DO UPDATE SET
				score = EXCLUDED.score, stats = EXCLUDED.stats`,
			fixtureID, teamID, season, fplLeagueID, goalsFor, string(stats)); err != nil {
			return fmt.Errorf("event_team_stats team %d: %w", teamID, err)
		}
		f.EventTeams++
	}

	if _, err := tx.Exec(ctx, `SELECT finalize_fixture($1, false)`, fixtureID); err != nil {
		return fmt.Errorf("finalize_fixture(%d): %w", fixtureID, err)
	}
	return tx.Commit(ctx)
}

// matchFPLPlayer resolves an element to a house player WITHOUT creating: every
// PL player exists from the seed, so a miss is a normalization event for the
// funnel. Match by normalized full name, narrowed by team; web_name is the
// fallback surface (the form many sources carry).
func matchFPLPlayer(ctx context.Context, q querier, res *Resolver, el fplElement, fullName string, teamID int) (int, error) {
	ext := strconv.Itoa(el.ID)
	if id, ok := res.players[ext]; ok {
		return id, nil
	}
	nameExpr := fmt.Sprintf(normNameSQL, "name")
	for _, candidate := range []string{fullName, el.WebName} {
		norm := normName(candidate)
		if norm == "" {
			continue
		}
		rows, err := q.Query(ctx, `
			SELECT id, COALESCE(team_id, 0) FROM players
			WHERE sport = 'FOOTBALL' AND (`+nameExpr+` = $1 OR $2 = ANY(COALESCE(search_aliases, '{}')))`,
			norm, candidate)
		if err != nil {
			return 0, err
		}
		type cand struct{ id, teamID int }
		var cands []cand
		for rows.Next() {
			var c cand
			if err := rows.Scan(&c.id, &c.teamID); err != nil {
				rows.Close()
				return 0, err
			}
			cands = append(cands, c)
		}
		rows.Close()
		if err := rows.Err(); err != nil {
			return 0, err
		}
		var id int
		switch {
		case len(cands) == 1:
			id = cands[0].id
		case len(cands) > 1:
			matched := 0
			for _, c := range cands {
				if c.teamID == teamID {
					id, matched = c.id, matched+1
				}
			}
			if matched != 1 {
				continue
			}
		default:
			continue
		}
		if err := res.bind(ctx, q, "player", ext, id); err != nil {
			return 0, err
		}
		res.players[ext] = id
		return id, nil
	}
	return 0, fmt.Errorf("no house player for %q (%s)", fullName, el.WebName)
}
