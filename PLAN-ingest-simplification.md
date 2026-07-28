# Plan — the Editor's newsroom

**Status 2026-07-28.** The design is settled. This document is the build order.

The one-line shape:

```
Google (one ranked query per entity)  ->  CANDIDATE ARTICLES     (the query is a hypothesis, never a claim)
  -> Go, generate:  which of our entities might be in this?      (lenient regex, ranked top-N, no judgment)
  -> THE EDITOR:    is this sport reporting?                     (gate)
                    who is actually in it?                       (discovery — names, not confirmations)
                    what is this story, and what does it feel like?
  -> PACKETS:       one storyline, multi-tagged, entity-indexed
  -> per-voice fan-out:  Journalist · Influencer · Insider       (tags)
                         Scout                                   (confirmed facts only)
```

Three properties make this durable where every previous version was not:

1. **The gate does not depend on the vetted list.** "Is this sport reporting?" is answerable from the
   text alone, so no upstream change to what is vetted, linked or queried can silently invert it.
   That inversion is exactly what cost the pipeline ~90% of its output for two days.
2. **Entity mapping is an Editor OUTPUT, not an ingest input.** The Editor adds links the query never
   guessed — including players — and drops the ones it cannot find.
3. **The packet is the unit, and it is read by every voice independently.** Three peers reading one
   packet through three lenses is what makes disagreement possible, and disagreement is the product.

---

## The checklist

Ordered so that each phase is verifiable before the next depends on it. Anything marked **BLOCKS**
gates work downstream of it.

### Phase A — foundations (no model contract changes)

- [x] **A1. `pg_trgm` installed** — mig `195_pg_trgm.sql`, applied to Archbox, recorded. Extension
      only, no index: a GIN trigram index on `news_articles.title` costs ~the size of the column and
      should not be paid for before thresholds are settled. `unaccent` was already present (1.1).
- [x] **A2. The Reader is renamed The Editor** (Tier 1) — commit `fc602f9`. Module, prose, eval task,
      fixture generator. 280 tests, clippy 12 (both at baseline). Deliberately NOT renamed:
      `article_read`/`ARTICLE_READ_*` (stage names, matching the convention that the Journalist's
      stage is `narratives`), `ARTICLE_READ_PROMPT_VERSION`'s **value** `"ar6"` (a cache key — see
      trap T1), and `Role::ArticleReader` (its `env_suffix()` is the live
      `COGNITION_ROUTE_ARTICLE_READER` key on both machines — Tier 3).
- [x] **A3. Exact-title dedup sweep.** Mig `196_collapse_exact_title_duplicates.sql`, applied to
      Archbox. Backfill: **3,618 marked, corpus 32,006 → 30,291 (−5.4%)**; second run returns 0, so
      it is idempotent. Wired HOURLY into the worker tick (not per-tick — it only has work after a
      scrub batch settles).

      Root cause was a race, not a missing mechanism: `novelty::gate` compares an article against
      canonical coverage of its OWN VETTED ENTITIES, so two copies scrubbed in the same pass, before
      either has membership, are invisible to each other. A per-article gate cannot close that.

      Three guards, one of which the dry run earned: the cross-source check must be **per pair, not
      per group** — a group of {A, A, B} passes a group-level `count(DISTINCT source) > 1` and then
      suppresses A with its own sibling, which is exactly the same-source collapse the cosine branch
      was deleted for. Verified: 0 same-source collapses among the 3,618. Also a 30-char minimum
      title length (section headers like "Match Centre" and "El Tiempo" repeat verbatim across
      unrelated articles), and a canonical that prefers the corpus-visible copy.

      **Corrects an earlier number in this plan:** the "4,749 collapsible" figure counted articles
      that never reach the Journalist. `load_vetted_corpus` already requires `vetted IS TRUE`, so
      marking those changes nothing. The real corpus-visible duplication was **2,057 of 32,016
      (6.4%)**.
- [x] **A4. `news_articles.bucket` → a routing tag SET.** Mig `197_article_routing_tags.sql`,
      applied to Archbox. Adds `news_articles.routing_tags text[]` (GIN indexed), the
      `stage_routing_subscriptions` table, and a fan-out trigger over NEWLY-ADDED tags only.
      `bucket.rs` gains `routing_tags_from_story_type`; the Editor writes tags alongside `bucket`.

      **Tags are content facts, not voice names** — `transfer`, `injury`, `roster`. Who wants them
      is data in `stage_routing_subscriptions`, which makes **E1 an INSERT rather than a code
      change**, and means adding a voice never touches the trigger.

      **Ships INERT**: the subscription table is empty, so the trigger fans out to nobody and
      transfers keeps running off mig 175. Seeding `('transfer','transfers')` here would double-
      enqueue against mig 175 with a *different* `input_version` — not a duplicate (ON CONFLICT
      handles that) but a churn loop where the two fingerprints alternate and reopen the item
      forever. Phase E migrates it deliberately.

      Verified on Archbox in rolled-back transactions: inert with no subscriptions (0 enqueued);
      one article with two tags reaches **two different stages**; and adding a third tag later wakes
      **only** that tag's subscriber — E2's per-voice re-wake guard, working.
