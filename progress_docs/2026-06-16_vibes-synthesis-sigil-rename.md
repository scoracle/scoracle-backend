# Vibes Synthesis + Sigil Rename

**Date:** 2026-06-16  
**Branch:** `feat/vibes-synthesis` (backend + frontend)

> **⚠️ Post-audit correction (2026-06-16).** The original plan renamed the engine
> columns `rating_specialist*` → `rating_sigil*` (migration 089). A pre-deploy audit
> caught that this **breaks the rating engine**: `compute_rating` (067), `compute_team_rating`
> (068), `_compute_rating_bundle` (080) and the event starline/pct functions all write those
> columns *by name*, and PL/pgSQL late-binds — so the rename passes `go build`, the typecheck
> sweep, and even `db.New()` prepared-statement validation, then fails on the next
> `finalize_fixture()` with `column "rating_specialist" does not exist`. (Two more gaps: 089
> only renamed `rating_specialist_pct` on the event tables, not `rating_specialist`/`_specialty`;
> and the engine builds `rating_modes` JSON with `specialist`-keyed blocks the frontend reads as
> `sigil`.) **Resolution (chosen by Scott): keep `Specialist` as the engine's internal column
> name; present the `Sigil` surface by aliasing in the read layer** (`db.go` + `stat_commentary.go`
> select `rating_specialist AS rating_sigil`, etc., and remap the `rating_modes` keys). Migration
> 089 is now a self-healing no-op. Validated read-only against the live schema: columns intact,
> aliases + remap run correctly. See [[Session - Vibes synthesis + Sigil rename]] for the full
> audit. The "089 column rename" and "db.go column sweep" lines below are superseded by this.

## Goals

1. Promote the 1-100 Gemma-generated sentiment score to an internal ingredient (renamed `sentiment_scores`); remove cosmetic round-number guardrails.
2. Replace the vibe product with a holistic three-pillar synthesis: news narrative + Sigil identity + momentum.
3. Rename "Special/Specialist" to "Sigil" throughout; have Gemma divine the Sigil label rather than receiving it pre-supplied.

## Decisions

- **R1 NOTIFY safety:** `newsVolumeChannel = "vibe_trigger"` kept on the wire string in R1. R2 atomically flips both `pg_notify(...)` SQL and the Go constant to `"sentiment_trigger"`. Never flip one side without the other.
- **SynthesisGenerator is a new type** (`ml.SynthesisGenerator`), not mixed into the existing `Generator`. Bounding blast radius.
- **`RecentlySynthesized` exported** so `listener` and `handler` packages can call it without a `corpus.*` wrapper.
- **Variadic `NewRouter`** accepts `synthGen ...*ml.SynthesisGenerator` to keep existing call sites unbroken.
- **`heroLabel` in SigilCard** prefers `commentary.divined_sigil` (Gemma s3+) but falls back to the engine's breakdown label. Art icon always uses the engine label for stability.
- **Outer-scope `synthGen` declaration** in `cmd/api/main.go` so it can be wired into both `listener.Start` and `api.NewRouter` from outside the `if dbPool != nil` block.

## Accomplishments

### Migrations
- **088** — `vibe_scores` → `sentiment_scores`; indexes + constraint renamed; `notify_vibe_trigger()` function kept (R1 safety)
- **089** — ~~DB column rename `rating_specialist*` → `rating_sigil*`~~ **REVERSED → self-healing no-op** (see correction above). Engine columns stay `rating_specialist*`; the `Sigil` surface is produced by read-layer aliasing in `db.go` + `stat_commentary.go`.
- **090** — `ALTER TABLE stat_summaries ADD COLUMN IF NOT EXISTS divined_sigil TEXT`
- **091** — New `vibe_synthesis` table (SMALLINT score/previous_score, TEXT blurb, JSONB input_components/trigger_payload, TEXT input_hash, prompt_version, model_version, TIMESTAMPTZ generated_at); indexes `idx_vibe_synthesis_entity_recent` + `idx_vibe_synthesis_sport_score`

### Backend Go
- `vibe.go` → `sentiment.go`: types `VibeRequest`→`SentimentRequest`, `VibeResult`→`SentimentResult`; prompt de-shackled (round-number guardrails removed); version `v4`→`v5`
- `stat_commentary.go` (D.2): Gemma now divines the Sigil (`SIGIL: <label>` on line 1); `parseSigilCommentary` extractor; `DivinedSigil` in result + persisted to `stat_summaries.divined_sigil`; prompt version `s2`→`s3`
- `vibe_synthesis.go` (new): three-pillar generator (P1=narratives, P2=sigil, P3=momentum slope); `SkipUnchanged` input-hash debounce; `RecentlySynthesized` exported; `parseSynthesisResponse` SCORE/BLURB extractor
- `cmd/vibesynth/main.go` (new): single / backfill / nightly corpus modes
- `db.go`: `entity_vibes` + `vibes_leaderboard` CTEs rewritten to read from `vibe_synthesis`; `entity_sigil` commentary subquery adds `divined_sigil`. **Read-layer Sigil aliasing** (post-audit): leaderboard, roster, `entity_stats`, `entity_sigil`, and the event series select `rating_specialist AS rating_sigil` (etc.); `rating_modes` JSON keys remapped `specialist→sigil`. `stat_commentary.go` `loadRatingProfile` reverted to the physical `rating_specialist*` columns.
- `listener/listener.go` (C.4): composite-shift trigger — `|Δ| ≥ 10 pct` spawns background synthesis with `TriggerType: "composite_shift"`; `ml.RecentlySynthesized` 24h debounce
- `listener/news_volume_worker.go` (C.1): Stage 4 synthesis after Stage 3 sentiment
- `handler/data.go` (C.2): lazy-view synthesis on cold `GetEntityVibes` requests; `SetSynthGen` setter on `Handler`
- `cmd/pipeline/main.go` (C.3): Stage 4 synthesis loop with `synth-skip-recent-hours` flag (default 24)
- `api/server.go`: variadic `NewRouter` wires `synthGen` to handler

