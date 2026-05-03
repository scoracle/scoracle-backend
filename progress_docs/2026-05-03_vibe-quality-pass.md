# Vibe quality pass: 48h tweet filter, v3 prompt, corpus filter alignment

## Goal

Three issues surfaced after a day of corpus-mode runs:

1. **Tweet feeds were serving multi-week-old posts.** The 24h `fetched_at`
   purge was working, but X List re-pulls (pinned, retweeted) kept
   refreshing `fetched_at` on tweets whose `posted_at` was weeks old. The
   read endpoints had no `posted_at` filter, so users saw 4/16 posts on
   5/3.
2. **Sentiments were always multiples of 10.** The v2 prompt asked Gemma
   for 1-10 then multiplied by 10 on persist. Output read low-resolution
   (always 50/60/70/80/90).
3. **Corpus mode was still writing null-sentiment markers.** `loadTouchedEntities`
   queued any entity with a fresh `news_article_entities` row, but
   `Generate()` filtered articles by `published_at > NOW() - 72h`. An
   entity whose only fresh link pointed to a 30-day-old article got
   queued, then null-skipped — defeating the design promise that corpus
   mode never writes nulls.

## Decisions

### 1. Tweet read filter: 48h posted_at

Added `AND posted_at > NOW() - INTERVAL '48 hours'` to both
`twitter_feed_by_sport` and `twitter_feed_by_entity` prepared statements
in `go/internal/db/db.go`. Updated the `feed_size` meta count to match
the visible window. Hardcoded 48h is intentional — short enough to feel
fresh, long enough that a slow news Sunday still has tweets to show.

The TTL purge stays at 24h on `fetched_at` (X ToS); the 48h read filter
is a presentation concern.

### 2. Vibe prompt: v3 (1-100 direct scale)

Bumped `vibePromptVersion` to `v3`. New system prompt:

```
Reply with ONLY a single integer from 1 to 100.
1 = overwhelmingly negative, 50 = neutral or mixed, 100 = overwhelmingly positive.
Use the FULL range with precision. Pick exact numbers like 47, 63, 78, 92.
Avoid round multiples of 10 (50, 60, 70, 80, 90) unless the evidence genuinely lands there.
…
```

Dropped the `×10` multiplier in `parseSentiment`; clamp now 1-100.
Updated `vibe_test.go` for the new range.

The `prompt_version` column already tracks this — v2 and v3 rows
coexist. Existing v2 rows stay valid (still numeric 1-100). The
frontend doesn't care.

### 3. Corpus candidate filter: align with news lookback

`loadTouchedEntities` now joins `news_articles` and applies the same
72h `published_at` filter `Generate()` uses:

```sql
WHERE nae.created_at >= $runStart
  AND (a.published_at IS NULL OR a.published_at > NOW() - INTERVAL '72 hours')
```

Single source of truth: exported `ml.NewsLookback` and used it from the
candidate query so the two windows can't drift apart. Entities with only
stale-but-newly-linked corpus are dropped at queue time instead of
producing null markers.

## Accomplishments

- `go/internal/db/db.go` — 48h `posted_at` filter on tweet feeds.
- `go/internal/ml/vibe.go` — v3 prompt; `parseSentiment` 1-100 direct;
  exported `NewsLookback`.
- `go/internal/ml/vibe_test.go` — covers the new range.
- `go/cmd/vibe/main.go` — `loadTouchedEntities` joins articles and
  applies the 72h filter.

## Verification

Smoke run (`./go/bin/vibe -mode corpus -sport NFL
-corpus-skip-recent-hours 0 -corpus-rss-limit 3`):

```
corpus: rss sweep complete  ok=32 fail=0 elapsed=35s
corpus: gemma queue starting  candidates=50
corpus: complete              ok=50 fail=0 skipped=0 no_corpus=0
```

Result table:

| entity_type | rows | non_round | min | avg | max |
|---|---|---|---|---|---|
| player | 18 | 18 | 21 | 53 | 91 |
| team   | 32 | 32 | 32 | 67 | 92 |

- **0 nulls, 0 fails** — corpus filter alignment works as designed.
- **50/50 non-round** — v3 prompt eliminated the multiples-of-10 bias.

Sample sentiments: 21, 32, 47, 53, 67, 73, 79, 82, 91, 92.

`go test ./internal/ml -run TestParseSentiment -v` — all 12 cases pass.

## Quick reference

```bash
# Restart the API to pick up the new tweet feed prepared statements
killall scoracle-api 2>/dev/null
./go/bin/scoracle-api &

# Verify the 48h tweet filter is active
curl -s :8000/api/v1/twitter/sport/nba | jq '.tweets[0].created_at, .meta.feed_size'

# Backfill a single entity with v3
./go/bin/vibe -entity-type team -entity-id 13 -sport NFL
```

## Follow-ups

- The legacy `-mode batch` path still produces null markers because it
  picks candidates from `fixtures` without a corpus check. The 2026-05-16
  remote agent will recommend retiring it pending DB-side comparison.
- Consider whether `prompt_version='v2'` rows should be backfilled to v3
  for visual consistency. Decision: not yet — v3 already discovered new
  range (21, 32, 47), so backfill would be cheap to add later if
  inconsistency is annoying in the UI.
- `feed_size` in tweet meta now reflects the 48h window; if a frontend
  surface was using it as a "total cached" counter, that semantics shift
  is worth a pass before next dev cycle.
