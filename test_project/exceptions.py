"""Custom exception hierarchy and error handling patterns."""


class AppError(Exception):
    """Base application error."""

    def __init__(self, message: str, code: int = 0) -> None:
        super().__init__(message)
        self.code = code

    @property
    def error_id(self) -> str:
        return f"ERR-{self.code:04d}"


class ValidationError(AppError):
    """Error raised when input validation fails."""

    def __init__(self, field: str, message: str) -> None:
        super().__init__(f"Validation failed for '{field}': {message}", code=1001)
        self.field = field


class NotFoundError(AppError):
    """Error raised when a resource is not found."""

    def __init__(self, resource_type: str, resource_id: str) -> None:
        super().__init__(
            f"{resource_type} '{resource_id}' not found",
            code=2001,
        )
        self.resource_type = resource_type
        self.resource_id = resource_id


class AuthenticationError(AppError):
    """Error raised on authentication failure."""

    def __init__(self, reason: str = "Invalid credentials") -> None:
        super().__init__(reason, code=3001)


class AuthorizationError(AppError):
    """Error raised when user lacks permission."""

    def __init__(self, action: str, resource: str) -> None:
        super().__init__(
            f"Not authorized to {action} on {resource}",
            code=3002,
        )
        self.action = action
        self.resource = resource


class RateLimitError(AppError):
    """Error raised when rate limit is exceeded."""

    def __init__(self, limit: int, window_seconds: int) -> None:
        super().__init__(
            f"Rate limit exceeded: {limit} requests per {window_seconds}s",
            code=4001,
        )
        self.limit = limit
        self.window_seconds = window_seconds

    @property
    def retry_after(self) -> int:
        return self.window_seconds


class DatabaseError(AppError):
    """Base database error."""

    def __init__(self, message: str, query: str = "") -> None:
        super().__init__(message, code=5001)
        self.query = query


class ConnectionError(DatabaseError):
    """Database connection failure."""

    def __init__(self, host: str, port: int) -> None:
        super().__init__(f"Cannot connect to {host}:{port}")
        self.host = host
        self.port = port


class QueryError(DatabaseError):
    """Error in SQL query execution."""

    def __init__(self, query: str, detail: str) -> None:
        super().__init__(f"Query failed: {detail}", query=query)
        self.detail = detail


def validate_email(email: str) -> str:
    """Function that raises custom exceptions."""
    if not email:
        raise ValidationError("email", "cannot be empty")
    if "@" not in email:
        raise ValidationError("email", "must contain @")
    return email


def get_user(user_id: str) -> dict[str, str]:
    """Function raising NotFoundError."""
    raise NotFoundError("User", user_id)


ERROR_CODES: dict[int, str] = {
    1001: "VALIDATION_ERROR",
    2001: "NOT_FOUND",
    3001: "AUTH_FAILED",
    3002: "FORBIDDEN",
    4001: "RATE_LIMITED",
    5001: "DB_ERROR",
}
