"""NBA seeder - seeds NBA data from BallDontLie API."""

import logging
from typing import Any
from dataclasses import dataclass, field

import asyncpg

from .tables import (
    TEAM_PROFILE_TABLE,
    PLAYER_PROFILE_TABLE,
    PLAYER_STATS_TABLE,
    TEAM_STATS_TABLE,
)
from .client import BallDontLieNBA

logger = logging.getLogger(__name__)


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
            player_stats_upserted=self.player_stats_upserted + other.player_stats_upserted,
            team_stats_upserted=self.team_stats_upserted + other.team_stats_upserted,
            errors=self.errors + other.errors,
        )


class NBASeeder:
    """Seeder for NBA data from BallDontLie."""
    
    def __init__(self, db: asyncpg.Connection, client: BallDontLieNBA):
        self.db = db
        self.client = client
    
    # =========================================================================
    # Teams
    # =========================================================================
    
    async def seed_teams(self) -> SeedResult:
        """Seed all NBA teams."""
        logger.info("Seeding NBA teams...")
        result = SeedResult()
        
        try:
            teams = await self.client.get_teams()
            logger.info(f"Fetched {len(teams)} teams from API")
            
            for team in teams:
                await self._upsert_team(team)
                result.teams_upserted += 1
            
            logger.info(f"Upserted {result.teams_upserted} teams")
            
        except Exception as e:
            error_msg = f"Error seeding teams: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)
        
        return result
    
    async def _upsert_team(self, team: dict[str, Any]) -> None:
        """Upsert a single team."""
        await self.db.execute(f"""
            INSERT INTO {TEAM_PROFILE_TABLE} (
                id, name, full_name, abbreviation, city, conference, division
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                full_name = EXCLUDED.full_name,
                abbreviation = EXCLUDED.abbreviation,
                city = EXCLUDED.city,
                conference = EXCLUDED.conference,
                division = EXCLUDED.division,
                updated_at = NOW()
        """,
            team["id"],
            team.get("name"),
            team.get("full_name"),
            team.get("abbreviation"),
            team.get("city"),
            team.get("conference"),
            team.get("division"),
        )
    
    # =========================================================================
    # Players
    # =========================================================================
    
    async def seed_players(self) -> SeedResult:
        """Seed all NBA players."""
        logger.info("Seeding NBA players...")
        result = SeedResult()
        
        try:
            count = 0
            async for player in self.client.get_players():
                try:
                    await self._upsert_player(player)
                    result.players_upserted += 1
                    count += 1
                    if count % 100 == 0:
                        logger.info(f"Processed {count} players...")
                except Exception as e:
                    error_msg = f"Error upserting player {player.get('id')}: {e}"
                    logger.warning(error_msg)
                    result.errors.append(error_msg)
            
            logger.info(f"Upserted {result.players_upserted} players")
            
        except Exception as e:
            error_msg = f"Error seeding players: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)
        
        return result
    
    async def _upsert_player(self, player: dict[str, Any]) -> None:
        """Upsert a single player."""
        team = player.get("team") or {}
        team_id = team.get("id") if team else None
        
        await self.db.execute(f"""
            INSERT INTO {PLAYER_PROFILE_TABLE} (
                id, first_name, last_name, position, height, weight,
                jersey_number, college, country, draft_year, draft_round,
                draft_number, team_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (id) DO UPDATE SET
                first_name = EXCLUDED.first_name,
                last_name = EXCLUDED.last_name,
                position = EXCLUDED.position,
                height = EXCLUDED.height,
                weight = EXCLUDED.weight,
                jersey_number = EXCLUDED.jersey_number,
                college = EXCLUDED.college,
                country = EXCLUDED.country,
                draft_year = EXCLUDED.draft_year,
                draft_round = EXCLUDED.draft_round,
                draft_number = EXCLUDED.draft_number,
                team_id = EXCLUDED.team_id,
                updated_at = NOW()
        """,
            player["id"],
            player.get("first_name"),
            player.get("last_name"),
            player.get("position"),
            player.get("height"),
            player.get("weight"),
            player.get("jersey_number"),
            player.get("college"),
            player.get("country"),
            player.get("draft_year"),
            player.get("draft_round"),
            player.get("draft_number"),
            team_id,
        )
    
    # =========================================================================
    # Player Stats
    # =========================================================================
    
    async def seed_player_stats(self, season: int, season_type: str = "regular") -> SeedResult:
        """Seed player season stats."""
        logger.info(f"Seeding NBA player stats for {season} {season_type}...")
        result = SeedResult()
        
        try:
            count = 0
            async for stats in self.client.get_all_season_averages(season, season_type):
                try:
                    await self._upsert_player_stats(stats, season, season_type)
                    result.player_stats_upserted += 1
                    count += 1
                    if count % 50 == 0:
                        logger.info(f"Processed {count} player stats...")
                except Exception as e:
                    player = stats.get("player", {})
                    error_msg = f"Error upserting stats for player {player.get('id')}: {e}"
                    logger.warning(error_msg)
                    result.errors.append(error_msg)
            
            logger.info(f"Upserted {result.player_stats_upserted} player stats")
            
        except Exception as e:
            error_msg = f"Error seeding player stats: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)
        
        return result
    
    async def _upsert_player_stats(
        self,
        stats_data: dict[str, Any],
        season: int,
        season_type: str,
    ) -> None:
        """Upsert player season stats."""
        player = stats_data.get("player", {})
        player_id = player.get("id")
        
        if not player_id:
            logger.warning(f"No player_id in stats data: {stats_data}")
            return
        
        # First ensure the player exists
        exists = await self.db.fetchval(
            f"SELECT 1 FROM {PLAYER_PROFILE_TABLE} WHERE id = $1",
            player_id
        )
        
        if not exists:
            # Create a minimal player record
            await self._upsert_player(player)
        
        # Extract stats from the nested 'stats' object
        s = stats_data.get("stats", {})
        
        import json
        raw_json = json.dumps(stats_data)
        
        await self.db.execute(f"""
            INSERT INTO {PLAYER_STATS_TABLE} (
                player_id, season, season_type, games_played, minutes,
                pts, reb, ast, stl, blk,
                fg_pct, fg3_pct, ft_pct, fgm, fga,
                fg3m, fg3a, ftm, fta, oreb, dreb,
                turnover, pf, plus_minus, raw_json
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15,
                $16, $17, $18, $19, $20, $21,
                $22, $23, $24, $25
            )
            ON CONFLICT (player_id, season, season_type) DO UPDATE SET
                games_played = EXCLUDED.games_played,
                minutes = EXCLUDED.minutes,
                pts = EXCLUDED.pts,
                reb = EXCLUDED.reb,
                ast = EXCLUDED.ast,
                stl = EXCLUDED.stl,
                blk = EXCLUDED.blk,
                fg_pct = EXCLUDED.fg_pct,
                fg3_pct = EXCLUDED.fg3_pct,
                ft_pct = EXCLUDED.ft_pct,
                fgm = EXCLUDED.fgm,
                fga = EXCLUDED.fga,
                fg3m = EXCLUDED.fg3m,
                fg3a = EXCLUDED.fg3a,
                ftm = EXCLUDED.ftm,
                fta = EXCLUDED.fta,
                oreb = EXCLUDED.oreb,
                dreb = EXCLUDED.dreb,
                turnover = EXCLUDED.turnover,
                pf = EXCLUDED.pf,
                plus_minus = EXCLUDED.plus_minus,
                raw_json = EXCLUDED.raw_json,
                updated_at = NOW()
        """,
            player_id,
            season,
            season_type,
            s.get("gp"),
            s.get("min"),
            s.get("pts"),
            s.get("reb"),
            s.get("ast"),
            s.get("stl"),
            s.get("blk"),
            s.get("fg_pct"),
            s.get("fg3_pct"),
            s.get("ft_pct"),
            s.get("fgm"),
            s.get("fga"),
            s.get("fg3m"),
            s.get("fg3a"),
            s.get("ftm"),
            s.get("fta"),
            s.get("oreb"),
            s.get("dreb"),
            s.get("tov"),
            s.get("pf"),
            s.get("plus_minus"),
            raw_json,
        )
    
    # =========================================================================
    # Team Stats
    # =========================================================================
    
    async def seed_team_stats(self, season: int, season_type: str = "regular") -> SeedResult:
        """Seed team season stats."""
        logger.info(f"Seeding NBA team stats for {season} {season_type}...")
        result = SeedResult()
        
        try:
            stats_list = await self.client.get_team_season_averages(season, season_type)
            logger.info(f"Fetched {len(stats_list)} team stats")
            
            for stats in stats_list:
                try:
                    await self._upsert_team_stats(stats, season, season_type)
                    result.team_stats_upserted += 1
                except Exception as e:
                    team = stats.get("team", {})
                    error_msg = f"Error upserting team stats for {team.get('id')}: {e}"
                    logger.warning(error_msg)
                    result.errors.append(error_msg)
            
            logger.info(f"Upserted {result.team_stats_upserted} team stats")
            
        except Exception as e:
            error_msg = f"Error seeding team stats: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)
        
        return result
    
    async def _upsert_team_stats(
        self,
        stats_data: dict[str, Any],
        season: int,
        season_type: str,
    ) -> None:
        """Upsert team season stats."""
        team = stats_data.get("team", {})
        team_id = team.get("id")
        
        if not team_id:
            return
        
        s = stats_data.get("stats", {})
        
        import json
        raw_json = json.dumps(stats_data)
        
        await self.db.execute(f"""
            INSERT INTO {TEAM_STATS_TABLE} (
                team_id, season, season_type, wins, losses, games_played,
                pts, reb, ast, fg_pct, fg3_pct, ft_pct, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (team_id, season, season_type) DO UPDATE SET
                wins = EXCLUDED.wins,
                losses = EXCLUDED.losses,
                games_played = EXCLUDED.games_played,
                pts = EXCLUDED.pts,
                reb = EXCLUDED.reb,
                ast = EXCLUDED.ast,
                fg_pct = EXCLUDED.fg_pct,
                fg3_pct = EXCLUDED.fg3_pct,
                ft_pct = EXCLUDED.ft_pct,
                raw_json = EXCLUDED.raw_json,
                updated_at = NOW()
        """,
            team_id,
            season,
            season_type,
            s.get("w"),
            s.get("l"),
            s.get("gp"),
            s.get("pts"),
            s.get("reb"),
            s.get("ast"),
            s.get("fg_pct"),
            s.get("fg3_pct"),
            s.get("ft_pct"),
            raw_json,
        )
    
    # =========================================================================
    # Full Seeding
    # =========================================================================
    
    async def seed_all(self, season: int, season_type: str = "regular") -> SeedResult:
        """Full seeding workflow for a season."""
        logger.info(f"Starting full NBA seeding for {season} {season_type}")
        
        result = SeedResult()
        
        # 1. Seed teams first (no season dependency)
        result = result + await self.seed_teams()
        
        # 2. Seed players
        result = result + await self.seed_players()
        
        # 3. Seed player stats
        result = result + await self.seed_player_stats(season, season_type)
        
        # 4. Seed team stats
        result = result + await self.seed_team_stats(season, season_type)
        
        logger.info(
            f"NBA seeding complete: "
            f"{result.teams_upserted} teams, "
            f"{result.players_upserted} players, "
            f"{result.player_stats_upserted} player stats, "
            f"{result.team_stats_upserted} team stats, "
            f"{len(result.errors)} errors"
        )
        
        return result
    
    # =========================================================================
    # Test Mode - Single Entity
    # =========================================================================
    
    async def test_single_team(self, team_id: int = 1) -> SeedResult:
        """Test seeding a single team."""
        logger.info(f"Testing single team seed: {team_id}")
        result = SeedResult()
        
        try:
            team = await self.client.get_team(team_id)
            await self._upsert_team(team)
            result.teams_upserted = 1
            logger.info(f"Successfully seeded team: {team.get('full_name')}")
        except Exception as e:
            result.errors.append(str(e))
            logger.error(f"Failed to seed team: {e}")
        
        return result
    
    async def test_single_player(self, player_id: int = 115) -> SeedResult:
        """Test seeding a single player (default: Stephen Curry)."""
        logger.info(f"Testing single player seed: {player_id}")
        result = SeedResult()
        
        try:
            player = await self.client.get_player(player_id)
            
            # Ensure team exists first
            team = player.get("team")
            if team:
                await self._upsert_team(team)
                result.teams_upserted = 1
            
            await self._upsert_player(player)
            result.players_upserted = 1
            logger.info(f"Successfully seeded player: {player.get('first_name')} {player.get('last_name')}")
        except Exception as e:
            result.errors.append(str(e))
            logger.error(f"Failed to seed player: {e}")
        
        return result
