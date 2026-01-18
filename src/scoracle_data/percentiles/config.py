"""
Configuration for percentile calculations.

Defines which statistics are used for percentile calculations
for each sport and entity type.
"""

from __future__ import annotations

from typing import Optional

# Stat categories for percentile calculations
# These are the key stats used for radar charts and player comparisons
PERCENTILE_CATEGORIES: dict[str, dict[str, list[str]]] = {
    "NBA": {
        "player": [
            # Per-36 normalized stats (industry standard for fair comparison)
            "points_per_36",
            "rebounds_per_36",
            "assists_per_36",
            "steals_per_36",
            "blocks_per_36",
            # Shooting efficiency (already rate-based)
            "fg_pct",
            "tp_pct",
            "ft_pct",
            "true_shooting_pct",
            # Advanced
            "efficiency",
            "plus_minus_per_game",
            "minutes_per_game",
        ],
        "team": [
            "win_pct",
            "points_per_game",
            "opponent_ppg",
            "point_differential",
            "fg_pct",
            "tp_pct",
            "total_rebounds_per_game",
            "assists_per_game",
            "steals_per_game",
            "blocks_per_game",
            "offensive_rating",
            "defensive_rating",
        ],
    },
    "NFL": {
        # QB stats
        "player_passing": [
            "pass_yards",
            "pass_yards_per_game",
            "pass_touchdowns",
            "passer_rating",
            "completion_pct",
            "yards_per_attempt",
            "td_int_ratio",
        ],
        # RB/WR rushing stats
        "player_rushing": [
            "rush_yards",
            "rush_yards_per_game",
            "rush_touchdowns",
            "yards_per_carry",
        ],
        # Receiving stats
        "player_receiving": [
            "receiving_yards",
            "receiving_yards_per_game",
            "receiving_touchdowns",
            "receptions",
            "yards_per_reception",
            "catch_pct",
        ],
        # Defensive stats
        "player_defense": [
            "tackles_total",
            "sacks",
            "interceptions",
            "passes_defended",
            "forced_fumbles",
        ],
        "team": [
            "win_pct",
            "points_per_game",
            "opponent_ppg",
            "point_differential",
            "yards_per_game",
            "pass_yards",
            "rush_yards",
            "yards_allowed",
            "takeaways",
            "sacks",
        ],
    },
    "FOOTBALL": {
        "player": [
            # Per-90 normalized stats (industry standard for fair comparison)
            "goals_per_90",
            "assists_per_90",
            "key_passes_per_90",
            "shots_per_90",
            "tackles_per_90",
            "interceptions_per_90",
            # Rate-based stats (already normalized)
            "shot_accuracy",
            "pass_accuracy",
            "dribble_success_rate",
            "duel_success_rate",
            # Playing time
            "minutes_played",
        ],
        # Goalkeeper specific
        "player_goalkeeper": [
            "save_percentage",
            "goals_conceded_per_90",
            "clean_sheets",
        ],
        "team": [
            # Rate-based stats (comparable across leagues)
            "points_per_game",
            "goals_per_game",
            "goals_conceded_per_game",
            "goal_difference",
            "clean_sheets",
            "avg_possession",
            "shot_accuracy",
            "pass_accuracy",
        ],
    },
}

# Minimum sample sizes for meaningful percentiles
# Note: These are no longer used to EXCLUDE data - percentiles are always calculated.
# They are kept for reference and potential future use.
MIN_SAMPLE_SIZES = {
    "NBA": {
        "player": 50,  # At least 50 players in comparison group
        "team": 20,    # All 30 teams, but some may be filtered
    },
    "NFL": {
        "player": 30,
        "team": 20,
    },
    "FOOTBALL": {
        "player": 100,  # Many players across leagues
        "team": 15,     # Per league
    },
}

# Threshold for small sample warning flag in API responses
# When sample_size < this value, percentile_metadata.small_sample_warning = true
SMALL_SAMPLE_WARNING_THRESHOLD = 20

# Stats where higher is worse (for inverse percentile calculation)
INVERSE_STATS = {
    "turnovers_per_game",
    "turnovers_per_36",
    "fouls_per_game",
    "interceptions",  # For QBs, not defensive players
    "sacks_taken",
    "fumbles",
    "fumbles_lost",
    "goals_conceded",
    "goals_conceded_per_90",
    "goals_conceded_per_game",
    "goals_against",
    "opponent_ppg",
    "yards_allowed",
    "yellow_cards",
    "red_cards",
}

