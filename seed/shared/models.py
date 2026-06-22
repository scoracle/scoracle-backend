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
    league_id: int | None = None
    search_aliases: list[str] = field(default_factory=list)
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
    search_aliases: list[str] = field(default_factory=list)
    meta: dict[str, Any] = field(default_factory=dict)
    raw: dict[str, Any] | None = None


@dataclass
class PlayerStats:
    player_id: int
    team_id: int | None = None
    # Provider-raw position string. Owned by the stats pipeline — partitions
    # percentile cohorts. Never touches public.players.
    position: str | None = None
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
    # Per-game position from the provider's stat embed (raw string).
    # finalize_fixture aggregates these into player_stats.position.
    position: str | None = None
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
class BoxScoreResult:
    """One fixture's full box score as returned by a provider handler.

    `provider_status` is the raw provider finality label (e.g. BallDontLie
    "Final", SportMonks "FT"/"NS"); None when the payload carried none. The
    completeness gate (services/event/completeness.py) interprets it per sport
    — handlers stay thin and do not decide finality themselves.
    """

    players: list[EventBoxScore] = field(default_factory=list)
    teams: list[EventTeamStats] = field(default_factory=list)
    provider_status: str | None = None
