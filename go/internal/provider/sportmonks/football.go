package sportmonks

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"math"
	"net/url"
	"sort"
	"strconv"
	"strings"

	"github.com/albapepper/scoracle-data/go/internal/provider"
)

// FootballHandler fetches and normalizes Football data from SportMonks.
type FootballHandler struct {
	client *Client
	logger *slog.Logger
}

// NewFootballHandler creates a Football handler.
func NewFootballHandler(apiToken string, logger *slog.Logger) *FootballHandler {
	return &FootballHandler{
		client: NewClient(apiToken, 300, logger),
		logger: logger,
	}
}

// --------------------------------------------------------------------------
// Code override maps — SportMonks codes that don't match our canonical keys
// after simple hyphen-to-underscore replacement.
// --------------------------------------------------------------------------

var playerCodeOverrides = map[string]string{
	"passes":              "passes_total",
	"accurate-passes":     "passes_accurate",
	"total-crosses":       "crosses_total",
	"accurate-crosses":    "crosses_accurate",
	"blocked-shots":       "blocks",
	"total-duels":         "duels_total",
	"dribble-attempts":    "dribbles_attempts",
	"successful-dribbles": "dribbles_success",
	"yellowcards":         "yellow_cards",
	"redcards":            "red_cards",
	"fouls":               "fouls_committed",
	"expected-goals":      "expected_goals",
}

var standingCodeOverrides = map[string]string{
	"overall-matches-played": "matches_played",
	"overall-won":            "wins",
	"overall-draw":           "draws",
	"overall-lost":           "losses",
	"overall-goals-for":      "goals_for",
	"overall-goals-against":  "goals_against",
	"home-matches-played":    "home_played",
	"away-matches-played":    "away_played",
}

func normalizeCode(code string, overrides map[string]string) string {
	if mapped, ok := overrides[code]; ok {
		return mapped
	}
	return strings.ReplaceAll(code, "-", "_")
}

// --------------------------------------------------------------------------
// Seasons
// --------------------------------------------------------------------------

type smSeason struct {
	ID   int    `json:"id"`
	Name string `json:"name"`
}

// DiscoverSeasonIDs maps target years to SportMonks season IDs for a league.
func (h *FootballHandler) DiscoverSeasonIDs(ctx context.Context, leagueID int, targetYears []int) (map[int]int, error) {
	resp, err := h.client.get(ctx, fmt.Sprintf("/leagues/%d", leagueID), url.Values{
		"include": {"seasons"},
	})
	if err != nil {
		return nil, fmt.Errorf("fetch league seasons: %w", err)
	}

	var leagueData struct {
		Seasons []smSeason `json:"seasons"`
	}
	if err := json.Unmarshal(resp.Data, &leagueData); err != nil {
		return nil, fmt.Errorf("decode league seasons: %w", err)
	}

	// Build target year set
	targetSet := make(map[int]bool, len(targetYears))
	for _, y := range targetYears {
		targetSet[y] = true
	}

	result := make(map[int]int)
	for _, season := range leagueData.Seasons {
		parts := strings.Split(season.Name, "/")
		startYearStr := strings.TrimSpace(parts[0])
		startYear, err := strconv.Atoi(startYearStr)
		if err != nil {
			continue
		}
		if targetSet[startYear] {
			if _, exists := result[startYear]; !exists {
				result[startYear] = season.ID
			}
		}
	}

	return result, nil
}

// --------------------------------------------------------------------------
// Teams
// --------------------------------------------------------------------------

type smTeamRaw struct {
	ID        int    `json:"id"`
	Name      string `json:"name"`
	ShortCode string `json:"short_code"`
	Founded   *int   `json:"founded"`
	ImagePath string `json:"image_path"`
	Country   *struct {
		Name string `json:"name"`
	} `json:"country"`
	Venue *struct {
		Name     string `json:"name"`
		Capacity *int   `json:"capacity"`
		City     string `json:"city"`
		Surface  string `json:"surface"`
	} `json:"venue"`
}

