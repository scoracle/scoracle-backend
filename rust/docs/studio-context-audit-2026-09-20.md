# Studio context audit: Scout

Source audit and local model controls, 2026-09-20. Scope: the complete Scout assignment path and the shared memory/provider/rewrite machinery it uses. This is not a completed audit of every other character's application adapter or deployed database state. No database configuration was available in the current environment or the previously used local configuration locations. No database or deployment was changed.

## Principal finding: absence has different meanings upstream

The FPL importer (`../go/internal/dataimport/fpl.go`, live elements and explanation loops) only retains nonzero numeric fields. A measured zero can disappear before storage. The football season aggregator (`football.aggregate_player_season` in `../sql/schema/schema.sql`) sums keys actually present in event JSON. An all-zero action can therefore remain absent from the season JSON.

Two paths then interpret that same sparse representation differently:

1. `rating_measurements` restores absent goals and assists to zero with `COALESCE(..., 0)`. Several other measures use conditional source-shape checks; some NFL composites also default absent components to zero.
2. `src/evidence/memories/performance.sql` reads `stats->key` directly. If a key occurs in one season but not the other, the missing side becomes JSON null. The Rust memory renderer describes it as unknown/unavailable for comparison. If neither season has the key, it is omitted entirely.

Example supported by these code paths: a current FPL season with zero goals can reach the rating block as Goalscoring 0, while a raw comparison against a prior season with goals treats the current goals value as missing. This is a confirmed representational inconsistency in source, not a verified deployed entity or proof that it caused a specific generated sentence.

Do not globally remove COALESCE or globally turn missing into zero. Sparse sources can deliberately omit measured zeros; unavailable source fields also occur. Observation status must originate with the provider/ingestion contract and survive aggregation. The same typed evidence should feed both memory comparisons and the rating block, while retaining which values are measured, derived, reconstructed from a documented sparse-source convention, or unknown.

## Two reproduced renderer defects fixed

- **Invented units:** `render_model_performance_comparison` called every value a “total,” discarding SQL's `stat_definitions.unit`. It now retains the unit and only presents a like-for-like pair when both nonempty units match. Missing/incompatible units are explicitly unavailable for comparison. Single-snapshot rendering now retains units too. Averages and percentages no longer become totals.
- **Lost identity facts:** `render_model_data` split the entire identity text at `; observed `. The identity loader appends dated source facts after that point, so the renderer discarded those facts as well as the date. It now preserves the text and dated facts, including playing status.

Both new regression tests failed before the fixes and passed afterward. Tests cover a measured zero, three unit types, incompatible/missing units, and multiline identity facts. Shared memory version advanced to `memories-v4` so changed rendering invalidates material hashes. Renamed the Scout argument/local variable from `identity` to `memory_context`: it carries the whole memory package, not only the identity card.

## Complete Scout context inventory

