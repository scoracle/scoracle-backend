#!/usr/bin/env python3
"""
Production seeding script for Scoracle database.

Seeds specific teams and players across all sports with full data coverage.
This uses the same approach as the successful test runs.

Usage:
    python scripts/seed_production.py
    python scripts/seed_production.py --sport FOOTBALL
    python scripts/seed_production.py --sport NFL
    python scripts/seed_production.py --sport NBA
    python scripts/seed_production.py --dry-run
"""

from __future__ import annotations

import argparse
import asyncio
import logging
import os
import sys
from pathlib import Path
from typing import Any, Optional

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from dotenv import load_dotenv
load_dotenv()

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
logger = logging.getLogger("seed_production")


# =============================================================================
# SEED CONFIGURATION
# =============================================================================
# Define specific teams and players to seed for each sport.
# Add more teams/players here as needed.

SEED_CONFIG = {
    "FOOTBALL": {
        "season": 2025,
        "leagues": [
            {"id": 39, "name": "Premier League", "country": "England"},
            {"id": 140, "name": "La Liga", "country": "Spain"},
            {"id": 78, "name": "Bundesliga", "country": "Germany"},
            {"id": 135, "name": "Serie A", "country": "Italy"},
            {"id": 61, "name": "Ligue 1", "country": "France"},
        ],
        # Seed ALL teams from each league (set to None to fetch all)
        # Or specify team IDs to limit: [49, 50, 51, ...]
        "team_ids": None,  # None = all teams in league
        # Seed top N players per team (set to None for all)
        "players_per_team": None,  # None = all players
    },
    "NFL": {
        "season": 2025,
        "league_id": 1,  # NFL
        # Seed ALL teams (set to None to fetch all)
        "team_ids": None,
        "players_per_team": None,
    },
    "NBA": {
        "season": 2025,
        "league": "standard",
        # Seed ALL teams (set to None to fetch all)
        "team_ids": None,
        "players_per_team": None,
    },
}


# =============================================================================
# SEEDING FUNCTIONS
# =============================================================================

async def seed_football(db, api, config: dict, dry_run: bool = False) -> dict:
    """Seed Football data."""
    from scoracle_data.seeders.football_seeder import FootballSeeder

    season = config["season"]
    leagues = config["leagues"]

    summary = {"teams": 0, "players": 0, "player_stats": 0, "team_stats": 0}

    seeder = FootballSeeder(db=db, api_service=api, leagues=leagues)
    seeder.ensure_leagues()
    season_id = seeder.ensure_season(season, is_current=True)

    for league in leagues:
        league_id = league["id"]
        logger.info(f"Seeding {league['name']} (ID: {league_id})...")

        # Fetch teams
        teams = await seeder.fetch_teams(season, league_id)
        team_ids = config.get("team_ids")
        if team_ids:
            teams = [t for t in teams if t["id"] in team_ids]

        logger.info(f"  Found {len(teams)} teams")

        if dry_run:
            continue

        # Upsert teams and fetch profiles
        for team in teams:
            team["league_id"] = league_id
            seeder.upsert_team(team, mark_profile_fetched=False)
            summary["teams"] += 1

            # Fetch full team profile
            profile = await seeder.fetch_team_profile(team["id"])
            if profile:
                profile["league_id"] = league_id
                seeder.upsert_team(profile, mark_profile_fetched=True)

            # Fetch team stats
            try:
                raw_stats = await seeder.fetch_team_stats(team["id"], season, league_id)
                if raw_stats:
                    transformed = seeder.transform_team_stats(raw_stats, team["id"], season_id)
                    seeder.upsert_team_stats(transformed)
                    summary["team_stats"] += 1
            except Exception as e:
                logger.warning(f"  Failed to fetch team stats for {team['name']}: {e}")

        # Fetch players for this league
        logger.info(f"  Fetching players for {league['name']}...")
        all_players = []
        page = 1
        while True:
            players = await api.list_players("FOOTBALL", season=str(season), league=league_id, page=page)
            if not players:
                break
            all_players.extend(players)
            page += 1
            if page > 50:  # Safety limit
                break

        logger.info(f"  Found {len(all_players)} players")

        # Limit players if configured
        players_per_team = config.get("players_per_team")
        if players_per_team:
            # Group by team and take top N per team
            # Football API structure: {"player": {...}, "statistics": [{"team": {...}, ...}]}
            from collections import defaultdict
            by_team = defaultdict(list)
            for p in all_players:
                stats = p.get("statistics", [])
                team_id = stats[0].get("team", {}).get("id") if stats else None
                if team_id:
                    by_team[team_id].append(p)
            all_players = []
            for team_id, players in by_team.items():
                all_players.extend(players[:players_per_team])

        # Seed players
        for player_data in all_players:
            # Football API returns {"player": {...}, "statistics": [...]}
            player = player_data.get("player", player_data)
            player_id = player.get("id")
            if not player_id:
                continue

            # Fetch full player profile
            profile = await seeder.fetch_player_profile(player_id)
            if profile:
                profile["current_league_id"] = league_id
                seeder.upsert_player(profile, mark_profile_fetched=True)
                summary["players"] += 1

                # Fetch player stats (league_id required for Football API)
                try:
                    raw_stats = await seeder.fetch_player_stats(player_id, season, league_id)
                    if raw_stats:
                        transformed = seeder.transform_player_stats(
                            raw_stats, player_id, season_id,
                            profile.get("current_team_id")
                        )
                        if transformed:
                            # Ensure league_id is set in transformed stats
                            transformed["league_id"] = league_id
                            seeder.upsert_player_stats(transformed)
                            summary["player_stats"] += 1
                except Exception as e:
                    logger.debug(f"  Failed to fetch stats for player {player_id}: {e}")

            # Progress logging
            if summary["players"] % 100 == 0:
                logger.info(f"  Progress: {summary['players']} players, {summary['player_stats']} stats")

    return summary


