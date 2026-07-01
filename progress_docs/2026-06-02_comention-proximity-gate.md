# 2026-06-02 — Co-mention precision: title-proximity gate

## Goal

Fix the root cause behind the former-player false positives (Gallagher, Pedro):
the news entity-linker treats a flat article TITLE as a bag of entities, so a
multi-subject roundup like *"Everton want Chelsea striker Liam Delap and Tottenham
Hotspur midfielder Conor Gallagher"* links **Chelsea↔Gallagher** even though
Gallagher is Spurs' player in that sentence (66 title-chars from "Chelsea"). That
spurious co-mention fed the heat engine and the local model/former-gate had to scrub it
after the fact. Lifts quality for **News** (fewer irrelevant articles per entity)
and **Transfers** (cleaner candidates before local model).

## Approach — proximity, not clause-splitting

Splitting titles on `" and "` is unsafe: *"Arsenal and Chelsea battle for Osimhen"*
has two **co-subjects** joined by "and" — splitting would wrongly drop
Arsenal↔Osimhen. A **character-distance** gate sidesteps the ambiguity and is
symmetric (no "primary" anchor needed, so it also handles roundups where neither
entity is the queried one).

`news_article_entities` gains **`title_pos SMALLINT`** — the char offset where the
entity is mentioned in the title, written by the same matcher as the link itself
(`thirdparty.FirstMatchPos`, mirroring `nameInText`'s acceptance exactly). A
team↔player pair counts as a genuine co-mention only when
`|title_pos_team − title_pos_player| ≤ 50` (~7 words). **NULL-tolerant**: a missing
position (legacy / purged entity / not-in-title) is treated leniently so the gate
never silently drops data it can't place.

## What Was Done

- **`033_comention_proximity.sql`** — `ADD COLUMN title_pos`; re-defines
  `compute_transfer_heat` (news corpus join) and `seed_transfer_rumors` (candidate
  CTE) with the proximity predicate. Tweets pass through (short text, no roundup
  risk, no position).
- **`thirdparty/match.go`** — `FirstMatchPos` / `firstMatchPos` (+ `earliest`,
  `wordBoundaryIndex`): earliest match index, or -1. `>= 0` ⇔ `MatchesEntity` true.
- **`thirdparty/news.go`** — `persistArticles` records `title_pos` for the primary
  (looked up in the entity pool) and every secondary link. `BackfillTitlePositions`
  recomputes positions for existing rows from stored titles (one tx per sport).
- **`cmd/comention-backfill`** — thin driver for the one-shot backfill.
- **`ml/transfer.go`** — live `loadCandidates` gets the same gate (const
  `comentionProximityChars = 50`), so spurious players never become candidates.
- **`news_test.go`** — `FirstMatchPos` position/absence, the Gallagher proximity
  case, and agreement with `MatchesEntity`.

## Verification

- `go build` + `go vet` + `go test ./internal/thirdparty/` all green.
- Backfill: **83,487 / 83,534 rows** positioned (99.77%; 195 NULL = purged
  entities), ~6s across all sports.
- **Chelsea candidate discovery: 30 → 24.** The 6 dropped are pure roundup
  artifacts — **Conor Gallagher (0 proximate articles)**, Richarlison (0), Elliot
  Anderson (4→1), Andrey Santos (4→1), André (4→1), Jorrel Hato (2→1). Every
  genuine squad member / target survives (Enzo 51, Pedro 24, Palmer 19, Delap 5,
  Son 7, Mbappé 3…). **This happens before local model runs — independent of the
  former-gate.**
- End-to-end `transfer-cli -team-id 18`: `candidates=24 rumors=17 cleared=7
  errored=0`. Live `/api/v1/football/team/18/transfers` → 17 rows, **none** of the
  6 dropped present, heat tightened to the proximate corpus (Cucurella 59→58,
  Enzo 32→28). API rebuilt + redeployed (new linker now writes `title_pos` on
  fresh fetches).

## Caveats / follow-ups

- **Window = 50 is tunable.** A genuinely far-but-real co-mention (e.g. an `" as "`
  two-clause headline ~26 chars apart) still passes; the local model + former-gate remain
  the backstop for those. Andrey Santos (a real Chelsea loanee) dropping to 1
  proximate article is the precision/recall edge — acceptable, revisit if recall
  complaints appear.
- **Append-model lag:** the gate prevents *new* spurious rows and reshapes heat,
  but a pair that was already written `is_rumor=TRUE` and then drops out of the
  candidate set keeps its last row until superseded. None of Chelsea's 6 dropped
  pairs were live, so no reconciliation was needed; a "cool-off" sweep could clear
  orphaned TRUE rows later if it ever matters.
- News read paths don't consume `title_pos` yet — the column is available when we
  want to tighten the News tab the same way.
