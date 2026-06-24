# Pete's Roster & Leaderboard Plan — Top-Down Roster Coverage + Enhanced Leaderboard Scopes

**Plan date:** 2026-06-21 · **Saved as Pete's plan:** 2026-06-23
**Baseline branch:** `main`
**Baseline commit (plan):** `9e2b7fbb55a1f66374185c0429f3115f4b05c319`
**Scope:** Cross-repo — backend ingestion/schema/API (`scoracle-backend`) + frontend roster card and leaderboard (`scoracle-frontend`). News pipeline benefits with minimal code change.

**Status:** PLAN — implementation not started, but Phase 1 is partly pre-drafted in the working tree. Picked up later.

> **Verified state as of 2026-06-23 (read before executing):**
> - Both repos synced with `origin/main` (backend HEAD past the plan baseline).
> - **Phase 1 migration `099_team_rosters.sql` already exists as an *untracked* working-tree file** (table + `player_current_team_roster` view; improves on the DDL sketch below — `jersey_number TEXT`, FK→players/teams `ON DELETE CASCADE`, partial `is_active` index, no `league_id`/`updated_at`). It is **NOT applied** — `team_rosters` is absent from the DB and not in `schema_migrations`.
> - **Numbering collision:** `100_fixture_completeness` is already committed *and applied*. The roster migration sits at `099`, below an applied `100`. The runner keys on filename-version so it would still apply, but **renumber the roster migration to `101`** for chronological sanity (free — it isn't applied yet).
> - Phase 2 (`seed/services/roster/`) not started; Phases 3–5 and the Phase-0 spikes not started.
> - ⚠️ The untracked `099` is likely from the parallel seeding session — confirm ownership before committing/building on it.

---

## ⚠️ Read first (next session) — the purge will delete what you seed

`_purge_statless` (`seed/services/meta/cli.py`) currently **DELETEs** every player with no
`event_box_scores` row. If you run the top-down roster seed (Phase 2) **before** changing this, the next
`meta seed` deletes exactly the stat-less rostered players you just added — silently reopening the
coverage hole this whole plan exists to close. This is the single highest-risk step.

**Solution — do it in the same change as the seeder, never after.** Rewrite `_purge_statless` →
`_purge_off_roster` so a player is deleted only when they are **both** (a) absent from any current-season
`team_rosters` row **and** (b) have no `event_box_scores`. Concretely, the existing DELETE keeps its
`WHERE NOT EXISTS (SELECT 1 FROM event_box_scores …)` and gains a second guard:

```sql
DELETE FROM players p
WHERE p.sport = %s
  AND NOT EXISTS (SELECT 1 FROM event_box_scores ebs
                  WHERE ebs.player_id = p.id AND ebs.sport = p.sport)
  AND NOT EXISTS (SELECT 1 FROM team_rosters tr            -- NEW guard
                  WHERE tr.sport = p.sport AND tr.player_id = p.id
                    AND tr.season = %s AND tr.is_active);
```

(Keep the existing rookie grace-window / draft-year carve-out as a third `OR` branch on the box-score
side.) **Validate on one Football team first** — seed its squad → run the purge → assert every squad
member survives — before enabling the seed for NBA/NFL.

---

## Problem

A coverage hole: we are missing many rostered players. Root cause confirmed — **player discovery is
bottom-up**. A player row is only created when they appear in a box score
(`upsert_player` runs inside `_seed_fixture_box_scores`, `seed/services/event/cli.py`), and
`_purge_statless` (`seed/services/meta/cli.py`) then **deletes** any player with no `event_box_scores`
row. "Roster" is a derived illusion: the `roster` prepared statement (`go/internal/db/db.go`) reads
`player_stats` and gates on `ps.rating_composite IS NOT NULL`. A rostered player who hasn't produced
stats yet (rookie, deep bench, recent signing, transfer-window arrival) simply does not exist in our
system — so they get no profile, no roster slot, and no news.

## The shift (lean into meta)

Make **roster membership a first-class meta fact**, independent of whether a player has produced stats.
Stats *decorate* a roster entry; they don't *create* it. We stop treating `player_stats` as the source
of truth for "who is on the team" and introduce a season-scoped membership table, **`team_rosters`**,
populated by a **top-down league → teams → players** seed. That single change is the spine of all four
asks:

1. Full-roster coverage in meta (top-down seed).
2. Roster card lists the entire roster, not just stat-bearing players.
3. News coverage extends to all rostered players (the news pipeline is already meta-driven).
4. Enhanced leaderboard scopes (drill-down) — backend already capable; this exposes it.

## Product invariants preserved

- Curated derivation engine (compile → scrub → reveal), not a passthrough aggregator. Roster seeding
  is ingestion-only; no response shaping in Python/Go.
- Postgres remains the domain engine — roster membership, position grouping, and all ranking/derivation
  stay in SQL. Go handlers stay thin (parse → cache/ETag → prepared statement → passthrough).
- Per-sport SQL boundaries kept (`nba` / `nfl` / `football`).
- Derived outputs append-only/time-stamped; this plan adds a membership table and reads, it does not
  rewrite derivation history.
- `/roster` stays a single precomputed product endpoint; cards own their data end-to-end.

## Verified current state (2026-06-21 research)

| Area | Finding | Source |
|---|---|---|
| Player discovery | Box-score-driven `upsert_player`; `_purge_statless` deletes no-box-score players | `seed/services/event/cli.py`, `seed/services/meta/cli.py` |
| Football provider | `get_team_squad()` (`/squads/seasons/{id}/teams/{id}`) already wired; `_seed_football_metadata` already walks league→teams→squad, then purge drops statless | `seed/services/event/handlers/sportmonks_football.py`, `seed/services/meta/cli.py` |
| NBA/NFL provider | Code uses historical `/players` + purge. BDL **`/players/active?team_ids[]=`** exists on all tiers (NBA + NFL), cursor-paginated | `seed/services/event/handlers/bdl_nba.py`, `bdl_nfl.py`; BDL docs (needs Phase-0 spike) |
| Schema | Latest migration **098**. No roster/membership table. `players.team_id` is stale ("last seeded"). Position lives on `player_stats.position` (statless players have none) | `sql/shared.sql`, `sql/migrations/` |
| Roster product | `roster` stmt joins `player_stats`, gates `rating_composite IS NOT NULL`. `RosterCard.tsx` assumes scores exist (no null checks) | `go/internal/db/db.go`, `scoracle-frontend/src/components/solid/RosterCard.tsx` |
| News pipeline | **Not** stats-gated. Corpus pipeline loads all `teams` and matches all `players` from meta (`loadEntityPool`). `vibesynth`/`statcommentary` nightly *are* gated (`rating_composite_score IS NOT NULL`) | `go/internal/corpus/corpus.go`, `go/internal/thirdparty/news.go`, `go/cmd/vibesynth`, `go/cmd/statcommentary` |
| Leaderboard | `leaderboard` stmt already filters `position`, `league_id`, `scope` (composite/fantasy/specialist), `conference`, `division`. Frontend `leaderboardUrl()` already accepts a `cohort` object | `go/internal/db/db.go`, `scoracle-frontend/src/lib/utils/data-sources.ts`, `routes/leaderboard.tsx` |

Two consequences that shrink the work:
- **News coverage is nearly free** — once rostered players land in `players`, the nightly corpus
  pipeline picks them up with zero pipeline code change.
- **The leaderboard drill-down already exists in the backend** —
  `/football/leaderboard?entity_type=player&position=Attacker&league_id=8&scope=fantasy` is already a
  valid query. Phase 5 is mostly UI.

## Locked decisions (2026-06-21)

- **Roster card layout:** one list, rated players first (by Composite+Specialist), stat-less players
  follow with "—" for missing ratings. No separate section, no position grouping (those are follow-ons).
- **Sequencing:** ship the **Football vertical slice first** (Phases 1–4 on Football only — the squad
  endpoint is already wired), prove roster → card → news end-to-end, then replicate to NBA/NFL after the
  Phase-0 BDL spike. Leaderboard (Phase 5) is independent and can ship anytime.

---

## Phase 0 — De-risk spikes (do first, ~½ day)

Each spike is a throwaway script / curl, not a commit.

1. **BDL `/players/active` completeness.** Fetch one NBA team and one NFL team via `team_ids[]`.
   Confirm the payload includes never-played rookies / recent signings / deep bench **and** carries a
   position field. The entire NBA/NFL path rests on "active" meaning "rostered," not "has appeared."
   If it only returns players who have logged a game, NBA/NFL fall back to historical-`/players` +
   a softened purge (still an improvement, but note the gap).
2. **News pool cost.** Count `players` per sport now vs. projected full-roster count
   (~3.5k → ~15–20k/sport estimated). Estimate `news_article_entities` growth and the O(pool)
   per-article match cost in `thirdparty/news.go` `loadEntityPool`. Output decides whether Phase 4
   needs a matching optimization or just monitoring.

**Exit criteria:** a yes/no on NBA/NFL active-roster feasibility, and a cost number for news pool growth.

## Phase 1 — Schema foundation (migration 099, Football-first but sport-agnostic)

New table **`team_rosters`** (sport-agnostic; one table, all sports):

```sql
CREATE TABLE IF NOT EXISTS team_rosters (
    sport           TEXT    NOT NULL REFERENCES sports(id),
    season          INTEGER NOT NULL,
    team_id         INTEGER NOT NULL,
    player_id       INTEGER NOT NULL,
    league_id       INTEGER NOT NULL DEFAULT 0,
    jersey_number   INTEGER,
    position        TEXT,                 -- provider-raw (feeds position_group())
    position_group  TEXT,                 -- normalized via public.position_group()
    is_active       BOOLEAN NOT NULL DEFAULT true,
    source          TEXT,                 -- 'sportmonks_squad' | 'bdl_active' | ...
    first_seen      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (sport, season, team_id, player_id)   -- allows multi-club in one season (loans/transfers)
);
CREATE INDEX IF NOT EXISTS idx_team_rosters_team   ON team_rosters(sport, season, team_id);
CREATE INDEX IF NOT EXISTS idx_team_rosters_player ON team_rosters(sport, season, player_id);
```

New view **`player_current_team_roster`** — resolve current team from `team_rosters` first (covers
stat-less players), falling back to `player_stats` (today's `player_current_team` logic). Gives every
rostered player a correct current club/team for the first time.

Migration applied by the standard `sql/migrate.sh` runner (records in `schema_migrations`).

## Phase 2 — Top-down seeder (the coverage fix)

New ingestion command `scoracle-seed roster seed --sport <s> --season <yr> [--league <id>]`
(`seed/services/roster/`), run **before** event seeding so rosters pre-exist box scores.

- **Football (slice 1):** lift the existing league→teams→squad walk out of `_seed_football_metadata`
  into the roster command; persist each squad member into `team_rosters` (reuse `upsert_player`,
  `upsert_team`, `upsert_provider_entity_map` from `seed/shared/upsert.py`). `get_team_squad()` already
  returns jersey + position IDs.
- **NBA/NFL (slice 2, after Phase-0):** swap `/players` (historical) → `/players/active?team_ids[]=`
  iterated over `get_teams()`; persist into `team_rosters`.
- **CRITICAL LANDMINE — the purge.** Rewrite `_purge_statless` → `_purge_off_roster`: delete a player
  only if they are **neither on a current `team_rosters` row nor have any `event_box_scores`**. Without
  this, the players we just seeded get deleted on the next `meta seed`. This is the single highest-risk
  change in the plan.
- Update `players.team_id` / `players.league_id` from the roster (retires the staleness bug for the
  current-club autofill path).
- Cron (`scripts/hosting/crontab.example`): weekly roster seed (tighter cadence near transfer windows),
  scheduled **ahead of** `event load-fixtures` / `event process`.

## Phase 3 — Roster product shows the whole roster (ask #2)

- **Backend** (`go/internal/db/db.go`, `roster` stmt): read from `team_rosters` **LEFT JOIN**
  `player_stats` (and `players` for name/image); **drop** the `ps.rating_composite IS NOT NULL` gate.
  Order: rated players first by `(rating_composite + rating_specialist) DESC`, stat-less players after
  (stable by name). Carry `position` / `jersey_number` from the roster row. Rating fields become
  **nullable** in the JSON payload. New prepared statement registered per the prepared-statement rule.
- **Frontend** (`scoracle-frontend`): `roster.server.ts` — make `rating_*` fields nullable.
  `RosterCard.tsx` — null-guard `rating_composite_score` / `rating_peak_score` (render "—"), keep
  position/jersey display. **Layout = one list, rated-first, "—" for blanks** (locked).

## Phase 4 — News for everyone (ask #3 — nearly free)

- **No pipeline code change.** Once Phase 2 lands rostered players in `players`, the nightly corpus
  pipeline matches them when they're mentioned (`loadEntityPool` already enumerates all players).
- Seed `search_aliases` for new players (match quality).
- Apply the Phase-0 cost decision: monitor `news_article_entities` growth; optimize the per-article
  match loop only if the spike flags it. Consider extending the maintenance `pipeline_stats` coverage
  snapshot to count rostered-but-statless players now reachable.
- **Leave `vibesynth` / `statcommentary` stats-gated.** A stat-less player rightly gets a vibe *when
  newsworthy* (via corpus), not a fabricated stat narrative.

## Phase 5 — Enhanced leaderboard scopes (ask #4 — mostly UI, independent)

- **Backend:** already supports it; verify with curl
  (`/football/leaderboard?entity_type=player&position=Attacker&league_id=8&scope=fantasy`). Add a scope
  dimension only if a gap is found (e.g. player-level division).
- **Frontend** (`routes/leaderboard.tsx`): add `league`, `position`, and metric/`scope` selectors to
  `ScopeStrip`; add the URL params; pass through the already-plumbed `cohort` in `leaderboardUrl()`.
  Position options per sport (football: GK/Def/Mid/Att; NFL: QB/RB/WR…); league options from `/meta`.
  Optionally surface the hidden Sigil board tab.
- Note: the leaderboard ranks **stat-bearing** players (you can't rank a stat-less player by fantasy
  output), so this is independent of roster coverage — full rosters don't change the leaderboard
  universe, they just make the existing drill-down more valuable as catalog depth grows.

---

## Risk register

| Risk | Mitigation |
|---|---|
| **Purge deletes freshly-seeded rostered players** | Phase 2 `_purge_off_roster` rewrite — gate deletion on roster membership AND no box scores. Test on a Football team before NBA/NFL. |
| BDL "active" ≠ "rostered" (only game-loggers) | Phase-0 spike decides; fallback keeps historical-`/players` + softened purge. |
| News pool 4–6× growth slows per-article matching / bloats `news_article_entities` | Phase-0 cost spike; optimize match loop or batch pool refresh only if measured. |
| Roster card crashes on null ratings | Phase 3 null-guards on both ends; nullable types. |
| Stale `players.team_id` powering current-club autofill | Phase 2 updates it from roster; Phase 1 view prefers roster. |
| Multi-club in one football season (loans) | `team_rosters` PK includes `team_id`; current-team view picks most recent. |

## Per-commit doc workflow

Each phase lands its own commit with a progress doc in `docs/progress/YYYY-MM-DD_*.md` (in-repo) and a
mirror in `~/scoracleWiki/Progress/scoracle-backend/` (and `scoracle-frontend/` for the card/leaderboard
commits), per `wiki/CONVENTIONS.md`.

## Dependency order

```
Phase 0 (spikes)
  └─► Phase 1 (migration 099: team_rosters + view)
        └─► Phase 2 (roster seeder — Football, then NBA/NFL) ──► Phase 4 (news, free)
              └─► Phase 3 (roster product + card)
Phase 5 (leaderboard scopes) — independent, ship anytime
```
