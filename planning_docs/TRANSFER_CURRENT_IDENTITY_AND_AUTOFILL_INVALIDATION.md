# Transfer-Driven Current Identity + Sport Autofill Invalidation

**Plan date:** 2026-07-03  
**Status:** PLAN  
**Scope:** backend schema/API/seeding/cognition pipeline, with frontend cache behavior documented.

## Problem

Current player identity is still vulnerable to historical backfill order. A retro-seed can make a player look attached to an old club/team, and that stale identity is not just cosmetic:

- Michael Olise can appear as a Crystal Palace player after historical football seeding.
- Cole Palmer can appear as a Manchester City player, causing the transfer pipeline to treat an old Manchester City to Chelsea move as live Chelsea transfer news.

That is active data contamination. The transfer system must not evaluate live transfer candidates against mutable "last seeded" identity.

## Product Contract

- `GET /api/v1/{sport}/{entityType}/{id}/meta`
  - Dedicated entity identity island.
  - Current team/club, position, image, display name, tier, etc.
  - No broad frontend local metadata DB.
- `GET /api/v1/{sport}/autofill`
  - Sport-scoped local search DB.
  - Text-only and lightweight: `id`, `type`, `sport`, `name`, `team`, `position`, `aliases`, `search_tokens`, `generated_at` / `version`.
  - No photos, logos, stat definitions, leagues arrays, venue/bio/profile blobs.
- Optional `GET /api/v1/autofill`
  - Manifest or all-sports bootstrap only if useful.
  - Must preserve sport partitions. Canonical operational unit remains the sport DB.

The frontend stores autofill by sport. When one sport's autofill version changes, the frontend purges only that sport DB and fetches the new one.

## Core Rule

Historical stats are immutable for this purpose. Current identity is mutable, audited, and reversible.

Do not rewrite historical `player_stats`, `event_box_scores`, `team_stats`, or historical roster rows to reflect a transfer. Update only current identity state.

## Phase 0: Stop Current-Identity Contamination

This is the first implementation phase because false current teams poison transfer scoring.

1. Define one canonical current-identity source for players.
   - Prefer a roster/current-identity table or view, not raw `players.team_id`.
   - Current source order:
     - explicit current override / applied transfer row, if present and active
     - latest active `team_rosters` row for the sport/player
     - latest real `player_stats` season as a fallback
     - `players.team_id` only as a last-resort legacy fallback, with source flagged
2. Prevent retro-seeds from clobbering current identity.
   - `upsert_player()` should not blindly update `players.team_id` from arbitrary provider payloads.
   - Historical `meta seed` and event backfills may write season-scoped stats/rosters, but should not overwrite current team identity unless the seed is explicitly the current roster/current season sync path.
3. Add a repair/backfill command.
   - Recompute current identity per sport from the canonical source.
   - Produce a dry-run report of mismatches such as `players.team_id != canonical_current_team_id`.
   - Include spot checks for known movers: Cole Palmer, Michael Olise, Christian Pulisic, Jude Bellingham.
4. Add transfer read guards.
   - If the candidate destination team already equals canonical current team, do not serve it as a live transfer.
   - If the candidate source team equals a historical team but not the canonical current team, require explicit "return" or "leaving current club" evidence.
   - Existing former-player guard remains, but it should use canonical current identity rather than stale player metadata.

## Phase 1: Applied Transfer Mechanism

Create an idempotent, audited applied-transfer mechanism.

Required fields:

- `source_rumor_id` and/or `source_synthesis_id`
- `sport`
- `player_id`
- `old_team_id`
- `new_team_id`
- `old_league_id` / `new_league_id` where relevant
- deterministic score / confidence
- LLM adjudication decision and confidence
- reason
- evidence payload
- `applied_at`
- `applied_by` / `source`
- `reverted_at`, `reverted_by`, `revert_reason`

Idempotency key should prevent applying the same player/team/source transition repeatedly.

## Phase 2: Threshold + Mistral Adjudication Gate

Automatic application requires two gates:

1. Deterministic transfer/trade confidence crosses configured threshold.
2. Mistral adjudication confirms the candidate as a real current move.

Mistral receives a compact evidence packet:

