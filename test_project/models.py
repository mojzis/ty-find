"""Models module with classes and functions for testing ty-find."""


class Animal:
    """Base class for animals."""

    def __init__(self, name: str, sound: str) -> None:
        self.name = name
        self.sound = sound

    def speak(self) -> str:
        return f"{self.name} says {self.sound}"


class Dog(Animal):
    """A dog."""

    def __init__(self, name: str) -> None:
        super().__init__(name, "woof")

    def fetch(self, item: str) -> str:
        return f"{self.name} fetches {item}"


class Cat(Animal):
    """A cat."""

    def __init__(self, name: str) -> None:
        super().__init__(name, "meow")

    def purr(self) -> str:
        return f"{self.name} purrs"


MAX_ANIMALS: int = 100


def create_dog(name: str) -> Dog:
    """Create a new Dog instance."""
    return Dog(name)


def create_cat(name: str) -> Cat:
    """Create a new Cat instance."""
    return Cat(name)
