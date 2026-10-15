# Four-score article Classifier — October 15, 2026

The target is one complete-article decision request with four independent usefulness
scores: Journalist relevance, Influencer emotional charge, Scout availability and
roster news, and Insider movement. Scores do not sum to 100. They nominate character
review; they do not establish an entity's relevance, a fact, or empirically calibrated
probabilities. The user prioritizes impactful narratives and fewer character calls,
accepting missed stories.

## Implementation

`classification-criteria-v5-article` supplies complete scrubbed publisher text and sport.
It no longer repeats inference for each candidate identity. Four article workers share
eight native Kev slots, with 8,192 tokens per slot. An oversized or incomplete request
fails visibly; there is no opening-only fallback.

One authoritative article spectrum and RSS-subject delivery projections commit with
all fired obligations and queue completion in one fenced transaction. Entity projections
reuse the same four scores and exact model receipt. Only RSS subjects receive these
character obligations; other canonical mentions remain retained acquisition and Graph
evidence. Characters determine whether the article supports their particular subject.
Scout additionally requires an applied source-linked structured record and its existing
70-point nomination policy before interpreting availability or roster changes.

Retries reuse a committed article measurement. If only RSS metadata or canonical context
changes, the new source can reuse a prior valid response only when article, sport, exact
request, model fingerprint, criteria and scrub version match. The new source and identity
checks still run before publication. A changed publisher body creates a new model request.
Cache-hit receipts are marked explicitly; their original model latency is not a new call.

An observed publisher failure message, “Sorry, this blog is currently unavailable,” now
fails acquisition rather than allowing surrounding menus to produce scores and deliveries.
It is a narrow source-availability guard, not a general article-extraction solution.
Other menus and related-story lists remain an identified calibration problem.

Migration 304 supersedes pending old-bank obligations without deleting history and queues
completed current sources for the new bank. Existing claims and retry backoff are preserved.
Historical six-question results in the [earlier report](classifier-native-calibration-2026-10-15.md)
are not measurements of this implementation.

## Calibration evidence

The development set contains 12 synthetic controls and 16 source-reviewed real cases.
Ten additional previously untested retained sources were labeled before reading their model
responses. These stress cases include navigation, schedules, related-story furniture and
real emotionally charged injury reporting. Two ambiguous candidates were excluded rather
than assigned convenient labels. All labels were assigned by the coding agent, not an
independent human reviewer. This small, selected set is not representative production
accuracy, and the additional ten cases are not an untouched final evaluation set.

| Model / questions | Errors at a common 60 cutoff, 112 original labels | Errors at 60, 40 added labels | Errors at 60, all 152 labels | Brier score, all 152 |
| --- | ---: | ---: | ---: | ---: |
| Kev / longer | 33 | 11 | 44 | 0.1860 |
| Kev / improved concise | 18 | 5 | 23 | 0.1614 |
| d1-3B Q4_K_M / longer | 14 | 9 | 23 | 0.1098 |
| d1-3B Q4_K_M / improved concise | 15 | 11 | 26 | 0.1301 |

The larger alternative has a real advantage over Kev with the original longer questions,
and a lower Brier score on this set. That advantage does not establish fewer errors than
Kev with its improved questions: each makes 23 errors at the common cutoff. Prompts and
operating points must be evaluated for each model. d1-omni-600M also ran natively but made
more errors in the initial development comparison and did not justify promotion.

A [later d1 refinement](classifier-d1-refinement-2026-10-15.md) tests native yes/no descriptions, per-role operating points and a frozen candidate on twelve separate source articles. It retains d1 as a challenger and leaves Kev live.

The provisional Kev cutoffs are Journalist 60, Influencer 50, Scout 60, Insider 70.
On the original set they yield seven false nominations and nine missed nominations.
On the additional ten they yield four false nominations and no misses. After the explicit
unavailable-page guard removes four cases across both cohorts, the remaining 136 labels
contain two false nominations and nine misses. These observations explain this candidate
policy; they do not prove a production false-positive or false-negative rate.

