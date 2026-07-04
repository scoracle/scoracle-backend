package handler

import (
	"context"
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

// GetLeaderboard returns the positionless rating leaderboard for a sport.
// Each row carries BOTH the Composite and Specialist score (+ the specialty label),
// so a single payload feeds the leaderboard board, the meta card, and the sparkline.
// @Summary Get rating leaderboard
// @Description Positionless rating board (z-score engine). entity_type=player (default) or team. Composite + Sigil in one payload.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entity_type query string false "Board type: player (default) or team"
// @Param scope query string false "Board: composite (default), sigil, or a sigil label (e.g. Sacks)"
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
	case "sigil":
		h.GetSigilLeaderboard(w, r)
		return
	case "news":
		h.GetNewsLeaderboard(w, r)
		return
	case "transfers":
		h.GetTransfersLeaderboard(w, r)
		return
	case "trending":
		h.GetTrendingLeaderboard(w, r)
		return
	default:
		respond.WriteError(w, http.StatusBadRequest, "INVALID_BOARD", "board must be one of: rating, vibes, sigil, news, transfers, trending")
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
// @Description Sport-wide board of entities ranked by latest vibe sentiment (1-100). With the Vibe felt-read blurb.
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

// GetSigilLeaderboard returns the sport-wide Sigil board: entities ranked by their
// latest Sigil synthesis score (1-100) — the holistic Rating+Vibe crown — enriched
// with name/image/team and carrying previous_score for the crown's delta.
// @Summary Sigil leaderboard
// @Description Sport-wide board of entities ranked by their latest Sigil crown score (1-100), the holistic Rating+Vibe synthesis. With the Sigil blurb + previous_score delta.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entity_type query string false "Filter: player or team (default both)"
// @Param limit query int false "Max rows (default 50)"
// @Param season query int false "Season year (default current)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leaderboard/sigil [get]
func (h *Handler) GetSigilLeaderboard(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}

	limit, ok := optionalIntQuery(w, r, "limit")
	if !ok {
		return
	}

	entityType := optionalTextQuery(r, "entity_type")

	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}

	h.serveStatementJSON(w, r, "sigil_leaderboard", dataCacheKey(r), cache.TTLData, false,
		sport, limit, entityType, season)
}

// GetTrendingLeaderboard returns the sport-wide TRENDING board — the risers: entities
// ranked by the slope of their recent trajectory. ?metric=vibe (default) ranks the
// vibe-sentiment trend; ?metric=rating ranks the composite-rating trend. score is the
// fitted per-week rise.
// @Summary Trending leaderboard (risers)
// @Description Entities ranked by their recent trajectory slope (risers). metric=vibe (default) or rating.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param metric query string false "Trajectory: vibe (default) or rating"
// @Param entity_type query string false "Filter: player or team (default both)"
// @Param limit query int false "Max rows (default 30)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/leaderboard/trending [get]
func (h *Handler) GetTrendingLeaderboard(w http.ResponseWriter, r *http.Request) {
	sport, ok := parseSport(w, r)
	if !ok {
		return
	}
	limit, ok := optionalIntQuery(w, r, "limit")
	if !ok {
		return
	}
	entityType := optionalTextQuery(r, "entity_type")
	stmt := "trending_vibe_leaderboard"
	if strings.ToLower(strings.TrimSpace(r.URL.Query().Get("metric"))) == "rating" {
		stmt = "trending_rating_leaderboard"
	}
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLData, false,
		sport, limit, entityType)
}

// GetNewsLeaderboard returns the sport-wide news board: entities ranked by their
// hottest narrative in the selected scope.
// @Summary News leaderboard
// @Description Sport-wide board of the hottest model narratives with source freshness and trajectory markers.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entity_type query string false "Filter: player or team (default both)"
// @Param scope query string false "News scope: current_week, last_week, two_weeks_ago, three_weeks_ago, last_month"
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
	scope := optionalTextQuery(r, "scope")

	// Two-rail model: the news board is now the hottest NARRATIVES (ranked by
	// per-narrative impact), not raw mention counts. narratives_leaderboard
	// supersedes the old news_leaderboard (which read news_article_entities).
	h.serveStatementJSON(w, r, "narratives_leaderboard", dataCacheKey(r), cache.TTLData, false,
		sport, limit, entityType, scope)
}

