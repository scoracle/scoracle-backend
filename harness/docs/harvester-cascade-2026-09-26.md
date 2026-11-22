# Harvester cascading relevance contract — September 26, 2026

## Decision

Harvester is a regular harness plugin. The older word *junction* does not denote a
different architectural kind; those units are plugins too. Harvester owns relevancy
combing, not editing, summarization, or final character judgment.

The versioned `harvest-v3` cascade is:

1. Google News runs one ranked query per entity. Its result membership, `feed_rank`, and
   query entity are the first relevance layer and become Harvester provenance. Harvester
   does not recreate Google's ranking with application rules.
2. Laya makes one simple second-layer decision: is the headline and source opening
   substantively relevant to the supplied entity? Passing mentions and same-name entities
   are irrelevant.
3. Only Laya relevance successes receive four independent distributions for the
   Journalist, Influencer, Insider, and Scout plugin perspectives.
4. Harvester mechanically publishes the unchanged headline and first three source
   sentences and attaches the stable IDs of character plugins whose `relevant` choice
   wins. The complete distribution is retained; no unvalidated numeric cutoff is hidden
   in the first contract.
5. Every tagged character plugin receives that source in its cognition context and makes the authoritative final
   relevance decision for its own work.

Harvester signals are advisory. A tag is permission to inspect source material, not an
article fact and not a requirement that the character use it. No generated summary,
sentiment interpretation, reporting gate, content rewrite, or keyword/regex semantic
filter participates in this contract. Editor has no role in this path: it neither admits,
routes, rewrites, nor interprets Harvester context.

Harvester retains two exact source slices. Laya sees a bounded 100-word/1,200-byte prefix
so the classification request fits its window. Tagged characters receive the first three
sentences, mechanically selected at `.`, `?`, or `!` boundaries without normalization.
Both slices carry byte offsets into the same retained body. This is selection, not semantic
extraction: no regex relevance logic, generated summary, or rewritten text is involved.
Transport failures, invalid distributions, and unreported model truncation are errors,
never negative relevance judgments.

The packet uses the character plugins' stable manifest identities:

- `scoracle.character.narrative`
- `scoracle.character.vibe`
- `scoracle.character.transfers`
- `scoracle.character.rating`

Analyst and Oracle retain their established downstream product dependencies rather than
receiving raw Harvester assignments.

## Editor cutover boundary

Editor is no longer part of semantic admission, character routing, source rewriting, or
character context. Its current production adapter still happens to own several effects
that must be moved deliberately before Editor can be pruned:

- fetching the publisher page and retaining `news_articles.full_text`;
- durable fetch/failure bookkeeping and exact-claim completion;
- name resolution, unknown-person nomination, and authoritative entity-link writes;
- fixture-result nomination;
- storyline membership and deterministic packet compilation obligations;
- Graph enqueueing.

Harvester should take the fetch, source retention, classification packet, and tagged
character-context publication in one claim-fenced adapter. Identity discovery can remain
an explicit Investigator/Graph handoff; storyline and packet behavior should be retained
only where the character products still need it. Removing Editor's generative voice must
not silently remove those unrelated durable effects.

## Historical Mac trial on the retained nightly corpus

Two immutable local MPS runs used the pinned English Laya base checkpoint and the 120
real September 24 sweep openings. Nothing called or wrote to the production queue or
database. The provisional entity labels are Codex source-review labels, not independent
human gold. Character relevance has no labeled evaluation set yet. These runs exercised
the prior `harvest-v2` paragraph/prefix extraction; `harvest-v3` preserves their classifier
finding while replacing delivery with the requested first-three-sentences contract.

The first cascade completed in 81.21 seconds with 99 relevance successes, 21 relevance
rejects, and no processing errors. Against 116 openings with provisional entity labels, the
simple gate retained 75/77 positives (97.4% recall), forwarded 97/116 labeled examples
(83.6%), and agreed with 92/116 labels (79.3%). This is promising evidence for the cheap
first gate, not a production accuracy claim.

The initial generic character option descriptions collapsed toward assigning every
plugin: 87 of 99 accepted articles received all four tags. A second run put the precise
character evidence definition into each option. It completed in 87.64 seconds with 98
accepts, 21 rejects, and one HTTP 422 processing error; the gate retained 73/77 labeled
positives (94.8% recall) and agreed with 89/116 evaluated labels (76.7%). Character
separation remained weak: 78 accepted articles still received all four tags. The single
error was article 759674; its flattened opening fit the earlier shorter question but the
longer four-question request exceeded the provider contract. It correctly remained an
error rather than becoming an irrelevant article.

