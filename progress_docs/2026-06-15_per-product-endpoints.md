# Per-product endpoints — "news + stats are sources, cards are products"

## Goal
Replace the two bundled "rail" reads with one endpoint per card product. `news` and
`stats` are the two data **sources**; each card (News, Transfers, Vibes, Composite,
Special, Trends) is a self-contained product with its own thin prepared statement,
handler, and route. Cards own their data; the client does zero shaping.

## Decisions
- **`/news` rail → `/news` (narratives) + `/transfers` (vetted heat list) + `/vibes`
  (current + history).** News is a post-transfers pipeline layer, so narratives already
  carry transfer context — `/news` is narratives-only and the News card drops its
  transfers scope.
- **`/sparkline` → `/stats` (full season rating + `available_seasons` + the per-event
  `events` series) + `/special` (lean specialist projection + Gemma `commentary`).** The
  heavy `fantasy/template/datapoints` blocks live only in `/stats`; `/special` omits them.
  The per-event series is stats data, so it stays in `/stats` (Trends reads it).
- One `/vibes` product serves both the Vibe card and the meta corner score.

## What Was Done
- **db.go**: added `entity_news`, `entity_transfers`, `entity_vibes`, `entity_stats`,
  `entity_special` (clean extractions from the old `entity_news_rail` / `sparkline`
  statements). Removed `entity_news_rail` + `sparkline`.
- **data.go**: added `GetEntityNarratives` / `GetEntityTransfers` / `GetEntityVibes` /
  `GetEntityStats` / `GetEntitySpecial`. Removed `GetSparkline` + `GetEntityNewsRail`.
- **server.go**: `/news`→`GetEntityNarratives`; added `/transfers` `/vibes` `/special`;
  `/stats` repointed from `GetProfilePage` to the stats product; retired `/sparkline` +
  `/starline`.
- Docs: README + ENDPOINTS rewritten for the per-product model.

## Deploy (additive → frontend → cleanup; zero 404 window)
1. Additive: added the new endpoints, kept the rail + `/sparkline` alive. Validated
   (all statements PREPARE), restarted.
2. Frontend switched to the per-product fetchers + deployed.
3. Cleanup (this): slimmed `/news`, retired the rail + `/sparkline`. Validated, restarted.

## Verification
`go build ./... && go vet` clean; validation-boot connected (all statements prepared);
restarted. Smoke (player + team, local + api.scoracle.com): `/stats` `/special` `/vibes`
`/transfers` `/trends` → 200 with correct shapes; `/news` → narratives-only; `/sparkline`
+ `/starline` → 404. scoracle.com profile → 200.

## Result
Six per-card products, each a thin SQL-shaped JSON passthrough. The bundled rails are
gone. iOS can consume the same product contracts 1:1.
