"""
Pydantic models for stats database entities.

These models document the shape of data in the unified schema.
The API returns raw dicts from Postgres views/functions — the contract
is enforced by integration tests (tests/test_api_contract.py) rather
than Pydantic response_model serialization, since the Go API will
query the same views and produce identical JSON without Pydantic.
"""

from __future__ import annotations

from typing import Any, Optional

from pydantic import BaseModel


# =============================================================================
# Core Entity Models — match unified table schemas
# =============================================================================


class TeamModel(BaseModel):
    """Team master record (unified teams table)."""

    id: int
    sport: str
    league_id: Optional[int] = None
    name: str
    short_code: Optional[str] = None
    logo_url: Optional[str] = None
    country: Optional[str] = None
    city: Optional[str] = None
    founded: Optional[int] = None
    venue_name: Optional[str] = None
    venue_capacity: Optional[int] = None
    meta: Optional[dict[str, Any]] = None


class PlayerModel(BaseModel):
    """Player master record (unified players table)."""

    id: int
    sport: str
    first_name: Optional[str] = None
    last_name: Optional[str] = None
    name: str
    position: Optional[str] = None
    team_id: Optional[int] = None
    league_id: Optional[int] = None
    meta: Optional[dict[str, Any]] = None


# =============================================================================
# Status Constants
# =============================================================================


class ProfileStatus:
    """Status constants for entity profiles."""

    COMPLETE = "complete"  # Full data available
    BUILDING = "building"  # Non-priority league, minimal data


class EntityMinimal(BaseModel):
    """Minimal entity data for autocomplete."""

    id: int
    entity_type: str  # "team" or "player"
    sport: str
    league_id: Optional[int] = None
    name: str
