# Scout source identity at the producer — September 20

Follow-up to the [investigation handoff](studio-investigation-handoff-2026-09-20.md). The handoff's
"preserve meaning at the producer" work is implemented as migration
`sql/migrations/264_rating_source_identity.sql`, verified locally, and exercised on a player and a
team for each covered sport through the real preparation and creation paths. Work is local and
uncommitted; production was not deployed or modified. All production inspection was read-only.

## What was wrong (confirmed against deployed data)

The disposable read-only probes against the deployed database confirmed migration 253 stored
`measure` for every NBA and NFL player breakdown equal to its display label (`Rim Protection` →
`Rim Protection`), for both the total bundles and the rate-mode bundles, and for the NBA team
branch. Because `format_datapoint_evidence` suppresses measure text that duplicates the label,
the model received only the category name for every rated NBA/NFL entity. FOOTBALL's derived
formulas already carried explicit identities from 252; production rows show them
(`expected assists`, `possession-adjusted tackles`, `saves relative to league save percentage`).

## What migration 264 does

- Returns precise source measurement identity in `measure` for the NBA and NFL player branches and
  the NBA and NFL team branches of `rating_measurements`/`rating_measurements_team`. Examples: NBA
  Rim Protection → `blocks`, Playmaking → `assists`, Ball Security → `turnovers`, On-Court Impact →
  `plus-minus`, Foul Drawing → `field goals attempted`; NFL Air Yards Responsible → the explicit
  six-component sum, Tackles For Loss → `max of tackles for loss and defensive sacks`; team
  Touchdowns → `passing + rushing touchdowns`. Plain labels whose identity is already unambiguous
  (team `Steals`, `Points Allowed`) keep themselves; the renderer suppresses those duplicates by
  design and no meaning is lost.
- Rebuilds every NBA/NFL player and team bundle inside the transaction. FOOTBALL branches are
  byte-identical to 252, so football stored breakdowns are untouched.
- Proves numerical parity in-database: rating, rank, score, scoped ranks/scores and every breakdown
  member except `measure` must be identical before and after (temp-table snapshots, `IS DISTINCT
  FROM` comparisons, rate-mode breakdowns included). Percentile partitions are unchanged by
  construction because each label still maps to exactly one measure.
- Contract checks pin the named handoff examples (`blk`, `ast`, `turnover`, `plus_minus`), the NFL
  formula identities, and that no stored NBA/NFL player breakdown or rate mode echoes its label.

## Verification

- `sql/tests/253_rating_evidence_contract.sql` passes before and after 264.
- New `sql/tests/264_rating_source_identity.sql` exercises NBA/NFL player and team compute through
  temp-table copies: identities present, ineligible samples stay unranked, thin-sample behavior
  preserved, rate modes still require denominators, team identities persisted.
- Disposable baseline DB (local Postgres 17, `sql/build.sh` + `sql/migrate.sh`): 264 applies cleanly.
- Rust: 509 library tests pass; all 57 database tests pass (`TEST_DATABASE_URL` on the disposable
  migrated database, `--test-threads=1` — they share fixed-key fixtures and must not run in
  parallel). fmt and clippy (`--all-targets -D warnings`) clean.
- Go analytics tests pass. The DuckDB port pushes `rating_measurements` down to Postgres
  (`analytics.go` `postgres_query`), so the identity flows through without Go changes;
  `equivalence_test.go` compares measure strings across engines.
- The checked-in baseline (`sql/schema/`) remains at the 263 snapshot on purpose: baselines are
  captured from a deployed database, so 264 enters the baseline only after production applies it.

## Real-path per-sport test (disposable database seeded from production rows)

