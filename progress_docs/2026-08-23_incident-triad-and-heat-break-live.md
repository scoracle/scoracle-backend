# The incident triad closed durably, and the heat break goes live

**Date:** 2026-08-23 (PRs #6, #8, #9 — all merged and deployed same day; head `c76689b`)
**Companions:** `2026-08-23_seat-roles-and-the-guard-pipeline.md` (Scott's session this rides on),
`planning_docs/PLAN-heat-contract.md` (now COMPLETE end-to-end).

## Where this came from

Post-mortem of the overnight state found three faults hiding under a record-throughput day
(~650 products/hr on the new two-host split), plus drop 3b sitting unmerged. Scott:
"let's start on these... install a durable fix so I don't have to keep monitoring this."

## The triad, each closed at the class level (PR #8)

1. **The fetch panic (23 crashes, 02:07–04:30).** `trim_boilerplate_tail` searched a
   `to_lowercase()` copy and sliced at a raw midpoint byte — the `strip_element_blocks`
   incident shape (07-26) reproduced: "not a char boundary; inside 'á' / '🫶' / '’'".
   Rewritten on `find_ascii_ci`: byte search, no lowercased copy, boundary-safe by
   construction (a byte-exact match of valid UTF-8 cannot start mid-char). Tests pin
   multibyte midpoints and the marker alphabet (no non-ASCII letters — ASCII-ci can't fold
   them).
2. **Recovery starved by the drain.** The panics orphaned 38 `running` rows; they sat 4h
   under a 30-min lease because `requeue_stale` ran at the top of `tick()` and `drain_all`
   does not return while any stage has claimable work — the Desk's D-T14 disease, verbatim
   ("a queue-length-dependent cadence is not a cadence"). Recovery now runs on its own 60s
   task. Post-deploy it recovered all 38 in its first second; the fixed trim digested the
   poison articles with zero panics.
3. **Slot starvation (rating 10 cards/12h vs 928 ready; sigil 0 vs 4,245, or11 refill
   stalled 3 days).** The two-host split moved model calls to the Mac but left the
   Journalist/Influencer/Oracle budgeting against `ARCHBOX_SLOTS`, so deep-queued
   narratives/vibe filled the archbox group with work that card never executes.
   The three seats moved to `MAC_SLOTS` (4 = the Mac's `OLLAMA_NUM_PARALLEL`, verified on
   the live plist). Documented rule: **the group follows the route.**
   - **PR #9, measured minutes after the split deploy:** right group, still zero sigil —
     claim top-up runs in VOICE_ORDER with no rotation, and Journalist(2)+Influencer(2)
     saturated the four Mac slots. The Influencer yields to 1: `2+1 ≤ 3` guarantees the
     Oracle a slot whenever the group is full. **The cap arithmetic IS the fairness
     mechanism — there is no rotation to fall back on.**

**The watchdog now sees both shapes** (`cron-watchdog.sh`): `stuck_running` (orphans >2h ⇒
recovery itself broke) and per-seat `stage_starved` (>200 claimable while the seat's product
table is silent 6h ⇒ one dead seat behind a humming aggregate). Dry-run against prod fired on
exactly the day's three live conditions and nothing else. `WATCHDOG_ALERT_URL` remains unset —
set an ntfy topic for push alerts.

## Drop 3b live (PR #6)

The heat contract is complete: every board row and voice card serves `heat` as the one
number key (voice-native scale); `score`/board-`impact`/`card_score`/vibe-card `sentiment`
no longer serve. Vibes ranks by charge (`ABS(sentiment−50)`), momentum boards default to
movers (`ABS(slope)`, `?direction=up|down` as one-side filters). Verified on the public API
post-deploy.

## Verified outcomes

- Orphans: 38 → 0 in the recovery loop's first pass; no panics since.
- Rating: 10 cards/12h → **56 in the first 40 minutes** after the split.
- Claim distribution in the designed shape for the first time in days:
  rating 2 · sigil 1 (the guaranteed Mac slot) · narratives 2 · vibe 1 · momentum free.
- `cargo test` 422/422; CI ×3 green on every PR.

## Watch items handed forward

- **Oracle crown guard rejects:** with sigil finally claiming, first crowns are failing
  `oracle_reading_ban` ("percentile", "the omen is") and retrying — the 62/76 gate state
  expressing itself in production. The 4,281-deep sigil queue drains at 1–2 Mac slots ×
  the pass rate; if the reject rate holds high, the Oracle prompt wants the same
  emission-site treatment the fail-rate session gave the Analyst.
- **The Scout's colon habit:** nearly every rating title is "Team: description" —
  `hook_colon` drops them and the salvage can't help (one-word head). Rating cards are
  shipping headline-less at high rate. One candidate fix: an emission-site clause on the
  title line (write it as a sentence, no colon) — Scott's seat, his call, gate-first.
- Vibe-board pool: hook-less vibe cards (~6%) omit from the board until re-voiced —
  deliberate (the drop-not-retry trade), noted for trend-watching.
- Backfill: trough 7,551 → 6,915 → 6,755; with all seats claiming, the remaining
  narratives/vibe melt in hours and sigil/rating are bounded by their slots — days,
  not weeks.
