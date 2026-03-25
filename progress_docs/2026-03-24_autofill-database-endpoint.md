# 2026-03-24: Autofill Database Endpoint

## Summary

Implemented new `/api/v1/{sport}/autofill` endpoint that returns the complete autofill database for frontend caching, enabling instant meta widget rendering without additional API calls.

## Changes

### SQL Schema

**Modified files:**
- `sql/nba.sql`
- `sql/nfl.sql`  
- `sql/football.sql`

**Changes:**
1. Expanded `autofill_entities` materialized views with full metadata:
   - Added all profile fields: `first_name`, `last_name`, `nationality`, `date_of_birth`, `height`, `weight`, `photo_url`, `team_id`, `team_abbr`, `team_name`, `league_id`, `league_name`
   - Added `search_tokens` JSONB array for frontend fuzzy search
   - Added filtered `meta` object containing only display-worthy fields

2. Removed `meta` column from player and team profile views (stats endpoints now return stats only)

### Go API

**Modified files:**
- `go/internal/db/db.go` - Added 3 prepared statements for autofill pages
- `go/internal/api/handler/data.go` - Added `GetAutofillPage()` handler and `serveStatementJSONNoCache()` helper
- `go/internal/api/server.go` - Registered `/api/v1/{sport}/autofill` route
- `go/internal/api/server_test.go` - Added route registration test
- `ENDPOINTS.md` - Added documentation for new endpoint

## API Changes

### New Endpoint

**`GET /api/v1/{sport}/autofill`**

Returns complete autofill database (~8-40KB gzipped per sport) with all entities and their metadata.

**No server-side caching** - frontend responsibility to cache at build time.

**Compression:** Already enabled via existing gzip middleware.

### Modified Endpoints

**`GET /api/v1/{sport}/players/{id}`** and **`GET /api/v1/{sport}/teams/{id}`**

- Removed `meta` field from response (now in autofill database)
- Endpoints now focus purely on stats and percentiles

## Frontend Impact

Frontend should:
1. Fetch autofill DB at build time: `GET /api/v1/{sport}/autofill`
2. Cache locally for instant search and meta widget rendering
3. Use `search_tokens` array for fuzzy matching
4. Query stats endpoints separately when viewing detailed stats

## Performance

- Total response size: ~76KB gzipped (all 3 sports combined)
- Compression: Handled automatically by gzip middleware (level 5)
- Database refresh: After fixture processing (entities rarely change)

## Notes

- The `meta` field in autofill responses is filtered to include only display-worthy fields (jersey numbers, draft info, venue details, etc.)
- Raw provider data remains in database but is not exposed through API
- Existing `/search?q={query}` endpoint remains unchanged for live search
