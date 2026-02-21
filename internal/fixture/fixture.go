// Package fixture provides post-match seeding and fixture scheduling.
// After a match completes (+ configurable delay), the seeder refreshes
// stats from the upstream provider and recalculates percentiles.
//
// Ported from Python PostMatchSeeder + SchedulerService.
package fixture

import (
	"fmt"
	"time"
)

// --------------------------------------------------------------------------
// Constants
// --------------------------------------------------------------------------

const (
	defaultMaxFixtures = 50
	defaultMaxRetries  = 3
)

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

// Row represents a fixture row from the database.
type Row struct {
	ID             int
	Sport          string
	LeagueID       *int
	Season         int
	HomeTeamID     int
	AwayTeamID     int
	StartTime      time.Time
	SeedDelayHours int
	SeedAttempts   int
	ExternalID     *int
}

// Result tracks the outcome of seeding a single fixture.
type Result struct {
	FixtureID               int
	Sport                   string
	HomeTeamID              int
	AwayTeamID              int
	PlayersUpdated          int
	TeamsUpdated            int
	PercentilesRecalculated bool
	Success                 bool
	Error                   string
	Duration                time.Duration
}

// Summary returns a human-readable summary.
func (r *Result) Summary() string {
	status := "ok"
	if !r.Success {
		status = "FAILED"
	}
	return fmt.Sprintf("fixture=%d sport=%s players=%d teams=%d pctiles=%v status=%s dur=%s",
		r.FixtureID, r.Sport, r.PlayersUpdated, r.TeamsUpdated,
		r.PercentilesRecalculated, status, r.Duration.Round(time.Second))
}

// SchedulerResult tracks the outcome of a full scheduler run.
type SchedulerResult struct {
	FixturesFound     int
	FixturesProcessed int
	FixturesSucceeded int
	FixturesFailed    int
	PlayersUpdated    int
	TeamsUpdated      int
	Duration          time.Duration
	Errors            []string
	Results           []Result
}

// Summary returns a human-readable summary.
func (r *SchedulerResult) Summary() string {
	return fmt.Sprintf(
		"found=%d processed=%d succeeded=%d failed=%d players=%d teams=%d dur=%s",
		r.FixturesFound, r.FixturesProcessed, r.FixturesSucceeded,
		r.FixturesFailed, r.PlayersUpdated, r.TeamsUpdated,
		r.Duration.Round(time.Second))
}
