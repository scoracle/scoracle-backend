# Editorial quality investigation — no application changes

Follow-up: [the September 14 implementation](2026-09-14-rating-evidence-contract.md)
addresses the data contract described here. This file records the earlier probes,
not the status of the subsequent code changes.

## Most important finding: the evidence contract is misleading

The live `_compute_rating_bundle` writes two different quantities to `pct`:
ranked players get percentiles; unranked players get the clamped standardized
score `50 + 10 * sign * z`. The Scout labels both as percentiles and applies
percentile quality tiers and season comparisons to both.

For Morgan Rogers (FOOTBALL/player/4592198), the 2026 row is unranked:
`rating_score` is null, Chance Creation has `z=4.5385` and `pct=95.4`, and
Creation has `z=1.6036` and `pct=66.0`. Those are fallback scores, not percentile
ranks. The 2025 row is ranked (`rating_score=72.4`); its Creation percentile is
94.8. A story about a decline from the 95th to the 66th percentile is therefore
invalid before the model writes anything.

The current row also contains one appearance, versus 37 in the prior season.
This sample size is absent from the Scout prompt. The performance row was updated
September 7; the July 3 roster date is identity provenance, not evidence of fitness
or availability. Nationality similarly does not establish national-team selection.

Migration 252's measurement identity is necessary but insufficient: comparable
observations also need the same score kind and a stated sample/cohort/time window.
Hold release of the preceding local changes until this semantic mismatch is fixed.

## Controlled probes

Requests use the existing local Ollama deployment, 4,096 context tokens and a
700-token generation ceiling. Seeds 17 and 43 support paired checks, not an estimate
of population-wide reliability. Temperature-zero repeats are deterministic, not
independent evidence. No surface retries were used in these probes: they isolate
first-pass interpretation. All 27 requests and their results are retained in
[the experiment record](../../run_docs/experiments/2026-09-13-editorial-quality.json).

- Current local prompt with prior prose: invented availability, confused missing
  trends with stability, and exceeded the card surface.
- Remove prior prose: reduced context, but invented trends remained.
- Compress numbers into a table: introduced percentile/rate confusion. Compression
  that loses explicit units or field meaning is not a viable improvement.
- Shorten character brief: reduced prompt size, but could yield either copying or
  unsupported interpretations, including reversed numeric comparisons.
- State measurement meanings and sample sizes: necessary evidence repairs, but did
  not make the current resident configuration reliably interpret the evidence.
- Temperature zero with the short, measurement-aware input: reproduced the facts
  with a generic summary hook, without a meaningful editorial synthesis.
- One unrelated positive editorial example: copied the example's pronouns into
  Rogers's card and still invented trends. A synthetic 30-appearance player was
  also tested. Do not promote this example into the character prompt.
- Native reasoning with the same 700-token ceiling: exhausted the budget without
  returning a card. This does not establish quality with a larger reasoning budget.
- Verified raw facts with no bogus ranks: the resident still invented reduced
  availability, tactical explanations, and a cross-season expected-goals comparison.
- The installed 8B comparison timed out twice at 80 seconds. No editorial-quality
  conclusion can be drawn from those timeouts; timings on a live service are not
  controlled throughput benchmarks.
- The other installed 3B model also timed out on both raw-fact checks. This run
  does not establish an alternative model's quality or a viable replacement.

The first probe rounds intentionally replayed the application's mislabeled values
before their true score kind was discovered. They test adherence to supplied text,
not the underlying truth of those percentile claims.

## Durable next changes

1. Separate true percentile rank, standardized score and eligibility in SQL and
   consumers. Never infer the quantity's meaning from a generic numeric field.
   Preserve raw measurements for unranked players; thin evidence can still support
   a useful card without manufacturing a percentile or a trend.
2. Supply compact measurement meaning, sample size, time window, observation date
   and comparison basis. Retain these semantics when reducing context. Distinguish
   sourced history from a previous model's prose; do not make flawed prose the
   default style example or fresh factual context.
3. Keep form to the surface and character to perspective. Do not promote any of
   these tested prompt variants as an established quality fix.
4. Qualify a model/runtime combination against actual evidence-linked readings.
   The existing `RatingTask::build_prompt` excludes live enrichment, and its grading
   mainly checks strings, length and bans. Those checks cannot establish whether
   claims are true or worth the card space. Keep editorial review offline, with
   human-reviewed real cards and a few contradictory/thin-evidence cases; do not add
   another daily inference stage.

This experiment design follows the task-specific, human-calibrated approach in
[official evaluation guidance](https://developers.openai.com/api/docs/guides/evaluation-best-practices),
without adopting a provider-specific prompt or model judge in the product.