| Input/stage | Source and selection | Audit finding |
|---|---|---|
| System instruction | `src/studio/scout/brief.rs`, `src/studio/form.rs` | s50 contribution language, universal partial-evidence/zero/unknown rules, metadata context, card/schema requirements. No old strengths/weaknesses outline remains. |
| Entity header | Request name/sport/type, rating-row position and season | Position is duplicated in memory identity and can cue unsupported role stereotypes. Keep identity as contextual information; tests demonstrate sensitivity, not a reason to erase useful metadata. |
| Current rating profile | `src/application/scout/evidence.rs`, stored rating breakdown and modes | Preserves optional numeric values; eligibility clears ranks rather than fabricating rank zero. Source SQL may already have reconstructed zeros. A Rust `Option` cannot recover their original provenance. |
| Statistical definitions | SQL `rating_measurements`, `rating_measurements_team` | Football has explicit underlying measures for several labels. NBA and NFL often set `measure` equal to the editorial label (`Scoring`, `Rim Protection`, etc.), losing raw-key/formula identity. A nonempty measure string does not guarantee an explicit definition. |
| Evidence selection | Scout filters and `model_prompt_profile` | Drops off-facet, display-only and near-average ranked zero artifacts; caps evidence and selects comparisons. Absence from the prompt is not necessarily source absence. No per-measure observation/omission status is currently sent. |
| Units in main block | `src/studio/scout/inputs.rs` | Blanket NBA-per-game/other-sports-total framing remains. It should eventually be replaced by per-measure metadata, especially for derived formulas, rates and adjustments. Memory's explicit unit fix does not fix this separate contract. |
| Cross-season ranks | `build_skill_changes`, prompt comparisons | Requires same league and measure; compares percentiles, not ability. Prior profile lookup independently chooses the richest previous-year row, so a same-league comparison can be missed. This does not manufacture a direction but can cause inconsistent coverage. |
| Memory identity | Canonical projection, active entity facts, relationships, statistical affiliations | Required group; multiple observations, dates, source notes. Renderer truncation fixed. Affiliation qualifications can still demand discussion of unresolved identity; review against each voice's scope rather than letting metadata become the story. |
| Nearby fixtures | Verified schedule SQL | Required group; selection is explicitly not a participation count. Can consume memory budget ahead of performance evidence. |
| Raw performance memory | `performance.sql`, `performance.rs`, memory renderer | A second statistics path with a curated raw-key allowlist. Uses same competition for its two snapshots but can choose an older prior season than the rank comparison. Per-90 calculation requires known positive minutes. Zero/missing inconsistency above remains upstream. SQL NULL league equality can also omit a prior NULL-league row. |
| DuckDB cohort context | `cohort.sql` over `analytics_entity_context` | Season-to-season composite movement and peer-delta distribution. This is separate from recent event form. Selects latest five rows across competitions up to the requested season, without filtering to the rating profile's selected league. Context labels competition, but records can differ from the main profile. |
| Recent form | `load_rating_trajectory` over PostgreSQL event ratings | Recent overall event-rating slope, 3–16 events; prompt gets label and sample size, not the full series/dates. Query spans the entity's sport/season without restricting league. Sparse results retain internal key `steady` but have no label, so Scout receives unavailable, not steady. |
| Thin-sample memory | `current_snapshot_view` | Removes historical/cohort comparisons and raw measurement detail; main rating block remains. Threshold is based on stored participation, not verified complete coverage. |
| Personnel/availability | Adjudicated records since prior read | Confirmed, returned and withdrawn are distinguished. Header still broadly says dates are effective dates although some renderer dates are application/observation dates. This needs a separate date-semantics correction. |
| Attributed reporting | News packets and successful Editor key-fact extraction | Sources and contradiction markers retained. These are upstream extracted claims, not direct raw-article quotations. A fabricated number can originate before Scout; source/article lineage must be checked when auditing a real case. |
| Prior generated Scout prose | Memory source selector | Scout does not receive a previous generated Scout reading through this path. Other missions can receive explicitly marked editorial memory. No evidence of Scout self-reinforcement through old Scout prose here. |
| Memory budget | `within_bytes`, default 4800 bytes | Retains required groups then whole optional groups; includes omission notice. Budget covers memory audit rendering, not complete system + all evidence + retries. Small fixtures do not establish that full enriched requests fit. |
| Enrichment toggle | `build_rating_request` | Disables explicit personnel/reports/rank comparisons/form, but still loads memory including raw history/cohort when participation permits. “Without enrichment” is not a clean stats-only ablation. |
| Output validation | `RatingRequestParser` | Checks selected directions/bands and certain unsupported claims/numbers. Number guard accepts any literal found anywhere in the prompt, including dates, IDs, samples or another measure. It does not establish measure-to-value provenance or qualitative truth. |
| Retry context | `src/studio/session.rs` | Appends correction messages to original evidence; requests main finding plus one detail and 500 characters. Error messages can repeat rejected phrases/numbers. Previous full answer is not appended. Our direct-provider controls bypass this machinery. |
| Provider wire | `src/runtime/providers/ollama.rs` | Exactly system and user messages plus schema/options, explicit context size, thinking setting. No hidden Rust few-shot examples or extra conversation. Previously captured Granite model metadata has no system field; provider template is model-native. |

