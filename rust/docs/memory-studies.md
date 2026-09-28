# On-demand memory studies

**Postgres stores the world. DuckDB studies the world. Each plugin chooses its
data and scope; the request/result contract stays the same.**

Implemented locally on September 28, 2026. Journalist now requests studied history
before articulation. No production deployment, data writes, precompute job or
new memory database was introduced.

## Request → snapshot → finding → context

`src/evidence/memory_studies` reads a bounded, repeatable-read/read-only Postgres
snapshot for the requested entity and time range. It invokes
`go/cmd/memory-study`, which uses the **existing Go DuckDB package**. The helper
receives JSON on stdin, has no database attachment, and returns typed findings.
The plugin selects a writing view; snapshot fingerprints and full selected source
IDs remain in provenance. A later request rereads the source records, so corrections
and deletions change the input fingerprint. There is no stale result cache.

The shared boundary comprises scope, named study/version, observed population,
findings, capture time and source lineage. Study-specific result types retain the
meaning of the data instead of forcing statistics and reporting into one prose
field. Other plugins can use these studies or add another study-specific adapter
without copying the engine or changing the articulation role.

| Request | Scope | Result |
| --- | --- | --- |
| `reporting` / `reporting_scope` | Team/player, half-open reporting window, optional entity pair and Graph predicates, excluded fresh IDs, maximum groups | Frequency-ranked groups, distinct canonical article count, publisher-label breakdown, source IDs and representative dated headlines |
| `statistic::team_matches` | Team, registered additive measure, competition, season, start/split/end timestamps | Both windows' fixture/measurement counts, totals, means per match, change and percentage change where supported, fixture IDs |

Timeframes are request parameters. The statistical split is also requested, not a
fixed precomputed weekly boundary. Team match totals are the first statistical
adapter; player measures, emotion history and the other character selectors still
belong to their respective alignment windows. Person reporting awaits reconciliation
of Graph-person and canonical-person identifiers and returns an explicit unsupported
scope rather than silently querying the wrong namespace.

## Frequency means recorded reporting

The reporting selector obtains source articles from extraction-origin Graph events.
Repeated extraction rows are collapsed. Recorded `duplicate_of` identities prevent
known reposts from inflating frequency. Sources are normalized by case/whitespace
for publisher counts; unrecorded syndication and outlet aliases are not inferred.

The requested entity's canonical name or a registered alias must occur as a whole
normalized surface in the source headline. Pair requests require both names, the
matching Graph endpoints, and any requested predicates. This is intentionally a
restricted, inspectable population; it does not count every mention in full text.
Zero qualifying records means no memory from this selection, not no interest or no
relationship in the world. Graph classification is retained as selection provenance,
not proof that a transaction happened.

For general history, stored storyline membership supplies a grouping index. An
article in several groups receives one deterministic group; an ungrouped article
uses its canonical article identity. DuckDB ranks by distinct article frequency,
then publisher breadth, recency and stable key. Stored storyline associations are
explicitly labeled as such, **not verified same-event identity**. Original publisher
headlines are served; generated storyline titles and coarse Graph predicates are
not turned into factual summaries. The known legacy binding/grouping defects in
the [live inspection](journalist-memory-world-2026-09-28.md) remain unresolved.

The pair request gives a useful shape for “these parties appeared together in N
qualifying reports during this window, from these sources.” Transfer-specific
requests additionally select transfer predicates. It does not turn N articles into
N independent confirmations, N transfers, or a probability supplied by the LLM.

## Comparable measurements

The team adapter selects one competition and season, completed/seeded fixtures not
marked as needing verification, and a registered `cumulative_total` measure. It
retains the measure's display label and stored unit classification. Null, malformed
or absent values stay missing; missing box scores remain visible through fixture
counts. This first study does not average ratios or percentages as additive totals.

DuckDB computes totals and per-match means in the requested windows. A missing
measurement in either selected window withholds the change comparison. A zero
baseline withholds percentage change. Fixture inventory completeness and sample
counts remain explicit; the output does not declare a trend “skyrocketing,” assign
causes, or manufacture statistical significance. Model prose must preserve these
supplied measurements and qualifications.

## Journalist integration

`src/plugins/journalist/memories.rs` currently requests **30 days** before the oldest
eligible fresh report, up to **three groups**, with **two representative reports per
group**. Change those plugin policy values to change its requested scope. Other
plugins need not adopt them.

