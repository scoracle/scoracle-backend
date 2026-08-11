# Handoff — the rail gets rebuilt

Written 2026-07-28, 22:40 EDT. **This file supersedes the DIRECTION of
[`PLAN-ingest-simplification.md`](PLAN-ingest-simplification.md)**, which is still accurate as a
record of what was measured and why, and is still the reference for the traps. Do not work the
checklist top-to-bottom any more — §7 says which items dissolved.

Also supersedes [`HANDOFF-editor-turn.md`](HANDOFF-editor-turn.md) and
[`HANDOFF-2026-07-28.md`](HANDOFF-2026-07-28.md).

---

## 0. The decision

**Greenfield the rail. Keep the substrate.** Scott's call, 2026-07-28, at the end of a session that
spent most of its time doing archaeology on the old one.

The next session starts a new plan built around this, plus **a new LLM junction Scott is designing**
(§6). This file exists so that session does not have to re-derive any of what follows.

---

## 1. The breakthrough — hold onto this sentence

> **Go only reads headlines. The Editor actually reads the body. So Go cannot decide anything — and
> it does not need to propose anything either, because the Google query IS the proposal.**

That is Scott's, and it is the thing that makes the rebuild simple rather than merely new.

An article came back from a search for Arsenal. That query is the hypothesis, already recorded, for
free. The regex tier was never *generating* candidates — it was *filtering* Google's, which is the
judgment we are taking away. So it is not demoted to clerk. It goes.

**The redundancy this exposes, which two sessions of planning missed:** the Editor already answers
"who is in this article" **twice**. Once as `co_mentions` — a numbered list of headline-derived
candidates it votes on — and once as `relevant_entities`, free names read out of the body. Two
mechanisms, one question, and the numbered one is built on exactly the headline evidence the
sentence above says not to trust. Only the free-name one should survive.

### The flow, whole

```
Google — one ranked query per entity      the query IS the candidate link
   |
   v
THE EDITOR reads the body
   is this sport reporting?               (gate)
   who is in it?                          (names -> resolver -> entities)
   what kind of story is it?              (transfer / injury / roster / ...)
   how does it feel?                      (register + the phrase that shows it)
   |
   v
A PACKET — one storyline, multi-tagged, entity-indexed
   |
   v
Journalist · Influencer · Insider  (tags)        Scout  (confirmed facts only)
```

**What that deletes outright:** the regex tier, `co_mentions` and its candidate plumbing, `scrub` as
a judge, BGE and the cosine `threads` clustering, `bucket` (already superseded by `routing_tags`),
and `match_confidence` as a decision input. `vetted` stops being a verdict two stages argue over and
becomes one fact: **the Editor linked it.**

---

## 2. The line — what is kept and what is replaced

This distinction is what makes the rebuild safe. Get it wrong in either direction and it fails.

| KEEP — this is not the problem | REPLACE — this is |
|---|---|
| Postgres and its data: 150,566 articles, 265,204 links, 204 teams, 15,986 players, the stats tables | everything between *article arrives* and *packet exists* |
| the harness: durable queue, model router, ledger, eval loop | ingest linking, the scrub verdict, the read handoff |
| the voice contracts — Journalist, Oracle, Insider, Influencer, Analyst, Scout | `news_article_entities.vetted` as a two-writer tri-state |
| the transfer adjudication chain (`transfer_identity_applications`) — news-derived, threshold-gated | `co_mentions`, `bucket`, BGE/`threads` |
| mig 198: `nrm()` + `entity_name_surfaces` (16,690 rows) — the resolver substrate is good | `MatchesEntity` as a filter |
| the eval fixtures and their annotations | the article_read → narratives handoff |

**The database is not greenfield and cannot be.** A greenfield *rail* is viable; a greenfield
*schema* would throw away the only part that isn't rotten.

---

## 3. Where the rot is — findings, not opinion

Every item here was found by measurement in the last two sessions. This is the case for the
rebuild, and it is also the list of things that will happen again in the new rail if it is built
the same way.

**A required field with no question attached.** `relevant_entities` — which `derive_relevance` turns
86% of its rejections on, and which B1 was about to wire to the entity resolver — appeared **exactly
once in a 4,900-character prompt**, inside the JSON template, with no definition anywhere. The model
was doing generic NER, which is the correct answer to the prompt it had. It returned `Paris` on a
Tour de France story, `Moulin Rouge`, `Séguéla–Diamba Sud`, and on a mining-stock page the invented
`Fortuna Düsseldorf` — with a blurb asserting *"Fortuna Mining Corp., a subsidiary of Fortuna
Düsseldorf."*

