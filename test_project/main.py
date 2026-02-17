"""Main module that uses models - for testing references and hover."""

from models import Animal, Dog, Cat, create_dog, create_cat, MAX_ANIMALS


def list_animals(animals: list[Animal]) -> None:
    """Print all animals and their sounds."""
    for animal in animals:
        print(animal.speak())


def main() -> None:
    """Entry point."""
    dog = create_dog("Rex")
    cat = create_cat("Whiskers")

    print(dog.fetch("ball"))
    print(cat.purr())

    animals: list[Animal] = [dog, cat]
    list_animals(animals)

    print(f"Max allowed: {MAX_ANIMALS}")


if __name__ == "__main__":
    main()
