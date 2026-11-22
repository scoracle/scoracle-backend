# Plugin skills live qualification — 2026-11-15

**Verdict: not qualified for release.** The existing runners completed all 79 Classifier requests and 52 Character requests without transport errors. Four Character suites failed mechanical checks, and Classifier scores disagreed with 13 supplied criterion labels at the replay's 50% threshold. A subsequent [baseline comparison](BASELINE-COMPARISON.md) reproduced every failure: no fixture-level regression was found.

Branch: `codex/character-skills`. Tested commit: `1c4573c9026dde37203be7f4d574b9f7209645ff`.

## Architecture comparison

[Editable Figma comparison: old plugins and new plugin-owned skills](https://www.figma.com/board/Ja3MlbUwEDby5GpRkx9GVs)

Previously, Classifier and each of the six character jobs were separate active plugins. Now the harness owns execution guarantees, plugins own capability families, and skills own individual jobs:

- **Classifier:** classify evidence and determine downstream work.
- **Character:** Journalist (narratives), Influencer (vibe), Scout (rating), Insider (transfer), Analyst (momentum), and Oracle (synthesis).
- **Caretaker:** organizes the existing Investigator and fixture-boxscore machinery; zero enabled handlers.

Skills remain independently scheduled. This grouping does not introduce a serial chain or a new model choosing skills. Durable task IDs, evidence preparation, output validation, atomic writes and follow-up events remain in the existing flow. Rating and vibe can feed momentum; Oracle assembles the character pillars.

## Live results

Classifier used the existing local tunnel on port 18081, reporting `vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707`. Character used the existing local server on port 8081 with `Qwen3.5-4B-Q4_K_M`, temperature zero and a 10,000-token requested context. Model and server metadata are saved alongside the logs.

| Classifier fixture set | Requests | Supplied labels | Disagreements at 50% |
| --- | ---: | ---: | ---: |
| Relations | 28 | 112 | 12 |
| Availability | 12 | 24 | 1 |
| Structured | 15 | 0 | Not scored |
| Team gender | 24 | 18 | 0 |

The 154 labels are criterion-level judgments, not 154 independent articles. Production thresholds and deterministic identity gates differ from this replay threshold. For example, the senior/junior identity collision scored incorrectly but was correctly refused by the identity gate. Structured fixtures have no supplied labels, so completion is not an accuracy result.

| Character skill | Fixtures | Model calls | Mechanical checks passed |
| --- | ---: | ---: | ---: |
| Journalist | 16 | 15 | 47 / 83 |
| Influencer | 6 | 6 | 6 / 6 |
| Scout | 11 | 11 | 27 / 29 |
| Insider | 9 | 9 | 52 / 73 |
| Analyst | 8 | 7 | 15 / 17 |
| Oracle | 4 | 4 | 16 / 16 |

Two fixtures correctly required no model call. Check counts measure individual assertions, not successful outputs or overall semantic quality. Influencer checks are effectively parser-only; Oracle checks are also limited mechanical assertions. Neither passing suite establishes full editorial quality.

## Follow-up priorities

1. **Separate overstrict guards from unsupported output.** Journalist rejected source-supported scoring details (including the 84th-minute winner), while other rejections correctly caught unsupported names. Analyst also rejected numerical references. Scout rejected wording that explicitly distinguished an unknown sample from a policy minimum; inspect that guard before changing prompts.
2. **Keep database metadata out of reported facts.** Journalist's `synthetic-discussion-no-new-event` included “Canadian defender” despite fixture exclusions. Scout's thin-sample response generalized stored coverage into games played, and its guard correctly rejected it.
3. **Review Insider extraction.** Existing positive controls produced missing findings or evidence-quote validation failures, including fully named interest and advanced talks.
4. **Review Classifier attribution and temporal relevance.** Disagreements include speaker versus subject emotion, vague revelations, stream headlines and historical injury treated as current availability.

No prompts, guards, fixtures or production code were tuned during this run. The earlier offline and database checks support the structural refactor; this live run exposes quality work still required before release.

## Evidence and boundaries

- [Machine-readable results](summary.json): per-case outcomes, checks and classifier mismatches.
- [Run manifest](run-manifest.json): tested commit, exact runner arguments, fixture hashes, concurrency and exit codes.
- `character-*.stdout` and `classifier-*.stdout`: fixture output and receipts; corresponding stderr files preserve runner summaries/errors.
- `*-models.json` and `*-props.json`: server-reported model configuration.
- `interrupted/`: partial initial run stopped by a turn interruption, excluded from final totals. The complete suites were restarted.

Built existing runners with `cargo build --offline --bin eval --example classifier_replay`. Character used fixture mode with an intentionally unreachable database URL; no database access occurred. The existing servers were not started, reconfigured or deployed. Caretaker stayed inactive.

No judge model, production database replay or production load qualification was run. The subsequent baseline replay is documented in [the comparison report](BASELINE-COMPARISON.md). Rejected Character responses are retained only to the extent the existing runner prints them, sometimes as excerpts. Results and this report are retained qualification artifacts.
