# Handoff — Phase A is done; the newsroom is next

Written 2026-07-28. Supersedes [`HANDOFF-editor-turn.md`](HANDOFF-editor-turn.md), which is still
accurate on the relevance collapse but whose "three decisions, open" are all now settled.

**Read [`PLAN-ingest-simplification.md`](PLAN-ingest-simplification.md) first.** It is a checklist,
it is current, and it is the governing document. This file only tells you where the work stopped and
what to be careful of.

---

## 1. Start here

**Phase A is complete, merged to `main`, and DEPLOYED.** Released at commit `cec766a` on
2026-07-28 12:17 EDT. `cargo test --lib` 284, clippy 12 — both at baseline.

| commit | what |
|---|---|
| `fc602f9` | The Reader → The Editor (Tier 1: junction, not stage) |
| `1da2e08` | The plan becomes a checklist |
| `af441be` | **A3** — exact-title dedup sweep |
| `fff0c27` + `c8e90fc` | **F4** — probed, then reframed when third-party ingestion was retired |
| `e953609` | **A4** — routing tags replace the single-valued bucket |
| `676258a` | **A5** — the corpus cap, on the path that actually runs |
| `4fce117` | the previous handoff |
| `cec766a` | schema snapshot (195/196/197) — the deployed commit |

Migrations 195, 196, 197 applied; `snapshot-schema.sh` run and committed. Both services active on
`cec766a`. Next unticked item is **B1** (the name resolver), and the cheapest way in is **B4** — the
re-mapping backfill over the 6,319 held articles, zero model calls, exercising the resolver offline
before it touches the live rail.

### Deploy verification — what is confirmed and what is NOT

| | |
|---|---|
| daemon boots on the new binary | ✅ Postgres, both Ollama hosts, 9 stages registered |
| **A4** routing tags being written | ✅ 36 articles inside 5 minutes, six tag sets matching the taxonomy exactly (`fixture`, `performance`, `transfer`, `general`, `injury`, `roster`) |
| **A4** trigger inert | ✅ subscription table empty; nothing enqueued |
| **A3** hourly sweep | ✅ runs, found 0 — expected, the backfill already cleared the 72h window |
| **A5** prompt reduction | ⚠️ **NOT CONFIRMED** — see below |

**A5 is the one still owed a verdict.** Baseline before deploy, 24h of narratives: **p50 1,850 /
p99 7,401 / max 8,374** tokens. First two generations after: p50 578, max 1,510. That looks like a
large win and **n = 2 cannot support the claim** — those two entities may simply have small corpora.
Re-run the same query over a few hundred generations before believing it:

```sql
SELECT percentile_disc(0.5) WITHIN GROUP (ORDER BY length(built_prompt)/4) AS p50,
       percentile_disc(0.99) WITHIN GROUP (ORDER BY length(built_prompt)/4) AS p99,
       max(length(built_prompt)/4) AS max, count(*) AS n
FROM cognition_ledger WHERE stage='narratives' AND built_prompt IS NOT NULL
  AND generated_at > '2026-07-28 12:17:00-04';
```

Also still unobserved: **`budget_truncated` in `cognition_ledger.excluded_evidence`** (0 so far, and
it should appear for the 126 over-cap teams). If it never appears while prompts shrink, the
exclusion accounting is silently dropping articles — which is the exact failure A5 was written to
avoid.

**Expect a ~127-entity narratives regen wave.** 126 of 204 teams (62%) were over the 40 cap, plus
one player. Their corpus shrank, so `input_hash` moved and each regenerates once. Bounded,
one-time, self-limiting — and each subsequent generation is cheaper than the one it replaced.

### One operational note from the deploy

The `.path` rebuild watchers restarted the cognition daemon the moment the new binary was placed —
**including through a scheduled pause window** (the pause fired 12:00, resume was armed for 13:00).
So a deploy overrides the rest schedule as a side effect, and `--build-only` does NOT avoid it,
because the watcher fires on binary placement rather than on the restart step. Worth knowing before
deploying into a rest window; Scott was told and left it running.

---

## 2. What Phase A actually changed

**A3 — dedup (`mig 196`, backfill applied).** 3,618 articles marked, corpus 32,006 → 30,291
(−5.4%). The root cause was a *race*, not a missing mechanism: `novelty::gate` compares an article
against canonical coverage of its own vetted entities, so two copies scrubbed in the same pass —
before either has membership — are invisible to each other. A per-article gate cannot close that.

**A4 — routing tags (`mig 197`, applied, INERT).** `news_articles.routing_tags text[]` plus
`stage_routing_subscriptions` plus a fan-out trigger over newly-added tags. This is the item that
unblocks the whole newsroom: `bucket` held one label, so a story could only ever reach one voice.

**A5 — the corpus cap.** `load_vetted_corpus` already had the fix; the production path
(`load_vetted_corpus_with_exclusions`) never did. FC Barcelona was feeding **166 articles into an
unbounded prompt**; it is now 40, and the kept set averages `feed_rank` 9.1 against 36.0 for the
newest-40 it was taking before.

