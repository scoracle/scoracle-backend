# HANDOFF — One Rail (the plan is DONE)

**Written:** 2026-08-18, the close-out act (`PLAN-one-rail.md` Phase 9.6).
**Verdict:** the one-rail pipeline is the only pipeline. The legacy two-rail system —
scrub, article_read, the novelty embedder, `news_article_readings`, the episode
lifecycle — is demolished from code (Phase 9.1, 08-08) and from the database
(migs 220/222/224). There is no rollback surface; recovery is `pre-phase9-demolition`
tag + backups.

## What shipped

- **One rail:** nightly RSS sweep → `news_articles` (Editor read enqueued in-txn) →
  the Editor reads every article once (`editor_reads`, contract `ep7`,
  grammar-constrained) → links (`news_article_entities`, Editor sole author),
  Investigator nominations, graph extraction, storyline attach → storyline **packets**
  → the newsroom voices (Journalist n20 / Insider / Influencer v19 / Analyst / Scout /
  Oracle) → product tables → prepared JSON → the cards. `RAIL=packet` live since 08-06;
  the env flag and Rail taxonomy are themselves deleted (08-16 "one rail" commits).
- **Stories as product:** storylines resolve against transfer ground truth
  (`seal_storylines`, nightly 02:45) and `/stories` serves the archive; the tray page
  shipped 08-15.
- **Two-host topology:** prose voices on the Mac's pinned `defiant-fable:9b`
  (parallel 6, num_ctx 4096 everywhere, char-budgeted prompts, cut evidence NAMED);
  editor/graph/momentum/transfers/investigator on the 1070's `ministral-3:3b`.
  Duty cycle 1h-on/1h-off (even hours ON), both machines rest.
- **The watchdog** (08:30/20:30) reads the DATA — ingest recency, per-team editor-read
  coverage, voice output, packet compiles, dead letters, drain-alive, queue depth —
  into `pipeline_runs`, non-zero exit on alarm. `WATCHDOG_ALERT_URL` still unset.
- **Queue discipline (the 08-16 fixes):** enqueue never restamps a pending row's
  `available_at` (re-noticed work keeps its place), and narratives/vibe/sigil claim
  teams before players. This ended the starvation inversion that froze Chelsea's cards
  at Jul 25 while quiet players monopolized the drain.
- **Adjudication under grammar (08-18):** transfer identity adjudication is
  schema-constrained like the Editor (the 3b failed bare json_mode 4/5), and fail-closed
  rows keep the verbatim model reply.

## The baselines (9.3 — the new normal, read at the 08-16/08-18 circle-backs)

- Ingest: ~200 team sweeps, ~1,500 fresh articles, ~2,000 editor reads enqueued/night
  (D-T21 cap: 10 reads/team/day; NFL sweeps ~70/team by volume — team COVERAGE is the
  health signal, not article share).
- Editor outcomes (48h sample, 08-16): ~40% irrelevant, ~27% success, remainder
  blocked/duplicate/empty/fetch/parse — the chaos tail retries itself.
- Products: watchdog-green means every swept team has a read and every voice produced
  within 48h. Teams refresh within a day by construction (team-first claiming).
- Queue: ~7–9k pending is the current normal; **player products accrue ~800/day faster
  than the duty-cycled drain clears them** — the standing capacity question (levers:
  more on-hours, higher 3b parallel, or trimming player inflow to storyline-placed
  players).
- Storylines: ~6k open / ~4k dormant; resolved grows only as ground truth lands
  (the seal is correct; the feed was the 08-18 fix).

## Open decisions (Appendix B leftovers + live items)

- F4 injury gates; national-team entities; out-of-scope clubs (Appendix B).
- The front page (the Stories tray deliberately did NOT take it — Scott's 08-12 call).
- Memory follow-ups D-7/D-8/D-9 — **D-8's graph-demolition guard especially** — and
  D-10's Postgres 19 adoption window.
- `WATCHDOG_ALERT_URL` (one env line + an ntfy topic when Scott picks one).
- statcommentary doesn't write `pipeline_runs` (its own log is healthy).
- The capacity conversation above — the only line on this page that trends worse on
  its own.

## Where the record lives

- The swap's diagnosis and fixes: `progress_docs/2026-08-15_rail-swap-closeout.md`
  (with the 08-16 and 08-18 addenda).
- The plan itself: `planning_docs/PLAN-one-rail.md` — DONE, stamped in its header.
- The tuning ledger continues in `PLAN-character-tuning.md` (alive — tuning outlives
  the rail).
