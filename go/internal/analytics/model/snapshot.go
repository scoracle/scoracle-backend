package model

import "time"

// CohortScope selects a complete league population, never an entity sample.
type CohortScope struct {
	Sport      string `json:"sport"`
	EntityType string `json:"entity_type"`
	Season     int32  `json:"season"`
}

// CohortInput preserves NULL ratings and league membership in two season partitions.
type CohortInput struct {
	EntityID int32    `json:"entity_id"`
	LeagueID int32    `json:"league_id"`
	Season   int32    `json:"season"`
	Rating   *float64 `json:"rating"`
}

// CohortSnapshot is replayable after loss of all DuckDB state. AsOf labels this
// observation; it is not a claim to reconstruct historical database state.
type CohortSnapshot struct {
	Scope        CohortScope   `json:"scope"`
	AsOf         time.Time     `json:"as_of"`
	CapturedAt   time.Time     `json:"captured_at"`
	MVCCSnapshot string        `json:"mvcc_snapshot"`
	Formula      string        `json:"formula"`
	InputHash    string        `json:"input_hash"`
	Inputs       []CohortInput `json:"inputs"`
}
