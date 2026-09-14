# PLAN — Voice, Form, Context

Written September 14, 2026. Active handoff for the next session, starting in
`scoracle-backend/rust`.

## 0. Start here from /rust

Read this file first: `../planning_docs/PLAN-voice-form-context.md`.
The repo root is one directory above the next session's working directory.
Paths in the rest of this document are repo-root-relative unless stated otherwise.

Then read:

1. `../docs/cognition-output.md` — the implemented output and metadata contract.
2. `../progress_docs/2026-09-14-rating-evidence-contract.md` — implementation,
   verification, remaining editorial failures, and release status.
3. `../progress_docs/2026-09-13-editorial-quality-investigation.md` — controlled
   experiments and the distinction between data defects and writing failures.
4. Relevant implementation files listed in section 11 below. Inspect the selected
   paths, not every historical planning document or every archived experiment.
5. The normal development/release instructions before any operational work.

**The next session is a context-package strategy and design session first.**
Voice and form are settled for this phase. Do not resume character tuning,
rewrite the shared form, change model routes, deploy migrations, or regenerate
the corpus as an inferred first step.

This plan records the user's latest direction. Older model-specific tuning
programs and their word-count/keyword goals are historical context, not the
objective of this next session. Operational safety requirements still apply.

## 1. User intent

The product should be simple, durable, model-agnostic, and able to support a
growing corpus without accumulating prompt rules, repair heuristics, or daily
inference stages.

The three responsibilities are:

- **Voice:** the character's perspective, judgment, and manner of expression.
- **Form:** the tarot card surface on which that character writes.
- **Context:** the entity-specific knowledge, memories, circumstances, and evidence
  from which the character can form a nuanced interpretation.

The user's description is that `form.rs` supplies the canvas and character prompts
supply the colors. The model does the writing. Context must give that writing
substance without scripting its conclusions.

Latest direction, quoted from the user:

> WE NEED IT TO INCLUDE ENOUGH MEMORIES TO PROVIDE NUANCED OUTPUT.

> WE NEED IT TO HAVE AN UNDERSTANDING OF THE ENTITY META DATA (MAYBE WE SHOULD
> INCLUDE HISTORICAL CLUBS/TEAMS IN THE META TABLES?), AND UNDERSTANDING OF WHERE
> WE ARE IN THE SUBJECT SPORT'S SEASON.

The user also explicitly warned against AI slop and emphasized the limited
context available while processing substantial daily data. Richer context does
not mean indiscriminately larger prompts.

### Working objective

Design one reusable, time-aware entity context contract that lets every junction
understand:

1. Who this entity is, including current role and relevant historical affiliations.
2. Where the entity and its competition are in the sporting calendar.
3. What has happened before and what remains unresolved.
4. What the present evidence actually establishes.
5. What is unknown, stale, disputed, or not comparable.

Enough memory to support interpretation; enough provenance to keep memory from
turning into invented evidence; small enough packages to sustain daily throughput.

## 2. Decisions already made versus proposals

### Settled by the user

- Use the three-pronged Voice / Form / Context approach.
- Focus next on context, not another round of voice/form tuning.
- Card body ceiling: **1,200 characters**, including spaces.
- Separate hook ceiling: **140 characters**.
- These are ceilings, not length targets or mandatory coverage requirements.
- Seek the durable cause-level solution; avoid accumulated special-case guards.
- Preserve model independence and be surgical with context consumption.
- Include meaningful memory, entity metadata, and season understanding.

### Proposed direction to work through next

- Represent historical affiliations as sourced, dated, role-specific relationships.
- Distinguish established history, developing history, and editorial memory.
- Use one shared identity/time foundation with junction-specific selections.
- Select memories by relevance to the current subject, not recency alone.
- Preserve a valid historical baseline even when a current sample is unranked.
- Inspect concrete context packages before deciding on new tables or abstractions.

These proposals are not implemented or fully approved field-level schemas.
Do not present them as shipped functionality. Exact package shape, memory
selection, budgets, and storage consolidation still need design work.

## 3. What the last implementation did — and did not do

The preceding work fixed real defects at the data and output boundaries:

- Shared form defines the tarot surface; character briefs define voice.
- Writers request structured output with adapters for existing stored fields.
- Provider completion reasons are checked. Exhausted output cannot become a card.
- Surface violations/incomplete completions receive one bounded rewrite over the
  same evidence, then ordinary queue backoff. Code does not chop prose to fit.
