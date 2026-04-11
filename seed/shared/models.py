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
