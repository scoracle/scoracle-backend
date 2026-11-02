# Context and instruction review — November 2, 2026

The local model needs a better specified writing task, and the package and output
contract must support that task. These trials identify useful changes without
establishing that all six characters are ready or that prompt tuning is exhausted.
The current local Insider candidate retains worked examples and a deterministic
quote-eligibility correction. It still has recurring material errors.

The user clarified that the useful legacy workflow involved repeated evaluation
batches. No specific Claude-authored prompt or commit was available. This pass
therefore inspected actual requests and ran successive batches on the existing
Qwen3.5-4B-Q4_K_M server. No larger model, extra generation stage, historical
prose, production write, publication or edition-policy change was introduced.

## What the audit established

The transport sends the plugin's authoritative `instructions.txt` as system text
and the complete `meta` / `fresh` / `tone` JSON as the user message. Native template
and tokenizer receipts verify that both reach the model. The issue is not an
observed omission of the system message or fresh context.

The shared envelope provides consistent labels, but its contents do not yet have
equally clear meanings. `meta.database_record` is an assembled prose paragraph
mixing subject attributes, relationships, names and observation dates. The team
loader's coach lookup selects the latest-created coach record; it does not itself
establish that person's current appointment. Insider reports serialize routing
provenance and candidate names ahead of readable text. Candidate identity is not
evidence of a movement relationship. Long URLs remain present because provenance
must be preserved; moving them is different from deleting them.

The previous index trial illustrated a source-reference improvement, not the end
of prompt tuning. The legacy Journalist pipeline also supplied extracted claims;
the plugin writer now interprets raw reports and writes the card in one call.
That responsibility change is documented in the earlier
[assembly investigation](../../journalist/assembly-2026-11-01/README.md). Restoring
the old pipeline was not tested here and is not required by this finding.

| Character | Concrete input or instruction issue | Direction for the next prompt pass |
| --- | --- | --- |
| Journalist | A 1,838-word system contract mixes the editorial task, many restrictions and six examples. Background and reports share the same compact packet. Earlier layout-only trials did not solve ownership errors. | Keep purpose, source ownership and representative complete examples together; test source selection and actor preservation on ordinary packages as well as regressions. |
| Influencer | The score estimates coverage tone, while the body can describe named people's reactions. Those are different evidence scopes. | Make the available basis of the reading explicit and keep estimated coverage tone separate from expressed feelings. |
| Scout | Bare `value` and numeric `cohort` fields accompany labels such as total rebounds, without an explicit general unit/aggregation contract. The Boston replay called rebounds and blocks points and attributed the subject's measurements to its cohort. | Supply supported unit, aggregation and cohort meaning with each measurement; leave unavailable semantics unknown. Do not infer per-game units from a plausible-looking number. |
| Insider | A broad movement briefing is coupled to narrow full-name quote extraction. Co-mentions offered pairs that could never meet that quote rule. Stage was emitted before status. | Separate prose selection from pair extraction in the instructions; compute literal quote eligibility in code; test decision order independently. |
| Analyst | Current peer readings and dated comparisons share `fresh`, but a current contrast is distinct from temporal change. The Sam LaPorta replay treated missing comparisons as preventing any current balance assessment. | Explain current balance first and use only each supplied comparison's own dates for change. Tone remains an instruction about wording, not a description of the entity. |
| Oracle | Peer prose can arrive beside a bare `direction` label without its comparison window. Generated-at dates do not date all underlying evidence. | Preserve peer claims and uncertainty; qualify the meaning/window of direction labels before using them as trajectory evidence. Missing perspectives call for a smaller synthesis. |

These are observations and proposed corrections, not proof that one formatting
convention will fix every role. Other roles' runtime prompts were not changed by
this pass. The stale role descriptions in the evaluation registry are not the
system instructions actually sent to Qwen and were not treated as a cause.

## Batches and findings

All cases reuse the frozen source packages from the
[twelve-subject Insider trial](../insider-indices-2026-11-01/README.md). They retain
original source text, candidates, IDs, links and dates. They are development
packages from the October 23 frozen edition, not newly collected November news,
independent holdouts or a random accuracy sample. Review is unblinded by Codex.

1. **Sixteen calls, four cases and four combinations.** Compared the current
   instructions with a coherent editorial brief, and compact JSON with readable
   ordered JSON plus explicit report indices. Every original field and value was
   preserved. The four orders rotate across cases. Layout alone still assigns
   Pellegrini's injury to the metadata coach. The revised instructions can improve
   a conditional statement while still swapping an actor in another case.
2. **Eight calls, schema ordering.** Kept the same allowed JSON values while
   emitting findings first, then report index, quote, counterparty, status and
   stage, then body. Herbert's denial is corrected in one control comparison;
   other ownership errors persist. Property order is part of the model's task,
   but this ordering was not retained as a universal remedy.
3. **Twelve calls, complete worked examples.** `insider-worked.txt` explains the
   briefing subject versus the report actor, conditional intentions, opinions,
   staff appointments, denials and positive interest. Mitchell's condition and
   Quinn's separate staff hire survive. Neal's release comes from the Raiders,
   replacing the earlier invented Giants departure. False findings still occur.
