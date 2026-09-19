# Card output and entity context

`rust/src/studio/form.rs` owns the tarot surface: a 140-character hook and a
1,200-character body, including spaces. These are ceilings, not targets. Journalist
narratives share one body allowance. Character prompts own perspective and voice.

All six writers request JSON. Scout, Analyst and Influencer share the `headline` /
`body` fields; the existing stored read, reading and narrative fields retain their
adapters. Schemas constrain structure, not string length: grammar length limits can
close a string mid-word. Parsers check the decoded surface. Typography normalization
preserves sentences and paragraphs; it does not shorten or rewrite the story.

The provider's completion reason is checked before parsing. An exhausted response
cannot become a card. A surface violation or exhausted completion gets up to two shorter
rewrites over the same evidence, with the same context and output limits. A third
failure returns to normal queue backoff. The successful request, including any
correction instruction, is recorded in generation provenance.

Runtime token reservations leave room for JSON and a complete card; they are not
prose targets. Scout, Analyst and Oracle reserve up to 700 output tokens in either
window size, while the decoded body still cannot exceed 1,200 characters.

The Scout reads each skill's current measurement alongside that skill's season
comparison and rate corroboration. SQL owns the measurement identity alongside the
numeric formula; a skill label alone is not a comparable unit. For example, shots
on target and expected goals can both occupy Shooting, but cannot produce a shared
season comparison. Missing ranks, changed measurement identities, or different
league cohorts leave the comparison unknown. The writer receives the sample and
statistics-update date, including the prior sample when comparing seasons.
No previous generated Scout prose is loaded. Internal distinctiveness scores
remain available for ranking and storage; they are not writing context.

When a formula changes meaning, revise its measurement identity at the same time.
The existing six-column rating functions remain compatibility adapters over the
measurement functions. In the stored breakdown, `pct` means an actual percentile;
`z` is a separate standardized distance, and `eligible` records the player sample
gate. An ineligible sample retains measurements but no percentile. A singleton or
constant measurement population also has no informative skill rank. Missing values
stay missing, not zero. Explicit feed-defined zero suppression remains supported.

Rate modes require a denominator and the requested rate inputs. They cannot fall
back to raw totals, mix those units into a cohort, or present an unchanged total
as per-minute corroboration. Measurement identity travels with both totals and
rates. Population calculations group by measurement as well as skill label.

The input identity includes values, samples, measurement identities and sourced
enrichment. Updating meaning or evidence can refresh a card even when a displayed
percentile happens to stay the same. Live rating evaluation uses the production
evidence assembly; archived fixtures retain their original context.

## Metadata convention

`corpus::load_identity_record` supplies compact identity descriptors; the card
wrapper adds one short framing line. Graph candidate lists and Editor hypotheses
use the same records. Transfer verification receives the subject and team records.
Every card writer uses the shared loader. A failed database read is an error, not
permission to generate without identity context.

Player affiliation comes from `player_current_identity`, preserving its override /
roster / stats precedence and observation date. Person records include role, club
and check date. Active, currently applicable role, affiliation, playing-status and
birth-date facts carry observation dates and source references. Identical fact
values are deduplicated in context; conflicting values remain visible. Photo and
other irrelevant dossier fields do not consume model context.

Acquisition uses career clubs to distinguish identities, never to choose a current
club. Only an explicit current coaching tenure can seed that affiliation. Subsequent
current-role and club changes belong to the reporting refresh, so rereading a
biography cannot undo a sourced correction.

The existing nightly `factsweep` checks news-active persons on a seven-day cadence.
It reads at most 12 dated excerpts, each capped at 280 description characters. Role
and affiliation are adjudicated independently. Each updated field requires quoted
support from two distinct supplied articles; co-mention frequency is not proof of
employment. Explicit, supported lack of affiliation can clear a club. Unknown
evidence leaves the existing dated value alone.

Updates lock the person and commit canonical fields, superseding fact history,
source citations and role relationships together. Policy controls which fields may
change. Confirmation-only check timestamps do not trigger fresh cards. Semantic
identity changes participate in writer and transfer debounce hashes, so subsequent
queued or scheduled work can revise the affected products.

The existing `refresh_dynamic_entities` clock remains the broader 30-day refresh
for news-active players, teams and accepted persons. Provider roster updates and
confirmed transfer applications retain ownership of player affiliation. Neither
writer prose nor a co-mention can mutate canonical identity.

## Verification and release

Run `cargo test --manifest-path rust/Cargo.toml`. Archived eval fixtures retain their
captured prompt versions; do not relabel old outputs as newly validated fixtures.

Migrations `252_rating_measurement_identity` and `253_rating_evidence_contract`
must precede the Rust release. Pause the affected writers during that sequence:
old Rust interprets a null rank as zero. Migration 253 atomically rebuilds existing
season and rate bundles from source statistics and checks the stored contract.
Budget a maintenance window for that rebuild; a failure rolls back the migration.
Do not infer historical measurement identities from old breakdowns.

Existing generated product rows are historical records; the migration does not
rewrite their prose. After release, verify new generations and the metadata sweep,
then use the existing explicit backfill workflow for affected stale cards. Refresh
the live schema snapshot only after applying the migrations.

Passing shape checks does not establish factual or editorial quality. Keep editorial
review offline: inspect representative real cards with their exact evidence and
runtime recorded, including thin samples, changed measurement sources, ordinary
profiles, and disputed personnel reports. Review the central claim, every comparison,
and whether the hook and body tell one supported story. Keep accepted examples and
rejected counterexamples in the review corpus; do not inject them all into daily
prompts or add a runtime judge.

The September 14 rechecks still rejected the existing 3B runtime's readings.
See the [implementation and release record](../progress_docs/2026-09-14-rating-evidence-contract.md)
before deployment; passing the data-contract tests is not release approval.