# Position groups for player comparisons
POSITION_GROUPS = {
    "NBA": ["Guard", "Forward", "Center"],
    "NFL": [
        "Offense - Skill",
        "Offense - Line",
        "Defense - Line",
        "Defense - Linebacker",
        "Defense - Secondary",
        "Special Teams",
    ],
    "FOOTBALL": ["Goalkeeper", "Defender", "Midfielder", "Forward"],
}


def get_stat_categories(
    sport_id: str,
    entity_type: str,
    position_group: Optional[str] = None,
) -> list[str]:
    """
    Get the stat categories for percentile calculation.

    Args:
        sport_id: Sport identifier (NBA, NFL, FOOTBALL)
        entity_type: 'player' or 'team'
        position_group: Optional position group for position-specific stats

    Returns:
        List of stat category names
    """
    sport_config = PERCENTILE_CATEGORIES.get(sport_id, {})

    if entity_type == "team":
        return sport_config.get("team", [])

    # For players, check position-specific categories
    if sport_id == "NFL" and position_group:
        if "Offense - Skill" in position_group:
            # Return all offensive skill position stats
            categories = []
            categories.extend(sport_config.get("player_passing", []))
            categories.extend(sport_config.get("player_rushing", []))
            categories.extend(sport_config.get("player_receiving", []))
            return list(set(categories))
        elif "Defense" in position_group:
            return sport_config.get("player_defense", [])

    if sport_id == "FOOTBALL" and position_group == "Goalkeeper":
        return sport_config.get("player_goalkeeper", [])

    return sport_config.get("player", [])


def is_inverse_stat(stat_name: str) -> bool:
    """Check if a stat should have inverse percentile (lower is better)."""
    return stat_name in INVERSE_STATS


def get_min_sample_size(sport_id: str, entity_type: str) -> int:
    """Get the minimum sample size for meaningful percentiles."""
    return MIN_SAMPLE_SIZES.get(sport_id, {}).get(entity_type, 30)


# =============================================================================
# STAT CATEGORY MAPPINGS FOR FRONTEND WIDGETS
# =============================================================================
# These define how stats are grouped into display categories for the UI.
# Each category has a label and a list of stats with their display labels.

