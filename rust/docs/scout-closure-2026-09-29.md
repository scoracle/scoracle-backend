# Scout S1–S5 closure evidence

The ownership target is **harness → plugin → cognition model**, with the plugin
owning DB selection, assembly, acceptance and publication. Harvester is a peer
plugin using Laya. There is no separate System 1 layer.

The plan revision was committed as `895b18d7`. S1–S4 and the S5 plumbing deletion
are implemented locally. **The S5 product gate is open. Nothing is deployed.**

## Implementation and retained dependencies

- Source-triggered Scout uses Harvester's current performance score or an applied,
  source-linked roster/availability record, plus independent entity resolution.
  Missing support produces a durable unused reason. No Scout classification LLM
  remains; the only model job is measured-profile articulation.
- Publication rechecks and locks the source receipt, publisher identity, entity
  link and applied-record eligibility. A source changed during articulation cannot
  publish. Existing identity/availability and statistical-update work remains the
  owner of later rating opportunities.
- Removed the unused palette, generated headline, duplicate comparison/band maps,
  flat material formatters, body validation duplication and inert slot registry.
  Deterministic title generation, admission and evidence guards remain. Existing
  coverage guards now inspect the actual JSON profile, not retired prompt phrases.
- The completed world and retained provenance determine the fingerprint. Regression
  checks change only measured memory, coverage, a profile limit or source revision;
  each changes the hash. Identical assembled inputs retain it.
- Evaluation/capture carries prepared parts and production options. Runtime fixture
  checks reject missing parts and package drift. Journalist derives report count
  from the prepared reports. Scout runs its request-specific parser. No-call
  dispositions skip the backend; null, malformed and rejected replies stay distinct.
  Version-1 Scout captures without parts require recapture.
- Scout still reads `evidence::memories` for historical-season policy and provenance.
  Raw personnel/availability/report records are retained in provenance rather than
  converted into unused prose. This loader is not dead code. Rolling fixture-window
  bounds are still time inputs, so a different bound can change the fingerprint.

No new dependencies, service, plugin trait, judge, classifier or prompt framework
were added. Harvester calibration and Journalist's manual are unchanged.

## Mechanical verification

- `cargo test --offline --all-targets`: 555 passed, 70 ignored across all targets
  (538 library tests; 14 evaluation-binary tests). Ignored tests are not counted as
  passing evidence.
- Isolated PostgreSQL 17, built from the checked-in schema through migration 286:
  four Harvester/delivery integration tests passed, plus the applied-identity and
  rating-fanout test. These cover deterministic source dispositions, wrong entity,
  direct missing-profile preparation, roster/availability records, delivery gates,
  source mutation during Scout articulation, source-world parity, repeated-claim
  fencing and publication.
  The stale-source check verifies that no stat summary is inserted.
- `cargo clippy --offline --all-targets` completes with existing warnings in
  unmodified mechanisms. The strict `-D warnings` run is not green; no repository-wide
  lint cleanup is included in this work.
- Byte-order, fixture reassembly, missing-part rejection, multi-report parser
  context, unsupported Scout numbers, malformed/declined output and backend-free
  no-call behavior have runnable regression checks.

The source integration test uses a fake backend that accepts only the measured
world and body schema; an eligibility prompt fails the test. The source replay
below uses that exact measured world, captured before test cleanup. This is
synthetic isolated-DB evidence, not a production-data canary.

## Local model evidence

Model: `alibayram/smollm3` (installed digest
`6463ebe1ce6ae2f6369498f54f136baad56b4380e5365bfecc938079f8e6733c`).
Ollama, thinking off, 4,096 context, temperature 0; Scout output budget 700.
No model comparison or tuning campaign was performed.

[Replay inputs](../fixtures/scout/closure-s62-inputs.jsonl) contain eight explicit
synthetic worlds and the source-triggered world.
[Results](../fixtures/scout/closure-s62-results.jsonl) retain the captures, actual
requests, available raw provider replies, parser errors and attempts. Reproduce with
`eval --task rating --replay-assignment fixtures/scout/closure-s62-inputs.jsonl`.

| World | Model calls | Retries | Request bytes, including system | Final mechanical result |
| --- | ---: | ---: | ---: | --- |
| Strong/comparable | 3 | 2 | 4,434–4,841 | Rejected: body ceiling |
| Sparse | 2 | 1 | 4,450–4,625 | Parser accepted; semantic failure |
| Composite only | 3 | 2 | 4,166–4,573 | Rejected: incomplete output |
| Missing profile | 0 | 0 | 0 | Correct no-call |
| Zero versus unknown | 3 | 2 | 4,343–4,693 | Rejected: incomplete output |
| Incompatible comparison | 3 | 2 | 4,494–4,958 | Rejected: body ceiling |
| Measured memory | 3 | 2 | 4,733–5,140 | Rejected: body ceiling |
| Withdrawn/disputed report | 1 | 0 | 4,689 | Parser accepted; semantic failure |
| Source trigger | 3 | 2 | 4,409–4,952 | Rejected: unsupported stability |

That is 21 calls, including 13 retries across eight callable worlds. Thirteen
completed replies report latency of 2.990–12.326 seconds per call. Eight incomplete
provider outputs were returned as errors: their requests and errors are retained,
but that provider path did not expose raw bodies, tokens or latency to this capture.
Those measurements are unavailable, not zero. The recorder now also measures wall
time on errors for subsequent runs; the retained run predates that addition.

Manual inspection found concrete failures even in the two accepted replies:

- **Sparse:** three stored games became “their first three games of the 2026 season”
  and “a promising start.” The input explicitly says thin source coverage; it does
  not establish early-season participation or future potential.
- **Withdrawn report:** the model denied a prior comparison despite supplied prior
  percentiles, claimed strong recent form with no trend, inferred a capable
  playmaker from team aggregates, and omitted the report's publisher/date. It did
  retain some availability uncertainty. Passing dimensions/numeric containment did
  not make the read faithful.
- Other replies exceeded the body ceiling or exhausted the output budget. The
  source-triggered read still invented cross-time stability after correction.

The [seven inherited Scout quality cases](../fixtures/scout/closure-s62-quality.txt)
also failed on all six callable cases; one had no measured profile and correctly
skipped inference. These fixtures contain pre-existing conversion defects, including
numeric strings in measurement identities and unknown-sample limits beside known
sample counts. They are not clean product evidence. The explicit synthetic controls
above isolate failures without relying on those malformed worlds. Those damaged cases are now retired from the active suite; their original
sources and failures remain below.

The [current Influencer v3 replay](../fixtures/scout/closure-v3-influencer.txt) made
four calls, no retries: 0/4 passed its paragraph contract. Request sizes were
1,281–1,392 bytes; completed call latencies were 3.70–5.61 seconds. The CLI retains
only excerpts of rejected outputs, so this run establishes surface failure, not a
complete factual audit of those four replies. Do not restore its removed reaction
classifier or add a correction pipeline to make the gate appear green.

The [Journalist multi-report replay](../fixtures/scout/closure-n95-journalist.txt)
made one call, no retries: 933 request bytes, 293 input tokens, 92 output tokens,
2.80 seconds. All three reports stayed separate, with the supplied injury/unknown
return, appointment/contract and match result; 4/4 mechanical checks passed. The
appointment's Monday and result's Sunday were omitted, so this is evidence for the
affected keyed parsing path, not a new comprehensive fidelity certification. The
retained n95 history replay remains the broader evidence for that plugin.

## September 30 follow-up — s63 / Influencer v4, still open

Starting point: branch `codex/harvester-cutover`, commit `585859fa`. The user's
clarified target is recorded in the existing [plan](PLAN-plugin-alignment-2026-09-27.md):
plugins supply data, explicit relationships, and task/tone instructions. The model
owns synthesis. Tone describes writing qualities, not a character, backstory or
motive to invent. The target is not a claim that voice explains every failure.

