# HEADLINES FEATURE - MASTER PLAN

**Project:** Add Headlines to News Rail
**Status:** Approved - Revised
**Date:** 2026-06-29
**Author:** Scotty Heneveld / Scoracle

---

## Executive Summary

Add headlines as a third product in the news rail, alongside existing narratives and transfers. Headlines are entity-scoped breaking-news bulletins — one-sentence blurbs about high-impact, time-sensitive events for a specific player or team.

This revision prunes v1 scope so the feature slots cleanly into the live architecture:

```
Go ingestion → Postgres → Rust Cognition Harness → Go endpoints
```

The news rail flow becomes:

```
ingestion → scrub (candle) → headlines → transfers → narratives → vibe
                                      ↓
                                 momentum (stats rail + vibe)
                                      ↓
                                     sigil (stats + vibe + momentum)
```

Headlines is a new Rust queue stage. It does **not** gate transfers or narratives; those stages continue to run on the full vetted corpus and may read the `headlines` table as enrichment in future iterations.

---

## All Decisions Locked In

| Aspect | Decision |
|--------|----------|
| Data source | Google RSS ingest (existing Go layer) |
| Pipeline | New Rust `headlines` stage after scrub, before transfers |
| Classification | Single structured-extraction prompt per entity (no YES/NO gate) |
| Categories | `transfer`, `injury`, `coaching`, `contract`, `other` |
| Expiration | Auto-expire after 2 days |
| Sorting | `published_at DESC` (recency, not heat) |
| Related entities | NO for v1 |
| Time format | Relative ("2h ago"), client-only |
| Source display | Same prominence as transfers |
| Heat score | Not needed |
| Entity links | **Deferred to v2** |
| Leaderboard | **Deferred to v2** |
| Endpoint | `GET /api/v1/{sport}/{entityType}/{id}/headlines` |

---

## Pipeline Flow

```
Go RSS sweep
    → Postgres (news_articles / news_article_entities)
    → Rust ScrubHandler (candle embed + model gate)
    → Postgres vetted=TRUE
    → migration-103 trigger enqueues headlines (plus transfers, narratives, vibe)
    → Rust HeadlinesHandler
    → Postgres headlines table
    → Rust TransferHandler / NarrativesHandler / VibeHandler / SigilHandler
    → Go endpoints
```

---

## Team Plans

### Backend
**File:** `scoracle-backend/planning_docs/HEADLINES_FEATURE.md`
**Estimated:** 13–18 hours

1. Database (2h) — `headlines` table + trigger update
2. Rust Stage (8–12h) — `HeadlinesHandler`, prompt, parser, registration
3. API Handler (2–3h) — endpoint, prepared statement, route
4. Documentation (1h) — `ENDPOINTS.md`, Swagger

**Key files:**
- `rust/src/work.rs`
- `rust/src/headlines.rs` (new)
- `rust/src/main.rs`
- `scripts/systemd/scoracle-cognition.service`
- `sql/migrations/113_create_headlines_table.sql`
- `sql/migrations/114_enqueue_headlines_stage.sql` (or edit 103)
- `go/internal/db/db.go`
- `go/internal/api/handler/data.go`
- `go/internal/api/server.go`

### Frontend
**File:** `scoracle-frontend/planning_docs/HEADLINES_FEATURE.md`
**Estimated:** 8–12 hours

1. Types & URL plumbing (1h)
2. Data layer (1–2h)
3. NewsCard rendering (3–4h)
4. CSS (1–2h)
5. Relative-time utility (1h)
6. Testing (2–3h)

**Key files:**
- `src/contexts/profile.ts`
- `src/routes/profile.tsx`
- `src/components/solid/ContentShell.tsx`
- `src/lib/utils/data-sources.ts`
- `src/lib/data/headlines.server.ts` (new)
- `src/components/solid/card-registry.tsx`
- `src/components/solid/NewsCard.tsx`
- `src/components/solid/NewsCard.css`
- `src/lib/utils/date.ts`

---

## Combined Timeline

| Phase | Backend | Frontend | Notes |
|-------|---------|----------|-------|
| Day 1 | DB + Rust stage skeleton | Types + data layer | Parallel |
| Day 2 | Rust prompt/parser + Go endpoint | NewsCard + CSS | Backend endpoint unblocks frontend |
| Day 3 | Tests + docs | Relative time + tests | Joint polish |

Realistic timeline: **one focused week**.

---

## Definition of Done

### Backend
- `headlines` Rust stage runs after scrub and before transfers.
- Endpoint works for all sports and entity types.
- Headlines properly linked to entities.
- Sorted by `published_at DESC`.
- 10-minute cache works.
- 2-day expiration works.
- All existing endpoints remain functional.
- Documentation updated.
- Tests passing.

### Frontend
- Headlines option visible in scope dropdown.
- Selecting Headlines displays breaking news for current entity.
- Headlines render with title, category, relative time, source.
- Scope switching is instant (eager fetch).
- Empty state shows appropriate message.
- Existing news/transfers unchanged.
- Mobile responsive.
- Tests passing.

---

## Communication

- Slack: #headlines-feature
- Daily 15-min standup during active development
- Final review before production deployment

---

## Next Steps

1. Backend: database migration + Rust `HeadlinesHandler` skeleton
2. Frontend: types + `getHeadlines` data layer (can use mock until endpoint is live)
3. Joint: end-to-end integration, performance check, edge cases
4. Deploy backend first, then frontend

---

## Related Documents

- Backend Detailed Plan: `scoracle-backend/planning_docs/HEADLINES_FEATURE.md`
- Frontend Detailed Plan: `scoracle-frontend/planning_docs/HEADLINES_FEATURE.md`
- Current ENDPOINTS.md: `scoracle-backend/ENDPOINTS.md`
