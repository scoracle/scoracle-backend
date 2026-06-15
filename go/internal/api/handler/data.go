package handler

import (
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/jackc/pgx/v5"

	"github.com/albapepper/scoracle-data/internal/api/respond"
	"github.com/albapepper/scoracle-data/internal/cache"
)

var validSports = map[string]struct{}{
	"nba":      {},
	"nfl":      {},
	"football": {},
}

var validEntityTypes = map[string]struct{}{
	"player": {},
	"team":   {},
}

// GetProfilePage returns a canonical profile payload for a sport entity.
// @Summary Get profile page
// @Description Returns the sport profile payload for an entity type and ID.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param season query int false "Season year"
// @Param league_id query int false "League ID filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/{entityType}/{id} [get]
func (h *Handler) GetProfilePage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}

	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}

	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}

	stmt := sport + "_profile_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, true, entityType, id, season, leagueID)
}

// GetLeagueProfilePage returns a league-scoped profile payload for a sport entity.
// @Summary Get league profile page
// @Description Returns the sport profile payload scoped to a specific league.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param leagueId path int true "League ID"
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param season query int false "Season year"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leagues/{leagueId}/{entityType}/{id} [get]
func (h *Handler) GetLeagueProfilePage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}

	leagueID, ok := parsePathID(w, r, "leagueId", "league id")
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}

	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}

	stmt := sport + "_profile_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, true, entityType, id, season, leagueID)
}

// GetLeaderboard returns the positionless rating leaderboard for a sport.
// Each row carries BOTH the Composite and Specialist score (+ the specialty label),
// so a single payload feeds the leaderboard board, the meta card, and the sparkline.
// @Summary Get rating leaderboard
// @Description Positionless rating board (z-score engine). entity_type=player (default) or team. Composite + Specialist in one payload.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entity_type query string false "Board type: player (default) or team"
// @Param scope query string false "Board: composite (default), specialist, or a specialty label (e.g. Sacks)"
// @Param season query int false "Season year (defaults to the latest rated season)"
// @Param position query string false "Filter to a position (player boards only)"
// @Param league_id query int false "Filter to a league (football)"
// @Param limit query int false "Max rows (default 50)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leaderboard [get]
func (h *Handler) GetLeaderboard(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	// Two-rail consolidation: one endpoint, many boards via ?board=. Delegate the
	// non-rating boards to their handlers; fall through to the rating board (the
	// default) below. The dedicated /leaderboard/{board} routes remain for now.
	switch strings.ToLower(strings.TrimSpace(r.URL.Query().Get("board"))) {
	case "", "rating":
		// rating board — handled below
	case "vibes":
		h.GetVibesLeaderboard(w, r)
		return
	case "news":
		h.GetNewsLeaderboard(w, r)
		return
	case "transfers":
		h.GetTransfersLeaderboard(w, r)
		return
	default:
		respond.WriteError(w, http.StatusBadRequest, "INVALID_BOARD", "board must be one of: rating, vibes, news, transfers")
		return
	}

	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}

	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}

	limit, ok := optionalIntQuery(w, r, "limit")
	if !ok {
		return
	}

	scope := optionalTextQuery(r, "scope")
	position := optionalTextQuery(r, "position")
	entityType := optionalTextQuery(r, "entity_type")
	conference := optionalTextQuery(r, "conference")
	division := optionalTextQuery(r, "division")

	h.serveStatementJSON(w, r, "leaderboard", dataCacheKey(r), cache.TTLData, false,
		sport, season, scope, position, leagueID, limit, entityType, conference, division)
}

// GetVibesLeaderboard returns the sport-wide vibe board: entities ranked by
// their latest sentiment score (1-100) in the last 48h, enriched with name/image/team.
// @Summary Vibes leaderboard
// @Description Sport-wide board of entities ranked by latest vibe sentiment (1-100). Enriched sibling of /vibe/hottest.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entity_type query string false "Filter: player or team (default both)"
// @Param limit query int false "Max rows (default 50)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leaderboard/vibes [get]
func (h *Handler) GetVibesLeaderboard(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	limit, ok := optionalIntQuery(w, r, "limit")
	if !ok {
		return
	}

	entityType := optionalTextQuery(r, "entity_type")

	h.serveStatementJSON(w, r, "vibes_leaderboard", dataCacheKey(r), cache.TTLData, false,
		sport, limit, entityType)
}

