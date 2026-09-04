# TODO: dead-letter pool needs triage (standing watchdog alarm)

Status: OPEN — noted 2026-09-04 during post-power-outage health check.

## The problem

The 08:30/20:30 watchdog has been firing the same single alarm for at least a week
(seen 08-30, 09-01, 09-04):

    watchdog ALARM dead_letters: 76 at attempt cap

These are pipeline_work rows that exhausted their retries and now sit permanently
failed. The pool grows slowly with each nightly run (+4 on 2026-09-04 alone).
Snapshot as of 2026-09-04 17:30:

| stage              | failed |
|--------------------|--------|
| editor             | 77     |
| investigate_entity | 2      |
| vibe               | 1      |

## Known failure classes (from cognition worker logs)

- `bookkeeping_citation` guard rejections (oracle readings carrying a bookkeeping
  citation) — retried into the cap.
- vibe context overflow: prompt 4264 tokens vs pinned num_ctx 4096
  (`exceed_context_size_error`) — deterministic, will never succeed on retry.
  Matches the known ~2% over-window class in the ctx ledger.

## What "fixed" looks like

1. Triage the 77 editor dead letters: bucket by error, decide requeue vs purge.
2. Deterministic failures (context overflow) should dead-letter immediately, not
   burn the retry budget — or the offending prompt should be trimmed under 4096.
3. Set WATCHDOG_ALERT_URL on archbox — the alarm currently lands only in
   logs/watchdog.log; nothing pages. (Long-standing item.)

## Related, parked deliberately (not part of this fix)

- 270 `fixture_boxscore` rows pending with no handler enabled in
  COGNITION_STAGES. Predates the outage; steady. Decide: enable a handler or
  purge the stage.
