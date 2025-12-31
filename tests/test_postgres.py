"""
PostgreSQL/Neon-specific tests for scoracle-data.

These tests verify PostgreSQL compatibility:
- Connection pooling and management
- Query placeholder syntax (%s vs ?)
- PostgreSQL-native percentile calculations
- UPSERT with ON CONFLICT syntax
- RETURNING clause support
"""

import os
import pytest
from unittest.mock import Mock, patch, MagicMock

# Load .env file for tests
_env_file = os.path.join(os.path.dirname(__file__), "..", ".env")
if os.path.exists(_env_file):
    with open(_env_file) as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                key, value = line.split("=", 1)
                if key.strip() not in os.environ:
                    os.environ[key.strip()] = value.strip().strip('"').strip("'")

# Skip all tests if DATABASE_URL is not set
pytestmark = pytest.mark.skipif(
    not os.getenv("DATABASE_URL") and not os.getenv("NEON_DATABASE_URL"),
    reason="DATABASE_URL environment variable not set"
)


class TestPostgresDBConnection:
    """Test PostgresDB connection management."""

    def test_connection_requires_url(self):
        """Connection should fail gracefully without URL."""
        from scoracle_data.pg_connection import PostgresDB

        with patch.dict(os.environ, {}, clear=True):
            # Remove DATABASE_URL if present
            os.environ.pop("DATABASE_URL", None)
            os.environ.pop("NEON_DATABASE_URL", None)
            with pytest.raises(ValueError, match="DATABASE_URL"):
                PostgresDB()

    def test_connection_with_valid_url(self):
        """Connection should succeed with valid URL."""
        from scoracle_data.pg_connection import PostgresDB

        db = PostgresDB()
        assert db is not None
        db.close()

    def test_connection_pool_creation(self):
        """Should create a connection pool."""
        from scoracle_data.pg_connection import PostgresDB

        db = PostgresDB()
        # Pool should be created on first access
        assert db._pool is not None or db._connection is not None
        db.close()

    def test_manual_connection(self):
        """Should work with manual connection management."""
        from scoracle_data.pg_connection import PostgresDB

        db = PostgresDB()
        result = db.fetchone("SELECT 1 as test")
        assert result is not None
        assert result.get("test") == 1
        db.close()


class TestPostgresDBQueries:
    """Test query execution with PostgreSQL syntax."""

    @pytest.fixture
    def db(self):
        """Create a database connection for tests."""
        from scoracle_data.pg_connection import PostgresDB
        db = PostgresDB()
        yield db
        db.close()

    def test_placeholder_syntax(self, db):
        """Queries should use %s placeholders, not ?."""
        # This should work with PostgreSQL syntax
        result = db.fetchone("SELECT %s as value", (42,))
        assert result["value"] == 42

    def test_fetchone_returns_dict(self, db):
        """fetchone should return a dictionary."""
        result = db.fetchone("SELECT 1 as a, 2 as b")
        assert isinstance(result, dict)
        assert result["a"] == 1
        assert result["b"] == 2

    def test_fetchall_returns_list_of_dicts(self, db):
        """fetchall should return a list of dictionaries."""
        results = db.fetchall("SELECT generate_series(1, 3) as n")
        assert isinstance(results, list)
        assert len(results) == 3
        assert all(isinstance(r, dict) for r in results)

    def test_execute_with_returning(self, db):
        """RETURNING clause should work for inserts."""
        # Drop and create temp table for test
        db.execute("DROP TABLE IF EXISTS test_returning")
        db.execute("""
            CREATE TEMP TABLE test_returning (
                id SERIAL PRIMARY KEY,
                value TEXT
            )
        """)

        result = db.fetchone(
            "INSERT INTO test_returning (value) VALUES (%s) RETURNING id, value",
            ("test_value",)
        )
        assert result is not None
        assert result["id"] == 1
        assert result["value"] == "test_value"


