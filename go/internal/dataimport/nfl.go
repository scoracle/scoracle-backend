package dataimport

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"strconv"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// nflverse flat-file releases — keyless, versioned by season, updated within
// hours of games (worst case: a game missed tonight is still in the gap
// tomorrow). games.csv is all seasons in one file.
const (
	nflSchedulesURL  = "https://github.com/nflverse/nflverse-data/releases/download/schedules/games.csv"
	nflRosterURL     = "https://github.com/nflverse/nflverse-data/releases/download/rosters/roster_%d.csv"
	nflPlayerWeekURL = "https://github.com/nflverse/nflverse-data/releases/download/stats_player/stats_player_week_%d.csv"
	nflTeamWeekURL   = "https://github.com/nflverse/nflverse-data/releases/download/stats_team/stats_team_week_%d.csv"
)

// Funnel counts every row through the run, RSS-funnel doctrine: bounded
// coverage is fine, silent truncation is not — anything skipped is counted and
// logged, and the gap query re-offers it next run.
type Funnel struct {
	ScheduleRows     int
	FixturesCreated  int
	FixturesUpdated  int
	TeamsUnmatched   int
	RosterRows       int
	PlayersCreated   int
	PlayersUnmatched int
	Gaps             int
	GapsFilled       int
	GapsWaiting      int // finished per the schedule, stats not published yet
	GapsFailed       int
	EventPlayers     int
	EventTeams       int
}

func (f Funnel) LogAttrs() []any {
	return []any{
		"schedule_rows", f.ScheduleRows,
		"fixtures_created", f.FixturesCreated,
		"fixtures_updated", f.FixturesUpdated,
		"teams_unmatched", f.TeamsUnmatched,
		"roster_rows", f.RosterRows,
		"players_created", f.PlayersCreated,
		"players_unmatched", f.PlayersUnmatched,
		"gaps", f.Gaps,
		"gaps_filled", f.GapsFilled,
		"gaps_waiting", f.GapsWaiting,
		"gaps_failed", f.GapsFailed,
		"event_players", f.EventPlayers,
		"event_teams", f.EventTeams,
	}
}

// nflverseTeamAlias translates the nflverse abbreviation dialect onto the
// house short_codes (checked against prod 2026-09-04: 32/32 match except these
// two). Kept in the adapter, not the DB: 'LA' as a teams.search_aliases entry
// would be ambiguous with the Chargers, and the dialect is nflverse's, not ours.
var nflverseTeamAlias = map[string]string{
	"LA":  "LAR",
	"WAS": "WSH",
}

func nflTeamCode(abbrev string) string {
	if house, ok := nflverseTeamAlias[abbrev]; ok {
		return house
	}
	return abbrev
}

var nyLoc = mustLoadNY()

func mustLoadNY() *time.Location {
	loc, err := time.LoadLocation("America/New_York")
	if err != nil {
		return time.FixedZone("ET", -5*3600)
	}
	return loc
}

// RunNFL is one gap-driven pass: schedules → rosters → gap-fill → one
// recompute_season per touched season → roll sports.current_season forward.
// seasonOverride (0 = auto) narrows the run to a single season for smokes and
// backfills.
func RunNFL(ctx context.Context, pool *pgxpool.Pool, seasonOverride int, logger *slog.Logger) (Funnel, error) {
	var f Funnel

	seasons, err := nflSeasons(ctx, pool, seasonOverride)
	if err != nil {
		return f, err
	}
	logger.Info("dataimport: nfl run starting", "seasons", seasons)

	res, err := NewResolver(ctx, pool, "nflverse", "NFL")
	if err != nil {
		return f, err
	}

	if err := importNFLSchedules(ctx, pool, res, seasons, &f, logger); err != nil {
		return f, fmt.Errorf("schedules: %w", err)
	}
	for _, season := range seasons {
		if err := importNFLRoster(ctx, pool, res, season, &f, logger); err != nil {
			// Roster trouble must not block the stats gap-fill; call-ups bind
			// or create from their stat lines anyway.
			logger.Warn("dataimport: nfl roster import failed", "season", season, "error", err)
		}
	}

	touched, err := fillNFLGaps(ctx, pool, res, &f, logger)
	if err != nil {
		return f, fmt.Errorf("gap-fill: %w", err)
	}
	for _, season := range touched {
		logger.Info("dataimport: recompute_season", "sport", "NFL", "season", season)
		if _, err := pool.Exec(ctx, `SELECT recompute_season('NFL', $1)`, season); err != nil {
			return f, fmt.Errorf("recompute_season NFL %d: %w", season, err)
		}
		// The manual pin, automated: the season advances when its first game is
		// real, and never moves backward.
		if _, err := pool.Exec(ctx, `
			UPDATE sports SET current_season = $1
			WHERE id = 'NFL' AND current_season < $1`, season); err != nil {
			return f, fmt.Errorf("roll current_season: %w", err)
		}
	}

	logger.Info("dataimport: nfl run complete", f.LogAttrs()...)
	return f, nil
}

