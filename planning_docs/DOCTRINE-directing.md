# DOCTRINE — Direct the model to empower it

*(Scott, 2026-08-19. Distilled from the seat-gate week: `progress_docs/2026-08-18_seat-and-reader-gates.md`.)*

**The goal is to direct the model as a way to empower it. Show the model the path so it
can express itself within the guide rails, instead of having to find its own way.**

A junction that relies on a model's intuition is tailored to that model, and every model
swap re-litigates everything the intuition covered. A junction that states its terms —
in schema, in code, in guards, in explicit scales — is model-blind: any capable model
plugged into it inherits the path, and its capability goes into the *expression*, not
into rediscovering the rules. The FEEL of a voice survives model upgrades exactly to the
extent that the direction is strong.

## The directing stack, strongest first

1. **Derive in code** — the model never owns a judgment. Relevance, momentum sign,
   tiers (T2). The error class ceases to exist.
2. **Constrain at decode** — grammar/schema (byte-ordered, raw). Invalid shape cannot
   be emitted even once. Governs shape only, never meaning.
3. **Guard with retry** — post-decode content scans for what grammar cannot express:
   banned terms, peer-seat names, digits in no-digit prose, card-fact mismatches.
   Violation → one retry → fail closed. Precedent: the analyst parser's
   `has_foreign_script` guard.
4. **Eval checks** — everything only judgment can see: register, calibration-as-feel,
   whether prose inhabits the room or summarizes it. The gate reads the PROSE.

Rule of thumb: **anything checkable by string-matching belongs at tier 3, not tier 4.**
A rule stranded in the eval harness is knowledge without enforcement — it protects
decisions, not products.

## Standing migration: eval checks → production guards

Promote the mechanical discretion rules into the junctions' output paths. First set:

- banned-vocabulary scans (per-voice lists already in the fixtures' expects)
- peer-seat / internal-field naming (the `reading_max_peers` class)
- digits in digit-free prose fields (momentum READ, oracle reading)
- voiced-fact consistency where the fact is on the card (e.g. a voiced heat number
  must match the wire's heat)

Guards cost only on violation, so they tax bad models, not throughput. Their second
product is telemetry: a per-model, per-junction violation *rate*, which turns every
future contender evaluation into a dashboard number (granite's momentum rate was 8/10
vs ministral's ~1/10 — two days of gates, compressed into one metric).

## Implicit scales are undirected judgment — make them explicit

Canonical instance (08-19): the vibe score. The scale was implicit; the 9B had
internalized it, and a challenger scored 18 where the honest read was ~40 — numeric
corruption flowing into momentum. The fix is direction, not model choice: **anchor the
scale in the contract** (what a quiet slump scores, what collapse-with-protests scores,
what euphoria requires). Any model that reads the anchors can hold the line; no model
should have to guess it. Audit other junction-owned numbers for the same implicitness
(card_score, conviction bands).

## What direction cannot do

Tier 4 never empties. No guard hears the difference between a felt read and a news
summary; no schema encodes restraint. The prose reading at the gate is the permanent
last mile — direction protects the floor; the gate protects the ceiling.
