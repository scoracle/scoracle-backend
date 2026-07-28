# Handoff — Phase A is done and PROVEN; B1's groundwork is in; `bin/remap.rs` is next

Written 2026-07-28, updated 18:10 EDT. Supersedes [`HANDOFF-editor-turn.md`](HANDOFF-editor-turn.md),
which is still accurate on the relevance collapse but whose "three decisions, open" are all now
settled.

**Read [`PLAN-ingest-simplification.md`](PLAN-ingest-simplification.md) first.** It is a checklist,
it is current, and it is the governing document. This file only tells you where the work stopped and
what to be careful of.

---

## 0. The one thing to do first

**Write `rust/src/bin/remap.rs`** — the B4 backfill. Everything it needs exists, is applied, and is
verified. Read **T10 before you write a line of it**: the half of B4 that looks free is not.

Decided, so you do not need to re-litigate: staged (flips first, inspect, then brand-new links);
brand-new rows carry a `match_confidence` sentinel distinct from Go's 0.95; dry-run default;
follow `bin/bucketlabel.rs`'s read-only posture.

---

## 1. Start here

**Phase A is complete, merged to `main`, DEPLOYED, and — as of this session — VERIFIED.** Released
at commit `cec766a` on 2026-07-28 12:17 EDT. `cargo test --lib` 284, clippy 12 — both at baseline.

**A5 is proven. The question two handoffs carried forward is closed.** See §1.1.

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
| `baaeb9a` | **B1 groundwork** — migs 198/199, trigram kept off the write path, T9/T10/F7 |

Migrations 195–199 applied; `snapshot-schema.sh` run and committed. Both services active on
`cec766a` — **`baaeb9a` is schema + docs only, no binary change, so nothing was redeployed.**

---

## 1.2 What this session added (2026-07-28 afternoon)

**B1's groundwork is applied and committed.** Mig `198_entity_name_resolution` — `nrm()` (the one
normalizer, in SQL), `entity_name_surfaces` (16,690 rows), an exact-lookup btree, a GIN trigram
index, and a hand-verified 15-alias seed. Mig `199_refresh_surfaces_analyze` — the rebuild now
ANALYZEs, because it left empty-table statistics and the same lookup planned Seq Scan 38.2ms vs
Bitmap Index Scan 2.3ms.

**The plan's B1 design was revised by measurement — see T9.** "Resolve with pg_trgm" is unsafe as
an automatic write path. Exact match on the normalized surface is the only automatic path; trigram
ranks and reviews.

**Mig 194 self-healed** exactly as T8 predicted — `migrate.sh` re-applied it as a no-op and recorded
it. T8 can be struck.

**The B4 cohort is now pinned.** `vetted IS FALSE` alone is **24,984 articles** today — normal scrub
rejections have accumulated since 07-27 and those are legitimate. The incident cohort is articles
whose Editor reading was updated between 07-27 00:00 and 07-28 07:04: **6,377 articles**, which
reproduces the recorded 6,319 to within drift. **A backfill written against the bare predicate would
touch 4× the intended set.**

Live yield, exact match, ambiguity refused (123 of 15,948 raw hits, 0.77%):

| class | team | player | total | articles |
|---|---|---|---|---|
| flip FALSE → TRUE | 7,727 | 2,105 | **9,832** | 5,278 |
| brand-new | 1,583 | 4,367 | **5,950** | 1,694 |

5,477 of 6,377 articles resolve at least one entity. Zero model calls.

### 1.1 Deploy verification — ALL THREE ITEMS NOW CLOSED

| | |
|---|---|
| daemon boots on the new binary | ✅ Postgres, both Ollama hosts, 9 stages registered |
| **A4** routing tags being written | ✅ 36 articles inside 5 minutes, six tag sets matching the taxonomy exactly |
| **A4** trigger inert | ✅ subscription table empty; nothing enqueued |
| **A3** hourly sweep | ✅ runs, found 0 — expected, the backfill already cleared the 72h window |
| **A5** prompt reduction | ✅ **CONFIRMED** n=128 — p99 7,401 → 3,097, max 8,374 → 3,470 |
| **A5** `budget_truncated` accounting | ✅ **CONFIRMED** — 5 bands, all reconciling exactly |
| regen wave draining | ✅ 153 items enqueued post-deploy, 114 over-cap; drains ~10 over-cap / 4h |

**How A5 was closed, because the method matters more than the number.** The trap both prior attempts
fell into was measuring the wrong population: the narratives queue is FIFO on `available_at`, the
over-cap entities were enqueued *after* the deploy, and so the first ~110 generations were all
small-corpus entities that never had large prompts. n=2 and n=48 were equally worthless for the same
reason. **Always check whether your sample contains the population the change acts on.**

*Mechanism, exact* (ledger 91976, team 79, 51 in-window): ranked all 51 by the production ordering
(`feed_rank ASC NULLS LAST, published_at DESC, id`) and compared the tail beyond rank 40 against the
recorded drop set — **11 of 11 predicted, 0 false drops, 0 false keeps.** All five capped
generations reconcile: `dropped_count` == `length(dropped_news_ids)` (11/11, 6/6, 17/17, 5/5, 7/7).

*Distribution:*

| | p50 | p90 | p99 | max | n |
|---|---|---|---|---|---|
| baseline (24h pre-deploy) | 1,850 | 4,886 | 7,401 | 8,374 | 228 |
| post-deploy | **642** | **1,983** | **3,097** | **3,470** | 128 |

