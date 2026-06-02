# 2026-06-02 — Transfers/Trades: Gemma vetting (Phase 2)

## Goal

Phase 2 of the Transfers/Trades feature: use self-hosted Gemma to **vet** the
deterministic heat candidates — is it a real rumor, what stage, a grounded
summary — and set **direction deterministically from roster membership** (per
the rule: a player on the team can only be OUTGOING, off the roster INCOMING).
Clones the Vibe Generator pattern.

## What Was Done

**`go/internal/ml/transfer.go`** — `TransferGenerator` mirroring `ml/vibe.go`.
Per team: load co-mention candidates (≥`min_articles` distinct shared articles,
14d, capped at 40 = the Gemma load governor) → per pair: compute deterministic
heat (`compute_transfer_heat`) + pull the pair corpus → call Gemma with
`JSONMode` (temp 0.3) → defensive-parse `{is_rumor, direction, stage, summary,
confidence}` (first-`{…}` extraction + enum coercion) → append a `transfer_rumors`
row. Layered reliability: JSON mode + defensive parse + a **deterministic
heat-only fallback** (provisional `is_rumor=TRUE`, no classification) on Gemma
failure/unparseable output, so the card never breaks. Grounding guard:
`source_attribution` is the corpus's top-tier source (not Gemma's citation);
confidence halved when a claimed rumor has no tier-1/2 source.

**Direction is deterministic, not Gemma's guess.** New `isOnRoster` (player's
latest-season team == this team) → `outgoing`; else `incoming`. The roster status
is also fed into the prompt so Gemma frames the summary correctly (other clubs'
interest vs the team pursuing). Gemma's `direction` field is ignored.

**`go/cmd/transfer/main.go`** — CLI, `single` (`-team-id -sport`) + `corpus`
(walk teams), cloned from `cmd/vibe`.

**Read fix (`db.go`).** The `team_transfers` statement now takes the latest row
**per pair regardless of verdict**, then filters `is_rumor IS TRUE` — so a fresh
Gemma "cleared" (is_rumor=false) supersedes an older heat-only seed row. (Before,
a cleared pair kept showing via its stale seed row.)

## Files Changed

```
go/internal/ml/transfer.go      (NEW)
go/cmd/transfer/main.go          (NEW)
go/internal/db/db.go             (team_transfers: latest-per-pair THEN filter)
```

## Verification

- `go build ./...` + `go vet` clean. Ollama (`gemma4:e4b`) live.
- `transfer -team-id 18 -sport FOOTBALL` (Chelsea): 30 candidates → **18 rumors,
  12 cleared**, 0 errors, ~2.5 min. Gemma correctly clears roster/match-report
  noise (Azpilicueta, Reece James, Xhaka).
- **Direction matches roster**: Cucurella/Palmer/Enzo/Delap/Garnacho → outgoing;
  Bowen/Gallagher/Mbappé/Konaté → incoming. Grounded summaries + tier-1 attribution
  ("per The New York Times", "per Sky Sports").
- Endpoint `/football/team/18/transfers` → 18 (cleared pairs gone); SSR card shows
  directions, stages, summaries, "exit" badges for outgoing.

## Result — Phase 2 in, follow-ups

- **Phase 3 next**: news-spike `transfer_trigger` (extend `notify_vibe_trigger`) +
  listener (clone `news_volume_worker.go`, per-team concurrency cap + 60-min
  debounce) + external cron staggered after the vibe run.
- Gemma still has tunable false-positives (e.g. Conor Gallagher — a tangential
  co-mention marked a rumor; the grounded summary exposes it). Prompt iteration is
  a knob, not a blocker.
- `isOnRoster` uses `EXISTS(max-season row on team)`; a dual-team-season player
  reads as outgoing (has a team row) — acceptable edge.