4. **Four calls, worked examples plus ordered schema.** Herbert's denial again
   improves, but Roma acquires invented departures. No overall ordering change
   was retained.
5. **Twelve calls, worked examples plus quote eligibility.** The final local
   candidate preserves all twelve bodies byte-for-byte versus batch 3. Findings
   fall from fourteen to eight. Six impossible full-name findings are avoided;
   Herbert's two remaining findings now describe a denial. Two Mavericks quotes
   still fail their cited-source check. Todibo still receives agreement stages,
   and the City finding for Scott still overstates the owner's speculation.
6. **Four sampler calls.** Replayed the same four worked-example requests with
   Qwen's published non-thinking settings: temperature .7, top-p .8, top-k 20,
   min-p 0, presence penalty 1.5 and repetition penalty 1, using seed 42. These
   are the recommendations on the [official Qwen model card](https://huggingface.co/Qwen/Qwen3.5-4B).
   This single-seed diagnostic does not demonstrate a general improvement: Roma
   still swaps actors and Quinn acquires an unsupported leadership appointment.
   Server defaults and production sampling remain unchanged.
7. **Two metadata ablations.** Removed only `database_record` from Roma and Howe;
   subject name/type/sport and all reports remain. Roma still invents a coaching
   role, now for Pellegrini; Howe loses the invented Newcastle return. This does
   not support simply discarding metadata as the solution. These reduced inputs
   remain diagnostic only.

Every generation is retained, including unsuccessful variants. The initial
sampler runner stopped after its first response because it compared llama.cpp's
changing inventory `created` timestamp. That response was retained and parsed
offline; it was not regenerated. The remaining three calls compare stable model
identity fields. Exact caller wall time for the first sampler call was not saved.

Total: **58 generations, 159,140 input tokens and 5,634 output tokens**. The 57 saved caller durations sum to **596.003 seconds**; one call has no saved wall duration. These figures count all trial arms, not only the retained candidate.

## Retained local change

`src/plugins/insider/instructions.txt` now contains the worked-example contract.
`options_for_reports` offers only counterparties and report indices where the
target's and candidate's full names both occur in one headline or excerpt. If no
such pair exists, the schema requires empty findings while leaving the complete
source packet available for prose. This applies the existing parser's literal
requirements before asking the model to extract a quote; it does not establish
movement, stage or factual truth.

The prompt version is `insider-source-v22-quote-eligibility`. All eight transfer
quality fixtures use that version and continue to obtain their system text from
the authoritative runtime file. A regression test proves that names cannot be
borrowed between fields or reports to establish quote eligibility. This change
adds no model call, repair pass, keyword rule or new dependency. Other characters,
publication parsers and the shared envelope are unchanged by this pass.

## Quality and remaining work

[review.json](review.json) assesses all twelve final cards against their original
reports: four usable, three needing a minor edit, and five with material errors.
These judgments are development feedback, not a measured routine accuracy rate.
They include the structured findings, not just the readable body.

**Mechanical acceptance remains 0/12.** This must not be hidden by the prose
improvements. Some rejections flag real errors, such as metadata leakage and
wrong source spans. Others reject canonical-name expansions, the supplied
"shut down" denial, or faithful source chronology. Those existing parser
limitations are separate from model quality; no parser was weakened to obtain a
better score. Neither semantic readiness nor daily publication readiness is
established.

The next substantive context change should turn database background into explicit
fields with relationship type and observation date where known, while retaining
the canonical subject separately. It should also carry the reporting edition's
as-of scope. That is a data-meaning change to test across all six roles, not
another whitespace experiment. Historical context, when deliberately introduced,
needs its own dates, source kind and permissions; generated past commentary must
not become independent evidence for a current claim. History remains deferred.

Continue the batch/review/revise workflow with specific errors and ordinary cases.
The retained candidate is a useful checkpoint, not the requested nine-in-ten
routine-card target. No conclusion about the model family's capability follows
from this limited development sample.

## Reproduction and validation

`cases.jsonl`, `ordered-cases.jsonl`, `worked-cases.jsonl`,
`worked-ordered-cases.jsonl`, `eligible-cases.jsonl` and `metadata-cases.jsonl`
freeze each request variant. `examples/character_prompt_trial.rs` uses production
preparation, full-input and completion checks, and the unchanged parser; its
`--validate FIXTURE.json OUTPUT.json` mode performs no model call. Schema-order
variants assert structural equality with the declared schema. Old v21 requests
remain frozen receipts; they are not silently relabeled as the final v22 runtime.

`build_cases.py` asserts field preservation. `analyze.py` counts every saved
response, audits quote location, and asserts that quote eligibility preserved all
twelve bodies. Its `analysis.json` records total tokens, per-arm costs, raw output
and receipt locations. A missing wall measurement remains missing, not zero-cost
inference. Timings came from a shared workstation, sometimes alongside compilation;
they are not a daily-capacity or controlled speed benchmark.

`validation.txt` contains the final library/example test output. The initial
sandboxed run exposed a fixture convention mistake and two socket-permission
failures; the fixtures were corrected and the checks rerun with local socket
access. Final validation: **415 tests passed, 74 ignored** (392 library, 9 example and 14 eval tests). Formatting, diff checks and release example builds passed. See also `validation-eval.txt`.