---

## 3. Traps this session added

**A capped eval path can hide an uncapped production path.** A5's fix existed for weeks on
`load_vetted_corpus` — which only `eval_tasks` calls. The narratives handler calls a *sibling*
function whose doc comment claimed to be "`load_vetted_corpus` plus the exclusions diagnostic" and
was neither capped nor rank-ordered. **Evals ran the fixed loader, production ran the broken one.**
This is T5 with a new face: a green gate cannot see a divergence it does not traverse. When two
functions claim to be the same query, diff the SQL, not the doc comment.

**Group-level guards are not pair-level guards.** A3's first draft tested
`count(DISTINCT source) > 1` per *group*, and the dry run caught it collapsing BeSoccer→BeSoccer: a
group of {A, A, B} passes a group-level test and then suppresses A with its own sibling. **Always
dry-run a data mutation inside a transaction you roll back**, and inspect rows, not just counts.

**A test on the wrong fixture measures zero and looks like a pass.** A4's first fan-out tests all
returned 0 and I nearly reported the trigger broken. The article I hardcoded had just been collapsed
by A3 and had no vetted team links, so the trigger correctly did nothing. **A zero result needs its
own explanation** before it is believed in either direction.

**Check availability before declaring a blocker.** F4 was recorded as "BLOCKED, no data" from a
schema check alone — which proved we do not STORE injuries, not that we could not GET them. The
probe found BallDontLie serving injury data on the live key.

---

## 4. State, and the things that will bite

**Migrations applied to Archbox: 195, 196, 197**, and `snapshot-schema.sh` has been run and
committed (`cec766a`). Note the ledger gained **12** versions in that snapshot, not 3 — `sql/schema/`
had drifted well behind live before this session, so it was not a reliable picture of prod for a
while. The CI schema job and the restore drill both diff against it.

**Mig 194 is applied but unrecorded** in `schema_migrations` (verified: column, index and comment all
present). Fully idempotent, so the next `migrate.sh` re-applies it as a no-op and records it. Self-
healing; do not hand-fix it.

**A4 ships inert on purpose. Do not seed `('transfer','transfers')` casually.** Mig 175 still routes
transfers off `bucket`. Seeding that pair would double-enqueue with a *different* `input_version` —
not a duplicate (ON CONFLICT covers that) but a **churn loop**, where the two fingerprints alternate
and reopen the item forever, on the slowest stage in the pipeline. Phase E migrates it deliberately,
and should retire the mig-175 trigger in the same change.

**`ARTICLE_READ_PROMPT_VERSION` is still `"ar6"` and that is deliberate.** It is a cache key: every
reading whose `prompt_version` differs is invalidated and re-read lazily. It gets bumped by C4, when
a real contract change earns the re-read wave.

**Do not re-arm the 6,319 held articles.** They need re-MAPPING, not re-judging — checklist item B4.

**`player_team_history` goes quiet.** It is written by `detect_team_change`, which ran off provider
roster sync, and third-party ingestion is retired. E4 does not need it —
`transfer_identity_applications` is news-derived (`'source': 'mistral_adjudication'`) and is the
richer record — but anything else keying on its freshness will now stall silently rather than error.

**Expect the busyness verdicts to move** (plan T7). A3 and A5 both shrink what the Journalist sees,
and n16 baselines are not comparable across them.

---

## 5. The design, in one paragraph

The ingestion layer is a **candidate generator**; the query is a hypothesis, never a claim. Go
proposes entities (lenient, ranked, judging nothing); **The Editor** gates on "is this sport
reporting?", discovers who is actually in the article, and says what the story is and how it reads.
Its output is a **packet** — one storyline, multi-tagged, entity-indexed — and each voice reads the
same packet through its own lens: the Journalist, the Influencer and the Insider off tags, the Scout
off confirmed facts only. Three peers reading one packet independently is what lets them **disagree**,
and the disagreement is the product. The Analyst is the only peer-aware seat; the Oracle stays blind,
reading five cards.

The measured case for it, in one line: Real Madrid's heaviest day was **110 candidate articles that
hand-count to about five stories** — and 18 of them were about Atlético.

---

## 6. Two things worth doing early

**The Oracle's disagreement machinery is built and silent** — 7 of 13,252 readings mention
disagreement; `WHY_NOW` has *never* fired, while the deterministic `pillar_convergence` sees
divergence down to 1. Two independent causes (plan E3/E5), and fixing one leaves it silent. This is
the cheapest large win on the board and it is not in Phase A.

**The Editor is never asked who is in the article.** It is handed `vetted_names` + `co_mentions` and
asked what part each plays. Measured: it read 99 articles mentioning Vinicius Junior and linked him
in 24 — and the miss rate on articles it *read* is barely better than on ones it never touched. That
is a contract gap, not a capacity gap, and C1 closes it.
