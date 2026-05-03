# Vibe: corpus-driven mode (RSS pre-warm + Gemma queue)

## Goal

Close the long-running NFL/Football vibe gap. NFL had 0 scored rows ever
(out-of-season → no fixtures → no batch candidates). Football had 1,536
no-corpus marker rows for every 94 real scores because the long-tail
players never got browsed by users → never got news write-through.

Same pattern: today's `cmd/vibe -mode batch` picks candidates from
`fixtures` and *hopes* corpus exists. New approach: use *fresh corpus
presence* as the candidate signal.

## Decision

Two phases in a new `-mode corpus`:

1. **RSS sweep** every team in NBA/NFL/FOOTBALL (~158 teams) by reusing
   `NewsService.GetEntityNews` — same code path as the public `/news`
   endpoint, so write-through and cross-entity linking are identical
   to user traffic.
2. **Gemma queue** runs only against the `news_article_entities` rows
   created at-or-after `runStart`. The set includes the queried teams
   plus any players co-mentioned in article titles via the existing
   cross-entity matcher in `persistArticles`. Player coverage is free.

X stays lazy demand-driven (per
`~/scoracleWiki/raw/scoracle-x-api-architecture.md` — keeps the X Basic
budget reserved for user traffic).

## Accomplishments

- `cmd/vibe/main.go` — new `-mode corpus`. Adds `runCorpus`,
  `loadTeams`, `loadTouchedEntities`, `recentlyVibed`, and
  `lookupEntityNameCtx`. New flags: `-corpus-skip-recent-hours`
  (default 10), `-corpus-rss-pause-ms` (100), `-corpus-rss-limit` (10).
- `scripts/hosting/cron-vibe.sh` — header rewritten. Recommended
  crontab is now `0 0,12 * * * cron-vibe.sh -mode corpus`. Legacy
  `-mode batch` still works for backfills.

The fixture-driven batch loader is left in place but no longer the
canonical path. Leave it for one cycle in case the corpus path misses
anything obvious.

## Verification

NFL smoke run (`-sport NFL -corpus-skip-recent-hours 0
-corpus-rss-limit 5`):

```
corpus: rss sweep starting    sport=NFL teams=32
corpus: rss sweep complete    ok=32 fail=0 elapsed=35s
corpus: gemma queue starting  candidates=59
corpus: progress              done=25 total=59 ok=25 fail=0 skipped=0 no_corpus=0
```

After the run (only ~half the queue completed before I killed it):

| sport | entity_type | scored | null | range | sample |
|---|---|---|---|---|---|
| NFL | player | 27 | 0 | 60–90 | Baker Mayfield 90, Jahmyr Gibbs 90, CeeDee Lamb 80 |
| NFL | team   |  4 | 0 | 60–70 | Jets 70, Dolphins 60 |

This was a sport with **zero** scored vibes ever. 31 real-input scores
in ~10 minutes from a 5-article-per-team RSS sweep, with **0 no-corpus
rows** — exactly the design goal. Cross-entity linking grabbed 27
players from the team articles for free.

## Quick reference

```bash
# Manual full run
./go/bin/vibe -mode corpus

# Single sport (smoke / debug)
./go/bin/vibe -mode corpus -sport NFL

# Tighter throttle for back-pressure
./go/bin/vibe -mode corpus -throttle-ms 200 -corpus-rss-pause-ms 200
```

Recommended crontab (local time, twice daily):

```
0 0,12 * * * /home/sheneveld/scoracle-data/scripts/hosting/cron-vibe.sh -mode corpus
```

## Follow-ups

- Per-run wall time scales with touch-set size. NFL smoke was ~12s per
  Gemma call; a full 3-sport run could be 60–90 minutes. If it bumps
  the next cron run, raise `-corpus-skip-recent-hours` past 12.
- Decide whether to retire the fixture-driven `-mode batch`. Probably
  yes — strictly dominated by corpus mode for the headliner+starter
  case, and the cross-entity matcher catches active players.
- `news_articles` will grow ~150–500 rows/day net-new. Migration 006
  has no purge job. Schedule a follow-up purge once the table tops 1M.
- Open question parked in `~/scoracleWiki/raw/Vibe score enhancements.md`:
  use a different `match_confidence` for scheduled write-through
  (e.g. 0.95) so we can later separate "user discovered" from
  "scheduled" in training-corpus stats.
