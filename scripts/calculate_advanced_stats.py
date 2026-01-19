#!/usr/bin/env python3
"""
Calculate advanced statistics from raw data.

This script runs AFTER seeding to calculate derived statistics like:
- Efficiency rating
- True Shooting Percentage
- Effective Field Goal Percentage
- Per-36 minute stats
- Assist/Turnover ratio

This follows the same pattern as percentile calculation:
1. Read raw totals from database
2. Calculate derived stats
3. Update database with calculated values

Usage:
    python scripts/calculate_advanced_stats.py --sport NBA --season 2024
    python scripts/calculate_advanced_stats.py --sport NBA --all-seasons
    python scripts/calculate_advanced_stats.py --help

The script separates stat calculation from data ingestion, keeping
seeding focused on raw data collection and enabling provider independence.
"""

from __future__ import annotations

import argparse
import logging
import sys
from datetime import datetime
from typing import Any

# Add src to path for imports
sys.path.insert(0, "src")

from scoracle_data.connection import get_db
from scoracle_data.seeders.utils import DataParsers, StatCalculators

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
)
logger = logging.getLogger(__name__)


def calculate_nba_advanced_stats(db, season_id: int) -> int:
    """
    Calculate advanced statistics for NBA players.
    
    Calculates:
    - efficiency: NBA efficiency rating
    - true_shooting_pct: True Shooting Percentage
    - effective_fg_pct: Effective Field Goal Percentage
    - assist_turnover_ratio: Assist to Turnover ratio
    
    Args:
        db: Database connection
        season_id: Season ID to calculate for
        
    Returns:
        Number of players updated
    """
    # Get all players with raw stats for this season
    players = db.fetchall("""
        SELECT 
            player_id, season_id, team_id,
            games_played, 
            points_total, 
            fgm, fga, 
            tpm, tpa,
            ftm, fta, 
            total_rebounds,
            assists, 
            turnovers, 
            steals, 
            blocks
        FROM nba_player_stats
        WHERE season_id = %s AND games_played > 0
    """, (season_id,))
    
    if not players:
        logger.warning(f"No players found for season_id {season_id}")
        return 0
    
    logger.info(f"Calculating advanced stats for {len(players)} NBA players")
    updated = 0
    
    for p in players:
        gp = p["games_played"]
        pts = p["points_total"] or 0
        fgm = p["fgm"] or 0
        fga = p["fga"] or 0
        tpm = p["tpm"] or 0
        ftm = p["ftm"] or 0
        fta = p["fta"] or 0
        reb = p["total_rebounds"] or 0
        ast = p["assists"] or 0
        tov = p["turnovers"] or 0
        stl = p["steals"] or 0
        blk = p["blocks"] or 0
        
        # Calculate efficiency
        # Formula: (PTS + REB + AST + STL + BLK - (FGA-FGM) - (FTA-FTM) - TOV) / GP
        efficiency = StatCalculators.calculate_nba_efficiency(
            pts, reb, ast, stl, blk, fgm, fga, ftm, fta, tov, gp
        )
        
        # Calculate True Shooting Percentage
        # Formula: PTS / (2 * (FGA + 0.44 * FTA))
        true_shooting = StatCalculators.calculate_true_shooting_pct(pts, fga, fta)
        
        # Calculate Effective Field Goal Percentage
        # Formula: (FGM + 0.5 * 3PM) / FGA
        effective_fg = StatCalculators.calculate_effective_fg_pct(fgm, tpm, fga)
        
        # Calculate Assist/Turnover Ratio
        assist_turnover = round(ast / max(tov, 1), 2) if ast else 0
        
        # Update the record
        db.execute("""
            UPDATE nba_player_stats
            SET efficiency = %s,
                true_shooting_pct = %s,
                effective_fg_pct = %s,
                assist_turnover_ratio = %s,
                updated_at = NOW()
            WHERE player_id = %s AND season_id = %s AND team_id = %s
        """, (
            efficiency,
            true_shooting,
            effective_fg,
            assist_turnover,
            p["player_id"],
            p["season_id"],
            p["team_id"],
        ))
        updated += 1
    
    logger.info(f"Updated {updated} NBA player records")
    return updated


