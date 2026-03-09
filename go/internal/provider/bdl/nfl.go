package bdl

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"net/url"
	"strconv"

	"github.com/albapepper/scoracle-data/internal/provider"
)

const nflBaseURL = "https://api.balldontlie.io/nfl/v1"

// NFLHandler fetches and normalizes NFL data from BallDontLie.
type NFLHandler struct {
	client *Client
	logger *slog.Logger
}

// NewNFLHandler creates an NFL handler with the given API key.
func NewNFLHandler(apiKey string, logger *slog.Logger) *NFLHandler {
	return &NFLHandler{
		client: NewClient(nflBaseURL, apiKey, 600, logger),
		logger: logger,
	}
}

// --------------------------------------------------------------------------
// Teams
// --------------------------------------------------------------------------

type bdlNFLTeamRaw struct {
	ID           int    `json:"id"`
	Name         string `json:"name"`
	FullName     string `json:"full_name"`
	Abbreviation string `json:"abbreviation"`
	Location     string `json:"location"`
	Conference   string `json:"conference"`
	Division     string `json:"division"`
}

// GetTeams fetches all NFL teams in canonical format.
func (h *NFLHandler) GetTeams(ctx context.Context) ([]provider.Team, error) {
	resp, err := h.client.get(ctx, "/teams", nil)
	if err != nil {
		return nil, fmt.Errorf("fetch NFL teams: %w", err)
	}

	var raw []bdlNFLTeamRaw
	if err := json.Unmarshal(resp.Data, &raw); err != nil {
		return nil, fmt.Errorf("decode NFL teams: %w", err)
	}

	teams := make([]provider.Team, len(raw))
	for i, t := range raw {
		teams[i] = normalizeNFLTeam(t)
	}
	return teams, nil
}

func normalizeNFLTeam(raw bdlNFLTeamRaw) provider.Team {
	meta := make(map[string]interface{})
	if raw.FullName != "" {
		meta["full_name"] = raw.FullName
	}
	return provider.Team{
		ID:         raw.ID,
		Name:       raw.Name,
		ShortCode:  raw.Abbreviation,
		City:       raw.Location,
		Conference: raw.Conference,
		Division:   raw.Division,
		Meta:       meta,
	}
}

// --------------------------------------------------------------------------
// Players (cursor-paginated)
// --------------------------------------------------------------------------

type bdlNFLPlayerRaw struct {
	ID                   int             `json:"id"`
	FirstName            string          `json:"first_name"`
	LastName             string          `json:"last_name"`
	Position             string          `json:"position"`
	PositionAbbreviation string          `json:"position_abbreviation"`
	Height               string          `json:"height"`
	Weight               string          `json:"weight"`
	Country              string          `json:"country"`
	Team                 *bdlNFLTeamRaw  `json:"team"`
	JerseyNumber         json.RawMessage `json:"jersey_number"`
	College              string          `json:"college"`
	Experience           json.RawMessage `json:"experience"`
	Age                  *int            `json:"age"`
}

// GetPlayers iterates all NFL players via cursor pagination, calling fn for each.
func (h *NFLHandler) GetPlayers(ctx context.Context, fn func(provider.Player) error) error {
	params := url.Values{"per_page": {"100"}}

	for {
		resp, err := h.client.get(ctx, "/players", params)
		if err != nil {
			return fmt.Errorf("fetch NFL players: %w", err)
		}

		var raw []bdlNFLPlayerRaw
		if err := json.Unmarshal(resp.Data, &raw); err != nil {
			return fmt.Errorf("decode NFL players: %w", err)
		}

		for _, p := range raw {
			if err := fn(normalizeNFLPlayer(p)); err != nil {
				return err
			}
		}

		if resp.Meta.NextCursor == nil {
			break
		}
		params.Set("cursor", strconv.Itoa(*resp.Meta.NextCursor))
	}
	return nil
}

func normalizeNFLPlayer(raw bdlNFLPlayerRaw) provider.Player {
	name := (raw.FirstName + " " + raw.LastName)
	if name == " " {
		name = fmt.Sprintf("Player %d", raw.ID)
	}

	meta := make(map[string]interface{})
	if raw.PositionAbbreviation != "" {
		meta["position_abbreviation"] = raw.PositionAbbreviation
	}
	if raw.JerseyNumber != nil && string(raw.JerseyNumber) != "null" {
		meta["jersey_number"] = json.RawMessage(raw.JerseyNumber)
	}
	if raw.College != "" {
		meta["college"] = raw.College
	}
	if raw.Experience != nil && string(raw.Experience) != "null" {
		meta["experience"] = json.RawMessage(raw.Experience)
	}
	if raw.Age != nil {
		meta["age"] = *raw.Age
	}

	var teamID *int
	if raw.Team != nil {
		teamID = &raw.Team.ID
	}

	return provider.Player{
		ID:          raw.ID,
		Name:        name,
		FirstName:   raw.FirstName,
		LastName:    raw.LastName,
		Position:    raw.Position,
		Height:      raw.Height,
		Weight:      raw.Weight,
		Nationality: raw.Country,
		TeamID:      teamID,
		Meta:        meta,
	}
}

// --------------------------------------------------------------------------
// Player Stats (cursor-paginated season stats)
// --------------------------------------------------------------------------

