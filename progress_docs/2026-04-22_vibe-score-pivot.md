# Vibe pivot: blurb → sentiment score

## Goal

Replace the narrative Vibe blurb with a single numeric sentiment score (1-100)
so the frontend can paint its own emoji/color bucket. Primary motivation: cut
Gemma output tokens per call so we can score the long tail of entities and
support a "hottest" feed, instead of the blurb path's headliner-only real-time
/ starters-only daily budget.

## Decisions

- **Scale**: Gemma rates 1-10 internally, generator multiplies by 10 to store
  1-100. Smaller output space stabilizes quantization on a small model; the
  column stays 1-100 so we can widen later without another migration.
- **Keep `blurb` column (nullable)** for future hybrid mode (score + short
  internal rationale). Generator currently writes NULL.
- **`NumPredict: 1200`** — ended up *higher* than I'd hoped. Gemma 4 e4b burns
  predict tokens on internal chain-of-thought before any visible output; too
  low a cap returns empty (`done_reason=length`, response empty). 256, 768
  both silently failed on large-corpus prompts (~3.8k chars). 1200 is empirically
  safe. The energy savings still come from the *actual* eval_count — we emit
  one digit vs. ~40 visible tokens of prose in v1 — so the looser cap is free.
- **Hottest endpoint**: `/api/v1/{sport}/vibe/hottest?entityType=&limit=`
  collapses to latest-per-entity in a 48h window, orders by score DESC.
- **Inline SQL** in the handler (not prepared statement), matching the
  existing vibe handler precedent. If this endpoint gets hot, promote to
  `db.go` then.

## Accomplishments

- Migration `009_vibe_sentiment.sql` — adds `sentiment SMALLINT` with CHECK
  (1..100), drops `NOT NULL` on `blurb`, adds partial index
  `idx_vibe_scores_sport_sentiment` covering the hottest query.
- `ml/vibe.go` — prompt `v2`, `parseSentiment` (regex first digit, clamp
  1-10, scale to 1-100), persistVibe writes the score.
- `ml/vibe_test.go` — 10-case table test (all green).
- `api/handler/vibe.go` — `vibeRow.Sentiment *int` (nullable for pre-v2
  rows), new `GetHottestEntities` handler.
- `api/server.go` — route wired at `/{sport}/vibe/hottest`.
- `cmd/vibe/main.go`, `listener/vibe_worker.go` — print/log the score.

## Verification (live run, 2026-04-22)

Migration applied against local Postgres, five v2 generations:

| entity | sentiment | duration | corpus size |
|---|---|---|---|
| Devin Booker (player 57) | 50 | 10.9s | 1 news / 0 tweets |
| Bam Adebayo (player 4) | 20 | 16.0s | 0 news / 2 tweets |
| OG Anunoby (player 18) | 30 | 15.3s | 2 news / 0 tweets |
| Lakers (team 14) | 80 | 50.2s | 12 news / 8 tweets |
| Deandre Ayton (player 22) | 80 | 11.2s | 2 news / 0 tweets |

Score distribution is real (not stuck on 50/70 as feared). Hottest-entities
SQL returns the expected DESC ordering.

## Quick reference

```
# Apply migration
psql $DATABASE_PRIVATE_URL -f sql/migrations/009_vibe_sentiment.sql

# Generate a single score
./go/bin/vibe -entity-type player -entity-id 57 -sport NBA

# Hottest endpoint
curl ':8000/api/v1/nba/vibe/hottest?limit=10'
curl ':8000/api/v1/nba/vibe/hottest?entityType=player&limit=5'
```

## Follow-ups worth considering

- **Team prompts are slow** (Lakers = 50s on local CPU-bound Ollama). If we
  want the long-tail coverage the pivot was designed to unlock, we may need
  to either trim `maxNewsItems`/`maxTweetItems` for team entities, or test
  whether `think: false` works on Gemma 4 via the Ollama API to skip
  reasoning entirely.
- **Batch coverage expansion**: current batch still filters to
  `starter`/`headliner` tiers. With score-only generation we could reasonably
  widen to `bench` tier in off-hours. Leaving gated until we observe
  sustained run times on Sunday NFL load.
- **Hybrid mode**: reintroduce blurb writes behind a flag for explainability
  on surprising scores. Schema already accommodates (blurb is nullable).

## Updated file layout

No directory changes. Touched files:

```
sql/migrations/009_vibe_sentiment.sql            (new)
go/internal/ml/vibe.go                           (prompt v2, parseSentiment, persist)
go/internal/ml/vibe_test.go                      (new)
go/internal/api/handler/vibe.go                  (Sentiment, GetHottestEntities)
go/internal/api/server.go                        (route)
go/cmd/vibe/main.go                              (print sentiment)
go/internal/listener/vibe_worker.go              (log sentiment)
```
