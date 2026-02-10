"""
Shared seeder types used by all sport-specific seeders.
"""

from dataclasses import dataclass, field


@dataclass
class SeedResult:
    """Result of a seeding operation."""

    teams_upserted: int = 0
    players_upserted: int = 0
    player_stats_upserted: int = 0
    team_stats_upserted: int = 0
    errors: list[str] = field(default_factory=list)

    def __add__(self, other: "SeedResult") -> "SeedResult":
        return SeedResult(
            teams_upserted=self.teams_upserted + other.teams_upserted,
            players_upserted=self.players_upserted + other.players_upserted,
            player_stats_upserted=self.player_stats_upserted
            + other.player_stats_upserted,
            team_stats_upserted=self.team_stats_upserted + other.team_stats_upserted,
            errors=self.errors + other.errors,
        )
