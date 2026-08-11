# Handoff — the session that found the relevance collapse

Supersedes [`HANDOFF-2026-07-28.md`](HANDOFF-2026-07-28.md), which is still accurate on hardware,
schedules and traps but **wrong in §6** (see §5 below). Written 2026-07-28 ~07:50.

Forward work lives in [`PLAN-ingest-simplification.md`](PLAN-ingest-simplification.md) — read
**"The turn"** at the bottom of it first; it is the governing design and it invalidates the shape
diagram at the top of that file.

---

## 1. Start here

**The architecture question is settled and the next step is a design conversation, not code.**
Scott's framing, verbatim, and it should lead:

> *"The idea behind the cron job firing for teams only isn't to exclude players, it's to gather the
> broad topics of the sport. So the idea is to gather the information about the subject sport,
> inclusive of players and teams. Our 'keep' criteria should be 'is this about the sport?' Which
> flows to 'what entities does it include?'"*

And, agreed at the close: **the ingestion layer is a candidate generator.** The query is a
hypothesis, never a claim.

Three decisions are open and **nothing should be built past them** — they are written out in
`PLAN-ingest-simplification.md` under "Three decisions, open":

1. How wide is "the sport"? (any sports reporting vs. only leagues we cover)
2. Sport news naming none of our entities — keep or junk?
3. **Is the packet sport-level rather than entity-level?** The one that matters most, and the
   version of this that actually reaches `VOICE_NUM_CTX` 4096.

---

## 2. What shipped this session

| commit | what |
|---|---|
| `c63a366` | `transfers` self-paces against the worker ceiling instead of being cancelled at it |
| `35be4de` | n9 removed — The Editor writes `news_articles.bucket`; narratives prompt `n15` -> `n16` |
| `3b565ed` | **ar7** — the relevance gate rebuilt around `absent`; `page_kind`/`entity_roles` persisted |

All three deployed to Archbox and live. `cargo test --lib` **280**, clippy 12 warnings (all
pre-existing). Narratives fixture gate **78/78** at n16.

### The one that matters: ar7

`derive_relevance` demanded a **vetted** entity be the `subject`. Calibrated 2026-07-26 against a
vetted list holding teams *and* players. Phase 2 then stopped players auto-vetting, the list became
teams-only, and the unchanged rule silently became *"reject every story whose subject is a person."*

| per day | 07-25 | 07-26 | 07-27 | 07-28 pre-fix | **07-28 post-fix** |
|---|---|---|---|---|---|
| Reader success | 71% | 73% | **2.2%** | 2.1% | **77%** (of model-path reads) |
| vetted player links | 48 | 193 | **12** | 6 | recovering |
| transfer rumors | 453 | 153 | **40** | 12 | recovering |

It was circular: a player link is vetted only by The Reader, but The Reader would not accept an
article whose subject was an unvetted player — so the player could never become vetted. And
`clear_vetted_entities_for_article` unvetted the correct TEAM link on the way out (`NULL` ->
`FALSE`), which removes it from the vetted list **and** from the co-mention candidate pool (that one
selects `vetted IS NULL`). Nothing could reconsider it.

**The bar is now `absent`** — the model's own rejection word, precise for name collisions and
not-in-the-text. `subject`/`opponent`/`passing_mention` all keep. **Opponent-only stories are now
KEPT, reversing an earlier deliberate call** — that reversal is intentional, not a side effect.

---

## 3. Do not re-arm the damaged links yet

**10,366 team links and 2,315 player links across 6,319 articles are sitting at `vetted = FALSE`**
from the incident. All 6,319 are still inside the 14-day window. They will not self-heal — FALSE is
a ratchet.

They need **re-mapping, not re-judging**. Re-arming now pushes them through a gate about to be
replaced, costs ~6,300 gemma3 reads, and re-derives links from the same query hypothesis that
misfiled 2,043 of them. The persisted `relevant_entities` already names the right entities on 5,423
of them, so after the mapping step exists a large share of the recovery costs **zero model calls**.

---

## 4. Corrections to the previous handoff — do not re-litigate

- **§6's `num_predict` thesis is WRONG and must not be actioned.** "Nothing has ever generated more
  than 2,806 tokens, so narratives 4000 -> 3000 is free" measured a *censored* distribution: it
  spans commit `5097607` (2026-07-26 11:33), which raised narratives 3000 -> 4000 and rating
  1200 -> 2000. Split at that commit, narratives' pre-raise max is **exactly 3,000** — the old cap.
  Post-raise p99 is **2,567**. Cutting back to 3,000 re-introduces the truncation that commit fixed.
- **`num_predict` is not a reservation.** It is a generation stop condition; the KV window is
  `num_ctx`. The real constraint is *prompt + actual output* (`ollama.rs:34` states this correctly).
  So "at 8192 the Journalist has only 4,192 tokens of prompt budget" overstates it.
