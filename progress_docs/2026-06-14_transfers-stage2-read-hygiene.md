# 2026-06-14 — Transfers as stage 2: read hygiene (task 11)

The reordered staging makes transfers stage 2 (the evidence layer that grounds narratives). Built +
verified, committed — db.go-only (prepared statements), so it lands with the batched binary deploy.

## Reassessment (honest correction)
The sample-payload "legacy noise" (Son / Pedro / dup João Pedro) was an **over-flag**: that sample
queried raw `heat>0`, but the SERVED reads filter `is_rumor IS TRUE`, and the local model vet had already
**cleared** that noise (Son/Pedro/André/Nícolas → is_rumor=false; the wrong same-name João Pedro →
is_rumor=false, the real one true). Served set = 21 vs 31 raw for Chelsea — the 10 dropped were exactly
the false positives. So the served transfers were already clean; the disambiguation works.

## What "transfers from the scrubbed corpus" already is
- `compute_transfer_heat` + `seed_transfer_rumors` read vetted links (migration 084, applied).
- `transfer.go loadCandidates` reads vetted links (task 4, awaiting the batched deploy).
- ⇒ post-deploy + a regeneration (`cmd/transfer -mode corpus`, or the task-10 cadence), every served
  row is generated off the scrubbed corpus. No new code needed for the sourcing.

## What task 11 added (read hygiene, db.go)
The three transfer reads (`transfers_leaderboard`, `team_transfers`, `player_suitors`) gained, in the
`latest`/`ranked` CTEs:
- **`generated_at > NOW() - INTERVAL '14 days'`** — only recently-regenerated rows serve, so stale /
  pre-scrub pairs **age out** (they don't self-heal under append + latest-per-pair) and dead pairs drop.
- **`heat > 0`** — drops zero-signal stragglers that slipped through `is_rumor IS TRUE`.

## Verification (read-only on prod)
- build + vet + gofmt clean.
- Chelsea served transfers **21 → 15**; the 6 dropped are all **heat=0** (Mbappé 6.3d, Konaté, Jørgensen,
  Colwill, Alex Scott, Valentín Barco). Real heat-bearing rumors (Rogers 85, Cucurella 53, Bowen 34…)
  all kept.

## State / next
- Committed, NOT deployed (db.go changes ride the batched binary deploy with tasks 4/6/9).
- Operational at deploy: a one-off `cmd/transfer -mode corpus` regeneration off the vetted corpus; then
  the task-10 cadence keeps it fresh.
- DEFERRED (optional bake): the transfer local model vet→determine promotion (Roadmap 1a — read heat_components
  to produce the surfaced heat). The current vet is working well, so not blocking.
- Next: task 12 (ground narratives on the vetted transfers), task 10 (orchestrate), task 5 (open gates),
  batched deploy, task 7 (frontend).
