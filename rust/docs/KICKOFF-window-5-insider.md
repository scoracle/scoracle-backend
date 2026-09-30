# Window 5 — Insider (alignment session kickoff)

**Paste this into a fresh session.** The plan file is `docs/PLAN-plugin-alignment-2026-09-27.md` — it is the durable index, and this prompt is only a pointer into it.

---

## The directive

> **THIS IS AN ALIGNMENT SESSION. WE ARE MOVING EVERY PLUGIN ONTO ONE SHAPE:**
>
> **THE PLUGIN NAMES THE DB TABLES IT READS → SELECTS AND COMPUTES → ASSEMBLES ONE WORLD FROM NAMED PARTS → THE LLM ARTICULATES THAT WORLD → THE PLUGIN ENFORCES ITS OWN DECLARED CONTRACT.**
>
> **THE PRODUCT IS A LIVING DATABASE TOLD THROUGH MULTIPLE LENSES. THE HARNESS DISPATCHES WORK; PLUGINS ARE DYNAMIC IN HOW THEY ASSEMBLE. WE ARE NOT BUILDING A PIPELINE.**
>
> **PRUNE HARD. OLD SCAR TISSUE, REDUNDANT PROMPTS AND UNNECESSARY LEGACY CODE ARE SLOP. THERE IS NO SECOND MEMORY IMPLEMENTATION, NO SECOND PARSER, NO SECOND PROMPT COMPOSER.**

---

## Read, in this order, and nothing else

1. **`The parts recipe, as Window 4 established it`** — in the plan, before anything else. It is the transferable lesson and it came out of Scout. Skipping it is how you repeat a session's worth of mistakes.
2. The ownership contract and the parts contract table.
3. The "Sharing is an optimization" rules.
4. Window 0's F5 and F8 sections.
5. **Window 5 in full.**
6. The decision register.

Do not reopen Window 0, Window 2's prose behavior, Window 3's architecture, or Window 4's parts. If something there is wrong, **say so rather than quietly reworking it.**

---

## Where the work stands

- **Window 0 is closed** — F1, F1b, F2, F3, F4, F4c, F6, F7. Nothing outstanding.
- **Window 4 (Scout) is code-complete and locally verified, but UNRELEASED and UNREPLAYED** — `73374a72`. Do not treat it as released or as evidence-backed. Its published output changed shape and no model has seen it.
- **Three plugins are on the parts contract** (`narratives`, `vibe`, `rating`); seven are not.
- **574 tests pass** at `cc6ef953`. Clippy clean, all targets compile.
- `support::prompt::compose` has **three callers left**: Insider, Analyst, Oracle. This window retires one.
- `RATING_BODY_BANS` has moved home. `MOMENTUM_BANNED_PHRASES` is the last misplaced guard and it is **Analyst's**, in Window 6.

---

## What this window owns

**F5 — give Insider its own manual and its own parts.** It is the second of three `compose` callers.

