# Three-character production release — November 2, 2026

User authorized commit, push and production activation, then explicitly approved independent publication and daily cycles requiring only Journalist, Insider and Influencer.

Code: `0f515c0c` (paragraph prompts and ten articles), `93e3aa7d` (preserves Archbox's deployed d3 v11 classifier), `9a00d628` (independent cards and required daily stages), `ce42ce2b` (Insider findings follow their generating assignment). Binaries are stamped `9a00d628ceab`; Rust sources are identical through `ce42ce2b`. Exact hashes and locations: `manifest.json`.

The Mac runs `narratives,vibe,transfers`, Qwen3.5-4B-Q4_K_M, thinking disabled, temperature zero, the existing four production slots and 10,000-token total window. Complete **input** sizes are separately measured in `live-token-summary.json`. Controlled development trials used one slot, so production measurements are not paired accuracy results.

Archbox runs the classifier/controller and RSS coverage; coverage is enabled at boot. The API and d3 model were kept online. Only the cognition binary was replaced: Go code had no changes against the deployed release. Required daily stages are now `{narratives,vibe,transfers}`. Paused roles' unfinished assignments remain recorded; editions with unfinished roles remain partial. A completed candidate from one of the three news roles can be served independently, including source-bound Insider findings. Pending/failed assignments stay hidden. Completion changes invalidate API caches.

Production-derived logs, database snapshots and API bodies remain local or on Archbox; they are not included in the GitHub commit. This directory’s committed files contain the release summary, aggregate token counts, manifest and recovery/test SQL.

## Recovery and receipts

Server-private backups hold the old binary, environment, complete schema, coverage/queue/delivery tables (custom-format dump), and scoped before-row JSONL files. Mac's release directory has the old launcher and environment. Source screening ran on Archbox; source payloads were not exported for recovery. One automatic review rejected the proposed source export; the safer path screened in place and exported only assignment keys.

- Journalist: explicit 1,719-assignment scope excluded existing candidate products. Preview and apply both changed 385 packets. Source currency, identity, and existing hold checks remained active. Top-ten selection excluded lower-ranked/duplicate/spam evidence with receipts.
- Changed-prompt retry: 199 Insider, eight Journalist, one Influencer assignment passed source currency and identity checks. Existing products and prepared packets were excluded. Preview rolled back; apply saved row backups.
- Live verification caught a mistake in the one-time retry marker: a prefix displaced Insider's `classifier-spectrum-v1` routing prefix, causing 197 jobs to take the legacy path and fail **before inference**. `fix_retry_routing.sql` corrected only those jobs under the claim lock, with another private backup. The two already terminal abstentions were left intact. The original script is retained with a DO NOT RUN header; the corrected script appends the marker. Tests now assert the routing prefix survives.
- No identity holds, stale-source holds, completed products, model validators, or raw rejection receipts were cleared to make this release pass.

`production-migrate-preview.log`, `production-recovery-apply.log`, `production-activation.log` and `production-routing-fix.log` contain the mutation receipts. Live raw requests and rejected model replies remain in the database and host logs. Local startup logs redact transient claim tokens.

## Verification

- Full SQL baseline/migration/recovery, Rust and Go validation passed after the d3 merge (`merge-check.log`) and after the required-stage/publication changes (`three-release-full-check.log`).
- Final baseline and publication regressions passed after adding Insider findings (`final-baseline.log`, `final-publication-tests.log`). Tests cover pending/failed visibility, cache invalidation, dated history, abstention, paused-stage preservation, and target versus related-entity completion.
- One-time retry tests cover rollback-only preview, stale sources, identity holds, prepared packets, existing products, top-ten selection, backups, repeat refusal and the classifier routing prefix (`retry-routing-tests.log`).
- `postflight-*.json` record product counts and queue state at explicit times. Generated product rows are not necessarily qualified cards: failed/partial assignments can have saved rows that stay hidden.
- Public API checks returned HTTP 200 for new Journalist, Influencer and Insider cards. The frontend homepage was reachable. Python's initial requests received HTTP 403; ordinary curl checks succeeded without changing access controls. Keep that transport failure distinct from API health.

At 18:42:50 EST, 21 new product rows existed: 15 Journalist, five Insider and one Influencer. Seven were independently qualified: one Journalist, five Insider and one Influencer. The other saved rows remained hidden because their assignments were not qualified. Across 91 observed native model calls, complete input ranged from 874 to 2,260 tokens. See `postflight-final.json`, `live-token-summary.json` and the API receipts. Counts continue changing while the workers run.

## Limits and follow-up

The frozen backlog is the October 22 reporting day / October 23 edition. Recovered cards retain those dates; they are not relabelled as today's news. RSS collection resumes when the three required characters settle this cycle.

Early live Insider replies still hit source/name/number validation failures. Accepted JSON and successful queue completion do not establish factual accuracy. Actor attribution, unsupported interpretation and false abstentions remain review targets; parsers were not relaxed. This release starts the requested overnight evidence collection, not a claim of solved accuracy.

For an operational rollback, stop the Mac writer and Archbox classifier/controller first, preserve current receipts, restore the saved launchers/binaries/configuration and restart only the intended services. The migrations are forward-compatible with the saved worker binary. Reverting publication policy requires restoring the saved view/functions under the coverage lock. Do **not** blindly restore whole database tables after new generations have committed; use the scoped before-row receipts and current claim/version fences.

The unrelated production-recovery planning document and `fixtures/journalist/lfm3-comparison-2026-11-02/` were left outside these commits.
