# Entity memories: shared context and runtime consolidation

September 14, 2026. This implements the first package-building slice of
`planning_docs/PLAN-voice-form-context.md`, with the user's subsequent naming and
organization direction. It is local development, not a deployed metadata repair.

## Product and flow

```text
src/evidence/memories.rs     sourced packages, selection, fingerprints, and rendering
src/evidence/memories/      source queries and identity/performance preparation
src/studio/                 character briefs, shared form, creation, and validation
src/evaluation/memory.rs    offline compose_card(memories, new_evidence) probes
```

Studio creates from prepared assignments. Application/evidence adapters gather material,
select model routes, and publish. The offline composition probe returns separate system
and user inputs using the same briefs. No additional daily inference stage is introduced.

All six writer paths now load `memories::load` before hashing their material,
including the Insider pair assessment. The local implementation replaces the old
entity/pair memory functions and clipped prior-card loaders. It has not been
deployed. Editor and Graph use the same identity descriptor in candidate lists.

Runtime lives in `src/runtime/`, evidence in `src/evidence/`, and evaluation support
in `src/evaluation/`. Current personnel/availability reads moved from the Scout's
large stage module into `evidence/personnel.rs`; they remain current evidence,
separate from historical memories. No root-module compatibility aliases were added.

`memories.rs` provides one small contract:

- An entity key and a mission describing the context needed by its junction.
- Separate identity, reporting clock, competition clock and participation clock.
- Present evidence, established history, developing history and editorial memory.
- Record references and observation times. Effective dates retain their own source
  fields and precision; an observation date is not an effective date.
- Indivisible evidence groups, qualifications, material unknowns and omissions.
  Required groups survive together; optional groups are considered in caller
  relevance order. If required context cannot fit, assembly fails instead of
  removing a contradiction or cutting a qualification mid-sentence.
- A material fingerprint wired into every writer's regeneration gate. Selected
  facts, status/effective-date changes and factual omissions change it. Observation
  wall times, diagnostic notes, previous scores, prior prose and editorial omissions
  do not trigger another generation. Identity text retains its dated house record.
- Read-only, repeatable-read assembly over existing tables. SQL errors propagate;
  missing records remain unknown. No new inference stage or database table.
- A default 4,800-byte ceiling (`COGNITION_MEMORY_MAX_BYTES`), with no minimum fill.
  Required conflicts stay together; optional performance/history groups stay whole.
  Editorial memory is always selected last and needs original source article IDs.
  Factual selection runs independently of prior prose; whole factual packages
  that fit keep every record. Space for a factual omission notice is reserved only
  when factual groups must be dropped. Editorial omissions remain in the audit.
  Byte budgets do not certify a full provider window.
- Two renderings of the same selected package: a complete audit view with row references and
  observation envelopes, and a writer view that removes those administrative fields while
  preserving sporting facts, conflicts, denominators and uncertainty.

Selection policy lives in the shared mission-aware source adapter. There is no
universal importance score, minimum memory count, vector index or summary model.
The JSON `data` payload preserves heterogeneous source fields while the memory
section, timestamps, references and selection semantics are typed. It is not yet
a complete typed schema for every sport or upstream record.

The product aims to explain changes in sporting contribution: for example, more
scoring while chance creation holds steady, or a move from provider to finisher
when the measurements support it. `performance.sql` selects current and previous
snapshots in the same competition, preserving each team, season, denominator and
measure. It includes goals, assists, chances created and key passes alongside
other sports' core production. Football cumulative player measurements gain
per-90-recorded-minute values; missing numerators/denominators stay unknown.
The model decides the interpretation. Neither a role-change label nor a preferred
sentence is injected. Different action counts are not interchangeable, and a small
source snapshot does not establish reduced actual playing time.

The captured packages describe current knowledge on September 14. Old-season
measurements retain their season/team; this does not pretend that today's
employment was known in that old season. Historical knowledge reconstruction is
not implemented. A source observed after the package snapshot fails validation;
unknown observation times remain explicit.

## Three inspected packages

`source-2026-09-14.json` comes from `capture.sql`, run in one repeatable-read,
read-only transaction. The live schema was `251_team_colors`; migrations 252/253
were not applied. `web-checks-2026-09-14.json` contains separately dated,
paraphrased official-source verification performed during this session. Web
checks are review inputs, not writes to production metadata.

`packages-2026-09-14.json` contains the selected records, exact model input text,
unchanged system prompts, byte/character counts and content fingerprints.

