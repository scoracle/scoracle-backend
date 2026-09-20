# The threads → storyline-parts collapse — one identity for one story

**Date:** 2026-08-14

## The vision that drove it

Scott's model of the product, restated: the **Editor compiles the stories of the
day**, and the downstream character voices **update those stories as they evolve —
each voice telling each entity's part**. The Oracle is blind (no memories). The
schema audit asked: does the database match this, and where is the unnecessary
complexity?

The audit found the Editor rail clean (editor_reads → storylines → packets) and every
voice's product table clean — **except one overlap**: `narrative_threads`, the
Journalist's entity-keyed story identity, duplicated what the Desk's `storylines`
already are. Two structures tracking "which story is this entity part of," matched
two different ways.

## The core insight

`narrative_threads` existed to solve a problem `storylines` has already solved
better. Threads were built (mig 181) because on the legacy rail a story had no
stable identity — only a title string, and the model re-titles stories (the 07-22
n10 wave reset ALL trajectories). Threads fixed that with embedding centroids:
cosine ≥ 0.80 against an EWMA centroid attaches a telling to its story.

On the packet rail, **a telling's storyline is a FACT, not a match**:

1. every corpus article reaches the Journalist through a packet (`storyline_id` NOT NULL)
2. every article belongs to exactly one storyline (the Editor attaches once)
3. every persisted narrative is grounded on cited article ids

So a chapter's storyline is the **mode of its citations** — deterministic, zero
tokens, zero embeddings, no threshold to tune, no centroid that can drift or be
hijacked. What the thread carried that was worth keeping is the *progression state*
(entry count, impacts, trajectory, sources, authority) — and that belongs on the
entity's **part** in the story. `storyline_entities` already had the D5 comment:
"an entity's part in a storyline has its own lifespan." Now it has progression too.

## The shape

```
storylines              the story (Editor-assembled, code, never model-matched)
  └─ storyline_entities the entity's PART — now also the progression unit
       + entry_count, peak/last_impact, last_trajectory,
         distinct_sources, source_names, authority, last_progressed_at
  └─ storyline_articles membership (one article, one storyline)
  └─ packets            compiled snapshots

news_summaries.thread_id  →  news_summaries.storyline_id (FK storylines.id)
```

The Journalist stops *creating story identity* entirely — creation authority lives
with the Desk. The Journalist only **updates the parts**. That is the newsroom model
the vision describes.

## Two steps, with a safe middle state

**Step A — mig 219, additive** (`76002d7`): the progression columns, `storyline_id`
FK + index, `fill_news_summaries_storylines()` (the citation-mode derivation,
idempotent — backfill AND nightly dual-period fill), the memory card rebuilt on
storylines (three thread CTEs collapse into one `story_parts` scan: established
parts render as one-line background facts, continuity parts render "Our story so
far" with the last 3 chapters, untold parts render the flat membership line), and
the nightly lifecycle rebuilt: `seal_storylines()` (ground-truth resolve + D5's
close-every-other-edge in one stroke; dormancy stays with `mark_dormant`) and
`promote_established_parts()` behind `storyline_part_established_gate()` (same
thresholds: ≥5 sources ∧ ≥3 tellings ∧ ≥14 days, OR resolved). `story_parts.rs`
replaced `threads.rs` with the same deadlock-safe `FOR UPDATE` ordering and
pre-generation anchor discipline; the Journalist's embed call is gone.

Applied numbers: **3,849 chapters filled** (95.8% of post-flip chapters), **2,246
parts rolled up**, **213 established-authority inheritances**. Memory cards
verified rendering the collapsed block.

**Step B — mig 220, demolition** (`499576b` prepared → applied same session): drops
`news_summaries.thread_id`, `narrative_threads`, `v_narrative_threads`, the thread
lifecycle functions, and the fill. Carries its own 045-style **data gate**: refuses
to apply unless ≥25 chapters were written with `storyline_id` by the new persist
path since the cutover AND `narrative_threads` had zero writes since. Lives in
`sql/prepared/` until the gate could pass (the flip-day precedent), then moves to
`sql/migrations/` (`879efed`).

## What the rehearsals taught

1. **Rehearsal 1 (gate must refuse)** — refused correctly on first run (2 chapters,
   needed 25). Later, re-run with 128 chapters accrued, the gate passed and the
   demolition applied — the fence did exactly its job.
2. **Rehearsal 2 caught a real dependency bug**: `v_narrative_threads` joins
   chapters on `thread_id`, so dropping the column before the view fails. Order
   fixed to functions → view → column → table, re-rehearsed green in a rolled-back
   transaction, production verified untouched.
3. The migration carries `SET LOCAL lock_timeout = '30s'`: `news_summaries` is hot
   (the drain writes it continuously), and the drops are metadata-only — fail loud
   and retry later rather than queue every writer behind a long persist.

## Known costs (accepted, bounded)

- **Legacy lineage**: 59,181 pre-Desk chapters had `thread_id` without a storyline
  mapping; they keep their rows but stop rendering in the memory card. ~840 of 1,053
  open established threads lost their background-fact lines (their articles predate
  the Desk). Graph memory (`narrative_episodes`) is untouched; memory accrues fresh
  on storylines from here. The old thread data lives in the pre-drop backups.
- **Rollback bridge burned by design**: after 220 the pre-`76002d7` binary's persist
  references a dropped column — the only way back is DB restore + git revert. That is
  what the data gate was for.

## Post-deploy verification

123+ chapters written on the new path after the drop, zero schema errors, memory
cards rendering, API healthy, both services active, schema snapshot refreshed
(14,123 lines, 222 migrations). The remaining `narrative_threads` mentions in
`schema.sql` are provenance comments only.

## Carry-forward

1. **Eval gates re-run** — narratives (110) and sigil (108/110) in a daemon-stopped
   quiet window; the memory-card prompt shape changed and output gates are the proof
   no voice drifted.
2. **The oMLX concurrency deep-dive** (Scott's parked directive from 08-10) — it
   throttled the drain all day and set the pace of the gate's accrual.
3. **Mirror backup disk** — 95% full; the off-disk mirror has skipped since Aug 10
   (primaries intact, daily at 04:00).