`rust/examples/scout_freeze.rs` freezes the exact prompt via `build_rating_request` (memory,
personnel, fixtures and cohort context included where the slice had data) and, with `-generate`,
creates the card through `scout::create` with the grounded request parser. The disposable database
was seeded read-only from production rows for six entities, with stored breakdowns re-measured to
the 264 identities (parity proven, so this is equivalent to the migration's rebuild). Scout ran on
`granite4.2:3b`, the production route (no `COGNITION_ROUTE_*` overrides in the deployed env).

| Entity | Prompt evidence | Card result |
|---|---|---|
| NBA player, Rim Protection pct 100 | `Rim Protection: 3.1 (blocks) … percentile 100.0 (elite); quality z +6.11` as a held anchor; Ball Security (turnovers, negative) and On-Court Impact (plus-minus) in comparisons | Grounded first try. Blocks interpreted correctly; no invented sequences, no superlative. |
| NBA team, rating rank 100 | `Rim Protection: 6.6 (blocks)`, `3PT Shooting: 10.9 (three-point field goals made)` with directions | Grounded; all numbers traceable; "ROSE metrics" phrasing slightly mechanical. |
| NFL player, thin sample | `Air Yards Responsible: 582 (sum of passing, receiving, kick-return, punt-return, punt and interception yards)`, `Points Responsible For: 54 (6 x touchdowns …)` | Grounded; explicitly declines cross-season inference. |
| NFL team, thin sample | `Penalty Yards For: 164 (penalty yards drawn)`, `Points Scored: 77` | Grounded numbers, but reads penalty yards drawn as "strong defensive discipline" — a facet misattribution the guards do not cover. |
| FOOTBALL player, thin sample | `Chance Creation: 1.41 (expected assists)`, `Goalscoring: 2 (goals)` | First attempt correctly REJECTED by the grounded parser (stability claim on a thin sample); retry landed grounded prose. |
| FOOTBALL team, thin sample | `Goals Against: 1`, `xG Against: 2.74` | Grounded, but "consistent pressure on the offensive line" stretches a downward overall-score trend into a mechanism. |

No pipeline or crash errors occurred in any of the six. One guard rejection fired as designed and
the retry path recovered. The residual overreach in two thin-sample cards matches the pressure
audit's conclusion: the identity fix removes the missing-definition failure mode (the rich,
full-cohort NBA cards were the cleanest), while soft mechanism phrasing on thin evidence remains a
model-quality matter, not an evidence-contract defect.

## Follow-up: sport guessing on the FOOTBALL id (same day)

The Arsenal thin-sample card invented "consistent pressure on the offensive line" — gridiron
terminology for an association-football team. The prepared prompt names the sport only by its raw
id: the header reads "Entity: Arsenal (FOOTBALL team)", the system prompt says "specific to the
sport" without naming it, and nothing in the assignment ever disambiguates FOOTBALL from American
football. The model was left to guess, and guessed across sports. `sports.display_name` already
curates the disambiguation ("Football (Soccer)", "NFL Football", "NBA Basketball") — the prompt
simply never rendered it.

Ablations (disposable seeded database, `granite4.2:3b`, five runs per arm, `-sport-label` ablation
flag in `examples/scout_freeze.rs`):

| Prompt | Gridiron idiom | Other failures |
|---|---|---|
| Baseline (raw `FOOTBALL` header) | 1 of 3 visible cards ("offensive line") | one card called Arsenal a "player"; one inverted the elite defensive read ("struggles to concede high-quality chances"); 2 runs guard-rejected |
| Header id replaced with "Football (Soccer)" | 0 of 4 | invented a "measured zero in offensive attempts"; invented an offensive trend; inverted a defensive read |
| Sport-id mapping sentence in the system prompt (shipped) | 0 of 12 | inverted reads and entity-type slips persist at 3B on the 2-datapoint thin profile |

Shipped fix: `CHARACTER` now states the mapping — "The assignment names its sport: NBA basketball,
NFL American football, FOOTBALL association football (soccer)." The brief stays inside its
200-word budget (188). `RATING_PROMPT_VERSION` bumped s54 → s55 because provenance must describe
the system prompt that produced a card; the seven rating fixtures' frozen `user_prompt` text is
unchanged (the user prompt was not touched), so their pins were updated to s55 rather than
recaptured. Post-fix Arsenal runs contain no cross-sport vocabulary; remaining thin-sample errors
(inverted quality readings, entity-type slips, invented zeros) are 3B model quality, consistent
with the pressure audit's expansion findings — the lexical thin-sample guard rejects benign
"consistent" phrasing while passing a genuinely inverted "measurable weakening in preventing
scoring", which is a separate guard-coverage gap.

Follow-up candidates, in priority order: render `sports.display_name` in the user prompt header so
the disambiguation is data-driven rather than embedded in the voice text (a prompt-contract change
requiring another version bump and fixture refresh); a band-contradiction guard for inverted
quality readings ("declining defensive performance" against an elite band); facet-claim checks for
team measures (the Bills' "defensive discipline" misattribution).

## Frozen prompts

Exact prompts and hashes for the six entities are reproducible:

```
DATABASE_PRIVATE_URL=<disposable db> OLLAMA_MODEL=granite4.2:3b \
  cargo run --example scout_freeze -- -sport NBA -entity-type player -entity-id 56677822 -season 2025
```

Input hashes are recorded per entity in the example output (`e4db…` NBA player, `8c9e…` NBA team,
`71db…` NFL player, `428e…` NFL team, `14dd…` FOOTBALL player, `b14a…` FOOTBALL team).
