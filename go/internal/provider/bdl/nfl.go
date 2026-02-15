package bdl

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"net/url"
	"strconv"

	"github.com/albapepper/scoracle-data/go/internal/provider"
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
	ID                   int            `json:"id"`
	FirstName            string         `json:"first_name"`
	LastName             string         `json:"last_name"`
	Position             string         `json:"position"`
	PositionAbbreviation string         `json:"position_abbreviation"`
	Height               string         `json:"height"`
	Weight               string         `json:"weight"`
	Country              string         `json:"country"`
	Team                 *bdlNFLTeamRaw `json:"team"`
	JerseyNumber         *int           `json:"jersey_number"`
	College              string         `json:"college"`
	Experience           *int           `json:"experience"`
	Age                  *int           `json:"age"`
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
	if raw.JerseyNumber != nil {
		meta["jersey_number"] = *raw.JerseyNumber
	}
	if raw.College != "" {
		meta["college"] = raw.College
	}
	if raw.Experience != nil {
		meta["experience"] = *raw.Experience
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

type bdlNFLPlayerStatsRaw struct {
	Player bdlNFLPlayerRaw        `json:"player"`
	Stats  map[string]interface{} `json:"stats"`
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

		var raw []bdlNFLPlayerStatsRaw
		if err := json.Unmarshal(resp.Data, &raw); err != nil {
			return fmt.Errorf("decode NFL player stats: %w", err)
		}

		for _, r := range raw {
			ps := normalizeNFLPlayerStats(r, postseason)
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

func normalizeNFLPlayerStats(raw bdlNFLPlayerStatsRaw, postseason bool) provider.PlayerStats {
	player := normalizeNFLPlayer(raw.Player)
	stats := normalizeStatKeys(raw.Stats)
	if postseason {
		stats["season_type"] = "postseason"
	} else {
		stats["season_type"] = "regular"
	}

	rawJSON, _ := json.Marshal(raw)

	return provider.PlayerStats{
		PlayerID: player.ID,
		TeamID:   player.TeamID,
		Player:   &player,
		Stats:    stats,
		Raw:      rawJSON,
	}
}

// --------------------------------------------------------------------------
// Team Stats (cursor-paginated)
// --------------------------------------------------------------------------

type bdlNFLTeamStatsRaw struct {
	Team  bdlNFLTeamRaw          `json:"team"`
	Stats map[string]interface{} `json:"stats"`
}

// GetTeamStats fetches all team season averages in canonical format.
func (h *NFLHandler) GetTeamStats(ctx context.Context, season int, seasonType string) ([]provider.TeamStats, error) {
	params := url.Values{
		"season":      {strconv.Itoa(season)},
		"season_type": {seasonType},
		"per_page":    {"100"},
	}

	var all []provider.TeamStats

	for {
		resp, err := h.client.get(ctx, "/team_season_averages/general", params)
		if err != nil {
			return nil, fmt.Errorf("fetch NFL team stats: %w", err)
		}

		var raw []bdlNFLTeamStatsRaw
		if err := json.Unmarshal(resp.Data, &raw); err != nil {
			return nil, fmt.Errorf("decode NFL team stats: %w", err)
		}

		for _, r := range raw {
			stats := normalizeStatKeys(r.Stats)
			stats["season_type"] = seasonType
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
