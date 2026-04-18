"""Seed logo + headshot URLs from api-sports.

Scope is intentionally narrow: api-sports is used only for team logos
and player photos. Box scores, fixtures, season stats, and roster
data all continue to come from BDL/SportMonks — see CLAUDE.md for
the division of responsibilities.

Matching
--------
api-sports uses its own entity IDs that don't align with BDL. To bridge
them we:

1. Try an existing `provider_entity_map` row (provider='api-sports').
   If present, we already mapped this entity — skip matching.
2. Match by `short_code` / abbreviation (`code` in api-sports).
3. Fall back to normalized team name match.
4. For players: match by normalized (first, last) within the
   api-sports → canonical team mapping. Tie-break on date_of_birth
   when both providers return it.

Any unmatched entity is logged (not hard-failed) so the operator can
hand-fix in a follow-up run. After the first successful pass the map
is populated and subsequent runs are O(1) lookups per entity.

Writes
------
`logo_url` and `photo_url` are only set when the existing column is
NULL. This keeps the seeder idempotent and prevents overwriting values
from another source later (e.g. SportsDataIO).
"""

from __future__ import annotations

import logging
import re
from dataclasses import dataclass
from typing import Any

import psycopg

from shared.apisports_client import APISportsClient
from shared.upsert import upsert_provider_entity_map

logger = logging.getLogger(__name__)

NBA_BASE_URL = "https://v2.nba.api-sports.io"
NFL_BASE_URL = "https://v1.american-football.api-sports.io"
PROVIDER = "api-sports"

# NFL league id is stable across seasons in the api-sports schema (league=1).
NFL_LEAGUE_ID = 1


@dataclass
class SeedReport:
    teams_mapped: int = 0
    team_logos_written: int = 0
    team_logos_skipped_present: int = 0
    teams_unmatched: int = 0
    players_mapped: int = 0
    player_photos_written: int = 0
    player_photos_skipped_present: int = 0
    players_unmatched: int = 0
    api_calls: int = 0


def _normalize(s: str | None) -> str:
    if not s:
        return ""
    return re.sub(r"[^a-z0-9]", "", s.lower())


def _find_canonical_team(
    conn: psycopg.Connection, sport: str, as_team: dict[str, Any]
) -> int | None:
    """Match an api-sports team to our canonical team id.

    Order of precedence: existing map row → short_code → normalized name.
    """
    as_id_str = str(as_team.get("id"))

    row = conn.execute(
        """
        SELECT canonical_entity_id FROM provider_entity_map
        WHERE provider=%s AND sport=%s AND entity_type='team'
          AND provider_entity_id=%s
        """,
        (PROVIDER, sport, as_id_str),
    ).fetchone()
    if row:
        return int(row[0])

    code = _normalize(as_team.get("code"))
    if code:
        row = conn.execute(
            """
            SELECT id FROM teams
            WHERE sport=%s
              AND regexp_replace(lower(coalesce(short_code, '')), '[^a-z0-9]', '', 'g') = %s
            LIMIT 1
            """,
            (sport, code),
        ).fetchone()
        if row:
            return int(row[0])

    nickname = _normalize(as_team.get("nickname"))
    if nickname:
        row = conn.execute(
            """
            SELECT id FROM teams
            WHERE sport=%s
              AND regexp_replace(lower(name), '[^a-z0-9]', '', 'g') LIKE %s
            LIMIT 1
            """,
            (sport, f"%{nickname}%"),
        ).fetchone()
        if row:
            return int(row[0])

    return None


def _find_canonical_player(
    conn: psycopg.Connection,
    sport: str,
    as_player: dict[str, Any],
    canonical_team_id: int | None,
) -> int | None:
    """Match an api-sports player to our canonical player id.

    Order: existing map row → (first+last+team) exact → DOB tiebreaker.
    """
    as_id_str = str(as_player.get("id"))

    row = conn.execute(
        """
        SELECT canonical_entity_id FROM provider_entity_map
        WHERE provider=%s AND sport=%s AND entity_type='player'
          AND provider_entity_id=%s
        """,
        (PROVIDER, sport, as_id_str),
    ).fetchone()
    if row:
        return int(row[0])

    first = _normalize(as_player.get("firstname") or as_player.get("first_name"))
    last = _normalize(as_player.get("lastname") or as_player.get("last_name"))
    if not (first and last):
        return None

    params: list[Any] = [sport, first, last]
    sql = """
        SELECT id, date_of_birth FROM players
        WHERE sport=%s
          AND regexp_replace(lower(coalesce(first_name, '')), '[^a-z0-9]', '', 'g') = %s
          AND regexp_replace(lower(coalesce(last_name, '')),  '[^a-z0-9]', '', 'g') = %s
    """
    if canonical_team_id is not None:
        sql += " AND (team_id = %s OR team_id IS NULL)"
        params.append(canonical_team_id)

    rows = conn.execute(sql, params).fetchall()
    if len(rows) == 1:
        return int(rows[0][0])
    if len(rows) > 1:
        # DOB tiebreaker
        birth = (as_player.get("birth") or {}).get("date") or as_player.get("date_of_birth")
        if birth:
            for row in rows:
                if row[1] is not None and str(row[1]) == str(birth):
                    return int(row[0])
        logger.warning(
            "ambiguous player match (%d candidates) for api-sports id=%s name=%s %s",
            len(rows), as_id_str, first, last,
        )
    return None