The highest-impact next quality work is better publisher extraction, then independently
reviewed article/character usefulness labels, grouped by source article with an untouched
final set. Character dispositions can supply review candidates, but agreement with SmolLM3
is not truth. High-impact missed narratives deserve separate inspection from aggregate
accuracy. Do not translate a model score of 80 into demonstrated 80% correctness.

## Native capacity

On the 1070 Ti, eight native slots and four concurrent article clients scored the same
256 complete retained articles twice, with the second pass reversed:

| Native model | Pass A | Pass B | Errors | Projected 8,500 articles |
| --- | ---: | ---: | ---: | ---: |
| Kev Q8_0 | 119.94 s | 119.94 s | 0 | 66.4 min |
| d1-3B Q4_K_M | 214.57 s | 215.47 s | 0 | 119.0 min |

These are native HTTP scoring measurements with the longer four-question bank. They
exclude acquisition, persistence and character generation. The corpus was drawn from
already-retained successful sources, not from all RSS discoveries; it does not establish
coverage for oversized or unavailable pages. d1 microbatch increases to 1,024 and 2,048
showed no useful gain in the matched 64-article tuning runs.

## Deployed throughput and backlog

Cut over at 15:09:26 Eastern. Migration 304 superseded 7,044 pending old-bank
obligations and queued 933 completed old-bank sources, preserving their history.
The actual Rust native smoke replay completed all 12 controls without transport
errors; its semantic mistakes remain calibration evidence, not a pass on accuracy.
Acquisition and the four-score Classifier are active. Graph and character work remain
queued under the existing calibration enrollment; their production routes are unchanged.

Between 15:10:07 and 15:12:41 Eastern, **308 complete articles committed in
153.79 seconds**, about **2.00 articles/second**, including database work.
That projects **70.7 minutes for 8,500 articles** at this short-window rate.
All 376 retained article rows at the boundary had four scores, complete coverage and
matching entity-projection receipts. A later check confirmed the same for 560 article
receipts, with no target in any model input. These counts are article measurements,
not multiplied entity projections.

Five model error receipts had accumulated by the earlier boundary; inspected examples
were complete inputs of about 9,400–9,700 tokens exceeding the 8,192-token slot.
These failures retain their raw response and receive no partial or invented spectrum.
The successful-article rate does not establish coverage or completion for every RSS
article, nor a sustained multi-day capacity guarantee.

At that boundary, 13,736 ready/running Classifier articles imply about 1.9 scoring hours
at the observed rate. Another approximately 29,800 articles still require acquisition.
Scoring all roughly 43,500 queued tasks would take about six hours **if their complete
text were already available and fit**. Acquisition remains the slower stage; its catch-up
estimate below governs the full backlog. Failed tasks need separate diagnosis.

Removed the unused 5.6 GiB retired Python dependency environment after checking process
mappings and runtime consumers. Package metadata, the 808 MiB checkpoint and all source,
measurement and delivery history remain preserved. Candle and Python model execution
are absent from the new production path. SmolLM3's Mac weights remain intact.

## Continuous processing feasibility

The pipeline already consumes durable work continuously through PostgreSQL notifications,
a safety poll, claim leases, fencing and systemd recovery. The RSS producer is currently
scheduled once daily at 02:00 Eastern. It can enqueue articles while acquisition,
classification, Graph and character workers process earlier arrivals; it need not wait for
the whole daily sweep to finish. No new orchestration platform is required.

At 8,500 articles/day, average demand is 5.9 new articles/minute. Either native decision
model has ample measured scoring capacity for that average. Graph is a generative task,
so it needs a separate generative model on Archbox; the decision model cannot replace it.
Characters can continue using the Mac's independent native SmolLM3 endpoint.

