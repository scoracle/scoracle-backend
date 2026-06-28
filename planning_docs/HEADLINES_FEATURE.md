# Headlines Feature - Backend Implementation Plan

**Status:** Approved
**Date:** 2026-06-28
**Author:** Scotty Heneveld / Scoracle

---

## Overview

Add headlines as a third product to the news rail, alongside narratives and transfers. Headlines are entity-scoped breaking news bulletins - one-sentence blurbs about high-impact events for a specific player or team.

### Key Decisions
- Data Source: Google RSS requests (existing Go layer)
- Pipeline: NEW step BEFORE transfers - Mistral 7b determines if breaking headline news
- Categories: transfer, injury, coaching, contract, other
- Expiration: Auto-expire after 2 days
- Sorting: published_at DESC (recency, NOT heat)
- Related Entities: NO for v1 (simplicity)
- Heat Score: Not needed for this product

### Data Flow
Google RSS Feed -> Rust Candle (initial scrub) -> Mistral 7b: Is this breaking headline news? -> YES: Create one-sentence blurb, store as HEADLINE -> NO: Continue to transfers stage -> Mistral 7b: Is this transfer news? -> YES: Store as TRANSFER -> NO: Continue to narratives stage -> Mistral 7b: Generate narrative -> Store as NARRATIVE

---

## Architecture

### Endpoint
GET /api/v1/{sport}/{entityType}/{id}/headlines

Follows the exact same pattern as /news and /transfers.

### Database Table
- id SERIAL PRIMARY KEY
- sport VARCHAR(20) NOT NULL
- entity_type VARCHAR(10) NOT NULL (player or team)
- entity_id INTEGER NOT NULL
- title TEXT NOT NULL
- category VARCHAR(50) NOT NULL
- source_url TEXT
- source_name VARCHAR(100)
- published_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()

### Indexes
- idx_headlines_entity ON headlines(sport, entity_type, entity_id, published_at DESC)
- idx_headlines_category ON headlines(category)
- idx_headlines_published ON headlines(published_at DESC)

### Expiration
Headlines auto-expire after 2 days. Query filter: WHERE published_at > NOW() - INTERVAL 2 days

---

## Implementation Tasks

### Phase 1: Database (2-3 hours)
- Create migration for headlines table
- Add expiration logic (query filter)

### Phase 2: Pipeline Integration (8-12 hours)
- Add headline determination step to existing RSS pipeline
- Modify Rust Candle output to feed Mistral 7b for headline check
- Mistral 7b prompt: Is this breaking headline news? Respond YES or NO only
- If YES: Generate one-sentence blurb, store in headlines table
- If NO: Continue existing flow to transfers
- Ensure entity linking works (map RSS item to entity_id)

### Phase 3: API Handler (4-6 hours)
- Create go/internal/api/handler/headlines.go
- Register route in go/internal/api/server.go
- Implement GetHeadlines function
- Support query param: ?limit=20 (default 20)
- Cache: 5 min TTL (matching narratives/transfers)
- Auto-filter expired headlines in query
- Return 200 with empty array if no headlines

### Phase 4: Leaderboard Integration (2-4 hours)
- Extend GET /api/v1/{sport}/leaderboard to support ?board=headlines
- Returns top entities by headline count

### Phase 5: Documentation (1-2 hours)
- Update ENDPOINTS.md with new endpoint
- Update swagger spec

---

## API Response Example

GET /api/v1/football/player/1592/headlines

Response contains page, sport, entity_type, entity_id, and headlines array with id, title, category, source_url, source_name, published_at.

---

## Performance
1. Caching: 5 min TTL
2. ETags: Implement ETag/If-None-Match
3. Query optimization: Index on (sport, entity_type, entity_id, published_at)
4. Expiration: Filter expired in query
5. Pagination: Support ?offset=20 and ?limit=20

---

## Testing
- Unit tests for handler logic
- Integration tests for endpoint
- Test with all sports (nba, nfl, football)
- Test with both player and team entities
- Test filtering and pagination
- Verify cache headers
- Test expiration (headlines older than 2 days not returned)
- Test pipeline: RSS -> headline determination -> storage

---

## Timeline
Phase | Time
------|------
Database | 2-3h
Pipeline Integration | 8-12h
API Handler | 4-6h
Leaderboard Integration | 2-4h
Documentation | 1-2h
Total | 17-27 hours

---

## Dependencies
- Frontend: Must implement UI to display headlines
- Existing RSS pipeline: Must be accessible for modification
- Entity Database: Must have entity lookup for linking headlines

---

## Success Criteria
- Endpoint works
- Headlines properly linked to entities
- Headlines appear in correct order (newest first by published_at)
- Caching works (5 min TTL)
- Expiration works (2 day cutoff)
- Leaderboard integration works
- All existing endpoints remain functional
- Documentation updated
- Pipeline correctly routes: headline -> transfer -> narrative
- Tests passing