Fresh eligibility is determined before a study is requested. Fresh article IDs and
their known canonical repost identities are excluded from history. Whole memory
groups fit a separate **2,200-byte** allowance alongside the **6,000-byte** fresh
package allowance; budgeting never clips a headline mid-observation. Memory selection
is reflected in the input hash and generation ledger. Historical IDs, publisher
counts and dates do not inflate fresh-source publication metadata or activity scores.
Recent publication history remains a separate 72-hour exact-text deduplication
input, independent of the analytical lookback.

The prompt remains the articulation task; form owns output shape; tone stays in the
voice file. There is one model call, no model memory query and no new generative
fact-preparation or fact-checking stage.

## Build and exercise

Release builds now include `scoracle-memory-study` from the existing Go module.
The Rust caller resolves it in the standard sibling `go/bin` layout, beside its
executable, or on PATH. `SCORACLE_MEMORY_STUDY_BIN` overrides the path for development.
Two concurrent helper calls are permitted per Rust process, each with two DuckDB
threads and a 256 MB buffer-manager cap. Requests are bounded to 20,000 observations,
8 MB input/output and a 25-second process timeout. Exceeding a population bound is
an error, not a silently truncated frequency. Empty reporting requires no helper.

Build the helper from `go/`:

```sh
go build -o /private/tmp/scoracle-memory-study ./cmd/memory-study
```

Use [`memory_request.rs`](../examples/memory_request.rs) to execute a scoped JSON
request without an LLM or publication. It also enforces a read-only connection:

```sh
SCORACLE_MEMORY_DATABASE_URL="$READ_ONLY_DATABASE_URL" \
SCORACLE_MEMORY_STUDY_BIN=/private/tmp/scoracle-memory-study \
cargo run --example memory_request -- request.json
```

Recorded requests and results are in
[`memory-live-requests-2026-09-28.json`](../fixtures/journalist/memory-live-requests-2026-09-28.json).
The live Chelsea two-week query selected **59 articles**; this is the entity-wide
population, not 59 player links. A Chelsea–Morgan Rogers 30-day request returned
**two articles from two publisher labels** (general association, not a transfer
claim). The selected Chelsea–Reece James fortnight was empty. The xG request
returned five measured fixtures across its two windows, including a one-match
baseline that must remain visible. These individual requests took approximately
30–152 ms after connection establishment, including snapshot selection and helper
startup. They are functional probes, not a throughput benchmark.

## Verification and current limits

- Rust library: **551 passed**, 77 environment-dependent tests ignored.
- Isolated cross-language integration: window/pair selection, wrong-name exclusion,
  distinct canonical frequency, publisher counts, source correction fingerprints,
  xG comparison and deletion/missingness passed.
- Six isolated Journalist publication/fencing tests passed. The checks caught and
  fixed an inclusive-second boundary in publication-history deduplication.
- Shared Go study tests passed for frequency, reposts, boundaries, duplicate input,
  numerical comparison and missing measurements. All Rust targets compile.
- Release script syntax and whitespace checks passed.

The first no-thinking n43 memory-plus-fresh articulation smoke test exhausted the
existing **900-token output allowance** after roughly 23 seconds. The complete
request and failure are retained in
[`memory-articulation-n43.jsonl`](../fixtures/journalist/memory-articulation-n43.jsonl),
with an explicitly synthetic source fixture. This is not a successful prose result
and does not establish factual fidelity. No prompt stack, automatic retry, model
judge or output-budget increase was added to hide it.

The subsequent [n47 contract audit](journalist-context-trim-2026-09-28.md) removes
wrappers and repeated memory headline text while retaining dated attribution,
population counts and full provenance. Only prompt.rs supplies task instructions;
form is structure only and voice is descriptive tone. The smoke-test instruction/data package is
22.8% smaller than n43. Its n47 no-thinking response completes and faithfully
preserves the training-time change, earlier schedule, venue and absent reason.
This is one synthetic case; other replay cases still fail reporting fidelity.

The on-demand data path is implemented. Reliable historical event grouping and
successful faithful articulation remain necessary before treating the complete
Journalist memory experience as finished. Other character runtime selectors have
not been migrated or declared complete by this work.
