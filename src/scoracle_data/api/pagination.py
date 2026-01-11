"""
Pagination utilities for API endpoints.

Usage for listing endpoints:
    @router.get("/players")
    async def list_players(
        pagination: PaginationParams = Depends(),
        db: DBDependency,
    ):
        query = "SELECT * FROM players WHERE sport_id = %s"
        total = db.fetchone(f"SELECT COUNT(*) FROM ({query}) t", params)["count"]

        paginated_query = pagination.apply_to_query(query)
        items = db.fetchall(paginated_query, params)

        return pagination.paginate(items, total)
"""

from dataclasses import dataclass
from typing import Any, Generic, TypeVar

from fastapi import Query

T = TypeVar("T")

# Default pagination limits
DEFAULT_PAGE_SIZE = 20
MAX_PAGE_SIZE = 100


@dataclass
class PaginationParams:
    """
    Pagination parameters extracted from query string.

    Usage as FastAPI dependency:
        async def endpoint(pagination: PaginationParams = Depends()):
            ...
    """
    page: int = Query(default=1, ge=1, description="Page number (1-indexed)")
    page_size: int = Query(
        default=DEFAULT_PAGE_SIZE,
        ge=1,
        le=MAX_PAGE_SIZE,
        alias="limit",
        description=f"Items per page (max {MAX_PAGE_SIZE})",
    )

    @property
    def offset(self) -> int:
        """Calculate SQL OFFSET for current page."""
        return (self.page - 1) * self.page_size

    @property
    def limit(self) -> int:
        """Return the page size as limit."""
        return self.page_size

    def apply_to_query(self, query: str) -> str:
        """
        Add LIMIT/OFFSET to a SQL query.

        Args:
            query: Base SQL query (without LIMIT/OFFSET)

        Returns:
            Query with pagination applied
        """
        return f"{query} LIMIT {self.limit} OFFSET {self.offset}"

    def paginate(self, items: list[T], total: int) -> dict[str, Any]:
        """
        Create paginated response envelope.

        Args:
            items: Items for current page
            total: Total count of all items

        Returns:
            Paginated response with metadata
        """
        total_pages = (total + self.page_size - 1) // self.page_size if total > 0 else 0

        return {
            "items": items,
            "pagination": {
                "page": self.page,
                "page_size": self.page_size,
                "total_items": total,
                "total_pages": total_pages,
                "has_next": self.page < total_pages,
                "has_prev": self.page > 1,
            },
        }


def get_pagination_params(
    page: int = Query(default=1, ge=1, description="Page number (1-indexed)"),
    limit: int = Query(
        default=DEFAULT_PAGE_SIZE,
        ge=1,
        le=MAX_PAGE_SIZE,
        description=f"Items per page (max {MAX_PAGE_SIZE})",
    ),
) -> PaginationParams:
    """
    FastAPI dependency for pagination parameters.

    Usage:
        @router.get("/items")
        async def list_items(pagination: PaginationParams = Depends(get_pagination_params)):
            ...
    """
    return PaginationParams(page=page, page_size=limit)