// GetTeams fetches all teams for a season in canonical format.
func (h *FootballHandler) GetTeams(ctx context.Context, seasonID int) ([]provider.Team, error) {
	rawItems, err := h.client.getPaginated(ctx,
		fmt.Sprintf("/teams/seasons/%d", seasonID),
		url.Values{"include": {"venue;country"}}, 50)
	if err != nil {
		return nil, fmt.Errorf("fetch football teams: %w", err)
	}

	teams := make([]provider.Team, 0, len(rawItems))
	for _, raw := range rawItems {
		var t smTeamRaw
		if err := json.Unmarshal(raw, &t); err != nil {
			h.logger.Warn("decode team", "error", err)
			continue
		}
		teams = append(teams, normalizeTeam(t))
	}
	return teams, nil
}

func normalizeTeam(raw smTeamRaw) provider.Team {
	team := provider.Team{
		ID:        raw.ID,
		Name:      raw.Name,
		ShortCode: raw.ShortCode,
		LogoURL:   raw.ImagePath,
		Founded:   raw.Founded,
		Meta:      make(map[string]interface{}),
	}

	if raw.Country != nil {
		team.Country = raw.Country.Name
	}
	if raw.Venue != nil {
		team.VenueName = raw.Venue.Name
		team.VenueCapacity = raw.Venue.Capacity
		if raw.Venue.City != "" {
			team.Meta["venue_city"] = raw.Venue.City
		}
		if raw.Venue.Surface != "" {
			team.Meta["venue_surface"] = raw.Venue.Surface
		}
	}

	return team
}

// --------------------------------------------------------------------------
// Players + Stats (fetched together via squad iteration)
// --------------------------------------------------------------------------

type smPlayerRaw struct {
	ID               int         `json:"id"`
	Firstname        string      `json:"firstname"`
	Lastname         string      `json:"lastname"`
	DisplayName      string      `json:"display_name"`
	PositionID       *int        `json:"position_id"`
	Position         interface{} `json:"position"`
	DateOfBirth      string      `json:"date_of_birth"`
	Height           *float64    `json:"height"` // cm
	Weight           *float64    `json:"weight"` // kg
	ImagePath        string      `json:"image_path"`
	Nationality      interface{} `json:"nationality"` // string or object
	DetailedPosition interface{} `json:"detailedposition"`
	Statistics       []struct {
		Details []struct {
			Type  *struct{ Code string } `json:"type"`
			Value interface{}            `json:"value"`
		} `json:"details"`
		Season *struct {
			League *struct{ ID int } `json:"league"`
		} `json:"season"`
	} `json:"statistics"`
}

