package transferidentity

import (
	"context"
	"encoding/json"
	"os"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

const (
	testSport      = "NBA"
	testLeagueID   = 990001
	testOldTeamID  = 990101
	testNewTeamID  = 990102
	testAwayTeamID = 990103
	testFixtureID  = 990201
	testPlayerID   = 990301
	testRumorBase  = int64(990401)
)

type applyResult struct {
	ApplicationID int64
	OverrideID    *int64
	Status        string
	Reason        string
}

func testPool(t *testing.T) *pgxpool.Pool {
	t.Helper()
	url := os.Getenv("TEST_DATABASE_URL")
	if url == "" {
		t.Skip("TEST_DATABASE_URL not set; skipping transfer identity integration tests")
	}
	pool, err := pgxpool.New(context.Background(), url)
	if err != nil {
		t.Fatalf("connect test db: %v", err)
	}
	clean(t, pool)
	seed(t, pool)
	t.Cleanup(func() {
		clean(t, pool)
		pool.Close()
	})
	return pool
}

func clean(t *testing.T, pool *pgxpool.Pool) {
	t.Helper()
	ctx := context.Background()
	statements := []string{
		`DELETE FROM public.transfer_identity_applications WHERE sport = 'NBA' AND (player_id BETWEEN 990300 AND 990399 OR source_rumor_id BETWEEN 990400 AND 990499)`,
		`DELETE FROM public.player_current_identity_overrides WHERE sport = 'NBA' AND (player_id BETWEEN 990300 AND 990399 OR source_rumor_id BETWEEN 990400 AND 990499)`,
		`DELETE FROM public.event_box_scores WHERE sport = 'NBA' AND fixture_id BETWEEN 990200 AND 990299`,
		`DELETE FROM public.fixtures WHERE sport = 'NBA' AND id BETWEEN 990200 AND 990299`,
		`DELETE FROM public.player_stats WHERE sport = 'NBA' AND player_id BETWEEN 990300 AND 990399`,
		`DELETE FROM public.players WHERE sport = 'NBA' AND id BETWEEN 990300 AND 990399`,
		`DELETE FROM public.teams WHERE sport = 'NBA' AND id BETWEEN 990100 AND 990199`,
	}
	for _, stmt := range statements {
		if _, err := pool.Exec(ctx, stmt); err != nil {
			t.Fatalf("clean %q: %v", stmt, err)
		}
	}
}

func seed(t *testing.T, pool *pgxpool.Pool) {
	t.Helper()
	ctx := context.Background()
	if _, err := pool.Exec(ctx, `
		INSERT INTO public.sports (id, display_name, current_season)
		VALUES
			('NBA', 'NBA', 2025),
			('NFL', 'NFL', 2025)
		ON CONFLICT (id) DO UPDATE
		SET display_name = EXCLUDED.display_name,
		    current_season = EXCLUDED.current_season`,
	); err != nil {
		t.Fatalf("seed sport: %v", err)
	}
	if _, err := pool.Exec(ctx, `
		INSERT INTO public.transfer_identity_thresholds
			(sport, min_heat, min_deterministic_confidence, min_adjudication_confidence)
		VALUES ('NBA', 80, 0.800, 0.850)
		ON CONFLICT (sport) DO UPDATE
		SET min_heat = EXCLUDED.min_heat,
		    min_deterministic_confidence = EXCLUDED.min_deterministic_confidence,
		    min_adjudication_confidence = EXCLUDED.min_adjudication_confidence`,
	); err != nil {
		t.Fatalf("seed thresholds: %v", err)
	}
	if _, err := pool.Exec(ctx, `
		INSERT INTO public.sport_autofill_versions (sport, total_entities, status)
		VALUES
			('NBA', 0, 'ready'),
			('NFL', 0, 'ready')
		ON CONFLICT (sport) DO UPDATE
		SET total_entities = EXCLUDED.total_entities,
		    status = EXCLUDED.status,
		    reason = NULL`,
	); err != nil {
		t.Fatalf("seed autofill versions: %v", err)
	}
	if _, err := pool.Exec(ctx, `
		INSERT INTO public.teams (id, sport, name, league_id)
		VALUES
			($1, 'NBA', 'Transfer Identity Old Team', $4),
			($2, 'NBA', 'Transfer Identity New Team', $4),
			($3, 'NBA', 'Transfer Identity Away Team', $4)
		ON CONFLICT (id, sport) DO UPDATE SET name = EXCLUDED.name, league_id = EXCLUDED.league_id`,
		testOldTeamID, testNewTeamID, testAwayTeamID, testLeagueID,
	); err != nil {
		t.Fatalf("seed teams: %v", err)
	}
	if _, err := pool.Exec(ctx, `
		INSERT INTO public.players (id, sport, name, team_id, league_id)
		VALUES ($1, 'NBA', 'Transfer Identity Test Player', $2, $3)
		ON CONFLICT (id, sport) DO UPDATE SET team_id = EXCLUDED.team_id, league_id = EXCLUDED.league_id`,
		testPlayerID, testOldTeamID, testLeagueID,
	); err != nil {
		t.Fatalf("seed player: %v", err)
	}
	if _, err := pool.Exec(ctx, `
		INSERT INTO public.player_stats (player_id, sport, season, league_id, team_id, stats, position)
		VALUES ($1, 'NBA', 2025, $2, $3, '{"points": 12, "rebounds": 4}'::jsonb, 'G')
		ON CONFLICT (player_id, sport, season, league_id)
		DO UPDATE SET team_id = EXCLUDED.team_id, stats = EXCLUDED.stats, position = EXCLUDED.position`,
		testPlayerID, testLeagueID, testOldTeamID,
	); err != nil {
		t.Fatalf("seed player stats: %v", err)
	}
	if _, err := pool.Exec(ctx, `
		INSERT INTO public.fixtures (id, external_id, sport, league_id, season, home_team_id, away_team_id, start_time, status)
		VALUES ($1, $1, 'NBA', $2, 2025, $3, $4, NOW() - INTERVAL '10 days', 'completed')
		ON CONFLICT (id) DO UPDATE SET home_team_id = EXCLUDED.home_team_id, away_team_id = EXCLUDED.away_team_id`,
		testFixtureID, testLeagueID, testOldTeamID, testAwayTeamID,
	); err != nil {
		t.Fatalf("seed fixture: %v", err)
	}
	if _, err := pool.Exec(ctx, `
		INSERT INTO public.event_box_scores (fixture_id, player_id, team_id, sport, season, league_id, position, minutes_played, stats)
		VALUES ($1, $2, $3, 'NBA', 2025, $4, 'G', 30, '{"points": 18}'::jsonb)
		ON CONFLICT (fixture_id, player_id)
		DO UPDATE SET team_id = EXCLUDED.team_id, stats = EXCLUDED.stats, position = EXCLUDED.position`,
		testFixtureID, testPlayerID, testOldTeamID, testLeagueID,
	); err != nil {
		t.Fatalf("seed event box score: %v", err)
	}
}

func applyCandidate(t *testing.T, pool *pgxpool.Pool, rumorID int64, heat int16, detConf float64, adj map[string]any) applyResult {
	t.Helper()
	raw, err := json.Marshal(adj)
	if err != nil {
		t.Fatalf("marshal adjudication: %v", err)
	}
	var out applyResult
	err = pool.QueryRow(context.Background(), `
		SELECT application_id, override_id, status, reason
		FROM public.apply_transfer_identity_candidate(
			$1,$2,$3,$4,$5,NULL,$6,$7::float8::numeric,$8::jsonb,$9,$10,$11
		)`,
		testSport, testPlayerID, testOldTeamID, testNewTeamID, rumorID,
		heat, detConf, string(raw), string(raw), "test-model", "test-prompt",
	).Scan(&out.ApplicationID, &out.OverrideID, &out.Status, &out.Reason)
	if err != nil {
		t.Fatalf("apply candidate: %v", err)
	}
	return out
}

func applyAdjudication(decision string) map[string]any {
	return map[string]any{
		"decision":    decision,
		"event_type":  "trade",
		"confidence":  0.93,
		"old_team_id": testOldTeamID,
		"new_team_id": testNewTeamID,
		"reason":      "integration test",
	}
}

func countOverrides(t *testing.T, pool *pgxpool.Pool) int {
	t.Helper()
	var n int
	if err := pool.QueryRow(context.Background(), `
		SELECT COUNT(*)::int
		FROM public.player_current_identity_overrides
		WHERE sport = 'NBA' AND player_id = $1 AND reverted_at IS NULL`,
		testPlayerID,
	).Scan(&n); err != nil {
		t.Fatalf("count overrides: %v", err)
	}
	return n
}

func currentTeam(t *testing.T, pool *pgxpool.Pool) *int {
	t.Helper()
	identity, err := GetCurrentIdentity(context.Background(), pool, testSport, testPlayerID)
	if err != nil {
		t.Fatalf("current identity: %v", err)
	}
	return identity.TeamID
}

func historicalRows(t *testing.T, pool *pgxpool.Pool) (statsTeam int, eventTeam int, eventStats string) {
	t.Helper()
	ctx := context.Background()
	if err := pool.QueryRow(ctx, `
		SELECT team_id FROM public.player_stats
		WHERE sport = 'NBA' AND player_id = $1 AND season = 2025 AND league_id = $2`,
		testPlayerID, testLeagueID,
	).Scan(&statsTeam); err != nil {
		t.Fatalf("read player stats history: %v", err)
	}
	if err := pool.QueryRow(ctx, `
		SELECT team_id, stats::text FROM public.event_box_scores
		WHERE sport = 'NBA' AND fixture_id = $1 AND player_id = $2`,
		testFixtureID, testPlayerID,
	).Scan(&eventTeam, &eventStats); err != nil {
		t.Fatalf("read event history: %v", err)
	}
	return statsTeam, eventTeam, eventStats
}

func TestThresholdCrossingAppliesOnceAndRevertRestoresPreviousIdentity(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t)

	nflBefore, err := GetAutofillVersion(ctx, pool, "NFL")
	if err != nil {
		t.Fatalf("nfl version before: %v", err)
	}
	statsBefore, eventBefore, eventStatsBefore := historicalRows(t, pool)

	first := applyCandidate(t, pool, testRumorBase+1, 90, 0.90, applyAdjudication("apply"))
	if first.Status != "applied" || first.OverrideID == nil {
		t.Fatalf("first apply = %+v, want applied with override", first)
	}
	if got := currentTeam(t, pool); got == nil || *got != testNewTeamID {
		t.Fatalf("current team after apply = %v, want %d", got, testNewTeamID)
	}
	refresh, err := RefreshSportAutofillConcurrent(ctx, pool, testSport, "integration_test_apply")
	if err != nil {
		t.Fatalf("refresh after apply: %v", err)
	}
	if refresh.Status != "ready" {
		t.Fatalf("refresh status = %q, want ready", refresh.Status)
	}

	second := applyCandidate(t, pool, testRumorBase+1, 90, 0.90, applyAdjudication("apply"))
	if second.Status == "applied" && second.ApplicationID != first.ApplicationID {
		t.Fatalf("second apply created duplicate applied application: first=%+v second=%+v", first, second)
	}
	if n := countOverrides(t, pool); n != 1 {
		t.Fatalf("active overrides = %d, want 1", n)
	}

	nflAfter, err := GetAutofillVersion(ctx, pool, "NFL")
	if err != nil {
		t.Fatalf("nfl version after: %v", err)
	}
	if nflAfter.Version != nflBefore.Version || nflAfter.GeneratedAt.Sub(nflBefore.GeneratedAt) > time.Second {
		t.Fatalf("NFL version changed during NBA refresh: before=%+v after=%+v", nflBefore, nflAfter)
	}

	statsAfter, eventAfter, eventStatsAfter := historicalRows(t, pool)
	if statsAfter != statsBefore || eventAfter != eventBefore || eventStatsAfter != eventStatsBefore {
		t.Fatalf("historical rows changed: before stats=%d event=%d %s after stats=%d event=%d %s",
			statsBefore, eventBefore, eventStatsBefore, statsAfter, eventAfter, eventStatsAfter)
	}

	reverted, err := Revert(ctx, pool, first.ApplicationID, "integration-test", "restore previous current identity")
	if err != nil {
		t.Fatalf("revert: %v", err)
	}
	if reverted.Status != "reverted" {
		t.Fatalf("revert status = %q, want reverted", reverted.Status)
	}
	if got := currentTeam(t, pool); got == nil || *got != testOldTeamID {
		t.Fatalf("current team after revert = %v, want %d", got, testOldTeamID)
	}
	if reverted.VersionAfter.Sport != testSport || reverted.VersionAfter.Version <= refresh.Version {
		t.Fatalf("version after revert = %+v, want NBA version > %d", reverted.VersionAfter, refresh.Version)
	}
}

