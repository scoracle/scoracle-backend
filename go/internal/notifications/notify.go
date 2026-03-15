// Package notifications provides FCM push notification dispatch and supporting
// queries. The listener receives percentile change events from Postgres
// LISTEN/NOTIFY, and the dispatch worker sends queued notifications via FCM.
package notifications

import "time"

// --------------------------------------------------------------------------
// Constants
// --------------------------------------------------------------------------

const (
	dispatchInterval  = 30 * time.Second
	dispatchBatchSize = 100
)

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

// Follower is a user following an entity.
type Follower struct {
	UserID   string
	Timezone string
}
