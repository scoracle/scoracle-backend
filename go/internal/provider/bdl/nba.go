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

const nbaBaseURL = "https://api.balldontlie.io/v1"

// NBAHandler fetches and normalizes NBA data from BallDontLie.
type NBAHandler struct {
	client *Client
	logger *slog.Logger
}

// NewNBAHandler creates an NBA handler with the given API key.
func NewNBAHandler(apiKey string, logger *slog.Logger) *NBAHandler {
	return &NBAHandler{
		client: NewClient(nbaBaseURL, apiKey, 600, logger),
		logger: logger,
	}
}

// --------------------------------------------------------------------------
// Teams
// --------------------------------------------------------------------------

type bdlTeamRaw struct {
	ID           int    `json:"id"`
	Name         string `json:"name"`
	FullName     string `json:"full_name"`
	Abbreviation string `json:"abbreviation"`
	City         string `json:"city"`
	Conference   string `json:"conference"`
	Division     string `json:"division"`
}

// GetTeams fetches all NBA teams in canonical format.
func (h *NBAHandler) GetTeams(ctx context.Context) ([]provider.Team, error) {
	resp, err := h.client.get(ctx, "/teams", nil)
	if err != nil {
		return nil, fmt.Errorf("fetch NBA teams: %w", err)
	}

	var raw []bdlTeamRaw
	if err := json.Unmarshal(resp.Data, &raw); err != nil {
		return nil, fmt.Errorf("decode NBA teams: %w", err)
	}

	teams := make([]provider.Team, len(raw))
	for i, t := range raw {
		teams[i] = normalizeNBATeam(t)
	}
	return teams, nil
}

func normalizeNBATeam(raw bdlTeamRaw) provider.Team {
	meta := make(map[string]interface{})
	if raw.FullName != "" {
		meta["full_name"] = raw.FullName
	}
	return provider.Team{
		ID:         raw.ID,
		Name:       raw.Name,
		ShortCode:  raw.Abbreviation,
		City:       raw.City,
		Conference: raw.Conference,
		Division:   raw.Division,
		Meta:       meta,
	}
}

// --------------------------------------------------------------------------
// Players (cursor-paginated)
// --------------------------------------------------------------------------

type bdlPlayerRaw struct {
	ID        int         `json:"id"`
	FirstName string      `json:"first_name"`
	LastName  string      `json:"last_name"`
	Position  string      `json:"position"`
	Height    string      `json:"height"`
	Weight    string      `json:"weight"`
	Country   string      `json:"country"`
	Team      *bdlTeamRaw `json:"team"`
	// Meta fields
	JerseyNumber json.RawMessage `json:"jersey_number"`
	College      string          `json:"college"`
	DraftYear    *int            `json:"draft_year"`
	DraftRound   *int            `json:"draft_round"`
	DraftNumber  *int            `json:"draft_number"`
}

