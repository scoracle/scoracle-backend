"""NFL seeder - seeds NFL data from BallDontLie API."""

import json
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
from .client import BallDontLieNFL

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


class NFLSeeder:
    """Seeder for NFL data from BallDontLie."""
    
    def __init__(self, db: asyncpg.Connection, client: BallDontLieNFL):
        self.db = db
        self.client = client
    
    # =========================================================================
    # Teams
    # =========================================================================
    
    async def seed_teams(self) -> SeedResult:
        """Seed all NFL teams."""
        logger.info("Seeding NFL teams...")
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
                id, name, full_name, abbreviation, location, conference, division
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                full_name = EXCLUDED.full_name,
                abbreviation = EXCLUDED.abbreviation,
                location = EXCLUDED.location,
                conference = EXCLUDED.conference,
                division = EXCLUDED.division,
                updated_at = NOW()
        """,
            team["id"],
            team.get("name"),
            team.get("full_name"),
            team.get("abbreviation"),
            team.get("location"),
            team.get("conference"),
            team.get("division"),
        )
    
    # =========================================================================
    # Players
    # =========================================================================
    
    async def seed_players(self) -> SeedResult:
        """Seed all NFL players."""
        logger.info("Seeding NFL players...")
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
                id, first_name, last_name, position, position_abbreviation,
                height, weight, jersey_number, college, experience, age, team_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (id) DO UPDATE SET
                first_name = EXCLUDED.first_name,
                last_name = EXCLUDED.last_name,
                position = EXCLUDED.position,
                position_abbreviation = EXCLUDED.position_abbreviation,
                height = EXCLUDED.height,
                weight = EXCLUDED.weight,
                jersey_number = EXCLUDED.jersey_number,
                college = EXCLUDED.college,
                experience = EXCLUDED.experience,
                age = EXCLUDED.age,
                team_id = EXCLUDED.team_id,
                updated_at = NOW()
        """,
            player["id"],
            player.get("first_name"),
            player.get("last_name"),
            player.get("position"),
            player.get("position_abbreviation"),
            player.get("height"),
            player.get("weight"),
            player.get("jersey_number"),
            player.get("college"),
            player.get("experience"),
            player.get("age"),
            team_id,
        )
    
    # =========================================================================
    # Player Stats
    # =========================================================================
    
    async def seed_player_stats(self, season: int, postseason: bool = False) -> SeedResult:
        """Seed player season stats."""
        ps_label = "postseason" if postseason else "regular"
        logger.info(f"Seeding NFL player stats for {season} {ps_label}...")
        result = SeedResult()
        
        try:
            count = 0
            async for stats in self.client.get_season_stats(season, postseason):
                try:
                    await self._upsert_player_stats(stats, season, postseason)
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
        postseason: bool,
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
        
        raw_json = json.dumps(stats_data)
        
        # Extract stats - NFL API nests stats differently
        # These may be at the top level or nested
        await self.db.execute(f"""
            INSERT INTO {PLAYER_STATS_TABLE} (
                player_id, season, postseason, games_played,
                -- Passing
                passing_completions, passing_attempts, passing_yards,
                passing_touchdowns, passing_interceptions, passing_yards_per_game,
                passing_completion_pct, qbr,
                -- Rushing
                rushing_attempts, rushing_yards, rushing_touchdowns,
                rushing_yards_per_game, yards_per_rush_attempt, rushing_first_downs,
                -- Receiving
                receptions, receiving_yards, receiving_touchdowns,
                receiving_targets, receiving_yards_per_game, yards_per_reception,
                receiving_first_downs,
                -- Defensive
                total_tackles, solo_tackles, assist_tackles,
                defensive_sacks, defensive_sack_yards, defensive_interceptions,
                interception_touchdowns, fumbles_forced, fumbles_recovered,
                -- Kicking
                field_goal_attempts, field_goals_made, field_goal_pct,
                punts, punt_yards,
                -- Returns
                kick_returns, kick_return_yards, kick_return_touchdowns,
                punt_returner_returns, punt_returner_return_yards, punt_return_touchdowns,
                raw_json
            ) VALUES (
                $1, $2, $3, $4,
                $5, $6, $7, $8, $9, $10, $11, $12,
                $13, $14, $15, $16, $17, $18,
                $19, $20, $21, $22, $23, $24, $25,
                $26, $27, $28, $29, $30, $31, $32, $33, $34,
                $35, $36, $37, $38, $39,
                $40, $41, $42, $43, $44, $45,
                $46
            )
            ON CONFLICT (player_id, season, postseason) DO UPDATE SET
                games_played = EXCLUDED.games_played,
                passing_completions = EXCLUDED.passing_completions,
                passing_attempts = EXCLUDED.passing_attempts,
                passing_yards = EXCLUDED.passing_yards,
                passing_touchdowns = EXCLUDED.passing_touchdowns,
                passing_interceptions = EXCLUDED.passing_interceptions,
                passing_yards_per_game = EXCLUDED.passing_yards_per_game,
                passing_completion_pct = EXCLUDED.passing_completion_pct,
                qbr = EXCLUDED.qbr,
                rushing_attempts = EXCLUDED.rushing_attempts,
                rushing_yards = EXCLUDED.rushing_yards,
                rushing_touchdowns = EXCLUDED.rushing_touchdowns,
                rushing_yards_per_game = EXCLUDED.rushing_yards_per_game,
                yards_per_rush_attempt = EXCLUDED.yards_per_rush_attempt,
                rushing_first_downs = EXCLUDED.rushing_first_downs,
                receptions = EXCLUDED.receptions,
                receiving_yards = EXCLUDED.receiving_yards,
                receiving_touchdowns = EXCLUDED.receiving_touchdowns,
                receiving_targets = EXCLUDED.receiving_targets,
                receiving_yards_per_game = EXCLUDED.receiving_yards_per_game,
                yards_per_reception = EXCLUDED.yards_per_reception,
                receiving_first_downs = EXCLUDED.receiving_first_downs,
                total_tackles = EXCLUDED.total_tackles,
                solo_tackles = EXCLUDED.solo_tackles,
                assist_tackles = EXCLUDED.assist_tackles,
                defensive_sacks = EXCLUDED.defensive_sacks,
                defensive_sack_yards = EXCLUDED.defensive_sack_yards,
                defensive_interceptions = EXCLUDED.defensive_interceptions,
                interception_touchdowns = EXCLUDED.interception_touchdowns,
                fumbles_forced = EXCLUDED.fumbles_forced,
                fumbles_recovered = EXCLUDED.fumbles_recovered,
                field_goal_attempts = EXCLUDED.field_goal_attempts,
                field_goals_made = EXCLUDED.field_goals_made,
                field_goal_pct = EXCLUDED.field_goal_pct,
                punts = EXCLUDED.punts,
                punt_yards = EXCLUDED.punt_yards,
                kick_returns = EXCLUDED.kick_returns,
                kick_return_yards = EXCLUDED.kick_return_yards,
                kick_return_touchdowns = EXCLUDED.kick_return_touchdowns,
                punt_returner_returns = EXCLUDED.punt_returner_returns,
                punt_returner_return_yards = EXCLUDED.punt_returner_return_yards,
                punt_return_touchdowns = EXCLUDED.punt_return_touchdowns,
                raw_json = EXCLUDED.raw_json,
                updated_at = NOW()
        """,
            player_id,
            season,
            postseason,
            stats_data.get("games_played"),
            # Passing
            stats_data.get("passing_completions"),
            stats_data.get("passing_attempts"),
            stats_data.get("passing_yards"),
            stats_data.get("passing_touchdowns"),
            stats_data.get("passing_interceptions"),
            stats_data.get("passing_yards_per_game"),
            stats_data.get("passing_completion_pct"),
            stats_data.get("qbr"),
            # Rushing
            stats_data.get("rushing_attempts"),
            stats_data.get("rushing_yards"),
            stats_data.get("rushing_touchdowns"),
            stats_data.get("rushing_yards_per_game"),
            stats_data.get("yards_per_rush_attempt"),
            stats_data.get("rushing_first_downs"),
            # Receiving
            stats_data.get("receptions"),
            stats_data.get("receiving_yards"),
            stats_data.get("receiving_touchdowns"),
            stats_data.get("receiving_targets"),
            stats_data.get("receiving_yards_per_game"),
            stats_data.get("yards_per_reception"),
            stats_data.get("receiving_first_downs"),
            # Defensive
            stats_data.get("total_tackles"),
            stats_data.get("solo_tackles"),
            stats_data.get("assist_tackles"),
            stats_data.get("defensive_sacks"),
            stats_data.get("defensive_sack_yards"),
            stats_data.get("defensive_interceptions"),
            stats_data.get("interception_touchdowns"),
            stats_data.get("fumbles_forced"),
            stats_data.get("fumbles_recovered"),
            # Kicking
            stats_data.get("field_goal_attempts"),
            stats_data.get("field_goals_made"),
            stats_data.get("field_goal_pct"),
            stats_data.get("punts"),
            stats_data.get("punt_yards"),
            # Returns
            stats_data.get("kick_returns"),
            stats_data.get("kick_return_yards"),
            stats_data.get("kick_return_touchdowns"),
            stats_data.get("punt_returner_returns"),
            stats_data.get("punt_returner_return_yards"),
            stats_data.get("punt_return_touchdowns"),
            raw_json,
        )
    
    # =========================================================================
    # Team Stats
    # =========================================================================
    
    async def seed_team_stats(self, season: int, postseason: bool = False) -> SeedResult:
        """Seed team season stats."""
        ps_label = "postseason" if postseason else "regular"
        logger.info(f"Seeding NFL team stats for {season} {ps_label}...")
        result = SeedResult()
        
        try:
            stats_list = await self.client.get_team_season_stats(season, postseason)
            logger.info(f"Fetched {len(stats_list)} team stats")
            
            for stats in stats_list:
                try:
                    await self._upsert_team_stats(stats, season, postseason)
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
        postseason: bool,
    ) -> None:
        """Upsert team season stats."""
        team = stats_data.get("team", {})
        team_id = team.get("id")
        
        if not team_id:
            return
        
        raw_json = json.dumps(stats_data)
        
        await self.db.execute(f"""
            INSERT INTO {TEAM_STATS_TABLE} (
                team_id, season, postseason, wins, losses, ties,
                points_for, points_against, point_differential, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (team_id, season, postseason) DO UPDATE SET
                wins = EXCLUDED.wins,
                losses = EXCLUDED.losses,
                ties = EXCLUDED.ties,
                points_for = EXCLUDED.points_for,
                points_against = EXCLUDED.points_against,
                point_differential = EXCLUDED.point_differential,
                raw_json = EXCLUDED.raw_json,
                updated_at = NOW()
        """,
            team_id,
            season,
            postseason,
            stats_data.get("wins"),
            stats_data.get("losses"),
            stats_data.get("ties"),
            stats_data.get("points_for"),
            stats_data.get("points_against"),
            stats_data.get("point_differential"),
            raw_json,
        )
    
    # =========================================================================
    # Full Seeding
    # =========================================================================
    
    async def seed_all(self, season: int, postseason: bool = False) -> SeedResult:
        """Full seeding workflow for a season."""
        ps_label = "postseason" if postseason else "regular"
        logger.info(f"Starting full NFL seeding for {season} {ps_label}")
        
        result = SeedResult()
        
        # 1. Seed teams first (no season dependency)
        result = result + await self.seed_teams()
        
        # 2. Seed players
        result = result + await self.seed_players()
        
        # 3. Seed player stats
        result = result + await self.seed_player_stats(season, postseason)
        
        # 4. Seed team stats
        result = result + await self.seed_team_stats(season, postseason)
        
        logger.info(
            f"NFL seeding complete: "
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
    
    async def test_single_player(self, player_id: int = 1) -> SeedResult:
        """Test seeding a single player."""
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