- **`transfers` had no dead letters.** `enqueue`'s conflict policy resets a `failed` row to
  `attempts=0`, so the next news enqueue resurrects it. And a timed-out attempt kept its work —
  pairs persist as they complete and debounce-skip on retry.

---

## 5. Measurements worth keeping

**Where the Journalist's tokens actually went** (this is what justified n16):

| | |
|---|---|
| prose in a full 6-storyline generation, measured from `news_summaries` | **max ever 887 tok** |
| whole generation, `eval_count` | **p99 2,567** |

The gap was `article_buckets` — one object per *corpus* article, so output scaled with the CORPUS
rather than the story. Between 25 and 40 articles the narrative count is pinned at its `maxItems: 6`
cap and output still climbed 1,603 -> 2,096.

**n16 early result**, like-for-like on 8–18 article corpora: p50 output 1,277 -> **714** (−44%), max
2,589 -> **851** (−67%). Sample was 7 generations — **still needs confirming at volume.**

**Prompt sizes by stage** (tokens, `built_prompt`/4, post-raise). The ceiling exists for ONE stage:

| stage | p50 | p99 | max |
|---|---|---|---|
| **narratives** | 2,253 | **8,915** | 9,849 |
| sigil | 830 | 2,246 | 2,838 |
| vibe | 932 | 1,548 | 1,665 |
| transfers | 242 | 964 | 1,112 |
| momentum | 462 | 868 | 947 |
| rating | 718 | 835 | 860 |

Five of six voices fit **4096 today**. Only narratives needs the room, and it needs it because it
re-reads the same articles per entity — which is decision 3 in §1.

**Untested assumption for 4096:** whether ollama reloads the model when a request's `num_ctx`
differs from the loaded slot size. If it does, per-role `num_ctx` thrashes and the shared const is
right. Measure in a rest window before designing around it.

---

## 6. Open, not started

- **`vibe` is truncating.** Post-raise p99 output jumped 144 -> 347 and **2 generations hit its 1100
  cap exactly** in 7 days. It was not in the 07-26 raise. Recommend `VIBE_NUM_PREDICT` -> 1600.
- **The Scout has no personnel feed.** Contract says injuries/suspensions/transfers/coaching from
  The Editor; `peak` reads stats only (`load_rating_profile`, `load_stat_memory`).
- **The `transfers` deferral path has never fired in production.** No big team has been claimed yet.
  West Ham took **705s for 2 candidates and 6 wrap targets** — 59% of the ceiling for one of the
  smallest teams — so it will fire. Watch for `pairs_deferred>0` in the new INFO line.
- **`article_read` probe that went nowhere:** article 187823 was enqueued, its work row cleared, and
  its reading was never touched. Unexplained. Low priority — the 02:00 sweep answered the question —
  but it means an `article_read` item can complete without doing anything.
- Everything in §5 of the previous handoff that is still unticked: `requeue_stale` on its own
  interval + startup guard, the unverified Oracle barrier, the vestigial edition-grid scaffolding,
  offsetting the two cards' rest windows, two `article_read` dead letters.

---

## 7. State as of 2026-07-28 07:50

Harness **active**. Queue: `article_read` 6,268 pending (the sweep plus the n16 regen wave),
`momentum` 465, `sigil` 346, `peak` 349, `narratives` 207, `transfers` 142.

`narratives` and `peak` climbed during the session — expected, and self-limiting: the `n16` prompt
bump forces exactly one regen per news-active entity, and ar7 restored the corpus flow that feeds
them.

**Schedules unchanged**: pause at 00,03,06,09,12,15,18,21:00, resume an hour later; ingest 02:00.
Mac permit `OLLAMA_NUM_PARALLEL=2` confirmed on the running process; Archbox
`COGNITION_BACKEND_CONCURRENCY="...localhost=4,...77:11434=2"`.

**Note for whoever restarts things:** the harness was found stopped at 22:07 on 07-27 with its resume
timer disarmed and a transient unit set to restore at 01:00. That was Scott deliberately resting the
Mac, not a fault. Ask before "fixing" an idle harness.

---

## 8. Traps this session added

- **A green fixture gate cannot see a relevance regression.** The 78/78 narratives gate passed
  throughout the two days the pipeline was discarding 98% of its corpus. Gates test the contract;
  only production rates test the premise.
- **The two fields that decided every relevance verdict were the two the evidence envelope did not
  persist** (`page_kind`, `entity_roles`). The whole diagnosis had to be inferred from blurbs. Fixed
  in ar7 — but the lesson generalises: persist the inputs to a deterministic gate, not just its
  output.
- **A funnel that improves is not a pipeline that improves.** Phase 2's numbers were real —
  `match_rejected` 50% -> 0, zero-admit clubs 15 -> 0 — and the product fell ~90% the same day. The
  funnel counted admissions, not usefulness.
- **A rule calibrated against one population silently inverts when the population changes.** ar6's
  evidence was three team-subject articles; it was still correct when written and catastrophic ten
  hours later. Anything keyed to "vetted" deserves a note about what vetting meant when it was
  written.
