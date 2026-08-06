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

## 0 · THE REGISTER — every friction point, roadblock and concern going into the session

*Assembled 2026-08-06 ~13:15 EDT, after 8.8, on Scott's instruction: "I want all the friction
points, the roadblocks, the concerns listed." Everything here is MEASURED on live post-flip
production, not inferred. Sections below carry the detail; this is the one place that lists all of
it. **Ordered by what would hurt most if ignored, not by how easy it is to fix.***

### 0a · The one that governs everything else: the model layer is the throughput ceiling

**Measured today.** The Editor sustains **430–490 reads/hour while the daemon is up**, and the
daemon is deliberately down **8 hours a day** (harness rest windows at 00/03/06/09/12/15/18/21:00,
+1h each — §0 rule 6 of `PLAN-one-rail.md`; the 12:00 window is visible in the journal as a clean
stop at 12:02 and restart at 13:00). That works out to:

| | |
|---|---|
| Editor reads/day, actual | Aug 3 **7,041** · Aug 4 **7,560** · Aug 5 **8,063** |
| Articles arriving/day | Aug 3 **6,960** · Aug 4 **8,027** · Aug 5 **8,401** |
| Editor backlog right now | **4,682 pending**, oldest stamped **Aug 5 02:01** (~35h latency) |

**The Editor runs at parity with ingest — about 96% — which means it never catches up.** It is not
falling behind, but it has no headroom to burn down a backlog, absorb a re-read, or take a prompt
that costs 20% more. Every knob in this file that makes a model call longer spends from this
budget. **This is the number to protect.** Coverage reads 97.3% on a complete day precisely
*because* throughput ≈ inflow; those are the same fact seen twice.

**And the Editor is not the deepest queue.** Full pending state at 13:07 EDT:

| stage | pending | oldest pending | note |
|---|---|---|---|
| `investigate_entity` | **8,624** | Aug 4 04:07 (**~57h**) | the deepest starvation on the rail; D-T10 |
| `editor` | 4,682 | Aug 5 02:01 (~35h) | at parity, permanent backlog |
| `narratives` | 2,059 | **Aug 6 10:55:52** | the flip's trigger burst, undrained 2h later |
| `vibe` | 1,912 | **Aug 6 10:55:52** | same burst, same story |
| `momentum` | 1,269 | Aug 2 04:38 | includes the failures in 0c |
| `sigil` | 474 | Aug 3 10:43 | |
| `peak` | 237 | Aug 4 03:00 | |
| `transfers` | 136 | Aug 6 11:23 | the Insider, post-gate-removal |
| `fixture_boxscore` | 72 | Aug 4 04:02 | Phase 4 is parked |
| `article_read` | 30,222 | Aug 3 02:01 | **PARKED BY DESIGN** — rollback surface, 0 compute, dies in Phase 9 |

**The structural tension to name out loud, because it is the thing tuning must actually resolve:**
six AI layers enriching every article is the product, and it is also the cost. The Mac holds
**3 concurrent permits for 6 voices on one GPU**. Adding quality by adding a layer, a re-read, or a
longer prompt is not free here — it comes directly out of coverage. **"Empower the models" and
"read everything" are in tension, and the session should decide which one wins where, per junction,
rather than letting the queue decide by starving whichever stage sorts last.**

### 0b · Defects — fix these, do not tune them

- **`editor::write_links` loses an article's ENTIRE link set on a duplicate resolve.**
  `ON CONFLICT DO UPDATE command cannot affect row a second time` — measured **5 articles in 2
  days out of 6,399 reads (0.08%)**. When one article's `names[]` resolves two different surfaces
  to the SAME entity, the write builds two rows with one conflict key and Postgres rejects the
  whole statement. The read persists, so it looks successful; the links vanish silently and the
  WARN is the only trace. **Fix: dedupe the resolved link set by (entity_type, entity_id) before
  the INSERT.** This is 8.5 code, untouched by 8.8, and it is a bug not a knob.
- **`momentum` answers in markdown instead of its contract.** 11 pending failures + 7
  dead-lettered, `momentum: invalid response (raw="**Momentum Read: …")`, spanning **Aug 2 →
  Aug 6 11:38**. The voice is writing a beautiful essay into a field that wants a structure. First
  candidate: it is the contract prompt, not the model, since the same model answers five other
  voices correctly.

### 0c · Instrument problems — you cannot tune against a gauge that moves

