# Plan — a simple, durable fetch funnel

**The shape being built toward:**

```
Google (one ranked query per entity)  ->  top N by rank  ->  dedup  ->  Reader  ->  characters
```

The Reader is the scrubbing layer. Nothing else judges relevance. No regex tier, no embedder,
no second opinion on a question Google already answered.

Opened 2026-07-26. Companion to [PLAN-backlog-churn.md](PLAN-backlog-churn.md) — that one is about
draining what exists, this one is about not manufacturing it in the first place.

---

## The measurement this rests on

One real sweep, 2026-07-26 12:00, from `logs/pipeline-ingest.log`:

| | count | share of what Google returned |
|---|---|---|
| RSS items Google returned | 9,694 | 100% |
| `match_rejected` — our regex overrules Google | 4,859 | **50%** |
| `limit_truncated` — over `-rss-limit`, was sorted by date | 2,235 | **23%** |
| `dedup_collapsed` | 1,160 | 12% |
| `matched` — actually persisted | 1,440 | 15% |

472 RSS calls to fetch 9,694 articles, of which 85% are discarded by our own logic before the
Reader sees anything. Then the read budget takes the top 4 per entity, so roughly **6–8% of what
Google ranked for us reaches the stage that was designed to judge it.**

Fifteen football teams were admitted **zero** articles: *Nantes, Huesca, Spezia, Real Valladolid,
Nice, Darmstadt 98, LOSC Lille, Amiens SC, Leganés, VfL Bochum*. Those are precisely the
short/ambiguous names the regex guard exists to protect, and it is starving them instead.

---

## Phase 1 — the ingest cut ranks by Google ✅ DONE

`8f2a1c` (deployed 22:52). `-rss-limit` cut by publish date; it now cuts by `feed_rank`.
`sortArticlesByDate` is deleted. The read budget downstream already ranked by `feed_rank`, so
"top N" now means one thing end to end instead of two disagreeing things.

---

## How big is "page 1"? — measured 2026-07-26

RSS has no pagination: one request returns one payload, and Google caps it at **100 items**.
Sampled live with `when:1d`:

| entity | items | | entity | items |
|---|---|---|---|---|
| Arsenal / Man Utd / Bayern / Lakers | 100 (capped) | | Leganés | 25 |
| Chiefs | 94 | | Real Valladolid | 4 |
| Celtics | 91 | | Darmstadt 98 | 4 |
| Packers | 82 | | Huesca | 3 |
| Jaguars | 64 | | Spezia | 3 |

Mean ≈ 52 over the sample. **The count is itself a relevance signal** — Arsenal returns 100 because
there are 100 stories today; Spezia returns 3 because there are three. Taking whatever page 1 gives
makes coverage scale with how much story actually exists, which is the product's stated lane.

A flat `-rss-limit 12` does the opposite of what it looks like: it never binds on the small clubs
(3 < 12, all kept) and **only ever bites the entities with the most news** — Arsenal keeps 12 of
100. The cap exclusively starves the biggest stories.

### What "no limit" actually costs

Mostly nothing, with one hard exception.

**Free:** GPU work does not scale with corpus size. `article_read` is capped at
`COGNITION_ARTICLE_READ_TOP_K` per entity. `narratives`/`vibe`/`sigil` enqueue **per entity**,
deduped by `input_version`, so one item per entity regardless of how many articles sit behind it —
the re-run count tracks the number of *readings*, which the read budget already bounds. `scrub` is
model-free. And one query per entity is **fewer** RSS calls than today's 472, not more.

**The exception, and it is binding:** the Journalist loads its article corpus with **no `LIMIT`**
(`journalist/mod.rs:285`) — every vetted, non-duplicate article in the window, `full_text` included.
The six voices share one 16,384-token context. At ~7 articles/entity today that fits; at ~52 it
will not. **Removing the ingest cap without capping the corpus load moves the overflow from ingest
to the prompt, where it fails as truncation instead of as a counter.**

That same query also does `ORDER BY COALESCE(a.published_at, a.fetched_at) DESC` — recency again,
the third place it outranks Google. It should order by `feed_rank`.

### The resulting shape

Take page 1 whole, and move the throttles to the two places that actually spend GPU:

1. **read budget** — top-K by `feed_rank` (exists, `COGNITION_ARTICLE_READ_TOP_K`)
2. **Journalist corpus load** — needs a `LIMIT`, ordered by `feed_rank` (does not exist yet)

The corpus becomes expansive and cheap — Postgres rows — and every cut that costs a model call is
ranked by Google. One ranking, two budgets, one judge. That is a smaller idea than what is there
now, not a larger one.

