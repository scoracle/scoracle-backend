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

## Phase 4 — SEED, parked: one edition per league, in that league's language

Scott's idea, 2026-07-26. Route each football team's RSS query to the edition of the country its
league is in — Bundesliga clubs to `de-DE`, Premier League to `en-GB`, La Liga to `es-ES`, Serie A
to `it-IT`, Ligue 1 to `fr-FR`.

**Why it is a strong idea.** Local sports press is where football is actually covered. *Bild* on
Bayern, *Marca* on Real Madrid, *Gazzetta* on Serie A — none of it reaches an `en-GB` query except
second-hand. The current code names this cost itself: *"four of our five leagues are now covered
only by English-language reporting of them,"* logged as a deliberate accuracy-over-breadth trade
while the pipeline is GPU-bound.

**How it differs from what was already tried, which matters.** Localized editions were live
2026-07-23 and retired. But that version ran **every** edition for **every** football team — 6–7×
the RSS calls, and a per-team corpus that was a language mix (24.1% English overall, 45.3%
definitively non-English). Scott's version is **one edition per team, the correct one**. Same call
count as today, no volume increase at all, and each team's corpus is coherently in one language
instead of scrambled across seven. The cost objection that retired the first attempt does not apply
to this shape.

**Data is ready.** `teams.country` is clean for football: England 25, France 24, Spain 24, Italy 23,
Germany 22, Monaco 1 (Ligue 1 → `fr-FR`). 23 teams have a NULL country and need a backfill or an
`en-GB` fallback. The hook already exists — `rssEditionsForEntity(entityType, sport)` just needs the
team's country threaded to it.

**What still blocks it.** Three reasons were recorded for parking; this plan clears one of them:

| blocker | status |
|---|---|
| BGE is `bge-small-en-v1.5`, English-only — scored a 76%-non-English corpus on text its weights never saw | **cleared by Phase 3**, which deletes the embedder outright |
| The Reader translating *and* summarizing in one call | open — but the Reader runs on `gemma3:4b`, which is multilingual. **Worth measuring before assuming**, it may already be a non-issue |
| Every prompt, guard list and stopword downstream is English | open, and the real work |

So the sequencing is: Phase 3 first (it removes the hardest blocker as a side effect), then measure
whether gemma3 reads the target languages well enough, then this. Do not start it before Phase 3 —
the embedder would silently score the new corpus on weights that never saw those languages, which is
exactly the failure that retired the first attempt.

---

## Sequencing — the prompt session is the LAST phase

Scott's call, 2026-07-26. Everything mechanical lands first; the LLM prompt session closes the plan
out. Nothing that needs a prompt decision gets attempted before it, and nothing mechanical is left
waiting on it.

1. ✅ **Phase 1** — the ingest cut ranks by `feed_rank`.
2. ✅ **Phase 2** — regex tier retired, players stop auto-vetting, `-rss-limit` = one Google page.
3. **Plumbing** — the open frictions from `HANDOFF-plumbing.md` (stale-lease recovery, the handler
   timeout that measures queueing, the unverified Oracle barrier, the two dead letters), retiring
   the vestigial edition-grid scaffolding, and offsetting the two cards' rest windows
   (see `PLAN-backlog-churn.md` — sequenced after the backlogs clear, since it buys pipeline
   continuity rather than throughput). All independent of any prompt.
