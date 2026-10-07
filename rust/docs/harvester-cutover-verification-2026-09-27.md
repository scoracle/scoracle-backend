# Harvester cutover verification — 2026-09-27

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

## Production state

The cognition worker is running commit `3d7bcf74` with `HARVESTER_INGEST_ENABLED=1`, `HARVESTER_SHADOW_MODE=1`, and an empty `HARVESTER_DELIVERY_CHARACTERS`. Editor remains the live publisher. The bounded canary briefly disabled shadow publication with character delivery still held, then restored these settings. Both the cognition service and its path watcher are active. The PR checks (Go, schema, shell) and focused Insider parser tests pass.

## Observed handoff

Two read-only-source replays from ingest run 333 exercised the v2 headline gate and v5 context contract. In the source-diverse replay, 12 articles produced 25 headline edges; six articles were rejected by every edge without a publisher fetch, and six were admitted and fetched. All seven resulting context edges matched the stored publisher body at the byte offsets and SHA-256 hash. There were no acquisition errors in that replay. In the first replay, three admitted articles from one publisher received HTTP 429 and remained retryable; this is a source-specific fetch limitation, not a Laya rejection.

A five-article bounded live canary then completed with no failed or missing Harvester replay receipts. Its nine headline edges included six admissions and three rejections. All six published context edges matched the stored body byte-for-byte. They created 21 plugin-recommended assignments, 26 resolved links, nine unresolved names, and four Graph extraction receipts. No character assignment was released by the Harvester publication itself.

Character releases were scoped to this canary batch. Two Journalist assignments became `used`; both point to `news_summaries` with `source=harvester` and the source article in `input_news_ids`. Three Influencer assignments yielded one `used` vibe score whose `input_news_ids` contains its source article, and two abstentions. Five Scout assignments yielded four `relevant_but_unused` and one abstention, so this sample did not prove a Scout rating product. One Insider assignment reviewed eight resolved source pairs, all `cleared`; eight `transfer_rumors` rows carry the Harvester trigger and exact source article, with zero positive rumors. Its source assignment abstained, as no move was supported. Positive rumor publication remains unproven by this sample.

The Insider canary exposed two overly strict negative-verdict checks: stray stage/quote fields and a changed subject string. The plugin now discards those fields on a negative verdict. A positive verdict still requires the resolved subject and an exact publisher quote. The corrected worker was installed and the same canary source settled without those errors.

The Insider queue item then completed its identity and scored-board follow-ups. The batch audit shows no unheld pending character assignment; the remaining pending assignments are explicitly held.

## Backup restore drill

After the timed run 333 sweep, `RESTORE_SOURCE_MODE=harvester scripts/hosting/restore-drill.sh /mnt/data/backup/scoracle/scoracle-20260927T050903Z.dump` passed on `archbox`. It restored the pre-Harvester snapshot into a throwaway database, applied migrations 267–286 (20 migrations), and matched the live migration ledger at 288 entries. Every critical table was nonempty, all 11 checked primary keys were present, and the restored schema had 265 public indexes and 114 public functions, matching the live counts. The Go API registered every prepared statement against the restored database. The script dropped the throwaway database on exit. This proves the backup can be restored and migrated to the current schema; it does not prove the still-open full-nightly v5 or character publication gates.

Run the batch audit on the production host with:

```sh
psql "$DATABASE_PRIVATE_URL" -X -qAt -v ON_ERROR_STOP=1 \
  -v run_id=333 -v canary_after='2026-09-27 11:48:10-04' \
  -f rust/examples/harvester_live_canary_check.sql
```

## Cutover gates still open

1. The next complete nightly ingest must produce v2/v5 coverage and a drained queue. Run `rust/examples/harvester_nightly_report.sql` against that run. The current run 333 was created before the current contract and cannot prove full production coverage.
2. Release each character on a small current-contract cohort, confirm terminal dispositions and actual product provenance, then expand. Scout needs a source that can produce a grounded rating; Insider needs a source that can support a positive transfer claim. Abstention on the current canary is correct but does not test those publication paths.
3. The `/stories`, `/story/{id}`, and related Go statements still read Editor-owned `storylines`, `storyline_articles`, and `packets`. Turning off Editor intake now would leave those app surfaces stale. Provide a Harvester-backed Stories contract or explicitly keep that Editor duty until its replacement is verified.
4. Keep source acquisition errors separate from relevance decisions. The observed HTTP 429s need their own retry/backoff and coverage check during a full run.

Do not remove Editor or its tables yet. The current live setting remains shadow mode while these gates are open. Probability calibration is tracked separately from this cutover verification.
