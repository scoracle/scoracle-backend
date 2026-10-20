# Studio: the architecture contract

**The harness provides the space → plugins bring tools and instructions → tools provide context → the LLM discovers what the clues support and articulates the answer. Cheap code owns collection; expensive compute owns discovery.**

The [governing wiki contract](../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) owns this platform boundary. Rust prepares evidence and executes model assignments; Postgres stores evidence, work and products. Models receive prepared input and never call tools, search or retrieve.

## Reporting pipeline

**Hourly team RSS searches → known entity extraction → decision-model headline scoring → daily character work → saved products → client API.**

The Go collector searches team entities. Rust scores the query subjects and known teams, players and people named in the returned headlines. The existing sport-scoped name index resolves unique canonical names and stored aliases. Coaches, agents, owners, executives, officials, family and other known figures retain the canonical `person` type; IDs remain separate from player and team IDs. Ambiguous names stay unresolved. Unknown-person discovery and registration are a [subsequent session task](docs/headline-entity-pipeline.md#follow-up-unknown-entities).

Classifier sends the headline, publisher and canonical subject metadata to d1 through llama.cpp's `/v1/systemone`. Its five scores are subject relevance, emotion, movement, availability and clickbait. Relevance ≥55 selects Journalist; Influencer also requires emotion ≥15, and Insider also requires movement ≥80. Scores describe headline evidence. They do not establish facts, corroboration or relationships; clickbait measures wording, not truth.

Classifier performs no publisher fetch, article extraction, browser rendering or full-text scoring. Harvester/Editor replay implementations, browser-reader runtime and the `classifier_acquire` worker are retired. Archived article evidence, decisions and products remain stored.

## Character packages and publication

Journalist, Influencer and Insider receive one JSON document containing `meta`, `task`, `fresh`, `memories` and `voice`. Each plugin's `fresh.rs` selects and bounds its evidence; `prompt.rs` owns instructions and assembly; `voice.rs` controls expression. Production and evaluation use these same preparation paths.

- Journalist groups supported fresh headlines into up to three narratives, retaining exact contributing report keys.
- Influencer produces one supported emotional reading, or abstains when feelings are unknown.
- Insider produces an attributed movement reading with exact headline quotations for supported findings. Co-mentions alone do not establish a move.
- Scout remains statistics-only. Analyst and Oracle retain their existing peer-product dependencies. These statistical stages run for teams and players; people receive the reporting products.

Fresh copies from the same publisher are supplied once, regardless of RSS URL or routing scores. Every delivery still receives a durable outcome. Feed rank controls queue ordering and cannot make unchanged evidence fresh again.

Memories are attributed headlines from the prior seven days, using the same character lens, with at most 20 complete records and 6 KB. Their publication dates, URLs and scores remain attached. The view excludes fresh article IDs and decisions after the edition freeze. Memories describe previous reporting; they cannot establish a new event or today's emotions. See the [headline corpus contract](docs/headline-corpus.md).

The shared coverage controller freezes daily assignments at midnight America/Detroit. Queue leases, current-source validation and claim-fenced transactions protect retries and publication. Reverting a headline to an archived revision reconciles its current pointer and outstanding obligations. Completed evidence is not republished merely because its discovery candidates change.

A complete daily package over the existing 24 KB context budget is held. It is never split into repeated per-article publications. Output parsers retain structural checks, source mappings, attribution boundaries and served-prose guards. Parser acceptance does not establish factual model quality.

## Serving

The API reads saved products and performs no inference. The reporting routes `/news`, `/vibe`, `/transfers`, `/headlines` and identity route `/meta` accept `team`, `player` and `person`. Reporting reads accept `date=YYYY-MM-DD`; the default selects the latest saved reporting day. Statistical routes remain scoped to teams and players. Weekly reporting filters and `/weeks` are retired.

## Ownership and source map

| Owner | Responsibility |
| --- | --- |
| `src/plugins/<name>/` | Evidence selection, task, voice, inference, acceptance and publication policy. |
| `src/harness/` | Registration, model transports, budgets, queue claims, coverage schedule, diagnostics and atomic publication coordination. |
| `src/tools/` | Shared identity, source presentation, form decoding and output guards. Network fetch tools remain for structured Investigator and box-score work. |
| Postgres / SQL | Canonical identities, exact name matching, source revisions, spectra, delivery obligations, daily membership, factual arithmetic and stored products. |
| `src/evaluation/`, `examples/`, `fixtures/` | Replays, contract checks and retained evaluation evidence outside production execution. |

Share ordinary tool functions when doing so reduces complexity. Plugins assemble clues rather than prewrite conclusions. Preserve dates, uncertainty, unknown values, publisher attribution and complete evidence. Historical prose cannot become measurement evidence. Keep model requests and responses inspectable outside compact character inputs.

`tools/meta.rs` provides canonical identity; `tools/fresh.rs` presents evidence; `classifier/memories.rs` selects headline history; `tools/form.rs` owns structural forms and decoding; `tools/guards.rs` owns served-prose checks. The older DuckDB study tools remain available for explicit offline studies and are not character preparation dependencies.

The six character plugins retain their existing interfaces and model routes. `statcommentary` remains an explicit statistics command. Investigator uses structured Wikimedia evidence; `factsweep` remains unavailable pending a qualified discovery gate.

## Activation and verification

The [October 19 release record](docs/headline-plumbing-deployment-2026-10-19.md) describes deployed migration 314. The current working-tree changes require **migration 315 and matching Go/Rust binaries**, coordinated between daily editions with workers stopped. They are not deployed by editing these files. See [entity pipeline changes and follow-up](docs/headline-entity-pipeline.md).

For a read-only model replay: `CLASSIFIER_URL=http://127.0.0.1:18081 cargo run --release --example classifier_replay -- cases.jsonl 4`. Cases supply `id`, `sport`, `headline`, `publisher` and canonical `meta`.

Use [development guidance](../run_docs/DEVELOPMENT.md), the [operations runbook](../run_docs/RUNBOOK.md), and `scripts/check.sh` with an empty disposable PostgreSQL 18 database. Database checks exercise publication and recovery; live-model replays separately assess evidence fidelity and writing quality. Historical deployment notes and article benchmarks describe their dated implementations.