- **D-T19 (§6a): the editor fixture gate is not deterministic** — 47/53 then 43/53 on identical
  runs at temp=0. **Nothing in this file can be scored until this is fixed. Start here.** Scoring a
  knob against a ±4 gauge produces confident nonsense.
- **§2's clause 3 link sample is emitted and UNSCORED.** Precision on the rail's links has never
  actually been measured — only sampled. The 0.90 Editor links are now the majority producer
  (0b, 0e), so this is scoring the new rail, not the old one.
- **Clause 4b is FAIL (43–47/53) and Scott waived it explicitly** for the flip (D-T19). The waiver
  is logged so it is auditable; it has not been retired.

### 0d · Architecture concerns — the hand-rolled complexity that SURVIVED 8.8

**This is the honest answer to "did we just take the fastest path and port a bunch of old Go?"**
The relevance regex is gone (0e). What remains, and it is the largest hand-rolled judgment left
anywhere on the rail:

- ~~**~350 lines in `news.go` decide WHAT WE ASK GOOGLE**~~ — **CLOSED 2026-08-06 by
  PLAN-one-rail step 8.9, on Scott's call, hours after this register was written.** The alias
  scoring, the 18-word risky-club list, the four trusted literals, the short-alias allowlist, the
  per-term suffix branching and the lane cap are all deleted (393 net lines). What runs now: one
  query per name we know the entity by, sport term on every lane, every lane runs, cap on results.
  **Two things survived deletion because measurement said to keep them, and the session should not
  re-open either without new numbers:** the sport suffix (bare "Nice" returns the NHS institute and
  Formula 1; "Nice soccer football" returns the club) and alias lanes (20–30% marginal unique
  recall — Spurs 18/47, Barça 29/102, PSG 16/47). Cost: ~44% more Google calls per sweep.
- **`fetch.rs::clean_html` is a naive strip-all-tags** (§1, D-T11) — nav menus and footers reach
  the prompt, and **34.3% of editor prompts hit the 9,000-char truncation cap.** Hand-rolled
  extraction is spending the model's window on page furniture.
- **`title_pos`'s only remaining readers are SQL functions** (`refresh_co_mention_links` and
  friends). Phase 9 owns them; noted here so nobody re-adds a writer.

### 0e · What is genuinely settled — do NOT re-litigate these in the session

Verified 2026-08-06 after 8.8, with numbers, so the session starts from fact rather than memory:

- **The relevance regex is gone.** The entire Go tree contains three `MustCompile` calls, all three
  RSS *parsers* (`<[^>]+>`, the entity decoder, whitespace). **The Rust tree contains no regex at
  all — the `regex` crate is not even a dependency.**
- **Google is the relevance source.** The primary link is the query hypothesis at 0.95; ingest
  applies no relevance filter; the funnel's only drops are window, dedup and limit, and it
  balances (residual 0 on a live sweep).
- **The Editor is the safety valve, and it fires.** **78 of 507 reads since the flip returned
  `irrelevant` (15.4%)**, and an irrelevant read retracts every vetted row for that article.
- **The Editor replaced the regex as a link GENERATOR and beat it.** Since the flip: **645 player
  links + 87 team + 5 person at 0.90, from 507 reads = 1.27 player links per read**, against the
  deleted regex loop's 3,589 player links per ~8,400 articles = **0.43 per article**. Reading the
  body finds roughly **3× the players** that substring-matching a headline did.
- **The resolver is not fuzzy matching wearing a new coat.** `editor::derive::resolve_names` is an
  EXACT match on `public.nrm()`-normalized surfaces in `entity_name_surfaces`, sport-scoped,
  kind-gated by the model's own `kind_hint` + `descriptor`; two candidates **refuse** rather than
  coin-flip; zero candidates go to the Investigator as discovery. That is describe-then-derive (T2)
  working as designed, and it is why deleting the regex cost nothing.

### 0f · Open decisions the session inherits (detail in the numbered sections)

| id | question | where |
|---|---|---|
| D-T19 | stabilize the fixture gate before scoring anything | §6a |
| D-T20 | knob (a) DONE (proximity clause deleted @ `28fcf45`); does `entity_roles` replace it? | §7a |
| D-T18 | syndication doubles facts in a packet — never dedupe across sources (T3) | §6b |
| D-T11/12 | Editor input hygiene + output dominance | §1, §2 |
| D-T10 | the Investigator's starvation — now 8,624 deep | §3 |
| D-T6/7/8 | Investigator evidence-class gaps | §4 |
| D-T9 | parked ops — **ONLY on Scott's go** | §4 |
| 7.11/7.15 | the voice diet + its eval dry-run — one re-earn event, one fleet-wide regen | §7b |