// GetNewsLeaderboard returns the sport-wide news board: entities ranked by the
// number of distinct article mentions in the rolling window.
// @Summary News leaderboard
// @Description Sport-wide board of the most-mentioned entities in the news corpus over the rolling window.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entity_type query string false "Filter: player or team (default both)"
// @Param days query int false "Rolling window in days (default 30)"
// @Param limit query int false "Max rows (default 50)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leaderboard/news [get]
func (h *Handler) GetNewsLeaderboard(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	limit, ok := optionalIntQuery(w, r, "limit")
	if !ok {
		return
	}

	entityType := optionalTextQuery(r, "entity_type")

	// Two-rail model: the news board is now the hottest NARRATIVES (ranked by
	// per-narrative impact), not raw mention counts. narratives_leaderboard
	// supersedes the old news_leaderboard (which read news_article_entities).
	h.serveStatementJSON(w, r, "narratives_leaderboard", dataCacheKey(r), cache.TTLData, false,
		sport, limit, entityType)
}

// GetTransfersLeaderboard returns the sport-wide transfer board: Gemma-vetted
// (team, player) rumors ranked by heat (0-100), enriched with both sides of the pair.
// @Summary Transfers leaderboard
// @Description Sport-wide board of the hottest Gemma-vetted transfer rumors, ranked by heat (0-100).
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param limit query int false "Max rows (default 50)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leaderboard/transfers [get]
func (h *Handler) GetTransfersLeaderboard(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	limit, ok := optionalIntQuery(w, r, "limit")
	if !ok {
		return
	}

	h.serveStatementJSON(w, r, "transfers_leaderboard", dataCacheKey(r), cache.TTLData, false,
		sport, limit)
}

// GetSparkline returns the rating-engine dataset for one entity: the season
// Composite + Specialist (+ specialty + ranks + that season's team) and the
// per-event dual-sparkline series. This is the dedicated rating dataset — kept
// separate from the profile and meta payloads. Served at /sparkline (the legacy
// /starline path remains a deprecated alias during the frontend rollout).
// @Summary Get entity sparkline (rating)
// @Description Season Composite/Specialist rating (+ season team) + per-event sparkline series for one entity.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param season query int false "Season year (defaults to the latest rated season)"
// @Param league_id query int false "League ID filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/{entityType}/{id}/sparkline [get]
func (h *Handler) GetSparkline(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}

	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}

	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}

	h.serveStatementJSON(w, r, "sparkline", dataCacheKey(r), cache.TTLData, false,
		sport, entityType, id, season, leagueID)
}

// GetTrendsPage returns last-3 entity event averages vs peer-cohort season averages.
// @Summary Get trends page
// @Description Returns the entity's last-3 event averages and peer-cohort season averages so the frontend can derive recent direction relative to peers. Raw values only.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param season query int false "Season year"
// @Param league_id query int false "League ID filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/{entityType}/{id}/trends [get]
func (h *Handler) GetTrendsPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}

	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}

	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}

	stmt := sport + "_trends_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false, entityType, id, season, leagueID)
}

// GetLeagueTrendsPage returns league-scoped last-3 entity event averages vs peer-cohort season averages.
// @Summary Get league trends page
// @Description League-scoped trends payload. Mirrors GetTrendsPage with leagueId taken from the URL path.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param leagueId path int true "League ID"
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param season query int false "Season year"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leagues/{leagueId}/{entityType}/{id}/trends [get]
func (h *Handler) GetLeagueTrendsPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}

	leagueID, ok := parsePathID(w, r, "leagueId", "league id")
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}

	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}

	stmt := sport + "_trends_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false, entityType, id, season, leagueID)
}

// GetTeamResults returns a team's finalized scorelines for a season.
// @Summary Get team season results
// @Description Returns the team's list of finalized fixtures (status completed/seeded) for a season, framed from the team's perspective with opponent identity and W/L/D.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param id path int true "Team ID"
// @Param season query int false "Season year"
// @Param league_id query int false "League ID filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/team/{id}/results [get]
func (h *Handler) GetTeamResults(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "team id")
	if !ok {
		return
	}

	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}

	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}

	stmt := sport + "_team_results"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false, id, season, leagueID)
}

