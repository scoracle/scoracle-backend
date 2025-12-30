"""
Shared utilities for sport-specific seeders.

These utilities are used across all seeders (NFL, NBA, Football) to:
- Parse player/team data from various API formats
- Convert units (height, weight, etc.)
- Calculate statistics safely

Design: Self-contained with no external dependencies beyond Python stdlib.
This module is designed to be extracted to scoracle-data repo.
"""

from __future__ import annotations

import re
from typing import Any, Optional


class DataParsers:
    """Shared data parsing utilities for all sports.

    Handles conversion between different API formats and units.
    """

    @staticmethod
    def parse_height_to_inches(value: Any, source_format: str = "auto") -> Optional[int]:
        """Parse height from various formats to total inches.

        Supports:
        - Imperial: "6' 2\"", "6-2", "6 2", dict with feets/inches
        - Metric: "180 cm", "180", "1.80 m", dict with meters/cm

        Args:
            value: Height value in any supported format
            source_format: "imperial", "metric", or "auto" (default)

        Returns:
            Total height in inches, or None if parsing fails
        """
        if not value:
            return None

        # Handle dict format (NBA/NFL API style)
        if isinstance(value, dict):
            # Try imperial format first
            if "feets" in value or "feet" in value:
                try:
                    feet = int(value.get("feets") or value.get("feet") or 0)
                    inches = int(value.get("inches") or 0)
                    return feet * 12 + inches
                except (ValueError, TypeError):
                    pass

            # Try metric format
            if "meters" in value and value["meters"]:
                try:
                    meters = float(value["meters"])
                    return int(meters * 39.3701)
                except (ValueError, TypeError):
                    pass

            if "cm" in value and value["cm"]:
                try:
                    cm = int(value["cm"])
                    return int(cm / 2.54)
                except (ValueError, TypeError):
                    pass

        # Handle string formats
        elif isinstance(value, str):
            # Try imperial: "6' 2\"", "6-2", "6 2"
            match = re.match(r"(\d+)['\-\s]+(\d+)", value)
            if match:
                try:
                    feet = int(match.group(1))
                    inches = int(match.group(2))
                    return feet * 12 + inches
                except (ValueError, TypeError):
                    pass

            # Try metric: "180 cm", "180"
            match = re.match(r"(\d+(?:\.\d+)?)\s*(?:cm)?", value)
            if match:
                try:
                    num = float(match.group(1))
                    # If > 10, assume cm; if < 10, assume meters
                    if num > 10:
                        return int(num / 2.54)
                    else:
                        return int(num * 39.3701)
                except (ValueError, TypeError):
                    pass

        # Handle numeric format (assume cm if > 10, meters if < 10)
        elif isinstance(value, (int, float)):
            if value > 10:
                return int(value / 2.54)
            else:
                return int(value * 39.3701)

        return None

    @staticmethod
    def parse_weight_to_lbs(value: Any) -> Optional[int]:
        """Parse weight from various formats to pounds.

        Supports:
        - Pounds: 220, "220", "220 lbs"
        - Kilograms: "75 kg", dict with pounds/kilograms

        Args:
            value: Weight value in any supported format

        Returns:
            Weight in pounds, or None if parsing fails
        """
        if not value:
            return None

        # Handle dict format (NBA/NFL API style)
        if isinstance(value, dict):
            # Prefer pounds if available
            if "pounds" in value and value["pounds"]:
                try:
                    return int(float(value["pounds"]))
                except (ValueError, TypeError):
                    pass

            # Convert from kilograms
            if "kilograms" in value and value["kilograms"]:
                try:
                    kg = float(value["kilograms"])
                    return int(kg * 2.205)
                except (ValueError, TypeError):
                    pass

        # Handle string formats
        elif isinstance(value, str):
            # Extract numeric portion: "220 lbs" -> 220, "75 kg" -> 75
            match = re.match(r"(\d+(?:\.\d+)?)\s*(?:(lbs?|kg))?", value.lower())
            if match:
                try:
                    num = float(match.group(1))
                    unit = match.group(2)

                    # Convert from kg if specified
                    if unit and unit.startswith("kg"):
                        return int(num * 2.205)

                    # Otherwise assume lbs
                    return int(num)
                except (ValueError, TypeError):
                    pass

        # Handle numeric format (assume lbs)
        elif isinstance(value, (int, float)):
            return int(value)

        return None

    @staticmethod
    def safe_percentage(made: int, attempted: int, decimals: int = 1) -> float:
        """Calculate percentage safely, returning 0 if attempted is 0.

        Args:
            made: Number of successful attempts
            attempted: Total number of attempts
            decimals: Number of decimal places (default: 1)

        Returns:
            Percentage as float, or 0.0 if attempted is 0
        """
        if not attempted or attempted == 0:
            return 0.0
        return round((made / attempted) * 100, decimals)

    @staticmethod
    def safe_int(value: Any, default: int = 0) -> int:
        """Safely convert value to int, with fallback.

        Handles None, strings with commas, etc.
        """
        if value is None:
            return default

        if isinstance(value, (int, float)):
            return int(value)

        if isinstance(value, str):
            try:
                # Remove commas and convert
                cleaned = value.replace(",", "").strip()
                return int(float(cleaned))
            except (ValueError, TypeError):
                return default

        return default

    @staticmethod
    def safe_float(value: Any, default: float = 0.0) -> float:
        """Safely convert value to float, with fallback.

        Handles None, strings with commas, etc.
        """
        if value is None:
            return default

        if isinstance(value, (int, float)):
            return float(value)

        if isinstance(value, str):
            try:
                # Remove commas and convert
                cleaned = value.replace(",", "").strip()
                return float(cleaned)
            except (ValueError, TypeError):
                return default

        return default


