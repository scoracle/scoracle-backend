# PLAN — The Story Cascade

Scott, 2026-09-06, the settled architecture (supersedes the same-day retirement plan this
file used to be — the git history holds it):

> *"Yes, story line first, entity after. Which is a strong case for keeping the Journalist
> as the per-entity story writer, while the Editor is the over arching story writer."*

And the pillar doctrine it sits inside: *"The two most influential, pillar voices then
become Scout and Influencer — logic vs emotion. The other voices are interpretations of
those outputs to some degree."*

## The cascade

```
Article level:   the Editor reads each article, tags EVERY entity in it (+ bucket,
                 evidence card) — the tag work is the keystone of everything downstream
Story level:     the Editor compiles each storyline ONCE, cross-entity — identity, arc,
                 membership, the daily packet. The overarching story writer.
Entity level:    the Journalist writes THIS entity's part in the compiled stories, read
                 through the claim fence — the per-entity story writer, facts register.
Charge level:    the Influencer writes the emotional charge of the same fenced stories
                 (v26, packets only — SHIPPED). Logic pillar: the Scout. Interpreters:
                 Analyst (collision), Insider (wire), Oracle (the table, five cards).
```

Why story-first won (measured, 2026-09-06): entity-first pays N× compilation on shared
sagas (Barcola touched nine clubs), fragments story identity into per-entity tellings that
need re-reconciling, and lets the record contradict itself between entities. Story-first
compiles once and derives fenced per-entity views — the claim fence + mixed-story framing
scrub (`load_packets_for_entity`) are the derivation layer, shared by every seat.

## Workstreams

1. **Tag completeness (prerequisite, highest leverage)**: everything keys off
   `news_article_entities` — the fence, factsweep, every per-entity derivation. Measured
   gap: match reports tagged to persons/players but NOT the clubs in them ("Ipswich 0-2
   Liverpool" carried Gakpo, Isak, Iraola and neither team; we route around it via
   rosters). Strengthen the Editor's team tagging at ingest.
2. **The Journalist's re-scope (n25)**: the contract stops asking him to DECIDE what the
   storylines are — "group the recent vetted news into distinct storylines" is assignment-
   desk work, and the desk already did it (the story-so-far framings in his prompt ARE the
   Editor's record). He files this entity's PART in each compiled story: what happened to
   them, evidence with publications credited in prose, where it stands for them. A story
   where the entity is a bystander gets a line or nothing. Facts register, deliberately
   distinct from the Influencer's charge — the de-dupe holds from both sides.
3. **Deterministic busyness (kept from the old plan)**: his card score is volume
   arithmetic the SIGNALS line already computes — move it to code when convenient; not
   urgent now that he stays.
4. **Cancelled from the retirement plan**: the Editor's-desk replacement render of the
   news card (the Journalist IS the per-entity face), the Oracle's four-card spread
   (five cards stand), stage 4's junction archive.

## Risks

- n25 must not reintroduce the vibe overlap: his lane is what HAPPENED to the entity,
  hers is how it LANDS. The register split (facts vs feelings) is the fence between them.
- Tag-completeness changes touch ingest throughput — measure tag counts before/after.
