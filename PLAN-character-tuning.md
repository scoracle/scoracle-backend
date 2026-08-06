# PLAN — Character Tuning (session notes)

**Founded 2026-08-05 by Scott's ruling: "this is a tuning issue… this session's goal is to
get the new rail built. Then we focus on tuning the LLM junctions to really ratchet up the
speed of the flow."**

**What this file is.** The working notes for post-rail **Character tuning sessions** — the
diagnosis detail behind Appendix D of `PLAN-one-rail.md`. The convention (written into the
Appendix D preamble):

- **Appendix D stays the ledger** — every junction quality/efficiency finding gets a D-T
  number there with its one-line measured baseline. A rail phase may cite the ledger; it may
  never halt on it.
- **This file carries the diagnosis** — numbers, code pointers, candidate knobs, and the
  measurement that would settle each knob. Add to it whenever rail work surfaces a finding;
  never fix mid-rail (the §4 law: plumbing gates phases, tuning is follow-up).
- **Tuning sessions run AFTER the rail stands** (post Phase 8 cutover, or in idle capacity on
  Scott's word), one knob at a time, one measurement per change (the ar4/ar5 lesson).

The laws still bind during tuning: describe-then-derive (T2); exact+discriminator or refuse;
a contract_version is a cache key — bumping one reopens ALL its work; stage wire names never
rename.

---

## 1 · The Editor — input hygiene (D-T11; measured 2026-08-04/05, 4,774 ledgered calls)

**Finding:** `fetch.rs::clean_html` (fetch.rs:261) is a naive strip-all-tags — it keeps every
visible string on the page: nav menus, footers, related-link rails. **34.3% of all editor
prompts hit the 9,000-char truncation cap** (`EDITOR_MAX_MODEL_CHARS`, editor/prompt.rs:96).
`sports.yahoo.com` — the #1 domain, 584 calls in the sample window — runs **95% at cap**, and
its "Article text:" begins with the site's entire chrome ("News Today's news US Politics …
Horoscopes Shopping Food Travel Autos …") before any article prose. On capped pages the chrome
eats the front of the window and **real article text is truncated off the tail** — feeding
D-T1's under-fill miss class (late-article names the model never sees). Capped domains beyond
Yahoo: si.com 77%, nytimes.com 65%, nbcsports.com 67%, cbssports.com 46%.

Bonus defect: `decode_entities` (fetch.rs:328) handles `&#39;` but not hex `&#x27;` (or numeric
entities generally) — quote-heavy articles carry six wasted chars per apostrophe inside the
budget, and the model reads `&#x27;` as literal text.

**Candidate knobs** (quality-first; modest wall-time win):
- (a) Main-content extraction before truncation: prefer the `<article>` element when present;
  else strip `<nav>/<header>/<footer>/<aside>` blocks — the existing `strip_element_blocks`
  machinery extends to this in-idiom. No contract change.
- (b) Decode numeric/hex entities in `decode_entities`.

**Measurement:** re-run the D-T1 per-name 2×2 on a capped-domain sample (Yahoo/SI) before/after;
watch pct-at-cap and extracted_words shift; the 5.7-style fixture set should include a captured
Yahoo page (real prompts are in `cognition_ledger.built_prompt`).

## 2 · The Editor — output dominance and capacity (D-T12; same sample)

**Finding: output generation, not prompt eval, is the wall.** Wall by prompt-size bucket:
~1.4k chars → 16.9s avg / 195 out-tok; ~5k → 30.6s / 387; capped ~8.9k → 38.8s / 476. Of the
~22s small→capped delta, ~19s is the extra 281 output tokens (~14 tok/s/slot at 4-parallel on
the 1070 Ti) and only ~3s is prompt eval. Longer input → more names/facts/evidence emitted →
generation time.

**Capacity is fully subscribed:** ~490 reads/hr active, rest windows pause 8h/day (every 3rd
hour +1h) → ~7,800 reads/day capacity vs arrivals grown to ~8,000–8,400/day (Aug-4: 7,985;
Aug-5: 8,358; ingest is one daily 02:00 EDT batch). Slot utilization measured 77% model-call
wall; concurrency verified real at both layers (worker `ARCHBOX_GEMMA_SLOTS`=4,
`OLLAMA_NUM_PARALLEL=4`, 100% GPU). Within-24h coverage still 100.0% post-deploy — but there is
no headroom left for arrival growth.

**Candidate knobs, by leverage:**
- Output-side (the real seconds): D-T4's `num_predict` 900→750 clips only the p95 tail (avg
  output 420). A real cut means a tighter ep1 envelope (bounded `key_facts[]`, shorter
  `evidence_blurb`) — **an ep2 contract bump that reopens all editor work; never casual.**
