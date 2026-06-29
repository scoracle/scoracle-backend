-- 099_team_rosters.sql  (Top-Down Roster Coverage — Phase 1)
--
-- Make roster membership a FIRST-CLASS meta fact, independent of whether a player
-- has produced stats. Today player discovery is bottom-up: a `players` row is only
-- created when a player appears in a box score (upsert_player inside
-- _seed_fixture_box_scores), and _purge_statless then deletes any player with no
-- event_box_scores row. A rostered rookie / deep-bench / fresh-signing simply does
-- not exist in our system — no profile, no roster slot, no news.
--
-- This migration introduces `team_rosters`: a season-scoped, sport-agnostic
-- membership table populated by a TOP-DOWN league → teams → players seed (Phase 2).
-- Stats DECORATE a roster entry; they no longer CREATE it. It is the spine of the
-- whole plan (full-roster coverage, roster card, news reach, leaderboard scopes).
--
-- Companion view `player_current_team_roster` resolves each player's CURRENT team
-- (latest active roster row first, most-recent player_stats team as fallback) so
-- stat-less players still resolve to a team for the profile / roster product.
--
-- Additive + idempotent: creates a new table + view only; touches no existing data.
-- Apply BEFORE the Phase-2 seeder runs (the seeder writes this table) and BEFORE the
-- _purge_statless → _purge_off_roster rewrite that reads it.

-- ============================================================================
-- team_rosters — season-scoped membership
-- ============================================================================
--
-- PK (sport, season, team_id, player_id): a player may appear on more than one
-- club in a season (transfer, loan), so team_id is part of the key rather than a
-- single "current team" per player.
--
-- season is the CANONICAL season-year (matches player_stats.season /
-- sports.current_season) — NOT a provider season id. Phase 2 must map the
-- provider's season (e.g. a SportMonks season_id) to the canonical year before
-- writing, so this table joins cleanly to player_stats and sports.
--
-- jersey_number is TEXT on purpose: jerseys are not always integers ("00" in the
-- NBA, leading zeros, the occasional non-numeric placeholder).
--
-- FK to players(id, sport) ON DELETE CASCADE: the Phase-2 seeder always upserts the
-- player before inserting the roster row, so the parent exists. CASCADE matters for
-- the rewritten _purge_off_roster — when a player is legitimately purged (off every
-- current roster AND no box scores) any stale historical roster rows are removed
-- with them, instead of a RESTRICT FK blocking the delete.
CREATE TABLE IF NOT EXISTS team_rosters (
    sport          TEXT    NOT NULL REFERENCES sports(id),
    season         INTEGER NOT NULL,
    team_id        INTEGER NOT NULL,
    player_id      INTEGER NOT NULL,
    jersey_number  TEXT,
    position       TEXT,
    position_group TEXT,
    is_active      BOOLEAN NOT NULL DEFAULT true,
    source         TEXT,
    first_seen     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (sport, season, team_id, player_id),
    FOREIGN KEY (player_id, sport) REFERENCES players(id, sport) ON DELETE CASCADE,
    FOREIGN KEY (team_id, sport)   REFERENCES teams(id, sport)   ON DELETE CASCADE
);

-- Roster-card lookups (a team's roster for a season) ride the PK prefix
-- (sport, season, team_id). Player-centric lookups — the view below, the
-- _purge_off_roster "is this player on any current roster?" guard, and the
-- player-profile current-team resolution — need their own index since the PK
-- leads with team_id, not player_id.
CREATE INDEX IF NOT EXISTS idx_team_rosters_player
    ON team_rosters (sport, player_id);

CREATE INDEX IF NOT EXISTS idx_team_rosters_active
    ON team_rosters (sport, season, is_active) WHERE is_active;

-- ============================================================================
-- player_current_team_roster — resolve each player's CURRENT team
-- ============================================================================
--
-- Roster first (a player's most-recent ACTIVE roster row), player_stats fallback
-- (most-recent season with a team) so a stat-less rostered player still resolves to
-- a team. `source` tells the consumer which arm answered ('roster' vs 'stats').
--
-- "Latest active roster row" rather than a hard sports.current_season match keeps
-- this robust to the season-encoding question above: is_active already means
-- "currently rostered", and ORDER BY season DESC picks the freshest one.
CREATE OR REPLACE VIEW player_current_team_roster AS
WITH roster_current AS (
    SELECT DISTINCT ON (tr.sport, tr.player_id)
        tr.sport,
        tr.player_id,
        tr.team_id,
        tr.position,
        tr.position_group,
        tr.jersey_number,
        'roster'::text AS source
    FROM team_rosters tr
    WHERE tr.is_active
    ORDER BY tr.sport, tr.player_id, tr.season DESC, tr.last_seen DESC NULLS LAST
),
stats_current AS (
    SELECT DISTINCT ON (ps.sport, ps.player_id)
        ps.sport,
        ps.player_id,
        ps.team_id,
        ps.position,
        NULL::text AS position_group,
        NULL::text AS jersey_number,
        'stats'::text AS source
    FROM player_stats ps
    WHERE ps.team_id IS NOT NULL
    ORDER BY ps.sport, ps.player_id, ps.season DESC
)
SELECT
    p.sport,
    p.id AS player_id,
    COALESCE(rc.team_id,        sc.team_id)        AS team_id,
    COALESCE(rc.position,       sc.position)       AS position,
    COALESCE(rc.position_group, sc.position_group) AS position_group,
    rc.jersey_number,
    COALESCE(rc.source, sc.source) AS source
FROM players p
LEFT JOIN roster_current rc ON rc.sport = p.sport AND rc.player_id = p.id
LEFT JOIN stats_current  sc ON sc.sport = p.sport AND sc.player_id = p.id;
