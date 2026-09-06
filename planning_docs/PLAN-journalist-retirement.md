# PLAN — The Journalist's Retirement

Scott, 2026-09-06: *"There's a huge overlap between the narratives and vibes output. Maybe
we can retire the Journalist and divvy up that work between the Editor (since that character
compiles the stories with just facts) and the Influencer, since that voice elaborates on the
emotional charge of the stories. In many ways, a good journalist is writing just that: the
emotional charge of the facts. … The two most influential, pillar voices then become Scout
and Influencer — logic vs emotion. The other voices are interpretations of those outputs to
some degree."*

Decisions recorded the same night: news card becomes **the Editor's desk — pure facts,
rendered deterministically from packets, zero model calls**; the busyness score goes **fully
deterministic**; sequencing is **de-dupe now, retire in stages**.

## Why the overlap was structural

Since the packet rail (7.6, E3 "first-voice-capable") the Influencer reads the same packets
the Journalist reads, PLUS his narratives block layered on top — two voices, one input,
convergent output. Stage 0 (SHIPPED, vibe v26) removes the narratives block from her prompt:
packets are her material whole and alone.

## Target architecture

| Surface | Owner | Nature |
|---|---|---|
| News/Narratives card | **The Editor** | deterministic packet render: storyline headline, sourced claim lines, cast, result — the record, no model |
| Vibe card | **The Influencer** | the story voice: score + hook + the emotional charge of the facts (packets only) |
| Scouting/Profile | The Scout | the logic pillar (computed shape, tiers, movement) |
| Momentum | The Analyst | the collision of the two pillars (unchanged — already two-rail) |
| Transfers | The Insider | the wire (unchanged) |
| Sigil | The Oracle | the table read, now over four cards |

## The Journalist's consumer map (what must re-home before the stage dies)

1. **/news card + news board + headlines page** (Go, db.go): re-source from packets.
   New statement(s) rendering storyline headline + claim lines + source names per entity;
   board ranks by the deterministic busyness number (below). `news_summaries` freezes as
   history — never deleted, no longer written.
2. **The busyness card score** (1-99): computed in code from the SIGNALS the prompt already
   tallies (articles after dedup, distinct sources, packet heat). Lands wherever the news
   payload needs it; no model.
3. **The Influencer's narratives block**: GONE (stage 0, shipped).
4. **The Oracle's narrative pillar** (`SynthNarrative`): replace with the Editor's packet
   headlines (a facts line, not a voice) or drop to a four-card spread. Decide at stage 3 —
   leaning four cards, since the Influencer's card already carries the story.
5. **story_parts / relational memory / prior-card reads**: audit which memory writes derive
   from `news_summaries` vs the graph; re-point graph-side, freeze summary-side.
6. **Wakers/subscriptions**: `stage_routing_subscriptions` narratives rows retire; the
   packet trigger already wakes vibe directly ('charged' tag). The one-waker rule for vibe
   (narratives does NOT wake vibe) simplifies away.
7. **Fixtures/eval**: `fixtures/narratives/` + generator + eval task retire with the stage.
8. **Corpus/articulator gates** that reference narratives versions: audit and retire.

## Stages

- **Stage 0 — de-dupe (SHIPPED tonight, v26)**: Influencer packets-only. Measure tomorrow:
  do her cards cover the storyline ground the narratives card carried?
- **Stage 1 — the Editor's desk render**: build the deterministic news payload from packets
  (Go statement + Rust render reuse); frontend NarrativesCard swaps to it behind the same
  route. The Journalist still writes (dark) — one release of side-by-side comparison.
- **Stage 2 — deterministic busyness**: code-compute the score; news board re-ranks.
- **Stage 3 — the Oracle's spread**: four cards (or Editor facts line); or-bump.
- **Stage 4 — the stage retires**: narratives handler unregistered, subscriptions removed,
  `news_summaries` frozen, Influencer's `narratives` param removed end to end (loaders,
  hash, eval, fixtures), junction folder archived. The Editor keeps the assignment desk;
  the Influencer keeps the telling.

## Risks

- The Influencer inherits sole story-voicing: her per-storyline coverage is untested at
  card grain (stage 0's measurement exists for exactly this).
- Attribution prose ("first reported by ESPN, since matched by Marca") was the Journalist's
  craft; the Editor's render carries source NAMES deterministically, the Influencer names
  outlets only when the room's reaction runs through them. Accept the register change.
- The relational memory audit (consumer 5) is the least-mapped surface — do it first in
  stage 1.