- A large amount of editorial salvage logic was removed.
- Shared identity loading reaches writers and extraction/verification paths.
- Biography/career affiliations no longer overwrite current identity as though
  the first historical club were the current club.
- News-active person metadata has a sourced refresh path in `factsweep`.
- True statistical percentile ranks no longer share a field with fallback
  standardized scores in the proposed SQL producers.
- Measurements carry their identity; different source measures cannot silently
  share a skill population or season comparison.
- Missing ranks and numeric values remain missing in Scout.
- Rate modes no longer substitute raw totals for absent rate inputs.
- Scout receives sample sizes and statistics-update dates.
- Automatic reuse of Scout's generated prose was removed.
- Live rating evaluation uses the same enrichment path as production.

**These changes are local, committed work, not a deployed release.**
Migrations 252 and 253 have not been applied to production. No model route was
changed and no stored production cards were regenerated.

### Important limitation to correct in the context strategy

The current Scout comparison path admits matched historical ranks only when the
current and prior measurements are comparable and ranked. That is appropriate for
a rank-to-rank comparison, but it is **not a sufficient historical memory policy**.

An unranked current sample does not invalidate a full previous season. The future
context package should preserve useful baseline history while withholding the
unsupported trend claim. Removing old generated prose also should not imply that
the character must have no memory.

The last writing probes used corrected but still narrow context. Their failures
are real; they do not prove that the intended richer context package has been
tested, or that every small model/runtime is incapable of the product.

## 4. Existing foundations to reuse and audit

Repository inspection found the following structures. Their existence does not
establish their present coverage or fitness as a canonical context source.

### Current identity and history

- `players`, `teams`, `persons`: primary entity records and metadata.
- `player_current_identity`: current-player affiliation/position projection with
  override, roster, and statistics precedence.
- `team_rosters`: season/team/player membership with first/last observation times.
- `player_team_history`: existing player/team history.
- `entity_facts`: sourced facts with validity dates and active/superseded/conflicted
  states.
- `entity_relationships`: sourced, dated typed edges, including role relationships.
- Transfer identity applications, ground truth, and availability records:
  structured changes with lifecycle/provenance.

**History caveat:** `detect_team_change()` is driven by arriving box scores.
It stamps `NOW()` into historical boundaries and changes `is_current` on observed
team changes. Those dates are not automatically actual signing/leaving dates.
Offseason changes are not comprehensively captured by that path. Audit the effect
of late/historical box-score ingestion before trusting it as a career timeline.

**Current-person caveat:** `persons.team_id` is documented as a convenience hint;
dated facts/relationships hold richer affiliation authority. The context contract
must define how these are reconciled, rather than silently treating whichever
table was queried first as the truth.

### Season and competition

- `sports.current_season` supplies a sport-level season key.
- `sports.clock_league_id` and `season_weeks` supply the reporting calendar.
- `fixtures` carries competition, season, teams, date, round, status, and metadata.
- `leagues` identifies the relevant competition.
- Player/team statistics and event records supply participation and evidence windows.

**Calendar caveat:** migration 250 intentionally anchors FOOTBALL's reporting grid
to a nominated league. Preserve that product/navigation decision. Reporting week
is not the same thing as every entity's own competition round or season stage.
Do not “fix” the navigation calendar to solve the context problem.

### Memory and story progression

- Storylines, storyline entity participation, packets, claims, and resolutions.
- `narrative_context_for_entity` and `narrative_context_for_pair`.
- Prior generated products and their recorded inputs/provenance.
- Per-character loaders for prior readings and sourced changes.

Memory currently enters through several separate paths. The entity memory function
mixes prior stories, completed moves, open-story progression, figures, and prior
model-derived scores/readings. A string assembled by that function is not
automatically a clean or uniformly authoritative memory package.

## 5. Proposed context contract

Think of this as one logical contract with several evidence types, not necessarily
one new database table or one giant JSON document.

### A. Identity and relevant affiliation history

The minimum shared identity should establish entity key/type, sport, current role,
current affiliation, relevant competition, and record freshness.

Historical affiliation should retain:

- The club/team and entity role during that affiliation.
- The relationship type when material: playing registration, loan, coaching,
  executive position, national-team membership.
- Effective start/end dates when known, preserving unknown or approximate dates.
- Source references and observation/verification time.
- Whether the relationship is current, historical, disputed, or superseded.

