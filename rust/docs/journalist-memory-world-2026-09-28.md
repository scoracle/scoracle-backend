# Journalist memory: stored world and studied context

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

**Postgres stores the world. DuckDB studies the world. Plugins select and present
the findings. Laya scores. SmolLM3 articulates.**

This is a live database inspection and a working local DuckDB experiment, not a
production cutover. Archbox was accessed over SSH on 2026-09-28 with PostgreSQL
read-only transactions enforced. No production rows, functions, services or
delivery flags changed. Harvester v7 calibration remains accepted.

**Later implementation update:** the [on-demand memory study path](memory-studies.md)
is now implemented and connected to the local Journalist draft. It replaces the
earlier provisional recent-excerpt approach with request-time DuckDB findings and
adds pair-frequency and team-stat comparison request shapes. The inspection and
benchmark below remain dated evidence; statements describing integration as future
work reflect the earlier research stage. Production deployment and a successful
memory-plus-fresh prose result are still outstanding.

## The matrix already exists

The earlier Journalist draft's seven-day prior-excerpt list is too narrow to be
the memory architecture. There is no need to invent another memory store.

| Existing relation | Observed population | What it contributes |
| --- | ---: | --- |
| `narrative_events` | 65,887 | Dated, source-linked relationship/event extractions and separate junction-origin observations. Earliest reporting is May 31, 2026. |
| `narrative_links` | 158,304 | 140,266 co-mention associations and 18,038 typed aggregate links. Retrieval/navigation and reporting-pattern measurements. |
| `storylines` | 20,549 | 4,234 open, 16,301 dormant, 14 resolved story records. |
| `storyline_entities` | 88,191 | Entity participation, roles, entry/exit, progression and source history; 8,320 distinct entity keys. |
| `storyline_articles` | 48,033 | Source articles attached to story records. |
| `news_summaries` | 184,428 | 132,209 prose-bearing products, of which 73,019 have a storyline. Previous articulation/coverage history, not independent evidence. |
| `entity_relationships` | 669 | Source-document-backed canonical relationships: 628 active coach relationships, 19 active owner relationships, 22 superseded coach relationships. |
| `player_team_history` | 43,840 | Stored affiliation history for 14,988 players. |
| `transfer_ground_truth` | 52 | Applied, unreverted transfer identity records from the two existing ledgers. |
| `narrative_persons` → `persons` | 5,910 candidates; 2,032 canonical people | 644 active graph figures, 158 linked to canonical people through `person_id`. The namespaces are not interchangeable. |
| `analytics_entity_context` | 26,576 | Existing DuckDB-derived season/cohort studies, seasons 2018–2026; latest refresh Sept 28 at 01:34 EDT. |

Graph extraction, aggregate-link refresh and storyline progression all had updates
on the inspection date. Of the 88,191 memberships, 13,292 have progressed tellings;
5,780 carry the existing `established` authority label. A populated membership is
not itself a verified event.

The matrix is substantial at an individual entity level too:

| Entity | Events | Typed links | Story memberships | Progressed memberships |
| --- | ---: | ---: | ---: | ---: |
| Chelsea / FOOTBALL / team 18 | 909 | 206 | 409 | 37 |
| Aston Villa / FOOTBALL / team 15 | 538 | 163 | 315 | 33 |
| Lakers / NBA / team 14 | 523 | 113 | 210 | 33 |

## Read the lineage, not just the table names

`narrative_events` contains 59,987 extraction-origin rows and 5,900 junction-origin
rows. All extraction rows currently have empty `details` objects. The six event
predicates are coarse labels, not retained prose observations. 31,988 extraction
rows have no counterparty, so reading only `narrative_links` would miss substantial
history. Event source publishers are retained, and all event article IDs resolved
to stored articles at inspection time.

`refresh_typed_links` aggregates extraction-origin events over a default 90-day
window. It excludes junction-origin events. Its confidence is the maximum observed
confidence rank, not a new verification of the relationship. Strength and
trajectory reflect the stored aggregation formula; they are not Journalist scores.
Co-mentions locate associated material but do not establish semantic relationships.

The extraction adapter sets `event_date` from article publication time, falling
back to ingestion time. Junction writers have different observation-time semantics.
Neither is a universal real-world event-effective date. The prototype excludes
junction rows and calls the extraction timestamp `reported_at`.