// GetTransfersLeaderboard returns the sport-wide transfer board: model-vetted
// (team, player) rumors ranked by heat (0-100), enriched with both sides of the pair.
// @Summary Transfers leaderboard
// @Description Sport-wide board of the hottest model-vetted transfer rumors, ranked by heat (0-100).
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param limit query int false "Max rows (default 50)"
// @Param scope query string false "Transfer scope: current_week, last_week, two_weeks_ago, three_weeks_ago, last_month"
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
	scope := optionalTextQuery(r, "scope")

	h.serveStatementJSON(w, r, "transfers_leaderboard", dataCacheKey(r), cache.TTLData, false,
		sport, limit, scope)
}

// GetTrendsPage returns last-3 entity event averages vs peer-cohort season averages.
// @Summary Get momentum (rating + vibe trajectory)
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
// @Router /{sport}/{entityType}/{id}/momentum [get]
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
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLMomentum, false, entityType, id, season, leagueID)
}

// GetLeagueTrendsPage returns league-scoped last-3 entity event averages vs peer-cohort season averages.
// @Summary Get league momentum page
// @Description League-scoped momentum payload. Mirrors GetTrendsPage with leagueId taken from the URL path.
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
// @Router /{sport}/leagues/{leagueId}/{entityType}/{id}/momentum [get]
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
	h.serveStatementJSON(w, r, stmt, dataCacheKey(r), cache.TTLMomentum, false, entityType, id, season, leagueID)
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
// @Description Players on the team's season roster with season Composite/Sigil (+ ranks + sigil label), ordered by the Composite+Sigil sum. Player names link to the player profile.
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

// GetTransfers + GetPlayerSuitors retired 2026-06-15 — the transfer scope is now
// its own product (GetEntityTransfers, /{type}/{id}/transfers).

// GetEntityNarratives returns the entity's NEWS product: the latest precomputed
// narratives, hottest first. Transfer rumors and vibe history are served by
// their own product endpoints.
// @Summary Get the entity news narratives
// @Description The entity's latest precomputed narratives (hottest first) ranked by per-narrative impact.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param scope query string false "News scope: current_week, last_week, two_weeks_ago, three_weeks_ago, last_month"
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
	scope := optionalTextQuery(r, "scope")
	h.serveStatementJSON(w, r, "entity_news", dataCacheKey(r), cache.TTLNews, false, sport, entityType, id, scope)
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
// @Param scope query string false "Transfer scope: current_week, last_week, two_weeks_ago, three_weeks_ago, last_month"
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
	scope := optionalTextQuery(r, "scope")
	h.serveStatementJSON(w, r, "entity_transfers", dataCacheKey(r), cache.TTLNews, false, sport, entityType, id, scope)
}

// GetEntityVibes returns the entity's SIGIL product — the crown synthesis (score
// 1-100 + blurb, fusing Rating + Vibe + Momentum) plus a bounded history. Read by
// the Sigil card AND the meta centre score. Canonical at /sigil.
// @Summary Get the entity Sigil (crown synthesis)
// @Description The entity's holistic Sigil synthesis — score (1-100) + blurb fusing Rating + Vibe + Momentum — plus a bounded history.
// @Tags data
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param entityType path string true "Entity type" Enums(player, team)
// @Param id path int true "Entity ID"
// @Param season query int false "Season year (default current)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 500 {object} respond.ErrorResponse
// @Router /{sport}/{entityType}/{id}/sigil [get]
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
	season, ok := optionalIntQuery(w, r, "season")
	if !ok {
		return
	}
	h.serveStatementJSON(w, r, "entity_vibes", dataCacheKey(r), cache.TTLNews, false, sport, entityType, id, season)
}

// GetEntityStats returns the entity's STATS product — the full season rating
// (Composite breakdown, modes, fantasy, template, scoped ranks) + available
// seasons. The Stats card + the ContentShell control strip read this. The
// per-event series moved to /momentum; the stat read lives at /rating.
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