### Demonstrated causes and narrow changes

- **Voice mixed tone with the job.** Scout's 1,384-character voice assigned a role
  and repeatedly requested a sporting interpretation of contributions. It is now
  “Observant, direct, specific to the sport and restrained.” Influencer's tone is
  likewise qualities only. Role-play and duplicated Scout instructions are deleted;
  factual boundaries stay in the plugin manual. Character product names remain.
- **Comparison assembly omitted a computed relationship.** `profile_parts` joined
  compatible prior percentiles; `Parts::comparison_directions` computed directions
  for the request parser, while the world sent just the percentiles and the manual
  prohibited model arithmetic. Each serialized measurement now carries the same
  computed `standing_change` that acceptance uses. It is derived from the stored
  percentiles, not a second editable direction or narrative conclusion.
- **Eligibility was easy to confuse with an actual comparison.** The manual now
  distinguishes `supports_cross_season` (sample eligibility) from a supplied prior
  percentile (an actual comparison). No new comparisons are fabricated.
- **The composite lacked its scale in the supplied data.** A draft manual supplied
  its peer mean, then the exact-request numeric guard rejected a reply using that
  number because it was absent from the world. The final world supplies
  `composite_peer_mean` only when a composite exists. The manual explains the field;
  numeric acceptance remains unchanged. The failed intermediate requests and
  corrections are retained in the [pre-scale replay](../fixtures/scout/s5-pre-scale-results.jsonl).
- **Instruction scope could demand more than the data supported.** Scout's job is
  now to describe the measured profile and its limits, with partial descriptions
  allowed. Influencer's job no longer demands emotional charge from routine news;
  feelings must be reported. These are plugin-owned instructions. Their replay is
  still failing, so these changes are not a claimed product cure.

The exact flow inspected was Scout adapter selection and `profile_parts` →
`Parts::render` and `generation_options` → Ollama's chat request →
`RatingRequestParser` with prepared directions/bands → existing Studio bounded
correction → plugin product construction. Influencer uses `assemble` → its own
options → `VibeParser`, without a correction pass. Ollama sends the requested
schema, explicit context/output budgets and `think:false`; completion reasons are
checked before acceptance. No provider, model route, budget, correction mechanic,
publication transaction or shared form implementation changed. A narrow Scout
correction formatting bug was fixed: `{error}` hid a contextual provider failure
behind “model generate”; `{error:#}` preserves the underlying incomplete-output
reason. This uses the same two-retry path, with a regression check for wrapped errors.

### Controlled comparisons, with full requests and replies

All controls retain the exact request and raw provider response, including
incomplete output. Local SmolLM3 digest is the same as the s62 run above;
temperature 0, context 4,096, thinking off, Scout budget 700 and Influencer 600
unless the named control changes that one setting. Model outputs can vary at
zero temperature. These are small development controls, not a statistical study.

| Retained comparison | Variable and finding |
| --- | --- |
| [Length control](../fixtures/scout/s5-controlled-length.jsonl) | Four Scout and four Influencer worlds, baseline versus an explicit compactness instruction. Some replies shorten, but sparse Scout invents a player role and composite-only invents scores. All four Influencer paragraph failures remain. |
| [Voice control](../fixtures/scout/s5-controlled-voice.jsonl) | Same eight baseline worlds, only the encoded voice string changed. Scout still says “first three games”; composite-only still becomes points per game. Influencer invents reactions and a successful offseason. Tone alone is not a sufficient fix. |
| [Manual control](../fixtures/scout/s5-controlled-manual.jsonl) | The tone-only request plus the draft plugin manual, otherwise unchanged. Scout stops some recent-form claims but still misreads coverage/cohorts and omits report qualifications. Influencer still invents sentiment. |
| [Model/thinking controls](../fixtures/scout/s5-controlled-model.jsonl) | Sparse, withdrawn, positive and quiet worlds: explicit `/no_think` on the baseline, or only the model changed to installed `ministral-3:3b`. Neither control yields faithful product evidence. No model is promoted. |
| [Budget/coverage controls](../fixtures/scout/s5-controlled-budget-coverage.jsonl) | Doubling only the strong world's output budget to 1,400 produces the same overlong, unsupported reply. Removing the sparse sample's duplicate `Games Played` field to match production's coverage presentation still produces unsupported strong form. |

Input token counts in the length/voice/model controls are 299–1,065, well below
the 4,096 context. Raising a generation ceiling cannot correct a completed false
claim. The long composite replies repeat and elaborate unsupported material;
truncation is not evidence that another output budget would make them faithful.
The tested requests expose an unresolved instruction-following/fidelity problem;
these controls do not prove that every model or every possible prompt will fail.

### Retired fixture defects and retained failures

[Original and pre-conversion sources](../fixtures/scout/s5-fixture-repair-sources.jsonl) retain all seven inherited Scout fixtures. The attempted repair restored numeric values, named identities, known sample limits and supplied quality z-scores without changing expectations or review criteria. Lost units, identity context and unsupported trend sample counts prevented a complete repair. Those seven cases are now removed from the active suite rather than presented as valid product gates.

The active suite contains eight coherent prepared worlds from the current production replay below. Their review criteria preserve scope, unknowns, comparisons and attribution; they do not prescribe wording or claim the outputs pass. The missing-profile no-call case remains in assignment replay and deterministic tests.

[Full fixture requests, replies and parser diagnostics](../fixtures/scout/s5-fixture-results.jsonl) retain the attempted repair replay: six Scout calls, one no-call and four Influencer calls, without corrections. Four Scout replies parsed but introduced unsupported claims. All original failed responses remain; acceptance annotations are consolidated into this result file.

Influencer's four complete replies all fail the 140-character paragraph limit.
Full text now establishes factual failures too: negative news gains a lack of
effort, demands for accountability and future scrutiny; positive news gains a
milestone and a community narrative; the quiet schedule gains a successful season
and management confidence; historical celebration becomes current excitement,
and a Tuesday training date becomes the announcement date. There is no classifier,
judge, phrase guard or new correction pass to hide these failures.

### Current production replay and checks

The [current inputs](../fixtures/scout/s5-production-inputs.jsonl) re-render the
same eight callable s62 worlds plus the missing-profile case through current
production parts/options. The [current results](../fixtures/scout/s5-production-results.jsonl)
use the existing `eval --task rating --replay-assignment` path, including the
request-specific parser and bounded correction. This reuses the retained
synthetic DB-source world; it is not a new live-DB or production-data canary.

The full current-world run made **17 calls, including 9 retries** across eight
callable worlds. Five replies passed the production parser, three were rejected,
and the missing-profile world correctly made no call. Seven incomplete provider
outputs retain their request/error/wall time but no raw response through the
provider error path; the separate HTTP controls retain their complete raw bodies.

| World | Calls | Mechanical result | Manual review |
| --- | ---: | --- | --- |
| Strong/comparable | 1 | Accepted | Invents assist decline despite `held`, denies the supplied comparison, and asserts no injuries/suspensions from absent reports. |
| Sparse | 3 | Rejected | Confuses cohort with sample; correction invents a previous-season history and keeps treating stored coverage as participation. |
| Composite only | 2 | Accepted after body correction | Invents physical presence, a 25-game trend threshold and cross-season scope for the peer mean. |
| Missing profile | 0 | No call | Correct unavailable behavior; no prose to grade. |
| Zero versus unknown | 3 | Rejected | All attempts incomplete; no accepted product. |
| Incompatible comparison | 3 | Accepted after incomplete attempts | Keeps no comparison but mislabels the peer mean as cross-season and turns coverage into actual participation. |
| Measured memory | 3 | Rejected | Invents 107 points by adding the supplied change to the already-current 105; other attempts incomplete. |
| Withdrawn/disputed report | 1 | Accepted | Calls unchanged assist standing a decline; omits publisher/date/disputed status, despite retaining unconfirmed availability. |
| Source trigger | 1 | Accepted | Turns the appearance minimum into a 10-point threshold and narrates tone as a property of the team. |