def calculate_nfl_advanced_stats(db, season_id: int) -> int:
    """
    Calculate advanced statistics for NFL players.
    
    NFL has position-specific calculations. Currently calculates:
    - passer_rating (for QBs)
    - yards_per_attempt
    - completion_pct
    - yards_per_carry (for RBs)
    - yards_per_reception (for WRs/TEs)
    
    Args:
        db: Database connection
        season_id: Season ID to calculate for
        
    Returns:
        Number of players updated
    """
    # Get all players with stats
    players = db.fetchall("""
        SELECT 
            player_id, season_id, team_id,
            games_played,
            pass_attempts, pass_completions, pass_yards, pass_touchdowns, interceptions_thrown,
            rush_attempts, rush_yards, rush_touchdowns,
            receptions, receiving_yards, receiving_touchdowns, targets
        FROM nfl_player_stats
        WHERE season_id = %s AND games_played > 0
    """, (season_id,))
    
    if not players:
        logger.warning(f"No NFL players found for season_id {season_id}")
        return 0
    
    logger.info(f"Calculating advanced stats for {len(players)} NFL players")
    updated = 0
    
    for p in players:
        updates = {}
        
        # Passing stats
        pass_att = p["pass_attempts"] or 0
        if pass_att > 0:
            pass_comp = p["pass_completions"] or 0
            pass_yds = p["pass_yards"] or 0
            pass_td = p["pass_touchdowns"] or 0
            ints = p["interceptions_thrown"] or 0
            
            updates["completion_pct"] = DataParsers.safe_percentage(pass_comp, pass_att)
            updates["yards_per_attempt"] = round(pass_yds / pass_att, 1)
            
            # NFL Passer Rating formula
            # a = ((COMP/ATT) - 0.3) * 5
            # b = ((YDS/ATT) - 3) * 0.25
            # c = (TD/ATT) * 20
            # d = 2.375 - ((INT/ATT) * 25)
            # Rating = ((a + b + c + d) / 6) * 100
            a = min(max(((pass_comp / pass_att) - 0.3) * 5, 0), 2.375)
            b = min(max(((pass_yds / pass_att) - 3) * 0.25, 0), 2.375)
            c = min(max((pass_td / pass_att) * 20, 0), 2.375)
            d = min(max(2.375 - ((ints / pass_att) * 25), 0), 2.375)
            passer_rating = round(((a + b + c + d) / 6) * 100, 1)
            updates["passer_rating"] = passer_rating
        
        # Rushing stats
        rush_att = p["rush_attempts"] or 0
        if rush_att > 0:
            rush_yds = p["rush_yards"] or 0
            updates["yards_per_carry"] = round(rush_yds / rush_att, 1)
        
        # Receiving stats
        receptions = p["receptions"] or 0
        if receptions > 0:
            rec_yds = p["receiving_yards"] or 0
            updates["yards_per_reception"] = round(rec_yds / receptions, 1)
        
        if updates:
            # Build dynamic update query
            set_clauses = [f"{k} = %s" for k in updates.keys()]
            set_clauses.append("updated_at = NOW()")
            
            query = f"""
                UPDATE nfl_player_stats
                SET {', '.join(set_clauses)}
                WHERE player_id = %s AND season_id = %s AND team_id = %s
            """
            
            params = list(updates.values()) + [p["player_id"], p["season_id"], p["team_id"]]
            db.execute(query, tuple(params))
            updated += 1
    
    logger.info(f"Updated {updated} NFL player records")
    return updated


