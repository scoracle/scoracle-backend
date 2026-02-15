package seed

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/go/internal/config"
	"github.com/albapepper/scoracle-data/go/internal/provider"
)

// UpsertTeam writes a canonical team to the teams table.
func UpsertTeam(ctx context.Context, pool *pgxpool.Pool, sport string, team provider.Team) error {
	meta, _ := json.Marshal(nonNilMap(team.Meta))
	_, err := pool.Exec(ctx, `
		INSERT INTO `+config.TeamsTable+` (
			id, sport, name, short_code, city, country, conference,
			division, venue_name, venue_capacity, founded, logo_url, meta
		) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
		ON CONFLICT (id, sport) DO UPDATE SET
			name = EXCLUDED.name,
			short_code = EXCLUDED.short_code,
			city = EXCLUDED.city,
			country = EXCLUDED.country,
			conference = EXCLUDED.conference,
			division = EXCLUDED.division,
			venue_name = EXCLUDED.venue_name,
			venue_capacity = EXCLUDED.venue_capacity,
			founded = EXCLUDED.founded,
			logo_url = EXCLUDED.logo_url,
			meta = EXCLUDED.meta,
			updated_at = NOW()`,
		team.ID, sport, team.Name, nilEmpty(team.ShortCode), nilEmpty(team.City),
		nilEmpty(team.Country), nilEmpty(team.Conference), nilEmpty(team.Division),
		nilEmpty(team.VenueName), team.VenueCapacity, team.Founded,
		nilEmpty(team.LogoURL), meta,
	)
	return err
}

// UpsertPlayer writes a canonical player to the players table.
func UpsertPlayer(ctx context.Context, pool *pgxpool.Pool, sport string, player provider.Player) error {
	meta, _ := json.Marshal(nonNilMap(player.Meta))
	_, err := pool.Exec(ctx, `
		INSERT INTO `+config.PlayersTable+` (
			id, sport, name, first_name, last_name, position,
			detailed_position, nationality, height, weight,
			date_of_birth, photo_url, team_id, meta
		) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
		ON CONFLICT (id, sport) DO UPDATE SET
			name = COALESCE(EXCLUDED.name, `+config.PlayersTable+`.name),
			first_name = COALESCE(EXCLUDED.first_name, `+config.PlayersTable+`.first_name),
			last_name = COALESCE(EXCLUDED.last_name, `+config.PlayersTable+`.last_name),
			position = COALESCE(EXCLUDED.position, `+config.PlayersTable+`.position),
			detailed_position = COALESCE(EXCLUDED.detailed_position, `+config.PlayersTable+`.detailed_position),
			nationality = COALESCE(EXCLUDED.nationality, `+config.PlayersTable+`.nationality),
			height = COALESCE(EXCLUDED.height, `+config.PlayersTable+`.height),
			weight = COALESCE(EXCLUDED.weight, `+config.PlayersTable+`.weight),
			date_of_birth = COALESCE(EXCLUDED.date_of_birth, `+config.PlayersTable+`.date_of_birth),
			photo_url = COALESCE(EXCLUDED.photo_url, `+config.PlayersTable+`.photo_url),
			team_id = COALESCE(EXCLUDED.team_id, `+config.PlayersTable+`.team_id),
			meta = COALESCE(EXCLUDED.meta, `+config.PlayersTable+`.meta),
			updated_at = NOW()`,
		player.ID, sport, player.Name, nilEmpty(player.FirstName), nilEmpty(player.LastName),
		nilEmpty(player.Position), nilEmpty(player.DetailedPosition), nilEmpty(player.Nationality),
		nilEmpty(player.Height), nilEmpty(player.Weight), nilEmpty(player.DateOfBirth),
		nilEmpty(player.PhotoURL), player.TeamID, meta,
	)
	return err
}

// UpsertPlayerStats writes canonical player stats to the player_stats table.
// Postgres triggers automatically compute derived stats on INSERT/UPDATE.
func UpsertPlayerStats(ctx context.Context, pool *pgxpool.Pool, sport string, season, leagueID int, data provider.PlayerStats) error {
	stats, _ := json.Marshal(nonNilMapI(data.Stats))
	raw := data.Raw
	if raw == nil {
		raw = []byte("{}")
	}
	_, err := pool.Exec(ctx, `
		INSERT INTO `+config.PlayerStatsTable+` (
			player_id, sport, season, league_id, team_id,
			stats, raw_response
		) VALUES ($1,$2,$3,$4,$5,$6,$7)
		ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
			team_id = EXCLUDED.team_id,
			stats = EXCLUDED.stats,
			raw_response = EXCLUDED.raw_response,
			updated_at = NOW()`,
		data.PlayerID, sport, season, leagueID, data.TeamID,
		stats, raw,
	)
	return err
}

// UpsertTeamStats writes canonical team stats to the team_stats table.
// Postgres triggers automatically compute derived stats on INSERT/UPDATE.
func UpsertTeamStats(ctx context.Context, pool *pgxpool.Pool, sport string, season, leagueID int, data provider.TeamStats) error {
	stats, _ := json.Marshal(nonNilMapI(data.Stats))
	raw := data.Raw
	if raw == nil {
		raw = []byte("{}")
	}
	_, err := pool.Exec(ctx, `
		INSERT INTO `+config.TeamStatsTable+` (
			team_id, sport, season, league_id,
			stats, raw_response
		) VALUES ($1,$2,$3,$4,$5,$6)
		ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
			stats = EXCLUDED.stats,
			raw_response = EXCLUDED.raw_response,
			updated_at = NOW()`,
		data.TeamID, sport, season, leagueID,
		stats, raw,
	)
	return err
}

// RecalculatePercentiles triggers the Postgres percentile calculation function.
func RecalculatePercentiles(ctx context.Context, pool *pgxpool.Pool, sport string, season int) (playersUpdated, teamsUpdated int, err error) {
	err = pool.QueryRow(ctx, "recalculate_percentiles", sport, season).Scan(&playersUpdated, &teamsUpdated)
	if err != nil {
		return 0, 0, fmt.Errorf("recalculate percentiles: %w", err)
	}
	return playersUpdated, teamsUpdated, nil
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

// nilEmpty returns nil for empty strings (maps to SQL NULL).
func nilEmpty(s string) interface{} {
	if s == "" {
		return nil
	}
	return s
}

// nonNilMap ensures a nil map becomes an empty map for JSON marshaling.
func nonNilMap(m map[string]interface{}) map[string]interface{} {
	if m == nil {
		return map[string]interface{}{}
	}
	return m
}

// nonNilMapI is the same as nonNilMap (Go uses the same type).
func nonNilMapI(m map[string]interface{}) map[string]interface{} {
	return nonNilMap(m)
}
