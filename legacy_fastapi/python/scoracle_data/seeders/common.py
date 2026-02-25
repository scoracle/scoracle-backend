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


@dataclass
class BatchSeedResult:
    """Aggregated result across a multi-season batch seed run."""

    seed_result: SeedResult = field(default_factory=SeedResult)
    seasons_completed: list[str] = field(default_factory=list)  # e.g. ["NBA/2023", "FOOTBALL/2024/PL"]
    percentiles_computed: list[str] = field(default_factory=list)  # e.g. ["NBA/2023", "FOOTBALL/2024"]
    provider_seasons_discovered: int = 0
    total_duration_seconds: float = 0.0

    def add_seed(self, result: SeedResult, label: str) -> None:
        """Accumulate a seed result and record the completed label."""
        self.seed_result = self.seed_result + result
        self.seasons_completed.append(label)

    def add_percentile(self, label: str) -> None:
        """Record a completed percentile calculation."""
        self.percentiles_computed.append(label)