Do not collapse role history into a generic “former player” tag. A current coach's
past playing career must not make them a current player transfer candidate.
Likewise, nationality is not evidence of national-team selection.

Loans, parent-club registration, and national-team membership may coexist. “Current”
must have a relationship-specific meaning; one global team field cannot safely
express every relationship.

Store the complete supported history, but select relevant history for the brief.
A former club is particularly relevant to a reunion, return-transfer report,
managerial connection, or career transition. It is not mandatory filler on every
statistical card.

Do not create another independent metadata blob merely because the prompt needs
a historical-club line. Audit and reuse the dated fact/relationship structures
first; propose schema additions only for demonstrated gaps.

### B. Sporting time and data coverage

Keep three clocks distinct:

1. **Competition clock:** the actual competition/season, stage, round or schedule
   progress, and relevant upcoming events.
2. **Entity participation clock:** appearances, minutes, recent activity and
   applicable availability records.
3. **Evidence clock:** what period the evidence covers and when it was observed
   or updated.

One appearance could reflect an early season, restricted participation, a new
arrival, or incomplete coverage. It does not itself establish any of those causes.

Potential context includes preseason, regular season, playoffs/knockout phase,
offseason, a scheduled break, or a relevant transfer window. Derive these from
supported competition information; do not infer them from the month, sport name,
or absence of local fixture rows.

Only show precise games-remaining counts or season fractions when schedule coverage
supports them. A partially ingested schedule must not become a precise claim.
Handle postponed fixtures and distinguish a competition-wide round from a team's
own games played.

Historical backfills need an explicit as-of policy. A report about an old season
must not silently receive today's employment, injuries, or unresolved stories as
though they were true in that historical window.

### C. Three kinds of memory

**Established history:** source-supported events and measurements. Examples include
a completed move, coaching appointment, injury/return, or full previous-season
performance. These can provide factual baselines.

**Developing history:** the dated progression of an unresolved situation, including
contradictions and explicit outcomes. A reported approach, denial, later confirmation,
and unresolved status should remain distinguishable. Silence does not prove failure.

**Editorial memory:** what a character previously concluded, with its date and
evidence references. This can support continuity without counting as independent
corroboration. Select it only when it adds value and remains applicable.

Recommended starting point: a small selection of consequential factual memories
plus, when useful, one prior editorial interpretation. This is a design hypothesis,
not a fixed quota or a required context section.

Avoid loading a truncated old paragraph simply because it is the latest one.
Truncation may remove qualifications. Do not turn unsupported old cards into
trusted memory during a migration or backfill.

A stored memory should retain enough provenance to answer:

- What happened, was reported, or was concluded?
- About which entity, relationship, or storyline?
- When did it apply, and when did we learn it?
- What supports it?
- Has it been superseded, contradicted, resolved, or corrected?

Derived memory summaries, if eventually useful, must be traceable to their inputs
and invalidatable when those inputs change. Do not start by adding a summarization
model call to every daily generation.

### D. Present evidence

Keep current evidence distinct from baseline/history and prior interpretation.

Statistical observations need their measurement identity, units/rate basis, sample,
competition scope, and time window. A missing current rank can coexist with known
historical production. No present-season trend should be invented from a different
measurement or from unequal/unknown evidence windows.

News evidence needs attribution, event/report dates, claim status, and source
references. Repeated reporting of the same assertion is not automatically
independent confirmation.

### E. Unknowns, conflicts, and freshness

Unknown information is part of the contract when its absence affects interpretation.
This does not require a long checklist of every empty database field.

Make material gaps visible: unresolved current affiliation, absent season-stage
coverage, unavailable comparison basis, stale identity, disputed availability.
Never translate missing data into zero, fit/available, no interest, stable form,
or a completed outcome.

Database failure is different from a legitimately unknown fact. Essential identity
load failures should not silently become “unknown, continue writing.”

## 6. Junction-specific selection from a shared foundation

One shared contract does not mean identical packages or maximum-length biographies
for every junction.

- **Graph / Editor / Investigator:** compact identity disambiguation, current role,
  relevant relationship history and temporal framing. Enough to attach the claim
  to the correct person and role; usually no need for several old card bodies.
- **Transfer verification:** the exact subject/team relationship, role, current
  affiliation, relevant past affiliations, and progression of this specific story.
- **Scout:** current measurements, meaningful historical performance baseline,
  participation context, and supported personnel/availability changes.