**A gate that could not run, for two days, and looked exactly like a gate that passed.** The A2
rename made the eval task `editor`; the fixture directory stayed `fixtures/reader/`. The harness
resolves `fixtures/<task>`. `--task editor --fixtures` had been erroring on a missing directory
since `fc602f9` — on the one junction whose coverage exists *because* it once ran as sole relevance
judge with none.

**A fixture pinning a rule the code deliberately abandoned.** `opponent-only-mention` expected
`relevant=false`; `derive_relevance` has KEPT opponent-only stories since ar6 (*"a match against us
is news about us"*). Wrong since that reversal, and invisible because of the item above.

**The same query, twice, with only one of them fixed.** A5: the corpus cap lived on
`load_vetted_corpus` for weeks while production called `load_vetted_corpus_with_exclusions`, whose
doc comment claimed to be the same query and was neither capped nor rank-ordered. **Evals ran the
fixed loader; production ran the broken one.**

**A backfill whose first stage destroyed its second stage's inventory.** `bin/remap`'s cohort is
"articles whose reading moved in the window AND that still carry a rejected link." Applying the
flips flipped 9,817 of those links, which removed 4,964 articles from the cohort — so `-pass new`
now finds **1,047 links instead of 5,938**, silently, and would have reported success. A staged
mutation whose cohort predicate reads state the earlier stage writes. Mine, found tonight, recorded
as **T13**.

**A trigger that re-arms articles from a write that looks inert.** T10: `enqueue_derive_on_vetted`
fires `AFTER UPDATE OF vetted` and enqueues `article_read` **on the article**, keyed on the vetted
COUNT — so restoring links reopens articles already read. The "free" half of B4 would have bought
~5,300 Editor re-reads through the very gate C1 was replacing.

**Lazy cache invalidation that has never once drained.** Four prompt-version bumps, and every old
population is still sitting there: ar1 65, ar2 14, ar3 4,178, ar5 386 readings — **zero touched in
24h**. The "re-read wave" C4 was written to fear has never happened. The real consequence is the
inverse: a new Editor contract reaches **new articles only**, and the existing corpus keeps its old
readings forever.

**A gate designed around a column that is empty.** `news_articles.full_text` is NULL for **all
150,566 articles** and nothing writes it. B1's gate (a) — "the name must appear in the body we
already have" — had no body to check against. Worse, when checked against what IS retained, both of
the failure classes above **pass it**: "Paris" is in the title, "Fortuna Düsseldorf" is in the blurb.

**Two routing mechanisms, both live.** Mig 175 routes transfers off `bucket`; A4 added
`routing_tags` + `stage_routing_subscriptions` to replace it. Seeding the obvious subscription would
start a **churn loop** — two fingerprints alternating, reopening the item forever, on the slowest
stage in the pipeline. A4 therefore ships INERT, which means the new mechanism is built, deployed,
and doing nothing while the old one runs.

**Schema drift as the default state.** The snapshot was **12 versions behind** live before
`cec766a`. The CI schema job and the restore drill both diff against it. It is accurate right now —
`snapshot-schema.sh` after every migration, committed with it.

**Silent staleness.** `player_team_history` is written by `detect_team_change`, driven by provider
roster sync, and third-party ingestion was cancelled 2026-07-28. Anything keying on its freshness
now stalls rather than errors.

---

## 4. The pain points — what it actually costs to work in here

- **Every session opens with archaeology.** Most of tonight was establishing what the code does
  versus what the docs claim. That is the tax the rebuild is meant to stop paying.
- **Docs and code disagree, and the code wins.** Mig 174's comment still describes the Journalist
  labelling articles; that stopped being true at n16. A doc comment claimed two functions were the
  same query (A5). **When two things claim to be the same, diff the SQL, not the prose.**
- **A green gate proves nothing, and a dead gate looks identical to a green one.** T5 said the first;
  tonight added the second.
- **Only measurement has ever settled anything here.** Every plausible theory that got tested got
  refuted: accent folding wasn't the player bleed; a trigram margin gate protects against the wrong
  failure (T9); "third-party injuries are blocked" was a schema check, not an availability check;
  ar4's reorder was measured neutral; ar5's grammar-enforced reject classes still said
  `relevant:true`. **Do not ship a theory here. Measure it.**
- **A sample that does not contain the population is worth nothing.** A5 stayed unproven across two
  sessions because the first 110 post-deploy generations were small-corpus entities the change could
  not act on. n=2 and n=48 were equally worthless.
- **One change, one measurement.** ar4 and ar5 were both confounded reorders.

---

## 5. State — what is live, what must NOT ship

| | state |
|---|---|
| **B4 flips** | **APPLIED to production.** 9,817 links (7,712 team / 2,105 player) across 5,374 articles, 2026-07-28 20:51 EDT. 0 articles re-armed. Good under either architecture. Reversal record: `planning_docs/data/remap_flips.tsv`. |
| **B4 brand-new links** | **DROPPED**, Scott's call. See below. |
| **ar7 + C2** | committed, **44/44 on the fixture gate, NOT DEPLOYED — and must not be.** |
| deployed commit | `cec766a`. Nothing since has been released. |
| migrations | 194–199 applied; schema snapshot current. |

**Why ar7/C2 must not deploy.** It is a new Editor contract for the rail we are deleting, and
deploying it spends a version bump on that rail. The *contract work* is not wasted — "gate, discover,
type, register" is the new Editor's opening specification, and the fixture set moves across with it.
It just must not ship into the thing being retired. Leave it committed and undeployed.

**Why the brand-new links were dropped.** 5,938 links whose only witness is the Editor's read of a
body **we never stored**. Of the 1,047 still visible, **8** have the matched name anywhere in the
retained title or description — which is near-definitional, because Go matches on exactly that text,
so if the name were there the link would already exist and be a *flip*. Sampling put roughly a
quarter of them wrong, against ~5% for the flips, and the flips had two independent witnesses. The
recovery worth having is already banked.

---

## 6. The open piece — the new junction

**Scott is designing a new LLM junction and will bring it to the next session.** The plan should be
built around it rather than have it inserted afterwards.

What the old plan had accumulated for a seat of roughly this shape (F7), which may or may not be
what Scott has in mind — offered as evidence, not as a specification:

| residue | size | why deterministic code cannot finish it |
|---|---|---|
| true namesake ties | 13 of 59 after roster context | Vinicius Junior and Vinícius Tobias share a club; the roster rule ties |
| people we do not model | ~60/day — `kyle shanahan`, `john lynch`, `andy reid` | coaches drive real stories and fuzzy-match to real *players* |
| clubs outside our leagues | `celtic`, `wrexham`, `ajax`, `galatasaray` | the DB boundary is a business decision; the article does not respect it |
| national teams | `spain`, `france`, `portugal` | a whole entity CLASS with no table |
| genuine noise | `andy burnham`, `lee child`, `ice` | needs judgment, not a threshold |

**The one design constraint that is not negotiable, whatever the seat turns out to be: T2.** A local
model will not render a verdict as a bare field. Proven three times — ar3 accepted 99.1%, ar5
labelled a boxscore `score_stub` and still said `relevant:true`, the Oracle's `DISAGREEMENT:` fired
7 times in 13,252. **Describe, then derive.** Ask what the text says; compute the judgment in code.

---

## 7. The failure mode to guard against, and it is not "the rewrite is hard"

**It is shipping without a cutover test.** Greenfield rails die by running in parallel forever
because nobody wrote down what has to be true to switch over.

So the new plan's first section — before any schema — should be:

1. **What a packet is.** The data contract, exactly. The packet is the product; stages are knobs.
2. **The single condition** under which the Journalist reads packets instead of
   `load_vetted_corpus_with_exclusions`.
3. **What happens to the old rail on that day.** Deleted, not left running.

---

## 8. What dissolves from the old checklist

More than half of `PLAN-ingest-simplification.md` was scar-tissue management, which is why it was
hard to follow. Under the new flow:

| item | fate |
|---|---|
| **B1** name resolver | not a bolt-on — it IS the Editor's write path |
| **B2** Go retires as judge | a deletion, not a refactor |
| **B3** unmatched-name capture | a column on the packet, not a new rail |
| **B4** | done (flips) + dropped (the rest) |
| **C3/C4/C5** field order, version bump, budget | artifacts of editing a live prompt with a cache to fear. Greenfield has neither |
| **D1–D6** packets | **the centre of the new plan, not phase four** |
| **E1/E2** routing + re-wake | falls out of the packet model for free |
| **F1** rename Tier 3 | free — name the new tables correctly the first time |
| **F2** delete BGE | don't build it |
| **C1/C2** the Editor's contract | **survives, and moves across.** It is the new Editor's spec |

Phase A is **done and deployed** and none of it is wasted: A3's dedup, A4's routing tags and A5's
corpus cap are all substrate-level.

---

## 9. Measurements worth carrying across

These cost real work and should not be re-derived.

**The collapse ratio.** Real Madrid, 2026-07-26: **110 candidate articles hand-count to about five
stories**, plus ~20 junk. 18 of the 110 were about Atlético. ~20:1 on the biggest cluster. This is
the number that justifies packets.

**Volume and coverage.** FOOTBALL 6,344 articles/day, NFL 1,148, NBA 648 — football is 78%, so
whatever gets built is a football system that also does NFL and NBA. **21.3%** of articles earn a
model read; the other 79% reach a voice on their headline. The Editor sustains ~7,400 readings/day
and is the throughput bottleneck. **Its output budget IS coverage.**

**The player-discovery bleed.** Mention-vs-link over 7 days: Vinicius Junior 182/39, Michael Olise
144/70, Yan Diomande 385/200. Split by whether the Editor *read* the article, the miss rate is
barely better — 24/99 read vs 15/81 unread. **A contract gap, not a capacity gap.**

**The disagreement finding — a built unlock sitting unused.** 13,252 sigil readings carry prose; **7**
mention disagreement; `WHY_NOW` has **never** fired — while the deterministic `pillar_convergence`
sees divergence down to 1. Two independent causes: vibe is a function of narratives, so two of five
pillars *cannot* disagree; and `DISAGREEMENT:` is an optional field. **Both must be fixed or it stays
silent.** Cheapest large win on the board.

**T9 — a trigram margin gate protects against the wrong failure.** The dominant error is a confident
single match to an entity that is simply not the one named, with no runner-up to hold it back:
`spain` → team 394 (sim 0.429, margin 0.429), `pep guardiola` → player `sergi guardiola` (0.500),
`sheffield wednesday` → `sheffield utd` (0.417). Every wrong row clears a margin gate more
comfortably than the one correct row, `vinicius jr` (margin 0.082). **Exact match on the normalized
surface is the only safe automatic path; trigram ranks and reviews.**

**T3 — similarity bands are not interchangeable.** At 0.71: *"Real Madrid reach agreement with RB
Leipzig"* vs *"Real Madrid yet to reach agreement."* Opposite claims, high similarity. Naive dedup
would delete the disagreement, which is the story.

**T4 — the Scout's reliability is the L8 discipline.** The percentile→tier mapping was taken away
from the model because local models invert it and call a 37th-percentile skill "above average."
Everything reaching that seat must arrive as a fact requiring no interpretation. **No prose reaches
the Scout.**

---

## 10. Operational notes that survive any architecture

- The harness pauses at 00,03,06,09,12,15,18,21:00 and resumes an hour later; ingest runs 02:00.
- **A deploy overrides the rest window.** The `.path` watchers fire on binary placement into
  `go/bin/` or `rust/bin/`, and `--build-only` does not avoid it. Building into `target/debug/` does
  NOT trip them — that is how `remap` and `eval` were run tonight without touching the daemon.
- Services are systemd **user** units on archbox (`systemctl --user`); `release.sh` needs
  `~/.cargo/bin` on PATH in a non-interactive shell.
- `scripts/hosting/snapshot-schema.sh` after every migration, committed with it.
- Long-poll background watchers get killed in this environment — use point-in-time checks.
- The DB role is superuser, so `SET LOCAL session_replication_role = 'replica'` works for suppressing
  triggers inside a transaction. **Never `ALTER TABLE ... DISABLE TRIGGER`** against a live pipeline —
  ACCESS EXCLUSIVE.

---

## 11. Two habits that earned their keep tonight

**Rehearse the real write, don't approximate it.** `remap -rollback` runs the production statements,
counts the rows, asserts the invariant, then throws it away. A copy of those statements in a psql
scratch file would be a second query claiming to be the first — the exact divergence that cost A5
weeks.

**Prove the guard can fire.** The T10 check reported 0 re-armed articles. That is worth nothing until
you know it *can* report non-zero, so: one unsuppressed flip in a rolled-back transaction produced
exactly one `article_read` row, and the same query caught it. **A guard never observed firing is not
yet a guard.**
