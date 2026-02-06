"""
Data Loaders for Scoracle ML

Handles loading and preparing data from various sources:
- Database queries for players, teams, and stats
- External API data (news, twitter, reddit)
- Historical transfer data
"""

from dataclasses import dataclass
from datetime import datetime, timedelta
from typing import Any

import psycopg


@dataclass
class PlayerData:
    """Player data for ML features."""

    id: int
    name: str
    sport: str
    team_id: int | None
    team_name: str | None
    position: str | None
    stats: dict[str, Any] | None = None


@dataclass
class TeamData:
    """Team data for ML features."""

    id: int
    name: str
    sport: str
    league: str | None
    stats: dict[str, Any] | None = None


@dataclass
class TransferLinkData:
    """Transfer link data from database."""

    id: int
    player_id: int
    player_name: str
    team_id: int
    team_name: str
    sport: str
    first_linked_at: datetime
    last_mention_at: datetime
    total_mentions: int
    tier_1_mentions: int
    tier_2_mentions: int
    current_probability: float | None
    trend_direction: str
    is_active: bool


class DataLoader:
    """
    Data loader for ML pipelines.

    Provides methods to load data from the database
    and external sources for ML model training and inference.
    """

    def __init__(self, conn: psycopg.Connection | None = None):
        """
        Initialize data loader.

        Args:
            conn: Database connection (optional, can be set later)
        """
        self._conn = conn

    def set_connection(self, conn: psycopg.Connection) -> None:
        """Set the database connection."""
        self._conn = conn

    @property
    def conn(self) -> psycopg.Connection:
        """Get the database connection."""
        if self._conn is None:
            raise RuntimeError("Database connection not set")
        return self._conn

    async def load_all_players(self, sport: str | None = None) -> list[PlayerData]:
        """
        Load all players from the database.

        Args:
            sport: Optional sport filter

        Returns:
            List of player data
        """
        query = """
            SELECT
                p.id,
                p.name,
                s.name as sport,
                pt.team_id,
                t.name as team_name,
                p.position
            FROM players p
            JOIN sports s ON p.sport_id = s.id
            LEFT JOIN player_teams pt ON p.id = pt.player_id AND pt.is_current = TRUE
            LEFT JOIN teams t ON pt.team_id = t.id
        """
        params: list[Any] = []

        if sport:
            query += " WHERE LOWER(s.name) = LOWER(%s)"
            params.append(sport)

        query += " ORDER BY p.name"

        async with self.conn.cursor() as cur:
            await cur.execute(query, params)
            rows = await cur.fetchall()

        return [
            PlayerData(
                id=row[0],
                name=row[1],
                sport=row[2],
                team_id=row[3],
                team_name=row[4],
                position=row[5],
            )
            for row in rows
        ]

    async def load_all_teams(self, sport: str | None = None) -> list[TeamData]:
        """
        Load all teams from the database.

        Args:
            sport: Optional sport filter

        Returns:
            List of team data
        """
        query = """
            SELECT
                t.id,
                t.name,
                s.name as sport,
                l.name as league
            FROM teams t
            JOIN sports s ON t.sport_id = s.id
            LEFT JOIN leagues l ON t.league_id = l.id
        """
        params: list[Any] = []

        if sport:
            query += " WHERE LOWER(s.name) = LOWER(%s)"
            params.append(sport)

        query += " ORDER BY t.name"

        async with self.conn.cursor() as cur:
            await cur.execute(query, params)
            rows = await cur.fetchall()

        return [
            TeamData(
                id=row[0],
                name=row[1],
                sport=row[2],
                league=row[3],
            )
            for row in rows
        ]

    async def load_player_names(self, sport: str | None = None) -> list[str]:
        """
        Load all player names for text matching.

        Args:
            sport: Optional sport filter

        Returns:
            List of player names
        """
        players = await self.load_all_players(sport)
        return [p.name for p in players]

    async def load_team_names(self, sport: str | None = None) -> list[str]:
        """
        Load all team names for text matching.

        Args:
            sport: Optional sport filter

        Returns:
            List of team names
        """
        teams = await self.load_all_teams(sport)
        return [t.name for t in teams]

    async def load_active_transfer_links(
        self,
        sport: str | None = None,
        min_mentions: int = 0,
    ) -> list[TransferLinkData]:
        """
        Load active transfer links from the database.

        Args:
            sport: Optional sport filter
            min_mentions: Minimum mention count filter

        Returns:
            List of active transfer links
        """
        query = """
            SELECT
                id, player_id, player_name, team_id, team_name, sport,
                first_linked_at, last_mention_at, total_mentions,
                tier_1_mentions, tier_2_mentions, current_probability,
                trend_direction, is_active
            FROM transfer_links
            WHERE is_active = TRUE
        """
        params: list[Any] = []

        if sport:
            query += " AND LOWER(sport) = LOWER(%s)"
            params.append(sport)

        if min_mentions > 0:
            query += " AND total_mentions >= %s"
            params.append(min_mentions)

        query += " ORDER BY current_probability DESC NULLS LAST, total_mentions DESC"

        async with self.conn.cursor() as cur:
            await cur.execute(query, params)
            rows = await cur.fetchall()

        return [
            TransferLinkData(
                id=row[0],
                player_id=row[1],
                player_name=row[2],
                team_id=row[3],
                team_name=row[4],
                sport=row[5],
                first_linked_at=row[6],
                last_mention_at=row[7],
                total_mentions=row[8],
                tier_1_mentions=row[9],
                tier_2_mentions=row[10],
                current_probability=row[11],
                trend_direction=row[12],
                is_active=row[13],
            )
            for row in rows
        ]

    async def load_recent_mentions(
        self,
        transfer_link_id: int,
        hours: int = 168,  # 7 days
    ) -> list[dict[str, Any]]:
        """
        Load recent mentions for a transfer link.

        Args:
            transfer_link_id: ID of the transfer link
            hours: Number of hours to look back

        Returns:
            List of mention records
        """
        query = """
            SELECT
                id, source_type, source_name, source_tier,
                headline, sentiment_score, engagement_score, mentioned_at
            FROM transfer_mentions
            WHERE transfer_link_id = %s
            AND mentioned_at >= NOW() - INTERVAL '%s hours'
            ORDER BY mentioned_at DESC
        """

        async with self.conn.cursor() as cur:
            await cur.execute(query, (transfer_link_id, hours))
            rows = await cur.fetchall()

        return [
            {
                "id": row[0],
                "source_type": row[1],
                "source_name": row[2],
                "source_tier": row[3],
                "headline": row[4],
                "sentiment_score": row[5],
                "engagement_score": row[6],
                "mentioned_at": row[7],
            }
            for row in rows
        ]

    async def load_historical_transfers(
        self,
        sport: str | None = None,
        min_date: datetime | None = None,
    ) -> list[dict[str, Any]]:
        """
        Load historical transfers for training.

        Args:
            sport: Optional sport filter
            min_date: Minimum transfer date filter

        Returns:
            List of historical transfer records
        """
        query = """
            SELECT
                id, player_id, player_name, from_team_id, from_team_name,
                to_team_id, to_team_name, sport, transfer_date, fee_millions,
                loan_deal, rumor_duration_days, peak_mentions
            FROM historical_transfers
            WHERE 1=1
        """
        params: list[Any] = []

        if sport:
            query += " AND LOWER(sport) = LOWER(%s)"
            params.append(sport)

        if min_date:
            query += " AND transfer_date >= %s"
            params.append(min_date)

        query += " ORDER BY transfer_date DESC"

        async with self.conn.cursor() as cur:
            await cur.execute(query, params)
            rows = await cur.fetchall()

        return [
            {
                "id": row[0],
                "player_id": row[1],
                "player_name": row[2],
                "from_team_id": row[3],
                "from_team_name": row[4],
                "to_team_id": row[5],
                "to_team_name": row[6],
                "sport": row[7],
                "transfer_date": row[8],
                "fee_millions": row[9],
                "loan_deal": row[10],
                "rumor_duration_days": row[11],
                "peak_mentions": row[12],
            }
            for row in rows
        ]

    async def load_player_stats(
        self,
        player_id: int,
        sport: str,
        season: str | None = None,
    ) -> dict[str, Any] | None:
        """
        Load player statistics.

        Args:
            player_id: Player ID
            sport: Sport name
            season: Optional season filter

        Returns:
            Dict of statistics or None
        """
        from ...core.types import PLAYER_STATS_TABLES

        table = PLAYER_STATS_TABLES.get(sport.upper())
        if not table:
            return None

        query = f"""
            SELECT *
            FROM {table}
            WHERE player_id = %s
        """
        params: list[Any] = [player_id]

        if season:
            query += " AND season = %s"
            params.append(season)

        query += " ORDER BY season DESC LIMIT 1"

        async with self.conn.cursor() as cur:
            await cur.execute(query, params)
            row = await cur.fetchone()
            if not row:
                return None

            # Get column names
            columns = [desc[0] for desc in cur.description]
            return dict(zip(columns, row))

    async def load_team_stats(
        self,
        team_id: int,
        sport: str,
        season: str | None = None,
    ) -> dict[str, Any] | None:
        """
        Load team statistics.

        Args:
            team_id: Team ID
            sport: Sport name
            season: Optional season filter

        Returns:
            Dict of statistics or None
        """
        from ...core.types import TEAM_STATS_TABLES

        table = TEAM_STATS_TABLES.get(sport.upper())
        if not table:
            return None

        query = f"""
            SELECT *
            FROM {table}
            WHERE team_id = %s
        """
        params: list[Any] = [team_id]

        if season:
            query += " AND season = %s"
            params.append(season)

        query += " ORDER BY season DESC LIMIT 1"

        async with self.conn.cursor() as cur:
            await cur.execute(query, params)
            row = await cur.fetchone()
            if not row:
                return None

            columns = [desc[0] for desc in cur.description]
            return dict(zip(columns, row))

    async def load_vibe_history(
        self,
        entity_type: str,
        entity_id: int,
        days: int = 30,
    ) -> list[dict[str, Any]]:
        """
        Load vibe score history for an entity.

        Args:
            entity_type: 'player' or 'team'
            entity_id: Entity ID
            days: Number of days to look back

        Returns:
            List of vibe score records
        """
        query = """
            SELECT
                overall_score, twitter_score, news_score, reddit_score,
                total_sample_size, calculated_at
            FROM vibe_scores
            WHERE entity_type = %s
            AND entity_id = %s
            AND calculated_at >= NOW() - INTERVAL '%s days'
            ORDER BY calculated_at DESC
        """

        async with self.conn.cursor() as cur:
            await cur.execute(query, (entity_type, entity_id, days))
            rows = await cur.fetchall()

        return [
            {
                "overall_score": row[0],
                "twitter_score": row[1],
                "news_score": row[2],
                "reddit_score": row[3],
                "total_sample_size": row[4],
                "calculated_at": row[5],
            }
            for row in rows
        ]

    async def load_entity_embedding(
        self,
        entity_type: str,
        entity_id: int,
        sport: str,
        season: str | None = None,
    ) -> list[float] | None:
        """
        Load pre-computed embedding for an entity.

        Args:
            entity_type: 'player' or 'team'
            entity_id: Entity ID
            sport: Sport name
            season: Optional season filter

        Returns:
            Embedding vector or None
        """
        query = """
            SELECT embedding
            FROM entity_embeddings
            WHERE entity_type = %s
            AND entity_id = %s
            AND LOWER(sport) = LOWER(%s)
        """
        params: list[Any] = [entity_type, entity_id, sport]

        if season:
            query += " AND season = %s"
            params.append(season)

        query += " ORDER BY computed_at DESC LIMIT 1"

        async with self.conn.cursor() as cur:
            await cur.execute(query, params)
            row = await cur.fetchone()
            return row[0] if row else None

    async def save_transfer_link(
        self,
        player_id: int,
        player_name: str,
        team_id: int,
        team_name: str,
        sport: str,
        player_current_team: str | None = None,
    ) -> int:
        """
        Save or update a transfer link.

        Args:
            player_id: Player ID
            player_name: Player name
            team_id: Target team ID
            team_name: Target team name
            sport: Sport name
            player_current_team: Player's current team name

        Returns:
            Transfer link ID
        """
        query = """
            INSERT INTO transfer_links (
                player_id, player_name, player_current_team,
                team_id, team_name, sport
            )
            VALUES (%s, %s, %s, %s, %s, %s)
            ON CONFLICT (player_id, team_id, sport) WHERE is_active = TRUE
            DO UPDATE SET
                last_mention_at = NOW(),
                total_mentions = transfer_links.total_mentions + 1,
                updated_at = NOW()
            RETURNING id
        """

        async with self.conn.cursor() as cur:
            await cur.execute(
                query,
                (player_id, player_name, player_current_team, team_id, team_name, sport),
            )
            result = await cur.fetchone()
            await self.conn.commit()
            return result[0]

    async def save_transfer_mention(
        self,
        transfer_link_id: int,
        source_type: str,
        source_name: str,
        source_tier: int,
        headline: str,
        url: str | None = None,
        sentiment_score: float | None = None,
        engagement_score: int = 0,
        mentioned_at: datetime | None = None,
    ) -> int:
        """
        Save a transfer mention.

        Args:
            transfer_link_id: Transfer link ID
            source_type: Source type ('news', 'twitter', 'reddit')
            source_name: Source name
            source_tier: Source tier (1-4)
            headline: Headline or text
            url: Source URL
            sentiment_score: Sentiment score
            engagement_score: Engagement metric
            mentioned_at: When mentioned

        Returns:
            Mention ID
        """
        query = """
            INSERT INTO transfer_mentions (
                transfer_link_id, source_type, source_name, source_tier,
                headline, source_url, sentiment_score, engagement_score, mentioned_at
            )
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s)
            RETURNING id
        """

        async with self.conn.cursor() as cur:
            await cur.execute(
                query,
                (
                    transfer_link_id,
                    source_type,
                    source_name,
                    source_tier,
                    headline,
                    url,
                    sentiment_score,
                    engagement_score,
                    mentioned_at or datetime.now(),
                ),
            )
            result = await cur.fetchone()

            # Update tier counts on the transfer link
            tier_field = f"tier_{source_tier}_mentions"
            await cur.execute(
                f"""
                UPDATE transfer_links
                SET {tier_field} = {tier_field} + 1,
                    updated_at = NOW()
                WHERE id = %s
                """,
                (transfer_link_id,),
            )

            await self.conn.commit()
            return result[0]