// GetRoster returns a team's roster with each player's season Composite +
// Specialist rating, ranked by the sum of the two.
// @Summary Get team roster (rating)
// @Description Players on the team's season roster with season Composite/Specialist (+ ranks + specialty), ordered by the Composite+Specialist sum. Player names link to the player profile.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param id path int true "Team ID"
// @Param season query int false "Season year"
// @Param league_id query int false "League ID filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/team/{id}/roster [get]
func (h *Handler) GetRoster(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "team id")
	if !ok {
		return
	}

	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}

	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}

	h.serveStatementJSON(w, r, "roster", dataCacheKey(r), cache.TTLData, false, sport, id, season, leagueID)
}

// GetTransfers + GetPlayerSuitors retired 2026-06-15 — the transfer scope folded
// into the news rail (GetEntityNewsRail); their team_transfers/player_suitors
// prepared statements are removed too.

// GetEntityNewsRail returns the entity's NEWS RAIL in one payload (the two-rail
// model): the latest Gemma narratives (hottest first), the transfer scope (a
// team's player rumors / a player's suitor clubs), and the vibe (current + a
// bounded history for the sparkline). Consolidates the split /news + /vibe +
// /transfers reads — cards render slices of this rail.
// @Summary Get the entity news rail (narratives + transfers + vibe)
// @Description One payload for the news rail: the entity's latest narratives (hottest first), its transfer scope (team→player rumors, player→suitor clubs), and its vibe sentiment (current + history for the sparkline).
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/{entityType}/{id}/news [get]
func (h *Handler) GetEntityNewsRail(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}

	h.serveStatementJSON(w, r, "entity_news_rail", dataCacheKey(r), cache.TTLNews, false, sport, entityType, id)
}

// GetEntityNarratives returns the entity's NEWS product — the latest Gemma
// narratives (hottest first). Per-product split of the old news rail; transfers +
// vibe are their own products now (/transfers, /vibes). Narratives already carry
// transfer context (news is a post-transfers pipeline layer), so this is
// narratives-only. (Named to avoid the older GetEntityNews raw-articles handler.)
// @Summary Get the entity news narratives
// @Description The entity's latest Gemma narratives (hottest first) ranked by per-narrative impact.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/{entityType}/{id}/news [get]
func (h *Handler) GetEntityNarratives(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}
	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}
	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}
	h.serveStatementJSON(w, r, "entity_news", dataCacheKey(r), cache.TTLNews, false, sport, entityType, id)
}

// GetEntityTransfers returns the entity's TRANSFERS product — the vetted rumor
// heat list (pre-narrative data). For a team: linked players; for a player:
// interested clubs. The counterparty is the OTHER entity type.
// @Summary Get the entity transfer rumors
// @Description The entity's vetted transfer/trade rumors ranked by deterministic heat (team→players, player→clubs).
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/{entityType}/{id}/transfers [get]
func (h *Handler) GetEntityTransfers(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}
	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}
	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}
	h.serveStatementJSON(w, r, "entity_transfers", dataCacheKey(r), cache.TTLNews, false, sport, entityType, id)
}

// GetEntityVibes returns the entity's VIBES product — the current sentiment plus a
// bounded history (for the trend sparkline). The single vibe product, read by the
// Vibe card AND the meta corner score.
// @Summary Get the entity vibe sentiment
// @Description The entity's current vibe sentiment (1-100) plus a bounded history for the trend sparkline.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/{entityType}/{id}/vibes [get]
func (h *Handler) GetEntityVibes(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}
	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}
	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}
	h.serveStatementJSON(w, r, "entity_vibes", dataCacheKey(r), cache.TTLNews, false, sport, entityType, id)
}

// GetEntityStats returns the entity's STATS product — the full season rating
// (Composite breakdown, modes, fantasy, template, scoped ranks) + available
// seasons. The Composite card + the ContentShell control strip read this. The
// per-event series moved to /trends; the commentary to /special.
// @Summary Get the entity season rating
// @Description The entity's season Composite rating (breakdown, modes, fantasy, scoped ranks) + available seasons.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param season query int false "Season year"
// @Param league_id query int false "League ID filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/{entityType}/{id}/stats [get]
func (h *Handler) GetEntityStats(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}
	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}
	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}
	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}
	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}
	h.serveStatementJSON(w, r, "entity_stats", dataCacheKey(r), cache.TTLData, false,
		sport, entityType, id, season, leagueID)
}

