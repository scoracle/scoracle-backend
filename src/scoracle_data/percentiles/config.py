"""
Configuration for percentile calculations.

Defines which statistics are used for percentile calculations
for each sport and entity type.
"""

from __future__ import annotations

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
    position_group: str | None = None,
) -> list[str]:
    """
    Get the stat categories for percentile calculation.

    Args:
        sport_id: Sport identifier (NBA, NFL, FOOTBALL)
        entity_type: 'player' or 'team'
        position_group: Position group for position-specific stats

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
