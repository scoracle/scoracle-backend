# Vibe: skip Gemma when corpus is empty

## Goal

Stop writing `sentiment=50` for entities that have zero matching news and
zero matching tweets. The first batch run after the v2 pivot exposed this:
73 of 78 football players landed at exactly 50 because Gemma was obeying
the system prompt's "answer around 5 if material is thin" fallback. The
flat-50s polluted `/vibe/hottest` and looked like real neutral scores.

## Decision

When both `loadRecentNews` and `loadRecentTweets` return empty, skip the
Gemma call entirely and write a row with `sentiment=NULL`. The API
surfaces NULL as an explicit `"sentiment": null` in JSON (vibeRow.Sentiment
is already `*int`). Frontend renders that as a "no vibe yet" state.

The hottest endpoint already filters `WHERE sentiment IS NOT NULL`, so
skip rows stay out of the feed for free.

The NULL row also serves as a debounce marker: the batch's
`skip-recent-hours` window covers it, so we don't re-attempt the same
empty entity every run.

## Accomplishments

- `ml/vibe.go` — early return when `len(news) == 0 && len(tweets) == 0`.
  New `persistNoCorpus` writer. New `VibeResult.SkippedNoCorpus` flag.
- `cmd/vibe/main.go` — CLI prints `Sentiment: (no data — corpus empty)`
  when skipped.
- `listener/vibe_worker.go` — logs `vibe: skipped (no corpus)` instead
  of `vibe: generated` for the skip path.
- Cleaned up 82 polluted v2 rows (`WHERE input_news_ids='{}' AND
  input_tweet_ids='{}'`) so the live data starts honest.

## Verification

Re-ran `vibe -mode batch -sport all -since-hours 24`:

| sport | entity | scored | null | min | avg | max |
|---|---|---|---|---|---|---|
| FOOTBALL | team | 5 | 5 | 30 | 50 | 70 |
| NBA | player | 4 | 0 | 20 | 45 | 80 |
| NBA | team | 3 | 4 | 20 | 63 | 90 |

Total runtime 1m28s (vs. 6m56s on the polluted run — 9 of 14 candidates
hit the fast skip path that doesn't call Ollama at all). 0 failures.

## Quick reference

```
# What "no data" looks like over the wire
curl :8000/api/v1/football/vibe/team/123
# {"sentiment": null, ...}

# Hottest endpoint stays clean (NULLs filtered)
curl :8000/api/v1/football/vibe/hottest
```

## Follow-ups

- Football corpus is the real bottleneck — 50% of teams scored, 50% had
  no matching news/tweets. Worth widening news + Twitter list coverage
  before adding more football tiers to the batch.
- Consider a periodic backfill that retries NULL rows older than N hours
  (corpus may have arrived since).
