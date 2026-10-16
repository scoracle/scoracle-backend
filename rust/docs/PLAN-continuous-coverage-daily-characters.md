# Continuous coverage, one daily fresh package

October 16, 2026. The daily package changes and disabled scheduling boundary are
implemented locally. Production still uses the previously deployed schedule.
Migrations 309–310 and these binaries have not been deployed.

The backend prepares one fresh package per character, canonical subject, sport
and reporting day. Memories are a separate, deferred layer. Weekly packets,
weekly wrap jobs and reporting-calendar dependencies are retired. A client can
build longer reporting views over stored daily outputs later.

## Daily flow

1. During coverage, Archbox discovers, acquires and classifies reporting. Use the
   existing queue and bounded extraction policy. Graph is retired; unknown
   entity discovery belongs to future Investigator work.
2. At **midnight America/Detroit**, stop new coverage. Give in-flight preparation
   at most **15 minutes** to drain. Unfinished work carries forward.
3. Freeze the day's finite assignments. Journalist, Insider and Influencer each
   receive one fresh reporting package; Scout receives the current measured
   profile and fresh reporting. Late-acquired reports retain their actual dates.
4. Run characters on independent local endpoints on Archbox and Mac. Analyst
   waits for that day's Scout and Influencer; Oracle waits for the five peers.
   Missing or failed peers cannot be replaced by yesterday's cards.
5. Reopen coverage when the edition and required handoffs finish. Allow at most
   **six hours** of character time; record a partial edition and fence unfinished
   claims at the deadline. Run a catch-up sweep, then poll RSS hourly.

The controller caps the entire stop at cutoff plus drain allowance plus character
allowance. Repeated ticks and restarts cannot create a second edition for the
same date. Midnight boundaries use local dates across 23- and 25-hour days.

## Package and output rules

- One bounded fresh package, with complete source text and provenance. No weekly
  packet, historical report collection, publisher-memory packet or rolling score
  study is loaded into character preparation.
- Keep current structured measurements as facts. Sports seasons, fixture rounds
  and NFL week data remain available; they do not schedule character outputs.
- Exact copies can be deduplicated with receipts. Oversized daily packages are
  held explicitly rather than published as a series of partial article passes.
  The initial reporting budget remains 24 KB. No source text is clipped.
- One publication transaction per character/subject/reporting date. A Journalist
  output may contain multiple source reports in that transaction. Abstention,
  unchanged input or a hold may produce no new card.
- Retain historical products, original reporting bounds and source evidence.
  New products have Eastern `reporting_date` values. API reporting filters accept
  `date=YYYY-MM-DD`; weekly/year/scope filters and the `/weeks` calendar are retired.

## Implemented boundary

Migration 309 adds the shared coverage clock, frozen queue assignments and source
spectrum membership. Queue claims and claim-fenced publication share a transition
lock. The independent worker clock is opt-in through
`COGNITION_COVERAGE_CONTROLLER=true`; database enrollment defaults off.

Migration 310 retires weekly stamp/seal functions and adds daily product dates.
Its publication guard requires the current queue lease and prevents a second
publication transaction for the same daily assignment. Character context readers,
daily API reads and generated API documentation use the simplified contract.

Tests cover competing claims, cutoff and publication races, coalesced offers,
failed work, outbox completion, restart recovery, DST dates, daily publication
uniqueness and package overflow. Offline fixture prompts are rebuilt through
production assemblers; this does not qualify model fidelity.

## Remaining before activation

Finish phase-aware RSS/data ingestion and immediate catch-up, freeze the Scout's
prepared measurement snapshot, and qualify the Archbox/Mac model handoff. Update
cron and both host configurations together, then benchmark a real frozen edition.
The controller flag or migration alone must not enable the live schedule.

With six hours reserved for characters, examining 8,500 discoveries requires
roughly 472 links per coverage hour before handoff overhead. The
[cold acquisition benchmark](acquisition-benchmark-2026-10-15.md) measured roughly
700–900 examined links/hour; sustained daily throughput and lost story coverage
remain unmeasured. Track discovery-to-ready delay, oldest unfinished work,
accepted/skipped acquisition, daily holds, useful outputs and edition duration.

The [Graph retirement deployment](graph-retirement-deployment-2026-10-16.md)
remains the current production baseline and recovery record.
