# 2026-06-09 — Durable player position in finalize_fixture (migration 049)

## Goal
Stop NFL (and NBA/football) player `position` from re-emptying when a season is
re-aggregated — the durability follow-on to the 048 backfill, so the counting-stat pizza
doesn't silently break again.

## Root cause
Old `event_box_scores.position` is an EMPTY STRING (''), not NULL. finalize_fixture
derived position via `(array_agg(e.position) FILTER (WHERE e.position IS NOT NULL))[1]`
— '' passes `IS NOT NULL` — and persisted it with
`position = COALESCE(EXCLUDED.position, player_stats.position)`, where COALESCE treats ''
as a real value. So re-finalizing an old fixture would overwrite a good position (incl.
the 048 backfill) with '' → position_group→NULL → no template.

## Fix (redefines finalize_fixture, all three sport branches)
- Position derivation now ignores empty-string events and falls back to the player's
  canonical `players.meta->>'position_abbreviation'` when the event gives nothing:
  `COALESCE(NULLIF(array_agg(...) FILTER (WHERE NULLIF(e.position,'') IS NOT NULL)[1], ''),
            (SELECT NULLIF(pl.meta->>'position_abbreviation','') FROM players pl WHERE pl.id = e.player_id AND pl.sport = v_sport))`
- `ON CONFLICT` guards with `position = COALESCE(NULLIF(EXCLUDED.position,''), player_stats.position)`
  so a stale '' can't overwrite an existing value.
- The meta fallback is a no-op where meta lacks the key (NBA/football today), so it's safe
  to apply uniformly; position now self-populates on each fixture's next finalize.

## Accomplishments
- `sql/shared.sql` — finalize_fixture position derivation + ON CONFLICT updated (×3 branches).
- `sql/migrations/049_finalize_position_durability.sql` — CREATE OR REPLACE finalize_fixture
  (DDL only; the function REFRESHes materialized views CONCURRENTLY, so it can't be invoked
  inside the migration txn — no functional smoke here).

## Verification
- Throwaway PG: derivation returns event position when present, meta abbreviation for
  empty/null events, NULL when neither; ON CONFLICT keeps the existing value against a
  stale ''. Prod: dry-run (ROLLBACK) compiled, then applied → COMMIT. No code deploy needed
  (DB function; effective on the next seed). The 048 backfill is now regression-proof.

## Result
Re-aggregating any season self-populates position from meta instead of emptying it. The
~58% of old NFL rows that are 'UNK' in meta stay empty (genuinely unknown — correct).
