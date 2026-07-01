# 2026-06-13 — Unified local model news-analysis generator (Stage 2a)

## Goal
Build + verify the unified per-entity news analysis: ONE local model call over an entity's recent
news corpus → prose summary + trending topics + 1-100 sentiment, plus a deterministic
0-100 impact (the news analog of transfer heat). Stage 2a of the vault plan
(`wiki/Plan - News to local model Summaries.md`).

## What Was Done
- `internal/ml/news_analysis.go` — `NewsAnalyzer.Analyze`, mirroring `ml/vibe.go`
  (corpus loader → prompt → local model → parse → persist). One `ollama.Generate` returns strict
  JSON `{sentiment, summary, trending_topics}`. `computeNewsImpact` derives a transparent
  0-100 impact from the corpus (saturating volume + distinct-source corroboration +
  recency) — local model never invents the number. Persists summary+topics+impact to
  `news_summaries`; a NULL-summary marker when no corpus (the vibe `persistNoCorpus` analog).
  A `DryRun` flag skips persistence for verification.
- **Additive + non-regressive:** does NOT modify `vibe.go`. For now it persists ONLY to
  `news_summaries` and *returns* the sentiment; wiring it into the live trigger (to write
  both tables from one pass and supersede the separate vibe call) is the next step.
- `cmd/newsanalyze/main.go` — CLI to dry-run/persist the analysis for one entity (mirrors
  `cmd/vibe` single mode).

## Verification (live, dry-run — no persistence, no prod change)
`go run ./cmd/newsanalyze -entity-type team -entity-id 18 -sport FOOTBALL` (Chelsea, 117
recent articles, 12 used), one local model call, 10.4s:
- Sentiment 68/100, Impact 95/100 (volume 54.6 + corroboration 25 + recency 15; 11 distinct sources).
- Trending: Cucurella transfer / transfer negotiations / incoming targets / coaching appointments.
- Summary: a coherent, original 4-sentence recap (Cucurella departure hints, swap-deal
  talks, City/Bayern competition, ex-Chelsea coach at Brentford) — no links, no headline
  dump, impactful-first. Exactly the intended value.

`go build ./...` + `go vet` clean.

**Prompt v2 (Scott feedback — "comprehensive, name-drop, not a teaser"):** widened the prompt
to a full briefing that names specific people/clubs (no genericizing "a Real Madrid star"),
bumped NumPredict 1200→2200 for the longer output. Re-run on Chelsea: sentiment 61 / impact
95, 13s, and a comprehensive recap naming Cucurella, Barcelona, Bayern (€25m bid), Xabi
Alonso, Man City, Real Madrid, Enzo Maresca, the seven "untouchables", four completed
transfers, and the Newcastle/Chelsea coach at Brentford. Markedly richer than v1.

## Result
The local model news-analysis generator is built and producing high-quality summaries on the
live corpus. Not yet persisted/deployed (news_summaries migration pending the batched
deploy). Verified the core value play before taking the handcuffs off news collection
(Stage 1 ID-gate + loosened RSS) — the next step.

## Notes / follow-ups
- `impact` v1 saturates near the top for big clubs (Chelsea = 95); leaderboard
  discrimination is a tunable refinement (source-tier weighting, velocity).
- Next: Stage-1 local model ID-gate (reuse the transfer subject-resolver) + loosen the RSS
  matching; then wire the trigger/cron + endpoint, write both tables from one pass.