class NameBuilder:
    """Utilities for building player/team names from various API formats."""

    @staticmethod
    def build_full_name(data: dict) -> str:
        """Build full name from first/last name or use 'name' field.

        Handles various API formats:
        - NFL: uses 'name' field directly
        - NBA: uses 'firstname' + 'lastname'
        - Football: uses 'firstname' + 'lastname' or 'name'

        Args:
            data: Player data dict with name fields

        Returns:
            Full name string, or "Unknown" if no name found
        """
        # Try direct name field first (NFL style)
        if data.get("name"):
            return data["name"]

        # Build from first + last
        first = (
            data.get("first_name") or
            data.get("firstname") or
            data.get("firstName") or
            ""
        )
        last = (
            data.get("last_name") or
            data.get("lastname") or
            data.get("lastName") or
            ""
        )

        full_name = f"{first} {last}".strip()
        return full_name if full_name else "Unknown"


class StatCalculators:
    """Advanced statistics calculators used across multiple sports."""

    @staticmethod
    def calculate_per_game_avg(total: float, games: int, decimals: int = 1) -> float:
        """Calculate per-game average safely.

        Args:
            total: Total stat value
            games: Number of games played
            decimals: Decimal places for rounding

        Returns:
            Per-game average, or 0.0 if games is 0
        """
        if not games or games == 0:
            return 0.0
        return round(total / games, decimals)

    @staticmethod
    def calculate_per_90_avg(total: float, minutes: int, decimals: int = 2) -> float:
        """Calculate per-90-minute average (football/soccer stat).

        Args:
            total: Total stat value
            minutes: Total minutes played
            decimals: Decimal places for rounding

        Returns:
            Per-90 average, or 0.0 if insufficient minutes
        """
        if not minutes or minutes < 90:
            return 0.0
        return round((total / minutes) * 90, decimals)

    @staticmethod
    def calculate_nba_efficiency(
        points: int,
        rebounds: int,
        assists: int,
        steals: int,
        blocks: int,
        fgm: int,
        fga: int,
        ftm: int,
        fta: int,
        turnovers: int,
        games: int
    ) -> float:
        """Calculate NBA efficiency rating.

        Formula: (PTS + REB + AST + STL + BLK - Missed FG - Missed FT - TO) / Games
        """
        if games == 0:
            return 0.0

        efficiency = (
            points + rebounds + assists + steals + blocks -
            (fga - fgm) - (fta - ftm) - turnovers
        )
        return round(efficiency / games, 1)

    @staticmethod
    def calculate_true_shooting_pct(points: int, fga: int, fta: int) -> float:
        """Calculate true shooting percentage.

        Formula: PTS / (2 * (FGA + 0.44 * FTA)) * 100
        """
        if not points or (not fga and not fta):
            return 0.0

        tsa = fga + (0.44 * fta)
        if tsa == 0:
            return 0.0

        return round((points / (2 * tsa)) * 100, 1)

    @staticmethod
    def calculate_effective_fg_pct(fgm: int, tpm: int, fga: int) -> float:
        """Calculate effective field goal percentage.

        Formula: (FGM + 0.5 * 3PM) / FGA * 100
        Accounts for 3-pointers being worth more.
        """
        if not fga:
            return 0.0

        return round(((fgm + 0.5 * tpm) / fga) * 100, 1)


