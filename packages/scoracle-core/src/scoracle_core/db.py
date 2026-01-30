"""Database connection and operations using asyncpg."""

import logging
import json
from typing import Any
from contextlib import asynccontextmanager

import asyncpg

logger = logging.getLogger(__name__)


class Database:
    """Async PostgreSQL database wrapper."""
    
    def __init__(self, dsn: str, min_size: int = 2, max_size: int = 10):
        self.dsn = dsn
        self.min_size = min_size
        self.max_size = max_size
        self._pool: asyncpg.Pool | None = None
    
    async def connect(self) -> None:
        """Create connection pool."""
        if self._pool is not None:
            return
        
        logger.info("Connecting to database...")
        self._pool = await asyncpg.create_pool(
            self.dsn,
            min_size=self.min_size,
            max_size=self.max_size,
            # Handle JSONB encoding/decoding
            init=self._init_connection,
        )
        logger.info("Database connected")
    
    async def _init_connection(self, conn: asyncpg.Connection) -> None:
        """Initialize connection with JSON codec."""
        await conn.set_type_codec(
            'jsonb',
            encoder=json.dumps,
            decoder=json.loads,
            schema='pg_catalog',
        )
    
    async def close(self) -> None:
        """Close connection pool."""
        if self._pool:
            await self._pool.close()
            self._pool = None
            logger.info("Database connection closed")
    
    async def __aenter__(self) -> "Database":
        await self.connect()
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        await self.close()
    
    @property
    def pool(self) -> asyncpg.Pool:
        """Get the connection pool."""
        if self._pool is None:
            raise RuntimeError("Database not connected. Call connect() first.")
        return self._pool
    
    async def execute(self, query: str, *args) -> str:
        """Execute a query and return status."""
        return await self.pool.execute(query, *args)
    
    async def fetch(self, query: str, *args) -> list[asyncpg.Record]:
        """Fetch multiple rows."""
        return await self.pool.fetch(query, *args)
    
    async def fetchrow(self, query: str, *args) -> asyncpg.Record | None:
        """Fetch a single row."""
        return await self.pool.fetchrow(query, *args)
    
    async def fetchval(self, query: str, *args) -> Any:
        """Fetch a single value."""
        return await self.pool.fetchval(query, *args)
    
    async def executemany(self, query: str, args: list[tuple]) -> None:
        """Execute a query for multiple parameter sets."""
        await self.pool.executemany(query, args)
    
    @asynccontextmanager
    async def transaction(self):
        """Context manager for transactions."""
        async with self.pool.acquire() as conn:
            async with conn.transaction():
                yield conn
    
    async def upsert(
        self,
        table: str,
        data: dict[str, Any],
        conflict_keys: list[str],
        update_keys: list[str] | None = None,
    ) -> None:
        """
        Insert or update a row.
        
        Args:
            table: Table name
            data: Column name -> value mapping
            conflict_keys: Columns to use for conflict detection
            update_keys: Columns to update on conflict (default: all except conflict_keys)
        """
        columns = list(data.keys())
        values = list(data.values())
        placeholders = [f"${i+1}" for i in range(len(values))]
        
        if update_keys is None:
            update_keys = [k for k in columns if k not in conflict_keys]
        
        update_clause = ", ".join(
            f"{k} = EXCLUDED.{k}" for k in update_keys
        )
        
        query = f"""
            INSERT INTO {table} ({", ".join(columns)})
            VALUES ({", ".join(placeholders)})
            ON CONFLICT ({", ".join(conflict_keys)})
            DO UPDATE SET {update_clause}, updated_at = NOW()
        """
        
        await self.execute(query, *values)
    
    async def bulk_upsert(
        self,
        table: str,
        rows: list[dict[str, Any]],
        conflict_keys: list[str],
        update_keys: list[str] | None = None,
    ) -> int:
        """
        Bulk insert or update rows.
        
        Returns number of rows processed.
        """
        if not rows:
            return 0
        
        # Use first row to determine columns
        columns = list(rows[0].keys())
        
        if update_keys is None:
            update_keys = [k for k in columns if k not in conflict_keys]
        
        placeholders = [f"${i+1}" for i in range(len(columns))]
        update_clause = ", ".join(
            f"{k} = EXCLUDED.{k}" for k in update_keys
        )
        
        query = f"""
            INSERT INTO {table} ({", ".join(columns)})
            VALUES ({", ".join(placeholders)})
            ON CONFLICT ({", ".join(conflict_keys)})
            DO UPDATE SET {update_clause}, updated_at = NOW()
        """
        
        # Convert rows to tuples
        args = [tuple(row[col] for col in columns) for row in rows]
        
        await self.executemany(query, args)
        return len(rows)