- **Analyst:** relevant trajectories and their windows, with historical context
  that distinguishes recent change from a long-established profile.
- **Journalist:** consequential developments, prior events explaining them,
  unresolved threads, attribution, and chronology.
- **Influencer:** supported emotional/social context and its development;
  reporting tone alone is not evidence of audience sentiment.
- **Insider:** affiliation history, exact transfer relationships, story progression,
  and relevant source track record.
- **Oracle:** a compact synthesis context whose selected findings retain their
  dates and evidence origins. Five derivative readings repeating a claim must not
  appear to be five independent sources.

These are proposed selections within the existing character roles, not authority
to rewrite voice or reassign product ownership. Inspect actual current call paths
before promising that a given input reaches a junction.

## 7. Selection, context budget, and refresh strategy

### Selection

Start with a small shared identity/time foundation and the evidence needed for the
current task. Add memories that explain the situation, supply a relevant baseline,
or establish a consequential change/unresolved thread.

Recency is one factor, not the sole ordering rule. An older verified appointment
can matter more than several fresh duplicate headlines. Pair-specific history
can outrank generic entity history.

Initially favor deterministic queries over the existing structured records.
Add semantic retrieval only if concrete cases show that those queries miss useful
memories. No default vector-database redesign or extra agent stage.

### Budget

Do not choose a blanket larger context window or a mandatory memory word count.
Measure actual packages and provider tokenization, while keeping product semantics
independent of one tokenizer or model family.

Preserve dates, units, provenance/status, and relevant qualifications when reducing
context. Deduplicate repeated facts across current evidence, metadata, and memories.
Discard low-value records before damaging the meaning of a retained record.

Reserve output capacity separately. The 1,200-character canvas does not establish
an input token budget, and a model's reasoning budget is not visible card length.

### Refresh and self-healing

Define ownership and reconciliation rather than adding periodic rewrites everywhere:

- Provider/confirmed transaction paths maintain the facts they are authoritative for.
- Reporting can nominate and support updates through the existing adjudication path.
- A verified change updates the current projection and preserves prior history.
- Corrections invalidate affected memory/context selections instead of silently
  accumulating competing “current” descriptions.
- Expired/stale data is eligible for re-verification, not automatic inversion.
- Context construction reads established records; it should not mutate identity
  merely because a generation is being attempted.
- Cache shared identity and competition context where safe. Invalidate on meaningful
  evidence changes; decide explicitly how as-of boundaries affect cached packages.
- Record selected source IDs and material omissions for diagnosis/replay.

The preceding person refresh checks news-active persons on a seven-day cadence with
bounded excerpts and quoted support. Treat that as the implemented starting point
to evaluate, not proof that every entity/field has adequate freshness coverage.

## 8. First deliverable: three concrete packages

Before creating new tables or replacing loaders, assemble three inspectable packages.
Use actual sourced records and explicit unknowns; do not write idealized invented
examples and then score the model against them.

### Case 1 — Morgan Rogers: thin current sample, substantial history

Captured diagnostic identity: FOOTBALL/player/4592198, Aston Villa, league 8,
Midfielder. This is a recorded test case, not a promise that these facts remain
current when the next session starts.

Captured source statistics:

- 2026: one appearance, 90 minutes; statistics updated September 7, 2026.
- 2025: 37 appearances, 3,285 minutes; statistics updated June 13, 2026.
- Rebuilt 2026 observations include 1.01 expected assists, one assist, 0.14 expected
  goals, and zero goals; no skill percentiles because this sample is ineligible.
- Prior-season production includes ten goals and six assists. Its Shooting measure
  is shots on target, not the current expected-goals measure.

Design question: how should a character receive a useful full-season baseline
without being encouraged to call a one-appearance snapshot a decline in ability?
The competition's actual progress must be established independently of his sample.

### Case 2 — A current coach incorrectly treated as a player target

The captured false-positive case was Chelsea/team/18 with Andoni Iraola/person/11.
The recorded database identity had him as a coach at Liverpool/team/8. The article
context concerned other recruits and quoted a coach. Verify the actual record and
source evidence again before using it as a current test.

Design question: can the package distinguish present employment/role, past playing
affiliations, and being quoted about somebody else's recruitment?
Historical club membership must help resolve the story, not create a transfer link.

### Case 3 — Competition stage changes the interpretation