// nflSeasons: the season the house is on plus the one after it — that pair
// covers the rollover window, and the schedule feed decides when "after"
// becomes real.
func nflSeasons(ctx context.Context, pool *pgxpool.Pool, override int) ([]int, error) {
	if override != 0 {
		return []int{override}, nil
	}
	var cur int
	if err := pool.QueryRow(ctx, `SELECT current_season FROM sports WHERE id = 'NFL'`).Scan(&cur); err != nil {
		return nil, fmt.Errorf("read sports.current_season: %w", err)
	}
	return []int{cur, cur + 1}, nil
}

func importNFLSchedules(ctx context.Context, pool *pgxpool.Pool, res *Resolver, seasons []int, f *Funnel, logger *slog.Logger) error {
	t, err := fetchCSV(ctx, nflSchedulesURL)
	if err != nil {
		return err
	}
	want := map[int]bool{}
	for _, s := range seasons {
		want[s] = true
	}

	badTeams := map[string]bool{} // log each unmatched abbreviation once
	for i := 0; i < t.Len(); i++ {
		season, _ := strconv.Atoi(t.Get(i, "season"))
		if !want[season] {
			continue
		}
		f.ScheduleRows++
		gameID := t.Get(i, "game_id")
		if gameID == "" {
			continue
		}

		homeID, err1 := res.Team(ctx, pool, nflTeamCode(t.Get(i, "home_team")))
		awayID, err2 := res.Team(ctx, pool, nflTeamCode(t.Get(i, "away_team")))
		if err1 != nil || err2 != nil {
			for _, e := range []error{err1, err2} {
				if e != nil && !badTeams[e.Error()] {
					badTeams[e.Error()] = true
					f.TeamsUnmatched++
					logger.Warn("dataimport: nfl team unmatched", "error", e)
				}
			}
			continue
		}

		startTime := nflKickoff(t.Get(i, "gameday"), t.Get(i, "gametime"))
		round := nflRound(t.Get(i, "game_type"), t.Get(i, "week"))
		venue := t.Get(i, "stadium")
		final := t.Get(i, "result") != "" // result is "" until the game is final; a tie is "0"
		var homeScore, awayScore *int
		if final {
			hs, _ := strconv.Atoi(t.Get(i, "home_score"))
			as, _ := strconv.Atoi(t.Get(i, "away_score"))
			homeScore, awayScore = &hs, &as
		}

		if fid := res.Fixture(gameID); fid != 0 {
			tag, err := pool.Exec(ctx, `
				UPDATE fixtures SET
					start_time = $2, round = $3,
					venue_name = COALESCE(NULLIF($4, ''), venue_name),
					home_score = COALESCE($5, home_score),
					away_score = COALESCE($6, away_score),
					status = CASE WHEN status IN ('seeded') THEN status
					              WHEN $7 THEN 'completed'
					              ELSE status END,
					updated_at = NOW()
				WHERE id = $1
				  AND (start_time IS DISTINCT FROM $2
				       OR round IS DISTINCT FROM $3
				       OR home_score IS DISTINCT FROM COALESCE($5, home_score)
				       OR away_score IS DISTINCT FROM COALESCE($6, away_score)
				       OR (status = 'scheduled' AND $7))`,
				fid, startTime, round, venue, homeScore, awayScore, final)
			if err != nil {
				return fmt.Errorf("update fixture %s: %w", gameID, err)
			}
			if tag.RowsAffected() > 0 {
				f.FixturesUpdated++
			}
			continue
		}

		// Unbound: adopt a same-teams same-day fixture if one exists (Editor
		// nominations from the news era), else create. Schedules are the fixture
		// authority from here on.
		var fid int
		err = pool.QueryRow(ctx, `
			SELECT id FROM fixtures
			WHERE sport = 'NFL' AND season = $1
			  AND home_team_id = $2 AND away_team_id = $3
			  AND start_time::date BETWEEN $4::date - 1 AND $4::date + 1
			ORDER BY id LIMIT 1`,
			season, homeID, awayID, startTime).Scan(&fid)
		if err == pgx.ErrNoRows {
			status := "scheduled"
			if final {
				status = "completed"
			}
			err = pool.QueryRow(ctx, `
				INSERT INTO fixtures (sport, league_id, season, home_team_id, away_team_id,
				                      start_time, venue_name, round, status, home_score, away_score)
				VALUES ('NFL', 0, $1, $2, $3, $4, NULLIF($5, ''), $6, $7, $8, $9)
				RETURNING id`,
				season, homeID, awayID, startTime, venue, round, status, homeScore, awayScore).Scan(&fid)
			if err != nil {
				return fmt.Errorf("insert fixture %s: %w", gameID, err)
			}
			f.FixturesCreated++
		} else if err != nil {
			return fmt.Errorf("match fixture %s: %w", gameID, err)
		} else {
			f.FixturesUpdated++
		}
		if err := res.BindFixture(ctx, pool, gameID, fid); err != nil {
			return err
		}
	}
	return nil
}