| Case | Mission | What the package establishes | What it does not establish |
|---|---|---|---|
| Morgan Rogers, FOOTBALL/player/4592198 | Scout | Chelsea's official signing; stale Villa projection; 2025 Villa baseline of 37 appearances, 3,285 minutes, ten goals and six assists; 2026 Chelsea snapshot of one appearance, 90 minutes, one assist, 0.14 xG and 1.01 xA | Current complete season participation, injury/availability, an ability decline, valid rank comparisons, or exact signing date |
| Andoni Iraola, FOOTBALL/person/11 | Insider | Liverpool head-coach appointment; Athletic Club playing history distinct from erroneous active coach edges; Alex Scott is the recruit in article 145830 | Iraola being a Chelsea player target, simultaneous coaching appointments, or June 4 being the exact employment start date |
| Detroit Lions, NFL/team/25 | Journalist | Official regular-season schedule, September 13 home opener and September 17 trip to Buffalo; reporting week 2 separate from a recorded Week 1 fixture | Full participation/fitness, results established by the schedule announcement, or a precise season fraction inferred from selected fixtures |

Official checks:

- [Chelsea signing announcement](https://www.chelseafc.com/en/news/article/morgan-rogers-signs-for-chelsea): confirms Rogers moving from Villa to Chelsea for 2026/27. The inspected page does not expose a publication/effective date, so neither is invented.
- [Liverpool appointment announcement](https://www.liverpoolfc.com/news/liverpool-fc-appoint-andoni-iraola-new-head-coach): published June 4, 2026; distinguishes Iraola's new coaching role from his Athletic Club playing career.
- [Premier League Matchweek 4 appointments](https://www.premierleague.com/en/news/4711668/match-officials-for-matchweek-4): published September 7. This supplies competition context independently of Rogers' one stored appearance and the reporting grid.
- [Detroit's official schedule](https://www.detroitlions.com/news/lions-announce-2026-schedule): published May 14, with explicit regular-season week labels and dates.

The package keeps the complete relevant adult Wikidata membership's date values
and year precision, with its statement ID. Other career statements remain in the
source capture and appear as explicit omissions. Repeated coach-role imports are
omitted because the retained identity/official appointment already establish that
role. Conflicting relationship records remain visible together.

The 2026 raw Rogers goals key is absent in this capture. It stays null. The prior
session's rebuilt zero is not silently substituted into a raw-source package.
Legacy stored rating scores/percentiles are excluded because their measurement
contract predates the pending migrations. The historical raw production remains.

## Rogers: why context must include reconciliation

The preserved `rogers-transfer-audit-2026-09-14.json` records the failure chain:

1. The current projection selects an active 2025 Villa roster last observed July 3.
   Its precedence is override, active roster, stats, legacy player row; roster
   freshness does not affect that precedence.
2. A 2026 statistical row and box-score history instead name Chelsea. History's
   September 6/7 boundary is an ingestion timestamp, not the signing date.
3. Chelsea identity adjudication failed on July 19 (application 27), then rejected
   on July 20 (application 29) because the evidence was not yet final. No active
   override exists. This does not mean the model's earlier prediction was wrong.
4. The most recent Chelsea pair read is `is_rumor=false`, heat 43. The current
   application path requires a true incoming rumor and heat at least 80 before
   adjudicating identity. These latest reads therefore cannot reconcile the move.

This is a demonstrated maintenance gap, not grounds to convert a prediction into
truth or lower its threshold. The next context adapter should nominate a sourced
identity check when authoritative records disagree, then read the corrected
projection and retain the move as established history. Official completion must
be able to reach that owner independently of continuing rumor heat.

Proposed ownership: existing facts/relationships and transfer application/override
records retain source support, effective dates and prior state. A confirmed update
reconciles current identity and supersedes erroneous active edges atomically.
Generation only reads; it must not mutate identity. Changed source states and
selected evidence invalidate context. An unknown check is not verification, and
silence does not reverse a fact. These ownership changes are design work remaining;
no production repair, threshold change or new table is hidden in this slice.

## Consolidated ownership

| Consumer | Shared memories | Current evidence retained in junction |
|---|---|---|
| Scout | Identity/conflicts, verified nearby fixtures, matched production snapshots | Current profile, matched ranks, personnel/availability, recent measured trend |
| Analyst | Identity/calendar and matched production snapshots | Rating/Vibe trajectories and sample counts |
| Journalist | Identity/calendar, recorded moves, attributed earlier stories, optional sourced prior interpretation | Numbered current articles, activity signals, packet framing |
| Influencer | Same factual history; optional prior interpretation with source IDs | Current mood-bearing packets |
| Insider | Exact subject/pair history, identity, recorded outcomes; target identity separately | Current pair articles, source reliability or active wire board |
| Oracle | Identity and calendar | Selected current pillar cards and combined direction |

Rust no longer calls `narrative_context_for_entity` or `narrative_context_for_pair`.
The Journalist's clipped previous filings, Influencer's separate previous-body
block and Insider's clipped wrap trail are removed. Old SQL function definitions
and immutable historical migrations are not deleted by this Rust refactor.

Historical-season Scout requests exclude present employment, nearby fixtures,
current personnel/availability, stories and editorial memory. Historical production
is still a current read of an old season, not reconstruction of past knowledge.

## Reproduce and verify

```sh
# Offline: no database access and no model call.
cargo run --example memory_packages > /tmp/memory-packages.json
# Optional explicit byte ceiling; this is not a provider-token estimate.
cargo run --example memory_packages -- 7500

# Inspect the production memory adapter without generation or persistence.
cargo run --example memory_inspect -- FOOTBALL player 4592198 scout
cargo run --example memory_inspect -- FOOTBALL person 11 insider
cargo run --example memory_inspect -- NFL team 25 journalist

# Read-only player diagnostic, using the normal DB URL environment.
cargo run --example identity_audit -- FOOTBALL 4592198

# Source capture: use an authorized database connection, never a migration runner.
psql "$DATABASE_PRIVATE_URL" -X -qAt -v ON_ERROR_STOP=1 -f fixtures/memories/capture.sql

cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

The audit SQL was also prepared/executed against the live database in a read-only
transaction; it returned `stats_disagree_with_projection=true` for Rogers. The
Rust audit executable compiles; its SQL was validated through psql rather than a
second Rust connection. Official-web observations require fresh verification
when refreshing a capture; the offline fixture generator does not browse.

No runtime/tokenization or editorial-quality claim is implied by byte counts or
passing tests. Inspect these packages before offline generation with exact router
settings. The first four semantic regression tests cover baseline preservation,
conflicts/clocks, source correction fingerprints, temporal rejection and whole
record budgeting; a composition test checks unchanged system input plus new evidence.

## First generation test

The [Rogers memory ablation review](probes/2026-09-14/REVIEW.md) records four offline
calls with exact requests and raw responses. The historical baseline cost 248
additional prompt tokens and reached both full-memory outputs, but all four
outputs were rejected for unsupported relational, temporal or causal claims.
Voice/form remained fixed. This identifies the next context-presentation work;
it does not qualify live memory integration.

## Follow-up validation and voice refinement

The [v2 context review](probes/2026-09-14-v2/REVIEW.md) compares the compact renderer
with the initial probe. Full input fell from 1,986 to 1,520 tokens on the recorded
model, but all four outputs still failed editorial review. The
[voice comparison](probes/2026-09-14-voice/REVIEW.md) tested a candidate Scout brief
against the same context; it did not resolve the defects and was not adopted.
Current character text and form remain unchanged. Raw candidate instructions are
archived with their exact requests, not maintained as a second runtime voice.

Production-adapter SQL was prepared and executed under read-only transactions on
schema 251 for Rogers, Iraola and the Lions. The tighter history query eliminated
an unrelated Camara article; matched production retrieved Rogers' 48 key passes
and 43 chances created at Villa as well as goals/assists. See
`source-adapter-checks-2026-09-14.json`. The Rust adapter compiles and its SQL has
been executed through psql; a complete daemon run against the database has not
been performed. No metadata, migrations, queue or corpus was changed.

`web-performance-checks-2026-09-14.json` preserves fresh official Chelsea accounts
of Rogers' league debut goal and three cup assists. These are review evidence,
not a replacement league aggregate. They illustrate why the product must allow
continued creative strength as well as a change in scoring role.

## Production request checkpoint

The [complete Rogers request review](probes/2026-09-14-production/REVIEW.md) preserves the first
production-built Scout request and raw provider response. It exposed stale identity, incomplete
canonical box-score coverage, invalid pre-migration ranks, audit metadata in model prose, and an
overlong output containing unsupported role claims. The local follow-up separates audit/model
rendering, includes current performance and roster reports, records provider telemetry, and adds a
source-backed identity nomination route that remains behind the existing fail-closed owner. The
same review now includes a qualified thin-sample request: two current Chelsea reports reach the
writer, legacy unidentified/ineligible ranks fail closed, the one-appearance sample produces no
directional comparison, and the complete response fits the card surface.

## Next milestone

See [the next-session handoff](NEXT.md): deploy through the normal migration sequence, repair source
coverage and reconciliation through their existing owners, then qualify supported comparative,
stable and disputed-affiliation readings before further voice tuning. The local checkpoint passes
466 all-target tests, Clippy with warnings denied, formatting and whitespace checks.