- [x] **A5. Journalist corpus `LIMIT` + `ORDER BY feed_rank`.** The fix already existed on
      `load_vetted_corpus` — but that function is only reached by `eval_tasks`. The production path
      is `load_vetted_corpus_with_exclusions`, and it was scanning **unbounded, ordered by
      recency**. The fix had landed on the eval path and the live path never got it, which is the
      direct cause of the Journalist's 8,915-token p99 while five of six voices fit 4096.

      Now two orderings, deliberately separate: `feed_rank` decides WHICH articles survive the
      budget, recency decides how survivors are PRESENTED. Restored the `budget_truncated`
      exclusion band with it (retired in Phase 3 alongside the old cap) — an article dropped from
      the evidence must be named somewhere, or the ledger's accounting silently stops adding up.

      Verified on Archbox against FC Barcelona, 166 in-window articles: kept exactly **40**
      (feed_rank 0–21), 126 `budget_truncated` (rank 21–98). The kept set averages **feed_rank 9.1
      vs 36.0** under the old recency ordering.

      **PROVEN IN PRODUCTION 2026-07-28 17:58, n=128.** The claim was open for two sessions; it is
      closed. Two independent confirmations:

      *Mechanism, exact.* Ledger 91976 (team 79, 51 in-window): ranked all 51 by the production
      ordering and compared the tail beyond position 40 against the recorded drop set — **11 of 11
      predicted, 0 false drops, 0 false keeps.** The cap cuts on `feed_rank`, not recency. All five
      capped generations reconcile (`dropped_count` == `length(dropped_news_ids)`: 11/11, 6/6,
      17/17, 5/5, 7/7), so nothing leaves the evidence unaccounted for.

      *Distribution.* | | p50 | p90 | p99 | max | n |
      |---|---|---|---|---|---|
      | baseline (24h pre) | 1,850 | 4,886 | 7,401 | 8,374 | 228 |
      | post-deploy | 642 | 1,983 | 3,097 | 3,470 | 128 |

      Capped prompts cluster at 2,746–3,470 — the population that produced the 7,401 p99 now *is*
      the ceiling, and the ceiling is 3,470. **The sample is enriched for over-cap entities** (47% of
      the narratives queue vs 26% of entities carrying corpus, because the regen wave is made of
      entities whose corpus shrank), so the measured reduction is conservative.

      Still draining: ~113 over-cap entities queued at deploy+6h, ~10 per four hours through the
      pause windows. Self-limiting, and each generation is cheaper than the one it replaces.

### Phase B — mapping and recovery (no new model contract)