async def seed_nfl(db, api, config: dict, dry_run: bool = False) -> dict:
    """Seed NFL data."""
    from scoracle_data.seeders.nfl_seeder import NFLSeeder

    season = config["season"]
    league_id = config.get("league_id", 1)

    summary = {"teams": 0, "players": 0, "player_stats": 0, "team_stats": 0}

    seeder = NFLSeeder(db=db, api_service=api)
    season_id = seeder.ensure_season(season, is_current=True)

    logger.info(f"Seeding NFL season {season}...")

    # Fetch teams
    teams = await seeder.fetch_teams(season, league_id)
    team_ids = config.get("team_ids")
    if team_ids:
        teams = [t for t in teams if t["id"] in team_ids]

    logger.info(f"  Found {len(teams)} teams")

    if dry_run:
        return summary

    # Upsert teams and fetch profiles
    for team in teams:
        seeder.upsert_team(team, mark_profile_fetched=False)
        summary["teams"] += 1

        # Fetch full team profile
        profile = await seeder.fetch_team_profile(team["id"])
        if profile:
            seeder.upsert_team(profile, mark_profile_fetched=True)

        # Fetch team stats
        try:
            raw_stats = await seeder.fetch_team_stats(team["id"], season, league_id)
            if raw_stats:
                transformed = seeder.transform_team_stats(raw_stats, team["id"], season_id)
                seeder.upsert_team_stats(transformed)
                summary["team_stats"] += 1
        except Exception as e:
            logger.warning(f"  Failed to fetch team stats for {team.get('name')}: {e}")

    # Fetch players
    logger.info("  Fetching NFL players...")
    for team in teams:
        team_id = team["id"]
        players = await api.list_players("NFL", season=str(season), team_id=team_id)

        players_per_team = config.get("players_per_team")
        if players_per_team:
            players = players[:players_per_team]

        for player in players:
            player_id = player.get("id")
            if not player_id:
                continue

            # Fetch full player profile
            profile = await seeder.fetch_player_profile(player_id)
            if profile:
                profile["current_team_id"] = team_id
                seeder.upsert_player(profile, mark_profile_fetched=True)
                summary["players"] += 1

                # Fetch player stats
                try:
                    raw_stats = await seeder.fetch_player_stats(player_id, season)
                    if raw_stats:
                        transformed = seeder.transform_player_stats(
                            raw_stats, player_id, season_id, team_id
                        )
                        if transformed:
                            seeder.upsert_player_stats(transformed)
                            summary["player_stats"] += 1
                except Exception as e:
                    logger.debug(f"  Failed to fetch stats for player {player_id}: {e}")

        logger.info(f"  Team {team.get('name')}: {len(players)} players processed")

    return summary


