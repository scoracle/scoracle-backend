"""
Seeder for sample player and team statistics.

This module seeds realistic sample statistics for the entities in small_dataset.json.
It does NOT call external APIs, making it suitable for offline testing and development.
"""

from __future__ import annotations

import logging
import os
import sys
from pathlib import Path
from typing import Any, Dict, Optional

# Allow running as a script
if __name__ == "__main__" and __package__ is None:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

try:
    from ..connection import get_stats_db
    from ..pg_connection import PostgresDB
except ImportError:
    from scoracle_data.connection import get_stats_db
    from scoracle_data.pg_connection import PostgresDB

logger = logging.getLogger(__name__)

# Sample NBA player statistics (realistic values for 2024-25 season)
NBA_SAMPLE_STATS = {
    2801: {  # Cade Cunningham
        "games_played": 45,
        "games_started": 45,
        "minutes_total": 1575,
        "minutes_per_game": 35.0,
        "points_total": 1035,
        "points_per_game": 23.0,
        "fgm": 360,
        "fga": 810,
        "fg_pct": 44.4,
        "tpm": 90,
        "tpa": 270,
        "tp_pct": 33.3,
        "ftm": 225,
        "fta": 270,
        "ft_pct": 83.3,
        "offensive_rebounds": 45,
        "defensive_rebounds": 315,
        "total_rebounds": 360,
        "rebounds_per_game": 8.0,
        "assists": 405,
        "assists_per_game": 9.0,
        "turnovers": 135,
        "turnovers_per_game": 3.0,
        "steals": 45,
        "steals_per_game": 1.0,
        "blocks": 18,
        "blocks_per_game": 0.4,
        "personal_fouls": 90,
        "fouls_per_game": 2.0,
        "plus_minus": 90,
        "plus_minus_per_game": 2.0,
        "efficiency": 24.5,
        "true_shooting_pct": 55.2,
        "effective_fg_pct": 49.9,
        "assist_turnover_ratio": 3.0,
        "double_doubles": 15,
        "triple_doubles": 2,
    },
    350: {  # Jalen Duren
        "games_played": 42,
        "games_started": 42,
        "minutes_total": 1260,
        "minutes_per_game": 30.0,
        "points_total": 588,
        "points_per_game": 14.0,
        "fgm": 252,
        "fga": 420,
        "fg_pct": 60.0,
        "tpm": 0,
        "tpa": 0,
        "tp_pct": 0.0,
        "ftm": 84,
        "fta": 168,
        "ft_pct": 50.0,
        "offensive_rebounds": 168,
        "defensive_rebounds": 336,
        "total_rebounds": 504,
        "rebounds_per_game": 12.0,
        "assists": 63,
        "assists_per_game": 1.5,
        "turnovers": 84,
        "turnovers_per_game": 2.0,
        "steals": 42,
        "steals_per_game": 1.0,
        "blocks": 63,
        "blocks_per_game": 1.5,
        "personal_fouls": 126,
        "fouls_per_game": 3.0,
        "plus_minus": 42,
        "plus_minus_per_game": 1.0,
        "efficiency": 18.2,
        "true_shooting_pct": 58.8,
        "effective_fg_pct": 60.0,
        "assist_turnover_ratio": 0.75,
        "double_doubles": 25,
        "triple_doubles": 0,
    },
    351: {  # Jaden Ivey
        "games_played": 40,
        "games_started": 35,
        "minutes_total": 1200,
        "minutes_per_game": 30.0,
        "points_total": 680,
        "points_per_game": 17.0,
        "fgm": 240,
        "fga": 560,
        "fg_pct": 42.9,
        "tpm": 80,
        "tpa": 240,
        "tp_pct": 33.3,
        "ftm": 120,
        "fta": 160,
        "ft_pct": 75.0,
        "offensive_rebounds": 40,
        "defensive_rebounds": 160,
        "total_rebounds": 200,
        "rebounds_per_game": 5.0,
        "assists": 160,
        "assists_per_game": 4.0,
        "turnovers": 120,
        "turnovers_per_game": 3.0,
        "steals": 60,
        "steals_per_game": 1.5,
        "blocks": 20,
        "blocks_per_game": 0.5,
        "personal_fouls": 100,
        "fouls_per_game": 2.5,
        "plus_minus": -40,
        "plus_minus_per_game": -1.0,
        "efficiency": 14.8,
        "true_shooting_pct": 52.1,
        "effective_fg_pct": 49.9,
        "assist_turnover_ratio": 1.33,
        "double_doubles": 3,
        "triple_doubles": 0,
    },
    265: {  # LeBron James
        "games_played": 50,
        "games_started": 50,
        "minutes_total": 1750,
        "minutes_per_game": 35.0,
        "points_total": 1250,
        "points_per_game": 25.0,
        "fgm": 500,
        "fga": 1000,
        "fg_pct": 50.0,
        "tpm": 100,
        "tpa": 300,
        "tp_pct": 33.3,
        "ftm": 150,
        "fta": 200,
        "ft_pct": 75.0,
        "offensive_rebounds": 50,
        "defensive_rebounds": 350,
        "total_rebounds": 400,
        "rebounds_per_game": 8.0,
        "assists": 400,
        "assists_per_game": 8.0,
        "turnovers": 150,
        "turnovers_per_game": 3.0,
        "steals": 50,
        "steals_per_game": 1.0,
        "blocks": 30,
        "blocks_per_game": 0.6,
        "personal_fouls": 100,
        "fouls_per_game": 2.0,
        "plus_minus": 200,
        "plus_minus_per_game": 4.0,
        "efficiency": 26.5,
        "true_shooting_pct": 60.2,
        "effective_fg_pct": 55.0,
        "assist_turnover_ratio": 2.67,
        "double_doubles": 20,
        "triple_doubles": 5,
    },
    132: {  # Anthony Davis
        "games_played": 48,
        "games_started": 48,
        "minutes_total": 1680,
        "minutes_per_game": 35.0,
        "points_total": 1200,
        "points_per_game": 25.0,
        "fgm": 480,
        "fga": 900,
        "fg_pct": 53.3,
        "tpm": 24,
        "tpa": 96,
        "tp_pct": 25.0,
        "ftm": 216,
        "fta": 288,
        "ft_pct": 75.0,
        "offensive_rebounds": 144,
        "defensive_rebounds": 432,
        "total_rebounds": 576,
        "rebounds_per_game": 12.0,
        "assists": 144,
        "assists_per_game": 3.0,
        "turnovers": 96,
        "turnovers_per_game": 2.0,
        "steals": 48,
        "steals_per_game": 1.0,
        "blocks": 96,
        "blocks_per_game": 2.0,
        "personal_fouls": 120,
        "fouls_per_game": 2.5,
        "plus_minus": 240,
        "plus_minus_per_game": 5.0,
        "efficiency": 28.3,
        "true_shooting_pct": 61.5,
        "effective_fg_pct": 54.6,
        "assist_turnover_ratio": 1.5,
        "double_doubles": 35,
        "triple_doubles": 0,
    },
    124: {  # Jayson Tatum
        "games_played": 50,
        "games_started": 50,
        "minutes_total": 1850,
        "minutes_per_game": 37.0,
        "points_total": 1350,
        "points_per_game": 27.0,
        "fgm": 450,
        "fga": 1050,
        "fg_pct": 42.9,
        "tpm": 150,
        "tpa": 450,
        "tp_pct": 33.3,
        "ftm": 300,
        "fta": 350,
        "ft_pct": 85.7,
        "offensive_rebounds": 50,
        "defensive_rebounds": 400,
        "total_rebounds": 450,
        "rebounds_per_game": 9.0,
        "assists": 250,
        "assists_per_game": 5.0,
        "turnovers": 150,
        "turnovers_per_game": 3.0,
        "steals": 50,
        "steals_per_game": 1.0,
        "blocks": 35,
        "blocks_per_game": 0.7,
        "personal_fouls": 100,
        "fouls_per_game": 2.0,
        "plus_minus": 300,
        "plus_minus_per_game": 6.0,
        "efficiency": 25.8,
        "true_shooting_pct": 59.5,
        "effective_fg_pct": 50.0,
        "assist_turnover_ratio": 1.67,
        "double_doubles": 18,
        "triple_doubles": 1,
    },
    308: {  # Jaylen Brown
        "games_played": 48,
        "games_started": 48,
        "minutes_total": 1680,
        "minutes_per_game": 35.0,
        "points_total": 1104,
        "points_per_game": 23.0,
        "fgm": 408,
        "fga": 864,
        "fg_pct": 47.2,
        "tpm": 96,
        "tpa": 288,
        "tp_pct": 33.3,
        "ftm": 192,
        "fta": 240,
        "ft_pct": 80.0,
        "offensive_rebounds": 48,
        "defensive_rebounds": 240,
        "total_rebounds": 288,
        "rebounds_per_game": 6.0,
        "assists": 168,
        "assists_per_game": 3.5,
        "turnovers": 120,
        "turnovers_per_game": 2.5,
        "steals": 48,
        "steals_per_game": 1.0,
        "blocks": 24,
        "blocks_per_game": 0.5,
        "personal_fouls": 96,
        "fouls_per_game": 2.0,
        "plus_minus": 240,
        "plus_minus_per_game": 5.0,
        "efficiency": 22.1,
        "true_shooting_pct": 58.3,
        "effective_fg_pct": 52.8,
        "assist_turnover_ratio": 1.4,
        "double_doubles": 8,
        "triple_doubles": 0,
    },
}


