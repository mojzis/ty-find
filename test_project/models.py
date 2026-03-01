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


class ServiceDog(Dog):
    """A trained service dog with certifications and tasks."""

    registry: dict[str, "ServiceDog"] = {}

    def __init__(self, name: str, handler: str, certification: str = "basic") -> None:
        super().__init__(name)
        self.handler = handler
        self.certification = certification
        self.tasks_completed: int = 0

    def assist(self, task: str, duration_minutes: int = 30) -> str:
        """Perform an assistance task."""
        self.tasks_completed += 1
        return f"{self.name} assists with {task} for {duration_minutes}min"

    def report_status(self) -> dict[str, str | int]:
        """Generate a status report."""
        return {
            "name": self.name,
            "handler": self.handler,
            "certification": self.certification,
            "tasks_completed": self.tasks_completed,
        }

    @property
    def is_certified(self) -> bool:
        """Check if the dog has advanced certification."""
        return self.certification != "basic"

    @classmethod
    def from_registry(cls, name: str) -> "ServiceDog | None":
        """Look up a service dog by name."""
        return cls.registry.get(name)


MAX_ANIMALS: int = 100


def create_dog(name: str) -> Dog:
    """Create a new Dog instance."""
    return Dog(name)


def create_cat(name: str) -> Cat:
    """Create a new Cat instance."""
    return Cat(name)