func TestBelowThresholdDoesNotApply(t *testing.T) {
	pool := testPool(t)
	res := applyCandidate(t, pool, testRumorBase+2, 70, 0.70, applyAdjudication("apply"))
	if res.Status == "applied" || res.OverrideID != nil {
		t.Fatalf("below threshold result = %+v, want non-applied without override", res)
	}
	if n := countOverrides(t, pool); n != 0 {
		t.Fatalf("active overrides = %d, want 0", n)
	}
}

func TestRejectAndManualReviewDoNotApply(t *testing.T) {
	pool := testPool(t)
	for i, decision := range []string{"reject", "manual_review"} {
		res := applyCandidate(t, pool, testRumorBase+int64(3+i), 90, 0.90, applyAdjudication(decision))
		if res.Status == "applied" || res.OverrideID != nil {
			t.Fatalf("%s result = %+v, want non-applied without override", decision, res)
		}
	}
	if n := countOverrides(t, pool); n != 0 {
		t.Fatalf("active overrides = %d, want 0", n)
	}
}

func TestInvalidAdjudicationAndFailureRowsFailClosed(t *testing.T) {
	pool := testPool(t)
	bad := applyCandidate(t, pool, testRumorBase+5, 90, 0.90, map[string]any{
		"decision":    "apply",
		"event_type":  "trade",
		"confidence":  0.93,
		"old_team_id": testOldTeamID,
		"new_team_id": testOldTeamID,
		"reason":      "conflicting team ids",
	})
	if bad.Status != "failed_closed" || bad.OverrideID != nil {
		t.Fatalf("bad adjudication result = %+v, want failed_closed without override", bad)
	}

	var failureID int64
	if err := pool.QueryRow(context.Background(), `
		SELECT public.record_transfer_identity_adjudication_failure(
			$1,$2,$3,$4,$5,NULL,$6,$7::float8::numeric,$8,$9,$10,$11
		)`,
		testSport, testPlayerID, testOldTeamID, testNewTeamID, testRumorBase+6,
		int16(90), 0.90, "{not json", "test-model", "test-prompt", "invalid identity adjudication JSON",
	).Scan(&failureID); err != nil {
		t.Fatalf("record failure row: %v", err)
	}
	app, err := Get(context.Background(), pool, failureID)
	if err != nil {
		t.Fatalf("get failure row: %v", err)
	}
	if app.Status != "failed_closed" || app.OverrideID != nil {
		t.Fatalf("failure app = %+v, want failed_closed without override", app)
	}
	if n := countOverrides(t, pool); n != 0 {
		t.Fatalf("active overrides = %d, want 0", n)
	}
}
