# Headlines Feature - Backend Implementation Plan

**Status:** Approved - Revised
**Date:** 2026-06-29
**Author:** Scotty Heneveld / Scoracle

---

## Overview

Add headlines as a third product in the news rail, alongside narratives and transfers. Headlines are entity-scoped breaking-news bulletins — one-sentence blurbs about high-impact events for a specific player or team.

This plan was pruned during audit to fit the live architecture: Go ingestion → Postgres → Rust Cognition Harness → Go endpoints.

### Key Decisions (Locked)

| Aspect | Decision |
|--------|----------|
| Data source | Google RSS ingest (existing Go `corpus.Sweep`) |
| Pipeline | New Rust stage `headlines` after scrub, before transfers |
| Classification | Single structured-extraction prompt per entity (no YES/NO gate) |
| Categories | `transfer`, `injury`, `coaching`, `contract`, `other` |
| Expiration | Auto-expire after 2 days |
| Sorting | `published_at DESC` (recency, not heat) |
| Related entities | NO for v1 |
| Heat score | Not needed |
| Leaderboard | Deferred to v2 |
| Entity links | Deferred to v2 |

### Data Flow

```
Go RSS sweep → Postgres news_articles / news_article_entities
                    ↓
             Rust ScrubHandler (candle embed + model gate)
                    ↓
             Postgres vetted=TRUE
                    ↓
             trigger enqueue_derive_on_vetted adds 'headlines' stage
                    ↓
             Rust HeadlinesHandler
                    ↓
             Postgres headlines table
                    ↓
             Rust TransferHandler / NarrativesHandler (can read headlines as enrichment)
                    ↓
             Rust VibeHandler → Rust SigilHandler
                    ↓
             Go endpoint GET /{sport}/{entityType}/{id}/headlines
```

Headlines are an independent product, not a gate. Transfers and narratives continue to run on the full vetted corpus and may optionally read the `headlines` table for enrichment.

---

## Architecture

### Endpoint

```
GET /api/v1/{sport}/{entityType}/{id}/headlines
```

Same shape as `/news` and `/transfers`.

### Database Table

```sql
CREATE TABLE headlines (
    id              BIGSERIAL PRIMARY KEY,
    sport           TEXT NOT NULL REFERENCES sports(id),
    entity_type     TEXT NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id       INTEGER NOT NULL,

    title           TEXT NOT NULL,
    category        TEXT NOT NULL CHECK (category IN ('transfer', 'injury', 'coaching', 'contract', 'other')),

    source_url      TEXT,
    source_name     TEXT,
    published_at    TIMESTAMPTZ NOT NULL,

    -- Provenance (matches news_summaries / transfer_rumors)
    input_news_ids  BIGINT[] NOT NULL DEFAULT '{}',
    model_version   TEXT,
    prompt_version  TEXT,
    trigger_type    TEXT NOT NULL CHECK (trigger_type IN ('news_spike', 'periodic', 'manual')),
    generated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### Indexes

- `idx_headlines_entity` on `(sport, entity_type, entity_id, published_at DESC)`
- `idx_headlines_category` on `(category)`
- `idx_headlines_published` on `(published_at DESC)`

### Expiration

Return only rows where `published_at > NOW() - INTERVAL '2 days'`.

---

## Implementation Tasks

### Phase 1: Database (2h)
- [ ] Create `sql/migrations/113_create_headlines_table.sql`
- [ ] Create follow-on migration to add `'headlines'` to `enqueue_derive_on_vetted()` `v_stages`
- [ ] Apply migrations **before** the next API restart

### Phase 2: Rust Stage (8–12h)
- [ ] Add `Stage::Headlines` to `rust/src/work.rs`
- [ ] Create `rust/src/headlines.rs`:
  - Load entity's vetted recent corpus (reuse narratives loader pattern)
  - Single prompt returning `{ "headlines": [{ "title", "category", "article_index" }] }`
  - Parser validates category and article index bounds
  - Persist one row per headline with provenance
  - Return `Ok(())` with zero headlines when none qualify (not a failure)
- [ ] Register handler in `rust/src/main.rs`
- [ ] Update `scripts/systemd/scoracle-cognition.service` `COGNITION_STAGES` to `scrub,headlines,transfers,narratives,vibe,sigil`

### Phase 3: Go API Handler (2–3h)
- [ ] Add prepared statement `entity_headlines` to `go/internal/db/db.go`
- [ ] Add `GetEntityHeadlines` to `go/internal/api/handler/data.go`
- [ ] Register route in `go/internal/api/server.go`
- [ ] Support `?limit=N` (default 20)
- [ ] Cache: `cache.TTLNews` (10 min)
- [ ] Filter expired rows in SQL

### Phase 4: Documentation (1h)
- [ ] Update `ENDPOINTS.md`
- [ ] Update Swagger annotations

---

## API Response Example

```json
{
  "page": "headlines",
  "sport": "football",
  "entity_type": "player",
  "entity_id": 1592,
  "headlines": [
    {
      "id": 42,
      "title": "Jarrod Bowen signs 5-year extension",
      "category": "contract",
      "source_url": "https://...",
      "source_name": "BBC Sport",
      "published_at": "2026-06-29T10:00:00Z"
    }
  ]
}
```

---

## Performance

- Single model call per entity per pipeline cycle.
- Cached 10 min TTL (`TTLNews`).
- ETag/If-None-Match via `serveStatementJSON`.
- Expiration filter in SQL.

---

## Testing

- [ ] Rust unit tests for prompt parser and category validation
- [ ] Rust integration test: empty corpus → zero rows
- [ ] Go handler test: 200 + empty array when no headlines
- [ ] Go test: expiration filter (2-day cutoff)
- [ ] Go test: all sports × player/team
- [ ] Pipeline test: vetted link enqueues `headlines` stage

---

## Dependencies

- Frontend scope dropdown implementation
- Existing RSS sweep and scrub pipeline

---

## Success Criteria

- `GET /{sport}/{type}/{id}/headlines` returns 200 with headlines or empty array.
- Headlines are linked to entities.
- Sorted `published_at DESC`.
- Expired rows (≥2 days) not returned.
- Rust stage runs without blocking transfers/narratives.
- All existing endpoints remain functional.
- Tests pass.
