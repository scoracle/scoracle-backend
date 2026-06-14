# 2026-06-14 — Narratives stage: per-narrative model + generator (task 6)

Stage 2 of the confirmed Gemma progression (raw → scrub → **NARRATIVES** → transfer heat → vibe).
Built + verified (dry-run), committed — migration 085 **NOT applied** + persist NOT verified (gated).

## Goal
Replace the single-summary model with **multiple narratives per entity, each its own write-up**:
Gemma groups an entity's vetted corpus into the distinct storylines, scores each with a deterministic
per-narrative impact, and we store one row per narrative.

## What was done
- **085_news_summaries_per_narrative.sql**: restructure the EMPTY news_summaries (081) — `summary`
  → `body`, add `narrative_title`, drop `trending_topics`. A "generation" = the rows sharing
  (entity_type, entity_id, sport, generated_at). `impact` is now PER-NARRATIVE and ranks the
  leaderboard; `input_news_ids` = that narrative's articles; NULL title/body = no-narratives marker.
  Leaderboard index recreated on `body`.
- **ml/news_narratives.go** (new `NewsNarrator`): loads the vetted corpus (wider window,
  maxNarrativeCorpus=25), asks Gemma to group it into narratives — `{title, body, articles[]}` each —
  grounds each narrative back to its articles, computes per-narrative impact (computeNewsImpact over
  that subset), and persists one row per narrative in a single tx sharing generated_at. Sentiment is
  NOT produced here (it moves to the vibe-last stage).
- **cmd/newsnarrate**: dry-run/persist verifier CLI.
- **Retired the superseded single-summary path**: deleted `ml/news_analysis.go` + `cmd/newsanalyze`
  (the one-call summary+sentiment+impact). It was CLI-only (never wired live) and 085 would break its
  persist (renamed/dropped columns). `computeNewsImpact` moved into `news_narratives.go` (its only
  remaining user). vibe.go is untouched.

## Verification
- build + vet + gofmt clean.
- Dry-run on Chelsea (team 18): **3 distinct narratives**, each a named write-up with its own impact —
  "Cucurella pushing for exit / engaging Xabi Alonso" (impact 72, 4 articles), "Chelsea–Newcastle
  multi-star swap talks" (47, 2), "Interest in Morgan Rogers + Tyler Adams" (60, 3). ~102s/entity at
  NumPredict=3000 over 25 articles (heavier than the old 17s single summary — a cadence tuning point
  for task 10; bounded by the 3×/day cron + accelerator).

## State / next
- Committed, NOT deployed. Gated: apply `085` + a persist-run to verify the multi-row write, then
  rebuild + restart at deploy time (batched with tasks 4/5).
- Next: task 9 (vibe as the final stage, determines one overall sentiment from narratives + heat),
  task 10 (orchestrate the staged cadence), task 5 (open gates), task 7 (frontend renders N narratives).