Select a real, well-supported case: offseason versus active competition, a playoff
or knockout phase, a relevant transfer window, a scheduled break, or a late-season
situation. Choose based on available coverage; no entity has yet been selected.

Design question: does the package give enough sporting-calendar context without
confusing the reporting grid, a partial fixture import, and actual competition state?

For every case, record the assembled package, underlying references, omissions,
character selected, as-of date, and the conclusions the evidence does and does not
support. Inspect the package before generating a card.

## 9. Acceptance criteria and implementation sequence

### Design acceptance

- A reader can identify the entity's current role and distinguish relevant history.
- Effective dates and observation dates are not conflated.
- Competition state, participation, and data coverage are distinguishable.
- Thin current evidence does not erase a valid historical baseline.
- Factual, reported, and editorial memories remain distinguishable.
- Material contradictions survive selection; repeated claims are deduplicated.
- The package supports nuance without requiring an invented causal explanation.
- Every selected item earns its space; there is no fill-to-budget behavior.
- Each junction receives the shared semantics it needs, with task-specific depth.
- Source corrections and identity changes have a defined invalidation path.

### Suggested implementation sequence after the design is agreed

1. Audit source coverage and current loader consumers for the three cases.
2. Assemble the proposed packages using existing sources.
3. Review usefulness, ambiguity, provenance, and context size with the user.
4. Agree the minimal typed package and ownership/reconciliation rules.
5. Implement a shared context assembler and narrow selection interfaces; adapt
   existing loaders rather than leaving multiple competing identity/memory paths.
6. Add fixtures for the real packages, temporal edge cases, and corrections.
7. Evaluate actual generated cards offline with voice/form held fixed.
8. Qualify runtime behavior and throughput using exact recorded requests/settings.
9. Plan deployment/backfill explicitly; do not silently combine it with design work.

Do not convert these steps into a large agent framework. No new runtime editorial
judge, universal memory summarizer, elaborate scoring rubric, or regex library of
“bad writing” is a prerequisite.

## 10. What verification has established

At the close of the preceding implementation:

- **448 Rust tests** passed across all targets.
- Strict Clippy, formatting, and `git diff --check` passed.
- Go articulator tests passed without changes to the user's Go implementation.
- Synthetic SQL tests covered missing ranks, real zeros, measurement changes,
  populations, team cohorts, and missing rate inputs/denominators.
- Final proposed SQL functions rebuilt temporary copies of **44,300 player rows
  and 1,224 team rows**, covering **25 player and 25 team sport/season cohorts**.
  Contract checks passed and all diagnostic transactions were rolled back.
- Rebuilding changes some derived scores intentionally. Morgan's captured 2025
  composite changes from 72.4 to 70.1. This is not a byte-preserving rating migration.

These results establish the implemented data/output contracts, not editorial
quality or production deployment.

### Writing experiments and limits

The September 13 record contains 27 controlled probes. Tests varied context,
prior prose, character length, examples, temperature, and thinking. Several probes
initially used the application's mislabeled percentile values; those are not
truth-valid percentile examples.

The September 14 recheck used the compiled s33 writer, rebuilt Morgan observations,
a recorded identity snapshot, and no optional personnel/report/recent-form blocks.
Two `granite4.2:3b` calls with `think=false`, context 4096, output reservation 700,
temperature 0.6, and seeds 17/43 still failed editorial review:

- One fit the surface at 1,186 body characters but invented strong defensive ranking
  and consistent/limited abilities.
- One expanded a combined CBI count into three component counts, invented other
  characteristics, and exceeded the surface at 1,277 characters.
- Both used 530 prompt tokens. These were first-pass probes, without surface retries.
- Source labels were subsequently made more explicit; those two outputs precede
  that final label clarification.
- An initial offline-client probe omitted the configured thinking preference and
  exhausted 700 reasoning tokens twice. It is excluded from the configured-role
  comparison. Future probes must preserve the router's exact settings.

The other installed models tested in the earlier session timed out; those attempts
do not establish their quality or a replacement route.

**Do not assume that adding context guarantees success on the incumbent, or that
these narrow packages settle the broader context strategy.** Build and inspect the
intended package, hold voice/form stable, then evaluate interpretation and cost.

Editorial review remains offline and evidence-linked. A passed schema, keyword
check, or length check does not mean a card is factually sound or worth reading.

## 11. Code and evidence map

### Product and provider boundaries

