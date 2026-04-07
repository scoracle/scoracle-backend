"""Canonical data models for seeding.

These dataclasses mirror the Go canonical structs in provider/canonical.go.
They are the contract between provider handlers and the upsert layer.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class Team:
    id: int
    name: str
    short_code: str | None = None
    city: str | None = None
    country: str | None = None
    conference: str | None = None
    division: str | None = None
    logo_url: str | None = None
    venue_name: str | None = None
    venue_capacity: int | None = None
    founded: int | None = None
    meta: dict[str, Any] = field(default_factory=dict)


@dataclass
class Player:
    id: int
    name: str
    first_name: str | None = None
    last_name: str | None = None
    position: str | None = None
    detailed_position: str | None = None
    nationality: str | None = None
    height: str | None = None
    weight: str | None = None
    date_of_birth: str | None = None
    photo_url: str | None = None
    team_id: int | None = None
    meta: dict[str, Any] = field(default_factory=dict)


@dataclass
class PlayerStats:
    player_id: int
    team_id: int | None = None
    player: Player | None = None
    stats: dict[str, Any] = field(default_factory=dict)
    raw: dict[str, Any] | None = None


@dataclass
class TeamStats:
    team_id: int
    team: Team | None = None
    stats: dict[str, Any] = field(default_factory=dict)
    raw: dict[str, Any] | None = None


@dataclass
class EventBoxScore:
    """One player's stat line for one fixture."""

    fixture_id: int
    player_id: int
    team_id: int
    player: Player | None = None
    minutes_played: float | None = None
    stats: dict[str, Any] = field(default_factory=dict)
    raw: dict[str, Any] | None = None


@dataclass
class EventTeamStats:
    """One team's stat line for one fixture."""

    fixture_id: int
    team_id: int
    score: int | None = None
    team: Team | None = None
    stats: dict[str, Any] = field(default_factory=dict)
    raw: dict[str, Any] | None = None


@dataclass
class SeedResult:
    """Accumulator for seed operation counts and errors."""

    teams_upserted: int = 0
    players_upserted: int = 0
    player_stats_upserted: int = 0
    team_stats_upserted: int = 0
    event_box_scores_upserted: int = 0
    event_team_stats_upserted: int = 0
    errors: list[str] = field(default_factory=list)

    def add(self, other: SeedResult) -> None:
        self.teams_upserted += other.teams_upserted
        self.players_upserted += other.players_upserted
        self.player_stats_upserted += other.player_stats_upserted
        self.team_stats_upserted += other.team_stats_upserted
        self.event_box_scores_upserted += other.event_box_scores_upserted
        self.event_team_stats_upserted += other.event_team_stats_upserted
        self.errors.extend(other.errors)

    def add_error(self, msg: str) -> None:
        self.errors.append(msg)

    def summary(self) -> str:
        return (
            f"teams={self.teams_upserted} players={self.players_upserted} "
            f"player_stats={self.player_stats_upserted} "
            f"team_stats={self.team_stats_upserted} "
            f"event_box_scores={self.event_box_scores_upserted} "
            f"event_team_stats={self.event_team_stats_upserted} "
            f"errors={len(self.errors)}"
        )
