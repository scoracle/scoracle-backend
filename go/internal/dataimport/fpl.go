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

	// Teams: resolve every FPL club once. A club the house has never seen is
	// CREATED, not skipped (Scott, 2026-09-06, the Coventry/Hull debug: "when
	// the newly promoted teams get read, they should be added by automation,
	// and then stats should flow"). The healthy gate is the source itself —
	// the league's own bootstrap is the authoritative membership list, a
	// different evidence class from a name in an article. Creation closes the
	// growth loop backward too: the club's name surfaces register so the
	// Editor resolves future mentions, and its rejected news candidates (the
	// census wall — "coventry city" sat at 105 mentions, "hull city" at 202)
	// resolve onto the new row.
	teamByFPL := map[int]int{}
	for _, t := range boot.Teams {
		id, err := resolveFPLTeam(ctx, pool, res, t)
		if err != nil {
			id, err = createFPLTeam(ctx, pool, res, t, logger)
			if err != nil {
				f.TeamsUnmatched++
				logger.Warn("dataimport: fpl team unmatched and uncreatable", "fpl_team", t.Name, "error", err)
				continue
			}
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

// createFPLTeam mints the club the league feed presented and the house lacks
// (teams_id_seq, mig 243), then closes the self-growing loop backward:
// name surfaces register in entity_name_surfaces (the Editor resolves future
// mentions and the claim fence routes the club's articles), and any news
// candidates the census walled off resolve onto the new row. Everything in
// one transaction — a club either fully joins the graph or does not exist.
func createFPLTeam(ctx context.Context, pool *pgxpool.Pool, res *Resolver, t fplTeam, logger *slog.Logger) (int, error) {
	tx, err := pool.Begin(ctx)
	if err != nil {
		return 0, err
	}
	defer tx.Rollback(ctx)

	fullName := t.Name
	if alias, ok := fplTeamAliases[t.Name]; ok && alias != "" {
		fullName = alias
	}
	var id int
	if err := tx.QueryRow(ctx, `
		INSERT INTO teams (sport, name, short_code, league_id, search_aliases, meta)
		VALUES ('FOOTBALL', $1, $2, $3, $4,
		        jsonb_build_object('created_by', 'dataimport-fpl'))
		RETURNING id`,
		fullName, t.ShortName, fplLeagueID,
		[]string{t.Name, t.ShortName}).Scan(&id); err != nil {
		return 0, fmt.Errorf("create club %q: %w", fullName, err)
	}

	// Surfaces: the Editor's resolver and the claim fence key on these.
	for _, surface := range []string{fullName, t.Name, t.ShortName} {
		if surface == "" {
			continue
		}
		if _, err := tx.Exec(ctx, `
			INSERT INTO entity_name_surfaces (entity_type, entity_id, sport, norm, surface_kind)
			VALUES ('team', $1, 'FOOTBALL', public.nrm($2),
			        CASE WHEN $2 = $3 THEN 'name' ELSE 'alias' END)
			ON CONFLICT DO NOTHING`,
			id, surface, fullName); err != nil {
			return 0, fmt.Errorf("club surface %q: %w", surface, err)
		}
	}

	// The census candidates that were counting mentions against the wall
	// resolve onto the club — the news history joins the row it was about.
	tag, err := tx.Exec(ctx, `
		UPDATE entity_candidates
		SET state = 'accepted', resolved_entity_type = 'team', resolved_entity_id = $1,
		    decided_at = NOW()
		WHERE sport = 'FOOTBALL' AND state LIKE 'rejected%'
		  AND norm_name IN (public.nrm($2), public.nrm($3), public.nrm($4))`,
		id, fullName, t.Name, t.ShortName)
	if err != nil {
		return 0, fmt.Errorf("resolve club candidates: %w", err)
	}

	if err := res.bind(ctx, tx, "team", strconv.Itoa(t.ID), id); err != nil {
		return 0, err
	}
	if err := tx.Commit(ctx); err != nil {
		return 0, err
	}
	res.teams[strconv.Itoa(t.ID)] = id
	logger.Info("dataimport: fpl club created",
		"club", fullName, "id", id, "candidates_resolved", tag.RowsAffected())
	return id, nil
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
// funnel. The first live run (2026-09-06) measured why plain equality fails —
// FPL carries FULL LEGAL names ("Emiliano Martínez Romero", "João Pedro
// Loureiro da Costa" for the man the house calls Costinha) with diacritics —
// so matching runs through the HOUSE normalizer (public.nrm, the Editor's own)
// and a ladder of surfaces:
//
//   1. nrm(full name) exact against players.name
//   2. entity_name_surfaces (the Editor's known-surface registry): full, web
//   3. first + last token ("Levi Samuels Colwill" → "levi colwill")
//   4. within the fixture's team: nrm(name) equal to or suffixed by nrm(web)
//
// Each rung takes a UNIQUE hit (team-narrowed when plural) and binds the FPL
// element id permanently, so the ladder runs once per player ever.
func matchFPLPlayer(ctx context.Context, q querier, res *Resolver, el fplElement, fullName string, teamID int) (int, error) {
	ext := strconv.Itoa(el.ID)
	if id, ok := res.players[ext]; ok {
		return id, nil
	}

	// sawCandidates distinguishes the two failure classes at the end of the
	// ladder: an AMBIGUOUS miss (rows existed, none uniquely resolvable — the
	// Bruno Fernandes class, where creating would mint a duplicate) funnels;
	// a ZERO-CANDIDATE miss (the name simply does not exist — a promoted
	// club's squad) creates from the stat line (Scott, 2026-09-07: "use the
	// stats payloads to create players and enqueue the investigator for meta
	// data" — the nflverse existence-comes-from-data rule, ambiguity-gated).
	sawCandidates := false
	unique := func(sql string, args ...any) (int, error) {
		rows, err := q.Query(ctx, sql, args...)
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
		if len(cands) > 0 {
			sawCandidates = true
		}
		switch {
		case len(cands) == 1:
			return cands[0].id, nil
		case len(cands) > 1:
			id, matched := 0, 0
			for _, c := range cands {
				if c.teamID == teamID {
					id, matched = c.id, matched+1
				}
			}
			if matched == 1 {
				return id, nil
			}
		}
		return 0, nil
	}

	commit := func(id int) (int, error) {
		if err := res.bind(ctx, q, "player", ext, id); err != nil {
			return 0, err
		}
		res.players[ext] = id
		return id, nil
	}

	// 1. The full name through the house normalizer.
	if id, err := unique(`
		SELECT id, COALESCE(team_id, 0) FROM players
		WHERE sport = 'FOOTBALL' AND public.nrm(name) = public.nrm($1)`, fullName); err != nil {
		return 0, err
	} else if id != 0 {
		return commit(id)
	}

	// 2. The Editor's surface registry — full name, then the web name.
	for _, surface := range []string{fullName, el.WebName} {
		if id, err := unique(`
			SELECT s.entity_id, COALESCE(p.team_id, 0)
			FROM entity_name_surfaces s
			JOIN players p ON p.id = s.entity_id AND p.sport = s.sport
			WHERE s.entity_type = 'player' AND s.sport = 'FOOTBALL'
			  AND s.norm = public.nrm($1)`, surface); err != nil {
			return 0, err
		} else if id != 0 {
			return commit(id)
		}
	}

	// 2b. The house name as a PREFIX of the FPL legal name — the biggest
	// measured class ("Ezri Konsa Ngoyo" → house "Ezri Konsa", "Alejandro
	// Garnacho Ferreyra" → "Alejandro Garnacho", "Emiliano Martínez Romero" →
	// "Emiliano Martínez"): FPL appends family names the house drops.
	if id, err := unique(`
		SELECT id, COALESCE(team_id, 0) FROM players
		WHERE sport = 'FOOTBALL'
		  AND public.nrm($1) LIKE public.nrm(name) || ' %'
		  AND length(public.nrm(name)) >= 8`, fullName); err != nil {
		return 0, err
	} else if id != 0 {
		return commit(id)
	}

	// 3. First + last token: the middle-names cut.
	first, last := splitName(fullName)
	if first != "" && last != "" {
		if id, err := unique(`
			SELECT id, COALESCE(team_id, 0) FROM players
			WHERE sport = 'FOOTBALL' AND public.nrm(name) = public.nrm($1)`,
			first+" "+last); err != nil {
			return 0, err
		} else if id != 0 {
			return commit(id)
		}
	}

	// 4. The web name as the surname surface — first within the fixture's
	// team, then league-wide requiring a GLOBALLY unique hit (the team rung
	// alone loses to stale team_ids on recent transfers; a unique surname in
	// the whole football table is safe without one).
	if el.WebName != "" {
		if teamID != 0 {
			if id, err := unique(`
				SELECT id, COALESCE(team_id, 0) FROM players
				WHERE sport = 'FOOTBALL' AND team_id = $2
				  AND (public.nrm(name) = public.nrm($1)
				       OR public.nrm(name) LIKE '%% ' || public.nrm($1))`,
				el.WebName, teamID); err != nil {
				return 0, err
			} else if id != 0 {
				return commit(id)
			}
		}
		if id, err := unique(`
			SELECT id, COALESCE(team_id, 0) FROM players
			WHERE sport = 'FOOTBALL'
			  AND length(public.nrm($1)) >= 6
			  AND (public.nrm(name) = public.nrm($1)
			       OR public.nrm(name) LIKE '%% ' || public.nrm($1))`,
			el.WebName); err != nil {
			return 0, err
		} else if id != 0 {
			return commit(id)
		}
	}

	if sawCandidates {
		return 0, fmt.Errorf("ambiguous house match for %q (%s) — not creating", fullName, el.WebName)
	}

	// Zero candidates anywhere on the ladder: the player does not exist, and
	// the stat line is the evidence of existence. Create, register surfaces
	// (the Editor resolves future mentions), and enqueue the Investigator for
	// the metadata dossier (dob/photo/aliases — the vetting-seed player grain).
	// (first/last already split at rung 3.)
	var id int
	if err := q.QueryRow(ctx, `
		INSERT INTO players (sport, name, first_name, last_name, team_id, league_id, meta)
		VALUES ('FOOTBALL', $1, $2, $3, NULLIF($4, 0), $5,
		        jsonb_build_object('position_abbreviation', $6::text, 'created_by', 'dataimport-fpl'))
		RETURNING id`,
		fullName, first, last, teamID, fplLeagueID, fplPositions[el.Type]).Scan(&id); err != nil {
		return 0, fmt.Errorf("create player %q: %w", fullName, err)
	}
	for _, surface := range []string{fullName, el.WebName} {
		if surface == "" {
			continue
		}
		if _, err := q.Exec(ctx, `
			INSERT INTO entity_name_surfaces (entity_type, entity_id, sport, norm, surface_kind)
			VALUES ('player', $1, 'FOOTBALL', public.nrm($2),
			        CASE WHEN $2 = $3 THEN 'name' ELSE 'alias' END)
			ON CONFLICT DO NOTHING`,
			id, surface, fullName); err != nil {
			return 0, fmt.Errorf("player surface %q: %w", surface, err)
		}
	}
	if _, err := q.Exec(ctx, `
		INSERT INTO pipeline_work (stage, entity_type, entity_id, sport, status, available_at, updated_at)
		VALUES ('investigate_entity', 'player', $1, 'FOOTBALL', 'pending', NOW(), NOW())
		ON CONFLICT (stage, entity_type, entity_id, sport) DO NOTHING`, id); err != nil {
		return 0, fmt.Errorf("enqueue investigator for player %d: %w", id, err)
	}
	return commit(id)
}