- `rust/src/junctions/form.rs`: shared surface and output adapters.
- `rust/src/junctions/*/prompt.rs`: character perspective and prompt versions.
- `rust/src/harness.rs`: bounded retry for incomplete/over-surface output.
- `rust/src/ollama.rs`, `rust/src/openai.rs`: provider completion handling.
- `rust/src/guards.rs`: remaining mechanical/non-editorial checks.

### Identity, refresh, and extraction

- `rust/src/corpus.rs`: `load_identity_record`, `load_identity_card`, identity
  versioning, and shared corpus utilities.
- `rust/src/bin/factsweep.rs`: person role/affiliation refresh.
- `rust/src/junctions/investigator/entity.rs`: acquisition and career/current
  affiliation distinction.
- `rust/src/junctions/insider/verification.rs`: transfer extraction context.
- `rust/src/junctions/graph/`, `rust/src/junctions/editor/`: candidate and
  article/claim context.

### Writer context and memory

- `rust/src/junctions/scout/mod.rs`, `inputs.rs`, `tests.rs`: profile loader,
  optional numeric evidence, samples/dates, matched prior comparisons.
- `rust/src/junctions/journalist/mod.rs`: `load_entity_memory` and prior filing
  loaders; `inputs.rs` renders the current sections.
- `rust/src/junctions/insider/mod.rs`: pair memory, prior wraps, transfer board.
- `rust/src/junctions/influencer/`, `analyst/`, `oracle/`: inspect actual
  context assembly rather than assuming all memories reach every character.
- `rust/src/eval_tasks.rs`: live request builders and mechanical/archived checks.

### SQL

- `sql/migrations/252_rating_measurement_identity.sql`: measurement-aware functions
  and compatibility adapters; explicit source/rate semantics.
- `sql/migrations/253_rating_evidence_contract.sql`: true percentile contract,
  stored measurement/eligibility, full derived rebuild and checks.
- `sql/tests/253_rating_evidence_contract.sql`: temporary-table regression cases.
- `sql/schema/schema.sql`: snapshot for discovery, not a substitute for live
  function definitions when writing a function-replacement migration.
- `sql/migrations/250_football_clock_league.sql`: reporting-calendar decision.
- `sql/README-migrations.md`: migration derivation, recording, and snapshot rules.

### Retained evidence and reports

- `progress_docs/2026-09-13-card-canvas-identity.md`
- `progress_docs/2026-09-13-editorial-quality-investigation.md`
- `progress_docs/2026-09-14-rating-evidence-contract.md`
- `run_docs/experiments/2026-09-13-editorial-quality.json`
- `run_docs/experiments/2026-09-14-rating-evidence.json`
- `run_docs/experiments/2026-09-14-editorial-recheck.json`

## 12. Operational handoff and commit boundary

Production release is still gated on context/editorial validation. This planning
request authorizes documentation and a local commit, not a push or deployment.

When a release is explicitly approved:

- Follow the normal runbook. Pause affected writers for the migration/binary
  sequence; old Scout Rust treated null ranks as zero.
- Apply 252 then 253 before the corresponding Rust release.
- Budget for the atomic derived-stat rebuild and its checks.
- Refresh the live schema snapshot only after applying migrations.
- Verify fresh outputs and metadata refresh, then explicitly schedule any needed
  regeneration through existing backfill mechanisms.
- Do not rewrite historical generated prose inside the rating migration.

From `/rust`, local verification commands are:

    cargo fmt --check
    cargo test --all-targets
    cargo clippy --all-targets -- -D warnings
    git diff --check

Database diagnostics in the preceding session used the configured `archbox` SSH
path with credentials loaded remotely, never printed. Do not put credentials in
this plan, artifacts, or command output. SQL replay used temporary tables/functions
and rollback, not production function replacement followed by rollback.

The session commit intentionally excludes pre-existing unrelated user work:

- `docs/articulator-slice-endpoint.md`
- `go/internal/api/handler/articulator.go`
- `go/internal/api/server.go`
- `go/internal/api/server_test.go`
- `go/internal/articulator/composer.go`
- `go/cmd/articulator-compose/`
- `go/internal/articulator/player_test.go`

Those may still be dirty when the next session begins. Preserve them.
Inspect `git status` and the latest commit rather than assuming a clean worktree.

**Next-session opening task:** assemble and inspect the three context packages in
section 8, then agree the smallest shared contract that carries identity, sporting
time, meaningful memories, and present evidence without conflating their authority.