def _extract_player_photo(as_player: dict[str, Any]) -> str | None:
    for key in ("photo", "image", "picture", "headshot", "image_url", "photo_url"):
        val = as_player.get(key)
        if isinstance(val, str) and val.strip():
            return val.strip()
    return None


def _seed_images(
    conn: psycopg.Connection,
    *,
    sport: str,
    base_url: str,
    api_key: str,
    season: int,
    teams_path: str,
    teams_params: dict[str, Any] | None,
    team_filter,  # callable(dict) -> bool
    players_path: str,
    players_params_builder,  # callable(as_team_id) -> dict
    dry_run: bool,
) -> SeedReport:
    report = SeedReport()
    client = APISportsClient(base_url, api_key)
    try:
        # --- Teams --------------------------------------------------------
        resp = client.get(teams_path, teams_params or {})
        report.api_calls += 1
        teams = [t for t in resp.get("response", []) if team_filter(t)]
        logger.info(
            "api-sports %s %s returned %d teams (after filter)",
            sport, teams_path, len(teams),
        )

        as_to_canonical: dict[int, int] = {}
        for as_team in teams:
            canonical = _find_canonical_team(conn, sport, as_team)
            if canonical is None:
                report.teams_unmatched += 1
                logger.warning(
                    "unmatched %s team: api_id=%s name=%r code=%r",
                    sport, as_team.get("id"), as_team.get("name"),
                    as_team.get("code"),
                )
                continue
            as_id = as_team["id"]
            as_to_canonical[as_id] = canonical
            report.teams_mapped += 1

            logo = as_team.get("logo")
            if dry_run:
                if logo:
                    logger.info(
                        "[dry-run] would set teams.id=%s logo_url=%s",
                        canonical, logo,
                    )
                continue

            upsert_provider_entity_map(
                conn, PROVIDER, sport, "team", str(as_id), canonical,
            )
            if logo:
                cur = conn.execute(
                    """
                    UPDATE teams SET logo_url=%s, updated_at=NOW()
                    WHERE id=%s AND sport=%s AND logo_url IS NULL
                    """,
                    (logo, canonical, sport),
                )
                if cur.rowcount:
                    report.team_logos_written += 1
                else:
                    report.team_logos_skipped_present += 1

        # --- Players ------------------------------------------------------
        for as_team_id, canonical_team_id in as_to_canonical.items():
            resp = client.get(players_path, players_params_builder(as_team_id))
            report.api_calls += 1
            roster = resp.get("response", [])
            logger.info(
                "api-sports %s %s team=%s returned %d players",
                sport, players_path, as_team_id, len(roster),
            )

            for as_player in roster:
                canonical_player = _find_canonical_player(
                    conn, sport, as_player, canonical_team_id,
                )
                if canonical_player is None:
                    report.players_unmatched += 1
                    logger.warning(
                        "unmatched %s player: api_id=%s name=%r %r team=%s",
                        sport,
                        as_player.get("id"),
                        as_player.get("firstname") or as_player.get("name"),
                        as_player.get("lastname"),
                        canonical_team_id,
                    )
                    continue
                report.players_mapped += 1

                photo = _extract_player_photo(as_player)
                if dry_run:
                    if photo:
                        logger.info(
                            "[dry-run] would set players.id=%s photo_url=%s",
                            canonical_player, photo,
                        )
                    continue

                upsert_provider_entity_map(
                    conn, PROVIDER, sport, "player",
                    str(as_player["id"]), canonical_player,
                )
                if photo:
                    cur = conn.execute(
                        """
                        UPDATE players SET photo_url=%s, updated_at=NOW()
                        WHERE id=%s AND sport=%s AND photo_url IS NULL
                        """,
                        (photo, canonical_player, sport),
                    )
                    if cur.rowcount:
                        report.player_photos_written += 1
                    else:
                        report.player_photos_skipped_present += 1
    finally:
        client.close()
    return report


def seed_nba_images(
    conn: psycopg.Connection,
    api_key: str,
    season: int,
    dry_run: bool = False,
) -> SeedReport:
    """Seed NBA team logos and player photos from api-sports.

    Call cost: 1 (teams) + ~30 (players per team) = ~31 per run.
    """
    return _seed_images(
        conn,
        sport="NBA",
        base_url=NBA_BASE_URL,
        api_key=api_key,
        season=season,
        teams_path="/teams",
        teams_params=None,
        team_filter=lambda t: bool(t.get("nbaFranchise")) and not t.get("allStar"),
        players_path="/players",
        players_params_builder=lambda tid: {"team": tid, "season": season},
        dry_run=dry_run,
    )


def seed_nfl_images(
    conn: psycopg.Connection,
    api_key: str,
    season: int,
    dry_run: bool = False,
) -> SeedReport:
    """Seed NFL team logos and player photos from api-sports.

    Call cost: 1 (teams) + 32 (players per team) = 33 per run.
    """
    return _seed_images(
        conn,
        sport="NFL",
        base_url=NFL_BASE_URL,
        api_key=api_key,
        season=season,
        teams_path="/teams",
        teams_params={"league": NFL_LEAGUE_ID, "season": season},
        team_filter=lambda t: True,
        players_path="/players",
        players_params_builder=lambda tid: {"team": tid, "season": season},
        dry_run=dry_run,
    )
