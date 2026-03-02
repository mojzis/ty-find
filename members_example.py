"""Test fixture for the members command."""


class Animal:
    """Base class for animals."""

    MAX_LEGS: int = 4
    DEFAULT_NAME: str = "Unknown"

    def __init__(self, name: str, legs: int = 4) -> None:
        self._name = name
        self._legs = legs

    @property
    def name(self) -> str:
        return self._name

    @property
    def is_quadruped(self) -> bool:
        return self._legs == 4

    def speak(self) -> str:
        return "..."

    def describe(self) -> str:
        return f"{self._name} with {self._legs} legs"

    def __repr__(self) -> str:
        return f"Animal({self._name!r})"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Animal):
            return NotImplemented
        return self._name == other._name


class Dog(Animal):
    """A dog."""

    SPECIES: str = "Canis familiaris"

    def __init__(self, name: str) -> None:
        super().__init__(name, legs=4)

    def speak(self) -> str:
        return "Woof!"

    def fetch(self, item: str) -> str:
        return f"{self.name} fetches {item}"


def standalone_function(x: int) -> int:
    """Not a class — used to test non-class error."""
    return x * 2


GLOBAL_VAR: int = 42
