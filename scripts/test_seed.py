#!/usr/bin/env python3
"""
Test seed script — seeds ONE team + ONE player from each data source,
then queries back and prints full data for inspection.

Sources:
  - NBA (BallDontLie)
  - NFL (BallDontLie)
  - Football: Premier League, La Liga, Bundesliga, Serie A, Ligue 1 (SportMonks)

Run:  .venv/bin/python scripts/test_seed.py
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import sys
from typing import Any

# Ensure the src directory is on the path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from dotenv import load_dotenv

load_dotenv()

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%H:%M:%S",
)
# Quiet down httpx request logs
logging.getLogger("httpx").setLevel(logging.WARNING)
logger = logging.getLogger("test_seed")

SEASON = 2024

# ============================================================================
# Helpers
# ============================================================================


def truncate(obj: Any, max_len: int = 600) -> str:
    """JSON-serialize and truncate for display."""
    s = json.dumps(obj, indent=2, default=str)
    if len(s) > max_len:
        return s[:max_len] + "\n  ... (truncated)"
    return s


def print_section(title: str) -> None:
    print(f"\n{'=' * 70}")
    print(f"  {title}")
    print(f"{'=' * 70}")


def print_entity(label: str, data: dict[str, Any] | None) -> None:
    """Print a labeled entity dict."""
    if data is None:
        print(f"\n  [{label}] NOT FOUND")
        return
    print(f"\n  [{label}]")
    for k, v in data.items():
        if k in ("stats", "percentiles", "raw_response", "meta"):
            if isinstance(v, str):
                try:
                    v = json.loads(v)
                except Exception:
                    pass
            if isinstance(v, dict) and len(v) > 0:
                print(f"    {k}:")
                for sk, sv in v.items():
                    print(f"      {sk}: {sv}")
            elif v:
                print(f"    {k}: {truncate(v, 300)}")
            else:
                print(f"    {k}: {{}}")
        else:
            print(f"    {k}: {v}")


def query_team(db, team_id: int, sport: str) -> dict[str, Any] | None:
    return db.fetchone(
        "SELECT * FROM teams WHERE id = %s AND sport = %s",
        (team_id, sport),
    )


def query_team_stats(db, team_id: int, sport: str, season: int, league_id: int = 0):
    return db.fetchone(
        "SELECT * FROM team_stats WHERE team_id = %s AND sport = %s AND season = %s AND league_id = %s",
        (team_id, sport, season, league_id),
    )


def query_player(db, player_id: int, sport: str) -> dict[str, Any] | None:
    return db.fetchone(
        "SELECT * FROM players WHERE id = %s AND sport = %s",
        (player_id, sport),
    )


def query_player_stats(db, player_id: int, sport: str, season: int, league_id: int = 0):
    return db.fetchone(
        "SELECT * FROM player_stats WHERE player_id = %s AND sport = %s AND season = %s AND league_id = %s",
        (player_id, sport, season, league_id),
    )


# ============================================================================
# BDL (NBA / NFL)
# ============================================================================


async def test_bdl_sport(db, sport: str) -> None:
    """Test-seed one team + one player for an American sport."""
    from scoracle_data.seeders.base import BaseSeedRunner

    if sport == "NBA":
        from scoracle_data.handlers.balldontlie import BDLNBAHandler

        api_key = os.environ["BALLDONTLIE_API_KEY"]
        handler = BDLNBAHandler(api_key=api_key)
    else:
        from scoracle_data.handlers.balldontlie import BDLNFLHandler

        api_key = os.environ["BALLDONTLIE_API_KEY"]
        handler = BDLNFLHandler(api_key=api_key)

    runner = BaseSeedRunner(db, handler, sport=sport)

    async with handler:
        # --- Teams: fetch all, pick first ---
        logger.info("[%s] Fetching teams...", sport)
        teams = await handler.get_teams()
        team = teams[0]
        logger.info("[%s] Picked team: %s (id=%s)", sport, team.get("name"), team["id"])
        runner._upsert_team(team)

        # --- Player stats: iterate until we find one with meaningful stats ---
        logger.info("[%s] Fetching player stats for season %d...", sport, SEASON)
        picked_player_data = None
        count = 0
        async for ps in handler.get_player_stats(SEASON):
            count += 1
            stats = ps.get("stats", {})
            # Pick first player who has at least 5 stat keys
            if len(stats) >= 5 and ps.get("player"):
                picked_player_data = ps
                break
            if count >= 200:
                # Safety limit — just pick what we have
                if ps.get("player"):
                    picked_player_data = ps
                break

        if picked_player_data is None:
            logger.error("[%s] No player stats found!", sport)
            return

        player = picked_player_data["player"]
        logger.info(
            "[%s] Picked player: %s (id=%s) — %d stat keys",
            sport,
            player.get("name"),
            player.get("id"),
            len(picked_player_data.get("stats", {})),
        )

        # Upsert player profile + stats
        runner._upsert_player(player)
        runner._upsert_player_stats(picked_player_data, SEASON)

        # --- Team stats: fetch all, find matching team ---
        logger.info("[%s] Fetching team stats for season %d...", sport, SEASON)
        team_stats_list = await handler.get_team_stats(SEASON)
        team_stats_data = None
        for ts in team_stats_list:
            if ts.get("team_id") == team["id"]:
                team_stats_data = ts
                break
        if team_stats_data is None and team_stats_list:
            team_stats_data = team_stats_list[0]
            team = teams[[t["id"] for t in teams].index(team_stats_data["team_id"])] if team_stats_data["team_id"] in [t["id"] for t in teams] else team
            runner._upsert_team(team)

        if team_stats_data:
            runner._upsert_team_stats(team_stats_data, SEASON)
            logger.info("[%s] Upserted team stats for team_id=%s", sport, team_stats_data["team_id"])

    # --- Query back and display ---
    print_section(f"{sport} — Test Seed Results (Season {SEASON})")

    print_entity(f"{sport} Team Profile", query_team(db, team["id"], sport))
    print_entity(
        f"{sport} Team Stats",
        query_team_stats(db, team_stats_data["team_id"] if team_stats_data else team["id"], sport, SEASON),
    )

    pid = picked_player_data["player_id"]
    print_entity(f"{sport} Player Profile", query_player(db, pid, sport))
    print_entity(f"{sport} Player Stats", query_player_stats(db, pid, sport, SEASON))


# ============================================================================
# Football (SportMonks)
# ============================================================================

def get_football_leagues(db) -> list[dict[str, Any]]:
    """Fetch benchmark football leagues from the DB (works for both v4 bridge and fresh schema)."""
    rows = db.fetchall(
        "SELECT id, name, sportmonks_id FROM leagues "
        "WHERE sport = 'FOOTBALL' AND is_benchmark = true AND is_active = true "
        "ORDER BY id"
    )
    return [{"id": r["id"], "name": r["name"], "sportmonks_id": r["sportmonks_id"]} for r in rows]


async def test_football(db) -> None:
    """Test-seed one team + one player from each of the 5 football leagues."""
    from scoracle_data.handlers.sportmonks import SportMonksHandler
    from scoracle_data.seeders.base import BaseSeedRunner

    football_leagues = get_football_leagues(db)
    if not football_leagues:
        logger.error("No benchmark football leagues found in database!")
        return

    logger.info("Found %d benchmark leagues: %s", len(football_leagues), ", ".join(l["name"] for l in football_leagues))

    api_token = os.environ["SPORTMONKS_API_TOKEN"]
    handler = SportMonksHandler(api_token=api_token)
    runner = BaseSeedRunner(db, handler, sport="FOOTBALL")

    async with handler:
        for league in football_leagues:
            league_id = league["id"]
            league_name = league["name"]
            sm_league_id = league["sportmonks_id"]

            print_section(f"FOOTBALL — {league_name} (Season {SEASON})")

            # Resolve provider season ID
            row = db.fetchone(
                "SELECT resolve_provider_season_id(%s, %s) AS sid",
                (league_id, SEASON),
            )
            sm_season_id = row["sid"] if row else None
            if not sm_season_id:
                logger.warning("[%s] No provider season ID for %d, skipping", league_name, SEASON)
                continue

            logger.info("[%s] SportMonks season_id=%d", league_name, sm_season_id)

            # --- Teams: fetch all, pick first ---
            logger.info("[%s] Fetching teams...", league_name)
            teams = await handler.get_teams(sm_season_id)
            if not teams:
                logger.warning("[%s] No teams found!", league_name)
                continue
            team = teams[0]
            logger.info("[%s] Picked team: %s (id=%s)", league_name, team.get("name"), team["id"])
            runner._upsert_team(team)

            # --- Players + stats: fetch for one team, pick first with stats ---
            logger.info("[%s] Fetching players with stats for team %s...", league_name, team["id"])
            picked = None
            async for data in handler.get_players_with_stats(sm_season_id, [team["id"]], sm_league_id):
                if data.get("player") and data.get("stats"):
                    picked = data
                    break
                # Even without stats, take the first one as fallback
                if picked is None and data.get("player"):
                    picked = data

            if picked is None:
                logger.warning("[%s] No player data found!", league_name)
                continue

            player = picked["player"]
            logger.info(
                "[%s] Picked player: %s (id=%s) — %d stat keys",
                league_name,
                player.get("name"),
                player.get("id"),
                len(picked.get("stats", {})),
            )

            # Upsert player + stats
            runner._upsert_player(player)
            if picked.get("stats"):
                runner._upsert_player_stats(picked, SEASON, league_id)

            # --- Team stats (standings): fetch all, find matching ---
            logger.info("[%s] Fetching team stats (standings)...", league_name)
            team_stats_list = await handler.get_team_stats(sm_season_id)
            team_stats_data = None
            for ts in team_stats_list:
                if ts.get("team_id") == team["id"]:
                    team_stats_data = ts
                    break
            if team_stats_data is None and team_stats_list:
                # Fallback to first entry
                team_stats_data = team_stats_list[0]

            if team_stats_data:
                # Upsert the team from standings if needed
                if team_stats_data.get("team"):
                    runner._upsert_team(team_stats_data["team"])
                runner._upsert_team_stats(team_stats_data, SEASON, league_id)

            # --- Query back and display ---
            print_entity(f"{league_name} Team Profile", query_team(db, team["id"], "FOOTBALL"))
            ts_team_id = team_stats_data["team_id"] if team_stats_data else team["id"]
            print_entity(
                f"{league_name} Team Stats",
                query_team_stats(db, ts_team_id, "FOOTBALL", SEASON, league_id),
            )

            pid = picked["player_id"]
            print_entity(f"{league_name} Player Profile", query_player(db, pid, "FOOTBALL"))
            print_entity(
                f"{league_name} Player Stats",
                query_player_stats(db, pid, "FOOTBALL", SEASON, league_id),
            )


# ============================================================================
# Main
# ============================================================================


async def main() -> None:
    from scoracle_data.pg_connection import get_db

    db = get_db()

    if not db.is_initialized():
        from scoracle_data.schema import init_database

        logger.info("Initializing database...")
        init_database(db)

    try:
        # American sports
        await test_bdl_sport(db, "NBA")
        await test_bdl_sport(db, "NFL")

        # Football (all 5 leagues)
        await test_football(db)

        print_section("TEST SEED COMPLETE")
        print("  All entities seeded and queried back successfully.")
        print("  Review the output above for data quality.\n")

    finally:
        db.close()


if __name__ == "__main__":
    asyncio.run(main())