Storyline lifecycle and membership progression were built with the legacy Editor
and Journalist path. They remain useful historical indexes, but title, membership,
role and `established` labels must not silently become world facts. The cutover
Journalist draft no longer depends on Editor packets or creates storyline identity.
Continuing the stored matrix across that cutover is a separate producer concern;
the presence of today's legacy updates does not prove the new path maintains it.

The former `narrative_context_for_entity` and `narrative_context_for_pair` functions
are absent on Archbox **by design**: migration 263 removed SQL prose builders.
Migration 222 describes an older design. Do not restore those builders or retired
episode tables. Shared `src/evidence/memories/sources.rs` reads some canonical
relationships, affiliations, fixtures, moves and cohort studies, but does not
currently retrieve `narrative_events`/`narrative_links` for Journalist.

## Concrete preparation defects found

The graph is rich, but its historical labels are not automatically ready to be
spoken. Two reproducible examples from the read-only inspection:

- Event **69185**, source article **789533**, links player **2503729** to Chelsea
  as a transfer rumor. That player resolves to **Santiago Gimenez**. The stored
  headline concerns **Scott**, and the retained full text contains neither
  “Santiago” nor “Gim”. This is an entity-binding problem in the stored record.
- Event **69309**, article **789575**, labels a Chelsea–Arsenal WSL highlights report
  as `injury`. The retained BBC text describes the derby result/highlights and has
  no injury observation. Turning that predicate into an injury fact would add
  information absent from the source.

These examples establish defects, not a measured fleet-wide error rate. Other
sampled story memberships also span apparently unrelated headlines and need
source-level inspection. No rows were repaired or deleted, and Graph's later
alignment window was not silently expanded into this task.

For Journalist, Graph should locate candidate history; source-bearing preparation
must determine which observations enter the package. Do not give SmolLM3 a graph
dump and ask it to adjudicate these defects. DuckDB makes relationships and patterns
cheap to study; it does not make a mislabeled upstream extraction true.

## DuckDB experiment

[`examples/journalist_memory_probe.py`](../examples/journalist_memory_probe.py)
exports seven explicit projections in one repeatable-read, read-only transaction,
with a fixed clock and SHA-256 file receipts. The Sept 28 **13:13:42 UTC** snapshot
contains 469,869 rows / 40,007,501 CSV bytes: events, links, stories, memberships,
story/article links, 70,255 linked source articles, and 18,650 canonical names.
Raw pages and prior generated prose are not exported. CSVs stay in the temporary
workspace, not in the repository.

The same snapshot is loaded into embedded DuckDB 1.5.5 and local PostgreSQL 17.11.
The production source is PostgreSQL 18.6 on Archbox. Both benchmark engines run on
this Mac, on persistent connections; no SSH/network distance to Archbox appears in
query timings. PostgreSQL uses indexed, analyzed session-local temporary tables.
DuckDB is limited to two threads, 512 MB buffer-manager memory and 256 MB temporary
storage. PostgreSQL uses 64 MB `work_mem`; the caps are not equivalent total-memory
measurements. PostgreSQL temporary-table scans cannot use parallel query, so this
is a frozen-input prototype comparison, not a production throughput benchmark.

Fifteen measured executions per engine/query alternate execution order, after a
separately recorded first execution. Every result is checked for exact parity and
repeatability. Timings include execution and fetching rows. The first execution
is not an OS-cold-cache measurement.

| Workload | Result rows | Postgres median | DuckDB median | Meaning |
| --- | ---: | ---: | ---: | --- |
| Two-window reporting study across entities | 3,142 | 30.87 ms | 11.02 ms | DuckDB ~2.8× faster |
| Entity event lookup | 8 | 0.68 ms | 2.91 ms | Small indexed lookup favors Postgres |
| Entity storyline lookup | 13 | 0.31 ms | 1.96 ms | Small indexed lookup favors Postgres |
| 90-day event-history grouping/ranking across entities | 11,385 | 269.43 ms | 38.53 ms | DuckDB ~7.0× faster |