**Watch while tuning:** `transfer_rumors` **70/24h** against a **68/24h** pre-flip baseline (the
proximity gate came out at 11:38 today — if pair volume climbs, that is the Insider eating the
Mac's permits and D-T20 knob (b) is the answer).

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

---

## 6 · Found during the Phase 8 build (2026-08-06; the first shadow compile + the first §2 reading)

Both are RECORDED, NOT FIXED, per Scott's ruling that session ("no tuning as we go — we'll tune
the weekend"). Both have D-numbers in Appendix D; the diagnosis is here.

### 6a · D-T19 — the editor fixture gate: 43–47/53, and NOT DETERMINISTIC

**This one is first in the queue, because §2 clause 4 depends on it.** The cutover condition asks
`eval --task editor --fixtures` for **100%**, and the gate has never delivered it. Scott waived it
for the flip (2026-08-06) on the standing rule that model quality never halts plumbing — the waiver
is explicit and logged, not silent, and it expires the moment this is tunable.

**Measured:** two consecutive runs, same binary, same fixtures, same `gemma3:4b`, `temp=0` →
**47/53 then 43/53**. Start here: *a gate that moves 4 points between identical runs cannot score
any knob you turn next.* Until it is stable, every other editor measurement in this file is being
read through a ±4 instrument. Candidate causes: Ollama `NUM_PARALLEL=4` batching affecting sampling,
KV-cache reuse across fixture cases, or temp=0 not being greedy in this runner. One measurement
settles it — run the same fixture 10× and plot the spread — and it costs nothing but time.

**The failure shapes**, once the instrument is trustworthy:

1. **`names[]` drops the coach/manager class.** Kyle Shanahan (`coach-discovery-kyle-shanahan` —
   all four of its checks fail together: name absent, so kind, descriptor and the resolver's
   unresolved-record all fall with it), Moyes (`fan-protest-register-outrage`), Arteta
   (`injury-report-accept-no-invention`), Bellingham (`result-line-verbatim-score`), Rangers as a
   club (`opponent-only-mention`). This is **D-T1's under-fill with a specific shape**: the model
   lists the CLUBS and drops the PEOPLE attached to them. That is exactly the channel §1a leans on
   for discovery — `names[]` is how the Investigator learns a person exists — so the miss costs the
   living database, not just the fixture. Knob: the ep1 prompt's names[] ask, which currently
   treats people and clubs as one list; consider naming the coach/manager role explicitly in the
   ask. Measure against D-T1's 16.7% baseline and D-T11's truncation fix together — all three are
   the same class seen from different angles.
2. **`register[outrage]` reads `neutral`** on the fan-protest fixture. This is **D-T2 reproducing**
   under the phrase-before-label order, which was supposed to help. D-T2 says it needs a fixture
   set before revisiting; it now HAS one. Fold them.

### 6b · D-T18 — syndication doubles facts inside one packet

**Finding:** packet 2 (storyline 7471, the first shadow compile) carries 15 claims that are closer
to 8 facts — "Celtic have wrapped up an 11 million pound deal for Kasper Hoog" beside "Celtic have
signed Kasper Hoog"; "Bayern Munich's sporting director denied rumours linking Michael Olise with a
move to Real Madrid" beside "Bayern Munich denies Michael Olise will be leaving".

**The compiler is not at fault, and this is the important part.** The two members are articles
186800 and 186793 — both Goal.com transfer roundups from the same hour, correctly clustered by the
Desk. The packet faithfully carries both, which is the right default: T3 says two outlets asserting
a thing is evidence, and silently suppressing a restatement is precisely how a preserved
contradiction gets dropped. But **two lanes of ONE outlet is syndication, not corroboration**, and
it spends the 2,000-token render budget twice on one fact.

**Do not tune this before measuring what it costs.** On a 3-member packet it is noise. The exact-
title dedup sweep already catches the byte-identical case, so what is left is near-duplicates from
one source. The measurement: over a day of packets, what fraction of render budget goes to claims
sharing a source AND high text similarity? Knobs, in increasing order of how much they can break:
(a) collapse same-source near-duplicate claims at compile, keeping the longer; (b) prefer one member
per (source, hour) at assembly — cheaper, but it discards an article, so it owes an A5 exclusion
line naming what it dropped. **Never dedupe across DIFFERENT sources** — that is the T3 line, and
crossing it turns the contradiction-preserving property of a packet into a summarizer.

---

## 7 · Carried out of the flip (2026-08-06) — read these before tuning the Insider

### 7a · D-T20 — the Insider's proximity gate went inert at the cutover, and nobody chose that

**Measured on live post-flip data:** of 170 `news_article_entities` rows created since
`RAIL=packet` went live, **0 carry a `title_pos`**. Every one is an Editor 8.5 insert, and the
Editor does not compute that column.

**Why it matters here rather than in the rail plan.** `insider::load_candidates`
(`insider/mod.rs:318`) picks the (team, player) pairs the Insider will spend model calls on, and it
is deliberately NOT rail-gated — 7.5 ruled that the packet replaces what articles SAY, never which
articles they ARE. Its thinning clause is:

```sql
AND (te.title_pos IS NULL OR pe.title_pos IS NULL
     OR abs(te.title_pos - pe.title_pos) <= $5)
```

NULL passes, by design. So at 10:55 EDT on 2026-08-06 the gate stopped thinning anything, and the
Insider's candidate set widened — not by a decision, but as a side effect of 8.5 not writing a
column 8.4 never mentioned.

**This is probably the right outcome, which is exactly why it needs deciding rather than
inheriting.** Headline proximity was a proxy for "is this co-mention real?" back when co-mentions
came from a regex scanning a title. On the packet rail they come from the Editor having READ the
body and resolved a name. Proximity is a crutch for noise that no longer exists.

**But measure before you keep it or cut it.** The baseline: `transfer_rumors` ran **68 per 24h**
pre-flip. If post-flip pair volume climbs sharply, the widened candidate set is spending Mac
throughput on pairs the gate used to drop, and that is Insider tuning — it competes directly with
the other five voices for the Mac's single permit. Knobs, in order: (a) delete the clause outright
and let `HAVING count(DISTINCT te.article_id) >= $3` do the thinning (it is a better filter — it
asks for corroboration across articles rather than adjacency in one headline); (b) replace
proximity with the Editor's own `entity_roles` — a `passing_mention` pair is exactly what the
gate was trying to drop, and now we have the model's word for it instead of a character offset.
Knob (b) is the describe-then-derive version and is the one to reach for if (a) proves too loose.

Tied to **8.8** in `PLAN-one-rail.md` (the regex excision session), which lists this as its one
judgment call among otherwise straightforward deletions.

**UPDATE 2026-08-06 ~11:50 EDT — knob (a) IS DONE. 8.8 removed the clause, deliberately and with
the decision logged.** Both sites (`load_candidates` and `load_stale_pair_news_ids`) and
`COMENTION_PROXIMITY_CHARS` are gone as of `28fcf45`; `HAVING count(DISTINCT te.article_id) >= $3`
is now the only thinning, exactly as (a) describes. Re-measured before the cut: **0 of the 271 rows
created since the flip carried a `title_pos`** (170/0 four hours earlier — the finding held as the
sample grew), so it is a no-op for new data and a real change only for the pre-flip tail
(310,705 rows carry a position).

**What is left for this session is knob (b) and the measurement that decides it.** Baseline to beat:
`transfer_rumors` was **68/24h pre-flip** and read **70/24h** just before the cut — no explosion in
the first hour, but one hour is not a reading. If pair volume climbs and the Insider starts eating
the Mac's single permit, (b) is the answer: replace proximity with the Editor's `entity_roles`
(`passing_mention` is exactly what the gate was reaching for), which is the describe-then-derive
version and needs no character offsets at all.

### 7b · Prompt fat is the weekend's main event — the inventory is already written

Scott, at the flip: *"We're going to be able to trim a LOT of fat from the legacy prompts that we
copied over to the new rail."* The measurements that scope that work already exist and should not
be re-derived:

- **§2 of this file (D-T12)** — the Editor's output dominance and capacity numbers.
- **PLAN-one-rail 7.2's window budget** — the per-voice 4096 envelope (system ≤550 tok, memory
  ≤700, packet render ≤2,000, `num_predict` ≤800, prompt p99 ≤3,300). That table is the target
  the trimming aims at, and `eval_count` telemetry is how you assert you hit it.
- **7.11 is the step that owns the RAIL-scoped diet prompts** and is still open — it is where the
  trimmed versions land, and its `s17` bump spends one fleet-wide regen, so batch every prompt
  change into it rather than bumping twice.
