# Character pipeline guard fixes: 20 October 2026

This release addresses the eight findings from the post-Qwen pipeline audit.
Qwen3.5-4B remains pinned for all six character routes, with four shared slots
and a 7,900-token ceiling including reserved output.

## Runtime changes

- Bundle the current `scoracle-memory-study` beside the Mac worker. Character
  workers verify a nonempty headline-history request before claiming work.
- Migration 316 makes short names embedded in ambiguous proper names unresolved.
  Migration 317 applies the boundary check after materializing literal matches,
  preventing the planner from checking every name in the sport.
  Delivery requires the exact target in the current canonical candidates;
  RSS query provenance and a high relevance score cannot certify identity.
  Historical headlines must also match the current target before entering memory.
- Insider quotes must name both parties and state a move. Denials require explicit
  move-denial wording. The native schema couples denied status with a null stage
  and reported status with a valid stage. An explicit denial example also distinguishes
  the source reporting a denial from a reported move. Missing evidence produces no structured finding.
- Journalist production and evaluation share the same named-fact guard. A number
  cannot be reassigned to a named person by combining unrelated citations.
- Analyst produces an explicit missing-peer reading without inference when either
  performance or mood is unavailable.
- Scout preserves decimal points within claims, accepts the supplied sample
  minimum, and allows within-snapshot relative standing without a time claim.
  Explicit statements that stability is unsupported remain valid evidence limits.
- Native token overflow is a permanent packet-admission result. Reporting stages
  retry preparation once without optional history, then hold complete fresh
  evidence if it still cannot fit. They do not clip sources or repeatedly retry
  an unchanged oversized packet.
- Source loading and canonical identity validation run in batches under the same
  source, name-index, identity-row, delivery and queue-lease fences.

Current quality fixtures retain their evidence and expectations; affected version
labels and the Insider instruction are recaptured. Historical model scores and
Chelsea replay receipts are preserved.

## Verification and limits

The retained Chelsea name check keeps 53 of 63 headlines. Exclusions cover Berekum
Chelsea, a school opponent, women's-team reporting, listings/profiles and headlines
without a literal Chelsea identity. Regression tests cover both wrong fresh
routing and historical-memory leakage despite high model relevance.

The 80-report stress packet remains complete: 6,310 input tokens plus 2,100 output
reservation exceeds the cap. All three reporting stages are tested to remove
history first and durably hold the packet without generation or retry-queue churn.

These are conservative lexical and structural guards, not a general proof of
semantic truth. Ambiguous names or unsupported prose can abstain. Natural nightly
completion and sustained throughput still require observation.

## Deployment and recovery

Build from a committed revision. Preserve each host's current binary, launcher,
private environment and service states. Save the previous canonical-name function.
Stop the managed workers and acquisition monitor during the brief migration,
apply migration 316, install matching binaries/helper, and restore only the
previously active services. Keep binary-directory watchers stopped during placement.
Do not launch a second full queue worker for verification.

Check worker commits, the Mac helper-startup receipt, pinned routes/context,
controller heartbeat, source revision reconciliation, locks and the public API.
The Go API and acquisition code are unchanged; their binaries need no replacement.

Rollback restores the saved binaries and launcher plus the previous canonical-name
function, with workers stopped. Remove migration 316's receipt only after restoring
its function contract. Retain all archived sources and products; source revisions
can be reconciled through the existing Classifier queue.

## Production activation

The Mac character worker and Archbox Classifier, statcommentary and factsweep
binaries run runtime commit `3df810c5189c661c96663d56ff64730cdef284e2`.
Migrations 316 and 317 are active; the SQL optimization is commit
`a680d3ecf38c7df9cca0308460059d522657758a`. Migration 317 completed at
2026-10-20 17:34 UTC. The API and acquisition binaries retain their prior build.

Verification includes 356 Rust library tests, 17 CLI/bin tests, 64 opted-in
queue/database recovery tests plus the statcommentary database test, Go vet/build,
prepared-statement registration and Go race tests. A fresh UTF-8 baseline restore
passes the full repository check with migrations already applied. Native Qwen
checks passed Insider 6/6, Scout 3/3, Journalist 3/3 and Oracle 12/12 mechanical
properties. These checks do not replace claim-level editorial review.

The public API/CORS smoke passed 18/18. The managed Mac worker verified its bundled
headline-memory helper and all six routes on Qwen; the 7,900-token logical cap and
four 8,192-token physical slots remain in effect. Production retained coverage
phase and the midnight Detroit cutoff, with a current heartbeat and no long lock
waiters at postflight.

Postflight caught a cached Archbox build-script stamp; rebuilding its metadata
and replacing the binaries corrected the runtime commit receipt. It also caught
Postgres pushing the boundary predicate ahead of the candidate filter: the
representative identity query fell from 426 ms to 7 ms after migration 317.
The initial broad source reconciliation hit its 30-second limit, and a smaller
attempt hit a coverage-lock timeout. Reconciliation subsequently uses short
transactions. Two replay statements left by retired workers were canceled by
exact backend identity. These diagnostics are retained, not counted as passes.

Private rollback artifacts, binary hashes, startup receipts and native smoke logs
are retained under `pipeline-guards-20261020-3df810c5` in
`/Users/scotty/scoracle-worker/releases/` and
`/mnt/data/backup/scoracle/releases/` on Archbox. Earlier staged guard builds
were superseded before activation. Restore the original saved function contract
and binaries together for rollback; migration 317 must also be removed from the
ledger if reverting to the pre-316 function.
