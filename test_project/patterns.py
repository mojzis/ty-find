"""Advanced Python patterns: nested classes, closures, generators, context managers, slots, metaclass."""

from contextlib import contextmanager
from typing import Any, Generator, Iterator


# ── Nested classes ──────────────────────────────────────────────────

class Outer:
    """Class with nested inner class."""

    class Inner:
        """Nested class."""

        def __init__(self, value: int) -> None:
            self.value = value

        def double(self) -> int:
            return self.value * 2

    def __init__(self, name: str) -> None:
        self.name = name
        self.inner = self.Inner(0)

    def create_inner(self, value: int) -> "Outer.Inner":
        return self.Inner(value)


# ── Closures and higher-order functions ────────────────────────────

def make_multiplier(factor: int) -> "Callable[[int], int]":
    """Closure: returns a function that multiplies by factor."""

    def multiplier(x: int) -> int:
        return x * factor

    return multiplier


def make_accumulator(initial: float = 0.0) -> "Callable[[float], float]":
    """Closure with mutable state via nonlocal."""
    total = initial

    def add(amount: float) -> float:
        nonlocal total
        total += amount
        return total

    return add


def compose(*funcs: "Callable[[Any], Any]") -> "Callable[[Any], Any]":
    """Higher-order function: compose multiple functions."""

    def composed(x: Any) -> Any:
        result = x
        for f in reversed(funcs):
            result = f(result)
        return result

    return composed


# ── Generators and iterators ───────────────────────────────────────

def fibonacci(limit: int) -> Generator[int, None, None]:
    """Generator function yielding Fibonacci numbers."""
    a, b = 0, 1
    while a < limit:
        yield a
        a, b = b, a + b


def chunked(items: list[Any], size: int) -> Generator[list[Any], None, None]:
    """Generator that yields chunks of a list."""
    for i in range(0, len(items), size):
        yield items[i : i + size]


class RangeIterator:
    """Custom iterator class implementing __iter__ and __next__."""

    def __init__(self, start: int, stop: int, step: int = 1) -> None:
        self.current = start
        self.stop = stop
        self.step = step

    def __iter__(self) -> "RangeIterator":
        return self

    def __next__(self) -> int:
        if self.current >= self.stop:
            raise StopIteration
        value = self.current
        self.current += self.step
        return value


# ── Context managers ───────────────────────────────────────────────

class Timer:
    """Context manager class using __enter__/__exit__."""

    def __init__(self, label: str = "Timer") -> None:
        self.label = label
        self.elapsed: float = 0.0

    def __enter__(self) -> "Timer":
        import time
        self._start = time.monotonic()
        return self

    def __exit__(self, *args: Any) -> None:
        import time
        self.elapsed = time.monotonic() - self._start


@contextmanager
def temporary_value(obj: Any, attr: str, value: Any) -> Iterator[None]:
    """Generator-based context manager: temporarily set an attribute."""
    original = getattr(obj, attr)
    setattr(obj, attr, value)
    try:
        yield
    finally:
        setattr(obj, attr, original)


# ── Slots ──────────────────────────────────────────────────────────

class Point:
    """Class using __slots__ for memory efficiency."""

    __slots__ = ("x", "y")

    def __init__(self, x: float, y: float) -> None:
        self.x = x
        self.y = y

    def distance_to(self, other: "Point") -> float:
        return ((self.x - other.x) ** 2 + (self.y - other.y) ** 2) ** 0.5

    def __repr__(self) -> str:
        return f"Point({self.x}, {self.y})"


class Point3D(Point):
    """Slots with inheritance — adds z coordinate."""

    __slots__ = ("z",)

    def __init__(self, x: float, y: float, z: float) -> None:
        super().__init__(x, y)
        self.z = z

    def distance_to(self, other: "Point") -> float:
        base = super().distance_to(other)
        if isinstance(other, Point3D):
            return (base**2 + (self.z - other.z) ** 2) ** 0.5
        return base


# ── Metaclass ──────────────────────────────────────────────────────

class SingletonMeta(type):
    """Metaclass implementing singleton pattern."""

    _instances: dict[type, Any] = {}

    def __call__(cls, *args: Any, **kwargs: Any) -> Any:
        if cls not in cls._instances:
            cls._instances[cls] = super().__call__(*args, **kwargs)
        return cls._instances[cls]


class AppConfig(metaclass=SingletonMeta):
    """Class using metaclass for singleton behavior."""

    def __init__(self, env: str = "production") -> None:
        self.env = env
        self.settings: dict[str, Any] = {}

    def get(self, key: str, default: Any = None) -> Any:
        return self.settings.get(key, default)

    def set(self, key: str, value: Any) -> None:
        self.settings[key] = value


# ── Module-level constants and utility ─────────────────────────────

MAX_RETRIES: int = 3
DEFAULT_TIMEOUT: float = 30.0
EMPTY_SENTINEL: object = object()


def identity(x: Any) -> Any:
    """Identity function — returns argument unchanged."""
    return x
