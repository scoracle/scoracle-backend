# FIRST-GPT-AUDIT Session 10 — Make transfer validation fail closed

**Date:** 2026-06-22 · **Machine:** archbox (prod) · **Code:** `1486b7b` · **Migration:** `104_transfer_fail_closed`
**Status:** ✅ deployed live + verified

## Goal

Only a successful **positive local model verdict** can become a served or downstream-consumed transfer rumor. A
model failure (timeout / unparseable output / a verdict that never commits to `is_rumor`) must never
masquerade as a vetted rumor — it must fail closed (unknown) and be retryable.

## What was wrong

- `ml/transfer.go` wrote a **provisional `is_rumor=TRUE`** row on any local model error or parse failure ("so the
  card never breaks"). That is fail-OPEN: a timeout produced a served rumor.
- Narrative/Vibe heat grounding (`loadTransferHeat`) and `pipeline_stats.transfer_rumors_active` did **not**
  filter `is_rumor`, so a cleared/unknown verdict still influenced downstream prose, scores, and metrics.
- `compute_transfer_heat` allowed `(vetted IS TRUE OR scrubbed_at IS NULL)` — i.e. **unscrubbed** links
  contributed heat before local model ever saw them (the candidate selector already required `vetted IS TRUE`, so
  the two disagreed).
- Vestigial tweet plumbing (`input_tweet_ids` columns, `tweet_ids` OUT param, `loadPairTweets`, `tweetItem`,
  the fail-OPEN `seed_transfer_rumors` Phase-1 seeder) lingered after X was decommissioned (migration 098).

## Decisions

- **Fail closed = `is_rumor NULL`, not absent.** On model failure we still INSERT an UNKNOWN row (durable
  audit + the verification's "row remains unknown and retryable"), counted as `res.Unknown`. It is never
  served (every read requires `is_rumor IS TRUE`). A parsed verdict with a **missing `is_rumor` field** is
  also routed to UNKNOWN (a missing verdict is not a confident "cleared").
- **Retry via the existing queue, no new mechanism.** `drainTransfers` returns an error when `res.Unknown>0`
  ⇒ `work.Fail` ⇒ the existing `pipeline_work(transfers)` backoff re-enqueues the **team** (transfers stage is
  team-grained). After `maxAttempts`(5) it dead-letters and the UNKNOWN rows simply stay unserved. Mirrors
  F-019's failure-as-retryable philosophy.
- **Filter `is_rumor IS TRUE` AFTER `DISTINCT ON`** (latest-per-counterparty), so a NEWER cleared/unknown
  verdict supersedes an older TRUE — matching the `/transfers` read contract (which already did this).
- **Historical fail-open rows: re-vet, don't mutate.** Append-only invariant forbids flipping history in
  place; instead enqueue the affected teams for a fail-closed re-vet (see F-020).
- **Deploy order inverted (F-022):** the new binary tolerates BOTH schemas, so release it first, then migrate
  — no broken window, no API stop.

## Changes

### Go (`1486b7b`)
- `internal/ml/transfer.go` — fail-closed `persist` (verdict==nil ⇒ `is_rumor NULL` + `res.Unknown++`);
  `analyzePair` routes local model error / `!ok` / `verdict.IsRumor==nil` to the unknown path; removed
  `loadPairTweets`, `transferMaxCorpusTweets`, the `tweet_ids` scan (now `SELECT heat, components, news_ids`),
  the `input_tweet_ids` INSERT column, and the `tweets` params on `bestSource`/`hasReturnSignal`/
  `buildTransferPrompt`. Added `TransferResult.Unknown`.
- `internal/derive/derive.go` — `drainTransfers` fails the item on `res.Unknown>0` (fail-closed retry).
- `internal/ml/transfer_heat.go` — `loadTransferHeat` (both entity branches) gates `is_rumor IS TRUE` after
  the latest-per-counterparty pick.
- `internal/maintenance/maintenance.go` — `pipeline_stats.transfer_rumors_active` counts only vetted active
  pairs.
- `internal/ml/vibe.go` + `cmd/sentiment/main.go` — removed `tweetItem`, `VibeResult.InputTweetIDs`,
  `tweetLookback`/`maxTweetItems`, the `input_tweet_ids` INSERT columns, and the `tweetIDs` param.
- `cmd/transfer/main.go` — surface `unknown=N` in the CLI summary.

### SQL (`104_transfer_fail_closed.sql`)
- `DROP FUNCTION seed_transfer_rumors(text,integer)` (unused fail-OPEN Phase-1 seeder — no live callers).
- `DROP`+`CREATE compute_transfer_heat(integer,integer,text)` — requires `te.vetted IS TRUE AND
  pe.vetted IS TRUE`; OUT params now `(heat, components, news_ids)` (dropped `tweet_ids`). Signature change ⇒
  DROP+CREATE (CREATE OR REPLACE can't alter OUT params).
- `ALTER TABLE … DROP COLUMN input_tweet_ids` on `transfer_rumors` and `vibe_scores`.

The `/transfers` read contract (`db.go` `entity_transfers` + `transfers_leaderboard`) already filtered
`is_rumor IS TRUE` after `DISTINCT ON` — no change needed.

## Verification

Pre-deploy, against the live DB (F-015 — authored against the live schema, not the migration files):
- Migration applied + rolled back cleanly (DROP×2, CREATE, ALTER×2); live schema unchanged by the dry run.
- **Vetted-only heat impact:** 1950 active pairs, 425 heat changed, **56 active→inactive, 0 inactive→active**
  (monotonic — tightening only removes unvetted-link heat). 2829 recent links were unscrubbed.
- **Newer verdict supersedes:** for a real served TRUE pair, inserting a newer `is_rumor=FALSE` dropped it
  from heat grounding (1→0); a newer `is_rumor=NULL` kept it absent (0). (All rolled back.)

Post-deploy (live):
- `release.sh` → API healthy, serving `1486b7b`. Migration applied + recorded (`schema_migrations` head =
  `104_transfer_fail_closed`). Schema confirmed flipped (seed fn gone, `tweet_ids` param gone, both
  `input_tweet_ids` columns gone).
- New `compute_transfer_heat(1,15,'NBA')` returns 3 columns. `/api/v1/nba/team/1/transfers` → 200 with vetted
  rumors; `/leaderboard/transfers` → 200.
- New binary wrote 21 `transfer_rumors` rows (14 FALSE, 7 TRUE), **all model-stamped, 0 fail-open**
  (`is_rumor=TRUE & model_version IS NULL` = 0). No schema errors in the API log.

## Deploy mechanics

1. `git fetch` (synced; parallel Sonnet session shares the tree — staged only my 8 files, left
   `099_team_rosters.sql` untracked). `go build ./...` / `gofmt` / `go vet` / `go test ./...` clean. Commit
   `1486b7b`.
2. `scripts/hosting/release.sh` — new binary live, API restarted, `/health/db` 200.
3. Applied `104` **per-file** (NOT `migrate.sh`, to leave 099 alone) + recorded `schema_migrations` (F-006).
4. **F-018:** the restart stranded 2 in-flight transfers (`team/10` NBA, `team/30` NFL — "context canceled");
   requeued the rows leased before T0.
5. **F-020:** enqueued the 3 teams (NBA 1, NFL 1, NFL 3) behind the 6 served fail-open pairs for a
   fail-closed re-vet.

## Quick reference

```bash
# served rows must all be local model-vetted (launch gate — expect 0):
SELECT count(*) FROM (
  SELECT DISTINCT ON (team_id,player_id,sport) is_rumor, model_version, heat, generated_at
  FROM transfer_rumors ORDER BY team_id,player_id,sport,generated_at DESC) l
WHERE is_rumor IS TRUE AND model_version IS NULL AND heat>0 AND generated_at > NOW()-INTERVAL '14 days';

# fail-closed unknowns (model failures awaiting retry):
SELECT count(*) FROM transfer_rumors WHERE is_rumor IS NULL AND generated_at > NOW()-INTERVAL '1 day';
```

## Follow-ups (see findings ledger)

- **F-019** (Open → Session 11): `NewsNarrator` still hard-errors on `{"narratives": []}`; those 3 NFL
  players are the only `pipeline_work` dead-letters.
- **F-020** (launch gate): assert no served rumor lacks a `model_version`.
- **F-021** (optimization): team-grained retry re-runs the whole team; pair-level skip-if-fresh later.
- **F-022** (ops): drop-column migrations release the new binary first, then migrate.
