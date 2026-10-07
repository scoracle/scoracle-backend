# Harvester final integrity review — September 27, 2026

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

**Follow-up:** the [v7 frame and shared subject metadata](harvester-frame-2026-09-27.md)
now address the interface and source-window gaps listed below. This review records
the pre-v7 findings and repairs. The user subsequently accepted the v7 90/96
development result for now and [deferred further calibration](harvester-calibration-follow-up-2026-09-27.md).
The historical verdict below is not a blocker for the next plugin window.

Verdict: aligned ownership and source boundaries; classification effectiveness is
not ready for sign-off. The cleanup is complete, but Harvester's semantic routing
work is not. Laya needs a better prepared classification frame, not more tools or
autonomy.

## What aligns

The plugin selects the target, evidence, questions, input bounds, thresholds and
eligible destinations. Laya returns typed probabilities. The application supplies
scoped acquisition, inference transport, claim fencing and durable dispatch.
Harvester makes no generative call and produces no editorial interpretation.
Source text, attribution and offsets are verified before delivery; downstream
plugins own product sufficiency and articulation.

One worker/replay path and one route table survive. Failure remains distinct from
negative evidence. Held delivery does not erase eligibility, and enabling delivery
cannot manufacture it. Source-bound identity and downstream recovery obligations
remain necessary consumers, not justification for retaining the retired packet path.

## Remaining effectiveness gaps

1. **Current theme questions are not selective enough.** The real-source smoke
   routed all seven admitted contexts to all four themes. The current frame also
   routed all 48 synthetic case/theme pairs positive, including negative controls.
   Source-only native boolean questions improve development results, but transfer
   and performance misses remain. The active Rust path still uses the original
   choice frame. These results prevent an effectiveness sign-off.
2. **Some requested judgments exceed the supplied identity context.** The worker
   supplies a team's canonical name, type and sport, but the headline predicate
   also asks about a direct consequence for that team. It supplies no resolved
   player/coach/alias relationship facts. Where such relationships are necessary,
   the model would have to infer them from its prior knowledge. Either provide
   narrow, source-matched canonical identity cues from the plugin or define a
   narrower text-supported predicate. This is an interface limitation, not a
   measured claim that every indirect headline fails.
3. **Coverage and calibration remain unproven.** Theme state combines the headline,
   publisher and target with only the first 100 words / 1,200 bytes of the retained
   three paragraphs. Metadata can provide signals absent from the selected source
   text, while useful evidence can fall after the model's prefix. Existing byte
   checks establish integrity, not sufficient semantic coverage. The 0.25 reading
   and 0.50 theme thresholds remain provisional. Independent reviewed cases must
   test missed useful material and unwanted routes before changing policy.

Do not fix these by adding a longer prompt, generative fallback or arbitrary keyword
gate. Define the evidence-supported predicates, fit them to the model's native
interface, freeze the candidate, and measure independent cases. The
[routing review](harvester-routing-review-2026-09-27.md) retains the development
results and limitations. Live intake currently supports team queries; player query
support is not implemented by the worker merely because the replay DTO is generic.

## Integrity repairs made in this pass

- **Complete task coverage:** the Laya SDK separately clips instructions and option
  descriptions. The old server checked only state length. Adapter
  `harvest-laya-v2` now compares the SDK's actual empty-state token sequence with an
  unshortened frame and rejects shortened questions/criteria before inference.
  State capacity is still checked, and coverage now records question token count.
  Normal sampled current questions fit; this gap is not presented as the cause of
  the observed over-routing. No model wording or thresholds changed.
- **Failure receipt source fence:** successful publication checked URL, headline,
  publisher and date, while retry/error recording checked only the headline. Both
  now use one row-lock/source-identity check. An old failed attempt cannot attach
  its acquisition metadata to a changed source identity.
- **Immutable replay receipts:** the database uniqueness key includes body and
  checkpoint but omits headline/attribution. A collision previously returned the
  old receipt ID without comparing its contents. Reuse now requires matching
  evidence, slices, admission, theme distributions and plugin policy provenance.
  Per-call provider provenance is excluded from that comparison; timing differences
  do not create a new receipt. A mismatch rolls back rather than silently mixing
  old receipts with new routing decisions.

Same-body source revisions now fail visibly and require an explicit new receipt
revision; automatic receipt-key migration/supersession is not implemented in this
pass. Do not solve that failure by overwriting a receipt already consumed downstream.
V1–v5 pending-delivery compatibility remains a separate historical recovery path.

## Verification and deployment

- 19 focused Rust unit tests passed.
- Four Harvester integration tests passed in a disposable local PostgreSQL database,
  including added assertions for failed-attempt metadata drift and same-body
  headline/publisher collisions. Exact-input idempotency still passes.
- 12 Python tests passed in the installed Laya environment: eight archival checks
  and four new SDK/tokenizer boundary tests. Long instructions, long single options
  and combined option-budget overflow are detected. Short choice/noul/score frames
  match the actual SDK. No model weights or network inference were needed.
- Ten questions prepared by the current Rust replay were checked against the real
  cached English tokenizer and SDK. All questions and states fit. Sample frames
  used 78–80 tokens for themes and 114 for the headline question; these are fixture
  observations, not a guarantee for every future entity/headline.
- All-target compilation and whitespace checks passed.

These are local changes, not a deployment or a new model-quality evaluation. The
question-coverage fix requires the updated Python server when deployed; a still
running v1 server does not acquire that protection from a Rust-only rollout.