func importNFLRoster(ctx context.Context, pool *pgxpool.Pool, res *Resolver, season int, f *Funnel, logger *slog.Logger) error {
	runStart := time.Now()
	t, err := fetchCSV(ctx, fmt.Sprintf(nflRosterURL, season))
	if err != nil {
		if errors.Is(err, errNotPublished) {
			logger.Info("dataimport: nfl roster not published yet", "season", season)
			return nil
		}
		return err
	}

	// The file is cumulative-weekly; keep each player's latest row.
	latest := map[string]int{}
	for i := 0; i < t.Len(); i++ {
		gsis := t.Get(i, "gsis_id")
		if gsis == "" {
			continue
		}
		if j, ok := latest[gsis]; !ok || t.Num(i, "week") >= t.Num(j, "week") {
			latest[gsis] = i
		}
	}

	for gsis, i := range latest {
		f.RosterRows++
		teamID, err := res.Team(ctx, pool, nflTeamCode(t.Get(i, "team")))
		if err != nil {
			f.TeamsUnmatched++
			continue
		}
		pid, created, err := res.Player(ctx, pool, gsis, t.Get(i, "full_name"), t.Get(i, "position"), teamID)
		if err != nil {
			f.PlayersUnmatched++
			logger.Debug("dataimport: nfl player unmatched", "gsis", gsis, "error", err)
			continue
		}
		if created {
			f.PlayersCreated++
		}
		active := t.Get(i, "status") == "ACT"
		if _, err := pool.Exec(ctx, `
			INSERT INTO team_rosters (sport, season, team_id, player_id, jersey_number,
			                          position, position_group, is_active, source, last_seen)
			VALUES ('NFL', $1, $2, $3, NULLIF($4, ''), NULLIF($5, ''),
			        public.position_group('NFL', NULLIF($5, '')), $6, 'nflverse', NOW())
			ON CONFLICT (sport, season, team_id, player_id) DO UPDATE SET
				jersey_number  = EXCLUDED.jersey_number,
				position       = EXCLUDED.position,
				position_group = EXCLUDED.position_group,
				is_active      = EXCLUDED.is_active,
				source         = 'nflverse',
				last_seen      = NOW()`,
			season, teamID, pid, t.Get(i, "jersey_number"), t.Get(i, "position"), active); err != nil {
			return fmt.Errorf("roster upsert player %d: %w", pid, err)
		}
	}

	// Rows this run did not touch are players no longer on that roster.
	if _, err := pool.Exec(ctx, `
		UPDATE team_rosters SET is_active = false
		WHERE sport = 'NFL' AND season = $1 AND source = 'nflverse'
		  AND is_active AND last_seen < $2`, season, runStart); err != nil {
		return fmt.Errorf("roster deactivation sweep: %w", err)
	}
	return nil
}