// GetPlayersWithStats iterates squads, fetches per-player stats, and calls fn.
func (h *FootballHandler) GetPlayersWithStats(ctx context.Context, seasonID int, teamIDs []int, smLeagueID int, fn func(provider.PlayerStats) error) error {
	for i, teamID := range teamIDs {
		h.logger.Info("Fetching squad", "team_id", teamID, "progress", fmt.Sprintf("%d/%d", i+1, len(teamIDs)))

		resp, err := h.client.get(ctx,
			fmt.Sprintf("/squads/seasons/%d/teams/%d", seasonID, teamID), nil)
		if err != nil {
			h.logger.Warn("squad fetch failed", "team_id", teamID, "error", err)
			continue
		}

		var squad []struct {
			PlayerID int `json:"player_id"`
			ID       int `json:"id"`
		}
		if err := json.Unmarshal(resp.Data, &squad); err != nil {
			h.logger.Warn("squad decode failed", "team_id", teamID, "error", err)
			continue
		}

		playerIDs := make([]int, 0, len(squad))
		for _, entry := range squad {
			pid := entry.PlayerID
			if pid == 0 {
				pid = entry.ID
			}
			if pid != 0 {
				playerIDs = append(playerIDs, pid)
			}
		}

		for j, pid := range playerIDs {
			playerResp, err := h.client.get(ctx, fmt.Sprintf("/players/%d", pid), url.Values{
				"include": {"statistics.details.type;statistics.season.league;nationality;detailedPosition"},
				"filters": {fmt.Sprintf("playerStatisticSeasons:%d", seasonID)},
			})
			if err != nil {
				h.logger.Warn("player fetch failed", "player_id", pid, "error", err)
				continue
			}

			var playerData smPlayerRaw
			if err := json.Unmarshal(playerResp.Data, &playerData); err != nil {
				h.logger.Warn("player decode failed", "player_id", pid, "error", err)
				continue
			}

			stats := extractLeagueStats(playerData.Statistics, smLeagueID)
			player := normalizePlayer(playerData)

			rawJSON, _ := json.Marshal(playerData)

			if err := fn(provider.PlayerStats{
				PlayerID: playerData.ID,
				TeamID:   &teamID,
				Player:   &player,
				Stats:    stats,
				Raw:      rawJSON,
			}); err != nil {
				return err
			}

			if (j+1)%10 == 0 {
				h.logger.Info("Player progress", "team_id", teamID, "count", j+1, "total", len(playerIDs))
			}
		}
	}
	return nil
}

func extractLeagueStats(statistics []struct {
	Details []struct {
		Type  *struct{ Code string } `json:"type"`
		Value interface{}            `json:"value"`
	} `json:"details"`
	Season *struct {
		League *struct{ ID int } `json:"league"`
	} `json:"season"`
}, smLeagueID int) map[string]interface{} {
	for _, block := range statistics {
		if block.Season == nil || block.Season.League == nil {
			continue
		}
		if block.Season.League.ID == smLeagueID {
			return normalizePlayerStats(block.Details)
		}
	}
	return map[string]interface{}{}
}

func normalizePlayerStats(details []struct {
	Type  *struct{ Code string } `json:"type"`
	Value interface{}            `json:"value"`
}) map[string]interface{} {
	stats := make(map[string]interface{})
	for _, detail := range details {
		if detail.Type == nil || detail.Type.Code == "" {
			continue
		}
		key := normalizeCode(detail.Type.Code, playerCodeOverrides)
		if val, ok := provider.ExtractValue(detail.Value); ok {
			stats[key] = val
		}
	}
	return stats
}

func normalizePlayer(raw smPlayerRaw) provider.Player {
	name := raw.DisplayName
	if name == "" {
		name = strings.TrimSpace(raw.Firstname + " " + raw.Lastname)
	}
	if name == "" {
		name = fmt.Sprintf("Player %d", raw.ID)
	}

	// Position from position_id
	var position string
	switch v := raw.Position.(type) {
	case string:
		position = v
	}
	if position == "" && raw.PositionID != nil {
		posMap := map[int]string{24: "Goalkeeper", 25: "Defender", 26: "Midfielder", 27: "Attacker"}
		position = posMap[*raw.PositionID]
	}

	// Detailed position
	var detailedPosition string
	switch v := raw.DetailedPosition.(type) {
	case map[string]interface{}:
		if n, ok := v["name"].(string); ok {
			detailedPosition = n
		}
	case string:
		detailedPosition = v
	}

	// Nationality
	var nationality string
	switch v := raw.Nationality.(type) {
	case map[string]interface{}:
		if n, ok := v["name"].(string); ok {
			nationality = n
		}
	case string:
		nationality = v
	}

	// Height: cm -> feet-inches
	var height string
	if raw.Height != nil && *raw.Height > 0 {
		height = cmToFeetInches(*raw.Height)
	}

	// Weight: kg -> lbs
	var weight string
	if raw.Weight != nil && *raw.Weight > 0 {
		weight = strconv.Itoa(int(math.Round(*raw.Weight * 2.20462)))
	}

	meta := make(map[string]interface{})
	if raw.DisplayName != "" {
		meta["display_name"] = raw.DisplayName
	}
	if raw.PositionID != nil {
		meta["position_id"] = *raw.PositionID
	}

	return provider.Player{
		ID:               raw.ID,
		Name:             name,
		FirstName:        raw.Firstname,
		LastName:         raw.Lastname,
		Position:         position,
		DetailedPosition: detailedPosition,
		Nationality:      nationality,
		Height:           height,
		Weight:           weight,
		DateOfBirth:      raw.DateOfBirth,
		PhotoURL:         raw.ImagePath,
		Meta:             meta,
	}
}