// GetEntitySpecial returns the entity's SPECIAL product — the lean specialist
// projection (peak skill + the specialty datapoints) plus the Gemma stat
// commentary. The Special card reads this. Distinct from /stats (no fantasy/
// template/datapoints blocks).
// @Summary Get the entity specialist + commentary
// @Description The entity's specialist rating, specialty, the specialty datapoints, and the Gemma stat commentary.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param season query int false "Season year"
// @Param league_id query int false "League ID filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/{entityType}/{id}/special [get]
func (h *Handler) GetEntitySpecial(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}
	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}
	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}
	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}
	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}
	h.serveStatementJSON(w, r, "entity_special", dataCacheKey(r), cache.TTLData, false,
		sport, entityType, id, season, leagueID)
}

// GetEntityMeta returns the entity's IDENTITY metadata (two-rail model) — name,
// image, physicals, current team/club, position, tier — the payload that drives
// the page header and is eager-loaded first. 404 when the entity doesn't exist.
// Distinct from the sport-level /{sport}/meta (the search index, now at /autofill).
// @Summary Get per-entity identity metadata
// @Description The entity's identity for the page header: name, image, physicals, current team/club, position, tier.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Router /{sport}/{entityType}/{id}/meta [get]
func (h *Handler) GetEntityMeta(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	entityType, ok := parseEntityType(w, r)
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "entity id")
	if !ok {
		return
	}

	h.serveStatementJSON(w, r, "entity_meta", dataCacheKey(r), cache.TTLData, true, sport, entityType, id)
}

// GetLeagueTeamResults returns league-scoped team season results.
// @Summary Get league team season results
// @Description League-scoped variant of GetTeamResults — leagueId comes from the URL path. Identical payload.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param leagueId path int true "League ID"
// @Param id path int true "Team ID"
// @Param season query int false "Season year"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leagues/{leagueId}/team/{id}/results [get]
func (h *Handler) GetLeagueTeamResults(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	leagueID, ok := parsePathID(w, r, "leagueId", "league id")
	if !ok {
		return
	}

	id, ok := parsePathID(w, r, "id", "team id")
	if !ok {
		return
	}

	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}

	stmt := sport + "_team_results"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false, id, season, leagueID)
}

// GetMetaPage returns the canonical metadata payload used for frontend local DB hydration.
// @Summary Get meta page
// @Description Returns complete metadata and search payload for a sport.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param league_id query int false "League ID filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/meta [get]
func (h *Handler) GetMetaPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}

	stmt := sport + "_meta_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false, leagueID)
}

// GetAutofill returns the sport's instant-search / autofill index — the payload the
// frontend bundles for zero-latency search. Same data the legacy /{sport}/meta route
// serves today; this is its dedicated home in the two-rail model (where /meta is
// repurposed to per-entity metadata). Additive: /{sport}/meta stays until the
// frontend's search migrates here, then it's retired.
// @Summary Get the sport autofill/search index
// @Description The sport's autofill + search payload for frontend local hydration — the dedicated home for what /meta historically served.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param league_id query int false "League ID filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/autofill [get]
func (h *Handler) GetAutofill(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}

	stmt := sport + "_meta_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false, leagueID)
}

// GetLeagueMetaPage returns league-scoped metadata payload for a sport.
// @Summary Get league meta page
// @Description Returns metadata and search payload for a specific league.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param leagueId path int true "League ID"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leagues/{leagueId}/meta [get]
func (h *Handler) GetLeagueMetaPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	leagueID, ok := parsePathID(w, r, "leagueId", "league id")
	if !ok {
		return
	}

	stmt := sport + "_meta_page"
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false, leagueID)
}

// GetSportHealthPage returns sport-level data health from Postgres.
// @Summary Get sport health page
// @Description Returns sport data freshness and counts.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param league_id query int false "League ID filter"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/health [get]
func (h *Handler) GetSportHealthPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	leagueID, ok := optionalIntQuery(w, r, "league_id")
	if !ok {
		return
	}

	stmt := sport + "_health_page"
	h.serveStatementJSONNoCache(w, r, stmt, leagueID)
}

// GetLeagueHealthPage returns league-scoped sport health from Postgres.
// @Summary Get league health page
// @Description Returns sport data freshness and counts for a specific league.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param leagueId path int true "League ID"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leagues/{leagueId}/health [get]
func (h *Handler) GetLeagueHealthPage(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	leagueID, ok := parsePathID(w, r, "leagueId", "league id")
	if !ok {
		return
	}

	stmt := sport + "_health_page"
	h.serveStatementJSONNoCache(w, r, stmt, leagueID)
}