class PositionMappers:
    """Map positions to position groups across different sports."""

    @staticmethod
    def get_nba_position_group(position: Optional[str]) -> Optional[str]:
        """Map NBA position to position group.

        Guards: PG, SG, G, G-F
        Forwards: SF, PF, F, F-C, F-G
        Centers: C, C-F
        """
        if not position:
            return None

        position = position.upper()

        if position in {"PG", "SG", "G", "G-F"}:
            return "Guard"
        elif position in {"SF", "PF", "F", "F-C", "F-G"}:
            return "Forward"
        elif position in {"C", "C-F"}:
            return "Center"

        return None

    @staticmethod
    def get_nfl_position_group(position: Optional[str]) -> Optional[str]:
        """Map NFL position to position group.

        Groups:
        - Offense - Skill: QB, RB, FB, WR, TE
        - Offense - Line: OL, OT, OG, C, T, G
        - Defense - Line: DL, DE, DT, NT
        - Defense - Linebacker: LB, ILB, OLB, MLB
        - Defense - Secondary: DB, CB, S, FS, SS
        - Special Teams: K, P, LS
        """
        if not position:
            return None

        position = position.upper()

        if position in {"QB", "RB", "FB", "WR", "TE"}:
            return "Offense - Skill"
        elif position in {"OL", "OT", "OG", "C", "T", "G"}:
            return "Offense - Line"
        elif position in {"DL", "DE", "DT", "NT"}:
            return "Defense - Line"
        elif position in {"LB", "ILB", "OLB", "MLB"}:
            return "Defense - Linebacker"
        elif position in {"DB", "CB", "S", "FS", "SS"}:
            return "Defense - Secondary"
        elif position in {"K", "P", "LS"}:
            return "Special Teams"

        return None

    @staticmethod
    def get_football_position_group(position: Optional[str]) -> Optional[str]:
        """Map Football (soccer) position to position group.

        Groups:
        - Goalkeeper: GK
        - Defender: CB, LB, RB, WB
        - Midfielder: CM, DM, AM, LM, RM
        - Forward: CF, LW, RW, ST
        """
        if not position:
            return None

        position_lower = position.lower()

        if "goalkeeper" in position_lower or position == "GK":
            return "Goalkeeper"
        elif "defender" in position_lower or position in ("CB", "LB", "RB", "WB"):
            return "Defender"
        elif "midfielder" in position_lower or position in ("CM", "DM", "AM", "LM", "RM"):
            return "Midfielder"
        elif "attacker" in position_lower or "forward" in position_lower or position in ("CF", "LW", "RW", "ST"):
            return "Forward"

        return None