- [~] **B1. The name resolver.** *Groundwork APPLIED and committed (`baaeb9a`); the resolver is not
      yet wired to the live rail.* Editor-emitted name strings → canonical entities. Mig
      `198_entity_name_resolution.sql` lays the groundwork: `nrm()` (the ONE normalizer, in SQL so
      the index and the query provably agree — a Rust copy is the T-A5 trap in a new costume, and
      there is no `unaccent` crate in the tree), `entity_name_surfaces`, and both indexes.

      **Revised by measurement — see T9.** Exact match on the normalized surface is the *only*
      automatic path. Trigram is a ranking and review channel, never an unattended write. The
      original gate (a) — "the name must appear in the body we already have" — is a **live-path gate
      only**: `news_articles.full_text` is NULL for all 150,566 articles and nothing writes it
      (`journalist/prompt.rs:158` already records this). The Editor holds the body at read time and
      does not persist it, so the offline backfill has no body to check against. Checked against
      what IS retained (title + description + `evidence_blurb` + `key_facts`), only 76.9% of correct
      resolutions "pass" — the failures are summarization, not hallucination, so applying it offline
      would discard a quarter of the recovery for the wrong reason.

      Ambiguity is **refused, not broken**: 59 of 15,654 exact resolutions (0.38%) hit more than one
      entity, all same-sport player namesakes, zero team/player and zero cross-sport collisions.
      Roster context (`team_rosters` ∩ the article's teams) resolves 46 of those 59 for free — but
      **not Vinicius Jr**, where Vinicius Junior and Vinícius Tobias share Real Madrid and the rule
      ties. That residual is what the aliases in mig 198 and the discovery seat below are for.
- [ ] **B2. Go retires as judge, stays as clerk.** `MatchesEntity` becomes a candidate *generator*:
      lenient, high-recall, ranked, top-N. It decides nothing. Leniency is bounded by prompt budget —
      candidates go INTO the prompt, so hand over a ranked top-N, not everything that matched.
- [ ] **B3. Unmatched-name capture.** Names the Editor found that resolve to nothing get persisted.
      That set is the candidate pool for growing the DB later, and it costs nothing to keep now.
      Junked articles keep a row so the unmatched names have something to hang on.

      **Measured, and it is the larger half.** On the incident cohort, 10,263 name instances / 6,408
      distinct names resolve to nothing. Sampling the most frequent shows they are not resolver
      failures — they are a **census of what the DB does not model**: national teams (`spain` 62,
      `france` 56), coaches and managers (`kyle shanahan` 60, `john lynch` 35, `andy reid` 23,
      `xabi alonso` 23, `pep guardiola`), clubs outside our five leagues (`celtic` 45, `wrexham` 44,
      `ajax` 31, `galatasaray`, `feyenoord`), other sports (`tadej pogacar`, `caitlin clark`), and
      genuine non-sport noise (`andy burnham`, `ice`). Persisting them turns the Editor into a
      standing survey of coverage gaps, which is worth more than the links B1 recovers.
- [ ] **B4. Re-mapping backfill over the 6,319 held articles.** **Do this before B2 goes live.** It
      exercises the resolver on 6,000 real articles offline, where mistakes are inspectable. The
      persisted `relevant_entities` already names the right entities on 5,423 of them, so a large
      share costs **zero model calls**. See "Recovery, deliberately held" below.

      **Pin the cohort to the incident window.** `vetted IS FALSE` alone is now **24,984 articles /
      28,280 team / 9,044 player links** — normal scrub rejections have accumulated on top since
      07-27, and those are legitimate. The cohort is articles whose Editor reading was updated
      between 07-27 00:00 and 07-28 07:04: **6,377 articles / 10,409 team / 2,366 player**, which
      reproduces the recorded 6,319 / 10,366 / 2,315 to within the drift since.

      **Measured yield** (exact match on `entity_name_surfaces`, sport-scoped, ambiguity refused):

      | | links | entities | articles |
      |---|---|---|---|
      | existing links flipping FALSE → TRUE | 9,818 | 905 | 5,370 |
      | brand-new links the query never proposed | 5,910 (1,507 team, **4,403 player**) | 1,935 | 2,152 |

      5,473 of 6,377 articles resolve at least one entity. Zero model calls.

      **Staged, Scott's call 2026-07-28:** flips first, inspect, then the brand-new links as a second
      pass. Brand-new rows carry a `match_confidence` sentinel distinct from Go's 0.95 primary so
      Editor-derived links stay greppable and reversible. **And the trigger must be suppressed —
      see T10**, or the cheap half of this item silently buys 5,370 Editor re-reads.

### Phase C — the Editor's new contract (one prompt version bump)

- [ ] **C1. Discovery.** The Editor is currently handed `vetted_names` + `co_mentions` and asked what
      part each plays. **It is never asked who else is in the article.** That is the whole bleed —
      measured, it read 99 articles mentioning Vinicius Junior and linked him in 24.
- [ ] **C2. Emotional register.** A small closed enum (celebration / outrage / resignation /
      anticipation / neutral) **plus the phrase that shows it**. Never a score — the Influencer owns
      the number. See trap T2.
- [ ] **C3. Field order.** Extraction before anything derived. Field order IS the contract (ar4);
      constrained decoding emits properties in schema order, and moving the verdict from first to
      last was the difference between 99.1% rubber-stamping and a working gate.
- [ ] **C4. Bump `ARTICLE_READ_PROMPT_VERSION`.** This is the change that *earns* the re-read wave.
      Free here, catastrophic if done casually earlier.
- [ ] **C5. Watch the output budget.** `ARTICLE_NUM_PREDICT` is 900 and the Editor covers 21% of
      articles because it is the throughput bottleneck. **The Editor's output budget is coverage** —
      every token added is articles/day not read. Keep both new fields terse.

### Phase D — packets (storylines)

- [ ] **D1. Schema.** `storylines` (identity, title, state, first_seen, last_updated) and
      `storyline_entities` (the edge: role, joined_at, left_at, exit_reason, last_seen_at).
- [ ] **D2. Incremental assignment.** **Not a batch compile** — there is no context window in which
      "here is today's football corpus, find the stories" is a call you can make (football alone is
      6,344 articles/day). Each article is offered a *shortlist* of candidate storylines — free,
      because mapping just named the entities and entities point at their open stories — and it
      attaches or opens a new one.
- [ ] **D3. Use the closed-candidate-list pattern.** Do NOT emit a free-text storyline name: "Saka
      injury" and "Bukayo Saka hamstring" will not match, which is the whole problem embeddings were
      solving. Show a NUMBERED list and take a pick by number — the same shape and parser discipline
      the co-mention path already uses (`resolve.rs`).
- [ ] **D4. Tail attachment.** 79% of articles never earn a model read. Attach them to existing
      storylines by similarity, no model call. **Respect the bands** — see trap T3.
- [ ] **D5. Entity participation lifecycle.** The story has a lifespan; the entity's part in it has a
      *separate* one. Arsenal joins, Arsenal's part ends, the story runs on at PSG. Fading is cheap
      (`last_seen_at` + decay). Losing out is not — put that judgment on the **story**, once: when a
      storyline resolves, name who it resolved for, and every other participant's edge closes as
      "not the outcome" in one stroke. Derive the close in code; never ask a 4B to render it.
- [ ] **D6. Packets carry contested state.** Same story, same day: *"Real Madrid reach agreement with
      RB Leipzig"* and *"Real Madrid yet to reach agreement — The Athletic."* The disagreement IS the
      story. A storyline storing one settled fact would flatten exactly what makes it interesting.

### Phase E — the newsroom fan-out

Depends on A4 and C2.

- [ ] **E1. Per-voice routing, derived not asked.** Tags fall out of fields the Editor already emits:
      `injury`/`roster` → (Scout, see E4), `transfer` → Insider, non-neutral register → Influencer,
      always → Journalist. Asking gemma "which voices should see this?" is a judgment call, and this
      codebase has paid three prompt revisions to learn it will not render those. Derived routing is
      inspectable and cannot silently invert when the cast changes.
- [ ] **E2. Per-voice re-wake fingerprint.** A packet that gains an article re-fans only to voices
      whose **slice** changed. Diomande gains a suspension → the Scout's slice moved; the Insider's
      did not. This is the mig-175 `IS DISTINCT FROM` guard one level up, and without it every
      article on a running saga wakes every voice for every involved entity.
- [ ] **E3. The Influencer goes direct.** Remove the Journalist gate. **This cannot land before C2** —
      `enqueue_vibe_if_needed` returns `Ok(false)` on empty context, so an Editor-side enqueue with
      no Editor-supplied material would silently no-op forever. Rebuild the debounce so Editor cues
      count as material. Update her contract text: she may now be the *first* voice on a story, and
      the current wording (*"may not introduce an event that no one else filed"*) assumes a peer
      filed it first.
- [ ] **E4. The Scout's fact feed.** The Scout is **not** a packet subscriber. Transfer speculation
      never reaches it. It wakes on the confirmation layer — `transfer_identity_applications`
      (adjudicated, with `deterministic_confidence`) and `transfer_ground_truth` (applied, with
      ledger + ref) — where a threshold has been met and entity meta is updated. A row there is a
      fact, exactly like the percentile band the Scout is already handed. **No prose reaches the
      Scout.** See trap T4.

      **This survives the retirement of third-party ingestion, and it is worth knowing why.**
      `transfer_identity_applications` is populated off `source_rumor_id`/`source_synthesis_id` with
      `'source': 'mistral_adjudication'` (migs 118/120/124) — it is **entirely news-derived**. No
      vendor ever touched it. The chain is rumor → deterministic heat + confidence → threshold →
      adjudication → applied, and that chain is exactly what makes a news-derived fact safe to hand
      the Scout. It is the template F4 should follow.

      **One caveat:** `player_team_history` is written by `detect_team_change`, which is driven by
      provider roster sync, so it goes quiet now. E4 does not need it —
      `transfer_identity_applications` is the richer record and the news-derived one — but anything
      else keying on `player_team_history` freshness should be re-checked.
- [ ] **E5. The Oracle's disagreement contract.** See "The disagreement finding" below — this is a
      real unlock sitting unused, and E3 is necessary but **not sufficient** for it.

### Phase F — deferred, with reasons

- [ ] **F1. Rename Tier 3** — the `article_read` queue kind (a live queue with 6,268 pending items),
      `news_article_readings`, `Role::ArticleReader` + `COGNITION_ROUTE_ARTICLE_READER`. Each is a
      migration or a coordinated two-machine config change, not a text substitution. I would argue
      for leaving `news_article_readings` alone: a migration and a schema snapshot for zero
      behavioural gain.
- [ ] **F2. Delete BGE.** Storylines replace `threads`' cosine clustering — grouping becomes a
      `GROUP BY` rather than a clustering pass, and the embedder has no consumer left in this path.
      This finishes a teardown already two-thirds done.
- [ ] **F3. Per-league editions** (old Phase 4). One edition per team, the correct one — Bundesliga →
      `de-DE`, La Liga → `es-ES`. Same call count as today. Blocked on F2; `teams.country` is clean
      for football (23 NULLs need a backfill or an `en-GB` fallback).
- [ ] **F4. Injuries and suspensions come from the NEWS RAIL, and need a confirmation gate.**
      **Third-party ingestion is retired** — Scott cancelled BallDontLie and SportMonks 2026-07-28.
      The provider-entitlement question this item used to carry is moot; the probes that produced it
      are recorded in commit `fff0c27` and should not be re-run.

      That makes the news rail the only source, which lands squarely on trap T4: the Scout must
      never interpret prose. **The resolution is the confirmation pattern the transfer path already
      proves** (see E4) — the news rail produces injury *claims*; a deterministic heat/confidence
      threshold plus an adjudication step promotes a claim to *confirmed*; the Scout reads confirmed
      only. The Scout still receives facts. They are simply facts confirmed by a gate rather than by
      a vendor, which is what the transfer path has been doing all along.

      Not scheduled — Scott, 2026-07-28: *"don't worry about those for now."* Recorded so the shape
      is known when it is.
- [ ] **F5. `vibe` is truncating.** Post-raise p99 output jumped 144 → 347 and 2 generations hit the
      1100 cap exactly in 7 days. Recommend `VIBE_NUM_PREDICT` → 1600. Unrelated to this plan.
- [ ] **F7. A discovery / identity seat — the junction this plan keeps writing around.** Scott's
      call, 2026-07-28: a future junction (a new character in the newsroom) owns the problems B1
      hands off rather than solves.

      **What accumulates for it, from the B1/B3 measurements:**

      | residue | size | why deterministic code cannot finish it |
      |---|---|---|
      | true namesake ties | 13 of 59 after roster context | Vinicius Junior and Vinícius Tobias share a club — the roster rule ties, not resolves |
      | people we do not model | ~60/day, e.g. `kyle shanahan`, `john lynch`, `andy reid` | coaches and executives drive real stories and fuzzy-match to real *players* (T9) |
      | entities outside our leagues | `celtic`, `wrexham`, `ajax`, `galatasaray` | the DB boundary is a business decision; the article does not respect it |
      | national teams | `spain`, `france`, `portugal` | a whole entity CLASS with no table |
      | genuine noise | `andy burnham`, `lee child`, `ice` | needs judgment, not a threshold |

      **The shape it must take, and why it is not just "ask a model which entity":** T2 says gemma
      will not render a verdict as a bare field, proven three times (ar3 99.1% accept, ar5's
      `score_stub` that still said `relevant:true`, the Oracle's `DISAGREEMENT:` at 7-in-13,252).
      Asking *"which of these two is most relevant?"* is asking for the verdict. **Describe, then
      derive** — the seat states what the text says about the person (club, role, competition,
      nationality) and the match falls out deterministically against those facts.

      Cheapest version first: much of this is **missing aliases wearing a disambiguation costume**.
      `Inter Milan` and `Vinicius Jr` are unambiguous to any reader and ambiguous only to trigram; an
      alias row fixes them permanently at zero inference cost (mig 198 seeds 15, hand-verified). The
      seat earns its keep on what an alias *cannot* fix — the residual above — and the natural first
      home for the evidence is **C1's discovery contract**, where the Editor already has the body
      open. ~4 names/article × a club each ≈ 16 output tokens against `ARTICLE_NUM_PREDICT` 900,
      under 2%, versus a whole extra inference. That buys disambiguation for *every* name rather
      than only the ties we happen to detect — and C5 is the standing constraint: the Editor's
      output budget IS coverage.
- [ ] **F6. Plumbing** — `requeue_stale` on its own interval + startup guard, the unverified Oracle
      barrier, the vestigial edition-grid scaffolding (`defaultRSSEditions`, `EditionsPlanned/
      Queried/Skipped`, `runRSSQueryPastLimit`'s `editionIdx`), offsetting the two cards' rest
      windows, two `article_read` dead letters.

---

## The newsroom, as designed

```
Editor ──packets, per-voice routing──> Journalist · Influencer · Insider
       ──confirmed facts────────────>  Scout
Analyst  <── Scout + Influencer outputs        (the one peer-aware seat, by design)
Oracle   <── five cards                        (blind — she reads cards, not material)
```

Verified against the code, 2026-07-28:

- The vetted trigger now enqueues **only** `article_read`. The Editor is the sole entry point and
  fans out itself.
- **The Journalist** is already enqueued by the Editor directly. ✅
- **The Insider** is already Editor-caused, via the `bucket` write → mig-175 trigger. This only
  became true at n16 (2026-07-27); mig 174's comment still says "the Journalist (n9) labels each
  article," which is now stale.
- **The Influencer** is enqueued by the **Journalist**, not the Editor. ❌ → E3.
- **The Analyst** reads *"The Scout's PEAK report, The Influencer's vibe"* — the only peer-aware
  voice, and correctly so.
- **The Oracle** reads five cards; `news_summaries` is the Journalist's *card*, not raw material.
  She is blind in the intended sense.

---

## The disagreement finding — a built unlock sitting unused

The Oracle's contract already makes disagreement first-class: *"the reply may carry `CONVERGENCE:`,
`DISAGREEMENT:` and `WHY_NOW:`... When the cards genuinely conflict, the honest reading says so."*

Measured over the whole `sigil_synthesis` table:

| | |
|---|---|
| sigil readings carrying prose | 13,252 |
| that mention disagreement | **7** (0.05%) |
| that carry `WHY_NOW` | **0 — never fired** |
| deterministic `pillar_convergence` | avg 67.9, p50 74, **min 1**, max 100 |

**The system detects divergence and the voice never says it.** Two independent causes, and both must
be fixed or the unlock does not land:

1. **Structural.** Two of the five pillars *cannot* disagree today, because vibe is a function of
   narratives. The Oracle is reading the Journalist twice — once directly, once laundered through a
   sentiment pass. E3 fixes this.
2. **Contractual.** `DISAGREEMENT:` is an **optional** field the model may volunteer — which is
   exactly what `relevant` was before ar6, when gemma rubber-stamped 99.1% of articles because
   nothing forced the negative judgment. E5 fixes this: `pillar_convergence` is already computing
   the divergence signal deterministically and the prompt does not use it. When it is low,
   disagreement stops being optional.

---

## Measurements this plan rests on (2026-07-28)

**Volume.** FOOTBALL 6,344 articles/day, NFL 1,148, NBA 648. Football is 78% of the pipeline —
whatever gets built is a football system that also does NFL and NBA.

**Coverage.** 21.3% of articles earn a model read. The other 79% reach the Journalist on their
headline. Links are vetted by **Scrub** (a naming *fact*), not by the Editor — so the tail is carried
at lower fidelity, not lost.

**The collapse ratio.** Real Madrid, 2026-07-26 — 110 candidate articles, hand-counted into about
**five stories** plus ~20 junk/spam/archive:

| story | articles |
|---|---|
| Diomande → Real Madrid (RB Leipzig), PSG bidding then walking out | ~25 |
| Vinicius Jr → Arsenal | ~15 |
| Rodri: Man City → Madrid/PSG | ~7 |
| Lee Kang-in → Atlético (**not a Real Madrid story at all**) | ~18 |
| Julián Álvarez / Atlético / Barça | ~3 |
| junk: Big Brother, Valencia wildfires, WTA Madrid, a Telugu film trailer, a U12 final from 2024, a fixture page dated 2027 | ~20 |
| exact-duplicate titles from different sources | 3 pairs |

**~20:1 on the biggest cluster**, on the entity where duplication is heaviest.

**The player-discovery bleed.** Mention-vs-link over 7 days, joined on stable IDs:

| player | mentions | linked |
|---|---|---|
| Rodri | 281 | 280 |
| Kang-in Lee | 113 | 113 |
| Yan Diomande | 385 | **200** |
| Michael Olise | 144 | **70** |
| Vinicius Junior | 182 | **39** |

Caveat kept deliberately: the patterns over-match (`vinicius` also hits three other Viniciuses), so
these are **upper bounds on mentions and therefore upper bounds on the miss rate**. Olise is the
cleanest signal — distinctive surname, exact canonical name, still ~49% missed.

Split by whether the Editor read the article, the miss rate is **barely better**: Olise 12/22 read vs
58/122 unread; Vinicius 24/99 read vs 15/81 unread. **It is a contract gap, not a capacity gap** —
the Editor has the body in context and is never asked who is in it.

**Theories this session killed, recorded so they are not re-derived:**
- *Accent folding.* `unaccent` was already installed; **0 teams and 8 players** lack an ASCII alias,
  and Atlético is linked on 286 of 289 mentions. Not the bleed.
- *Aliases generally.* Team matching is fine (~99%). The gap is player *discovery*, not name forms.
- *"The tail is lost."* It is not — Scrub vets links for 100% of articles; reading is an upgrade.

---

## Recovery, deliberately held

10,366 team links and 2,315 player links across **6,319 articles** were set `vetted = FALSE` during
the 07-27 incident. That state is a ratchet: FALSE is excluded from the vetted list AND from the
co-mention candidate pool (which selects `vetted IS NULL`), so nothing reconsiders them. All 6,319
are still inside the 14-day window.

**Do not re-arm them.** They need re-MAPPING, not re-judging. Re-arming pushes them through a gate
about to be replaced, pays ~6,300 gemma reads to do it, and re-derives their links from the same
query hypothesis that misfiled 2,043 of them. This is checklist item **B4**.

---

## Traps

**T1 — `ARTICLE_READ_PROMPT_VERSION`'s value is a cache key, not a label.** Every reading whose
`prompt_version` differs is invalidated and re-read lazily. Renaming the `ar` namespace as part of a
cosmetic sweep would silently spend thousands of gemma calls re-reading the whole corpus. It gets
bumped when a real contract change earns it (C4).

**T2 — gemma will not render a negative or conflicting judgment as a bare field.** Proven three
times: ar3 (99.1% accept), ar5 (labelled a boxscore `score_stub` and still said `relevant:true`), and
now the Oracle's `DISAGREEMENT:` at 7-in-13,252. The answer every time is the same: **describe, then
derive**. Never ask for the verdict.

**T3 — similarity bands are not interchangeable.** Measured on the Real Madrid day:

| band | pairs | meaning | action |
|---|---|---|---|
| ≥ 0.9 | 4 | true restatements | safe to dedup |
| 0.5–0.75 | 28 | same story, **different or contradictory** claim | attach as a source — never collapse |

At 0.71: *"Real Madrid reach agreement with RB Leipzig"* vs *"Real Madrid **yet to** reach agreement
with RB Leipzig."* Opposite claims, high similarity. Naive dedup at a loose threshold would quietly
delete the disagreement, which is the story.

**T4 — the Scout's reliability is the L8 discipline, and prose breaks it.** The percentile→tier
mapping was *taken away from the model* and fed in as a labeled fact, because local models invert the
relation and call a 37th-percentile skill "above average." Everything reaching that seat must arrive
as a fact requiring no interpretation. The other three voices are interpreters by trade; the Scout is
not.

**T5 — a green fixture gate cannot see a relevance regression.** The 78/78 narratives gate passed
throughout the two days the pipeline was discarding 98% of its corpus. Gates test the contract; only
production rates test the premise.

**T6 — a rule calibrated against one population silently inverts when the population changes.** ar6's
evidence was three team-subject articles; it was correct when written and catastrophic ten hours
later, once Phase 2 changed what "vetted" meant. Anything keyed to the cast deserves a note about
what the cast was when it was written.

**T7 — expect the busyness verdicts to move.** Today five outlets on one rumor look like **five
signals of activity**. Under packets it is one story with five sources. `card_score`, momentum, the
whole "how much is happening here" read is currently counting *coverage volume* and calling it *news
volume* — and it will separate hardest on the biggest clubs, where duplication is heaviest. The n16
baselines and the 78/78 fixture gate are **not comparable across this change.**

**T8 — mig 194 is applied but unrecorded** in `schema_migrations` (column, index and comment all
verified present). Fully idempotent, so the next `migrate.sh` re-applies it as a no-op and records
it. Self-healing — but it means 194 was applied by hand rather than through the runner.

**T9 — a trigram margin gate protects against the wrong failure.** The intuition is that fuzzy
matching goes wrong on TIES, so requiring the best match to beat the runner-up should make it safe.
Measured over the 120 most frequent unresolved names, sport-scoped, it does not: the dominant error
is a **confident single match to an entity that is simply not the one named**, and those have no
runner-up at all, so they clear any margin gate:

| model's name | best match | sim | margin | verdict |
|---|---|---|---|---|
| `spain` | team 394 `spa` | 0.429 | **0.429** | wrong — a national team we do not carry |
| `pep guardiola` | player `sergi guardiola` | 0.500 | **0.500** | wrong — a manager, not a player |
| `sheffield wednesday` | team 21 `sheffield utd` | 0.417 | **0.417** | wrong — a rival, same city |
| `charlton athletic` | team 13258 `athletic club` | 0.455 | **0.455** | wrong — different country |
| `lee child` | team 71 `lee` | 0.400 | **0.400** | wrong — a novelist |
| `vinicius jr` | player 600687 `vinicius junior` | 0.556 | 0.082 | **correct** |

Every wrong row clears the gate more comfortably than the one correct row. So **exact match is the
only automatic write path**; trigram is a ranking and review channel. What the margin gate *does*
catch is the true tie — `inter milan` scores 0.500 against **both** Inter (2930) and AC Milan (113),
and a top-1 pick would link Inter Milan stories to their rivals on a coin flip.

**T10 — flipping `vetted` FALSE → TRUE re-arms the article.** `enqueue_derive_on_vetted` fires
`AFTER UPDATE OF vetted` and enqueues **`article_read` on the article**, with
`input_version = 'ar:' || vetted_count`. Flipping links changes that count, so the `ON CONFLICT`
clause re-opens even articles already read. Verified on Archbox in a rolled-back transaction: one
flip, one new `article_read` row.

That means the "cheap" half of B4 — restoring links that already exist — would have bought **~5,370
Editor re-reads through the ar6 gate that C1 is about to replace**, which is precisely what
"do not re-arm the 6,319" forbids. The backfill must suppress the trigger:
`SET LOCAL session_replication_role = 'replica'` inside the transaction (verified: suppresses it,
no lock, auto-reverts at commit; `ALTER TABLE ... DISABLE TRIGGER` would take an ACCESS EXCLUSIVE
lock against a live pipeline). The links then become visible to the Journalist immediately, carrying
the reading they already have. **Note the shape:** the trigger is article-keyed, so the blast radius
of a vetted write is measured in re-reads, not in entity derivations.

---

# Appendix — the turn that produced this design (2026-07-28)

Scott, opening the session that found it: *"The idea behind the cron job firing for teams only isn't
to exclude players, it's to gather the broad topics of the sport. Our keep criteria should be 'is
this about the sport?' — which flows to 'what entities does it include?'"*

## What the old shape assumed, and why it broke

Every earlier version of this plan treated the ingest query as a **claim**: we asked Google for
Arsenal, so this article is about Arsenal, and the Reader's job is to confirm or deny it. Phase 2
leaned harder on that assumption, not less.

The assumption is false. Google's page 1 for a team is a sample of that team's *news neighbourhood*:
the club, its players, its rivals, its league. Asking "is this about Arsenal?" of an article about
Saka's hamstring gets a defensible **no**, and then we delete it.

| per day | 07-25 | 07-26 | 07-27 | 07-28 (pre-fix) | 07-28 (post-fix) |
|---|---|---|---|---|---|
| Reader success rate | 71% | 73% | **2.2%** | 2.1% | **77%** |
| vetted player links | 48 | 193 | **12** | 6 | recovering |
| transfer rumors | 453 | 153 | **40** | 12 | recovering |
| narratives | 1,686 | 1,242 | 793 | 217 | recovering |

The proximate bug is written up in `ar7` (`3b565ed`). The architectural cause is this section's
subject: **a relevance rule keyed to "is it about the entity we guessed" is only ever as good as the
guess**, and it silently inverts whenever the guess changes.

## The three decisions, resolved

1. **How wide is "the sport"?** As wide as the sports we cover. But the league question turned out to
   be *free*: an article about a league we do not cover names none of our entities, so the mapping
   step junks it anyway. **The gate never has to know what leagues we cover** — and it should not,
   because that recreates the same fragility. The gate asks a shape question only.
2. **Sport news naming none of our entities?** Junk — but AFTER mapping, never before, and capture
   the unmatched names (B3). Flagged for the future: once the DB grows on its own, those names are
   the candidates.
3. **Is the packet sport-level rather than entity-level?** Yes, **as long as the downstream output
   stays entity-level.** The Editor compiles the stories of the day; the voices tell each entity's
   version. This is what actually reaches `VOICE_NUM_CTX` 4096 — today the same article is re-read
   and re-reasoned per entity, which is why the Journalist's prompt is 8,915 tokens at p99.

## The regex was never wrong; it was wired as the judge instead of the clerk

Phase 2 retired `MatchesEntity` because it rejected 50% of everything Google returned and starved
fifteen clubs to zero. Measured again on 07-28, as a gate it is worse than that: restoring it would
remove **64% of the junk and 54% of the genuinely successful reads**, because `teams.name` is
canonical and the press writes "Spurs."

As a **candidate generator** those same numbers are fine. Recall misses are cheap when a model
adjudicates the shortlist afterwards, and precision costs nothing because the model discards what is
not there. 204 teams and 15,986 players is a trivial lookup.

**Retire it as a filter, restore it as a generator.** That is the sentence this plan should have had
from the start.

## Measured: how much was misfiled rather than junk

Of the 6,296 articles the broken gate rejected between 07-27 00:00 and 07-28 07:04:

| | |
|---|---|
| name at least one of our entities | **5,423** |
| name one of ours that was **not linked** | **2,043** |
| player mentions among them | **6,210** |

Roughly a third of the "junk" was an article about a real entity of ours, filed under the wrong one.
Both figures are FLOORS — the match was exact-lowercase-name, so every "Spurs" and "Man Utd" missed.

---

# Appendix — completed phases (historical)

**Phase 1 ✅** (`8f2a1c`, 2026-07-26) — the ingest cut ranks by `feed_rank` instead of publish date.
`sortArticlesByDate` deleted. "Top N" now means one thing end to end instead of two disagreeing
things.

**Phase 2 ✅** (2026-07-26 23:14) — regex tier retired, players stop auto-vetting, `-rss-limit` = one
Google page. **Note:** this phase's funnel numbers were real (`match_rejected` 50% → 0, zero-admit
clubs 15 → 0) and the product fell ~90% the same day. The funnel counted admissions, not usefulness.
See T5.

**The measurement Phase 1/2 rested on** — one sweep, 2026-07-26 12:00:

| | count | share |
|---|---|---|
| RSS items Google returned | 9,694 | 100% |
| `match_rejected` — our regex overruled Google | 4,859 | **50%** |
| `limit_truncated` | 2,235 | 23% |
| `dedup_collapsed` | 1,160 | 12% |
| `matched` — actually persisted | 1,440 | 15% |

Fifteen football teams were admitted **zero** articles — precisely the short/ambiguous names the
regex guard existed to protect.

**How big is page 1?** RSS has no pagination; Google caps at 100 items. Arsenal/Man Utd/Bayern/Lakers
return 100 (capped); Spezia and Huesca return 3. **The count is itself a relevance signal** — coverage
scales with how much story actually exists. A flat `-rss-limit` never binds on small clubs and
exclusively starves the biggest stories.
