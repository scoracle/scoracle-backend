// Package provider defines canonical data types that all providers normalize
// into. These structs are the contract between provider handlers and the seed
// runner — providers output these, seeders write them to Postgres.
//
// Adding a new provider means implementing functions that return these types.
// The seed runner and Postgres schema never change.
package provider

import "encoding/json"

// Team is the canonical team profile shape written to the teams table.
type Team struct {
	ID            int                    `json:"id"`
	Name          string                 `json:"name"`
	ShortCode     string                 `json:"short_code,omitempty"`
	City          string                 `json:"city,omitempty"`
	Country       string                 `json:"country,omitempty"`
	Conference    string                 `json:"conference,omitempty"`
	Division      string                 `json:"division,omitempty"`
	LogoURL       string                 `json:"logo_url,omitempty"`
	VenueName     string                 `json:"venue_name,omitempty"`
	VenueCapacity *int                   `json:"venue_capacity,omitempty"`
	Founded       *int                   `json:"founded,omitempty"`
	Meta          map[string]interface{} `json:"meta,omitempty"`
}

// Player is the canonical player profile shape written to the players table.
type Player struct {
	ID               int                    `json:"id"`
	Name             string                 `json:"name"`
	FirstName        string                 `json:"first_name,omitempty"`
	LastName         string                 `json:"last_name,omitempty"`
	Position         string                 `json:"position,omitempty"`
	DetailedPosition string                 `json:"detailed_position,omitempty"`
	Nationality      string                 `json:"nationality,omitempty"`
	Height           string                 `json:"height,omitempty"`
	Weight           string                 `json:"weight,omitempty"`
	DateOfBirth      string                 `json:"date_of_birth,omitempty"` // "YYYY-MM-DD"
	PhotoURL         string                 `json:"photo_url,omitempty"`
	TeamID           *int                   `json:"team_id,omitempty"`
	Meta             map[string]interface{} `json:"meta,omitempty"`
}

// PlayerStats is the canonical shape for a player's season statistics.
// Stats is a flat map of stat key → numeric value (sport-specific keys).
// Raw preserves the full provider response for debugging.
type PlayerStats struct {
	PlayerID int                    `json:"player_id"`
	TeamID   *int                   `json:"team_id,omitempty"`
	Player   *Player                `json:"player,omitempty"`
	Stats    map[string]interface{} `json:"stats"`
	Raw      json.RawMessage        `json:"raw,omitempty"`
}

// TeamStats is the canonical shape for a team's season statistics.
type TeamStats struct {
	TeamID int                    `json:"team_id"`
	Team   *Team                  `json:"team,omitempty"`
	Stats  map[string]interface{} `json:"stats"`
	Raw    json.RawMessage        `json:"raw,omitempty"`
}