// GetPlayers iterates all NBA players via cursor pagination, calling fn for each.
func (h *NBAHandler) GetPlayers(ctx context.Context, fn func(provider.Player) error) error {
	params := url.Values{"per_page": {"100"}}

	for {
		resp, err := h.client.get(ctx, "/players", params)
		if err != nil {
			return fmt.Errorf("fetch NBA players: %w", err)
		}

		var raw []bdlPlayerRaw
		if err := json.Unmarshal(resp.Data, &raw); err != nil {
			return fmt.Errorf("decode NBA players: %w", err)
		}

		for _, p := range raw {
			if err := fn(normalizeNBAPlayer(p)); err != nil {
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

func normalizeNBAPlayer(raw bdlPlayerRaw) provider.Player {
	name := (raw.FirstName + " " + raw.LastName)
	if name == " " {
		name = fmt.Sprintf("Player %d", raw.ID)
	}

	meta := make(map[string]interface{})
	if raw.JerseyNumber != nil && string(raw.JerseyNumber) != "null" {
		meta["jersey_number"] = json.RawMessage(raw.JerseyNumber)
	}
	if raw.College != "" {
		meta["college"] = raw.College
	}
	if raw.DraftYear != nil {
		meta["draft_year"] = *raw.DraftYear
	}
	if raw.DraftRound != nil {
		meta["draft_round"] = *raw.DraftRound
	}
	if raw.DraftNumber != nil {
		meta["draft_number"] = *raw.DraftNumber
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
// Player Stats (cursor-paginated season averages)
// --------------------------------------------------------------------------

type bdlPlayerStatsRaw struct {
	Player bdlPlayerRaw           `json:"player"`
	Stats  map[string]interface{} `json:"stats"`
}

// GetPlayerStats iterates all player season averages, calling fn for each.
func (h *NBAHandler) GetPlayerStats(ctx context.Context, season int, seasonType string, fn func(provider.PlayerStats) error) error {
	params := url.Values{
		"season":      {strconv.Itoa(season)},
		"season_type": {seasonType},
		"type":        {"base"},
		"per_page":    {"100"},
	}

	for {
		resp, err := h.client.get(ctx, "/season_averages/general", params)
		if err != nil {
			return fmt.Errorf("fetch NBA player stats: %w", err)
		}

		var raw []bdlPlayerStatsRaw
		if err := json.Unmarshal(resp.Data, &raw); err != nil {
			return fmt.Errorf("decode NBA player stats: %w", err)
		}

		for _, r := range raw {
			ps := normalizeNBAPlayerStats(r, seasonType)
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

func normalizeNBAPlayerStats(raw bdlPlayerStatsRaw, seasonType string) provider.PlayerStats {
	player := normalizeNBAPlayer(raw.Player)
	stats := normalizeStatKeys(raw.Stats)
	stats["season_type"] = seasonType

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
// Team Stats (cursor-paginated season averages)
// --------------------------------------------------------------------------

type bdlTeamStatsRaw struct {
	Team  bdlTeamRaw             `json:"team"`
	Stats map[string]interface{} `json:"stats"`
}

// GetTeamStats fetches all team season averages in canonical format.
func (h *NBAHandler) GetTeamStats(ctx context.Context, season int, seasonType string) ([]provider.TeamStats, error) {
	params := url.Values{
		"season":      {strconv.Itoa(season)},
		"season_type": {seasonType},
		"type":        {"base"},
		"per_page":    {"100"},
	}

	var all []provider.TeamStats

	for {
		resp, err := h.client.get(ctx, "/team_season_averages/general", params)
		if err != nil {
			return nil, fmt.Errorf("fetch NBA team stats: %w", err)
		}

		var raw []bdlTeamStatsRaw
		if err := json.Unmarshal(resp.Data, &raw); err != nil {
			return nil, fmt.Errorf("decode NBA team stats: %w", err)
		}

		for _, r := range raw {
			ts := normalizeNBATeamStats(r, seasonType)
			all = append(all, ts)
		}

		if resp.Meta.NextCursor == nil {
			break
		}
		params.Set("cursor", strconv.Itoa(*resp.Meta.NextCursor))
	}

	return all, nil
}

func normalizeNBATeamStats(raw bdlTeamStatsRaw, seasonType string) provider.TeamStats {
	stats := normalizeStatKeys(raw.Stats)
	stats["season_type"] = seasonType

	rawJSON, _ := json.Marshal(raw)

	return provider.TeamStats{
		TeamID: raw.Team.ID,
		Stats:  stats,
		Raw:    rawJSON,
	}
}

// --------------------------------------------------------------------------
// Shared stat key normalization
// --------------------------------------------------------------------------

// normalizeStatKeys renames BDL stat keys to match our canonical names.
// Filters out nil values.
func normalizeStatKeys(stats map[string]interface{}) map[string]interface{} {
	out := make(map[string]interface{}, len(stats))
	for k, v := range stats {
		if v == nil {
			continue
		}
		switch k {
		case "tov":
			out["turnover"] = v
		case "w":
			out["wins"] = v
		case "l":
			out["losses"] = v
		case "gp":
			out["games_played"] = v
		default:
			out[k] = v
		}
	}
	return out
}
