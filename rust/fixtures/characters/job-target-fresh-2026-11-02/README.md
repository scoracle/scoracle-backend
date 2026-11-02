# Job → target → fresh evidence — November 2, 2026

All six character requests now lead with the requested work, explicitly introduce
the target entity, then present fresh evidence. This implements the user's
assembly direction. It is not a claim that the model's commentary is ready for
routine publication.

## The request the model receives

The system message contains the character's authoritative `instructions.txt`.
The user message is one JSON document in this order:

1. `job`: the opening paragraph taken directly from that same instruction file.
2. `meta`: a plain-language `target` introduction, canonical name/type/sport, and
   explicit `attributes` from database records.
3. `fresh`: the supplied reports, measurements or completed peer readings.
4. `tone`: writing style only.

The job is not independently authored in a second constant. Job and target
sections each start on a new line; the evidence remains compact to avoid the
cost of deeply indented JSON. Output schemas and publication parsers are unchanged.

The Journalist's opening is:

> Your task is to articulate what is being said about the target entity. Faithfully read the supplied headlines or excerpts and summarize up to three distinct stories circulating about it. Do not invent facts or fill gaps. Write in a confident, steady tone, like a veteran journalist.

The real Chelsea lookup produces:

> The target entity is Chelsea, a football team based in London. Its league on record is Premier League.

Chelsea's stored gender is missing, so this live introduction does not say men's.
The regression test using an explicitly populated men's category produces the
user's intended men's-team introduction. No demographic category is inferred or
written back to the database. [chelsea-request.json](chelsea-request.json) is a
readable preview of the complete request; its retained reporting is dated source
material, not newly fetched November news. No Chelsea generation was made.

## Explicit metadata and evidence ownership

Character loaders now read named fields directly: position, nationality, role,
league, location, conference/division, gender, and recorded affiliations. A
relationship keeps its available source and observation date. A coach record is
explicitly `coach_on_record`, not an assertion that the coach is newly appointed
or still current. The target introduction names the subject, not a neighboring
person from that background record.

Existing active identity facts retain their attribute names, typed values,
validity dates where present, recording dates and source-document IDs. Recording
time is not relabeled as an observation time. Missing or blank attributes are
omitted; they do not become zero, neutral, current, or a guessed value. These are
identity records already loaded by the prior path, not historical article or
generated-card injection.

The live metadata path uses `load_character_identity[_on]`. The classifier's
existing compact identity loader remains separate. Old saved character parts
containing a prose `database_record` can still deserialize for explicit replay;
their strings are retained rather than parsed into invented structured fields.
Live character captures now store attribute objects. Four synthetic metadata
fixtures were explicitly converted to structured fields.

Journalist, Influencer and Insider reports now place the publisher, date and
readable source text before long provenance fields. Every original source field
and value remains present. Insider gets explicit zero-based report indices while
retaining the existing quote-eligibility constraints. No report is clipped,
reclassified, rewritten or sent to an extra preparation model.

## Verification and paired replay

`character_capture --identity-only` reads both the legacy representation and
structured fields in a repeatable, read-only transaction for each subject. Eight
subjects supplied the comparison metadata; Chelsea is a separate preview capture.
All identity reads use production loader functions and make zero model calls.

Twelve paired cases cover two assignments per character, football/NBA/NFL, and
teams/players/people. Eleven use frozen real reporting, statistics or peer cards;
one Analyst comparison is explicitly synthetic. The frozen real evidence comes
from the earlier October 23 edition captures. It is not fresh November reporting
or an independent population sample.

`build_cases.py` verifies identical fresh evidence and tone in both arms, ignoring
only the new code-assigned Insider indices when comparing evidence objects.
Metadata is captured once per subject in both representations. The comparison
therefore changes instruction framing, metadata representation and source field
order together; it does not isolate the causal effect of one field.

Both arms use Qwen3.5-4B-Q4_K_M at the existing local endpoint, temperature zero,
thinking disabled, one call per assignment, no rewrites and alternating arm order.
Full-input token admission, completion checks and model identity checks remain
active. All 24 generations complete, with no clipping or context holds. The actual
request settings and output schemas are identical between each pair after removal
of their tokenized prompts.

| Measure | Previous assembly | Job/target/fresh assembly |
| --- | ---: | ---: |
| Generation calls | 12 | 12 |
| Mechanically accepted | 8 | 10 |
| Input tokens | 26,404 | 28,533 |
| Output tokens | 1,030 | 1,054 |
| Summed request seconds | 117.003 | 109.362 |

Total: **24 calls, 54,937 input tokens, 2,084 output tokens and 226.365 summed
request seconds**. The new framing adds about 8.1% input tokens in this set and
adds no generation stage. This shared-workstation run, with compilation occurring
alongside some calls, is not a throughput or speed benchmark.

## What improved and what did not

[review.json](review.json) contains unblinded source review for every pair.
Both arms have four usable, four minor-edit and four material-error judgments in
this small development set. The higher parser acceptance is not an overall
semantic-quality improvement.

- Scout now assigns Boston's measurements to Boston instead of to its comparison
  cohort. Its supplied standings and changes remain intact.
- Influencer's De Zerbi reading stays with reported delight and hope; the earlier
  generalized interpretation of leadership and momentum disappears.
- Roma loses the unrelated coach name, but the candidate converts injury-related
  absences into departures. This is a material regression despite parser acceptance.
- Analyst still collapses two different comparison windows into one. The current
  LaPorta reading also changes a conditional willingness to leave into a desire.
- Oracle still invents interpretations of the Boston profile; it now incorrectly
  calls the composite poor and adds strategic disjointedness.
- Some smaller problems remain: combined unrelated topics, instructional wording
  in prose, and publisher characterizations repeated as the writer's own judgment.

The new assembly is retained because it expresses the requested job and target
explicitly and preserves the data contract. Its output still needs further
calibration against ordinary cases. This trial does not establish the aspirational
nine-in-ten routine-card target. Historical context remains deferred.

## Changed paths and checks

The shared assembler and metadata loader are in `src/tools/context.rs` and
`src/tools/meta.rs`. All six character loaders, parts, instructions and prompt
versions use the new contract. Current quality fixtures reassemble through the
same runtime path. Metadata cannot replace the job or tone; missing identity
attributes stay missing. Source scope and exact wording remain protected by the
existing checks.

Final validation: **416 tests passed, 74 ignored**, covering library, eval and
example tests. Formatting and diff checks pass; release examples build. The first
test pass exposed one stale Scout expected JSON shape, which was updated before
the successful rerun. See `validation.txt`.

`selection.json`, `cases/`, `baseline-instructions.json`,
`candidate-instructions.json`, `trial/`, `analysis.json` and `review.json` preserve
the complete comparison. `build_cases.py`, `run_comparison.py` and `analyze.py`
provide the replay procedure; the run receipt records the control and candidate
binary hashes. Frozen controls use their original prompt versions rather than
being silently relabeled as current requests. Nothing was deployed or published,
and no production data was changed.