4. **LLM prompt session — FINAL PHASE.** In order:
   1. **Bucketing** — first, and the critical one. Group trending summaries into one generation
      covering N entities instead of N separate ones. The only lever that reduces the *number* of
      items rather than the time each takes, and it spends gemma3's idle capacity.

      **Do the sorting in the Reader, not the Journalist** (Scott, 2026-07-27): *"It's not the brain
      work, it's the sorting work."* Sorting belongs on the 1070, which has headroom; synthesis
      belongs on the Mac, which does not. The Journalist should consume buckets, not build them.

      **This is mostly already built, which is the surprise.** The Reader's output schema already
      emits `story_type` per article — and it discriminates well. Measured 2026-07-27 over 2,809
      successful readings:

      | transfer | fixture | general | performance | injury | roster | contract |
      |---|---|---|---|---|---|---|
      | 1,005 | 581 | 424 | 293 | 277 | 69 | 63 |

      So the sorting key **already exists, on the right machine, at no additional model cost**. Two
      consequences:

      - **`news_articles.bucket` is being derived twice.** `bucket.rs` records that the
        transfer/non-transfer judgment was *moved to* the Journalist ("the Journalist labels each
        article it reads, as the tail of the read", step n9). But the Reader already knows —
        `story_type='transfer'` is 1,005 rows — and it knows from the **full body**, where the
        Journalist only sees a 900-byte evidence blurb. Writing the bucket from the Reader and
        deleting n9 removes work from the saturated host, shortens the narratives prompt AND its
        output schema, and uses the better-informed judge. `ArticleBucket::from_model_tag` already
        maps `trade`/`trades` → `Transfer`, so the off-vocabulary values the model emits are handled.
      - **Grouping the Journalist's corpus by `story_type` needs no new inference at all** — it is a
        prompt restructuring over a field already persisted in `news_article_readings.evidence`.
        That is the cheapest possible version of "the Journalist consumes buckets".

      One trap, recorded so it is not rediscovered: a comment in `eval_tasks.rs` cites `story_type →
      general on 84% of reads`. That is **ar5 history, not current state** — it is 15% now. Reading
      it as current would wrongly rule out the whole approach.

      **Second bucket: related stories** (Scott, 2026-07-27). Group articles covering the same event
      so the Journalist receives one storyline instead of six retellings of it — lean, concise,
      still thorough. This is the highest-value half of the idea, because a corpus of 40 articles is
      mostly the same handful of stories told repeatedly, and the Journalist currently pays full
      prompt cost for every retelling.

      **It is also the replacement Phase 3 needs.** Grouping-by-similarity is exactly what `threads`
      does today — `cosine(title+body embedding, thread centroid) >= 0.80` — and that is one of the
      two remaining BGE consumers. Phase 3's plan was for the Journalist to declare thread identity
      itself; doing it in the Reader is better on every axis: it is sorting rather than synthesis,
      it runs on the card with headroom, and the Reader sees the **full body** where the Journalist
      sees a 900-byte blurb.

      **The mechanism already exists in the Reader.** Do NOT have it emit a free-text storyline name
      — "Saka injury" and "Bukayo Saka hamstring" will not match, which is the whole problem
      embeddings were solving. Use the house closed-candidate-list pattern instead (the `resolve.rs`
      trick, already used by graph): show the Reader the entity's currently-open storylines as a
      NUMBERED list, and have it either attach to one by number or declare a new one. The Reader
      already takes numbered co-mention candidates and returns picks by number — same shape, same
      parser discipline, no free-text resolution anywhere.

      Grouping then becomes a `GROUP BY` rather than a clustering pass, and BGE has no consumer left
      in this path.
   2. **Context size** — goes hand in hand with bucketing, since bucketing is what makes the context
      budget bind. Owns `COGNITION_JOURNALIST_CORPUS_LIMIT` (currently 40) and the six voices'
      shared 16,384-token window.
   3. **Delete BGE** — The Journalist declares thread identity in-prompt, retiring the last two
      embedder consumers. Finishes a teardown already two-thirds done.
5. **After the plan: per-league editions** (the Phase 4 seed above). It depends on BGE being gone,
   and BGE goes in the final phase — so this is the first thing of the *next* epoch, not this one.

Phase 2 landed 2026-07-26 23:14. Its first sweep is 02:00 the next morning: the funnel numbers will
move a lot, and **`Residual()` going non-zero is the alarm** that the refactor dropped something
silently. Everything else moving hard is expected.

---

## Decisions taken

- **Read budget → 10** (was 4, never overridden). Set in `.env.local` 2026-07-26 23:28. The Reader
  is now the only relevance judge, and a budget of 4 was sized for a pipeline that discarded 85%
  before reaching it. The capacity is real and idle: `article_read`/`graph`/`scrub` run on Archbox's
  gemma3 card, which held **zero** pending work all night while the Mac's single permit carried
  ~930 items. **Flagged for the 21:00 checkpoint** — the second-order cost lands on the Mac, since
  `narratives`' `input_version` hashes each article's read status and every new reading can reopen
  that entity's Mac-routed narratives row. Drop toward 8 if narratives inflow climbs with it.
- **Zero-admit teams.** Fifteen clubs were admitted nothing (Nice, Spezia, Leganés, Huesca,
  Amiens …). Phase 2 should fix them as a side effect; verify explicitly at the 02:00 sweep rather
  than assuming, since they are the hardest names and the reason to believe is a deletion.