A bounded native coexistence test loaded SmolLM3 Q4_K_M with one 32,768-token
Graph slot beside either decision model. GPU allocation was 6,653 MiB with Kev and
7,151 MiB with d1-3B, out of 8,192 MiB. Both Classifier load tests completed 128
requests without transport errors while Graph used the production Rust `LlamaClient`,
its real template/tokenization checks, and complete canonical source preparation.
Kev took 96.0 seconds; d1-3B took 162.5 seconds. These shared-GPU measurements are
not the Classifier-only timing above.

Each Graph probe inspected 16 recent current sources, skipped one with no candidates,
and made 15 model calls. The runs took 127.6 and 128.2 seconds; their source cohorts
were different as acquisition continued, so they are not a matched quality comparison.
The first had three incomplete-output and two parsing failures; the second had one
incomplete-output and one parsing failure. Among parsed replies, two and seven had
names or result lines absent from the source under an exact substring check. Other
replies copied publisher brands as people. Exact substring support alone does not
establish that a name is a person or that a result line is valid. No model nomination
from these probes was published.

A final probe used four 8,192-token Graph slots with the same total 32,768-token
allocation alongside the live four-score Classifier. It inspected the same 16 article
IDs in 90.0 seconds, skipped two with no current candidates, and made 14 calls. Three
hit the output limit; five parsed replies contained spans absent from their source.
GPU allocation remained 6,641 MiB. Source/candidate state and competing Classifier load
changed, so this is not a clean 1.42× benchmark or a quality improvement. Full input
coverage checks remained active with the smaller per-slot context. It provides a
practical concurrency option to qualify rather than a production enrollment decision.

This demonstrates native protocol and memory feasibility, not production Graph
qualification or a sustainable useful-output rate. The probe makes one generation
attempt; it does not include Studio's corrective retries or Graph publication. Its
roughly eight seconds per inspected source would consume about 19 hours for 8,500
sources if extrapolated naively, before retries; the short, selected cohorts do not
justify that extrapolation as a forecast. Graph quality, source extraction and
selective useful work need attention before promising the entire 1070 Ti workload.
An earlier temporary Python Graph driver omitted special-token parsing; its Graph
quality/capacity results are discarded. The routing model comparisons use the native
System-1 endpoint and were unaffected.

The last four production RSS sweeps covered 206 teams and took 5.0, 5.1, 5.4 and 22.7
minutes. They already use a PostgreSQL advisory lock against overlapping runs and a
24-hour lookback. Frequent sweeps repeat RSS requests and discovery-version database
work even when model requests are reused. Cadence should be measured before changing
it; the current daily schedule is retained.

A pre-cutover 30-minute window acquired 279 articles, about 558/hour. At that rate,
8,500 publisher acquisitions take roughly 15.2 hours. An acquisition queue near 30,000
articles takes about 54 hours with no arrivals, or about six days while 8,500/day continue
to arrive, assuming that short-window rate holds. Retries and inaccessible sources have
no guaranteed completion date. These are acquisition estimates, not Classifier scoring
estimates, and they are not a 1–2-hour end-to-end promise.

Freshness also requires measuring burst load, backlog ordering and Mac character
acceptance/latency. More frequent discovery alone cannot make queued old work disappear.
No production Graph route, character enrollment or fetch cadence is changed by the
capacity experiment.

## Receipts and verification

The [machine-readable aggregate receipt](../fixtures/classifier/article-calibration-2026-10-15.json) records the observations and limits.