func cmToFeetInches(cm float64) string {
	totalInches := cm / 2.54
	if totalInches <= 0 {
		return ""
	}
	feet := int(totalInches / 12)
	inches := int(math.Round(math.Mod(totalInches, 12)))
	if inches == 12 {
		feet++
		inches = 0
	}
	return fmt.Sprintf("%d-%d", feet, inches)
}

// --------------------------------------------------------------------------
// Team Stats (Standings)
// --------------------------------------------------------------------------

type smStandingRaw struct {
	ParticipantID int             `json:"participant_id"`
	Participant   json.RawMessage `json:"participant"`
	Points        *int            `json:"points"`
	Position      *int            `json:"position"`
	Form          string          `json:"form"`
	Details       []struct {
		Type  *struct{ Code string } `json:"type"`
		Value interface{}            `json:"value"`
	} `json:"details"`
}

// GetTeamStats fetches standings for a season in canonical format.
func (h *FootballHandler) GetTeamStats(ctx context.Context, seasonID int) ([]provider.TeamStats, error) {
	resp, err := h.client.get(ctx,
		fmt.Sprintf("/standings/seasons/%d", seasonID),
		url.Values{"include": {"participant;details.type"}})
	if err != nil {
		return nil, fmt.Errorf("fetch football standings: %w", err)
	}

	var raw []smStandingRaw
	if err := json.Unmarshal(resp.Data, &raw); err != nil {
		return nil, fmt.Errorf("decode standings: %w", err)
	}

	result := make([]provider.TeamStats, 0, len(raw))
	for _, standing := range raw {
		ts := normalizeStanding(standing)
		result = append(result, ts)
	}

	// Sort by position
	sort.Slice(result, func(i, j int) bool {
		pi, _ := result[i].Stats["position"].(float64)
		pj, _ := result[j].Stats["position"].(float64)
		return pi < pj
	})

	return result, nil
}

func normalizeStanding(raw smStandingRaw) provider.TeamStats {
	stats := make(map[string]interface{})

	for _, detail := range raw.Details {
		if detail.Type == nil || detail.Type.Code == "" {
			continue
		}
		key := normalizeCode(detail.Type.Code, standingCodeOverrides)
		if val, ok := provider.ExtractValue(detail.Value); ok {
			stats[key] = val
		}
	}

	if raw.Points != nil {
		stats["points"] = float64(*raw.Points)
	}
	if raw.Position != nil {
		stats["position"] = float64(*raw.Position)
	}
	if raw.Form != "" {
		stats["form"] = raw.Form
	}

	// Try to parse the participant for team data
	var team *provider.Team
	if raw.Participant != nil {
		var t smTeamRaw
		if err := json.Unmarshal(raw.Participant, &t); err == nil && t.ID != 0 {
			normalized := normalizeTeam(t)
			team = &normalized
		}
	}

	teamID := raw.ParticipantID
	if team != nil && teamID == 0 {
		teamID = team.ID
	}

	rawJSON, _ := json.Marshal(raw)

	return provider.TeamStats{
		TeamID: teamID,
		Team:   team,
		Stats:  stats,
		Raw:    rawJSON,
	}
}
