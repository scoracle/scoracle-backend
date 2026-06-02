# 2026-06-02 — Transfers: 3-way roster relationship + former-player noise filter

## Goal

Fix two related direction/precision issues the heat+Gemma layer surfaced: (1)
direction was binary (on-roster→outgoing / off→incoming), which mislabeled
former players; (2) FORMER players generated false rumors from historical /
multi-entity co-mentions (e.g. "Everton want [Chelsea's Delap] and [Spurs'
Gallagher]" spuriously links Gallagher↔Chelsea). User insight: a player on the
roster is an OUTGOING rumor; off the roster, INCOMING — and former players are
mostly background noise.

## What Was Done (`go/internal/ml/transfer.go`)

**3-way team relationship** (replaces the `isOnRoster` bool). `teamRelationship()`
classifies from `player_stats` history: `current` (latest season on the team) →
**outgoing**; `former` (on the team in a past season, not now) / `none` (never) →
**incoming**. Fed into the prompt so Gemma frames summaries correctly, and used
for the deterministic direction (Gemma's direction field is ignored).

**Former-player return gate** (deterministic). gemma4:e4b did NOT reliably honor
the prompt's "clear unless returning" instruction (it kept flagging Gallagher).
So: a `former` player is a live rumor ONLY if the pair corpus contains a
return-signal phrase (`return to`, `rejoin`, `re-sign`, `back to`, `comeback`,
`second spell`, …); otherwise `is_rumor` is forced false. This catches the
historical / multi-entity-article noise without trusting the small model.

## Verification

- `go build ./...` clean. Chelsea re-run: 30 candidates → **16 rumors / 14
  cleared**. **Gallagher cleared** (former, no return signal in corpus); **Delap
  kept** (current Chelsea, Everton-wanted → outgoing — a genuine rumor). Direction
  split 10 outgoing / 6 incoming, all consistent with roster membership. Live
  endpoint confirms (after cache flush).

## Caveats / follow-ups

- The return-signal keyword list is **tunable** — broad phrases (`back to`,
  `comeback`) can false-positive on non-transfer text; a genuine return phrased
  oddly could be missed. Tighten toward team-name proximity ("return to Chelsea")
  if needed.
- **Deeper root cause** (not fully solved): the news entity-linker creates
  spurious player↔team co-mentions in multi-team/multi-player articles. The
  former-gate handles the common case; improving link precision (text proximity,
  "Chelsea's Gallagher" patterns) in `thirdparty/news.go` is a future backend pass.
- Relationship is bounded by seeded history (a stint predating our data → `none`);
  accepted.