STAT_CATEGORY_MAPPINGS: dict[str, dict[str, dict[str, dict]]] = {
    "NBA": {
        "player": {
            "scoring": {
                "label": "Scoring",
                "stats": [
                    {"key": "points_per_36", "label": "Points/36"},
                    {"key": "points_per_game", "label": "Points/Game"},
                    {"key": "fg_pct", "label": "FG%"},
                    {"key": "tp_pct", "label": "3PT%"},
                    {"key": "ft_pct", "label": "FT%"},
                    {"key": "true_shooting_pct", "label": "True Shooting%"},
                    {"key": "effective_fg_pct", "label": "Effective FG%"},
                ],
            },
            "possession": {
                "label": "Possession",
                "stats": [
                    {"key": "assists_per_36", "label": "Assists/36"},
                    {"key": "assists_per_game", "label": "Assists/Game"},
                    {"key": "rebounds_per_36", "label": "Rebounds/36"},
                    {"key": "rebounds_per_game", "label": "Rebounds/Game"},
                    {"key": "offensive_rebounds", "label": "Off Rebounds"},
                    {"key": "defensive_rebounds", "label": "Def Rebounds"},
                    {"key": "turnovers_per_36", "label": "Turnovers/36"},
                    {"key": "turnovers_per_game", "label": "Turnovers/Game"},
                    {"key": "assist_turnover_ratio", "label": "AST/TO Ratio"},
                ],
            },
            "defense": {
                "label": "Defense",
                "stats": [
                    {"key": "steals_per_36", "label": "Steals/36"},
                    {"key": "steals_per_game", "label": "Steals/Game"},
                    {"key": "blocks_per_36", "label": "Blocks/36"},
                    {"key": "blocks_per_game", "label": "Blocks/Game"},
                ],
            },
            "discipline": {
                "label": "Discipline",
                "stats": [
                    {"key": "fouls_per_game", "label": "Fouls/Game"},
                ],
            },
        },
        "team": {
            "record": {
                "label": "Record",
                "stats": [
                    {"key": "win_pct", "label": "Win %"},
                    {"key": "wins", "label": "Wins"},
                    {"key": "losses", "label": "Losses"},
                    {"key": "point_differential", "label": "Point Diff"},
                ],
            },
            "offense": {
                "label": "Offense",
                "stats": [
                    {"key": "points_per_game", "label": "Points/Game"},
                    {"key": "fg_pct", "label": "FG%"},
                    {"key": "tp_pct", "label": "3PT%"},
                    {"key": "ft_pct", "label": "FT%"},
                    {"key": "assists_per_game", "label": "Assists/Game"},
                    {"key": "offensive_rating", "label": "Off Rating"},
                ],
            },
            "defense": {
                "label": "Defense",
                "stats": [
                    {"key": "opponent_ppg", "label": "Opp Points/Game"},
                    {"key": "steals_per_game", "label": "Steals/Game"},
                    {"key": "blocks_per_game", "label": "Blocks/Game"},
                    {"key": "defensive_rating", "label": "Def Rating"},
                ],
            },
            "discipline": {
                "label": "Discipline",
                "stats": [
                    {"key": "turnovers_per_game", "label": "Turnovers/Game"},
                    {"key": "fouls_per_game", "label": "Fouls/Game"},
                ],
            },
        },
    },
    "NFL": {
        "player": {
            "passing": {
                "label": "Passing",
                "stats": [
                    {"key": "pass_yards", "label": "Pass Yards"},
                    {"key": "pass_yards_per_game", "label": "Pass Yards/Game"},
                    {"key": "pass_touchdowns", "label": "Pass TDs"},
                    {"key": "passer_rating", "label": "Passer Rating"},
                    {"key": "completion_pct", "label": "Completion %"},
                    {"key": "yards_per_attempt", "label": "Yards/Attempt"},
                    {"key": "td_int_ratio", "label": "TD/INT Ratio"},
                    {"key": "interceptions", "label": "Interceptions"},
                ],
            },
            "rushing": {
                "label": "Rushing",
                "stats": [
                    {"key": "rush_yards", "label": "Rush Yards"},
                    {"key": "rush_yards_per_game", "label": "Rush Yards/Game"},
                    {"key": "rush_touchdowns", "label": "Rush TDs"},
                    {"key": "yards_per_carry", "label": "Yards/Carry"},
                    {"key": "rush_attempts", "label": "Rush Attempts"},
                    {"key": "longest_rush", "label": "Long Rush"},
                ],
            },
            "receiving": {
                "label": "Receiving",
                "stats": [
                    {"key": "receiving_yards", "label": "Receiving Yards"},
                    {"key": "receiving_yards_per_game", "label": "Rec Yards/Game"},
                    {"key": "receiving_touchdowns", "label": "Receiving TDs"},
                    {"key": "receptions", "label": "Receptions"},
                    {"key": "targets", "label": "Targets"},
                    {"key": "yards_per_reception", "label": "Yards/Reception"},
                    {"key": "catch_pct", "label": "Catch %"},
                ],
            },
            "defense": {
                "label": "Defense",
                "stats": [
                    {"key": "tackles_total", "label": "Total Tackles"},
                    {"key": "tackles_solo", "label": "Solo Tackles"},
                    {"key": "sacks", "label": "Sacks"},
                    {"key": "interceptions", "label": "Interceptions"},
                    {"key": "passes_defended", "label": "Passes Defended"},
                    {"key": "forced_fumbles", "label": "Forced Fumbles"},
                    {"key": "qb_hits", "label": "QB Hits"},
                ],
            },
            "kicking": {
                "label": "Kicking",
                "stats": [
                    {"key": "fg_made", "label": "FG Made"},
                    {"key": "fg_attempts", "label": "FG Attempts"},
                    {"key": "fg_pct", "label": "FG %"},
                    {"key": "fg_long", "label": "Long FG"},
                    {"key": "xp_made", "label": "XP Made"},
                    {"key": "xp_pct", "label": "XP %"},
                    {"key": "total_points", "label": "Total Points"},
                ],
            },
        },
        "team": {
            "record": {
                "label": "Record",
                "stats": [
                    {"key": "win_pct", "label": "Win %"},
                    {"key": "wins", "label": "Wins"},
                    {"key": "losses", "label": "Losses"},
                    {"key": "ties", "label": "Ties"},
                    {"key": "point_differential", "label": "Point Diff"},
                ],
            },
            "offense": {
                "label": "Offense",
                "stats": [
                    {"key": "points_per_game", "label": "Points/Game"},
                    {"key": "yards_per_game", "label": "Yards/Game"},
                    {"key": "pass_yards_per_game", "label": "Pass Yards/Game"},
                    {"key": "rush_yards_per_game", "label": "Rush Yards/Game"},
                    {"key": "third_down_pct", "label": "3rd Down %"},
                    {"key": "red_zone_pct", "label": "Red Zone %"},
                ],
            },
            "defense": {
                "label": "Defense",
                "stats": [
                    {"key": "opponent_ppg", "label": "Opp Points/Game"},
                    {"key": "yards_allowed_per_game", "label": "Yards Allowed/Game"},
                    {"key": "pass_yards_allowed", "label": "Pass Yards Allowed"},
                    {"key": "rush_yards_allowed", "label": "Rush Yards Allowed"},
                    {"key": "takeaways", "label": "Takeaways"},
                    {"key": "sacks_allowed", "label": "Sacks"},
                ],
            },
            "discipline": {
                "label": "Discipline",
                "stats": [
                    {"key": "turnovers", "label": "Turnovers"},
                    {"key": "penalties", "label": "Penalties"},
                    {"key": "penalty_yards", "label": "Penalty Yards"},
                    {"key": "turnover_differential", "label": "Turnover Diff"},
                ],
            },
        },
    },
    "FOOTBALL": {
        "player": {
            "scoring": {
                "label": "Scoring",
                "stats": [
                    {"key": "goals", "label": "Goals"},
                    {"key": "assists", "label": "Assists"},
                    {"key": "goals_assists", "label": "Goals + Assists"},
                    {"key": "goals_per_90", "label": "Goals/90"},
                    {"key": "assists_per_90", "label": "Assists/90"},
                    {"key": "penalties_scored", "label": "Penalties Scored"},
                ],
            },
            "possession": {
                "label": "Possession",
                "stats": [
                    {"key": "pass_accuracy", "label": "Pass Accuracy"},
                    {"key": "passes_per_90", "label": "Passes/90"},
                    {"key": "key_passes", "label": "Key Passes"},
                    {"key": "key_passes_per_90", "label": "Key Passes/90"},
                    {"key": "dribbles_successful", "label": "Successful Dribbles"},
                    {"key": "dribble_success_rate", "label": "Dribble Success %"},
                ],
            },
            "defense": {
                "label": "Defense",
                "stats": [
                    {"key": "tackles", "label": "Tackles"},
                    {"key": "tackles_per_90", "label": "Tackles/90"},
                    {"key": "interceptions", "label": "Interceptions"},
                    {"key": "interceptions_per_90", "label": "Interceptions/90"},
                    {"key": "duel_success_rate", "label": "Duel Success %"},
                    {"key": "aerial_duel_success_rate", "label": "Aerial Duel %"},
                    {"key": "clearances", "label": "Clearances"},
                    {"key": "blocks", "label": "Blocks"},
                ],
            },
            "discipline": {
                "label": "Discipline",
                "stats": [
                    {"key": "yellow_cards", "label": "Yellow Cards"},
                    {"key": "red_cards", "label": "Red Cards"},
                    {"key": "fouls_committed", "label": "Fouls Committed"},
                    {"key": "fouls_drawn", "label": "Fouls Drawn"},
                ],
            },
            "goalkeeping": {
                "label": "Goalkeeping",
                "stats": [
                    {"key": "saves", "label": "Saves"},
                    {"key": "save_percentage", "label": "Save %"},
                    {"key": "clean_sheets", "label": "Clean Sheets"},
                    {"key": "goals_conceded", "label": "Goals Conceded"},
                    {"key": "goals_conceded_per_90", "label": "Goals Conceded/90"},
                    {"key": "penalty_saves", "label": "Penalty Saves"},
                ],
            },
        },
        "team": {
            "record": {
                "label": "Record",
                "stats": [
                    {"key": "points", "label": "Points"},
                    {"key": "wins", "label": "Wins"},
                    {"key": "draws", "label": "Draws"},
                    {"key": "losses", "label": "Losses"},
                    {"key": "goal_difference", "label": "Goal Diff"},
                    {"key": "league_position", "label": "Position"},
                ],
            },
            "offense": {
                "label": "Offense",
                "stats": [
                    {"key": "goals_for", "label": "Goals Scored"},
                    {"key": "goals_per_game", "label": "Goals/Game"},
                    {"key": "shots_per_game", "label": "Shots/Game"},
                    {"key": "shots_on_target_per_game", "label": "Shots on Target/Game"},
                    {"key": "avg_possession", "label": "Possession %"},
                    {"key": "pass_accuracy", "label": "Pass Accuracy"},
                ],
            },
            "defense": {
                "label": "Defense",
                "stats": [
                    {"key": "goals_against", "label": "Goals Conceded"},
                    {"key": "goals_conceded_per_game", "label": "Conceded/Game"},
                    {"key": "clean_sheets", "label": "Clean Sheets"},
                    {"key": "tackles_per_game", "label": "Tackles/Game"},
                    {"key": "interceptions_per_game", "label": "Interceptions/Game"},
                ],
            },
            "discipline": {
                "label": "Discipline",
                "stats": [
                    {"key": "yellow_cards", "label": "Yellow Cards"},
                    {"key": "red_cards", "label": "Red Cards"},
                    {"key": "fouls_per_game", "label": "Fouls/Game"},
                ],
            },
        },
    },
}


