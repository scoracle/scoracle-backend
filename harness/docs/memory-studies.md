# DuckDB character context

Postgres stores the durable world: RSS headlines, classifier receipts, source
identities, measurements, queue state and published character products. DuckDB
analyzes bounded source snapshots into the context fed to plugins. Rust prepares
requests, validates results and coordinates publication. The API reads the saved
products from Postgres.

Memories are a small reference point for continuity and nuance: dated reporting
about recent stories, emotions, movement and injuries. They are not a second
writing pass, an inferred event timeline or a source of new current facts.
Fresh reporting establishes the current development; memories provide context.
An earlier injury report remains an attributed report, not proof of present
availability or recovery.

## Active memory path

1. `src/plugins/classifier/memories.rs` selects retained headline sources for the
   canonical subject and character, respecting the frozen edition. It excludes
   the fresh article IDs, unresolved identities, instruction overrides and exact
   duplicate publisher/title pairs.
2. `src/tools/memories.rs` sends the bounded snapshot to `go/cmd/memory-study`.
   DuckDB owns the ranking in `go/internal/analytics/duckdb/memory.go`.
3. DuckDB orders by the character's relevance score, then subject relevance,
   publication time and article ID. Journalist uses subject relevance,
   Influencer emotion and Insider movement.
4. Rust resolves returned IDs back to the retained source records. Writing gets
   headline, publisher and publication date. Article IDs, URLs and classifier
   scores remain in provenance. Unknown or duplicate returned IDs are errors.
5. Each plugin fits whole memories after its fresh evidence. Journalist skips
   memory loading when its prepared fresh package contains no usable reports.

The history window is seven days before the earlier of the fresh-source cutoff
and the frozen reporting day's start. Selection allows at most 20,000 candidate
rows, returns at most 20 memories, and bounds retained memory JSON to 6,000 bytes.
The full character packet remains subject to its byte budget and exact model
context admission. A memory is removed whole when it does not fit; fresh evidence
is not clipped to make room.

The helper uses a private DuckDB engine with a 256 MB memory limit and two
threads. The Rust runner permits two concurrent memory studies, bounds input and
output to 8 MB, and enforces a 25-second timeout. The Go process has no Postgres
credentials, attachment or write-back path. Postgres remains the durable store.

## Other analytical context

The production cohort/rating context continues through the existing
`go/internal/analytics/snapshot` exporter, DuckDB computation and Postgres
publication. Removing unused historical memory modes does not replace this
analytical path with Rust or Postgres computation.

The retired generic reporting-frequency, standalone team-statistic,
publisher-reliability and Scout-record memory modes had no production consumers.
Their adapters and the old `memory_request` example were removed. Current source
selection, headline ranking and production cohort analytics remain active.

## Verification and runtime

Release builds include `scoracle-memory-study`. The Rust runner resolves it from
the standard release layout, beside the worker, or on PATH;
`SCORACLE_MEMORY_STUDY_BIN` overrides its location for tests. Worker startup checks
a nonempty headline-memory request before claiming character work.

Build the helper from the Go module:

```sh
go build -o /private/tmp/scoracle-memory-study ./cmd/memory-study
```
 The ordinary DuckDB tests cover character
ranking, tie order and date boundaries. Rust's ignored memory tests additionally
cover complete headlines, provenance and frozen-edition selection; use a migrated
disposable `classifier_test*` database and the helper override to run them.

As of 20 October 2026, the checked-in [Go driver is `v2.10505.0`](https://github.com/duckdb/duckdb-go/releases/tag/v2.10505.0) (DuckDB engine
1.5.5). Engine 2.0 qualification is a separate measured upgrade. The architecture
keeps analytical work in DuckDB; this cleanup makes no throughput claim for a
version that has not been run here.
