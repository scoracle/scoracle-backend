# 2026-08-25 — Granite becomes the resident, the rail learns to think, and the form arrives

**The day in one line: the fleet's engine was swapped on a measured A/B
(ministral-3:3b → granite4.2:3b, both workers), the Ollama rail moved to
`/api/chat`, thinking was measured and disqualified fleet-wide, and the seats
got THE STORY FORM — a shared structure file that turns character prompts into
voice descriptions.**

Scott, at close: *"This prose reads very close to what I envision. And it's a
3B model. That's no accident, that's a good product. And it's building a moat
that will pay dividends down the road as the corpus of stories grows."*

## The resident switch (morning)

granite4.2:3b (released this day) vs the incumbent ministral-3:3b, on the
fixture gate — all 9 tasks, 66 fixtures, both Q4_K_M, temp 0. Granite 93.8%
vs Ministral 91.8%, and the SHAPE of the errors decided it: Ministral's
failures were the expensive kind (oracle UNPARSEABLE on 4 of 8 fixtures —
fail-closed retry burns; role bleed; the ~2% foreign-script leak), Granite's
the cheap kind (a ` · ` echo tic, sentence overruns). Granite's tokenizer is
~42% denser — the 4096 window now holds ~1.7× the material, prefill wall-time
tied, generation 44 vs 38 tok/s. Both workers' env switched; rollback is the
same edit reversed, ministral stays pulled.

Full record: `run_docs/2026-08-25_resident-model-switch-granite.md` — the A/B
tables, the think measurements, the window experiment (8192 measured and
declined), and the landmines below.

## Landmines found and defused

- **The Modelfile window.** A model's own `num_ctx` parameter outranks the
  server default — granite ships 131072, and one omitting call loaded a 25GB
  KV monster onto the 16GB Mac. `ollama.rs::build_request` now ALWAYS sends
  the window. Never inherit a model's own default.
- **The DHCP outage that wasn't.** archbox's lease moved .92 → .94; everything
  pinned to the IP read as a dead host for hours. References are now
  `archbox.attlocal.net` (router DNS tracks the lease). Also: archbox's stack
  runs as USER-level systemd units — `systemctl --user`, or you will diagnose
  a running worker as missing.
- **Thinking.** `/api/generate` never implemented thinking separation for
  granite (the flag was inert — which had made transfer's 75/75 "think win"
  an artifact). The rail moved to `/api/chat` (equivalence verified on all 9
  tasks), where thinking actually runs — and granite deliberates in proportion
  to CONTRACT mass, not material (~2,500 tokens of rule rehearsal for a
  331-char card), starving every budget even with +600 headroom (transfer:
  10/75). **`_THINK=false` on all 11 roles, both hosts.** The chat rail, the
  `GenerateResult.thinking` field, and `THINK_NUM_PREDICT_HEADROOM` stay — the
  plumbing is correct for a future model whose deliberation scales with
  material. Scott's ruling, now doctrine: a budget is a ceiling, not a target;
  report what's there, compress abundance, never expand into space.
- **Self-review in the card.** On busy live entities granite grades its own
  answer inside the answer ("But wait—this doesn't quite fit the required
  format… Revised VIBE:"). Fixtures never provoked it. Fix at the output:
  `guards::truncate_self_review` inside `clean_served_prose` (measured
  markers, strip-not-reject), now covering every served body including the
  Analyst's READ, which had never been routed through the shared scrub.

## THE STORY FORM (evening)

Scott's structure, from teaching English: a lead (the tweet hook), then one
paragraph per claim — claim sentence, one-to-three evidence sentences, closing
sentence. `src/junctions/form.rs` is the dedicated format/structure file:

- `STORY_FORM` — the paragraph anatomy + the invisible-frame rule
- `CLAIM_SELECTION` — what deserves a statement: elite, abysmal, honest
  ordinariness, real ambiguity; never filler
- `WIRE_COPY` — the AP register (subject, verb, fact; no while/where/as
  chains; the read-it-aloud test)
- `card_face(front, back)` — the tarot-fit block, parameterized

Character `prompt.rs` files compose these and describe VOICE. Pilots: the
Influencer (full form; her parser now preserves paragraph breaks) and the
Scout (selection + wire copy; his labelled sections await the call on full
paragraphs). Remaining seats adopt on their next contract pass. All prompt
changes shipped version-unbumped per the Twitter-rule precedent; cut at
2026-08-25.

**The worked-example law, measured:** an example is safe only where it cannot
be mistaken for input. The Scout's numberless invented-club report holds
(numeric input, prose example). The Influencer's prose example was copied
VERBATIM onto a real card — fabricated events included — and was removed the
same evening. Do not retry a prose example on a prose-input seat.

**The echo ladder** (each fix at the layer the law prescribes): example
numbers → out of the material; form vocabulary → invisible-frame rule +
label-strip/marker-truncation in the scrub; the ` · ` datapoint notation →
comma render (eighth application of the input-shouting law;
`RATING_BODY_BANS`'s " · " is now a should-never-fire tripwire).

## Known residuals, deliberately accepted

Occasional inline "Evidence:" mid-sentence (line-prefix strip misses it); a
rare duplicated paragraph (no dedup in the scrub yet); one Scout tier wobble
(an above-average mark listed under Limitations — the L8 inversion class).
All gate-visible; the deck judges.

## For the next session

- Roll the form to Journalist (nearly native), Analyst, Insider, Oracle.
- The Scout label question: keep Strengths/Limitations/Summary as claim
  groups, or go full paragraphs (the articulator's conformance checks key on
  the labels — coordinate).
- Fixtures are now teaching-drifted from the live prompts (frozen systems
  predate the form); re-freeze when a seat's pass settles.
- The articulator corpus should extract AFTER the form settles fleet-wide, so
  the five voices train in this register.
