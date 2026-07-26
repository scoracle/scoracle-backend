# The Mac character rail — two open regressions, and what the workload actually costs

Companion to the `characters/peer-length-allowance` branch. Written 2026-07-26 while Archbox was
serving production through the Mac, so **every GPU-bound measurement here predates that traffic**;
the two estimated figures are flagged as such and want re-measuring on an idle box.

Hardware: M4 Mac mini, 16 GB. `ministral-3:14b` (13.9B Q4_K_M), 9.5 GB resident, 100% GPU,
16384 ctx, `NUM_PARALLEL=1`. Measured generation rate **11.99 tok/s**.

---

## Part 1 — Two Oracle regressions the allowance pass introduced

Both surfaced on the `or7` gate run and both are **caused by the longer allowance**, not by the
model choice. They share one mechanism: given more room, the Oracle fills it with atmosphere
rather than with facts from the cards.

### R1 — internal field words leak into the reading

`ascendant-aligned`, check `reading_excludes:z-score`. The reading ran:

> "…the only tension lies between the Scout's climbing **z-scores** and the Analyst's momentum…"

`oracle/prompt.rs` bans this explicitly: *"never use the internal field words (notability,
convergence, sentiment, impact, heat, slope, z-score)"*. At 2-4 sentences the Oracle had only room
for the conclusion; at eight it starts narrating *how the peers reached* their conclusions, and the
peers' cards are where the bookkeeping vocabulary lives. The extra sentences pull it toward the
machinery.

**Fix direction.** The ban is a flat list buried among ten other bullets. Give it the same
treatment that worked on the Analyst's Markdown problem — a short, prominent, quoted-phrase rule
near the output contract rather than a mid-list clause. Worth also naming the *source* of the
temptation: the peer cards themselves carry these words, and quoting a peer's card is already
banned separately.

### R2 — the reading stops naming the entity

`waning-freefall`, check `reading_includes:Coastal`. Five sentences, entity never named:

> "**The team** stands in a shadow of its own making—no light from the bench, no spark from the
> front line. The manager's authority frays at the edges while the attack chokes on its own
> momentum…"

This is the more serious of the two, because the Oracle's own rule calls it out: *"A reading that
could belong to another entity is no reading."* That is exactly what this is. Coastal City FC never
appears; swap in any struggling side and the text is unchanged.

The mechanism is the same as R1 inverted. Concrete nouns are finite — one entity, maybe one
counterparty — so a reading that doubles in length cannot double its supply of proper names. The
surplus goes to imagery, and imagery is entity-agnostic. **The longer format made the Oracle more
generic, not less**, which is the opposite of the pass's intent.

**Fix direction.** Make naming scale with length rather than sit as a one-off instruction: require
the entity by name in the opening sentence AND at least once more, and state plainly that added
sentences must add *facts from the cards*, never further imagery on facts already stated. The
existing "let ONE figurative image color the reading" rule was written for a 2-4 sentence reading
and now reads as permission for one image per sentence — it should be re-scoped to one image for
the whole reading.

### Not regressions, for the record

- The Influencer landing at **7.4 sentences** is the design working, not runaway length.
- The Analyst dropping to **4.4 sentences** under the allowance framing is also intended.
- `momentum` 39/42 → 37/42 was NOT caused by the allowance framing; see the s10 note below.

### Also open: the Analyst's magnitude compression (s10, ungated)

Separate from the above and already committed. Across 8 fixtures and two prompt revisions,
ministral never left `{-1, 0, 1}` on the `-5..5` scale; nemo reached `-2` and `3` on identical
inputs. On a **rising** entity ministral returned `-1`, violating the sign contract outright. `s9`
attributed this to padding and was **wrong**. `s10` sets the sign from the decided direction as
arithmetic before magnitude is considered, forces the number to agree with the READ's own
adjectives, and hard-bans the leaked `"the engine sees this as"` closers.

**`s10` has never been gated** — the GPU was yielded to Archbox before it ran. It is the first
thing to test when the box is free. If it does not take, the fallback is routing `MOMENTUM_LOGIC`
to nemo, which the topology already supports per-role — but note that means two resident models,
and 9.1 + 7.1 GB does not fit in 16 GB. That would push the Analyst back to Archbox, not just to a
different tag.

---

## Part 2 — What the workload actually costs

### The pipeline does not pace itself