class TestPostgresDBUpsert:
    """Test UPSERT operations with ON CONFLICT."""

    @pytest.fixture
    def db(self):
        """Create a database connection and temp table."""
        from scoracle_data.pg_connection import PostgresDB
        db = PostgresDB()

        # Drop and create temp table for upsert tests
        db.execute("DROP TABLE IF EXISTS test_upsert")
        db.execute("""
            CREATE TEMP TABLE test_upsert (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                value INTEGER DEFAULT 0
            )
        """)

        yield db
        db.close()

    def test_insert_on_conflict_do_update(self, db):
        """ON CONFLICT DO UPDATE should work."""
        # First insert
        db.execute("""
            INSERT INTO test_upsert (id, name, value)
            VALUES (%s, %s, %s)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                value = EXCLUDED.value
        """, (1, "first", 100))

        result = db.fetchone("SELECT * FROM test_upsert WHERE id = %s", (1,))
        assert result["name"] == "first"
        assert result["value"] == 100

        # Update via upsert
        db.execute("""
            INSERT INTO test_upsert (id, name, value)
            VALUES (%s, %s, %s)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                value = EXCLUDED.value
        """, (1, "updated", 200))

        result = db.fetchone("SELECT * FROM test_upsert WHERE id = %s", (1,))
        assert result["name"] == "updated"
        assert result["value"] == 200

    def test_insert_on_conflict_do_nothing(self, db):
        """ON CONFLICT DO NOTHING should work."""
        db.execute(
            "INSERT INTO test_upsert (id, name) VALUES (%s, %s)",
            (1, "original")
        )

        # This should not raise and should not update
        db.execute("""
            INSERT INTO test_upsert (id, name)
            VALUES (%s, %s)
            ON CONFLICT (id) DO NOTHING
        """, (1, "duplicate"))

        result = db.fetchone("SELECT * FROM test_upsert WHERE id = %s", (1,))
        assert result["name"] == "original"


class TestPostgresPercentileCalculations:
    """Test PostgreSQL-native percentile calculations."""

    @pytest.fixture
    def db(self):
        """Create a database connection with test data."""
        from scoracle_data.pg_connection import PostgresDB
        db = PostgresDB()

        # Drop and create temp table with sample data
        db.execute("DROP TABLE IF EXISTS test_stats")
        db.execute("""
            CREATE TEMP TABLE test_stats (
                id SERIAL PRIMARY KEY,
                player_id INTEGER,
                stat_value REAL
            )
        """)

        # Insert test data (100 values from 1 to 100)
        for i in range(1, 101):
            db.execute(
                "INSERT INTO test_stats (player_id, stat_value) VALUES (%s, %s)",
                (i, float(i))
            )

        yield db
        db.close()

    def test_percentile_cont(self, db):
        """PERCENTILE_CONT should calculate percentiles correctly."""
        result = db.fetchone("""
            SELECT PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY stat_value) as median
            FROM test_stats
        """)
        # Median of 1-100 should be 50.5
        assert result["median"] == pytest.approx(50.5, rel=0.01)

    def test_percentile_disc(self, db):
        """PERCENTILE_DISC should return discrete percentiles."""
        result = db.fetchone("""
            SELECT PERCENTILE_DISC(0.5) WITHIN GROUP (ORDER BY stat_value) as median
            FROM test_stats
        """)
        # PERCENTILE_DISC returns an actual value from the dataset
        assert result["median"] in [50, 51]

    def test_percent_rank(self, db):
        """PERCENT_RANK window function should work."""
        results = db.fetchall("""
            SELECT
                player_id,
                stat_value,
                PERCENT_RANK() OVER (ORDER BY stat_value) * 100 as percentile
            FROM test_stats
            WHERE player_id IN (1, 50, 100)
            ORDER BY stat_value
        """)

        assert len(results) == 3
        # Player with value 1 should be at 0th percentile
        assert results[0]["percentile"] == pytest.approx(0, abs=1)
        # Player with value 100 should be at 100th percentile
        assert results[2]["percentile"] == pytest.approx(100, abs=1)