### Frontend
- `sigil.server.ts` / `SigilCard.tsx` / `SigilCard.css` / `sigil-art.tsx` / `bodies/sigil.ts`: full Special→Sigil rename; `SigilCard` hero shows `divined_sigil ?? fallback`; team parity (removed `showFor: player` gate)
- `card-registry.tsx`, `profile-tabs.ts`, `card-meta.ts`, `share-url.ts`, `data-sources.ts`, `og-bodies.ts`: all references updated
- `vibes.server.ts`: `VibeCurrent.sentiment`→`score`; added `blurb`, `previous_score`
- `VibeCard.tsx` + `VibeCard.css`: renders blurb between archetype name and subtext
- `EntityMeta.tsx`, `og-bodies.ts`: `v.sentiment`→`v.score`
- `TrendsCard.tsx`: `entity_season_vibe_series`→`entity_season_sentiment_series`; label hardcoded "Sentiment"

## Quick Reference

```
# Build check
cd scoracle-backend/go && go build ./... && go test ./...

# Frontend
cd scoracle-frontend && npm run typecheck && npm test

# Smoke: vibes endpoint
curl http://localhost:8000/api/v1/nba/player/237/vibes
# expect: current.score (int), current.blurb (string|null), history[].score

# Smoke: sigil endpoint
curl http://localhost:8000/api/v1/nba/player/237/sigil
# expect: commentary.divined_sigil (string|null)

# Smoke: trends sentiment series
curl http://localhost:8000/api/v1/nba/player/237/trends
# expect: entity_season_sentiment_series key (not vibe_series)
```

## Updated File Layout (changed files)

```
scoracle-backend/go/
  cmd/api/main.go                          — synthGen wired to listener.Start + NewRouter
  cmd/pipeline/main.go                     — Stage 4 synthesis added
  cmd/vibesynth/main.go                    — NEW: corpus/backfill runner
  internal/ml/sentiment.go                 — was vibe.go; de-shackled prompt, v5
  internal/ml/stat_commentary.go           — divines Sigil (s3), DivinedSigil field
  internal/ml/vibe_synthesis.go            — NEW: three-pillar SynthesisGenerator
  internal/db/db.go                        — entity_vibes + vibes_leaderboard rewritten
  internal/listener/listener.go            — composite-shift trigger (C.4)
  internal/listener/news_volume_worker.go  — Stage 4 hook (C.1)
  internal/api/handler/handler.go          — synthGen field + SetSynthGen
  internal/api/handler/data.go             — lazy-view synthesis (C.2)
  internal/api/server.go                   — variadic NewRouter
sql/migrations/
  088_rename_vibe_to_sentiment.sql
  089_rename_special_to_sigil.sql
  090_stat_summaries_divined_sigil.sql
  091_vibe_synthesis.sql

scoracle-frontend/src/
  lib/data/vibes.server.ts                 — score + blurb + previous_score
  lib/data/sigil.server.ts                 — divined_sigil field
  components/solid/VibeCard.tsx+css        — score field + blurb render
  components/solid/SigilCard.tsx           — divined_sigil hero label
  components/solid/SigilCard.css
  components/solid/sigil-art.tsx
  components/solid/TrendsCard.tsx          — sentiment series rename
  components/solid/EntityMeta.tsx
  components/solid/card-registry.tsx       — team parity, Sigil name
  lib/data/stats.server.ts
  lib/data/roster.server.ts
  lib/cards/bodies/sigil.ts
  lib/cards/card-meta.ts
  lib/cards/card-registry.tsx
  lib/cards/og-bodies.ts
  lib/router/profile-tabs.ts
  lib/router/share-url.ts
  lib/data/data-sources.ts
```

## Deployed (2026-06-17) — LIVE on archbox

Shipped from archbox: `main` `7f07b18`; migrations 088→091 applied (089 a no-op on the clean DB); `bin/scoracle-api` + cron `bin/pipeline`/`bin/statcommentary`/`bin/vibesynth` rebuilt (prior binary saved `scoracle-api.bak087`); `systemctl --user restart scoracle-api`. All data endpoints 200 internally + via `api.scoracle.com`.

**Post-deploy fixes:**
- `8c528e8` — the synthesis **momentum pillar** filtered the event tables by `entity_type`/`entity_id` (they key on `player_id`/`team_id`) and ordered by `start_time` (which lives on `fixtures`), so `vibe_synthesis` stayed empty (lazy-view, triggers, and nightly Stage 4 all hit it). Fixed → filter by the id column + join `fixtures`. Verified end-to-end: LeBron 237 → vibe 78 + three-pillar blurb, `divined_sigil="Playmaking"`.
- `6dc05ff` — Swagger regen (`swag init`): dropped the phantom `/starline` route, added the live two-rail routes (`/sigil` `/news` `/transfers` `/vibes` `/trends`), switched scope param + descriptions to "sigil". Repointed `cron-vibe.sh` → `bin/sentiment` (superseded by `cron-pipeline.sh`); removed the orphaned `bin/vibe`.

Frontend shipped the same day (`scoracle-frontend` `e6a5a08` → `npm run cf:deploy`, Cloudflare worker `beedc9b3`, live on scoracle.com). divined_sigil + vibe_synthesis populate broadly via the 3am `statcommentary` + midnight `pipeline` crons (+ event triggers / lazy-view); graceful fallbacks until then.