Export took **1.73 s** and DuckDB loading **0.27 s**, separately from warm study
times. Loading/indexing/analyzing the Postgres control took **0.66 s**. This favors
amortizing bounded studies across a batch; exporting the entire slice for each
article would dominate the query savings. The links projection was inventoried
and loaded but is not queried by these studies; a production snapshot should
include only the relations required by its requested study.

Exact results, SQL, timings, source receipts and example findings are in
[`memory-duckdb-probe-2026-09-28.json`](../fixtures/journalist/memory-duckdb-probe-2026-09-28.json).
The prototype uncovered and corrected CSV null semantics: PostgreSQL's quoted empty
strings must remain empty strings; DuckDB loading uses `ALLOW_QUOTED_NULLS FALSE`.
This is covered by actual cross-engine result parity. See DuckDB's
[CSV import options](https://duckdb.org/docs/stable/data/csv/overview).

The explicit reporting-window test also checks equal half-open windows, boundary
timestamps, repeated extraction rows, self-edges, distinct articles/publisher
labels, and exclusion of junction/future/out-of-window records. It passes.

## What studied memory should supply

The prototype produces a `recorded_reporting_change` finding with two explicit
windows, article/publisher-label counts, the calculated count change, and the full
source-ID set. For example, the stored Chelsea graph slice has 28 distinct articles
in the recent seven days versus 58 in the prior seven days. That is **a change in
recorded, Graph-linked reporting**, not evidence that interest, pressure or the
entity's real-world situation improved or worsened. Extraction versions/coverage
also affect this observed population. The artifact marks these examples as
retrieval probes pending entity-link approval; they were not sent to the LLM.

Reporting volume proves the computation/provenance boundary. It is not a proposal
to put coverage counts into every Journalist story. More useful plugin-selected
studies are:

1. **Continuity:** source-supported earlier observations for the same identified
   development, ordered before the fresh report. Match an explicit event, fixture,
   relationship or source link; a shared entity alone does not prove one story.
2. **Relationship history:** dated canonical relationship/affiliation changes with
   their source documents and distinction between recorded and effective dates.
3. **Comparable change:** differences over compatible windows or seasons, with
   units, population and coverage carried in the finding. Reuse the existing
   cohort study where appropriate.

The target flow is:

```
Postgres source records + graph/history + canonical identity
    -> bounded, consistent snapshot for a named study
    -> DuckDB joins/orders/compares and returns typed findings + provenance
    -> memories.rs selects findings relevant to the fresh assignment
    -> meta + fresh + memories, with form + voice + prompt
    -> one SmolLM3 articulation call
```

No second generative preparation call, new memory database, SQL prose builder or
model fact-checking responsibility is introduced. SQL should live with the study
owner; `memories.rs` owns selection/presentation, not a competing analytical engine.
The existing Go analytical snapshot machinery is the integration precedent
(`go/internal/analytics/{snapshot,duckdb}`). This Python program is only the trial;
it adds no Rust dependency or production producer.

Before serving these findings in Journalist, replace the temporary excerpt-memory
draft with a typed study result, select it against the actual fresh assignment,
and bind source/snapshot/formula receipts into the prepared input hash. Keep memory
lineage distinct from fresh-source publication receipts and source-activity scores.
Corrections/deletions must invalidate a reused study; a highest-ID watermark alone
is insufficient. Fresh content remains fetched reporting, identity remains meta,
form owns output shape, voice owns tone, and prompt owns the articulation task.

## Shared across plugins

Memory is a shared product capability. Journalist is the first consumer being
aligned, not the owner of a separate memory world. Reuse the stored relationships,
study implementations and source receipts across plugins. Each plugin's
`memories.rs` owns which relevant findings enter its assignment and how much context
they receive. Reusing a finding must not change its factual meaning or authority.

Keep three responsibilities separate:

| Owner | Responsibility |
| --- | --- |
| Postgres and existing ingestion/identity producers | Store source-bearing observations, relationships, revisions and product history. |
| Shared analytical study owner | Load a bounded consistent population; compute a named, versioned finding in DuckDB; preserve its source lineage and availability. |
| Plugin `memories.rs` | Request/select findings against fresh material, retain necessary qualifications, and present a compact context view. |

The study result carries enough information to establish **what was studied, what
was found, and what the finding means**. Shared metadata includes entity keys,
study/version, capture time, source-content hash and source references. Each study
owns its specific result shape: comparison windows and units for a measurement;
participants and reporting/effective dates for relationship history; dated,
attributed observations for continuity. Do not force these into a universal prose
field or repeat every possible field in every model context.

Keep full receipts outside the writing view. The model receives the finding plus
the identity, dates, units, attribution and qualifications needed to express it
honestly. Shared snapshot/result infrastructure supplies reuse and invalidation;
the plugin supplies relevance and context budgeting. Empty, unavailable, stale and
measured zero remain distinct. A study with no useful connection to the fresh
assignment adds no memory text.

| Consumer | Useful studied memory | Meaning that must survive selection |
| --- | --- | --- |
| Journalist | Earlier observations about the identified development; relevant relationship changes | Earlier reporting stays dated and attributed; shared entities alone do not establish continuity. |
| Influencer | Earlier observed reactions by the same speaker/group to the identified subject | Preserve who reacted and the observed scope; reporting volume is not audience mood. |
| Scout | Compatible historical measurements and cohort comparisons | Preserve units, season/competition, sample coverage and comparison population. |
| Insider | History of the specific parties/relationship and recorded transaction states | Preserve rumor/report/applied-state distinctions and recorded versus effective dates. |
| Analyst | Comparable sequences of completed measurements and findings | Preserve time basis and upstream evidence origins; calculate direction before articulation. |
| Oracle | Relevant findings across the other products with their lineage | Shared source evidence is counted once; agreement among generated products is not corroboration. |

Internal plugins may consume the same scoped studies without an articulation call.
Memory work must not reopen Harvester calibration or bypass the planned Graph
alignment. The specific bad bindings found above remain source/preparation issues,
not instructions for a downstream voice to fact-check.

### What makes this ready to reuse

Prove one useful source-to-context path in Journalist, then exercise the same
snapshot/result boundary in the next relevant plugin. Share concrete study code
when its inputs and meaning are the same; keep character selection local. Evolve
the existing `src/evidence/memories` and Go analytical snapshot machinery rather
than adding a second memory store, a new generic workflow framework or a large
cross-plugin migration before the first integration works. The Python trial is
evidence for this work, not an additional production engine to maintain.

Readiness is about context quality as well as speed:

- A reviewer can reconstruct a finding from its retained inputs, formula and
  timestamps. Corrections and deletions invalidate affected reuse.
- Entity, relationship and development matches are supported before selection.
  Historical/future observations cannot silently become current facts.
- Numerical comparisons are computed over compatible populations; missing inputs
  and partial coverage do not become zeros or complete-world conclusions.
- Source reuse, syndicated reports and prior model prose cannot manufacture
  independent support or inflate fresh-source scores.
- A concrete context example shows why each retained memory helps the fresh
  assignment, while keeping provenance bookkeeping out of the prose task.
- Articulation preserves the supplied finding and qualifications. Structural JSON
  validity and cross-engine SQL parity alone do not demonstrate that result.

These are shared implementation and replay properties, not another list of prompt
guards for SmolLM3. Finish the preparation path before tuning wording around its
gaps. Roll consumers through their existing alignment windows; a fast study does
not mark the shared memory integration complete.

## Reproduce

Use an isolated Python environment with `duckdb==1.5.5` and
`psycopg[binary]==3.3.6`. Start an SSH tunnel to Archbox's PostgreSQL loopback, then:

```sh
python examples/journalist_memory_probe.py export \
  --source 'postgresql://scoracle@127.0.0.1:56488/scoracle' \
  --snapshot /private/tmp/journalist-memory-snapshot-20260928
python examples/journalist_memory_probe.py benchmark \
  --postgres 'postgresql://scotty@127.0.0.1:56487/postgres' \
  --snapshot /private/tmp/journalist-memory-snapshot-20260928 \
  --output fixtures/journalist/memory-duckdb-probe-2026-09-28.json
python -m unittest discover -s examples -p test_journalist_memory_probe.py
```

The export destination must be new. The benchmark database is the disposable local
cluster, never the production source. Its writes are only session-local temp tables,
which disappear when the connection closes. Snapshot checksums are verified before
loading either engine; more than one million rows per projection is rejected.