---

## Phase 2 — retire the regex tier, and let the query be the link

**The load-bearing question:** delete `MatchesEntity` and what links an article to an entity?

Today: query Google for "Arsenal" → `MatchesEntity` re-checks that "Arsenal" appears in
title+description → write `news_article_entities`. The regex is re-deriving something the query
already established. We asked Google for Arsenal news; the link is the query.

**The change.** Link optimistically on the query that returned the article, and let the Reader
confirm or reject. The Reader already emits relevance verdicts, `page_kind`, roles and co-mention
findings — it is a strictly better judge of "is this actually about Nice the football club or Nice
the city" than a regex over a headline, because it reads the body.

**Why this does not blow up GPU load** — the important part:

| | today | after |
|---|---|---|
| RSS calls | 472 | ~204 (one query per entity) |
| articles fetched | 9,694 | ~2,448 (204 × top 12) |
| articles persisted | 1,440 | ~2,000 after dedup |
| **articles read** | **top 4/entity** | **top 4/entity — unchanged** |

The read budget is what costs GPU, and it does not move. We fetch a quarter as much, discard
almost none of it, and hand the Reader *better-ranked* candidates for the same model spend. The
simplification is roughly load-neutral; it is the quality of what reaches the Reader that improves.

**What is genuinely lost:** the regex is a cheap pre-filter that keeps obvious junk out of the
corpus for free. Dropping it means some non-football "Nice" articles get persisted and occupy a
read slot before being rejected. That is the trade — a small amount of wasted reading in exchange
for deleting a tier that currently rejects half of everything and starves fifteen teams outright.

**Also retire here:** the edition-grid scaffolding. `defaultRSSEditions` and
`footballTeamRSSEditions` are one entry each; `EditionsPlanned/Queried/Skipped`,
`runRSSQueryPastLimit`'s `editionIdx` arithmetic and `teams_edition_capped` are all machinery for
a grid that no longer exists. Every log line reads `editions_skipped=0` because it structurally
cannot read anything else.

**Keep:** the `Funnel`. It is the reason this plan is evidence-based rather than a hunch, and its
`Residual()` invariant is what would catch a silent drop introduced by this very refactor. Keep
dedup. Keep the `newsLookbackSlack` boundary overlap.

---

## Phase 3 — delete the embedder

Already scoped in the code. From `harness.rs`:

> SHRINKING: the relevance gate and the novelty gate no longer embed anything (teardown §2.1/§2.2).
> The last two consumers are `narratives`' pre-model corpus clustering and `threads`' centroid
> cosine — both retire in Phase 3, when The Journalist declares thread identity itself and the
> embedder can be deleted outright.

So "no more BGE" is not a new direction — it is the finish of a teardown already two-thirds done.
Two consumers remain, and both are replaced by the same move: **The Journalist declares thread
identity in-prompt instead of having it computed from cosine distance.**

This is the largest piece and the only one that is really a prompt change, so it belongs with the
prompt session rather than here. Sequence it after Phase 2 — fewer, better-ranked articles make
thread identity an easier judgment, not a harder one.

**Unlock worth knowing:** the localized football editions (es/it/fr/de) are parked partly *because*
the embedder is `bge-small-en-v1.5`, English-only, and was scoring a 76%-non-English corpus on text
its weights never saw. Deleting BGE removes one of the three blockers. The other two — the Reader
translating and summarizing in one call, and English-only prompts downstream — remain, so this
unlocks the option, not the feature.

---

## Sequencing, and why this order

1. **Phase 1** ✅ — independent, few lines, strictly closer to the goal.
2. **Phase 2** — do while the backlog is throttled and the 02:00 daily sweep is the only inflow.
   Changing what lands in the corpus is much easier to read against one sweep a day than against
   a continuous drip.
3. **Phase 3** — after the prompt session, since it *is* a prompt change.

Phase 2 should land on a day when its first sweep can be watched: the funnel numbers will move a
lot, and `Residual()` going non-zero is the alarm that the refactor dropped something silently.

---

## Open questions for Scott

- **Read budget after Phase 2.** `COGNITION_ARTICLE_READ_TOP_K` is 4 and has never been overridden.
  Once the Reader is the only relevance judge, is 4 still right, or does the "clean, expansive base"
  want 6–8? This is the one dial that directly buys GPU load, so it is a deliberate call rather than
  a default — and it is the thing the 21:00 gauge should inform.
- **Zero-admit teams.** Fifteen teams currently get nothing. Phase 2 should fix them as a side
  effect; worth verifying explicitly rather than assuming, since they are the hardest names.