- player identity
- canonical current team/league
- proposed new team/league
- source articles/snippets
- extracted entities
- deterministic confidence
- sport-specific context

Mistral returns strict JSON:

```json
{
  "decision": "apply | reject | manual_review",
  "event_type": "transfer | trade | loan | signing | extension | rumor | false_positive",
  "confidence": 0.0,
  "old_team_id": 0,
  "new_team_id": 0,
  "reason": "",
  "evidence_spans": []
}
```

Fail closed on invalid JSON, unknown IDs, conflicting teams, low confidence, unsupported event type, or any mismatch between the model response and the deterministic candidate.

Mistral confirms or rejects a candidate. It does not invent canonical IDs or write identity fields directly.

## Phase 3: Current Metadata Write

When both gates pass:

- Insert applied-transfer audit row.
- Update current identity fields only.
- Prefer structured fields or current override table.
- If using `players.team_id` / `players.league_id`, only update through the applied-transfer/current-roster sync path, not through historical seeding.
- Update `players.updated_at` or the current identity row timestamp so meta/autofill versions advance.

Do not touch historical stat rows.

## Phase 4: Meta + Autofill Resolution

Route all current-team consumers through the same canonical current-identity source:

- `GET /api/v1/{sport}/{entityType}/{id}/meta`
- `GET /api/v1/{sport}/autofill`
- transfer candidate identity cards
- vibe/news/transfer leaderboards when they display current team
- universal text search if it includes team text

The autofill payload remains sport-scoped and lightweight. If the legacy `/meta` payload still backs `/autofill`, migrate intentionally rather than growing it further.

## Phase 5: Sport-Scoped Autofill Versioning

Add an invalidation mechanism per sport.

Required:

- sport
- version or content hash
- generated_at
- total_entities
- ETag support if compatible with existing handlers

Update only the affected sport version whenever:

- applied transfer changes current identity
- roster/current identity sync changes current team text/tokens
- player/team display name or alias changes

Frontend detection can be either:

- `GET /api/v1/{sport}/autofill` carries version/ETag, or
- tiny `GET /api/v1/{sport}/autofill/status` manifest.

Unrelated sports must not be invalidated.

## Phase 6: Tests

Required tests:

- historical retro-seed does not overwrite canonical current team
- current roster/current identity sync updates canonical current team
- Cole Palmer-style old move to current team is rejected as live transfer
- threshold-crossing transfer applies once
- below-threshold transfer does not apply
- Mistral `reject` / `manual_review` does not apply
- invalid Mistral JSON fails closed
- applying transfer updates player current team in `/meta`
- applying transfer updates sport autofill team text and tokens
- old sport autofill version changes after apply
- unrelated sports are not invalidated
- historical stats/team rows are not rewritten
- idempotency prevents repeated application
- revert path restores prior current identity and advances sport autofill version

## Risk Controls

- Keep automatic writes auditable and reversible.
- Prefer canonical structured fields over opaque metadata blobs.
- If conflicts exist, require manual review.
- Loans/temporary moves require explicit representation before auto-apply.
- Speculation language should not auto-apply.
- Multiple destination teams means manual review.
- Official/team/player-source reports can use a lower deterministic threshold, but still require adjudication.

## Implementation Notes From Current Code

- `seed/shared/upsert.py::upsert_player()` currently writes `team_id = COALESCE(EXCLUDED.team_id, players.team_id)`. That is the contamination path for retro-seeds.
- `public.player_current_team` from migration `076` uses latest `player_stats` and avoids `players.team_id`, but it does not cover every current-roster case.
- `team_rosters` / `player_current_team_roster` exists in migration `099` and is the better foundation for statless/current roster identity.
- `rust/src/transfer.rs::load_candidates()` currently reads `public.player_current_team`; it should read the final canonical current-identity view.
- `go/internal/db/db.go::entity_meta` currently reads `public.player_current_team`; it should read the final canonical current-identity view.

## Done Criteria

The plan is complete only when:

- retro-seeding cannot make active identity older
- profile meta shows canonical current team
- transfer prompts use canonical current team
- historical moves to the already-current team are filtered out
- applied transfers update current identity quickly and audibly
- sport autofill versioning causes only the affected sport cache to refresh
