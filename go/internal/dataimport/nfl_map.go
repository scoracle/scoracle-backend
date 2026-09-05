package dataimport

// The nflverse → canonical key mapping. The canonical vocabulary is whatever
// nfl.aggregate_player_season / nfl.aggregate_team_season read off
// event_box_scores.stats / event_team_stats.stats (sql/nfl.sql) — this file
// exists so the aggregate functions, the derived-stat triggers, and the
// rating_datapoints arm never have to learn a second dialect.

// playerStatCols maps canonical event stat keys to nflverse
// stats_player_week_{season}.csv columns, for the 1:1 cases. Composites and
// renames-with-arithmetic are in mapPlayerRow.
var playerStatCols = map[string]string{
	// Passing
	"passing_completions":   "completions",
	"passing_attempts":      "attempts",
	"passing_yards":         "passing_yards",
	"passing_touchdowns":    "passing_tds",
	"passing_interceptions": "passing_interceptions",
	"sacks":                 "sacks_suffered",
	"sacks_loss":            "sack_yards_lost",
	// Rushing
	"rushing_attempts":   "carries",
	"rushing_yards":      "rushing_yards",
	"rushing_touchdowns": "rushing_tds",
	// Receiving
	"receptions":           "receptions",
	"receiving_targets":    "targets",
	"receiving_yards":      "receiving_yards",
	"receiving_touchdowns": "receiving_tds",
	// Ball security
	"fumbles":      "fumbles_total",
	"fumbles_lost": "fumbles_lost_total",
	// Defense
	"solo_tackles":            "def_tackles_solo",
	"defensive_sacks":         "def_sacks",
	"defensive_interceptions": "def_interceptions",
	"interception_yards":      "def_interception_yards",
	"fumbles_recovered":       "fumble_recovery_opp",
	"fumbles_touchdowns":      "fumble_recovery_tds",
	"tackles_for_loss":        "def_tackles_for_loss",
	"passes_defended":         "def_pass_defended",
	"qb_hits":                 "def_qb_hits",
	// Kicking
	"field_goal_attempts":  "fg_att",
	"field_goals_made":     "fg_made",
	"long_field_goal_made": "fg_long",
	"extra_points_made":    "pat_made",
	// Punting (nflverse pt_* = the punter's line)
	"punts":           "pt_att",
	"punt_yards":      "pt_yards",
	"punts_inside_20": "pt_inside_20",
	"long_punt":       "pt_long",
	"touchbacks":      "pt_touchback",
	// Returns
	"kick_returns":      "kickoff_returns",
	"kick_return_yards": "kickoff_return_yards",
	"punt_returns":      "punt_returns",
	"punt_return_yards": "punt_return_yards",
}

// mapPlayerRow builds one event_box_scores.stats payload from a weekly player
// row. Zero-valued keys are kept: the aggregate's played-game filter and the
// derived-stat trigger both read absolute values, and a 0 is information.
func mapPlayerRow(t *Table, i int) map[string]any {
	s := make(map[string]any, len(playerStatCols)+8)
	for canon, col := range playerStatCols {
		s[canon] = t.Num(i, col)
	}
	// total_tackles = solo + assists; the season aggregate re-derives
	// assist_tackles as (total − solo).
	s["total_tackles"] = t.Num(i, "def_tackles_solo") + t.Num(i, "def_tackle_assists")
	// def_tds is all defensive return TDs; fumble return TDs are broken out, so
	// the remainder approximates interception TDs (feeds Points Responsible For).
	intTD := t.Num(i, "def_tds") - t.Num(i, "fumble_recovery_tds")
	if intTD < 0 {
		intTD = 0
	}
	s["interception_touchdowns"] = intTD
	// special_teams_tds is KR+PR combined with no split. Points Responsible For
	// sums both return-TD keys, so parking the combined count on one key keeps
	// that composite exact; the per-key display splits are simply not offered.
	s["kick_return_touchdowns"] = t.Num(i, "special_teams_tds")
	s["punt_return_touchdowns"] = 0.0
	// Kicker scoreboard line (aggregate reads 'total_points' for kicking).
	s["total_points"] = 3*t.Num(i, "fg_made") + t.Num(i, "pat_made")
	// Classic passer rating — nflverse ships EPA/CPOE instead; the cards still
	// speak passer rating.
	if att := t.Num(i, "attempts"); att > 0 {
		s["qb_rating"] = passerRating(
			t.Num(i, "completions"), att, t.Num(i, "passing_yards"),
			t.Num(i, "passing_tds"), t.Num(i, "passing_interceptions"))
	}
	return s
}

