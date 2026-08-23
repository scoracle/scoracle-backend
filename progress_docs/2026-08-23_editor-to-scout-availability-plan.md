# Editor → Scout: routing injury/suspension events to the scouting card

**Status: PARTIALLY BUILT.** This builds out the `NEXT TASK` block of
`2026-08-23_seat-roles-and-the-guard-pipeline.md`. Read that first for Scott's verbatim design.

| Phase | State |
|---|---|
| 0 — source of truth | **Decided** (a): an adjudicated record |
| 1 — the record | **BUILT** — `sql/migrations/229_player_availability.sql`, unapplied |
| — a bug found on the way | **BUILT** — `sql/migrations/228_stat_summaries_trigger_type_widening.sql`, unapplied |
| 3 — the trigger | **BUILT** — `scout/mod.rs`, 424 lib tests green, no new clippy warnings |
| 2 — the read | **NOT BUILT** — `load_personnel_changes` still selects transfers only |
| — the writer | **NOT BUILT** — nothing creates a `player_availability` row yet |
| 5 — fixtures + gate | **NOT BUILT** |
| propensity | **DEFERRED** — needs closed spans; every span is open on day one |

**Neither migration has been applied and neither can be validated here** — there is no `psql` on
this machine, so both are unrun SQL. Apply 228 first; it stands alone and fixes a live defect.

## Caught up to `f50c382` (2026-08-23 cleanup)

Three changes since `d5e019a` bear on this task. Every code anchor below survived them.

- **`RATING_PROMPT_VERSION` is now `s24`** (the HOOK pass), so the `input_version` this task mints
  carries `s24`. That is correct and needs nothing — but note the standing trap: the s24 bump
  regenerates NOTHING by itself, so every card in production is still s23 prose until something
  enqueues it.
- **The two-host split fixed the starvation this task depends on.** `MAC_SLOTS` moved
  narratives/vibe/sigil off the archbox card; before it, Mac-routed seats filled the four shared
  archbox slots with work that card never ran, and **rating claimed 10 cards in 12h against 928
  ready rows**. An availability trigger landing into that would have looked like it did nothing.
  It is fixed, which is what makes this task worth shipping now.
- **`guards::settle_title` now exists** and the eval gate measures what SHIPS (salvage-then-serve)
  rather than the raw hook. Phase 5's fixtures should assert through it, not around it.

This document exists because two claims in that handoff do not survive contact with the code,
and one whole half of the task is missing from it.

---

## Finding 1 — "wiring the Scout is an INSERT, not a code change" is wrong

The handoff says both of these, and they are mutually exclusive:

> Routing is DATA, not code … wiring the Scout is an INSERT, not a code change.

> Copy `rating_work_input_version_for_transfer` … key the marker on the DAY.

The subscription route runs through mig 225's `enqueue_voices_on_packet`, arm 1, which
**hardcodes** the queue fingerprint:

```sql
'pk:' || COALESCE(NEW.slice_fingerprints ->> s.stage, NEW.id::text)
```

An `INSERT INTO stage_routing_subscriptions ('injury','rating','player')` therefore fails three
ways at once:

1. **It cannot mint the marker, so nothing runs.** The `pk:` version carries no `avail` mark, so
   the `RatingHandler` debounce check (`scout/mod.rs:1892`) returns false, `skip_unchanged` stays
   ON, and `generate_rating` short-circuits before the model call. This is trap 2 from the
   handoff — the INSERT path walks straight into the trap the same document warns about.
2. **It breaks Scott's once-per-day rule rather than getting it for free.** There is no `"rating"`
   key in `slice_fingerprints` — `editor/packet.rs:180` builds exactly three: `narratives`,
   `vibe`, `transfers`. So the `COALESCE` falls through to `NEW.id::text`, the packet id. Every
   injury packet that day is a **distinct** `input_version`, every one reopens the row. Five
   injury stories on a Saturday is five Scout runs on the slowest seat in the fleet — the precise
   outcome Scott asked us to prevent.
3. **The season stops parsing.** `rating_work_season` expects the `rating:s<season>:` prefix and
   returns `None` on a `pk:` version, falling back to `current_season`. Benign today, but it
   silently drops the season pinning that the marker format exists to preserve.

**Decision: the enqueue is Rust, in `scout/mod.rs`, next to the transfer helper it copies.** No
`stage_routing_subscriptions` row is added for the Scout. Phase 4 records this so a later session
does not "simplify" the Rust enqueuer back into the INSERT this handoff recommends.