## Explicit-status model controls

Local `granite4.2:3b`, thinking false, temperature 0, context 4096, output limit 700. Synthetic Noah Reed fixture; same s50 system for story runs. Replaced two stat rows with JSON observation records carrying separate `status` and `value`, plus an explicit zero-versus-null explanation. Retained the rest of the fixture. Exact requests, results and diagnostic runner are in `logs/studio-context-audit-20260920/`.

| Task | Chance-creation observation | Result |
|---|---|---|
| Literal extraction | status unmeasured, value null | Correct: measured false. |
| Literal extraction | status measured, value 0 | Correct: measured true. Previously failed without explicit status. |
| Literal extraction | status measured, value 1.5 | Correct: measured true. |
| Scout narrative | status unmeasured, value null | Acknowledged unknown creation and unavailable trend, but invented precise low-risk play, consistency and positional discipline. |
| Scout narrative | status measured, value 0 | Stated zero expected assists yet claimed consistent chance creation and a steady baseline. |

All runs completed normally. This is one run per condition, not a statistical evaluation or proof of general reliability. No production observation format was changed on the strength of these trials.

The previous run set had no memory package or imported database data. Therefore the renderer and ingestion defects cannot explain those synthetic failures: the model also adds unsupported interpretation to clean limited inputs. Conversely, prompt-only trials cannot rule out upstream context defects in real assignments.

## Next causal isolation

1. Preserve observation status and source coverage at ingestion/aggregation, including documented sparse-zero reconstruction. Reconcile rating and raw-memory interpretations of the same measure before further prose tuning.
2. Capture a real assignment plus its source rows under read-only inspection. Compare selected league, season, value, unit, status, polarity and observation date across every block; current environment lacks database configuration.
3. Run repeatable ablations on that frozen assignment: core measurements; add metadata; add raw history; add cohort; add reporting. The existing enrichment flag cannot perform these independent ablations.
4. Require provenance for claims, not mere presence of a numeric literal. First assess evidence-reading accuracy independently; then assess whether narrative interpretation stays within the supplied actions/outcomes without inventing technique, role or causality.

Validation for the changes in this audit: 502 library tests and 14 eval tests passed; 57 database-dependent tests ignored. All-target Clippy with warnings denied, formatting and diff whitespace checks passed. See the accompanying run artifacts for model outputs. Code checks are not a semantic quality pass.

## Follow-up: legacy creation labels across the FPL transition

The checked-in SQL explicitly preserves old category names across different source measures. Migration 247 introduced vendor-first/FPL-fallback selection; migration 252 retained the labels while adding underlying measurement identity and removing missing Shooting/Chance Creation zero fallbacks. Current schema mappings are:

| Category | Earlier/vendor source | FPL source |
|---|---|---|
| Player Creation | assists | assists |
| Player Chance Creation | key_passes | expected_assists |
| Player Shooting | shots_on_target | expected_goals |
| Team Creation | big_chances_created | assists when the row is FPL-shaped |

FPL's `creativity` field is imported separately by `fplFlatStatKey`; these creation mappings do not use it. Thus the presence of “Creation” does identify surviving category vocabulary, but does not establish that a generated claim came from stale old-model values. Rust consumes stored rating breakdowns assembled under these definitions. The current legacy `rating_datapoints` functions are adapters to `rating_measurements`, not independent alternative calculators.

There is an explicit source priority risk: a row carrying both non-null `key_passes` and `expected_assists` selects key passes, even if its value is zero. Migration 247 assumes era-homogeneous season rows. Determining whether that assumption holds requires source-row and deployed-function inspection. The same category cannot safely stand in for identical evidence across eras; the current Rust percentile comparison checks underlying measure identity as well as label and league.

The synthetic Noah tests explicitly supplied `Chance Creation: unmeasured (expected assists)` without SQL or any z-score values. Therefore stale SQL values cannot explain those particular invented claims. Legacy label semantics remain a plausible narrative cue and should be isolated by a label-only ablation against the precise source name, with every value/status and all other context fixed. This follow-up did not run that ablation or change rating definitions.