The isolated SQLite importer reverified every published byte range and stored all 327
advisory character-plugin contexts from the second run. Production storage was untouched.

The base checkpoint warns that some of its temperature values are invalid and clamps
them at load time. Its returned values are model distributions and must not be described
as calibrated probabilities of correctness.

## `harvest-v3` verification

The clarified contract was replayed on the same frozen 120-article corpus on Mac MPS.
It completed in 50.00 seconds: 103 passed the Laya layer, 17 were rejected, and none
errored. Against the 116 provisional entity labels it retained 76/77 positives (98.7%
recall), had 76.0% precision, and agreed with 91/116 labels (78.4%). It forwarded 86.2%
of labeled inputs. These figures describe Laya as the permissive second layer after
Google; they do not isolate or score Google's first-layer retrieval quality.

All 120 bounded Laya inputs and all 120 three-sentence selections were verified against
the original source bytes. Among the 103 accepted articles, 100 had three detectable
sentence endings and three contained all available opening sentences because fewer than
three were detectable. The longest character context was 3,824 characters; the separate
Laya input never exceeded 100 words or 783 characters, eliminating the earlier model-window
error without shortening character evidence. The isolated SQLite replay stored 354 tagged
contexts and did not touch production.

Zero-shot character routing remains deliberately high-recall and expensive: 81 of 103
accepted articles tagged all four source-reading character plugins. The packet now exposes
the `relevant` value for each plugin directly as `character_relevance_probabilities`, in
addition to the full Laya distribution and winning choice. The characters can therefore
be the final safety guard immediately; later calibration can reduce fan-out without moving
semantic routing back into Editor.

The frozen September 24 export predates `feed_rank` in the replay JSON, so its v3 packets
record a null rank. The export query now includes `news_articles.feed_rank`; fresh cohorts
will preserve Google's ranking alongside the query entity.

## One-article Mac smoke trace

The read-only smoke harness followed article 755102, “Andy Reid provides positive injury
updates on 3 Chiefs starters,” from the retained Google News corpus through the real two-call
Laya cascade, a JSON serialization/storage boundary, and the real Journalist cognition path
on the Mac. It made no database, queue, or publication writes.

Laya accepted the article for the Kansas City Chiefs with a `relevant` distribution value of
0.8358. The character pass tagged all four source-reading plugins, with `relevant` values of
0.7366 for Journalist, 0.8076 for Influencer, 0.8230 for Insider, and 0.8091 for Scout. These
are routing distributions from the temperature-clamped base checkpoint, not calibrated
probabilities of correctness.

Harvester selected source bytes 0–355 as the mechanical first-three-sentence context. The
serialized packet was decoded again, hashed, and checked byte-for-byte against that range in
the retained publisher body. Journalist then received the full 355-character slice through
its actual prompt builder, bypassing the legacy 200-character RSS-description cap, and made
the final decision to publish one narrative with the headline “Kansas City Chiefs receive
positive injury updates on three starters.” The final local `granite4.2:3b` call took 4.604
seconds.

The final immutable trace is
`logs/context-harvest-20260924/smoke-755102-v2.json`. The historical corpus predates the new
rank export, so this trace correctly records a null Google `feed_rank`; it retains the ranked
entity-query hypothesis rather than inventing a rank. Evaluation-only Editor baseline data is
explicitly excluded from Harvester requests, stored packets, and character context.

## September 26 Chelsea, Cowboys, and Pistons cohort

The focused Mac replay included every candidate from the current Google sweeps for Chelsea,
the Dallas Cowboys, and the Detroit Pistons. Google returned 94 rows; ingestion-level exact
deduplication removed ten, leaving 84 canonical candidates. The old Editor had retained
publisher text for only 24 of them because of its ten-per-entity ceiling. Harvester attempted
publisher acquisition for every other canonical result, without that ceiling.