// BDL NFL /season_stats returns flat JSON objects where stat fields are at
// the top level alongside "player", "season", and "postseason". We decode
// into a generic map, extract the player, and treat all remaining numeric
// fields as stats.

// nflPlayerStatsNonStatKeys are keys in the /season_stats response that are
// not stat values and should be excluded from the stats JSONB.
var nflPlayerStatsNonStatKeys = map[string]bool{
	"player":     true,
	"season":     true,
	"postseason": true,
	"team":       true,
}

// GetPlayerStats iterates all player season stats, calling fn for each.
func (h *NFLHandler) GetPlayerStats(ctx context.Context, season int, postseason bool, fn func(provider.PlayerStats) error) error {
	params := url.Values{
		"season":     {strconv.Itoa(season)},
		"postseason": {strconv.FormatBool(postseason)},
		"per_page":   {"100"},
	}

	for {
		resp, err := h.client.get(ctx, "/season_stats", params)
		if err != nil {
			return fmt.Errorf("fetch NFL player stats: %w", err)
		}

		// Decode as array of generic maps to capture flat stat fields
		var rawItems []map[string]json.RawMessage
		if err := json.Unmarshal(resp.Data, &rawItems); err != nil {
			return fmt.Errorf("decode NFL player stats: %w", err)
		}

		for _, item := range rawItems {
			ps, err := normalizeNFLPlayerStatsFlat(item, postseason)
			if err != nil {
				h.logger.Warn("skip NFL player stat", "error", err)
				continue
			}
			if err := fn(ps); err != nil {
				return err
			}
		}

		if resp.Meta.NextCursor == nil {
			break
		}
		params.Set("cursor", strconv.Itoa(*resp.Meta.NextCursor))
	}
	return nil
}

func normalizeNFLPlayerStatsFlat(item map[string]json.RawMessage, postseason bool) (provider.PlayerStats, error) {
	// Extract and decode the player object
	playerJSON, ok := item["player"]
	if !ok {
		return provider.PlayerStats{}, fmt.Errorf("no player field in season_stats item")
	}
	var rawPlayer bdlNFLPlayerRaw
	if err := json.Unmarshal(playerJSON, &rawPlayer); err != nil {
		return provider.PlayerStats{}, fmt.Errorf("decode player: %w", err)
	}
	player := normalizeNFLPlayer(rawPlayer)

	// Extract all non-player, non-metadata fields as stats
	stats := make(map[string]interface{}, len(item))
	for k, v := range item {
		if nflPlayerStatsNonStatKeys[k] {
			continue
		}
		// Try to decode as a number
		var num float64
		if err := json.Unmarshal(v, &num); err == nil {
			stats[k] = num
			continue
		}
		// Try to decode as a string (some stats may be string-encoded)
		var s string
		if err := json.Unmarshal(v, &s); err == nil {
			stats[k] = s
			continue
		}
		// null values — skip
	}

	// Apply canonical key renames
	stats = normalizeStatKeys(stats)

	if postseason {
		stats["season_type"] = "postseason"
	} else {
		stats["season_type"] = "regular"
	}

	rawJSON, _ := json.Marshal(item)

	return provider.PlayerStats{
		PlayerID: player.ID,
		TeamID:   player.TeamID,
		Player:   &player,
		Stats:    stats,
		Raw:      rawJSON,
	}, nil
}

// --------------------------------------------------------------------------
// Team Stats (via /standings endpoint)
// --------------------------------------------------------------------------

type bdlNFLStandingRaw struct {
	Team              bdlNFLTeamRaw `json:"team"`
	Wins              int           `json:"wins"`
	Losses            int           `json:"losses"`
	Ties              int           `json:"ties"`
	PointsFor         int           `json:"points_for"`
	PointsAgainst     int           `json:"points_against"`
	PointDifferential int           `json:"point_differential"`
	Season            int           `json:"season"`
}

// GetTeamStats fetches NFL standings and normalizes to canonical team stats.
func (h *NFLHandler) GetTeamStats(ctx context.Context, season int, seasonType string) ([]provider.TeamStats, error) {
	params := url.Values{
		"season":   {strconv.Itoa(season)},
		"per_page": {"100"},
	}

	var all []provider.TeamStats

	for {
		resp, err := h.client.get(ctx, "/standings", params)
		if err != nil {
			return nil, fmt.Errorf("fetch NFL standings: %w", err)
		}

		var raw []bdlNFLStandingRaw
		if err := json.Unmarshal(resp.Data, &raw); err != nil {
			return nil, fmt.Errorf("decode NFL standings: %w", err)
		}

		for _, r := range raw {
			stats := map[string]interface{}{
				"wins":               float64(r.Wins),
				"losses":             float64(r.Losses),
				"ties":               float64(r.Ties),
				"points_for":         float64(r.PointsFor),
				"points_against":     float64(r.PointsAgainst),
				"point_differential": float64(r.PointDifferential),
				"season_type":        seasonType,
			}
			rawJSON, _ := json.Marshal(r)
			all = append(all, provider.TeamStats{
				TeamID: r.Team.ID,
				Stats:  stats,
				Raw:    rawJSON,
			})
		}

		if resp.Meta.NextCursor == nil {
			break
		}
		params.Set("cursor", strconv.Itoa(*resp.Meta.NextCursor))
	}

	return all, nil
}
