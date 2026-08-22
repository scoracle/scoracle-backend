# The headline/body contract — one payload per junction, two views of it

**Date:** 2026-08-22 (both drops shipped this session; commits `972a09f`, `ce81643`)
**Companion wiki docs:** `scoracle-wiki/progress_docs/scoracle-backend/2026-08-22_drop1-voice-headlines.md` and `2026-08-22_drop2-headline-contract.md`

## Goal

Scott's product insight: every junction output should be **one payload with a uniform
shape — `{headline, body}`** — so the whole product surfaces in a logical way:

- **Leaderboards** serve the headline + rank score + identity/trajectory metadata.
  Titles fit on a board; prose bodies never belonged there (the old sigil board was
  telling clients to clamp the Oracle's reading to one line).
- **Profile cards** serve `{headline, body}` — the title plus the write-up behind it.
- **Stories** are the Editor's own daily account: what happened today, tagged cast,
  no voice-memory nuance. Dynamic day-to-day, not ranked by banked character scores.

The voices carry nuance because they track the evolving story; stories are "what's
happening TODAY." Lean, nimble, durable: one storage contract, two serving views.

## How we did it

Plan → audit → two additive drops, each gated before touching prod:

1. **Audit cut real complexity.** Re-verifying the plan against code showed the
   Insider needed NO Rust work (`model_summary` is contractually one sentence = its own
   headline), surfaced `insider_scores.read` as never-served Insider voice worth
   exposing, killed a redundant `carried_headline` column (voiced_at already marks
   carry-forwards), and replaced per-board NULL-fallback punts with ONE rule: boards
   omit rows lacking a headline; profiles always render the body. Scout sequenced last
   (profile-card-only consumer). Net: 4 seat contracts planned → 3 shipped, fixture
   cost nearly halved.
2. **Drop 1 — backend-only, additive-safe:** migration + seat contracts + stories heat
   rewrite + wire_read surfacing. Nothing served changed shape except additions, so
   clients could not break.
3. **Drop 2 — the coordinated break,** shipped only after scoracle-frontend/iOS were
   ready: boards lose prose bodies and gain the uniform `headline` key; profiles move
   to `{headline, body}`.

Every drop ran the same gate ladder before prod: local Rust suite / archbox scratch-tree
Go gates → CI (go + validate-stmts vs schema snapshot · shell · schema lineage) →
validate-stmts against live prod schema → EXECUTE smokes of every changed statement
against prod data through the REAL prepared-statement layer (`db.New` + `QueryRow`,
never hand-typed args) → merge only when green → fresh dump → migrate (drop 1) →
`release.sh` → live API smoke → restore drill (drop 1).

## What Changed

### Drop 1 (`972a09f`)

- **mig `226_voice_headlines`:** `headline text` on `sigil_synthesis`,
  `momentum_summaries`, `stat_summaries`. Lazy NULL backfill by design.
- **Oracle → or11:** extractive card title in the crown reply
  (`reading` → `headline` → `score`; grammar-required via format_schema on the live
  route, optional at parse so offline paths never fail). Shared hook-contract +
  foreign-script guards, fail-closed. Version deliberately OUTSIDE the pillar input_hash:
  no fleet regen; crowns re-fire as pillars move.
- **Analyst → momentum-s17:** `HEADLINE:` line contracted after the READ; parser accepts
  any position (order drift ends the READ); same guards.
- **Scout → s20:** closing `HEADLINE:` line split off BEFORE body guards run
  (`split_rating_headline`) so a title can't pollute the brief or its invariant checks.
  Rating's debounce includes prompt_version, so s20 is an intended one-time fleet regen
  on the nightly cadence (s14/s19 precedent).
- **/stories heat rewritten editor-native:** `report_count ÷ (1 + days since latest
  packet compile)` — one commented formula in `story_list`. Journalist `card_score` and
  Influencer sentiment no longer rank the page. Payload keys unchanged.
- **Transfers card serves `wire_read`:** `insider_scores.read` was audit/prompt-memory
  only; now the card's body beside `card_score`. The Insider's voice is user-visible at
  zero generation cost.

### Drop 2 (`ce81643`) — pure Go statements

- Boards → headline-only: news (`narrative_title AS headline`, body dropped),
  transfers (`model_summary AS headline`, omit NULL-summary rows — measured 0 of 1,428),
  vibes (`hook AS headline`, whole-prompt blurb gone, hook-less rows omit — 99% carry
  one), sigil (or11 headline replaces reading; reading leg kept in the marker filter so
  the partial index still narrows; pre-or11 crowns omit until regenerated).
- Momentum boards (vibe + rating): nullable `headline` via latest-per-entity
  `momentum_summaries` join — numeric slopes stay the product, rows never omit for a
  missing headline (the deliberate divergence from the prose-first boards).
- Profiles → `{headline, body}`: /news rename, /transfers per-pair rename,
  /sigil `reading`→`body` + add headline, /momentum `blurb`→`body` + add headline,
  /rating commentary adds headline.
- ENDPOINTS.md rewritten; route inventory reflects the contract.

Unchanged on purpose: Journalist (`narrative_title`), Influencer (`hook`), Editor
(`packets.headline`) already emitted headlines; no migration in drop 2; no Rust in drop 2.

## Verification

- `cargo test --lib` **402 passed / 0 failed** (new parse/guard tests for all three seats)
- Go gofmt/vet/build/test clean in scratch trees; CI green on both PRs (#3, #4)
- validate-stmts vs prod schema before every deploy; all 11 drop-2 statements EXECUTED
  against prod data with automated key assertions (boards carry zero
  body/reading/blurb/summary/prompt keys; profiles carry `{headline, body}`)
- Prod: fresh dumps pre-migration, restore drill VERIFIED (lineage incl. 226, bootable
  backend), release.sh health checks, zero errors/panics post-deploy
- Post-ship health check: both daemons active, 0 errors/60min, Analyst headlines already
  landing (237/24h)

## Result

The uniform contract is end-to-end in production. Storage is one payload per junction
output; leaderboards rank titles; profiles tell the story behind the title; stories are
the Editor's own record of the day. Rollback remains trivial (additive migration ⇒ old
binaries boot; `git checkout <last-good> && release.sh`).

## Follow-Up

- Passive watch: sigil board refills as or11 crowns regenerate (~300/72h cadence);
  Scout fleet regen lands s20 headlines nightly. If the sigil board is still empty after
  the next nightly cycles, inspect the pillar barrier queue.
- If any client surface still reads a removed key (`reading`, `blurb`, `summary`,
  `narrative_title`, board `body`), fix forward — ENDPOINTS.md is authoritative.