## Finding 2 — the Scout has nothing to say when he wakes up

This is the larger gap, and the handoff does not mention it.

`grep` over every migration finds **no injury or suspension table and no availability column**.
The only occurrences of those words in the codebase are classification vocabulary — the Editor's
`story_type` taxonomy, `bucket::routing_tags_from_story_type`, the guards.

The record the Scout reads is `load_personnel_changes` (`scout/mod.rs:887`), and it selects from
**`transfer_identity_applications` only** — the `applied`/`reverted` arms of the adjudicated
transfer chain. That is the entire content of the `personnel` block.

So a Scout woken correctly by an injury reads a card containing **zero availability facts**, and
his s21 rule (*"Availability is part of the profile … never speculate past what is recorded"*)
correctly forbids him from inventing any. Best case he re-renders a byte-identical card. The wake
is pure waste on the seat with the slowest drain.

**The trigger is the LAST step of this task, not the first.** Order: record → read → trigger →
gate.

---

## Phase 0 — where availability facts come from (needs Scott)

Scott's design is `Editor spots event → Investigator scrapes → SQL updates stats → Scout
enqueued`. For a box score, step C is the stats write. **For an injury there is no step C.** It
has to be built, and there are two shapes:

**(a) Adjudicate availability into its own record — RECOMMENDED.** A `player_availability` table
mirroring `transfer_identity_applications`: pending → applied, with a revert path. The
Investigator writes it, the Scout reads dates and enums only. This is the only shape that keeps
**T4** (`editor/render.rs:44` — *"The Scout is absent by construction … confirmed facts reach it
by the stats platform and `transfer_identity_applications`, never by packet prose"*) and the only
one his s21 rule can speak from.

**(b) Route the Editor's claim prose to the Scout — REJECTED.** `Voice` has no `Scout` variant
deliberately, and the law is enforced by the type rather than by a reviewer's memory. This would
also hand unadjudicated prose to a 3B model, which ar3/ar5/ar6 already measured as unreliable.

**RESOLVED 2026-08-23: Scott chose (a)** — *"we need to build an injury/suspension table … isn't
that the durable fix?"* — and added injury propensity as a second consumer, which is what fixes
the shape of the record in Phase 1.

## Phase 1 — the record (`sql/migrations/228_player_availability.sql`)

Scott, 2026-08-23: *"Isn't that the durable fix? It'll also let us know if an entity has an
injury propensity."* Propensity is the reason the shape below is an **event log with closed
spans**, not a status flag — and it forces one distinction the transfer table does not have.

```sql
id, sport, player_id, team_id,
kind            -- 'injury' | 'suspension'  (the Editor's story_type already separates these)
status          -- 'pending' | 'applied' | 'rejected'
event_date      DATE          -- the DAY. What the marker keys on.
expected_return DATE NULL     -- the REPORTED prognosis at event time. A claim, not an outcome.
returned_at     DATE NULL     -- availability actually resumed. The propensity denominator.
reverted_at     TIMESTAMPTZ NULL  -- this RECORD was wrong. Not a return. See below.
body_part       TEXT NULL     -- nullable, never guessed. See below.
source_article_id, applied_at, created_at
```

- **A table, not a flag on `players`.** The revert case is why. `load_personnel_changes`'s doc
  comment already argues this for transfers: *"a correction to a move the last brief was written
  around is invisible"* if you only keep current state. Propensity makes the argument twice over
  — a flag has no history to count.
- **`returned_at` and `reverted_at` are different facts and must not share a column.**
  `reverted_at` on the transfer table means *the adjudication was wrong*. An injury also needs
  *the player came back*, which is a real-world outcome, not a correction. Collapsing them
  corrupts propensity at the root: a retracted false report and a genuine three-week absence
  would be indistinguishable, and days-out would be computed off both. This is the one place
  where copying `transfer_identity_applications` verbatim is the wrong move.
- **`expected_return` is a third, distinct thing** — the prognosis as reported. Useful for the
  card ("expected back Sep 02"), useless as ground truth. Never compute propensity from it.
- **`event_date` is a DATE and it is what the day-marker keys on.** If it is a timestamp, or if
  the marker is rendered from local time rather than a fixed zone, one event day splits across
  two `input_version`s and the once-per-day collapse silently stops collapsing.
- **`body_part` ships nullable and empty.** Recurrence — the propensity signal people actually
  want — needs it, but the Editor cannot supply it today: `story_type` is a single enum value
  (`editor/prompt.rs:118`) with no structured detail, so extracting it means a new field on the
  Editor's contract, which is a prompt change governed by the prompt-vs-guard law. The column
  costs nothing now and a schema change later costs a migration on a live table. **Propensity v1
  is frequency and days-out, not "hamstring recurrence."**

### The bias trap that has to be designed against

A news-derived injury record is biased by **coverage, not by health**. A starter gets his knock
written up; a squad player's does not. **Absence of records is not absence of injuries**, and a
Scout who says "injury-prone" off two events is reporting our crawl density as if it were a
medical finding — precisely the slop Scott diagnosed on 2026-08-22.

Two defences, both house patterns:

1. **Code decides the label, the Scout voices whether it matters** — the `build_scouting_decision`
   / s19 movement-line discipline. Render counted facts and a thresholded label; never ask the
   model to judge propensity from raw rows.
2. **Gate on a minimum record density** and suppress the line entirely below it, rather than
   rendering a weak one. Fail-open to silence, the same discipline as an off-vocabulary
   `story_type` routing to nobody.

## Phase 2 — the read (`scout/mod.rs`)

**The two facts go to two different places, and the codebase already drew the line.**
`load_personnel_changes`'s own doc comment says it: *"The memory card keeps its slow cross-season
arc lines; this block is the delta."*

- **Current availability → the personnel block.** Who is out right now, since when, expected
  back. That is a delta since the last read.
- **Propensity → the cross-season memory card** (`load_stat_memory`, `:757`, over
  `stat_context_for_entity`, mig 164). Frequency and days-out across seasons is an arc line by
  definition, and putting it in the delta block would re-render the same slow fact on every read
  while competing for the six-line cap.

This split is free on the invariant that matters: `:1588` records that the memory card is
*"best-effort, prompt-only, outside `input_components`/`input_hash`"* — the same flag the
personnel block travels on. **Both reads stay outside the hash pre-image**, so neither one
triggers the fleet-wide regeneration of trap 3.

Propensity is also a **later phase than the trigger** — it needs closed spans, and on day one
every span is open. Ship the record and the current-availability read first; the propensity line
has nothing to say until the table has history.

### Current availability, in the personnel block

- **`load_personnel_changes` (`:887`)** gains two arms in the `changes` CTE — an `applied` arm
  and a `reverted` arm over `player_availability` — `UNION ALL`'d alongside the transfer arms,
  under the same `since` window. The `reverted` arm is dated by when it was undone, exactly as
  the transfer one is.
- **`PersonnelChange` (`:836`)** gains a discriminant so `render_personnel_block` can render "out
  injured since Aug 21" / "suspended, available Sep 02" rather than the "signed X from Y" shape.
- **`PERSONNEL_LINE_CAP` = 6 (`:856`) now has two claimants.** Transfers and availability share
  one budget. Recommend availability sorts first at equal recency — an unavailable player changes
  what every stat on the card means — and the **A5 rule still applies**: name what the cap
  dropped, never truncate silently.

**The invariant that must not break, and it is the whole reason this phase is cheap:** none of
this touches `input_components` or `input_hash`. The block travels on `with_enrichment`, which
parity/eval/input-version callers pass `false` (`:1478`). The hash pre-image stays byte-identical,
so **nobody who has no injuries regenerates**. This is trap 3 from the handoff and it is the
single most important line in this plan.

## Phase 3 — the trigger (`scout/mod.rs`, alongside the transfer helpers)

```rust
const RATING_WORK_AVAIL_MARK: &str = "avail";

/// Keyed on the DAY, not the event: every injury for this entity on this date produces the
/// same input_version, so work::enqueue's `WHERE input_version IS DISTINCT FROM EXCLUDED`
/// collapses them into one row and the Scout runs once. Scott's constraint, no debounce table.
pub fn rating_work_input_version_for_availability(season: i32, day: &str) -> String {
    format!("{RATING_WORK_PREFIX}{season}:{RATING_PROMPT_VERSION}:{RATING_WORK_AVAIL_MARK}{day}")
}
```

- **`rating_work_is_transfer_triggered` (`:1892`) gets a sibling, not a widening.** A combined
  `rating_work_bypasses_debounce` drives `skip_unchanged`, but the two marks stay separately
  detectable so `persist_stat_summary`'s `trigger_type` can be a three-way
  (`periodic` / `transfer` / `availability`). That provenance is what makes the eval splittable
  later — the same reason the conformance script splits by contract version.
- **`RatingHandler::handle` (`:2135`)**: `by_transfer` becomes the combined predicate;
  `trigger_type` becomes the three-way.
- **`enqueue_rating_for_applied_availability`**, mirroring `enqueue_rating_for_applied_transfer`
  (`:2052`). Two targets, not three — the player and their current club. An injury has no
  old/new club. Best-effort, and a failure must never fail the adjudication that earned it; the
  nightly batch stays the backstop.
- Called from wherever the Investigator flips an availability row to `applied`.

## Phase 4 — what deliberately does NOT get built

- **No `stage_routing_subscriptions` row for the Scout.** See Finding 1. The packet trigger
  cannot carry the marker, cannot collapse per day, and would no-op behind `skip_unchanged`. The
  handoff recommends this INSERT; it is wrong, and this line is here so it does not get
  re-recommended.
- **No `Voice::Scout`.** T4 stands.

## Phase 5 — the gate

- `eval --task rating --fixtures --live-system` before deploy. It replays frozen inputs against
  the current source constant, which is the right gate for a prompt-adjacent change.
- New fixtures: a player with an applied injury, a team with one, a **reverted** suspension.
- Assert with `prose_includes_any` synonym sets, never single keywords. Measured lesson:
  `prose_includes:falling` failed on "a steady slide" and "in decline", both of which read fine.
  Lean prompts and keyword gates are incompatible — the gate forces vocabulary stuffing.
- **Do not bump `RATING_PROMPT_VERSION` expecting cards to move.** A bump regenerates nothing on
  its own; this burned the last session three times. The s21 availability rule is already in the
  prompt, so if the prompt text does not change there is nothing to bump, and the new enqueue is
  the only thing that should move cards.
- Verify by watching guard rejections in the journal, not by waiting out the drain — the Scout's
  normal cadence is 400 nightly targets against 2,257, about six days per team.

## The latent defect this task found on the way in (mig 228)

`RatingHandler` has written `trigger_type = "transfer"` since `615bdcb` (Scott's 2026-08-15
brief), and `persist_stat_summary` binds it straight through — but
`stat_summaries.trigger_type` has carried `CHECK (trigger_type IN
('stat_change','periodic','manual'))` since mig 086 and **no migration ever widened it**. So a
transfer-triggered Scout run claims the item, runs the model call (the debounce is deliberately
disabled for these), generates the card, and then throws at the INSERT and loses it.

**Verified against prod 2026-08-23, and it has cost nothing yet.** The constraint is confirmed
narrow; `stat_summaries` holds 20,322 `periodic` + 163 `manual` and **zero** `transfer`;
`pipeline_work` has never held a rating row with the `xfer` marker; no `last_error` anywhere
mentions `trigger_type`. The reason is not that the code works — it is that **the trigger has
never fired**: all five all-time applied transfers landed in July (last: 2026-07-29), before it
shipped, and nothing has reached `applied` since.

So the defect is real, unexercised, and armed. Mig 228 widens the constraint to admit
`'transfer'` and `'availability'` before either can fire it.

> **Worth a separate look:** no transfer has reached `applied` in ~4 weeks. The trigger Scott
> commissioned on 08-15 has had zero opportunities in its entire life. That may be correct
> (a quiet window) or may be an adjudication stall — it is not this task's scope, but it is the
> reason this task's gate cannot lean on the transfer path for evidence.

## Sequencing against the fleet

The 08-23 two-host split already fixed the starvation that would have masked this work (see the
catch-up section). The rating stage is `max_in_flight = 2` inside `stage::ARCHBOX_SLOTS`, whose
membership is now correct — the Scout sits in the group of the card that actually runs his model.

Apply **228 before 229**: the constraint fix stands alone, closes a live defect, and shipping the
availability trigger onto the un-widened constraint would reproduce that defect immediately on a
second path.

## Known behaviour change, accepted

Coupling a statistical card to the news cadence. The rating stage is triggered by stat
recomputation today; a busy injury week will regenerate rating cards more often. That is probably
desirable — availability changes what the profile means — but it is a real change on the seat
with the slowest drain, and it is the thing to watch first if throughput falls.
