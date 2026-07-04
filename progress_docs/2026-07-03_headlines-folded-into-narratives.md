# 2026-07-03 - Headlines Folded Into Narratives

## Goal

Retire Headlines as a standalone product and fold its useful breaking-story signals into the Narratives/News product.

## Changes

- Added migration `121_fold_headlines_into_narratives.sql`.
- Restored live news derivation flow to `scrub -> transfers -> narratives -> vibe -> sigil`.
- Added source freshness fields, source metadata, and trajectory markers to both `news_summaries` and `transfer_rumors`.
- Changed `/news`, `/transfers`, `/leaderboard/news`, and `/leaderboard/transfers` to accept `scope=current_week|last_week|two_weeks_ago|three_weeks_ago|last_month`.
- Shared the same staleness protocol across Narratives and Transfers: current-week rows marked `cooling_off` retire from live views after three days unless renewed by fresher source/update timestamps.
- Retired public `/headlines`, `/leaderboard/headlines`, and `?board=headlines` routes.
- Fed narrative trajectory markers into Vibe and Sigil prompts so downstream products inherit the richer News read.
- Added deterministic Composite/PEAK z-score trajectory metadata to the stats rail via `stat_summaries.peak_trajectory`, `peak_trajectory_label`, and `peak_trajectory_components`.
- Fed PEAK trajectory into Sigil's Rating pillar and input hash, so the crown can react when recent stat form diverges from the season-long PEAK identity.

## Self-Audit

- Removed the dead Rust `headline` stage source instead of leaving an uncompiled file that referenced a retired `Stage::Headlines` variant.
- Tightened migration cleanup to delete all outstanding `pipeline_work` rows for `stage='headlines'`, avoiding unclaimable operator-dashboard noise.
- Added unit coverage for deterministic source metadata on grounded narratives.
- Kept the old `headlines` table as inert history only; no routes, prepared statements, worker registration, or service config can produce or serve it.
- Applied the same historical scope and cooling-off rule to Transfers instead of creating a separate transfer-specific freshness model; this keeps the flow simple and makes Transfers a constrained facet of News.
- Ordered backfilled/live transfer source-name arrays deterministically, so unchanged source sets do not churn downstream payloads or hashes.
- Kept PEAK trajectory deterministic from the event `rating_composite` and `rating_specialist` z-score values used by the ranking engine instead of asking the rating model to infer recent form; this keeps Rating aligned with actual value metrics while Momentum/Sigil receive a clean recent-form signal.
- Optimization note: the week-scope CTE is intentionally duplicated in the read statements for now. A future SQL helper/function could centralize it once the API contract settles, but the current form keeps prepared statements transparent and avoids introducing a new dependency point during the product fold.

## Verification

- `cargo test --lib`
- `cargo build --bin scoracle-cognition --bin statcommentary`
- `GOCACHE=/tmp/go-build go test ./internal/api ./internal/db ./internal/work`
- `GOCACHE=/tmp/go-build go build -o bin/scoracle-api ./cmd/api`
- `GOCACHE=/tmp/go-build go run github.com/swaggo/swag/cmd/swag@v1.16.6 init -g cmd/api/main.go -o docs`