// gapFixture is one finished-per-the-feed fixture with no promoted stats.
type gapFixture struct {
	id                   int
	gameID               string
	season               int
	homeID, awayID       int
	homeScore, awayScore int
}

// fillNFLGaps promotes every gapped fixture whose stats the feed has published,
// one transaction per fixture: event rows + finalize_fixture(id, false). The
// per-season recompute runs once afterward (finalize's documented bulk shape).
// Returns the seasons that gained at least one seeded fixture.
func fillNFLGaps(ctx context.Context, pool *pgxpool.Pool, res *Resolver, f *Funnel, logger *slog.Logger) ([]int, error) {
	rows, err := pool.Query(ctx, `
		SELECT f.id, e.external_id, f.season, f.home_team_id, f.away_team_id,
		       COALESCE(f.home_score, 0), COALESCE(f.away_score, 0)
		FROM fixtures f
		JOIN entity_external_ids e
		  ON e.namespace = 'nflverse' AND e.entity_type = 'fixture' AND e.entity_id = f.id
		WHERE f.sport = 'NFL' AND f.status = 'completed'
		ORDER BY f.season, f.start_time`)
	if err != nil {
		return nil, err
	}
	var gaps []gapFixture
	for rows.Next() {
		var g gapFixture
		if err := rows.Scan(&g.id, &g.gameID, &g.season, &g.homeID, &g.awayID, &g.homeScore, &g.awayScore); err != nil {
			rows.Close()
			return nil, err
		}
		gaps = append(gaps, g)
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return nil, err
	}
	f.Gaps = len(gaps)
	if len(gaps) == 0 {
		return nil, nil
	}

	bySeason := map[int][]gapFixture{}
	for _, g := range gaps {
		bySeason[g.season] = append(bySeason[g.season], g)
	}

	var touched []int
	for season, group := range bySeason {
		players, err := fetchCSV(ctx, fmt.Sprintf(nflPlayerWeekURL, season))
		if err != nil {
			if errors.Is(err, errNotPublished) {
				f.GapsWaiting += len(group)
				continue
			}
			return touched, err
		}
		teams, err := fetchCSV(ctx, fmt.Sprintf(nflTeamWeekURL, season))
		if err != nil {
			if errors.Is(err, errNotPublished) {
				f.GapsWaiting += len(group)
				continue
			}
			return touched, err
		}
		playersByGame := indexByGame(players)
		teamsByGame := indexByGame(teams)

		seeded := 0
		for _, g := range group {
			pRows, tRows := playersByGame[g.gameID], teamsByGame[g.gameID]
			if len(tRows) != 2 || len(pRows) == 0 {
				// Final per the schedule, stats not published yet — the gap
				// holds it for the next run.
				f.GapsWaiting++
				continue
			}
			if err := promoteNFLFixture(ctx, pool, res, players, teams, g, pRows, tRows, f); err != nil {
				f.GapsFailed++
				logger.Warn("dataimport: nfl fixture promotion failed",
					"game", g.gameID, "fixture", g.id, "error", err)
				continue
			}
			f.GapsFilled++
			seeded++
		}
		if seeded > 0 {
			touched = append(touched, season)
		}
		logger.Info("dataimport: nfl season gap-fill",
			"season", season, "gaps", len(group), "seeded", seeded)
	}
	return touched, nil
}

func indexByGame(t *Table) map[string][]int {
	m := map[string][]int{}
	for i := 0; i < t.Len(); i++ {
		if id := t.Get(i, "game_id"); id != "" {
			m[id] = append(m[id], i)
		}
	}
	return m
}