class TestQueryBuilder:
    """Test query builder generates PostgreSQL-compatible queries."""

    def test_upsert_query_uses_postgres_syntax(self):
        """Upsert queries should use PostgreSQL ON CONFLICT syntax."""
        from scoracle_data.query_builder import query_cache

        query = query_cache.get_or_build_upsert(
            table="test_table",
            columns=["id", "name", "value"],
            conflict_keys=["id"],
        )

        assert "%s" in query  # PostgreSQL placeholder
        assert "?" not in query  # Not SQLite placeholder
        assert "ON CONFLICT" in query
        assert "DO UPDATE SET" in query
        assert "excluded." in query.lower()  # Case-insensitive check

    def test_upsert_with_multiple_conflict_keys(self):
        """Upsert with composite key should work."""
        from scoracle_data.query_builder import query_cache

        query = query_cache.get_or_build_upsert(
            table="stats",
            columns=["player_id", "season_id", "points"],
            conflict_keys=["player_id", "season_id"],
        )

        assert "(player_id, season_id)" in query


class TestPGCalculator:
    """Test PostgreSQL percentile calculator."""

    @pytest.fixture
    def db(self):
        """Create a database connection."""
        from scoracle_data.pg_connection import PostgresDB
        db = PostgresDB()
        yield db
        db.close()

    def test_calculator_initialization(self, db):
        """Calculator should initialize with database connection."""
        from scoracle_data.percentiles.pg_calculator import PostgresPercentileCalculator

        calc = PostgresPercentileCalculator(db)
        assert calc is not None
        assert calc.db == db

    def test_calculator_has_required_methods(self, db):
        """Calculator should have required methods."""
        from scoracle_data.percentiles.pg_calculator import PostgresPercentileCalculator

        calc = PostgresPercentileCalculator(db)

        # Should have key methods for percentile calculation
        assert hasattr(calc, "calculate_all_player_percentiles")
        assert hasattr(calc, "calculate_all_team_percentiles")
        assert hasattr(calc, "recalculate_all_percentiles")
        assert hasattr(calc, "get_player_percentiles")
        assert hasattr(calc, "get_team_percentiles")


class TestSchemaCompatibility:
    """Test that schema works with PostgreSQL."""

    @pytest.fixture
    def db(self):
        """Create a database connection."""
        from scoracle_data.pg_connection import PostgresDB
        db = PostgresDB()
        yield db
        db.close()

    def test_tables_exist(self, db):
        """Required tables should exist."""
        tables_query = """
            SELECT table_name
            FROM information_schema.tables
            WHERE table_schema = 'public'
            AND table_type = 'BASE TABLE'
        """
        results = db.fetchall(tables_query)
        table_names = {r["table_name"] for r in results}

        required_tables = {
            "meta",
            "sports",
            "seasons",
            "leagues",
            "teams",
            "players",
            "nba_player_stats",
            "nba_team_stats",
            "nfl_player_stats",
            "nfl_team_stats",
            "football_player_stats",
            "football_team_stats",
            "percentile_cache",
            "sync_log",
        }

        missing = required_tables - table_names
        assert not missing, f"Missing tables: {missing}"

    def test_timestamptz_columns(self, db):
        """Timestamp columns should use TIMESTAMPTZ type."""
        result = db.fetchone("""
            SELECT data_type
            FROM information_schema.columns
            WHERE table_name = 'teams'
            AND column_name = 'updated_at'
        """)

        assert result is not None
        # PostgreSQL reports 'timestamp with time zone' for TIMESTAMPTZ
        assert "timestamp" in result["data_type"].lower()

    def test_serial_primary_keys(self, db):
        """Auto-increment columns should use SERIAL."""
        result = db.fetchone("""
            SELECT column_default
            FROM information_schema.columns
            WHERE table_name = 'seasons'
            AND column_name = 'id'
        """)

        assert result is not None
        # SERIAL columns have nextval() as default
        assert "nextval" in (result["column_default"] or "").lower()


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
