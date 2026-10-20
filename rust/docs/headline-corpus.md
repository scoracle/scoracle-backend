# Headline corpus

The new intake is RSS → headline spectrum → daily character package → saved output.
There is no article read, URL resolution, browser rendering or text extraction in
Classifier. Existing article bodies and products remain historical records.

Team RSS queries supply candidates alongside unique known player, team and person name/alias matches from each headline. Every known person role is eligible; unknown-person registration is a subsequent session task. Each canonical discovery candidate gets five native decision scores: subject
relevance, emotion, movement, availability and clickbait. Subject relevance ≥55
admits a headline to Journalist; Influencer also requires emotion ≥15, and Insider
also requires movement ≥80. These existing subject-scoped cutoffs select each
character’s evidence through its own lens. Availability and clickbait remain
context. A clickbait score measures headline wording;
it does not establish truth. Characters can abstain when headlines are too thin.
Scout continues to use structured statistics; Analyst and Oracle consume their
existing daily peer products.

Archive rows retain the exact headline, publisher, URL, publication time,
candidate provenance, scores, criteria version, model fingerprint and request /
response receipt. Current-source pointers, queue leases, edition membership and
publication fences still protect retries and daily outputs. Historical headline
revisions and rejected or failed decisions are retained. A changed stored article
body cannot invalidate a headline decision.

Journalist groups related fresh headlines into up to three new or continuing
narratives, retaining the exact contributing source IDs for each. Influencer
reads the emotional narratives across her selected headlines and memories.

Reporting characters receive `meta`, `fresh`, `memories`, `task` and `voice` in
one JSON document. Headline evidence is explicitly labelled. A headline is not
expanded into unseen article facts. Memories are earlier attributed headlines
with dates, URLs and scores, not generated summaries or proof of a new event.
The memory view uses the same character lens over the prior seven days,
at most 20 complete headlines and 6 KB. It keeps one copy of identical publisher/headline pairs, skips incomplete metadata, and normalizes publication times to UTC. It excludes fresh article IDs and decisions created after the edition
freeze. It respects the earlier of the reporting day's Eastern midnight and the
oldest fresh headline, including late-arriving reporting.

Postgres exports the eligible frozen archive; the shared DuckDB memory runner
ranks it by subject relevance for Journalist, emotion then relevance for Influencer,
and movement then relevance for Insider. Newer publication times and article IDs
break ties. It studies up to 20,000 eligible records before selecting 20, preserving
each selected headline's date, publisher, URL and full spectrum within the 6 KB budget.

## Classifier to daily context

The decision model remains d1; this preparation change does not alter its model,
criteria, thresholds or saved scores.

| Step | Owner | Handoff |
| --- | --- | --- |
| Score and archive | `classifier/adapter.rs`, `classifier/decision.rs` | Headline, publisher, date, URL, spectrum and immutable decision receipt. |
| Route | `classifier_spectrum_deliveries` | Separate pending obligation for each qualifying character. |
| Freeze the daily edition | `coverage_tick` | Capture each character/entity's spectrum IDs in `coverage_assignments`. |
| Read selected evidence | `classifier/delivery.rs` | Current, identity-validated sources from that character's frozen membership. |
| Prepare fresh context | Each reporting character's `fresh.rs` | Load headlines, preserve dates and attribution, apply existing completeness/budget guards, and load dated memories. Insider also retains canonical co-mentions. |
| Assemble the model call | Each character's `prompt.rs` | One document containing `meta`, `fresh`, `memories`, `task` and `voice`. |
| Save output | Each character's publication code | Revalidate source identity and delivery under the queue claim, save output/provenance, then settle obligations in the same transaction. |

Journalist's loader and selection, Influencer's reporting-day snapshot, and
Insider's loader and movement evidence preparation belong to their `fresh.rs`.
Production and evaluation use those same entry points. Headline presentation
omits article text; archived article-scoped receipts retain their original scope.
Dates that are absent stay unknown rather than being assigned an invented time.

## Stored-headline backscoring

Set `CLASSIFIER_REPLAY_BACKLOG=true` on the Classifier host. Its existing replay
operation now offers the newest unscored retained headlines in batches of 1,000.
It uses RSS query candidates and existing team/player/person article links and known headline name matches as candidates
for reassessment, never as positive relevance labels. Replay also reconciles a stale current source after a canonical-name change. The batch limit includes
existing runnable Classifier work, keeping historical offers bounded while d1
processes the queue. Headlines without a known
canonical candidate cannot be scored for a subject by this path.

Replay jobs older than 48 hours (or lacking a publication date) carry
`headline-history:` in their input version. They archive spectra without fresh
character obligations. The dated, relevant portion becomes available as memories
immediately, or at the next freeze if an edition is already open. Completed
backscoring is restart-safe; normal fresh discoveries still outrank old jobs.

## Activation

Migration 314 was deployed in the [October 19 plumbing release](headline-plumbing-deployment-2026-10-19.md). The migration 315 cleanup is implemented locally and requires a separate coordinated deployment.
For a coordinated cutover, stop the workers during the
coverage phase, apply migrations through `315_headline_entity_pipeline`, and
deploy matching Go/Rust binaries. Migration 314 refuses a cutover during an active
edition, supersedes pending historical deliveries, and removes retired reader
claims. Saved evidence and published products are retained.

On Archbox use `COGNITION_STAGES=classifier`, `CLASSIFIER_URL`,
`CLASSIFIER_REPLAY_BACKLOG=true`, and the existing coverage controller. Disable
`scoracle-classifier-source.service` and `scoracle-reader.service` before the
release so deployment scripts do not restart them. Remove any retired
`scoracle-cognition.service.d/reader.conf` dependency and reload systemd; otherwise
starting cognition can start the reader again. Migration 315 retires `classifier_acquire`; remove it from host stage lists and remove its installed service unit. The installer no longer renders it. The [entity cleanup](headline-entity-pipeline.md) records the current changes and unknown-entity follow-up.
The hourly RSS monitor and midnight Eastern character schedule are unchanged.
Mac character routes and saved-output publication remain in the existing worker.

The older reader and acquisition benchmarks describe the previous product and
are not performance estimates for headline scoring. The new five-score packet
still needs real-model quality and throughput measurements before promotion.

## Identity and tone

`meta.rs` supplies canonical name, entity kind and underlying sport to the writing
model. Database IDs stay in provenance. Stored affiliations and biographical facts
used for Classifier disambiguation do not become evidence of a new event.

`voice.rs` controls expression. Journalist remains clear and restrained; Insider
remains informed and measured. Influencer is emotionally perceptive and expressive
while allowing ambiguity and mixed feelings. Narrative selection, attribution,
uncertainty and the distinction between fresh headlines and memories live in
`prompt.rs`. Voice and model tuning are outside this plumbing release. Request
assembly checks do not establish factual model quality.
