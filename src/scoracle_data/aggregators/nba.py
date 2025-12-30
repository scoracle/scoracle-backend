"""
NBA statistics aggregator.

Handles aggregation of game-by-game stats into season totals.
The NBA API returns individual game records, which need to be summed
into season-level statistics for storage.

Design: Self-contained with no external dependencies.
This module is designed to be extracted to scoracle-data repo.
"""

from __future__ import annotations

from typing import Any, Dict, List, Optional


class NBAStatsAggregator:
    """Aggregate NBA game-by-game statistics into season totals."""

    @staticmethod
    def parse_minutes(minutes_value: Any) -> float:
        """Parse minutes from various formats (MM:SS string, float, or int).

        Args:
            minutes_value: Minutes in MM:SS format, or numeric value

        Returns:
            Total minutes as a float, or 0 if parsing fails
        """
        if not minutes_value:
            return 0.0

        # Handle "MM:SS" format
        if isinstance(minutes_value, str) and ":" in minutes_value:
            parts = minutes_value.split(":")
            minutes = int(parts[0])
            seconds = int(parts[1]) if len(parts) > 1 else 0
            return minutes + (seconds / 60.0)

        # Handle numeric formats
        try:
            return float(minutes_value)
        except (ValueError, TypeError):
            return 0.0

    @staticmethod
    def parse_plus_minus(plus_minus_value: Any) -> int:
        """Parse plus/minus value from string (e.g., '+5', '-3') or int.

        Args:
            plus_minus_value: Plus/minus value as string or int

        Returns:
            Parsed plus/minus value, or 0 if parsing fails
        """
        if not plus_minus_value:
            return 0

        # Handle string format like "+5" or "-3"
        if isinstance(plus_minus_value, str):
            try:
                return int(plus_minus_value)
            except ValueError:
                return 0

        # Handle numeric format
        try:
            return int(plus_minus_value)
        except (ValueError, TypeError):
            return 0

    @staticmethod
    def extract_game_season(game: Dict[str, Any]) -> Optional[int]:
        """Extract season from a game record.

        Attempts to find season in game.game.season or game.team.season.

        Args:
            game: Game record from API

        Returns:
            Season as integer, or None if not found
        """
        # Try game.game.season first
        game_data = game.get("game", {}) or {}
        game_season = game_data.get("season")

        # Fall back to team.season
        if not game_season:
            team_data = game.get("team", {}) or {}
            game_season = team_data.get("season")

        # Parse season as integer
        if game_season:
            try:
                return int(game_season)
            except (ValueError, TypeError):
                return None
        return None

    @staticmethod
    def filter_games_by_season(
        games: List[Dict[str, Any]], target_season: int
    ) -> List[Dict[str, Any]]:
        """Filter games to only include those from the target season.

        If a game's season cannot be determined, it is included by default.

        Args:
            games: List of game records
            target_season: Target season to filter for

        Returns:
            Filtered list of games
        """
        filtered_games = []
        for game in games:
            game_season = NBAStatsAggregator.extract_game_season(game)
            # Include game if: no season found, or season matches target
            if game_season is None or game_season == target_season:
                filtered_games.append(game)
        return filtered_games

    @staticmethod
    def aggregate_player_stats(
        games: List[Dict[str, Any]], target_season: Optional[int] = None
    ) -> Dict[str, Any]:
        """Aggregate NBA game-by-game stats into season totals.

        The NBA API returns individual game records. This method sums them
        into season totals for storage in the stats database.

        Args:
            games: List of game records from the API
            target_season: If provided, only aggregate games from this season

        Returns:
            Aggregated season statistics
        """
        if not games:
            return {}

        # Filter games by season if target_season specified
        if target_season:
            games = NBAStatsAggregator.filter_games_by_season(games, target_season)

        # Use first game as template for player/team info
        first_game = games[0]

        # Initialize aggregates
        games_played = len(games)
        games_started = 0
        total_minutes = 0
        points = 0
        fgm = 0
        fga = 0
        tpm = 0  # 3-pointers made
        tpa = 0  # 3-pointers attempted
        ftm = 0
        fta = 0
        off_reb = 0
        def_reb = 0
        tot_reb = 0
        assists = 0
        turnovers = 0
        steals = 0
        blocks = 0
        fouls = 0
        plus_minus = 0

        for game in games:
            # Check if started
            if game.get("pos") or game.get("game", {}).get("start"):
                games_started += 1

            # Parse minutes (can be "MM:SS" or just minutes)
            mins = game.get("min") or game.get("minutes")
            total_minutes += NBAStatsAggregator.parse_minutes(mins)

            # Sum counting stats
            points += game.get("points") or 0
            fgm += game.get("fgm") or 0
            fga += game.get("fga") or 0
            tpm += game.get("tpm") or 0
            tpa += game.get("tpa") or 0
            ftm += game.get("ftm") or 0
            fta += game.get("fta") or 0
            off_reb += game.get("offReb") or 0
            def_reb += game.get("defReb") or 0
            tot_reb += game.get("totReb") or 0
            assists += game.get("assists") or 0
            turnovers += game.get("turnovers") or 0
            steals += game.get("steals") or 0
            blocks += game.get("blocks") or 0
            fouls += game.get("pFouls") or 0

            # Plus/minus can be string like "+5" or "-3"
            plus_minus += NBAStatsAggregator.parse_plus_minus(game.get("plusMinus"))

        # Build aggregated result matching the structure expected by transform_player_stats
        # The transform expects specific key names and nested structures
        return {
            "player": first_game.get("player"),
            "team": first_game.get("team"),
            # Transform expects "games" with "played" and "started" keys
            "games": {"played": games_played, "started": games_started},
            # Transform checks for points dict with "total" or "points_total" key
            "points_total": points,
            # Minutes as integer
            "min": int(total_minutes),
            # Shooting stats as simple integers (transform handles this)
            "fgm": fgm,
            "fga": fga,
            "fgp": round((fgm / fga * 100), 1) if fga > 0 else 0,
            "tpm": tpm,
            "tpa": tpa,
            "tpp": round((tpm / tpa * 100), 1) if tpa > 0 else 0,
            "ftm": ftm,
            "fta": fta,
            "ftp": round((ftm / fta * 100), 1) if fta > 0 else 0,
            # Rebounds - transform looks for offReb, defReb, totReb
            "offReb": off_reb,
            "defReb": def_reb,
            "totReb": tot_reb,
            # Other counting stats
            "assists": assists,
            "turnovers": turnovers,
            "steals": steals,
            "blocks": blocks,
            "pFouls": fouls,
            "plusMinus": plus_minus,
            # Mark as aggregated season totals
            "_aggregated": True,
            "_games_count": games_played,
        }