def calculate_football_advanced_stats(db, season_id: int, league_id: int | None = None) -> int:
    """
    Calculate advanced statistics for Football (Soccer) players.
    
    Calculates:
    - goals_per_90: Goals per 90 minutes
    - assists_per_90: Assists per 90 minutes
    - goals_assists: Goals + Assists
    - shot_accuracy: Shots on target / Total shots
    - pass_accuracy: Accurate passes / Total passes
    - dribble_success_rate: Successful dribbles / Attempted dribbles
    
    Args:
        db: Database connection
        season_id: Season ID
        league_id: Optional league filter
        
    Returns:
        Number of players updated
    """
    query = """
        SELECT 
            player_id, season_id, team_id, league_id,
            appearances, minutes_played,
            goals, assists,
            shots_total, shots_on_target,
            passes_total, passes_accurate,
            dribbles_attempted, dribbles_successful
        FROM football_player_stats
        WHERE season_id = %s AND appearances > 0
    """
    params: list[Any] = [season_id]
    
    if league_id:
        query += " AND league_id = %s"
        params.append(league_id)
    
    players = db.fetchall(query, tuple(params))
    
    if not players:
        logger.warning(f"No Football players found for season_id {season_id}")
        return 0
    
    logger.info(f"Calculating advanced stats for {len(players)} Football players")
    updated = 0
    
    for p in players:
        updates = {}
        
        minutes = p["minutes_played"] or 0
        goals = p["goals"] or 0
        assists = p["assists"] or 0
        
        # Goals + Assists
        updates["goals_assists"] = goals + assists
        
        # Per-90 stats
        if minutes >= 90:
            ninety_mins = minutes / 90
            updates["goals_per_90"] = round(goals / ninety_mins, 2)
            updates["assists_per_90"] = round(assists / ninety_mins, 2)
        else:
            updates["goals_per_90"] = 0
            updates["assists_per_90"] = 0
        
        # Shot accuracy
        shots_total = p["shots_total"] or 0
        shots_on = p["shots_on_target"] or 0
        updates["shot_accuracy"] = DataParsers.safe_percentage(shots_on, shots_total)
        
        # Pass accuracy
        passes_total = p["passes_total"] or 0
        passes_acc = p["passes_accurate"] or 0
        updates["pass_accuracy"] = DataParsers.safe_percentage(passes_acc, passes_total)
        
        # Dribble success rate
        dribbles_att = p["dribbles_attempted"] or 0
        dribbles_succ = p["dribbles_successful"] or 0
        updates["dribble_success_rate"] = DataParsers.safe_percentage(dribbles_succ, dribbles_att)
        
        # Build update query
        set_clauses = [f"{k} = %s" for k in updates.keys()]
        set_clauses.append("updated_at = NOW()")
        
        query = f"""
            UPDATE football_player_stats
            SET {', '.join(set_clauses)}
            WHERE player_id = %s AND season_id = %s AND team_id = %s AND league_id = %s
        """
        
        params = list(updates.values()) + [
            p["player_id"], p["season_id"], p["team_id"], p["league_id"]
        ]
        db.execute(query, tuple(params))
        updated += 1
    
    logger.info(f"Updated {updated} Football player records")
    return updated


def get_season_id(db, sport: str, season_year: int) -> int | None:
    """Get season ID for a sport and year."""
    result = db.fetchone(
        "SELECT id FROM seasons WHERE sport_id = %s AND season_year = %s",
        (sport.upper(), season_year)
    )
    return result["id"] if result else None


def get_all_seasons(db, sport: str) -> list[tuple[int, int]]:
    """Get all seasons for a sport. Returns [(season_id, season_year), ...]"""
    results = db.fetchall(
        "SELECT id, season_year FROM seasons WHERE sport_id = %s ORDER BY season_year",
        (sport.upper(),)
    )
    return [(r["id"], r["season_year"]) for r in results]


def main():
    parser = argparse.ArgumentParser(
        description="Calculate advanced statistics from raw data"
    )
    parser.add_argument(
        "--sport",
        required=True,
        choices=["NBA", "NFL", "FOOTBALL"],
        help="Sport to calculate for",
    )
    parser.add_argument(
        "--season",
        type=int,
        help="Season year (e.g., 2024)",
    )
    parser.add_argument(
        "--all-seasons",
        action="store_true",
        help="Calculate for all seasons",
    )
    parser.add_argument(
        "--league-id",
        type=int,
        help="League ID (for FOOTBALL)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be calculated without making changes",
    )
    
    args = parser.parse_args()
    
    if not args.season and not args.all_seasons:
        parser.error("Either --season or --all-seasons is required")
    
    db = get_db()
    sport = args.sport.upper()
    
    # Determine seasons to process
    if args.all_seasons:
        seasons = get_all_seasons(db, sport)
        if not seasons:
            logger.error(f"No seasons found for {sport}")
            return 1
        logger.info(f"Processing {len(seasons)} seasons for {sport}")
    else:
        season_id = get_season_id(db, sport, args.season)
        if not season_id:
            logger.error(f"Season {args.season} not found for {sport}")
            return 1
        seasons = [(season_id, args.season)]
    
    if args.dry_run:
        logger.info("DRY RUN - no changes will be made")
        for season_id, season_year in seasons:
            logger.info(f"Would calculate stats for {sport} {season_year} (season_id={season_id})")
        return 0
    
    # Calculate stats for each season
    total_updated = 0
    
    for season_id, season_year in seasons:
        logger.info(f"Processing {sport} {season_year}...")
        
        if sport == "NBA":
            updated = calculate_nba_advanced_stats(db, season_id)
        elif sport == "NFL":
            updated = calculate_nfl_advanced_stats(db, season_id)
        elif sport == "FOOTBALL":
            updated = calculate_football_advanced_stats(db, season_id, args.league_id)
        else:
            logger.error(f"Unknown sport: {sport}")
            return 1
        
        total_updated += updated
    
    logger.info(f"Complete! Updated {total_updated} total records")
    return 0


if __name__ == "__main__":
    sys.exit(main())
