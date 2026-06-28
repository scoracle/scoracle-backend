# HEADLINES FEATURE - MASTER PLAN

**Project:** Add Headlines to News Rail
**Status:** Approved - Ready for Implementation
**Date:** 2026-06-28
**Author:** Scotty Heneveld / Scoracle

---

## Executive Summary

Add headlines as a third product in the news rail, alongside existing narratives and transfers. Headlines are entity-scoped breaking news bulletins - one-sentence blurbs about high-impact, time-sensitive events for a specific player or team.

CRITICAL: Headlines follow the SAME entity-scoped pattern as narratives and transfers.

Examples:
- On Jarrod Bowen page: Bowen signs 5-year extension, Bowen named to England squad
- On Chelsea page: Chelsea hires new manager, Chelsea completes record transfer

---

## All Decisions Locked In

Aspect | Decision
------|----------
Data Source | Google RSS requests (existing Go layer)
Pipeline | NEW step BEFORE transfers: Mistral 7b identifies breaking news
Categories | transfer, injury, coaching, contract, other
Expiration | Auto-expire after 2 days
Sorting | published_at DESC (recency, NOT heat)
Related Entities | NO for v1 (simplicity)
Time Format | Relative (2h ago)
Source Display | Same prominence as transfers
Heat Score | Not needed for this product
Entity Links | Clickable if in DB
Endpoint | GET /api/v1/{sport}/{entityType}/{id}/headlines

---

## Pipeline Flow

Google RSS Feed -> Rust Candle (initial scrub) -> Mistral 7b: Is this breaking headline news? -> YES: Create one-sentence blurb, store as HEADLINE -> NO: Continue to existing flow -> Mistral 7b: Is this transfer news? -> YES: Store as TRANSFER -> NO: Continue -> Mistral 7b: Generate narrative, store as NARRATIVE

---

## Team Plans

### Backend
File: scoracle-backend/planning_docs/HEADLINES_FEATURE.md
Lead: Backend team
Estimated: 17-27 hours

Phases:
1. Database (2-3h) - headlines table with expiration
2. Pipeline Integration (8-12h) - add headline determination step
3. API Handler (4-6h) - GET /{sport}/{type}/{id}/headlines
4. Leaderboard Integration (2-4h) - support board=headlines
5. Documentation (1-2h) - update ENDPOINTS.md, swagger

Key Files:
- go/internal/api/handler/headlines.go (new)
- sql/migrations/XXXX_create_headlines_table.sql (new)
- go/internal/api/server.go (modified)
- ENDPOINTS.md (modified)

### Frontend
File: scoracle-frontend/planning_docs/HEADLINES_FEATURE.md
Lead: Frontend team
Estimated: 14-22 hours

Phases:
1. Type Updates (1h) - extend NewsScope type
2. Data Layer (2-3h) - getHeadlines() server function
3. NewsCard Updates (4-6h) - add headlines rendering
4. CSS Styling (2-3h) - headline-specific styles
5. ScopeRail Integration (1-2h) - add dropdown option
6. Entity Linking (2-3h, optional) - clickable entity names
7. Testing (4-6h) - full test coverage

Key Files:
- src/lib/data/headlines.server.ts (new)
- src/contexts/profile.ts (modified)
- src/components/solid/NewsCard.tsx (modified)
- src/components/solid/NewsCard.css (modified)
- ScopeRail component (modified)

---

## Combined Timeline

Phase | Backend | Frontend | Notes
------|---------|----------|------
Week 1 | Database + Pipeline | Types + Data Layer | Can work in parallel
Week 1 | API Handler | NewsCard + CSS | Backend leads
Week 2 | Leaderboard + Docs | ScopeRail + Testing | Frontend catches up
Week 2 | Testing | Polish | Joint effort
Total | 17-27h | 14-22h | 25-41h combined

Realistic timeline: 1 week of focused work

---

## Definition of Done

### Backend
- Endpoint works
- Headlines properly linked to entities
- Headlines sorted by published_at DESC (newest first)
- Caching works (5 min TTL)
- Expiration works (2 day cutoff)
- Leaderboard integration works
- All existing endpoints remain functional
- Documentation updated
- Pipeline correctly routes: headline -> transfer -> narrative
- Tests passing

### Frontend
- Headlines option visible in ScopeRail dropdown
- Selecting Headlines displays breaking news for current entity
- Headlines render with: title, category, relative time, source
- Switching between scopes is smooth and instant
- Empty state shows appropriate message
- All existing news/transfers functionality unchanged
- Mobile responsive design works correctly
- Entity links work (if implemented)
- All tests passing

### Both
- End-to-end flow works (RSS -> headline -> API -> frontend)
- Performance acceptable (< 500ms endpoint response)
- Ready for production deployment

---

## Team Coordination

Backend Team: Database, pipeline, API handler, leaderboard, docs, testing
Frontend Team: Types, data layer, components, styling, testing
Joint: End-to-end integration, performance, edge cases, final QA

---

## Communication

Slack channel: #headlines-feature
Daily sync: 15 min standup during active development
Weekly demo: Friday to stakeholders
Final review: Before production deployment

---

## Next Steps

1. Backend starts - Database + Pipeline Integration (Phase 1)
2. Frontend starts - Types + Data Layer (Phase 1, parallel)
3. Daily coordination - Sync on progress and blockers
4. Midpoint review - After backend API is working
5. Final testing - Joint end-to-end validation
6. Deploy - Backend first, then frontend

---

## Related Documents

- Backend Detailed Plan: scoracle-backend/planning_docs/HEADLINES_FEATURE.md
- Frontend Detailed Plan: scoracle-frontend/planning_docs/HEADLINES_FEATURE.md
- Current ENDPOINTS.md: scoracle-backend/ENDPOINTS.md

---

## Status

All decisions locked in per Scotty input on 2026-06-28. Ready for implementation.