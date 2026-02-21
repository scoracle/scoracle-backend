"""
Unified error handling for consistent API error responses.

All API errors should use these classes to ensure consistent response format:
{
    "error": {
        "code": "ERROR_CODE",
        "message": "Human-readable message",
        "detail": "Optional additional context"
    }
}
"""

from typing import Any

from fastapi import HTTPException, Request
from fastapi.responses import JSONResponse


class APIError(HTTPException):
    """
    Base API error class for consistent error responses.

    All API errors use this format:
    {
        "error": {
            "code": "ERROR_CODE",
            "message": "Human-readable message",
            "detail": "Optional context"
        }
    }
    """

    def __init__(
        self,
        status_code: int,
        code: str,
        message: str,
        detail: str | None = None,
        headers: dict[str, str] | None = None,
    ):
        self.code = code
        self.message = message
        self.error_detail = detail
        super().__init__(
            status_code=status_code,
            detail={"code": code, "message": message, "detail": detail},
            headers=headers,
        )


class NotFoundError(APIError):
    """Resource not found (404)."""

    def __init__(self, resource: str, identifier: Any, context: str | None = None):
        message = f"{resource} not found"
        detail = f"{resource} with ID {identifier}"
        if context:
            detail = f"{detail} in {context}"
        super().__init__(
            status_code=404,
            code="NOT_FOUND",
            message=message,
            detail=detail,
        )


class ValidationError(APIError):
    """Invalid input (400)."""

    def __init__(self, message: str, detail: str | None = None):
        super().__init__(
            status_code=400,
            code="VALIDATION_ERROR",
            message=message,
            detail=detail,
        )


class RateLimitedError(APIError):
    """Rate limit exceeded (429)."""

    def __init__(self, retry_after: int = 60):
        super().__init__(
            status_code=429,
            code="RATE_LIMITED",
            message="Too many requests. Please slow down.",
            detail=f"Retry after {retry_after} seconds",
            headers={"Retry-After": str(retry_after)},
        )


class ServiceUnavailableError(APIError):
    """External service unavailable (503)."""

    def __init__(self, service: str, message: str | None = None):
        super().__init__(
            status_code=503,
            code="SERVICE_UNAVAILABLE",
            message=message or f"{service} is currently unavailable",
            detail=f"The {service} service is not configured or experiencing issues",
        )


class ExternalServiceError(APIError):
    """External API error (502)."""

    def __init__(self, service: str, message: str, status_code: int = 502):
        super().__init__(
            status_code=status_code,
            code="EXTERNAL_API_ERROR",
            message=message,
            detail=f"Error from {service} API",
        )


async def api_error_handler(request: Request, exc: APIError) -> JSONResponse:
    """
    FastAPI exception handler for APIError.

    Converts APIError exceptions to consistent JSON responses.
    """
    content = {
        "error": {
            "code": exc.code,
            "message": exc.message,
        }
    }
    if exc.error_detail:
        content["error"]["detail"] = exc.error_detail

    return JSONResponse(
        status_code=exc.status_code,
        content=content,
        headers=exc.headers,
    )