// promoteNFLFixture is the promotion transaction PLAN-one-rail specified:
// event_box_scores + event_team_stats + finalize_fixture, atomically.
func promoteNFLFixture(ctx context.Context, pool *pgxpool.Pool, res *Resolver,
	players, teams *Table, g gapFixture, pRows, tRows []int, f *Funnel) error {

	tx, err := pool.Begin(ctx)
	if err != nil {
		return err
	}
	defer tx.Rollback(ctx)

	for _, i := range tRows {
		teamID, err := res.Team(ctx, tx, nflTeamCode(teams.Get(i, "team")))
		if err != nil {
			return err
		}
		score := g.awayScore
		if teamID == g.homeID {
			score = g.homeScore
		} else if teamID != g.awayID {
			return fmt.Errorf("team row %q is neither side of %s", teams.Get(i, "team"), g.gameID)
		}
		stats, err := json.Marshal(mapTeamRow(teams, i))
		if err != nil {
			return err
		}
		if _, err := tx.Exec(ctx, `
			INSERT INTO event_team_stats (fixture_id, team_id, sport, season, league_id, score, stats)
			VALUES ($1, $2, 'NFL', $3, 0, $4, $5::jsonb)
			ON CONFLICT (fixture_id, team_id) DO UPDATE SET
				score = EXCLUDED.score, stats = EXCLUDED.stats`,
			g.id, teamID, g.season, score, string(stats)); err != nil {
			return fmt.Errorf("event_team_stats %s: %w", g.gameID, err)
		}
		f.EventTeams++
	}

	for _, i := range pRows {
		teamID, err := res.Team(ctx, tx, nflTeamCode(players.Get(i, "team")))
		if err != nil {
			return err
		}
		ext := players.Get(i, "player_id") // gsis id
		name := players.Get(i, "player_display_name")
		pos := players.Get(i, "position")
		pid, created, err := res.Player(ctx, tx, ext, name, pos, teamID)
		if err != nil {
			// One unmatched player must not hold a whole game's stats hostage;
			// count it and promote the rest.
			f.PlayersUnmatched++
			continue
		}
		if created {
			f.PlayersCreated++
		}
		stats, err := json.Marshal(mapPlayerRow(players, i))
		if err != nil {
			return err
		}
		if _, err := tx.Exec(ctx, `
			INSERT INTO event_box_scores (fixture_id, player_id, team_id, sport, season, league_id, stats, position)
			VALUES ($1, $2, $3, 'NFL', $4, 0, $5::jsonb, NULLIF($6, ''))
			ON CONFLICT (fixture_id, player_id) DO UPDATE SET
				team_id = EXCLUDED.team_id, stats = EXCLUDED.stats, position = EXCLUDED.position`,
			g.id, pid, teamID, g.season, string(stats), pos); err != nil {
			return fmt.Errorf("event_box_scores %s player %d: %w", g.gameID, pid, err)
		}
		f.EventPlayers++
	}

	// Aggregate the touched season rows and mark the fixture seeded; the
	// season-wide recompute runs once per season after the batch.
	if _, err := tx.Exec(ctx, `SELECT finalize_fixture($1, false)`, g.id); err != nil {
		return fmt.Errorf("finalize_fixture(%d): %w", g.id, err)
	}
	return tx.Commit(ctx)
}

// nflKickoff builds the ET start time; a missing gametime lands at noon ET,
// which is wrong by hours and right by day — the day is what the product keys on.
func nflKickoff(gameday, gametime string) time.Time {
	day, err := time.ParseInLocation("2006-01-02", gameday, nyLoc)
	if err != nil {
		return time.Time{}
	}
	if hm := strings.Split(gametime, ":"); len(hm) == 2 {
		h, _ := strconv.Atoi(hm[0])
		m, _ := strconv.Atoi(hm[1])
		return day.Add(time.Duration(h)*time.Hour + time.Duration(m)*time.Minute)
	}
	return day.Add(12 * time.Hour)
}

func nflRound(gameType, week string) string {
	switch gameType {
	case "REG", "":
		return "Week " + week
	case "WC":
		return "Wild Card"
	case "DIV":
		return "Divisional"
	case "CON":
		return "Conference"
	case "SB":
		return "Super Bowl"
	}
	return gameType + " " + week
}
