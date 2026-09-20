# Seat roles, the shared guard pipeline, and the two-host split

**Date:** 2026-08-23 (all shipped; head `acb82b3`)
**Companion:** `rust/README.md` § Seat Doctrine — the durable rules; this doc is the session record.

## Goal

Scott's diagnosis, verbatim: *"these are all getting meshed together into AI slop... it's
not the training that's the issue, it's slop output."* The Articulator corpus is built
from the five voice cards, so a card that carries someone else's job cannot be summarised
into a clean one. This session fixed the cards, not the training.

## What was wrong, measured

Role bleed across eight well-covered teams — rows are the seat writing, columns the domain
it talked about, `[]` marks its own job:

```text
seat          stat profile  trajectory  emotion  transfers   news
rating             [100%]        12%      25%        0%      25%
momentum              42%      [57%]      42%       28%      42%
vibe                  12%        25%    [87%]       75%      25%
transfers              0%        25%       0%     [100%]     12%
news                  25%        12%       0%      100%     [62%]
```

Off-diagonal average 26%. **No seat was disobeying its contract.** The Analyst's already
asked for exactly the two-rail read; she was narrating her INPUTS, four fifths of which
belonged to other seats. The Influencer called `write_heat_lines` — the identical function
that builds the Insider's prompt — so she recited his ledger.

## What shipped

