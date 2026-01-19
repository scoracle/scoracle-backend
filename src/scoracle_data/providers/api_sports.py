"""
API-Sports.com data provider implementation.

Handles all API-Sports specific logic including:
- API communication
- Field mapping using YAML config
- Response normalization
- Raw response preservation for JSONB storage
"""

from __future__ import annotations

import logging
import os
from datetime import datetime
from typing import Any

import httpx

from .base import (
    DataProviderProtocol,
    RawEntityData,
    ProviderError,
    RateLimitError,
    AuthenticationError,
)
from ..sport_configs import get_config, SportConfig

logger = logging.getLogger(__name__)


class ApiSportsProvider(DataProviderProtocol):
    """
    API-Sports.com data provider.
    
    Implements the DataProviderProtocol for API-Sports.com services.
    Uses YAML configuration for field mapping to ensure provider-agnostic
    canonical data output.
    """
    
    provider_name = "api_sports"
    
    # Base URLs for each sport
    BASE_URLS = {
        "NBA": "https://v2.nba.api-sports.io",
        "NFL": "https://v1.american-football.api-sports.io",
        "FOOTBALL": "https://v3.football.api-sports.io",
    }
    
    def __init__(
        self,
        api_key: str | None = None,
        timeout: float = 30.0,
    ):
        """
        Initialize the API-Sports provider.
        
        Args:
            api_key: API key (uses API_SPORTS_KEY env var if not provided)
            timeout: Request timeout in seconds
        """
        self.api_key = api_key or os.getenv("API_SPORTS_KEY")
        self.timeout = timeout
        self.config = get_config()
        self._client: httpx.AsyncClient | None = None
        
        if not self.api_key:
            logger.warning("API_SPORTS_KEY not set - API calls will fail")
    
    # ==========================================================================
    # HTTP Client Management
    # ==========================================================================
    
    async def _get_client(self) -> httpx.AsyncClient:
        """Get or create HTTP client."""
        if self._client is None or self._client.is_closed:
            self._client = httpx.AsyncClient(timeout=self.timeout)
        return self._client
    
    async def close(self) -> None:
        """Close HTTP client."""
        if self._client and not self._client.is_closed:
            await self._client.aclose()
            self._client = None
    
    def _get_base_url(self, sport: str) -> str:
        """Get base URL for sport."""
        sport_upper = sport.upper()
        if sport_upper not in self.BASE_URLS:
            raise ProviderError(f"Unsupported sport: {sport}")
        return self.BASE_URLS[sport_upper]
    
    def _headers(self) -> dict[str, str]:
        """Get request headers."""
        return {"x-apisports-key": self.api_key or ""}
    
    async def _request(
        self,
        sport: str,
        endpoint: str,
        params: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """
        Make an API request.
        
        Args:
            sport: Sport identifier
            endpoint: API endpoint (e.g., "teams", "players")
            params: Query parameters
            
        Returns:
            API response as dict
            
        Raises:
            ProviderError: On API error
            RateLimitError: On rate limit exceeded
            AuthenticationError: On auth failure
        """
        base_url = self._get_base_url(sport)
        client = await self._get_client()
        
        try:
            response = await client.get(
                f"{base_url}/{endpoint}",
                headers=self._headers(),
                params=params,
            )
            
            # Handle rate limiting
            if response.status_code == 429:
                retry_after = response.headers.get("Retry-After")
                raise RateLimitError(
                    "API rate limit exceeded",
                    retry_after=int(retry_after) if retry_after else None,
                )
            
            # Handle auth errors
            if response.status_code in (401, 403):
                raise AuthenticationError("API authentication failed")
            
            response.raise_for_status()
            return response.json()
            
        except httpx.HTTPStatusError as e:
            raise ProviderError(f"API request failed: {e}")
        except httpx.RequestError as e:
            raise ProviderError(f"API request error: {e}")
    
    # ==========================================================================
    # Field Mapping
    # ==========================================================================
    
    def _get_nested(self, data: dict, path: str) -> Any:
        """
        Get nested value using dot notation.
        
        Args:
            data: Source dict
            path: Dot-notation path (e.g., "birth.date", "leagues.standard.pos")
            
        Returns:
            Value at path, or None if not found
        """
        if not path or path.startswith("_"):
            return None
            
        parts = path.split(".")
        value = data
        
        for part in parts:
            if isinstance(value, dict):
                value = value.get(part)
            else:
                return None
                
        return value
    
    def _map_fields(
        self,
        raw: dict,
        mapping: dict[str, Any],
        context: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """
        Map raw API fields to canonical fields using config mapping.
        
        Args:
            raw: Raw API response dict
            mapping: Field mapping from YAML config
            context: Optional context data (e.g., team_id)
            
        Returns:
            Dict with canonical field names and values
        """
        result = {}
        context = context or {}
        
        for canonical_name, source in mapping.items():
            # Skip special fields (prefixed with _)
            if canonical_name.startswith("_"):
                continue
            
            value = None
            
            if isinstance(source, list):
                # Try multiple source paths, use first non-null
                for path in source:
                    value = self._get_nested(raw, path)
                    if value is not None:
                        break
            elif isinstance(source, str):
                if source.startswith("_context."):
                    # Get from context
                    context_key = source.replace("_context.", "")
                    value = context.get(context_key)
                else:
                    value = self._get_nested(raw, source)
            
            if value is not None:
                result[canonical_name] = value
        
        return result
    
    # ==========================================================================
    # Team Operations
    # ==========================================================================
    
    async def fetch_teams(
        self,
        sport: str,
        season: int,
        *,
        league_id: int | None = None,
    ) -> list[RawEntityData]:
        """Fetch teams for discovery phase."""
        sport_upper = sport.upper()
        sport_config = self.config.get_sport(sport_upper)
        mapping = sport_config.get_provider_mapping(self.provider_name, "team_discovery")
        filters = sport_config.get_filters().get("teams", {})
        
        # Build params
        params: dict[str, Any] = {}
        if sport_upper == "NBA":
            params["league"] = "standard"
        elif sport_upper == "FOOTBALL" and league_id:
            params["league"] = league_id
            params["season"] = season
        elif sport_upper == "NFL":
            params["league"] = league_id or 1
            params["season"] = season
        
        # Make request
        response = await self._request(sport_upper, "teams", params or None)
        raw_teams = response.get("response", [])
        
        results = []
        for raw in raw_teams:
            # Handle nested team structure (FOOTBALL wraps in team/venue)
            team_data = raw.get("team", raw) if isinstance(raw, dict) else raw
            
            # Apply filters (e.g., nbaFranchise=True)
            skip = False
            for filter_field, filter_value in filters.items():
                actual_value = team_data.get(filter_field)
                if actual_value is not None and actual_value != filter_value:
                    skip = True
                    break
            
            if skip:
                continue
            
            # Map fields
            canonical = self._map_fields(team_data, mapping)
            
            # Ensure ID is present
            if "id" not in canonical:
                canonical["id"] = team_data.get("id")
            
            results.append(RawEntityData(
                entity_type="team",
                provider_id=str(team_data.get("id")),
                canonical_data=canonical,
                raw_response=raw,  # Store full original response
                provider_name=self.provider_name,
            ))
        
        logger.info(f"Fetched {len(results)} teams for {sport_upper}")
        return results
    
    async def fetch_team_profile(
        self,
        sport: str,
        team_id: int,
    ) -> RawEntityData | None:
        """Fetch complete team profile."""
        sport_upper = sport.upper()
        sport_config = self.config.get_sport(sport_upper)
        mapping = sport_config.get_provider_mapping(self.provider_name, "team_profile")
        
        response = await self._request(sport_upper, "teams", {"id": team_id})
        rows = response.get("response", [])
        
        if not rows:
            return None
        
        raw = rows[0]
        
        # Handle FOOTBALL's nested structure
        if sport_upper == "FOOTBALL":
            team_data = raw.get("team", raw)
            venue_data = raw.get("venue", {})
            # Merge venue into team for mapping
            merged = {**team_data}
            if venue_data:
                merged["venue"] = venue_data
        else:
            merged = raw
        
        canonical = self._map_fields(merged, mapping)
        canonical["id"] = team_id
        
        return RawEntityData(
            entity_type="team",
            provider_id=str(team_id),
            canonical_data=canonical,
            raw_response=raw,
            provider_name=self.provider_name,
        )
    
    async def fetch_team_stats(
        self,
        sport: str,
        team_id: int,
        season: int,
        *,
        league_id: int | None = None,
    ) -> RawEntityData | None:
        """Fetch team statistics for a season."""
        sport_upper = sport.upper()
        sport_config = self.config.get_sport(sport_upper)
        mapping = sport_config.get_provider_mapping(self.provider_name, "team_stats")
        
        if sport_upper == "NBA":
            # NBA: Get from standings
            response = await self._request(
                sport_upper, 
                "standings", 
                {"season": season, "league": "standard"}
            )
            standings = response.get("response", [])
            
            # Find this team
            raw = None
            for s in standings:
                if s.get("team", {}).get("id") == team_id:
                    raw = s
                    break
            
            if not raw:
                return None
            
            # Also try to get team statistics for PPG
            try:
                stats_response = await self._request(
                    sport_upper,
                    "teams/statistics",
                    {"id": team_id, "season": season}
                )
                team_stats = stats_response.get("response")
                if team_stats:
                    raw["_team_stats"] = team_stats
            except Exception:
                pass  # PPG is optional
                
        elif sport_upper == "NFL":
            # NFL: Also from standings
            response = await self._request(
                sport_upper,
                "standings",
                {"season": season, "league": league_id or 1, "team": team_id}
            )
            rows = response.get("response", [])
            raw = rows[0] if rows else None
            
        elif sport_upper == "FOOTBALL":
            # Football: standings endpoint
            response = await self._request(
                sport_upper,
                "standings",
                {"season": season, "league": league_id, "team": team_id}
            )
            rows = response.get("response", [])
            if rows and isinstance(rows[0], dict):
                league_data = rows[0].get("league", {})
                standings = league_data.get("standings", [[]])
                if standings and standings[0]:
                    raw = standings[0][0]  # First team in first group
                else:
                    raw = None
            else:
                raw = None
        else:
            return None
        
        if not raw:
            return None
        
        canonical = self._map_fields(raw, mapping)
        canonical["team_id"] = team_id
        
        return RawEntityData(
            entity_type="team_stats",
            provider_id=str(team_id),
            canonical_data=canonical,
            raw_response=raw,
            provider_name=self.provider_name,
        )
    
    # ==========================================================================
    # Player Operations
    # ==========================================================================
    
    async def fetch_players(
        self,
        sport: str,
        season: int,
        *,
        team_id: int | None = None,
        league_id: int | None = None,
    ) -> list[RawEntityData]:
        """Fetch players for discovery phase."""
        sport_upper = sport.upper()
        sport_config = self.config.get_sport(sport_upper)
        mapping = sport_config.get_provider_mapping(self.provider_name, "player_discovery")
        
        results = []
        seen_ids: set[int] = set()
        
        if sport_upper == "NBA":
            # NBA: Fetch players by team
            if team_id:
                team_ids = [team_id]
            else:
                # Get all teams first
                teams = await self.fetch_teams(sport_upper, season)
                team_ids = [int(t.provider_id) for t in teams]
            
            for tid in team_ids:
                try:
                    response = await self._request(
                        sport_upper,
                        "players",
                        {"team": tid, "season": season}
                    )
                    players = response.get("response", [])
                    
                    for raw in players:
                        player_id = raw.get("id")
                        if not player_id or player_id in seen_ids:
                            continue
                        
                        seen_ids.add(player_id)
                        
                        # Map with team context
                        canonical = self._map_fields(
                            raw, 
                            mapping, 
                            context={"team_id": tid}
                        )
                        canonical["id"] = player_id
                        canonical["current_team_id"] = tid
                        
                        results.append(RawEntityData(
                            entity_type="player",
                            provider_id=str(player_id),
                            canonical_data=canonical,
                            raw_response=raw,
                            provider_name=self.provider_name,
                            context={"team_id": tid},
                        ))
                        
                except Exception as e:
                    logger.warning(f"Failed to fetch players for team {tid}: {e}")
        
        elif sport_upper == "NFL":
            # NFL: Similar team-by-team approach
            if team_id:
                team_ids = [team_id]
            else:
                teams = await self.fetch_teams(sport_upper, season, league_id=league_id)
                team_ids = [int(t.provider_id) for t in teams]
            
            for tid in team_ids:
                try:
                    response = await self._request(
                        sport_upper,
                        "players",
                        {"team": tid, "season": season}
                    )
                    players = response.get("response", [])
                    
                    for raw in players:
                        player_id = raw.get("id")
                        if not player_id or player_id in seen_ids:
                            continue
                        
                        seen_ids.add(player_id)
                        canonical = self._map_fields(raw, mapping, context={"team_id": tid})
                        canonical["id"] = player_id
                        canonical["current_team_id"] = tid
                        
                        results.append(RawEntityData(
                            entity_type="player",
                            provider_id=str(player_id),
                            canonical_data=canonical,
                            raw_response=raw,
                            provider_name=self.provider_name,
                            context={"team_id": tid},
                        ))
                        
                except Exception as e:
                    logger.warning(f"Failed to fetch NFL players for team {tid}: {e}")
        
        elif sport_upper == "FOOTBALL":
            # Football: Paginated by league
            if not league_id:
                raise ProviderError("league_id required for FOOTBALL players")
            
            page = 1
            while True:
                response = await self._request(
                    sport_upper,
                    "players",
                    {"league": league_id, "season": season, "page": page}
                )
                
                players = response.get("response", [])
                paging = response.get("paging", {})
                
                for item in players:
                    raw = item.get("player", item)
                    player_id = raw.get("id")
                    
                    if not player_id or player_id in seen_ids:
                        continue
                    
                    seen_ids.add(player_id)
                    
                    # Get team from statistics if available
                    stats = item.get("statistics", [])
                    player_team_id = None
                    if stats and isinstance(stats[0], dict):
                        team_info = stats[0].get("team", {})
                        player_team_id = team_info.get("id")
                    
                    canonical = self._map_fields(raw, mapping)
                    canonical["id"] = player_id
                    if player_team_id:
                        canonical["current_team_id"] = player_team_id
                        canonical["current_league_id"] = league_id
                    
                    results.append(RawEntityData(
                        entity_type="player",
                        provider_id=str(player_id),
                        canonical_data=canonical,
                        raw_response=item,  # Store full item with stats
                        provider_name=self.provider_name,
                    ))
                
                # Check pagination
                current = paging.get("current", 1)
                total = paging.get("total", 1)
                if current >= total:
                    break
                page += 1
        
        logger.info(f"Fetched {len(results)} players for {sport_upper}")
        return results
    
    async def fetch_player_profile(
        self,
        sport: str,
        player_id: int,
    ) -> RawEntityData | None:
        """Fetch complete player profile."""
        sport_upper = sport.upper()
        sport_config = self.config.get_sport(sport_upper)
        mapping = sport_config.get_provider_mapping(self.provider_name, "player_profile")
        
        if sport_upper == "FOOTBALL":
            # Football uses different endpoint
            response = await self._request(
                sport_upper,
                "players/profiles",
                {"player": player_id}
            )
        else:
            response = await self._request(
                sport_upper,
                "players",
                {"id": player_id}
            )
        
        rows = response.get("response", [])
        if not rows:
            return None
        
        raw = rows[0]
        
        # Handle FOOTBALL nested structure
        if sport_upper == "FOOTBALL":
            player_data = raw.get("player", raw)
        else:
            player_data = raw
        
        canonical = self._map_fields(player_data, mapping)
        canonical["id"] = player_id
        
        return RawEntityData(
            entity_type="player",
            provider_id=str(player_id),
            canonical_data=canonical,
            raw_response=raw,
            provider_name=self.provider_name,
        )
    
    async def fetch_player_stats(
        self,
        sport: str,
        player_id: int,
        season: int,
        *,
        league_id: int | None = None,
    ) -> RawEntityData | None:
        """Fetch player statistics for a season."""
        sport_upper = sport.upper()
        sport_config = self.config.get_sport(sport_upper)
        mapping = sport_config.get_provider_mapping(self.provider_name, "player_stats")
        
        if sport_upper == "NBA":
            # NBA: Aggregate from game logs
            response = await self._request(
                sport_upper,
                "players/statistics",
                {"id": player_id, "season": season}
            )
            games = response.get("response", [])
            
            if not games:
                return None
            
            # Aggregate game stats
            canonical = self._aggregate_nba_stats(games, mapping)
            canonical["player_id"] = player_id
            
            # Get team from first game
            if games and "team" in games[0]:
                canonical["team_id"] = games[0]["team"].get("id")
            
            return RawEntityData(
                entity_type="player_stats",
                provider_id=str(player_id),
                canonical_data=canonical,
                raw_response={"games": games},  # Store all games
                provider_name=self.provider_name,
            )
        
        elif sport_upper == "NFL":
            # NFL: Get from player statistics endpoint
            response = await self._request(
                sport_upper,
                "players/statistics",
                {"id": player_id, "season": season}
            )
            rows = response.get("response", [])
            
            if not rows:
                return None
            
            raw = rows[0]
            canonical = self._map_fields(raw, mapping)
            canonical["player_id"] = player_id
            
            return RawEntityData(
                entity_type="player_stats",
                provider_id=str(player_id),
                canonical_data=canonical,
                raw_response=raw,
                provider_name=self.provider_name,
            )
        
        elif sport_upper == "FOOTBALL":
            # Football: Get from players endpoint with season
            if not league_id:
                raise ProviderError("league_id required for FOOTBALL stats")
            
            response = await self._request(
                sport_upper,
                "players",
                {"id": player_id, "season": season, "league": league_id}
            )
            rows = response.get("response", [])
            
            if not rows:
                return None
            
            raw = rows[0]
            stats = raw.get("statistics", [])
            
            if stats:
                # Use first stats entry
                stat_data = stats[0]
                canonical = self._map_fields(stat_data, mapping)
            else:
                canonical = {}
            
            canonical["player_id"] = player_id
            canonical["league_id"] = league_id
            
            return RawEntityData(
                entity_type="player_stats",
                provider_id=str(player_id),
                canonical_data=canonical,
                raw_response=raw,
                provider_name=self.provider_name,
            )
        
        return None
    
    # ==========================================================================
    # NBA-specific Aggregation
    # ==========================================================================
    
    def _aggregate_nba_stats(
        self,
        games: list[dict],
        mapping: dict[str, str],
    ) -> dict[str, Any]:
        """
        Aggregate NBA game logs into season totals.
        
        Args:
            games: List of game stat dicts
            mapping: Field mapping from config
            
        Returns:
            Aggregated stats dict
        """
        if not games:
            return {"games_played": 0}
        
        games_played = len(games)
        
        # Initialize accumulators
        totals: dict[str, int | float] = {}
        
        for game in games:
            for canonical_name, source_field in mapping.items():
                if canonical_name.startswith("_"):
                    continue
                
                raw_value = game.get(source_field, 0)
                
                # Handle string values (e.g., plusMinus: "+5")
                if isinstance(raw_value, str):
                    raw_value = raw_value.replace("+", "")
                    try:
                        raw_value = int(raw_value)
                    except ValueError:
                        try:
                            raw_value = float(raw_value)
                        except ValueError:
                            raw_value = 0
                
                # Handle minutes (can be "25:30" format)
                if source_field == "min" and isinstance(raw_value, str) and ":" in str(raw_value):
                    parts = str(raw_value).split(":")
                    try:
                        raw_value = int(parts[0])
                    except ValueError:
                        raw_value = 0
                
                if raw_value is None:
                    raw_value = 0
                
                totals[canonical_name] = totals.get(canonical_name, 0) + (raw_value or 0)
        
        # Build result with totals and per-game averages
        result = {
            "games_played": games_played,
        }
        
        gp = max(games_played, 1)
        
        for field, total in totals.items():
            # Store total
            total_field = f"{field}_total" if not field.endswith("_total") else field
            result[field] = total
            
            # Calculate per-game average for key stats
            if field in ("minutes", "points"):
                result[f"{field}_per_game"] = round(total / gp, 1)
        
        # Calculate total rebounds from components
        off_reb = totals.get("offensive_rebounds", 0)
        def_reb = totals.get("defensive_rebounds", 0)
        result["total_rebounds"] = off_reb + def_reb
        result["rebounds_per_game"] = round((off_reb + def_reb) / gp, 1)
        
        # Per-game stats
        for field in ("assists", "turnovers", "steals", "blocks", "personal_fouls"):
            if field in totals:
                result[f"{field}_per_game"] = round(totals[field] / gp, 1)
        
        # Percentages (calculated from totals, not averaged)
        fgm = totals.get("fgm", 0)
        fga = totals.get("fga", 0)
        result["fg_pct"] = round(fgm / fga * 100, 1) if fga > 0 else 0.0
        
        tpm = totals.get("tpm", 0)
        tpa = totals.get("tpa", 0)
        result["tp_pct"] = round(tpm / tpa * 100, 1) if tpa > 0 else 0.0
        
        ftm = totals.get("ftm", 0)
        fta = totals.get("fta", 0)
        result["ft_pct"] = round(ftm / fta * 100, 1) if fta > 0 else 0.0
        
        # Plus/minus per game
        pm = totals.get("plus_minus", 0)
        result["plus_minus_per_game"] = round(pm / gp, 1)
        
        # Rename minutes for consistency
        if "minutes" in result:
            result["minutes_total"] = result.pop("minutes")
            result["minutes_per_game"] = result.get("minutes_per_game", 0)
        
        # Rename points for consistency  
        if "points" in result:
            result["points_total"] = result.pop("points")
            result["points_per_game"] = result.get("points_per_game", 0)
        
        return result
    
    # ==========================================================================
    # Utility Methods
    # ==========================================================================
    
    async def health_check(self) -> bool:
        """Check if API is accessible."""
        try:
            # Use NBA status endpoint as health check
            await self._request("NBA", "status")
            return True
        except Exception:
            return False
    
    def get_rate_limit_info(self) -> dict[str, Any]:
        """Get rate limit info from last response."""
        # API-Sports includes rate limit in headers
        # Would need to track from last response
        return {
            "provider": self.provider_name,
            "note": "Rate limit info available in response headers",
        }