// teamStatCols: canonical event_team_stats keys ← stats_team_week columns.
var teamStatCols = map[string]string{
	"passing_yards":           "passing_yards",
	"passing_touchdowns":      "passing_tds",
	"passing_attempts":        "attempts",
	"passing_completions":     "completions",
	"passing_interceptions":   "passing_interceptions",
	"rushing_yards":           "rushing_yards",
	"rushing_touchdowns":      "rushing_tds",
	"rushing_attempts":        "carries",
	"defensive_sacks":         "def_sacks",
	"defensive_interceptions": "def_interceptions",
	"solo_tackles":            "def_tackles_solo",
	"passes_defended":         "def_pass_defended",
	"tackles_for_loss":        "def_tackles_for_loss",
	"qb_hits":                 "def_qb_hits",
	"fumbles_recovered":       "fumble_recovery_opp",
	"fumbles_touchdowns":      "fumble_recovery_tds",
	"fumbles":                 "fumbles_total",
	"fumbles_lost":            "fumbles_lost_total",
	"defensive_touchdowns":    "def_tds",
	"field_goals_made":        "fg_made",
	"field_goal_attempts":     "fg_att",
	"extra_points_made":       "pat_made",
	"punts":                   "pt_att",
	"punt_yards":              "pt_yards",
	"punts_inside_20":         "pt_inside_20",
	"touchbacks":              "pt_touchback",
	"kick_returns":            "kickoff_returns",
	"kick_return_yards":       "kickoff_return_yards",
	"punt_returns":            "punt_returns",
	"punt_return_yards":       "punt_return_yards",
	"sack_yards_lost":         "sack_yards_lost",
	"penalties":               "penalties",
	"penalty_yards":           "penalty_yards",
}

// mapTeamRow builds one event_team_stats.stats payload. Keys the feed cannot
// supply (third/fourth-down splits, red zone, drives, possession time) are
// omitted — their season aggregates are display-only rate stats that
// jsonb_strip_nulls already tolerates as absent, and no in-composite team
// datapoint depends on them.
func mapTeamRow(t *Table, i int) map[string]any {
	s := make(map[string]any, len(teamStatCols)+6)
	for canon, col := range teamStatCols {
		s[canon] = t.Num(i, col)
	}
	s["total_tackles"] = t.Num(i, "def_tackles_solo") + t.Num(i, "def_tackle_assists")
	intTD := t.Num(i, "def_tds") - t.Num(i, "fumble_recovery_tds")
	if intTD < 0 {
		intTD = 0
	}
	s["interception_touchdowns"] = intTD
	s["kick_return_touchdowns"] = t.Num(i, "special_teams_tds")
	s["punt_return_touchdowns"] = 0.0
	// total_yards feeds the opponent's yards_allowed (the composite −z term in
	// the team arm) — it must be present per event, and pass+rush is exactly how
	// the season aggregate derives it too.
	s["total_yards"] = t.Num(i, "passing_yards") + t.Num(i, "rushing_yards")
	s["net_passing_yards"] = t.Num(i, "passing_yards") - t.Num(i, "sack_yards_lost")
	// passing_first_downs + rushing_first_downs undercounts true first downs
	// (penalty first downs are not in the feed); kept because first_downs is
	// display-only and a close undercount beats an absent stat.
	s["first_downs"] = t.Num(i, "passing_first_downs") + t.Num(i, "rushing_first_downs")
	s["first_downs_passing"] = t.Num(i, "passing_first_downs")
	s["first_downs_rushing"] = t.Num(i, "rushing_first_downs")
	return s
}

// passerRating is the classic NFL formula, each component clamped to [0, 2.375].
func passerRating(cmp, att, yds, td, ints float64) float64 {
	clamp := func(v float64) float64 {
		if v < 0 {
			return 0
		}
		if v > 2.375 {
			return 2.375
		}
		return v
	}
	a := clamp((cmp/att - 0.3) * 5)
	b := clamp((yds/att - 3) * 0.25)
	c := clamp(td / att * 20)
	d := clamp(2.375 - ints/att*25)
	r := (a + b + c + d) / 6 * 100
	// One decimal, like the league publishes it.
	return float64(int(r*10+0.5)) / 10
}
