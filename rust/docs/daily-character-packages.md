# Daily character packages

These packages and daily scheduling were deployed October 16, 2026. Archbox owns
coverage and Classifier work; the Mac owns the six character stages. The existing
SmolLM3 model is unchanged. See the [deployment record](daily-pipeline-deployment-2026-10-16.md).

Each character receives one self-contained JSON document. Its `prompt.rs` owns
the task instructions and assembles four fields:

- `meta`: canonical target entity metadata, presented by `tools/meta.rs`.
- `task`: that plugin's instructions, stored in its `prompt.rs`.
- `fresh`: complete fresh reporting, current measurements or that day's finished
  peer cards, prepared by the plugin's `fresh.rs`. Publisher rendering is shared
  in `tools/fresh.rs`.
- `voice`: that plugin's own `voice.rs`, including Influencer.

The call has no separate system instruction and exposes no tools. Bounded
validation retries append their correction inside `task`, keeping one JSON
document and leaving entity metadata and evidence unchanged. Form
and memory context are absent. Native output schemas, response parsers, evidence
validation, complete-source budgets and claim-fenced publication remain active.
Reporting bounds belong to fresh evidence, not entity metadata. Scout's numeric
validation reads fresh evidence only; numbers in task instructions cannot prove
an output claim.

Export an inspectable package without a database or model call, from `rust/`:

```sh
cargo run --example character_package -- fixtures/quality/rating/synthetic-source-trigger.json > /tmp/scout-package.json
```

Current quality cases were updated offline. They verify assembly and contracts;
they do not establish model fidelity. Existing `eval --task rating --fixtures`
and the corresponding `narratives`, `vibe`, `transfer`, `momentum` and `oracle`
tasks compare the configured incumbent and optional candidate without publishing.
Use the `COGNITION_ROUTE_<ROLE>_CANDIDATE`, `_CANDIDATE_BASE_URL` and
`_CANDIDATE_BACKEND` settings for daytime experiments.

## Coverage monitor and host split

Go's `pipeline -mode monitor` waits for an enabled coverage schedule, performs an
immediate RSS sweep on startup or reopening, and starts another sweep hourly.
It checks the shared phase every 15 seconds between sweeps. Existing jobrun locks
prevent overlapping sweeps. A sweep checks the phase between teams; discovery
transactions acquire the shared coverage transition lock and check again before
persisting. Work interrupted by cutoff records a partial run and is rediscovered
on reopening. Clock errors pause discovery. The RSS lookback remains 24 hours.

The new `scoracle-coverage.service` template is rendered by the installer, but is
not automatically enabled. Archbox runs the monitor, acquisition and Classifier.
Mac runs all six character stages; the daily queue gate parks those stages during
coverage, leaving its inference endpoint available for read-only experiments.
The existing bounded cutoff, edition freeze, dependency ordering and one daily
publication guard still govern character work.

[Archbox](../examples/coverage/archbox.env.example) and
[Mac](../examples/coverage/mac.env.example) provide explicit stage configuration.
The Mac example pins character routes to its local endpoint. Replace its model
alias and retain the real database connection. The 32K request envelope matches
the existing article-reading minimum; it is not an increase in the measured
source budget or a claim that a server has that much native context. The native
llama.cpp token-fit check still rejects a package that cannot fit.

## Deployment contract

Deploy matching Go/Rust binaries and migrations 309–311 together. Existing Go RSS
ingestion now requires the coverage schema even while scheduling is disabled.
Replace the daily RSS cron with the coverage service and remove standalone
statcommentary/vibesynth character cron jobs when the daily worker owns them.
Do not use the legacy mixed Archbox stage enrollment with the Mac-only split.

The controller now freezes Scout measurements in the edition-opening transaction;
retries use the stored input while data ingestion continues independently. Use the
controller to open editions so measurements and reporting membership commit
together. Matching services, cron changes, host environments and database
enrollment are now active. A real end-to-end edition on the Mac remains to be
observed; the production clock was not advanced for testing.
Larger source budgets and an 8B candidate require measured token fit, useful
outputs and edition duration on the actual Mac before promotion.

## Recorded first-pass outputs

The [October 16 SmolLM3 baseline](../fixtures/character-outputs/2026-10-16-smollm3-single-json/review.txt)
contains thirteen fictional-control outputs across all six characters. Its
`outputs.txt` keeps the readings and `review.json` separates evidence review
from mechanical acceptance. Per-case folders retain the exact prompt, native
request, tokenizer coverage, raw response, output and validation results.
The supplemental Scout case includes a fresh-report correction; the older
withdrawal-named fixture has no reporting and cannot test that behavior.

Record a new run using the current fixture assemblers and validators:

```sh
cargo run --example character_outputs -- NEW_OUTPUT_DIR http://127.0.0.1:8080 MODEL_ALIAS fixtures/quality/narratives/late-qualification.json
```

Append more fixture paths to cover additional characters. The directory must be
new. Requests run sequentially at 32K context with thinking disabled and no
retries, database access or publication. The recorder refuses stale contracts
and preserves rejected or failed responses for review. It uses the existing
fixture evaluation path. Insider fixtures now rebuild report-specific schemas and
reuse the production evidence parser, including exact quotations and co-mentions.
The recorder does not exercise database publication fences or qualify a complete
production edition.