An initial ordering experiment classified the thin Google RSS description before publisher
acquisition. It was not viable: Laya forwarded 83 of 84 candidates, accepted obvious unrelated
items, and rejected a clearly relevant Pistons/Jalen Duren result. RSS descriptions in this feed
are usually the headline plus publisher name, so they did not contain enough evidence for the
second relevance layer. This failed trace is retained as
`logs/harvest-focus-20260926/cohort-v1.json`.

The decisive replay used the intended evidence boundary: Google selected and ranked the
candidates, Harvester acquired publisher text for all canonical rows, Laya judged a bounded
verbatim publisher opening, and only then did Harvester store the headline and first three
sentences and route the survivors. Of 84 canonical candidates, 77 acquired publisher text;
seven remained explicit acquisition errors rather than relevance rejections. Laya accepted 67
and rejected ten of the 77 materialized articles:

| Entity | Canonical | Fetch failures | Laya accepted | Laya rejected |
|---|---:|---:|---:|---:|
| Chelsea | 25 | 0 | 21 | 4 |
| Dallas Cowboys | 19 | 2 | 17 | 0 |
| Detroit Pistons | 40 | 5 | 29 | 6 |

All 67 accepted contexts were verified against their retained publisher byte ranges. Sixty-one
contained three detected sentence endings; six stored all available opening sentences. Together
they retain 42,878 bytes of reverse-enrichment context instead of URL-only records. Including
the rejected audit packets, the replay serialized 77 packets totaling 1,112,509 bytes. The
immutable corrected trace is
`logs/harvest-focus-20260926/cohort-v2-publisher-first.json`.

The economics match the target architecture. The complete replay took 284.57 seconds on the
Mac. Successful network acquisitions had a 0.896-second median. Laya's complete per-article
cascade had a 1.011-second median. Eight grouped Granite character calls had a 26.95-second
median, so model cognition—not classification—is the expensive work.

The quality result is mixed and prevents an immediate production claim. Zero-shot Laya routing
is still too broad: 53 of 67 survivors received all four character tags. Visible entity-gate
false positives include a Pistons query accepting unrelated Raptors, Lakers, Grizzlies, NFL,
Michigan State, and entertainment openings; visible false negatives include several genuinely
Pistons-linked Duren and rotation stories and Chelsea-linked player stories. No single numeric
threshold cleanly separates those cases, and the base checkpoint's clamped temperatures remain
uncalibrated.

The final character guard did useful work. Journalist received all 62 narrative-tagged contexts
across bounded entity batches, with every stored context verified present in its actual prompt.
It produced six narratives and cited 18 unique articles, leaving 44 tagged inputs unused. That
is evidence that Granite can perform the final product-level safety decision. Influencer also
completed its four entity batches, but its current product has no per-article citations, so it
cannot yet prove which individual contexts it accepted. Insider and Scout were not invoked by
this isolated replay because their actual creation contracts require resolved person/roster and
measured-stat context from their production adapters; Laya tags alone are not a safe substitute.

This replay supports the Google → publisher acquisition → Laya → stored exact context → character
shape. It does not support deploying the zero-shot character router unchanged or deleting the
Editor's remaining durable side effects yet. The next gate is labeled calibration for the four
character decisions, acquisition retry/fallback for the seven failures, and claim-fenced storage
and fan-out. Editor's generative classification and story voice can then be removed without
discarding fetch bookkeeping, identity/graph handoffs, or publication obligations.

## What is established and what is next

Google's ranked results plus the simple Laya gate are worth continuing: on this exploratory
set the Laya layer is fast, permissive, and exceeds the desired roughly 75% positive retention.
The present zero-shot character
stage is not yet a useful comb. Prompt wording alone did not create adequate separation.

Before selecting character-tag thresholds or training a character router:

1. export multiple fresh nightly cohorts on the Mac without changing production;
2. label each survivor independently for all four character plugins, grouped by story;
3. reserve a later nightly cohort as untouched evaluation data;
4. train or calibrate the four Laya decisions and measure per-plugin recall, wasted fan-out,
   all-four collapse, latency, truncation, and final character acceptance;
5. keep rejected and errored records for audit and replay;
6. add the durable claim-fenced persistence and fan-out adapter before putting Harvester
   in the deployed worker roster.

The manifest and explicit invocation path already use the same harness plugin vocabulary
as the rest of the fleet. Live roster enrollment remains intentionally incomplete because
claim-fenced storage and character delivery do not yet exist; registering a handler that
cannot durably publish would create false completion.
