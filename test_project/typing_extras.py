"""Advanced typing patterns: TypedDict, NamedTuple, Literal, Union, overloads."""

from typing import Literal, NamedTuple, TypedDict, Union


class Address(TypedDict):
    """TypedDict for structured address data."""

    street: str
    city: str
    zip_code: str
    country: str


class UserProfile(TypedDict, total=False):
    """TypedDict with optional fields (total=False)."""

    name: str
    email: str
    age: int
    address: Address
    tags: list[str]


class Coordinate(NamedTuple):
    """NamedTuple for immutable coordinate pair."""

    x: float
    y: float

    def distance_to(self, other: "Coordinate") -> float:
        return ((self.x - other.x) ** 2 + (self.y - other.y) ** 2) ** 0.5


class RGB(NamedTuple):
    """NamedTuple for color values."""

    red: int
    green: int
    blue: int

    def hex(self) -> str:
        return f"#{self.red:02x}{self.green:02x}{self.blue:02x}"

    @classmethod
    def from_hex(cls, hex_str: str) -> "RGB":
        hex_str = hex_str.lstrip("#")
        return cls(
            red=int(hex_str[0:2], 16),
            green=int(hex_str[2:4], 16),
            blue=int(hex_str[4:6], 16),
        )


# Literal types
Direction = Literal["north", "south", "east", "west"]
LogLevel = Literal["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"]


def move(direction: Direction, steps: int = 1) -> str:
    """Function using Literal type."""
    return f"Moving {direction} by {steps} steps"


def log(message: str, level: LogLevel = "INFO") -> str:
    """Function with Literal default."""
    return f"[{level}] {message}"


# Union types
NumberLike = Union[int, float, str]
OptionalStr = str | None


def to_number(value: NumberLike) -> float:
    """Function using Union type."""
    if isinstance(value, str):
        return float(value)
    return float(value)


def first_or_none(items: list[str]) -> OptionalStr:
    """Function returning Optional (pipe syntax)."""
    return items[0] if items else None


# Complex nested types
NestedConfig = dict[str, dict[str, list[int]]]
Tree = dict[str, "Tree | str"]
Matrix = list[list[float]]


def flatten_config(config: NestedConfig) -> dict[str, list[int]]:
    """Function with complex nested type parameter."""
    result: dict[str, list[int]] = {}
    for outer_key, inner in config.items():
        for inner_key, values in inner.items():
            result[f"{outer_key}.{inner_key}"] = values
    return result


def transpose(matrix: Matrix) -> Matrix:
    """Function using type alias for list[list[float]]."""
    if not matrix:
        return []
    return [[row[i] for row in matrix] for i in range(len(matrix[0]))]


class Config:
    """Class combining multiple typing patterns."""

    def __init__(
        self,
        name: str,
        debug: bool = False,
        log_level: LogLevel = "INFO",
        tags: list[str] | None = None,
    ) -> None:
        self.name = name
        self.debug = debug
        self.log_level = log_level
        self.tags: list[str] = tags or []

    def to_dict(self) -> dict[str, object]:
        return {
            "name": self.name,
            "debug": self.debug,
            "log_level": self.log_level,
            "tags": self.tags,
        }

    @classmethod
    def from_dict(cls, data: dict[str, object]) -> "Config":
        return cls(
            name=str(data.get("name", "")),
            debug=bool(data.get("debug", False)),
        )


ORIGIN: Coordinate = Coordinate(0.0, 0.0)
WHITE: RGB = RGB(255, 255, 255)
BLACK: RGB = RGB(0, 0, 0)