// GetEntityRating returns the entity's RATING product — the statistical rail's end
// product: the positionless magnitude score + the divined defining strength + the
// precomputed on-field identity blurb. The Rating card reads this. Served at
// /rating; the /sigil path serves the crown synthesis.
// @Summary Get the entity rating (magnitude + strength + identity blurb)
// @Description The entity's positionless magnitude score, divined defining strength, and precomputed stat-identity blurb.
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
// @Router /{sport}/{entityType}/{id}/rating [get]
func (h *Handler) GetEntityRating(w http.ResponseWriter, r *http.Request) {
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
	h.serveStatementJSON(w, r, "entity_rating", dataCacheKey(r), cache.TTLData, false,
		sport, entityType, id, season, leagueID)
}

// GetEntityMeta returns the entity's IDENTITY metadata (two-rail model) — name,
// image, physicals, current team/club, position, tier — the payload that drives
// the page header and is eager-loaded first. 404 when the entity doesn't exist.
// It is one of the dedicated page-island hydration endpoints; the frontend's
// only local DB should be the small universal /api/v1/entities search directory.
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

// GetMetaPage returns the legacy sport-wide metadata/search payload. New
// frontend surfaces should hydrate metadata from dedicated per-island endpoints;
// home search should use /api/v1/entities.
// @Summary Get legacy sport metadata/search payload
// @Description Returns the legacy complete metadata and search payload for a sport.
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

// GetAutofill returns the legacy sport-scoped autofill payload. Keep this
// behavior unchanged for downstream/profile-specific consumers; the home page's
// universal search DB is /api/v1/entities.
// @Summary Get the legacy sport autofill/search payload
// @Description The sport-scoped autofill/search payload for existing consumers. Use /api/v1/entities for universal home search.
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

// GetEntities returns a universal, lightweight, text-only entity directory for
// the home-page search. It deliberately reads canonical public.players/public.teams
// instead of the sport-scoped autofill materialized views, because those views
// can be stat-filtered and carry profile hydration metadata.
// @Summary Get universal entity autofill directory
// @Description Cross-sport text-only directory of players and teams for home-page search.
// @Tags data
// @Produce json
// @Success 200 {object} map[string]interface{}
// @Failure 500 {object} respond.ErrorResponse
// @Router /entities [get]
func (h *Handler) GetEntities(w http.ResponseWriter, r *http.Request) {
	h.serveStatementJSON(w, r, "entities_directory", dataCacheKey(r), cache.TTLData, false)
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

// dbQueryTimeout bounds a single coalesced DB load in serveStatementJSON. The
// load runs on a detached context (not the caller's) so one client disconnecting
// cannot cancel the shared single-flight result for the others.
const dbQueryTimeout = 8 * time.Second

// cachedPayload is the single-flight result shared across coalesced cache misses.
type cachedPayload struct {
	data []byte
	etag string
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

	// Single-flight the miss. An eager profile open fans out ~6-9 endpoints at
	// once, and many visitors can miss the same hot entity simultaneously;
	// coalesce identical (cacheKey) loads so only ONE runs the SQL and the rest
	// share its result. The load runs on a detached, timeout-bounded context so
	// one caller disconnecting can't cancel the shared query for the others.
	v, err, _ := h.flight.Do(cacheKey, func() (any, error) {
		// Another flight may have filled the cache between our miss and here.
		if data, etag, ok := h.cache.Get(cacheKey); ok {
			return cachedPayload{data, etag}, nil
		}
		ctx, cancel := context.WithTimeout(context.Background(), dbQueryTimeout)
		defer cancel()
		var data []byte
		if err := h.pool.QueryRow(ctx, stmt, args...).Scan(&data); err != nil {
			return nil, err
		}
		etag := h.cache.Set(cacheKey, data, ttl)
		return cachedPayload{data, etag}, nil
	})
	if err != nil {
		if notFoundOnNoRows && errors.Is(err, pgx.ErrNoRows) {
			respond.WriteError(w, http.StatusNotFound, "NOT_FOUND", "resource not found")
			return
		}
		respond.WriteError(w, http.StatusInternalServerError, "DB_ERROR", "database query failed")
		return
	}
	res := v.(cachedPayload)
	respond.WriteJSON(w, res.data, res.etag, ttl, false)
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

	ctx, cancel := context.WithTimeout(r.Context(), dbQueryTimeout)
	defer cancel()
	var data []byte
	err := h.pool.QueryRow(ctx, stmt, args...).Scan(&data)
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