- `src/plugins/insider/adapter/{mod,harvester,identity}.rs`; `adapter/harvester/{identity,wrap}.rs`; `cognition/{mod,inputs,brief,verification}.rs`.
- Replace `cognition/brief.rs` with a plugin-owned `cognition/prompt.rs`, exactly as Scout did.
- Build `insider/cognition/parts.rs` and an `insider/memories.rs` if its memory is not reporting history.
- Convert the `transfer` fixtures to parts and set `stores_parts()` → true.
- **A deterministic non-prose field (Insider's score) is owned by the plugin outside the prose map.** The Journalist precedent is `NarrativesProduct::card_score`; do not put a score in a prose slot.

**Two things only Insider can decide:**

1. **Identity obligations.** Insider holds a Graph inference route whose contract is only a parser. Under F1b, *a plugin that reaches a model with no declared, enforced response contract has an undefined model role.* That is the finding to name — not "the LLM does too much." Canonical promotion goes to Investigator's evidence gate. A co-mention or a verbatim quote is not a move.
2. **Pair memory.** F1 removed `pair` from `reporting_scope`; a pair-needing caller now passes a pre-resolved article list of article ids. **Insider is the caller that motivated that change and has never exercised the replacement.** F1 also moved pair-name containment out of the shared study and made it opt-in — every caller that resolves a pair must now apply it. That is Insider's job.

---

## Copy these — the Scout precedent

| Question | Scout's answer | Where |
| --- | --- | --- |
| What does production *actually* send? | Not what the prompt module builds. Trace the options to their final value before designing anything — this is how the window's premise turned out to be wrong. | `scout/cognition/mod.rs::create` |
| What are the parts? | `identity, fresh, memory, rate_standouts, trend, voice, form` | `scout/cognition/parts.rs` |
| Where does the parts type live? | In the plugin, beside `Prose`. The harness only chooses the JSON. | `journalist/cognition::Parts`, `influencer/cognition::Parts` |
| What if the contract varies with the world? | `assemble` returns a `Prepared { user_prompt, options }`, and `gen_options` refuses. A fixed option set would test a request production never sends. | `evaluation/tasks.rs` |
| What if memory is a different KIND of thing? | Write your own. The shared `HistoryItem` was the wrong type for the Scout, not a lazy one. | `scout/memories.rs` |
| A computed boundary — typed or prose? | Typed. `Limit` with five variants, so the guard does not depend on the model having read a paragraph. | `scout/cognition/parts.rs` |
| Paragraph rule? | Per plugin, and a **declared non-participation** beats a number nobody was measured against. Insider should decide this explicitly, not inherit 140. | `SCOUT_PARAGRAPH_MAX_CHARS = None` |
| Test migration? | This is where the invariants surface. Verify a ported fixture still exercises its branch. | Scout found five defects this way |

---

## The standing rules

- **Assemble ≠ articulate.** The plugin decides which parts exist, what nests in what, and what pairs with what — all before inference. The model decides wording, order, emphasis, compression, voice. **If a reader cannot tell what the product is about with the model deleted, the plugin has not finished assembling.** No manual may ask the model to resolve a pairing.
- **Arithmetic stays in code.** Percentiles, bands, deltas, ranks, trajectories. The model verbalizes; it does not compute, and it may not compute a delta between two figures the plugin supplied.
- **Missing stays unknown.** Absence, an unreadable value, a measured zero, a declined slot, acquisition failure and classifier failure are six different things. A plugin that conflates them has a bug — Scout had exactly one and its own tests caught it.
- **A test that passes because the subject is empty is not a test.** Scout shipped a vacuously-passing assertion for a window; it looked fine.
- **Raise correctable surface violations as `SurfaceError`.** `publishing_correction` recognises that type. A plain error silently disables the bounded retry, and one bad body then costs the whole read.
- **A title and a body are different kinds of text.** The paragraph ceiling is for prose; a title is governed by the hook contract and salvage. A blank *filled slot* is still a violation.
- **The gate is a test, so mutation-test it.** Rename a part key and confirm the fixture gate fails. A guard never seen to fail is not a guard.
- **Record a correction in the decision register when your own sketch is wrong.** Three of this plan's sketches turned out to be wrong against the code (F3's join, F4c's `Value`, F6's `assemble -> String`). All three are in the register rather than quietly applied.

---

## Verify before you claim done

- Pair scope, rumor versus confirmation, negated move, stale report, wrong person, non-transfer co-mention, duplicate coverage, conflicting sources, ineligible identity review, resumed/superseded work.
- A positive source claim **cannot** bypass the canonical write boundary.
- Rumour and personnel memory reach the world attributed and dated, and a retraction reads as a retraction.
- Every score, stage, heat value and date in the world was computed or selected by the plugin before inference.
- Production and the `transfer` eval task use the same active contract.

## Leave a compact handoff

What changed, what was deleted, what was retained and why, verification evidence, **deployment state (code-complete / locally verified / deployed are three different claims)**, and the next plugin's interface. Update this plan; do not create a parallel one.

---

## The one thing I would flag

Window 4 finished a plugin on the parts contract and the result is **unreplayed**. Three of eleven plugins are now on the shape, and one of those is unverified against a model. If you finish Insider and the Scout is still unreplayed, the count of *unverified* parts plugins goes to two. That is worth a sentence in the handoff either way — and it may be worth a decision from the user about running the Scout replay before this window ends.
