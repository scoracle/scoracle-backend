// Package notifications detects percentile threshold crossings after fixture
// seeding and schedules push notifications to entity followers.
//
// Pipeline: detect changes → fan out to followers → schedule delivery → persist.
// A background dispatch worker sends due notifications via FCM.
package notifications

import "time"

// --------------------------------------------------------------------------
// Constants
// --------------------------------------------------------------------------

const (
	defaultWindowHours = 12
	quietStartHour     = 22 // 10 PM local
	quietEndHour       = 9  // 9 AM local
	dispatchInterval   = 30 * time.Second
	dispatchBatchSize  = 100
	maxScheduleRetries = 20
)

// Percentile milestones that trigger notifications.
var milestones = []float64{90, 95, 99}

// Minimum percentile delta to trigger on movement alone.
const deltaThreshold = 10.0

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

// Change represents a single percentile movement for an entity stat.
type Change struct {
	FixtureID  int
	EntityType string // "player" | "team"
	EntityID   int
	Sport      string
	Season     int
	LeagueID   int
	StatKey    string
	OldPctile  float64
	NewPctile  float64
	SampleSize int
}

// Follower is a user following an entity.
type Follower struct {
	UserID   string
	Timezone string
}

// Pending is a notification ready to be persisted with a scheduled time.
type Pending struct {
	UserID      string
	EntityType  string
	EntityID    int
	Sport       string
	FixtureID   int
	StatKey     string
	Percentile  float64
	Message     string
	ScheduleFor time.Time
}