Private complete articles, requests and raw responses remain in the user-approved
restricted Archbox archive `/mnt/data/backup/scoracle/classifier-calibration/2026-10-15`.
The archive includes exact question banks, GGUF fingerprints, native load-test receipts,
expanded quality labels, and deployment evidence. No private source excerpts are copied
into this report. Public model references: [Kev model card](https://github.com/jaredpalmer/kev/blob/main/docs/model-cards/kev-0.8b.md),
[LiquidAI d1-3B GGUF](https://huggingface.co/LiquidAI/d1-3B-GGUF), and
[llama.cpp server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md).

All 376 runnable library tests and 17 binary tests pass; environment-dependent tests
remain explicitly ignored. The disposable PostgreSQL article-to-character test additionally
passes with changed-body remeasurement, RSS-rank-only response reuse, multiple RSS subjects,
incidental identity suppression, failed-response retention, unavailable-source rejection,
stale-source fences and exactly-once character dispositions. Character prompt fixtures
have updated contract versions, not a new live generation qualification.


## Production rollout and scoped Ponytail audit

### Release status at 15:48 Eastern

Commit `7ec86bc7` removes the client-side Kev name restriction. The same strict native
protocol accepts any named SHA-256 model alias, while model changes during work still
fail publication. Exact requests, question versions and model fingerprints continue
separating receipts and caches. The existing native launcher supports
`CLASSIFIER_MODEL_FILE` and `CLASSIFIER_MODEL_NAME` through the optional systemd file
`~/.config/scoracle/classifier-model.env`; Kev remains the default. A replacement still
requires compatible native typed probabilities, its own calibration and capacity checks.
This is model independence within llama.cpp's decision protocol, not automatic support
for every inference API or automatic promotion of a new model.

The release binary was built on Archbox and installed atomically. Kev, publisher acquisition
and Classifier remain enabled. The Mac's native SmolLM3 endpoint and tunnel passed health
checks. A short release of `narratives,vibe,rating,transfers` was rolled back to the prior
`COGNITION_STAGES=classifier` after real character jobs failed their reading/output budgets.
No new character output was verified. Pending deliveries and failed attempts remain retained;
Scout explicitly held six sources during the initial check. Graph remains unenrolled.

At 15:45:52, all 1,612 retained v5 article spectra had exactly four scores and complete
coverage; none used a target in the model request or an unexpected model identity. This
is integrity evidence, not evidence of narrative accuracy or complete character delivery.
The earlier 71-minute successful-scoring extrapolation is not a current backlog-completion
forecast: many remaining source revisions require reacquisition before scoring, and the
new live inspection shows slow identity/discovery reconciliation queries. RSS cadence is
unchanged. Do not describe this release as a healthy end-to-end character pipeline yet.

Validation: 376 runnable library tests and 17 binary tests passed. The disposable database
article-to-character test passed with a non-Kev model alias, including receipt reuse,
changed-body inference, stale-source fences and exactly-once settlement. The release/launcher
self-check passed for Kev defaults and replacement-model configuration. The existing
watchdog SQL tests passed in a separate disposable database; an additional audit reproduction
below demonstrates a monitoring case they do not cover.

### Scoped audit report

**What this repo does:** RSS discovery feeds durable publisher acquisition and a native
four-score Classifier. Character plugins consume source-bound obligations to build their
products. This audit covers the October 15 Classifier changes, touched delivery paths,
native provider changes, release scripts and their immediate callers; the assumed load is
8,500 articles/day, four decision requests concurrently, eight native decision slots and
four shared Mac generation slots.

**Must fix before releasing character consumption:**

1. **One oversized packet can stop an entity's character work.**
   - **What this is:** Influencer builds a period packet in
     `src/plugins/influencer/memories.rs:101`; Insider assembles all delivered reports in
     `src/plugins/insider/prompt.rs:98` and rejects the whole package in
     `src/plugins/insider/mod.rs:79`.
   - **Problem:** The initial production check recorded five Influencer packet-budget
     failures and three Insider package-budget failures. The same input retries without
     reducing the packet or settling its offending obligations; after five failures that
     entity's task stops retrying until another event or operator intervention.
   - **Fix:** Pack bounded units of complete articles and retain overflow for another pass.
     Explicitly hold a single article that cannot fit; Influencer needs a recorded period
     hold if removing that article would omit a denial or change the period's meaning.
   - **If we skip it:** Fresh articles keep accumulating behind the same failing packet.
     Raising the model's context alone does not remove these smaller byte limits.

2. **The current Journalist generation budget failed the native release smoke.**
   - **What this is:** `src/plugins/journalist/prompt.rs:18` allows 900 generated tokens for
     up to three reports, with complete-output enforcement in the native client.
   - **Problem:** The first two live Journalist jobs failed with native output-limit errors.
     Updated prompt fixtures and parser tests did not qualify generation with SmolLM3;
     those attempts produced no verified character product.
   - **Fix:** Replay those exact retained packets, make the report count/output budget fit
     the native contract, and require complete parsed outputs before re-enrollment. Keep
     the incomplete-output rejection; accepting truncated JSON is not a fix.
   - **If we skip it:** The same jobs spend generation time and back off without publishing.

**Should fix:**

3. **Repeated identity scans make reconciliation much slower than inference.**
   - **What this is:** `sql/migrations/302_classifier_source_revisions.sql:13` recomputes
     canonical candidates inside discovery versions. Source acquisition calls it twice;
     `src/plugins/classifier/delivery.rs:145` also computes it per returned source, then
     line 177 separately recomputes candidates, including for later publication checks.
   - **Problem:** Live reconciliation calls took about 1.5 seconds before character release
     and 2.2 seconds under the smoke load; inspected Kev calls in that window were often
     under one second. Historical/used reads also compute current-source state they do
     not enforce. The native-only benchmark cannot establish total backlog throughput.
   - **Fix:** Profile the canonical-candidate query and reuse a validated result within the
     same transaction; avoid current-source scans for immutable historical reads. Preserve
     the source and canonical-identity fences across inference.
   - **If we skip it:** Database reconciliation consumes the time saved by fast inference,
     particularly while rebuilding stale source revisions and reading character history.

4. **Acquisition can mask a stopped Classifier in the watchdog.**
   - **What this is:** `scripts/hosting/cron-watchdog.sh:30` combines acquisition,
     measurement, spectrum and generation timestamps into one progress timestamp.
   - **Problem:** In a disposable reproduction, ready Classifier work with no Classifier
     progress reported `OK drain_alive` solely because acquisition was fresh. Error
     measurements can also count as progress. Existing watchdog tests do not cover this.
   - **Fix:** Check successful progress separately for each enabled stage with ready work,
     and add the acquisition-active/Classifier-stalled case to the existing SQL self-check.
   - **If we skip it:** A partial pipeline outage can look alive while its queue grows.

5. **Current-corpus Influencer evaluation still reads retired Harvester deliveries.**
   - **What this is:** `src/evaluation/tasks.rs:638` and
     `examples/influencer_replay.rs:7` select sources through the old delivery reader;
     production uses `plugins::classifier::delivery`.
   - **Problem:** An entity with only new Classifier obligations can disappear from live
     evaluation, or the evaluation can anchor an old article absent from production's
     eligible population. Recorded-packet replay is a separate path and is unaffected.
   - **Fix:** Switch both current-corpus entry points to the same Classifier delivery reader
     and verify a new-only fixture is included before using these tools for promotion.
   - **If we skip it:** A model can look qualified on inputs that do not match production.

The broad Harvester/Editor tree is not wholly dead: reader utilities, the legacy current-
corpus evaluation paths and explicit historical replays still reference it. Deleting the
whole tree now would break callers. Migrate the current-corpus readers first, then retire
obsolete replay adapters deliberately; historical measurements need not be deleted.

**Verdict:** Kev's native four-score Classifier is deployed with model-independent identity
handling. Fix character packet/output budgets before enabling character consumption again.

**Not checked:** A representative human-labeled accuracy set, sustained multi-day throughput,
successful native character publication after correcting these failures, Graph qualification,
other repository subsystems and a full production-database restore drill. This audit changes
no implementation code; the character enrollment rollback is recorded operationally above.