def seed_nba_player_stats(db, season_id: int = 2025) -> int:
    """Seed sample NBA player statistics.

    Args:
        db: Database connection
        season_id: Season ID (default 2025)

    Returns:
        Number of player stat records inserted
    """
    count = 0

    for player_id, stats in NBA_SAMPLE_STATS.items():
        try:
            # Build the INSERT statement with all stat fields
            db.execute(
                """
                INSERT INTO nba_player_stats (
                    player_id, season_id, team_id,
                    games_played, games_started, minutes_total, minutes_per_game,
                    points_total, points_per_game,
                    fgm, fga, fg_pct,
                    tpm, tpa, tp_pct,
                    ftm, fta, ft_pct,
                    offensive_rebounds, defensive_rebounds, total_rebounds, rebounds_per_game,
                    assists, assists_per_game,
                    turnovers, turnovers_per_game,
                    steals, steals_per_game,
                    blocks, blocks_per_game,
                    personal_fouls, fouls_per_game,
                    plus_minus, plus_minus_per_game,
                    efficiency, true_shooting_pct, effective_fg_pct, assist_turnover_ratio,
                    double_doubles, triple_doubles,
                    updated_at
                )
                VALUES (
                    %s, %s, NULL,
                    %s, %s, %s, %s,
                    %s, %s,
                    %s, %s, %s,
                    %s, %s, %s,
                    %s, %s, %s,
                    %s, %s, %s, %s,
                    %s, %s,
                    %s, %s,
                    %s, %s,
                    %s, %s,
                    %s, %s,
                    %s, %s,
                    %s, %s, %s, %s,
                    %s, %s,
                    NOW()
                )
                ON CONFLICT (player_id, season_id) DO UPDATE SET
                    games_played = excluded.games_played,
                    games_started = excluded.games_started,
                    minutes_total = excluded.minutes_total,
                    minutes_per_game = excluded.minutes_per_game,
                    points_total = excluded.points_total,
                    points_per_game = excluded.points_per_game,
                    fgm = excluded.fgm,
                    fga = excluded.fga,
                    fg_pct = excluded.fg_pct,
                    tpm = excluded.tpm,
                    tpa = excluded.tpa,
                    tp_pct = excluded.tp_pct,
                    ftm = excluded.ftm,
                    fta = excluded.fta,
                    ft_pct = excluded.ft_pct,
                    offensive_rebounds = excluded.offensive_rebounds,
                    defensive_rebounds = excluded.defensive_rebounds,
                    total_rebounds = excluded.total_rebounds,
                    rebounds_per_game = excluded.rebounds_per_game,
                    assists = excluded.assists,
                    assists_per_game = excluded.assists_per_game,
                    turnovers = excluded.turnovers,
                    turnovers_per_game = excluded.turnovers_per_game,
                    steals = excluded.steals,
                    steals_per_game = excluded.steals_per_game,
                    blocks = excluded.blocks,
                    blocks_per_game = excluded.blocks_per_game,
                    personal_fouls = excluded.personal_fouls,
                    fouls_per_game = excluded.fouls_per_game,
                    plus_minus = excluded.plus_minus,
                    plus_minus_per_game = excluded.plus_minus_per_game,
                    efficiency = excluded.efficiency,
                    true_shooting_pct = excluded.true_shooting_pct,
                    effective_fg_pct = excluded.effective_fg_pct,
                    assist_turnover_ratio = excluded.assist_turnover_ratio,
                    double_doubles = excluded.double_doubles,
                    triple_doubles = excluded.triple_doubles,
                    updated_at = excluded.updated_at
                """,
                (
                    player_id, season_id,
                    stats["games_played"], stats["games_started"],
                    stats["minutes_total"], stats["minutes_per_game"],
                    stats["points_total"], stats["points_per_game"],
                    stats["fgm"], stats["fga"], stats["fg_pct"],
                    stats["tpm"], stats["tpa"], stats["tp_pct"],
                    stats["ftm"], stats["fta"], stats["ft_pct"],
                    stats["offensive_rebounds"], stats["defensive_rebounds"],
                    stats["total_rebounds"], stats["rebounds_per_game"],
                    stats["assists"], stats["assists_per_game"],
                    stats["turnovers"], stats["turnovers_per_game"],
                    stats["steals"], stats["steals_per_game"],
                    stats["blocks"], stats["blocks_per_game"],
                    stats["personal_fouls"], stats["fouls_per_game"],
                    stats["plus_minus"], stats["plus_minus_per_game"],
                    stats["efficiency"], stats["true_shooting_pct"],
                    stats["effective_fg_pct"], stats["assist_turnover_ratio"],
                    stats["double_doubles"], stats["triple_doubles"],
                ),
            )
            count += 1
            logger.info("Seeded NBA stats for player %d", player_id)
        except Exception as e:
            logger.warning("Failed to seed stats for player %d: %s", player_id, e)

    return count


def seed_all_stats(db=None) -> Dict[str, int]:
    """Seed all sample statistics.

    Args:
        db: Optional database connection. If not provided, will create one.

    Returns:
        Dictionary with counts of seeded records by sport
    """
    if db is None:
        conn_str = os.environ.get("DATABASE_URL") or os.environ.get("NEON_DATABASE_URL")
        db = PostgresDB(connection_string=conn_str) if conn_str else get_stats_db(read_only=False)

    counts = {
        "nba_player_stats": 0,
        "nfl_player_stats": 0,  # TODO: Implement NFL stats
        "football_player_stats": 0,  # TODO: Implement Football stats
    }

    # Seed NBA stats
    try:
        counts["nba_player_stats"] = seed_nba_player_stats(db)
    except Exception as e:
        logger.error("Failed to seed NBA player stats: %s", e)

    # TODO: Add NFL and Football stats seeding

    return counts


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)

    # Load .env if present
    try:
        from dotenv import load_dotenv
        repo_root = Path(__file__).resolve().parents[3]
        load_dotenv(repo_root / ".env")
    except Exception:
        pass

    result = seed_all_stats()
    logger.info("Statistics seeding complete: %s", result)
