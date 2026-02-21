# Scoracle Data — API Architecture

## Overview

This document outlines the API layer architecture for `scoracle-data`, defining responsibilities between Go and PostgREST, the security model, notification pipeline, and unified API documentation via a multi-spec Swagger UI.

The guiding principle is simple: **Postgres owns the data logic. PostgREST exposes it. Go handles everything external.**

---

## Responsibility Split

### PostgREST — Stats Read Layer

PostgREST serves all stat-related read endpoints directly from Postgres. Since all aggregations, percentile calculations, and derived metrics are computed inside Postgres via views and functions, Go was acting as a pass-through for these endpoints. PostgREST eliminates that boilerplate without sacrificing any logic.

**PostgREST handles:**
- All stat read endpoints (player stats, team stats, standings, leaderboards)
- Percentile and aggregation results via Postgres views
- Filtering, pagination, and sorting via URL query parameters (native PostgREST behavior)
- JWT validation and Row Level Security enforcement at the database layer

**PostgREST does not handle:**
- Any write operations involving external data sources
- Third-party API calls
- Push notifications
- Business logic that requires external I/O

### Go — Ingestion and Push Layer

Go is retained for all work that requires concurrency, external HTTP clients, retry logic, and persistent connections. These are areas where Go earns its keep and where a pass-through architecture would be inappropriate.

**Go handles:**
- Sports data provider ingestion (seeding and updating the database)
- News API integration
- Twitter/X API integration
- Firebase Cloud Messaging (FCM) push notifications
- LISTEN/NOTIFY consumer for real-time milestone event processing
- Any write endpoints that require coordination or validation beyond RLS

---

## Database Event Pipeline

### Real-Time Milestone Notifications (LISTEN/NOTIFY)

When the Go ingestion service writes a new stat row, a Postgres trigger fires `pg_notify` on the `milestone_reached` channel with a JSON payload identifying the entity and milestone.

The Go service holds an open `pgx` connection listening on this channel. On receiving a notification, it evaluates the payload against user subscriptions and dispatches FCM push notifications accordingly.

```
Ingestion write → Postgres trigger → pg_notify('milestone_reached', payload)
                                          ↓
                              Go LISTEN consumer
                                          ↓
                              FCM push to subscribed users
```

This model is event-driven, not polling-based. There is no scheduled interval introducing lag between a stat event and the notification.

### Scheduled and Maintenance Jobs (pg_cron)

`pg_cron` handles all time-based work that is purely internal to the database:

- Refreshing materialized views on a defined schedule
- Generating digest notification records for batch delivery
- Expiring stale cache records and cleaning up processed notification rows
- Periodic catch-up sweeps to handle any NOTIFY events missed during downtime

```sql
-- Example: refresh stats materialized view every 10 minutes
SELECT cron.schedule('refresh-stats', '*/10 * * * *', 'REFRESH MATERIALIZED VIEW CONCURRENTLY mv_player_stats');
```

pg_cron does **not** make external HTTP calls. All scheduled work operates on data already in Postgres.

---

## Security Model

### Row Level Security (RLS)

All tables and views exposed via PostgREST are governed by Postgres RLS policies. Security lives at the database layer, which is the most coherent location given that Postgres is the authoritative data store.

- Anonymous users receive a restricted role with read-only access to public stat views
- Authenticated users receive a role scoped to their own subscription and preference data
- PostgREST validates JWTs and sets the appropriate Postgres role per request

### Exposing Views, Not Tables

Raw tables are not exposed via PostgREST. All public endpoints are backed by views or Postgres functions. This provides a stable API surface and prevents internal schema changes from leaking to consumers.

```sql
-- Grant access to views, not base tables
GRANT SELECT ON player_stats_view TO web_anon;
GRANT SELECT ON team_standings_view TO web_anon;
```

### Go Endpoints

Go endpoints handling writes and external data are protected via middleware authentication checks and do not rely on PostgREST. These are internal service boundaries and are not publicly documented in the same spec as stat reads.

---

## Multi-Spec Swagger UI

### Overview

Both the PostgREST API and the Go API are documented in a single Swagger UI instance using the multi-spec dropdown feature. This provides a unified debugging interface without requiring spec merging.

### PostgREST OpenAPI Spec

PostgREST automatically generates an OpenAPI 3.0 spec at the root endpoint:

```
GET https://api.scoracle.com/
```

This spec is derived directly from the Postgres schema and stays in sync automatically. To enrich descriptions, add PostgreSQL comments to views and functions:

```sql
COMMENT ON VIEW player_stats_view IS 'Aggregated player stats with percentile ranks for the current season.';
COMMENT ON FUNCTION get_top_performers IS 'Returns top N players by stat category with percentile context.';
```

### Go OpenAPI Spec

The Go service generates its own OpenAPI spec using a library such as `swaggo/swag`. Annotate handlers with doc comments and generate the spec as part of the build process:

```
GET https://api.scoracle.com/go/swagger.json
```

### Swagger UI Configuration

Host a single Swagger UI instance (e.g., as a static file served by Go or as a separate Railway service) configured with both specs:

```javascript
SwaggerUIBundle({
  urls: [
    { url: "https://api.scoracle.com/", name: "Stats API (PostgREST)" },
    { url: "https://api.scoracle.com/go/swagger.json", name: "Ingestion & Notifications API (Go)" }
  ],
  dom_id: "#swagger-ui",
  presets: [SwaggerUIBundle.presets.apis, SwaggerUIStandalonePreset],
  layout: "StandaloneLayout"
})
```

The dropdown in the top-right of Swagger UI allows switching between the two specs. This keeps the boundary between the stat read layer and the ingestion/notification layer visible and explicit during development and debugging.

---

## Deployment

Both PostgREST and the Go service are deployed as separate services within the same Railway project. They share a private internal network, which means PostgREST's connection to Neon and Go's connection to Neon both benefit from low-latency private networking rather than public internet routing.

### PostgREST Railway Service

Deploy using the official PostgREST Docker image. The only required configuration is the database connection string and JWT secret, supplied as Railway environment variables:

```
PGRST_DB_URI=postgresql://...
PGRST_JWT_SECRET=your_jwt_secret
PGRST_DB_SCHEMA=api
PGRST_DB_ANON_ROLE=web_anon
```

### Connection Pooling

Neon provides built-in connection pooling via its serverless proxy. PgBouncer is not required at this stage. Revisit if connection count becomes a bottleneck under load.

### Region Alignment

Ensure the Railway project region and the Neon database region are co-located (e.g., both `AWS us-east-1`) to minimize database round-trip latency.

---

## Summary

| Concern | Owner |
|---|---|
| Stat reads | PostgREST |
| Aggregations & percentiles | Postgres (views/functions) |
| Auth & row security | Postgres RLS |
| Sports data ingestion | Go |
| News & social API | Go |
| Push notifications (FCM) | Go |
| Real-time stat events | Postgres LISTEN/NOTIFY → Go |
| Scheduled maintenance | pg_cron |
| API documentation | Multi-spec Swagger UI |
