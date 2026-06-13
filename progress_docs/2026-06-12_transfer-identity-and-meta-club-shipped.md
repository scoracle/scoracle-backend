# 2026-06-12 — Transfer identity vetting + meta current-club (CODED, pending prod apply)

Follow-up to `progress_docs/2026-06-11_transfer-identity-vetting-and-meta-season-fix.md`
(the plan). Both workstreams are now **coded, validated read-only, committed to
`main`** — but **not applied to prod and not restarted**. Finish on Archbox.

## ⚠️ Pick-up on Archbox: `git fetch && git pull --ff-only` FIRST
Then the runbook at the bottom. The Go API prepares every statement at boot, so the
migration MUST be applied to prod **before** the API restart, or the restart fails fast.

## Goal (recap)
- **#1** Transfers · Heat ranked the wrong entity on co-mention noise (Nícolas←Nicolas
  Jackson; "Florentino" the midfielder ← Florentino Pérez, Real Madrid president; both
  João Pedros + a bare "Pedro" on one Chelsea article).
- **#2** Search/meta + the vibes/news boards showed a player's most-recently-**seeded**
  club, not most-recently-**played** (Pulisic→Chelsea, Bellingham→Dortmund, Olise→Crystal
  Palace).

## What changed vs. the plan
The plan's direction was right; three corrections + one scope expansion (per Scott):
1. **#2 is a migration, not an edit to `sql/football.sql`.** The live `autofill_entities`
   is migration **044** (the base SQL file is even behind it — no `position` column).
   Fix ships as migration **076**.
2. **Scope = all sports, all consumers** (not just football autofill). The same stale
   `players.team_id` join also lived in `db.go` `vibes_leaderboard` + `news_leaderboard`,
   and structurally in the NBA/NFL autofill MVs. All fixed.
3. **One canonical source** — `public.player_current_team` — so the four consumers can't
   drift. Club = `COALESCE(player_current_team.team_id, players.team_id)`: latest-played
   wins for the 99.5% with a stats row; the old field is a fallback only for the 40
   stat-less football players (+ NBA draftees / NFL rookies), so nobody loses a club.
4. **Prompt version bumped `t1→t2`** (the const's own rule; the plan omitted it).

## Accomplishments

### #2 — meta current-club  (commit `bd04325`)
- `sql/migrations/076_autofill_current_club.sql` — new view `public.player_current_team`;
  DROP+CREATE the nba/nfl/football `autofill_entities` MVs (byte-for-byte migration 044
  except the club join) + recreate the `(id,type)` unique indexes; smoke NOTICE block.
- `go/internal/db/db.go` — `vibes_leaderboard` and `news_leaderboard` player branches
  resolve club from a latest-season `player_stats` LATERAL (mirrors the canonical view;
  inlined for per-request index pushdown via `player_stats_pkey`).

### #1 — transfer identity vetting  (commit `8eddbdc`)
- `go/internal/ml/transfer.go`:
  - `transferCandidate` carries `nationality`/`currentClub`/`position`; `loadCandidates`
    fetches them (club from `public.player_current_team`, **not** `players.team_id`).
  - `buildTransferPrompt` emits an identity card led by current club (nationality is only
    ~39% covered, so club + position carry it).
  - `transferSystemPrompt` rewritten: `is_rumor=true` ⇒ THIS exact player AND a real
    transfer; explicit name-collision guard (the president example).
  - Gemma returns `subject`; persisted into the existing `trigger_payload` JSONB as a
    discard audit trail (no migration — column exists since 031).
  - Discard stays lossless: the wide net also links the correct entity, so the existing
    `is_rumor=false` read filter does the dropping. No `match.go` / capture change.

## Validation (read-only, local `scoracle` DB — prod untouched)
- Migration club-resolution dry-run: Pulisic→AC Milan, Jude Bellingham→Real Madrid,
  Olise→FC Bayern München. Collisions split cleanly: Nícolas/Pisa vs Nicolas Jackson/Bayern,
  Bernardo/Hoffenheim vs Bernardo Silva/Man City, João Pedro 28931574/Chelsea vs
  129664/Cagliari. → "appears twice" = two real players, not duplicate rows; the identity
  card resolves it (the plan's dedup risk does not bite here).
- Pre-flight signal: nationality 38.7% (sparse → lead the card with club), current club
  resolvable for 8228/8268 (99.5%), 1784 players (21.6%) had a stale `players.team_id`.
- Go: `gofmt` clean, `go build ./...` OK, `go vet` OK, `internal/ml` tests pass.

## Quick reference
- `sql/migrations/076_autofill_current_club.sql` — the meta fix (view + 3 MVs)
- `public.player_current_team` — canonical current-club source; use instead of `players.team_id`
- `go/internal/db/db.go` — vibes/news boards (search `cur.team_id`)
- `go/internal/ml/transfer.go` — identity card + `t2` prompt + `trigger_payload` audit
- `seed/shared/upsert.py:98` — where `players.team_id` goes stale (left as-is by design;
  it's "last-seeded", so the robust fix is to never trust it for current club)

## Runbook to finish on Archbox  (each step is gated/manual)
1. `cd scoracle-backend && git pull --ff-only`
2. Apply the migration to **prod**: `DATABASE_PRIVATE_URL=… ./sql/migrate.sh`
   (or `psql "$PROD_URL" -f sql/migrations/076_autofill_current_club.sql`). `CREATE … AS`
   repopulates the MVs — no separate REFRESH. Watch the `076 …` NOTICE lines.
3. Restart the Go API: `systemctl --user restart scoracle-api` (validate prepared
   statements first; never pkill by pattern — prod shares the repo bin path).
4. #1 only takes effect as rows regenerate under `t2` — re-run transfer generation for
   the affected teams (e.g. Chelsea id 18, Atlético), then QA: Nícolas / João Pedro /
   Bernardo / Florentino should drop off the wrong coverage.
5. No frontend `cf:deploy` — nothing in the frontend changed.