Worth stating first because it shapes every answer below. `STAGE_ROTATION_BATCH = 1` in
`worker.rs` with **no interval timer** — the worker drains the queue as fast as generation
completes. So Mac GPU time per day is exactly `calls/day × seconds/call`, and there is **no
idle-by-design**. The machine rests only when the queue empties.

If quiet hours are wanted, that is new mechanism, not a tuning knob: a `systemctl --user` timer
around the unit, or an env-gated quiet window in the worker loop.

### Per-call cost

`cost = prompt_tokens / prompt_rate + output_tokens / 11.99`

Generation rate is **measured**. Prompt-eval rate is **estimated at ~200 tok/s** (compute-bound
prefill for a 14B Q4 on M4) and is the one soft number here — it is the smaller term, but it wants
a real measurement on an idle box.

| character | junction | prompt tok | output tok | prefill | generate | **per call** |
|---|---|---|---|---|---|---|
| The Influencer | vibe | 1,265 | 168 ᴹ | 6.3 s | 14.0 s | **~20 s** |
| The Analyst | momentum | 1,594 | 121 ᴹ | 8.0 s | 10.1 s | **~18 s** |
| The Oracle | oracle | 1,678 | 141 ᴹ | 8.4 s | 11.8 s | **~20 s** |
| The Insider | transfer | 1,101 | ~175 ᴱ | 5.5 s | 14.6 s | **~20 s** |
| The Scout | rating | 1,851 | ~340 ᴱ | 9.3 s | 28.4 s | **~38 s** |
| The Journalist | narratives | 1,564 | ~400 ᴱ | 7.8 s | 33.4 s | **~41 s** |

ᴹ measured on post-pass prompts · ᴱ estimated from the contract; the gate died before these three
ran at `or7`/`s16`/`n15`/`is3`. Prompt sizes are fixture averages and run smaller than production.

**A full six-character pass on one entity: ~157 s, call it 2.6 minutes.**

### Capacity

| entity passes / day | Mac GPU hours / day | duty cycle |
|---|---|---|
| 100 | 4.4 | 18% |
| 200 | 8.7 | 36% |
| 350 | 15.3 | 64% |
| **550** | **24.0** | **100% — saturated** |

**The ceiling is roughly 550 full character passes per day.** For a 50% duty cycle — genuinely
ample rest — budget **≤275**. Real load sits below these lines because stages debounce on
`input_hash`, so an entity whose inputs did not move is skipped entirely; this is the ceiling, not
the forecast. The number that turns it into a forecast is dirty-entities-per-day from
`pipeline_work` on Archbox.

### What the allowance pass cost

Comparing measured output length before and after, on identical fixtures:

- The Influencer roughly doubled (≈80 → 168 output tokens): **~13 s → ~20 s**, **+56%** per call.
- The Analyst is close to flat — the allowance framing made it *shorter*, offsetting the raised
  ceiling.

Prefill is fixed regardless of output length, which damps the effect: doubling the prose does not
double the call. Across the six seats the pass costs on the order of **+30-50% GPU time**, not
+100%.

### If trimming becomes necessary, in order of value returned

1. **The Scout and The Journalist are 50% of the bill** (~79 s of the ~157 s). Trim there first;
   the other four are ~20 s each and trimming them buys little.
2. **`num_predict` is a ceiling, not a target** — lowering it does not speed up a call that was
   already shorter. Trimming means shortening the *contract* in the prompt, not the budget.
3. **Prefill is ~40% of the bill** (45 s of 157 s) and is pure prompt size. Shrinking what the
   builders pack in is as valuable as shortening the output, and costs no voice.
4. **Only then** reduce the sentence allowance — that undoes the pass, so it should be last.

### Two operational hazards this exposes

- **`OLLAMA_TIMEOUT_SECONDS` defaults to 60 s** (`config.rs:72`), sized for `mistral:7b` at
  ~40 tok/s on the 1070 Ti. At 11.99 tok/s that budget covers ~720 output tokens *before* prefill.
  Against production's current `num_predict` values the Journalist (3000), Scout (1200) and Insider
  (900) can all exceed it. Symptom: intermittent timeouts concentrated on the two most expensive
  seats. **Raise to 300 on Archbox.** Two Oracle fixtures — the *cheapest* seat — already failed
  this way under contention.
- **`NUM_PARALLEL=1` means strict serialization.** Deliberate (16 GB will not hold two KV
  allocations), but concurrent stage calls queue with their clocks running. Keep the Mac's
  `COGNITION_BACKEND_CONCURRENCY` entry at `=1`.
