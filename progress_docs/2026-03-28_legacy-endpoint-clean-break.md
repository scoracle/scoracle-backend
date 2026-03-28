# Legacy Endpoint Clean Break

**Date:** 2026-03-28

## Summary

Successfully removed all legacy API endpoints in a clean break migration. The system now exclusively uses the canonical vNext endpoint structure.

## What Was Removed

### 7 Legacy Endpoints

| Legacy Endpoint | Canonical Replacement |
|-----------------|----------------------|
| `GET /api/v1/{sport}/players/{id}` | `GET /api/v1/{sport}/{entityType}/{id}` |
| `GET /api/v1/{sport}/teams/{id}` | `GET /api/v1/{sport}/{entityType}/{id}` |
| `GET /api/v1/{sport}/standings` | `GET /api/v1/{sport}/meta` |
| `GET /api/v1/{sport}/leaders` | `GET /api/v1/{sport}/meta` |
| `GET /api/v1/{sport}/search` | `GET /api/v1/{sport}/meta` |
| `GET /api/v1/{sport}/autofill` | `GET /api/v1/{sport}/meta` |
| `GET /api/v1/{sport}/stat-definitions` | `GET /api/v1/{sport}/meta` |
| `GET /api/v1/football/leagues` | `GET /api/v1/football/meta` |

### Code Removed

1. **8 Legacy Handlers** from `go/internal/api/handler/data.go` (~310 lines):
   - `GetPlayerPage`
   - `GetTeamPage`
   - `GetStandingsPage`
   - `GetLeadersPage`
   - `GetSearchPage`
   - `GetAutofillPage`
   - `GetStatDefinitionsPage`
   - `GetLeaguesPage`

2. **Helper Code** from `go/internal/api/handler/data.go`:
   - `setLegacyRouteDeprecationHeaders()` function
   - `legacyRouteSunset` constant

3. **22 Prepared Statements** from `go/internal/db/db.go` (~241 lines):
   - Player pages: `nba_player_page`, `nfl_player_page`, `football_player_page`
   - Team pages: `nba_team_page`, `nfl_team_page`, `football_team_page`
   - Standings: `nba_standings_page`, `nfl_standings_page`, `football_standings_page`
   - Leaders: `nba_leaders_page`, `nfl_leaders_page`, `football_leaders_page`
   - Search: `nba_search_page`, `nfl_search_page`, `football_search_page`
   - Stat definitions: `nba_stat_definitions_page`, `nfl_stat_definitions_page`, `football_stat_definitions_page`
   - Autofill: `nba_autofill_page`, `nfl_autofill_page`, `football_autofill_page`
   - Leagues: `football_leagues_page`

4. **Route Registrations** from `go/internal/api/server.go`:
   - Removed all 7 legacy route registrations from the `/api/v1/{sport}` router

5. **Tests** from `go/internal/api/server_test.go`:
   - `TestLegacyRouteDeprecationHeaders` (entire test function)
   - 3 legacy route test cases from `TestRouteOwnershipSplit`

6. **Documentation**:
   - `ENDPOINTS.md`: Removed legacy endpoint section
   - `README.md`: Removed legacy routes mention
   - `AGENTS.md`: Updated public route shape
   - `CLAUDE.md`: Updated public route shape

7. **Swagger Docs** (`go/docs/`):
   - Regenerated via `swag init` - automatically removed legacy endpoint documentation

## Current Canonical Endpoint Structure

```
/api/v1/
├── {sport:nba|nfl|football}
│   ├── {entityType:player|team}/{id}  → GetProfilePage
│   ├── meta                           → GetMetaPage
│   ├── health                         → GetSportHealthPage
│   └── leagues/{leagueId}
│       ├── {entityType:player|team}/{id}  → GetLeagueProfilePage
│       ├── meta                           → GetLeagueMetaPage
│       └── health                         → GetLeagueHealthPage
├── news
│   ├── status
│   └── {entityType}/{entityID}
└── twitter
    ├── journalist-feed
    └── status
```

## Migration Strategy

The football leagues endpoint was already covered by the `football_meta_page` prepared statement which includes all leagues. The only difference is that the meta page doesn't support the `active` and `benchmark` query filters that the legacy leagues endpoint had. This is acceptable for the clean break since filtering can be done client-side if needed.

## Safety Verification

All tests pass:
```bash
$ cd go && go test ./...
ok      github.com/albapepper/scoracle-data/internal/api
ok      github.com/albapepper/scoracle-data/internal/config
```

Build succeeds:
```bash
$ cd go && go build ./cmd/api
# No errors
```

Swagger regenerated successfully without legacy endpoints.

## Breaking Changes

This is a **breaking change**. Clients using the following endpoints will receive 404 Not Found:

- `/api/v1/{sport}/players/{id}`
- `/api/v1/{sport}/teams/{id}`
- `/api/v1/{sport}/standings`
- `/api/v1/{sport}/leaders`
- `/api/v1/{sport}/search`
- `/api/v1/{sport}/autofill`
- `/api/v1/{sport}/stat-definitions`
- `/api/v1/football/leagues`

Migration guide for affected clients:
- Player/Team pages → use `/{sport}/{entityType}/{id}`
- Standings, Leaders, Search, Autofill, Stat Definitions → use `/{sport}/meta`
- Football Leagues → use `/football/meta`

## Lines of Code Impact

- **Removed:** ~580 lines
- **Net reduction:** ~580 lines (no new code added)

## Files Modified

1. `go/internal/api/handler/data.go`
2. `go/internal/api/server.go`
3. `go/internal/api/server_test.go`
4. `go/internal/db/db.go`
5. `go/docs/docs.go`
6. `go/docs/swagger.json`
7. `go/docs/swagger.yaml`
8. `ENDPOINTS.md`
9. `README.md`
10. `AGENTS.md`
11. `CLAUDE.md`

## Next Steps

- Monitor production for 404s on legacy routes
- Update any remaining client code references
- Archive legacy endpoint documentation from planning docs if needed
