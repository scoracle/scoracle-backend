# 2026-06-02 — Player-suitors endpoint (transfers player-side mirror)

## Goal

Add the deferred player-side view of the transfer/trade data: given a player,
"who's after them" — the teams linked with that player ranked by heat. The mirror
of the existing team transfers endpoint, over the same pair-level `transfer_rumors`.

## What Was Done

- **Route** (`server.go`): `GET /api/v1/{sport}/player/{id}/suitors → GetPlayerSuitors`,
  beside the team transfers route.
- **Handler** (`handler/data.go`): `GetPlayerSuitors` — clone of `GetTransfers`
  (parse sport + player id) → `serveStatementJSON("player_suitors", …, TTLData)`.
- **DB statement** (`db.go`, `"player_suitors"`): mirrors `team_transfers` but
  pivots on `player_id` and returns the linked TEAMS (`teams.logo_url AS image`),
  newest-row-per-pair (`DISTINCT ON`, so a fresh "cleared" supersedes an old
  heat-only seed), `WHERE is_rumor IS TRUE`, ranked by heat, top 25. Backed by the
  `(player_id, sport, generated_at DESC)` index already shipped in migration 031.

Payload: `{page:'suitors', sport, player_id, count, suitors:[{id, name, image,
heat, heat_components, direction, stage, gemma_summary, source_attribution,
rank}]}` — same shape as transfers, teams instead of players.

## Verification

- `go build ./...` + `go vet` clean. API rebuilt + redeployed.
- Son (190227): `count=25` (capped), teams ranked by heat — Liverpool 59,
  Nottingham Forest 43, AC Milan 35, Spurs 30, Juventus 20, Barcelona 19…; full
  field set, team logos as `image`.
- Empty case (player 1): `count=0, suitors=[]`. NBA route 200. Bad id → 400.

## Notes

- `direction` is `null` for pairs that are still heat-only **seed** rows (Gemma
  sets direction only when a team's pairs are vetted). Only Chelsea + West Ham have
  been Gemma-run so far; the daily transfer corpus cron (just installed) will vet
  the rest and fill direction/stage/summary across the board. Heat ranking is valid
  in the meantime — the Phase-1 "heat renders before Gemma" property holds here too.
- No frontend yet — this is the endpoint only. A player-profile "Linked with"
  card would be the natural next step if/when wanted (clone TransfersCard, swap
  player rows for team rows).
