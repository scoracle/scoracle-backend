# 2026-06-14 — Flip consumers onto the vetted link set (task 4)

Task 4 of the News→Gemma pipeline. Built + verified non-regressive, committed — **NOT deployed**
(rebuild + apply 084 + restart is gated). Follows tasks 1–3 (foundation + vetted-flag scrub + async
sweep, all live in prod).

## Goal
Point every consumer of `news_article_entities` at the Gemma-vetted set so news/vibe/transfers stop
reading fuzzy false-positive links — without regressing while coverage is still building.

## Transition filter (non-regressive)
Every consumer gains `(<link>.vetted IS TRUE OR <link>.scrubbed_at IS NULL)` — keep links Gemma
confirmed genuine, plus any not yet scrubbed (shown until judged). Only links the scrub explicitly
rejected (`vetted = FALSE`) are dropped. A later step tightens to `vetted IS TRUE` once coverage is
high. The **033 title-proximity gate is RETAINED** here — it retires with the fuzzy matcher in task 5.

## What was done
- **ml/news_analysis.go** + **ml/vibe.go** `loadRecentNews`: added the vetted filter to the corpus query.
- **ml/transfer.go** `loadCandidates`: gated BOTH the team link (`te`) and player link (`pe`) — `te`
  can be a 0.8 secondary co-mention too, not always the primary. Proximity gate kept.
- **internal/db/db.go** `news_leaderboard` prepared statement: vetted filter in the `counts` CTE so
  "most mentioned" reflects real coverage, not fuzzy noise.
- **084_transfer_vetted_corpus.sql** (migration): `CREATE OR REPLACE compute_transfer_heat` +
  `seed_transfer_rumors` with the vetted filter on the `te`/`pe` joins (proximity gate kept).

## Verification (read-only / rolled-back on prod)
- build + vet + gofmt clean.
- No-regression — Chelsea(18): news corpus links **2091 → 2091** (0 confirmed-false dropped);
  transfer co-mention pairs **60 → 60**.
- Migration 084 validated in a rolled-back tx: both functions redefine; `compute_transfer_heat(18,
  4592198, FOOTBALL)` returns heat=80 / total_14d=7 over the vetted corpus.

## State / next
- Committed, NOT deployed. Deploy = rebuild `bin/scoracle-api` + apply `084` (before restart) + manual
  `systemctl --user restart`. The flip is non-regressive, so it can deploy anytime; precision ramps
  as the scrub ticker builds coverage.
- NOTE (2026-06-14): Scott proposed a Gemma-stage progression — raw → scrubbed → **narratives
  (multiple per entity, each its own write-up)** → transfer heat → **vibe last (richest context)**.
  This reshapes task 6 (generator cadence/order) + the narrative storage model (multi-narrative, not
  one summary) + sentiment-last. Captured for re-planning before task 6; does not affect tasks 4/5.
