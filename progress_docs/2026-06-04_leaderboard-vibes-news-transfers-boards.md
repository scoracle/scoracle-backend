# 2026-06-04 — Sport-wide leaderboard boards: vibes, news, transfers

## Goals

Stand up the three missing **sport-wide leaderboard** endpoints so the frontend's new
`/leaderboard` page can switch between board types. The rating board already existed
(`/leaderboard`); vibes/news/transfers did not exist as sport-wide boards.

## Decisions

- **Three new prepared statements** in `db.go`, each returning final JSON
  (Postgres-as-serializer) and modeled on proven, live statements:
  - `vibes_leaderboard` — wraps the **exact** inner query from the live `/vibe/hottest`
    handler (latest sentiment per entity in 48h, `prompt_version <> 'v2'`), then **enriches**
    it with a join to `players`/`teams` for `name`/`image`/`team_*`. `/vibe/hottest` is left
    untouched (it stays a thin feed); the new board is the enriched sibling so the frontend
    has **one row shape** across single-entity boards.
  - `news_leaderboard` — most-mentioned entities: `COUNT(DISTINCT article_id)` from
    `news_article_entities` over a rolling window (`make_interval(days => …)`, default 30),
    same enriched row shape, `score` = mention count.
  - `transfers_leaderboard` — sport-wide sibling of `team_transfers`/`player_suitors`:
    latest row per `(team, player)` pair (`DISTINCT ON`), `is_rumor IS TRUE`, ranked by
    `heat` desc, with **both** sides of the pair on each row (player + team).
- **Routes nested under `/leaderboard/…`** (`/leaderboard/vibes|news|transfers`) so the
  board family is discoverable; the composite board keeps `/leaderboard`.
- **Thin handlers** mirroring `GetLeaderboard` exactly (parseSport → optional params →
  `serveStatementJSON` with `cache.TTLData`). No service layer, swaggo annotations added.
- **No schema migration** — every column already existed (`vibe_scores.sentiment`,
  `news_article_entities.{article_id,created_at}`, `transfer_rumors.{heat,is_rumor,…}`).

## Accomplishments

- `db.go`: +`vibes_leaderboard`, `news_leaderboard`, `transfers_leaderboard`.
- `data.go`: +`GetVibesLeaderboard`, `GetNewsLeaderboard`, `GetTransfersLeaderboard`.
- `server.go`: +3 routes under the `{sport}` group.
- `ENDPOINTS.md` + `README.md`: documented all three (params + response shapes).
- Built, `gofmt`/`go vet` clean. **Pre-flight** (throwaway `cmd/lbcheck`, since removed)
  ran the full `db.New` registration path + executed all three against the live DB —
  every statement registered cleanly (no risk of degraded-mode on restart).
- Restarted the API via `systemctl --user restart scoracle-api.service`; health 200;
  all three endpoints verified live on `:8000`.

## Quick reference

```
GET /api/v1/{sport}/leaderboard/vibes?entity_type=player|team&limit=N
GET /api/v1/{sport}/leaderboard/news?entity_type=player|team&days=N&limit=N
GET /api/v1/{sport}/leaderboard/transfers?limit=N
```

Verified samples: NBA vibes (Knicks 92), NBA news/14d (Spurs 1183 mentions),
football transfers (Haaland→Real Madrid heat 95; 432 rumors), NBA transfers (255).

## Follow-ups

- **Swagger regen pending**: `swag` is not installed on this box, so `go/docs/*` is stale
  for the three new routes (annotations are in place). Run `swag init -g cmd/api/main.go`
  on a box with swag to refresh `/docs`.