- Input-side: D-T11 above (secondary for time, primary for quality).
- Rest windows: 8h/day of wall (33%) — hardware-stress policy, **Scott's call only**.
- Model/quant swap on the same card: a Character decision, needs the D-T1 yardstick replayed.

## 3 · The Investigator — starvation and volume (D-T10; day-2 verdict 2026-08-05)

**The design works but the arithmetic doesn't:** the investigator caught exactly the idle the
design predicted — 70 runs in the 01:52–02:00 EDT window before the daily batch re-buried the
card (Aug-5). Decisions honest: 8 accepted / 23 ambiguous / 20 not_sport / 19 insufficient
(11.4% acceptance). But steady-state nominations are ~3k persons/day (day-2 pace matched
day-1 — NOT a corpus flush), so the queue grows ~2.7k/day against a ~70/day drain (6,670
pending at day 2). Even fully unblocked, the 4.2 budget (2s Wikimedia spacing) caps drain at
~900/day.

**Candidate knobs, in leverage order** (from the D-T10 ledger entry): (a) the v1 investigator
makes ZERO model calls — holding an `ARCHBOX_GEMMA_SLOTS` card slot for pure HTTP work is the
structural mismatch; a separate slot group frees it entirely; (b) tighten the 5.2 enqueue rule
(descriptor-on-first-sight admits ~100% of person names; the 2-mention floor is near-dead
letter); (c) run the investigator through GPU rest windows (the card rests; HTTP doesn't need
it) — interacts with the pause-timer design; (d) raise `max_in_flight` only after (a).

**The compounding upside already measured:** the 8 overnight accepts collected 102 resolver
links onto `persons` rows within the same day (Xabi Alonso 59, Andoni Iraola 23) — every
accepted person immediately stops being an unresolved name. Drain rate is the direct multiplier
on this loop.

## 4 · The Investigator — evidence-class gaps (D-T6/7/8) and parked ops (D-T9)

- **D-T6** enrichment refusals leave no durable trace (log-only) — review surface can't count
  them. Candidate: census row or `players.meta` note on refusal.
- **D-T7** initials in `nrm()` ("A.J. Green" → `a j green` vs Wikidata `aj green`) — honest
  refusal, missed enrichment. Measure the class size across rosters before touching the one
  normalizer (mig-198 caution doubly applies).
- **D-T8** legal-name vs known-name ("Airious" vs "Ace" Bailey) — the designed answer is the
  deferred 5.4 prose arm: Wikipedia REST search + gemma **describes** the page, code decides.
  Build when the class proves big enough; this is also the first real model-call load for
  `Role::Investigator` (interacts with D-T10's slot question).
- **D-T9** the meta-gathering RUN (FULL NBA seed ~603 players → 20-row hand-check → widen to
  FOOTBALL rosters at season start) — **ops on Scott's go**, machinery ready. Box-score target
  URLs themselves stay parked with Phase 4 until a season provides them (pulselive_pl seed
  one-liner still awaits Scott).

## 5 · Older ledger items carried (see Appendix D for baselines)

- **D-T1** names[] under-fill — the miss class (16.7% of successful name-reads missed the
  player in the replay). Knobs: quoted-people re-scan for title principals; the Investigator
  nomination backstop now live structurally catches what the prompt drops. D-T11's truncation
  fix plausibly shrinks this class — measure them together.
- **D-T2** register `outrage` reads neutral under phrase-first order (declined reorder; needs a
  fixture set before revisiting).
- **D-T3** parse_failed 2.6% vs legacy 0.1% — diagnose format_schema violations vs truncation
  (interplay with D-T4/num_predict).
- **D-T4** editor call cost / num_predict 900→750 — superseded in part by §2's decomposition:
  the knob only clips the tail.
- **D-T5** descriptor leakage ("team 277" — an internal id in a text-copy field). Count
  instances before caring.
