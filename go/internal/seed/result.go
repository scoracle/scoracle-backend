// Package seed provides database upsert orchestration for all sports.
package seed

import "fmt"

// SeedResult tracks counts and errors from a seeding operation.
type SeedResult struct {
	TeamsUpserted       int
	PlayersUpserted     int
	PlayerStatsUpserted int
	TeamStatsUpserted   int
	Errors              []string
}

// Add merges another SeedResult into this one.
func (r *SeedResult) Add(other SeedResult) {
	r.TeamsUpserted += other.TeamsUpserted
	r.PlayersUpserted += other.PlayersUpserted
	r.PlayerStatsUpserted += other.PlayerStatsUpserted
	r.TeamStatsUpserted += other.TeamStatsUpserted
	r.Errors = append(r.Errors, other.Errors...)
}

// AddError records an error message.
func (r *SeedResult) AddError(msg string) {
	r.Errors = append(r.Errors, msg)
}

// AddErrorf records a formatted error message.
func (r *SeedResult) AddErrorf(format string, args ...interface{}) {
	r.Errors = append(r.Errors, fmt.Sprintf(format, args...))
}

// Summary returns a human-readable summary of the seed operation.
func (r *SeedResult) Summary() string {
	return fmt.Sprintf(
		"teams=%d players=%d player_stats=%d team_stats=%d errors=%d",
		r.TeamsUpserted, r.PlayersUpserted,
		r.PlayerStatsUpserted, r.TeamStatsUpserted,
		len(r.Errors),
	)
}
