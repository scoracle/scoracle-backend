# 2026-06-13 — News→Gemma pipeline: foundation session (Twitter out, generator, scrub)

A landmark session: pivoted news from external-article aggregation toward in-house Gemma
intelligence. Everything below is **committed locally, batched for one deploy — prod
UNCHANGED** (every Gemma run was a dry-run; no live API/DB change). Twitter commit is pushed.

## Goal
Kill the platform's only multi-second upstream (live Twitter), and stand up the Gemma news
pipeline: one-pass per-entity analysis (summary + sentiment + impact) and the "scrub"
(Gemma ID-gate that disambiguates the fuzzy link table). Companion plans in the vault:
`Plan - News to Gemma Summaries.md` (what), `Plan - News pipeline integration.md` (how/next).

## What was done (in order)
1. **Suspend Twitter** (`634e364`, pushed) — `TWITTER_ENABLED` master switch (default off,
   mirrors `TRANSFER_ENABLED`). `TwitterService.enabled` gates `HasBearerToken()` +
   `IsConfigured()`, so the entity-feed handler skips the ~11s `GetSportFeed` live fetch.
   Vibe unaffected (reads tweets cache-only + news). Re-enable = `TWITTER_ENABLED=true`.
2. **Schema** (`6adf30c`) — `081_news_summaries` (append; `summary`, `trending_topics`,
   deterministic `impact` 0-100 + components, `as_of` history via `generated_at`) +
   `082_pipeline_stats` (daily corpus/asset-growth snapshot). Both validated in a rolled-back
   tx (syntax + `sports` FK), not applied.
3. **Unified analysis generator** (`3c96bad`, `99c468b`) — `ml/news_analysis.go` +
   `cmd/newsanalyze`. ONE Gemma call → `{sentiment, summary, trending_topics}`; deterministic
   impact in Go. Prompt **v2** (comprehensive, name-dropped). Additive (doesn't touch
   `vibe.go`; persists only `news_summaries` for now).
4. **Gemma scrub / ID-gate** (`f03558b`) — `ml/news_scrub.go` + `cmd/newsscrub`. Vets the
   fuzzy link table per article via identity cards (name · nationality · canonical current
   club · position); disambiguates same-name people. Primary (1.0) link preserved; secondary
   (0.8) guesses vetted. DryRun reads only; persist deletes dropped links.

## Verification (live, dry-run only)
- **Analysis** on Chelsea (117 articles, 12 used): one call, ~13s, sentiment 61 / impact 95,
  a comprehensive name-dropped recap (Cucurella, Xabi Alonso, Maresca, Bayern €25m, the seven
  "untouchables", the Brentford-bound coach).
- **Scrub** on Chelsea: **killed the fuzzy "Roma" link that came from the journalist
  "Romano"**, dropped incidental "Portu"/"Son", kept genuine subjects; preserves the primary
  team link (fixed a bug where it first dropped Chelsea on the Cucurella article).
- `go build ./...` + `go vet` + touched-package tests clean throughout.

## Key finding (measured)
The fuzzy matcher and the transfer proximity gate (`033`) share `news_article_entities`, so
you can't move one without the other: tightening the gate halved FOOTBALL transfer candidates
(532→245); loosening the matcher floods transfers (NULL `title_pos` passes the gate).
**Therefore "open the gates" == "build the scrub"** — they ship together (the scrub vets, so
both the fuzzy gate and `033` retire at once). Do NOT loosen the matcher before the scrub is
the precision pass.

## State / next
- 5 backend commits (Twitter pushed; schema + 2 generators local), staged for ONE batched
  deploy (apply `081`/`082` before the API restart; rebuild binary → path-watcher restarts).
- Next = **integration** (`Plan - News pipeline integration.md`): scrub `vetted` flag + async
  wiring → flip consumers → open the gates + retire `033` → generators into cron/trigger →
  frontend. Then the parked fast-follow: **Gemma stat-profile summaries**
  (`Plan - Gemma stat-profile summaries.md`).

## Result
Twitter de-risked behind a flag; the two hardest Gemma pieces (analysis + scrub) built and
verified on the live corpus. The non-stats value play is real — Gemma summarizes *and*
disambiguates; the scarcity-era name-matching rules are obsolete.