Capped prompts cluster at 2,746–3,470. The sample is **enriched** for over-cap entities (47% of the
queue vs 26% of entities carrying corpus), so the reduction is conservative, not flattering.

~113 over-cap entities were still queued at deploy+6h. Self-limiting; each generation is cheaper
than the one it replaces. Nothing to watch.

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

## 3. Traps — read these before writing `bin/remap.rs`

**⚠️ T10 — flipping `vetted` FALSE → TRUE RE-ARMS the article.** The single most dangerous thing in
this handoff, because the code looks safe. `enqueue_derive_on_vetted` fires `AFTER UPDATE OF vetted`
and enqueues **`article_read` on the article**, with `input_version = 'ar:' || vetted_count` — so
flipping links changes the count and the `ON CONFLICT` clause re-opens even articles already read.
Verified in a rolled-back transaction: one flip, one new `article_read` row.

B4's "free" half — restoring 9,832 links that already exist — would therefore buy **~5,278 Editor
re-reads through the ar6 gate that C1 is about to replace.** That is exactly what *"do not re-arm
the 6,319"* forbids, arriving through a side door. The instruction and the mechanism disagreed.

Fix, verified: `SET LOCAL session_replication_role = 'replica'` inside the transaction. No lock,
auto-reverts at commit. (`ALTER TABLE ... DISABLE TRIGGER` takes an ACCESS EXCLUSIVE lock against a
live pipeline — do not.) The links then go visible to the Journalist immediately, carrying the
reading they already have. **Note the shape: the trigger is article-keyed, so the blast radius of a
vetted write is measured in re-reads, not entity derivations.**

**T9 — a trigram margin gate protects against the wrong failure.** The intuition is that fuzzy
matching fails on ties, so requiring a margin over the runner-up makes it safe. Measured over the
120 most frequent unresolved names, it does not — the dominant error is a *confident single* match
to an entity that is simply not the one named, and those have no runner-up at all:

| model's name | best match | sim | margin | |
|---|---|---|---|---|
| `spain` | team 394 `spa` | 0.429 | 0.429 | wrong |
| `pep guardiola` | player `sergi guardiola` | 0.500 | 0.500 | wrong |
| `sheffield wednesday` | team 21 `sheffield utd` | 0.417 | 0.417 | wrong — a rival |
| `lee child` | team 71 `lee` | 0.400 | 0.400 | wrong — a novelist |
| `vinicius jr` | player 600687 | 0.556 | 0.082 | **correct** |

Every wrong row clears the gate more comfortably than the one correct row. What a margin gate *does*
catch is the true tie: `inter milan` scores 0.500 against **both** Inter and AC Milan.

**Ambiguity is refused, not broken.** 123 of 15,948 (0.77%), all same-sport player namesakes. Roster
context (`team_rosters` ∩ the article's teams) would resolve 46 of 59 exact ties for free — **but
not Vinicius Jr**, where Vinicius Junior and Vinícius Tobias share Real Madrid and the rule ties.
That residual is what the mig-198 aliases and the F7 discovery seat are for.

**`full_text` is NULL for all 150,566 articles** and nothing writes it (`journalist/prompt.rs:158`
already records this). So B1's gate (a) — "the name must appear in the body we already have" — is a
**live-path gate only**. Applied offline against what *is* retained, only 76.9% of correct
resolutions pass, and the failures are summarization, not hallucination.

---

## 3.1 Traps from the morning session

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

**Migrations applied to Archbox: 195, 196, 197, 198, 199** — plus **194**, which self-healed. The
ledger is at 201 versions and `snapshot-schema.sh` has been run and committed (`baaeb9a`). Note the
ledger gained **12** versions in the `cec766a` snapshot, not 3 — `sql/schema/` had drifted well
behind live before that session, so it was not a reliable picture of prod for a while. The CI schema
job and the restore drill both diff against it. **It is accurate now; keep it that way — run
`snapshot-schema.sh` and commit it with the migration, every time.**

**~~Mig 194 is applied but unrecorded~~ — RESOLVED.** `migrate.sh` re-applied it as a no-op and
recorded it during this session's run, exactly as T8 predicted. No action needed; T8 can be struck.

**A4 ships inert on purpose. Do not seed `('transfer','transfers')` casually.** Mig 175 still routes
transfers off `bucket`. Seeding that pair would double-enqueue with a *different* `input_version` —
not a duplicate (ON CONFLICT covers that) but a **churn loop**, where the two fingerprints alternate
and reopen the item forever, on the slowest stage in the pipeline. Phase E migrates it deliberately,
and should retire the mig-175 trigger in the same change.

**`ARTICLE_READ_PROMPT_VERSION` is still `"ar6"` and that is deliberate.** It is a cache key: every
reading whose `prompt_version` differs is invalidated and re-read lazily. It gets bumped by C4, when
a real contract change earns the re-read wave.

**Do not re-arm the held articles.** They need re-MAPPING, not re-judging — checklist item B4. Two
things this session sharpened: the cohort is **6,377** articles pinned to the incident window, not
the 24,984 that `vetted IS FALSE` now matches (§1.2); and re-arming is not only something you might
choose to do, it is something the `vetted` trigger will do FOR you unless suppressed (**T10**).

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
