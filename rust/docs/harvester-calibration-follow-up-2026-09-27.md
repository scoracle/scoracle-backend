# Harvester calibration: accepted baseline and future options

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

## Decision — September 27, 2026

The user accepts the current v7 behavior: **90 of 96 synthetic route decisions
matched the expected labels**, with three unwanted routes and three missed routes.
Further calibration is deferred until experience with this format provides a
reason to revisit it. It is not a prerequisite for continuing plugin alignment.

Implementation baseline: commit `6b3ae88d`. Keep its frame, checkpoint, thresholds
and window aggregation unchanged for now. The result describes the existing
development set, not an estimate of production accuracy. The
[frame report](harvester-frame-2026-09-27.md) preserves the evidence and limitations.
This acceptance does not itself deploy code or change delivery settings.

## Options when experience warrants a change

| Observed problem | Possible response | Evidence needed before choosing it |
| --- | --- | --- |
| Useful cases score above irrelevant cases, but the cutoff admits too much or misses too much | Adjust plugin-owned thresholds per predicate, evaluating the resulting destination decisions | Reviewed real cases showing the tradeoff between useful-source recall, unwanted assignments and cost |
| Irrelevant cases score above useful cases | Inspect the predicate and subject frame; simplify ambiguous wording or supply missing canonical identity facts where available | Paired failures with the exact metadata and source seen by Laya; compare one framing change at a time |
| Errors cluster around aliases, namesakes or unstated affiliations | Extend shared metadata with narrowly resolved identity cues, if the plugin can obtain trustworthy facts | Recurring identity failures and a canonical source for the additional facts; no model-invented relationships |
| Evidence spans window boundaries or longer openings produce extra routes | Compare sentence-aware boundaries, bounded overlap, or alternative aggregation | Exact source spans, window counts and per-window scores; measure both misses and inference cost |
| Useful information appears after the selected third paragraph | Reconsider plugin source selection or the bounded reading budget | Reviewed examples proving the missing material is outside the current frame, rather than a scoring failure |
| A downstream character repeatedly rejects a particular class of assignments | Revisit that destination's eligibility definition with the receiving plugin | Source dispositions with reasons; distinguish lack of evidence from redundancy or an already-covered story |
| A stable, adequate frame still cannot separate supported and unsupported cases | Evaluate a different checkpoint or targeted model training | Sufficient reviewed examples, a frozen comparison set and measured operational benefit |
| A consumer needs scores to mean actual likelihoods | Fit and validate a probability calibration mapping separately from route policy | Enough independent labels to test reliability; no need to add this merely to choose destinations |

Threshold changes alone cannot fix every error. In the development run, a valid
narrative scored 0.4537 while an unrelated-team narrative scored 0.5255. No single
narrative threshold correctly separates that pair. Conversely, an NBA result at
0.6744 narrowly missed the 0.70 performance cutoff; that is a candidate for a
threshold tradeoff, not proof that lowering the cutoff would improve overall use.

The plugin continues to prepare and govern in every option. Laya scores supplied
predicates; the harness dispatches approved destinations. Additional prompt rules,
keyword patches and a generative fallback are not default remedies.

## Minimal process for a future calibration pass

1. **Collect concrete failures and a representative sample.** Use existing source
   receipts and character dispositions. Include multiple sports, publishers, days,
   opening lengths and rejected headlines. Keep targeted failure examples separate
   from the representative sample so their enrichment does not distort rate estimates.
2. **Review evidence before revealing model scores.** Label explicit headline
   reference, whether a publisher read is warranted, each source-supported predicate
   and appropriate destinations. Cite supporting passages. Preserve uncertain labels
   and acquisition failures separately from negatives. Downstream non-use is a clue,
   not automatic proof that a route was wrong.
3. **Separate diagnosis, tuning and evaluation.** Group duplicate/syndicated stories
   and their entity-query variants together. Reserve untouched stories or later days
   for final evaluation. The existing 24 synthetic cases remain regression fixtures.
4. **Test the smallest justified change.** Compare thresholds only after checking
   score ordering. Evaluate the complete pipeline as well as individual predicates,
   including headline misses, maximum-over-windows aggregation and destination OR
   rules. Inspect sport and window-count differences before introducing special cases.
5. **Freeze and compare.** Report useful material retained, unwanted routes, source
   coverage, fanout and fetch/inference cost on the untouched set. Prefer avoiding
   useful-source loss at headline admission, while balancing character selectivity
   against downstream capacity. Make that tradeoff explicit when selecting policy.

Keep this offline: reuse `context_harvest`, adapt the existing review format to the
seven predicates, and extend `harvest_routing_score.py` for reviewed labels and
threshold comparisons. The old headline scorer's fixed 36-case split is specific
to its historical experiment. Avoid parallel evaluation frameworks or another
runtime decision layer. Only a selected, versioned policy should enter production.

Threshold selection and probability calibration address different questions; see
the primary references on [decision thresholds](https://scikit-learn.org/stable/modules/classification_threshold.html)
and [probability calibration](https://scikit-learn.org/stable/modules/calibration.html).

## Reasons to reopen

Revisit when recurring missed stories or irrelevant assignments become noticeable,
when a sport or source format behaves differently, when window-related errors recur,
or when the checkpoint, subject metadata or downstream product requirements change.
Retain a small set of source-bound examples as they arise. No new scheduled monitor,
training run or collection service is required by this decision.