**None of those five parser acceptances is a faithful product pass.** Numeric
containment still cannot prove correct association, scope, negation or fidelity.
No new phrase guard is added to make these particular examples appear fixed.
The [correction-only replay](../fixtures/scout/s5-correction-results.jsonl) reruns
the three worlds whose attempts hit provider incompleteness, after preserving the
underlying error message. It made eight calls (five retries): zero-versus-unknown parsed after one
correction but falsely called low turnovers defensive strength; incompatible
comparison exhausted all three attempts; measured memory still invented 107
points after correction and was rejected. The error is now correctly conveyed,
but the correction is not a fidelity cure.

Reproduce the current-world run with the existing evaluation binary and the
recorded local model/temperature/context options: `eval --task rating
--replay-assignment fixtures/scout/s5-production-inputs.jsonl`. Captures retain
prepared parts and serialized requests. Controlled rows retain the exact `.request`
to POST to local Ollama's `/api/chat`. The one-off exporter and duplicate input
exports have been removed; the existing evaluation path remains the replay entry.

Verification is local and separate from product status:

- `cargo test --offline --all-targets`: **557 passed, 70 ignored, 0 failed**,
  including 540 library tests and 14 evaluation-binary tests. The two added
  regressions cover attached comparison directions/composite scale and preserving
  provider errors beneath context. Existing role-assignment snapshot assertions
  were updated to the new tone and job; factual test expectations were retained.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- Before retirement, the eleven repaired rating/vibe fixtures retained their
  original expectations and review criteria, checked against `585859fa`.
- [Check record](../fixtures/scout/s5-checks.txt) retains commands and counts.
  Clippy and isolated DB integration checks were not rerun in this follow-up. The five
isolated DB checks at the top of this file are retained starting-point evidence,
not rerun results for this follow-up. No DB selection, publication fencing or
Harvester code changed. Journalist's manual, voice, forms and history evidence
remain untouched; no new Journalist fidelity certification is claimed.

## Legacy cleanup

Removed 18 superseded Studio, palette and handoff documents, the standalone palette probe, seven damaged active Scout fixtures, duplicate S5 exports and the one-off parts exporter. The alignment plan is the design authority; this file is the S5 evidence index. Full failed captures, original fixture sources, Harvester calibration and Journalist history evidence remain.

The Scout rate-standout selector now has one function instead of a forwarding wrapper. Stale implementation-history comments are shortened. Shared legacy runtime paths with remaining consumers stay in the plan's retirement ledger until those consumers migrate; this cleanup does not begin Insider.

All eight replacement Scout fixtures exactly match the retained production parts and serialized prompts. Local Markdown links in the edited documents resolve. Cleanup verification: `cargo test --offline --all-targets` passed **557 tests, 70 ignored**, across 15 targets; formatting and diff checks passed. Product acceptance is unchanged; no model replay or deployment is claimed by this pruning pass.

## Remaining gate

S5 remains open. Do not begin Insider. Obtain faithful, useful outputs
from the same representative worlds before closing product verification. Keep the
tone-only target and factual boundaries; do not add classifiers, judges, palettes,
speculative keyword guards, another correction pipeline or a framework. Harvester
calibration and Journalist's accepted prose/history evidence are unchanged. No
plugin is deployed by this work.

## September 30 transport follow-up — confirmed framing defect, no product cure

The next S5 investigation resolved the template question left open in the
Journalist transport notes. No production code, plugin instructions, model
installation, routing or service configuration changed.