async def seed_nba(db, api, config: dict, dry_run: bool = False) -> dict:
    """Seed NBA data."""
    from scoracle_data.seeders.nba_seeder import NBASeeder

    season = config["season"]

    summary = {"teams": 0, "players": 0, "player_stats": 0, "team_stats": 0}

    seeder = NBASeeder(db=db, api_service=api)
    season_id = seeder.ensure_season(season, is_current=True)

    logger.info(f"Seeding NBA season {season}...")

    # Fetch teams
    teams = await seeder.fetch_teams(season)
    team_ids = config.get("team_ids")
    if team_ids:
        teams = [t for t in teams if t["id"] in team_ids]

    logger.info(f"  Found {len(teams)} teams")

    if dry_run:
        return summary

    # Upsert teams and fetch profiles
    for team in teams:
        seeder.upsert_team(team, mark_profile_fetched=False)
        summary["teams"] += 1

        # Fetch full team profile
        profile = await seeder.fetch_team_profile(team["id"])
        if profile:
            seeder.upsert_team(profile, mark_profile_fetched=True)

        # Fetch team stats
        try:
            raw_stats = await seeder.fetch_team_stats(team["id"], season)
            if raw_stats:
                transformed = seeder.transform_team_stats(raw_stats, team["id"], season_id)
                seeder.upsert_team_stats(transformed)
                summary["team_stats"] += 1
        except Exception as e:
            logger.warning(f"  Failed to fetch team stats for {team.get('name')}: {e}")

    # Fetch players
    logger.info("  Fetching NBA players...")
    for team in teams:
        team_id = team["id"]
        players = await api.list_players("NBA", season=str(season), team_id=team_id)

        players_per_team = config.get("players_per_team")
        if players_per_team:
            players = players[:players_per_team]

        for player in players:
            player_id = player.get("id")
            if not player_id:
                continue

            # Fetch full player profile
            profile = await seeder.fetch_player_profile(player_id)
            if profile:
                profile["current_team_id"] = team_id
                seeder.upsert_player(profile, mark_profile_fetched=True)
                summary["players"] += 1

                # Fetch player stats
                try:
                    raw_stats = await seeder.fetch_player_stats(player_id, season)
                    if raw_stats:
                        transformed = seeder.transform_player_stats(
                            raw_stats, player_id, season_id, team_id
                        )
                        if transformed:
                            seeder.upsert_player_stats(transformed)
                            summary["player_stats"] += 1
                except Exception as e:
                    logger.debug(f"  Failed to fetch stats for player {player_id}: {e}")

        logger.info(f"  Team {team.get('name')}: {len(players)} players processed")

    return summary


async def main_async(args: argparse.Namespace) -> int:
    """Main async entry point."""
    from scoracle_data.pg_connection import PostgresDB
    from scoracle_data.api_client import StandaloneApiClient

    # Initialize connections
    db = PostgresDB()
    api = StandaloneApiClient()

    sports_to_seed = []
    if args.sport:
        sports_to_seed = [args.sport.upper()]
    else:
        sports_to_seed = ["FOOTBALL", "NFL", "NBA"]

    total_summary = {"teams": 0, "players": 0, "player_stats": 0, "team_stats": 0}

    try:
        for sport in sports_to_seed:
            config = SEED_CONFIG.get(sport)
            if not config:
                logger.warning(f"No config for sport: {sport}")
                continue

            logger.info(f"\n{'='*60}")
            logger.info(f"SEEDING {sport}")
            logger.info(f"{'='*60}")

            if sport == "FOOTBALL":
                summary = await seed_football(db, api, config, args.dry_run)
            elif sport == "NFL":
                summary = await seed_nfl(db, api, config, args.dry_run)
            elif sport == "NBA":
                summary = await seed_nba(db, api, config, args.dry_run)
            else:
                continue

            for key in total_summary:
                total_summary[key] += summary.get(key, 0)

            logger.info(f"\n{sport} Summary:")
            logger.info(f"  Teams: {summary['teams']}")
            logger.info(f"  Players: {summary['players']}")
            logger.info(f"  Player Stats: {summary['player_stats']}")
            logger.info(f"  Team Stats: {summary['team_stats']}")

        logger.info(f"\n{'='*60}")
        logger.info("TOTAL SUMMARY")
        logger.info(f"{'='*60}")
        logger.info(f"  Teams: {total_summary['teams']}")
        logger.info(f"  Players: {total_summary['players']}")
        logger.info(f"  Player Stats: {total_summary['player_stats']}")
        logger.info(f"  Team Stats: {total_summary['team_stats']}")

        return 0

    finally:
        db.close()


def main() -> int:
    """Main entry point."""
    parser = argparse.ArgumentParser(description="Production database seeding")
    parser.add_argument("--sport", help="Seed specific sport (FOOTBALL, NFL, NBA)")
    parser.add_argument("--dry-run", action="store_true", help="Show what would be seeded")

    args = parser.parse_args()

    return asyncio.run(main_async(args))


if __name__ == "__main__":
    sys.exit(main())