**Seat roles.** `momentum-s19` (two rails only, every figure rendered as words),
`vibe-v22` (wire temperature, not the Insider's ledger), `rating-s21/s22/s23`
(front-office report on the entity, spanning the z-range, present tense about the current
club, and the z-score renamed "rating" — Scott: *"z-score is going to be meaningless for
99% of our users"*), Oracle trimmed and de-roll-called.

**Prompt diet.** System prompts 9,684 → ~3,900 tokens fleet-wide (−60%). Output budgets
sized by job: the Journalist and Influencer keep the most room because they track multiple
stories; the single-read seats get card budgets. What came out was accumulated rationale
aimed at humans, rules the code already enforces, and shape descriptions the grammar
already enforces.

**The shared guard pipeline** (`guards::clean_served_prose`, `guards::settle_title`).
Two rules every voice needs, previously fixed per-seat in production four and three times
respectively — and never fixed at all on the Journalist or Oracle.

**Pipeline fixes.** `work::VOICE_ORDER` (dependency claim order, unit-tested);
teams-before-players extended to rating/momentum/transfers; **mig 227** took a blocking
`REFRESH MATERIALIZED VIEW` off the write path (throughput 70/hr → ~396/hr).

**Two-host split.** The Mac (M4) joined as a second runner via env only —
`COGNITION_ROUTE_<ROLE>_BASE_URL` — taking the Journalist, Influencer and Oracle.

## Verification

Fixture gates, `--live-system` (frozen inputs, current prompts): momentum **86/86**,
narratives 106/110, rating 82/87, vibe 36/39, oracle **62/76 against 33/60 for the
original it replaced**. Full lib suite 420 passing.

Guard rejections over the session: `digits_in_read` 1,005/24h → 0; failures 63 → single
digits.

---

# NEXT TASK: route injury/suspension events to the Scout

**This is the only designed-but-unbuilt item from the session.** The Scout's card currently
has no idea whether a player is available.

## Scott's design, verbatim

> "The way the Scout should be enqueued now is A. the Editor identifies that an event took
> place and enqueues the Investigator → B. the Investigator is deployed to scrape the box
> score → C. SQL updates the current season stats → D. The Scout is enqueued. I think we
> should use this same strategy for injuries/suspensions, but we need to make sure on an
> event day, the Scout is enqueued one time instead of multiple."

And on why it belongs to him: *"Injuries, suspensions, transfers add to the richness, and
is exactly what a real Scout does."*

## What already exists (do not rebuild)

- **The tag vocabulary is complete.** `bucket::routing_tags_from_story_type` emits
  `injury`, and `suspension` deliberately carries `injury` TOO so existing subscribers keep
  working. `roster` and `transfer` also exist.
- **Routing is DATA, not code.** `stage_routing_subscriptions` rows are `(tag, stage,
  entity_type, note)`. The design comment is explicit: *"Which stage wants `injury` lives
  in `stage_routing_subscriptions`, as data. That keeps the routing decision an INSERT
  rather than a code change."* Current rows: `charged→vibe`, `narratives→narratives`,
  `transfer→transfers`. **The Scout has none.**
- **The Scout already reads confirmed roster moves** — `build_stat_prompt`'s `personnel`
  parameter renders "Personnel changes since our last read" from the adjudicated transfer
  record. That covers Scott's "new player added (crosses the transfer threshold)". Only
  injuries and suspensions are missing.
- **The Editor's `Voice` enum has no `Scout` variant** (`editor/render.rs`) — he is the
  only one of the six not wired to the packet rail.

## The blueprint already exists — copy `rating_work_input_version_for_transfer`

**Read `scout/mod.rs:1865` before writing anything.** Scott asked for the same treatment
transfers already got ("We need the Scout to be aware of when a transfer crossed the
threshold", 2026-08-15), and that implementation documents the two traps this task will hit
and the one obvious fix that is wrong:

1. **Reopening.** `work::enqueue` only reopens a row when `input_version` CHANGED. An injury
   does not move the rating snapshot, so enqueuing with the ordinary stats-derived version
   collapses into the existing row and **nothing runs**. The transfer path keys on
   `application_id` to make each event its own version.
2. **The debounce.** `generate_rating`'s `skip_unchanged` compares the last row's
   `input_hash`, and personnel is deliberately NOT in that pre-image — so even a reopened
   row short-circuits before the model call. `RatingHandler` reads the `xfer` marker and
   turns the debounce off for exactly those items. An availability marker needs the same.
3. **Do NOT put availability in `input_components`.** The existing comment calls this out as
   "the obvious fix and it is the wrong one": changing the hash pre-image re-mints every
   entity's `input_hash` fleet-wide and triggers a full regeneration.

### And this is where Scott's once-per-day constraint comes for free

The transfer marker keys on `application_id` — one enqueue per move. For availability, **key
the marker on the DAY instead of the event**:

```rust
const RATING_WORK_AVAIL_MARK: &str = "avail";
pub fn rating_work_input_version_for_availability(season: i32, day: &str) -> String {
    format!("{RATING_WORK_PREFIX}{season}:{RATING_PROMPT_VERSION}:{RATING_WORK_AVAIL_MARK}{day}")
}
```

Every injury or suspension for that entity on that date produces the **same**
`input_version`, so `work::enqueue`'s `ON CONFLICT ... WHERE input_version IS DISTINCT FROM
EXCLUDED.input_version` collapses them into one row and the Scout runs **once that day** —
exactly the requirement, with no new debounce table. Mirror
`rating_work_is_transfer_triggered` so `RatingHandler` also disables `skip_unchanged` for
availability items.

## Known risks

- **Coupling a statistical card to the news cadence.** Today the rating stage is triggered
  by stat recomputation only. Subscribing it to `injury` means a busy injury week
  regenerates rating cards more often. That is probably desirable — availability changes
  what the profile means — but it is a real behaviour change on the seat with the slowest
  drain.
- **The Scout is already the slowest seat to refresh.** Its normal enqueue is the nightly
  `statcommentary` batch: 400 per night across 2,257 targets, so a team gets re-rated about
  every six days. Adding a second trigger helps, but see below.
- **A `prompt_version` bump regenerates NOTHING on its own.** This bit us three times
  today. The item must be enqueued. Forced re-enqueues were done by hand this session
  (`INSERT ... ON CONFLICT DO UPDATE SET status='pending'` over `public.teams`).

---

## State to resume from

- **Deployed head:** `acb82b3` (archbox + Mac both serving `ministral-3:3b`, ollama 0.32.x).
- **Draining now:** 758 team-grain items force-queued at session end (204 vibe, 204
  transfers, 158 rating, 192 sigil). Expect all five corpus voices current on ~204 teams
  within hours.
- **The verification gate before corpus work:** `scoracle-articulator/eval/voice_conformance.py`
  — checks each seat against its stated job and splits results BY CONTRACT VERSION, which is
  what made the stale-drain diagnosis visible.
- **Untouched:** the Editor's article cap. The ollama 0.32 upgrade freed ~1.9 GB on archbox,
  making an 8192 window feasible (~71% of card), which would roughly triple the article
  budget. It is a trade, not a free win: bigger prefills on a power-capped card mean fewer
  articles per hour.