def get_category_mappings(
    sport_id: str,
    entity_type: str,
    position_group: Optional[str] = None,
    available_stats: Optional[set[str]] = None,
) -> dict[str, dict]:
    """
    Get category mappings for a sport/entity type.

    For NFL players, returns ALL categories - the service layer will filter
    based on which stats actually have data. This allows a WR who threw a pass
    to show passing stats, or a RB with receptions to show receiving stats.

    For Football (soccer) players, goalkeeping stats are only shown for goalkeepers.

    Args:
        sport_id: Sport identifier (NBA, NFL, FOOTBALL)
        entity_type: 'player' or 'team'
        position_group: Optional position group for position-specific filtering
        available_stats: Optional set of stat keys that have data (for data-driven filtering)

    Returns:
        Dict of category_id -> category config with label and stats
    """
    sport_config = STAT_CATEGORY_MAPPINGS.get(sport_id, {})
    entity_config = sport_config.get(entity_type, {})

    if not entity_config:
        return {}

    # For NFL players, return ALL categories - let the service filter by available data
    # This allows trick plays (WR throws, RB catches, etc.) to show up
    if sport_id == "NFL" and entity_type == "player":
        # Return all categories - the service will only include ones with data
        return entity_config

    # For Football (soccer) players, filter goalkeeping based on position
    if sport_id == "FOOTBALL" and entity_type == "player":
        position_upper = position_group.upper() if position_group else ""

        if "GOALKEEPER" in position_upper or "GK" in position_upper:
            # Goalkeepers get goalkeeping + discipline only
            return {k: v for k, v in entity_config.items() if k in ["goalkeeping", "discipline"]}
        else:
            # Outfield players don't see goalkeeping stats
            return {k: v for k, v in entity_config.items() if k != "goalkeeping"}

    return entity_config


def get_stat_label(sport_id: str, entity_type: str, stat_key: str) -> str:
    """
    Get the display label for a stat key.

    Args:
        sport_id: Sport identifier
        entity_type: 'player' or 'team'
        stat_key: The stat key (e.g., 'points_per_game')

    Returns:
        Human-readable label or the key itself if not found
    """
    sport_config = STAT_CATEGORY_MAPPINGS.get(sport_id, {})
    entity_config = sport_config.get(entity_type, {})

    for category in entity_config.values():
        for stat in category.get("stats", []):
            if stat["key"] == stat_key:
                return stat["label"]

    # Fallback: convert snake_case to Title Case
    return stat_key.replace("_", " ").title()
