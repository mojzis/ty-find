"""Decorator patterns: custom decorators, stacked decorators, decorator factories."""

import functools
from typing import Any, Callable, TypeVar

F = TypeVar("F", bound=Callable[..., Any])


def log_calls(func: F) -> F:
    """Simple function decorator."""

    @functools.wraps(func)
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        print(f"Calling {func.__name__}")
        return func(*args, **kwargs)

    return wrapper  # type: ignore[return-value]


def retry(max_attempts: int = 3, delay: float = 1.0) -> Callable[[F], F]:
    """Decorator factory with arguments."""

    def decorator(func: F) -> F:
        @functools.wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            for attempt in range(max_attempts):
                try:
                    return func(*args, **kwargs)
                except Exception:
                    if attempt == max_attempts - 1:
                        raise
            return None

        return wrapper  # type: ignore[return-value]

    return decorator


def validate_positive(func: Callable[..., Any]) -> Callable[..., Any]:
    """Decorator that validates first arg is positive."""

    @functools.wraps(func)
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        if args and isinstance(args[0], (int, float)) and args[0] <= 0:
            raise ValueError("First argument must be positive")
        return func(*args, **kwargs)

    return wrapper


def singleton(cls: type) -> type:
    """Class decorator implementing singleton pattern."""
    instances: dict[type, Any] = {}

    @functools.wraps(cls, updated=[])
    def get_instance(*args: Any, **kwargs: Any) -> Any:
        if cls not in instances:
            instances[cls] = cls(*args, **kwargs)
        return instances[cls]

    return get_instance  # type: ignore[return-value]


@log_calls
def greet(name: str) -> str:
    """Function with single decorator."""
    return f"Hello, {name}!"


@retry(max_attempts=5, delay=0.5)
@log_calls
def fetch_data(url: str) -> dict[str, Any]:
    """Function with stacked decorators (decorator factory + simple)."""
    return {"url": url, "data": "mock"}


class TestConfig:
    """Configuration for test decorators."""
    def __init__(
        self,
        paint_blue: bool = False,
        tags: list[str] | None = None,
        visibility: float = 1.0,
        something: int = 0,
    ) -> None:
        self.paint_blue = paint_blue
        self.tags = tags or []
        self.visibility = visibility
        self.something = something


def test_dec(config: TestConfig) -> Callable[[F], F]:
    """Decorator factory that accepts a complex config object."""
    def decorator(func: F) -> F:
        @functools.wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            return func(*args, **kwargs)
        return wrapper  # type: ignore[return-value]
    return decorator


@test_dec(
    config=TestConfig(
        paint_blue=True,
        tags=["a", "b"],
        visibility=0.9,
        something=1,
    )
)
def complex_decorated(x: int, y: int) -> int:
    """Function with a complex multi-line decorator."""
    return x + y


@validate_positive
def square_root(x: float) -> float:
    """Decorated function with validation."""
    return x**0.5


@singleton
class DatabaseConnection:
    """Singleton class via decorator."""

    def __init__(self, host: str = "localhost", port: int = 5432) -> None:
        self.host = host
        self.port = port
        self._connected = False

    def connect(self) -> str:
        self._connected = True
        return f"Connected to {self.host}:{self.port}"

    def disconnect(self) -> None:
        self._connected = False

    @property
    def is_connected(self) -> bool:
        return self._connected


class Cached:
    """Descriptor-based caching decorator (class as decorator)."""

    def __init__(self, func: Callable[..., Any]) -> None:
        self.func = func
        self.cache: dict[Any, Any] = {}

    def __get__(self, obj: Any, objtype: type | None = None) -> Any:
        if obj is None:
            return self
        return functools.partial(self.__call__, obj)

    def __call__(self, *args: Any, **kwargs: Any) -> Any:
        key = (args, tuple(sorted(kwargs.items())))
        if key not in self.cache:
            self.cache[key] = self.func(*args, **kwargs)
        return self.cache[key]


class MathService:
    """Class using descriptor-based caching."""

    @Cached
    def expensive_computation(self, n: int) -> int:
        """Cached method via descriptor decorator."""
        return sum(range(n))

    @staticmethod
    def add(a: int, b: int) -> int:
        return a + b

    @classmethod
    def create_default(cls) -> "MathService":
        return cls()