Local Ollama **0.32.14** still serves the same SmolLM3 digest recorded above.
Its server log selects `gguf_chat_template`; the running inference server's
`/props` returns the same Jinja template shown by `/api/show`. Crucially,
`/apply-template` with the captured messages and `enable_thinking:false` returns
an actual rendered prompt whose system turn has **no `<|im_end|>` before the
user turn**. The closing marker in the installed template sits inside its tools
conditional. This confirms the missing boundary for these tool-free requests,
rather than merely inferring it from template text. The diagnostic endpoint is
[documented by llama.cpp](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md#post-apply-template-apply-chat-template-to-a-conversation).

[Complete probe records](../fixtures/scout/s5-template-probe.jsonl) retain four
worlds, each under three conditions:

1. The current production chat request, without retries.
2. Raw generation using the exact prompt returned by the running renderer.
3. The same raw request with only `<|im_end|>\n` inserted before the user turn.

The **two raw conditions** isolate the delimiter. Chat versus raw also changes
the endpoint and is only a transport reference, not a single-variable comparison.
All conditions retain the same model, schema, plugin instructions, evidence,
temperature 0, 4,096-token context and original 700/600-token Scout/Influencer
budgets. The runner supplies the same September 29 local metadata date within
each pair. No new inference, correction, judge or prose repair stage was added.

All **12 calls completed**, with no retries or incomplete responses. Production
parsers accept the withdrawn-report reply in each condition (**3/12**). Manual
factual review passes **0/12**; restoring the delimiter yields **0/4** product
passes. Total measured call wall time was 44,288 ms. One run per condition is a
small diagnostic, not a statistical quality estimate; temperature zero does not
guarantee identical output.

| World | Result with the delimiter restored |
| --- | --- |
| Sparse Scout | Still turns stored coverage into actual games played, invents offensive/defensive presence, and makes the 10-appearance comparison minimum an elite-rebounding threshold. Production parser rejects it. |
| Withdrawn report | Preserves held assist standing but omits publisher/date/disputed status. Parser accepts it; product review fails. The later instruction trace identifies ambiguity in what the input's withdrawal flag qualifies, so uncertainty about the withdrawal alone is not clean evidence of model invention. |
| Positive Influencer | Invents a stadium, community pride, dedication and future expectations; turns publication date into win date. Its single paragraph has 523 characters, exceeding 140. |
| Quiet Influencer | Invents a successful prior season, eager fans, management confidence and expected player performance. Its single paragraph has 566 characters, exceeding 140. |

Verification rebuilt all four active fixtures through `prepare_fixture`, checked
prompt versions and asserted that every retained request's plugin content,
schema and options match the current production request builder. Each response
was then checked with the existing prepared evaluation and plugin parser; exact
guard errors are attached to the records. These offline checks passed; their
successful execution does not mean the model replies passed. No full Rust or DB
suite was rerun because this follow-up changes only evidence and documentation.

The records are directly replayable: POST any row's `request` as JSON to its
`endpoint`. `template_request` records the renderer input and `rendered_prompt`
its exact response. To independently check the controlled difference without
making any model calls:

```python
import json
from pathlib import Path
rows = [json.loads(line) for line in Path("fixtures/scout/s5-template-probe.jsonl").read_text().splitlines()]
assert len(rows) == 12
for key in {row["key"] for row in rows}:
    variants = {row["variant"]: row for row in rows if row["key"] == key}
    before = variants["raw_missing_end"]["request"].copy()
    after = variants["raw_closed_system"]["request"].copy()
    prompt = before.pop("prompt")
    boundary = prompt.index("<|im_start|>user\n")
    assert "<|im_end|>" not in prompt[:boundary]
    assert after.pop("prompt") == prompt[:boundary] + "<|im_end|>\n" + prompt[boundary:]
    assert before == after
```

The framing defect is real; this experiment does **not** establish its causal
contribution to hallucination, and fixing it alone is insufficient. S5 remains
open. Further work must demonstrate faithful synthesis on the existing prepared
worlds before changing production behavior or starting Insider. Harvester and
Journalist's accepted behavior remain unchanged; nothing was deployed.

## September 30 instruction trace — establishing a working control

The user's observation changes the investigation: reproduce SmolLM3's useful
conversational behavior, then identify which task requirements break it. Failed
publication prompts alone do not establish that the model cannot use the evidence.
This follow-up makes no production code or model changes.

### Actual request path

Scout and Influencer construct their own manuals, worlds and schemas. Studio
sends them unchanged on the first attempt. The route governor only limits
concurrency. Ollama receives the manual as a system message and the serialized
world as a user message; with thinking disabled, the provider adds no writing
instruction. Scout corrections append another instruction only after a correctable
failure. Influencer has no correction pass. The observed first-answer inventions
therefore do not require a hidden shared writing prompt or retry to explain them.

The shared legacy composer remains live for Analyst, Oracle and Insider scoring,
not Scout, Influencer or Journalist. It adds character scope, a coherent read,
a hook/main observation and other writing obligations. Oracle additionally asks
for “one living pattern.” Those are relevant pressures to audit in their windows;
this experiment does not claim to have replayed those plugins.

### What was compared

[All 35 calls](../fixtures/scout/s5-instruction-trace.jsonl) retain exact requests,
complete HTTP replies, completion status and wall time. These are diagnostic
controls, not a new active fixture suite. Model and budgets remain those of the
previous probe. No retries, judges, response repair or model swaps were used.
Thirty-three calls completed; two long Scout-manual controls exhausted their
700-token budget, with their full partial replies retained.

- `factorial`: the withdrawn-report and quiet-news worlds, crossing the original
  manual versus a short question, system versus user placement, and schema versus
  unconstrained decoding (16 calls). Removing a custom system message also restores
  the model template's default system text, so placement includes that difference.
- `conversation`: three plain summary/unknown questions, with bare source text or
  the complete quiet-news world. An open “How are the fans feeling?” question still
  invites speculation, even while admitting the source does not state feelings.
- `separation`: four worlds with voice/form relocated into writing instructions,
  using the original or a shorter manual (8 calls). This bundled framing change
  reduces some elaboration but is not a demonstrated product fix.
- `minimal`: four bounded restatement/unknown controls. These deliberately narrow
  the job to establish basic evidence use; they do not stand in for synthesis.
- `task_boundary`: two worlds with a concrete task appended to the original user
  message, or replacing only the system manual (4 calls).

### Demonstrated findings

**A working baseline exists on the same local model.** Asked whether the quiet
report states fans' feelings, it says the report does not provide that information.
Asked to restate its source sentence, it returns the supplied schedule without
adding a season narrative. This also works with the existing JSON schema and the
entire original world, including voice and form.

**One added instruction changes the failing quiet-news request into a faithful
reply.** The original request produces a 757-character paragraph with invented
successful-season, fan and management claims. Keeping its manual, model, schema,
options and world bytes, then appending:

> Restate the publisher_excerpt in one short sentence without adding any information.

produces:

> Cedar Comets announced training starts on Tuesday and the team travels on Friday.

That is 81 characters. The same instruction replacing only the system manual also
produces that sentence. Four narrow JSON controls pass the existing Influencer
parser, verified with `influencer_replay --validate`; manual inspection finds no
added facts. These are four variants of **one quiet-news case**, not four diverse
product passes. They establish instruction sensitivity, not a universal cure.

**Writing controls become asserted facts.** With the same world under a short
question, answers describe the *team's* voice as emotionally attentive or restrained.
The full manual moved into the user turn yields “Memories.rs history shows no prior
announcements.” Neither is source evidence. An empty selected history is not proof
that no earlier announcements exist. Voice/form and implementation explanations
are adjacent to evidence, and the model demonstrably confuses their roles. Moving
them alone is insufficient; a narrow task succeeds even with them still present.

**Scout's assignment contains competing pressures and ambiguous data.** Its
manual asks for useful findings “without repeating the stat lines,” then prohibits
arithmetic and unsupported ability, role, tactical, trend and causal interpretations.
This pressures the model away from the very statements it can safely make; it is
a plausible contributor, not an isolated causal result from these controls.
Shortening the manual does not cure Scout: it still confuses composite score with
points, percentiles with top percentages, and report scope.

The withdrawal input also needs repair before grading that subproblem. The headline
says the club withdrew an earlier report, while `withdrawn:true` and `disputed:true`
are attached to the object containing that headline. The target of those flags is
not explicit. This pattern exists in `Reported::from_records`, which supplies a
retraction sentence plus a withdrawal flag; it is not solely a fixture concern.
The prior review also incorrectly treated attribution of withdrawal to the club
as invented, despite that attribution appearing in the headline. The earlier
probe now retains that assessment as `superseded_manual_review` and corrects it.
Other failures, such as changing held assist standing into decline, remain clear.

### Consequence for the next change

Start with a concrete task over clearly identified evidence, then add the required
history, comparison and expressive scope one at a time against the same controls.
Keep writing instructions distinguishable from source facts. Make the target of
retractions and measurement scope explicit in assembly rather than requiring the
model to untangle them. A concise supported description must be an acceptable
outcome when that is all the evidence supports.

The successful restatement is a diagnostic starting point, **not a replacement for
the user's requested synthesis**. No general shared prompt wrapper, keyword guard,
new classifier or correction pipeline follows from these findings. S5 remains open;
its cause is not established as model incapacity. Existing product limits remain.

## September 30 six-part instruction cleanup

Implemented the first bounded instruction-ownership cleanup as Scout `s64` and
Influencer `vibe-frame-v5`. Both plugins now source tone from `voice.rs`; the old
Scout `brief.rs` and Influencer `influencer.rs` modules and imports are removed.
Prompt modules own the task and explain that voice/form are writing instructions,
not subject evidence. Scout no longer asks for findings while discouraging
repetition of the stat lines. A short supported description is explicitly valid.
Influencer's manual names the supplied evidence, preserves dated context and
attribution, and makes empty history a selection gap rather than proof of absence.
It does not replace synthesis with the diagnostic one-sentence rewrite.

Shared observation form now describes paragraph separation and existing limits,
without editorial instructions about observations, sentence style or conclusions.
Its only production consumer is Influencer; Journalist's accepted keyed form and
Harvester's four-part subset are unchanged. Existing factual guards, nullable
output, structural parsers, correction boundaries and product dimensions remain.
The test pinning Scout manual phrases is deleted; behavioral checks remain. Active
fixture versions and Influencer's rendered form were refreshed without changing
source/history data, factual expectations or manual review criteria.

Verification: `cargo test --offline --lib` passed 539 tests (69 ignored), and
`cargo check --offline --all-targets` passed. All twelve active Scout/Influencer
fixtures reassemble through their production preparation paths. The temporary
request exporter/validator is removed after capture; no new tooling framework is
introduced.

The [retained first-call replay](../fixtures/scout/s5-six-part-results.jsonl)
contains exact current requests, complete raw provider responses (including any
truncated model output), production-parser checks and manual findings. It uses
local `alibayram/smollm3`, `think:false`, temperature 0 and context 4096 with the
existing plugin schemas and output budgets. These are diagnostic first calls;
Scout's bounded correction path is not exercised in this capture. Fixture/parser
checks are mechanical evidence, not a factual verdict.

Result: 11 provider completions and one length-truncated reply; one of eight Scout
replies passes the production parser, and none of four Influencer replies does.
All twelve fail manual fidelity review. The accepted Scout composite reply turns
voice into a balanced, disciplined team approach. Other replies invent tactics,
deny supplied comparisons or measured memory, and mistake a sample threshold for
points. Influencer's quiet update retains the schedule but adds an unsupported
claim that no further details were provided. Its positive case still invents
analyst praise and community pride; the history case narrates voice and form.
All four Influencer replies are single paragraphs over 140 characters. These
results do not demonstrate a synthesis cure or establish model incapacity.

Two legacy parser diagnostics also need their own follow-up: `exact` triggers
`contains("xa")` in the measure-association guard, and the zero/unknown reply is
rejected as calling turnovers average despite explicitly calling them elite.
These diagnostics are not accepted as factual grading. The replies have separate
manual failures, recorded above. This change preserves those guards rather than
silently weakening them to improve a replay score; audit their actual matching
and correction effects as part of the remaining cleanup.

Next assembly work: distinguish measurement coverage from actual participation,
make the target of a withdrawal explicit, and separate source evidence from
writing controls in the prepared package. Scout's fresh preparation has not yet
been extracted into the target module layout. Shared legacy composition still has
live Insider/Analyst/Oracle callers and is not deleted prematurely. Harvester and
Journalist are unchanged; Insider remains gated on S5 product fidelity.

## September 30 input-path audit and tool-access reconsideration

The user requested a granular trace after continuing hallucinations, then asked
that these findings be saved while reconsidering model-directed database tools.
This section records the current local s64 / vibe-frame-v5 implementation, not a
new production deployment. No causal cure has been established.

### Verification boundary correction

The twelve-case replay loads synthetic, already-prepared `Parts`. It exercises
plugin assembly/rendering, request construction, inference and production parsers.
It does **not** execute PostgreSQL acquisition, DuckDB research or the earlier
profile-selection stages. The earlier statement that fixtures reassemble through
production preparation paths must be read with this limitation. Passing library
checks is not evidence that the live database path works.

For example, the sparse fixture includes `sample: {"Games Played": 3}`, while
live thin-profile preparation removes that participation label. Do not attribute
all synthetic-fixture behavior to the fully prepared live path. The retained
[requests and responses](../fixtures/scout/s5-six-part-results.jsonl) remain useful
inference diagnostics, not end-to-end acceptance evidence.

### Actual model boundary

PostgreSQL reads → plugin selection/transformation → optional DuckDB research →
ordered JSON rendering → Studio → Ollama `/api/chat` → installed chat template →
model output → Rust parser and optional correction.

The model has **no callable database/research tools** in these paths. Six modules
do not mean six messages or six tools. Both plugins send exactly one system
message (`prompt.rs::TASK`) and one user message containing compact JSON, with
writing controls adjacent to evidence. No previous conversation, repository
instructions or full provenance package is automatically sent.

Both requests use `stream:false` and a `format` schema requiring a single nullable
string `body`, forbidding extra properties. The schema contains no character
limits or factual constraints. Limits are text in the user message and Rust
validation after generation. The replay uses `alibayram/smollm3`, `think:false`,
4096 context and temperature 0. Scout's production temperature constant is 0.6;
Influencer's is 0.0. Output budgets are respectively 700 and 600 tokens. With
SmolLM3 thinking enabled, the client additionally prepends a reasoning instruction;
that branch was not active in these replays.

Sources: [Ollama client](../src/runtime/providers/ollama.rs),
[renderer](../src/plugins/assembly.rs), [Studio loop](../src/studio/session.rs).
The full literal system instructions and user JSON are retained per request in
the replay, and in [Scout prompt](../src/plugins/scout/prompt.rs) and
[Influencer prompt](../src/plugins/influencer/prompt.rs).

The earlier installed-template capture adds knowledge-cutoff/date/reasoning
metadata and a Custom Instructions wrapper. It lacks the system-closing delimiter
before the user turn for these tool-free requests. Its captured date is September
29, not a newly checked current date. Restoring the delimiter alone did not cure
the tested failures; see the retained template probe. Tools would exercise a
different template branch, so that needs independent verification.

### Influencer source-to-field inventory

The adapter consumes the oldest pending Harvester source. Delivery joins
`harvester_assignments`, `harvester_classifications` and `news_articles`; validates
body SHA-256, title, excerpt byte range, and current-receipt attribution/date.
Harvester context is the exact first three source paragraphs, not a generated
summary or necessarily the complete article. Qualifications later in an article
can therefore be outside the delivered source span.

Admission declines empty text, text over 6000 bytes, known dates outside the
72-hour fresh window or in the future, and certain explicit override phrases.
Unknown publication time uses now only for admission; the model still receives
null and no dated history can be loaded.

Wire order and complete field inventory:

| Part | Fields and source |
| --- | --- |
| `identity` | `name` from canonical players/teams; `entity_type` from work identity; `sport` maps NBA to basketball and NFL to American football. Entity ID is omitted. |
| `fresh` | `publisher`, `published_at` (UTC or null), `publisher_excerpt`: verified publisher attribution, publication time and exact retained context. |
| `history[]` | `publisher`, `published_at`, `reported_headline`: selected earlier source titles. `group` is omitted for this plugin. |
| `voice` | Literal `Emotionally attentive, clear, concise and restrained.` |
| `form.body` | `type:"string or null"`, `paragraphs:"Blank lines separate paragraphs."`, `paragraph_max_chars:140`, `max_chars:1200`, `lengths:"Ceilings, not targets; no minimum length."`, `paragraph_breaks:"escaped newlines"`. |

History research anchors seven days before the fresh publication timestamp.
Candidate articles come through `narrative_events` with origin extraction and
subject/object linkage. Source title must match the canonical name or a surface;
publication and candidate event dates are window-bounded. Current article and its
duplicate family are excluded. The PostgreSQL snapshot supplies article/canonical
IDs, publisher, time and title to the Go DuckDB helper. It deduplicates canonical
articles and selects bounded findings. Influencer presents at most two headlines
within 1200 serialized bytes; no historical article bodies are presented.
Research receipts, IDs, candidate counts, window bounds and reasons for an empty
selection do not accompany the model-facing history list.

The fresh article title is not in the model request. After generation it becomes
the product heading if it fits 140 characters; otherwise the entity name is used.
Sentiment stays None. Influencer's Studio correction callback always returns None.

Sources: [delivery](../src/plugins/harvester/delivery.rs),
[adapter](../src/plugins/influencer/mod.rs),
[assembly](../src/plugins/influencer/prompt.rs),
[memory policy](../src/plugins/influencer/memories.rs),
[shared research](../src/plugins/memories.rs),
[research SQL](../src/plugins/memories/reporting.sql),
[DuckDB implementation](../../go/internal/analytics/duckdb/memory.go).

### Scout source-to-field inventory

There are two paths. Normal ratings enable enrichment. Harvester-triggered work
first checks resolved identity and an admitted performance signal or an applied,
source-linked personnel/availability record, then builds with enrichment and
storyline history disabled. The triggering article authorizes the run and stays
in provenance; its text is **not** sent to Scout. `fresh` means statistical profile
here. Event-rating trajectory is still loaded and may appear despite disabled
enrichment; the switch does not disable all historical computation.

The profile loader reads `player_stats` / `team_stats`: requested/latest season,
unscoped row preferred, otherwise the richest league breakdown. It reads stored
rating score, breakdown, scoped ranks, player rate modes and sample statistics
named through `stat_definitions`. `updated_at::date` becomes `observed_at`.
Player rank eligibility is rechecked using `rating_thresholds`; ineligible ranks
and composite are cleared. The model does not calculate the original rating.

Selection removes opposite-facet NFL stats, zero values with near-average signed
z, and display-only stats. Cross-season eligibility needs ten recognized
appearances. Thin profiles lose composite and Discipline, retain at most two
selected measures, and hide participation labels from sample. With comparisons,
selection keeps up to four largest standing changes plus two additional held
standings. Bands are Rust labels at percentile cutoffs 90/75/60/50/35. Direction
is rose for delta >1, fell for delta <-1, otherwise held.

Wire order and complete field inventory:

| Part | Fields and source |
| --- | --- |
| `identity` | Compact canonical name/type/sport; no entity ID. |
| `fresh` | `season`, `observed_at`, optional `sport_name` from sports display name; `sample`; selected `values`; `composite`; `supports_cross_season`; optional `not_selected`, `limit`; `composite_peer_mean:50` when composite exists. |
| `fresh.values[]` | `label`, `measure`, nullable `value` and `percentile`; optional `cohort`, `band`, signed `quality_z`, `prior_percentile`, computed `standing_change`. Prior match requires same league, label and measurement identity. |
| `fresh.limit` | `kind`: thin_sample, unknown_sample, one_appearance, withheld_identity or no_measurements, with applicable appearances/minimum. |
| `memory.measured[]` | `measure_label`, `unit`, `from`, `before`, `fixtures`, `measured`, optional `per_match`, `per_match_change`, `percent_change`, and `fixture_ids`. |
| `memory.reported[]` | `publisher`, `published_at`, `reported_headline`, optional `withdrawn`, `disputed`. |
| `memory.coverage_limits` | Code-authored qualifications, potentially including study-error text. |
| `rate_standouts[]` | `mode`, `label`, `percentile`, from stored player rate modes; up to five per mode at percentile >=80. Underlying measurement identity is not included in this rendered type. |
| `trend` | `direction`, `sample_size`, optional `note`, or null. |
| `voice` | Literal `Observant, direct, specific to the sport and restrained.` |
| `form` | `keys:["body"]`, `max_chars:1200`, `paragraph_max_chars:null`. |

Measured memory studies one registered additive team measure over two adjacent
30-day windows. PostgreSQL reads fixtures and event_team_stats, selected team,
competition and season, completed/seeded and not needing verification. DuckDB
computes the window results. Rendered `from` starts the previous window, but
fixtures/measured/per_match describe the current window; the split and complete
previous-window values are absent from the presented object.

Reported records come from transfer_identity_applications and player_availability.
Code authors sentences with publisher `Adjudicated record`; published_at can be a
formatted record-change date, not a publisher timestamp. Additional contested
claims use accepted Harvester publisher contexts when configured; the legacy
branch still uses news packets and Editor-generated key_facts. Only marked
contested claims from that additional reporting are appended.

Trend is independently computed from event_box_scores/event_team_stats ratings
ordered by fixture date. The recent window is 10% of scored events bounded to
3–16; a fitted slope above .25 is rising, below -.25 falling, otherwise steady.

The broad evidence::memories package is still loaded and retained as provenance,
with preparation decisions such as historical-season gating. It is not inserted
wholesale into the model's memory part. Prior sample/date/season in SkillChange,
full rows, source receipts, all current reporting, trigger article and notability
are also not presented wholesale. Stored input_components is not the wire input.

Sources: [adapter](../src/plugins/scout/adapter/mod.rs),
[profile SQL](../src/plugins/scout/performance.rs),
[selection/parser](../src/plugins/scout/cognition/mod.rs),
[presentation](../src/plugins/scout/prompt.rs),
[memory selection](../src/plugins/scout/memories.rs),
[statistics research](../src/plugins/memories/statistic.rs),
[reporting branches](../src/evidence/personnel.rs).

### Findings to preserve for cleanup

1. Evidence, tone and form are adjacent JSON values; actual replies promote tone
   to subject facts. The model cannot request missing evidence with tools.
2. The six-part contract is not fully realized: Scout fresh means selected stats,
   source-trigger text is absent, and legacy research/provenance paths remain.
3. Prior-season dates and samples are loaded but dropped from the model-facing
   comparison; cohort size does not supply a full population definition.
4. Scout memory budgeting removes coverage_limits first, then reported claims.
   Thus qualifications can disappear before the evidence they qualify.
5. Two-window measured-memory presentation compresses distinct time scopes.
6. Retraction sentences and withdrawn/disputed flags leave the target ambiguous.
7. Legacy generated reporting is reachable depending on configuration; do not
   describe every Scout claim as an untouched publisher excerpt.
8. Scout SurfaceError/IncompleteOutput can cause up to two correction calls.
   Studio appends `Output correction: <error> Return the complete requested JSON
   within the supplied form limits. Preserve the supplied qualifications and use
   only the prepared evidence.` Previous rejected answers are not sent; appended
   corrections accumulate. False guard diagnoses can therefore become model
   instructions. `exact` matching `contains("xa")` is a concrete example.
9. The reporting.sql title predicate at line 29 is malformed as written:
   `strpos(' '||public.nrm(a.title)||' ')||' '||s.norm||' ')>0)`.
   This audit did not execute it against PostgreSQL. Synthetic replay bypasses
   the query; existing passing library tests do not validate this path.
10. The latest replay is a prepared-input diagnostic, not end-to-end validation.
    It cannot prove the DB tools or selection pipeline work, nor prove model
    incapacity. Keep request, provider completion, parser and manual review
    outcomes distinct.

These are code findings and observed failure modes, not proof of one universal
hallucination cause. No runtime behavior was changed in this audit.

### Architectural question raised by the user

Reconsider preassembling a large world. A plugin could instead provide the task,
canonical subject, tone/output requirements and only the research tools needed
for that task; the model requests evidence, the application executes the tools,
and results return as tool messages. Existing SQL/DuckDB work should be reused
behind small read operations. This is a proposed direction, not an implemented
migration or a demonstrated SmolLM3 capability.

[Ollama documents native tool calling and agent loops](https://docs.ollama.com/capabilities/tool-calling).
Our current client does not supply tools or dispatch tool calls. Support in Ollama
does not establish reliable tool selection/argument generation for this installed
SmolLM3 model/template. Verify a minimal real read, an unavailable-data result and
an answer grounded in the returned evidence before expanding the architecture.
Tool results still need clear scope, dates, units and source attribution; wrapping
the current oversized ambiguous package in a tool would preserve its problems.

## September 30 first model-directed tool slice

The user accepted lean six-part context plus minimal model-callable database tools.
Implemented a read-only Influencer pilot, not a worker/publication cutover:

- `Inference::chat` carries native messages and tool definitions. Ollama implements
  it using the existing client and completion checks; the shared GPU governor also
  gates these calls. Other providers fail explicitly rather than flattening tools
  into prose. No final-body grammar is applied to a tool-selection turn.
- Influencer starts with canonical subject identity plus a short task, tone and
  form instructions. It offers only `read_source`, with no model-supplied entity,
  SQL or article selection arguments. The subject comes from the application.
- Invocation reuses verified Harvester delivery and source admission. The result
  is attributed source text plus its article ID, or explicit unavailability.
  No article/history world is preloaded; no unused memory tool is scaffolded.
- One read and one final answer are permitted. Native undeclared calls, arguments,
  duplicate/excess reads, answers before reading, and factual bodies after an
  unavailable result are rejected. Existing body parsing remains. Structural
  acceptance is explicitly not a factual verdict. No phrase corrections are added.
- `examples/influencer_tools.rs` resolves the canonical subject through PostgreSQL,
  runs with read-only connections and a query timeout, and prints the transcript.
  It has no publication code. The old worker path remains until this is verified.

The installed `alibayram/smollm3` advertises tools, but the running renderer omits
native `tools` definitions: the embedded template expects `xml_tools` or
`python_tools`. A direct `/apply-template` capture confirms the missing schema.
A matched runner diagnostic supplying `xml_tools` produces the intended
`<tool_call>{"name":"read_source","arguments":{}}</tool_call>`, while the
baseline asks for the report instead. Neither produces native tool_calls; no
actual DB call occurred in this diagnostic. The result establishes schema
visibility matters for this tool task, not end-to-end tool reliability or a cure
for prior synthesis failures.

[Full paired tool-access evidence](../fixtures/scout/s5-tool-access-probe.jsonl)
retains both requests, raw responses and rendered prompts. A temporary local model
alias with a Go template did not establish a working native path and was removed;
the original model is unchanged. Do not add XML/code-block guessing to production
as a substitute for resolving the template and provider protocol.

Live database validation also awaits a connection: this shell has neither
DATABASE_URL nor DATABASE_PRIVATE_URL and the default local PostgreSQL socket did
not answer. The user was asked for the existing connection/config location. The
scripted protocol test covers source-before-answer, native result round-trip,
unavailable abstention, wrong-tool and wrong-argument refusal; it is not a live
SQL or model-quality test. S5 and production migration remain open.

### September 30 provider template follow-up

The active `/api/show` template, unlike the saved Go Modelfile, consumes
`xml_tools`/`python_tools`, renders assistant history from `content`, and renders
tool results as user turns. [SmolLM3's documented tool format](https://huggingface.co/blog/smollm3)
uses a JSON `name`/`arguments` object inside `<tool_call>` tags. Ollama's
[tool-call API](https://docs.ollama.com/capabilities/tool-calling) supplies native
`tools`, assistant `tool_calls`, and `tool` result messages. The provider now
bridges these exact contracts for the installed `alibayram/smollm3` tool chat: it places function schemas
in the system turn, uses the template's `/system_override` branch to close that
turn, normalizes only a complete standalone XML call, and retains exact raw
responses alongside the normalized assistant message. The plugin still rejects
undeclared names or arguments before executing any read.

The active runner's `/apply-template` output confirms schema visibility, a closed
system turn, assistant-call history and the returned tool-result turn. The
pilot-shaped Rust/model diagnostic nevertheless produced an invented answer
without calling `read_source`; the application rejected it as `source_not_read`.
The runner's direct `xml_tools` path also skipped the read on that same task,
so this failure is not explained by the provider's schema placement alone.
A single read-first diagnostic emitted `read_source` with a made-up URL argument;
the pilot would reject it before any read. Neither probe demonstrates an
acceptable tool request, so no output-instruction shuffle was adopted.
No database call or factual review occurred. Exact request, raw response,
normalized message and rendered prompts are retained in
[the provider probe](../fixtures/scout/s5-tool-provider-probe.jsonl). Its
history/result render uses labeled synthetic text solely to check mechanics.
An existing PostgreSQL configuration is still needed for the read-only pilot.
Validation after this patch: `cargo test --offline --lib` passed 541 tests with
69 ignored; `cargo check --offline --all-targets`, formatting and diff checks
passed. These checks do not establish live database behavior or factual quality.

### September 30 read-only Archbox pilot

The existing Archbox backend configuration was found at
`/home/sheneveld/scoracle/scoracle-backend/.env.local`. A temporary local-only
SSH tunnel supplied its PostgreSQL connection to the unchanged read-only pilot;
the password was not printed or stored in the repository. Metadata queries found
two pending Influencer assignments, both `delivery_held`, plus three already
used assignments. None was pending and eligible for a publisher-text read.

The pilot resolved NBA team 5 as the Chicago Bulls from PostgreSQL. Granite 4.2
3B made one native `read_source({})` call. The application executed the scoped
database read and returned `{"status":"unavailable","reason":"no_pending_source"}`.
The exhausted tool was omitted on the second turn; the existing observation
schema constrained only that final turn. Granite returned `{"body":null}` and
passed the structural pilot check. This validates the live unavailable-data loop,
not an available-source report or factual quality. Ministral 3B and 8B supplied
undeclared subject arguments and were rejected before reading. The preferred
SmolLM3 answered before reading and was rejected. Exact requests, raw responses,
tool result and acceptance outcomes are in
[the live DB probe](../fixtures/scout/s5-tool-live-db-probe.jsonl).

Two additional SmolLM3 model-only diagnostics in
[the provider probe](../fixtures/scout/s5-tool-provider-probe.jsonl) tested a
server-assigned no-argument tool name and removal of initial identity. The model
still invented arguments, so neither change was adopted. An available-source
tool round trip remains unverified until an eligible fresh assignment exists;
none was released or changed for this diagnostic.

### September 30 Granite historical-source check

No eligible fresh Influencer assignment appeared on a further read-only Archbox
check. For an evidence-to-answer diagnostic, two already-used Harvester receipts
(articles 96208 and 70146) were replayed without changing their status. Their
stored body hashes, UTF-8 context offsets, headlines and source identities were
verified before returning the stored excerpts. Granite 4.2 3B made a native
`read_source({})` call in both cases and received the corresponding PostgreSQL
evidence. These historical replays bypass pending/fresh admission, so they are
not live available-source pilot passes. Exact publisher-bearing transcripts are
retained privately in `/private/tmp/granite-used-source-replay.json` and
`/private/tmp/granite-used-source-replay-70146.json`.

Neither answer passed product review. The first produced a 558-character single
paragraph against a 140-character ceiling and included writing-process
commentary. The second exhausted its output budget before completing valid
JSON. A shorter form instruction was unstable and, on a successful tool call,
still produced a 330-character paragraph that treated the page byline date as
the event date. A `maxLength: 140` schema variant cut the answer mid-sentence.
Those variants were not adopted. The stored excerpts include site navigation
and affiliate text, and their page byline dates differ from
`news_articles.published_at`. Source presentation and date meaning need review
at the database/tool boundary before claiming factual quality. No worker or
database state was changed.

Validation for this slice: `cargo test --offline --lib` passed 541 tests with 69
ignored; `cargo check --offline --all-targets`, formatting and diff checks passed.
The unavailable-data database round trip worked. Native Granite calling also
worked with historical evidence, but the final answer did not pass product
review. SmolLM3 has not made a valid first read in this pilot. A live eligible
source and its factual answer remain open before any worker change.

### September 30 acquisition cost and garbled-output diagnosis

The user's next question is whether native tool calling is more efficient than
the plugin reading the database and supplying the result. The comparison uses
the same two historical receipts above, with identical evidence, task, voice,
final schema, 4096-token context and 600-token output budget. The prepared control
removes only the instruction to call `read_source` and supplies its result beside
the subject. It is a lean prepared request, not the legacy full-world worker.
The native route must produce a valid call before receiving that evidence.

Both receipts were re-read from Archbox and again verified against body hash,
context offsets, headline and source identity. They exactly matched the retained
evidence. Each remote psql read took about 8 ms; the whole SSH invocation took
233–240 ms. These are diagnostic process timings, not pooled application DB
latency. Timed model comparisons replay the verified result, so DB time is
excluded equally from both routes. No synthetic source, fresh-admission pass or
DuckDB research benchmark is claimed.

Granite 4.2 3B Q4_K_M was already resident in Ollama 0.32.14. The matched comparison
used three seeds (42–44), alternating route order, `think:false`, and IBM's stated
`temperature:1.0` / `top_p:0.95` sampling settings. The existing 600-token product
budget was retained. The initial native request is identical across these two
source cases for each seed; these are six attempts per route, not six independent
subjects. [IBM's model card](https://huggingface.co/ibm-granite/granite-4.2-3b)
specifies those sampling values; the earlier pilot forced temperature zero.

| Measure | Native read request | Plugin-prepared evidence |
| --- | --- | --- |
| Model calls when evidence is delivered | 2 | 1 |
| Input tokens for those attempts | 785–848 | 314–377 |
| Additional read-decision latency, median | 1.19 seconds | None |
| Evidence delivered | 4/6 attempts | 6/6 attempts |
| Production form parser passes | 0/6 | 1/6 |
| Manually acceptable reports | 0/6 | 0/6 |

The single prepared parser pass contains only an opening brace in `body`; it is
not useful prose. Two native attempts emit JSON tool intent as ordinary content,
so no evidence is delivered. For the same four pairs where native dispatch did
work, median model wall time was 5.24 seconds native versus 3.67 seconds prepared,
with 2.36 times as many input tokens. Different answer lengths and a native
truncation affect those wall times. The separate 1.19-second call-decision cost
is the clearer overhead measure; no throughput of acceptable reports can be
inferred when neither route passes product review.

The diagnosis narrows the failure without claiming a single proven cause:

- Ollama's `_debug_render_only` output shows complete source results and closed
  message boundaries in both routes. Final prompt token counts also match the
  runner's tokenizer. The evidence is present and comfortably within context.
- The malformed prose includes instructions about its own tone, attribution and
  length, format fragments inside the body string, and repetition until the
  output budget ends. These occur in both routes. JSON grammar constrains the
  envelope; it does not make its string useful or factual.
- Removing the stray `<tools>` marker from an earlier assistant call stops that
  particular runaway, but leaves instruction commentary. Recommended sampling
  also shortens one retained replay, but the matched runs still fail. Neither is
  a demonstrated cure.
- Keeping only the verbatim publisher sentence still yields repeated instruction
  commentary. Removing or simplifying the form specification also fails. Page
  clutter is a source-quality defect, but it does not explain all this garbling.
- Enabling thinking produces a separate reasoning field but still fails: the
  native final takes 473 tokens and 12.4 seconds with an overlong answer; the
  prepared final consumes all 600 tokens before any answer. This is a bounded
  diagnostic, not a claim about larger-budget reasoning.
- Date errors have an additional input cause: the byline and stored publication
  dates disagree, and the pilot task omits the existing production task's
  report-date/event-date distinction. Changing acquisition order supplies neither
  that missing relationship nor a resolution of the conflicting dates.

One provider defect is independently confirmed: Ollama drops
`additionalProperties:false` and the empty `required` list from the native tool
schema before rendering it. Its
[typed parameters structure](https://github.com/ollama/ollama/blob/v0.32.14/api/types.go#L438)
accounts for this. Application argument validation remains strict. This defect
does not explain final-answer failures in the prepared route.

For this mandatory, application-scoped source read, plugin preparation has the
lower cost and removes a failing tool-selection step. Native calling has not
saved a read or reduced evidence volume. Adaptive research might justify it when
the model can avoid or choose among actual reads; that remains unmeasured. The
six responsibilities and lean evidence remain applicable to either route.

The runnable comparison is `examples/influencer_tool_compare.py`; its protocol
check covers valid dispatch and refusal of undeclared arguments. The existing
Rust form parser was run on all 24 comparison/control outcomes: one mechanical
pass, zero manually acceptable answers. No production prompt, guard or worker
was changed in this diagnostic. The
[comparison index](../fixtures/scout/s5-tool-acquisition-comparison.jsonl) retains
metrics, manual findings and hashes/locations of all exact private transcripts,
including the render-only captures and database recheck.

## October 5 Scout fidelity verification — gate remains open

Started from `53e19cbf`, with the current s65 request and the existing Archbox
SmolLM3 model. Production services, model configuration and database products
were not changed. PostgreSQL captures used connection-level read-only settings.

- The eight current synthetic fixtures replayed through the existing evaluation
  path: 9/14 mechanical properties passed, but all eight replies failed manual
  fidelity review. Replies misstate held assists, invent strategy and stability,
  and present sample thresholds as observed counts.
- Four real subjects were captured with enrichment off and on: two NFL teams,
  a football team and an NFL player. All eight preparations succeeded; their
  current-snapshot memory fingerprints matched. The four NFL-team captures
  replayed through the production parser and bounded correction. An accepted
  Miami reply invented ten tackles per game from the appearance threshold.
  One final Cleveland correction accurately disclaimed stability but was
  rejected by a word-based guard. Parser acceptance remains separate from
  factual review.
- Three demonstrated parser matching defects were repaired with regression
  checks: `exact` no longer matches xA, `averaged` no longer matches the average
  band, and singular `assist` matches the Assists comparison. These repairs
  do not establish fidelity or make the guards semantic judges.
- Two task formulations were compared on all eight synthetic worlds, then with
  the installed template's closed-system branch on four diagnostic worlds.
  All 24 replies still failed fidelity. No production task or transport change
  was adopted. Exact requests and complete replies are retained in
  [the diagnostic capture](../fixtures/scout/s5-task-framing-2026-10-05.jsonl).

Measured-memory preparation has a concrete assembly defect: `from` uses the
previous window's start, `before` uses the current window's end, but counts and
`per_match` describe only the current window. The study already retains separate
previous/current bounds, counts and averages. The user subsequently approved exposing those two windows, while keeping
fixture IDs in provenance. The local s66 implementation is recorded below.
The real subjects captured here had no eligible measured-window study, so they
do not verify that branch.

Private captures remain in `/private/tmp/scout-live-capture.jsonl`,
`/private/tmp/scout-live-football-player.jsonl` and
`/private/tmp/scout-live-replay.txt`; they are not repository fixtures.

Verification: `cargo test --offline --all-targets --quiet` passed (468 library
checks, 62 ignored, plus other targets). Formatting and diff checks passed.
No deployment or successful product closure is claimed.

### Approved measured-window repair — local s66

The user approved the proposed window contents on October 5. Scout now presents
`previous` and `current`, each with its own UTC bounds, fixture count, measured
count and nullable average, copied from the existing study. Change remains the
study's current-minus-previous result. Fixture IDs stay in fingerprinted
provenance and no longer enter the model request. Regression checks cover the
window conversion, missing versus zero, changed bounds and averages, and lineage
changes that invalidate work without changing the model's evidence text.

The current eight fixtures use s66 and reassemble with the production renderer.
The measured-memory fixture is an explicitly reconstructed synthetic comparison:
August 1–30 average 103, August 30–September 29 average 105, three measured
fixtures out of four in each period. It is not a newly verified historical
receipt; the earlier request and failures remain in their retained captures.

[Eight-case first-call replay](../fixtures/scout/s66-quality.txt): 9/14 mechanical
properties pass; all eight replies still fail fidelity. The measured-memory reply
correctly distinguishes current 105 from previous 103 and retains the missing
fixture qualification, but exceeds 1,200 characters and invents a balanced
profile. A separate [complete wire capture](../fixtures/scout/s66-measured-window-results.jsonl)
passes the production parser while inventing ability improvement, top-30% league
standing from the composite, cross-season consistency, and subject traits from
tone. This is an assembly repair, not a closed fidelity gate.

A read-only capability query on Archbox finds 66 additive team stat definitions
for football and 51 for NFL, with none for NBA. Scout's existing selector requires
exactly one additive team measure per sport, so no live sport currently reaches
this measured-window branch. This remains an explicit live-verification limit;
no new metric-selection policy or registered measurements were introduced.
Database credentials remained on Archbox for this query after automatic approval
review rejected local credential persistence. No worker or database product changed.

Post-repair verification: `cargo test --offline --all-targets --quiet` passed
(469 library tests, 62 ignored, plus other targets); formatting and diff checks
passed. The next fidelity work remains comparison eligibility versus actual
change, unsupported synthesis and the surviving word-based guard diagnostics.

## October 5 user-directed alignment closure

Scout alignment is closed. The user explicitly directed this session to finish
plugin alignment rather than continue plugin tuning and to move on to the rest
of the plan. Local s66, the ownership/context repairs and passing mechanical
checks are retained. Failed fidelity replays, word-based guard limitations and
the unavailable live measured-window branch remain recorded; they are not passing
quality evidence or deployment approval. Further prose tuning is deferred and
no longer blocks Insider or later alignment windows. The next window is Insider.
