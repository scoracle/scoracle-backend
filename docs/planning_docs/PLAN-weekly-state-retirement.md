# Retire weekly processing state

A week should be a requested time window, not a reason to generate another reading. Preserve timestamped facts and character products; calculate numerical studies from bounded inputs when an assignment or analytical request needs them. Reuse the current DuckDB snapshot boundary. Cheap indexed archive reads can stay in PostgreSQL.

The September 20 caller audit found three separate mechanisms:

| Existing behavior | Treatment |
|---|---|
| `refresh_momentum_scores` aggregates weekly Vibe/event scores. Go runs it on dirty-source notifications with a five-minute throttle and a fifteen-minute catch-up. | It is already refreshed during the week. Replace its analytical formula/producer only after comparing a bounded, request-time DuckDB study. Do not keep two competing producers. |
| The Rust desk calls `seal_weeks` hourly. In a week's last six hours, SQL re-enqueues six character seats for every entity that generated that week. It also persists `week_seals`. | Retire the calendar-driven regeneration. Derive the archive's closed status from its end timestamp, then remove the caller, function and seal table. Preserve held jobs, active claims and durable product history. |
| `season_weeks`, nine card-stamping triggers, stored `week_season`/`week_no`, `week_of`, `restamp_card_weeks` and a nightly calendar rebuild maintain reporting labels. | Replace consumers with explicit `[start, end)` windows before removing denormalized stamps and calendar maintenance. Keep one definition of timezone and season/week boundaries. |

`entity_headlines` already reads each character's timestamped products inside a requested window. Vibe/Sigil leaderboards still filter stored week stamps. Other entity queries and `/weeks` resolve their windows from `season_weeks`. The web profile and leaderboard use the week-navigation API, so deleting calendar objects first would break a current feature.

Preserve the existing season opener, Monday boundary, timezone and cross-season meaning until an explicit product change replaces them. An on-demand calculation still needs a defined window. A newly corrected source can legitimately revise an analytical result; original published prose remains dated history. Source timestamps must distinguish event time from when evidence became available if a study promises an historical "as known then" result.

Implementation order:

1. Replace seal-dependent API status with a time comparison and remove week-close re-enqueueing.
2. Move stamped archive/leaderboard filters to the same timestamp windows; check null-marker handling, boundary instants, DST and season transitions.
3. Remove unused stamping triggers/columns/helpers, seal state and calendar rebuilding with a forward migration after backing up history. Generate boundaries from minimal season-clock information, preserving current navigation responses.
4. Evaluate request-time DuckDB Momentum/Vibe studies against retained inputs and current consumers. Keep computation bounded and reuse a result by source hash where repeated requests would repeat the same scan. A page read must not trigger an LLM call.

This is a caller-backed retirement plan, not an applied migration. The adjacent Vibe-history experiment tests richer context independently; it does not change weekly production behavior.