func (h *Handler) serveStatementJSON(
	w http.ResponseWriter,
	r *http.Request,
	stmt string,
	cacheKey string,
	ttl time.Duration,
	notFoundOnNoRows bool,
	args ...any,
) {
	if h.pool == nil {
		respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE", "database pool unavailable")
		return
	}

	if data, etag, ok := h.cache.Get(cacheKey); ok {
		if cache.CheckETagMatch(r.Header.Get("If-None-Match"), etag) {
			respond.WriteNotModified(w, etag)
			return
		}
		respond.WriteJSON(w, data, etag, ttl, true)
		return
	}

	var data []byte
	err := h.pool.QueryRow(r.Context(), stmt, args...).Scan(&data)
	if err != nil {
		if notFoundOnNoRows && errors.Is(err, pgx.ErrNoRows) {
			respond.WriteError(w, http.StatusNotFound, "NOT_FOUND", "resource not found")
			return
		}
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "database query failed")
		return
	}

	etag := h.cache.Set(cacheKey, data, ttl)
	respond.WriteJSON(w, data, etag, ttl, false)
}

func (h *Handler) serveStatementJSONNoCache(
	w http.ResponseWriter,
	r *http.Request,
	stmt string,
	args ...any,
) {
	if h.pool == nil {
		respond.WriteError(w, http.StatusServiceUnavailable, "SERVICE_UNAVAILABLE", "database pool unavailable")
		return
	}

	var data []byte
	err := h.pool.QueryRow(r.Context(), stmt, args...).Scan(&data)
	if err != nil {
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "database query failed")
		return
	}

	respond.WriteJSON(w, data, "", 0, false)
}

func parseSport(w http.ResponseWriter, r *http.Request) (string, bool) {
	sport := strings.ToLower(strings.TrimSpace(chi.URLParam(r, "sport")))
	if _, ok := validSports[sport]; !ok {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_SPORT", "sport must be one of: nba, nfl, football")
		return "", false
	}
	return sport, true
}

func parseEntityType(w http.ResponseWriter, r *http.Request) (string, bool) {
	entityType := strings.ToLower(strings.TrimSpace(chi.URLParam(r, "entityType")))
	if _, ok := validEntityTypes[entityType]; !ok {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_ENTITY_TYPE", "entityType must be one of: player, team")
		return "", false
	}
	return entityType, true
}

func parsePathID(w http.ResponseWriter, r *http.Request, param string, label string) (int, bool) {
	v := chi.URLParam(r, param)
	id, err := strconv.Atoi(v)
	if err != nil || id <= 0 {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_ID", fmt.Sprintf("%s must be a positive integer", label))
		return 0, false
	}
	return id, true
}

func requiredIntQuery(w http.ResponseWriter, r *http.Request, key string) (int, bool) {
	v := strings.TrimSpace(r.URL.Query().Get(key))
	if v == "" {
		respond.WriteError(w, http.StatusBadRequest, "MISSING_QUERY_PARAM", fmt.Sprintf("%s query parameter is required", key))
		return 0, false
	}
	n, err := strconv.Atoi(v)
	if err != nil {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_QUERY_PARAM", fmt.Sprintf("%s must be an integer", key))
		return 0, false
	}
	return n, true
}

func optionalIntQuery(w http.ResponseWriter, r *http.Request, key string) (any, bool) {
	v := strings.TrimSpace(r.URL.Query().Get(key))
	if v == "" {
		return nil, true
	}
	n, err := strconv.Atoi(v)
	if err != nil {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_QUERY_PARAM", fmt.Sprintf("%s must be an integer", key))
		return nil, false
	}
	return n, true
}

func optionalBoolQuery(w http.ResponseWriter, r *http.Request, key string) (any, bool) {
	v := strings.TrimSpace(r.URL.Query().Get(key))
	if v == "" {
		return nil, true
	}
	b, err := strconv.ParseBool(v)
	if err != nil {
		respond.WriteError(w, http.StatusBadRequest, "INVALID_QUERY_PARAM", fmt.Sprintf("%s must be a boolean", key))
		return nil, false
	}
	return b, true
}

func optionalTextQuery(r *http.Request, key string) any {
	v := strings.TrimSpace(r.URL.Query().Get(key))
	if v == "" {
		return nil
	}
	return v
}

func dataCacheKey(r *http.Request) string {
	if r.URL.RawQuery == "" {
		return "data:" + r.URL.Path
	}
	return "data:" + r.URL.Path + "?" + r.URL.RawQuery
}
