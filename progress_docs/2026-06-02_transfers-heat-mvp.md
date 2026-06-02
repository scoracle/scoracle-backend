# 2026-06-02 — Transfers/Trades: deterministic heat MVP (Phases 0–1)

## Goal

Stand up the Transfers/Trades feature's foundation + MVP per the plan
(`~/.claude/plans/zany-dazzling-hamster.md`): a transparent, deterministic rumor
**heat index** over the existing news co-mention graph, served on the team
profile — **before** any Gemma vetting, to de-risk the shared-GPU integration.

## What Was Done

**Migration 031 (schema).** `source_tiers` (credibility weighting covering BOTH
news publications AND tweet author handles — the tier-1 transfer sources
Romano/Ornstein/Woj/Shams/Schefter appear as *tweet authors*, never as a news
source; seeded ~20 known-good, unknowns default low). `transfer_rumors`
(pair-level: team↔player, append model like `vibe_scores`, `heat` +
`heat_components` JSONB, nullable Gemma fields = "not vetted / cleared", all three
trigger_types in the CHECK from day one — closes the `vibe_scores` `news_spike`
gap). Three indexes (team-by-heat partial, pair-recent, player).

**Migration 032 (deterministic heat).** `compute_transfer_heat(team, player,
sport)` — pure SQL over the pair corpus (news + tweets linking both, 14d):
`heat = 100 · tier_weight · recency · (0.6·volume + 0.4·recent_frac)`, components
stored transparently (mirrors `rating_breakdown`). `sign`-correct credibility: a
single Romano tweet (weight 1.0) outweighs ten aggregators (default 0.3).
`seed_transfer_rumors(sport)` — heat-only backfill over co-mention candidate
pairs (≥2 distinct shared articles).

**Endpoint.** `GET /{sport}/team/{id}/transfers` — `team_transfers` statement in
`db.go` (latest-per-pair `DISTINCT ON`, ranked by heat, `is_rumor IS TRUE` filter,
joins players for name/image), `GetTransfers` handler (clone `GetRoster`), route.

## Files Changed

```
sql/migrations/031_transfer_rumors.sql      (NEW)
sql/migrations/032_transfer_heat.sql         (NEW)
go/internal/db/db.go                          (team_transfers statement)
go/internal/api/handler/data.go               (GetTransfers)
go/internal/api/server.go                     (route)
```

## Verification

- Migrations apply clean; `source_tiers` seeded (20 rows); seed produced 528
  FOOTBALL / 384 NBA / 396 NFL heat rows; distribution sane (most low via the
  aggregator discount, a few hot; max 68).
- Content validates: Chelsea ← Elliot Anderson (55), Liverpool ← Son (59) — real
  rumors, all tier-1.00. Roster co-mention also surfaces (Spurs ← Wembanyama 54,
  53 sources) — exactly the noise Gemma's `is_rumor` filter removes in Phase 2.
- `curl …/football/team/18/transfers` (Chelsea) → 26 ranked rumors with heat +
  components; `GET …/team/591/transfers` (PSG) → 3.

## Result — MVP in, KNOWN follow-ups

Deterministic heat card is live and fully transparent **without the LLM**, as
designed. Next: **Phase 2** (Gemma analyzer — `is_rumor`/direction/stage/grounded
summary, JSON-mode + defensive parse + deterministic fallback) then **Phase 3**
(news-spike trigger + listener + cron). Caveat: heat-only currently shows
"most co-mentioned players" — includes current-roster noise + a few bad entity
matches (e.g. "Capita"); Gemma vetting is what turns it into "actual rumors."
